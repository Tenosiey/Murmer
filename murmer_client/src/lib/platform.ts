/**
 * Which shell the app is running in.
 *
 * The same bundle is served by the Tauri desktop shell and by any static web
 * host (see the "Web client" section in `README.md`), so every native
 * integration has to ask before reaching for a plugin. Tauri v2 exposes
 * `__TAURI_INTERNALS__` on the window unconditionally; `__TAURI__` only exists
 * when `withGlobalTauri` is enabled, which this app does not set — checking for
 * it silently reports "browser" inside the desktop app.
 */
export const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

/** Running as a plain web page rather than inside the desktop shell. */
export const isWebClient = !isTauri;
