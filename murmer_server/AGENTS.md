# Murmer Server Guide

This crate implements the Murmer WebSocket/HTTP server using **Axum 0.7**.
Authentication is based on Ed25519 signatures and PostgreSQL is used for
persistence.

## Development commands
- `cargo check` – compile-time validation
- `cargo fmt` – format Rust sources
- `cargo clippy -- -D warnings` (optional but recommended)
- `cargo run` – launch the server locally (requires `DATABASE_URL`)

The repository includes a `docker-compose.yml` that provisions PostgreSQL and
launches the server in one step: `docker compose up --build`.

## Key modules
- `main.rs` – sets up the Axum router, middleware and shared state
- `ws.rs` – WebSocket handshake and message handling
- `db.rs` – database connection + schema helpers
- `upload.rs` – multipart image upload endpoint with MIME validation
- `admin.rs` – `/role` endpoint guarded by a bearer token
- `security.rs` – rate limiting, replay protection and validation utilities

Each module starts with a short doc comment describing its responsibilities.
Expand these comments when adding new behaviour.

## Configuration
Required environment variables:
- `DATABASE_URL` – PostgreSQL connection string

Optional environment variables:
- `UPLOAD_DIR` – directory for uploaded images (`uploads/` by default)
- `SERVER_PASSWORD` – shared secret required during presence/auth flows
- `ADMIN_TOKEN` – enables the `/role` endpoint and channel management controls
- `ENABLE_CORS` – set only during development to enable permissive CORS headers
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
