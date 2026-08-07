#!/usr/bin/env bash
# Hardware HDR graph benchmark (ADR 0064, milestone p8b-02).
#
# Compares the HDR tone-map graphs ErsatzRS can generate, so a candidate is only
# adopted on measured evidence. Run it on the hardware you care about: the
# results are meaningless across machines, and a software Vulkan device
# (llvmpipe) says nothing about a real GPU.
#
# Usage:
#   ERSATZRS_FFMPEG=/path/to/ffmpeg scripts/bench/hdr-graph-bench.sh [1080|4k|both]
#
# Fixtures are generated from lavfi into a temporary directory, so nothing is
# downloaded and nothing is committed.
set -euo pipefail

FFMPEG="${ERSATZRS_FFMPEG:-ffmpeg}"
WHICH="${1:-both}"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

command -v "$FFMPEG" >/dev/null 2>&1 || {
    echo "set ERSATZRS_FFMPEG to an ffmpeg binary" >&2
    exit 1
}

echo "runtime: $("$FFMPEG" -hide_banner -version 2>/dev/null | head -1)"
echo

# --- capability report ------------------------------------------------------
# A candidate that cannot initialize is reported as unavailable rather than
# silently skipped, so an empty comparison is never mistaken for a tie.
probe_device() {
    "$FFMPEG" -hide_banner -loglevel error -init_hw_device "$1" \
        -f lavfi -i "testsrc=size=64x64:duration=0.05" -f null - >/dev/null 2>&1
}
for device in cuda vulkan vaapi amf qsv; do
    if probe_device "$device"; then
        echo "device $device: available"
    else
        echo "device $device: UNAVAILABLE (candidates needing it cannot be measured here)"
    fi
done
if probe_device vulkan; then
    echo "vulkan device: $("$FFMPEG" -hide_banner -loglevel verbose -init_hw_device vulkan \
        -f lavfi -i "testsrc=size=64x64:duration=0.05" -f null - 2>&1 \
        | grep -m1 'Device .* selected' || echo unknown)"
fi
echo

make_source() {
    local size="$1" out="$2"
    "$FFMPEG" -hide_banner -loglevel error -y \
        -f lavfi -i "testsrc2=size=${size}:duration=10:rate=30" \
        -vf "format=yuv420p10le,setparams=color_primaries=bt2020:color_trc=smpte2084:colorspace=bt2020nc:range=tv" \
        -c:v libx265 -profile:v main10 -preset ultrafast -pix_fmt yuv420p10le \
        -x265-params "master-display=G(13250,34500)B(7500,3000)R(34000,16000)WP(15635,16450)L(10000000,50):max-cll=1000,400:hdr-opt=1:repeat-headers=1:log-level=error" \
        "$out"
}

bench() {
    local label="$1"; shift
    local result
    if ! result=$( { /usr/bin/time -f "%e %P" timeout 300 "$FFMPEG" -nostdin -hide_banner \
        -loglevel error "$@" -f null - ; } 2>&1 | tail -1 ); then
        printf "  %-46s UNAVAILABLE\n" "$label"
        return
    fi
    printf "  %-46s elapsed=%s cpu=%s\n" "$label" "${result%% *}" "${result##* }"
}

run_size() {
    local size="$1" out_w="$2" out_h="$3" source="$WORK/src.mkv"
    echo "### ${size} HDR10 -> SDR ${out_w}x${out_h} with an alpha overlay ###"
    make_source "$size" "$source"
    local overlay=(-f lavfi -i "color=c=red@0.5:size=200x60:rate=30:duration=10" -shortest)

    bench "software zscale+tonemap, then scale" \
        -i "$source" "${overlay[@]}" \
        -filter_complex "[0:0]zscale=t=linear,tonemap=hable,zscale=p=bt709:t=bt709:m=bt709:r=tv,format=yuv420p,scale=${out_w}:${out_h}[s];[s][1:v]overlay=10:10[v]" \
        -map '[v]' -c:v libx264 -preset veryfast

    bench "sw tonemap -> cuda scale -> nvenc" \
        -init_hw_device cuda -i "$source" "${overlay[@]}" \
        -filter_complex "[0:0]zscale=t=linear,tonemap=hable,zscale=p=bt709:t=bt709:m=bt709:r=tv,format=yuv420p[s];[s][1:v]overlay=10:10[o];[o]format=nv12,hwupload_cuda,scale_cuda=${out_w}:${out_h}[v]" \
        -map '[v]' -c:v h264_nvenc -preset:v p4

    # Candidates 1 and 2 from the milestone: one fused libplacebo pass, and
    # libplacebo peak detection versus the reproducible static-metadata path.
    for peak in true false; do
        bench "fused libplacebo, peak_detect=${peak}" \
            -init_hw_device vulkan -i "$source" \
            -vf "format=yuv420p10le,hwupload,libplacebo=tonemapping=bt.2390:w=${out_w}:h=${out_h}:format=nv12:colorspace=bt709:color_primaries=bt709:color_trc=bt709:peak_detect=${peak}" \
            -c:v libx264 -preset veryfast
    done

    # Candidate 3: the VAAPI/OpenCL bridge versus native tonemap_vaapi.
    bench "vaapi via opencl bridge" \
        -init_hw_device vaapi -i "$source" \
        -vf "format=p010le,hwupload=derive_device=vaapi,hwmap=derive_device=opencl,tonemap_opencl=tonemap=hable:format=nv12,hwmap=derive_device=vaapi:reverse=1,scale_vaapi=${out_w}:${out_h}" \
        -c:v h264_vaapi
    bench "native tonemap_vaapi" \
        -init_hw_device vaapi -i "$source" \
        -vf "format=p010le,hwupload,tonemap_vaapi=format=nv12,scale_vaapi=${out_w}:${out_h}" \
        -c:v h264_vaapi

    echo
}

case "$WHICH" in
    1080) run_size 1920x1080 1280 720 ;;
    4k)   run_size 3840x2160 1920 1080 ;;
    both) run_size 1920x1080 1280 720; run_size 3840x2160 1920 1080 ;;
    *)    echo "usage: $0 [1080|4k|both]" >&2; exit 1 ;;
esac

echo "Reminder: adopt a candidate only when it preserves ADR 0064 output"
echo "semantics AND shows a measured win on real hardware. A software Vulkan"
echo "device (llvmpipe) is not evidence about a GPU."
