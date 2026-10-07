# Hybrid voice: mesh for small channels, SFU above a threshold

**Status:** accepted, phase 1 (docs) done, phases 2–7 not built. Written
2026-10-07 against `dev` at `19b02f1`; it replaced `plans/turn-support.md`.
When it ships it moves into `docs/voice.md`.

## Decision in one paragraph

Keep today's full mesh for voice, camera and screen share while a channel
is small. When a channel reaches **7 people** the server flips that channel
to an **SFU embedded in `murmer_server`** (the `str0m` crate, one UDP port),
and flips it back to mesh when it drops to **4**. The same switch is the
relay fallback: if two members of a small channel cannot reach each other
directly, the channel goes through the SFU instead of through TURN. **TURN
is dropped**: its plan, its TODO entry and the doc text promising it are
removed. STUN stays, because the mesh still needs it.

---

## 1. Should TURN stay?

No. Every problem TURN was meant to solve is solved by the SFU, or is no
worse than it is today:

| Problem (from `plans/turn-support.md`) | With the hybrid SFU |
| --- | --- |
| Symmetric NAT on both ends of a mesh pair | The pair reports failure and the channel moves to the SFU (§4.4). A client behind symmetric NAT can always reach a server on a public IP, because the server is not behind a NAT. |
| Large channels relaying N² streams through a relay | Doesn't arise: large channels are on the SFU, one upstream per client |
| UDP blocked entirely (corporate Wi-Fi) | Not fixed by either today. TURN's answer was `turns:` on TCP 443. The SFU's answer, if it is ever needed, is ICE-TCP on the SFU port (§9), with no second server to run |
| Hardcoded Google STUN | Already fixed (`STUN_SERVERS`, `ice-config`) |

TURN would only beat the SFU in one way: a TURN relay never decrypts
media, while the SFU does (§6). That does not justify running a second
server (coturn), a second credential scheme (HMAC-SHA1 ephemeral
credentials, two new crates) and the credential-refresh interaction with
`webrtc/recovery.ts`, all of which the TURN plan flags as easy to get
wrong and invisible when broken. A mesh pair that needs a relay is rare,
and routing that one channel through the SFU costs the same bandwidth a
TURN relay would.

**What "removing TURN" touches.** No TURN code exists, so this is docs
only, done in phase 1:

- delete `plans/turn-support.md`
- `TODO.md`: drop the "TURN support" bug and the "An SFU for large voice
  channels" future idea (this plan replaces both)
- `docs/voice.md`: rewrite "Relay support" and the last line of "Room size"
- keep the `stun:`/`stuns:`-only check in `config.rs::parse_stun_servers`
  and `stores/iceConfig.ts`. It is now permanent rather than a stopgap,
  so reword its comments to say so

## 2. Why an embedded SFU, and why str0m

| Option | Verdict |
| --- | --- |
| **`str0m` inside `murmer_server`** | **Chosen.** A sans-IO Rust WebRTC library (0.24.1, released Oct 2026, actively maintained), whose `chat` example is a minimal SFU. It runs as one tokio task on one UDP socket, so it adds no second process, no second auth system and no port range. Signaling stays on the existing WebSocket, so it inherits authentication, `can_view_channel` and the rate limiter for free |
| `webrtc-rs` | A port of pion with its own async stack. It has more moving parts than str0m and is no closer to an SFU |
| LiveKit / mediasoup / Janus | A second service with its own SDK, tokens and client library, which means a second voice client beside the mesh one. That is too much for a project maintained by one or two people |

## 3. How a channel picks its mode

The server decides; clients follow. That keeps the decision where every
other voice decision already lives (`handle_voice_join`, `ws/handlers/mod.rs`).

- `VoiceChannelInfo` (in `AppState.voice_channels`) gains
  `mode: VoiceMode { Mesh, Sfu }` and `sticky: bool`.
- A pure function `next_mode(current, headcount, sticky, threshold)` in
  `security.rs` (next to `voice_channel_has_room`) applies:
  - `Mesh → Sfu` at `headcount >= threshold` (default 7) or when `sticky`
  - `Sfu → Mesh` at `headcount <= threshold − 3`, unless `sticky`
  - the hysteresis band stops a channel of 6–7 from flapping each time
    somebody's Wi-Fi blinks
  - an empty channel resets to `Mesh` and clears `sticky`
- It runs on every join, leave and disconnect, the same places that call
  `broadcast_voice` today. On a change the server broadcasts
  `voice-mode { channelId, mode }` to the channel's members. A joiner gets
  the current mode in its `voice-permissions` reply, so it never builds a
  mesh it is about to tear down.
- If the SFU is not configured (§7), `next_mode` always returns `Mesh` and
  the server behaves exactly as it does today.

The threshold is one env var, `SFU_THRESHOLD`. The band of 3 is a constant,
because nobody will tune two numbers.

## 4. Server work

### 4.1 The SFU task (`src/sfu/`, new)

- `sfu/mod.rs` owns one `UdpSocket` and a `str0m::Rtc` per joined client,
  driven by a single tokio task in the shape of str0m's `chat` example:
  poll every `Rtc`, route incoming datagrams with `Rtc::accepts`, forward
  media between them.
- **ICE-lite, server side.** The server has a known public address
  (`SFU_PUBLIC_IP`) and offers a single host candidate. Nothing on the
  server needs STUN.
- The WebSocket handlers talk to the task over an `mpsc` channel: `Join`,
  `Leave`, `Offer`, `SetPermissions`. The task reports answers and failures
  back through the existing `send_to_user`.
- Forwarding: audio from every sender goes to every other member. Camera
  video is forwarded only while the sender is in `AppState.active_webcams`,
  which the server already maintains from `webcam-start`/`-stop`.
- **No simulcast and no per-subscriber bandwidth estimation in v1.**
  Cameras already stop at 720p / 1.5 Mbps (`voice/camera.ts`), so one layer
  is fine at the channel caps Murmer allows.
  `// ponytail:` mark it in code: add simulcast when a slow member drags a
  big channel's video down.

### 4.2 Signaling without renegotiation: slots

The mesh already avoids glare by never renegotiating: the camera
transceiver is created up front and toggled with `replaceTrack`. The SFU
connection uses the same trick so that it is **offered once and never
renegotiated**:

- When it enters SFU mode, the client sends one `sfu-offer` with:
  - one `sendonly` audio and one `sendonly` video transceiver for itself
  - `slots` pairs of `recvonly` audio + video transceivers, where
    `slots = effective channel cap − 1`, taken from the
    `voice-mode` frame
- The server answers (`sfu-answer`) and from then on only tells the
  client who is in which slot: `sfu-slots { channelId, slots: [{ mid,
  user, kind }] }`, sent on every join and leave.
- Joining, leaving and camera toggles become slot reassignments plus
  forwarding changes, never SDP. That removes the glare question entirely,
  and an idle `recvonly` transceiver costs nothing on the wire.
- This needs a finite cap. If the SFU is configured and
  `MAX_VOICE_CHANNEL_USERS=0`, the server refuses to start with a clear
  error (`config.rs`). Channel limits are already ≤ 99.

`sfu-offer`, `sfu-answer` and `sfu-candidate` are new frame types. Follow
`agents/skills/websocket-frames.md` for them and for `voice-mode`,
`sfu-slots` and `voice-p2p-failed`.

### 4.3 Enforcement gets better, not worse

- Only a member of the channel (`info.users`, after `can_view_channel` at
  join) gets an `Rtc`. A `sfu-offer` from anyone else is rejected, the
  same rule as `signals_within_own_voice_channel`.
- **Talk permission becomes real in SFU mode.** The server simply does not
  forward audio from a member without `SEND_MESSAGES` in the channel, and
  a server mute stops forwarding too. The `voice-permissions` hint stays,
  because mesh mode still needs it, but the AGENTS.md invariant and
  `docs/security.md` should say it is enforced in SFU mode and a hint
  only in mesh.
- Kick, ban, move and `breakout-move` already make the client leave. The
  server additionally drops the `Rtc` itself, so a client that ignores
  the frame stops receiving immediately.
- ICE ufrag/pwd are per-`Rtc` and random. A datagram that does not match
  any `Rtc` is dropped.

### 4.4 The relay fallback (what replaces TURN)

- A new client frame, `voice-p2p-failed { channelId, target }`, is sent
  when a mesh pair **never reached `connected`** within 15 s of its first
  offer, or when `PeerRecovery` exhausts its rebuilds for that pair.
- The server checks that both users are in the sender's channel, then sets
  `sticky = true` and re-runs `next_mode`. The channel stays on the SFU
  until it empties: the network condition that broke the pair will not fix
  itself mid-call.
- Abuse ceiling: any member can push their own channel onto the SFU. The
  cost is server bandwidth for a channel they are in, which is no different
  from inviting six friends. It needs no permission bit.

### 4.5 Config (`config.rs`, documented in `README.md` only)

| Variable | Meaning |
| --- | --- |
| `SFU_PUBLIC_IP` | Address advertised in the SFU's ICE candidate. **Unset disables the SFU**, which keeps today's behavior and the default for anyone who upgrades without reading the notes |
| `SFU_UDP_PORT` | Single UDP port for all SFU media (default `3479`) |
| `SFU_THRESHOLD` | Headcount at which a channel moves to the SFU (default `7`) |

`docker-compose.yml` gains `- "3479:3479/udp"` and the two variables,
commented out. A single port means no Docker port-range pain, the biggest
practical problem in the coturn sketch.

## 5. Client work

### 5.1 Voice and camera (`voice/manager.ts`)

The microphone chain (capture → denoise → gain → gate → track) and the
camera track do not depend on topology, and they stay as they are. Only
what the outgoing track is attached to changes.

- New `voice/sfu.ts` with a `SfuConnection` class. It holds one
  `RTCPeerConnection` and the slot map, sends `sfu-offer`, applies
  `sfu-answer` / `sfu-slots`, and produces the same `RemotePeer[]` shape
  the mesh does (`id` = account name, `stream`, `video`, `stats`). That
  way `remoteAudio.ts`, the speaking rings, the member list and the camera
  tiles don't change.
- A pure function in `src/lib/voice/mode.ts` maps
  `(current transport, voice-mode frame) → actions`. This is the part a
  Vitest test can reach (`agents/skills/client-tests.md`).
- `VoiceManager` holds either the mesh `peers` map or a `SfuConnection`,
  never both for long:
  - **Mesh → SFU, make before break.** Open the SFU connection. When it
    reports `connected`, close every mesh peer through the existing
    `cleanupPeer` path, which also clears recovery entries.
  - **SFU → Mesh.** Build the mesh with the existing offer rules (who
    offers is decided by comparing account names). Close the SFU
    connection once every mesh peer is `connected`, or after 10 s,
    whichever comes first.
- `setCameraTrack` calls `replaceTrack` on the SFU video sender in SFU
  mode, exactly as it does on each mesh sender today. No offer.
- Connection repair: `PeerRecovery` is keyed by id, so the SFU connection
  is simply peer `"sfu"`. It uses the same grace period, ICE restart and
  rebuild. A rebuild is a fresh `sfu-offer`; the server replaces the old
  `Rtc` when it sees a new DTLS fingerprint (`webrtc/fingerprint.ts`
  already tells these apart).
- Stats: `updateStats` takes rtt from the single connection's
  candidate-pair. Loss and jitter come per user from each slot's
  `inbound-rtp`, so the connection bars still mean something per member.
  The existing `MIN_LOSS_SAMPLE_PACKETS` window applies unchanged.
- `withOpusFeatures` (DTX + FEC) is applied to the SFU offer too. The SFU
  forwards Opus as is, so DTX's uplink savings carry over.

### 5.2 Screen share (`screenshare/manager.ts`)

This is a separate phase, and the biggest bandwidth win: a 1080p share at
8 Mbps to ten viewers is 80 Mbps of upload from the sharer in mesh, and
8 Mbps through the SFU.

- In SFU mode the sharer publishes once: one `sendonly` connection to the
  server (`target: "sfu"`, `role: "sharer"`).
- A viewer's offer goes to the server instead of to the sharer. That is
  the flow the manager already has (offers only ever travel viewer →
  sharer), with the target swapped. The server forwards the sharer's
  stream to every subscribed viewer connection.
- The floating windows, reconciliation, `MAX_REBUILDS` and the bitrate cap
  are unchanged.

### 5.3 UI

- A small "Via server" badge on the voice panel while the channel is in
  SFU mode, with a tooltip saying media passes through the server. The
  user chose mesh for privacy, so they should be able to see when they
  don't have it. Follow `agents/skills/svelte-ui.md` and the existing
  tokens.
- The connection panel shows one rtt for the server instead of one per
  peer.

## 6. Privacy and security changes

Write these down in `docs/security.md`:

- **In SFU mode the server decrypts media.** DTLS-SRTP terminates at the
  SFU, so an operator *can* record a large call. In mesh mode nothing
  changes: the server never sees media. This is why the badge exists, and
  why the SFU is off unless the operator sets `SFU_PUBLIC_IP`.
- End-to-end encryption through the SFU (encoded transforms / SFrame) is
  possible later. It is out of scope here, because WebView2, WebKitGTK and
  WKWebView support for `RTCRtpScriptTransform` would need checking first.
- The SFU is a new unauthenticated UDP listener. Datagrams are only
  accepted for a live `Rtc` whose ICE credentials were handed out over the
  authenticated WebSocket, and str0m already hardens SRTP/RTCP parsing
  against malformed input (its changelog #1029). Log rejected traffic with
  `warn!` and a rate limit, never `println!`.

## 7. Operators who don't enable it

With `SFU_PUBLIC_IP` unset, nothing changes:

- every channel stays mesh
- `MAX_VOICE_CHANNEL_USERS` keeps defaulting to 10
- two members behind symmetric NAT still can't connect, exactly as today

`voice-p2p-failed` is accepted and ignored. README should say plainly that
the SFU is how large channels and hostile NATs are supported.

## 8. Phases

Each phase is a PR into `dev` that leaves the app working.

| Phase | Contents | Size |
| --- | --- | --- |
| 1 | Docs: remove the TURN plan, TODO entries and doc promises; add this plan to `plans/` | small |
| 2 | Server: config, `VoiceMode`, `next_mode`, `voice-mode` frame (always `Mesh` until phase 3), unit tests for `next_mode` | ~1 day |
| 3 | Server: `src/sfu/` with str0m, slots, `sfu-*` frames, forwarding, talk enforcement, compose port | the bulk: several days |
| 4 | Client: `SfuConnection`, `mode.ts`, make-before-break switching, stats, badge | ~2–3 days |
| 5 | Relay fallback: `voice-p2p-failed` on both sides | ~half a day |
| 6 | Screen share through the SFU | ~1–2 days |
| 7 | Docs: `voice.md`, `screen-sharing.md`, `protocol.md`, `security.md`, `architecture.md`, README env rows, AGENTS.md talk-permission invariant | small but required |

## 9. Testing

- **Server unit:** `next_mode` truth table (threshold, hysteresis band,
  sticky, empty reset, SFU disabled).
- **Server integration** (`murmer_server/tests/`, see
  `agents/skills/server-tests.md`): seven WebSocket clients join, all get
  `voice-mode: sfu`; one leaves at a time and the switch back happens at
  4. A non-member's `sfu-offer` is rejected. `voice-p2p-failed` from a
  non-member is ignored.
- **SFU forwarding:** str0m can play the client too, so a test can run
  two in-process str0m peers against the SFU task over loopback UDP and
  assert that RTP from A reaches B, and doesn't reach B once A loses
  Talk. No browser is needed.
- **Client unit:** `mode.ts` transitions; slot-map parsing that rejects
  malformed `sfu-slots` frames.
- **Visual** (`agents/skills/visual-verification.md`): seven Chromium
  contexts with `--use-fake-device-for-media-stream` join one channel. The
  check is that audio meters move, camera tiles show, the badge appears
  at 7 and disappears at 4, and nobody's audio drops during the switch.
- **Relay fallback by hand:** `iceTransportPolicy: 'relay'` is no longer
  meaningful, so instead set `STUN_SERVERS=` (empty) on a server and join
  from two different networks. The pair fails, and the channel should
  move to the SFU within ~15 s.

## 10. Open questions

1. **Threshold 7, back at 4.** This is within the 6–8 you named, and it
   puts the switch where a mesh client starts uploading its microphone six
   times. Say if you'd rather have 6 or 8.
2. **Should a channel limit above 10 become the norm** when the SFU is on?
   The plan keeps `MAX_VOICE_CHANNEL_USERS` at 10 and leaves raising it to
   the operator.
3. **ICE-TCP** for UDP-blocked networks is not in v1. str0m handles TCP
   candidates (#797), so it is a follow-up on the same port if anyone
   reports it.
4. **Audio fan-out at large sizes.** v1 forwards every speaker's audio.
   Top-N forwarding by audio level only matters past ~25 people, which the
   caps don't allow today.
