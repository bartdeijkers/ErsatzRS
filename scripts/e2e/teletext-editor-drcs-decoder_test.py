"""Fixed dense-bank oracle regressions; mocks do not claim native decoding."""

import ctypes as C
import io
import json
from pathlib import Path
import tempfile
import types
import unittest
from unittest.mock import Mock, patch

SCRIPT = Path(__file__).with_name("teletext-editor-drcs-decoder.py")
ORACLE = types.ModuleType("teletext_editor_drcs_contract")
ORACLE.__file__ = str(SCRIPT)
exec(compile(SCRIPT.read_bytes(), str(SCRIPT), "exec"), ORACLE.__dict__)

# Literal 12-pixel rows, independently readable from the synthetic asymmetric
# glyphs. The editor lights column ten (zero based) in the first normal row.
ORIGINAL = (
    "100000000001", "100000000000", "010000000000", "001000000000",
    "000100000000", "000010000000", "000001000000", "000000100000",
    "000000010000", "000000001001",
)
EDITED = ("100000000011", *ORIGINAL[1:])
MIRRORED = (
    "100000000001", "000000000001", "000000000010", "000000000100",
    "000000001000", "000000010000", "000000100000", "000001000000",
    "000010000000", "100100000000",
)


def decoded_fixture(number=0x124, theme="day", edited=True, defect=None):
    subpage = int(number == 0x125)
    rows = MIRRORED if subpage else EDITED if edited else ORIGINAL
    bits = [int(bit) for row in rows for bit in row]
    packed = (C.c_uint8 * 60)(*(bits[index] | bits[index + 1] << 4 for index in range(0, 120, 2)))
    if defect == "bitmap":
        packed[0] ^= 0x10
    lib = Mock()

    def fetch(_decoder, pointer, _number, _subcode, level, _rows, _navigation):
        page = pointer._obj
        page.pgno, page.subno, page.rows, page.columns = number, 32, 25, 41
        page.color_map[0], page.color_map[7] = 0xFF000000, 0xFFFFFFFF
        page.color_map[18] = 0xFF0000FF if theme == "day" else 0xFFFF0000
        for column, unicode in ((8, 90), (9, 90), (10, 32)):
            page.text[41 + column].bits1 = 7 << 24
            page.text[41 + column].bits2 = unicode << 16
        if level == 2:
            page.drcs[16 + subpage] = C.cast(packed, C.POINTER(C.c_uint8))
            for column in (8, 10):
                page.text[41 + column].bits1 = 18 << 24
                page.text[41 + column].bits2 = (0xF400 + subpage * 64) << 16
            if defect == "second_identity":
                page.text[51].bits2 = (0xF400 + (1 - subpage) * 64) << 16
            elif defect == "second_palette":
                page.text[51].bits1 = 7 << 24
            elif defect == "missing_resource":
                page.drcs[16 + subpage] = C.POINTER(C.c_uint8)()
        elif defect == "fallback_glyph":
            page.text[51].bits2 = 65 << 16
        elif defect == "fallback_colour":
            page.text[49].bits1 = 18 << 24
        elif defect == "fallback_palette":
            page.color_map[7] = 0xFF0000FF
        if defect == "page":
            page.pgno += 1
        elif defect == "subcode":
            page.subno += 1
        elif defect == "geometry":
            page.columns = 56
        return int(defect != "missing_page")

    def draw(_page, _format, canvas, _stride, column, *_rest):
        canvas[:] = [18 if bit else 0 for bit in bits]
        if defect == "second_render" and column == 10:
            canvas[10] ^= 18

    lib.vbi_fetch_vt_page.side_effect = fetch
    lib.vbi_draw_vt_page_region.side_effect = draw
    return lib, packed  # Keep the native ABI pointer's storage alive.


class DenseEditedTests(unittest.TestCase):
    def test_both_consumers_use_literal_edited_zero_and_unchanged_one(self):
        for theme in ("day", "night"):
            for number, subpage in ((0x124, 0), (0x125, 1)):
                edited = theme == "day" and subpage == 0
                with self.subTest(theme=theme, page=number):
                    lib, storage = decoded_fixture(number, theme, edited)
                    result = ORACLE.check_page(lib, 123, number, theme, subpage, edited, columns=(8, 10))
                    self.assertEqual(result["unicode"], 0xF400 + 64 * subpage)
                    self.assertEqual(result["pixels"], 120)
                    self.assertEqual(len(storage), 60)
                    self.assertEqual([call.args[4] for call in lib.vbi_draw_vt_page_region.call_args_list], [8, 10])
                    lib.vbi_unref_page.assert_called_once()

    def test_original_zero_cannot_satisfy_edited_zero_or_mirrored_one(self):
        for expected_subpage, edited in ((0, True), (1, False)):
            lib, _storage = decoded_fixture(edited=False)
            with self.subTest(subpage=expected_subpage):
                with self.assertRaises(ORACLE.base.CalibrationError):
                    ORACLE.check_page(lib, 123, 0x124, "day", expected_subpage, edited, columns=(8, 10))
                lib.vbi_unref_page.assert_called_once()
        lib, _storage = decoded_fixture(0x125, defect="bitmap")
        with self.assertRaisesRegex(ORACLE.base.CalibrationError, "bitmap differs"):
            ORACLE.check_page(lib, 123, 0x125, "day", 1, False, columns=(8, 10))
        lib.vbi_unref_page.assert_called_once()

    def test_second_consumer_and_native_page_identity_are_not_ignored(self):
        for defect in ("second_identity", "second_palette", "second_render", "missing_resource",
                       "page", "subcode", "geometry", "missing_page"):
            with self.subTest(defect=defect):
                lib, _storage = decoded_fixture(defect=defect)
                with self.assertRaises(ORACLE.base.CalibrationError):
                    ORACLE.check_page(lib, 123, 0x124, "day", 0, True, columns=(8, 10))
                self.assertEqual(lib.vbi_unref_page.call_count, int(defect != "missing_page"))

    def test_literal_level_one_and_one_point_five_fallback(self):
        for number in (0x124, 0x125):
            lib, _storage = decoded_fixture(number)
            result = ORACLE.check_indexed_fallback(lib, 123, number)
            self.assertEqual([page["level"] for page in result], ["1", "1.5"])
            self.assertTrue(all(page["unicode"] == [90, 90, 32] for page in result))
            self.assertEqual([call.args[4] for call in lib.vbi_fetch_vt_page.call_args_list], [0, 1])
            self.assertEqual(lib.vbi_unref_page.call_count, 2)
        for defect in ("fallback_glyph", "fallback_colour", "fallback_palette", "page", "subcode", "geometry"):
            with self.subTest(defect=defect):
                lib, _storage = decoded_fixture(defect=defect)
                with self.assertRaises(ORACLE.base.CalibrationError):
                    ORACLE.check_indexed_fallback(lib, 123, 0x124)
                lib.vbi_unref_page.assert_called_once()

    def test_explicit_mode_routes_only_day_page124_to_edited_zero(self):
        with tempfile.TemporaryDirectory() as directory:
            units = Path(directory) / "units.json"
            units.write_text(json.dumps([[2, 44, 0, 0xE4] + [0] * 42]), encoding="utf-8")
            for theme in ("day", "night"):
                lib = Mock()
                lib.vbi_decoder_new.return_value = 123
                lib.vbi_event_handler_register.return_value = 1
                output = io.StringIO()
                with patch.object(ORACLE.sys, "argv", [str(SCRIPT), str(units), theme, "indexed-edited"]), \
                        patch.object(ORACLE.base, "load_decoder", return_value=(lib, "0.2.42")), \
                        patch.object(ORACLE, "check_page", side_effect=[{}, {}]) as check, \
                        patch.object(ORACLE, "check_indexed_fallback", return_value=[]) as fallback, \
                        patch.object(ORACLE.sys, "stdout", output):
                    ORACLE.main()
                self.assertEqual([call.args[2:] for call in check.call_args_list],
                                 [(0x124, theme, 0, theme == "day"), (0x125, theme, 1, False)])
                self.assertTrue(all(call.kwargs == {"columns": (8, 10)} for call in check.call_args_list))
                self.assertEqual([call.args[2] for call in fallback.call_args_list], [0x124, 0x125])
                self.assertTrue(all(page["checked_columns"] == [8, 10] for page in json.loads(output.getvalue())["pages"]))
                lib.vbi_decoder_new.assert_called_once()
                lib.vbi_decoder_delete.assert_called_once_with(123)


if __name__ == "__main__":
    unittest.main()
