# Contributor Guide

**Murmer** is a self-hostable voice and text chat prototype: an Axum WebSocket
server (`murmer_server/`) and one SvelteKit client (`murmer_client/`) whose
single build runs both as a Tauri desktop shell and, unchanged, as a **web
client** in a plain browser. The two halves talk over one WebSocket (`/ws`,
JSON frames with a `type` field) plus a few HTTP endpoints (`/upload`,
`/link-preview`, `/role`, `/files`, the bot REST API).

New to the codebase? Read [`docs/architecture.md`](docs/architecture.md)
first — it is the map everything else hangs off.

## Task Guides

Deeper instructions for specific kinds of work live in `agents/skills/`. Read
the matching guide **before** starting:

- [`agents/skills/websocket-frames.md`](agents/skills/websocket-frames.md) — adding or changing a WebSocket message type
- [`agents/skills/mirrored-constants.md`](agents/skills/mirrored-constants.md) — changing anything the server and client both define
- [`agents/skills/svelte-ui.md`](agents/skills/svelte-ui.md) — building or editing Svelte components
- [`agents/skills/visual-verification.md`](agents/skills/visual-verification.md) — verifying any change with a visible effect in the running app
- [`agents/skills/client-tests.md`](agents/skills/client-tests.md) — writing Vitest tests in `murmer_client/`
- [`agents/skills/server-tests.md`](agents/skills/server-tests.md) — writing Rust integration tests in `murmer_server/tests/`
- [`agents/skills/database-changes.md`](agents/skills/database-changes.md) — changing the SQLite schema or backfilling existing servers
- [`agents/skills/crypto-changes.md`](agents/skills/crypto-changes.md) — touching identity keys, DM or channel encryption
- [`agents/skills/releasing.md`](agents/skills/releasing.md) — bumping the version and cutting a release

## Documentation Layout

Four documentation trees, split by genre and audience:

- `agents/skills/` — task procedure ("do this when doing X"), for anyone
  working on the codebase
- `docs/` — reference on how the system is shaped: architecture, protocol,
  security model, voice pipeline, testing. Also for anyone working on the
  codebase; skills link here for depth
- `plans/` — design notes for work that is **not built yet**; a plan moves
  into `docs/` when it ships
- `README.md`, `murmer_server/BOT_API.md`, `CONTRIBUTING.md` — end-user,
  operator and newcomer documentation; never codebase internals

Two rules keep the trees from rotting:

- **One authority per fact.** Environment variables are documented in
  `README.md` and nowhere else; the permission flags are defined in
  `murmer_server/src/permissions.rs` and nowhere else. When another document
  needs such a list, link to the authority instead of copying it. A second
  copy is a copy that will eventually be wrong.
- **Explain the why, not the what.** The code already says what it does.
  Documentation earns its place by recording *why* a thing is shaped the way
  it is — especially where the failure mode is invisible: a key rotation that
  does not happen, a rate-limit sweep that silently stopped running, a voice
  gate that never closes.

## Hard constraints

- **TypeScript stays on major 6.** Do not upgrade to 7 or merge dependabot
  PRs that do; `svelte-check` is the blocker.
- **No backwards compatibility.** Only the latest versions of everything are
  supported; never add compat shims, polyfills or legacy code paths.
- **Rust is edition 2024**; the toolchain is pinned in `rust-toolchain.toml`.
- **rusqlite is pinned by tokio-rusqlite** — bump it only when a new
  tokio-rusqlite release allows it.
- **The client toolchain is Bun 1.x.** Dependencies, scripts and CI use Bun
  (`bun install` writes `bun.lock`); there is no npm lockfile. `package.json`
  declares no `engines` (Bun ignores it).
- **Never bump versions by hand** — only via `bun run bump`, see
  [`agents/skills/releasing.md`](agents/skills/releasing.md).
- **Svelte components use the runes syntax** (`$state`, `$props`, `$derived`,
  `$effect`) — `runes: true` in `svelte.config.js` fails the build on legacy
  syntax (`export let`, `$:`). Shared state still lives in `svelte/store`
  modules (`src/lib/stores/`), consumed via `$store` auto-subscription; never
  import from `svelte/legacy`.
- **Keep it simple.** Murmer is maintained by one or two people. Prefer the
  flatter folder, the smaller component, the plain function over the
  abstraction. A pattern that pays off at ten contributors costs at two.

## Invariants

These hold everywhere in the codebase. Breaking one is a security or data
bug, not a style disagreement.

- **The server is the only enforcement point.** Every permission, membership
  and rate decision is made in `murmer_server/src/ws/helpers.rs` and the
  handlers. Client-side gating keeps the UI honest and is *cosmetic*; never
  move a decision into the client. There is exactly one exception —
  voice **talk** permission, a hint (`voice-permissions`) because audio is
  peer-to-peer and the server cannot stop two peers that already connected.
  It is labelled as such where it appears. Everything else, including the
  soundboard cooldown, is decided server-side with at most a cosmetic mirror
  on the client.
- **The account name is the identity.** Display names and nicknames are
  decoration. Auth, roles, moderation, DM routing and message authorship all
  key on the account name, bound to a public key on first connect. Any lookup
  by display name is a bug — they are deliberately not unique. See
  [`docs/features.md`](docs/features.md).
- **Encrypted content has no plaintext path.** For DMs and end-to-end
  encrypted channels the server validates and stores opaque
  `nonce`/`ciphertext` pairs. Never add a code path that accepts or produces
  plaintext there — *reject* the frame rather than stripping the field, so a
  buggy client's user hears about it. See [`docs/security.md`](docs/security.md).
- **The identity key is the account, on every server at once.** Losing it
  loses every account plus every DM ever received, which is why it is backed
  up rather than merely stored, and why its on-disk formats are a promise to
  anyone holding an old backup. See
  [`agents/skills/crypto-changes.md`](agents/skills/crypto-changes.md).
- **Validate before you mutate.** Server frames are untrusted input to the
  client, client frames are untrusted input to the server. Parse, check, then
  act — in both directions.
- **Duplicated tables are tested, not trusted.** Where a table exists in both
  Rust and TypeScript, `murmer_client/test/server-mirror.test.ts` parses the
  Rust source and fails when the copies drift. See
  [`agents/skills/mirrored-constants.md`](agents/skills/mirrored-constants.md).

## Use these, not the raw thing

Each of these exists because the obvious approach is broken here in a way
that is not obvious until it ships.

| Instead of | Use | Because |
| --- | --- | --- |
| `window.prompt` / `confirm` / `alert` | `dialogs.*` from `stores/dialogs.ts` | WebView2 does not support them at all, so the desktop app silently gets nothing |
| A hand-rolled `FormData` for `/upload` | `uploadForm` in `src/lib/upload.ts` | The signed proof must be a multipart field ahead of the file; the server rejects anything else |
| The raw user name, or `profile.displayName` | `$displayNames(user)` from `stores/profiles.ts` | Skips the nickname a moderator may have set. The raw name is the account name and stays the lookup key |
| New CSS for a button or input | The tokens and primitives from `src/routes/+layout.svelte` | A hardcoded color or size follows neither the theme nor the accent re-tint |
| Inlining the logo artwork | `MurmerLogo.svelte` | It reads the brand tokens and switches with the theme itself |
| Touching the WebSocket directly | `chat.on(type, cb)` / `chat.off` | The manager owns reconnect and per-server reset |
| A hand-written permission check | `has_permission` / `can_view_channel` in `ws/helpers.rs` | One enforcement point, and private channels change the answer |
| `conn.prepare` | `prepare_cached` | Every query shares one connection thread |
| A raw `ALTER TABLE` | `ensure_column` in `db/mod.rs` | SQLite has no `ADD COLUMN IF NOT EXISTS` |
| `println!` | `tracing` (`info!`, `warn!`) | Structured logging is what operators actually read |
| Sleeping in a rate-limit test | `RateLimiter::with_clock(Clock::manual())` | The windows are a minute long |
| Editing a version by hand | `bun run bump` | Six files carry it and `--locked` builds fail when they disagree |

## Quality checks

Run these before pushing; `.github/workflows/ci.yml` runs the same ones on
every push to `main`/`dev` and on every pull request. A second workflow,
`.github/workflows/audit.yml`, audits the lockfiles for security advisories
every Monday and fails on nothing else.

```bash
cd murmer_server && cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test
```

```bash
cd murmer_client && bun run check && bun run test
```

```bash
cd murmer_client/src-tauri && cargo fmt && cargo clippy --all-targets -- -D warnings
```

`bun run check` must report **0 errors, 0 warnings**. How the suites are
shaped, what belongs in a test and what deliberately does not, is
[`docs/testing.md`](docs/testing.md).

Beyond the automated checks:

- Anything with a visible effect is verified in the running app —
  [`agents/skills/visual-verification.md`](agents/skills/visual-verification.md).
- Networking, authentication and file-handling changes get a manual smoke
  test; reconnect and auth-failure paths are not covered by the suites.
- Document complex security-sensitive logic with inline comments.
- Keep `README.md`, `AGENTS.md` and `docs/` in sync with behaviour. A change
  that makes a document wrong is not finished.

## Style

- Wrap prose in markdown and in comments at **78 columns**, matching the
  surrounding file. Do not reflow paragraphs you did not otherwise touch —
  it buries the real change in the diff.
- Write code that reads like the code around it: same comment density, same
  naming, same idiom. Match the file, not your preference.
- Rust: bubble errors with `Result`; no `panic!` on any path a client can
  reach. Log through `tracing` (`info!`, `warn!`), never `println!`. Every
  module opens with a doc comment naming its responsibility — extend it when
  you add behaviour.
- TypeScript: no `any` on anything that touches a server frame; parse it into
  a checked shape first. Prefer a pure function in `src/lib/` over logic
  inside a component — that is the part a test can reach.
- Comments explain *why*. A comment restating the line above it is noise; a
  comment recording the failure that made the line necessary is the reason
  the next person leaves it alone.

## Git

- Commits are atomic: one coherent change per commit, no unrelated work
  mixed in.
- Commit messages are succinct and describe the change being made.
- Feature branches merge into `dev`; `main` is the release branch.

## Where things live

```
murmer_client/      SvelteKit client — Tauri desktop shell and web client (TypeScript)
murmer_server/      Axum WebSocket + HTTP server (Rust)
agents/skills/      task guides, indexed above
docs/               architecture and subsystem reference
plans/              design notes for unbuilt work
docker-compose.yml  boots the server; the SQLite database lives on a volume
```

`murmer_client/AGENTS.md` and `murmer_server/AGENTS.md` carry the per-half
commands and module maps, and are loaded on top of this file when you work in
those directories.
