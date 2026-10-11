//! End-to-end tests for appearing offline.
//!
//! The point of the status is that nobody can tell it apart from being
//! disconnected, so what these pin is what the *other* members are sent:
//! never an `online` for the hidden member, and an `online-users` list
//! without them, including at the moment they connect.

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

/// Serve `/ws`; nobody needs a role to choose their own status.
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
        Self::connect_as(addr, name, "online").await
    }

    /// Connect as `name`, announcing `status` in the presence frame.
    async fn connect_as(addr: SocketAddr, name: &'static str, status: &str) -> Self {
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
                "status": status,
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
}

fn online(frame: &Value) -> Vec<&str> {
    frame["users"]
        .as_array()
        .expect("users")
        .iter()
        .filter_map(Value::as_str)
        .collect()
}

#[tokio::test]
async fn connecting_while_offline_is_never_announced_as_online() {
    let (addr, _state) = start_server().await;
    let mut alice = Client::connect(addr, "alice").await;
    let _bob = Client::connect_as(addr, "bob", "offline").await;

    let frames = alice
        .until(|f| {
            f["type"] == "online-users"
                && f["all"]
                    .as_array()
                    .is_some_and(|all| all.contains(&json!("bob")))
        })
        .await;
    assert!(!online(frames.last().unwrap()).contains(&"bob"));
    assert!(
        !frames.iter().any(|f| f["type"] == "status-update"
            && f["user"] == "bob"
            && f["status"] != "offline")
    );
}

#[tokio::test]
async fn switching_to_offline_and_back_moves_the_member_between_lists() {
    let (addr, _state) = start_server().await;
    let mut alice = Client::connect(addr, "alice").await;
    let mut bob = Client::connect(addr, "bob").await;

    bob.send(json!({ "type": "status-update", "status": "offline" }))
        .await;
    let frames = alice
        .until(|f| f["type"] == "online-users" && !online(f).contains(&"bob"))
        .await;
    assert!(online(frames.last().unwrap()).contains(&"alice"));

    bob.send(json!({ "type": "status-update", "status": "online" }))
        .await;
    alice
        .until(|f| f["type"] == "online-users" && online(f).contains(&"bob"))
        .await;
}

#[tokio::test]
async fn an_unknown_status_in_presence_falls_back_to_online() {
    let (addr, _state) = start_server().await;
    let mut alice = Client::connect(addr, "alice").await;
    let _bob = Client::connect_as(addr, "bob", "lurking").await;

    alice
        .until(|f| f["type"] == "online-users" && online(f).contains(&"bob"))
        .await;
}
