# syntax=docker/dockerfile:1.7

ARG DEBIAN_VERSION=bookworm
ARG RUST_VERSION=1.97.1
ARG CARGO_LEPTOS_VERSION=0.3.6
ARG STYLANCE_CLI_VERSION=0.8.2
ARG WASM_BINDGEN_CLI_VERSION=0.2.122

# Stage the optional Next runtime on the build host for the target platform.
# Keep Python and download tooling out of the runtime image.
FROM --platform=$BUILDPLATFORM python:3.12-slim-bookworm AS next-artifacts
ARG TARGETARCH
ARG TARGETVARIANT
# Change per release to refresh the mutable upstream develop artifact.
ARG NEXT_RELEASE_REFRESH=dev
COPY scripts/release/stage-next.py /stage-next.py
RUN test -n "$NEXT_RELEASE_REFRESH" && python /stage-next.py --docker-arch "$TARGETARCH" \
    --docker-variant "$TARGETVARIANT" --output /next

FROM --platform=$BUILDPLATFORM rust:${RUST_VERSION}-${DEBIAN_VERSION} AS web-build
ARG CARGO_LEPTOS_VERSION
ARG STYLANCE_CLI_VERSION
ARG WASM_BINDGEN_CLI_VERSION
ENV DEBIAN_FRONTEND=noninteractive \
    CARGO_BUILD_JOBS=1 \
    CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 \
    CARGO_PROFILE_RELEASE_LTO=false \
    LEPTOS_HASH_FILES=false
WORKDIR /build

RUN set -eux; \
    apt-get update; \
    apt-get install -y --no-install-recommends \
        ca-certificates \
        clang \
        curl \
        libcairo2-dev \
        libssl-dev \
        pkg-config; \
    rm -rf /var/lib/apt/lists/*; \
    rustup target add wasm32-unknown-unknown; \
    cargo install cargo-leptos --version "${CARGO_LEPTOS_VERSION}" --locked; \
    cargo install wasm-bindgen-cli --version "${WASM_BINDGEN_CLI_VERSION}" --locked; \
    cargo install stylance-cli --version "${STYLANCE_CLI_VERSION}" --locked

COPY . .
RUN set -eux; \
    cd crates/ersatzrs-web; \
    cargo leptos build --release --frontend-only --lib-features hydrate; \
    mkdir -p /build/target/site; \
    for site_root in \
        /build/crates/ersatzrs-web/target/site \
        /build/target/site \
        target/site; \
    do \
        if [ -d "$site_root/pkg" ]; then \
            if [ "$site_root" != /build/target/site ]; then \
                cp -a "$site_root"/. /build/target/site/; \
            fi; \
            break; \
        fi; \
    done; \
    test -d /build/target/site/pkg || { \
        find /build -maxdepth 5 -type d \( -name site -o -name pkg \); \
        exit 1; \
    }; \
    test -f /build/crates/ersatzrs-web/public/bundle.css; \
    test -f /build/target/site/bundle.css

FROM rust:${RUST_VERSION}-${DEBIAN_VERSION} AS rust-build
ENV DEBIAN_FRONTEND=noninteractive \
    CARGO_BUILD_JOBS=1 \
    CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 \
    CARGO_PROFILE_RELEASE_LTO=false \
    ERSATZRS_SKIP_STYLANCE=1
WORKDIR /build

RUN set -eux; \
    apt-get update; \
    apt-get install -y --no-install-recommends \
        ca-certificates \
        clang \
        curl \
        libcairo2-dev \
        libssl-dev \
        mold \
        pkg-config; \
    rm -rf /var/lib/apt/lists/*

COPY . .
RUN set -eux; \
    cargo build --release --features mysql --bin ersatzrs-web --bin ersatzrs-scanner

FROM debian:${DEBIAN_VERSION}-slim AS runtime-base
ARG TARGETARCH
ARG TARGETVARIANT
ENV DEBIAN_FRONTEND=noninteractive \
    LEPTOS_OUTPUT_NAME=ersatzrs-web \
    LEPTOS_SITE_ROOT=/build/target/site \
    LEPTOS_SITE_PKG_DIR=pkg \
    LEPTOS_SITE_ADDR=0.0.0.0:8409 \
    LEPTOS_ENV=PROD \
    HOME=/data \
    DATABASE_URL=sqlite:///data/.ersatzrs/ErsatzRS.db \
    LIBVA_MESSAGING_LEVEL=0 \
    LIBVA_DISPLAY=drm \
    NVIDIA_DRIVER_CAPABILITIES=compute,video,utility \
    NVIDIA_VISIBLE_DEVICES=all

RUN set -eux; \
    apt-get update; \
    apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
        fontconfig \
        fonts-noto-core \
        libcairo2 \
        libaom3 \
        libass9 \
        libdav1d6 \
        libdrm2 \
        libfontconfig1 \
        libfreetype6 \
        libfribidi0 \
        libharfbuzz0b \
        libmp3lame0 \
        libopus0 \
        libssl3 \
        libsvtav1enc1 \
        libv4l-0 \
        libva-drm2 \
        libva-x11-2 \
        libva2 \
        libvorbis0a \
        libvorbisenc2 \
        libvpx7 \
        libvulkan1 \
        libwebp7 \
        libwebpmux3 \
        libx264-164 \
        libx265-199 \
        sqlite3 \
        tzdata; \
    if [ "$TARGETARCH" = "amd64" ]; then \
        apt-get install -y --no-install-recommends libvpl2 mesa-va-drivers mesa-vulkan-drivers va-driver-all; \
    elif [ "$TARGETARCH" = "arm64" ]; then \
        apt-get install -y --no-install-recommends mesa-va-drivers mesa-vulkan-drivers va-driver-all; \
    fi; \
    mkdir -p /data/.ersatzrs; \
    rm -rf /var/lib/apt/lists/*

FROM runtime-base AS runtime-release
COPY --from=rust-build /build/target/release/ersatzrs-web /usr/local/bin/ersatzrs-web
COPY --from=rust-build /build/target/release/ersatzrs-scanner /usr/local/bin/ersatzrs-scanner
COPY --from=next-artifacts /next /usr/local/bin/next
# release-web-assets/site is already the single merged admin-UI directory
# (pkg/ plus the assets-dir/public contents cargo-leptos copies alongside
# it); no separate public/ copy is needed.
COPY release-web-assets/site /build/target/site
COPY --from=rust-build /build/assets /app/assets
COPY --from=rust-build /build/migrations /app/migrations

WORKDIR /app
VOLUME ["/data"]
EXPOSE 8409
HEALTHCHECK --interval=30s --timeout=5s --start-period=30s --retries=3 \
    CMD [ "curl", "-fsS", "http://127.0.0.1:8409/healthz" ]
ENTRYPOINT [ "ersatzrs-web" ]

FROM runtime-base AS runtime
COPY --from=rust-build /build/target/release/ersatzrs-web /usr/local/bin/ersatzrs-web
COPY --from=rust-build /build/target/release/ersatzrs-scanner /usr/local/bin/ersatzrs-scanner
COPY --from=next-artifacts /next /usr/local/bin/next
# /build/target/site is already the single merged admin-UI directory (pkg/
# plus the assets-dir/public contents cargo-leptos copies alongside it); no
# separate public/ copy is needed.
COPY --from=web-build /build/target/site /build/target/site
COPY --from=rust-build /build/assets /app/assets
COPY --from=rust-build /build/migrations /app/migrations

WORKDIR /app
VOLUME ["/data"]
EXPOSE 8409
HEALTHCHECK --interval=30s --timeout=5s --start-period=30s --retries=3 \
    CMD [ "curl", "-fsS", "http://127.0.0.1:8409/healthz" ]
ENTRYPOINT [ "ersatzrs-web" ]
