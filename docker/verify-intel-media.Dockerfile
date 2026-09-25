# syntax=docker/dockerfile:1.7
# Build the production intel-media and runtime-base targets first.
ARG DRIVER_BUILD_IMAGE=ersatzrs-intel-build:test
ARG RUNTIME_IMAGE=ersatzrs-runtime:test
FROM ${DRIVER_BUILD_IMAGE} AS probe
COPY docker/verify-intel-loader.c /build/verify-intel-loader.c
RUN cc -Wall -Wextra -Werror /build/verify-intel-loader.c -ldl -o /build/verify-intel-loader

FROM ${RUNTIME_IMAGE} AS loader-check
COPY --from=probe /build/verify-intel-loader /verify-intel-loader
RUN /verify-intel-loader libva.so.2 libva-drm.so.2 libigdgmm.so.12 libvpl.so.2 libmfx-gen.so.1.2 \
    /usr/lib/x86_64-linux-gnu/dri/iHD_drv_video.so
RUN test -s /usr/local/share/licenses/intel-media/sources.json \
    && test ! -e /usr/bin/cc
RUN for component in libva gmmlib media-driver vpl vpl-gpu-rt; do \
        test -n "$(find /usr/local/share/licenses/intel-media/$component -type f -print -quit)" || exit 1; \
    done

# The alternate Dockerfile's Mesa Vulkan runtime already depends on Python.
# The root/release minimal runtime must not inherit Python from the builder.
FROM loader-check AS minimal-runtime-check
RUN test ! -e /usr/bin/python3
