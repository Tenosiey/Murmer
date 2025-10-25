# ADR 0001: Security Hardening and Observability

## Status
Accepted – 2025-02-14

## Context
The initial Murmer deployment lacked explicit tooling pinning, structured logging and hardened desktop/webview configuration. The
server enabled permissive CORS by default, accepted uploads without telemetry, and Docker images ran as root from unpinned base
layers. Client IPC capabilities were broader than required and CSP was disabled, creating attack surface.

## Decision
- Pin the Rust toolchain to 1.89.0 and require `rustfmt`/`clippy` in CI and local workflows.
- Load configuration via a dedicated `Config` structure, enforce graceful shutdown and structured logging using `tracing`.
- Restrict CORS through the new `CORS_ALLOW_ORIGINS` variable and add security headers to every HTTP response.
- Harden file uploads with additional instrumentation and body limits exposed through Axum layers.
- Ship Docker images from pinned digests, drop privileges to an unprivileged user and add a healthcheck.
- Upgrade the Tauri shell to apply a CSP, restrict capabilities, add logging initialisation and return structured errors.
- Add rate limiter integration tests covering nonce expiry and abuse limits.

## Consequences
- Operators must set `CORS_ALLOW_ORIGINS` explicitly for development; production deployments remain locked down by default.
- CI and local flows run more commands, but failures surface before merge.
- Docker builds are deterministic and safer at the cost of refreshing digests when upgrading base images.
- Developers gain additional telemetry from both the server and Tauri shell, making it easier to investigate incidents.
