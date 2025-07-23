# murmer_server Guide

This folder hosts the Rust WebSocket server built with Axum.

## Running
1. Ensure Postgres is available and set `DATABASE_URL` accordingly.
2. Start the server with `cargo run`.

You can also run the server together with Postgres via Docker Compose from the repository root:

```bash
docker compose up --build
```

The server listens on `ws://localhost:3001/ws`.

## Validation
There are no automated tests. Run `cargo check` to verify the code builds and
format it with `cargo fmt` before committing.
