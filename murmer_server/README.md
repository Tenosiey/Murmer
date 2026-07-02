# Murmer Server

Axum-based WebSocket/HTTP server with PostgreSQL persistence. For setup,
configuration and deployment see the root [README.md](../README.md); for the
bot REST API see [BOT_API.md](BOT_API.md).

## Module layout

```
src/
  main.rs        entry point: router, middleware, CLI `set-role` subcommand
  lib.rs         shared state (AppState, RateLimiter) and module re-exports
  config.rs      environment variable parsing and CORS setup
  ws/            WebSocket endpoint
    handlers/    auth, chat messages, channel management
    helpers.rs   broadcast and presence utilities
    validation.rs / errors.rs / constants.rs
  db/            PostgreSQL schema + queries (channels, messages, reactions, roles)
  bot/           REST API for bots (routes, models, queries)
  upload.rs      multipart image upload with MIME/size validation
  admin.rs       /role endpoint guarded by ADMIN_TOKEN
  security.rs    rate limiting, nonce replay protection
  roles.rs       built-in role definitions
```

## Development

```bash
cargo run          # requires DATABASE_URL (see ../.env.example)
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo audit
```

Integration tests live in `tests/` and exercise rate limiting and role logic.
