"""Measure audio content time against frame presentations in the same VLC session."""

import argparse
import json
import math
from bisect import bisect_right

import numpy as np

from channel1_timecode import BLOCK, CODE_ID, FPS, FRAMES, RATE, reference_envelope

BLOCK_S = BLOCK / RATE
READ = 320  # 1.280 s = 32 coded frames
PERIOD = FRAMES / FPS


def offset_limit(value):
    limit = float(value)
    if not math.isfinite(limit) or limit < 0:
        raise argparse.ArgumentTypeError("expected a finite, nonnegative offset in milliseconds")
    return limit


def offset_verdict(report):
    limit = report["max_abs_offset_limit_ms"]
    if limit is not None and "worst" in report and abs(report["worst"]["offset_ms"]) > limit:
        return "FAIL"
    return "PASS" if report["measurement_complete"] else "NIET GEMETEN"


def signed_offset_ms(video_s, source_audio_s):
    offset = (video_s - source_audio_s + PERIOD / 2) % PERIOD - PERIOD / 2
    return round(offset * 1000, 3)


class CodeReader:
    def __init__(self):
        ref = reference_envelope()
        self.size = len(ref)
        self.spectrum = np.fft.rfft(ref)
        wrapped = np.r_[ref, ref[:READ - 1]]
        def sums(values):
            total = np.r_[0.0, np.cumsum(values)]
            return total[READ:] - total[:-READ]
        self.norm = np.sqrt(sums(wrapped ** 2) - sums(wrapped) ** 2 / READ)

    def locate(self, rms, valid):
        if len(rms) != READ or valid.sum() < 0.98 * READ:
            return None, "insufficient selected-tone PCM"
        signal = rms[valid]
        lo, hi = np.percentile(signal, [10, 90])
        if lo <= 0 or hi < lo * 1.04:
            return None, "uncoded or insufficient level contrast"
        # AC-3 splice attenuation can leave a detectable carrier far below
        # either authored level. Such a block contains no usable code bit.
        level_tolerance = 10 ** (1 / 20)
        valid = valid & (rms >= lo / level_tolerance) & (rms <= hi * level_tolerance)
        if valid.sum() < 0.98 * READ:
            return None, "insufficient in-band timecode levels"
        centered = np.where(valid, rms - rms[valid].mean(), 0)
        spectrum = np.fft.rfft(centered, n=self.size)
        scores = np.fft.irfft(self.spectrum * spectrum.conj(), n=self.size)
        scores /= self.norm * np.linalg.norm(centered)
        pos = int(scores.argmax())
        best = float(scores[pos])
        # Adjacent 4 ms bins can identify the same frame. Demand a unique
        # match against the rest of the entire five-minute code, including its seam.
        scores[(pos + np.arange(-10, 11)) % self.size] = -np.inf
        margin = best - float(scores.max())
        if best < 0.90 or margin < 0.10:
            return None, f"weak or ambiguous timecode (correlation {best:.4f}, margin {margin:.4f})"
        return (pos * BLOCK_S, round(best, 4), round(margin, 4)), None


def video_position(frames, times, at):
    i = bisect_right(times, at) - 1
    if i < 0 or i >= len(frames):
        return None
    frame = frames[i][1]
    if frame < 0:
        return None
    if at == times[i]:
        return frame / FPS
    if i + 1 == len(times) or frames[i + 1][1] != frame + 1 or times[i + 1] - times[i] > 0.1:
        return None
    phase = (at - times[i]) / (times[i + 1] - times[i])
    return (frame + phase) / FPS


def read_window(reader, rms, valid, frames, times, start):
    at = start * BLOCK_S
    row = {"at_s": round(at, 6), "offset_ms": None}
    match, reason = reader.locate(rms[start:start + READ], valid[start:start + READ])
    if not match:
        return {**row, "reason": reason}
    src, score, margin = match
    # Use the presented frame at the start of the interval. The intervening
    # silence is never part of the audio-content clock.
    video = video_position(frames, times, at)
    if video is None:
        return {**row, "reason": "no adjacent visible counters at measurement"}
    return {**row, "source_clip_s": round(src, 6), "visible_video_s": round(video, 6),
            "offset_ms": signed_offset_ms(video, src), "correlation": score, "margin": margin}


def transport_offsets(samples, pts, expected):
    """Independent points on each transmitted PID's own decoded-sample clock."""
    reader = CodeReader()
    blocks = samples.reshape(-1, BLOCK)
    rows = []
    for at in range(0, 600, 10):
        row = {"planned_s": at, "offset_ms": None}
        candidates = np.flatnonzero((pts >= at) & (pts < at + BLOCK_S + 2 / RATE))
        if len(candidates):
            i = candidates[0]
            times = pts[i:i + READ]
            if len(times) == READ and np.all(np.abs(np.diff(times) - BLOCK_S) <= 2.1 / RATE):
                rms = np.sqrt(np.mean(blocks[i:i + READ].astype(float) ** 2, axis=1))
                match, reason = reader.locate(rms, expected[i:i + READ])
                if match:
                    src, score, margin = match
                    row.update(video_pts_s=float(times[0]), source_clip_s=src,
                               offset_ms=signed_offset_ms(times[0], src), correlation=score, margin=margin)
                else:
                    row["reason"] = reason
        rows.append(row)
    return rows


def measure_offset(out, data, pcm, covered, active, switches, limit_ms=None):
    report = {
        "code": CODE_ID, "clock": "video content time minus audio content time at VLC presentation",
        "positive": "audio late", "block_ms": 4, "window_ms": 1280,
        "unambiguous_range_ms": [-150000, 150000], "max_abs_offset_limit_ms": limit_ms,
        "windows": [], "switches": [], "measurement_complete": False,
    }
    source = json.loads((out / "source-validation.json").read_text()).get("audio_timecode", {})
    if source.get("id") != CODE_ID or not source.get("verified"):
        return {**report, "reason": "source has no verified frame-locked audio timecode"}
    frames = data["frames"]
    times = [(frame[0] - data["epoch_us"]) / 1e6 for frame in frames]
    reader = CodeReader()
    count = min(len(pcm) // BLOCK, len(active))
    rms = np.empty(count)
    blocks = pcm[:count * BLOCK].reshape(-1, BLOCK)
    for start in range(0, count, 1024):
        part = blocks[start:start + 1024].astype(float)
        rms[start:start + len(part)] = np.sqrt(np.mean(part ** 2, axis=1))
    present = covered[:count * BLOCK].reshape(-1, BLOCK).all(axis=1)
    limit = min(count, round(600 / BLOCK_S))
    # Stints are bounded by actual commands and audible onsets. A missing
    # switch cannot shift the identity or timestamps of subsequent rows.
    for i in range(len(switches) + 1):
        switch = switches[i - 1] if i else None
        onset = switch["onset_s"] if switch else 0.0
        planned = switch["planned_s"] if switch else 0
        track = 1 + (planned // 10) % 2
        summary = {"planned_s": planned, "track": "AB"[track - 1], "onset_s": onset,
                   "first_offset_ms": None, "last_window_offset_ms": None}
        if switch:
            previous = report["switches"][-1] if report["switches"] else report["initial"]
            summary.update(command_s=switch["command_s"], latency_s=switch["latency_s"],
                           previous_window_at_s=previous.get("last_measurement_s"),
                           previous_window_offset_ms=previous["last_window_offset_ms"])
            report["switches"].append(summary)
        else:
            report["initial"] = summary
        if onset is None or (switch and switch["result"] != 0):
            summary["reason"] = "no verified audible selection"
            continue
        first = max(0, math.ceil(onset / BLOCK_S - 1e-8))
        stop = limit if i == len(switches) else min(limit, math.floor(switches[i]["command_s"] / BLOCK_S))
        last = stop - READ
        if first > last:
            summary["reason"] = "insufficient audio before next command"
            continue
        valid = present & (active == track)
        starts = list(range(first, last + 1, READ))
        if starts[-1] != last:
            starts.append(last)
        rows = [read_window(reader, rms, valid, frames, times, n) for n in starts]
        report["windows"].extend({**row, "stint_s": planned, "track": summary["track"]} for row in rows)
        summary.update(first_measurement_s=rows[0]["at_s"], first_offset_ms=rows[0]["offset_ms"],
                       last_measurement_s=rows[-1]["at_s"], last_window_offset_ms=rows[-1]["offset_ms"])
    measured = [r for r in report["windows"] if r["offset_ms"] is not None]
    report["measured_windows"] = len(measured)
    report["measured_post_switches"] = sum(s["first_offset_ms"] is not None for s in report["switches"])
    report["measurement_complete"] = bool(data["complete"] and report["measured_post_switches"] == 59
        and report["initial"]["first_offset_ms"] is not None and len(measured) == len(report["windows"]))
    if measured:
        offsets = [r["offset_ms"] for r in measured]
        report.update(min_ms=min(offsets), median_ms=float(np.median(offsets)), max_ms=max(offsets),
                      worst=max(measured, key=lambda r: abs(r["offset_ms"])))
        report["by_track"] = {}
        for track in ("A", "B"):
            values = [r["offset_ms"] for r in measured if r["track"] == track]
            if values:
                report["by_track"][track] = {"windows": len(values), "min_ms": min(values),
                                             "median_ms": float(np.median(values)), "max_ms": max(values)}
    return report
