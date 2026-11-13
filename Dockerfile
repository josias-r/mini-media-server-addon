FROM node:22-bookworm-slim AS frontend-builder
WORKDIR /app/app
RUN corepack enable pnpm
COPY app/package.json app/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY app/ ./
RUN pnpm run build

FROM lukemathwalker/cargo-chef:0.1.77-rust-1.97.1-bookworm AS chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
WORKDIR /app
COPY --from=planner /app/recipe.json recipe.json
# Build dependencies - this is the caching Docker layer!
RUN cargo chef cook --release --recipe-path recipe.json
# Build application
COPY . .
RUN cargo build --release --bin mini-media-server-addon

# Runtime stage
FROM debian:bookworm-slim
WORKDIR /app

# Install ca-certificates for reqwest to make HTTPS requests and ffmpeg for stream remuxing
RUN apt-get update && apt-get install -y ca-certificates ffmpeg && rm -rf /var/lib/apt/lists/*

# Copy the compiled binary, static frontend assets, and placeholder video
COPY --from=builder /app/target/release/mini-media-server-addon /app/mini-media-server-addon
COPY --from=frontend-builder /app/app/dist /app/app/dist
COPY placeholder.mp4 /app/placeholder.mp4

# Environment variables
ENV PORT=7000
ENV DATA_DIR=/app/data
ENV STATIC_DIR=/app/app/dist

# Expose port
EXPOSE 7000

# Start server
CMD ["/app/mini-media-server-addon"]
