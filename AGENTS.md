# Contributor Guide

This monorepo hosts **Murmer**, a desktop chat prototype split into a
Tauri/SvelteKit client (`murmer_client/`) and an Axum-based Rust server
(`murmer_server/`). Each directory contains its own `AGENTS.md` with tooling
specifics.

## Workflow overview
- Install the latest [Rust toolchain](https://www.rust-lang.org/tools/install)
  and [Node.js 22+](https://nodejs.org).
- See `README.md` for detailed setup, build and configuration instructions.
- When developing locally run the client with `npm run tauri dev` and the server
  with `cargo run` or `docker compose up --build`.

## Quality checks
- Run `cargo check` inside `murmer_server/` and format with `cargo fmt`.
- Run `npm run check` inside `murmer_client/` for Svelte + TypeScript diagnostics.
- Document complex security-sensitive logic with inline comments.
- Sanitize or validate all user-supplied data before acting on it.

## Client code organisation
- `src/routes/` – SvelteKit pages (login, server selection, chat)
- `src/lib/components/` – reusable UI primitives and overlays
- `src/lib/stores/` – Svelte stores holding client state
- `src/lib/chat/` – constants and helpers shared by the chat page
- `src/lib/voice/` – WebRTC helpers and push-to-talk tooling
- `src/lib/screenshare/` – WebRTC screen sharing manager
- `src-tauri/` – Rust-side glue for native integrations

## Security expectations
- Authentication relies on Ed25519 signatures with replay protection.
- Rate limiting exists for both authentication and chat traffic.
- File uploads are validated by size and an extension safe-list; images are additionally checked by magic bytes. Active content (HTML, SVG, scripts) is never accepted.
- Channel management honours role assignments when `ADMIN_TOKEN` is configured.
- Lifetime user stats are double opt-in: recording requires the server-wide
  toggle (Owner/Admin) AND the user's own opt-in, enforced in
  `murmer_server/src/db/stats.rs`. Only aggregate counters are stored — never
  message contents or recipients.
- Production deployments should keep CORS disabled unless explicitly required.

## Versioning
Releases use the date-based scheme `YYYY.MDD.N` (year, month+day, counter for
multiple releases on the same day), e.g. `2026.710.0` for the first release on
2026-07-10. The scheme stays semver-ordered, which the Tauri updater requires.
**Client and server share one version** and are bumped in lockstep — the
server crate (`murmer_server/Cargo.toml`) must not be bumped by hand or
skipped. When asked to bump versions:

1. Run `npm run bump` inside `murmer_client/`. The script
   (`scripts/bump-version.mjs`) computes the next version and writes it into
   all six versioned files: the client's `package.json`,
   `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` and
   `src-tauri/Cargo.lock`, plus the server's `Cargo.toml` and `Cargo.lock`
   (the lock files matter: `--locked` builds fail when they disagree).
2. Commit with `Release v<version>` and create a matching `v<version>` git
   tag. Pushing the tag triggers the GitHub Actions release workflow, which
   builds the installers and updater manifest.

See `README.md` for the full release process.

## Validation checklist
- Ensure CI-equivalent commands above pass before opening a pull request.
- Perform manual smoke tests after changing networking, authentication or file
  handling logic.
- Keep documentation (`README.md`, `AGENTS.md`) in sync with code behaviour.
