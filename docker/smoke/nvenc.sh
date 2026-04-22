#!/usr/bin/env sh
set -eu

IMAGE="${IMAGE:-ghcr.io/bartdeijkers/ersatzrs:dev}"

exec docker run --rm --gpus all --entrypoint ffmpeg "$IMAGE" \
    -hide_banner \
    -f lavfi -i testsrc2=s=128x72:d=1 \
    -c:v h264_nvenc \
    -f null -
