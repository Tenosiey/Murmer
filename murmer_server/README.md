# Murmer Server

Axum-based WebSocket/HTTP server with embedded SQLite persistence.

| You want | Go to |
| --- | --- |
| Setup, configuration, deployment | the root [README.md](../README.md) |
| The bot REST API | [BOT_API.md](BOT_API.md) |
| Module map and contributor conventions | [AGENTS.md](AGENTS.md) |
| How the system fits together | [`../docs/architecture.md`](../docs/architecture.md) |

## Development

```bash
cargo run          # creates murmer.db in the working directory by default
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo audit        # requires cargo-audit (cargo install cargo-audit)
```

## CLI subcommands

`main.rs` accepts two subcommands alongside the normal server run:

- `set-role <key> <role>` — assign a role to a public key, creating the role
  definition if it does not exist. This is the bootstrap path for the first
  Owner when you would rather not use the `/role` HTTP endpoint.
- `unbind-name <name>` — release a user name from the key it is bound to.
  Names bind permanently on first use, so this is the recovery path for a
  user who lost their keypair.
