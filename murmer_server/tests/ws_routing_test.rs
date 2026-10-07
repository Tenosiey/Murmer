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
//! WebRTC signaling is the other silent leak: a client answers any offer
//! naming its voice channel with its microphone, camera or screen, so a
//! relayed offer from outside the call is an eavesdropper let in.
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
/// `main` loads them. Alice is bootstrapped as Owner, the way `/role` does
/// it, so she may create a private channel.
async fn start_server() -> SocketAddr {
    start_server_with_password(None).await
}

/// [`start_server`], optionally behind `SERVER_PASSWORD`.
async fn start_server_with_password(password: Option<&str>) -> SocketAddr {
    let database = db::init(":memory:").await.expect("in-memory db");
    db::assign_named_role(&database, &public_key("alice"), "Owner", None)
        .await
        .expect("bootstrap owner");
    let role_defs = db::list_role_defs(&database).await.expect("role defs");
    serve(AppState {
        password: password.map(str::to_string),
        role_defs: tokio::sync::Mutex::new(role_defs.into_iter().map(|d| (d.id, d)).collect()),
        ..AppState::new(database)
    })
    .await
}

/// Serve `/ws` for `state` on an ephemeral port.
async fn serve(state: AppState) -> SocketAddr {
    let router = Router::new()
        .route("/ws", get(ws_handler))
        .with_state(Arc::new(state));
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
    /// Everything the server sent while authenticating, up to the pong.
    greeting: Vec<Value>,
}

impl Client {
    /// Connect and authenticate as `name`.
    async fn connect(addr: SocketAddr, name: &'static str) -> Self {
        Self::connect_with_password(addr, name, None).await
    }

    /// [`Client::connect`], presenting `password` to a protected server.
    async fn connect_with_password(
        addr: SocketAddr,
        name: &'static str,
        password: Option<&str>,
    ) -> Self {
        let (ws, _) = connect_async(format!("ws://{addr}/ws"))
            .await
            .expect("connect");
        let mut client = Self {
            name,
            ws,
            greeting: Vec::new(),
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
                "password": password,
            }))
            .await;
        // Frames are handled in order, so the pong lands after the whole
        // post-auth snapshot.
        client.send(json!({ "type": "ping", "id": name })).await;
        client.greeting = client.until(|f| f["type"] == "pong").await;
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

/// Alice creates a public voice channel and hands back its id.
async fn create_voice_channel(alice: &mut Client, name: &str) -> i64 {
    alice
        .send(json!({ "type": "create-voice-channel", "name": name }))
        .await;
    let frames = alice
        .until(|f| f["type"] == "voice-channel-add" && f["name"] == name)
        .await;
    frames.last().unwrap()["channelId"]
        .as_i64()
        .expect("voice channel id")
}

async fn join_voice(client: &mut Client, channel: i64) {
    client
        .send(json!({ "type": "voice-join", "channelId": channel }))
        .await;
    client.until(|f| f["type"] == "voice-hands-active").await;
}

/// Alice creates a public voice channel and both clients join it.
async fn shared_voice_channel(alice: &mut Client, bob: &mut Client) -> i64 {
    let channel = create_voice_channel(alice, "Lounge").await;
    join_voice(alice, channel).await;
    join_voice(bob, channel).await;
    channel
}

/// A channel's own user limit is decided on the server: the client greys
/// out a full channel, but only the refusal here keeps the mesh small.
#[tokio::test]
async fn a_full_voice_channel_refuses_the_next_joiner() {
    let addr = start_server().await;
    let mut alice = Client::connect(addr, "alice").await;
    let mut bob = Client::connect(addr, "bob").await;
    let channel = create_voice_channel(&mut alice, "Booth").await;

    alice
        .send(json!({ "type": "update-voice-channel", "channelId": channel, "userLimit": 100 }))
        .await;
    let refused = alice.until(|f| f["type"] == "error").await;
    assert_eq!(
        refused.last().unwrap()["message"],
        "invalid-voice-user-limit"
    );

    alice
        .send(json!({ "type": "update-voice-channel", "channelId": channel, "userLimit": 1 }))
        .await;
    let updated = alice
        .until(|f| f["type"] == "voice-channel-update" && f["channelId"] == channel)
        .await;
    assert_eq!(updated.last().unwrap()["userLimit"], 1);
    join_voice(&mut alice, channel).await;

    bob.send(json!({ "type": "voice-join", "channelId": channel }))
        .await;
    let seen = bob.until(|f| f["type"] == "error").await;
    assert_eq!(seen.last().unwrap()["message"], "voice-channel-full");
}

#[tokio::test]
async fn a_hand_is_raised_only_in_the_channel_its_owner_sits_in() {
    let addr = start_server().await;
    let mut alice = Client::connect(addr, "alice").await;
    let mut bob = Client::connect(addr, "bob").await;
    let channel = shared_voice_channel(&mut alice, &mut bob).await;

    // Into a channel bob is not in: refused. Then the real one: raised.
    bob.send(json!({ "type": "voice-hand", "channelId": channel + 1, "raised": true }))
        .await;
    bob.send(json!({ "type": "voice-hand", "channelId": channel, "raised": true }))
        .await;
    bob.mark("away").await;

    let seen = alice.until_mark("bob", "away").await;
    let hands = of_type(&seen, "voice-hand");
    assert_eq!(hands.len(), 1, "{seen:?}");
    assert_eq!(hands[0]["user"], "bob");
    assert_eq!(hands[0]["channelId"], channel);
    assert_eq!(hands[0]["raised"], true);
}

#[tokio::test]
async fn leaving_voice_lowers_the_hand_for_everyone_else() {
    let addr = start_server().await;
    let mut alice = Client::connect(addr, "alice").await;
    let mut bob = Client::connect(addr, "bob").await;
    let channel = shared_voice_channel(&mut alice, &mut bob).await;

    bob.send(json!({ "type": "voice-hand", "channelId": channel, "raised": true }))
        .await;
    bob.send(json!({ "type": "voice-leave", "channelId": channel }))
        .await;
    bob.mark("away").await;

    let seen = alice.until_mark("bob", "away").await;
    let raised: Vec<_> = of_type(&seen, "voice-hand")
        .into_iter()
        .map(|f| f["raised"].as_bool())
        .collect();
    assert_eq!(raised, [Some(true), Some(false)], "{seen:?}");
}

fn voice_offer(from: &str, to: &str, channel: i64) -> Value {
    json!({
        "type": "voice-offer",
        "user": from,
        "target": to,
        "channelId": channel,
        "sdp": { "type": "offer", "sdp": "v=0" },
    })
}

/// Have `outsider` send each of `frames` to bob, then let alice send bob a
/// legitimate offer, and return everything bob received up to it.
///
/// Signaling arrives through bob's direct mailbox, not the broadcast, so the
/// status mark alone proves nothing. Bob first waits for the outsider's mark
/// — by then the outsider's frames were handled and anything relayed already
/// sits in his mailbox — and alice's offer then queues behind it there.
async fn signaling_reaching_bob(
    alice: &mut Client,
    bob: &mut Client,
    outsider: &mut Client,
    frames: Vec<Value>,
    channel: i64,
) -> Vec<Value> {
    for frame in frames {
        outsider.send(frame).await;
    }
    outsider.mark("away").await;
    let mut seen = bob.until_mark(outsider.name, "away").await;

    alice.send(voice_offer("alice", "bob", channel)).await;
    seen.extend(
        bob.until(|f| f["type"] == "voice-offer" && f["user"] == "alice")
            .await,
    );
    seen
}

#[tokio::test]
async fn signaling_from_outside_voice_never_reaches_a_member() {
    let addr = start_server().await;
    let mut alice = Client::connect(addr, "alice").await;
    let mut bob = Client::connect(addr, "bob").await;
    let mut carol = Client::connect(addr, "carol").await;
    let channel = shared_voice_channel(&mut alice, &mut bob).await;

    let seen = signaling_reaching_bob(
        &mut alice,
        &mut bob,
        &mut carol,
        vec![
            voice_offer("carol", "bob", channel),
            json!({
                "type": "screenshare-offer",
                "user": "carol",
                "target": "bob",
                "channelId": channel,
                "sdp": { "type": "offer", "sdp": "v=0" },
            }),
        ],
        channel,
    )
    .await;

    let from_carol: Vec<_> = seen
        .iter()
        .filter(|f| f["user"] == "carol" && f["type"] != "status-update")
        .collect();
    assert!(
        from_carol.is_empty(),
        "carol's signaling reached bob: {from_carol:?}"
    );
}

#[tokio::test]
async fn signaling_does_not_cross_between_voice_channels() {
    let addr = start_server().await;
    let mut alice = Client::connect(addr, "alice").await;
    let mut bob = Client::connect(addr, "bob").await;
    let mut carol = Client::connect(addr, "carol").await;
    let channel = shared_voice_channel(&mut alice, &mut bob).await;
    let elsewhere = create_voice_channel(&mut alice, "Elsewhere").await;
    join_voice(&mut carol, elsewhere).await;

    // Naming bob's channel, which carol is not in; then naming her own,
    // which bob is not in.
    let seen = signaling_reaching_bob(
        &mut alice,
        &mut bob,
        &mut carol,
        vec![
            voice_offer("carol", "bob", channel),
            voice_offer("carol", "bob", elsewhere),
        ],
        channel,
    )
    .await;

    let from_carol = of_type(&seen, "voice-offer")
        .into_iter()
        .filter(|f| f["user"] == "carol")
        .count();
    assert_eq!(from_carol, 0, "carol's offer crossed channels: {seen:?}");
}

#[tokio::test]
async fn a_voice_mute_is_rebuilt_and_kept_to_the_senders_channel() {
    let addr = start_server().await;
    let mut alice = Client::connect(addr, "alice").await;
    let mut bob = Client::connect(addr, "bob").await;
    let channel = shared_voice_channel(&mut alice, &mut bob).await;

    // Into a channel bob is not in: refused. Then his own, claiming to be
    // alice and carrying a field of his own: sent as bob, without it.
    bob.send(json!({ "type": "voice-mute", "channelId": channel + 1, "micMuted": true }))
        .await;
    bob.send(json!({
        "type": "voice-mute",
        "user": "alice",
        "channelId": channel,
        "micMuted": true,
        "outputMuted": false,
        "injected": "payload",
    }))
    .await;
    bob.mark("away").await;

    let seen = alice.until_mark("bob", "away").await;
    let mutes = of_type(&seen, "voice-mute");
    assert_eq!(mutes.len(), 1, "{seen:?}");
    assert_eq!(
        *mutes[0],
        json!({
            "type": "voice-mute",
            "user": "bob",
            "channelId": channel,
            "micMuted": true,
            "outputMuted": false,
        })
    );
}

#[tokio::test]
async fn a_voice_mute_in_a_private_channel_reaches_only_who_can_see_it() {
    let addr = start_server().await;
    let mut alice = Client::connect(addr, "alice").await;
    let mut bob = Client::connect(addr, "bob").await;

    alice
        .send(json!({ "type": "create-voice-channel", "name": "Backroom", "private": true }))
        .await;
    let frames = alice
        .until(|f| f["type"] == "voice-channel-add" && f["name"] == "Backroom")
        .await;
    let channel = frames.last().unwrap()["channelId"]
        .as_i64()
        .expect("voice channel id");
    join_voice(&mut alice, channel).await;
    alice
        .send(json!({ "type": "voice-mute", "channelId": channel, "micMuted": true }))
        .await;
    alice.mark("away").await;

    let own = alice.until_mark("alice", "away").await;
    assert_eq!(of_type(&own, "voice-mute").len(), 1, "{own:?}");

    let seen = bob.until_mark("alice", "away").await;
    assert!(
        of_type(&seen, "voice-mute").is_empty(),
        "bob saw a mute in a private channel: {seen:?}"
    );
}

#[tokio::test]
async fn a_frame_beyond_the_size_limit_closes_the_connection() {
    let addr = start_server().await;
    let mut alice = Client::connect(addr, "alice").await;

    // A wiki save's worth of padding still gets through.
    let fits = "x".repeat(200 * 1024);
    alice
        .send(json!({ "type": "ping", "id": "fits", "pad": fits }))
        .await;
    alice
        .until(|f| f["type"] == "pong" && f["id"] == "fits")
        .await;

    let too_big = "x".repeat(300 * 1024);
    // The server may already have closed by the time the send completes.
    let _ = alice
        .ws
        .send(Message::text(
            json!({ "type": "ping", "pad": too_big }).to_string(),
        ))
        .await;
    loop {
        let next = tokio::time::timeout(Duration::from_secs(5), alice.ws.next())
            .await
            .expect("connection stayed open");
        match next {
            Some(Ok(Message::Text(text))) => {
                assert!(!text.contains("pong"), "oversized frame was handled");
            }
            Some(Ok(Message::Close(_)) | Err(_)) | None => break,
            Some(Ok(_)) => {}
        }
    }
}

#[tokio::test]
async fn a_client_in_a_loop_is_cut_off_after_its_burst() {
    let addr = start_server().await;
    let mut alice = Client::connect(addr, "alice").await;

    // Pings are the cheapest frame there is, which is the point: the budget
    // covers every frame, not just the ones that post something. The default
    // allows 200 at once, two of which the connect already spent.
    for id in 0..300 {
        alice.send(json!({ "type": "ping", "id": id })).await;
    }
    let seen = alice
        .until(|f| f["type"] == "error" && f["message"] == "frame-rate-limit")
        .await;
    let pongs = of_type(&seen, "pong").len();
    assert!(
        (198..300).contains(&pongs),
        "{pongs} pongs before the limit"
    );
}

/// Every frame a connection receives before the server closes it, except
/// the challenge every connection is greeted with: it is random, and it is
/// the one frame a socket is meant to receive before authenticating.
async fn frames_until_closed(ws: &mut WebSocketStream<MaybeTlsStream<TcpStream>>) -> Vec<Value> {
    let mut seen = Vec::new();
    loop {
        let next = tokio::time::timeout(Duration::from_secs(5), ws.next())
            .await
            .unwrap_or_else(|_| panic!("connection stayed open; saw {seen:?}"));
        match next {
            Some(Ok(Message::Text(text))) => {
                let frame: Value = serde_json::from_str(&text).expect("json frame");
                if frame["type"] != "auth-challenge" {
                    seen.push(frame);
                }
            }
            Some(Ok(_)) => {}
            Some(Err(_)) | None => return seen,
        }
    }
}

#[tokio::test]
async fn a_socket_that_never_authenticated_hears_nothing() {
    let addr = start_server_with_password(Some("hunter2")).await;
    // Connected first, so it would be subscribed to everything below.
    let (mut intruder, _) = connect_async(format!("ws://{addr}/ws"))
        .await
        .expect("connect");
    let mut alice = Client::connect_with_password(addr, "alice", Some("hunter2")).await;

    alice
        .send(json!({ "type": "chat", "user": "alice", "text": "members only" }))
        .await;
    alice.mark("away").await;
    alice.until_mark("alice", "away").await;

    // Any frame from an unauthenticated socket ends it, after an error.
    intruder
        .send(Message::text(json!({ "type": "ping" }).to_string()))
        .await
        .expect("send");
    let seen = frames_until_closed(&mut intruder).await;
    assert!(
        seen.iter().all(|f| f["type"] == "error"),
        "an unauthenticated socket received server traffic: {seen:?}"
    );
}

#[tokio::test]
async fn a_keyless_presence_does_not_reveal_whether_the_password_was_right() {
    let addr = start_server_with_password(Some("hunter2")).await;
    let mut answers = Vec::new();
    for password in ["hunter2", "wrong"] {
        let (mut ws, _) = connect_async(format!("ws://{addr}/ws"))
            .await
            .expect("connect");
        ws.send(Message::text(
            json!({ "type": "presence", "user": "mallory", "password": password }).to_string(),
        ))
        .await
        .expect("send");
        answers.push(frames_until_closed(&mut ws).await);
    }
    assert_eq!(answers[0], answers[1], "the answer is a password oracle");
}

/// One identity key is the account on every server, so a proof another
/// server (or another connection) could have seen must not log in here.
#[tokio::test]
async fn a_presence_proof_signed_for_another_connection_is_refused() {
    let addr = start_server().await;
    let challenge_of = |frame: Value| frame["challenge"].as_str().unwrap().to_owned();
    let (mut elsewhere, _) = connect_async(format!("ws://{addr}/ws"))
        .await
        .expect("connect");
    let Some(Ok(Message::Text(greeting))) = elsewhere.next().await else {
        panic!("no challenge");
    };
    let foreign = challenge_of(serde_json::from_str(&greeting).unwrap());

    let (mut ws, _) = connect_async(format!("ws://{addr}/ws"))
        .await
        .expect("connect");
    let signature = signing_key("alice").sign(format!("presence:{foreign}").as_bytes());
    ws.send(Message::text(
        json!({
            "type": "presence",
            "user": "alice",
            "publicKey": public_key("alice"),
            "signature": STANDARD.encode(signature.to_bytes()),
        })
        .to_string(),
    ))
    .await
    .expect("send");
    let seen = frames_until_closed(&mut ws).await;
    assert_eq!(
        seen,
        vec![json!({ "type": "error", "message": "invalid-signature" })]
    );
}

#[tokio::test]
async fn a_chat_frame_cannot_carry_a_forwarding_stamp_or_reactions() {
    let addr = start_server().await;
    let mut alice = Client::connect(addr, "alice").await;
    let mut bob = Client::connect(addr, "bob").await;

    alice
        .send(json!({
            "type": "chat",
            "user": "alice",
            "text": "I never said this",
            "forwardedFrom": { "id": 1, "user": "bob", "channel": "general", "channelId": 1 },
            "reactions": { "👍": ["bob", "carol"] },
            "edited": true,
        }))
        .await;
    let seen = bob.until(|f| f["type"] == "chat").await;
    let chat = seen.last().unwrap();
    assert!(chat.get("forwardedFrom").is_none(), "{chat}");
    assert!(chat.get("edited").is_none(), "{chat}");
    assert_eq!(chat["reactions"], json!({}), "{chat}");
}

#[tokio::test]
async fn a_member_who_cannot_see_the_default_channel_gets_none_of_it() {
    let addr = start_server().await;
    let mut alice = Client::connect(addr, "alice").await;
    // `general` is the first channel the schema creates.
    let general = 1;
    alice
        .send(json!({
            "type": "set-channel-override",
            "channelId": general,
            "target": { "type": "everyone" },
            "deny": murmer_server::permissions::VIEW_CHANNELS,
        }))
        .await;
    alice.until(|f| f["type"] == "channel-list").await;
    alice
        .send(json!({ "type": "chat", "user": "alice", "text": "old secret" }))
        .await;
    alice.until(|f| f["type"] == "chat").await;

    let mut bob = Client::connect(addr, "bob").await;
    let history: Vec<_> = of_type(&bob.greeting, "history")
        .into_iter()
        .filter(|f| f["messages"].as_array().is_some_and(|m| !m.is_empty()))
        .collect();
    assert!(
        history.is_empty(),
        "bob got the hidden history: {history:?}"
    );

    alice
        .send(json!({ "type": "chat", "user": "alice", "text": "new secret" }))
        .await;
    alice
        .send(json!({ "type": "load-thread", "rootId": 1 }))
        .await;
    alice.mark("away").await;
    bob.send(json!({ "type": "load-thread", "rootId": 1 }))
        .await;
    bob.send(json!({ "type": "ping", "id": "after-thread" }))
        .await;
    let mut seen = bob.until_mark("alice", "away").await;
    seen.extend(bob.until(|f| f["type"] == "pong").await);
    let leaked: Vec<_> = seen
        .iter()
        .filter(|f| f["type"] == "chat" || f["type"] == "thread")
        .collect();
    assert!(leaked.is_empty(), "bob read the hidden channel: {leaked:?}");
}

#[tokio::test]
async fn who_sits_in_a_private_call_is_not_in_a_newcomers_snapshot() {
    let addr = start_server().await;
    let mut alice = Client::connect(addr, "alice").await;
    alice
        .send(json!({ "type": "create-voice-channel", "name": "Backroom", "private": true }))
        .await;
    let frames = alice
        .until(|f| f["type"] == "voice-channel-add" && f["name"] == "Backroom")
        .await;
    let channel = frames.last().unwrap()["channelId"]
        .as_i64()
        .expect("voice channel id");
    join_voice(&mut alice, channel).await;

    let bob = Client::connect(addr, "bob").await;
    let leaked: Vec<_> = of_type(&bob.greeting, "voice-users")
        .into_iter()
        .filter(|f| f["channelId"] == channel)
        .collect();
    assert!(leaked.is_empty(), "bob saw the private call: {leaked:?}");
}

/// A socket that never authenticates only holds a file descriptor, and there
/// is no per-IP cap on connections, so it is closed after a grace period.
#[tokio::test]
async fn a_socket_that_never_authenticates_is_closed() {
    let addr = serve(AppState {
        auth_timeout: Duration::from_millis(200),
        ..AppState::new(db::init(":memory:").await.expect("in-memory db"))
    })
    .await;
    let (mut ws, _) = connect_async(format!("ws://{addr}/ws"))
        .await
        .expect("connect");
    let seen = frames_until_closed(&mut ws).await;
    assert_eq!(
        seen,
        vec![json!({ "type": "error", "message": "unauthenticated" })]
    );
}

/// What readers see as a message's time is the server's, not the sender's.
#[tokio::test]
async fn a_message_is_stamped_with_the_servers_time() {
    let addr = start_server().await;
    let mut alice = Client::connect(addr, "alice").await;
    let mut bob = Client::connect(addr, "bob").await;
    let before = chrono::Utc::now();

    alice
        .send(json!({
            "type": "chat",
            "user": "alice",
            "text": "from the past",
            "timestamp": "2001-01-01T00:00:00Z",
            "time": "00:00:00",
        }))
        .await;
    let seen = bob.until(|f| f["type"] == "chat").await;
    let chat = seen.last().unwrap();
    let stamped =
        chrono::DateTime::parse_from_rfc3339(chat["timestamp"].as_str().unwrap()).expect("rfc3339");
    assert!(stamped >= before - chrono::Duration::seconds(1), "{chat}");
    assert_ne!(chat["time"], "00:00:00", "{chat}");
}

/// Wiki links name their channel, so answering for a private one would let
/// anyone probe its pages by guessing slugs.
#[tokio::test]
async fn wiki_links_into_a_hidden_channel_resolve_as_missing() {
    let addr = start_server().await;
    let mut alice = Client::connect(addr, "alice").await;
    let mut bob = Client::connect(addr, "bob").await;
    let general = 1;
    alice
        .send(json!({
            "type": "wiki-create",
            "channelId": general,
            "slug": "plans",
            "title": "Plans",
            "body": "secret",
        }))
        .await;
    alice.until(|f| f["type"] == "wiki-index").await;
    alice
        .send(json!({
            "type": "set-channel-override",
            "channelId": general,
            "target": { "type": "everyone" },
            "deny": murmer_server::permissions::VIEW_CHANNELS,
        }))
        .await;
    alice.until(|f| f["type"] == "channel-list").await;

    let resolve = json!({
        "type": "wiki-resolve",
        "requestId": 1,
        "links": [{ "channel": "general", "slug": "plans" }],
    });
    for (client, expected) in [(&mut alice, true), (&mut bob, false)] {
        client.send(resolve.clone()).await;
        let seen = client.until(|f| f["type"] == "wiki-resolved").await;
        assert_eq!(
            seen.last().unwrap()["results"][0]["exists"],
            expected,
            "{}",
            client.name
        );
    }
}

#[tokio::test]
async fn a_poke_reaches_only_its_target_and_is_paced() {
    let addr = start_server().await;
    let mut alice = Client::connect(addr, "alice").await;
    let mut bob = Client::connect(addr, "bob").await;
    let mut carol = Client::connect(addr, "carol").await;

    alice.send(json!({ "type": "poke", "target": "bob" })).await;
    let seen = bob.until(|f| f["type"] == "poke").await;
    assert_eq!(seen.last().unwrap()["from"], "alice");

    // Inside the cooldown the second poke is refused, not delivered.
    alice.send(json!({ "type": "poke", "target": "bob" })).await;
    let refused = alice.until(|f| f["type"] == "error").await;
    assert_eq!(refused.last().unwrap()["message"], "poke-cooldown");

    // Nobody can poke themselves or someone who is not online.
    carol
        .send(json!({ "type": "poke", "target": "carol" }))
        .await;
    let refused = carol.until(|f| f["type"] == "error").await;
    assert_eq!(refused.last().unwrap()["message"], "poke-unavailable");
    carol
        .send(json!({ "type": "poke", "target": "nobody" }))
        .await;
    let refused = carol.until(|f| f["type"] == "error").await;
    assert_eq!(refused.last().unwrap()["message"], "poke-unavailable");

    alice.mark("away").await;
    let seen = carol.until_mark("alice", "away").await;
    assert!(
        of_type(&seen, "poke").is_empty(),
        "a poke is direct: {seen:?}"
    );
    let seen = bob.until_mark("alice", "away").await;
    assert!(of_type(&seen, "poke").is_empty(), "the refused poke leaked");
}

#[tokio::test]
async fn a_status_line_is_broadcast_and_its_expiry_checked() {
    let addr = start_server().await;
    let mut alice = Client::connect(addr, "alice").await;
    let mut bob = Client::connect(addr, "bob").await;

    let now = chrono::Utc::now().timestamp_millis();
    alice
        .send(json!({ "type": "set-status-text", "text": "back at 3", "expiresAt": now + 60_000 }))
        .await;
    let seen = bob
        .until(|f| f["type"] == "profile-update" && f["profile"]["user"] == "alice")
        .await;
    let profile = &seen.last().unwrap()["profile"];
    assert_eq!(profile["statusText"], "back at 3");
    assert_eq!(profile["statusExpiresAt"], now + 60_000);

    // An expiry in the past, or one beyond a week, is refused outright.
    for expires in [now - 1, now + 8 * 24 * 60 * 60 * 1000] {
        alice
            .send(json!({ "type": "set-status-text", "text": "x", "expiresAt": expires }))
            .await;
        let refused = alice.until(|f| f["type"] == "error").await;
        assert_eq!(refused.last().unwrap()["message"], "invalid-status-text");
    }

    alice
        .send(json!({ "type": "set-status-text", "text": "" }))
        .await;
    let seen = bob
        .until(|f| f["type"] == "profile-update" && f["profile"]["user"] == "alice")
        .await;
    let profile = &seen.last().unwrap()["profile"];
    assert_eq!(profile["statusText"], "");
    assert!(profile["statusExpiresAt"].is_null());
}

/// On an open server a keyless presence used to take any name nobody had
/// bound yet. When the real owner bound it later, both sockets ran as that
/// name, and the keyless one acted with every role the owner was given.
#[tokio::test]
async fn a_keyless_presence_is_refused_on_an_open_server() {
    let addr = start_server().await;
    let (mut ws, _) = connect_async(format!("ws://{addr}/ws"))
        .await
        .expect("connect");
    ws.send(Message::text(
        json!({ "type": "presence", "user": "dave" }).to_string(),
    ))
    .await
    .expect("send");
    assert_eq!(
        frames_until_closed(&mut ws).await,
        vec![json!({ "type": "error", "message": "invalid-signature" })]
    );
}

/// A manager may reorder only roles below them, and doing so must never lift
/// one to or past them. Numbering the list n..1 did, given enough roles tied
/// below the manager (which `create-role` used to produce): the sockpuppet's
/// role landed above Admin and could ban one.
#[tokio::test]
async fn a_role_reorder_never_lifts_a_role_past_the_manager() {
    use murmer_server::permissions::{BAN_MEMBERS, DEFAULT_EVERYONE, MANAGE_ROLES};
    let database = db::init(":memory:").await.expect("in-memory db");
    db::assign_named_role(&database, &public_key("carol"), "Admin", None)
        .await
        .expect("admin");
    let mask = DEFAULT_EVERYONE | MANAGE_ROLES | BAN_MEMBERS;
    let manager = db::create_role_def(&database, "Manager", None, mask, 2)
        .await
        .expect("manager");
    let mut tied = Vec::new();
    for name in ["r1", "r2", "r3"] {
        let id = db::create_role_def(&database, name, None, mask, 1)
            .await
            .expect("role");
        tied.push((id, 1));
    }
    // Manager level with Admin, every other custom role tied below.
    let admin = db::get_role_def_by_name(&database, "Admin")
        .await
        .expect("query")
        .expect("admin");
    let mod_id = db::get_role_def_by_name(&database, "Mod")
        .await
        .expect("query")
        .expect("mod")
        .id;
    let mut layout = tied.clone();
    layout.extend([(mod_id, 1), (manager, 2), (admin.id, 2)]);
    db::set_role_positions(&database, layout)
        .await
        .expect("layout");
    db::set_user_roles(&database, &public_key("mallory"), &[manager])
        .await
        .expect("assign manager");
    db::set_user_roles(&database, &public_key("sock"), &[tied[0].0])
        .await
        .expect("assign sock");
    let role_defs = db::list_role_defs(&database).await.expect("role defs");
    let addr = serve(AppState {
        role_defs: tokio::sync::Mutex::new(role_defs.into_iter().map(|d| (d.id, d)).collect()),
        ..AppState::new(database)
    })
    .await;

    let _carol = Client::connect(addr, "carol").await;
    let mut sock = Client::connect(addr, "sock").await;
    let mut mallory = Client::connect(addr, "mallory").await;

    let ids: Vec<i64> = tied.iter().map(|(id, _)| *id).chain([mod_id]).collect();
    mallory
        .send(json!({ "type": "reorder-roles", "orderedIds": ids }))
        .await;
    let frames = mallory.until(|f| f["type"] == "role-definitions").await;
    let roles = frames.last().unwrap()["roles"].as_array().unwrap().clone();
    let pos = |id: i64| {
        roles.iter().find(|r| r["id"] == id).unwrap()["position"]
            .as_i64()
            .unwrap()
    };
    for id in &ids {
        assert!(
            pos(*id) < pos(manager),
            "role {id} was lifted past the manager"
        );
    }

    sock.send(json!({ "type": "ban-user", "user": "carol" }))
        .await;
    let seen = sock.until(|f| f["type"] == "error").await;
    assert!(of_type(&seen, "force-disconnect").is_empty());
}

/// A profile change reaches every connection, so a muted member could keep
/// talking to everyone through their status line or display name.
#[tokio::test]
async fn a_muted_member_cannot_publish_through_their_profile() {
    let addr = start_server().await;
    let mut alice = Client::connect(addr, "alice").await;
    let mut bob = Client::connect(addr, "bob").await;

    alice
        .send(json!({ "type": "mute-user", "user": "bob" }))
        .await;
    bob.until(|f| f["type"] == "user-muted").await;

    for frame in [
        json!({ "type": "set-status-text", "text": "still here" }),
        json!({ "type": "set-profile", "displayName": "still here" }),
        json!({ "type": "set-nickname", "nickname": "still here" }),
    ] {
        bob.send(frame).await;
        let refused = bob.until(|f| f["type"] == "error").await;
        assert_eq!(refused.last().unwrap()["message"], "muted");
        assert!(of_type(&refused, "profile-update").is_empty());
    }
}

/// A ban takes the banned member's queue with it; otherwise everything they
/// scheduled before the ban still posts on time.
#[tokio::test]
async fn a_ban_clears_the_banned_members_scheduled_messages() {
    let database = db::init(":memory:").await.expect("in-memory db");
    db::assign_named_role(&database, &public_key("alice"), "Owner", None)
        .await
        .expect("bootstrap owner");
    let role_defs = db::list_role_defs(&database).await.expect("role defs");
    let addr = serve(AppState {
        role_defs: tokio::sync::Mutex::new(role_defs.into_iter().map(|d| (d.id, d)).collect()),
        ..AppState::new(database.clone())
    })
    .await;
    let mut alice = Client::connect(addr, "alice").await;
    let _bob = Client::connect(addr, "bob").await;

    let channel = db::get_channel_id_by_name(&database, "general")
        .await
        .expect("general");
    let body = json!({ "type": "chat", "user": "bob", "text": "later" }).to_string();
    db::insert_scheduled_message(
        &database,
        "bob",
        channel,
        &body,
        chrono::Utc::now() + chrono::Duration::days(30),
        25,
    )
    .await
    .expect("schedule")
    .expect("under the cap");

    alice
        .send(json!({ "type": "ban-user", "user": "bob" }))
        .await;
    alice.until(|f| f["type"] == "force-disconnect").await;

    assert!(
        db::get_scheduled_messages(&database, "bob")
            .await
            .expect("list")
            .is_empty()
    );
}
