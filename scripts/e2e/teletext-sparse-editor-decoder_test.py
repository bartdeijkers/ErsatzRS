"""Pure literal sparse-editor oracle regressions; no native decoding claim."""

import argparse
import ctypes as C
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock, patch

SCRIPT = Path(__file__).with_name("teletext-sparse-editor-decoder.py")
SPEC = importlib.util.spec_from_file_location("sparse_editor_literal_contract", SCRIPT)
EDITOR = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(EDITOR)

# Readable literal pixels independently specify the asymmetric mirrored glyph.
NIGHT = ("100000000001", "000000000001", "000000000010", "000000000100",
         "000000001000", "000000010000", "000000100000", "000001000000",
         "000010000000", "100100000000")
DAY = ("100000000011", *NIGHT[1:])


def fixture(theme="day", defect=None):
    bits = [int(bit) for row in (DAY if theme == "day" else NIGHT) for bit in row]
    if defect == "original_day":
        bits = [int(bit) for row in NIGHT for bit in row]
    elif defect == "nonmirrored":
        bits[12], bits[23] = 1, 0
    packed = (C.c_uint8 * 60)(*(bits[index] | bits[index + 1] << 4 for index in range(0, 120, 2)))
    if defect == "nibble":
        packed[1] = 2
    lib = Mock()
    lib.vbi_decoder_new.return_value = 123
    lib.vbi_event_handler_register.return_value = 1

    def fetch(_decoder, pointer, number, _subcode, level, _rows, _navigation):
        page = pointer._obj
        page.pgno, page.subno, page.rows, page.columns = number, 32, 25, 41
        page.color_map[0], page.color_map[7] = 0xFF000000, 0xFFFFFFFF
        page.color_map[18] = 0xFF0000FF if theme == "day" else 0xFFFF0000
        for column, unicode in ((8, 90), (9, 90), (10, 32)):
            page.text[41 + column].bits1 = 7 << 24
            page.text[41 + column].bits2 = unicode << 16
        if level == 2:
            page.drcs[17] = C.cast(packed, C.POINTER(C.c_uint8))
            for column in (8, 10):
                page.text[41 + column].bits1 = 18 << 24
                page.text[41 + column].bits2 = 0xF440 << 16
            if defect in ("first_identity", "second_identity"):
                page.text[41 + (8 if defect == "first_identity" else 10)].bits2 = 0xF400 << 16
            elif defect in ("first_palette", "second_palette"):
                page.text[41 + (8 if defect == "first_palette" else 10)].bits1 = 7 << 24
            elif defect == "background":
                page.color_map[0] = 0xFF0000FF
            elif defect == "palette_index":
                page.text[51].bits1 = 255 << 24
            elif defect == "missing_resource":
                page.drcs[17] = C.POINTER(C.c_uint8)()
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
        if defect in ("first_render", "second_render") and column == (8 if defect == "first_render" else 10):
            canvas[119] ^= 18

    lib.vbi_fetch_vt_page.side_effect = fetch
    lib.vbi_draw_vt_page_region.side_effect = draw
    return lib, packed  # Retain the pointer storage until the page is released.


def candidate_arguments():
    return argparse.Namespace(decoder_candidate_proof=Path("positive.json"),
                              decoder_candidate_proof_sha256="a" * 64,
                              decoder_candidate_build=Path("build"),
                              decoder_candidate_proof_root=Path("input"),
                              decoder_candidate_build_sha256="b" * 64)


class SparseLiteralTests(unittest.TestCase):
    def test_both_consumers_on_both_pages_match_all_literal_rows_pixels_and_themes(self):
        for theme in ("day", "night"):
            for number in (0x124, 0x125):
                with self.subTest(theme=theme, page=number):
                    lib, storage = fixture(theme)
                    report = EDITOR.check_sparse_page(lib, 123, number, theme)
                    self.assertEqual(report["bitmap_rows"], [int(row, 2) for row in (DAY if theme == "day" else NIGHT)])
                    self.assertEqual((report["normal_drcs_bank"], report["logical_subpage"], report["slot"]), (17, 1, 0))
                    self.assertEqual([cell["column"] for cell in report["cells"]], [8, 10])
                    self.assertTrue(all(cell["unicode"] == 0xF440 and cell["rendered_pixels_checked"] == 120
                                        and cell["decoded_nibbles_checked"] == 120 for cell in report["cells"]))
                    self.assertEqual([call.args[4] for call in lib.vbi_draw_vt_page_region.call_args_list], [8, 10])
                    self.assertEqual(len(storage), 60)
                    lib.vbi_unref_page.assert_called_once()

    def test_unedited_day_wrong_rows_any_nibble_and_each_consumer_cannot_pass(self):
        defects = ("original_day", "nonmirrored", "nibble", "first_identity", "second_identity",
                   "first_palette", "second_palette", "background", "palette_index", "first_render", "second_render",
                   "missing_resource", "page", "subcode", "geometry", "missing_page")
        for number in (0x124, 0x125):
            for defect in defects:
                with self.subTest(page=number, defect=defect):
                    lib, _storage = fixture(defect=defect)
                    with self.assertRaises(ValueError):
                        EDITOR.check_sparse_page(lib, 123, number, "day")
                    self.assertEqual(lib.vbi_unref_page.call_count, int(defect != "missing_page"))
        lib, _storage = fixture("day")
        with self.assertRaisesRegex(ValueError, "bitmap differs"):
            EDITOR.check_sparse_page(lib, 123, 0x124, "night")

    def test_real_decode_sequence_checks_both_pages_fallback_and_actual_mapping(self):
        qualified = {"path": "/synthetic/build/sparse/libzvbi.so.0.13.2", "sha256": "c" * 64}
        for theme in ("day", "night"):
            lib, _storage = fixture(theme)
            verifier = Mock()
            verifier.mapped_library.return_value = qualified
            with patch.object(EDITOR.BASE, "load_decoder", return_value=(lib, "0.2.42")), \
                    patch.object(EDITOR.QUALIFIER, "sparse_verifier", return_value=verifier), \
                    patch.object(EDITOR.QUALIFIER, "bounded", return_value=b"mapped library"):
                version, pages, mapped = EDITOR.decode([], theme, qualified)
            self.assertEqual(version, "0.2.42")
            self.assertEqual([page["raw_page"] for page in pages], [0x124, 0x125])
            self.assertEqual(mapped, qualified)
            for page in pages:
                self.assertEqual([level["level"] for level in page["fallback"]], ["1", "1.5"])
                self.assertTrue(all(level["unicode"] == [90, 90, 32] for level in page["fallback"]))
            self.assertEqual([(call.args[2], call.args[4]) for call in lib.vbi_fetch_vt_page.call_args_list],
                             [(0x124, 2), (0x124, 0), (0x124, 1), (0x125, 2), (0x125, 0), (0x125, 1)])
            verifier.mapped_library.assert_called_once_with(
                Path("/synthetic/build"), "sparse", {"library": "sparse/libzvbi.so.0.13.2", "library_sha256": "c" * 64}, "mapped library")
            lib.vbi_decoder_delete.assert_called_once_with(123)

    def test_fallback_defects_reject_without_mapping_success_and_still_delete_decoder(self):
        for defect in ("fallback_glyph", "fallback_colour", "fallback_palette"):
            with self.subTest(defect=defect):
                lib, _storage = fixture(defect=defect)
                verifier = Mock()
                with patch.object(EDITOR.BASE, "load_decoder", return_value=(lib, "0.2.42")), \
                        patch.object(EDITOR.QUALIFIER, "sparse_verifier", return_value=verifier):
                    with self.assertRaises(EDITOR.BASE.CalibrationError):
                        EDITOR.decode([], "day", {})
                verifier.mapped_library.assert_not_called()
                lib.vbi_decoder_delete.assert_called_once_with(123)

    def test_provenance_and_exact_startup_library_path_precede_decoder_loading(self):
        args = candidate_arguments()
        decoder = {"path": "/synthetic/build/sparse/libzvbi.so.0.13.2", "sha256": "c" * 64}
        with patch.object(EDITOR.QUALIFIER, "qualify", return_value=decoder) as qualify, \
                patch.dict(EDITOR.os.environ, {"LD_LIBRARY_PATH": "/synthetic/build/sparse"}):
            self.assertEqual(EDITOR.candidate(args), decoder)
        qualify.assert_called_once_with(args.decoder_candidate_proof, args.decoder_candidate_proof_sha256,
                                        args.decoder_candidate_build, args.decoder_candidate_proof_root,
                                        args.decoder_candidate_build_sha256)
        for value in ("", "/synthetic/build/original", "/synthetic/build/sparse:/foreign"):
            with self.subTest(path=value), patch.object(EDITOR.QUALIFIER, "qualify", return_value=decoder), \
                    patch.dict(EDITOR.os.environ, {"LD_LIBRARY_PATH": value}):
                with self.assertRaisesRegex(ValueError, "LD_LIBRARY_PATH"):
                    EDITOR.candidate(args)
        argv = [str(SCRIPT), "fresh.json", "day", "--decoder-candidate-proof", "positive.json",
                "--decoder-candidate-proof-sha256", "a" * 64, "--decoder-candidate-build", "build",
                "--decoder-candidate-proof-root", "input", "--decoder-candidate-build-sha256", "b" * 64]
        with patch.object(EDITOR.sys, "argv", argv), patch.object(EDITOR, "captured_units", return_value=(b"[]", [])), \
                patch.object(EDITOR.QUALIFIER, "qualify", side_effect=ValueError("bad provenance")), \
                patch.object(EDITOR.BASE, "load_decoder") as load:
            with self.assertRaisesRegex(ValueError, "bad provenance"):
                EDITOR.main()
            load.assert_not_called()

    def test_fresh_captured_input_is_bounded_strict_and_not_fixed_to_an_old_capture(self):
        unit = [2, 44, 0, 0xE4] + [0] * 42
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "units.json"
            for count in (1, 2):
                raw = json.dumps([unit] * count).encode()
                path.write_bytes(raw)
                self.assertEqual(EDITOR.captured_units(path), (raw, [unit] * count))
            bad = [[], [unit] * 513, {"units": [unit]}, [[True, *unit[1:]]], [unit[:-1]], [[2, 44, 0, 0, *unit[4:]]]]
            for value in bad:
                path.write_text(json.dumps(value))
                with self.subTest(value=str(value)[:80]), self.assertRaises(ValueError):
                    EDITOR.captured_units(path)
            path.write_bytes(b"x" * (EDITOR.BASE.MAX_INPUT + 1))
            with self.assertRaisesRegex(ValueError, "byte bound"):
                EDITOR.captured_units(path)
            link = Path(directory) / "link.json"
            link.symlink_to(path)
            with self.assertRaisesRegex(ValueError, "regular"):
                EDITOR.captured_units(link)

    def test_changed_shared_source_cannot_execute(self):
        with patch.object(Path, "open", return_value=io.BytesIO(b"raise AssertionError('must not execute')")):
            with self.assertRaisesRegex(ValueError, "dependency source changed"):
                EDITOR.frozen("teletext-client-decoder-candidate.py", EDITOR.QUALIFIER_SHA256)


if __name__ == "__main__":
    unittest.main()
