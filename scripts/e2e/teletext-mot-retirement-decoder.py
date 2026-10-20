#!/usr/bin/env python3
"""Independent cached-page MOT retirement for four pinned ordinary captures.

One original libzvbi decoder receives old, neutral-prefix, successor and repeat.
It fetches the OLD cached pages immediately after the prefix, before receiving
any successor display. --omit-prefix is an expected-failure negative control.
This Linux-x64 probe adds no client/runtime dependency or transport timing claim.
"""

import ctypes as C
import hashlib
import json
from pathlib import Path
import sys
import types

sys.dont_write_bytecode = True
PINS = (
    (78, "5d6657d656d3a908b63c11fdaa36c114c14bf7f222dabfda836ad36a2f11e779"),
    (20, "c5932564879f6ada19afae3429ac2d6f3f5c243d060ef4ce36178ce9cbc8a9f4"),
    (56, "9576707bf345c86ca39c00b82e57d34d2f608f9cdafc5084912bd134e1db73cc"),
    (56, "c1118ec1aaa7958befa2ce07e56c5fb59f33971f810f66a9a847b3c3064a7f30"),
)
ORACLE_SHA = "fe250705ddc7665a5012d2bb207d1702cfd3bf765716b0c1d4022265d1959a47"
CACHE_SHA = "64d7ac23a0d4f49094c749b0d3a92e31ab43e781e86d7e9049e72d25df31bc23"
# Fixed wstfont2.xbm f6a1fa6714e3604a466eb8d6ca3b496c2dcf217f217985c710eee288137d41ad:
# exp-gfx.c unicode_wstfont2 maps ASCII A/Z to glyphs 33/58. No decoder output
# or producer-supplied expectation is used to create these literal 12x10 rows.
FONT_Z = (0, 0x3FC, 0x018, 0x030, 0x060, 0x0C0, 0x180, 0x3FC, 0, 0)
FONT_A = (0, 0x0F0, 0x198, 0x30C, 0x606, 0x7FE, 0x606, 0x606, 0, 0)


def frozen(name, digest):
    path = Path(__file__).with_name(name)
    with path.open("rb") as stream:
        raw = stream.read(65537)
    if len(raw) > 65536 or hashlib.sha256(raw).hexdigest() != digest:
        raise RuntimeError("Frozen MOT qualification helper changed")
    module = types.ModuleType("authenticated_mot_" + name.replace("-", "_"))
    module.__file__ = str(path)
    exec(compile(raw, str(path), "exec"), module.__dict__)
    return module


ORACLE = frozen("teletext-editor-drcs-decoder.py", ORACLE_SHA)
CACHE = frozen("teletext-magazine-cache-decoder.py", CACHE_SHA)
base, require = ORACLE.base, ORACLE.require


def inputs(paths):
    require(len(paths) == 4, "Expected old, neutral-prefix, successor and repeat capture files")
    result = []
    for path, (count, digest) in zip(paths, PINS):
        raw, units = ORACLE.warm_suffix(path)
        require(len(units) == count and hashlib.sha256(raw).hexdigest() == digest,
                "Qualified MOT capture bytes or order changed")
        result.append(units)
    require(sum(map(len, result)) <= base.MAX_UNITS, "Combined MOT acquisition exceeds existing bound")
    return result


def font_page(lib, decoder, number, cleared):
    page, fetched = base.Page(), False
    unicode, foreground, rows = (90, 7, FONT_Z) if cleared else (65, 18, FONT_A)
    try:
        fetched = bool(lib.vbi_fetch_vt_page(decoder, C.byref(page), number, 32, 2, 25, 0))
        require(fetched and page.pgno == number and page.subno == 32
                and page.rows == 25 and page.columns == 41, "MOT cached-page owner/geometry differs")
        cell = page.text[41 + 8]
        require(cell.bits2 >> 16 == unicode, "MOT cached glyph is not the fixed fallback/successor")
        require(cell.bits1 & 0x1F == 0 and (cell.bits1 >> 8) & 255 == 0
                and (cell.bits1 >> 16) & 255 == 3, "MOT cached glyph is not normal steady opaque")
        require(cell.bits1 >> 24 == foreground and cell.bits2 & 255 == 0,
                "MOT cached glyph colour indices differ")
        require(int(page.color_map[foreground]) == (0xFFFFFFFF if cleared else 0xFF0000FF)
                and int(page.color_map[0]) == 0xFF000000, "MOT cached glyph literal palette differs")
        canvas = (C.c_uint8 * 152)(*([0xA5] * 152))
        lib.vbi_draw_vt_page_region(C.byref(page), 6, C.byref(canvas, 16), 12, 8, 1, 1, 1, 1, 1)
        require(list(canvas[:16]) == [0xA5] * 16 and list(canvas[-16:]) == [0xA5] * 16,
                "MOT drawing exceeded bounded native canvas")
        expected = [foreground if row & (1 << (11-x)) else 0 for row in rows for x in range(12)]
        require(list(canvas[16:136]) == expected, "MOT literal fallback/successor pixels differ")
        return {"page": number, "subcode": 32, "unicode": unicode, "foreground": foreground,
                "background": 0, "literal_pixels": 120}
    finally:
        if fetched:
            lib.vbi_unref_page(C.byref(page))


def decode(lib, phases, omit=False):
    decoder = lib.vbi_decoder_new()
    require(bool(decoder), "MOT decoder allocation failed")
    callback = base.CALLBACK(lambda event, user: None)
    results, tick = [], 0
    try:
        require(lib.vbi_event_handler_register(decoder, 2, callback, None), "MOT callback registration failed")
        lib.vbi_teletext_set_level(decoder, 2)
        for index, units in enumerate(phases):
            if not (omit and index == 1):
                ORACLE.feed(lib, decoder, units, tick)
            tick += len(units)
            # No display is fed between the first old fetch and the clear check.
            pages = [ORACLE.check_page(lib, decoder, number, "day", 0) if index == 0
                     else font_page(lib, decoder, number, index == 1) for number in (0x124, 0x125)]
            results.append({"phase": ("old", "cached_after_prefix", "successor", "repeat")[index],
                            "units_fed": 0 if omit and index == 1 else len(units), "pages": pages})
    finally:
        lib.vbi_decoder_delete(decoder)
    return results


def main():
    omit = sys.argv[1:2] == ["--omit-prefix"]
    phases = inputs(sys.argv[2:] if omit else sys.argv[1:])
    lib, version = base.load_decoder()
    CACHE.mapped_original()
    observations = decode(lib, phases, omit)
    require(not omit, "Missing-prefix negative control was not detected")
    print(json.dumps({"mode": "mot-retirement", "decoder_version": version,
                      "loaded_library_sha256": CACHE.LIBRARY_SHA256, "frozen_abi_sha256": ORACLE.HELPER_SHA,
                      "source_commit": CACHE.SOURCE_COMMIT, "capture_sha256": [digest for _, digest in PINS],
                      "decoder_lifetimes": 1, "continuous_decoder_clock": True, "units_fed": 210,
                      "cached_pages_without_refeed": True, "observations": observations,
                      "independent_mot_retirement_proven": True, "client_conformance_proven": False,
                      "transport_timing_proven": False, "full_level25_proven": False}, sort_keys=True))


if __name__ == "__main__":
    try:
        main()
    except (base.CalibrationError, OSError, ValueError, AttributeError, TypeError, RecursionError) as error:
        message = str(error) if isinstance(error, base.CalibrationError) else "MOT native qualification failed"
        print(json.dumps({"error": message}), file=sys.stderr)
        sys.exit(1)
