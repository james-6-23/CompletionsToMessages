# ── Stage 1: 构建前端 ──
FROM node:22-alpine AS frontend
WORKDIR /app/web
COPY web/package.json web/package-lock.json ./
RUN npm ci
COPY web/ ./
RUN npm run build

# ── Stage 2: 构建 Rust 后端 ──
FROM rust:1.85-bookworm AS backend
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
RUN cargo build --release

# ── Stage 3: 运行时镜像 ──
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=backend /app/target/release/completions-to-messages /app/completions-to-messages
COPY --from=frontend /app/web/dist /app/web/dist

RUN mkdir -p /app/data

ENV CC_PROXY_LISTEN=0.0.0.0:8080
ENV RUST_LOG=info

EXPOSE 8080
VOLUME ["/app/data"]

ENTRYPOINT ["/app/completions-to-messages"]
