"""Controls for the sign, clock reference, range and absence of VLC A/V measurements."""

import sys
import json
import tempfile
import unittest
import wave
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "calibration"))
from channel1_av_offset import BLOCK_S, READ, CodeReader, offset_verdict, read_window
from channel1_timecode import BLOCK, FPS, FRAMES, RATE, reference_envelope, write_tone
from channel1_metrics import audio_checks


class OffsetControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.work = tempfile.TemporaryDirectory()
        cls.addClassCleanup(cls.work.cleanup)
        cls.reader = CodeReader()
        for hz in (750, 1500):
            write_tone(Path(cls.work.name) / f"{hz}.wav", hz)

    def measurement(self, hz, at, offset_ms, video_delay_s=0):
        # The source time is injected independently of the decoder and of
        # the expected frame index. Wrap a window that crosses the clip end.
        clip = (at - video_delay_s - offset_ms / 1000) % (FRAMES / FPS)
        with wave.open(str(Path(self.work.name) / f"{hz}.wav"), "rb") as audio:
            audio.setpos(round(clip * RATE) % audio.getnframes())
            samples = audio.readframes(READ * BLOCK)
            if len(samples) < READ * BLOCK * 4:
                audio.rewind()
                samples += audio.readframes(READ * BLOCK - len(samples) // 4)
        mono = np.frombuffer(samples, dtype="<i2").reshape(READ, BLOCK, 2)[:, :, 0]
        start = round(at / BLOCK_S)
        rms = np.r_[np.zeros(start), np.sqrt(np.mean(mono.astype(float) ** 2, axis=1))]
        frame = int((at - video_delay_s) * FPS)
        frames = [(0, n) for n in range(frame - 1, frame + 3)]
        times = [f[1] / FPS + video_delay_s for f in frames]
        return read_window(self.reader, rms, rms > 0, frames, times, start)

    def test_post_switch_gap_is_independent_of_content_delay(self):
        # All three observations start 1.816 s after the scheduled switch.
        for offset in (0, 1816, -97):
            with self.subTest(offset=offset):
                row = self.measurement(750, 301.816, offset)
                self.assertAlmostEqual(row["offset_ms"], offset, delta=2.1)

    def test_both_tracks_and_clip_wrap_with_a_late_video_callback(self):
        for hz in (750, 1500):
            for offset in (17, -1234, 7008):
                with self.subTest(hz=hz, offset=offset):
                    row = self.measurement(hz, 300.120, offset, video_delay_s=0.120)
                    self.assertAlmostEqual(row["offset_ms"], offset, delta=2.1)

    def test_uncoded_incomplete_and_ambiguous_audio(self):
        valid = np.ones(READ, dtype=bool)
        for signal, mask in [(np.ones(READ), valid), (np.zeros(READ), ~valid),
                             (np.resize([1, 0.9], READ), valid)]:
            match, reason = self.reader.locate(signal, mask)
            self.assertIsNone(match)
            self.assertTrue(reason)

    def test_splice_attenuation_is_an_erasure_with_a_coverage_limit(self):
        valid = np.ones(READ, dtype=bool)
        signal = reference_envelope()[5000:5000 + READ].copy()
        signal[-2:] *= 0.5
        match, _ = self.reader.locate(signal, valid)
        self.assertEqual(match[0], 20.0)
        signal[:8] *= 0.5
        self.assertIsNone(self.reader.locate(signal, valid)[0])

    def test_optional_limit_and_incomplete_measurements(self):
        report = {"max_abs_offset_limit_ms": None, "measurement_complete": True,
                  "worst": {"offset_ms": -128}}
        self.assertEqual(offset_verdict(report), "PASS")
        report["measurement_complete"] = False
        self.assertEqual(offset_verdict(report), "NIET GEMETEN")
        report["max_abs_offset_limit_ms"] = 40
        self.assertEqual(offset_verdict(report), "FAIL")


class SwitchSilenceControls(unittest.TestCase):
    def run_selection(self, missing_start, missing_stop, quiet=True):
        pcm = np.zeros(13 * RATE, dtype="<i2")
        covered = np.ones(len(pcm), dtype=bool)
        covered[missing_start:missing_stop] = False
        times = np.arange(len(pcm) // BLOCK) * BLOCK_S
        silence = (times >= 10) & (times < 12) if quiet else np.zeros(len(times), dtype=bool)
        active = np.where(silence, 0, np.where(times < 12, 1, 2))
        data = {"media_epoch_ms": 0, "media_clock": [(ms * 1000, ms) for ms in range(0, 13001, 100)],
                "complete": True, "audio_ids": [257, 258], "initial_audio_ids": [257, 258],
                "initial_audio_id": 257, "switches": [{"planned_s": 10, "presented_s": 10,
                "command_us": 10000000, "target_id": 258, "result": 0}]}
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory)
            audio_checks(out, data, 0, pcm, covered, 0,
                         np.column_stack((active == 1, active == 2)), silence, active)
            return json.loads((out / "audio.json").read_text())

    def test_two_seconds_only_during_confirmed_switch(self):
        a = self.run_selection(10 * RATE, 12 * RATE)
        self.assertTrue(a["switches"][0]["passed"])
        self.assertEqual(a["switches"][0]["switch_silence_ms"], 2000)
        self.assertEqual(a["missing_pcm_outside_accepted_switches"], 0)
        self.assertFalse(self.run_selection(10 * RATE, 12 * RATE + 1)["switches"][0]["passed"])
        self.assertGreater(self.run_selection(10 * RATE - 1, 12 * RATE)["missing_pcm_outside_accepted_switches"], 0)
        self.assertFalse(self.run_selection(0, 0, quiet=False)["switches"][0]["passed"])


if __name__ == "__main__":
    unittest.main()
