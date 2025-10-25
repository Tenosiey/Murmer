# Contributing to Murmer

Thank you for considering contributing to Murmer! To make your contributions as smooth as possible, please follow these guidelines.

## Development Environment

Refer to the root [README.md](README.md) and the per-module development guides:
- **murmer_client/AGENTS.md** – instructions for setting up and validating the Tauri/SvelteKit client.
- **murmer_server/AGENTS.md** – instructions for setting up and validating the Rust WebSocket server.

## Code Style & Quality

- **Rust**: run `cargo fmt --all --check` and `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- **Testing**: run `cargo test --workspace` and `npm run check`.
- **Security**: run `cargo audit` and `npm audit` to ensure dependencies are free of known vulnerabilities.
- Prefer structured logging using `tracing` (`info!`, `warn!`) instead of `println!`.
- Bubble errors using `Result`/`anyhow` instead of `panic!`.

## Pull Requests

- Keep changes focused and minimal.
- Provide a clear description of your changes and reference any related issues or feature requests.
- Update documentation (README, AGENTS.md, TODO.md) as needed.

## Reporting Issues

Use the issue tracker to report bugs or request features. Please include steps to reproduce, expected behavior, and any relevant log output.
