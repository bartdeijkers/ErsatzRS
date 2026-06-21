#!/usr/bin/env sh

MANAGED_FFMPEG_BOOTSTRAP='
set -eu

tag="${ERSATZRS_FFMPEG_RELEASE_TAG:-v1.0.1}"
case "$(uname -m)" in
    x86_64|amd64) rid="linux-x64" ;;
    aarch64|arm64) rid="linux-arm64" ;;
    armv7l|armv7*) rid="linux-arm" ;;
    *)
        echo "unsupported ErsatzRS-ffmpeg Docker smoke architecture: $(uname -m)" >&2
        exit 1
        ;;
esac

root="${HOME:-/data}/.ersatzrs/ffmpeg"
ffmpeg="$root/current/bin/ffmpeg"
ffprobe="$root/current/bin/ffprobe"

if [ ! -x "$ffmpeg" ] || [ ! -x "$ffprobe" ]; then
    version="${tag#v}"
    asset="ersatzrs-ffmpeg-${version}-${rid}.tar.gz"
    url="https://github.com/bartdeijkers/ErsatzRS-ffmpeg/releases/download/${tag}/${asset}"
    tmp="$root/.smoke-download"

    rm -rf "$tmp"
    mkdir -p "$tmp/extracted" "$root"
    curl -fsSL "$url" -o "$tmp/runtime.tar.gz"
    curl -fsSL "${url}.sha256" -o "$tmp/runtime.tar.gz.sha256"

    expected="$(set -- $(cat "$tmp/runtime.tar.gz.sha256"); printf "%s" "$1")"
    actual="$(sha256sum "$tmp/runtime.tar.gz" | cut -d " " -f 1)"
    if [ "$expected" != "$actual" ]; then
        echo "ErsatzRS-ffmpeg checksum mismatch for $asset" >&2
        exit 1
    fi

    tar -xzf "$tmp/runtime.tar.gz" -C "$tmp/extracted"
    ffmpeg_file="$(find "$tmp/extracted" -type f -name ffmpeg -print -quit)"
    ffprobe_file="$(find "$tmp/extracted" -type f -name ffprobe -print -quit)"
    if [ -z "$ffmpeg_file" ] || [ -z "$ffprobe_file" ]; then
        echo "$asset did not contain ffmpeg and ffprobe" >&2
        exit 1
    fi

    bin_dir="$(dirname "$ffmpeg_file")"
    if [ "$bin_dir" != "$(dirname "$ffprobe_file")" ]; then
        echo "$asset had ffmpeg and ffprobe in different directories" >&2
        exit 1
    fi

    rm -rf "$root/current"
    if [ "$(basename "$bin_dir")" = "bin" ]; then
        cp -a "$(dirname "$bin_dir")" "$root/current"
    else
        mkdir -p "$root/current/bin"
        cp -a "$bin_dir"/. "$root/current/bin/"
    fi
    printf "%s\n" "$tag" > "$root/current-tag"
    chmod +x "$ffmpeg" "$ffprobe"
    rm -rf "$tmp"
fi

exec "$ffmpeg" "$@"
'
