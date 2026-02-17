# syntax=docker/dockerfile:1.7

FROM rust:1.90-bookworm AS builder
WORKDIR /workspace

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

RUN cargo build --release -p tau-coding-agent

FROM debian:bookworm-slim

RUN apt-get update \
  && apt-get install --yes --no-install-recommends ca-certificates \
  && rm -rf /var/lib/apt/lists/*

RUN useradd --create-home --home-dir /home/tau --uid 10001 --shell /usr/sbin/nologin tau

COPY --from=builder /workspace/target/release/tau-coding-agent /usr/local/bin/tau-coding-agent

WORKDIR /home/tau
USER tau

ENTRYPOINT ["tau-coding-agent"]
CMD ["--help"]
