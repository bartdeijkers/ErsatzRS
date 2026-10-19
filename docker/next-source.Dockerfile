# syntax=docker/dockerfile:1.7
# ADR 0116 reproduction only. Ordinary packaging stages accepted archives.
# Invoke Docker on the host; no Docker socket is mounted in this build.
FROM rust:1.97.1-slim-bookworm@sha256:2775a09d208ff0d7c1f50490c45b62db929e87ba1dcbc3f2132ac71a704bcdd3 AS build
RUN apt-get update && apt-get install -y --no-install-recommends \
    cmake make perl python3 git \
    gcc-arm-linux-gnueabihf libc6-dev-armhf-cross \
    gcc-aarch64-linux-gnu libc6-dev-arm64-cross \
    && rm -rf /var/lib/apt/lists/*
ARG NEXT_TARGET=x86_64-unknown-linux-gnu
RUN case "$NEXT_TARGET" in x86_64-unknown-linux-gnu|aarch64-unknown-linux-gnu|armv7-unknown-linux-gnueabihf) ;; *) exit 1 ;; esac \
    && rustup target add "$NEXT_TARGET"
COPY scripts/release/prepare-next-source.py /build-tools/
COPY scripts/release/next-patches/ /build-tools/next-patches/
# Named context contains the reviewed upstream source archive. Its checksum
# is verified before extraction by the same helper used for macOS candidates.
COPY --from=next-source source.tar.gz /inputs/source.tar.gz
ARG NEXT_REVIEW_MANIFEST=manifest.json
RUN python3 /build-tools/prepare-next-source.py \
    --source-archive /inputs/source.tar.gz --output /build/next-source \
    --manifest "/build-tools/next-patches/$NEXT_REVIEW_MANIFEST"
WORKDIR /build/next-source
ENV CARGO_BUILD_JOBS=4
ENV CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABIHF_LINKER=arm-linux-gnueabihf-gcc \
    CC_armv7_unknown_linux_gnueabihf=arm-linux-gnueabihf-gcc \
    AR_armv7_unknown_linux_gnueabihf=arm-linux-gnueabihf-ar \
    CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
    CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc \
    AR_aarch64_unknown_linux_gnu=aarch64-linux-gnu-ar
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/build/next-source/target \
    ETV_VERSION=$(python3 -c 'import json; print(json.load(open("ersatzrs-source.json"))["distribution_version"])') \
    cargo +1.97.1 build --locked --release --all-features --target "$NEXT_TARGET" --bin ersatztv-channel \
    && python3 -c 'import hashlib,json; assert hashlib.sha256(open("Cargo.lock","rb").read()).hexdigest() == json.load(open("ersatzrs-source.json"))["cargo_lock_sha256"]' \
    && mkdir /next \
    && cp "target/$NEXT_TARGET/release/ersatztv-channel" LICENSE ersatzrs-source.json /next/ \
    && case "$NEXT_TARGET" in \
         armv7-*) arm-linux-gnueabihf-strip /next/ersatztv-channel ;; \
         aarch64-*) aarch64-linux-gnu-strip /next/ersatztv-channel ;; \
         *) strip /next/ersatztv-channel ;; \
       esac \
    && python3 -c 'import re,subprocess; data=subprocess.check_output(["readelf","--version-info","/next/ersatztv-channel"],text=True); versions={tuple(map(int,v.split("."))) for v in re.findall(r"GLIBC_([0-9.]+)",data)}; assert versions and max(versions) <= (2,36), "Next exceeds GLIBC 2.36"' \
    && rustc +1.97.1 --version > /next/rustc-version.txt
FROM scratch AS export
COPY --from=build /next/ /
