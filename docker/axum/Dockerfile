FROM oven/bun:1.4.4-slim AS web-builder
WORKDIR /app/web
COPY web/package.json web/bun.lock ./
RUN bun install --frozen-lockfile
COPY web ./
RUN bun run build

FROM rust:1.98-bookworm AS builder
RUN apt-get update && apt-get install -y --no-install-recommends cmake make g++ pkg-config \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs && cargo build --release --locked && rm -rf src
COPY features.json installation_defaults.json ./
COPY migrations ./migrations
COPY src ./src
RUN touch src/main.rs && cargo build --release --locked

FROM debian:bookworm-slim
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 1001 circleit && useradd --uid 1001 --gid circleit --create-home circleit \
    && install -d -o circleit -g circleit -m 750 /srv/pulsar/storage
WORKDIR /srv/pulsar
COPY --from=builder /app/target/release/pulsar /usr/local/bin/pulsar
COPY --from=web-builder --chown=circleit:circleit /app/web/build/client /srv/pulsar/web/build/client
COPY --chown=circleit:circleit migrations /srv/pulsar/migrations
ENV BIND=0.0.0.0:3000 WEB_DIR=/srv/pulsar/web/build/client STORAGE_DIR=/srv/pulsar/storage HOME=/home/circleit
USER circleit
EXPOSE 3000
HEALTHCHECK --interval=15s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -fsS http://127.0.0.1:3000/health >/dev/null || exit 1
ENTRYPOINT ["/usr/local/bin/pulsar"]
