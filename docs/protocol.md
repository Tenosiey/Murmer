# Protocol

The client/server contract: one WebSocket carrying JSON frames, three ways a
frame can leave the server, and the validation every inbound frame passes
through. To *add* a frame, follow
[`../agents/skills/websocket-frames.md`](../agents/skills/websocket-frames.md);
this document is the reference behind it.

## Frame shape

Every frame is a JSON object with a `type` field naming it. The dispatch loop
lives in `murmer_server/src/ws/handlers/mod.rs` and routes on that field to a
handler in `ws/handlers/`, split by domain — auth, messages, channels,
channel overrides, channel keys, chat settings, DMs, emojis, identity,
maintenance, moderation, pins, profile, screenshare, soundboard, stats,
uploads, voice defaults and wiki.

On the client, `stores/chat.ts` owns the `WebSocketManager`. Handlers
register with `chat.on(type, cb)` and must be cleaned up with `chat.off`.

## The three routes out

Picking the wrong one is either a privacy leak or a performance bug, so this
is the first decision when adding a frame.

| Route | Mechanism | Use for |
| --- | --- | --- |
| Server-wide broadcast | `AppState.tx` | Events every connected client needs: profile updates, role changes, emoji edits. |
| Channel-scoped broadcast | the per-channel sender | Anything that belongs to one channel: messages, reactions, pins, `screenshare-start`/`-stop`, `soundboard-play`. |
| Direct | `AppState.direct` | Anything addressed to a single user. |

`AppState.direct` is a registry of per-connection mailboxes keyed by user
name, then by a unique connection id, so one account signed in twice keeps
both sessions. Mailboxes are bounded (`DIRECT_MAILBOX_CAPACITY`) and **drop
rather than block**, mirroring what the broadcast channels already do to a
receiver that falls behind.

WebRTC signaling takes the direct route: `voice-offer`/`-answer`/`-candidate`
and the three `screenshare-*` equivalents all name their recipient in
`target`, and clients have always discarded the rest — broadcasting them cost
every connected client a socket write and a parse per frame. `target` is
therefore load-bearing: a signaling frame without one is dropped and its
session never connects.

`screenshare-start` and `-stop` stay on the channel broadcast, because they
announce to a channel rather than to one peer.

### Visibility filtering

Private channels mean a channel-scoped broadcast cannot simply be fanned out.
The `global_rx` loop in `ws/handlers/mod.rs` filters per recipient:
`channel_scope` reads the channel a frame belongs to, `channel_frame_hint`
marks the frame types that need the check at all, and `can_view_channel`
decides per connection. Channel-list senders are viewer-aware for the same
reason. See [`permissions.md`](permissions.md).

## Inbound validation

Three checks matter, in `ws/helpers.rs` and `ws/handlers/mod.rs`:

- **`claims_own_user`** — a relayed frame must prove it speaks for its
  sender before being routed. Addressing a frame to a `target` narrows *who
  sees* it; it is not an authorization check and must never be treated as one
  when adding a new relayed type.
- **`has_permission` / `top_position`** — the single enforcement point for
  the permission bitmask and the role hierarchy.
- **`can_view_channel` / `channel_permissions`** — per-channel overrides
  resolved on top of the server-wide permissions.

Reads that name their channel in the frame — search, wiki `get`/`history`/
`revision` — **repeat** the view gate rather than trusting the channel the
connection happens to have joined. That is deliberate: the joined channel is
client-supplied state, and a frame that names its own target must be checked
against that target.

Sealed payloads (`validate_sealed_payload`) are shape-checked only: base64,
a 24-byte nonce, a bounded size. The server never sees inside them. See
[`security.md`](security.md).

## HTTP endpoints

| Endpoint | Module | Auth |
| --- | --- | --- |
| `/upload` | `upload.rs` | Ed25519 proof over `upload:<timestamp>` as multipart fields ahead of the file, plus per-IP rate limit |
| `/files/<key>` | `main.rs` | none (unguessable key) |
| `/link-preview` | `link_preview.rs` | none |
| `/role` | `admin.rs` | `ADMIN_TOKEN` bearer, constant-time compared |
| `/api/…` | `bot/` | bot token — [`../murmer_server/BOT_API.md`](../murmer_server/BOT_API.md) |

`WEB_CLIENT_DIR` additionally makes the router serve the built client as its
fallback, with unmatched paths falling back to `200.html` so the prerendered
SPA routes its own deep links. Serving it there puts the client on the same
origin as `/ws` and `/upload`, which is what lets a browser use it with CORS
off.
