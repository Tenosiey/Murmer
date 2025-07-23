# Murmer Client Guide

This directory contains the Tauri/SvelteKit desktop client for the Murmer chat application.

## Architecture Overview
- **Frontend**: SvelteKit 2.x with TypeScript for the web interface
- **Desktop Shell**: Tauri 2.x for native desktop integration
- **State Management**: Svelte stores for reactive state management
- **Communication**: WebSocket connection to the Rust server
- **Cryptography**: Ed25519 signatures for authentication (using tweetnacl)
- **UI**: Markdown support for rich text with DOMPurify sanitization

## Setup
1. Run `npm install` to install dependencies
2. Launch the app with `npm run tauri dev` for development
3. Build for production with `npm run tauri build`

The Tauri configuration in `src-tauri/tauri.conf.json` runs `npm run dev` during development.

## Security Notes
- Private keys are stored in localStorage (consider more secure storage for production)
- WebSocket messages should be validated before processing
- Markdown rendering uses DOMPurify but should be regularly updated
- Content Security Policy should be configured in production

## File Structure
- `src/routes/` - SvelteKit pages (login, chat, servers)
- `src/lib/components/` - Reusable Svelte components
- `src/lib/stores/` - Reactive state management
- `src/lib/voice/` - WebRTC voice chat implementation
- `src-tauri/` - Rust code for desktop integration

## Development Guidelines
- Use TypeScript for all new code
- Validate all user inputs and server responses
- Follow Svelte best practices for reactive programming
- Test WebSocket reconnection and error scenarios

## Validation
- Run `npm run check` to perform Svelte and TypeScript checks
- The Rust code in `src-tauri` should compile successfully with `cargo check`
- Test the built application on the target platforms
- Verify WebSocket connectivity and authentication flows
- There are no automated tests yet - this should be added for production use
