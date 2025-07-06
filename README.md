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

## Running the server
### Prerequisite
Docker is required to host a server.

Use Docker Compose to run the server together with Postgres:
```bash
docker compose up --build
```
The server exposes a WebSocket endpoint at `ws://localhost:3001/ws`. The client can store multiple server URLs and connect to any of them via the "Servers" screen. Added servers are persisted locally so favorites remain after restart.
The `DATABASE_URL` used by the server is defined in `docker-compose.yml`.

### Image Uploads

Uploaded images are stored on disk under an `uploads/` directory. Set `PUBLIC_URL` to the base URL clients use to access files (defaults to `http://localhost:3001`). Files can then be fetched from `<PUBLIC_URL>/files/<filename>`.

`docker-compose.yml` mounts a volume for the uploads directory so files persist between restarts.

## Docker
A `docker-compose.yml` runs the Rust server alongside Postgres. The client is run locally without Docker.

## Notes
This project is an early prototype demonstrating login, server selection, text chat and a stub for voice communication via WebRTC.
