# syntax=docker/dockerfile:1.6
# The Rust version is spelled out in the tag, not passed as an ARG: Dependabot
# cannot update an image named through a variable. It must match the channel
# in the repo-root rust-toolchain.toml, which CI checks.
FROM docker.io/library/rust:1.99.0-slim-trixie@sha256:0952c7a429d2f53c7f1f5e0796190690e7d8db367ec4a66ce0419763b9e5e0d0 AS builder
WORKDIR /app

# Cache dependencies
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs \
    && cargo build --release --locked \
    && rm -rf src

# Build the actual binary
COPY src ./src
RUN cargo build --release --locked

FROM docker.io/library/debian:trixie-slim@sha256:a99cfc517144bc59b1978475ec53b46ecabec7e43635402ee5b77cc54cd1b20a
LABEL org.opencontainers.image.source="https://github.com/murmer/murmer"
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

USER murmer
EXPOSE 3001
ENV RUST_LOG=murmer_server=info

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 CMD curl -fsS http://127.0.0.1:3001/ || exit 1

CMD ["murmer_server"]
