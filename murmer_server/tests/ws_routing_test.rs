//! End-to-end tests for the per-recipient filter on the server-wide broadcast.
//!
//! Every connection receives every global frame and drops the ones its user
//! may not see — the `global_rx` arm of the socket loop. That arm is the only
//! thing keeping a private channel's existence and message notifications, and
//! the envelope of every DM, away from everyone else, and a mistake in it is
//! invisible: the leaked frame arrives, the client ignores or merely lists
//! it, and nothing errors. `direct_routing_test.rs` and
//! `visibility_cache_test.rs` cover the helpers; these drive the real `/ws`
//! handshake and dispatch loop over a real socket.
//!
//! Absence is asserted without sleeping: after the action under test the
//! sender broadcasts a status change, which travels the same ordered channel
//! and reaches everyone. Anything the filter let through arrives before it.

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

/// Serve `/ws` on an ephemeral port, with the role definitions loaded the way
/// `main` loads them. `ADMIN_TOKEN` must be set: without it every user holds
/// `MANAGE_CHANNELS` and so, by design, sees every private channel. Alice is
/// bootstrapped as Owner, the way `/role` does it, so she may create one.
async fn start_server() -> SocketAddr {
    let database = db::init(":memory:").await.expect("in-memory db");
    db::assign_named_role(&database, &public_key("alice"), "Owner", None)
        .await
        .expect("bootstrap owner");
    let role_defs = db::list_role_defs(&database).await.expect("role defs");
    let state = Arc::new(AppState {
        admin_token: Some("token".to_string()),
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
}

impl Client {
    /// Connect and authenticate as `name`.
    async fn connect(addr: SocketAddr, name: &'static str) -> Self {
        let (ws, _) = connect_async(format!("ws://{addr}/ws"))
            .await
            .expect("connect");
        let mut client = Self { name, ws };

        let key = signing_key(name);
        let timestamp = chrono::Utc::now().timestamp_millis().to_string();
        client
            .send(json!({
                "type": "presence",
                "user": name,
                "publicKey": public_key(name),
                "signature": STANDARD.encode(key.sign(timestamp.as_bytes()).to_bytes()),
                "timestamp": timestamp,
            }))
            .await;
        // Frames are handled in order, so the pong lands after the whole
        // post-auth snapshot.
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

    /// Broadcast a status change everyone receives: the sync point that makes
    /// "nothing arrived before it" a meaningful assertion.
    async fn mark(&mut self, status: &str) {
        self.send(json!({ "type": "status-update", "status": status }))
            .await;
    }

    /// Frames received up to `sender`'s `status` mark.
    async fn until_mark(&mut self, sender: &str, status: &str) -> Vec<Value> {
        self.until(|f| f["type"] == "status-update" && f["user"] == sender && f["status"] == status)
            .await
    }
}

fn of_type<'a>(frames: &'a [Value], ty: &str) -> Vec<&'a Value> {
    frames.iter().filter(|f| f["type"] == ty).collect()
}

/// Alice creates a private channel and hands back its id, from her own view.
async fn create_private_channel(alice: &mut Client) -> i64 {
    alice
        .send(json!({ "type": "create-channel", "name": "secret", "private": true }))
        .await;
    let frames = alice
        .until(|f| f["type"] == "channel-add" && f["name"] == "secret")
        .await;
    frames.last().unwrap()["channelId"]
        .as_i64()
        .expect("channel id")
}

#[tokio::test]
async fn a_private_channel_is_announced_only_to_who_can_see_it() {
    let addr = start_server().await;
    let mut alice = Client::connect(addr, "alice").await;
    let mut bob = Client::connect(addr, "bob").await;

    create_private_channel(&mut alice).await;
    alice.mark("away").await;

    let seen = bob.until_mark("alice", "away").await;
    assert!(
        of_type(&seen, "channel-add").is_empty(),
        "bob learned of a private channel: {seen:?}"
    );
}

#[tokio::test]
async fn a_private_channels_messages_notify_only_its_members() {
    let addr = start_server().await;
    let mut alice = Client::connect(addr, "alice").await;
    let mut bob = Client::connect(addr, "bob").await;

    let channel = create_private_channel(&mut alice).await;
    alice
        .send(json!({ "type": "join", "channelId": channel }))
        .await;
    alice
        .send(json!({ "type": "chat", "user": "alice", "text": "for members only" }))
        .await;
    alice.mark("away").await;

    // The positive case keeps the denial honest: the notify was sent.
    let own = alice.until_mark("alice", "away").await;
    assert_eq!(of_type(&own, "message-notify").len(), 1, "{own:?}");

    let seen = bob.until_mark("alice", "away").await;
    assert!(
        of_type(&seen, "message-notify").is_empty(),
        "bob was notified of a private message: {seen:?}"
    );
}

#[tokio::test]
async fn a_dm_reaches_only_its_two_participants() {
    let addr = start_server().await;
    let mut alice = Client::connect(addr, "alice").await;
    let mut bob = Client::connect(addr, "bob").await;
    let mut carol = Client::connect(addr, "carol").await;

    // A well-formed sealed envelope; the server never opens it.
    alice
        .send(json!({
            "type": "dm",
            "to": "carol",
            "nonce": STANDARD.encode([0u8; 24]),
            "ciphertext": STANDARD.encode([0u8; 32]),
        }))
        .await;
    alice.mark("away").await;

    let delivered = carol.until_mark("alice", "away").await;
    assert_eq!(of_type(&delivered, "dm").len(), 1, "{delivered:?}");

    let seen = bob.until_mark("alice", "away").await;
    assert!(
        of_type(&seen, "dm").is_empty(),
        "bob received someone else's DM: {seen:?}"
    );
}
