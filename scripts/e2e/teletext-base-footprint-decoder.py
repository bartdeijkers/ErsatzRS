#!/usr/bin/env python3
"""Pinned independent coded-base fragment oracle, before final object overlays.

The request contains actual DVB data units and literal six-field expectations:
code, repertoire, diacritic, foreground, background, public fragment tag. This
does not certify pixels, deferred attributes, object overlap or client navigation.
The original main-composition oracle and its fixture bytes remain unchanged.
"""
import importlib.util
import io
import json
from pathlib import Path
import sys

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location(
    "teletext_base_fragment_oracle",
    Path(__file__).with_name("teletext-main-composition-decoder.py"),
)
composition = importlib.util.module_from_spec(spec)
spec.loader.exec_module(composition)
resource = composition.resource
require = resource.require


def validate(value):
    require(type(value) is dict and set(value) == {"expected_version", "cases"},
            "invalid request members")
    cases = value["cases"]
    require(type(cases) is list and 1 <= len(cases) <= 8, "invalid case count")
    projected = []
    for case in cases:
        require(type(case) is dict
                and set(case) == {"name", "page", "subcode", "units", "cells"},
                "invalid case members")
        cells = case["cells"]
        require(type(cells) is list and 1 <= len(cells) <= 32, "invalid cell count")
        checked = []
        for cell in cells:
            require(type(cell) is dict and set(cell) == {"row", "column", "l15", "l25"},
                    "invalid cell members")
            for key in ("l15", "l25"):
                expected = cell[key]
                require(type(expected) is list and len(expected) == 6
                        and resource.integer(expected[5], 0, 8), "invalid fragment tuple")
            checked.append({**cell, "l15": cell["l15"][:5], "l25": cell["l25"][:5]})
        projected.append({**case, "cells": checked})
    composition.validate({"expected_version": value["expected_version"], "cases": projected})
    return cases


def decode(reader_type, decoder_type, case):
    packets = bytes(resource.BIT_REVERSE[byte] for unit in case["units"] for byte in unit[4:])
    pages = reader_type().read(io.BytesIO(packets))
    require(type(pages) is list and len(pages) == 1, "unexpected decoded page count")
    page = pages[0]
    require(page.get("number") == case["page"] and page.get("subcode") == case["subcode"],
            "decoded identity differs")
    for level, key in (("1.5", "l15"), ("2.5", "l25")):
        decoder = decoder_type()
        decoder.decode(page, level=level)
        for cell in case["cells"]:
            row, column = cell["row"], cell["column"]
            require(decoder.get_drcs_ptu(row, column) is None, "unexpected DRCS glyph")
            observed = [decoder.get_char_code(row, column), decoder.get_char_set(row, column),
                        decoder.get_char_diacritic(row, column), decoder.get_foreground(row, column),
                        decoder.get_background(row, column), decoder.get_fragment(row, column).value]
            require(observed == cell[key],
                    f"{case['name']} Level {level} cell {row},{column}: "
                    f"actual {observed}, expected {cell[key]}")


def main():
    raw = sys.stdin.buffer.read(resource.MAX_INPUT + 1)
    require(0 < len(raw) <= resource.MAX_INPUT, "invalid bounded input size")
    cases = validate(json.loads(raw, object_pairs_hook=resource.unique_object))
    reader_type, decoder_type = composition.imager.load_decoder()
    for case in cases:
        decode(reader_type, decoder_type, case)
    print(json.dumps({"decoder": "teletextimager", "decoder_version": composition.imager.VERSION,
                      "decoder_commit": composition.imager.COMMIT, "cases_passed": len(cases),
                      "levels": ["1.5", "2.5"], "fragment_tags": True}, sort_keys=True))


if __name__ == "__main__":
    try:
        main()
    except (resource.CalibrationError, OSError, ValueError, AttributeError, RecursionError,
            KeyError, IndexError, TypeError) as error:
        message = str(error) if isinstance(error, resource.CalibrationError) else "independent base fragment calibration could not run"
        print(json.dumps({"error": message}), file=sys.stderr)
        sys.exit(1)
