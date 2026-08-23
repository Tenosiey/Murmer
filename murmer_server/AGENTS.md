# Murmer Server Guide

This crate implements the Murmer WebSocket/HTTP server using **Axum 0.8**
(Rust edition 2024). Authentication is based on Ed25519 signatures and an
embedded SQLite database is used for persistence (rusqlite via
tokio-rusqlite, which pins the rusqlite version).

## Development commands
- `cargo check` – compile-time validation
- `cargo fmt` – format Rust sources
- `cargo clippy --all-targets -- -D warnings` – must pass clean
- `cargo test` – integration tests in `tests/`
- `cargo run` – launch the server locally (creates `murmer.db` by default)

The repository includes a `docker-compose.yml` that launches the server (the
SQLite database lives on a named volume): `docker compose up --build`.

`[profile.dev]` builds with `debug = "line-tables-only"` and
`split-debuginfo = "unpacked"`. Every file in `tests/` becomes its own test
binary, and full DWARF made each one well over a hundred megabytes — line
tables keep the file:line in panics and backtraces at a fraction of the link
time and disk. Do not merge the `tests/` files into a single binary to save
those links: `security_limits.rs` mutates process-wide environment variables
with `temp_env`, and separate binaries are what currently keep that isolated
from the tests that build a `RateLimiter`.

## Key modules
- `main.rs` – sets up the Axum router, middleware and shared state
- `config.rs` – environment variable parsing and CORS setup
- `ws/` – WebSocket handshake and message handling (`handlers/` for auth,
  messages, channels, channel overrides, channel keys, chat settings, DMs,
  emojis, identity, maintenance, moderation, pins, profile, screenshare,
  soundboard, stats, uploads, voice defaults and wiki; the dispatch loop lives
  in `handlers/mod.rs`).
  Frames leave the server by one of three routes. Server-wide events go on
  `AppState.tx`, channel-scoped ones on the per-channel sender, and anything
  addressed to a single user goes through `AppState.direct` — a registry of
  per-connection mailboxes keyed by user name, then by a unique connection id
  so one account signed in twice keeps both. WebRTC signaling
  (`voice-offer`/`-answer`/`-candidate` and the three `screenshare-*`
  equivalents) takes the direct route: every one of those frames names its
  recipient in `target` and clients have always discarded the rest, so
  broadcasting them cost every connected client a socket write and a parse per
  frame. `screenshare-start`/`-stop` stay on the broadcast — they announce to
  a channel rather than to one peer. Mailboxes are bounded
  (`DIRECT_MAILBOX_CAPACITY`) and drop rather than block, mirroring what the
  broadcast channels already do to a receiver that falls behind.
- `db/` – database connection, schema and queries, split by the same domains.
  All queries run on one connection thread, so per-query cost is shared by
  everything: statements go through `prepare_cached` (the cache capacity is
  raised in `db::init`, since the default of 16 is below the number of
  distinct statements here), and the FTS backfill only runs when `db::init`
  had to create `messages_fts` — its anti-join scans every message row, and it
  used to run on every startup
- `bot/` – REST API for bots (see `BOT_API.md`)
- `upload.rs` – multipart file upload endpoint: the signed-proof gate on
  `/upload`, extension/MIME validation and the categorised safe-list behind the
  configurable upload policy
- `admin.rs` – `/role` endpoint guarded by a bearer token
- `roles.rs` – role definitions and default role color helpers
- `link_preview.rs` – `/link-preview` endpoint returning OpenGraph metadata
- `security.rs` – rate limiting, replay protection and validation utilities
- `profanity.rs` – the word-list filter applied to chat messages

Each module starts with a short doc comment describing its responsibilities.
Expand these comments when adding new behaviour.

## Versioning
The crate version in `Cargo.toml` is bumped in lockstep with the client by
`npm run bump` in `murmer_client/` (which also syncs `Cargo.lock`). Never bump
it by hand — see the Versioning section in the repository root `AGENTS.md`.

## Configuration
Optional environment variables:
- `DATABASE_PATH` – path to the SQLite database file (`murmer.db` by default)
- `BIND_ADDRESS` – socket address to bind to (`0.0.0.0:3001` by default)
- `UPLOAD_DIR` – directory for uploaded files (`uploads/` by default)
- `SERVER_PASSWORD` – shared secret required during presence/auth flows
- `ADMIN_TOKEN` – enables the `/role` endpoint and channel management controls
- `CORS_ALLOW_ORIGINS` – comma-separated origins allowed to call HTTP
  endpoints; set only during development
- `WEB_CLIENT_DIR` – directory with the built web client
  (`murmer_client/build`). When set, `main.rs` serves it as the router's
  fallback instead of answering `/` with a bare 200, with unmatched paths
  falling back to `200.html` so the prerendered SPA routes its own deep links.
  Serving it here puts the client on the same origin as `/ws` and `/upload`,
  which is what lets a browser use it with CORS off
- `MAX_MESSAGES_PER_MINUTE`, `MAX_AUTH_ATTEMPTS_PER_MINUTE`,
  `MAX_UPLOADS_PER_MINUTE`, `NONCE_EXPIRY_SECONDS` – override rate limiting
  defaults

Authorization uses a permission bitmask (`src/permissions.rs`), not fixed role
names. Roles are custom `role_definitions` rows with a permission mask and a
hierarchy `position`; users hold any number of them (`user_roles`) and their
effective permissions are the union plus the built-in `@everyone` baseline.
`has_permission`/`top_position` in `ws/helpers.rs` are the single enforcement
point. `ADMIN_TOKEN` still gates the `/role` bootstrap endpoint, and without it
channel/wiki management stays open to everyone (the historical fallback);
every other capability is role-gated regardless. The `/role` endpoint and the
`set-role` CLI add a named role to a key, creating the definition if missing —
use them to bootstrap the first Owner. Role CRUD and assignment otherwise flow
through the `create-role`/`update-role`/`delete-role`/`reorder-roles`/
`set-user-roles` WebSocket frames (`ws/handlers/roles.rs`), all requiring the
`MANAGE_ROLES` permission and bounded by the hierarchy to prevent escalation.
`update-role` also carries the optional role `icon` (an `/files/<key>` upload
URL — an image or a custom emoji's file), re-validated against the upload
directory and `MAX_ROLE_ICON_BYTES` like the server icon; replaced icon files
are intentionally left on disk since emojis and other roles may share them.
Legacy single-role databases are migrated once by `db::migrate_roles`.

Encrypted channels (`ws/handlers/channel_keys.rs`, `db/channel_keys.rs`) add a
`channels.e2ee` flag and a per-member wrapped-key store. The server is a dumb
store plus a member directory: `get-channel-keys` returns the wraps addressed to
the requester along with the roster (from `channel_members`, i.e. everyone
`can_view_channel` admits) and the current epoch's holders; `put-channel-keys`
files new wraps. Three rules in `db::insert_channel_keys` carry the security of
the whole feature and are covered by `tests/channel_keys_test.rs`: a write may
only open the *next* epoch or extend an existing one, extending requires the
author to hold a wrap at that epoch, and an existing wrap is **never**
overwritten (otherwise a member could swap another member's wrap for one sealed
to a key they control). The handler additionally refuses any wrap addressed to
somebody off the roster. Everything else — when to rotate, who to wrap for — is
client policy; see `murmer_client/src/lib/stores/channelKeys.ts`.

Private channels add per-channel allow/deny overrides (`channel_overrides`
table + in-memory cache in `AppState.channel_overrides`), resolved by
`channel_permissions`/`can_view_channel` in `ws/helpers.rs`. Overrides are
clamped to `CHANNEL_OVERRIDABLE` (View + Write/Talk). Enforcement: viewer-aware
channel-list senders, a per-recipient filter on channel-scoped broadcasts in the
`global_rx` loop, and channel-aware gates on join/history/search/send/react/pin,
the wiki reads and writes, and `voice-join`. Search names its channel in the
frame, so it repeats the history gate rather than trusting the joined channel —
that gate covers the wiki page hits it answers with as well. Voice talk is a
client-enforced hint (`voice-permissions`) since audio is peer-to-peer. Managers
edit overrides through the
`set-channel-override`/`remove-channel-override`/`get-channel-overrides` frames
(`ws/handlers/channel_overrides.rs`), and creating a channel with `private: true`
seeds an `@everyone` View-deny plus a creator allow.

The channel wiki (`ws/handlers/wiki.rs`, `db/wiki.rs`) keeps every saved
version of a page in `wiki_revisions`, pruned to `MAX_WIKI_REVISIONS_KEPT` on
each save. `wiki-history` lists them (newest first, bodies omitted — 50
revisions of a 100 kB page would otherwise be a 5 MB sidebar), `wiki-revision`
fetches one body, and `wiki-restore` re-applies an old version through the same
compare-and-swap as `wiki-update`, which is why it answers with the same
`wiki-saved`/`wiki-conflict` frames. A restore is stored as a *new* revision
rather than rewinding the counter: history is only ever appended to, so a
restore is itself undoable and a concurrent editor still loses the CAS instead
of being silently overwritten. Reads name their channel in the frame, so
`wiki-get`/`wiki-history`/`wiki-revision` repeat the `can_view_channel` gate
rather than trusting the joined channel, and `require_wiki_writer` checks it
alongside `MANAGE_WIKI` — a private channel's pages are as much its content as
its messages are. `wiki-resolve` stays ungated: it answers "does this page
exist" for channels addressed by name.

The soundboard (`ws/handlers/soundboard.rs`, `db/soundboard.rs`) stores a
server-wide sound library. `add/rename/remove-sound` require `MANAGE_SOUNDS`;
`play-sound` requires `USE_SOUNDBOARD`, that the connection is actually in the
named voice channel, `can_view_channel` for it, and that the user is not
server-muted. Audio never touches the server beyond `/upload`: playback is a
`soundboard-play` broadcast that every client renders locally, so the frame is
filtered per recipient by `channel_scope`/`channel_frame_hint` like the other
voice-scoped frames. `AppState.soundboard_cooldowns` enforces the per-user
playback cooldown and is pruned on disconnect. Adding a sound re-validates the
referenced upload (extension, `MAX_SOUND_FILE_BYTES`, magic bytes) because any
authenticated member may upload and these files auto-play on every listener.
`db::migrate_soundboard_permissions` grants the two flags to pre-soundboard
databases once, marker-guarded, so an existing server matches a fresh one.

User profiles live on the `user_keys` binding row: `avatar`, `display_name`,
`nickname`, `about` and the `created_at` that doubles as "member since"
(`ws/handlers/profile.rs`, `db/users.rs`). `set-avatar` and `set-profile` only
ever touch the requester's own row; absent fields are left alone and `null`
clears one. Every client gets an `avatar-snapshot` plus a `profile-snapshot`
(all bindings, offline users included) after auth and `avatar-update`/
`profile-update` broadcasts on change. The display name is **cosmetic** — no
server code resolves one back to a user, which is why it needs no uniqueness
check; the account name stays the identity for auth, roles, moderation, DMs
and message authorship.

`set-nickname` is the one field here somebody else may write, and the only
reason that is safe is that a nickname is exactly as cosmetic as a display
name. Setting your own needs nothing beyond being authenticated; setting
anybody else's needs `MANAGE_NICKNAMES` **and** strictly outranking them, the
same hierarchy check the moderation handlers run — without it a fresh moderator
could relabel the owner. It writes through `db::set_user_nickname`, its own
statement rather than a fourth parameter on `set_user_profile`, so an
authorized nickname change can never carry a display name or "about" edit with
it. `db::migrate_nickname_permissions` grants the flag to roles that already
hold `KICK_MEMBERS` once, marker-guarded, so an existing server's moderators
match a freshly seeded one — `@everyone` never gains it.

The Server Dashboard's remaining settings all live in the generic
`server_settings` key-value table, next to the stats toggle, the upload policy
and the screen share cap: the chat policy (`db/chat_settings.rs`: slow mode,
message length cap, profanity filter and its word list) and the voice defaults
(`db/voice_defaults.rs`). The chat policy is the one that is *cached* in
`AppState.chat_settings`, because every chat message consults all three values;
`ws/handlers/chat_settings.rs` refreshes that cache in the same step that
writes the row. Slow mode timestamps live in `AppState.slow_mode_sends` and are
dropped on disconnect — it paces a live conversation rather than punishing
someone across sessions — and members with `MANAGE_MESSAGES` are exempt.
The profanity filter masks words *before* a message is stored or broadcast,
in `handle_chat` **and** `handle_edit_message`, so posting and immediately
editing cannot walk past it; matching is per whole word so a filter for "ass"
leaves "class" alone. A configured message cap may only narrow
`MAX_MESSAGE_LENGTH`, never widen it: `db::clamp_chat_settings` enforces that
on write and again on read, so a hand-edited row cannot raise it.

The Danger Zone actions (`db/maintenance.rs`, `ws/handlers/maintenance.rs`)
require `ADMINISTRATOR` *and* the confirmation phrase (`PURGE`/`RESET`) echoed
in the frame, and run their deletions in one transaction. A reset deliberately
keeps `@everyone` and the Owner role (deleting them would leave the server with
nobody who can administer it) along with identities, bans, mutes, emojis,
sounds and stats; it also has to clear the in-memory mirrors of what it
deleted (`voice_channels`, `channel_overrides`, `role_defs`, `user_roles`) and
broadcast `channels-refresh`, or a deleted channel stays joinable and a deleted
role keeps granting permissions until the next restart.

## Security notes
- Direct messages are end-to-end encrypted by the clients; the server only
  shape-checks `nonce`/`ciphertext` (base64, 24-byte nonce, bounded size —
  see `validate_sealed_payload`) and stores the frame verbatim. Do not add any
  code path that accepts or produces plaintext DM content. Clients fetch a
  peer's key via the `get-user-key` frame, answered from the `user_keys`
  binding; users without a binding (e.g. bots) cannot receive DMs.
- End-to-end encrypted channels use the same shape check on their `enc`
  envelope, and the same rule applies: no code path may accept or produce
  plaintext for a channel whose `e2ee` flag is set. `handle_chat` and
  `handle_edit_message` branch on `channel_is_e2ee` and reject a `text`,
  `image` or `attachment` field there rather than stripping it silently; the
  bot REST API refuses to post into such a channel at all, because a bot has
  no identity key to encrypt with. Reply quotes are the one visible casualty:
  the server rebuilds them from stored plaintext, so in an encrypted channel it
  sends `replyTo` with an empty snippet and the client supplies the quote from
  inside the ciphertext.
- Client IP addresses are used for authentication and upload rate limiting –
  ensure the service runs behind a proxy that forwards the real IP if
  applicable.
  The limits (`MAX_MESSAGES_PER_MINUTE`, `MAX_AUTH_ATTEMPTS_PER_MINUTE`,
  `MAX_UPLOADS_PER_MINUTE`, `NONCE_EXPIRY_SECONDS`) are read once when the
  `RateLimiter` is built rather than on every check, so they take effect at
  startup. Each limiter map is
  swept end to end on a timer; the window for the key being checked is always
  pruned on access, so the limit itself stays exact.
  All of that time comes from `RateLimiter::clock` rather than from
  `Instant::now()`: the window and the sweep interval are both a minute long,
  so `RateLimiter::with_clock(Clock::manual())` plus `Clock::advance` is what
  lets `tests/security_limits.rs` reach behaviour a real sleep never could. A
  sweep that silently stopped running would look exactly like a working rate
  limiter, which is why it is worth a test at all.
- Nonces combine the public key and timestamp; replayed signatures are rejected.
  A nonce is treated as unused once it is older than `NONCE_EXPIRY_SECONDS`,
  whether or not the periodic sweep has removed it yet.
- Stat recording checks `AppState::stats_enabled` (an in-memory mirror of the
  server-wide toggle) before touching the database, so a server with tracking
  off — the default — runs no query per message. That is a shortcut, not a
  gate: the authoritative double opt-in check stays inside
  `db::record_user_stats`, in the same call that performs the increments.
- `/upload` is authenticated, not open: `upload::authorize` requires a fresh,
  single-use Ed25519 proof (signed over `upload:<timestamp>`, so a presence
  signature cannot be spent on it) from a key that `db::user_for_key` resolves
  to an account, and rejects banned users. Credentials are multipart fields
  ahead of the file — verified before any file bytes are buffered, and kept out
  of headers so the request needs no CORS preflight. The per-IP
  `check_upload_rate_limit` runs before the body is touched at all.
  `security::verify_key_signature` is shared with the presence path, so both
  transports verify a proof the same way.
- Uploaded files are streamed to disk after validating type, size and filename.
- Admin tokens are compared using constant-time equality.
- Avoid adding new WebSocket message types without updating validation helpers.
- Relayed frames still prove they speak for their sender (`claims_own_user`)
  before being routed. Addressing signaling to its `target` narrows who sees a
  frame; it is not itself an authorization check, and must not be treated as
  one when adding new relayed types.

## QA checklist
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings` and `cargo test`.
- Exercise WebSocket authentication (invalid signatures, stale timestamps).
- Verify file uploads reject invalid MIME types, oversize payloads and
  categories disabled by the current upload policy.
- Confirm channel/voice channel management respects role permissions when
  `ADMIN_TOKEN` is configured.
