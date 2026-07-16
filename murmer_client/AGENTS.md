# Murmer Client Guide

The desktop client is built with **SvelteKit 2** (static adapter, SSR off) and
ships inside a **Tauri 2** shell. Svelte 5 is used with the **runes syntax**
throughout — `$props()` props, `$state`/`$derived` reactivity and `$effect`
side effects; `runes: true` in `svelte.config.js` rejects legacy syntax at
build time. Cross-component state stays in `svelte/store` modules under
`src/lib/stores/`, consumed via `$store` auto-subscription. TypeScript is
pinned to major 6.

## Development commands
- `npm install` – install/update dependencies and refresh `package-lock.json`
- `npm run dev` – run the Svelte dev server with hot module reloading
- `npm run tauri dev` – launch the desktop shell backed by the dev server
- `npm run build` – produce static assets consumed by Tauri
- `npm run tauri build` – package installers/bundles for distribution
- `npm run check` – TypeScript + Svelte diagnostics (run before committing)

## Code organisation
- `src/routes/` – SvelteKit pages (login, server selection, chat)
- `src/lib/components/` – reusable UI components (overlays, menus, indicators)
- `src/lib/components/chat/` – sections of the chat page (sidebar, header, …)
- `src/lib/stores/` – Svelte stores holding client state
- `src/lib/chat/` – constants and helper functions for the chat page
- `src/lib/voice/` – WebRTC helpers and push-to-talk tooling
- `src/lib/screenshare/` – WebRTC screen sharing manager
- `src-tauri/` – Rust-side glue for native integrations

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

## Security considerations
- Key pairs are stored in `localStorage`; treat this as acceptable for the
  prototype but evaluate more secure storage for production.
- Always validate server responses before mutating client state.
- DOMPurify sanitises Markdown output – keep the dependency up to date.
- Avoid `{@html ...}` unless the content is sanitised explicitly.

## Rust (Tauri) side
The native shell lives in `src-tauri/`. Run `cargo check` there after making
changes. Keep the Rust code minimal – prefer implementing features in Svelte
unless native APIs are required.

## QA checklist
- Run `npm run check` before submitting changes.
- Exercise the reconnect flow and authentication failure cases manually.
- Verify that push-to-talk works with the configured keybinding on Windows.
