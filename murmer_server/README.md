# Murmer Server

Axum-based WebSocket/HTTP server with embedded SQLite persistence. For setup,
configuration and deployment see the root [README.md](../README.md); for the
bot REST API see [BOT_API.md](BOT_API.md).

## Module layout

```
src/
  main.rs          entry point: router, middleware, CLI `set-role` subcommand
  lib.rs           shared state (AppState, RateLimiter) and module re-exports
  config.rs        environment variable parsing and CORS setup
  ws/              WebSocket endpoint
    handlers/      auth, messages, channels, DMs, moderation, pins
    helpers.rs     broadcast, permission and ephemeral-message utilities
    validation.rs / errors.rs / constants.rs
  db/              SQLite schema + queries, split by domain (channels,
                   messages, reactions, roles, moderation, pins, DMs)
  bot/             REST API for bots (routes, models, queries)
  upload.rs        multipart file/image upload with type and size validation
  link_preview.rs  server-side OpenGraph fetching with SSRF protection
  admin.rs         /role endpoint guarded by ADMIN_TOKEN
  security.rs      rate limiting, nonce replay protection
  roles.rs         built-in role definitions
```

## Development

```bash
cargo run          # creates murmer.db in the working directory by default
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo audit
```

Integration tests live in `tests/` and exercise rate limiting, moderation and
persistence logic.
