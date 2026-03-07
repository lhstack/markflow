# syntax=docker/dockerfile:1.7

FROM oven/bun:1.3.9-alpine AS frontend-builder
WORKDIR /app/frontend
COPY frontend/package.json frontend/bun.lock ./
RUN bun install --frozen-lockfile
COPY frontend ./
RUN bun run build

FROM rust:1.93.1-alpine AS backend-builder
WORKDIR /app/backend
RUN apk add --no-cache build-base musl-dev pkgconfig

COPY backend/.cargo ./.cargo
COPY backend/Cargo.toml backend/Cargo.lock ./
RUN mkdir -p src && \
    printf 'fn main() { println!("build cache warmup"); }\n' > src/main.rs && \
    cargo build --release --locked && \
    rm -rf src

COPY backend/src ./src
COPY --from=frontend-builder /app/frontend/dist /app/frontend/dist
RUN find src -type f -exec touch {} + && cargo build --release --locked

FROM alpine:3.21
WORKDIR /app
RUN apk add --no-cache ca-certificates tzdata su-exec tini && \
    addgroup -S markflow && \
    adduser -S -G markflow markflow && \
    mkdir -p /app/data /app/logs && \
    chown -R markflow:markflow /app

COPY --from=backend-builder /app/backend/target/release/markflow /app/markflow
COPY backend/config.toml /app/config.toml
COPY docker-entrypoint.sh /app/docker-entrypoint.sh
RUN chmod +x /app/docker-entrypoint.sh

EXPOSE 3000

ENV PORT=3000 \
    DATABASE_URL=sqlite:data/markflow.db \
    UPLOAD_DIR=uploads \
    LOG_DIR=logs

ENTRYPOINT ["/sbin/tini", "--", "/app/docker-entrypoint.sh"]
