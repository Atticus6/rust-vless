# syntax=docker/dockerfile:1

# 前端构建。
FROM oven/bun:1 AS web
WORKDIR /app/web
COPY web/package.json web/bun.lock ./
RUN bun install --frozen-lockfile
COPY web/ ./
RUN bun run build

# Rust 构建。DuckDB 使用 bundled 特性，需要 CMake 与 C++ 编译器。
FROM rust:1-bookworm AS builder
RUN apt-get update \
    && apt-get install -y --no-install-recommends cmake g++ pkg-config \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY Cargo.toml Cargo.lock version.txt ./
COPY crates ./crates
COPY proto ./proto
RUN cargo build --release -p rust-vless-master -p rust-vless-node

# 运行时公共层。
FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --shell /usr/sbin/nologin rustvless \
    && mkdir -p /data /app/web \
    && chown -R rustvless:rustvless /data /app
WORKDIR /app
ENV RUST_VLESS_DB=/data/master.db \
    RUST_VLESS_WEB_DIR=/app/web \
    RUST_VLESS_ACCESS_LOG_FILE=/data/access_logs.jsonl
VOLUME ["/data"]
EXPOSE 8080 50051

# 主节点镜像（含前端静态文件）。
FROM runtime AS master
COPY --from=builder /src/target/release/rust-vless-master /usr/local/bin/rust-vless-master
COPY --from=web /app/web/dist /app/web
USER rustvless
ENTRYPOINT ["rust-vless-master"]

# 子节点镜像。
FROM runtime AS node
COPY --from=builder /src/target/release/rust-vless-node /usr/local/bin/rust-vless-node
USER rustvless
ENTRYPOINT ["rust-vless-node"]
