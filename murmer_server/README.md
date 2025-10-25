# Murmer Server

This directory contains the WebSocket server built with Axum. The code is split into a few modules:

- `main.rs` – launches the server and sets up routes.
- `db.rs` – database connection and helper functions such as fetching chat history.
- `ws.rs` – WebSocket handlers for chat and voice events.
- `upload.rs` – multipart file upload endpoint.

Run the server using the Docker Compose setup from the repository root or locally with `cargo run`.

## Environment
The server reads the following environment variables:

- `DATABASE_URL` – PostgreSQL connection string
- `UPLOAD_DIR` – directory where uploaded files are stored (defaults to `uploads`)
- `SERVER_PASSWORD` – optional password required to connect via WebSocket
- `ADMIN_TOKEN` – secret token required for the `/role` endpoint
- `BIND_ADDRESS` – socket address to bind to (`0.0.0.0:3001` by default)
- `CORS_ALLOW_ORIGINS` – comma-separated origins that may call HTTP endpoints (omit in production)
- `MAX_MESSAGES_PER_MINUTE`, `MAX_AUTH_ATTEMPTS_PER_MINUTE`, `NONCE_EXPIRY_SECONDS` – rate limiting overrides

These are configured automatically when running via `docker compose`.

## Development workflow

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo audit
```
