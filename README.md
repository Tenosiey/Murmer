# Murmer

Murmer is a self-hostable voice and text chat prototype. The project is split
into a Rust WebSocket server and a cross-platform desktop client powered by
Tauri and SvelteKit. Both halves are designed with security-first defaults so a
small team can deploy a private chat space quickly.

## Features
- Persistent text chat stored in PostgreSQL
- Experimental WebRTC voice rooms with presence tracking
- Ed25519 signature authentication with nonce-based replay protection
- Rate limiting on authentication and chat events
- Markdown rendering with DOMPurify sanitisation and syntax highlighting
- Configurable user roles with optional colour accents
- Secure image uploads (content-type checks, size limits and path sanitisation)
- Desktop client with auto-reconnect and connection quality indicators
- Slash commands, ephemeral messaging, and history search controls documented in
  [`docs/chat-commands.md`](docs/chat-commands.md)

## Repository layout
- `murmer_client/` – Tauri + SvelteKit desktop client (TypeScript)
- `murmer_server/` – Axum-based WebSocket server (Rust)
- `docker-compose.yml` – boots the server together with PostgreSQL
- `CONTRIBUTING.md`, `AGENTS.md` – workflow notes for contributors

## Requirements
- [Rust](https://www.rust-lang.org/tools/install) 1.89 (managed automatically via `rust-toolchain.toml`)
- [Node.js](https://nodejs.org) 22 or newer
- Docker and Docker Compose (for container-based workflows)

Copy `.env.example` to `.env` and adjust values before running the stack locally. The server expects a PostgreSQL instance; the
provided Compose stack provisions one automatically.

## Quick start (Docker)
1. Install Docker and Docker Compose.
2. Copy `.env.example` to `.env` and update values as needed.
3. From the repository root run:
   ```bash
   docker compose up --build
   ```
4. The server listens on `http://localhost:3001` (WebSocket at `/ws`).
5. Launch the client locally:
   ```bash
   cd murmer_client
   npm install
   npm run tauri dev
   ```
   The desktop shell opens with a development build of the Svelte UI.

Added servers are stored locally by the client so your favourite instances
remain available after restarts.

## Local development
### Client
```bash
cd murmer_client
npm install          # install dependencies / refresh package-lock
npm run dev          # hot module reloading for the Svelte UI
npm run tauri dev    # launch the native shell
npm run check        # TypeScript + Svelte diagnostics
```

### Server
```bash
cd murmer_server
cargo check          # compile-time checks
cargo fmt            # format Rust code
```

## Quality checks
Run the following commands before opening a pull request to ensure code style, tests and security audits stay green:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo audit

cd murmer_client
npm run check
npm audit
```

Environment variables recognised by the server:
- `DATABASE_URL` (required) – PostgreSQL connection string
- `UPLOAD_DIR` – directory for stored uploads (defaults to `uploads/`)
- `SERVER_PASSWORD` – optional shared secret required during presence/auth
- `ADMIN_TOKEN` – enables the administrative `/role` endpoint
- `BIND_ADDRESS` – override the socket address (defaults to `0.0.0.0:3001`)
- `CORS_ALLOW_ORIGINS` – comma-separated list of origins allowed to call HTTP endpoints (omit in production)
- `MAX_MESSAGES_PER_MINUTE`, `MAX_AUTH_ATTEMPTS_PER_MINUTE`, `NONCE_EXPIRY_SECONDS` – tweak rate limiter thresholds

When `ADMIN_TOKEN` is configured only users with the roles `Admin`, `Mod` or
`Owner` may create or delete text/voice channels. Without an admin token the
behaviour remains fully open for backwards compatibility.

## Windows build instructions
1. Install the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for
   Windows (Visual Studio Build Tools, WebView2, etc.).
2. Install [Rust](https://www.rust-lang.org/tools/install) and
   [Node.js 22+](https://nodejs.org) and ensure both are available in `PATH`.
3. Build the client from `murmer_client/`:
   ```bash
   npm install
   npm run build
   npm run tauri build
   ```
   Bundles are produced in `murmer_client/src-tauri/target/release/bundle`.
4. (Optional) Produce an optimised server binary:
   ```bash
   cd ../murmer_server
   cargo build --release
   ```
   The executable is written to `murmer_server/target/release/murmer_server`.

## Security highlights
- Authentication uses Ed25519 signatures; timestamps are validated and bound to
  per-user nonces.
- IP-based rate limiting protects authentication and chat message throughput.
- Filenames are sanitised and image contents inspected before saving.
- Admin operations use constant-time comparisons to mitigate timing attacks.
- Channel management honours server-side role assignments when admin mode is on.

## Contributing
1. Format Rust code with `cargo fmt` and ensure `cargo check` passes.
2. Run `npm run check` from `murmer_client`.
3. Update documentation and changelog entries relevant to your change.
4. Keep pull requests focused and describe security implications where relevant.

Additional contributor notes live in:
- [`AGENTS.md`](AGENTS.md) – repository overview and shared conventions
- [`murmer_client/AGENTS.md`](murmer_client/AGENTS.md) – client-specific tips
- [`murmer_server/AGENTS.md`](murmer_server/AGENTS.md) – server-specific tips

Murmer is an experimental project; please perform manual QA before using it for
sensitive workloads.
