# syntax=docker/dockerfile:1.7

# Build GPU userspace libraries independently from the Rust/WASM graph.
FROM debian:bookworm-slim AS intel-media
ARG TARGETARCH
COPY docker/build-intel-media.sh docker/intel-media-sources.json /build/
RUN bash /build/build-intel-media.sh "$TARGETARCH"
# Keep driver debugging symbols out of the shipped runtime layer.
RUN if [ "$TARGETARCH" = amd64 ]; then \
        find /out/usr/local/lib -type f -name '*.so*' -exec strip --strip-unneeded '{}' +; \
    fi

# Stage the optional Next runtime on the build host for the target platform.
# Keep Python and download tooling out of the runtime image.
FROM --platform=$BUILDPLATFORM python:3.12-slim-bookworm AS next-official
ARG TARGETARCH
ARG TARGETVARIANT
# Change per release to refresh the mutable upstream develop artifact.
ARG NEXT_RELEASE_REFRESH=dev
COPY scripts/release/stage-next.py /stage-next.py
RUN test -n "$NEXT_RELEASE_REFRESH" && python /stage-next.py --docker-arch "$TARGETARCH" \
    --docker-variant "$TARGETVARIANT" --output /next

# Upstream has no ARMv7 artifact. Build its pinned, unmodified source using
# the runtime's Bookworm baseline (ADR 0100); never use the host Docker socket.
FROM --platform=$BUILDPLATFORM rust:1.97.1-slim-bookworm@sha256:2775a09d208ff0d7c1f50490c45b62db929e87ba1dcbc3f2132ac71a704bcdd3 AS next-armv7-builder
RUN apt-get update && apt-get install -y --no-install-recommends \
    gcc-arm-linux-gnueabihf libc6-dev-armhf-cross cmake make perl python3 \
    && rm -rf /var/lib/apt/lists/*
RUN rustup target add armv7-unknown-linux-gnueabihf
ENV CARGO_BUILD_JOBS=4
COPY scripts/release/stage-next.py scripts/release/build-next-armv7.py /build-tools/
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/build/next-source/target \
    python3 /build-tools/build-next-armv7.py --work-dir /build --output /next

FROM next-official AS next-artifacts-amd64
FROM next-official AS next-artifacts-arm64
FROM next-armv7-builder AS next-artifacts-arm
ARG TARGETVARIANT
RUN test "$TARGETVARIANT" = v7
FROM next-artifacts-${TARGETARCH} AS next-artifacts

FROM scratch AS next-artifacts-export
COPY --from=next-artifacts /next/ /

# ─── Stage 1: builder ────────────────────────────────────────────────────────
# Match the exact Rust toolchain required by the workspace, CI, and releases.
FROM rust:1.97.1-slim-bookworm AS builder

# Build-time dependencies for ring / openssl crates used by axum/sqlx
RUN apt-get update && apt-get install -y --no-install-recommends \
    cmake \
    make \
    perl \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

RUN rustup target add wasm32-unknown-unknown

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    set -eux; \
    cargo install stylance-cli --version 0.8.2 --locked; \
    cargo install cargo-leptos --version 0.3.6 --locked

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
    set -eux; \
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
    set -eux; \
    SQLX_OFFLINE=true CARGO_TARGET_DIR=/workspace/target-cache \
        cargo build --release -p ersatzrs-web -p ersatzrs-healthcheck; \
    mkdir -p /workspace/target/release; \
    cp /workspace/target-cache/release/ersatzrs-web /workspace/target/release/ersatzrs-web; \
    cp /workspace/target-cache/release/ersatzrs-healthcheck /workspace/target/release/ersatzrs-healthcheck
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    set -eux; \
    rustup target add wasm32-unknown-unknown; \
    cd crates/ersatzrs-web; \
    SQLX_OFFLINE=true cargo leptos build --release --frontend-only --lib-features hydrate; \
    cd /workspace; \
    test -f crates/ersatzrs-web/public/bundle.css; \
    test -d target/site/pkg; \
    test -f target/site/bundle.css

# ─── Stage 2: runtime ─────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime-base
ARG TARGETARCH

# The NVIDIA runtime injects Vulkan driver components only with graphics enabled.
ENV NVIDIA_DRIVER_CAPABILITIES=compute,video,graphics,utility \
    NVIDIA_VISIBLE_DEVICES=all
COPY docker/install-vulkan-runtime.sh /install-vulkan-runtime.sh

# Runtime library dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl \
    libssl3 libdrm2 libpciaccess0 libstdc++6 libvulkan1 \
    && bash /install-vulkan-runtime.sh "$TARGETARCH" \
    && rm /install-vulkan-runtime.sh \
    && rm -rf /var/lib/apt/lists/*

COPY --from=intel-media /out/ /
RUN ldconfig

FROM runtime-base AS runtime
# Copy the ersatzrs-web server binary
COPY --from=builder /workspace/target/release/ersatzrs-web /usr/local/bin/ersatzrs-web
COPY --from=builder /workspace/target/release/ersatzrs-healthcheck /usr/local/bin/ersatzrs-healthcheck
COPY --from=next-artifacts /next/ersatztv-channel /usr/local/bin/ersatztv-channel
COPY --from=next-artifacts /next/LICENSE /next/origin.json /usr/local/bin/next/
# cargo-leptos already copies the assets-dir (public/) contents into the top
# level of target/site alongside pkg/, so target/site is the single merged
# admin-UI directory; ship only that, not a separate public/ copy.
COPY --from=builder /workspace/target/site /usr/local/bin/site
COPY --from=builder /workspace/assets /usr/local/bin/assets

# Run as a dedicated non-root service account
RUN useradd -r -s /bin/false ersatzrs
RUN mkdir -p /data/.ersatzrs && chown -R ersatzrs /data

# Persistent data directory (SQLite database, managed FFmpeg runtime, etc.)
VOLUME ["/data"]
WORKDIR /data

# Default database location; override with -e DATABASE_URL=... at runtime
ENV HOME=/data
ENV DATABASE_URL=sqlite:///data/.ersatzrs/ErsatzRS.db
ENV LEPTOS_SITE_ROOT=/usr/local/bin/site
ENV LEPTOS_SITE_PKG_DIR=pkg
ENV LEPTOS_OUTPUT_NAME=ersatzrs-web

# HTTP port exposed by the axum server
EXPOSE 8409

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD [ "ersatzrs-healthcheck" ]

USER ersatzrs

ENTRYPOINT ["/usr/local/bin/ersatzrs-web"]
