//! End-to-end tests for direct calls.
//!
//! A call is an ordinary voice channel on the inside, and everything that
//! keeps it private is a check that, when it is missing, fails silently: a
//! third member joins and listens, an administrator sees it in the channel
//! list, or a call nobody answered keeps both parties "busy" for good. Those
//! are what these pin, over the real `/ws` dispatch loop.

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

/// Serve `/ws` with Alice bootstrapped as Owner: an administrator, the one
/// role every other permission check lets through, which is exactly who a
/// private call must still keep out. The ring timeout is cut short so a
/// missed call does not take thirty seconds.
async fn start_server() -> (SocketAddr, Arc<AppState>) {
    let database = db::init(":memory:").await.expect("in-memory db");
    db::assign_named_role(&database, &public_key("alice"), "Owner", None)
        .await
        .expect("bootstrap owner");
    let role_defs = db::list_role_defs(&database).await.expect("role defs");
    let state = Arc::new(AppState {
        role_defs: tokio::sync::Mutex::new(role_defs.into_iter().map(|d| (d.id, d)).collect()),
        ring_timeout: Duration::from_millis(300),
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

impl Client {
    /// Round-trip a ping, so every frame sent before it has been handled.
    async fn settle(&mut self) {
        let id = format!("settle-{}", self.name);
        self.send(json!({ "type": "ping", "id": id })).await;
        self.until(|f| f["type"] == "pong" && f["id"] == id.as_str())
            .await;
    }
}

/// `caller` rings `callee`; both hear `call-ringing`. Returns the call id.
async fn ring(caller: &mut Client, callee: &mut Client) -> i64 {
    caller
        .send(json!({ "type": "call-start", "target": callee.name }))
        .await;
    let frames = callee.until(|f| f["type"] == "call-ringing").await;
    let ringing = frames.last().unwrap().clone();
    assert_eq!(ringing["caller"], caller.name);
    assert_eq!(ringing["callee"], callee.name);
    caller.until(|f| f["type"] == "call-ringing").await;
    ringing["callId"].as_i64().expect("call id")
}

async fn join_call(client: &mut Client, id: i64) {
    let name = client.name;
    client
        .send(json!({ "type": "voice-join", "user": name, "channelId": id }))
        .await;
    client
        .until(|f| f["type"] == "voice-join" && f["user"] == name)
        .await;
}

async fn ended(client: &mut Client) -> String {
    let frames = client.until(|f| f["type"] == "call-ended").await;
    frames.last().unwrap()["reason"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

#[tokio::test]
async fn an_answered_call_is_closed_to_everyone_else_and_ends_on_leave() {
    let (addr, state) = start_server().await;
    let mut bob = Client::connect(addr, "bob").await;
    let mut carol = Client::connect(addr, "carol").await;
    let mut alice = Client::connect(addr, "alice").await;

    let id = ring(&mut bob, &mut carol).await;
    join_call(&mut bob, id).await;
    // The caller cannot answer their own call.
    bob.send(json!({ "type": "call-accept", "callId": id }))
        .await;
    bob.settle().await;
    assert!(!state.calls.lock().await[&(id as i32)].accepted);

    carol
        .send(json!({ "type": "call-accept", "callId": id }))
        .await;
    bob.until(|f| f["type"] == "call-accepted").await;
    join_call(&mut carol, id).await;

    // Not even an administrator joins, lists, deletes or ends it.
    alice
        .send(json!({ "type": "voice-join", "user": "alice", "channelId": id }))
        .await;
    alice
        .send(json!({ "type": "call-end", "callId": id }))
        .await;
    alice
        .send(json!({ "type": "delete-voice-channel", "channelId": id }))
        .await;
    assert_eq!(alice.next_error().await, "unknown-voice-channel");
    alice
        .send(json!({ "type": "open-breakouts", "channelId": id, "rooms": 2 }))
        .await;
    assert_eq!(alice.next_error().await, "unknown-voice-channel");
    alice.settle().await;
    let users = state.voice_channels.lock().await[&(id as i32)]
        .users
        .clone();
    assert_eq!(users.len(), 2);
    assert!(!users.contains("alice"));
    let list = murmer_server::ws::helpers::voice_channel_list_frame(&state, Some("carol"))
        .await
        .unwrap();
    assert!(!list.contains(&format!("\"id\":{id}")), "{list}");

    // One party leaving ends it for the other.
    bob.send(json!({ "type": "voice-leave", "user": "bob", "channelId": id }))
        .await;
    assert_eq!(ended(&mut carol).await, "ended");
    assert!(state.calls.lock().await.is_empty());
    assert!(!state.voice_channels.lock().await.contains_key(&(id as i32)));
}

#[tokio::test]
async fn outsiders_never_hear_who_is_in_a_call() {
    let (addr, _state) = start_server().await;
    let mut bob = Client::connect(addr, "bob").await;
    let mut carol = Client::connect(addr, "carol").await;
    let mut alice = Client::connect(addr, "alice").await;

    let id = ring(&mut bob, &mut carol).await;
    join_call(&mut bob, id).await;
    // Bob's join was broadcast; settle Alice after it and check nothing of
    // the call reached her.
    let frames = {
        alice.send(json!({ "type": "ping", "id": "after" })).await;
        alice
            .until(|f| f["type"] == "pong" && f["id"] == "after")
            .await
    };
    assert!(
        frames
            .iter()
            .all(|f| f["channelId"] != id && f["type"] != "call-ringing"),
        "{frames:?}"
    );
}

#[tokio::test]
async fn the_callee_declines_and_the_caller_cancels() {
    let (addr, state) = start_server().await;
    let mut bob = Client::connect(addr, "bob").await;
    let mut carol = Client::connect(addr, "carol").await;

    let id = ring(&mut bob, &mut carol).await;
    carol
        .send(json!({ "type": "call-end", "callId": id }))
        .await;
    assert_eq!(ended(&mut bob).await, "declined");

    let id = ring(&mut bob, &mut carol).await;
    bob.send(json!({ "type": "call-end", "callId": id })).await;
    assert_eq!(ended(&mut carol).await, "cancelled");
    assert!(state.calls.lock().await.is_empty());
}

#[tokio::test]
async fn busy_unreachable_and_self_calls_are_refused() {
    let (addr, _state) = start_server().await;
    let mut bob = Client::connect(addr, "bob").await;
    let mut carol = Client::connect(addr, "carol").await;
    let mut dave = Client::connect(addr, "dave").await;

    ring(&mut bob, &mut carol).await;
    dave.send(json!({ "type": "call-start", "target": "carol" }))
        .await;
    assert_eq!(dave.next_error().await, "call-busy");
    dave.send(json!({ "type": "call-start", "target": "dave" }))
        .await;
    assert_eq!(dave.next_error().await, "call-unavailable");
    dave.send(json!({ "type": "call-start", "target": "nobody" }))
        .await;
    assert_eq!(dave.next_error().await, "call-unavailable");
}

#[tokio::test]
async fn an_unanswered_call_rings_out_and_a_departed_callee_misses_it() {
    let (addr, state) = start_server().await;
    let mut bob = Client::connect(addr, "bob").await;
    let mut carol = Client::connect(addr, "carol").await;

    ring(&mut bob, &mut carol).await;
    assert_eq!(ended(&mut bob).await, "missed");

    ring(&mut bob, &mut carol).await;
    drop(carol);
    assert_eq!(ended(&mut bob).await, "missed");
    assert!(state.calls.lock().await.is_empty());
}
