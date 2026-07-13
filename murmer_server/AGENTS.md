# Murmer Server Guide

This crate implements the Murmer WebSocket/HTTP server using **Axum 0.8**.
Authentication is based on Ed25519 signatures and an embedded SQLite database
is used for persistence.

## Development commands
- `cargo check` – compile-time validation
- `cargo fmt` – format Rust sources
- `cargo clippy -- -D warnings` (optional but recommended)
- `cargo run` – launch the server locally (creates `murmer.db` by default)

The repository includes a `docker-compose.yml` that launches the server (the
SQLite database lives on a named volume): `docker compose up --build`.

## Key modules
- `main.rs` – sets up the Axum router, middleware and shared state
- `config.rs` – environment variable parsing and CORS setup
- `ws/` – WebSocket handshake and message handling (`handlers/` for auth,
  messages and channels)
- `db/` – database connection, schema and queries
- `bot/` – REST API for bots (see `BOT_API.md`)
- `upload.rs` – multipart file upload endpoint with extension/MIME validation
- `admin.rs` – `/role` endpoint guarded by a bearer token
- `roles.rs` – role definitions and default role color helpers
- `link_preview.rs` – `/link-preview` endpoint returning OpenGraph metadata
- `security.rs` – rate limiting, replay protection and validation utilities

Each module starts with a short doc comment describing its responsibilities.
Expand these comments when adding new behaviour.

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

When `ADMIN_TOKEN` is set, only users with the roles `Admin`, `Mod` or `Owner`
may create or delete text/voice channels. The server logs and returns an error
for unauthorised attempts.

## Security notes
- Client IP addresses are used for authentication rate limiting – ensure the
  service runs behind a proxy that forwards the real IP if applicable.
- Nonces combine the public key and timestamp; replayed signatures are rejected.
- Uploaded files are streamed to disk after validating type, size and filename.
- Admin tokens are compared using constant-time equality.
- Avoid adding new WebSocket message types without updating validation helpers.

## QA checklist
- Run `cargo check` and `cargo fmt`.
- Exercise WebSocket authentication (invalid signatures, stale timestamps).
- Verify file uploads reject invalid MIME types and oversize payloads.
- Confirm channel/voice channel management respects role permissions when
  `ADMIN_TOKEN` is configured.
