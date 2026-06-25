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
RUN SQLX_OFFLINE=true cargo build --release -p ersatzrs-web -p ersatzrs-healthcheck

# ─── Stage 2: runtime ─────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

# Runtime library dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Copy the ersatzrs-web server binary
COPY --from=builder /workspace/target/release/ersatzrs-web /usr/local/bin/ersatzrs-web
COPY --from=builder /workspace/target/release/ersatzrs-healthcheck /usr/local/bin/ersatzrs-healthcheck

# Run as a dedicated non-root service account
RUN useradd -r -s /bin/false ersatzrs
RUN mkdir -p /data/.ersatzrs && chown -R ersatzrs /data

# Persistent data directory (SQLite database, managed FFmpeg runtime, etc.)
VOLUME ["/data"]
WORKDIR /data

# Default database location; override with -e DATABASE_URL=... at runtime
ENV HOME=/data
ENV DATABASE_URL=sqlite:///data/.ersatzrs/ErsatzRS.db

# HTTP port exposed by the axum server
EXPOSE 8409

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD [ "ersatzrs-healthcheck" ]

USER ersatzrs

ENTRYPOINT ["/usr/local/bin/ersatzrs-web"]
