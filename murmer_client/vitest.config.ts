import { defineConfig } from 'vitest/config';
import { fileURLToPath } from 'node:url';

const resolvePath = (path: string) => fileURLToPath(new URL(path, import.meta.url));

// Deliberately *not* an extension of `vite.config.ts`: the SvelteKit plugin
// wants a synced `.svelte-kit/` and a dev server, neither of which the store
// tests need. The two aliases below are all the app modules ask of the
// framework — `$lib` for imports and `$app/environment` for the browser flag.
export default defineConfig({
  resolve: {
    alias: {
      $lib: resolvePath('./src/lib'),
      '$app/environment': resolvePath('./test/stubs/app-environment.ts')
    }
  },
  test: {
    environment: 'node',
    include: ['src/**/*.test.ts', 'test/**/*.test.ts'],
    setupFiles: ['./test/setup.ts']
  }
});
