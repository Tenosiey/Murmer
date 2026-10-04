import { defineConfig } from 'vitest/config';
import { fileURLToPath } from 'node:url';

const resolvePath = (path: string) => fileURLToPath(new URL(path, import.meta.url));

// Deliberately *not* an extension of `vite.config.ts`: the SvelteKit plugin
// wants a synced `.svelte-kit/` and a dev server, neither of which the store
// tests need. The aliases below are all the app modules ask of the
// framework — `$lib` for imports, `$app/environment` for the browser flag and
// `$app/paths` for the asset prefix the theme store builds the favicon from.
export default defineConfig({
  resolve: {
    alias: {
      $lib: resolvePath('./src/lib'),
      '$app/environment': resolvePath('./test/stubs/app-environment.ts'),
      '$app/paths': resolvePath('./test/stubs/app-paths.ts')
    }
  },
  test: {
    environment: 'node',
    include: ['src/**/*.test.ts', 'test/**/*.test.ts'],
    setupFiles: ['./test/setup.ts']
  }
});
