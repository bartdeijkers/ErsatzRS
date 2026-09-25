#!/bin/bash
# Build the matched Intel userspace stack on the runtime's Debian baseline.
# The kernel driver and GPU device access remain host responsibilities (ADR 0008).
set -euo pipefail

mkdir -p /out/usr/local
case "${1:?target architecture required}" in
    amd64) ;;
    arm64|arm) exit 0 ;;
    *) echo 'unsupported Intel runtime target architecture' >&2; exit 1 ;;
esac

export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get install -y --no-install-recommends \
    ca-certificates curl python3 build-essential cmake ninja-build meson \
    pkg-config libdrm-dev libpciaccess-dev

mkdir -p /build/sources /usr/local/share/licenses/intel-media
python3 - <<'PY' > /build/sources.tsv
import json
for source in json.load(open('/build/intel-media-sources.json')):
    print(source['name'], source['url'], source['sha256'], sep='\t')
PY
while IFS=$'\t' read -r name url checksum; do
    archive="/build/sources/$name.tar.gz"
    curl --fail --location --retry 3 --connect-timeout 20 --max-time 300 \
        "$url" --output "$archive"
    printf '%s  %s\n' "$checksum" "$archive" | sha256sum --check --strict -
    mkdir -p "/build/sources/$name"
    tar -xzf "$archive" --strip-components=1 -C "/build/sources/$name"
done < /build/sources.tsv

export PKG_CONFIG_PATH=/usr/local/lib/pkgconfig
export LD_LIBRARY_PATH=/usr/local/lib
jobs=${INTEL_BUILD_JOBS:-2}
cmake_install() {
    local name=$1
    shift
    cmake -S "/build/sources/$name" -B "/build/$name" -G Ninja \
        -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX=/usr/local \
        -DCMAKE_INSTALL_LIBDIR=lib "$@"
    cmake --build "/build/$name" --parallel "$jobs"
    cmake --install "/build/$name"
}

meson setup /build/libva /build/sources/libva --prefix=/usr/local --libdir=lib \
    --buildtype=release -Ddriverdir=/usr/lib/x86_64-linux-gnu/dri \
    -Dwith_x11=no -Dwith_glx=no -Dwith_wayland=no
meson compile -C /build/libva -j "$jobs"
meson install -C /build/libva
cmake_install gmmlib -DRUN_TEST_SUITE=OFF
cmake_install media-driver -DLIBVA_DRIVERS_PATH=/usr/local/lib/dri
cmake_install vpl -DBUILD_TESTS=OFF -DBUILD_EXAMPLES=OFF -DINSTALL_EXAMPLES=OFF \
    -DENABLE_LIBDIR_IN_RUNTIME_SEARCH=ON
cmake_install vpl-gpu-rt -DBUILD_TESTS=OFF -DBUILD_TOOLS=OFF

# Ship libraries and their licenses, not compilers, headers, or build trees.
mkdir -p /out/usr/local/lib /out/usr/local/share/licenses/intel-media \
    /out/usr/lib/x86_64-linux-gnu/dri
cp -a /usr/local/lib/. /out/usr/local/lib/
find /out/usr/local/lib -type f \( -name '*.a' -o -name '*.la' \) -delete
rm -rf /out/usr/local/lib/pkgconfig /out/usr/local/lib/cmake
for name in libva gmmlib media-driver vpl vpl-gpu-rt; do
    mkdir -p "/out/usr/local/share/licenses/intel-media/$name"
    find "/build/sources/$name" -maxdepth 1 -type f \
        \( -iname '*license*' -o -name COPYING \) \
        -exec cp '{}' "/out/usr/local/share/licenses/intel-media/$name/" \;
done
cp /build/intel-media-sources.json /out/usr/local/share/licenses/intel-media/sources.json
# Static libva/oneVPL dispatchers in managed FFmpeg also search distro paths.
ln -s /usr/local/lib/dri/iHD_drv_video.so /out/usr/lib/x86_64-linux-gnu/dri/iHD_drv_video.so
for library in /usr/local/lib/libmfx-gen.so*; do
    ln -s "$library" "/out/usr/lib/x86_64-linux-gnu/$(basename "$library")"
done
