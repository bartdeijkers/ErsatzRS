#!/usr/bin/env bash
# Rebuild FFmpeg 9.0 with NVENC/CUDA support for WSL2 + NVIDIA.
#
# Requires:
#   - NVIDIA driver visible in WSL (nvidia-smi works)
#   - /usr/lib/wsl/lib/libnvidia-encode.so.1 (WSL GPU libs)
#   - nv-codec-headers (installed by this script)
#   - libzimg development headers (apt: libzimg-dev) for the zscale filter
#
# The existing FFmpeg in ~/.local/ffmpeg-9.0/ was built without
# --enable-nvenc / --enable-cuda-nvcc, so h264_nvenc and friends
# are absent from `ffmpeg -encoders`. It was also built without
# --enable-libzimg, so the `zscale` filter (required by the HDR/tonemap
# color pipeline and the ErsatzRS startup runtime check) is absent from
# `ffmpeg -filters`.

set -euo pipefail

PREFIX="$HOME/.local/ffmpeg-9.0"
FFMPEG_SRC_DIR="/tmp/ffmpeg-9.0-src"
NV_HEADERS_DIR="/tmp/nv-codec-headers"
CUDA_HOME="${CUDA_HOME:-/usr/local/cuda}"
WSL_LIB="/usr/lib/wsl/lib"

echo "==> Checking prerequisites..."
if ! nvidia-smi &>/dev/null; then
    echo "ERROR: nvidia-smi not found. NVIDIA drivers not visible in WSL." >&2
    exit 1
fi
if [[ ! -f "$WSL_LIB/libnvidia-encode.so.1" ]]; then
    echo "ERROR: $WSL_LIB/libnvidia-encode.so.1 not found. WSL GPU libs missing." >&2
    exit 1
fi
if [[ ! -d "$CUDA_HOME" ]]; then
    echo "ERROR: CUDA_HOME=$CUDA_HOME not found. Install CUDA toolkit." >&2
    exit 1
fi
# zimg provides the zscale filter, required for the HDR/tonemap color pipeline
# and asserted by the ErsatzRS startup runtime check. Fail clearly here rather
# than letting configure silently drop --enable-libzimg.
if ! pkg-config --exists zimg; then
    echo "ERROR: zimg development headers not found (pkg-config can't find 'zimg')." >&2
    echo "       Install them so FFmpeg can be built with --enable-libzimg (zscale)." >&2
    echo "       Debian/Ubuntu/WSL: sudo apt-get install libzimg-dev" >&2
    exit 1
fi

echo "==> Installing nv-codec-headers (NVENC/NVDEC SDK headers)..."
if [[ ! -d "$NV_HEADERS_DIR" ]]; then
    git clone --depth=1 https://github.com/FFmpeg/nv-codec-headers.git "$NV_HEADERS_DIR"
fi
(cd "$NV_HEADERS_DIR" && make PREFIX="$PREFIX" install)

echo "==> Checking for FFmpeg 9.0 source..."
if [[ ! -d "$FFMPEG_SRC_DIR" ]]; then
    echo "Downloading FFmpeg 9.0..."
    curl -L "https://ffmpeg.org/releases/ffmpeg-9.0.tar.xz" | tar -xJ -C /tmp
    mv /tmp/ffmpeg-9.0 "$FFMPEG_SRC_DIR"
fi

echo "==> Configuring FFmpeg with NVENC + CUDA..."
cd "$FFMPEG_SRC_DIR"

# Add WSL libs to linker path so nvenc/cuvid link correctly
export PKG_CONFIG_PATH="$PREFIX/lib/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
export LDFLAGS="-L$PREFIX/lib -L$WSL_LIB ${LDFLAGS:-}"
export CFLAGS="-I$PREFIX/include -I$CUDA_HOME/include ${CFLAGS:-}"

./configure \
    --prefix="$PREFIX" \
    --disable-debug \
    --disable-doc \
    --disable-ffplay \
    --enable-cairo \
    --enable-ffprobe \
    --enable-libfontconfig \
    --enable-gpl \
    --enable-libass \
    --enable-libdav1d \
    --enable-libfreetype \
    --enable-libfribidi \
    --enable-libharfbuzz \
    --enable-libmp3lame \
    --enable-libopus \
    --enable-libsvtav1 \
    --enable-libvpx \
    --enable-libx264 \
    --enable-libx265 \
    --enable-libzimg \
    --enable-openssl \
    --enable-pic \
    --enable-shared \
    --enable-version3 \
    --enable-cuda \
    --enable-cuvid \
    --enable-nvenc \
    --enable-nvdec \
    --enable-ffnvcodec \
    --extra-cflags="-I$CUDA_HOME/include -I$PREFIX/include" \
    --extra-ldflags="-L$CUDA_HOME/lib64 -L$WSL_LIB -L$PREFIX/lib"

echo "==> Building (this takes a few minutes)..."
make -j"$(nproc)"

echo "==> Installing to $PREFIX ..."
make install

echo ""
echo "==> Verifying build..."

FFMPEG_BIN="$PREFIX/bin/ffmpeg"
LLP="$PREFIX/lib:$WSL_LIB"

LD_LIBRARY_PATH="$LLP" "$FFMPEG_BIN" -encoders 2>/dev/null | grep -i nvenc || {
    echo "ERROR: nvenc encoders not found after build. Check configure output above." >&2
    exit 1
}
echo "    [ok] NVENC encoders present"

# Capture the filter list once. Do not discard stderr: if the freshly installed
# binary can't load its shared libs, we want the real linker error here instead
# of a misleading "filter missing" message.
if ! FILTERS="$(LD_LIBRARY_PATH="$LLP" "$FFMPEG_BIN" -hide_banner -filters 2>&1)"; then
    echo "ERROR: '$FFMPEG_BIN -filters' failed to run. Output:" >&2
    echo "$FILTERS" >&2
    exit 1
fi

# ADR 0005: drawvg is the hard runtime requirement for the ErsatzRS graphics engine.
# cairo must be compiled in; drawvg must appear in -filters.
echo "$FILTERS" | grep -qw drawvg || {
    echo "ERROR: drawvg filter missing — cairo was not compiled in. Re-run configure with --enable-cairo." >&2
    exit 1
}
echo "    [ok] drawvg filter present (ADR 0005)"

# zscale (via zimg) is the hard runtime requirement for the HDR/tonemap color
# pipeline; ErsatzRS startup fails fast without it. Confirm the configure flag
# is recorded, the filter is exposed, and a real zscale/tonemap graph runs.
LD_LIBRARY_PATH="$LLP" "$FFMPEG_BIN" -hide_banner -buildconf 2>/dev/null | grep -q -- --enable-libzimg || {
    echo "ERROR: --enable-libzimg missing from buildconf — zimg was not compiled in. Re-run configure with --enable-libzimg." >&2
    exit 1
}
echo "    [ok] --enable-libzimg present in buildconf"

echo "$FILTERS" | grep -qw zscale || {
    echo "ERROR: zscale filter missing — zimg was not compiled in. Re-run configure with --enable-libzimg." >&2
    exit 1
}
echo "    [ok] zscale filter present"

# The synthetic source must carry HDR color metadata, otherwise zscale reports
# "no path between colorspaces" regardless of whether zimg is present. Tag the
# input as bt2020/PQ, tonemap to bt709, and force an 8-bit output.
LD_LIBRARY_PATH="$LLP" "$FFMPEG_BIN" -hide_banner -loglevel error \
    -f lavfi -i "color=c=black:s=64x64:d=0.1,format=yuv420p10le,setparams=color_primaries=bt2020:color_trc=smpte2084:colorspace=bt2020nc" \
    -vf "zscale=t=linear:npl=100,tonemap=hable,zscale=p=bt709:t=bt709:m=bt709,format=yuv420p" \
    -frames:v 1 -f null - || {
    echo "ERROR: zscale/tonemap smoke transcode failed — the color pipeline is not usable." >&2
    exit 1
}
echo "    [ok] zscale/tonemap smoke transcode succeeded"

echo ""
echo "==> Done. $PREFIX/bin/ffmpeg has NVENC, drawvg, and zscale."
echo "    Restart the ErsatzRS server with ./run.sh dev to pick up the new binary."
