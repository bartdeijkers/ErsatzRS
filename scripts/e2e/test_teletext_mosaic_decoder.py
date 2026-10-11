"""Native-library-free input and oracle fences for the G1 calibration."""

import copy
import importlib.util
import io
import json
from pathlib import Path
from types import SimpleNamespace
import unittest
from unittest.mock import patch


SPEC = importlib.util.spec_from_file_location(
    "teletext_mosaic_calibration", Path(__file__).with_name("teletext-mosaic-decoder.py")
)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError("fixed mosaic calibration helper is unavailable")
HELPER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(HELPER)


def request():
    # Structurally valid framing only; this synthetic unit cannot decode a page.
    return {"expected_version": "0.2.42", "cases": [{
        "name": "synthetic_contract", "units": [[2, 44, 0, 0xE4] + [0] * 42],
        "page": 0x200, "subcode": 0, "row": 1, "column": 8,
        "pattern": "contiguous", "raster_rgb4": [[15, 15, 15] for _ in range(120)],
    }]}


class MosaicInputTests(unittest.TestCase):
    def reject(self, raw, error=HELPER.BASE.CalibrationError):
        with patch.object(HELPER.sys, "stdin", SimpleNamespace(buffer=io.BytesIO(raw))), \
             patch.object(HELPER.BASE, "load_decoder") as loader:
            with self.assertRaises(error):
                HELPER.main()
            loader.assert_not_called()

    def test_valid_structural_request_reaches_native_barrier(self):
        with patch.object(HELPER.sys, "stdin", SimpleNamespace(
                buffer=io.BytesIO(json.dumps(request()).encode()))), \
             patch.object(HELPER.BASE, "load_decoder", side_effect=RuntimeError("barrier")) as loader:
            with self.assertRaisesRegex(RuntimeError, "barrier"):
                HELPER.main()
            loader.assert_called_once_with()

    def test_invalid_case_fields_cannot_load_native(self):
        mutations = [
            {"extra": 1}, {"pattern": "unknown"}, {"pattern": ["contiguous"]},
            {"page": 0x2A0}, {"subcode": 0x4321}, {"row": 25}, {"column": 40},
            {"row": True}, {"name": "../../synthetic"},
            {"units": []}, {"units": [[2, 44, 0, 0xE4] + [0] * 42] * 257},
            {"units": [[2, 44, 0, 0] + [0] * 42]},
            {"units": [[2, 44, 0, 0xE4, True] + [0] * 41]},
            {"raster_rgb4": [[15, 15, 15]]},
            {"raster_rgb4": [[0, 0, 0] for _ in range(120)]},
            {"raster_rgb4": [[True, 15, 15] for _ in range(120)]},
            {"raster_rgb4": [[16, 15, 15] for _ in range(120)]},
        ]
        for updates in mutations:
            with self.subTest(updates=updates):
                value = request()
                value["cases"][0].update(updates)
                self.reject(json.dumps(value).encode())
        value = request()
        del value["cases"][0]["pattern"]
        self.reject(json.dumps(value).encode())

    def test_request_and_json_limits_cannot_load_native(self):
        for changes in [{"extra": 0}, {"expected_version": "0.2.45"}, {"cases": []},
                        {"cases": [copy.deepcopy(request()["cases"][0]) for _ in range(17)]},
                        {"cases": [copy.deepcopy(request()["cases"][0]) for _ in range(2)]}]:
            value = request()
            value.update(changes)
            self.reject(json.dumps(value).encode())
        raw = json.dumps(request()).encode()
        self.reject(raw.replace(b'"row": 1', b'"row": 1, "row": 1'))
        self.reject(raw.replace(b'"expected_version": "0.2.42"',
                    b'"expected_version": "0.2.42", "expected_version": "0.2.42"'))
        self.reject(b"")
        self.reject(b" " * (HELPER.BASE.MAX_INPUT + 1))
        self.reject(b'{"cases":', ValueError)


if __name__ == "__main__":
    unittest.main()
