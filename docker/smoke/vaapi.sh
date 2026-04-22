#!/usr/bin/env sh
set -eu

IMAGE="${IMAGE:-ghcr.io/bartdeijkers/ersatzrs:dev}"
DRI_DEVICE="${DRI_DEVICE:-/dev/dri}"
VAAPI_DEVICE="${VAAPI_DEVICE:-/dev/dri/renderD128}"

exec docker run --rm \
    --device="${DRI_DEVICE}:${DRI_DEVICE}" \
    --entrypoint ffmpeg \
    "$IMAGE" \
    -hide_banner \
    -vaapi_device "$VAAPI_DEVICE" \
    -f lavfi -i testsrc2=s=128x72:d=1 \
    -vf format=nv12,hwupload \
    -c:v h264_vaapi \
    -f null -
