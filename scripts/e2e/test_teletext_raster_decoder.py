"""Native-library-free negative contract checks for the raster calibration.

Run with: python3 -m unittest discover -s scripts/e2e -p test_teletext_raster_decoder.py
"""

import copy
import importlib.util
import io
import json
from pathlib import Path
from types import SimpleNamespace
import unittest
from unittest.mock import patch


SPEC = importlib.util.spec_from_file_location(
    "teletext_raster_calibration", Path(__file__).with_name("teletext-raster-decoder.py")
)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError("fixed raster calibration helper is unavailable")
HELPER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(HELPER)


def request():
    # This is structurally valid framing only, never an alleged decodable page.
    # The independent one-pixel bitmap has a literal white top-left RGB pixel.
    return {
        "expected_version": "0.2.42",
        "cases": [{
            "name": "synthetic_contract",
            "units": [[2, 44, 0, 0xE4] + [0] * 42],
            "page": 0x200, "subcode": 0, "row": 1, "column": 8,
            "slot": 0, "s1": 0, "normal": False,
            "pixels": [0x800] + [0] * 9,
            "size": "normal", "foreground": [15, 15, 15],
            "background": [0, 0, 0], "invert": False,
            "conceal": False, "reveal": False,
            "raster_rgb4": [[15, 15, 15]] + [[0, 0, 0] for _ in range(119)],
        }],
    }


class RasterInputTests(unittest.TestCase):
    def assert_rejected_before_load(self, raw, error=HELPER.BASE.CalibrationError):
        with patch.object(HELPER.sys, "stdin", SimpleNamespace(buffer=io.BytesIO(raw))), \
             patch.object(HELPER.BASE, "load_decoder") as native_loader:
            with self.assertRaises(error):
                HELPER.main()
            native_loader.assert_not_called()

    def test_valid_structural_request_reaches_native_barrier(self):
        raw = json.dumps(request()).encode()
        # Verify the negative-test fixture actually gets past every validation
        # fence, without loading a library or pretending its fake unit decodes.
        with patch.object(HELPER.sys, "stdin", SimpleNamespace(buffer=io.BytesIO(raw))), \
             patch.object(HELPER.BASE, "load_decoder", side_effect=RuntimeError("native barrier")) as loader:
            with self.assertRaisesRegex(RuntimeError, "native barrier"):
                HELPER.main()
            loader.assert_called_once_with()

    def test_invalid_case_fields_rejected_before_load(self):
        # Expectations are the public strict-input contract, not helper output.
        mutations = {
            "unknown_member": {"unexpected": 1},
            "boolean_rgb": {"foreground": [True, 15, 15]},
            "rgb_outside_nibble": {"background": [0, 0, 16]},
            "equal_colours": {"background": [15, 15, 15]},
            "unknown_size": {"size": "triple_width"},
            "size_map_bypass": {"size": ["normal"]},
            "wide_last_column": {"size": "double_width", "column": 39},
            "tall_last_row": {"size": "double_height", "row": 24},
            "invalid_page": {"page": 0x2A0},
            "invalid_subcode": {"subcode": 0x4321},
            "invalid_scope_flag": {"normal": 0},
            "invalid_reveal_flag": {"reveal": 1},
            "invalid_slot": {"slot": 48},
            "invalid_subtable": {"s1": 16},
            "bitmap_boolean": {"pixels": [True] + [0] * 9},
            "bitmap_row_overflow": {"pixels": [4096] + [0] * 9},
            "vacuous_bitmap": {"pixels": [0] * 10},
            "truncated_raster": {"raster_rgb4": [[15, 15, 15]]},
            "wrong_raster_oracle": {"raster_rgb4": [[0, 0, 0] for _ in range(120)]},
            "raster_boolean": {"raster_rgb4": [[True, 15, 15]] + [[0, 0, 0] for _ in range(119)]},
            "unit_count_limit": {"units": [[2, 44, 0, 0xE4] + [0] * 42 for _ in range(257)]},
            "invalid_framing": {"units": [[2, 44, 0, 0] + [0] * 42]},
            "unit_boolean": {"units": [[2, 44, 0, 0xE4, True] + [0] * 41]},
        }
        for name, updates in mutations.items():
            with self.subTest(name=name):
                value = request()
                value["cases"][0].update(updates)
                self.assert_rejected_before_load(json.dumps(value).encode())
        missing = request()
        del missing["cases"][0]["foreground"]
        self.assert_rejected_before_load(json.dumps(missing).encode())

    def test_request_members_case_limits_and_names_rejected_before_load(self):
        values = []
        unknown = request()
        unknown["extra"] = 1
        values.append(unknown)
        wrong_version = request()
        wrong_version["expected_version"] = "0.2.45"
        values.append(wrong_version)
        empty = request()
        empty["cases"] = []
        values.append(empty)
        overflow = request()
        overflow["cases"] = [copy.deepcopy(overflow["cases"][0]) for _ in range(17)]
        values.append(overflow)
        duplicate = request()
        duplicate["cases"].append(copy.deepcopy(duplicate["cases"][0]))
        values.append(duplicate)
        unsafe_name = request()
        unsafe_name["cases"][0]["name"] = "../../synthetic"
        values.append(unsafe_name)
        for index, value in enumerate(values):
            with self.subTest(index=index):
                self.assert_rejected_before_load(json.dumps(value).encode())

    def test_duplicate_members_malformed_json_and_bounded_bytes_rejected_before_load(self):
        valid = json.dumps(request()).encode()
        duplicate_top = valid.replace(b'"expected_version": "0.2.42"',
                                      b'"expected_version": "0.2.42", "expected_version": "0.2.42"')
        duplicate_nested = valid.replace(b'"slot": 0', b'"slot": 0, "slot": 0')
        for raw in (duplicate_top, duplicate_nested, b"", b" " * (HELPER.BASE.MAX_INPUT + 1)):
            with self.subTest(bytes=len(raw)):
                self.assert_rejected_before_load(raw)
        self.assert_rejected_before_load(b'{"cases":', ValueError)


if __name__ == "__main__":
    unittest.main()
