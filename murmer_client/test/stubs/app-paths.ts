/**
 * Stand-in for SvelteKit's `$app/paths`, aliased in `vitest.config.ts`.
 *
 * `asset` adds no prefix because the app is served from the root, as it is
 * in the real build.
 */
export const asset = (file: string) => `/${file}`;
