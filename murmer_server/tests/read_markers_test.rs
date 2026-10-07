//! Tests for read markers (`mark-read`, `read-marker`, `read-markers`).
//!
//! What goes wrong here is quiet. A marker that reaches another account tells
//! them what somebody read; one stored for a private channel is a row the
//! member had no business making. A marker that moves backwards resurrects a
//! badge on every other client. And the upgrade seed, if it ran twice or not
//! at all, either hides a DM or floods the user with every DM they ever got.

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

/// Serve `/ws` with Alice bootstrapped as Owner, so she can make a private
/// channel.
async fn start_server() -> SocketAddr {
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
        .with_state(state);
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
    addr
}

struct Client {
    name: &'static str,
    ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
    /// The `read-markers` frame sent at sign-in.
    markers: Value,
}

impl Client {
    async fn connect(addr: SocketAddr, name: &'static str) -> Self {
        let (ws, _) = connect_async(format!("ws://{addr}/ws"))
            .await
            .expect("connect");
        let mut client = Self {
            name,
            ws,
            markers: Value::Null,
        };
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
        let frames = client.until(|f| f["type"] == "pong").await;
        client.markers = frames
            .into_iter()
            .find(|f| f["type"] == "read-markers")
            .expect("read-markers at sign-in");
        client
    }

    async fn send(&mut self, frame: Value) {
        self.ws
            .send(Message::text(frame.to_string()))
            .await
            .expect("send");
    }

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

    async fn mark(&mut self, frame: Value) {
        let mut frame = frame;
        frame["type"] = json!("mark-read");
        self.send(frame).await;
    }

    /// Post in the joined channel and return the stored message's id.
    async fn say(&mut self, text: &str) -> i64 {
        self.send(json!({ "type": "chat", "text": text })).await;
        let frames = self
            .until(|f| f["type"] == "chat" && f["text"] == text)
            .await;
        frames.last().unwrap()["id"].as_i64().expect("message id")
    }

    async fn dm(&mut self, to: &str) -> i64 {
        self.send(json!({
            "type": "dm",
            "to": to,
            "nonce": STANDARD.encode([0u8; 24]),
            "ciphertext": STANDARD.encode([0u8; 32]),
        }))
        .await;
        let frames = self.until(|f| f["type"] == "dm").await;
        frames.last().unwrap()["id"].as_i64().expect("dm id")
    }
}

/// The next `read-marker` this client receives. Direct frames reach a
/// connection in the order they were queued, so a sentinel marker sent after
/// a refused one proves the refused one produced nothing.
async fn next_echo(client: &mut Client) -> Value {
    client
        .until(|f| f["type"] == "read-marker")
        .await
        .pop()
        .unwrap()
}

#[tokio::test]
async fn a_read_on_one_client_reaches_the_accounts_other_clients_and_no_one_else() {
    let addr = start_server().await;
    let mut desktop = Client::connect(addr, "alice").await;
    let mut browser = Client::connect(addr, "alice").await;
    let mut bob = Client::connect(addr, "bob").await;
    let first = desktop.say("one").await;
    let second = desktop.say("two").await;

    desktop
        .mark(json!({ "channelId": 1, "messageId": second }))
        .await;
    assert_eq!(
        next_echo(&mut browser).await,
        json!({ "type": "read-marker", "channelId": 1, "messageId": second })
    );

    // A client still showing an older position cannot move the marker back,
    // and says nothing to the others when it tries.
    desktop
        .mark(json!({ "channelId": 1, "messageId": first }))
        .await;
    desktop.mark(json!({ "with": "bob", "messageId": 1 })).await;
    assert_eq!(next_echo(&mut browser).await["with"], json!("bob"));

    // Bob's first echo is his own: none of Alice's reached him.
    bob.mark(json!({ "channelId": 1, "messageId": first }))
        .await;
    assert_eq!(next_echo(&mut bob).await["messageId"], json!(first));

    let later = Client::connect(addr, "alice").await;
    assert_eq!(later.markers["channels"]["1"], json!(second));
}

#[tokio::test]
async fn a_marker_is_refused_for_a_channel_the_member_cannot_see() {
    let addr = start_server().await;
    let mut alice = Client::connect(addr, "alice").await;
    let mut bob = Client::connect(addr, "bob").await;

    alice
        .send(json!({ "type": "create-channel", "name": "secret", "private": true }))
        .await;
    let frames = alice
        .until(|f| f["type"] == "channel-add" && f["name"] == "secret")
        .await;
    let secret = frames.last().unwrap()["channelId"].as_i64().expect("id");

    bob.mark(json!({ "channelId": secret, "messageId": 5 }))
        .await;
    bob.mark(json!({ "channelId": 1, "messageId": 5 })).await;
    assert_eq!(next_echo(&mut bob).await["channelId"], json!(1));

    let later = Client::connect(addr, "bob").await;
    assert_eq!(later.markers["channels"], json!({ "1": 5 }));
}

#[tokio::test]
async fn direct_messages_past_the_marker_count_as_unread_at_sign_in() {
    let addr = start_server().await;
    let mut alice = Client::connect(addr, "alice").await;
    let bob = Client::connect(addr, "bob").await;
    drop(bob);

    let first = alice.dm("bob").await;
    alice.dm("bob").await;

    let mut bob = Client::connect(addr, "bob").await;
    assert_eq!(
        bob.markers["dms"]["alice"],
        json!({ "lastRead": 0, "unread": 2 })
    );
    // Alice's own messages are never unread to her.
    assert_eq!(alice.markers["dms"], json!({}));

    bob.mark(json!({ "with": "alice", "messageId": first }))
        .await;
    next_echo(&mut bob).await;
    let bob = Client::connect(addr, "bob").await;
    assert_eq!(
        bob.markers["dms"]["alice"],
        json!({ "lastRead": first, "unread": 1 })
    );
}

/// A database from before the markers has no DM marker anywhere. Seeding them
/// at the newest message is what keeps the upgrade from reporting every DM
/// ever received as unread, and it must happen once: a second start must not
/// swallow a DM that arrived in between.
#[tokio::test]
async fn existing_direct_messages_are_seeded_as_read_exactly_once() {
    let database = db::init(":memory:").await.expect("in-memory db");
    let old = db::insert_direct_message(&database, "alice", "bob", "{}")
        .await
        .expect("insert");
    database
        .call(|conn| conn.execute_batch("DROP TABLE dm_read_markers"))
        .await
        .expect("drop");

    db::run_schema(&database).await.expect("upgrade");
    let state = db::dm_read_state(&database, "bob").await.expect("state");
    assert_eq!(state["alice"], (old, 0));

    db::insert_direct_message(&database, "alice", "bob", "{}")
        .await
        .expect("insert");
    db::run_schema(&database).await.expect("restart");
    let state = db::dm_read_state(&database, "bob").await.expect("state");
    assert_eq!(state["alice"], (old, 1));
}
