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
        let socket = UdpSocket::bind("127.0.0.1:0").expect("bind");
        socket.set_nonblocking(true).expect("nonblocking");
        let mut rtc = Rtc::new(Instant::now());
        rtc.add_local_candidate(Candidate::host(socket.local_addr().unwrap(), "udp").unwrap());
        let mut change = rtc.sdp_api();
        let send_audio = change.add_media(MediaKind::Audio, Direction::SendOnly, None, None, None);
        change.add_media(MediaKind::Video, Direction::SendOnly, None, None, None);
        for _ in 0..slots {
            change.add_media(MediaKind::Audio, Direction::RecvOnly, None, None, None);
            change.add_media(MediaKind::Video, Direction::RecvOnly, None, None, None);
        }
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
