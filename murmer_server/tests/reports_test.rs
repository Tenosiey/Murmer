//! End-to-end tests for message reports: a member flags a message, the
//! moderators work the queue.
//!
//! The queue names who reported whom, so who may read it is the decision
//! worth pinning, along with the report that must not be fileable at all:
//! one about a message in a channel the reporter cannot see, which would
//! turn reports into a probe for hidden message ids.

use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Duration};

use axum::{Router, routing::get};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signer, SigningKey};
use futures::{SinkExt, StreamExt};
use murmer_server::channel_overrides::{ChannelKind, OverridePair, OverrideSet};
use murmer_server::permissions::VIEW_CHANNELS;
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

async fn post(state: &AppState, channel: i32, user: &str, text: &str) -> i64 {
    let body = json!({ "user": user, "text": text });
    db::insert_message(&state.db, channel, &body.to_string())
        .await
        .expect("insert")
}

async fn general(state: &AppState) -> i32 {
    db::get_channel_id_by_name(&state.db, "general")
        .await
        .expect("general channel")
}

/// The `reports` answer to the next `list-reports`.
async fn queue(client: &mut Client) -> Vec<Value> {
    client.send(json!({ "type": "list-reports" })).await;
    let frames = client.until(|f| f["type"] == "reports").await;
    frames.last().unwrap()["reports"]
        .as_array()
        .expect("reports array")
        .clone()
}

#[tokio::test]
async fn a_report_reaches_the_moderators_queue_and_can_be_dismissed() {
    let (addr, state) = start_server().await;
    let id = post(&state, general(&state).await, "carol", "buy cheap stuff").await;
    let mut alice = Client::connect(addr, "alice").await;
    let mut bob = Client::connect(addr, "bob").await;

    bob.send(json!({ "type": "report-message", "messageId": id, "reason": "spam" }))
        .await;
    bob.until(|f| f["type"] == "message-reported" && f["messageId"] == id)
        .await;

    let reports = queue(&mut alice).await;
    assert_eq!(reports.len(), 1);
    let report = &reports[0];
    assert_eq!(
        (&report["author"], &report["reporter"], &report["reason"]),
        (&json!("carol"), &json!("bob"), &json!("spam"))
    );
    assert_eq!(report["excerpt"], "buy cheap stuff");

    // Reporting twice files nothing new.
    bob.send(json!({ "type": "report-message", "messageId": id }))
        .await;
    bob.until(|f| f["type"] == "message-reported").await;
    assert_eq!(queue(&mut alice).await.len(), 1);

    alice
        .send(json!({ "type": "resolve-report", "id": report["id"] }))
        .await;
    let frames = alice.until(|f| f["type"] == "reports").await;
    assert_eq!(frames.last().unwrap()["reports"], json!([]));
}

#[tokio::test]
async fn a_member_cannot_read_or_resolve_the_queue() {
    let (addr, state) = start_server().await;
    let id = post(&state, general(&state).await, "carol", "hello").await;
    let mut alice = Client::connect(addr, "alice").await;
    let mut bob = Client::connect(addr, "bob").await;
    bob.send(json!({ "type": "report-message", "messageId": id }))
        .await;
    bob.until(|f| f["type"] == "message-reported").await;

    // Bob moderates no channel, so his queue is empty rather than an error.
    assert_eq!(queue(&mut bob).await, Vec::<Value>::new());
    let report_id = queue(&mut alice).await[0]["id"].clone();
    bob.send(json!({ "type": "resolve-report", "id": report_id }))
        .await;
    assert_eq!(bob.next_error().await, "report-not-found");
    assert_eq!(queue(&mut alice).await.len(), 1);
}

#[tokio::test]
async fn a_message_in_a_hidden_channel_cannot_be_reported() {
    let (addr, state) = start_server().await;
    let private = db::add_channel(&state.db, "private", None)
        .await
        .expect("add channel")
        .expect("channel created")
        .id;
    state.channel_overrides.lock().await.insert(
        (ChannelKind::Text, private),
        OverrideSet {
            everyone: OverridePair {
                allow: 0,
                deny: VIEW_CHANNELS,
            },
            roles: HashMap::new(),
            users: HashMap::new(),
        },
    );
    let id = post(&state, private, "carol", "secret").await;
    let mut bob = Client::connect(addr, "bob").await;

    bob.send(json!({ "type": "report-message", "messageId": id }))
        .await;
    assert_eq!(bob.next_error().await, "message-not-found");
    let mut alice = Client::connect(addr, "alice").await;
    assert_eq!(queue(&mut alice).await, Vec::<Value>::new());
}

#[tokio::test]
async fn an_overlong_reason_is_refused() {
    let (addr, state) = start_server().await;
    let id = post(&state, general(&state).await, "carol", "hello").await;
    let mut bob = Client::connect(addr, "bob").await;

    bob.send(json!({ "type": "report-message", "messageId": id, "reason": "x".repeat(501) }))
        .await;
    assert_eq!(bob.next_error().await, "report-reason-too-long");
}
