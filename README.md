# Murmer

Murmer is a **self-hostable** minimal voice and text chat prototype built with Tauri and SvelteKit.

## Prerequisites
- [Rust](https://www.rust-lang.org/tools/install)
- [Node.js](https://nodejs.org) 22+
- [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)

## Running the client
```bash
cd murmer_client
npm install
npm run tauri dev
```
This launches the desktop app.
On first launch the client generates an Ed25519 keypair stored in the user's configuration directory.

## Running the server
### Prerequisite
Docker is required to host a server.

Use Docker Compose to run the server together with Postgres:
```bash
docker compose up --build
```
The server exposes a WebSocket endpoint at `ws://localhost:3001/ws`. The client can store multiple server URLs and connect to any of them via the "Servers" screen. Added servers are persisted locally so favorites remain after restart.
The `DATABASE_URL` used by the server is defined in `docker-compose.yml`.
If you set the `SERVER_PASSWORD` environment variable in `docker-compose.yml`, the server will require clients to provide that password when connecting.
The server also supports admin authentication using Ed25519 keypairs. Set `ADMIN_KEYS` to a comma separated list of hex-encoded public keys to grant admin access.

### Image Uploads

Uploaded images are stored on disk under an `uploads/` directory. The `/upload` endpoint returns a relative path like `/files/<filename>` which clients combine with the server URL to load the image.

`docker-compose.yml` mounts a volume for the uploads directory so files persist between restarts.

## Docker
A `docker-compose.yml` runs the Rust server alongside Postgres. The client is run locally without Docker.

## Notes
This project is an early prototype demonstrating login, server selection, text chat and a stub for voice communication via WebRTC.
