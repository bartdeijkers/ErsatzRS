#!/usr/bin/env sh
set -eu

IMAGE="${IMAGE:-ghcr.io/bartdeijkers/ersatzrs:dev}"
DRI_DEVICE="${DRI_DEVICE:-/dev/dri}"

exec docker run --rm \
    --device="${DRI_DEVICE}:${DRI_DEVICE}" \
    -e ANV_VIDEO_DECODE="${ANV_VIDEO_DECODE:-1}" \
    -e RADV_PERFTEST="${RADV_PERFTEST:-video_decode}" \
    --entrypoint ffmpeg \
    "$IMAGE" \
    -hide_banner \
    -init_hw_device vulkan=vk:0 \
    -filter_hw_device vk \
    -f lavfi -i testsrc2=s=128x72:d=1 \
    -vf format=rgba,hwupload,scale_vulkan=128:72,hwdownload,format=yuv420p \
    -f null -
