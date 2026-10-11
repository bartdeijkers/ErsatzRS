"""Pure closed-input/ABI lifetime regressions; no native decoder is loaded."""

import importlib.util
import io
import json
import marshal
from pathlib import Path
import struct
import tempfile
import types
import unittest
from unittest.mock import Mock, patch

SCRIPT = Path(__file__).with_name("teletext-magazine-cache-decoder.py")
CACHE = types.ModuleType("teletext_magazine_contract")
CACHE.__file__ = str(SCRIPT)
exec(compile(SCRIPT.read_bytes(), str(SCRIPT), "exec"), CACHE.__dict__)

# Exact synthetic captured-packet fixtures, not expected values supplied by a producer.
SEED = bytes.fromhex(
    "022ce0e431bfa8c0111fbc0208a8ff0007faff2a03acb101d4983e768633fc7deff66ffadcaeff7ea6fe77b70100"
)

TARGET = bytes.fromhex(
    "022ce0e492a8a8a826d99240a8e30404040404040404040404040404040404040404040404040404040404040404"
    "022ce0e4926da82348115262e0234811833cc5934300d37391aeff60c36ae0112582427b91d100e0013101903901"
    "022ce0e4926d40110d000004811125826effa06effa06effa06effa06effa06effa06effa06effa06effa06effa0"
    "022ce0e431a80404040404040404045b040404040404040404040404040404040404040404040404040404040404"
    "022ce0e4924004430404040404040404040404040404040404040404040404040404040404040404040404040404"
    "022ce0e4314004040404040404040404040404040404040404040404040404040404040404040404040404040404"
    "022ce0e4929204040404040404040404040404040404040404040404040404040404040404040404040404040404"
    "022ce0e4319204040404040404040404040404040404040404040404040404040404040404040404040404040404"
    "022ce0e4927a04040404040404040404040404040404040404040404040404040404040404040404040404040404"
    "022ce0e4317a04040404040404040404040404040404040404040404040404040404040404040404040404040404"
    "022ce0e4922604040404040404040404040404040404040404040404040404040404040404040404040404040404"
    "022ce0e4312604040404040404040404040404040404040404040404040404040404040404040404040404040404"
    "022ce0e492ce04040404040404040404040404040404040404040404040404040404040404040404040404040404"
    "022ce0e431ce04040404040404040404040404040404040404040404040404040404040404040404040404040404"
    "022ce0e4921c04040404040404040404040404040404040404040404040404040404040404040404040404040404"
    "022ce0e4311c04040404040404040404040404040404040404040404040404040404040404040404040404040404"
    "022ce0e492f404040404040404040404040404040404040404040404040404040404040404040404040404040404"
    "022ce0e431f404040404040404040404040404040404040404040404040404040404040404040404040404040404"
    "022ce0e4920b04040404040404040404040404040404040404040404040404040404040404040404040404040404"
    "022ce0e4310b04040404040404040404040404040404040404040404040404040404040404040404040404040404"
    "022ce0e492e304040404040404040404040404040404040404040404040404040404040404040404040404040404"
    "022ce0e431e304040404040404040404040404040404040404040404040404040404040404040404040404040404"
    "022ce0e4923104040404040404040404040404040404040404040404040404040404040404040404040404040404"
    "022ce0e4313104040404040404040404040404040404040404040404040404040404040404040404040404040404"
    "022ce0e492d904040404040404040404040404040404040404040404040404040404040404040404040404040404"
    "022ce0e431d904040404040404040404040404040404040404040404040404040404040404040404040404040404"
    "022ce0e4928504040404040404040404040404040404040404040404040404040404040404040404040404040404"
    "022ce0e492a85757a8a8a8a840400404040404040404040404040404040404040404040404040404040404040404"
)


def request():
    def units(raw):
        return [list(raw[index:index + 46]) for index in range(0, len(raw), 46)]
    return {"expected_version": "0.2.42", "seed_units": units(SEED), "target_units": units(TARGET)}


class ContractTests(unittest.TestCase):
    def rejects_before_loading(self, value):
        self.rejects_raw_before_loading(json.dumps(value).encode())

    def rejects_raw_before_loading(self, raw):
        stream = io.TextIOWrapper(io.BytesIO(raw))
        with patch.object(CACHE.BASE, "load_decoder") as loader:
            with patch.object(CACHE.sys, "stdin", stream):
                with self.assertRaises(CACHE.BASE.CalibrationError):
                    CACHE.main()
            loader.assert_not_called()

    def test_exact_captured_pair_and_independent_palette_literals(self):
        value = request()
        self.assertEqual(CACHE.validate(value), (value["seed_units"], value["target_units"]))
        self.assertEqual(CACHE.address(value["seed_units"][0]), (2, 29))
        self.assertEqual(CACHE.header(value["target_units"][0]), (2, 0, 0x1234, 1))
        self.assertEqual(CACHE.header(value["target_units"][-1]), (2, 0xFF, 0, 0))
        self.assertFalse(any(CACHE.address(unit)[1] in (28, 29) for unit in value["target_units"]))
        self.assertEqual(CACHE.EXPECTED_COLOURS, (0xFF5500FF, 0xFF332211, 0xFF332211))
        self.assertEqual(CACHE.rgb(CACHE.EXPECTED_COLOURS[0]), [255, 0, 85])
        self.assertEqual(CACHE.rgb(CACHE.EXPECTED_COLOURS[1]), [17, 34, 51])

    def test_closed_schema_has_no_supplied_oracle_or_reset(self):
        for key in ("reset", "expected_pixels", "expected_colour", "stages"):
            value = request()
            value[key] = False
            self.rejects_before_loading(value)
        for version in ("0.2.45", 42, True):
            value = request()
            value["expected_version"] = version
            self.rejects_before_loading(value)
        value = request()
        del value["seed_units"]
        self.rejects_before_loading(value)

    def test_exact_counts_bytes_framing_and_digests_precede_loading(self):
        for field in ("seed_units", "target_units"):
            for defect in ("empty", "extra", "bool", "framing", "truncated", "changed"):
                value = request()
                units = value[field]
                if defect == "empty":
                    units.clear()
                elif defect == "extra":
                    units.append(units[0])
                elif defect == "bool":
                    units[0][0] = True
                elif defect == "framing":
                    units[0][3] = 0
                elif defect == "truncated":
                    units[0].pop()
                else:
                    units[0][-1] ^= 1
                with self.subTest(field=field, defect=defect):
                    self.rejects_before_loading(value)
        value = request()
        value["target_units"][0], value["target_units"][1] = value["target_units"][1], value["target_units"][0]
        self.rejects_before_loading(value)

    def test_duplicate_json_and_input_bound_precede_loading(self):
        for raw in (b'{"expected_version":"0.2.42","expected_version":"0.2.42"}',
                    b"x" * (CACHE.BASE.MAX_INPUT + 1), b""):
            self.rejects_raw_before_loading(raw)

    def test_same_hashed_source_is_executed_despite_valid_cached_foreign_bytecode(self):
        source = Path(CACHE.BASE.__file__).read_bytes()
        with tempfile.TemporaryDirectory() as directory:
            helper = Path(directory) / "teletext-resource-decoder.py"
            helper.write_bytes(source)
            cached = Path(importlib.util.cache_from_source(str(helper)))
            cached.parent.mkdir()
            cached.write_bytes(importlib.util.MAGIC_NUMBER
                               + struct.pack("<III", 0, int(helper.stat().st_mtime), len(source))
                               + marshal.dumps(compile("CACHED_SENTINEL = True", str(helper), "exec")))
            spec = importlib.util.spec_from_file_location("cached_control", helper)
            control = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(control)
            self.assertTrue(control.CACHED_SENTINEL)
            actual = CACHE._load_frozen_helper(helper)
            self.assertFalse(hasattr(actual, "CACHED_SENTINEL"))
            self.assertTrue(callable(actual.load_decoder))
            self.assertEqual(actual.__name__, "teletext_magazine_resource_abi")
            self.assertEqual(actual.__file__, str(helper))

    def test_changed_or_oversize_abi_source_is_rejected_before_compile(self):
        with tempfile.TemporaryDirectory() as directory:
            helper = Path(directory) / "teletext-resource-decoder.py"
            for source in (Path(CACHE.BASE.__file__).read_bytes() + b"\n", b"x" * 65537):
                helper.write_bytes(source)
                with patch.object(CACHE, "compile", create=True) as compiler:
                    with self.assertRaisesRegex(RuntimeError, "ABI helper changed"):
                        CACHE._load_frozen_helper(helper)
                    compiler.assert_not_called()

    def test_one_decoder_three_checkpoints_and_85_monotonic_ticks_without_reset(self):
        lib = Mock()
        lib.vbi_decoder_new.return_value = 123
        lib.vbi_event_handler_register.return_value = 1
        seed, target = CACHE.validate(request())
        observed = [{"colour_map16": raw, "rgb": CACHE.rgb(raw)} for raw in CACHE.EXPECTED_COLOURS]
        with patch.object(CACHE, "check_page", side_effect=observed) as check:
            self.assertEqual(CACHE.decode_trial(lib, seed, target), observed)
        lib.vbi_decoder_new.assert_called_once_with()
        lib.vbi_decoder_delete.assert_called_once_with(123)
        lib.vbi_teletext_set_level.assert_called_once_with(123, 2)
        self.assertEqual([call.args for call in check.call_args_list], [(lib, 123, index) for index in range(3)])
        times = [call.args[3] for call in lib.vbi_decode.call_args_list]
        self.assertEqual(len(times), 85)
        self.assertTrue(all(before < after for before, after in zip(times, times[1:])))
        # Packet bytes fed to the native ABI are exactly target, seed+target, target.
        actual = [bytes(call.args[1]._obj.data[:42]) for call in lib.vbi_decode.call_args_list]
        expected = [bytes(CACHE.BASE.BIT_REVERSE[byte] for byte in unit[4:])
                    for unit in target + seed + target + target]
        self.assertEqual(actual, expected)

    def test_fetch_checks_only_fixed_baseline_and_palette_and_always_unrefs(self):
        for defect in (None, "glyph", "colour", "palette", "geometry"):
            lib = Mock()
            def fetch(_decoder, pointer, *args):
                page = pointer._obj
                page.pgno, page.subno, page.rows, page.columns = 0x200, 0x1234, 25, 41
                for row, column, unicode in ((1, 9, 90), (2, 1, 66)):
                    page.text[row * 41 + column].bits1 = 7 << 24
                    page.text[row * 41 + column].bits2 = unicode << 16
                # The unrelated enhanced cell is deliberately not an oracle here.
                page.text[41 + 8].bits2 = 163 << 16
                page.color_map[16] = 0xFF332211
                if defect == "glyph":
                    page.text[2 * 41 + 1].bits2 = 65 << 16
                elif defect == "colour":
                    page.text[41 + 9].bits1 = 1 << 24
                elif defect == "palette":
                    page.color_map[16] = 0xFF5500FF
                elif defect == "geometry":
                    page.subno = 0
                return 1
            lib.vbi_fetch_vt_page.side_effect = fetch
            if defect is None:
                self.assertEqual(CACHE.check_page(lib, 123, 2),
                                 {"colour_map16": 0xFF332211, "rgb": [17, 34, 51]})
            else:
                with self.assertRaises(CACHE.BASE.CalibrationError):
                    CACHE.check_page(lib, 123, 2)
            lib.vbi_fetch_vt_page.assert_called_once()
            self.assertEqual(lib.vbi_fetch_vt_page.call_args.args[2:], (0x200, 0x1234, 2, 25, 0))
            lib.vbi_unref_page.assert_called_once()

    def test_failed_checkpoint_deletes_decoder_without_later_feeds(self):
        lib = Mock()
        lib.vbi_decoder_new.return_value = 123
        lib.vbi_event_handler_register.return_value = 1
        seed, target = CACHE.validate(request())
        with patch.object(CACHE, "check_page", side_effect=CACHE.BASE.CalibrationError("literal failure")):
            with self.assertRaises(CACHE.BASE.CalibrationError):
                CACHE.decode_trial(lib, seed, target)
        self.assertEqual(lib.vbi_decode.call_count, 28)
        lib.vbi_decoder_delete.assert_called_once_with(123)


if __name__ == "__main__":
    unittest.main()
