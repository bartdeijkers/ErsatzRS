"""Fixed-reference fences; actual original acquisition is qualified separately."""
import copy
import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location(
    "header_reference_contract", Path(__file__).with_name("teletext-header-national-reference.py")
)
REF = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(REF)
ERROR = REF.IMAGER.resource.CalibrationError
# EN300706 Hamming8/4 codewords, used only to construct synthetic unit tests.
H84 = (0x15, 0x02, 0x49, 0x5E, 0x64, 0x73, 0x38, 0x2F,
       0xD0, 0xC7, 0x8C, 0x9B, 0xA1, 0xB6, 0xFD, 0xEA)


def request():
    units = []
    for index, row in enumerate([0, 28, *range(1, 25), 0]):
        address = 2 | row << 3
        payload = [H84[address & 15], H84[address >> 4]] + [0x20] * 40
        if row == 0:
            fields = [0, 0, 0, 8, 0, 0, 0, 1] if index == 0 else [15, 15, 0, 0, 0, 0, 1, 1]
            payload[2:10] = [H84[n] for n in fields]
        units.append([2, 44, 0xE0, 0xE4] + [REF.IMAGER.resource.BIT_REVERSE[b] for b in payload])
    return {"name": "header", "page": 0x200, "subcode": 0,
            "points": copy.deepcopy(REF.POINTS), "units": units}


def page(extension):
    result = {"number": 0x200, "subcode": 0, "control": {4, 11},
              **{i: b" " * 40 for i in range(25)}}
    result[0] = result[1] = b" " * 8 + b"#" + b" " * 31
    if extension:
        result[28, 0] = [0] * 13
    return result


class HeaderNationalReference(unittest.TestCase):
    def test_original_geometry_and_reference_preserve_input(self):
        value = request()
        before = copy.deepcopy(value)
        REF.validate(value)
        REF.wire_rows(value["units"], lambda b: H84.index(b) if b in H84 else 0xFF)
        self.assertEqual(value, before)

    def test_header_and_body_keep_distinct_original_scalar_and_mask(self):
        for extension in (True, False):
            value = page(extension)
            before = copy.deepcopy(value)
            cells = REF.reference_cells(value)
            self.assertEqual([(c[0], c[2], c[3], c[4]) for c in cells],
                             [(0, 35, "G0_latin", 35), (1, 35, "G0_NOS_en", 163)])
            self.assertNotEqual(cells[0][5], cells[1][5])
            self.assertEqual(value, before)

    def test_wrong_identity_controls_and_missing_rows_reject(self):
        for key, wrong in (("number", 0x201), ("subcode", 32), ("control", {4, 11, 12})):
            value = page(True)
            value[key] = wrong
            with self.assertRaises(ERROR):
                REF.reference_cells(value)
        value = page(False)
        del value[24]
        with self.assertRaises(ERROR):
            REF.reference_cells(value)

    def test_wrong_text_in_either_original_cell_rejects(self):
        for row in (0, 1):
            value = page(False)
            value[row] = b" " * 8 + b"$" + b" " * 31
            with self.assertRaises(ERROR):
                REF.reference_cells(value)

    def test_nonlatin_or_incomplete_x28_rejects(self):
        for words in ([128] + [0] * 12, [None] + [0] * 12, [0] * 12):
            value = page(True)
            value[28, 0] = words
            with self.assertRaises(ERROR):
                REF.reference_cells(value)

    def test_numeric_alias_framing_and_size_reject(self):
        for kind in ("point_bool", "byte_bool", "extra_byte", "missing_unit", "framing", "unknown"):
            value = request()
            if kind == "point_bool": value["points"][0]["row"] = False
            elif kind == "byte_bool": value["units"][0][10] = False
            elif kind == "extra_byte": value["units"][0].append(0)
            elif kind == "missing_unit": value["units"].pop()
            elif kind == "framing": value["units"][0][3] = 0
            else: value["unknown"] = 1
            with self.subTest(kind=kind), self.assertRaises(ERROR):
                REF.validate(value)

    def test_parity_chronology_and_filling_header_are_observed(self):
        for kind in ("parity", "order", "filling", "magazine", "address"):
            units = request()["units"]
            if kind == "parity": units[0][14] ^= 1
            elif kind == "order": units[2], units[3] = units[3], units[2]
            elif kind == "filling": units[-1][6] = REF.IMAGER.resource.BIT_REVERSE[H84[0]]
            elif kind == "magazine": units[0][4] = REF.IMAGER.resource.BIT_REVERSE[H84[3]]
            else: units[0][4] = REF.IMAGER.resource.BIT_REVERSE[0xFF]
            with self.subTest(kind=kind), self.assertRaises(ERROR):
                REF.wire_rows(units, lambda b: H84.index(b) if b in H84 else 0xFF)


if __name__ == "__main__":
    unittest.main()
