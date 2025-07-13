# Murmer Server

This directory contains the WebSocket server built with Axum. The code is split into a few modules:

- `main.rs` – launches the server and sets up routes.
- `db.rs` – database connection and helper functions such as fetching chat history.
- `ws.rs` – WebSocket handlers for chat and voice events.
- `upload.rs` – multipart file upload endpoint.

Run the server with `cargo run` or use the Docker Compose setup from the repository root.

## Environment
The server reads the following environment variables:

- `DATABASE_URL` – PostgreSQL connection string
- `UPLOAD_DIR` – directory where uploaded files are stored (defaults to `uploads`)
- `SERVER_PASSWORD` – optional password required to connect via WebSocket
- `ADMIN_TOKEN` – secret token required for the `/role` endpoint

These are configured automatically when running via `docker compose`.
