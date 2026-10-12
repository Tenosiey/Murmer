import { defineConfig } from 'vitest/config';
import { fileURLToPath } from 'node:url';

const resolvePath = (path: string) => fileURLToPath(new URL(path, import.meta.url));

// Deliberately *not* an extension of `vite.config.ts`: the SvelteKit plugin
// wants a synced `.svelte-kit/` and a dev server, neither of which the store
// tests need. The aliases below are all the app modules ask of the
// framework — `$app/env` for the browser flag and `$app/paths` for
// the asset prefix the theme store builds the favicon from. `#lib` needs no
// alias: it is a package.json subpath import Vite resolves on its own.
export default defineConfig({
  resolve: {
    alias: {
      '$app/env': resolvePath('./test/stubs/app-env.ts'),
      '$app/paths': resolvePath('./test/stubs/app-paths.ts')
    }
  },
  test: {
    environment: 'node',
    include: ['src/**/*.test.ts', 'test/**/*.test.ts'],
    setupFiles: ['./test/setup.ts']
  }
});
