# Architecture

How Murmer is put together: the two halves, the one build that serves two
targets, what crosses the wire between them, and where state lives on each
side. Start here; the other documents in `docs/` go deep on one subsystem
each.

## The two halves

```
murmer_client/                          murmer_server/
┌──────────────────────────┐            ┌───────────────────────────┐
│ SvelteKit 2 (static,     │  /ws       │ Axum 0.8 (Rust 2024)      │
│ SSR off) + Svelte 5 runes│◄──────────►│ WebSocket + HTTP          │
│                          │  JSON      │                           │
│ ┌──────────┐ ┌─────────┐ │  frames    │ ┌───────────────────────┐ │
│ │ Tauri 2  │ │ browser │ │            │ │ SQLite (rusqlite via  │ │
│ │ shell    │ │ tab     │ │  /upload   │ │ tokio-rusqlite), one  │ │
│ └──────────┘ └─────────┘ │  /files    │ │ connection thread     │ │
│   one build, both        │  /role     │ └───────────────────────┘ │
│                          │  /link-…   │                           │
└──────────────────────────┘            └───────────────────────────┘
         ▲                                          
         │  WebRTC: voice audio, camera video and screen shares
         └─ travel peer-to-peer between clients. The server
            only relays signaling (offer/answer/candidate).
```

Two consequences of that last box are worth internalising before touching
anything voice-shaped:

- **Media never reaches the server.** It relays SDP and ICE candidates and
  otherwise knows nothing about the call. Everyone in a voice channel holds a
  connection to everyone else, which is why per-peer cost matters so much —
  see [`voice.md`](voice.md).
- **Voice "permissions" are therefore hints.** The server can refuse to let
  you join a voice channel, but it cannot stop audio between two peers that
  already connected. That gap is documented where it appears and is the only
  place client-side gating is load-bearing.

## One build, two targets

There is no second bundle and no build flag. `src/lib/platform.ts` answers
`isTauri` at runtime by checking `__TAURI_INTERNALS__` — never `__TAURI__`,
which only exists with `withGlobalTauri` and would report "browser" inside
the desktop app. Every native integration (updater, OS-level global hotkeys,
tray icon, native notifications) asks it first and imports its Tauri plugin
**dynamically**, so the plugin never lands in the web bundle.

The static adapter also writes a `200.html` SPA fallback, which is what lets
a plain static host resolve a deep link such as `/invite#…`.

The browser cannot do everything the shell can, and the difference is
enforced by the platform rather than by us:

- Global hotkeys stay in-app (a web page cannot grab a key from the OS) and
  the updater is hidden — both gated on `isTauri` in `SettingsModal.svelte`.
- Microphone and screen capture need a **secure context**, so voice only
  works over HTTPS (or on `localhost`).
- An HTTPS page may not open a `ws://` socket or load `http://` attachments.
  The server hub warns when an added address would hit that, because
  otherwise the failure is a console message nobody sees.

Anything reaching for a Tauri plugin has to keep both paths working. The
browser path is a shipped target, not a development convenience.

The simplest way to run both targets at once is to point the server at the
built client: `WEB_CLIENT_DIR=…/murmer_client/build` makes the server serve
the page itself, so the page and `/ws` share an origin and CORS can stay off.

## What crosses the wire

**One WebSocket, `/ws`**, carrying JSON objects with a `type` field. Frames
leave the server by one of three routes — server-wide broadcast,
channel-scoped broadcast, or a per-connection direct mailbox — and picking
the wrong one is either a leak or a performance bug. That choice, the
validation helpers and the walkthrough for adding a frame are in
[`protocol.md`](protocol.md) and
[`../agents/skills/websocket-frames.md`](../agents/skills/websocket-frames.md).

**A handful of HTTP endpoints**, all in `murmer_server/src/`:

| Endpoint | Module | Notes |
| --- | --- | --- |
| `/upload` | `upload.rs` | Multipart. Ed25519-authenticated *ahead of* the file bytes; per-IP rate limited. |
| `/files/<key>` | `main.rs` | Serves uploaded files. |
| `/link-preview` | `link_preview.rs` | OpenGraph metadata for pasted URLs. |
| `/role` | `admin.rs` | Bearer-token bootstrap for the first Owner. |
| `/api/…` | `bot/` | The bot REST API — [`../murmer_server/BOT_API.md`](../murmer_server/BOT_API.md). |

`/upload` deliberately carries its credentials as multipart **fields**, not
headers: a custom header would make the request preflighted, and production
servers run with CORS disabled. See [`security.md`](security.md).

## Where state lives

**On the server**, in one SQLite database plus a few in-memory mirrors of
rows that every request consults (`AppState.chat_settings`,
`channel_overrides`, `role_defs`, `user_roles`, `voice_channels`,
`stats_enabled`). The mirrors are an optimisation, not a second source of
truth: whatever writes the row refreshes the mirror in the same step, and
anything that deletes rows wholesale — the Danger Zone reset — must clear
them too, or a deleted channel stays joinable until the next restart.

All queries run on **one connection thread**, so per-query cost is shared by
everything. Statements go through `prepare_cached` (the cache capacity is
raised in `db::init`, since the default of 16 is below the number of distinct
statements here).

**On the client**, in Svelte stores under `src/lib/stores/`, most persisted
to `localStorage` under `murmer_*` keys and namespaced by server URL where
the value is per-server. The important distinction — local preference versus
cached server answer — is [`client-state.md`](client-state.md).

## Subsystem reference

| Document | Covers |
| --- | --- |
| [`protocol.md`](protocol.md) | WebSocket frames, the three routing paths, HTTP endpoints |
| [`security.md`](security.md) | Identity keys, DM and channel E2EE, upload auth, rate limiting |
| [`permissions.md`](permissions.md) | The permission bitmask, roles, private channels, chat policy, Danger Zone |
| [`voice.md`](voice.md) | Capture chain, RNNoise, VAD, Opus/DTX, connection repair |
| [`screen-sharing.md`](screen-sharing.md) | The screen-share manager and its floating windows |
| [`client-state.md`](client-state.md) | Stores, persistence, per-server namespacing, display names |
| [`ui.md`](ui.md) | Design tokens, shared primitives, icons, brand assets |
| [`features.md`](features.md) | Wiki, soundboard, profiles, emojis, stats |
| [`testing.md`](testing.md) | How the suites are shaped and what belongs in one |
