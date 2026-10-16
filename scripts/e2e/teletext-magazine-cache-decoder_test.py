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



def transition_request():
    def packet(row, body=None):
        raw = 2 | (row << 3)
        return [2, 44, 0, 0xE4, CACHE.BASE.BIT_REVERSE[CACHE.HAM[raw & 15]],
                CACHE.BASE.BIT_REVERSE[CACHE.HAM[raw >> 4]]] + ([0] * 40 if body is None else body)

    def owner_header(page, subcode, c4=0):
        body = [page & 15, page >> 4, subcode & 15, ((subcode >> 4) & 7) | (c4 << 3),
                (subcode >> 8) & 15, (subcode >> 12) & 3, 0, 0]
        return packet(0, [CACHE.BASE.BIT_REVERSE[CACHE.HAM[value]] for value in body] + [0] * 32)

    phases = []
    for index, name in enumerate(CACHE.TRANSITION_PHASES):
        units = []
        for cycle in range(2):
            if index == 0:
                units.append(packet(29, [CACHE.BASE.BIT_REVERSE[CACHE.HAM[0]]] + [0] * 39))
            for owner, subcode in CACHE.TRANSITION_OWNERS:
                units.append(owner_header(owner & 255, subcode, int(cycle == 0 and index < 2)))
                if index:
                    units.append(packet(28, [CACHE.BASE.BIT_REVERSE[CACHE.HAM[0]]] + [0] * 39))
                units.extend(packet(row) for row in range(1, 25))
            units.append(owner_header(255, 0))
        phase = dict.fromkeys(CACHE.TRANSITION_PHASE_FIELDS)
        phase.update(name=name, complete_cycles=2, complete_ordered_model_equal=True, complete_pes_boundary=True,
                     start_packet=index * 1000, end_packet=(index + 1) * 1000, acknowledged_end_packet=(index + 1) * 1000,
                     sink_elapsed_ns=(index + 1) * 100, units=units)
        rehash_phase(phase)
        phases.append(phase)
    value = dict.fromkeys(CACHE.TRANSITION_TOP_FIELDS)
    value.update(probe_version=1, packet_count=3000, phases=phases, declared_context={"not_an_oracle": True})
    return value


def rehash_phase(phase):
    import hashlib
    phase["unit_count"] = len(phase["units"])
    phase["units_sha256"] = hashlib.sha256(bytes(byte for unit in phase["units"] for byte in unit)).hexdigest()


def transition_library(checkpoint=0, defect=None):
    lib = Mock()
    foreground = [153, 85, 34, 255] if checkpoint == 0 else [51, 170, 238, 255]
    background = [17, 34, 51, 255] if checkpoint == 0 else [34, 68, 102, 255]

    def fetch(_decoder, pointer, owner, subcode, level, *_rest):
        page = pointer._obj
        page.pgno, page.subno, page.rows, page.columns, page.screen_color = owner, subcode, 25, 41, 16
        page.color_map[23], page.color_map[16] = int.from_bytes(bytes(foreground), "little"), int.from_bytes(bytes(background), "little")
        for column, unicode in zip((1, 2), (0x410, 0x411) if checkpoint == 0 else (0x391, 0x392)):
            page.text[41 + column].bits1 = (23 << 24) | (3 << 16)
            page.text[41 + column].bits2 = (unicode << 16) | 16
        if defect == "owner": page.subno ^= 16
        elif defect == "glyph": page.text[43].bits2 ^= 1 << 16
        elif defect == "palette": page.color_map[23] ^= 1
        elif defect == "indices": page.text[42].bits2 &= ~255
        elif defect == "opaque": page.text[42].bits1 &= ~(3 << 16)
        elif defect == "screen": page.screen_color = 0
        return int(defect != "missing")

    def draw(_page, _format, canvas, *_rest):
        pixels = (foreground + background) * 60
        if defect == "single_colour": pixels = foreground * 120
        elif defect == "third_colour": pixels[:4] = [0, 0, 0, 255]
        C = CACHE.C
        C.cast(canvas, C.POINTER(C.c_uint8 * 480)).contents[:] = pixels
        if defect == "guard": C.cast(canvas, C.POINTER(C.c_uint8 * 481)).contents[480] = 0

    lib.vbi_fetch_vt_page.side_effect = fetch
    lib.vbi_draw_vt_page_region.side_effect = draw
    return lib


class PagePresentationTransitionTests(unittest.TestCase):
    def test_two_complete_cycles_and_exact_three_owner_order_per_phase(self):
        value = transition_request()
        self.assertEqual(CACHE.validate_transition(value), value["phases"])
        self.assertEqual([phase["unit_count"] for phase in value["phases"]], [154, 158, 158])
        self.assertEqual(CACHE.TRANSITION_OWNERS, ((0x200, 0), (0x200, 16), (0x201, 0)))

    def test_missing_owner_x28_rows_closure_and_hash_are_rejected_before_native(self):
        for defect in ("owner", "x28", "row", "closure", "trailing", "hash", "framing", "bool", "cycles", "order", "boundary"):
            value = transition_request()
            phase = value["phases"][1]
            if defect == "owner": phase["units"][26] = phase["units"][52][:]
            elif defect == "x28": phase["units"].pop(1)
            elif defect == "row": phase["units"].pop(2)
            elif defect == "closure": phase["units"][78] = phase["units"][0][:]
            elif defect == "trailing": phase["units"].append(phase["units"][-1][:])
            elif defect == "framing": phase["units"][0][3] = 0
            elif defect == "bool": phase["units"][0][0] = True
            elif defect == "cycles": phase["complete_cycles"] = True
            elif defect == "order": value["phases"].reverse()
            elif defect == "boundary": phase["start_packet"] += 1
            if defect not in ("hash", "bool"):
                rehash_phase(phase)
            if defect == "hash": phase["units_sha256"] = "0" * 64
            stream = io.TextIOWrapper(io.BytesIO(json.dumps(value).encode()))
            with self.subTest(defect=defect), patch.object(CACHE.sys, "stdin", stream), patch.object(CACHE.BASE, "load_decoder") as load:
                with self.assertRaises(CACHE.BASE.CalibrationError):
                    CACHE.page_presentation_transition_main()
                load.assert_not_called()

    def test_level25_literal_glyphs_palettes_screen_and_guarded_two_colour_pixels(self):
        for checkpoint in range(3):
            for owner, subcode in CACHE.TRANSITION_OWNERS:
                lib = transition_library(checkpoint)
                report = CACHE.check_transition_page(lib, 123, owner, subcode, checkpoint)
                self.assertEqual([cell["unicode"] for cell in report["cells"]], [0x410, 0x411] if checkpoint == 0 else [0x391, 0x392])
                self.assertEqual(report["level"], "2.5")
                self.assertEqual(lib.vbi_fetch_vt_page.call_args.args[2:], (owner, subcode, 2, 25, 0))
                self.assertEqual(lib.vbi_draw_vt_page_region.call_count, 2)
                lib.vbi_unref_page.assert_called_once()
        for defect in ("owner", "glyph", "palette", "indices", "opaque", "screen", "missing", "single_colour", "third_colour", "guard"):
            lib = transition_library(1, defect)
            with self.subTest(defect=defect), self.assertRaises(CACHE.BASE.CalibrationError):
                CACHE.check_transition_page(lib, 123, 0x200, 16, 1)
            self.assertEqual(lib.vbi_unref_page.call_count, int(defect != "missing"))

    def test_one_lifetime_one_clock_and_all_exact_phase_bytes_without_reset(self):
        lib = Mock()
        lib.vbi_decoder_new.return_value = 123
        lib.vbi_event_handler_register.return_value = 1
        phases = CACHE.validate_transition(transition_request())
        with patch.object(CACHE, "check_transition_page", return_value={}) as check:
            reports, ticks = CACHE.decode_transition(lib, phases)
        self.assertEqual(ticks, 470)
        self.assertEqual([report["phase"] for report in reports], ["old", "replacement", "repeat"])
        self.assertEqual([call.args[2:] for call in check.call_args_list], [(owner, subcode, phase) for phase in range(3) for owner, subcode in CACHE.TRANSITION_OWNERS])
        lib.vbi_decoder_new.assert_called_once()
        lib.vbi_decoder_delete.assert_called_once_with(123)
        times = [call.args[3] for call in lib.vbi_decode.call_args_list]
        self.assertTrue(all(a < b for a, b in zip(times, times[1:])))
        self.assertEqual([bytes(call.args[1]._obj.data[:42]) for call in lib.vbi_decode.call_args_list],
                         [bytes(CACHE.BASE.BIT_REVERSE[byte] for byte in unit[4:]) for phase in phases for unit in phase["units"]])

    def test_failure_deletes_single_decoder_and_stops_later_phases(self):
        lib = Mock()
        lib.vbi_decoder_new.return_value = 123
        lib.vbi_event_handler_register.return_value = 1
        with patch.object(CACHE, "check_transition_page", side_effect=CACHE.BASE.CalibrationError("literal")):
            with self.assertRaises(CACHE.BASE.CalibrationError):
                CACHE.decode_transition(lib, CACHE.validate_transition(transition_request()))
        self.assertEqual(lib.vbi_decode.call_count, 154)
        lib.vbi_decoder_delete.assert_called_once_with(123)

    def test_explicit_mode_reports_level25_only_and_never_fetches_level1(self):
        value = transition_request()
        stream = io.TextIOWrapper(io.BytesIO(json.dumps(value).encode()))
        output = io.StringIO()
        with patch.object(CACHE.sys, "stdin", stream), patch.object(CACHE.sys, "stdout", output), \
                patch.object(CACHE.BASE, "load_decoder", return_value=(Mock(), "0.2.42")), \
                patch.object(CACHE, "mapped_original") as mapping, \
                patch.object(CACHE, "decode_transition", return_value=([{}, {}, {}], 470)):
            CACHE.page_presentation_transition_main()
        report = json.loads(output.getvalue())
        self.assertTrue(report["page_presentation_transition_level25_proven"])
        self.assertFalse(report["level1_fallback_proven"])
        self.assertFalse(report["font_shape_proven"])
        self.assertFalse(report["client_conformance_proven"])
        mapping.assert_called_once()

if __name__ == "__main__":
    unittest.main()
