# Contributor Guide

This repository hosts **Murmer**, a minimal voice and text chat prototype.
It is split into a Tauri/SvelteKit client and a Rust server.

## Repository Structure
- `murmer_client/` – desktop client built with Tauri and SvelteKit.
- `murmer_server/` – Rust WebSocket server that persists chat messages to Postgres.
- `docker-compose.yml` – runs the server together with Postgres.

Each subfolder contains its own `AGENTS.md` with more details.

## Getting Started
1. Install [Rust](https://www.rust-lang.org/tools/install) and [Node.js](https://nodejs.org) 22+.
2. See `README.md` for quick commands to run the client and server.

## Security Considerations
- The server implements Ed25519 signature-based authentication with anti-replay protection
- Rate limiting is enforced for authentication attempts and message sending
- File uploads are restricted to images only with size limits and type validation
- Input validation is performed on channel names, usernames, and message content
- CORS is configured restrictively in production builds

## Code Quality Standards
- All Rust code must pass `cargo check` and be formatted with `cargo fmt`
- Frontend code should pass `npm run check` for TypeScript and Svelte validation
- Complex functions should include inline documentation explaining security implications
- All user inputs must be validated and sanitized

## Validation
- There is currently no automated test suite
- Run `npm run check` inside `murmer_client` to perform Svelte/TypeScript checks
- Run `cargo check` in `murmer_server` to ensure it builds successfully
- Format Rust code with `cargo fmt` before committing
- Test the application manually after making changes to core security or networking code

## Docker
Use `docker compose up --build` to start the server and a Postgres database defined in `docker-compose.yml`.

## Versioning
Murmer uses date-based pre-release versions such as `2025.7.13-alpha.1`.
When asked to **bump the version**, follow these steps:
1. Determine today's date in `YYYY.M.D` format.
2. If a tag `YYYY.M.D-alpha.1` already exists, increment the numeric
   suffix (`alpha.2`, `alpha.3`, ...). Otherwise start with `alpha.1`.
3. Replace the old version in:
   - `murmer_client/package.json`
   - `murmer_client/package-lock.json`
   - `murmer_client/src-tauri/Cargo.toml`
   - `murmer_client/src-tauri/Cargo.lock`
   - `murmer_client/src-tauri/tauri.conf.json`
   - `murmer_server/Cargo.toml`
   - `murmer_server/Cargo.lock`
4. Commit the changes with a message like `Bump version to <new version>`
   and create a git tag with the same version string.
