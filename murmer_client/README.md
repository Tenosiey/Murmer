# Murmer Client

Built with Tauri 2 and SvelteKit. The same build is both the desktop shell
and the web client.

| You want | Go to |
| --- | --- |
| Project setup and configuration | the root [README.md](../README.md) |
| Code organisation and conventions | [AGENTS.md](AGENTS.md) |
| Deploying the web client and its constraints | ["Web client" in the root README](../README.md#web-client) |
| How the system fits together | [`../docs/architecture.md`](../docs/architecture.md) |

## Development

```bash
bun install
bun run tauri dev    # SvelteKit dev server + Tauri shell with hot reloading
bun run check        # TypeScript + Svelte diagnostics
bun run test         # Vitest unit tests
bun audit            # dependency vulnerability scan
```

The desktop shell logs to STDOUT; adjust verbosity with `RUST_LOG` when
launching `bun run tauri dev`.

## Two targets, one build

`bun run build` writes `build/`, which is both what the Tauri bundle embeds
and a complete web client: serve that directory — the Murmer server does it
with `WEB_CLIENT_DIR` — and Murmer runs in a browser, invite links included.
`src/lib/platform.ts` decides at runtime which of the two it is running as.

Client state (server list, session, settings, keypair) is persisted in
`localStorage` via the stores in `src/lib/stores/`.

> [!NOTE]
> On Linux systems with Snap installed, `bun run tauri` strips any
> `/snap/core*` entries from `LD_LIBRARY_PATH` before launching the Tauri CLI
> (see `scripts/run-tauri.js`). This avoids runtime errors such as
> `undefined symbol: __libc_pthread_init` caused by mixing the Snap glibc with
> the system toolchain. If you invoke the Tauri CLI manually, clean that
> variable the same way.
