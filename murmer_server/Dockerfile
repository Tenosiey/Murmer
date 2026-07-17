# syntax=docker/dockerfile:1.6
ARG RUST_VERSION=1.97.1
FROM docker.io/library/rust:${RUST_VERSION}-slim-trixie@sha256:754a8924e308fb20a327febeda1a07053a2b0fd7474b5ac1cc460a6d33ab18f3 AS builder
WORKDIR /app

# Cache dependencies
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs \
    && cargo build --release --locked \
    && rm -rf src

# Build the actual binary
COPY src ./src
RUN cargo build --release --locked

FROM docker.io/library/debian:trixie-slim@sha256:020c0d20b9880058cbe785a9db107156c3c75c2ac944a6aa7ab59f2add76a7bd
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
