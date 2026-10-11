#!/usr/bin/env python3
"""Bounded independent G1 pixel calibration with public libzvbi 0.2.42.

Production raster and native wire display each match fixed 12x10 literals.
Separated font geometry is explicitly receiver-specific (EN300706 Annex G.2).
This opt-in Linux x86-64 diagnostic does not certify full client acceptance.
"""
import ctypes as C
import importlib.util
import json
from pathlib import Path
import re
import sys

sys.dont_write_bytecode = True
_SPEC = importlib.util.spec_from_file_location(
    "teletext_resource_calibration", Path(__file__).with_name("teletext-resource-decoder.py")
)
if _SPEC is None or _SPEC.loader is None:
    raise RuntimeError("fixed calibration helper is unavailable")
BASE = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(BASE)
PATTERNS = {
    "contiguous": [4095] * 10,
    "separated": [0, 1950, 1950, 0, 1950, 1950, 1950, 0, 1950, 1950],
}
# The pinned libzvbi font places gaps at x=5,11 and y=2,6,9.
# Host MIT atlas gaps remain unchanged and are checked independently above.
NATIVE_PATTERNS = {
    "contiguous": [4095] * 10,
    "separated": [4030, 4030, 0, 4030, 4030, 4030, 0, 4030, 4030, 0],
}
FIELDS = {"name", "units", "page", "subcode", "row", "column", "pattern", "raster_rgb4"}


def literal_raster(pattern):
    return [[15, 15, 15] if (bits >> (11 - x)) & 1 else [0, 0, 0]
            for bits in PATTERNS[pattern] for x in range(12)]


def validate_input(value):
    require = BASE.require
    require(type(value) is dict and set(value) == {"expected_version", "cases"},
            "invalid request members")
    require(value["expected_version"] == "0.2.42", "unsupported expected decoder version")
    cases = value["cases"]
    require(type(cases) is list and 1 <= len(cases) <= BASE.MAX_CASES, "invalid case count")
    names = set()
    for case in cases:
        require(type(case) is dict and set(case) == FIELDS, "invalid case members")
        name = case["name"]
        require(type(name) is str and re.fullmatch(r"[A-Za-z0-9_-]{1,64}", name),
                "invalid synthetic case name")
        require(name not in names, "duplicate case name")
        names.add(name)
        require(type(case["pattern"]) is str and case["pattern"] in PATTERNS,
                "invalid literal mosaic pattern")
        # Reuse strict DVB framing/address limits only, before native loading.
        legacy = {key: case[key] for key in
                  ("name", "units", "page", "subcode", "row", "column")}
        legacy.update(slot=0, s1=0, normal=False, pixels=PATTERNS[case["pattern"]],
                      expect_glyph=True)
        BASE.validate_input({"expected_version": "0.2.42", "cases": [legacy]})
        raster = case["raster_rgb4"]
        require(type(raster) is list and len(raster) == 120 and all(
            type(pixel) is list and len(pixel) == 3 and all(
                BASE.integer(component, 0, 15) for component in pixel) for pixel in raster),
            "invalid bounded RGB4 raster")
        require(raster == literal_raster(case["pattern"]),
                "production raster differs from fixed independent mosaic literal")
    return cases


def check_page(lib, page, case):
    require = BASE.require
    require(page.pgno == case["page"] and page.subno == case["subcode"]
            and page.columns == 41 and page.rows == 25, "unexpected fetched page geometry")
    require(page.text[page.columns * 2 + 1].bits2 >> 16 == ord("B"),
            "base calibration character is missing")
    cell = page.text[page.columns * case["row"] + case["column"]]
    expected_unicode = 0xEE7F if case["pattern"] == "contiguous" else 0xEE5F
    require(cell.bits2 >> 16 == expected_unicode, "decoded full G1 mosaic is incorrect")
    require((cell.bits1 >> 8) & 255 == 0 and (cell.bits1 >> 16) & 255 == 3,
            "calibration requires normal-size opaque glyph")
    require(cell.bits1 & 0x18 == 0, "calibration requires steady unconcealed glyph")
    for index, expected in ((cell.bits1 >> 24, [255, 255, 255, 255]),
                            (cell.bits2 & 255, [0, 0, 0, 255])):
        require(index < 40, "decoded colour index exceeds palette")
        require(list(int(page.color_map[index]).to_bytes(4, "little")) == expected,
                "decoded foreground/background differs from literal calibration colours")
    # A normal one-cell region is verified before this guarded native draw.
    byte_count, guard = 12 * 10 * 4, 16
    canvas = (C.c_uint8 * (byte_count + guard * 2))(*([0xA5] * (byte_count + guard * 2)))
    lib.vbi_draw_vt_page_region(C.byref(page), 32, C.byref(canvas, guard), 12 * 4,
                               case["column"], case["row"], 1, 1, 0, 1)
    require(list(canvas[:guard]) == [0xA5] * guard
            and list(canvas[-guard:]) == [0xA5] * guard,
            "native renderer exceeded bounded canvas")
    expected = bytes(component
                     for bits in NATIVE_PATTERNS[case["pattern"]] for x in range(12)
                     for component in ([255, 255, 255, 255]
                         if bits & (1 << (11 - x)) else [0, 0, 0, 255]))
    require(bytes(canvas[guard:guard + byte_count]) == expected,
            "independent native pixels differ from fixed mosaic literal")


def decode_case(lib, case):
    decoder = lib.vbi_decoder_new()
    BASE.require(bool(decoder), "decoder allocation failed")
    callback = BASE.CALLBACK(lambda event, user: None)
    page, fetched = BASE.Page(), False
    try:
        BASE.require(lib.vbi_event_handler_register(decoder, 2, callback, None),
                     "teletext event registration failed")
        lib.vbi_teletext_set_level(decoder, 2)
        for index, unit in enumerate(case["units"]):
            sliced = BASE.Sliced()
            sliced.id, sliced.line = 3, unit[2] & 31
            sliced.data[:42] = bytes(BASE.BIT_REVERSE[byte] for byte in unit[4:])
            lib.vbi_decode(decoder, C.byref(sliced), 1, 1.0 + index * 0.04)
        fetched = bool(lib.vbi_fetch_vt_page(decoder, C.byref(page), case["page"],
                                           case["subcode"], 2, 25, 0))
        BASE.require(fetched, "display page was not fetched")
        check_page(lib, page, case)
    except BASE.CalibrationError as error:
        raise BASE.CalibrationError(f"{case['name']}: {error}") from error
    finally:
        if fetched:
            lib.vbi_unref_page(C.byref(page))
        lib.vbi_decoder_delete(decoder)


def main():
    raw = sys.stdin.buffer.read(BASE.MAX_INPUT + 1)
    BASE.require(0 < len(raw) <= BASE.MAX_INPUT, "invalid bounded input size")
    cases = validate_input(json.loads(raw, object_pairs_hook=BASE.unique_object))
    lib, version = BASE.load_decoder()
    for case in cases:
        decode_case(lib, case)
    print(json.dumps({"decoder_version": version, "cases_passed": len(cases)}, sort_keys=True))


if __name__ == "__main__":
    try:
        main()
    except (BASE.CalibrationError, OSError, ValueError, AttributeError, RecursionError):
        error = sys.exc_info()[1]
        message = str(error) if isinstance(error, BASE.CalibrationError) else "native mosaic calibration could not run"
        print(json.dumps({"error": message}), file=sys.stderr)
        sys.exit(1)
