#!/usr/bin/env python3
"""Wrap engine-authored units in the independent synthetic decoder TS fixture.

This is calibration tooling, not the production encoder, carousel or muxer.
Read at most 128 KiB of complete EN 300 472 data units from stdin. Reuse only
the original fixture transport, leaving authored packets/characters unchanged.
"""

import importlib.util
from pathlib import Path
import sys

spec = importlib.util.spec_from_file_location(
    "teletext_fixture", Path(__file__).with_name("generate-teletext-fixture.py"))
fixture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixture)


def wrap(raw):
    if not raw or len(raw) > 128 * 1024 or len(raw) % 46:
        raise ValueError("invalid authored unit extent")
    pages = []
    for offset in range(0, len(raw), 46):
        unit = raw[offset:offset + 46]
        if unit[0] not in (2, 3) or unit[1:4] != bytes([44, 0xE0, 0xE4]):
            raise ValueError("invalid authored unit field")
        first = fixture.HAMMING_8_4.index(fixture.reverse_bits(unit[4]))
        second = fixture.HAMMING_8_4.index(fixture.reverse_bits(unit[5]))
        packet = (first | (second << 4)) >> 3
        if packet == 0:
            pages.append([])
        if not pages:
            raise ValueError("authored page has no header")
        pages[-1].append(unit)
    if len(pages) > 4:
        raise ValueError("calibration supports at most four pages")
    transport = fixture.FixtureTransport()
    for cycle in range(8):
        transport.tables()
        for index, units in enumerate(pages):
            pts = 90000 + cycle * 90000 + index * 18000
            transport.pes(units[:1], pts)
            transport.pes(units[1:], pts + 3600)
        header = bytes(fixture.HAMMING_8_4[n] for n in [15, 15, 0, 0, 0, 0, 1, 1])
        header += bytes(map(fixture.odd_parity, b" " * 32))
        transport.pes([fixture.data_unit(200, 0, header)],
                      90000 + cycle * 90000 + 72000)
    return bytes(transport.output)


def wrap_packetized(raw):
    """Supply calibration PSI/PCR while retaining Rust PES/TS packets verbatim."""
    if not raw or len(raw) > 128 * 1024 or len(raw) % 188:
        raise ValueError("invalid packetized extent")
    transport = fixture.FixtureTransport()
    for offset in range(0, len(raw), 188):
        packet = raw[offset:offset + 188]
        if packet[0] != 0x47 or packet[1] & 0x3F != 1 or packet[2] != 0:
            raise ValueError("calibration requires PID 256")
        if packet[3] & 0xF0 != 0x10:
            raise ValueError("teletext requires payload-only clear packets")
        if packet[1] & 0x40:
            if packet[4:8] != b"\x00\x00\x01\xbd":
                raise ValueError("invalid private PES header")
            pts = packet[13:18]
            stamp = ((pts[0] & 14) << 29) | (pts[1] << 22)
            stamp |= ((pts[2] & 254) << 14) | (pts[3] << 7) | (pts[4] >> 1)
            transport.tables()
            transport.continuity[0x100] = packet[3] & 15
            transport.clock(stamp)
        transport.output.extend(packet)
    return bytes(transport.output)


if __name__ == "__main__":
    writer = wrap_packetized if sys.argv[1:] == ["--packetized"] else wrap
    sys.stdout.buffer.write(writer(sys.stdin.buffer.read(128 * 1024 + 1)))
