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

Use Docker Compose to run the server together with Postgres and MinIO:
```bash
docker compose up --build
```
The server exposes a WebSocket endpoint at `ws://localhost:3001/ws`. The client can store multiple server URLs and connect to any of them via the "Servers" screen. Added servers are persisted locally so favorites remain after restart.
The `DATABASE_URL` used by the server is defined in `docker-compose.yml`.

### Image Uploads

The server stores uploaded images in a MinIO bucket. Configure the following environment variables when running the server:

```
MINIO_ENDPOINT=<http://localhost:9000>
MINIO_BUCKET=<bucket-name>
MINIO_PUBLIC_URL=<public-base-url>
AWS_ACCESS_KEY_ID=<access-key>
AWS_SECRET_ACCESS_KEY=<secret-key>
```

`MINIO_PUBLIC_URL` should be the base URL clients use to access objects. The bucket is created separately.

`docker-compose.yml` starts a MinIO instance with the default `minioadmin` credentials and uses a bucket named `murmer`.

## Docker
A `docker-compose.yml` runs the Rust server alongside Postgres and MinIO. The client is run locally without Docker.

## Notes
This project is an early prototype demonstrating login, server selection, text chat and a stub for voice communication via WebRTC.
