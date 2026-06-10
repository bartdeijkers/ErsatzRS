# syntax=docker/dockerfile:1.7

ARG DEBIAN_VERSION=bookworm
ARG RUST_VERSION=1.90
ARG FFMPEG_VERSION=8.1
ARG FFMPEG_SHA256=b072aed6871998cce9b36e7774033105ca29e33632be5b6347f3206898e0756a
ARG NVCODEC_VERSION=n13.0.19.0
ARG VULKAN_HEADERS_VERSION=v1.3.299
ARG CARGO_LEPTOS_VERSION=0.3.6
ARG STYLANCE_CLI_VERSION=0.8.2
ARG WASM_BINDGEN_CLI_VERSION=0.2.122

FROM debian:${DEBIAN_VERSION}-slim AS ffmpeg-build
ARG TARGETARCH
ARG TARGETVARIANT
ARG FFMPEG_VERSION
ARG FFMPEG_SHA256
ARG NVCODEC_VERSION
ARG VULKAN_HEADERS_VERSION
ENV DEBIAN_FRONTEND=noninteractive

RUN set -eux; \
    apt-get update; \
    apt-get install -y --no-install-recommends \
        build-essential \
        ca-certificates \
        cmake \
        curl \
        git \
        glslang-dev \
        libcairo2-dev \
        libaom-dev \
        libass-dev \
        libdav1d-dev \
        libdrm-dev \
        libfontconfig-dev \
        libfreetype6-dev \
        libfribidi-dev \
        libharfbuzz-dev \
        libmp3lame-dev \
        libopus-dev \
        libssl-dev \
        libsvtav1-dev \
        libsvtav1enc1 \
        libv4l-dev \
        libva-dev \
        libvorbis-dev \
        libvpx-dev \
        libvulkan-dev \
        libwebp-dev \
        libx264-dev \
        libx265-dev \
        libzimg-dev \
        make \
        nasm \
        perl \
        pkg-config \
        python3 \
        xz-utils \
        yasm \
        zlib1g-dev; \
    if [ "$TARGETARCH" = "amd64" ]; then \
        apt-get install -y --no-install-recommends libvpl-dev; \
    elif [ "$TARGETARCH" = "arm64" ]; then \
        true; \
    fi; \
    rm -rf /var/lib/apt/lists/*

# Debian bookworm's libsvtav1-dev package ships headers, while the encoder
# runtime package ships only versioned shared objects and no pkg-config file.
# FFmpeg 8.1's libsvtav1 probe expects the conventional linker symlink and
# SvtAv1Enc.pc metadata.
RUN set -eux; \
    multiarch="$(gcc -print-multiarch)"; \
    svt_version="$(dpkg-query -W -f='${Version}' libsvtav1enc1 | sed 's/[+-].*//')"; \
    ln -sf libSvtAv1Enc.so.1 "/usr/lib/${multiarch}/libSvtAv1Enc.so"; \
    mkdir -p "/usr/lib/${multiarch}/pkgconfig"; \
    printf '%s\n' \
        'prefix=/usr' \
        "libdir=\${prefix}/lib/${multiarch}" \
        'includedir=${prefix}/include/svt-av1' \
        '' \
        'Name: SvtAv1Enc' \
        'Description: SVT-AV1 encoder' \
        "Version: ${svt_version}" \
        'Libs: -L${libdir} -lSvtAv1Enc' \
        'Cflags: -I${includedir}' \
        > "/usr/lib/${multiarch}/pkgconfig/SvtAv1Enc.pc"; \
    pkg-config --modversion SvtAv1Enc

RUN set -eux; \
    if [ "$TARGETARCH" != "arm" ]; then \
        git clone --branch "${NVCODEC_VERSION}" --depth 1 \
            https://github.com/FFmpeg/nv-codec-headers.git /tmp/nv-codec-headers; \
        make -C /tmp/nv-codec-headers install; \
        rm -rf /tmp/nv-codec-headers; \
        git clone --branch "${VULKAN_HEADERS_VERSION}" --depth 1 \
            https://github.com/KhronosGroup/Vulkan-Headers.git /tmp/vulkan-headers; \
        cmake -S /tmp/vulkan-headers -B /tmp/vulkan-headers/build \
            -DCMAKE_BUILD_TYPE=Release \
            -DCMAKE_INSTALL_PREFIX=/usr/local; \
        cmake --install /tmp/vulkan-headers/build; \
        rm -rf /tmp/vulkan-headers; \
    fi

RUN set -eux; \
    curl -fsSL "https://ffmpeg.org/releases/ffmpeg-${FFMPEG_VERSION}.tar.xz" \
        -o /tmp/ffmpeg.tar.xz; \
    echo "${FFMPEG_SHA256}  /tmp/ffmpeg.tar.xz" | sha256sum -c -; \
    mkdir -p /tmp/ffmpeg; \
    tar -xJf /tmp/ffmpeg.tar.xz -C /tmp/ffmpeg --strip-components=1; \
    rm /tmp/ffmpeg.tar.xz

RUN set -eux; \
    cd /tmp/ffmpeg; \
    extra_flags=""; \
    if [ "$TARGETARCH" != "arm" ]; then \
        extra_flags="$extra_flags --enable-ffnvcodec --enable-nvdec --enable-nvenc"; \
    fi; \
    if [ "$TARGETARCH" = "amd64" ]; then \
        extra_flags="$extra_flags --enable-libglslang --enable-libvpl --enable-vaapi --enable-vulkan"; \
    elif [ "$TARGETARCH" = "arm64" ]; then \
        extra_flags="$extra_flags --enable-libglslang --enable-vaapi --enable-vulkan"; \
    fi; \
    ./configure \
        --prefix=/opt/ffmpeg \
        --disable-debug \
        --disable-doc \
        --disable-ffplay \
        --enable-cairo \
        --enable-ffprobe \
        --enable-libfontconfig \
        --enable-gpl \
        --enable-libaom \
        --enable-libass \
        --enable-libdav1d \
        --enable-libdrm \
        --enable-libfreetype \
        --enable-libfribidi \
        --enable-libharfbuzz \
        --enable-libmp3lame \
        --enable-libopus \
        --enable-libsvtav1 \
        --enable-libv4l2 \
        --enable-libvorbis \
        --enable-libvpx \
        --enable-libwebp \
        --enable-libx264 \
        --enable-libx265 \
        --enable-libzimg \
        --enable-openssl \
        --enable-pic \
        --enable-shared \
        --enable-version3 \
        $extra_flags; \
    make -j"$(nproc)"; \
    make install; \
    export LD_LIBRARY_PATH=/opt/ffmpeg/lib; \
    /opt/ffmpeg/bin/ffmpeg -version; \
    /opt/ffmpeg/bin/ffprobe -version; \
    /opt/ffmpeg/bin/ffmpeg -hide_banner -filters | grep -w drawvg; \
    /opt/ffmpeg/bin/ffmpeg -hide_banner \
        -f lavfi -i color=c=black:s=32x32:d=0.1 \
        -vf "drawvg=script='rect 4 4 16 16 fill',format=yuv420p" \
        -frames:v 1 -f null -; \
    if [ "$TARGETARCH" = "amd64" ]; then \
        /opt/ffmpeg/bin/ffmpeg -hide_banner -hwaccels | grep -w cuda; \
        /opt/ffmpeg/bin/ffmpeg -hide_banner -hwaccels | grep -w vaapi; \
        /opt/ffmpeg/bin/ffmpeg -hide_banner -hwaccels | grep -w vulkan; \
        /opt/ffmpeg/bin/ffmpeg -hide_banner -hwaccels | grep -w qsv; \
        /opt/ffmpeg/bin/ffmpeg -hide_banner -encoders | grep -w h264_nvenc; \
        /opt/ffmpeg/bin/ffmpeg -hide_banner -encoders | grep -w hevc_nvenc; \
        /opt/ffmpeg/bin/ffmpeg -hide_banner -encoders | grep -w h264_vaapi; \
        /opt/ffmpeg/bin/ffmpeg -hide_banner -encoders | grep -w hevc_vaapi; \
        /opt/ffmpeg/bin/ffmpeg -hide_banner -encoders | grep -w h264_qsv; \
        /opt/ffmpeg/bin/ffmpeg -hide_banner -encoders | grep -w hevc_qsv; \
        /opt/ffmpeg/bin/ffmpeg -hide_banner -decoders | grep -w h264_cuvid; \
        /opt/ffmpeg/bin/ffmpeg -hide_banner -decoders | grep -w hevc_cuvid; \
    elif [ "$TARGETARCH" = "arm64" ]; then \
        /opt/ffmpeg/bin/ffmpeg -hide_banner -hwaccels | grep -w vaapi; \
        /opt/ffmpeg/bin/ffmpeg -hide_banner -hwaccels | grep -w vulkan; \
        /opt/ffmpeg/bin/ffmpeg -hide_banner -encoders | grep -w h264_vaapi; \
        /opt/ffmpeg/bin/ffmpeg -hide_banner -encoders | grep -w hevc_vaapi; \
    fi

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
    test -f /build/crates/ersatzrs-web/public/bundle.css

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
    LD_LIBRARY_PATH=/opt/ffmpeg/lib \
    PATH=/opt/ffmpeg/bin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
    LEPTOS_OUTPUT_NAME=ersatzrs-web \
    LEPTOS_SITE_ROOT=/build/target/site \
    LEPTOS_SITE_PKG_DIR=pkg \
    LEPTOS_SITE_ADDR=0.0.0.0:8409 \
    LEPTOS_ENV=PROD \
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
        libzimg2 \
        sqlite3 \
        tzdata; \
    if [ "$TARGETARCH" = "amd64" ]; then \
        apt-get install -y --no-install-recommends libvpl2 mesa-va-drivers mesa-vulkan-drivers va-driver-all; \
    elif [ "$TARGETARCH" = "arm64" ]; then \
        apt-get install -y --no-install-recommends mesa-va-drivers mesa-vulkan-drivers va-driver-all; \
    fi; \
    rm -rf /var/lib/apt/lists/*

COPY --from=ffmpeg-build /opt/ffmpeg /opt/ffmpeg

RUN set -eux; \
    ldd /opt/ffmpeg/bin/ffmpeg | tee /tmp/ffmpeg.ldd; \
    ! grep -q "not found" /tmp/ffmpeg.ldd; \
    ldd /opt/ffmpeg/bin/ffprobe | tee /tmp/ffprobe.ldd; \
    ! grep -q "not found" /tmp/ffprobe.ldd; \
    ffmpeg -version; \
    ffprobe -version; \
    ffmpeg -hide_banner -filters | grep -w drawvg

FROM runtime-base AS runtime-release
COPY --from=rust-build /build/target/release/ersatzrs-web /usr/local/bin/ersatzrs-web
COPY --from=rust-build /build/target/release/ersatzrs-scanner /usr/local/bin/ersatzrs-scanner
COPY release-web-assets/public /build/crates/ersatzrs-web/public
COPY release-web-assets/site /build/target/site
COPY --from=rust-build /build/assets /app/assets
COPY --from=rust-build /build/migrations /app/migrations

WORKDIR /app
EXPOSE 8409
HEALTHCHECK --interval=30s --timeout=5s --start-period=30s --retries=3 \
    CMD [ "curl", "-fsS", "http://127.0.0.1:8409/healthz" ]
ENTRYPOINT [ "ersatzrs-web" ]

FROM runtime-base AS runtime
COPY --from=rust-build /build/target/release/ersatzrs-web /usr/local/bin/ersatzrs-web
COPY --from=rust-build /build/target/release/ersatzrs-scanner /usr/local/bin/ersatzrs-scanner
COPY --from=web-build /build/crates/ersatzrs-web/public /build/crates/ersatzrs-web/public
COPY --from=web-build /build/target/site /build/target/site
COPY --from=rust-build /build/assets /app/assets
COPY --from=rust-build /build/migrations /app/migrations

WORKDIR /app
EXPOSE 8409
HEALTHCHECK --interval=30s --timeout=5s --start-period=30s --retries=3 \
    CMD [ "curl", "-fsS", "http://127.0.0.1:8409/healthz" ]
ENTRYPOINT [ "ersatzrs-web" ]
