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
- Secure file and image sharing (extension safe-list, content-type checks, size limits and path sanitisation)
- Desktop client with auto-reconnect and connection quality indicators
- Slash commands (`/help`, `/me`, `/shrug`, `/topic`, `/status`, `/focus`,
  `/ephemeral`, `/search`)
- Ephemeral messaging, message search, server-synced pinned messages and message editing
- Message replies with quoted previews and lightweight threads
- Typing indicators and per-channel unread badges with new-message markers
- Moderation tools: role-gated kick, ban and timed mutes
- Direct messages between users with persistent history and unread badges
- Screen sharing in voice channels
- REST API for bots (see [`murmer_server/BOT_API.md`](murmer_server/BOT_API.md))

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
- `murmer_server/BOT_API.md` – REST API reference for bots
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

Note: because the app ships auto-updates (see below), `npm run tauri build`
signs the updater artifacts and therefore needs the signing key in the
environment:

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY = Get-Content "$env:USERPROFILE\.tauri\murmer.key" -Raw
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = "<password>"
```

4. (Optional) Produce an optimised server binary:

```bash
cd ../murmer_server
cargo build --release
```

## Releases and auto-updates

The desktop client updates itself via the Tauri updater: **Settings → Updates →
Check for Updates** downloads and installs the latest GitHub release without a
manual download. This requires every release to ship signed updater artifacts
and a `latest.json`, which the `Release` GitHub Actions workflow
([`.github/workflows/release.yml`](.github/workflows/release.yml)) produces
automatically.

One-time setup (already done for this repository once the secrets exist):

1. Generate the updater signing keypair:

   ```bash
   cd murmer_client
   npm run tauri signer generate -- -w ~/.tauri/murmer.key
   ```

   Keep the private key safe — if it is lost, existing installs can no longer
   receive updates and users must reinstall manually.
2. Put the public key into `plugins.updater.pubkey` in
   `murmer_client/src-tauri/tauri.conf.json`.
3. Add the repository secrets `TAURI_SIGNING_PRIVATE_KEY` (contents of the key
   file) and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (the password chosen during
   generation) under GitHub → Settings → Secrets → Actions.

Publishing a release:

1. Bump the version:

   ```bash
   cd murmer_client
   npm run bump
   ```

   Versions follow the date-based scheme `YYYY.MDD.N` (year, month+day,
   counter for multiple releases on the same day), e.g. `2026.710.0` for the
   first release on 2026-07-10. The script writes the new version into
   `package.json`, `src-tauri/tauri.conf.json` and `src-tauri/Cargo.toml`.
   The scheme stays semver-ordered — required, because installed clients only
   offer an update when the new version compares greater than theirs.
2. Commit, tag and push:

   ```bash
   git commit -am "Release v<version>"
   git tag v<version>
   git push origin v<version>
   ```

The workflow builds the NSIS installer, signs the updater artifacts and
publishes everything as a regular (non-prerelease) GitHub release. Releases
must not be marked as pre-release — the updater endpoint
`releases/latest/download/latest.json` ignores prereleases.

## Security highlights

- Authentication uses Ed25519 signatures; timestamps are validated and bound to
  per-user nonces. A claimed public key is always verified — also on servers
  without a password — so roles and moderation identity cannot be spoofed.
- IP-based rate limiting protects authentication and chat message throughput.
- Filenames are sanitised, uploads are limited to a safe-list of extensions and image contents are inspected before saving.
- Admin token and server password checks use constant-time comparisons to
  mitigate timing attacks.
- Channel management honours server-side role assignments when admin mode is on.

## Contributing

See [`CONTRIBUTING.md`](CONTRIBUTING.md) for detailed guidelines.
