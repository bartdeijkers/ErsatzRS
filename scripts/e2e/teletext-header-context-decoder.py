#!/usr/bin/env python3
"""Exact header-context fixture: reuse fixed cells, additionally require steady tails.

The unchanged composition oracle does not inspect flash. This bounded observer
checks that a physical AP materializer adds no flash to this original steady
fixture. It does not establish timed-flash or suitable-client acceptance.
"""
import importlib.util
import io
import json
from pathlib import Path
import sys

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location(
    "header_composition", Path(__file__).with_name("teletext-main-composition-decoder.py")
)
composition = importlib.util.module_from_spec(spec)
spec.loader.exec_module(composition)
resource = composition.resource


def main():
    raw = sys.stdin.buffer.read(resource.MAX_INPUT + 1)
    resource.require(0 < len(raw) <= resource.MAX_INPUT, "invalid bounded input size")
    cases = composition.validate(json.loads(raw, object_pairs_hook=resource.unique_object))
    resource.require(len(cases) == 1, "expected exact header-context fixture")
    case = cases[0]
    resource.require(case["name"] == "nonuniform_active_header_background", "unexpected fixture")
    reader_type, decoder_type = composition.imager.load_decoder()
    composition.decode(reader_type, decoder_type, case)
    packets = bytes(resource.BIT_REVERSE[byte] for unit in case["units"] for byte in unit[4:])
    pages = reader_type().read(io.BytesIO(packets))
    resource.require(len(pages) == 1, "unexpected page count")
    for level in ("1.5", "2.5"):
        decoder = decoder_type()
        decoder.decode(pages[0], level=level)
        for column in range(8, 40):
            observed = [decoder.get_flash_mode(0, column), decoder.get_flash_rate_phase(0, column)]
            resource.require(observed == [0, 0],
                             f"{case['name']} Level {level} cell 0,{column}: "
                             f"flash actual {observed}, expected steady [0, 0]")
    print(json.dumps({"decoder": "teletextimager", "decoder_version": "0.2.0",
                      "decoder_commit": composition.imager.COMMIT, "cases_passed": 1,
                      "levels": ["1.5", "2.5"], "steady_tail_cells": 32}, sort_keys=True))


if __name__ == "__main__":
    try:
        main()
    except (resource.CalibrationError, OSError, ValueError, AttributeError, RecursionError,
            KeyError, IndexError, TypeError) as error:
        message = str(error) if isinstance(error, resource.CalibrationError) else "header-context decoding could not run"
        print(json.dumps({"error": message}), file=sys.stderr)
        sys.exit(1)
