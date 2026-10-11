#!/usr/bin/env python3
"""Bounded warm-cache DRCS trial using the receipted original libzvbi 0.2.42.

One decoder receives four complete production-carousel checkpoints. Expectations
are fixed literals, never inferred from input bitmaps or a producer raster. This
is an opt-in Linux x64 cache diagnostic, not full client or publication evidence.
The frozen ABI helper remains unchanged. Run with LD_LIBRARY_PATH selecting the
original library from the maintained proof-build.json receipt.
"""

import ctypes as C
import hashlib
import json
from pathlib import Path
import re
import sys
import types

sys.dont_write_bytecode = True
HELPER_SHA256 = "615bc0ceb7b24738e7cb7e8a6df076c8b58eb9968ecdc7e07d226f76f5d7a76d"
LIBRARY_SHA256 = "2991c13dcd1f8c2b694bcce57b5edd2e68ba5bfab1c3cada55f6e088463e9bf9"
SOURCE_COMMIT = "3785481f51f41a49e28f2b7f6fd5bd9187d24ae1"
# Independent fixed geometry: each bit becomes a foreground/background pixel.
PATTERN_A = (0x801, 0x800, 0x400, 0x200, 0x100, 0x080, 0x040, 0x020, 0x010, 0x009)
PATTERN_B = (0x802, 0x001, 0x002, 0x004, 0x008, 0x010, 0x020, 0x040, 0x080, 0x101)
# Source's wstfont2.xbm SHA f6a1fa6714e3604a466eb8d6ca3b496c2dcf217f217985c710eee288137d41ad:
# ASCII B, glyph 34 (exp-gfx.c unicode_wstfont2), grid row 1/column 2.
BASE_B = (0x000, 0x3FC, 0x306, 0x306, 0x3FC, 0x306, 0x306, 0x3FC, 0x000, 0x000)
STATES = ("a_changed", "a_unchanged", "b_changed", "b_repeat")
COUNTERS = (7, 7, 0, 0)
FIRST_FLAGS = ((1, 1), (0, 0), (1, 1), (0, 0))
HAM = (0x15, 0x02, 0x49, 0x5E, 0x64, 0x73, 0x38, 0x2F,
       0xD0, 0xC7, 0x8C, 0x9B, 0xA1, 0xB6, 0xFD, 0xEA)

def _load_frozen_helper(path):
    with path.open("rb") as stream:
        source = stream.read(65537)
    if len(source) > 65536 or hashlib.sha256(source).hexdigest() != HELPER_SHA256:
        raise RuntimeError("frozen resource ABI helper changed")
    module = types.ModuleType("teletext_update_resource_abi")
    module.__file__ = str(path)
    # Execute only the bytes just authenticated, never cached bytecode or a reread.
    exec(compile(source, str(path), "exec"), module.__dict__)
    return module


BASE = _load_frozen_helper(Path(__file__).with_name("teletext-resource-decoder.py"))
require = BASE.require


def nibble(byte):
    require(byte in HAM, "noncanonical Hamming header byte")
    return HAM.index(byte)


def header(unit):
    address = nibble(BASE.BIT_REVERSE[unit[4]]) | (nibble(BASE.BIT_REVERSE[unit[5]]) << 4)
    if address >> 3 != 0:
        return None
    body = [nibble(BASE.BIT_REVERSE[byte]) for byte in unit[6:14]]
    return address & 7, body[0] | (body[1] << 4), body


def validate_headers(units, index):
    resources, closing = [], 0
    for unit in units:
        decoded = header(unit)
        if decoded is None:
            continue
        magazine, page, body = decoded
        if page == 0xFF:
            closing += 1
        if magazine == 2 and page == 0xC0:
            resources.append(body)
    final = header(units[-1])
    require(closing == 2 and final is not None and final[1] == 0xFF,
            "checkpoint must end after two complete carousel closures")
    require(len(resources) == 2, "expected one DRCS acquisition per complete cycle")
    for repeat, body in enumerate(resources):
        flags = (body[3] >> 3, (body[6] >> 1) & 1)
        require(body[2] == 0 and body[3] & 7 == COUNTERS[index]
                and body[4] == 1 and body[5] == 0,
                "DRCS S1, S2 or last-packet count differs from the fixed lifecycle")
        require(flags == (FIRST_FLAGS[index] if repeat == 0 else (0, 0)),
                "DRCS C4/C8 differs from first/repeated full transmission")
        require(body[6] & 0xD == 0xD and body[7] == 1,
                "resource suppression/serial/full-page controls changed")


def validate(value):
    require(type(value) is dict and set(value) == {"expected_version", "sequences"},
            "invalid request members")
    require(value["expected_version"] == "0.2.42", "unsupported decoder version")
    sequences = value["sequences"]
    require(type(sequences) is list and 1 <= len(sequences) <= 4, "invalid sequence count")
    names = set()
    for sequence in sequences:
        require(type(sequence) is dict and set(sequence) == {"name", "stages"},
                "invalid sequence members")
        name = sequence["name"]
        require(type(name) is str and re.fullmatch(r"[A-Za-z0-9_-]{1,64}", name)
                and name not in names, "invalid or duplicate sequence name")
        names.add(name)
        stages = sequence["stages"]
        require(type(stages) is list and len(stages) == 4, "expected four fixed lifecycle stages")
        for index, stage in enumerate(stages):
            require(type(stage) is dict and set(stage) == {"state", "units"}
                    and stage["state"] == STATES[index], "invalid lifecycle stage")
            units = stage["units"]
            require(type(units) is list and 1 <= len(units) <= BASE.MAX_UNITS,
                    "invalid checkpoint unit count")
            for unit in units:
                require(type(unit) is list and len(unit) == 46
                        and all(BASE.integer(byte, 0, 255) for byte in unit), "invalid unit bytes")
                require(unit[0] == 2 and unit[1] == 44 and unit[3] == 0xE4,
                        "invalid DVB teletext framing")
            validate_headers(units, index)
    return sequences


def mapped_original():
    with Path("/proc/self/maps").open("rb") as stream:
        raw = stream.read(4 * 1024 * 1024 + 1)
    require(len(raw) <= 4 * 1024 * 1024, "native library map exceeds bound")
    paths = {line.split()[-1] for line in raw.decode().splitlines() if "/libzvbi.so." in line}
    require(len(paths) == 1, "expected exactly one mapped decoder library")
    path = Path(paths.pop())
    require(path.is_file() and path.stat().st_size <= 16 * 1024 * 1024,
            "invalid mapped decoder library")
    with path.open("rb") as stream:
        binary = stream.read(16 * 1024 * 1024 + 1)
    require(len(binary) <= 16 * 1024 * 1024
            and hashlib.sha256(binary).hexdigest() == LIBRARY_SHA256,
            "mapped decoder is not the receipted original library")


def literal_pixels(rows):
    return [(bits >> (11 - x)) & 1 for bits in rows for x in range(12)]


def draw_literal(lib, page, row, column, rows):
    cell = page.text[row * page.columns + column]
    require(cell.bits1 & 0x1F == 0 and (cell.bits1 >> 8) & 255 == 0
            and (cell.bits1 >> 16) & 255 == 3, "expected normal steady opaque unstyled glyph")
    require(cell.bits1 >> 24 == 7 and cell.bits2 & 255 == 0,
            "fixed glyph foreground/background changed")
    guard = 16
    canvas = (C.c_uint8 * (120 + guard * 2))(*([0xA5] * (120 + guard * 2)))
    lib.vbi_draw_vt_page_region(C.byref(page), 6, C.byref(canvas, guard), 12,
                               column, row, 1, 1, 1, 1)
    require(list(canvas[:guard]) == [0xA5] * guard and list(canvas[-guard:]) == [0xA5] * guard,
            "native drawing exceeded the guarded 12x10 region")
    require(list(canvas[guard:guard + 120]) == [7 if bit else 0 for bit in literal_pixels(rows)],
            "rendered pixels differ from the fixed independent literal")


def check_page(lib, decoder, index):
    page, fetched = BASE.Page(), False
    try:
        fetched = bool(lib.vbi_fetch_vt_page(decoder, C.byref(page), 0x200, 0, 2, 25, 0))
        require(fetched and page.pgno == 0x200 and page.subno == 0
                and page.rows == 25 and page.columns == 41, "unexpected fetched display geometry")
        require(page.text[page.columns * 2 + 1].bits2 >> 16 == 66, "Base B calibration changed")
        draw_literal(lib, page, 2, 1, BASE_B)
        cell = page.text[page.columns + 8]
        require(cell.bits2 >> 16 == 0xF400 and bool(page.drcs[16]),
                "normal DRCS S1=0/slot=0 is absent or changed")
        rows = PATTERN_A if index < 2 else PATTERN_B
        address = C.cast(page.drcs[16], C.c_void_p).value
        packed = C.string_at(address, 60)
        pixels = [(packed[x // 2] >> (4 * (x % 2))) & 15 for x in range(120)]
        require(pixels == literal_pixels(rows), "cached DRCS pixels differ from the fixed literal")
        draw_literal(lib, page, 1, 8, rows)
    finally:
        if fetched:
            lib.vbi_unref_page(C.byref(page))


def decode_sequence(lib, sequence):
    decoder = lib.vbi_decoder_new()
    require(bool(decoder), "decoder allocation failed")
    callback = BASE.CALLBACK(lambda event, user: None)
    tick = 0
    try:
        require(lib.vbi_event_handler_register(decoder, 2, callback, None),
                "teletext event registration failed")
        lib.vbi_teletext_set_level(decoder, 2)
        for index, stage in enumerate(sequence["stages"]):
            for unit in stage["units"]:
                sliced = BASE.Sliced()
                sliced.id, sliced.line = 3, unit[2] & 31
                sliced.data[:42] = bytes(BASE.BIT_REVERSE[byte] for byte in unit[4:])
                lib.vbi_decode(decoder, C.byref(sliced), 1, 1.0 + tick * 0.04)
                tick += 1
            check_page(lib, decoder, index)
    except BASE.CalibrationError as error:
        raise BASE.CalibrationError(f"{sequence['name']}: {error}") from error
    finally:
        lib.vbi_decoder_delete(decoder)


def main():
    raw = sys.stdin.buffer.read(BASE.MAX_INPUT + 1)
    require(0 < len(raw) <= BASE.MAX_INPUT, "invalid bounded input size")
    sequences = validate(json.loads(raw, object_pairs_hook=BASE.unique_object))
    lib, version = BASE.load_decoder()
    mapped_original()
    for sequence in sequences:
        decode_sequence(lib, sequence)
    print(json.dumps({"decoder_version": version, "loaded_library_sha256": LIBRARY_SHA256,
                      "source_commit": SOURCE_COMMIT, "sequences_passed": len(sequences),
                      "checkpoints_passed": len(sequences) * 4, "warm_cache_diagnostic": True,
                      "client_conformance_proven": False}, sort_keys=True))


if __name__ == "__main__":
    try:
        main()
    except (BASE.CalibrationError, OSError, ValueError, AttributeError, RecursionError, TypeError) as error:
        message = str(error) if isinstance(error, BASE.CalibrationError) else "native update calibration could not run"
        print(json.dumps({"error": message}), file=sys.stderr)
        sys.exit(1)
