# Murmer Client

Client built with Tauri 2 and SvelteKit. For project setup and
configuration see the root [README.md](../README.md); code organisation and
conventions are described in [AGENTS.md](AGENTS.md).

## Development

```bash
bun install
bun run tauri dev    # SvelteKit dev server + Tauri shell with hot reloading
bun run check        # TypeScript + Svelte diagnostics
bun run test         # Vitest unit tests
bun audit            # dependency vulnerability scan
```

The desktop shell logs to STDOUT; adjust verbosity via the `RUST_LOG`
environment variable when launching `bun run tauri dev`.

## Web client

`bun run build` writes `build/`, which is both what the Tauri bundle embeds and
a complete web client: serve that directory (the Murmer server does it with
`WEB_CLIENT_DIR`) and Murmer runs in a browser, invite links included. The
build is identical either way — `src/lib/platform.ts` decides at runtime which
of the two it is running as. See "Web client" in the root
[README.md](../README.md#web-client) for deployment and its constraints.

Client state (server list, session, settings, keypair) is persisted in
`localStorage` via the stores in `src/lib/stores/`.

> [!NOTE]
> On Linux systems with Snap installed, `bun run tauri` strips any
> `/snap/core*` entries from `LD_LIBRARY_PATH` before launching the Tauri CLI
> (see `scripts/run-tauri.js`). This avoids runtime errors such as
> `undefined symbol: __libc_pthread_init` caused by mixing the Snap glibc with
> the system toolchain. If you invoke the Tauri CLI manually, clean that
> variable the same way.
