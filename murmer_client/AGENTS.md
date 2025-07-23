# Murmer Client Guide

This directory contains the Tauri/SvelteKit desktop client.

## Setup
1. Run `npm install` to install dependencies.
2. Launch the app with `npm run tauri dev`.

The Tauri configuration in `src-tauri/tauri.conf.json` runs `npm run dev` during development.

## Validation
Run `npm run check` to perform Svelte and TypeScript checks. The Rust code in
`src-tauri` should compile successfully with `cargo check`. There are no
automated tests yet.
