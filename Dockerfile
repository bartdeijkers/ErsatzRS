# syntax=docker/dockerfile:1.7

# ─── Stage 1: builder ────────────────────────────────────────────────────────
# Pin to the same Rust version declared in rust-toolchain.toml (stable / 1.85).
FROM rust:1.85-slim-bookworm AS builder

# Build-time dependencies for ring / openssl crates used by axum/sqlx
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /workspace

# Prime a dependency layer from manifests and lockfiles before copying source.
# The temporary crate sources make Cargo compile third-party dependencies
# without letting ordinary source edits invalidate this layer.
COPY rust-toolchain.toml Cargo.toml Cargo.lock ./
COPY .cargo .cargo
COPY crates/ersatzrs-app/Cargo.toml crates/ersatzrs-app/Cargo.toml
COPY crates/ersatzrs-core/Cargo.toml crates/ersatzrs-core/Cargo.toml
COPY crates/ersatzrs-domain/Cargo.toml crates/ersatzrs-domain/Cargo.toml
COPY crates/ersatzrs-ffmpeg/Cargo.toml crates/ersatzrs-ffmpeg/Cargo.toml
COPY crates/ersatzrs-healthcheck/Cargo.toml crates/ersatzrs-healthcheck/Cargo.toml
COPY crates/ersatzrs-hwprobe/Cargo.toml crates/ersatzrs-hwprobe/Cargo.toml
COPY crates/ersatzrs-infra/Cargo.toml crates/ersatzrs-infra/Cargo.toml
COPY crates/ersatzrs-launcher/Cargo.toml crates/ersatzrs-launcher/Cargo.toml
COPY crates/ersatzrs-scanner/Cargo.toml crates/ersatzrs-scanner/Cargo.toml
COPY crates/ersatzrs-scheduler/Cargo.toml crates/ersatzrs-scheduler/Cargo.toml
COPY crates/ersatzrs-web/Cargo.toml crates/ersatzrs-web/Cargo.toml
COPY xtask/Cargo.toml xtask/Cargo.toml
RUN set -eux; \
    for crate in \
        crates/ersatzrs-app \
        crates/ersatzrs-core \
        crates/ersatzrs-domain \
        crates/ersatzrs-ffmpeg \
        crates/ersatzrs-hwprobe \
        crates/ersatzrs-infra \
        crates/ersatzrs-scheduler \
        xtask; \
    do \
        mkdir -p "$crate/src"; \
        printf '' > "$crate/src/lib.rs"; \
    done; \
    for crate in crates/ersatzrs-healthcheck crates/ersatzrs-launcher; do \
        mkdir -p "$crate/src"; \
        printf 'fn main() {}\n' > "$crate/src/main.rs"; \
    done; \
    mkdir -p crates/ersatzrs-scanner/src crates/ersatzrs-web/src; \
    printf '' > crates/ersatzrs-scanner/src/lib.rs; \
    printf 'fn main() {}\n' > crates/ersatzrs-scanner/src/main.rs; \
    printf '' > crates/ersatzrs-web/src/lib.rs; \
    printf 'fn main() {}\n' > crates/ersatzrs-web/src/main.rs
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/workspace/target-cache \
    SQLX_OFFLINE=true ERSATZRS_SKIP_STYLANCE=1 \
        CARGO_TARGET_DIR=/workspace/target-cache \
        cargo build --release -p ersatzrs-web -p ersatzrs-healthcheck; \
    rm -rf \
        /workspace/target-cache/release/.fingerprint/ersatzrs-* \
        /workspace/target-cache/release/build/ersatzrs-* \
        /workspace/target-cache/release/deps/ersatzrs* \
        /workspace/target-cache/release/deps/libersatzrs* \
        /workspace/target-cache/release/ersatzrs-web \
        /workspace/target-cache/release/ersatzrs-healthcheck

# Copy the full workspace after dependency compilation so source-only edits
# rebuild local crates without recompiling the whole dependency graph.
COPY . .

# SQLX_OFFLINE=true skips the live database check during compilation.
# A checked-in .sqlx/ directory with query metadata must exist; it is
# produced by running `cargo sqlx prepare --workspace` locally.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/workspace/target-cache \
    SQLX_OFFLINE=true CARGO_TARGET_DIR=/workspace/target-cache \
        cargo build --release -p ersatzrs-web -p ersatzrs-healthcheck; \
    mkdir -p /workspace/target/release; \
    cp /workspace/target-cache/release/ersatzrs-web /workspace/target/release/ersatzrs-web; \
    cp /workspace/target-cache/release/ersatzrs-healthcheck /workspace/target/release/ersatzrs-healthcheck

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
