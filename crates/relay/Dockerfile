# Built in CI (GitHub Actions) and pulled by dokku as an image; the host never
# compiles. Context: repo root.
# Toolchain and base images pinned by digest; .github/workflows/release-candidate.yml
# tests with the same rust version and fails if the two disagree.
FROM rust:1.93.1-bookworm@sha256:7c4ae649a84014c467d79319bbf17ce2632ae8b8be123ac2fb2ea5be46823f31 AS builder
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
# Only this crate's binary; other workspace members are built on their own.
RUN cargo build --release --locked -p relay

FROM debian:bookworm-slim@sha256:99c0b25c46d0214d5a2091092e7908907dcaecd6a962cde0ac700acee222b6f3
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /build/target/release/relay /app/relay
RUN mkdir -p /data
ENV RELAY_PORT=5000 DATABASE_PATH=/data/relay.db
EXPOSE 5000
CMD ["/app/relay"]
