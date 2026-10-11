#!/usr/bin/env python3
"""Captured M/29 cache diagnostic with the receipted original libzvbi 0.2.42.

One decoder receives target, seed+target, then the same target without M/29.
This deliberately isolated captured-packet trial is not full publication,
replacement, removal-safety or client-conformance evidence. No producer-supplied
oracle or decoder reset is accepted. Opt-in Linux x64 ABI and library pin only.
Only direct baseline Z/B are checked; the prior compiled-display enhanced-glyph
oracle remains unverified here and is not changed by this palette diagnostic.
"""

import ctypes as C
import hashlib
import json
from pathlib import Path
import sys
import types

sys.dont_write_bytecode = True
HELPER_SHA256 = "615bc0ceb7b24738e7cb7e8a6df076c8b58eb9968ecdc7e07d226f76f5d7a76d"
LIBRARY_SHA256 = "2991c13dcd1f8c2b694bcce57b5edd2e68ba5bfab1c3cada55f6e088463e9bf9"
SOURCE_COMMIT = "3785481f51f41a49e28f2b7f6fd5bd9187d24ae1"
PACKET_SHA256 = "e41fa3c60308599ab6a008da21b793f18e86a5011fac32fe6f84befc9dd6db18"
TELETEXT_SHA256 = "c0920093077fe80924e483bd6a61c76418b7c20f0418dcfc6b148b1986fae4b8"
SEED_SHA256 = "cc165638bc162755447098e3f9f73748a8d80b3d0c1d610e77e4d54edd13e9d6"
TARGET_SHA256 = "3ac04da12333307f13f30850a159a1c49dd0dfd7cd490a6a8a9782d0b1ad5ba8"
# Literal fixed palette/default CLUT2[0] versus the captured M/29 RGB4[1,2,3].
EXPECTED_COLOURS = (0xFF5500FF, 0xFF332211, 0xFF332211)
HAM = (0x15, 0x02, 0x49, 0x5E, 0x64, 0x73, 0x38, 0x2F,
       0xD0, 0xC7, 0x8C, 0x9B, 0xA1, 0xB6, 0xFD, 0xEA)


def _load_frozen_helper(path):
    with path.open("rb") as stream:
        source = stream.read(65537)
    if len(source) > 65536 or hashlib.sha256(source).hexdigest() != HELPER_SHA256:
        raise RuntimeError("frozen resource ABI helper changed")
    module = types.ModuleType("teletext_magazine_resource_abi")
    module.__file__ = str(path)
    # Authenticate and execute the same bytes once; never load cached bytecode.
    exec(compile(source, str(path), "exec"), module.__dict__)
    return module


BASE = _load_frozen_helper(Path(__file__).with_name("teletext-resource-decoder.py"))
require = BASE.require


def nibble(byte):
    require(byte in HAM, "noncanonical Hamming address/header byte")
    return HAM.index(byte)


def address(unit):
    raw = nibble(BASE.BIT_REVERSE[unit[4]]) | (nibble(BASE.BIT_REVERSE[unit[5]]) << 4)
    return raw & 7, raw >> 3


def header(unit):
    magazine, packet = address(unit)
    require(packet == 0, "expected a header packet")
    body = [nibble(BASE.BIT_REVERSE[byte]) for byte in unit[6:14]]
    page = body[0] | (body[1] << 4)
    subcode = body[2] | ((body[3] & 7) << 4) | (body[4] << 8) | ((body[5] & 3) << 12)
    return magazine, page, subcode, body[3] >> 3


def validate_units(units, count, digest):
    require(type(units) is list and len(units) == count, "unexpected captured unit count")
    for unit in units:
        require(type(unit) is list and len(unit) == 46
                and all(BASE.integer(byte, 0, 255) for byte in unit), "invalid captured unit bytes")
        require(unit[0] == 2 and unit[1] == 44 and unit[3] == 0xE4,
                "invalid captured DVB teletext framing")
    require(hashlib.sha256(bytes(byte for unit in units for byte in unit)).hexdigest() == digest,
            "captured unit digest changed")


def validate(value):
    require(type(value) is dict and set(value) == {"expected_version", "seed_units", "target_units"},
            "invalid request members")
    require(value["expected_version"] == "0.2.42", "unsupported decoder version")
    seed, target = value["seed_units"], value["target_units"]
    validate_units(seed, 1, SEED_SHA256)
    validate_units(target, 28, TARGET_SHA256)
    require(address(seed[0]) == (2, 29)
            and nibble(BASE.BIT_REVERSE[seed[0][6]]) == 0, "expected captured M/29/0 seed")
    require(header(target[0]) == (2, 0, 0x1234, 1),
            "target page/subcode/C4 changed")
    require(header(target[-1]) == (2, 0xFF, 0, 0), "target acquisition closure changed")
    require(all(address(unit)[1] not in (28, 29) for unit in target),
            "target must contain no X/28 or M/29")
    return seed, target


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


def rgb(colour):
    return [colour & 255, (colour >> 8) & 255, (colour >> 16) & 255]


def check_page(lib, decoder, checkpoint):
    page, fetched = BASE.Page(), False
    try:
        fetched = bool(lib.vbi_fetch_vt_page(decoder, C.byref(page), 0x200, 0x1234, 2, 25, 0))
        require(fetched and page.pgno == 0x200 and page.subno == 0x1234
                and page.rows == 25 and page.columns == 41, "unexpected fetched display geometry")
        for row, column, unicode in ((1, 9, 90), (2, 1, 66)):
            cell = page.text[row * page.columns + column]
            require(cell.bits2 >> 16 == unicode, "unaffected literal calibration glyph changed")
            require(cell.bits1 >> 24 == 7 and cell.bits2 & 255 == 0,
                    "unaffected literal calibration colours changed")
        colour = page.color_map[16]
        require(colour == EXPECTED_COLOURS[checkpoint],
                "M/29 cache colour differs from the fixed cold/warm/retained literal")
        return {"colour_map16": colour, "rgb": rgb(colour)}
    finally:
        if fetched:
            lib.vbi_unref_page(C.byref(page))


def decode_trial(lib, seed, target):
    decoder = lib.vbi_decoder_new()
    require(bool(decoder), "decoder allocation failed")
    callback = BASE.CALLBACK(lambda event, user: None)
    observations, tick = [], 0
    try:
        require(lib.vbi_event_handler_register(decoder, 2, callback, None),
                "teletext event registration failed")
        lib.vbi_teletext_set_level(decoder, 2)
        # One lifetime and one monotonic clock, including the final C4 target.
        for checkpoint, units in enumerate((target, seed + target, target)):
            for unit in units:
                sliced = BASE.Sliced()
                sliced.id, sliced.line = 3, unit[2] & 31
                sliced.data[:42] = bytes(BASE.BIT_REVERSE[byte] for byte in unit[4:])
                lib.vbi_decode(decoder, C.byref(sliced), 1, 1.0 + tick * 0.04)
                tick += 1
            observations.append(check_page(lib, decoder, checkpoint))
    finally:
        lib.vbi_decoder_delete(decoder)
    return observations


def main():
    raw = sys.stdin.buffer.read(BASE.MAX_INPUT + 1)
    require(0 < len(raw) <= BASE.MAX_INPUT, "invalid bounded input size")
    seed, target = validate(json.loads(raw, object_pairs_hook=BASE.unique_object))
    lib, version = BASE.load_decoder()
    mapped_original()
    observations = decode_trial(lib, seed, target)
    print(json.dumps({"decoder_version": version, "loaded_library_sha256": LIBRARY_SHA256,
                      "frozen_abi_sha256": HELPER_SHA256, "source_commit": SOURCE_COMMIT,
                      "packet_source_sha256": PACKET_SHA256, "teletext_source_sha256": TELETEXT_SHA256,
                      "seed_units_sha256": SEED_SHA256, "target_units_sha256": TARGET_SHA256,
                      "checkpoints_passed": 3, "units_fed": 85,
                      "cold": observations[0], "warm": observations[1], "retained": observations[2],
                      "warm_magazine_cache_diagnostic": True, "removal_safe_proven": False,
                      "client_conformance_proven": False,
                      "compiled_display_enhanced_glyph_proven": False}, sort_keys=True))


if __name__ == "__main__":
    try:
        main()
    except (BASE.CalibrationError, OSError, ValueError, AttributeError, RecursionError, TypeError) as error:
        message = str(error) if isinstance(error, BASE.CalibrationError) else "native magazine cache calibration could not run"
        print(json.dumps({"error": message}), file=sys.stderr)
        sys.exit(1)
