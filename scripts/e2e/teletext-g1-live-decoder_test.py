"""Pure full-vector/known-negative fences; no imager/native/client acceptance."""

import ctypes as C
import hashlib
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock, patch

SCRIPT = Path(__file__).with_name("teletext-g1-live-decoder.py")
SPEC = importlib.util.spec_from_file_location("g1_live_contract", SCRIPT)
G1 = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(G1)

# These independent literal vectors deliberately distinguish the unaffected
# contiguous pair12/13 from both separated pairs at the base level.
VECTORS = {
    "1.5": {(1, 8): [127, 25, 0, 7, 0], (1, 9): [127, 25, 0, 7, 0],
            (1, 12): [127, 24, 0, 7, 0], (1, 13): [127, 24, 0, 7, 0],
            (1, 20): [127, 25, 0, 7, 0], (1, 21): [127, 25, 0, 7, 0], (2, 1): [66, 15, 0, 7, 0]},
    "2.5": {(1, 8): [127, 24, 0, 7, 0], (1, 9): [127, 24, 0, 7, 0],
            (1, 12): [127, 24, 0, 7, 0], (1, 13): [127, 24, 0, 7, 0],
            (1, 20): [127, 25, 0, 7, 0], (1, 21): [127, 25, 0, 7, 0], (2, 1): [66, 15, 0, 7, 0]},
}
HAM = (0x15, 0x02, 0x49, 0x5E, 0x64, 0x73, 0x38, 0x2F,
       0xD0, 0xC7, 0x8C, 0x9B, 0xA1, 0xB6, 0xFD, 0xEA)


class Decoder:
    def decode(self, page, level):
        self.page, self.level = page, level

    def values(self, row, column):
        value = list(VECTORS[self.level][row, column])
        change = self.page.get("change")
        if change and (self.level, row, column) == change[:3]:
            value[change[3]] = change[4]
        return value

    def get_drcs_ptu(self, row, column):
        return 1 if self.page.get("alias") == (self.level, row, column) else None

    def get_char_code(self, row, column):
        return self.values(row, column)[0]

    def get_char_set(self, row, column):
        return self.values(row, column)[1]

    def get_char_diacritic(self, row, column):
        return self.values(row, column)[2]

    def get_foreground(self, row, column):
        return self.values(row, column)[3]

    def get_background(self, row, column):
        return self.values(row, column)[4]


def unit():
    return [2, 44, 0, 0xE4] + [0] * 42


def header(nibbles=None):
    nibbles = [1, 0, 4, 2, 0, 2, 0, 0, 0, 0] if nibbles is None else nibbles
    return [2, 44, 0, 0xE4] + [G1.BASE.BIT_REVERSE[HAM[n]] for n in nibbles] + [0] * 32


def stock_fixture(defect=None):
    lib = Mock()

    def fetch(_decoder, pointer, number, _subcode, level, *_rest):
        page = pointer._obj
        page.pgno, page.subno, page.rows, page.columns = number, 32, 25, 41
        page._level = level
        page.color_map[7], page.color_map[0] = 0xFFFFFFFF, 0xFF000000
        for column in (8, 9, 12, 13, 20, 21):
            separated = level == 1 and (column < 12 or column >= 20)
            if defect == "normative_fixed" and column >= 20:
                separated = True
            cell = page.text[41 + column]
            cell.bits1 = (7 << 24) | (3 << 16)
            cell.bits2 = (0xEE5F if separated else 0xEE7F) << 16
            if column == 20 and level == 2:
                if defect == "glyph":
                    cell.bits2 = 65 << 16
                elif defect == "palette":
                    cell.bits1 = (1 << 24) | (3 << 16)
                elif defect == "opaque":
                    cell.bits1 = 7 << 24
        page.text[83].bits1 = (7 << 24) | (3 << 16)
        page.text[83].bits2 = 66 << 16
        if defect == "B":
            page.text[83].bits2 = 65 << 16
        elif defect == "B_opaque":
            page.text[83].bits1 = 7 << 24
        return 1

    def draw(pointer, _format, canvas, _stride, column, *_rest):
        page = pointer._obj
        separated = page._level == 1 and (column < 12 or column >= 20)
        rows = [4030, 4030, 0, 4030, 4030, 4030, 0, 4030, 4030, 0] if separated else [4095] * 10
        pixels = [component for row in rows for x in range(12)
                  for component in ([255, 255, 255, 255] if row & (1 << (11 - x)) else [0, 0, 0, 255])]
        if defect == "pixels" and column == 20 and page._level == 2:
            pixels[0] = 0
        C.cast(canvas, C.POINTER(C.c_uint8 * 480)).contents[:] = pixels

    lib.vbi_fetch_vt_page.side_effect = fetch
    lib.vbi_draw_vt_page_region.side_effect = draw
    return lib


class G1Tests(unittest.TestCase):
    def test_complete_imager_vectors_and_aliases_on_both_pages(self):
        for number in (0x124, 0x125):
            result = G1.check_imager_page(Decoder, {"number": number, "subcode": 32})
            for level in result["levels"]:
                self.assertEqual({(cell["row"], cell["column"]): cell["tuple"] for cell in level["cells"]}, VECTORS[level["level"]])
            for level in VECTORS:
                for position in VECTORS[level]:
                    for index, wrong in ((0, 65), (1, 0), (2, 1), (3, 1), (4, 1)):
                        page = {"number": number, "subcode": 32, "change": (level, *position, index, wrong)}
                        with self.subTest(page=number, level=level, position=position, index=index), self.assertRaises(ValueError):
                            G1.check_imager_page(Decoder, page)
                    with self.assertRaisesRegex(ValueError, "DRCS alias"):
                        G1.check_imager_page(Decoder, {"number": number, "subcode": 32, "alias": (level, *position)})
        for page in ({"number": 0x126, "subcode": 32}, {"number": 0x124, "subcode": 0}, {"number": True, "subcode": 32}):
            with self.assertRaisesRegex(ValueError, "Unexpected imager"):
                G1.check_imager_page(Decoder, page)

    def test_all_repeated_acquisitions_are_checked_without_reading_prefetched_eof_page(self):
        acquisitions = [{"number": 0x124, "subcode": 32}, {"number": 0x125, "subcode": 32},
                        {"number": 0x124, "subcode": 32}]

        class Reader:
            reads = 0

            def read(self, stream):
                value = acquisitions[self.reads]
                self.reads += 1
                stream.read(42)
                return [value]

        units = [unit()] * 3
        for bad_duplicate in (False, True):
            if bad_duplicate:
                acquisitions[2] = {**acquisitions[2], "change": ("2.5", 1, 21, 1, 24)}
            with patch.object(G1.COMPOSITION.imager, "load_decoder", return_value=(Reader, Decoder)), \
                    patch.object(G1, "terminal_header", return_value={"terminal": True}), \
                    patch.dict(G1.os.environ, {G1.IMAGER_ENV: "retained-value"}):
                # The real pinned reader exposes its authenticated Hamming module
                # in read.__globals__; the terminal parser is tested separately.
                Reader.read.__globals__["hamming_8_4"] = None
                if bad_duplicate:
                    with self.assertRaisesRegex(ValueError, "cell1,21"):
                        G1.imager_decode(units, Path("/synthetic/imager"))
                else:
                    result = G1.imager_decode(units, Path("/synthetic/imager"))
                    self.assertEqual(result["acquisition_counts"], {"124": 2, "125": 1})
                    self.assertEqual(len(result["pages"]), 3)
                self.assertEqual(G1.os.environ[G1.IMAGER_ENV], "retained-value")

    def test_terminal_real_header_is_required_without_synthesizing_or_filtering(self):
        hamming = Mock()
        hamming.decode.side_effect = lambda byte: HAM.index(byte) if byte in HAM else 255
        self.assertEqual(G1.terminal_header([header()], hamming)["raw_page"], 0x124)
        for index, value in ((0, 9), (2, 6), (5, 0)):
            n = [1, 0, 4, 2, 0, 2, 0, 0, 0, 0]
            n[index] = value
            with self.subTest(nibbles=n), self.assertRaisesRegex(ValueError, "trailing partial"):
                G1.terminal_header([header(n)], hamming)
        bad = header()
        bad[4] = 0
        with self.assertRaisesRegex(ValueError, "valid Hamming"):
            G1.terminal_header([bad], hamming)

    def test_stock_exact_known_negative_and_observed_pixels_are_both_mandatory(self):
        for number in (0x124, 0x125):
            for level in (1, 2):
                lib = stock_fixture()
                report = G1.stock_page(lib, 123, number, level)
                failures = [cell for cell in report["cells"] if cell["normative_error"]]
                self.assertEqual([cell["column"] for cell in failures], [20, 21] if level == 2 else [])
                self.assertTrue(all(cell["normative"] == "separated" and cell["observed"] == "contiguous"
                                    and cell["normative_error"] == "decoded full G1 mosaic is incorrect" for cell in failures))
                self.assertEqual(lib.vbi_draw_vt_page_region.call_count, 6)
                lib.vbi_unref_page.assert_called_once()
        for defect in ("normative_fixed", "glyph", "palette", "opaque", "pixels", "B", "B_opaque"):
            lib = stock_fixture(defect)
            with self.subTest(defect=defect), self.assertRaises((ValueError, G1.BASE.CalibrationError)):
                G1.stock_page(lib, 123, 0x124, 2)
            lib.vbi_unref_page.assert_called_once()

    def test_stock_mapping_hash_path_inode_and_no_ld_override(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "libzvbi.so.0.13.2"
            path.write_bytes(b"stock")
            stat = path.stat()
            line = f"1000-2000 r-xp 0 {G1.os.major(stat.st_dev):x}:{G1.os.minor(stat.st_dev):x} {stat.st_ino} {path}\n"
            with patch.object(G1, "STOCK", path), patch.object(G1, "STOCK_SHA256", hashlib.sha256(b"stock").hexdigest()):
                self.assertTrue(G1.stock_mapping(line)["mapped_file_identity_verified"])
                for mapping in ("", line + "2000-3000 r-xp 0 0:1 2 /foreign/libzvbi.so.0.13.2\n", line.replace(f" {stat.st_ino} ", " 1 ")):
                    with self.assertRaises(ValueError):
                        G1.stock_mapping(mapping)
                path.write_bytes(b"modified")
                with self.assertRaisesRegex(ValueError, "hash changed"):
                    G1.stock_mapping(line)
        with patch.dict(G1.os.environ, {"LD_LIBRARY_PATH": "/alternate"}), patch.object(G1.BASE, "load_decoder") as load:
            with self.assertRaisesRegex(ValueError, "LD_LIBRARY_PATH"):
                G1.stock_decode([])
            load.assert_not_called()

    def test_malformed_units_and_changed_helper_cannot_reach_decoder(self):
        import json
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "units.json"
            for value in ([], [unit()] * 513, {"units": [unit()]}, [[True, *unit()[1:]]], [unit()[:-1]]):
                path.write_text(json.dumps(value))
                with self.assertRaises(ValueError):
                    G1.captured_units(path)
            path.write_bytes(b"x" * (G1.BASE.MAX_INPUT + 1))
            with self.assertRaisesRegex(ValueError, "byte bound"):
                G1.captured_units(path)
        with patch.object(G1, "bounded", return_value=b"raise AssertionError('not executed')"):
            with self.assertRaisesRegex(ValueError, "source changed"):
                G1.frozen_helpers()


if __name__ == "__main__":
    unittest.main()
