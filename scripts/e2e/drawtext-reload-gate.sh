#!/usr/bin/env sh
set -eu

# Manual DTX-A02 package gate. It keeps one FFmpeg process alive, atomically
# replaces the UTF-8 text file, and proves decoded late-frame pixels differ.
# Override FFMPEG_BIN and FONT_FILE when running outside the repository.

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
ffmpeg_bin=${FFMPEG_BIN:-ffmpeg}
font_file=${FONT_FILE:-"$script_dir/../../assets/fonts/Roboto-Regular.ttf"}
work_dir=$(mktemp -d "${TMPDIR:-/tmp}/ersatzrs-drawtext.XXXXXX")
ffmpeg_pid=

cleanup() {
    if [ -n "$ffmpeg_pid" ] && kill -0 "$ffmpeg_pid" 2>/dev/null; then
        kill "$ffmpeg_pid" 2>/dev/null || true
        wait "$ffmpeg_pid" 2>/dev/null || true
    fi
    rm -rf "$work_dir"
}
trap cleanup EXIT HUP INT TERM

text_file="$work_dir/dynamic text ü.txt"
output_file="$work_dir/decoded frames.rgb"
printf '%s' 'FIRST' > "$text_file"

filter="drawtext=fontfile='$font_file':textfile='$text_file':reload=1:expansion=none:fontsize=40:x=10:y=30:fontcolor=white:text_shaping=1"
"$ffmpeg_bin" \
    -nostdin -hide_banner -loglevel error -re \
    -f lavfi -i color=c=black:s=320x100:d=3:r=4 \
    -vf "$filter" -frames:v 12 -pix_fmt rgb24 -f rawvideo "$output_file" &
ffmpeg_pid=$!

sleep 1.3
kill -0 "$ffmpeg_pid"
replacement="$work_dir/replacement.txt"
printf '%s' 'SECOND' > "$replacement"
mv "$replacement" "$text_file"

wait "$ffmpeg_pid"
completed_pid=$ffmpeg_pid
ffmpeg_pid=

frame_bytes=96000
expected_bytes=1152000
actual_bytes=$(wc -c < "$output_file")
test "$actual_bytes" -eq "$expected_bytes"
dd if="$output_file" of="$work_dir/early.rgb" bs="$frame_bytes" skip=1 count=1 2>/dev/null
dd if="$output_file" of="$work_dir/late.rgb" bs="$frame_bytes" skip=10 count=1 2>/dev/null
if cmp -s "$work_dir/early.rgb" "$work_dir/late.rgb"; then
    echo "drawtext reload gate failed: early and late decoded frames are identical" >&2
    exit 1
fi

echo "drawtext same-process reload proof passed (pid $completed_pid)"
