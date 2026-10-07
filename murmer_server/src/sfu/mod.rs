//! The SFU that carries large voice channels (`docs/voice.md`).
//!
//! One tokio task owns one UDP socket and a `str0m::Rtc` per member in SFU
//! mode, in the shape of str0m's `chat` example: poll every `Rtc`, route each
//! datagram to the `Rtc` that [`Rtc::accepts`] it, and forward media between
//! them. The WebSocket handlers talk to the task over a channel ([`Sfu`]);
//! the task answers through an outbox of `(user, frame)` pairs that
//! [`deliver`] hands to `send_to_user`.
//!
//! Three choices keep it small:
//!
//! - **ICE-lite with one host candidate** (`SFU_PUBLIC_IP` and the port).
//!   The client's connectivity checks tell the server where it is, so the
//!   client never trickles candidates and the server needs no STUN.
//! - **Slots instead of renegotiation.** A client offers once: a send-only
//!   audio and video transceiver for itself, and a receive-only pair per
//!   other possible member. Joins and leaves only move users between slots
//!   (`sfu-slots`), never SDP, so there is no glare to handle.
//! - **The server decides what flows.** Audio is forwarded only from a
//!   member with Talk who is not server-muted, video only while the sender's
//!   camera is announced. Unlike the mesh, where Talk is a hint to the
//!   sender's own client, this is enforcement. Membership comes from
//!   [`Sfu::sync`], which `broadcast_voice` calls on every join and leave: a
//!   member who left, was kicked or banned loses their `Rtc` at once,
//!   whatever their client does.
//!
//! Screen shares ride the same task on connections of their own: the sharer
//! publishes once ([`Sfu::offer_screen`] with themselves as the sharer) and
//! every viewer pulls from the server instead of from the sharer, so a share
//! costs the sharer one upload however many people watch. A share is
//! forwarded only while it is announced (`screenshare-start`), and a viewer
//! connection only lives while both ends are in the channel.
//!
//! The server terminates DTLS-SRTP here, so in SFU mode it can see media;
//! `docs/security.md` says so, and the SFU is off unless the operator sets
//! `SFU_PUBLIC_IP`.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use str0m::change::SdpOffer;
use str0m::media::{KeyframeRequestKind, MediaData, MediaKind, Mid};
use str0m::net::{Protocol, Receive};
use str0m::{Candidate, Event, Input, Output, Rtc};
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use crate::{AppState, Frame};

/// One member of a channel as the SFU sees it: who, and what the server
/// lets them send.
#[derive(Debug, Clone)]
pub struct Member {
    pub user: String,
    /// Talk in the channel and not server-muted: their audio is forwarded.
    pub talk: bool,
    /// Camera announced with `webcam-start`: their video is forwarded.
    pub camera: bool,
    /// Screen share announced with `screenshare-start`: it is forwarded to
    /// whoever watches it.
    pub screen: bool,
}

/// What one of the task's connections carries.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Conn {
    /// A member's voice and camera, with a receive slot per other member.
    Voice(String),
    /// A member publishing their screen share.
    Share(String),
    /// `viewer` watching `sharer`'s screen share: one receive slot, filled
    /// by the sharer for as long as the connection lives.
    Watch { viewer: String, sharer: String },
}

impl Conn {
    /// The account the connection belongs to, who gets its frames.
    fn user(&self) -> &str {
        match self {
            Conn::Voice(user) | Conn::Share(user) => user,
            Conn::Watch { viewer, .. } => viewer,
        }
    }

    /// Whether the channel's members, as the server last described them,
    /// still allow this connection.
    fn allowed(&self, members: &[Member]) -> bool {
        let member = |user: &str| members.iter().find(|m| m.user == user);
        match self {
            Conn::Voice(user) => member(user).is_some(),
            Conn::Share(sharer) => member(sharer).is_some_and(|m| m.screen),
            Conn::Watch { viewer, sharer } => {
                member(viewer).is_some() && member(sharer).is_some_and(|m| m.screen)
            }
        }
    }
}

enum Command {
    Offer {
        conn: Conn,
        channel: i32,
        sdp: String,
        max_slots: usize,
    },
    Sync {
        channel: i32,
        members: Vec<Member>,
    },
}

/// Handle to the SFU task. Cheap to clone; dropping every handle stops it.
#[derive(Clone)]
pub struct Sfu {
    tx: mpsc::UnboundedSender<Command>,
}

/// Frames the task wants delivered: `(account name, frame)`.
pub type Outbox = mpsc::UnboundedReceiver<(String, Frame)>;

impl Sfu {
    /// Bind the UDP socket and start the task. `public_ip` is what the ICE
    /// candidate advertises; the socket listens on every interface, because
    /// behind Docker or NAT the public address is not a local one. Port `0`
    /// picks a free port, which only tests use.
    pub async fn spawn(public_ip: IpAddr, port: u16) -> std::io::Result<(Self, Outbox)> {
        let any: IpAddr = match public_ip {
            IpAddr::V4(_) => Ipv4Addr::UNSPECIFIED.into(),
            IpAddr::V6(_) => Ipv6Addr::UNSPECIFIED.into(),
        };
        let socket = UdpSocket::bind((any, port)).await?;
        let public = SocketAddr::new(public_ip, socket.local_addr()?.port());
        info!(%public, "SFU listening");
        let (tx, rx) = mpsc::unbounded_channel();
        let (out_tx, out_rx) = mpsc::unbounded_channel();
        let task = Task {
            socket,
            public,
            clients: HashMap::new(),
            channels: HashMap::new(),
            outbox: out_tx,
            last_keyframe: HashMap::new(),
        };
        tokio::spawn(task.run(rx));
        Ok((Self { tx }, out_rx))
    }

    /// Hand a member's `sfu-offer` to the task. The caller has checked that
    /// they sit in `channel` and that it is in SFU mode; the task checks the
    /// membership again against its own copy, which [`Sfu::sync`] keeps.
    pub fn offer(&self, user: &str, channel: i32, sdp: String, max_slots: usize) {
        let _ = self.tx.send(Command::Offer {
            conn: Conn::Voice(user.to_string()),
            channel,
            sdp,
            max_slots,
        });
    }

    /// Hand a screen share offer to the task: `user` publishing their own
    /// share when `sharer` is themselves, watching `sharer`'s otherwise. The
    /// caller has checked that both sit in `channel`, that it is in SFU mode
    /// and that the share is announced; the task checks against its own copy
    /// again. It answers with `sfu-screen-answer`, and a refused offer gets
    /// no answer at all, as a refused mesh `screenshare-offer` does.
    pub fn offer_screen(&self, user: &str, sharer: &str, channel: i32, sdp: String) {
        let conn = if user == sharer {
            Conn::Share(user.to_string())
        } else {
            Conn::Watch {
                viewer: user.to_string(),
                sharer: sharer.to_string(),
            }
        };
        let _ = self.tx.send(Command::Offer {
            conn,
            channel,
            sdp,
            max_slots: 1,
        });
    }

    /// Tell the task who is in `channel` now and what each may send. Anyone
    /// with an `Rtc` in the channel who is not listed loses it, and so does
    /// every connection of a screen share that is no longer announced.
    pub fn sync(&self, channel: i32, members: Vec<Member>) {
        let _ = self.tx.send(Command::Sync { channel, members });
    }
}

/// Deliver the task's frames to their users until the task stops.
pub fn deliver(state: Arc<AppState>, mut outbox: Outbox) {
    tokio::spawn(async move {
        while let Some((user, frame)) = outbox.recv().await {
            crate::ws::helpers::send_to_user(&state, &user, frame).await;
        }
    });
}

/// A receive-only audio + video pair in one client's offer, and whose media
/// it currently carries.
#[derive(Debug, Default)]
struct Slot {
    audio: Option<Mid>,
    video: Option<Mid>,
    user: Option<String>,
}

struct Client {
    channel: i32,
    rtc: Rtc,
    /// The offer's DTLS fingerprint. A new offer with the same one is an ICE
    /// restart of the same browser connection and is applied to this `Rtc`;
    /// a different one is a rebuilt connection and replaces it.
    fingerprint: String,
    max_slots: usize,
    upstream_audio: Option<Mid>,
    upstream_video: Option<Mid>,
    slots: Vec<Slot>,
    /// The slot map may have changed since it was last worked out.
    slots_dirty: bool,
    /// The client has had its first `sfu-slots`, so later ones are only
    /// sent when something moved.
    told: bool,
}

/// Something one client produced that another needs.
enum Propagated {
    Media(Conn, Box<MediaData>),
    /// A subscriber asked for a keyframe of whoever fills its slot's video.
    Keyframe(Conn),
}

/// How often a sender is asked for a keyframe at most. Every subscriber that
/// loses a packet asks; the sender only needs to hear it once.
const KEYFRAME_INTERVAL: Duration = Duration::from_secs(1);

struct Task {
    socket: UdpSocket,
    public: SocketAddr,
    clients: HashMap<Conn, Client>,
    channels: HashMap<i32, Vec<Member>>,
    outbox: mpsc::UnboundedSender<(String, Frame)>,
    last_keyframe: HashMap<Conn, Instant>,
}

impl Task {
    async fn run(mut self, mut rx: mpsc::UnboundedReceiver<Command>) {
        let mut buf = vec![0u8; 2000];
        loop {
            let deadline = self.poll_all();
            tokio::select! {
                command = rx.recv() => match command {
                    Some(command) => self.handle_command(command),
                    None => return,
                },
                received = self.socket.recv_from(&mut buf) => match received {
                    Ok((n, source)) => self.handle_datagram(&buf[..n], source),
                    Err(e) => warn!("SFU socket read failed: {e}"),
                },
                _ = tokio::time::sleep_until(deadline.into()) => {}
            }
            let now = Instant::now();
            for client in self.clients.values_mut() {
                if let Err(e) = client.rtc.handle_input(Input::Timeout(now)) {
                    debug!("SFU client failed: {e}");
                    client.rtc.disconnect();
                }
            }
        }
    }

    fn handle_datagram(&mut self, data: &[u8], source: SocketAddr) {
        let Ok(contents) = data.try_into() else {
            return;
        };
        let input = Input::Receive(
            Instant::now(),
            Receive {
                proto: Protocol::Udp,
                source,
                destination: self.public,
                contents,
            },
        );
        // ICE credentials are random per `Rtc` and were handed out over the
        // authenticated WebSocket, so a datagram no `Rtc` accepts is noise
        // or a probe, and is dropped.
        match self.clients.values_mut().find(|c| c.rtc.accepts(&input)) {
            Some(client) => {
                if let Err(e) = client.rtc.handle_input(input) {
                    debug!("SFU client failed: {e}");
                    client.rtc.disconnect();
                }
            }
            None => debug!(%source, "SFU dropped a datagram no client accepts"),
        }
    }

    fn handle_command(&mut self, command: Command) {
        match command {
            Command::Offer {
                conn,
                channel,
                sdp,
                max_slots,
            } => self.handle_offer(conn, channel, &sdp, max_slots),
            Command::Sync { channel, members } => {
                for (conn, client) in &mut self.clients {
                    if client.channel != channel {
                        continue;
                    }
                    if !conn.allowed(&members) {
                        client.rtc.disconnect();
                    }
                    client.slots_dirty = true;
                }
                if members.is_empty() {
                    self.channels.remove(&channel);
                } else {
                    self.channels.insert(channel, members);
                }
            }
        }
    }

    fn handle_offer(&mut self, conn: Conn, channel: i32, sdp: &str, max_slots: usize) {
        let allowed = self
            .channels
            .get(&channel)
            .is_some_and(|members| conn.allowed(members));
        if !allowed {
            debug!(?conn, channel, "SFU offer the channel does not allow");
            return;
        }
        let Ok(offer) = SdpOffer::from_sdp_string(sdp) else {
            self.reject(&conn);
            return;
        };
        let fingerprint = sdp
            .lines()
            .find_map(|l| l.strip_prefix("a=fingerprint:"))
            .unwrap_or_default()
            .trim()
            .to_string();

        let restart = self
            .clients
            .get(&conn)
            .filter(|c| c.channel == channel && c.fingerprint == fingerprint && c.rtc.is_alive());
        let answer = if restart.is_some() {
            let client = self.clients.get_mut(&conn).expect("checked above");
            client.rtc.sdp_api().accept_offer(offer)
        } else {
            let mut rtc = Rtc::builder().set_ice_lite(true).build(Instant::now());
            match Candidate::host(self.public, "udp") {
                Ok(candidate) => {
                    rtc.add_local_candidate(candidate);
                }
                Err(e) => {
                    warn!("SFU host candidate {}: {e}", self.public);
                    return;
                }
            }
            let answer = rtc.sdp_api().accept_offer(offer);
            if answer.is_ok() {
                if let Some(mut old) = self.clients.remove(&conn) {
                    old.rtc.disconnect();
                }
                // A viewer's one slot belongs to the sharer for good.
                let slots = match &conn {
                    Conn::Watch { sharer, .. } => vec![Slot {
                        user: Some(sharer.clone()),
                        ..Slot::default()
                    }],
                    _ => Vec::new(),
                };
                self.clients.insert(
                    conn.clone(),
                    Client {
                        channel,
                        rtc,
                        fingerprint,
                        max_slots,
                        upstream_audio: None,
                        upstream_video: None,
                        slots,
                        slots_dirty: true,
                        told: false,
                    },
                );
            }
            answer
        };
        let sdp = match answer {
            Ok(answer) => answer.to_sdp_string(),
            Err(e) => {
                debug!(?conn, "SFU rejected an offer: {e}");
                self.reject(&conn);
                return;
            }
        };
        let frame = match &conn {
            Conn::Voice(_) => serde_json::json!({
                "type": "sfu-answer",
                "channelId": channel,
                "sdp": sdp,
            }),
            Conn::Share(sharer) | Conn::Watch { sharer, .. } => serde_json::json!({
                "type": "sfu-screen-answer",
                "channelId": channel,
                "sharer": sharer,
                "sdp": sdp,
            }),
        };
        self.send(conn.user(), frame);
    }

    /// Drive every client until it has nothing more to say, forward what
    /// they produced, and return when the earliest of them next needs time.
    fn poll_all(&mut self) -> Instant {
        let mut deadline = Instant::now() + Duration::from_millis(100);
        loop {
            let mut propagated = Vec::new();
            for (conn, client) in &mut self.clients {
                if let Some(t) = poll_client(conn, client, &self.socket, &mut propagated) {
                    deadline = deadline.min(t);
                }
            }
            if propagated.is_empty() {
                break;
            }
            for p in propagated {
                match p {
                    Propagated::Media(from, data) => self.forward(&from, &data),
                    Propagated::Keyframe(from) => self.request_keyframe(&from),
                }
            }
        }

        let before = self.clients.len();
        self.clients.retain(|_, c| c.rtc.is_alive());
        if self.clients.len() != before {
            for client in self.clients.values_mut() {
                client.slots_dirty = true;
            }
        }
        self.publish_slots();
        deadline
    }

    /// Forward one sender's media to every subscriber in their channel, if
    /// the server lets them send it: a member's voice and camera to every
    /// other member's voice connection, a screen share to its viewers.
    // ponytail: one video layer, no per-subscriber bandwidth estimation.
    // Cameras stop at 720p / 1.5 Mbps; add simulcast when a slow member
    // drags a big channel's video down.
    fn forward(&mut self, from: &Conn, data: &MediaData) {
        let Some(sender) = self.clients.get(from) else {
            return;
        };
        let channel = sender.channel;
        let kind = if Some(data.mid) == sender.upstream_audio {
            MediaKind::Audio
        } else if Some(data.mid) == sender.upstream_video {
            MediaKind::Video
        } else {
            return;
        };
        let from_user = from.user();
        let allowed = self.channels.get(&channel).and_then(|members| {
            members
                .iter()
                .find(|m| m.user == from_user)
                .map(|m| match (from, kind) {
                    (Conn::Voice(_), MediaKind::Audio) => m.talk,
                    (Conn::Voice(_), MediaKind::Video) => m.camera,
                    (Conn::Share(_), _) => m.screen,
                    (Conn::Watch { .. }, _) => false,
                })
        });
        if allowed != Some(true) {
            return;
        }
        if kind == MediaKind::Video && !data.contiguous {
            self.request_keyframe(from);
        }
        for (conn, client) in &mut self.clients {
            if client.channel != channel {
                continue;
            }
            let subscribed = match (from, conn) {
                (Conn::Voice(_), Conn::Voice(user)) => user != from_user,
                (Conn::Share(_), Conn::Watch { sharer, .. }) => sharer == from_user,
                _ => false,
            };
            if !subscribed {
                continue;
            }
            let Some(slot) = client
                .slots
                .iter()
                .find(|s| s.user.as_deref() == Some(from_user))
            else {
                continue;
            };
            let mid = match kind {
                MediaKind::Audio => slot.audio,
                MediaKind::Video => slot.video,
            };
            let Some(writer) = mid.and_then(|mid| client.rtc.writer(mid)) else {
                continue;
            };
            let Some(pt) = writer.match_params(data.params) else {
                continue;
            };
            if let Err(e) = writer.write(pt, data.network_time, data.time, data.data.clone()) {
                debug!(?conn, "SFU write failed: {e}");
                client.rtc.disconnect();
            }
        }
    }

    fn request_keyframe(&mut self, from: &Conn) {
        let now = Instant::now();
        if self
            .last_keyframe
            .get(from)
            .is_some_and(|t| now.duration_since(*t) < KEYFRAME_INTERVAL)
        {
            return;
        }
        let Some(client) = self.clients.get_mut(from) else {
            return;
        };
        let Some(mut writer) = client.upstream_video.and_then(|mid| client.rtc.writer(mid)) else {
            return;
        };
        if writer
            .request_keyframe(None, KeyframeRequestKind::Pli)
            .is_ok()
        {
            self.last_keyframe.insert(from.clone(), now);
        }
    }

    /// Reassign the slots of every client whose map may have changed, and
    /// tell each one whose map did. Anyone who still has a slot keeps it, so
    /// a join or leave moves nobody else's stream.
    fn publish_slots(&mut self) {
        let mut keyframes = Vec::new();
        for (conn, client) in &mut self.clients {
            // Only voice connections have slots that move.
            let Conn::Voice(user) = conn else {
                continue;
            };
            if !client.slots_dirty {
                continue;
            }
            client.slots_dirty = false;
            let others: Vec<&str> = self
                .channels
                .get(&client.channel)
                .map(|members| {
                    members
                        .iter()
                        .map(|m| m.user.as_str())
                        .filter(|u| u != user)
                        .collect()
                })
                .unwrap_or_default();
            let current: Vec<Option<String>> =
                client.slots.iter().map(|s| s.user.clone()).collect();
            let next = assign_slots(&current, &others);
            if client.told && next == current {
                continue;
            }
            client.told = true;
            for (slot, user) in client.slots.iter_mut().zip(next) {
                if slot.user != user {
                    // A new face in a video slot needs a keyframe to show.
                    keyframes.extend(user.clone().map(Conn::Voice));
                    slot.user = user;
                }
            }
            let slots: Vec<serde_json::Value> = client
                .slots
                .iter()
                .filter_map(|s| s.user.as_deref().map(|u| (s, u)))
                .flat_map(|(s, u)| {
                    [(s.audio, "audio"), (s.video, "video")]
                        .into_iter()
                        .filter_map(move |(mid, kind)| {
                            mid.map(|mid| {
                                serde_json::json!({ "mid": mid.to_string(), "user": u, "kind": kind })
                            })
                        })
                })
                .collect();
            let frame = serde_json::json!({
                "type": "sfu-slots",
                "channelId": client.channel,
                "slots": slots,
            });
            let _ = self.outbox.send((user.clone(), frame.to_string().into()));
        }
        for user in keyframes {
            self.request_keyframe(&user);
        }
    }

    /// Tell a voice client its offer failed. A screen share offer that fails
    /// is dropped silently, like a refused mesh `screenshare-offer`: the
    /// share's window and its repair already cover it.
    fn reject(&self, conn: &Conn) {
        if let Conn::Voice(user) = conn {
            let _ = self
                .outbox
                .send((user.clone(), crate::ws::errors::SFU_OFFER_REJECTED.into()));
        }
    }

    fn send(&self, user: &str, frame: serde_json::Value) {
        let _ = self
            .outbox
            .send((user.to_string(), frame.to_string().into()));
    }
}

/// Poll one client until it asks for time, sending what it transmits and
/// collecting what others need. Returns when it next wants a timeout.
fn poll_client(
    conn: &Conn,
    client: &mut Client,
    socket: &UdpSocket,
    propagated: &mut Vec<Propagated>,
) -> Option<Instant> {
    while client.rtc.is_alive() {
        let output = match client.rtc.poll_output() {
            Ok(output) => output,
            Err(e) => {
                debug!(?conn, "SFU client failed: {e}");
                client.rtc.disconnect();
                return None;
            }
        };
        match output {
            Output::Timeout(t) => return Some(t),
            Output::Transmit(t) => {
                // A full socket buffer drops the packet, exactly as the
                // network would; RTP and ICE both recover from loss.
                let _ = socket.try_send_to(&t.contents, t.destination);
            }
            Output::Event(Event::MediaAdded(m)) => {
                if m.direction.is_receiving() {
                    match m.kind {
                        MediaKind::Audio => client.upstream_audio = Some(m.mid),
                        MediaKind::Video => client.upstream_video = Some(m.mid),
                    }
                } else if m.direction.is_sending() {
                    add_slot_mid(&mut client.slots, client.max_slots, m.kind, m.mid);
                    client.slots_dirty = true;
                }
            }
            Output::Event(Event::MediaData(data)) => {
                propagated.push(Propagated::Media(conn.clone(), Box::new(data)));
            }
            Output::Event(Event::KeyframeRequest(req)) => {
                if let Some(source) = client
                    .slots
                    .iter()
                    .find(|s| s.video == Some(req.mid))
                    .and_then(|s| s.user.clone())
                {
                    // A viewer's slot carries the sharer's screen, a voice
                    // slot that member's camera.
                    let source = match conn {
                        Conn::Watch { .. } => Conn::Share(source),
                        _ => Conn::Voice(source),
                    };
                    propagated.push(Propagated::Keyframe(source));
                }
            }
            Output::Event(_) => {}
        }
    }
    None
}

/// Pair receive-only transceivers into slots in offer order: the n-th audio
/// with the n-th video. Past `max_slots`, a transceiver gets no slot and
/// stays idle; the effective channel cap means it would never be filled.
fn add_slot_mid(slots: &mut Vec<Slot>, max_slots: usize, kind: MediaKind, mid: Mid) {
    let free = slots.iter().position(|s| match kind {
        MediaKind::Audio => s.audio.is_none(),
        MediaKind::Video => s.video.is_none(),
    });
    let index = match free {
        Some(index) => index,
        None if slots.len() < max_slots => {
            slots.push(Slot::default());
            slots.len() - 1
        }
        None => return,
    };
    let slot = &mut slots[index];
    match kind {
        MediaKind::Audio => slot.audio = Some(mid),
        MediaKind::Video => slot.video = Some(mid),
    }
}

/// Assign `others` to slots, keeping everyone who still has one where they
/// are, so a join or leave moves nobody else's stream. Returns one entry per
/// slot in `current`; who does not fit gets none.
fn assign_slots(current: &[Option<String>], others: &[&str]) -> Vec<Option<String>> {
    let mut next: Vec<Option<String>> = current
        .iter()
        .map(|u| u.clone().filter(|u| others.contains(&u.as_str())))
        .collect();
    let placed: Vec<String> = next.iter().flatten().cloned().collect();
    let mut waiting = others.iter().filter(|u| !placed.iter().any(|p| p == *u));
    for slot in next.iter_mut().filter(|s| s.is_none()) {
        match waiting.next() {
            Some(user) => *slot = Some(user.to_string()),
            None => break,
        }
    }
    next
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slots(v: &[Option<&str>]) -> Vec<Option<String>> {
        v.iter().map(|u| u.map(str::to_string)).collect()
    }

    #[test]
    fn assignment_keeps_members_in_place() {
        // Bob leaves, dave joins: carol stays in slot 2, dave takes slot 1.
        let current = slots(&[Some("bob"), Some("carol"), None]);
        assert_eq!(
            assign_slots(&current, &["carol", "dave"]),
            slots(&[Some("dave"), Some("carol"), None])
        );
    }

    #[test]
    fn assignment_fills_in_order_and_drops_overflow() {
        assert_eq!(
            assign_slots(&slots(&[None, None]), &["a", "b", "c"]),
            slots(&[Some("a"), Some("b")])
        );
        assert_eq!(assign_slots(&slots(&[Some("a")]), &[]), slots(&[None]));
    }
}
