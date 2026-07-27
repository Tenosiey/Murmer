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

## Key modules
- `main.rs` – sets up the Axum router, middleware and shared state
- `config.rs` – environment variable parsing and CORS setup
- `ws/` – WebSocket handshake and message handling (`handlers/` for auth,
  messages, channels, DMs, emojis, identity, moderation, pins, profile,
  screenshare, soundboard, stats, uploads and wiki; the dispatch loop lives in
  `handlers/mod.rs`)
- `db/` – database connection, schema and queries, split by the same domains
- `bot/` – REST API for bots (see `BOT_API.md`)
- `upload.rs` – multipart file upload endpoint with extension/MIME validation
  and the categorised safe-list behind the configurable upload policy
- `admin.rs` – `/role` endpoint guarded by a bearer token
- `roles.rs` – role definitions and default role color helpers
- `link_preview.rs` – `/link-preview` endpoint returning OpenGraph metadata
- `security.rs` – rate limiting, replay protection and validation utilities

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
- `MAX_MESSAGES_PER_MINUTE`, `MAX_AUTH_ATTEMPTS_PER_MINUTE`,
  `NONCE_EXPIRY_SECONDS` – override rate limiting defaults

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

Private channels add per-channel allow/deny overrides (`channel_overrides`
table + in-memory cache in `AppState.channel_overrides`), resolved by
`channel_permissions`/`can_view_channel` in `ws/helpers.rs`. Overrides are
clamped to `CHANNEL_OVERRIDABLE` (View + Write/Talk). Enforcement: viewer-aware
channel-list senders, a per-recipient filter on channel-scoped broadcasts in the
`global_rx` loop, and channel-aware gates on join/history/send/react/pin and
`voice-join`. Voice talk is a client-enforced hint (`voice-permissions`) since
audio is peer-to-peer. Managers edit overrides through the
`set-channel-override`/`remove-channel-override`/`get-channel-overrides` frames
(`ws/handlers/channel_overrides.rs`), and creating a channel with `private: true`
seeds an `@everyone` View-deny plus a creator allow.

The soundboard (`ws/handlers/soundboard.rs`, `db/soundboard.rs`) stores a
server-wide sound library. `add/rename/remove-sound` require `MANAGE_SOUNDS`;
`play-sound` requires `USE_SOUNDBOARD`, that the connection is actually in the
named voice channel, `can_view_channel` for it, and that the user is not
server-muted. Audio never touches the server beyond `/upload`: playback is a
`soundboard-play` broadcast that every client renders locally, so the frame is
filtered per recipient by `channel_scope`/`channel_frame_hint` like the other
voice-scoped frames. `AppState.soundboard_cooldowns` enforces the per-user
playback cooldown and is pruned on disconnect. Adding a sound re-validates the
referenced upload (extension, `MAX_SOUND_FILE_BYTES`, magic bytes) because the
upload endpoint is open and these files auto-play on every listener.
`db::migrate_soundboard_permissions` grants the two flags to pre-soundboard
databases once, marker-guarded, so an existing server matches a fresh one.

## Security notes
- Direct messages are end-to-end encrypted by the clients; the server only
  shape-checks `nonce`/`ciphertext` (base64, 24-byte nonce, bounded size —
  see `validate_dm_payload`) and stores the frame verbatim. Do not add any
  code path that accepts or produces plaintext DM content. Clients fetch a
  peer's key via the `get-user-key` frame, answered from the `user_keys`
  binding; users without a binding (e.g. bots) cannot receive DMs.
- Client IP addresses are used for authentication rate limiting – ensure the
  service runs behind a proxy that forwards the real IP if applicable.
- Nonces combine the public key and timestamp; replayed signatures are rejected.
- Uploaded files are streamed to disk after validating type, size and filename.
- Admin tokens are compared using constant-time equality.
- Avoid adding new WebSocket message types without updating validation helpers.

## QA checklist
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings` and `cargo test`.
- Exercise WebSocket authentication (invalid signatures, stale timestamps).
- Verify file uploads reject invalid MIME types, oversize payloads and
  categories disabled by the current upload policy.
- Confirm channel/voice channel management respects role permissions when
  `ADMIN_TOKEN` is configured.
