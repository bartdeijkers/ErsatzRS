"""Bounded base-oracle request fences; no independent/native decoder claim."""

import copy
import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location(
    "base_footprint_contract", Path(__file__).with_name("teletext-base-footprint-decoder.py")
)
BASE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BASE)


def request(extended=False):
    cell = {"row": 1, "column": 8, "l15": [65, 15, 0, 7, 0, 0],
            "l25": [65, 15, 0, 7, 0, 0]}
    if extended:
        cell.update(l1=[65, 15, 0, 7, 0, 0], attributes={
            level: [1, 0, True, False, False, False] for level in ("l1", "l15", "l25")
        })
    return {"expected_version": "0.2.0", "cases": [{
        "name": "base_request", "page": 0x200, "subcode": 0,
        "units": [[2, 44, 0, 0xE4] + [0] * 42], "cells": [cell]
    }]}


class BaseFootprintContract(unittest.TestCase):
    def test_legacy_and_extended_requests_preserve_input(self):
        for extended in (False, True):
            value = request(extended)
            before = copy.deepcopy(value)
            self.assertEqual(BASE.validate(value), value["cases"])
            self.assertEqual(value, before)

    def test_level1_uses_existing_scalar_and_fragment_bounds(self):
        for component, wrong in ((0, 128), (0, True), (1, 27), (2, 16), (5, 9)):
            with self.subTest(component=component, wrong=wrong):
                value = request(True)
                value["cases"][0]["cells"][0]["l1"][component] = wrong
                with self.assertRaises(BASE.resource.CalibrationError):
                    BASE.validate(value)

    def test_attribute_flags_cannot_be_numeric_aliases(self):
        for component in range(2, 6):
            value = request(True)
            value["cases"][0]["cells"][0]["attributes"]["l15"][component] = 0
            with self.assertRaisesRegex(BASE.resource.CalibrationError, "attribute tuple"):
                BASE.validate(value)

    def test_every_level_and_complete_attribute_tuple_is_required(self):
        for kind in ("missing_level", "short_tuple", "flash_mode", "flash_rate"):
            value = request(True)
            attributes = value["cases"][0]["cells"][0]["attributes"]
            if kind == "missing_level":
                del attributes["l1"]
            elif kind == "short_tuple":
                attributes["l15"].pop()
            elif kind == "flash_mode":
                attributes["l1"][0] = 4
            else:
                attributes["l25"][1] = 6
            with self.assertRaises(BASE.resource.CalibrationError):
                BASE.validate(value)

    def test_partial_extended_or_mixed_cell_shapes_reject(self):
        for kind in ("no_attributes", "unknown", "mixed"):
            value = request(True)
            cell = value["cases"][0]["cells"][0]
            if kind == "no_attributes":
                del cell["attributes"]
            elif kind == "unknown":
                cell["unexpected"] = True
            else:
                value["cases"][0]["cells"].append(request()["cases"][0]["cells"][0])
            with self.assertRaisesRegex(BASE.resource.CalibrationError, "cell members"):
                BASE.validate(value)

    def test_extended_mode_keeps_existing_payload_and_duplicate_fences(self):
        for kind in ("duplicate", "extra_byte", "unit_limit", "version"):
            value = request(True)
            case = value["cases"][0]
            if kind == "duplicate":
                case["cells"].append(copy.deepcopy(case["cells"][0]))
            elif kind == "extra_byte":
                case["units"][0].append(0)
            elif kind == "unit_limit":
                case["units"] *= 257
            else:
                value["expected_version"] = "unqualified"
            with self.assertRaises(BASE.resource.CalibrationError):
                BASE.validate(value)


if __name__ == "__main__":
    unittest.main()
