"""Pure warm-leaf qualification; frozen literals are reused, not redefined."""

import hashlib
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import types
import unittest
from unittest.mock import Mock, patch

SCRIPT = Path(__file__).with_name("teletext-editor-bank-warm-decoder.py")
BANK = types.ModuleType("bank_warm_contract")
BANK.__file__ = str(SCRIPT)
exec(compile(SCRIPT.read_bytes(), str(SCRIPT), "exec"), BANK.__dict__)


def unit(value=0):
    return [2, 44, 0, 0xE4] + [value] * 42


class BankWarmTests(unittest.TestCase):
    def test_original_source_and_all_literal_entrypoints_remain_frozen(self):
        path = SCRIPT.with_name("teletext-editor-drcs-decoder.py")
        self.assertEqual(hashlib.sha256(path.read_bytes()).hexdigest(), BANK.ORACLE_SHA256)
        self.assertEqual(BANK.ORACLE_SHA256, "fe250705ddc7665a5012d2bb207d1702cfd3bf765716b0c1d4022265d1959a47")
        for name in ("check_page", "check_indexed_fallback", "warm_suffix", "feed", "main"):
            self.assertTrue(callable(getattr(BANK.ORACLE, name)))
        self.assertEqual(BANK.ORACLE.PATTERNS[0][0], 0x801)
        self.assertEqual(BANK.ORACLE.PATTERNS[1], (0x801, 1, 2, 4, 8, 16, 32, 64, 128, 0x900))

    def test_one_lifetime_exact_feed_order_aliases_fallback_and_only_day_zero_changes(self):
        with tempfile.TemporaryDirectory() as directory:
            prefix, suffix = Path(directory) / "prefix.json", Path(directory) / "suffix.json"
            raw, next_raw = json.dumps([unit(1), unit(2)]).encode(), json.dumps([unit(3), unit(4), unit(5)]).encode()
            prefix.write_bytes(raw)
            suffix.write_bytes(next_raw)
            for theme in ("day", "night"):
                lib, events = Mock(), []
                lib.vbi_decoder_new.return_value = 123
                lib.vbi_event_handler_register.return_value = 1
                lib.vbi_decode.side_effect = lambda *_args: events.append("feed")
                output = io.StringIO()

                def glyph(_lib, _decoder, number, colour, subpage, edited, *, columns):
                    events.append(("glyph", number, subpage, edited))
                    return {"raw_page": number, "logical_subpage": subpage, "unicode": 0xF400 + (subpage << 6)}

                def fallback(_lib, _decoder, number):
                    events.append(("fallback", number))
                    return [{"level": "1"}, {"level": "1.5"}]

                with patch.object(BANK.sys, "argv", [str(SCRIPT), str(prefix), theme, "indexed-warm-edited", str(suffix)]), \
                        patch.object(BANK.base, "load_decoder", return_value=(lib, "0.2.42")), \
                        patch.object(BANK.ORACLE, "check_page", side_effect=glyph) as check, \
                        patch.object(BANK.ORACLE, "check_indexed_fallback", side_effect=fallback), \
                        patch.object(BANK.sys, "stdout", output):
                    BANK.main()
                expected = ["feed"] * 2 + [("glyph", 0x124, 0, False), ("fallback", 0x124),
                    ("glyph", 0x125, 1, False), ("fallback", 0x125)] + ["feed"] * 3 + [
                    ("glyph", 0x124, 0, theme == "day"), ("fallback", 0x124),
                    ("glyph", 0x125, 1, False), ("fallback", 0x125)]
                self.assertEqual(events, expected)
                self.assertTrue(all(call.kwargs == {"columns": (8, 10)} for call in check.call_args_list))
                lib.vbi_decoder_new.assert_called_once()
                lib.vbi_decoder_delete.assert_called_once_with(123)
                lib.vbi_teletext_set_level.assert_called_once_with(123, 2)
                times = [call.args[3] for call in lib.vbi_decode.call_args_list]
                self.assertEqual(times, [1.0 + index * .04 for index in range(5)])
                self.assertEqual([bytes(call.args[1]._obj.data[:42]) for call in lib.vbi_decode.call_args_list],
                                 [bytes(BANK.base.BIT_REVERSE[value] for value in captured[4:])
                                  for captured in [unit(1), unit(2), unit(3), unit(4), unit(5)]])
                report = json.loads(output.getvalue())
                self.assertEqual(report["prefix_sha256"], hashlib.sha256(raw).hexdigest())
                self.assertEqual(report["suffix_sha256"], hashlib.sha256(next_raw).hexdigest())
                self.assertEqual((report["prefix_units"], report["suffix_units"]), (2, 3))
                self.assertTrue(report["single_decoder"] and report["decoder_input_clock_continuous"])
                self.assertFalse(report["panels_proven"] or report["full_level25_proven"])
                for key in ("original_pages", "updated_pages"):
                    self.assertEqual([page["logical_subpage"] for page in report[key]], [0, 1])
                    self.assertTrue(all(page["checked_columns"] == [8, 10] and page["fallback"] == [{"level": "1"}, {"level": "1.5"}] for page in report[key]))

    def test_new_mode_misuse_and_both_file_bounds_reject_before_decoder_loading(self):
        for argv in (["driver", "prefix", "day", "indexed-warm-edited"],
                     ["driver", "prefix", "day", "indexed-warm-edited", "suffix", "extra"],
                     ["driver", "prefix", "other", "indexed-warm-edited", "suffix"],
                     ["driver", "prefix", "day", "singleton-warm-edited", "suffix"]):
            with self.subTest(argv=argv), patch.object(BANK.sys, "argv", argv), \
                    patch.object(BANK.base, "load_decoder") as load, patch.object(BANK.ORACLE, "warm_suffix") as read:
                with self.assertRaises(BANK.base.CalibrationError):
                    BANK.main()
                load.assert_not_called()
                read.assert_not_called()
        with tempfile.TemporaryDirectory() as directory:
            prefix, suffix = Path(directory) / "prefix.json", Path(directory) / "suffix.json"
            valid = json.dumps([unit()]).encode()
            for selected in (prefix, suffix):
                for raw in (b"[]", json.dumps([unit()] * 513).encode(), json.dumps([[True, *unit()[1:]]]).encode(),
                            json.dumps([[2, 44, 0, 0] + [0] * 42]).encode(), b"x" * (BANK.base.MAX_INPUT + 1)):
                    prefix.write_bytes(valid)
                    suffix.write_bytes(valid)
                    selected.write_bytes(raw)
                    with self.subTest(file=selected.name, size=len(raw)), \
                            patch.object(BANK.sys, "argv", [str(SCRIPT), str(prefix), "day", "indexed-warm-edited", str(suffix)]), \
                            patch.object(BANK.base, "load_decoder") as load:
                        with self.assertRaises(BANK.base.CalibrationError):
                            BANK.main()
                        load.assert_not_called()

    def test_literal_failure_stops_suffix_and_always_deletes_single_decoder(self):
        lib = Mock()
        lib.vbi_decoder_new.return_value = 123
        lib.vbi_event_handler_register.return_value = 1
        with patch.object(BANK.sys, "argv", ["driver", "prefix", "day", "indexed-warm-edited", "suffix"]), \
                patch.object(BANK.ORACLE, "warm_suffix", return_value=(b"units", [unit(), unit()])), \
                patch.object(BANK.base, "load_decoder", return_value=(lib, "0.2.42")), \
                patch.object(BANK.ORACLE, "check_page", side_effect=BANK.base.CalibrationError("literal bitmap differs")):
            with self.assertRaisesRegex(BANK.base.CalibrationError, "literal bitmap"):
                BANK.main()
        self.assertEqual(lib.vbi_decode.call_count, 2)
        lib.vbi_decoder_delete.assert_called_once_with(123)

    def test_changed_oversize_source_cannot_execute_and_cached_bytecode_is_ignored(self):
        original = SCRIPT.with_name("teletext-editor-drcs-decoder.py").read_bytes()
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "teletext-editor-drcs-decoder.py"
            with patch.object(BANK, "__file__", str(Path(directory) / "driver.py")):
                for source in (original + b"\n", b"x" * 65537):
                    path.write_bytes(source)
                    with self.assertRaisesRegex(RuntimeError, "literal oracle changed"):
                        BANK.frozen_oracle()
                # The original checks its frozen sibling ABI itself.
                path.write_bytes(original)
                path.with_name("teletext-resource-decoder.py").write_bytes(Path(BANK.base.__file__).read_bytes())
                import marshal
                import struct
                cached = Path(importlib.util.cache_from_source(str(path)))
                cached.parent.mkdir()
                cached.write_bytes(importlib.util.MAGIC_NUMBER
                                   + struct.pack("<III", 0, int(path.stat().st_mtime), len(original))
                                   + marshal.dumps(compile("FOREIGN_CACHE = True", str(path), "exec")))
                actual = BANK.frozen_oracle()
                self.assertFalse(hasattr(actual, "FOREIGN_CACHE"))
                self.assertTrue(callable(actual.check_page))


if __name__ == "__main__":
    unittest.main()
