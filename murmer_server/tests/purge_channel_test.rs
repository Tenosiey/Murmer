//! End-to-end tests for `/purge`, a moderator deleting the newest messages
//! of a channel in one go.
//!
//! A purge cannot be undone and reaches past the requester's own messages,
//! so the permission check is the whole of the protection; the refusals are
//! what these pin, together with the bound on how much one frame removes.
//! They drive the real `/ws` dispatch loop over a real socket, the way
//! `move_member_test.rs` does.

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

/// Serve `/ws` with Alice bootstrapped as Owner, who therefore holds
/// `MANAGE_MESSAGES`; everyone else holds only `@everyone`, which does not.
async fn start_server() -> (SocketAddr, Arc<AppState>) {
    let database = db::init(":memory:").await.expect("in-memory db");
    db::assign_named_role(&database, &public_key("alice"), "Owner", None)
        .await
        .expect("bootstrap owner");
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

/// Post `count` messages to `general` and return its id with their ids,
/// oldest first.
async fn seed(state: &AppState, count: usize) -> (i32, Vec<i64>) {
    let general = db::get_channel_id_by_name(&state.db, "general")
        .await
        .expect("general channel");
    let mut ids = Vec::new();
    for n in 0..count {
        let body = format!(r#"{{"user":"bob","text":"message {n}"}}"#);
        ids.push(
            db::insert_message(&state.db, general, &body)
                .await
                .expect("insert"),
        );
    }
    (general, ids)
}

async fn remaining(state: &AppState, channel: i32) -> Vec<i64> {
    let mut ids: Vec<i64> = db::fetch_history(&state.db, channel, None, 50)
        .await
        .expect("history")
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    ids.sort();
    ids
}

#[tokio::test]
async fn a_moderator_purges_the_newest_messages_and_it_is_logged() {
    let (addr, state) = start_server().await;
    let (general, ids) = seed(&state, 3).await;
    let mut alice = Client::connect(addr, "alice").await;
    let mut bob = Client::connect(addr, "bob").await;

    alice
        .send(json!({ "type": "purge-channel-messages", "count": 2 }))
        .await;
    // Everyone in the channel is told about each removed message.
    let frames = bob
        .until(|f| f["type"] == "message-deleted" && f["id"] == ids[1])
        .await;
    assert!(
        frames
            .iter()
            .any(|f| f["type"] == "message-deleted" && f["id"] == ids[2])
    );
    assert_eq!(remaining(&state, general).await, [ids[0]]);

    let log = db::list_audit_entries(&state.db).await.expect("audit log");
    let entry = log
        .iter()
        .find(|e| e.action == "purge-channel")
        .expect("purge logged");
    assert_eq!(
        (
            entry.actor.as_str(),
            entry.target.as_str(),
            entry.detail.as_str()
        ),
        ("alice", "general", "2 messages")
    );
}

#[tokio::test]
async fn a_member_without_the_permission_cannot_purge() {
    let (addr, state) = start_server().await;
    let (general, ids) = seed(&state, 2).await;
    let mut bob = Client::connect(addr, "bob").await;

    // Bob wrote both messages himself; a purge is still a moderator's tool.
    bob.send(json!({ "type": "purge-channel-messages", "count": 2 }))
        .await;
    assert_eq!(bob.next_error().await, "message-permission-denied");
    assert_eq!(remaining(&state, general).await, ids);
}

#[tokio::test]
async fn a_count_outside_the_bound_is_refused() {
    let (addr, state) = start_server().await;
    let (general, ids) = seed(&state, 2).await;
    let mut alice = Client::connect(addr, "alice").await;

    for count in [json!(0), json!(101), json!(-1), json!("2")] {
        alice
            .send(json!({ "type": "purge-channel-messages", "count": count }))
            .await;
        assert_eq!(alice.next_error().await, "invalid-purge-count");
    }
    assert_eq!(remaining(&state, general).await, ids);
}
