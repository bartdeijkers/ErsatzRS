#!/usr/bin/env sh
set -eu

IMAGE="${IMAGE:-ghcr.io/bartdeijkers/ersatzrs:dev}"
script_dir="$(dirname "$0")"
. "$script_dir/managed-ffmpeg.sh"

# Derive Vulkan from the CUDA device, as native playback does. A default
# software Vulkan device cannot validate NVIDIA external-memory interop.
exec docker run --rm --gpus all --entrypoint sh "$IMAGE" \
    -c "$MANAGED_FFMPEG_BOOTSTRAP" sh \
    -hide_banner -nostdin \
    -init_hw_device cuda=nv:0 \
    -init_hw_device vulkan=vk@nv \
    -filter_hw_device vk \
    -f lavfi -i testsrc2=size=320x240:duration=1:rate=25 \
    -vf 'format=yuv420p10le,setparams=range=limited:color_primaries=bt2020:color_trc=smpte2084:colorspace=bt2020nc,hwupload,libplacebo=tonemapping=hable:colorspace=bt709:color_primaries=bt709:color_trc=bt709:format=nv12,hwupload_cuda,scale_cuda=256:144' \
    -c:v h264_nvenc -frames:v 25 -f null -
