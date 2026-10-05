# Murmer Client

Tauri 2 and SvelteKit. The same build is both the desktop app and the web
client.

| You want | Go to |
| --- | --- |
| Project setup and configuration | the root [README.md](../README.md) |
| Deploying the web client | ["Web client" in the root README](../README.md#web-client) |
| Commands, code organisation and conventions | [AGENTS.md](AGENTS.md) |
| How the system fits together | [`../docs/architecture.md`](../docs/architecture.md) |

> [!NOTE]
> On Linux systems with Snap installed, `bun run tauri` strips any
> `/snap/core*` entries from `LD_LIBRARY_PATH` before launching the Tauri CLI
> (see `scripts/run-tauri.js`). This avoids runtime errors such as
> `undefined symbol: __libc_pthread_init` caused by mixing the Snap glibc with
> the system toolchain. If you invoke the Tauri CLI manually, clean that
> variable the same way.
