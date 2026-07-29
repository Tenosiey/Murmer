# TURN support — design note

**Status:** not implemented, not scheduled. Written 2026-07-29 as a reference
for deciding later.

This note exists so the problem does not have to be re-derived from scratch.
Nothing here is committed to; the "Open questions" section at the end lists what
is genuinely undecided.

---

## The problem

Both WebRTC managers hardcode a single public STUN server and nothing else:

```ts
// voice/manager.ts and screenshare/manager.ts, identically
private config: RTCConfiguration = {
  iceServers: [{ urls: 'stun:stun.l.google.com:19302' }]
};
```

STUN only tells a client its own public address. That is enough when at least
one side's NAT is well behaved: both peers learn each other's public `ip:port`
and send media directly. It fails in two cases that are common enough to matter:

- **Symmetric NAT** — the NAT assigns a *different* external port per
  destination, so the address STUN discovered is useless for reaching anyone
  else. Common on mobile carriers and on some corporate and hotel routers.
- **UDP blocked outright** — many corporate networks allow only TCP on 80/443
  outbound.

For an affected user, voice does not connect at all, and there is nothing they
or the server operator can do about it: the ICE configuration is not
configurable. The connection-repair work (`src/lib/webrtc/recovery.ts`) does not
help here — it will politely restart ICE for 30 s and then rebuild, forever,
against a path that cannot exist.

There is a second, smaller problem in the same line of code: a self-hostable,
privacy-minded project currently has every client contact Google whenever a call
starts, with no way for an operator to opt out.

## What TURN does

A TURN server is a relay. Instead of finding a path between two peers, both
peers connect *to the server* and it forwards media between them. It always
works, because it is an ordinary outbound client→server connection on a port the
operator controls — and over `turns:` on TCP 443 it looks like any other TLS
connection, which is what gets through restrictive firewalls.

The cost is that **all relayed media flows through the server**. TURN is
bandwidth, not cleverness.

### The Murmer-specific multiplier

Voice and screen share are **full mesh** — every participant holds a connection
to every other. If a call falls back to relay:

- Six people in a voice channel = 30 relayed streams.
- One 1080p screen share at 8 Mbps to four viewers = 32 Mbps out of the relay,
  for one sharer.

In practice only a minority of *pairs* need the relay, so this is usually fine.
But it is the cost that eventually argues for an SFU: with an SFU each client
sends one stream and the server fans it out, which is the same bandwidth
position as TURN but without the N² multiplier. **If an SFU is ever on the
table, evaluate it before building TURN out fully** — an SFU subsumes most of
what TURN buys here, and the two overlap heavily.

## Work breakdown

### 1. Make ICE servers configurable at all

Prerequisite for everything else, and independently worth shipping: it removes
the hardcoded Google dependency and lets an operator point at their own STUN or
at a hosted TURN without waiting for the rest of this note.

The delivery path already exists in exactly the right shape. `screenshare.rs`
builds a `screenshare-config` frame and `auth.rs` sends it to a client right
after authentication (`send_screenshare_config`). Mirror that:

- `config.rs`: read ICE configuration from the environment, alongside
  `ADMIN_TOKEN` and friends.
- A new `ws/handlers/ice.rs` building an `ice-config` frame.
- Send it from the same place in `auth.rs` — **only after authentication**.
  Anonymous connections get STUN only; relay bandwidth is not free.
- A client store holding the config; both managers read from it instead of the
  literal. They construct `RTCPeerConnection(this.config)` per peer, so the
  config only has to be set before joining — but see §3.

Frame shape (matching what `RTCConfiguration` wants, so the client can pass it
through untouched):

```json
{
  "type": "ice-config",
  "iceServers": [
    { "urls": ["stun:turn.example.com:3478"] },
    {
      "urls": [
        "turn:turn.example.com:3478?transport=udp",
        "turn:turn.example.com:3478?transport=tcp",
        "turns:turn.example.com:443?transport=tcp"
      ],
      "username": "1785400000:alice",
      "credential": "base64hmac..."
    }
  ],
  "expiresAt": 1785400000
}
```

### 2. Credentials

A TURN server cannot ship without authentication — an open relay gets found and
used to proxy other people's traffic within days.

**Static credentials** (one username/password in coturn's config, shipped to
every client) are simple but poor: a shared secret in every install, unrevokable,
and anyone who reads it gets free relay bandwidth.

**Ephemeral credentials — the recommended option.** This is what coturn's
`use-auth-secret` mode is for. coturn and the Murmer server share a secret; the
server mints per-user, time-limited credentials that coturn validates by
recomputing the HMAC, with no user database and no callback:

```
username   = "<unix-expiry-timestamp>:<account-name>"
credential = base64(HMAC_SHA1(shared_secret, username))
```

Roughly 20 lines of Rust:

```rust
let expiry = (Utc::now() + Duration::hours(12)).timestamp();
let username = format!("{expiry}:{account}");
let mut mac = Hmac::<Sha1>::new_from_slice(secret.as_bytes())?;
mac.update(username.as_bytes());
let credential = general_purpose::STANDARD.encode(mac.finalize().into_bytes());
```

**Crate note:** the server has `sha2`, but this scheme is specified on **SHA-1**
(draft-uberti-behave-turn-rest / what coturn implements). It needs `hmac` and
`sha1` added to `murmer_server/Cargo.toml`.

This fits the existing security model well: credentials are bound to the
authenticated account name, so a banned user's relay access expires on its own.
Treat the shared secret like `ADMIN_TOKEN` — never logged, never sent to a
client, environment only.

### 3. The interaction with connection repair — do not skip this

Relay candidates are gathered using whatever credentials the `RTCConfiguration`
held **at gathering time**, and an ICE restart re-gathers.

So when a credential expires mid-session, established connections keep working,
but every repair silently loses its relay candidate. `PeerRecovery` will restart,
retry, hit the deadline, rebuild, and fail — on exactly the flaky connections
that needed the relay in the first place.

Required:

- Re-issue `ice-config` before expiry (a server-side timer, or a client refresh
  scheduled at half-life from `expiresAt`).
- Call `setConfiguration()` on live peer connections when new credentials
  arrive, not just on newly created ones.
- Long TTL — 12–24 h — so the refresh path is rarely exercised in anger.

This is easy to get wrong and produces a failure visible only to the minority of
users on relay, which is the worst kind. Worth a test around the refresh
scheduling if it is built.

### 4. Deployment

A `coturn` service in `docker-compose.yml`. Sketch of the parts that are easy to
get wrong:

```conf
listening-port=3478
tls-listening-port=443
external-ip=<public ip>      # required on any NAT'd cloud VM, or relay
                             # candidates advertise an unroutable address
realm=murmer.example.com
use-auth-secret
static-auth-secret=<same secret the Murmer server holds>

# Relay range. The default is 49152-65535; publishing that through Docker's
# userland proxy is painful and slow, so either narrow it or run the container
# with network_mode: host.
min-port=49160
max-port=49200

cert=/etc/coturn/fullchain.pem
pkey=/etc/coturn/privkey.pem

# Hardening: without these the relay can be used to reach the host's own
# network. Not optional.
no-multicast-peers
denied-peer-ip=10.0.0.0-10.255.255.255
denied-peer-ip=172.16.0.0-172.31.255.255
denied-peer-ip=192.168.0.0-192.168.255.255

# So one call cannot saturate the host.
total-quota=100
bps-capacity=0
no-cli
```

`turns:` on TCP **443** with a real certificate is the configuration that
actually gets through corporate firewalls, and is most of the reason to bother.
It does mean the port cannot also serve HTTPS on the same address.

### 5. Verifying it works

The dangerous property of TURN is that it is invisible when broken — everything
looks fine until someone on a hostile network joins. Two cheap checks:

- A debug toggle setting `iceTransportPolicy: 'relay'`, which forces *all*
  traffic through TURN. If a call connects with that on, the relay genuinely
  works.
- `updateStats` in `voice/manager.ts` already walks `candidate-pair` reports;
  the selected pair carries the candidate types, so surfacing "relayed" in the
  connection panel is nearly free and shows what is happening in the field.

## Alternative: hosted TURN

Cloudflare Calls, Twilio, metered.dev and others sell TURN by the gigabyte.
Sections 1–3 are **identical** — the same config plumbing and, for most
providers, the same ephemeral-credential scheme. Section 4 becomes a credential
paste.

Worth supporting regardless of whether coturn is ever shipped: a self-hoster on a
small VPS probably should not relay video through it, and the choice belongs to
the operator.

## Effort

Rough shape, assuming ephemeral credentials and self-hosted coturn:

| Part | Size |
| --- | --- |
| §1 ICE config plumbing (server + client) | ~2 h, independently shippable |
| §2 Ephemeral credentials | ~150 lines server-side |
| §3 Refresh + `setConfiguration` | ~80 lines client-side, needs care |
| §4 coturn deployment | the part that actually takes the time |
| §5 Verification toggle + stat | ~1 h |

## Open questions

- **SFU first?** An SFU would subsume most of this. If one is ever seriously
  considered, decide between them before building §2–§4.
- **Who gets relay credentials?** Currently assumed: every authenticated user.
  Could be gated on a permission bit if bandwidth becomes a problem, but that
  makes voice work for some members and not others, which is hard to explain.
- **Ship §1 alone now?** It has standalone value (removes the Google STUN
  dependency, unblocks operators who already run a TURN server) and no
  dependency on the rest.
- **Default STUN when nothing is configured** — keep Google's, drop to no ICE
  servers at all (LAN-only), or ship a different default? Dropping it would
  break direct connections for existing installs that currently work.
