#!/usr/bin/env python3
"""Record the pinned imager's private Passive charset gap without changing goldens.

This is a negative decoder calibration, never Level 2.5 conformance evidence.
The unchanged strict oracle validates the request and all six Level 1.5 cells
before its first Level 2.5 mismatch. Its pinned source guards remain mandatory.
"""
import importlib.util
import json
from pathlib import Path
import sys

sys.dont_write_bytecode = True
SPEC = importlib.util.spec_from_file_location(
    "teletext_national_oracle",
    Path(__file__).with_name("teletext-main-composition-decoder.py"),
)
ORACLE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(ORACLE)


def main():
    raw = sys.stdin.buffer.read(ORACLE.resource.MAX_INPUT + 1)
    ORACLE.resource.require(0 < len(raw) <= ORACLE.resource.MAX_INPUT,
                            "invalid bounded input size")
    cases = ORACLE.validate(json.loads(raw, object_pairs_hook=ORACLE.resource.unique_object))
    ORACLE.resource.require(len(cases) == 1
                            and cases[0]["name"] == "english_national_enlarged_overlap"
                            and len(cases[0]["cells"]) == 6,
                            "unexpected negative calibration fixture")
    reader, decoder = ORACLE.imager.load_decoder()
    try:
        ORACLE.decode(reader, decoder, cases[0])
    except ORACLE.resource.CalibrationError as error:
        expected = ("english_national_enlarged_overlap Level 2.5 cell 1,7: "
                    "actual [124, 0, 0, 7, 0], expected [124, 6, 0, 7, 0]")
        ORACLE.resource.require(str(error) == expected, "unexpected decoder failure")
        print(json.dumps({"decoder": "teletextimager", "decoder_version": "0.2.0",
                          "decoder_commit": ORACLE.imager.COMMIT,
                          "conformance": False, "known_failure": expected,
                          "level_1_5_selected_cells_passed": 6}, sort_keys=True))
        return
    raise ORACLE.resource.CalibrationError("expected pinned decoder gap was not reproduced")


if __name__ == "__main__":
    try:
        main()
    except (ORACLE.resource.CalibrationError, OSError, ValueError, AttributeError,
            RecursionError, KeyError, IndexError, TypeError) as error:
        print(json.dumps({"error": str(error)}), file=sys.stderr)
        sys.exit(1)
