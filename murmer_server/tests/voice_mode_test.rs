//! End-to-end tests for the voice channel mode (mesh or SFU).
//!
//! The mode is the server's decision and clients only follow it, so a frame
//! that reaches the wrong people or never arrives fails nowhere visible: a
//! client just keeps the topology it had. These pin who hears about a
//! switch, that the hysteresis gap holds, and that a joiner learns the mode
//! from its `voice-permissions`. The SFU itself is the server's bandwidth,
//! so they also pin that only a member of an SFU-mode channel may offer to
//! it, and that only a pair inside a channel can push it there early.

use std::collections::{HashMap, HashSet};
use std::{net::SocketAddr, sync::Arc, time::Duration};

use axum::{Router, routing::get};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signer, SigningKey};
use futures::{SinkExt, StreamExt};
use murmer_server::{
    AppState, VoiceChannelState, VoiceMode, db, security::SfuThresholds, sfu::Sfu, ws::ws_handler,
};
use serde_json::{Value, json};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};

const LOUNGE: i32 = 1;

/// A key derived from the name, so every run binds the same identity.
fn signing_key(name: &str) -> SigningKey {
    let mut seed = [0u8; 32];
    seed[..name.len()].copy_from_slice(name.as_bytes());
    SigningKey::from_bytes(&seed)
}

fn public_key(name: &str) -> String {
    STANDARD.encode(signing_key(name).verifying_key().to_bytes())
}

/// Serve `/ws` with one voice channel, Lounge, and a running SFU with its
/// threshold at 4, so a channel moves to the SFU at four and back to the
/// mesh at one.
async fn start_server() -> SocketAddr {
    let (sfu, outbox) = Sfu::spawn("127.0.0.1".parse().unwrap(), 0)
        .await
        .expect("sfu");
    let database = db::init(":memory:").await.expect("in-memory db");
    let role_defs = db::list_role_defs(&database).await.expect("role defs");
    let lounge = VoiceChannelState {
        name: "Lounge".to_string(),
        users: HashSet::new(),
        quality: "standard".to_string(),
        bitrate: None,
        category_id: None,
        position: 0,
        breakout_parent: None,
        user_limit: 0,
        mode: VoiceMode::Mesh,
        sticky: false,
    };
    let state = Arc::new(AppState {
        role_defs: tokio::sync::Mutex::new(role_defs.into_iter().map(|d| (d.id, d)).collect()),
        voice_channels: tokio::sync::Mutex::new(HashMap::from([(LOUNGE, lounge)])),
        sfu_thresholds: Some(SfuThresholds {
            to_sfu: 4,
            to_mesh: 1,
        }),
        sfu: Some(sfu),
        ..AppState::new(database)
    });
    murmer_server::sfu::deliver(Arc::clone(&state), outbox);
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
}

/// Every `voice-mode` frame received before the client's next pong.
async fn modes_seen(client: &mut Client) -> Vec<Value> {
    let id = format!("{}-sync", client.name);
    client.send(json!({ "type": "ping", "id": id })).await;
    client
        .until(|f| f["type"] == "pong")
        .await
        .into_iter()
        .filter(|f| f["type"] == "voice-mode")
        .collect()
}

/// Every `voice-mode` frame up to and including the next one.
async fn next_modes(client: &mut Client) -> Vec<Value> {
    client
        .until(|f| f["type"] == "voice-mode")
        .await
        .into_iter()
        .filter(|f| f["type"] == "voice-mode")
        .collect()
}

#[tokio::test]
async fn members_follow_the_mode_and_outsiders_never_hear_it() {
    let addr = start_server().await;
    let names = ["alice", "bob", "carol", "dave"];
    let mut members = Vec::new();
    for name in names {
        members.push(Client::connect(addr, name).await);
    }
    let mut outsider = Client::connect(addr, "erin").await;

    // A joiner learns the mode from its answer: three is still a mesh...
    for (i, member) in members.iter_mut().enumerate() {
        member
            .send(json!({ "type": "voice-join", "channelId": LOUNGE }))
            .await;
        let frames = member.until(|f| f["type"] == "voice-permissions").await;
        let want = if i < 3 { "mesh" } else { "sfu" };
        assert_eq!(frames.last().unwrap()["mode"], want, "{}", member.name);
    }
    // ...and the fourth flips the channel for everyone in it, the joiner
    // included, before the joiner's answer is built.
    for member in &mut members {
        let modes = next_modes(member).await;
        assert_eq!(modes.len(), 1, "{} saw {modes:?}", member.name);
        assert_eq!(modes[0]["mode"], "sfu");
        assert_eq!(modes[0]["channelId"], LOUNGE);
        // One receive slot per other member the channel could hold, under
        // the default cap of 10.
        assert_eq!(modes[0]["slots"], 9);
    }
    assert!(modes_seen(&mut outsider).await.is_empty());

    // Down to two: inside the gap, so still the SFU.
    for member in &mut members[2..] {
        member
            .send(json!({ "type": "voice-leave", "channelId": LOUNGE }))
            .await;
    }
    members[0]
        .until(|f| f["type"] == "voice-users" && f["users"].as_array().unwrap().len() == 2)
        .await;
    assert!(modes_seen(&mut members[0]).await.is_empty());

    members[1]
        .send(json!({ "type": "voice-leave", "channelId": LOUNGE }))
        .await;
    let modes = next_modes(&mut members[0]).await;
    assert_eq!(modes[0]["mode"], "mesh");
    // Whoever left is no longer a member, so is not told.
    assert!(modes_seen(&mut members[1]).await.is_empty());
}

/// An `sfu-offer` is only taken from a member of a channel the server has
/// put on the SFU; the SFU's bandwidth is the server's to spend.
#[tokio::test]
async fn sfu_offers_need_membership_and_sfu_mode() {
    let addr = start_server().await;
    let mut alice = Client::connect(addr, "alice").await;
    let mut outsider = Client::connect(addr, "erin").await;
    let offer = json!({ "type": "sfu-offer", "channelId": LOUNGE, "sdp": "v=0" });

    // Not in the channel at all.
    outsider.send(offer.clone()).await;
    let frames = outsider.until(|f| f["type"] == "error").await;
    assert_eq!(frames.last().unwrap()["message"], "sfu-offer-rejected");

    // In the channel, but it is still a mesh.
    alice
        .send(json!({ "type": "voice-join", "channelId": LOUNGE }))
        .await;
    alice.until(|f| f["type"] == "voice-permissions").await;
    alice.send(offer).await;
    let frames = alice.until(|f| f["type"] == "error").await;
    assert_eq!(frames.last().unwrap()["message"], "sfu-offer-rejected");
    assert!(frames.iter().all(|f| f["type"] != "sfu-answer"));
}

/// `voice-p2p-failed` moves a small channel to the SFU for good, but only
/// when both ends of the failed pair are in the sender's channel.
#[tokio::test]
async fn a_failed_mesh_pair_moves_its_channel_to_the_sfu_until_it_empties() {
    let addr = start_server().await;
    let mut alice = Client::connect(addr, "alice").await;
    let mut bob = Client::connect(addr, "bob").await;
    let mut outsider = Client::connect(addr, "erin").await;
    for member in [&mut alice, &mut bob] {
        member
            .send(json!({ "type": "voice-join", "channelId": LOUNGE }))
            .await;
        member.until(|f| f["type"] == "voice-permissions").await;
    }
    let failed =
        |target: &str| json!({ "type": "voice-p2p-failed", "channelId": LOUNGE, "target": target });

    // Someone outside the call cannot push it onto the server, and a member
    // cannot name a pair that is not in the channel.
    outsider.send(failed("alice")).await;
    alice.send(failed("erin")).await;
    assert!(modes_seen(&mut alice).await.is_empty());
    assert!(modes_seen(&mut bob).await.is_empty());

    // A real pair moves both members, well under the threshold of four.
    alice.send(failed("bob")).await;
    for member in [&mut alice, &mut bob] {
        let modes = next_modes(member).await;
        assert_eq!(modes.len(), 1, "{} saw {modes:?}", member.name);
        assert_eq!(modes[0]["mode"], "sfu");
    }
    // The other end reporting the same pair changes nothing.
    bob.send(failed("alice")).await;
    assert!(modes_seen(&mut alice).await.is_empty());

    // Down to one, below the hysteresis gap, and still on the SFU.
    bob.send(json!({ "type": "voice-leave", "channelId": LOUNGE }))
        .await;
    alice
        .until(|f| f["type"] == "voice-users" && f["users"].as_array().unwrap().len() == 1)
        .await;
    assert!(modes_seen(&mut alice).await.is_empty());

    // Emptying the channel clears it: the next pair starts on the mesh.
    alice
        .send(json!({ "type": "voice-leave", "channelId": LOUNGE }))
        .await;
    alice
        .until(|f| f["type"] == "voice-users" && f["users"].as_array().unwrap().is_empty())
        .await;
    bob.send(json!({ "type": "voice-join", "channelId": LOUNGE }))
        .await;
    let frames = bob.until(|f| f["type"] == "voice-permissions").await;
    assert_eq!(frames.last().unwrap()["mode"], "mesh");
}
