# Murmer Client

This directory contains the desktop client built with Tauri and SvelteKit.

## Development

Install dependencies and launch the app:

```bash
npm install
npm run tauri dev
```

Running `npm run tauri dev` starts the SvelteKit dev server and opens the Tauri shell with hot reloading.

## Stores

Several Svelte stores persist client state:

- **servers** – the list of known Murmer server URLs. You can manage this list on the Servers page. Entries are saved to `localStorage`.
- **session** – holds the currently logged in user name. Clearing this store logs the user out.

Use `npm run check` to run TypeScript and Svelte checks. Run `npm audit` to verify that installed dependencies are free from
known vulnerabilities. The desktop shell logs to STDOUT; adjust verbosity via the `RUST_LOG` environment variable when launching
`npm run tauri dev`.
