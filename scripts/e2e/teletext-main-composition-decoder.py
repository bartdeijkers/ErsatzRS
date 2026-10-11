#!/usr/bin/env python3
"""Opt-in fixed-cell oracle for production-composed main programs.

Uses the unchanged pinned-imager loader and its eight upstream source guards.
Input contains DVB bytes and literal expected decoder tuples, never host semantic
objects or enhancement commands. This proves selected Level 1.5/2.5 cells, not
Active overlap, full non-Latin composition, rendering pixels or client acceptance.
"""
import importlib.util
import io
import json
from pathlib import Path
import re
import sys

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location(
    "teletext_composition_imager", Path(__file__).with_name("teletext-imager-decoder.py")
)
imager = importlib.util.module_from_spec(spec)
spec.loader.exec_module(imager)
resource = imager.resource
require = resource.require


def validate(value):
    require(type(value) is dict and set(value) == {"expected_version", "cases"}, "invalid request members")
    require(value["expected_version"] == "0.2.0", "unsupported expected decoder version")
    cases = value["cases"]
    require(type(cases) is list and 1 <= len(cases) <= 8, "invalid case count")
    names = set()
    for case in cases:
        require(type(case) is dict and set(case) == {"name", "page", "subcode", "units", "cells"}, "invalid case members")
        name = case["name"]
        require(type(name) is str and re.fullmatch(r"[A-Za-z0-9_-]{1,64}", name), "invalid synthetic name")
        require(name not in names, "duplicate case name")
        names.add(name)
        require(resource.integer(case["page"], 0x100, 0x899)
                and (case["page"] & 15) <= 9 and ((case["page"] >> 4) & 15) <= 9, "invalid display page")
        require(resource.integer(case["subcode"], 0, 0x3F7F)
                and case["subcode"] & 0xC080 == 0, "invalid subcode")
        units = case["units"]
        require(type(units) is list and 1 <= len(units) <= 256, "invalid unit count")
        for unit in units:
            require(type(unit) is list and len(unit) == 46
                    and all(resource.integer(byte, 0, 255) for byte in unit), "invalid unit")
            require(unit[:2] == [2, 44] and unit[3] == 0xE4, "invalid DVB envelope")
        cells = case["cells"]
        require(type(cells) is list and 1 <= len(cells) <= 32, "invalid cell count")
        positions = set()
        for cell in cells:
            require(type(cell) is dict and set(cell) == {"row", "column", "l15", "l25"}, "invalid cell members")
            position = (cell["row"], cell["column"])
            require(resource.integer(position[0], 0, 24) and resource.integer(position[1], 0, 39), "invalid cell position")
            require(position not in positions, "duplicate cell position")
            positions.add(position)
            for level in ("l15", "l25"):
                values = cell[level]
                require(type(values) is list and len(values) == 5
                        and all(resource.integer(v, 0, maximum) for v, maximum in zip(values, (127, 26, 15, 31, 31))),
                        "invalid expected tuple")
    return cases


def decode(reader_type, decoder_type, case):
    packets = bytes(resource.BIT_REVERSE[byte] for unit in case["units"] for byte in unit[4:])
    pages = reader_type().read(io.BytesIO(packets))
    require(type(pages) is list and len(pages) == 1, "unexpected decoded page count")
    page = pages[0]
    require(page.get("number") == case["page"] and page.get("subcode") == case["subcode"], "decoded identity differs")
    for level, key in (("1.5", "l15"), ("2.5", "l25")):
        decoder = decoder_type()
        decoder.decode(page, level=level)
        for cell in case["cells"]:
            row, column = cell["row"], cell["column"]
            require(decoder.get_drcs_ptu(row, column) is None, "unexpected DRCS glyph")
            observed = [decoder.get_char_code(row, column), decoder.get_char_set(row, column),
                        decoder.get_char_diacritic(row, column), decoder.get_foreground(row, column),
                        decoder.get_background(row, column)]
            require(observed == cell[key],
                    f"{case['name']} Level {level} cell {row},{column}: actual {observed}, expected {cell[key]}")


def main():
    raw = sys.stdin.buffer.read(resource.MAX_INPUT + 1)
    require(0 < len(raw) <= resource.MAX_INPUT, "invalid bounded input size")
    cases = validate(json.loads(raw, object_pairs_hook=resource.unique_object))
    reader_type, decoder_type = imager.load_decoder()
    for case in cases:
        decode(reader_type, decoder_type, case)
    print(json.dumps({"decoder": "teletextimager", "decoder_version": imager.VERSION,
                      "decoder_commit": imager.COMMIT, "cases_passed": len(cases),
                      "levels": ["1.5", "2.5"]}, sort_keys=True))


if __name__ == "__main__":
    try:
        main()
    except (resource.CalibrationError, OSError, ValueError, AttributeError, RecursionError,
            KeyError, IndexError, TypeError) as error:
        message = str(error) if isinstance(error, resource.CalibrationError) else "independent composition calibration could not run"
        print(json.dumps({"error": message}), file=sys.stderr)
        sys.exit(1)
