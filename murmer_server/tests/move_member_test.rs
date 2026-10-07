//! End-to-end tests for moving a member between voice channels.
//!
//! The move is a request the target's client carries out, so a move that the
//! server should have refused does not fail anywhere: the target simply
//! changes channel. The permission and hierarchy checks are the whole of the
//! protection, which is why the refusals are what these pin. They drive the
//! real `/ws` dispatch loop over a real socket, the way
//! `group_mentions_test.rs` does.

use std::collections::{HashMap, HashSet};
use std::{net::SocketAddr, sync::Arc, time::Duration};

use axum::{Router, routing::get};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signer, SigningKey};
use futures::{SinkExt, StreamExt};
use murmer_server::permissions::{DEFAULT_EVERYONE, DEFAULT_MOD, MOVE_MEMBERS};
use murmer_server::{AppState, VoiceChannelState, VoiceMode, db, ws::ws_handler};
use serde_json::{Value, json};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};

const LOUNGE: i32 = 1;
const STAGE: i32 = 2;

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
/// `MOVE_MEMBERS`; everyone else holds only `@everyone`, which does not. Two
/// voice channels exist, Lounge (1) and Stage (2), and Stage admits one.
async fn start_server() -> (SocketAddr, Arc<AppState>) {
    let database = db::init(":memory:").await.expect("in-memory db");
    db::assign_named_role(&database, &public_key("alice"), "Owner", None)
        .await
        .expect("bootstrap owner");
    let role_defs = db::list_role_defs(&database).await.expect("role defs");
    let voice = |name: &str, user_limit| VoiceChannelState {
        name: name.to_string(),
        users: HashSet::new(),
        quality: "standard".to_string(),
        bitrate: None,
        category_id: None,
        position: 0,
        breakout_parent: None,
        user_limit,
        mode: VoiceMode::Mesh,
        sticky: false,
    };
    let state = Arc::new(AppState {
        role_defs: tokio::sync::Mutex::new(role_defs.into_iter().map(|d| (d.id, d)).collect()),
        voice_channels: tokio::sync::Mutex::new(HashMap::from([
            (LOUNGE, voice("Lounge", 0)),
            (STAGE, voice("Stage", 1)),
        ])),
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

/// Join `channel` and wait until the server has recorded it.
async fn join_voice(client: &mut Client, channel: i32) {
    let name = client.name;
    client
        .send(json!({ "type": "voice-join", "channelId": channel }))
        .await;
    client
        .until(|f| f["type"] == "voice-join" && f["user"] == name)
        .await;
}

#[tokio::test]
async fn a_moderator_moves_a_member_and_it_is_logged() {
    let (addr, state) = start_server().await;
    let mut alice = Client::connect(addr, "alice").await;
    let mut bob = Client::connect(addr, "bob").await;
    join_voice(&mut bob, LOUNGE).await;

    alice
        .send(json!({ "type": "move-member", "user": "bob", "channelId": STAGE }))
        .await;
    let frames = bob.until(|f| f["type"] == "breakout-move").await;
    assert_eq!(frames.last().unwrap()["channelId"], STAGE);

    let log = db::list_audit_entries(&state.db).await.expect("audit log");
    let entry = log
        .iter()
        .find(|e| e.action == "move")
        .expect("move logged");
    assert_eq!(
        (entry.actor.as_str(), entry.target.as_str()),
        ("alice", "bob")
    );
    assert_eq!(entry.detail, "Stage");
}

#[tokio::test]
async fn a_member_without_the_permission_cannot_move_anyone() {
    let (addr, _state) = start_server().await;
    let mut bob = Client::connect(addr, "bob").await;
    let mut carol = Client::connect(addr, "carol").await;
    join_voice(&mut carol, LOUNGE).await;

    bob.send(json!({ "type": "move-member", "user": "carol", "channelId": STAGE }))
        .await;
    assert_eq!(bob.next_error().await, "moderation-permission-denied");
}

#[tokio::test]
async fn nobody_outside_voice_is_pulled_in_and_a_full_channel_is_refused() {
    let (addr, _state) = start_server().await;
    let mut alice = Client::connect(addr, "alice").await;
    let mut bob = Client::connect(addr, "bob").await;
    let mut carol = Client::connect(addr, "carol").await;

    // Carol is online but not in a call: moving her would open her
    // microphone without her asking.
    alice
        .send(json!({ "type": "move-member", "user": "carol", "channelId": STAGE }))
        .await;
    assert_eq!(alice.next_error().await, "moderation-target-not-found");

    join_voice(&mut bob, STAGE).await;
    join_voice(&mut carol, LOUNGE).await;
    alice
        .send(json!({ "type": "move-member", "user": "carol", "channelId": STAGE }))
        .await;
    assert_eq!(alice.next_error().await, "voice-channel-full");

    alice
        .send(json!({ "type": "move-member", "user": "carol", "channelId": 99 }))
        .await;
    assert_eq!(alice.next_error().await, "unknown-voice-channel");
}

#[test]
fn moderators_may_move_members_and_everyone_else_may_not() {
    assert_eq!(DEFAULT_MOD & MOVE_MEMBERS, MOVE_MEMBERS);
    assert_eq!(DEFAULT_EVERYONE & MOVE_MEMBERS, 0);
}
