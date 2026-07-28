/**
 * Stand-in for SvelteKit's `$app/environment`, aliased in `vitest.config.ts`.
 *
 * `browser` is `true` because the app only ever runs in a browser (static
 * adapter, SSR off) — stores guard their `localStorage` access with it, and a
 * `false` here would silently skip every persistence path under test.
 */
export const browser = true;
export const building = false;
export const dev = true;
export const version = 'test';
