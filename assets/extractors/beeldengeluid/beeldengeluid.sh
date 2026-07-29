#!/bin/sh

# Resolve and stream a Beeld & Geluid "Schatkamer" episode to stdout for an
# ErsatzRS Remote Stream definition.

set -eu
set -f

usage() {
    echo "Usage: beeldengeluid.sh <Schatkamer episode URL>" >&2
    echo "Expected URL: https://schatkamer.beeldengeluid.nl/serie/.../.../aflevering/..." >&2
}

fail() {
    echo "beeldengeluid.sh: $1" >&2
    exit "${2:-1}"
}

find_program() {
    program=$1
    description=$2
    if ! command -v "$program" >/dev/null 2>&1; then
        fail "$description was not found: $program" 69
    fi
}

query_value() {
    query_string=$1
    wanted_name=$2
    old_ifs=$IFS
    IFS='&'
    set -- $query_string
    IFS=$old_ifs
    for query_part do
        case "$query_part" in
            "$wanted_name="*)
                printf '%s' "${query_part#*=}"
                return 0
                ;;
        esac
    done
    return 1
}

# CloudFront uses a URL-safe Base64 alphabet, but signed query values may
# percent-encode padding or the standard Base64 characters. Decode exactly
# those characters before moving the values from the query into cookies.
decode_cloudfront_value() {
    printf '%s' "$1" | sed \
        -e 's/%2[Bb]/+/g' \
        -e 's/%2[Ff]/\//g' \
        -e 's/%3[Dd]/=/g' \
        -e 's/%2[Dd]/-/g' \
        -e 's/%5[Ff]/_/g' \
        -e 's/%7[Ee]/~/g' \
        -e 's/%25/%/g'
}

if [ "$#" -ne 1 ]; then
    usage
    exit 64
fi

episode_url=$1
case "$episode_url" in
    https://schatkamer.beeldengeluid.nl/serie/*/*/aflevering/* | \
        http://schatkamer.beeldengeluid.nl/serie/*/*/aflevering/*)
        ;;
    *)
        fail "unsupported Schatkamer episode URL: $episode_url" 64
        ;;
esac

url_without_query=${episode_url%%\?*}
url_without_query=${url_without_query%%\#*}
video_id=${url_without_query##*/}
case "$video_id" in
    '' | *[!0-9]*)
        fail "the Schatkamer episode ID must be numeric" 64
        ;;
esac

curl_bin=${CURL_BIN:-curl}
ffmpeg_bin=${FFMPEG_BIN:-ffmpeg}
find_program "$curl_bin" "curl"
find_program "$ffmpeg_bin" "FFmpeg"
find_program grep "grep"
find_program dd "dd"
find_program sed "sed"
find_program mktemp "mktemp"

work_dir=$(mktemp -d "${TMPDIR:-/tmp}/ersatzrs-beeldengeluid.XXXXXX") \
    || fail "unable to create a temporary directory"
response_file=$work_dir/response.rsc
markers_file=$work_dir/markers.txt
streams_file=$work_dir/streams.txt
page_file=$work_dir/page.html
chunks_file=$work_dir/chunks.txt
chunk_file=$work_dir/chunk.js
cookies_file=$work_dir/cookies.txt

cleanup() {
    rm -f \
        "$response_file" \
        "$markers_file" \
        "$streams_file" \
        "$page_file" \
        "$chunks_file" \
        "$chunk_file" \
        "$cookies_file"
    rmdir "$work_dir" 2>/dev/null || true
}
trap cleanup EXIT HUP INT TERM

"$curl_bin" \
    --fail \
    --silent \
    --show-error \
    --location \
    --cookie-jar "$cookies_file" \
    --output "$page_file" \
    "$episode_url" \
    || fail "the Schatkamer episode page request failed"

stream_action_id=${BEELDENGELUID_ACTION_ID:-}
if [ -z "$stream_action_id" ]; then
    LC_ALL=C grep -aoE 'src="[^"]+\.js[^"]*"' "$page_file" \
        | sed -e 's/^src="//' -e 's/"$//' \
        >"$chunks_file" || true

    while IFS= read -r chunk_path; do
        case "$chunk_path" in
            /_next/static/chunks/*.js*)
                ;;
            *)
                continue
                ;;
        esac

        "$curl_bin" \
            --fail \
            --silent \
            --show-error \
            --location \
            --cookie "$cookies_file" \
            --output "$chunk_file" \
            "https://schatkamer.beeldengeluid.nl$chunk_path" \
            || continue

        action_reference=$(LC_ALL=C grep -aoE \
            '"[0-9a-f]{32,64}"[^;]{0,300}"getProgramStreamById"' \
            "$chunk_file" \
            | sed -n '1p' \
            || true)
        if [ -n "$action_reference" ]; then
            stream_action_id=$(printf '%s' "$action_reference" \
                | sed -n 's/^"\([0-9a-f]\{32,64\}\)".*/\1/p')
            [ -n "$stream_action_id" ] && break
        fi
    done <"$chunks_file"
fi

if [ "${#stream_action_id}" -lt 32 ] || [ "${#stream_action_id}" -gt 64 ]; then
    fail "unable to discover a valid getProgramStreamById action"
fi
case "$stream_action_id" in
    *[!0-9a-f]*)
        fail "unable to discover a valid getProgramStreamById action"
        ;;
esac

payload="[\"$video_id\",false]"
"$curl_bin" \
    --fail \
    --silent \
    --show-error \
    --request POST \
    --cookie "$cookies_file" \
    --cookie-jar "$cookies_file" \
    --header 'Content-Type: text/plain;charset=UTF-8' \
    --header "Next-Action: $stream_action_id" \
    --header 'Accept: text/x-component' \
    --data-binary "$payload" \
    --output "$response_file" \
    "$episode_url" \
    || fail "the Schatkamer stream request failed"

# RSC text chunks use "<row>:T<hex-size>,<content>". Locate each marker by
# byte offset, then use dd to read the declared number of bytes so adjacent
# chunks cannot corrupt a signed URL.
LC_ALL=C grep -aobE '[0-9]+:T[0-9a-fA-F]+,' "$response_file" >"$markers_file" || true
: >"$streams_file"

while IFS= read -r marker_match; do
    [ -n "$marker_match" ] || continue
    marker_offset=${marker_match%%:*}
    marker=${marker_match#*:}
    hex_size=${marker#*T}
    hex_size=${hex_size%,}
    chunk_size=$((0x$hex_size))
    chunk_start=$((marker_offset + ${#marker}))
    chunk=$(dd if="$response_file" bs=1 skip="$chunk_start" count="$chunk_size" 2>/dev/null)

    case "$chunk" in
        *sk-video.cdn.beeldengeluid.nl*.m3u8*)
            printf '%s\n' "$chunk" \
                | sed \
                    -e 's/^[[:space:]]*//' \
                    -e 's/[[:space:]]*$//' \
                    -e 's/\\u0026/\&/g' \
                >>"$streams_file"
            ;;
    esac
done <"$markers_file"

[ -s "$streams_file" ] || fail "no signed HLS stream URL was found"

while IFS= read -r signed_url; do
    [ -n "$signed_url" ] || continue
    case "$signed_url" in
        https://sk-video.cdn.beeldengeluid.nl/*.m3u8\?*)
            ;;
        *)
            fail "the Server Action returned an unexpected stream URL"
            ;;
    esac

    base_url=${signed_url%%\?*}
    query=${signed_url#*\?}
    policy_raw=$(query_value "$query" CloudFront-Policy) \
        || fail "the stream URL has no CloudFront-Policy"
    signature_raw=$(query_value "$query" CloudFront-Signature) \
        || fail "the stream URL has no CloudFront-Signature"
    key_pair_id_raw=$(query_value "$query" CloudFront-Key-Pair-Id) \
        || fail "the stream URL has no CloudFront-Key-Pair-Id"

    policy=$(decode_cloudfront_value "$policy_raw")
    signature=$(decode_cloudfront_value "$signature_raw")
    key_pair_id=$(decode_cloudfront_value "$key_pair_id_raw")
    cookie_header="Cookie: CloudFront-Policy=$policy; CloudFront-Signature=$signature; CloudFront-Key-Pair-Id=$key_pair_id"

    # Keep stdout media-only. ErsatzRS forwards it as video/mp2t.
    "$ffmpeg_bin" \
        -nostdin \
        -hide_banner \
        -loglevel error \
        -headers "$cookie_header" \
        -i "$base_url" \
        -map 0:v:0? \
        -map 0:a:0? \
        -c copy \
        -f mpegts \
        pipe:1 \
        || fail "FFmpeg could not stream the signed HLS source"
done <"$streams_file"
