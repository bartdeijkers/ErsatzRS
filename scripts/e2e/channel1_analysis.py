"""Independent source, transport and renderer checks for the channel-one gate."""

import itertools
import json
import re
import shutil
import subprocess
import xml.etree.ElementTree as ET
from fractions import Fraction

import numpy as np
from channel1_fixture import FPS
from channel1_timecode import BLOCK, CODE_ID, reference_envelope


def run(args):
    return subprocess.check_output(args, encoding="utf-8", errors="replace")


def snapshot_hls_pair(out, destination):
    """Copy two consecutive published segments before rolling cleanup."""
    folder = out / "temp" / "ersatzrs-hls" / "1"
    try:
        playlist = (folder / "live.m3u8").read_text(encoding="utf-8")
        files = [
            folder / name for name in playlist.splitlines() if re.fullmatch(r"live\d+\.ts", name)
        ]
        for first, second in itertools.pairwise(files):
            if (
                int(first.stem[4:]) + 1 != int(second.stem[4:])
                or not first.is_file()
                or not second.is_file()
            ):
                continue
            destination.mkdir(exist_ok=True)
            for segment in (first, second):
                shutil.copyfile(segment, destination / segment.name)
            (destination / "live.m3u8").write_text(playlist)
            return True
    except (FileNotFoundError, PermissionError):
        pass  # The producer can rotate a segment between listing and opening it.
    return False


def inspect_ts(path, out, prefix, tools):
    probe = json.loads(
        run(
            [
                tools["ffprobe"],
                "-v",
                "error",
                "-fix_teletext_pts",
                "0",
                "-show_streams",
                "-show_programs",
                "-of",
                "json",
                path,
            ]
        )
    )
    analysis = json.loads(run(["tsanalyze", "--json", path]))
    (out / f"{prefix}-probe.json").write_text(json.dumps(probe, indent=2))
    (out / f"{prefix}-transport.json").write_text(json.dumps(analysis, indent=2))
    pmt_pid = probe["programs"][0]["pmt_pid"]
    xml = run(["tstables", "--pid", str(pmt_pid), "--xml-output", "-", path])
    (out / f"{prefix}-pmt.xml").write_text(xml)
    tables = ET.fromstring(xml).findall("PMT")
    subtitles = bool(tables) and all(
        any(
            x.get("page_number") == "888" and int(x.get("teletext_type"), 0) in (2, 5)
            for x in tab.iter("teletext")
        )
        for tab in tables
    )
    audio = [s for s in probe["streams"] if s["codec_name"] == "ac3"]
    return probe, analysis, subtitles, audio


def validate_source(out, tools, src):
    probe, _, page, audio = inspect_ts(src, out, "source", tools)
    video = next(s for s in probe["streams"] if s["codec_type"] == "video")
    if not (
        page
        and len(audio) == 2
        and all(s["sample_rate"] == "48000" and s["channels"] == 2 for s in audio)
        and (
            video["width"],
            video["height"],
            video["color_space"],
            video["color_transfer"],
            video["field_order"],
            video["r_frame_rate"],
        )
        == (1920, 1080, "bt709", "bt709", "progressive", "25/1")
        and abs(float(video["duration"]) - 300) < 1 / FPS
    ):
        raise ValueError("invalid synthetic source; see source-probe.json and source-pmt.xml")
    # -copyts preserves the common clock; compare extracted cue PTS to video.
    text = run(
        [
            tools["ffmpeg"],
            "-v",
            "error",
            "-copyts",
            "-fix_teletext_pts",
            "0",
            "-txt_page",
            "888",
            "-txt_format",
            "text",
            "-i",
            src,
            "-map",
            "0:s:0",
            "-c:s",
            "srt",
            "-f",
            "srt",
            "pipe:1",
        ]
    )
    (out / "source-888.srt").write_text(text)
    times = {}
    emissions = []
    for h, m, s, ms, label in re.findall(r"(\d+):(\d+):(\d+),(\d+) --> [^\n]+\n(\d+)", text):
        at = 3600 * int(h) + 60 * int(m) + int(s) + int(ms) / 1000 - float(video["start_time"])
        times.setdefault(int(label), at)
        emissions.append((at, int(label)))
    expected = [(tick / 2, tick // 10 * 5) for tick in range(10, 600)]
    if len(emissions) != len(expected) or any(
        abs(t - e) > 1 / FPS or label != value
        for (t, label), (e, value) in zip(emissions, expected)
    ):
        raise ValueError(
            "source page 888 did not repeat the current subtitle on its video timeline"
        )
    tone_levels = []
    code_checks = []
    reference = reference_envelope().reshape(300, -1)
    # AC-3 priming can precede video frame zero. Use the common media clock
    # to check every source sample in the two tracks' programme interval.
    first_sample = round(int(video["start_pts"]) * Fraction(video["time_base"]) * 48000)
    sample_count = 300 * 48000
    for index, hz in enumerate((750, 1500)):
        cmd = [
            tools["ffmpeg"],
            "-v",
            "error",
            "-copyts",
            "-i",
            src,
            "-map",
            f"0:a:{index}",
            "-af",
            f"atrim=start_pts={first_sample}:end_pts={first_sample + sample_count}",
            "-ar",
            "48000",
            "-f",
            "s16le",
            "pipe:1",
        ]
        decoded = np.frombuffer(subprocess.check_output(cmd), dtype="<i2")
        if len(decoded) != sample_count * 2:
            raise ValueError("source stereo audio does not cover the exact video interval")
        for channel in range(2):
            # Align by the video's native PTS above. Validate the authored
            # timing independently in every second, after actual AC-3 decode.
            seconds = decoded.reshape(300, -1, BLOCK, 2)
            correlations = []
            for signal, expected in zip(seconds, reference):
                envelope = np.sqrt(np.mean(signal[:, :, channel].astype(float) ** 2, axis=1))
                if np.ptp(envelope) < 0.03 * envelope.mean():
                    correlations.append(0.0)
                else:
                    correlations.append(float(np.corrcoef(envelope, expected)[0, 1]))
            code_checks.append({"track": index, "channel": channel,
                                "min_zero_lag_correlation": min(correlations)})
        blocks = decoded.reshape(-1, 960, 2)
        levels = []
        for part in np.array_split(blocks, 32):
            spectral = np.abs(np.fft.rfft(part, axis=1)) * 2 / (960 * 32768)
            if not np.all(np.argmax(spectral, axis=1) == hz // 50):
                raise ValueError("wrong source tone")
            levels.append(20 * np.log10(spectral[:, hz // 50, :]))
        levels = np.concatenate(levels)
        low, high = float(levels.min()), float(levels.max())
        if low < -19 or high > -17:
            raise ValueError("source AC-3 level outside the calibration tolerance")
        tone_levels.append({
            "frequency_hz": hz, "min_dbfs": low, "max_dbfs": high,
            "samples_per_channel": sample_count, "first_20ms_dbfs": levels[0].tolist(),
            "last_20ms_dbfs": levels[-1].tolist(),
        })
    (out / "source-validation.json").write_text(
        json.dumps({"tone_levels": tone_levels, "first_cue_times": times, "audio_timecode": {
            "id": CODE_ID, "checks": code_checks,
            "verified": all(c["min_zero_lag_correlation"] >= 0.97 for c in code_checks),
        }}, indent=2)
    )
