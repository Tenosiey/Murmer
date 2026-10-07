//! The SFU's forwarding, end to end over loopback UDP.
//!
//! Talk is only a hint on the mesh, but on the SFU it is enforcement: the
//! server simply stops forwarding a member's audio. A forwarding rule that
//! is wrong fails silently — the room just hears someone it should not, or
//! nobody at all — and no browser test sees it before a user does. str0m can
//! play the client as well as the server, so two in-process peers connect
//! to the real SFU task here, and the test asserts what actually arrives.

use std::net::UdpSocket;
use std::time::{Duration, Instant};

use murmer_server::sfu::{Member, Outbox, Sfu};
use serde_json::Value;
use str0m::change::{SdpAnswer, SdpPendingOffer};
use str0m::media::{Direction, Frequency, MediaKind, MediaTime, Mid};
use str0m::net::{Protocol, Receive};
use str0m::{Candidate, Event, Input, Output, Rtc};

const CHANNEL: i32 = 1;

/// One side of a call: a str0m `Rtc` playing the browser, offering the way
/// the client does — a send-only audio and video, then one receive-only
/// pair per slot.
struct Peer {
    name: &'static str,
    rtc: Rtc,
    socket: UdpSocket,
    send_audio: Mid,
    /// Audio packets received on any slot.
    heard: usize,
    sent: u64,
}

impl Peer {
    fn offer(name: &'static str, slots: usize) -> (Self, String, SdpPendingOffer) {
        let mut medias = vec![
            (MediaKind::Audio, Direction::SendOnly),
            (MediaKind::Video, Direction::SendOnly),
        ];
        for _ in 0..slots {
            medias.push((MediaKind::Audio, Direction::RecvOnly));
            medias.push((MediaKind::Video, Direction::RecvOnly));
        }
        Self::with_media(name, &medias)
    }

    /// Offer exactly `medias`, in order. The first audio is what `speak`
    /// sends on, if it sends; everything else counts as heard.
    fn with_media(
        name: &'static str,
        medias: &[(MediaKind, Direction)],
    ) -> (Self, String, SdpPendingOffer) {
        let socket = UdpSocket::bind("127.0.0.1:0").expect("bind");
        socket.set_nonblocking(true).expect("nonblocking");
        let mut rtc = Rtc::new(Instant::now());
        rtc.add_local_candidate(Candidate::host(socket.local_addr().unwrap(), "udp").unwrap());
        let mut change = rtc.sdp_api();
        let mids: Vec<Mid> = medias
            .iter()
            .map(|&(kind, dir)| change.add_media(kind, dir, None, None, None))
            .collect();
        let send_audio = medias
            .iter()
            .zip(&mids)
            .find(|((kind, dir), _)| *kind == MediaKind::Audio && *dir == Direction::SendOnly)
            .map_or(mids[0], |(_, mid)| *mid);
        let (offer, pending) = change.apply().expect("an offer");
        let peer = Self {
            name,
            rtc,
            socket,
            send_audio,
            heard: 0,
            sent: 0,
        };
        (peer, offer.to_sdp_string(), pending)
    }

    /// Feed in what arrived, let time pass, and send what is due.
    fn drive(&mut self) {
        let mut buf = [0u8; 2000];
        let local = self.socket.local_addr().unwrap();
        while let Ok((n, source)) = self.socket.recv_from(&mut buf) {
            let Ok(contents) = buf[..n].try_into() else {
                continue;
            };
            let input = Input::Receive(
                Instant::now(),
                Receive {
                    proto: Protocol::Udp,
                    source,
                    destination: local,
                    contents,
                },
            );
            self.rtc.handle_input(input).expect("input");
        }
        self.rtc
            .handle_input(Input::Timeout(Instant::now()))
            .expect("timeout");
        loop {
            match self.rtc.poll_output().expect("output") {
                Output::Timeout(_) => break,
                Output::Transmit(t) => {
                    let _ = self.socket.send_to(&t.contents, t.destination);
                }
                Output::Event(Event::MediaData(data)) if data.mid != self.send_audio => {
                    self.heard += 1;
                }
                Output::Event(_) => {}
            }
        }
    }

    /// Send one 20 ms Opus frame. The bytes are not real Opus; the SFU
    /// forwards payloads without decoding them.
    fn speak(&mut self) {
        if !self.rtc.is_connected() {
            return;
        }
        let Some(writer) = self.rtc.writer(self.send_audio) else {
            return;
        };
        let pt = writer.payload_params().next().expect("a codec").pt();
        let time = MediaTime::new(self.sent * 960, Frequency::FORTY_EIGHT_KHZ);
        writer
            .write(pt, Instant::now(), time, vec![0xfc, 0xff, 0xfe])
            .expect("write");
        self.sent += 1;
    }
}

fn member(user: &str, talk: bool) -> Member {
    Member {
        user: user.to_string(),
        talk,
        camera: false,
        screen: false,
    }
}

/// Every frame the SFU has queued for delivery so far.
fn drain(outbox: &mut Outbox) -> Vec<(String, Value)> {
    std::iter::from_fn(|| outbox.try_recv().ok())
        .map(|(user, frame)| (user, serde_json::from_str(frame.as_str()).unwrap()))
        .collect()
}

/// Run both peers for `time`, with alice speaking throughout.
async fn run(alice: &mut Peer, bob: &mut Peer, time: Duration) {
    let end = Instant::now() + time;
    let mut next_frame = Instant::now();
    while Instant::now() < end {
        if Instant::now() >= next_frame {
            alice.speak();
            next_frame += Duration::from_millis(20);
        }
        alice.drive();
        bob.drive();
        // Yields to the SFU task as well as pacing the loop.
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
}

/// Connect alice and bob through a fresh SFU, each with one slot.
async fn connect() -> (Sfu, Outbox, Peer, Peer) {
    let (sfu, mut outbox) = Sfu::spawn("127.0.0.1".parse().unwrap(), 0)
        .await
        .expect("spawn");
    sfu.sync(CHANNEL, vec![member("alice", true), member("bob", true)]);
    let mut peers = Vec::new();
    for name in ["alice", "bob"] {
        let (mut peer, sdp, pending) = Peer::offer(name, 1);
        sfu.offer(name, CHANNEL, sdp, 1);
        let answer = loop {
            tokio::time::sleep(Duration::from_millis(5)).await;
            if let Some((_, frame)) = drain(&mut outbox)
                .into_iter()
                .find(|(user, f)| user == name && f["type"] == "sfu-answer")
            {
                break frame;
            }
        };
        let answer = SdpAnswer::from_sdp_string(answer["sdp"].as_str().unwrap()).unwrap();
        peer.rtc
            .sdp_api()
            .accept_answer(pending, answer)
            .expect("answer");
        peers.push(peer);
    }
    let bob = peers.pop().unwrap();
    let alice = peers.pop().unwrap();
    (sfu, outbox, alice, bob)
}

#[tokio::test]
async fn audio_reaches_the_channel_only_while_the_sender_may_talk() {
    let (sfu, mut outbox, mut alice, mut bob) = connect().await;
    run(&mut alice, &mut bob, Duration::from_secs(2)).await;
    assert!(alice.rtc.is_connected() && bob.rtc.is_connected());
    assert!(bob.heard > 0, "bob heard nothing of {} frames", alice.sent);

    // Bob was told which slot carries alice.
    let frames = drain(&mut outbox);
    let slots = frames
        .iter()
        .rev()
        .find(|(user, f)| user == "bob" && f["type"] == "sfu-slots")
        .map(|(_, f)| f["slots"].clone())
        .expect("bob's slot map");
    assert!(
        slots
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["user"] == "alice" && s["kind"] == "audio"),
        "{slots}"
    );

    // Alice loses Talk: the server stops forwarding, whatever her client
    // keeps sending.
    sfu.sync(CHANNEL, vec![member("alice", false), member("bob", true)]);
    run(&mut alice, &mut bob, Duration::from_millis(300)).await;
    bob.heard = 0;
    let sent = alice.sent;
    run(&mut alice, &mut bob, Duration::from_secs(1)).await;
    assert!(alice.sent > sent);
    assert_eq!(bob.heard, 0, "{} kept talking without Talk", alice.name);

    // Alice leaves the channel: her connection is dropped server-side.
    sfu.sync(CHANNEL, vec![member("bob", true)]);
    run(&mut alice, &mut bob, Duration::from_millis(300)).await;
    let slots = drain(&mut outbox)
        .into_iter()
        .rev()
        .find(|(user, f)| user == "bob" && f["type"] == "sfu-slots")
        .map(|(_, f)| f["slots"].clone())
        .expect("bob's slot map after alice left");
    assert_eq!(slots, serde_json::json!([]), "{}", bob.name);
}

#[tokio::test]
async fn an_offer_from_outside_the_channel_gets_no_answer() {
    let (sfu, mut outbox) = Sfu::spawn("127.0.0.1".parse().unwrap(), 0)
        .await
        .expect("spawn");
    sfu.sync(CHANNEL, vec![member("alice", true)]);
    let (_mallory, sdp, _) = Peer::offer("mallory", 1);
    sfu.offer("mallory", CHANNEL, sdp, 1);
    // A member's offer after it, so the task has certainly handled both.
    let (_alice, sdp, _) = Peer::offer("alice", 1);
    sfu.offer("alice", CHANNEL, sdp, 1);
    let frames = loop {
        tokio::time::sleep(Duration::from_millis(5)).await;
        let frames = drain(&mut outbox);
        if frames.iter().any(|(user, _)| user == "alice") {
            break frames;
        }
    };
    assert!(
        frames.iter().all(|(user, _)| user != "mallory"),
        "{frames:?}"
    );
}

/// Wait for the SFU's answer to `user` of `kind` and apply it to `peer`.
async fn accept(outbox: &mut Outbox, peer: &mut Peer, pending: SdpPendingOffer, kind: &str) {
    let answer = loop {
        tokio::time::sleep(Duration::from_millis(5)).await;
        if let Some((_, frame)) = drain(outbox)
            .into_iter()
            .find(|(user, f)| user == peer.name && f["type"] == kind)
        {
            break frame;
        }
    };
    let answer = SdpAnswer::from_sdp_string(answer["sdp"].as_str().unwrap()).unwrap();
    peer.rtc
        .sdp_api()
        .accept_answer(pending, answer)
        .expect("answer");
}

fn sharing(user: &str, screen: bool) -> Member {
    Member {
        screen,
        ..member(user, true)
    }
}

/// A screen share is published once and pulled from the server: what the
/// sharer sends reaches a viewer's own connection, and stops the moment the
/// share is no longer announced, whatever the sharer's client does.
#[tokio::test]
async fn a_screen_share_reaches_its_viewer_only_while_announced() {
    let (sfu, mut outbox) = Sfu::spawn("127.0.0.1".parse().unwrap(), 0)
        .await
        .expect("spawn");
    sfu.sync(CHANNEL, vec![sharing("alice", true), sharing("bob", false)]);

    // The client's shapes: the sharer sends its capture, the viewer
    // receives video then audio.
    let (mut alice, sdp, pending) = Peer::with_media(
        "alice",
        &[
            (MediaKind::Video, Direction::SendOnly),
            (MediaKind::Audio, Direction::SendOnly),
        ],
    );
    sfu.offer_screen("alice", "alice", CHANNEL, sdp);
    accept(&mut outbox, &mut alice, pending, "sfu-screen-answer").await;
    let (mut bob, sdp, pending) = Peer::with_media(
        "bob",
        &[
            (MediaKind::Video, Direction::RecvOnly),
            (MediaKind::Audio, Direction::RecvOnly),
        ],
    );
    sfu.offer_screen("bob", "alice", CHANNEL, sdp);
    accept(&mut outbox, &mut bob, pending, "sfu-screen-answer").await;

    run(&mut alice, &mut bob, Duration::from_secs(2)).await;
    assert!(alice.rtc.is_connected() && bob.rtc.is_connected());
    assert!(bob.heard > 0, "bob saw nothing of {} frames", alice.sent);

    // The share stops: both connections are dropped server-side.
    sfu.sync(
        CHANNEL,
        vec![sharing("alice", false), sharing("bob", false)],
    );
    run(&mut alice, &mut bob, Duration::from_millis(300)).await;
    bob.heard = 0;
    run(&mut alice, &mut bob, Duration::from_secs(1)).await;
    assert_eq!(bob.heard, 0, "the share kept flowing after it stopped");
}

#[tokio::test]
async fn nobody_can_watch_a_share_that_is_not_announced() {
    let (sfu, mut outbox) = Sfu::spawn("127.0.0.1".parse().unwrap(), 0)
        .await
        .expect("spawn");
    sfu.sync(
        CHANNEL,
        vec![sharing("alice", false), sharing("bob", false)],
    );
    let watch = [(MediaKind::Video, Direction::RecvOnly)];
    let (_bob, sdp, _) = Peer::with_media("bob", &watch);
    sfu.offer_screen("bob", "alice", CHANNEL, sdp);
    let (_alice, sdp, _) = Peer::with_media("alice", &watch);
    sfu.offer_screen("alice", "alice", CHANNEL, sdp);
    // A voice offer after them, so the task has certainly handled both.
    let (_alice, sdp, _) = Peer::offer("alice", 1);
    sfu.offer("alice", CHANNEL, sdp, 1);
    let frames = loop {
        tokio::time::sleep(Duration::from_millis(5)).await;
        let frames = drain(&mut outbox);
        if frames.iter().any(|(_, f)| f["type"] == "sfu-answer") {
            break frames;
        }
    };
    assert!(
        frames.iter().all(|(_, f)| f["type"] != "sfu-screen-answer"),
        "{frames:?}"
    );
}
