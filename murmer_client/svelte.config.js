// Tauri doesn't have a Node.js server to do proper SSR
// so we will use adapter-static to prerender the app (SSG)
// See: https://v2.tauri.app/start/frontend/sveltekit/ for more info
import adapter from "@sveltejs/adapter-static";
import { vitePreprocess } from "@sveltejs/vite-plugin-svelte";

/** @type {import('@sveltejs/kit').Config} */
const config = {
  preprocess: vitePreprocess(),
  compilerOptions: {
    // The whole app uses runes syntax; fail the build on legacy syntax.
    runes: true,
  },
  kit: {
    adapter: adapter({
      // Every route is prerendered for the Tauri shell, but the same output is
      // also served as a web client from a plain static host (see "Web client"
      // in README.md). `200.html` is the SPA fallback such a host serves for a
      // path it has no file for, which is what makes a deep link like
      // `/invite#...` resolve instead of 404ing.
      fallback: "200.html",
    }),
  },
};

export default config;
