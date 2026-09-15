# Built in CI (GitHub Actions) and pulled by dokku as an image; the host never
# compiles. Context: repo root.
FROM rust:1-bookworm AS builder
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
# Only this crate's binary; other workspace members are built on their own.
RUN cargo build --release -p custodian

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /build/target/release/custodian /app/custodian
RUN mkdir -p /data
ENV PAY_PORT=5000 DATABASE_PATH=/data/custodian.db
EXPOSE 5000
CMD ["/app/custodian"]
