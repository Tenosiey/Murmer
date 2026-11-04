# syntax=docker/dockerfile:1.6
# Build from the repo root: docker build -f murmer_server/Dockerfile .
FROM docker.io/library/rust:1.99.1-slim-trixie@sha256:ea0999ff67355d958b8799b6751c879a6d770076d26047dca66f6e51e77930d3 AS builder
WORKDIR /app

# Build with the Rust pinned in rust-toolchain.toml, not whatever the image
# ships. The two used to have to match by hand, and every toolchain bump left
# CI red until someone noticed: Dependabot releases the toolchain and the
# image tag days apart. Now a lagging image only costs one download. Kept
# out of /app so its components (clippy, rustfmt, docs) are not installed.
COPY rust-toolchain.toml /tmp/
RUN rustup default "$(sed -n 's/^channel = "\(.*\)"$/\1/p' /tmp/rust-toolchain.toml)"

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
FROM docker.io/oven/bun:1.4.4-slim@sha256:1846adf22496bd711036aae8be147e57bdebbaea2e5264e3e0b5af1db187d9a4 AS web
WORKDIR /app
COPY murmer_client/package.json murmer_client/bun.lock ./
RUN bun install --frozen-lockfile
COPY murmer_client ./
RUN bun run build

FROM docker.io/library/debian:trixie-slim@sha256:a29215f6a35e51e22adffa17f89e9d2ef06214e64a2bad10d765c46aea49f11f
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
