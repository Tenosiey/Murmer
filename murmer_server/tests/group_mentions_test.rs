//! End-to-end tests for group mentions: `@here` and role pings.
//!
//! Recipients ping on a message's `mentions` field, never on its text, so
//! this field is the whole of the feature's spam protection — and every way
//! it could leak past `MENTION_GROUPS` is silent. A member without the flag
//! whose ping got through would simply notify the whole server; a scheduled
//! or forwarded body that kept a ping would fire it later, on behalf of
//! somebody who may no longer hold the permission. Nothing errors in either
//! case. These drive the real `/ws` dispatch loop over a real socket, the way
//! `ws_routing_test.rs` does.

use std::{net::SocketAddr, sync::Arc, time::Duration};

use axum::{Router, routing::get};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signer, SigningKey};
use futures::{SinkExt, StreamExt};
use murmer_server::permissions::{DEFAULT_EVERYONE, DEFAULT_MOD, MENTION_GROUPS};
use murmer_server::ws::helpers::forwarded_body;
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
/// `MENTION_GROUPS`; everyone else holds only `@everyone`, which does not.
async fn start_server() -> (SocketAddr, Arc<AppState>) {
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

/// The id of the built-in role called `name`.
async fn role_id(state: &Arc<AppState>, name: &str) -> i64 {
    state
        .role_defs
        .lock()
        .await
        .values()
        .find(|def| def.name == name)
        .map(|def| def.id)
        .unwrap_or_else(|| panic!("no role named {name}"))
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

    /// Send `frame`, then a status change everyone receives. Every server-wide
    /// frame the first one caused — a `message-notify` — reaches the others
    /// before the mark does, so "nothing arrived before the mark" is a
    /// meaningful assertion without sleeping. The `chat` frame itself travels
    /// on the channel's own broadcast and is *not* ordered with the mark; see
    /// [`Client::until_mark_and_chat`].
    async fn send_then_mark(&mut self, frame: Value, status: &str) {
        self.send(frame).await;
        self.send(json!({ "type": "status-update", "status": status }))
            .await;
    }

    async fn until_mark(&mut self, sender: &str, status: &str) -> Vec<Value> {
        self.until(|f| f["type"] == "status-update" && f["user"] == sender && f["status"] == status)
            .await
    }

    /// Frames up to `sender`'s mark, plus the `chat` frame from `sender` if
    /// the channel broadcast delivered it after the mark.
    async fn until_mark_and_chat(&mut self, sender: &str, status: &str) -> Vec<Value> {
        let mut seen = self.until_mark(sender, status).await;
        if !seen
            .iter()
            .any(|f| f["type"] == "chat" && f["user"] == sender)
        {
            seen.extend(
                self.until(|f| f["type"] == "chat" && f["user"] == sender)
                    .await,
            );
        }
        seen
    }
}

fn of_type<'a>(frames: &'a [Value], ty: &str) -> Vec<&'a Value> {
    frames.iter().filter(|f| f["type"] == ty).collect()
}

#[tokio::test]
async fn a_member_without_the_permission_cannot_ping_a_group() {
    let (addr, _) = start_server().await;
    let mut alice = Client::connect(addr, "alice").await;
    let mut bob = Client::connect(addr, "bob").await;

    bob.send_then_mark(
        json!({ "type": "chat", "text": "@here look", "mentions": { "here": true } }),
        "away",
    )
    .await;
    assert_eq!(bob.next_error().await, "group-mention-denied");

    // Refused outright rather than delivered without the ping: the sender
    // must not believe they notified anyone.
    let seen = alice.until_mark("bob", "away").await;
    assert!(of_type(&seen, "chat").is_empty(), "{seen:?}");
    assert!(of_type(&seen, "message-notify").is_empty(), "{seen:?}");
}

#[tokio::test]
async fn a_permitted_ping_reaches_everyone_rebuilt_from_known_parts() {
    let (addr, state) = start_server().await;
    let mod_role = role_id(&state, "Mod").await;
    let mut alice = Client::connect(addr, "alice").await;
    let mut bob = Client::connect(addr, "bob").await;

    alice
        .send_then_mark(
            json!({
                "type": "chat",
                "text": "@here and @Mod",
                "mentions": { "here": true, "roles": [mod_role, mod_role], "users": ["x"] },
            }),
            "away",
        )
        .await;

    let seen = bob.until_mark_and_chat("alice", "away").await;
    let expected = json!({ "here": true, "roles": [mod_role] });
    let chat = of_type(&seen, "chat");
    assert_eq!(chat.len(), 1, "{seen:?}");
    assert_eq!(chat[0]["mentions"], expected);
    // The notify is what reaches members looking at another channel, so it
    // has to carry the ping too.
    let notify = of_type(&seen, "message-notify");
    assert_eq!(notify.len(), 1, "{seen:?}");
    assert_eq!(notify[0]["mentions"], expected);
}

#[tokio::test]
async fn a_malformed_or_unknown_group_is_refused() {
    let (addr, state) = start_server().await;
    let everyone = role_id(&state, "@everyone").await;
    let mut alice = Client::connect(addr, "alice").await;

    for mentions in [
        json!({ "roles": [everyone] }),
        json!({ "roles": [9999] }),
        json!({ "roles": ["Mod"] }),
        json!({ "here": "yes" }),
        json!(["here"]),
    ] {
        alice
            .send(json!({ "type": "chat", "text": "hi", "mentions": mentions }))
            .await;
        assert_eq!(alice.next_error().await, "invalid-mentions", "{mentions}");
    }
}

#[tokio::test]
async fn the_text_alone_pings_nobody() {
    let (addr, _) = start_server().await;
    let mut alice = Client::connect(addr, "alice").await;
    let mut bob = Client::connect(addr, "bob").await;

    // Typing "@here" is allowed to anyone; it is only words without the field.
    bob.send_then_mark(json!({ "type": "chat", "text": "@here hi" }), "away")
        .await;
    let seen = alice.until_mark_and_chat("bob", "away").await;
    let chat = of_type(&seen, "chat");
    assert_eq!(chat.len(), 1, "{seen:?}");
    assert!(chat[0].get("mentions").is_none(), "{chat:?}");

    // An empty field pings nobody either, so it needs no permission.
    bob.send_then_mark(
        json!({ "type": "chat", "text": "hello", "mentions": { "here": false, "roles": [] } }),
        "busy",
    )
    .await;
    let seen = alice.until_mark_and_chat("bob", "busy").await;
    let chat = of_type(&seen, "chat");
    assert_eq!(chat.len(), 1, "{seen:?}");
    assert!(chat[0].get("mentions").is_none(), "{chat:?}");
}

#[tokio::test]
async fn a_scheduled_message_never_carries_a_group_ping() {
    let (addr, state) = start_server().await;
    let general = db::get_channels(&state.db)
        .await
        .into_iter()
        .find(|c| c.name == "general")
        .expect("general")
        .id;
    let mut alice = Client::connect(addr, "alice").await;

    let at = (chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339();
    alice
        .send(json!({
            "type": "schedule-message",
            "channelId": general,
            "text": "@here later",
            "scheduledFor": at,
            "mentions": { "here": true },
        }))
        .await;
    alice.until(|f| f["type"] == "scheduled-messages").await;

    let stored = db::get_scheduled_messages(&state.db, "alice")
        .await
        .expect("scheduled");
    assert_eq!(stored.len(), 1);
    let body: Value = serde_json::from_str(&stored[0].body).expect("body");
    assert!(body.get("mentions").is_none(), "{body}");
}

#[test]
fn a_forward_drops_the_ping() {
    // The forwarder may not hold the permission, and the original ping was
    // for the original channel.
    let source = json!({
        "user": "alice",
        "text": "@here look",
        "mentions": { "here": true, "roles": [] },
    });
    let body = forwarded_body(7, &source, 1, "general").expect("forward body");
    assert!(body.get("mentions").is_none(), "{body}");
}

#[test]
fn moderators_may_ping_groups_and_everyone_else_may_not() {
    assert_eq!(DEFAULT_MOD & MENTION_GROUPS, MENTION_GROUPS);
    assert_eq!(DEFAULT_EVERYONE & MENTION_GROUPS, 0);
}
