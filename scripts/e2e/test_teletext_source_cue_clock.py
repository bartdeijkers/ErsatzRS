"""Offline adversarial controls for the approved fixed-source 8ms criterion."""

import copy
from pathlib import Path
import tempfile
from types import ModuleType
import unittest

SCRIPT = Path(__file__).with_name("teletext-source-cue-clock.py")
C = ModuleType("source_cue_clock")
C.__file__ = str(SCRIPT)
exec(compile(SCRIPT.read_bytes(), str(SCRIPT), "exec"), C.__dict__)


def cues():
    return ([{"pts": 9000, "num_rects": 0}, {"pts": 45000, "num_rects": 0},
             {"pts": 135000, "num_rects": 1}],
            [{"pts": 1000000, "num_rects": 1}, {"pts": 2000000, "num_rects": 1},
             {"pts": 3000000, "num_rects": 0}],
            [{"shift": -351000}, {"shift": 45000}])


def prove(events, source, epochs, first=9000, final=200000):
    return C.qualify_cues(events, source, epochs, 45000, first, final)


class CueControls(unittest.TestCase):
    def test_native_source_cue_and_explicit_idempotent_clears_qualify(self):
        self.assertEqual(prove(*cues())["visible_error_us"], [0])

    def test_existing_eight_ms_boundary_is_inclusive_in_both_directions(self):
        for shift in (-720, 720):
            events, source, epochs = cues()
            events[-1]["pts"] += shift
            self.assertEqual(abs(prove(events, source, epochs)["visible_error_us"][0]), 8000)

    def test_one_tick_beyond_existing_limit_fails(self):
        for shift in (-721, 721):
            events, source, epochs = cues()
            events[-1]["pts"] += shift
            with self.assertRaises(ValueError):
                prove(events, source, epochs)

    def test_one_second_shift_and_wire_instead_of_native_clock_fail(self):
        for shift in (90000, 48664):
            events, source, epochs = cues()
            events[-1]["pts"] += shift
            with self.assertRaises(ValueError):
                prove(events, source, epochs)

    def test_missing_or_extra_visible_cue_fails(self):
        for events in (cues()[0][:-1], cues()[0] + [{"pts": 180000, "num_rects": 1}]):
            with self.assertRaises(ValueError):
                prove(events, *cues()[1:])

    def test_clear_cannot_replace_a_still_visible_source_cue(self):
        events, source, epochs = cues()
        events.insert(2, {"pts": 136000, "num_rects": 0})
        with self.assertRaises(ValueError):
            prove(events, source, epochs)

    def test_capture_start_must_independently_be_clear(self):
        with self.assertRaises(ValueError):
            prove(*cues(), first=-100000)

    def test_truncated_av_proof_cannot_hide_the_expected_cue(self):
        with self.assertRaises(ValueError):
            prove(*cues(), final=134999)

    def test_missing_source_explicit_clear_fails(self):
        events, source, epochs = cues()
        source[-1]["num_rects"] = 1
        with self.assertRaises(ValueError):
            prove(events, source, epochs)

    def test_nonrepresentable_native_timestamp_cannot_be_guessed(self):
        with self.assertRaises(ValueError):
            C.native_tick(1)


class ClockControls(unittest.TestCase):
    def fixture(self, text=None, **changes):
        values = dict(pid=258, count=3, flags=1, first=43412, final=641250)
        values.update(changes)
        text = text or "258,1589,0,PTS,1,43412,0,-9\n258,21778,1,PTS,2,581760,0,-13338\n258,23531,2,PTS,3,641250,0,-1749\n"
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "clock.csv"
            path.write_text(text)
            return C.clock_rows(path, **values)

    def test_variable_packet_lead_retains_original_absolute_clock_bound(self):
        self.assertEqual(len(self.fixture()), 3)

    def test_missing_or_wrong_counts_flags_endpoints_and_pid_fail(self):
        for changes in ({"pid": 259}, {"count": 4}, {"flags": 0}, {"first": 43413}, {"final": 641251}):
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                self.fixture(**changes)

    def test_missing_clock_backwards_pts_and_over_three_seconds_fail(self):
        baseline = "258,1589,0,PTS,1,43412,0,-9\n258,21778,1,PTS,2,581760,0,-13338\n258,23531,2,PTS,3,641250,0,-1749\n"
        for wrong in (baseline.replace("-13338", ""), baseline.replace("581760", "12345"), baseline.replace("-13338", "270001")):
            with self.subTest(wrong=wrong), self.assertRaises(ValueError):
                self.fixture(wrong)


def av():
    video = [{"pts": value, "dts": value, "data_hash": str(index)}
             for index, value in enumerate((0, 9000, 18000, 27000, 360000, 369000))]
    output = [dict(video[index], pts=video[index]["pts"] + shift, dts=video[index]["dts"] + shift)
              for index, shift in ((4, -351000), (5, -351000), (0, 45000), (1, 45000), (2, 45000))]
    audio = [{"pts": value, "dts": value, "data_hash": str(value)} for value in (0, 9000, 360000)]
    received = [dict(audio[index], pts=audio[index]["pts"] + shift, dts=audio[index]["dts"] + shift)
                for index, shift in ((2, -351000), (0, 45000), (1, 45000))]
    return video, output, audio, received


class SourceControls(unittest.TestCase):
    def test_exact_video_and_selected_audio_identify_two_source_epochs(self):
        epochs, seam, counts, differences = C.match_epochs(*av())
        self.assertEqual(([epoch["shift"] for epoch in epochs], seam, counts, differences),
                         ([-351000, 45000], 45000, [1, 2], []))

    def test_wrong_or_ambiguous_source_video_and_skipped_units_fail(self):
        baseline = av()
        variants = []
        wrong = copy.deepcopy(baseline)
        wrong[1][0]["data_hash"] = "unrelated"
        variants.append(wrong)
        wrong = copy.deepcopy(baseline)
        wrong[0].append(dict(wrong[0][0]))
        variants.append(wrong)
        wrong = copy.deepcopy(baseline)
        del wrong[1][-2]
        variants.append(wrong)
        for wrong in variants:
            with self.assertRaises(ValueError):
                C.match_epochs(*wrong)

    def test_wrong_audio_or_audio_clock_fails(self):
        for key, value in (("data_hash", "wrong"), ("pts", 135001), ("dts", 135001)):
            wrong = copy.deepcopy(av())
            wrong[3][1][key] = value
            with self.assertRaises(ValueError):
                C.match_epochs(*wrong)

    def test_single_epoch_cannot_claim_a_programme_rollover(self):
        wrong = av()
        del wrong[1][2:]
        with self.assertRaises(ValueError):
            C.match_epochs(*wrong)


def subtitle_packet(pts=641250):
    stamp = bytes([0x21 | ((pts >> 30 & 7) << 1), pts >> 22 & 255,
                   ((pts >> 15 & 127) << 1) | 1, pts >> 7 & 255, ((pts & 127) << 1) | 1])
    body = b"\x00\x00\x01\xbd\x00\x08\x80\x80\x05" + stamp
    return bytearray(b"\x47\x41\x02\x10" + body + bytes([255]) * (184 - len(body)))


class PesControls(unittest.TestCase):
    def test_pts_is_reconstructed_from_the_actual_raw_pes(self):
        packet = subtitle_packet()
        self.assertEqual(C.subtitle_pes(packet, 258)[0]["pts"], 641250)

    def test_transport_error_and_bad_pts_marker_cannot_qualify(self):
        for offset in (1, 13):
            packet = subtitle_packet()
            packet[offset] = packet[offset] | 128 if offset == 1 else packet[offset] & 254
            with self.assertRaises(ValueError):
                C.subtitle_pes(packet, 258)

    def test_incomplete_pes_and_wrong_pid_cannot_qualify(self):
        packet = subtitle_packet()
        packet[8:10] = b"\xff\xff"
        with self.assertRaises(ValueError):
            C.subtitle_pes(packet, 258)
        with self.assertRaises(ValueError):
            C.subtitle_pes(subtitle_packet(), 259)

    def test_another_start_cannot_hide_an_incomplete_pes(self):
        packet = subtitle_packet()
        packet[8:10] = b"\xff\xff"
        with self.assertRaises(ValueError):
            C.subtitle_pes(packet + subtitle_packet(), 258)

    def test_file_bound_is_checked_before_native_tools(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "input"
            path.write_bytes(b"12345")
            with self.assertRaises(ValueError):
                C.read(path, 4)


if __name__ == "__main__":
    unittest.main()
