//! End-to-end tests for editing and deleting direct messages.
//!
//! Both are the sender's alone, and there is no moderator override: the
//! server cannot read a DM, so nobody but its author could judge it. The
//! refusals are what these pin, together with the stored row actually
//! changing — a delete has to remove the ciphertext for both sides.

use std::{net::SocketAddr, sync::Arc, time::Duration};

use axum::{Router, routing::get};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signer, SigningKey};
use futures::{SinkExt, StreamExt};
use murmer_server::{AppState, db, ws::ws_handler};
use serde_json::{Value, json};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};

/// A key derived from the name, so every run binds the same identity.
fn signing_key(name: &str) -> SigningKey {
    let mut seed = [0u8; 32];
    seed[..name.len()].copy_from_slice(name.as_bytes());
    SigningKey::from_bytes(&seed)
}

fn public_key(name: &str) -> String {
    STANDARD.encode(signing_key(name).verifying_key().to_bytes())
}

/// Serve `/ws` with nobody holding any role beyond `@everyone`: editing and
/// deleting a DM is the sender's right, not a permission.
async fn start_server() -> (SocketAddr, Arc<AppState>) {
    let database = db::init(":memory:").await.expect("in-memory db");
    let role_defs = db::list_role_defs(&database).await.expect("role defs");
    let state = Arc::new(AppState {
        role_defs: tokio::sync::Mutex::new(role_defs.into_iter().map(|d| (d.id, d)).collect()),
        ..AppState::new(database)
    });
    let router = Router::new()
        .route("/ws", get(ws_handler))
        .with_state(Arc::clone(&state));
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local addr");
    tokio::spawn(async move {
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .expect("serve");
    });
    (addr, state)
}

struct Client {
    name: &'static str,
    ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
}

impl Client {
    /// Connect and authenticate as `name`; the server joins it to `general`.
    async fn connect(addr: SocketAddr, name: &'static str) -> Self {
        let (ws, _) = connect_async(format!("ws://{addr}/ws"))
            .await
            .expect("connect");
        let mut client = Self { name, ws };
        let key = signing_key(name);
        let challenge = client.until(|f| f["type"] == "auth-challenge").await;
        let challenge = challenge.last().unwrap()["challenge"]
            .as_str()
            .unwrap()
            .to_owned();
        client
            .send(json!({
                "type": "presence",
                "user": name,
                "publicKey": public_key(name),
                "signature": STANDARD.encode(key.sign(format!("presence:{challenge}").as_bytes()).to_bytes()),
            }))
            .await;
        client.send(json!({ "type": "ping", "id": name })).await;
        client.until(|f| f["type"] == "pong").await;
        client
    }

    async fn send(&mut self, frame: Value) {
        self.ws
            .send(Message::text(frame.to_string()))
            .await
            .expect("send");
    }

    /// Every frame up to and including the first that matches `done`.
    async fn until(&mut self, done: impl Fn(&Value) -> bool) -> Vec<Value> {
        let mut seen = Vec::new();
        loop {
            let next = tokio::time::timeout(Duration::from_secs(5), self.ws.next())
                .await
                .unwrap_or_else(|_| panic!("{} timed out; saw {seen:?}", self.name));
            let Some(Ok(Message::Text(text))) = next else {
                continue;
            };
            let frame: Value = serde_json::from_str(&text).expect("json frame");
            let matched = done(&frame);
            seen.push(frame);
            if matched {
                return seen;
            }
        }
    }

    /// The error code the server answers the next refused frame with.
    async fn next_error(&mut self) -> String {
        let frames = self.until(|f| f["type"] == "error").await;
        frames.last().unwrap()["message"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    }
}

/// A shape-valid sealed payload; the server never opens it.
fn sealed(fill: u8) -> (String, String) {
    (STANDARD.encode([fill; 24]), STANDARD.encode([fill; 17]))
}

/// Alice DMs Bob; returns the stored id.
async fn send_dm(alice: &mut Client) -> i64 {
    let (nonce, ciphertext) = sealed(1);
    alice
        .send(json!({ "type": "dm", "to": "bob", "nonce": nonce, "ciphertext": ciphertext }))
        .await;
    let frames = alice.until(|f| f["type"] == "dm").await;
    frames.last().unwrap()["id"].as_i64().expect("dm id")
}

#[tokio::test]
async fn the_sender_edits_a_dm_and_both_sides_hear_of_it() {
    let (addr, state) = start_server().await;
    let mut alice = Client::connect(addr, "alice").await;
    let mut bob = Client::connect(addr, "bob").await;
    let id = send_dm(&mut alice).await;

    let (nonce, ciphertext) = sealed(2);
    alice
        .send(json!({ "type": "edit-dm", "id": id, "nonce": nonce, "ciphertext": ciphertext }))
        .await;
    let frames = bob.until(|f| f["type"] == "dm-edited").await;
    let edited = frames.last().unwrap();
    assert_eq!(edited["id"], id);
    assert_eq!(edited["ciphertext"], ciphertext.as_str());

    let (_, _, stored) = db::get_direct_message(&state.db, id)
        .await
        .expect("db")
        .expect("still stored");
    let stored: Value = serde_json::from_str(&stored).expect("json");
    assert_eq!(stored["ciphertext"], ciphertext.as_str());
    assert_eq!(stored["edited"], true);
}

#[tokio::test]
async fn the_sender_deletes_a_dm_for_both_sides() {
    let (addr, state) = start_server().await;
    let mut alice = Client::connect(addr, "alice").await;
    let mut bob = Client::connect(addr, "bob").await;
    let id = send_dm(&mut alice).await;

    alice.send(json!({ "type": "delete-dm", "id": id })).await;
    bob.until(|f| f["type"] == "dm-deleted" && f["id"] == id)
        .await;
    assert!(
        db::get_direct_message(&state.db, id)
            .await
            .expect("db")
            .is_none()
    );
}

#[tokio::test]
async fn nobody_but_the_sender_may_edit_or_delete() {
    let (addr, state) = start_server().await;
    let mut alice = Client::connect(addr, "alice").await;
    let mut bob = Client::connect(addr, "bob").await;
    let mut carol = Client::connect(addr, "carol").await;
    let id = send_dm(&mut alice).await;
    let (nonce, ciphertext) = sealed(3);

    // The recipient knows the message exists, so they hear "not yours".
    bob.send(json!({ "type": "delete-dm", "id": id })).await;
    assert_eq!(bob.next_error().await, "message-permission-denied");
    bob.send(json!({ "type": "edit-dm", "id": id, "nonce": nonce, "ciphertext": ciphertext }))
        .await;
    assert_eq!(bob.next_error().await, "message-permission-denied");

    // A third party must not learn even that the id is a DM.
    carol.send(json!({ "type": "delete-dm", "id": id })).await;
    assert_eq!(carol.next_error().await, "message-not-found");

    // An edit is sealed like a new DM; plaintext is refused, not stored.
    alice
        .send(json!({ "type": "edit-dm", "id": id, "text": "hello" }))
        .await;
    assert_eq!(alice.next_error().await, "invalid-dm-payload");

    let (_, _, stored) = db::get_direct_message(&state.db, id)
        .await
        .expect("db")
        .expect("still stored");
    let stored: Value = serde_json::from_str(&stored).expect("json");
    assert_eq!(stored["ciphertext"], sealed(1).1.as_str());
}
