# Contributor Guide

This monorepo hosts **Murmer**, a desktop chat prototype split into a
Tauri/SvelteKit client (`murmer_client/`) and an Axum-based Rust server
(`murmer_server/`). Each directory contains its own `AGENTS.md` with tooling
specifics. Client and server communicate over one WebSocket (`/ws`, JSON
frames with a `type` field) plus a few HTTP endpoints (`/upload`,
`/link-preview`, `/role`, `/files`, bot REST API).

## Hard constraints
- **TypeScript stays on major 6.** Do not upgrade to 7 or merge dependabot
  PRs that do.
- **No backwards compatibility.** Only the latest versions of everything are
  supported; never add compat shims, polyfills or legacy code paths.
- **Rust is edition 2024**; the toolchain is pinned in `rust-toolchain.toml`.
- **rusqlite is pinned by tokio-rusqlite** — bump it only when a new
  tokio-rusqlite release allows it.
- **The client toolchain is Bun 1.x.** Dependencies, scripts and CI use Bun
  (`bun install` writes `bun.lock`); there is no npm lockfile. `package.json`
  declares no `engines` (Bun ignores it).
- **Never bump versions by hand** — only via `bun run bump` (see Versioning).
- **Svelte components use the runes syntax** (`$state`, `$props`, `$derived`,
  `$effect`) — `runes: true` is enforced in `svelte.config.js`, so legacy
  syntax (`export let`, `$:`) fails the build. Shared state still lives in
  `svelte/store` modules (`src/lib/stores/`), consumed via `$store`
  auto-subscription; never import from `svelte/legacy`.

## Workflow overview
- Install the latest [Rust toolchain](https://www.rust-lang.org/tools/install)
  and [Bun 1.x](https://bun.sh).
- See `README.md` for detailed setup, build and configuration instructions.
- When developing locally run the client with `bun run tauri dev` and the server
  with `cargo run` or `docker compose up --build`.

## Quality checks
- Server: `cargo fmt`, `cargo clippy --all-targets -- -D warnings` and
  `cargo test` inside `murmer_server/` — all three pass clean; keep it that way.
- Client: `bun run check` inside `murmer_client/` (0 errors, 0 warnings);
  `cargo clippy` in `murmer_client/src-tauri/` for the shell.
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
- Direct messages are end-to-end encrypted (NaCl box over X25519 keys derived
  from the users' Ed25519 identity keys via ed2curve). The server only
  validates, stores and relays `nonce`/`ciphertext` pairs — it must never
  gain a plaintext DM path. Clients pin peer keys on first use
  (`stores/peerKeys.ts`), block sending on key changes until the user trusts
  the new key, and expose a fingerprint for out-of-band verification.
- Rate limiting exists for both authentication and chat traffic.
- File uploads are validated by size and an extension safe-list; images are additionally checked by magic bytes. Active content (HTML, SVG, scripts) is never accepted.
- Authorization is a **permission bitmask**, not fixed roles. Server owners
  define custom roles in the Server Dashboard and toggle each capability
  (view/send/manage channels/kick/ban/manage roles/…) per role. A user's
  effective permissions are the union of the built-in `@everyone` baseline
  role and every role assigned to them; `ADMINISTRATOR` (the Owner role) grants
  everything. The flag set is defined in `murmer_server/src/permissions.rs` and
  mirrored in `murmer_client/src/lib/chat/permissions.ts` — keep them in sync.
  Roles stack (a user may hold several) and carry a hierarchy `position`;
  moderation and role management require strictly outranking the target, and a
  manager can never grant a permission it lacks. Every check is enforced
  server-side (`ws/helpers.rs::has_permission`/`top_position`); client gating
  is cosmetic. Without `ADMIN_TOKEN`, channel and wiki management stay open to
  everyone so a small unadministered server remains usable.
- **Private channels** layer per-channel allow/deny overrides (for `@everyone`,
  roles and individual users) on top of the server-wide permissions, resolved
  by `channel_permissions`/`can_view_channel` in `ws/helpers.rs`. Overrides only
  touch "see" (`VIEW_CHANNELS`) and "write/talk" (`SEND_MESSAGES`). The server
  hides invisible channels from listings, filters channel-scoped broadcasts per
  recipient (the `global_rx` loop in `ws/handlers/mod.rs`), and refuses
  join/history/send/voice-join for channels a user cannot see. Voice **talk** is
  the one client-enforced piece (mic disabled via the `voice-permissions` hint)
  because audio is peer-to-peer; view/join and all text gates are server-enforced.
  Managers (`MANAGE_CHANNELS`) edit overrides via the `set/remove-channel-override`
  frames (`ws/handlers/channel_overrides.rs`); override data is sent only to
  managers.
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

1. Run `bun run bump` inside `murmer_client/`. The script
   (`scripts/bump-version.mjs`) computes the next version and writes it into
   all six versioned files: the client's `package.json`,
   `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`
   and `src-tauri/Cargo.lock`, plus the server's `Cargo.toml` and
   `Cargo.lock` (the lock files matter: `--locked` builds fail when they
   disagree). `bun.lock` needs no bump — it does not record the root version.
2. Commit with `Release v<version>` and create a matching `v<version>` git
   tag. Pushing the tag triggers the GitHub Actions release workflow, which
   builds the installers and updater manifest.

See `README.md` for the full release process.

## Validation checklist
- Ensure CI-equivalent commands above pass before opening a pull request.
- Perform manual smoke tests after changing networking, authentication or file
  handling logic.
- Keep documentation (`README.md`, `AGENTS.md`) in sync with code behaviour.
