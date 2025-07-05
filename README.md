# Murmer

Murmer is a minimal voice and text chat prototype built with Tauri and SvelteKit.

## Prerequisites
- [Rust](https://www.rust-lang.org/tools/install)
- [Node.js](https://nodejs.org) 18+

## Running the client
```bash
cd murmer_client
npm install
npm run tauri dev
```
This launches the desktop app.

## Running the server
```bash
cd murmer_server
cargo run
```
The server exposes a WebSocket endpoint at `ws://localhost:3001/ws` used by the client for chat.

## Docker
A `Dockerfile` is provided to bootstrap a development container with all dependencies.

## Notes
This project is an early prototype demonstrating login, server selection, text chat and a stub for voice communication via WebRTC.
