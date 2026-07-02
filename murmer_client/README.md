# Murmer Client

Desktop client built with Tauri 2 and SvelteKit. For project setup and
configuration see the root [README.md](../README.md); code organisation and
conventions are described in [AGENTS.md](AGENTS.md).

## Development

```bash
npm install
npm run tauri dev    # SvelteKit dev server + Tauri shell with hot reloading
npm run check        # TypeScript + Svelte diagnostics
npm audit            # dependency vulnerability scan
```

The desktop shell logs to STDOUT; adjust verbosity via the `RUST_LOG`
environment variable when launching `npm run tauri dev`.

Client state (server list, session, settings, keypair) is persisted in
`localStorage` via the stores in `src/lib/stores/`.

> [!NOTE]
> On Linux systems with Snap installed, `npm run tauri` strips any
> `/snap/core*` entries from `LD_LIBRARY_PATH` before launching the Tauri CLI
> (see `scripts/run-tauri.js`). This avoids runtime errors such as
> `undefined symbol: __libc_pthread_init` caused by mixing the Snap glibc with
> the system toolchain. If you invoke the Tauri CLI manually, clean that
> variable the same way.
