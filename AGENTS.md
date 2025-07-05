# Contributor Guide

This repository hosts **Murmer**, a minimal voice and text chat prototype.
It is split into a Tauri/SvelteKit client and a Rust server.

## Repository Structure
- `murmer_client/` – desktop client built with Tauri and SvelteKit.
- `murmer_server/` – Rust WebSocket server that persists chat messages to Postgres.
- `docker-compose.yml` – runs the server together with Postgres.

Each subfolder contains its own `AGENTS.md` with more details.

## Getting Started
1. Install [Rust](https://www.rust-lang.org/tools/install) and [Node.js](https://nodejs.org) v18 or newer.
2. See `README.md` for quick commands to run the client and server.

## Validation
- There is currently no automated test suite.
- Run `npm run check` inside `murmer_client` to perform Svelte/TypeScript checks.
- Format Rust code with `cargo fmt` before committing.

## Docker
Use `docker compose up --build` to start the server and a Postgres database defined in `docker-compose.yml`.
