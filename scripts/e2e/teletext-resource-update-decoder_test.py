"""Pure closed-input contract regressions; no native library is loaded."""

import copy
import importlib.util
import io
import json
import marshal
from pathlib import Path
import struct
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location(
    "teletext_resource_update_contract",
    Path(__file__).with_name("teletext-resource-update-decoder.py"),
)
UPDATE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(UPDATE)


def unit(page, counter=0, flags=(0, 0)):
    data = [0] * 46
    data[:4] = [2, 44, 0, 0xE4]
    data[4:6] = [UPDATE.BASE.BIT_REVERSE[UPDATE.HAM[2]],
                 UPDATE.BASE.BIT_REVERSE[UPDATE.HAM[0]]]
    nibbles = [page & 15, page >> 4, 0, counter | (flags[0] << 3),
               1, 0, 13 | (flags[1] << 1), 1]
    data[6:14] = [UPDATE.BASE.BIT_REVERSE[UPDATE.HAM[value]] for value in nibbles]
    return data


def request():
    stages = []
    for index, state in enumerate(UPDATE.STATES):
        stages.append({"state": state, "units": [
            unit(0xC0, UPDATE.COUNTERS[index], UPDATE.FIRST_FLAGS[index]), unit(0xFF),
            unit(0xC0, UPDATE.COUNTERS[index]), unit(0xFF),
        ]})
    return {"expected_version": "0.2.42", "sequences": [{"name": "closed_contract", "stages": stages}]}


class ContractTests(unittest.TestCase):
    def test_frozen_source_executes_despite_valid_foreign_cached_bytecode(self):
        source = Path(UPDATE.BASE.__file__).read_bytes()
        with tempfile.TemporaryDirectory() as directory:
            helper = Path(directory) / "teletext-resource-decoder.py"
            helper.write_bytes(source)
            cached = Path(importlib.util.cache_from_source(str(helper)))
            cached.parent.mkdir()
            code = compile("CACHED_SENTINEL = True", str(helper), "exec")
            cached.write_bytes(importlib.util.MAGIC_NUMBER
                               + struct.pack("<III", 0, int(helper.stat().st_mtime), len(source))
                               + marshal.dumps(code))
            # The timestamp/size cache is valid and would replace the checked source.
            spec = importlib.util.spec_from_file_location("cache_control", helper)
            control = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(control)
            self.assertTrue(control.CACHED_SENTINEL)
            module = UPDATE._load_frozen_helper(helper)
            self.assertFalse(hasattr(module, "CACHED_SENTINEL"))
            self.assertTrue(callable(module.require))
            self.assertEqual(module.__file__, str(helper))
            self.assertEqual(module.__name__, "teletext_update_resource_abi")

    def test_frozen_source_digest_and_read_bound_reject_before_execution(self):
        with tempfile.TemporaryDirectory() as directory:
            helper = Path(directory) / "teletext-resource-decoder.py"
            for source in [Path(UPDATE.BASE.__file__).read_bytes() + b"\n",
                           b"x" * 65537]:
                helper.write_bytes(source)
                with self.subTest(size=len(source)):
                    with patch.object(UPDATE, "compile", create=True) as compiler:
                        with self.assertRaisesRegex(RuntimeError, "ABI helper changed"):
                            UPDATE._load_frozen_helper(helper)
                        compiler.assert_not_called()

    def rejects_before_loading(self, value):
        stream = io.TextIOWrapper(io.BytesIO(json.dumps(value).encode()))
        with patch.object(UPDATE.BASE, "load_decoder") as loader:
            with patch.object(UPDATE.sys, "stdin", stream):
                with self.assertRaises(UPDATE.BASE.CalibrationError):
                    UPDATE.main()
            loader.assert_not_called()

    def test_fixed_lifecycle_contract_and_literals(self):
        value = request()
        self.assertEqual(UPDATE.validate(value), value["sequences"])
        self.assertEqual(UPDATE.COUNTERS, (7, 7, 0, 0))
        self.assertEqual(UPDATE.FIRST_FLAGS, ((1, 1), (0, 0), (1, 1), (0, 0)))
        self.assertEqual(UPDATE.BASE_B,
                         (0, 0x3FC, 0x306, 0x306, 0x3FC, 0x306, 0x306, 0x3FC, 0, 0))
        self.assertEqual(len(UPDATE.literal_pixels(UPDATE.PATTERN_A)), 120)
        self.assertNotEqual(UPDATE.PATTERN_A, UPDATE.PATTERN_B)

    def test_closed_schema_counts_and_no_supplied_oracle(self):
        mutations = []
        value = request()
        value["sequences"][0]["stages"][0]["pixels"] = [0] * 10
        mutations.append(value)
        value = request()
        value["sequences"] *= 5
        mutations.append(value)
        value = request()
        value["sequences"][0]["stages"] *= 2
        mutations.append(value)
        value = request()
        value["sequences"][0]["stages"][0]["units"] *= 65
        mutations.append(value)
        for value in mutations:
            with self.subTest(value=value):
                self.rejects_before_loading(value)

    def test_counter_flags_repeat_owner_and_framing_are_not_guessed(self):
        for byte, replacement in [(9, UPDATE.BASE.BIT_REVERSE[UPDATE.HAM[14]]),
                                  (12, UPDATE.BASE.BIT_REVERSE[UPDATE.HAM[13]]),
                                  (8, UPDATE.BASE.BIT_REVERSE[UPDATE.HAM[1]]),
                                  (10, UPDATE.BASE.BIT_REVERSE[UPDATE.HAM[2]]),
                                  (0, True), (3, 0)]:
            value = request()
            value["sequences"][0]["stages"][0]["units"][0][byte] = replacement
            with self.subTest(byte=byte):
                self.rejects_before_loading(value)
        value = request()
        value["sequences"][0]["stages"][0]["units"][2] = unit(0xC0, 7, (1, 1))
        self.rejects_before_loading(value)

    def test_truncated_final_closure_and_wrong_stage_order_are_rejected(self):
        value = request()
        value["sequences"][0]["stages"][0]["units"].pop()
        self.rejects_before_loading(value)
        value = request()
        value["sequences"][0]["stages"].reverse()
        self.rejects_before_loading(value)

    def test_duplicate_json_and_two_mib_fence_precede_loading(self):
        for raw in [b'{"expected_version":"0.2.42","expected_version":"0.2.42"}',
                    b"x" * (UPDATE.BASE.MAX_INPUT + 1)]:
            stream = io.TextIOWrapper(io.BytesIO(raw))
            with patch.object(UPDATE.BASE, "load_decoder") as loader:
                with patch.object(UPDATE.sys, "stdin", stream):
                    with self.assertRaises(UPDATE.BASE.CalibrationError):
                        UPDATE.main()
                loader.assert_not_called()

    def test_one_decoder_four_fetches_and_monotonic_time(self):
        from unittest.mock import Mock
        lib = Mock()
        lib.vbi_decoder_new.return_value = 123
        lib.vbi_event_handler_register.return_value = 1
        sequence = copy.deepcopy(request()["sequences"][0])
        with patch.object(UPDATE, "check_page") as check:
            UPDATE.decode_sequence(lib, sequence)
        lib.vbi_decoder_new.assert_called_once_with()
        lib.vbi_decoder_delete.assert_called_once_with(123)
        self.assertEqual([call.args[2] for call in check.call_args_list], [0, 1, 2, 3])
        times = [call.args[3] for call in lib.vbi_decode.call_args_list]
        self.assertEqual(len(times), 16)
        self.assertTrue(all(before < after for before, after in zip(times, times[1:])))


if __name__ == "__main__":
    unittest.main()
