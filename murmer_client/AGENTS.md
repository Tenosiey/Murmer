# Murmer Client Guide

The desktop client is built with **SvelteKit 2** and ships inside a **Tauri 2**
shell. This document outlines the most common workflows for contributors.

## Development commands
- `npm install` – install/update dependencies and refresh `package-lock.json`
- `npm run dev` – run the Svelte dev server with hot module reloading
- `npm run tauri dev` – launch the desktop shell backed by the dev server
- `npm run build` – produce static assets consumed by Tauri
- `npm run tauri build` – package installers/bundles for distribution
- `npm run check` – TypeScript + Svelte diagnostics (run before committing)

The Tauri configuration in `src-tauri/tauri.conf.json` invokes `npm run dev`
when you start the shell in development mode.

## Code organisation
- `src/routes/` – SvelteKit pages (login, server selection, chat)
- `src/lib/components/` – reusable UI primitives
- `src/lib/stores/` – Svelte stores holding client state
- `src/lib/voice/` – WebRTC helpers and push-to-talk tooling
- `src-tauri/` – Rust-side glue for native integrations

Document complex components with HTML comments at the top of the file and prefer
small, composable Svelte components. Re-export shared utilities from
`src/lib/index.ts` if they need to be consumed in multiple places.

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
