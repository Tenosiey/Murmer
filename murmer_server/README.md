# Murmer Server

This directory contains the WebSocket server built with Axum. The code is split into a few modules:

- `main.rs` – launches the server and sets up routes.
- `db.rs` – database connection and helper functions such as fetching chat history.
- `ws.rs` – WebSocket handlers for chat and voice events.
- `upload.rs` – multipart file upload endpoint.

Run the server with `cargo run` or use the Docker Compose setup from the repository root.
