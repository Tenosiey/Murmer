import { defineConfig } from "vite";
import { sveltekit } from "@sveltejs/kit/vite";
import adapter from "@sveltejs/adapter-static";
import { vitePreprocess } from "@sveltejs/vite-plugin-svelte";
import { fileURLToPath } from "node:url";
import { dirname } from "node:path";

const host = process.env.TAURI_DEV_HOST;
const rootDir = dirname(fileURLToPath(import.meta.url));

// https://vitejs.dev/config/
export default defineConfig(async () => ({
  plugins: [
    sveltekit({
      preprocess: vitePreprocess(),
      compilerOptions: {
        // The whole app uses runes syntax; fail the build on legacy syntax.
        runes: true,
      },
      // Tauri has no Node.js server to do SSR, so adapter-static prerenders
      // the app (SSG): https://v2.tauri.app/start/frontend/sveltekit/
      adapter: adapter({
        // Every route is prerendered for the Tauri shell, but the same output
        // is also served as a web client from a plain static host (see "Web
        // client" in README.md). `200.html` is the SPA fallback such a host
        // serves for a path it has no file for, which is what makes a deep
        // link like `/invite#...` resolve instead of 404ing.
        fallback: "200.html",
      }),
    }),
  ],

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
    fs: {
      allow: [rootDir],
    },
  },
}));
