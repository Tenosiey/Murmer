# Murmer Client Guide

The client is built with **SvelteKit 2** (static adapter, SSR off) and ships
inside a **Tauri 2** shell. The *same* build is also the **web client**: it is
served as a plain web page by any static host, most simply by the Murmer server
itself (`WEB_CLIENT_DIR`). There is no second bundle and no build flag —
`src/lib/platform.ts` answers `isTauri` at runtime, and every native
integration (the updater, OS-level global hotkeys, the tray icon, native
notifications) asks it first and imports its Tauri plugin dynamically so the
plugin never reaches the web bundle. The adapter also writes a `200.html` SPA
fallback, which is what lets a static host resolve a deep link such as
`/invite#…`.

Svelte 5 is used with the **runes syntax** throughout — `$props()` props,
`$state`/`$derived` reactivity and `$effect` side effects; `runes: true` in `svelte.config.js` rejects legacy syntax at
build time. Cross-component state stays in `svelte/store` modules under
`src/lib/stores/`, consumed via `$store` auto-subscription. TypeScript is
pinned to major 6.

## Development commands
- `bun install` – install/update dependencies and refresh `bun.lock`
- `bun run dev` – run the Svelte dev server with hot module reloading
- `bun run tauri dev` – launch the desktop shell backed by the dev server
- `bun run build` – produce the static assets, consumed both by Tauri and by
  anything serving `build/` as the web client
- `bun run tauri build` – package installers/bundles for distribution
- `bun run check` – TypeScript + Svelte diagnostics (run before committing)
- `bun run test` – Vitest unit tests (`bun run test:watch` while iterating)

## Code organisation
- `src/routes/` – SvelteKit pages (login, server selection, invite, chat)
- `src/lib/platform.ts` – `isTauri`/`isWebClient`, the single answer to which
  shell we are in. Check `__TAURI_INTERNALS__`, never `__TAURI__`: the latter
  only exists with `withGlobalTauri`, which this app does not set, so it
  reports "browser" inside the desktop app
- `src/lib/invite.ts` – invite links. One `https://…/invite#…` URL serves both
  clients: clicking it opens the web client's `/invite` route, pasting it into
  the server hub's address field is parsed there. The server details ride in
  the **fragment** because an invite may carry the server password, and a
  fragment never reaches a web server (no access logs, no `Referer`)
- `src/lib/components/` – reusable UI components (overlays, menus, indicators)
- `src/lib/components/chat/` – sections of the chat page (sidebar, header, …)
- `src/lib/stores/` – Svelte stores holding client state
- `src/lib/chat/` – constants and helper functions for the chat page
- `src/lib/channel-crypto.ts` – sealing and key wrapping for encrypted channels
  (`dm-crypto.ts` holds the identity→X25519 conversion both share)
- `src/lib/voice/` – WebRTC helpers, push-to-talk tooling, RNNoise noise
  suppression (`denoise.ts`) and the settings-UI microphone tools (level meter,
  record-and-play-back test)
- `src/lib/wiki/` – channel wiki helpers: slug rules, the `[[wikilink]]`
  action and the pure line diff (`diff.ts`) the revision history view renders
- `src/lib/screenshare/` – WebRTC screen sharing manager
- `src/lib/webrtc/` – what both WebRTC managers share: the repair policy for
  broken connections (`recovery.ts`) and the DTLS fingerprint check that tells
  an ICE restart from a rebuilt connection (`fingerprint.ts`)
- `src-tauri/` – Rust-side glue for native integrations
- `test/` – Vitest harness: the `localStorage` stub (`setup.ts`) and the
  `$app/environment` stand-in (`stubs/`), plus `server-mirror.test.ts`, which
  parses `murmer_server/src/{permissions,upload,db/chat_settings,db/voice_defaults}.rs`
  and asserts the client's copies of those tables and bounds still match.
  Everything else lives next to the module it covers as `*.test.ts`.

Prefer small, composable Svelte components. Styling rules: components use
the design tokens defined in `src/routes/+layout.svelte` —
`--color-*`, `--space-*` (4px scale, for all padding/margin/gap),
`--text-*`, `--radius-*`, `--shadow-*`, `--control-height*` and `--z-*`.
No hardcoded colors, font sizes or one-off spacing values. Reuse the shared
primitives from the layout (`.btn`, `.btn-primary`, `.btn-ghost`,
`.btn-danger`, `.icon-btn`, `.field`, `.menu-panel`, `.badge`,
`.surface-card`) instead of restyling buttons/inputs per component. UI text
uses Inter (`--font-sans`); JetBrains Mono (`--font-mono`) is reserved for
code, timestamps and server addresses. Icons are inline stroke SVGs
(1.8 stroke width) — no emoji as icons.

The app logo is `src/lib/components/MurmerLogo.svelte` — reuse it instead of
inlining the artwork. It draws itself from the fixed `--color-brand-tile` /
`--color-brand-mark` tokens, which the light/dark theme switches; those two are
deliberately exempt from the accent re-tinting that `--color-*` gets. The same
artwork is duplicated in `static/logo/` (favicon) and `src-tauri/icons/`
(installer/tray) because those are consumed outside the DOM — keep all three in
sync, see the Brand section in `README.md`.

## State and persistence
- All client state lives in Svelte stores (`src/lib/stores/`); most persist to
  `localStorage` under `murmer_*` keys.
- Server connection state flows through `stores/chat.ts` (owns the
  `WebSocketManager`); register frame handlers with `chat.on(type, cb)` and
  clean them up with `chat.off`.
- Per-channel client state that persists (last-read markers, notification
  preferences) is namespaced by server URL — channel ids are only unique per
  server. Follow that pattern for any new per-channel persistence.
- Server-wide settings are **the server's answer, cached** — never local
  state. `stores/{chatSettings,uploadConfig,voiceDefaults,screenShare,
  serverIdentity}.ts` each parse the frame the server sends after
  authentication and broadcasts on change, validate it before mutating
  anything, and reset on disconnect so one server's policy never leaks into
  the next. The Server Dashboard edits them by sending a `set-*` frame and
  waiting for the broadcast to confirm; it never writes the store itself.
  Two of these are answers to a request rather than broadcasts, because they
  are manager-only: the profanity word list (`chat-settings` in reply to
  `get-chat-settings`) and `stores/bans.ts`/`stores/storageUsage.ts`. Their
  "not disclosed yet" state is `null`, which is deliberately not the same as
  "empty" — an editor must not offer to save an empty list over a real one.
- Names: render `$displayNames(user)` from `stores/profiles.ts`, never the raw
  user name — that is the account name and stays the key for every lookup
  (`$roles[user]`, `$avatars[user]`, DM peers, mentions). `UserProfileModal`
  shows both and is the only place a user edits their own display name, about
  text and avatar.

## Security considerations
- Key pairs are stored in `localStorage`; treat this as acceptable for the
  prototype but evaluate more secure storage for production. The Ed25519
  identity key doubles as the DM encryption key (converted to X25519 in
  `src/lib/dm-crypto.ts`), so losing it also makes past DMs unreadable.
- DM encryption trusts the server as key directory only on first contact:
  `stores/peerKeys.ts` pins each peer's key per server URL, flags changes,
  and blocks sending until the user explicitly trusts the new key. Keep that
  flow intact when touching DM code; there is no forward secrecy.
- Encrypted channels (`channel-crypto.ts`, `stores/channelKeys.ts`) seal
  messages under a shared per-channel key and wrap that key per member with the
  same identity keys — so they inherit the same pinning: `channelKeys` **must
  not** wrap the channel key for a member whose key changed under the pin, and
  the store surfaces those members instead. The key store is deliberately a leaf
  module: the chat store injects its transport with `setTransport` rather than
  being imported back, which is what keeps its policy (open epoch 1, hand out
  the current key, rotate away from a departed holder) unit-testable without a
  WebSocket. Keys live in memory only — the server holds the durable wraps —
  and are dropped on `connect`/`disconnect` so they never cross servers.
  Messages keep their sealed `enc` envelope after decryption so a key that
  arrives late can re-open them; `decryptPending` is the "waiting for the key"
  state, `decryptFailed` is the final one.
- Uploads are authenticated: `src/lib/upload.ts` signs `upload:<timestamp>`
  with the identity key and puts the proof in the multipart body ahead of the
  file. Build every `/upload` request with `uploadForm` — a hand-rolled
  `FormData` is rejected by the server, and moving the credentials into a
  header would add a CORS preflight servers do not answer.
- Always validate server responses before mutating client state.
- DOMPurify sanitises Markdown output – keep the dependency up to date.
  `src/lib/markdown.test.ts` guards that boundary and is the only suite that
  leaves the Node default (`// @vitest-environment jsdom`, because DOMPurify
  needs a DOM). Not happy-dom: it mis-drives DOMPurify's tree walk and lets
  `<script>` through, so the tests would pass on broken output.
- Avoid `{@html ...}` unless the content is sanitised explicitly.

## Web client
The browser cannot do everything the shell can, and the difference is enforced
by the platform rather than by us:
- Global hotkeys stay in-app (a web page cannot grab a key from the OS) and the
  updater is hidden — both gated on `isTauri` in `SettingsModal.svelte`.
- Microphone and screen capture need a **secure context**, so voice only works
  over HTTPS (or on `localhost`).
- An HTTPS page may not open a `ws://` socket or load `http://` attachments.
  The server hub warns when an added address would hit that, because otherwise
  the failure is a console message nobody sees.
Anything that reaches for a Tauri plugin has to keep both paths working — the
browser path is a shipped target, not a development convenience.

## Rust (Tauri) side
The native shell lives in `src-tauri/`. After making changes there, run
`cargo clippy --all-targets -- -D warnings`. Keep the Rust code minimal –
prefer implementing features in Svelte unless native APIs are required.

## QA checklist
- Run `bun run check` and `bun run test` before submitting changes.
- Store logic worth a test is the kind whose failure is invisible in the UI:
  state namespaced per server URL, request/response correlation, parsing of
  untrusted server frames. Stores are module-level singletons that wire
  themselves up on import, so tests take a fresh instance with
  `vi.resetModules()` plus a dynamic `import()`, and mock `./chat` rather than
  pulling in the WebSocket manager.
- Exercise the reconnect flow and authentication failure cases manually.
- Verify that push-to-talk works with the configured keybinding on Windows,
  both with Murmer focused and with another application in front (the latter
  only applies to combos with a modifier or function key — a bare key is
  never grabbed system-wide).
