#!/usr/bin/env python3
"""Independent MAIN pixels/fallback for actual edited sparse-bank captures.

Linux x64 test-only decoder candidate qualification is mandatory. Input consists
of unchanged captured DVB units, never native semantic pages or expected rasters.
Both MAIN consumers on pages124/125 must use logical S1=1. This proves neither
side panels nor a production client/runtime dependency or full Level2.5 gate.
"""

import argparse
import ctypes as C
import hashlib
import json
import os
from pathlib import Path
import sys
import types

sys.dont_write_bytecode = True
QUALIFIER_SHA256 = "f1acfc1ab867e67b2ca3e1eecb26e1289e0192c62b9f78b383adec6832a0d4ec"
ORACLE_SHA256 = "fe250705ddc7665a5012d2bb207d1702cfd3bf765716b0c1d4022265d1959a47"
MAX_SOURCE = 65536
MIRRORED = (0x801, 0x001, 0x002, 0x004, 0x008, 0x010, 0x020, 0x040, 0x080, 0x900)


def require(condition, message):
    if not condition:
        raise ValueError(message)


def frozen(name, expected):
    path = Path(__file__).with_name(name)
    require(path.is_file() and not path.is_symlink(), "Expected regular frozen oracle source")
    with path.open("rb") as stream:
        body = stream.read(MAX_SOURCE + 1)
    require(len(body) <= MAX_SOURCE and hashlib.sha256(body).hexdigest() == expected,
            "Frozen sparse-editor dependency source changed")
    module = types.ModuleType("authenticated_" + name.replace("-", "_").replace(".", "_"))
    module.__file__ = str(path)
    exec(compile(body, str(path), "exec"), module.__dict__)
    return module


QUALIFIER = frozen("teletext-client-decoder-candidate.py", QUALIFIER_SHA256)
ORACLE = frozen("teletext-editor-drcs-decoder.py", ORACLE_SHA256)
BASE = ORACLE.base


def captured_units(path):
    raw = QUALIFIER.bounded(path, BASE.MAX_INPUT)
    units = json.loads(raw, object_pairs_hook=BASE.unique_object)
    require(type(units) is list and 1 <= len(units) <= 512, "Captured unit count bound exceeded")
    for unit in units:
        require(type(unit) is list and len(unit) == 46
                and all(BASE.integer(byte, 0, 255) for byte in unit)
                and unit[:2] == [2, 44] and unit[3] == 0xE4,
                "Invalid captured sparse-editor DVB unit")
    return raw, units


def check_sparse_page(lib, decoder, number, theme):
    require(number in (0x124, 0x125) and theme in ("day", "night"), "Unknown sparse-editor literal oracle")
    page, fetched = BASE.Page(), False
    pattern = (0x803, *MIRRORED[1:]) if theme == "day" else MIRRORED
    expected = [(row >> (11 - x)) & 1 for row in pattern for x in range(12)]
    foreground_rgba = [255, 0, 0, 255] if theme == "day" else [0, 0, 255, 255]
    try:
        fetched = bool(lib.vbi_fetch_vt_page(decoder, C.byref(page), number, 0x20, 2, 25, 0))
        require(fetched and page.pgno == number and page.subno == 0x20
                and page.rows == 25 and page.columns == 41, "Unexpected sparse-editor MAIN page identity/geometry")
        require(bool(page.drcs[17]), "Sparse-editor normal DRCS/S1=1 missing")
        packed = C.string_at(C.cast(page.drcs[17], C.c_void_p).value, 60)
        require([(packed[x // 2] >> (4 * (x % 2))) & 15 for x in range(120)] == expected,
                "Sparse-editor decoded bitmap differs from literal")
        cells = []
        for column in (8, 10):
            cell = page.text[page.columns + column]
            require(cell.bits2 >> 16 == 0xF440, "Sparse-editor consumer does not reference normal S1=1 slot0")
            foreground, background = cell.bits1 >> 24, cell.bits2 & 255
            require(foreground < 40 and background < 40, "Sparse-editor decoded palette index is invalid")
            require(list(int(page.color_map[foreground]).to_bytes(4, "little")) == foreground_rgba,
                    "Sparse-editor decoded foreground palette differs")
            require(list(int(page.color_map[background]).to_bytes(4, "little")) == [0, 0, 0, 255],
                    "Sparse-editor decoded background palette differs")
            canvas = (C.c_uint8 * 120)()
            lib.vbi_draw_vt_page_region(C.byref(page), 6, canvas, 12, column, 1, 1, 1, 1, 1)
            require(list(canvas) == [foreground if bit else background for bit in expected],
                    "Sparse-editor rendered pixels differ from literal")
            cells.append({"row": 1, "column": column, "unicode": 0xF440,
                          "decoded_nibbles_checked": 120, "rendered_pixels_checked": 120,
                          "foreground_rgba": foreground_rgba, "background_rgba": [0, 0, 0, 255]})
        return {"page": f"{number:03X}", "raw_page": number, "subcode": 32,
                "rows": 25, "columns": 41, "normal_drcs_bank": 17,
                "logical_subpage": 1, "slot": 0, "bitmap_rows": list(pattern), "cells": cells}
    finally:
        if fetched:
            lib.vbi_unref_page(C.byref(page))


def candidate(arguments):
    qualified = QUALIFIER.qualify(
        arguments.decoder_candidate_proof, arguments.decoder_candidate_proof_sha256,
        arguments.decoder_candidate_build, arguments.decoder_candidate_proof_root,
        arguments.decoder_candidate_build_sha256)
    require(os.environ.get("LD_LIBRARY_PATH") == str(Path(qualified["path"]).parent),
            "LD_LIBRARY_PATH must select exactly the qualified candidate directory before decoder loading")
    return qualified


def decode(units, theme, qualified):
    lib, version = BASE.load_decoder()
    decoder = lib.vbi_decoder_new()
    require(bool(decoder), "Sparse-editor decoder allocation failed")
    callback = BASE.CALLBACK(lambda event, user: None)
    try:
        require(lib.vbi_event_handler_register(decoder, 2, callback, None), "Sparse-editor decoder registration failed")
        lib.vbi_teletext_set_level(decoder, 2)
        ORACLE.feed(lib, decoder, units, 0)
        pages = []
        for number in (0x124, 0x125):
            page = check_sparse_page(lib, decoder, number, theme)
            page["fallback"] = ORACLE.check_indexed_fallback(lib, decoder, number)
            pages.append(page)
        verifier = QUALIFIER.sparse_verifier()
        library = Path(qualified["path"])
        mapped = verifier.mapped_library(library.parent.parent, "sparse",
                                        {"library": "sparse/libzvbi.so.0.13.2", "library_sha256": qualified["sha256"]},
                                        QUALIFIER.bounded(Path("/proc/self/maps"), 4 * 1024 * 1024).decode())
        return version, pages, mapped
    finally:
        lib.vbi_decoder_delete(decoder)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("units", type=Path)
    parser.add_argument("theme", choices=("day", "night"))
    parser.add_argument("--decoder-candidate-proof", required=True, type=Path)
    parser.add_argument("--decoder-candidate-proof-sha256", required=True)
    parser.add_argument("--decoder-candidate-build", required=True, type=Path)
    parser.add_argument("--decoder-candidate-proof-root", required=True, type=Path)
    parser.add_argument("--decoder-candidate-build-sha256", required=True)
    arguments = parser.parse_args()
    raw, units = captured_units(arguments.units)
    qualified = candidate(arguments)  # Complete provenance before any CDLL call.
    version, pages, mapped = decode(units, arguments.theme, qualified)
    require(QUALIFIER.bounded(arguments.units, BASE.MAX_INPUT) == raw, "Sparse-editor captured input changed during decoding")
    print(json.dumps({"schema": 1, "mode": "sparse-editor", "theme": arguments.theme,
                      "decoder_version": version, "decoder_candidate": qualified, "mapped_library": mapped,
                      "qualifier_source_sha256": QUALIFIER_SHA256, "oracle_source_sha256": ORACLE_SHA256,
                      "abi_source_sha256": ORACLE.HELPER_SHA, "units_sha256": hashlib.sha256(raw).hexdigest(),
                      "unit_count": len(units), "pages": pages, "decoder_library_test_only": True,
                      "independent_main_columns": 41, "panels_proven": False, "actual_client_proven": False,
                      "full_level25_proven": False, "full_client_acceptance": False}, sort_keys=True))


if __name__ == "__main__":
    try:
        main()
    except (BASE.CalibrationError, OSError, ValueError, AttributeError, KeyError, TypeError, RecursionError, RuntimeError) as error:
        print(json.dumps({"error": str(error)}), file=sys.stderr)
        sys.exit(1)
