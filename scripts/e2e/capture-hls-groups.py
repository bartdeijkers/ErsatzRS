#!/usr/bin/env python3
"""Capture plain MPEG-TS HLS without losing declared discontinuity boundaries.

RFC 8216 section 3 permits counter/timestamp resets only at a declared boundary.
Keep original bytes together across every unmarked segment boundary so TSDuck
still detects packet loss there. This bounded fixture helper supports neither
encryption nor byte-range/fMP4 playlists.
"""

import argparse
import hashlib
import json
from pathlib import Path
import time
from urllib.parse import urljoin, urlsplit
from urllib.request import urlopen


def segments(body):
    if not body.startswith('#EXTM3U\n'):
        raise ValueError('Expected a media playlist')
    sequence, discontinuity, duration = 0, False, None
    result = []
    for line in body.splitlines():
        if line.startswith(('#EXT-X-KEY:', '#EXT-X-BYTERANGE:', '#EXT-X-MAP:')):
            raise ValueError('Unsupported encrypted, byte-range or fMP4 fixture')
        if line.startswith('#EXT-X-MEDIA-SEQUENCE:'):
            sequence = int(line.split(':', 1)[1])
        elif line == '#EXT-X-DISCONTINUITY':
            discontinuity = True
        elif line.startswith('#EXTINF:'):
            duration = float(line.split(':', 1)[1].split(',')[0])
            if not 0 < duration < 60:
                raise ValueError('Invalid segment duration')
        elif line and not line.startswith('#'):
            if duration is None:
                raise ValueError('Missing segment duration')
            result.append((sequence, line, duration, discontinuity))
            sequence += 1
            discontinuity, duration = False, None
    return result


def fetch(url, limit):
    with urlopen(url, timeout=10) as response:
        body = response.read(limit + 1)
    if len(body) > limit:
        raise ValueError('Fixture download exceeds size limit')
    return body


def capture(url, seconds, output):
    output.mkdir(parents=True, exist_ok=False)
    deadline = time.monotonic() + 60
    total, previous, group = 0.0, None, 0
    receipt = []
    while time.monotonic() < deadline:
        body = fetch(url, 1024 * 1024)
        (output / f'playlist-{len(receipt):04}.m3u8').write_bytes(body)
        for sequence, uri, duration, discontinuity in segments(body.decode('utf-8')):
            if previous is not None and sequence <= previous:
                continue
            if previous is not None and sequence != previous + 1:
                raise ValueError('Playlist skipped a media sequence')
            segment_url = urljoin(url, uri)
            if urlsplit(segment_url).netloc != urlsplit(url).netloc:
                raise ValueError('Fixture segment changed origin')
            payload = fetch(segment_url, 64 * 1024 * 1024)
            if not payload or len(payload) % 188 or any(
                    payload[offset] != 0x47 for offset in range(0, len(payload), 188)):
                raise ValueError('Invalid MPEG-TS segment framing')
            if previous is not None and discontinuity:
                group += 1
            with (output / f'group-{group:04}.ts').open('ab') as stream:
                stream.write(payload)
            with (output / 'capture.ts').open('ab') as stream:
                stream.write(payload)
            receipt.append(dict(sequence=sequence, duration=duration, group=group,
                                discontinuity=discontinuity,
                                sha256=hashlib.sha256(payload).hexdigest()))
            total += duration
            previous = sequence
            if total >= seconds:
                (output / 'segments.json').write_text(json.dumps(receipt, indent=2) + '\n')
                return
        time.sleep(0.5)
    raise ValueError('Insufficient media before capture deadline')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('url')
    parser.add_argument('seconds', type=float)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    capture(args.url, args.seconds, args.output)
