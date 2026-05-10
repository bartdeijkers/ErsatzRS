#!/usr/bin/env bash
# Rebuild FFmpeg 8.1 with NVENC/CUDA support for WSL2 + NVIDIA.
#
# Requires:
#   - NVIDIA driver visible in WSL (nvidia-smi works)
#   - /usr/lib/wsl/lib/libnvidia-encode.so.1 (WSL GPU libs)
#   - nv-codec-headers (installed by this script)
#
# The existing FFmpeg in ~/.local/ffmpeg-8.1/ was built without
# --enable-nvenc / --enable-cuda-nvcc, so h264_nvenc and friends
# are absent from `ffmpeg -encoders`.

set -euo pipefail

PREFIX="$HOME/.local/ffmpeg-8.1"
FFMPEG_SRC_DIR="/tmp/ffmpeg-8.1-src"
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

echo "==> Installing nv-codec-headers (NVENC/NVDEC SDK headers)..."
if [[ ! -d "$NV_HEADERS_DIR" ]]; then
    git clone --depth=1 https://github.com/FFmpeg/nv-codec-headers.git "$NV_HEADERS_DIR"
fi
(cd "$NV_HEADERS_DIR" && make PREFIX="$PREFIX" install)

echo "==> Checking for FFmpeg 8.1 source..."
if [[ ! -d "$FFMPEG_SRC_DIR" ]]; then
    echo "Downloading FFmpeg 8.1..."
    curl -L "https://ffmpeg.org/releases/ffmpeg-8.1.tar.xz" | tar -xJ -C /tmp
    mv /tmp/ffmpeg-8.1 "$FFMPEG_SRC_DIR"
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

# ADR 0005: drawvg is the hard runtime requirement for the ErsatzRS graphics engine.
# cairo must be compiled in; drawvg must appear in -filters.
LD_LIBRARY_PATH="$LLP" "$FFMPEG_BIN" -filters 2>/dev/null | grep -q drawvg || {
    echo "ERROR: drawvg filter missing — cairo was not compiled in. Re-run configure with --enable-cairo." >&2
    exit 1
}
echo "    [ok] drawvg filter present (ADR 0005)"

echo ""
echo "==> Done. $PREFIX/bin/ffmpeg has both NVENC and drawvg."
echo "    Restart the ErsatzRS server with ./run.sh dev to pick up the new binary."
