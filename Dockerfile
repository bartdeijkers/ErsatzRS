# ─── Stage 1: builder ────────────────────────────────────────────────────────
# Pin to the same Rust version declared in rust-toolchain.toml (stable / 1.85).
FROM rust:1.85-slim-bookworm AS builder

# Build-time dependencies for ring / openssl crates used by axum/sqlx
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /workspace

# Copy the full workspace
COPY . .

# SQLX_OFFLINE=true skips the live database check during compilation.
# A checked-in .sqlx/ directory with query metadata must exist; it is
# produced by running `cargo sqlx prepare --workspace` locally.
RUN SQLX_OFFLINE=true cargo build --release -p nastv-web

# ─── Stage 2: FFmpeg static binary source ────────────────────────────────────
# mwader/static-ffmpeg ships a statically-linked ffmpeg/ffprobe built from
# the FFmpeg master branch. The "latest" tag tracks the most recent release.
#
# NOTE: nastv-web checks the detected FFmpeg version at startup and refuses
# to start if it is older than 8.1 (required for the `drawvg` filter).
# Verify the exact version shipped by `mwader/static-ffmpeg:latest` before
# deploying to production. If a specific 8.x tag is available (e.g. :8.1)
# prefer that over :latest for reproducibility.
FROM mwader/static-ffmpeg:latest AS ffmpeg-src

# ─── Stage 3: runtime ─────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

# Runtime library dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    wget \
    && rm -rf /var/lib/apt/lists/*

# Copy static FFmpeg binaries (no shared-library dependencies needed)
COPY --from=ffmpeg-src /ffmpeg  /usr/local/bin/ffmpeg
COPY --from=ffmpeg-src /ffprobe /usr/local/bin/ffprobe

# Copy the nastv-web server binary
COPY --from=builder /workspace/target/release/nastv-web /usr/local/bin/nastv-web

# Run as a dedicated non-root service account
RUN useradd -r -s /bin/false nastv

# Persistent data directory (SQLite database, channel config, etc.)
VOLUME ["/data"]
WORKDIR /data

# Default database location; override with -e DATABASE_URL=... at runtime
ENV DATABASE_URL=sqlite:///data/nastv.db

# HTTP port exposed by the axum server
EXPOSE 8080

# Liveness probe using the /healthz endpoint provided by nastv-web
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD wget -qO- http://localhost:8080/healthz || exit 1

USER nastv

ENTRYPOINT ["/usr/local/bin/nastv-web"]
