#!/usr/bin/env sh
set -eu

IMAGE="${IMAGE:-ghcr.io/bartdeijkers/ersatzrs:dev}"
script_dir="$(dirname "$0")"
. "$script_dir/managed-ffmpeg.sh"

exec docker run --rm --gpus all --entrypoint sh "$IMAGE" \
    -c "$MANAGED_FFMPEG_BOOTSTRAP" sh \
    -hide_banner \
    -f lavfi -i testsrc2=s=128x72:d=1 \
    -c:v h264_nvenc \
    -f null -
