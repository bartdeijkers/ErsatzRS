#!/usr/bin/env python3
"""Pinned independent coded-base fragment oracle, before final object overlays.

The request contains actual DVB data units and literal six-field expectations:
code, repertoire, diacritic, foreground, background, public fragment tag. This
does not certify pixels, deferred attributes, object overlap or client navigation.
The original main-composition oracle and its fixture bytes remain unchanged.
An explicit extended case adds Level1 and raw flash/conceal/box/separation/invert
goldens from the same pinned decoder; it does not prove visible flash timing.
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
CELL = {"row", "column", "l15", "l25"}
EXTENDED = CELL | {"l1", "attributes"}


def extended(case):
    return set(case["cells"][0]) == EXTENDED


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
        shape = set(cells[0]) if type(cells[0]) is dict else set()
        require(shape in (CELL, EXTENDED), "invalid cell members")
        for cell in cells:
            require(type(cell) is dict and set(cell) == shape,
                    "invalid cell members")
            for key in (("l1", "l15", "l25") if shape == EXTENDED else ("l15", "l25")):
                expected = cell[key]
                require(type(expected) is list and len(expected) == 6
                        and resource.integer(expected[5], 0, 8), "invalid fragment tuple")
            if shape == EXTENDED:
                attributes = cell["attributes"]
                require(type(attributes) is dict and set(attributes) == {"l1", "l15", "l25"},
                        "invalid attribute levels")
                for expected in attributes.values():
                    require(type(expected) is list and len(expected) == 6
                            and resource.integer(expected[0], 0, 3)
                            and resource.integer(expected[1], 0, 5)
                            and all(type(flag) is bool for flag in expected[2:]),
                            "invalid attribute tuple")
                # Reuse the unchanged bounded character/position validation for
                # Level1 as well; no new payload or command parser is introduced.
                composition.validate({"expected_version": value["expected_version"],
                    "cases": [{**case, "cells": [{"row":cell["row"],
                        "column":cell["column"], "l15":cell["l1"][:5],
                        "l25":cell["l1"][:5]}]}]})
            checked.append({"row": cell["row"], "column": cell["column"],
                            "l15": cell["l15"][:5], "l25": cell["l25"][:5]})
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
    levels = (("1", "l1"), ("1.5", "l15"), ("2.5", "l25")) if extended(case) else (
        ("1.5", "l15"), ("2.5", "l25"))
    for level, key in levels:
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
            if extended(case):
                attributes = [decoder.get_flash_mode(row, column),
                              decoder.get_flash_rate_phase(row, column),
                              decoder.get_conceal(row, column),
                              decoder.cells[row][column].attr.display.box_win,
                              decoder.get_und_sep(row, column),
                              decoder.get_invert(row, column)]
                require(attributes == cell["attributes"][key],
                        f"{case['name']} Level {level} attributes {row},{column}: "
                        f"actual {attributes}, expected {cell['attributes'][key]}")


def main():
    raw = sys.stdin.buffer.read(resource.MAX_INPUT + 1)
    require(0 < len(raw) <= resource.MAX_INPUT, "invalid bounded input size")
    cases = validate(json.loads(raw, object_pairs_hook=resource.unique_object))
    reader_type, decoder_type = composition.imager.load_decoder()
    for case in cases:
        decode(reader_type, decoder_type, case)
    report = {"decoder": "teletextimager", "decoder_version": composition.imager.VERSION,
                      "decoder_commit": composition.imager.COMMIT, "cases_passed": len(cases),
                      "levels": ["1.5", "2.5"], "fragment_tags": True}
    count = sum(extended(case) for case in cases)
    if count:
        report.update(level1_and_attributes_cases=count)
    print(json.dumps(report, sort_keys=True))


if __name__ == "__main__":
    try:
        main()
    except (resource.CalibrationError, OSError, ValueError, AttributeError, RecursionError,
            KeyError, IndexError, TypeError) as error:
        message = str(error) if isinstance(error, resource.CalibrationError) else "independent base fragment calibration could not run"
        print(json.dumps({"error": message}), file=sys.stderr)
        sys.exit(1)
