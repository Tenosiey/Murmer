# Murmer Client Guide

Read the repository root [`AGENTS.md`](../AGENTS.md) first — it carries the
hard constraints, the invariants and the index of task guides. This file adds
the client's commands and module map.

**SvelteKit 2** (static adapter, SSR off) with **Svelte 5 runes**, inside a
**Tauri 2** shell. The *same* build is also the **web client**, served as a
plain page by any static host — most simply by the Murmer server itself
(`WEB_CLIENT_DIR`). There is no second bundle and no build flag. How that
works, and what the browser cannot do, is
[`../docs/architecture.md`](../docs/architecture.md).

## Development commands

```bash
bun install          # install/update dependencies, refresh bun.lock
bun run dev          # Svelte dev server with HMR
bun run tauri dev    # desktop shell backed by the dev server
bun run build        # static assets — consumed by Tauri and by the web client
bun run tauri build  # package installers/bundles
bun run check        # TypeScript + Svelte diagnostics — must be 0 errors, 0 warnings
bun run test         # Vitest (bun run test:watch while iterating)
```

## Code organisation

| Path | Contents |
| --- | --- |
| `src/routes/` | SvelteKit pages: login, server selection, invite, chat |
| `src/lib/components/` | Reusable UI components (overlays, menus, indicators) |
| `src/lib/components/chat/` | Sections of the chat page (sidebar, header, …) |
| `src/lib/stores/` | Svelte stores holding client state — [`../docs/client-state.md`](../docs/client-state.md) |
| `src/lib/chat/` | Constants and helpers for the chat page |
| `src/lib/voice/` | WebRTC voice, camera video, push-to-talk, RNNoise, mic tooling — [`../docs/voice.md`](../docs/voice.md) |
| `src/lib/screenshare/` | Screen sharing manager — [`../docs/screen-sharing.md`](../docs/screen-sharing.md) |
| `src/lib/webrtc/` | What both WebRTC managers share: `recovery.ts`, `fingerprint.ts` |
| `src/lib/wiki/` | Channel wiki: slug rules, `[[wikilink]]`, the pure line diff |
| `src/lib/channel-crypto.ts` | Sealing and key wrapping for encrypted channels |
| `src/lib/dm-crypto.ts` | The identity→X25519 conversion DMs and channels share |
| `src/lib/identity.ts` | Identity backup/restore formats — [`../agents/skills/crypto-changes.md`](../agents/skills/crypto-changes.md) |
| `src/lib/upload.ts` | Signed `/upload` bodies — always build them with `uploadForm` |
| `src/lib/platform.ts` | `isTauri`/`isWebClient`, the single answer to which shell we are in |
| `src/lib/invite.ts` | Invite links; the payload rides in the URL fragment |
| `src-tauri/` | Rust-side glue for native integrations |
| `test/` | Vitest harness and `server-mirror.test.ts`; unit tests live next to their module |

Two module-level rules that are easy to miss:

- `platform.ts` checks `__TAURI_INTERNALS__`, **never** `__TAURI__` — the
  latter only exists with `withGlobalTauri`, which this app does not set, so
  it reports "browser" inside the desktop app.
- `invite.ts` puts the server details in the **fragment** because an invite
  may carry the server password, and a fragment never reaches a web server —
  no access logs, no `Referer`.

## Working here

| Task | Guide |
| --- | --- |
| Building or editing a component | [`../agents/skills/svelte-ui.md`](../agents/skills/svelte-ui.md) |
| Adding a WebSocket frame | [`../agents/skills/websocket-frames.md`](../agents/skills/websocket-frames.md) |
| Writing a test | [`../agents/skills/client-tests.md`](../agents/skills/client-tests.md) |
| Verifying a visible change | [`../agents/skills/visual-verification.md`](../agents/skills/visual-verification.md) |
| Touching crypto | [`../agents/skills/crypto-changes.md`](../agents/skills/crypto-changes.md) |
| Copying a value from the server | [`../agents/skills/mirrored-constants.md`](../agents/skills/mirrored-constants.md) |

Design tokens, shared primitives and brand assets: [`../docs/ui.md`](../docs/ui.md).

## The Tauri shell

The native shell lives in `src-tauri/`. Keep it minimal — implement features
in Svelte unless a native API is genuinely required. After changing anything
there:

```bash
cd src-tauri && cargo fmt && cargo clippy --all-targets -- -D warnings
```

## QA checklist

- `bun run check` (0/0) and `bun run test` before submitting.
- Look at the change in the running app — there are no component rendering
  tests on purpose.
- Exercise the reconnect flow and authentication failure cases manually; the
  suites do not cover them.
- Verify push-to-talk with the configured keybinding on Windows, both with
  Murmer focused and with another application in front. The latter only
  applies to combos with a modifier or function key — a bare key is never
  grabbed system-wide.
