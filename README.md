# Murmer

Murmer is a minimal voice and text chat prototype built with Tauri and SvelteKit.

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

Use Docker Compose to run the server together with a Postgres database:
```bash
docker compose up --build
```
The server exposes a WebSocket endpoint at `ws://localhost:3001/ws`. The client can store multiple server URLs and connect to any of them via the "Servers" screen. Added servers are persisted locally so favorites remain after restart.
The `DATABASE_URL` used by the server is defined in `docker-compose.yml`.

## Docker
A `docker-compose.yml` runs the Rust server alongside a Postgres database. The client is run locally without Docker.

## Notes
This project is an early prototype demonstrating login, server selection, text chat and a stub for voice communication via WebRTC.
