//! Tests for the mentions inbox (`load-mentions`).
//!
//! Two ways it can go wrong, both silent. The inbox can **leak**: it reads
//! across every channel at once, so a missing visibility check would hand a
//! member the words of a private channel as long as somebody named them there.
//! And it can **disagree with the badges**: the client decides live pings with
//! `containsMention`, the server decides the inbox with `mentions_user`, and a
//! mention one of them sees and the other does not looks like a lost message.
//! The matcher cases below are the client's own cases from
//! `message-utils.test.ts`, so a drift between the two shows up here.

use std::{net::SocketAddr, sync::Arc, time::Duration};

use axum::{Router, routing::get};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signer, SigningKey};
use futures::{SinkExt, StreamExt};
use murmer_server::mentions::{is_mention_of, mentions_user};
use murmer_server::{AppState, db, ws::ws_handler};
use serde_json::{Value, json};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};

#[test]
fn matches_a_mention_anywhere_case_insensitively() {
    assert!(mentions_user("@alice hello", "alice"));
    assert!(mentions_user("hey @alice!", "alice"));
    assert!(mentions_user("hey @Alice", "alice"));
    assert!(mentions_user("hey @alice", "Alice"));
    assert!(mentions_user("thanks @mary jane, see you", "mary jane"));
}

#[test]
fn does_not_fire_on_a_longer_name_that_merely_starts_the_same() {
    assert!(!mentions_user("hey @alicia", "alice"));
    assert!(!mentions_user("hey @alice-bob", "alice"));
    assert!(!mentions_user("hey @alice_bob", "alice"));
}

#[test]
fn does_not_fire_on_an_address_that_merely_contains_the_name() {
    assert!(!mentions_user("mail bob@alice.example", "alice"));
    assert!(!mentions_user("@@alice", "alice"));
}

#[test]
fn a_member_is_never_their_own_mention_and_here_is_not_kept() {
    let own = json!({ "user": "alice", "text": "note to self @alice" });
    assert!(!is_mention_of(&own, "alice", &[]));

    let role_ping =
        json!({ "user": "bob", "enc": {}, "mentions": { "here": false, "roles": [4] } });
    assert!(is_mention_of(&role_ping, "alice", &[4]));
    assert!(!is_mention_of(&role_ping, "alice", &[5]));

    // `@here` reached whoever was connected; it is not an inbox entry later.
    let here = json!({ "user": "bob", "text": "@here", "mentions": { "here": true, "roles": [] } });
    assert!(!is_mention_of(&here, "alice", &[]));
}

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
/// channel. `ADMIN_TOKEN` is set, or every member would see every channel.
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

    /// Post in the joined channel and wait for the broadcast, so the next
    /// frame sent is ordered after the message is stored.
    async fn say(&mut self, text: &str) {
        self.send(json!({ "type": "chat", "text": text })).await;
        self.until(|f| f["type"] == "chat" && f["text"] == text)
            .await;
    }

    /// The texts in this client's inbox, newest first.
    async fn inbox(&mut self) -> Vec<String> {
        self.send(json!({ "type": "load-mentions" })).await;
        let frames = self.until(|f| f["type"] == "mentions-inbox").await;
        frames.last().unwrap()["messages"]
            .as_array()
            .expect("messages")
            .iter()
            .map(|m| m["text"].as_str().unwrap_or_default().to_string())
            .collect()
    }
}

#[tokio::test]
async fn the_inbox_lists_mentions_newest_first_and_nothing_else() {
    let addr = start_server().await;
    let mut alice = Client::connect(addr, "alice").await;
    let mut bob = Client::connect(addr, "bob").await;

    alice.say("morning @bob").await;
    alice.say("@bobby is someone else").await;
    alice.say("no mention here").await;
    bob.say("talking about @bob myself").await;
    alice.say("@BOB, lunch?").await;

    assert_eq!(bob.inbox().await, vec!["@BOB, lunch?", "morning @bob"]);
}

#[tokio::test]
async fn the_inbox_never_reaches_into_a_channel_the_member_cannot_see() {
    let addr = start_server().await;
    let mut alice = Client::connect(addr, "alice").await;
    let mut bob = Client::connect(addr, "bob").await;

    // The positive case keeps the denial honest: the query does run, and does
    // find what Bob may see.
    alice.say("@bob see you in general").await;

    alice
        .send(json!({ "type": "create-channel", "name": "secret", "private": true }))
        .await;
    let frames = alice
        .until(|f| f["type"] == "channel-add" && f["name"] == "secret")
        .await;
    let secret = frames.last().unwrap()["channelId"].as_i64().expect("id");
    alice
        .send(json!({ "type": "join", "channelId": secret }))
        .await;
    alice.say("between us: @bob is getting a raise").await;

    assert_eq!(bob.inbox().await, vec!["@bob see you in general"]);
}
