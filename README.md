# Murmer

Murmer is a self-hostable voice and text chat prototype. The project is split
into a Rust WebSocket server and a cross-platform desktop client powered by
Tauri and SvelteKit. Both halves are designed with security-first defaults so a
small team can deploy a private chat space quickly.

## Features

- Persistent text chat stored in PostgreSQL
- WebRTC voice rooms with presence tracking
- Ed25519 signature authentication with nonce-based replay protection
- Rate limiting on authentication and chat events
- Markdown rendering with DOMPurify sanitisation and syntax highlighting
- Configurable user roles with optional colour accents
- Secure image uploads (content-type checks, size limits and path sanitisation)
- Desktop client with auto-reconnect and connection quality indicators
- Slash commands (`/help`, `/me`, `/shrug`, `/topic`, `/status`, `/focus`,
  `/ephemeral`, `/search`)
- Ephemeral messaging, message search and pinned messages
- Screen sharing in voice channels

## Repository layout

```
murmer_client/   Tauri + SvelteKit desktop client (TypeScript)
murmer_server/   Axum-based WebSocket server (Rust)
docker-compose.yml   boots the server together with PostgreSQL
```

Key documentation for contributors:

- `AGENTS.md` – repository overview and shared conventions
- `murmer_client/AGENTS.md` – client-specific tips
- `murmer_server/AGENTS.md` – server-specific tips
- `CONTRIBUTING.md` – code style and PR guidelines

## Requirements

- [Rust](https://www.rust-lang.org/tools/install) (managed automatically via
  `rust-toolchain.toml`)
- [Node.js](https://nodejs.org) 22 or newer
- Docker and Docker Compose (for container-based workflows)

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

The desktop shell opens with a development build of the Svelte UI. Added servers
are stored locally by the client so your favourite instances remain available
after restarts.

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
cargo clippy -- -D warnings
```

## Quality checks

Run the following commands before opening a pull request:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo audit

cd murmer_client
npm run check
npm audit
```

## Configuration

Environment variables recognised by the server:

| Variable | Required | Description |
|----------|----------|-------------|
| `DATABASE_URL` | Yes | PostgreSQL connection string |
| `UPLOAD_DIR` | No | Directory for stored uploads (defaults to `uploads/`) |
| `SERVER_PASSWORD` | No | Shared secret required during presence/auth |
| `ADMIN_TOKEN` | No | Enables the administrative `/role` endpoint |
| `BIND_ADDRESS` | No | Override the socket address (defaults to `0.0.0.0:3001`) |
| `CORS_ALLOW_ORIGINS` | No | Comma-separated allowed origins (omit in production) |
| `MAX_MESSAGES_PER_MINUTE` | No | Per-user message rate limit (default: 30) |
| `MAX_AUTH_ATTEMPTS_PER_MINUTE` | No | Per-IP auth rate limit (default: 5) |
| `NONCE_EXPIRY_SECONDS` | No | Replay protection window (default: 300) |

When `ADMIN_TOKEN` is configured only users with the roles `Admin`, `Mod` or
`Owner` may create or delete text/voice channels.

## Role management

Roles control who can manage channels and delete other users' messages. The
available built-in roles are **Owner**, **Admin** and **Mod**.

### Bootstrapping the Owner from Docker

The first Owner must be assigned from the server terminal because no one has
permission to grant roles yet. Run the CLI subcommand inside the Docker
container:

```bash
docker exec <server-container> murmer_server set-role <public_key> Owner
```

Replace `<public_key>` with the user's Ed25519 public key (shown in the client
settings) and `<server-container>` with the container name (e.g.
`murmer-server-1`). You can also pass an optional hex colour as a third
argument.

### Managing roles from the client

Once a user has the **Owner** role they can assign or remove roles for other
users directly in the desktop client. Right-click any user in the sidebar user
list and choose a role from the context menu. The change takes effect
immediately for all connected clients.

### Using the HTTP endpoint

The `POST /role` endpoint (guarded by `ADMIN_TOKEN`) still works for scripted or
external integrations. See the configuration table above for details.

## Windows build instructions

1. Install the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)
   for Windows (Visual Studio Build Tools, WebView2, etc.).
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

## Security highlights

- Authentication uses Ed25519 signatures; timestamps are validated and bound to
  per-user nonces.
- IP-based rate limiting protects authentication and chat message throughput.
- Filenames are sanitised and image contents inspected before saving.
- Admin operations use constant-time comparisons to mitigate timing attacks.
- Channel management honours server-side role assignments when admin mode is on.

## Contributing

See [`CONTRIBUTING.md`](CONTRIBUTING.md) for detailed guidelines.
