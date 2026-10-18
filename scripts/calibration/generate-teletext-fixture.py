#!/usr/bin/env python3
"""Generate original synthetic DVB teletext; never use operator or NOS content.

EN 300 706 sections 8/9 define packet protection/page coding; EN 300 472
section 4 defines DVB carriage. This is a fixture producer, not the runtime
engine. Verify it independently with FFmpeg/libzvbi and TSDuck.
"""

import argparse
from pathlib import Path
import struct

HAMMING_8_4 = (
    0x15, 0x02, 0x49, 0x5E, 0x64, 0x73, 0x38, 0x2F,
    0xD0, 0xC7, 0x8C, 0x9B, 0xA1, 0xB6, 0xFD, 0xEA,
)


def crc32_mpeg(data):
    value = 0xFFFFFFFF
    for byte in data:
        value ^= byte << 24
        for _ in range(8):
            value = ((value << 1) ^ (0x04C11DB7 if value & 0x80000000 else 0)) & 0xFFFFFFFF
    return struct.pack(">I", value)


def odd_parity(byte):
    return byte | (0x80 if byte.bit_count() % 2 == 0 else 0)


def reverse_bits(byte):
    return int(f"{byte:08b}"[::-1], 2)


def data_unit(page, row, data, subtitle=False):
    address = (page // 100 % 8) | (row << 3)
    fields = bytes([HAMMING_8_4[address & 15], HAMMING_8_4[address >> 4]]) + data
    assert len(fields) == 42
    return bytes([3 if subtitle else 2, 44, 0xE0, 0xE4]) + bytes(map(reverse_bits, fields))


def page_header(page, subtitle=False):
    # Erase, subtitle/suppress-header and serial transmission flags; English G0.
    nibbles = [page % 10, page % 100 // 10, 0, 8, 0, 8 if subtitle else 0,
               1 if subtitle else 0, 1]
    text = b"ERSATZRS SYNTHETIC TEST".ljust(32)
    return bytes(HAMMING_8_4[n] for n in nibbles) + bytes(map(odd_parity, text))


def page_row(text):
    assert len(text) <= 40
    return bytes(map(odd_parity, text.ljust(40)))


def timestamp(pts):
    return bytes([0x21 | ((pts >> 29) & 14), (pts >> 22) & 255,
                  ((pts >> 14) & 254) | 1, (pts >> 7) & 255, ((pts << 1) & 254) | 1])


class FixtureTransport:
    def __init__(self):
        self.continuity = {}
        self.output = bytearray()

    def packet(self, pid, payload, start=False):
        counter = self.continuity.get(pid, 0)
        self.continuity[pid] = (counter + 1) % 16
        assert len(payload) <= 184
        self.output.extend(bytes([0x47, (0x40 if start else 0) | (pid >> 8),
                                  pid & 255, 0x10 | counter]) + payload.ljust(184, b"\xff"))

    def section(self, pid, table):
        self.packet(pid, b"\x00" + table + crc32_mpeg(table), True)

    def tables(self, caption_pages=None):
        self.section(0, bytes.fromhex("00 b0 0d 00 01 c1 00 00 00 01 f0 00"))
        if caption_pages is None:
            descriptor = bytes.fromhex("56 0a") + b"eng" + bytes([0x0A, 0x00])
            descriptor += b"eng" + bytes([0x10, 0x88])
        else:
            entries = b"".join(language + bytes([(2 << 3) | (page // 100 % 8),
                                                  ((page % 100 // 10) << 4) | (page % 10)])
                               for language, page in caption_pages)
            descriptor = bytes([0x56, len(entries)]) + entries
        body = bytes.fromhex("00 01 c1 00 00 e1 00 f0 00 06 e1 00")
        body += bytes([0xF0, len(descriptor)]) + descriptor
        self.section(0x1000, bytes([2, 0xB0, len(body) + 4]) + body)

    def clock(self, pts):
        # An adaptation-only PCR establishes the clock without advancing payload CC.
        base = pts - 9000
        clock = bytes([(base >> 25) & 255, (base >> 17) & 255, (base >> 9) & 255,
                       (base >> 1) & 255, ((base & 1) << 7) | 0x7E, 0])
        counter = (self.continuity.get(0x100, 0) - 1) % 16
        self.output.extend(bytes([0x47, 1, 0, 0x20 | counter, 183, 0x10])
                           + clock + b"\xff" * 176)

    def pes(self, units, pts):
        self.clock(pts)
        units = list(units)
        # Fixed 45-byte PES header plus identifier; three units fit one TS payload.
        while len(units) % 4 != 3:
            units.append(bytes([255, 44]) + b"\xff" * 44)
        payload = b"\x10" + b"".join(units)
        content = b"\x00\x00\x01\xbd" + struct.pack(">H", 39 + len(payload))
        content += bytes([0x84, 0x80, 36]) + timestamp(pts) + b"\xff" * 31 + payload
        assert len(content) % 184 == 0
        for offset in range(0, len(content), 184):
            self.packet(0x100, content[offset:offset + 184], offset == 0)


def generate_fixture():
    transport = FixtureTransport()
    pages = [(200, False, b"\x06TV GUIDE INDEX"),
             (201, False, b"\x03CHANNEL ONE  12:00 SYNTHETIC SHOW"),
             (888, True, b"\x0b\x0bSynthetic subtitle page 888\x0a\x0a")]
    for cycle in range(8):
        transport.tables()
        for index, (page, subtitle, text) in enumerate(pages):
            pts = 90000 + cycle * 90000 + index * 18000
            transport.pes([data_unit(page, 0, page_header(page, subtitle), subtitle)], pts)
            transport.pes([data_unit(page, 1, page_row(text), subtitle),
                           data_unit(page, 2, page_row(b"\x02Independent decoder fixture"), subtitle)],
                          pts + 3600)
        # Close the last transmitted page; this is deliberately not user content.
        transport.pes([data_unit(899, 0, page_header(899))], 90000 + cycle * 90000 + 72000)
    return bytes(transport.output)


def generate_caption_lifecycle_fixture():
    """Emit one A, one B and an explicit blank page, without caption repeats.

    Offsets below are authored PES times relative to the first PES, not a
    promise about the remuxed media origin or a decoder's display times. The
    independent decoder must establish A/B/empty events after the real remux.
    A seek into the quiet interval deliberately cannot recover A from a recent
    repeated page. PCR/PSI continue independently through the 60-second source.
    """
    transport = FixtureTransport()
    events = {}

    def page(offset, number, rows, subtitle=False):
        header = data_unit(number, 0, page_header(number, subtitle), subtitle)
        body = [data_unit(number, row, page_row(text), subtitle)
                for row, text in rows]
        # A separate same-magazine header completes the transmitted page. It is
        # not a substitute for the explicit C4/blank page 888 at the clear time.
        closing = data_unit(899, 0, page_header(899))
        events[offset] = [header]
        events[offset + 3600] = body
        events[offset + 7200] = [closing]

    page(0, 200, [(1, b"\x03WRONG PAGE 200 LIFECYCLE CONTROL")])
    page(45000, 888, [(1, b"\x0b\x0bSynthetic lifecycle caption A\x0a\x0a")], True)
    page(25 * 90000, 888, [(1, b"\x0b\x0bSynthetic lifecycle caption B\x0a\x0a")], True)
    # page_header already sets C4. Transmit every display row as spaces as well,
    # so the clear has positive wire evidence instead of relying on silence.
    page(40 * 90000, 888, [(row, b"") for row in range(1, 25)], True)

    clock_times = set(range(0, 60 * 90000 + 1, 1800))
    for offset in sorted(clock_times | events.keys()):
        pts = 90000 + offset
        if offset in clock_times:
            transport.tables()
        if offset in events:
            # pes() supplies the PCR for this time. Do not duplicate it when a
            # periodic clock tick and a page transmission have the same time.
            transport.pes(events[offset], pts)
        else:
            transport.clock(pts)
    return bytes(transport.output)


def generate_two_caption_fixture():
    """Two subtitle pages on one PID, without a forced/default descriptor claim."""
    transport = FixtureTransport()
    pages = [(b"eng", 888, b"\x0b\x0bENGLISH PAGE 888\x0a\x0a"),
             (b"nld", 889, b"\x0b\x0bNEDERLANDSE PAGINA 889\x0a\x0a")]
    for cycle in range(8):
        transport.tables([(language, page) for language, page, _ in pages])
        for index, (_, page, text) in enumerate(pages):
            if page == 889:
                suffix = b" EARLY" if cycle < 4 else (b" LATE" if cycle < 7 else b" FINAL")
                text = b"\x0b\x0bNEDERLANDSE PAGINA 889" + suffix + b"\x0a\x0a"
            pts = 90000 + cycle * 90000 + index * 36000
            transport.pes([data_unit(page, 0, page_header(page, True), True)], pts)
            transport.pes([data_unit(page, 1, page_row(text), True)], pts + 3600)
        transport.pes([data_unit(899, 0, page_header(899))],
                      90000 + cycle * 90000 + 72000)
    # Positive wire clear completes the last finite nonempty caption. Silence
    # or an EOF is not a substitute for a transmitted page update.
    transport.tables([(language, page) for language, page, _ in pages])
    pts = 90000 + 8 * 90000 + 36000
    transport.pes([data_unit(889, 0, page_header(889, True), True)], pts)
    transport.pes([data_unit(889, row, page_row(b""), True) for row in range(1, 25)], pts + 3600)
    transport.pes([data_unit(899, 0, page_header(899))], pts + 36000)
    return bytes(transport.output)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, help="Destination TS fixture")
    parser.add_argument("--scenario", choices=("default", "caption-lifecycle", "two-captions"),
                        default="default", help="Optional source-state qualification scenario")
    args = parser.parse_args()
    scenarios = {"default": generate_fixture,
                 "caption-lifecycle": generate_caption_lifecycle_fixture,
                 "two-captions": generate_two_caption_fixture}
    fixture = scenarios[args.scenario]()
    args.output.write_bytes(fixture)
