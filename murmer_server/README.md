# Murmer Server

This directory contains the WebSocket server built with Axum. The code is split into a few modules:

- `main.rs` – launches the server and sets up routes.
- `db.rs` – database connection and helper functions such as fetching chat history.
- `ws.rs` – WebSocket handlers for chat and voice events.
- `upload.rs` – multipart file upload endpoint.
- `auth.rs` – REST API for user registration and authentication.

Run the server with `cargo run` or use the Docker Compose setup from the repository root.

### REST API
The server exposes a small REST API:

- `POST /register` – create a new user
- `POST /login` – authenticate and receive a JWT
- `POST /users/:id/roles` – assign a role (requires Owner/Admin)
- `GET /users` – list all users with their roles
- `GET /me` – return the authenticated user

## Environment
The server reads the following environment variables:

- `DATABASE_URL` – PostgreSQL connection string
- `UPLOAD_DIR` – directory where uploaded files are stored (defaults to `uploads`)
- `SERVER_PASSWORD` – optional password required to connect via WebSocket
- `JWT_SECRET` – secret key used to sign authentication tokens

These are configured automatically when running via `docker compose`.
