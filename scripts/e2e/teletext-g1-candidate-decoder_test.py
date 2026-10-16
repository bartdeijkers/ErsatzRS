"""Pure candidate receipt/vector regressions; no native/client acceptance."""

import copy
import ctypes as C
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock, patch

SCRIPT = Path(__file__).with_name("teletext-g1-candidate-decoder.py")
SPEC = importlib.util.spec_from_file_location("g1_candidate_contract", SCRIPT)
G1 = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(G1)


def sha(body):
    return hashlib.sha256(body).hexdigest()


def native_fixture(defect=None):
    lib = Mock()
    lib.vbi_decoder_new.return_value = 123
    lib.vbi_event_handler_register.return_value = 1

    def fetch(_decoder, pointer, number, _subcode, level, *_rest):
        page = pointer._obj
        page.pgno, page.subno, page.rows, page.columns = number, 32, 25, 41
        page._level = level
        page.color_map[7], page.color_map[0] = 0xFFFFFFFF, 0xFF000000
        for column in (8, 9, 12, 13, 20, 21):
            separated = column in (20, 21) or level == 1 and column in (8, 9)
            cell = page.text[41 + column]
            cell.bits1 = (7 << 24) | (3 << 16)
            cell.bits2 = (0xEE5F if separated else 0xEE7F) << 16
            if level == 2 and column >= 20 and defect == "stock":
                cell.bits2 = 0xEE7F << 16
            if column == 21 and defect == "last_alias":
                cell.bits2 = 65 << 16
        page.text[83].bits1 = (7 << 24) | (3 << 16)
        page.text[83].bits2 = 66 << 16
        if defect == "B":
            page.text[83].bits2 = 65 << 16
        elif defect == "B_opaque":
            page.text[83].bits1 = 7 << 24
        elif defect == "palette":
            page.color_map[7] = 0xFF0000FF
        elif defect == "identity":
            page.pgno += 1
        return 1

    def draw(pointer, _format, canvas, _stride, column, *_rest):
        separated = column >= 20 or pointer._obj._level == 1 and column < 12
        rows = [4030, 4030, 0, 4030, 4030, 4030, 0, 4030, 4030, 0] if separated else [4095] * 10
        pixels = [component for row in rows for x in range(12)
                  for component in ([255, 255, 255, 255] if row & (1 << (11 - x)) else [0, 0, 0, 255])]
        if defect == "pixels" and column == 21:
            pixels[0] ^= 255
        C.cast(canvas, C.POINTER(C.c_uint8 * 480)).contents[:] = pixels

    lib.vbi_fetch_vt_page.side_effect = fetch
    lib.vbi_draw_vt_page_region.side_effect = draw
    return lib


class BuildFixture:
    def __init__(self, root):
        self.build, self.proof = root / "build", root / "input"
        self.build.mkdir()
        self.proof.mkdir()
        self.base_raw = b"retained base receipt"
        self.sparse = {"source": {**G1.SPARSE.SOURCE, "tag": "v0.2.42"},
                       "base_tool_versions_sha256": sha(b"base tools"), "tool_versions_sha256": sha(b"sparse tools")}
        self.sparse_raw = json.dumps(self.sparse).encode()
        self.entry = {"label": G1.LABEL, "packet_sha256": G1.SPARSE.PARSERS["sparse"],
                      "formatter_sha256": G1.FORMATTER_SHA256, "library": "g1/libzvbi.so.0.13.2",
                      "library_sha256": sha(b"candidate library"), "configure_log_sha256": sha(b"configure"),
                      "bootstrap_log_sha256": sha(b"bootstrap"), "build_log_sha256": sha(b"build")}
        self.recipes = {**G1.SPARSE.RECIPES, "g1": {"path": "scripts/teletext-zvbi-g1-build.sh", "sha256": G1.RECIPE_SHA256}}
        self.receipt = {"schema": 1, "source": self.sparse["source"], "requested_image": G1.SPARSE.IMAGE,
                        "base_receipt": {"path": "proof-build.json", "sha256": sha(self.base_raw)},
                        "sparse_receipt": {"path": "proof-sparse-build.json", "sha256": sha(self.sparse_raw)},
                        "recipes": self.recipes, "original_formatter_sha256": G1.SPARSE.FORMATTERS["original"],
                        "sparse_formatter_sha256": G1.SPARSE.FORMATTERS["sparse"],
                        "preserved_outputs": {"retained.txt": {"sha256": sha(b"retained")}},
                        "configuration": G1.SPARSE.CONFIGURATION, "base_tool_versions_sha256": self.sparse["base_tool_versions_sha256"],
                        "sparse_tool_versions_sha256": self.sparse["tool_versions_sha256"], "tool_versions_sha256": sha(b"g1 tools"),
                        "variants": {"g1": self.entry}, "limitations": copy.deepcopy(G1.LIMITATIONS)}
        self.hashes = {"scripts/teletext-zvbi-g1-build.sh": G1.RECIPE_SHA256,
                       "g1-tool-versions.log": sha(b"g1 tools"), "retained.txt": sha(b"retained"),
                       "g1/packet.c": G1.SPARSE.PARSERS["sparse"], "g1/teletext.c": G1.FORMATTER_SHA256,
                       "g1/COPYING.md": G1.SPARSE.SOURCE["copying_sha256"],
                       "g1/libzvbi.so.0.13.2": self.entry["library_sha256"],
                       "g1/config-details.log": self.entry["configure_log_sha256"],
                       "g1/bootstrap.log": self.entry["bootstrap_log_sha256"], "g1/build.log": self.entry["build_log_sha256"]}

    def verify(self, external=None):
        raw = json.dumps(self.receipt).encode()
        (self.build / "proof-g1-build.json").write_bytes(raw)
        def digest(path, *_limits):
            for root in (self.build, self.proof):
                if path.is_relative_to(root):
                    return self.hashes[str(path.relative_to(root))]
            raise AssertionError("unexpected file")
        with patch.object(G1.SPARSE, "verify_build", return_value=(self.base_raw, self.sparse_raw, {})) as chain, \
                patch.object(G1.SPARSE, "digest", side_effect=digest):
            result = G1.verify_build(self.build, self.proof, sha(raw) if external is None else external)
        chain.assert_called_once_with(self.build, self.proof, sha(self.sparse_raw))
        return result


class CandidateTests(unittest.TestCase):
    def test_positive_normative_vectors_include_later_separation_and_both_aliases(self):
        expected = G1.expected_report()
        self.assertEqual([(page["raw_page"], page["level"]) for page in expected],
                         [(0x124, "1.5"), (0x124, "2.5"), (0x125, "1.5"), (0x125, "2.5")])
        lib = native_fixture()
        for page in expected:
            self.assertEqual([cell["column"] for cell in page["cells"]], [8, 9, 12, 13, 20, 21])
            self.assertTrue(all(cell["pattern"] == "separated" and cell["unicode"] == 0xEE5F for cell in page["cells"][-2:]))
            self.assertEqual([cell["pattern"] for cell in page["cells"][:4]],
                             ["separated", "separated", "contiguous", "contiguous"] if page["level"] == "1.5" else ["contiguous"] * 4)
            G1.check_page(lib, 123, page)
        self.assertEqual(lib.vbi_draw_vt_page_region.call_count, 24)
        self.assertEqual(lib.vbi_unref_page.call_count, 4)

    def test_stock_known_negative_and_wrong_glyph_pixels_control_cannot_claim_positive(self):
        for defect in ("stock", "last_alias", "pixels", "B", "B_opaque", "palette", "identity"):
            with self.subTest(defect=defect):
                lib = native_fixture(defect)
                with self.assertRaises((ValueError, G1.BASE.CalibrationError)):
                    G1.check_page(lib, 123, G1.expected_report()[1])
                lib.vbi_unref_page.assert_called_once()

    def test_external_receipt_chain_and_known_recipe_and_library_pins_are_required(self):
        with tempfile.TemporaryDirectory() as directory:
            fixture = BuildFixture(Path(directory))
            raw, entry = fixture.verify()
            self.assertEqual(json.loads(raw), fixture.receipt)
            self.assertEqual(entry, fixture.entry)
            with self.assertRaisesRegex(ValueError, "Externally pinned"):
                fixture.verify("0" * 64)
        changes = [lambda f: f.receipt.update(schema=True), lambda f: f.receipt.update(extra=True),
                   lambda f: f.receipt.update(base_receipt={"path": "other", "sha256": "a" * 64}),
                   lambda f: f.entry.update(library="sparse/libzvbi.so.0.13.2"),
                   lambda f: f.entry.update(formatter_sha256="0" * 64),
                   lambda f: f.receipt.update(recipes={}), lambda f: f.receipt["limitations"].update(full_client_acceptance=True),
                   lambda f: f.hashes.update({"g1/libzvbi.so.0.13.2": "0" * 64}),
                   lambda f: f.hashes.update({"retained.txt": "0" * 64}),
                   lambda f: f.receipt.update(preserved_outputs={"../outside": {"sha256": "0" * 64}})]
        for index, change in enumerate(changes):
            with self.subTest(change=index), tempfile.TemporaryDirectory() as directory:
                fixture = BuildFixture(Path(directory))
                change(fixture)
                with self.assertRaises(ValueError):
                    fixture.verify()

    def test_mapping_requires_exact_sole_library_hash_and_inode(self):
        with tempfile.TemporaryDirectory(prefix="candidate mapping ") as directory:
            library = Path(directory) / "libzvbi.so.0.13.2"
            library.write_bytes(b"candidate")
            stat = library.stat()
            path = str(library).replace(" ", r"\040")
            line = f"1000-2000 r-xp 0 {G1.os.major(stat.st_dev):x}:{G1.os.minor(stat.st_dev):x} {stat.st_ino} {path}\n"
            self.assertTrue(G1.mapped_library(library, sha(b"candidate"), line)["mapped_file_identity_verified"])
            for maps in ("", line + "2000-3000 r-xp 0 0:1 2 /stock/libzvbi.so.0.13.2\n", line.replace(f" {stat.st_ino} ", " 1 ")):
                with self.assertRaises(ValueError):
                    G1.mapped_library(library, sha(b"candidate"), maps)
            with self.assertRaisesRegex(ValueError, "content changed"):
                G1.mapped_library(library, "0" * 64, line)

    def test_wrong_ld_path_prevents_native_load_and_success_checks_actual_mapping(self):
        library = Path("/synthetic/build/g1/libzvbi.so.0.13.2")
        with patch.dict(G1.os.environ, {"LD_LIBRARY_PATH": "/stock"}), patch.object(G1.BASE, "load_decoder") as load:
            with self.assertRaisesRegex(ValueError, "LD_LIBRARY_PATH"):
                G1.decode([], library, "a" * 64)
            load.assert_not_called()
        lib = native_fixture()
        units = [[2, 44, 0, 0xE4] + list(range(42))]
        with patch.dict(G1.os.environ, {"LD_LIBRARY_PATH": str(library.parent)}), \
                patch.object(G1.BASE, "load_decoder", return_value=(lib, "0.2.42")), \
                patch.object(G1.LIVE, "bounded", return_value=b"mapped"), \
                patch.object(G1, "mapped_library", side_effect=ValueError("wrong mapped decoder")) as mapping:
            with self.assertRaisesRegex(ValueError, "wrong mapped"):
                G1.decode(units, library, "a" * 64)
        mapping.assert_called_once_with(library, "a" * 64, "mapped")
        self.assertEqual(lib.vbi_decode.call_count, 1)
        sliced = lib.vbi_decode.call_args.args[1]._obj
        self.assertEqual(bytes(sliced.data[:42]), bytes(G1.BASE.BIT_REVERSE[value] for value in units[0][4:]))
        lib.vbi_decoder_delete.assert_called_once_with(123)

    def test_shared_sources_and_captured_input_remain_strict(self):
        with patch.object(Path, "open", return_value=__import__("io").BytesIO(b"raise AssertionError('not executed')")):
            with self.assertRaisesRegex(ValueError, "dependency changed"):
                G1.frozen("teletext-g1-live-decoder.py", G1.LIVE_SHA256)
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "units.json"
            for value in ([], [[2, 44, 0, 0xE4] + [False] * 42], [[2, 44, 0, 0xE4] + [0] * 41], {"units": []}):
                path.write_text(json.dumps(value))
                with self.assertRaises(ValueError):
                    G1.LIVE.captured_units(path)


if __name__ == "__main__":
    unittest.main()
