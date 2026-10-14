# syntax=docker/dockerfile:1.6
# Build from the repo root: docker build -f murmer_server/Dockerfile .
# The Rust version is spelled out in the tag, not passed as an ARG: Dependabot
# cannot update an image named through a variable. It must match the channel
# in the repo-root rust-toolchain.toml, which CI checks.
FROM docker.io/library/rust:1.99.0-slim-trixie@sha256:0952c7a429d2f53c7f1f5e0796190690e7d8db367ec4a66ce0419763b9e5e0d0 AS builder
WORKDIR /app

# Cache dependencies
COPY murmer_server/Cargo.toml murmer_server/Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs \
    && cargo build --release --locked \
    && rm -rf src

# Build the actual binary
COPY murmer_server/src ./src
RUN cargo build --release --locked

# The web client rides along in every image so an operator can turn it on
# with one setting (WEB_CLIENT_DIR=/app/web) instead of installing Bun and
# building it themselves. Unset, the server never reads the directory.
FROM docker.io/oven/bun:1.4.2-slim@sha256:cb3bbbb08e13a4a2ff400f24c7a2a1d5efa83f6ef8544d52d95a519631e2fc61 AS web
WORKDIR /app
COPY murmer_client/package.json murmer_client/bun.lock ./
RUN bun install --frozen-lockfile
COPY murmer_client ./
RUN bun run build

FROM docker.io/library/debian:trixie-slim@sha256:a99cfc517144bc59b1978475ec53b46ecabec7e43635402ee5b77cc54cd1b20a
LABEL org.opencontainers.image.source="https://github.com/Tenosiey/Murmer"
LABEL org.opencontainers.image.description="Murmer WebSocket server"
WORKDIR /app

# sqlite3 is included so operators can inspect the database from inside the
# container: docker compose exec server sqlite3 /app/data/murmer.db
RUN apt-get update \
    && apt-get install --no-install-recommends -y ca-certificates curl sqlite3 \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --system --home /app --shell /usr/sbin/nologin murmer
COPY --from=builder /app/target/release/murmer_server /usr/local/bin/murmer_server
RUN chown murmer:murmer /usr/local/bin/murmer_server \
    && mkdir -p /app/uploads /app/data \
    && chown -R murmer:murmer /app
# Root-owned, so a compromised server process cannot rewrite the JavaScript
# that holds its users' identity keys.
COPY --from=web /app/build /app/web

USER murmer
EXPOSE 3001
ENV RUST_LOG=murmer_server=info

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 CMD curl -fsS http://127.0.0.1:3001/health || exit 1

CMD ["murmer_server"]
