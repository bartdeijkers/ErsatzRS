"""Closed-input, native ownership and pixel-guard tests; no decoder is loaded."""

import hashlib
import io
from pathlib import Path
import types
import unittest
from unittest.mock import Mock, patch

SCRIPT = Path(__file__).with_name("teletext-mot-retirement-decoder.py")
MOT = types.ModuleType("mot_retirement_contract")
MOT.__file__ = str(SCRIPT)
exec(compile(SCRIPT.read_bytes(), str(SCRIPT), "exec"), MOT.__dict__)


class MotRetirementTests(unittest.TestCase):
    def test_exact_capture_order_counts_and_hash_precede_native_loading(self):
        self.assertEqual([count for count, _ in MOT.PINS], [78, 20, 56, 56])
        for paths in ([], ["old"] * 3, ["old"] * 5):
            with patch.object(MOT.base, "load_decoder") as load, \
                    patch.object(MOT.ORACLE, "warm_suffix") as read, \
                    patch.object(MOT.sys, "argv", [str(SCRIPT), *paths]):
                with self.assertRaises(MOT.base.CalibrationError):
                    MOT.main()
                read.assert_not_called()
                load.assert_not_called()
        for index in range(4):
            values = [(f"file{n}".encode(), [n] * count) for n, (count, _) in enumerate(MOT.PINS)]
            pins = [(len(units), hashlib.sha256(raw).hexdigest()) for raw, units in values]
            values[index] = (b"changed bytes", values[index][1])
            with patch.object(MOT, "PINS", pins), patch.object(MOT.ORACLE, "warm_suffix", side_effect=values), \
                    patch.object(MOT.base, "load_decoder") as load, \
                    patch.object(MOT.sys, "argv", [str(SCRIPT), "old", "prefix", "new", "repeat"]):
                with self.assertRaisesRegex(MOT.base.CalibrationError, "capture bytes"):
                    MOT.main()
                load.assert_not_called()

    def test_one_lifetime_exact_feed_order_clock_and_cached_page_checks(self):
        lib = Mock()
        lib.vbi_decoder_new.return_value = 123
        lib.vbi_event_handler_register.return_value = 1
        phases = [[index] * count for index, (count, _) in enumerate(MOT.PINS)]
        with patch.object(MOT.ORACLE, "feed") as feed, \
                patch.object(MOT.ORACLE, "check_page", return_value={}) as old, \
                patch.object(MOT, "font_page", return_value={}) as font:
            report = MOT.decode(lib, phases)
        self.assertEqual([item["phase"] for item in report], ["old", "cached_after_prefix", "successor", "repeat"])
        self.assertEqual([call.args[2:] for call in feed.call_args_list], list(zip(phases, (0, 78, 98, 154))))
        self.assertEqual([call.args[2:] for call in old.call_args_list], [(0x124, "day", 0), (0x125, "day", 0)])
        self.assertEqual([call.args[2:] for call in font.call_args_list],
                         [(number, cleared) for cleared in (True, False, False) for number in (0x124, 0x125)])
        lib.vbi_decoder_new.assert_called_once()
        lib.vbi_teletext_set_level.assert_called_once_with(123, 2)
        lib.vbi_decoder_delete.assert_called_once_with(123)

    def test_missing_prefix_fails_cached_fallback_before_any_successor_and_deletes_decoder(self):
        lib = Mock()
        lib.vbi_decoder_new.return_value = 123
        lib.vbi_event_handler_register.return_value = 1
        phases = [[index] * count for index, (count, _) in enumerate(MOT.PINS)]
        for omit, feeds in ((False, 2), (True, 1)):
            lib.reset_mock()
            with patch.object(MOT.ORACLE, "feed") as feed, \
                    patch.object(MOT.ORACLE, "check_page", return_value={}), \
                    patch.object(MOT, "font_page", side_effect=MOT.base.CalibrationError("stale DRCS")):
                with self.assertRaisesRegex(MOT.base.CalibrationError, "stale DRCS"):
                    MOT.decode(lib, phases, omit)
            self.assertEqual(feed.call_count, feeds)
            lib.vbi_decoder_delete.assert_called_once_with(123)
        # A non-distinguishing decoder must never turn the negative mode into
        # positive proof, even if every requested page check returns success.
        with patch.object(MOT.sys, "argv", [str(SCRIPT), "--omit-prefix", "a", "b", "c", "d"]), \
                patch.object(MOT, "inputs", return_value=phases), \
                patch.object(MOT.base, "load_decoder", return_value=(lib, "0.2.42")), \
                patch.object(MOT.CACHE, "mapped_original"), \
                patch.object(MOT, "decode", return_value=[]), \
                patch.object(MOT.sys, "stdout", io.StringIO()) as output:
            with self.assertRaisesRegex(MOT.base.CalibrationError, "not detected"):
                MOT.main()
            self.assertEqual(output.getvalue(), "")

    def test_fixed_font_pixels_guard_geometry_palette_and_failed_fetch_ownership(self):
        for cleared in (False, True):
            for defect in (None, "missing", "owner", "glyph", "colour", "palette", "pixels", "guard"):
                lib = Mock()
                unicode, foreground, rows = (90, 7, MOT.FONT_Z) if cleared else (65, 18, MOT.FONT_A)

                def fetch(_decoder, pointer, *_args):
                    page = pointer._obj
                    page.pgno, page.subno, page.rows, page.columns = 0x124, 32, 25, 41
                    page.text[49].bits1 = (foreground << 24) | (3 << 16)
                    page.text[49].bits2 = unicode << 16
                    page.color_map[foreground] = 0xFFFFFFFF if cleared else 0xFF0000FF
                    page.color_map[0] = 0xFF000000
                    if defect == "owner": page.subno = 0
                    elif defect == "glyph": page.text[49].bits2 ^= 1 << 16
                    elif defect == "colour": page.text[49].bits1 ^= 1 << 24
                    elif defect == "palette": page.color_map[0] ^= 1
                    return int(defect != "missing")

                def draw(_page, _format, buffer, *_args):
                    canvas = MOT.C.cast(buffer, MOT.C.POINTER(MOT.C.c_uint8 * 120)).contents
                    canvas[:] = [foreground if row & (1 << (11-x)) else 0 for row in rows for x in range(12)]
                    if defect == "pixels": canvas[0] ^= 1
                    elif defect == "guard": MOT.C.cast(buffer, MOT.C.POINTER(MOT.C.c_uint8 * 121)).contents[120] = 0

                lib.vbi_fetch_vt_page.side_effect = fetch
                lib.vbi_draw_vt_page_region.side_effect = draw
                with self.subTest(cleared=cleared, defect=defect):
                    if defect is None:
                        report = MOT.font_page(lib, 123, 0x124, cleared)
                        self.assertEqual((report["unicode"], report["literal_pixels"]), (unicode, 120))
                    else:
                        with self.assertRaises(MOT.base.CalibrationError):
                            MOT.font_page(lib, 123, 0x124, cleared)
                self.assertEqual(lib.vbi_unref_page.call_count, int(defect != "missing"))
                self.assertEqual(lib.vbi_fetch_vt_page.call_args.args[2:], (0x124, 32, 2, 25, 0))


if __name__ == "__main__":
    unittest.main()
