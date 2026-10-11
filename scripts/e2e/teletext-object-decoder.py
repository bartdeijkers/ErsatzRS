#!/usr/bin/env python3
"""Public libzvbi 0.2.42 object calibration, Linux x86-64 only.

The fixed frozen sibling supplies the public ABI/version guards. Each synthetic
case checks exact formatted glyphs and colours with a fresh decoder. This is
not a font-pixel oracle, platform gate or production dependency.
"""
import ctypes as C
import importlib.util
import json
from pathlib import Path
import re
import sys

spec = importlib.util.spec_from_file_location(
    "teletext_resource_calibration",
    Path(__file__).with_name("teletext-resource-decoder.py"),
)
resource = importlib.util.module_from_spec(spec)
spec.loader.exec_module(resource)


def validate(value):
    require = resource.require
    require(type(value) is dict and set(value) == {"expected_version", "cases"}, "invalid request members")
    require(value["expected_version"] == "0.2.42", "unsupported decoder version")
    cases = value["cases"]
    require(type(cases) is list and 1 <= len(cases) <= 12, "invalid case count")
    names = set()
    for case in cases:
        require(type(case) is dict and set(case) == {"name", "units", "page", "subcode", "cells"}, "invalid case members")
        name = case["name"]
        require(type(name) is str and re.fullmatch(r"[A-Za-z0-9_-]{1,64}", name), "invalid synthetic case name")
        require(name not in names, "duplicate case name")
        names.add(name)
        require(resource.integer(case["page"], 0x100, 0x899), "invalid display page")
        require((case["page"] & 15) <= 9 and ((case["page"] >> 4) & 15) <= 9, "non-decimal display page")
        require(resource.integer(case["subcode"], 0, 0x3F7F) and case["subcode"] & 0xC080 == 0, "invalid display subcode")
        units = case["units"]
        require(type(units) is list and 1 <= len(units) <= resource.MAX_UNITS, "invalid data-unit count")
        for unit in units:
            require(type(unit) is list and len(unit) == 46 and all(resource.integer(byte, 0, 255) for byte in unit), "invalid data-unit bytes")
            require(unit[0] == 2 and unit[1] == 44 and unit[3] == 0xE4, "invalid data-unit framing")
        cells = case["cells"]
        require(type(cells) is list and 1 <= len(cells) <= 16, "invalid cell count")
        locations = set()
        for cell in cells:
            require(type(cell) is dict and set(cell) == {"row", "column", "unicode", "foreground", "background"}, "invalid cell members")
            require(resource.integer(cell["row"], 1, 24) and resource.integer(cell["column"], 0, 39), "invalid cell location")
            require(resource.integer(cell["unicode"], 0, 65535), "invalid cell Unicode")
            require(resource.integer(cell["foreground"], 0, 39) and resource.integer(cell["background"], 0, 39), "invalid cell colour")
            location = (cell["row"], cell["column"])
            require(location not in locations, "duplicate cell location")
            locations.add(location)
    return cases


def decode_case(lib, case):
    require = resource.require
    decoder = lib.vbi_decoder_new()
    require(bool(decoder), "decoder allocation failed")
    callback = resource.CALLBACK(lambda event, user: None)
    page = resource.Page()
    fetched = False
    try:
        require(lib.vbi_event_handler_register(decoder, 2, callback, None), "teletext event registration failed")
        lib.vbi_teletext_set_level(decoder, 2)
        for index, unit in enumerate(case["units"]):
            sliced = resource.Sliced()
            sliced.id = 3
            sliced.line = unit[2] & 31
            sliced.data[:42] = bytes(resource.BIT_REVERSE[byte] for byte in unit[4:])
            lib.vbi_decode(decoder, C.byref(sliced), 1, 1.0 + index * 0.04)
        fetched = bool(lib.vbi_fetch_vt_page(decoder, C.byref(page), case["page"], case["subcode"], 2, 25, 0))
        require(fetched, "display page was not fetched")
        require(page.pgno == case["page"] and page.subno == case["subcode"] and page.columns == 41 and page.rows == 25, "unexpected fetched geometry")
        require(page.text[page.columns * 2 + 1].bits2 >> 16 == ord("B"), "base calibration character is missing")
        for expected in case["cells"]:
            cell = page.text[page.columns * expected["row"] + expected["column"]]
            actual = (cell.bits2 >> 16, cell.bits1 >> 24, cell.bits2 & 255)
            wanted = (expected["unicode"], expected["foreground"], expected["background"])
            require(actual == wanted, f"cell {expected['row']},{expected['column']} differs: actual {actual}, expected {wanted}")
    except resource.CalibrationError as error:
        raise resource.CalibrationError(f"{case['name']}: {error}") from error
    finally:
        if fetched:
            lib.vbi_unref_page(C.byref(page))
        lib.vbi_decoder_delete(decoder)


def main():
    raw = sys.stdin.buffer.read(resource.MAX_INPUT + 1)
    resource.require(0 < len(raw) <= resource.MAX_INPUT, "invalid bounded input size")
    cases = validate(json.loads(raw, object_pairs_hook=resource.unique_object))
    lib, version = resource.load_decoder()
    for case in cases:
        decode_case(lib, case)
    print(json.dumps({"decoder_version": version, "cases_passed": len(cases)}, sort_keys=True))


if __name__ == "__main__":
    try:
        main()
    except (resource.CalibrationError, OSError, ValueError, AttributeError, RecursionError) as error:
        message = str(error) if isinstance(error, resource.CalibrationError) else "native object calibration could not run"
        print(json.dumps({"error": message}), file=sys.stderr)
        sys.exit(1)
