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

## Security expectations
- Authentication relies on Ed25519 signatures with replay protection.
- Rate limiting exists for both authentication and chat traffic.
- File uploads are restricted to images and validated by content-type and size.
- Channel management honours role assignments when `ADMIN_TOKEN` is configured.
- Production deployments should keep CORS disabled unless explicitly required.

## Versioning
Murmer uses date-based pre-release versions (`YYYY.M.D-alpha.N`). When asked to
bump versions:

1. Derive the new version string.
2. Update the version fields in:
   - `murmer_client/package.json`
   - `murmer_client/package-lock.json`
   - `murmer_client/src-tauri/Cargo.toml`
   - `murmer_client/src-tauri/Cargo.lock`
   - `murmer_client/src-tauri/tauri.conf.json`
   - `murmer_server/Cargo.toml`
   - `murmer_server/Cargo.lock`
3. Commit with `Bump version to <new version>` and create a matching git tag.

## Validation checklist
- Ensure CI-equivalent commands above pass before opening a pull request.
- Perform manual smoke tests after changing networking, authentication or file
  handling logic.
- Keep documentation (`README.md`, `AGENTS.md`, `TODO.md`) in sync with code
  behaviour.
