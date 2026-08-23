# Murmer Server Guide

Read the repository root [`AGENTS.md`](../AGENTS.md) first — it carries the
hard constraints, the invariants and the index of task guides. This file adds
the server's commands and module map.

**Axum 0.8**, Rust edition 2024. Authentication is Ed25519 signatures with
replay protection; persistence is an embedded SQLite database (rusqlite via
tokio-rusqlite, which pins the rusqlite version).

## Development commands

```bash
cargo check                                # compile-time validation
cargo fmt                                  # format
cargo clippy --all-targets -- -D warnings  # must pass clean
cargo test                                 # integration tests in tests/
cargo run                                  # launch locally (creates murmer.db)
```

`docker compose up --build` from the repository root launches the server with
the SQLite database on a named volume.

## Module map

| Path | Responsibility |
| --- | --- |
| `main.rs` | Axum router, middleware, shared state |
| `config.rs` | Environment variable parsing and CORS setup |
| `ws/` | WebSocket handshake and dispatch; `handlers/` split by domain |
| `ws/helpers.rs` | `has_permission`, `top_position`, `can_view_channel` — the enforcement point |
| `db/` | Connection, schema and queries, split by the same domains |
| `bot/` | REST API for bots — [`BOT_API.md`](BOT_API.md) |
| `upload.rs` | `/upload`: the signed-proof gate, validation, the categorised safe-list |
| `admin.rs` | `/role`, guarded by a bearer token |
| `roles.rs` | Role definitions and default role colors |
| `link_preview.rs` | `/link-preview`, returning OpenGraph metadata |
| `security.rs` | Rate limiting, replay protection, validation utilities |
| `permissions.rs` | The permission bitmask — the authority the client mirrors |
| `profanity.rs` | The word-list filter applied to chat messages |
| `automod.rs` | Auto-moderation rules: patterns, actions and the matcher |

Each module opens with a doc comment describing its responsibilities. Extend
it when you add behaviour.

## Working here

| Task | Guide |
| --- | --- |
| Adding or changing a frame | [`../agents/skills/websocket-frames.md`](../agents/skills/websocket-frames.md) |
| Schema or one-time backfill | [`../agents/skills/database-changes.md`](../agents/skills/database-changes.md) |
| Writing a test | [`../agents/skills/server-tests.md`](../agents/skills/server-tests.md) |
| Touching crypto or key storage | [`../agents/skills/crypto-changes.md`](../agents/skills/crypto-changes.md) |
| Changing a value the client also has | [`../agents/skills/mirrored-constants.md`](../agents/skills/mirrored-constants.md) |

Reference: [`../docs/protocol.md`](../docs/protocol.md) for frame routing,
[`../docs/permissions.md`](../docs/permissions.md) for authorization,
[`../docs/security.md`](../docs/security.md) for the crypto and rate-limiting
model, [`../docs/features.md`](../docs/features.md) for wiki, soundboard and
profiles.

## Configuration

Environment variables are documented in the Configuration section of
[`../README.md`](../README.md) — the single authority — and parsed in
`config.rs`. Do not duplicate that list here.

Two of them change behaviour in ways worth knowing while developing:

- **`ADMIN_TOKEN`** gates `/role`, the bootstrap path for the first Owner.
  Without it, channel and wiki management stay open to everyone — the
  historical fallback that keeps a small unadministered server usable. Every
  other capability is role-gated regardless.
- **`WEB_CLIENT_DIR`** makes the router serve the built client as its
  fallback, with unmatched paths falling back to `200.html` so the
  prerendered SPA routes its own deep links. That puts the client on the same
  origin as `/ws` and `/upload`, which is what lets a browser use it with
  CORS off — and it is the easiest way to drive the app end to end.

## Build profile

`[profile.dev]` uses `debug = "line-tables-only"` and
`split-debuginfo = "unpacked"`. Every file in `tests/` becomes its own test
binary, and full DWARF made each one well over a hundred megabytes.

**Do not merge the `tests/` files into a single binary to save those links.**
`security_limits.rs` mutates process-wide environment variables with
`temp_env`, and separate binaries are what currently keeps that isolated from
the tests that build a `RateLimiter`.

## Versioning

The crate version is bumped in lockstep with the client by `bun run bump` in
`murmer_client/`, which also syncs `Cargo.lock`. Never bump it by hand — see
[`../agents/skills/releasing.md`](../agents/skills/releasing.md).

## QA checklist

- `cargo fmt`, `cargo clippy --all-targets -- -D warnings` and `cargo test`.
- Exercise WebSocket authentication: invalid signatures, stale timestamps.
- Verify uploads reject invalid MIME types, oversize payloads and categories
  disabled by the current upload policy.
- Confirm channel and voice channel management respects role permissions when
  `ADMIN_TOKEN` is configured.
- For anything authorization-shaped, test the **denial** — that is the
  assertion that matters.
