"""Analyze one retained libVLC capture without tuning the product or replaying it."""

import argparse
import csv
import itertools
import json
import re
import subprocess
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

import numpy as np
from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "calibration"))
from channel1_analysis import inspect_ts, run, snapshot_hls_pair
from channel1_fixture import FPS, FRAMES, counters
from channel1_receiver import make_templates, read_line
from channel1_av_offset import measure_offset, offset_limit, offset_verdict, transport_offsets

RATE = 48000
WINDOW = 192  # 4 ms: exactly 3 or 6 cycles.
MAX_NON_SILENT_LATENCY = 0.25
MAX_SWITCH_SILENCE = 2.0
DURATION = 600


def dump(path, obj):
    path.write_text(json.dumps(obj, indent=2), encoding="utf8")


def player_seconds(data, presentation_us):
    """Reconstruct the same libVLC media clock used by the selection loop."""
    anchors = np.asarray(data["media_clock"], dtype=np.int64)
    times = np.asarray(presentation_us)
    if not len(anchors) or data["media_epoch_ms"] is None:
        raise ValueError("missing libVLC media-clock calibration")
    indices = np.searchsorted(anchors[:, 0], times, side="right") - 1
    anchor = anchors[np.maximum(indices, 0)]
    age = times - anchor[:, 0]
    seconds = (anchor[:, 1] - data["media_epoch_ms"] + age / 1000) / 1000
    return np.where((indices >= 0) & (age <= 750000), seconds, np.nan)


def longest(flags):
    runs = [(len(list(g)), k) for k, g in itertools.groupby(flags)]
    start = 0
    best = {"duration_ms": 0, "start_s": None}
    for count, flag in runs:
        if flag and count * 4 > best["duration_ms"]:
            best = {"duration_ms": count * 4, "start_s": start * 0.004}
        start += count
    return best


def pcm_clock(out, data, epoch):
    raw = np.memmap(out / "vlc.s16le", dtype="<i2", mode="r")
    end = data.get("end_us", data["frames"][-1][0] + 1e6 / FPS)
    pcm = np.zeros(round((end - epoch) * RATE / 1e6), dtype="<i2")
    covered = np.zeros(len(pcm), dtype=np.bool_)
    overlaps = 0
    events = iter(data.get("audio_events", []))
    event = next(events, None)

    def discard_queued(at):
        index = max(0, round((at - epoch) * RATE / 1e6))
        pcm[index:] = 0
        covered[index:] = False

    with (out / "pcm.csv").open() as f:
        for row in csv.DictReader(f):
            pos = round((int(row["presentation_us"]) - epoch) * RATE / 1e6)
            count, offset = int(row["sample_count"]), int(row["sample_offset"])
            while event and event[3] <= offset:
                if event[1] == "flush":
                    discard_queued(event[0])
                event = next(events, None)
            lo, hi = max(0, pos), min(len(pcm), pos + count)
            if hi > lo:
                overlaps += int(covered[lo:hi].sum())
                pcm[lo:hi] = raw[offset + lo - pos : offset + hi - pos]
                covered[lo:hi] = True
    while event:
        if event[1] == "flush":
            discard_queued(event[0])
        event = next(events, None)
    return pcm, covered, overlaps


def detect_tones(blocks):
    """Measure both tones and silence in complete four-millisecond PCM blocks."""
    angles = 2 * np.pi * np.arange(WINDOW)[:, None] * np.array([750, 1500]) / RATE
    basis = (2 / (WINDOW * 32768) * np.exp(1j * angles)).astype(np.complex64)
    amplitudes = np.concatenate(
        [np.abs(part.astype(np.float32) @ basis) for part in np.array_split(blocks, 100)]
    )
    rms = np.concatenate(
        [
            np.sqrt(np.mean((part.astype(np.float32) / 32768) ** 2, axis=1))
            for part in np.array_split(blocks, 100)
        ]
    )
    return amplitudes > 10 ** (-36 / 20), rms < 10 ** (-48 / 20)


def switch_silence(covered, silent, missing_ranges, command, onset):
    """Count silent samples and whole missing-PCM runs attributable to this switch."""
    lo = max(0, round(command * RATE))
    # The first identified new-tone block can start with a few missing samples.
    hi = min(len(silent) * WINDOW, round(onset * RATE) + WINDOW)
    if hi <= lo:
        return 0, 0
    blocks = silent[lo // WINDOW:(hi + WINDOW - 1) // WINDOW]
    quiet = np.repeat(blocks, WINDOW)[lo % WINDOW:lo % WINDOW + hi - lo]
    quiet_samples = int((quiet | ~covered[lo:hi]).sum())
    # A dropout that began before the command or continued beyond the new
    # tone remains chargeable outside the switch, including any partial block.
    gaps = missing_ranges[(missing_ranges[:, 0] >= lo) & (missing_ranges[:, 1] <= hi)]
    return quiet_samples, int((gaps[:, 1] - gaps[:, 0]).sum())


def audio_checks(out, data, epoch, pcm, covered, overlaps, tones, silent, active):
    count = len(pcm) // WINDOW
    a, b = tones.T
    ticks = np.arange(len(a)) * WINDOW / RATE
    # Map presentation time to programme time even during stalls and drops.
    # Audio latency and actual silence duration remain clock-time measurements.
    media = player_seconds(data, epoch + ticks * 1e6)
    commands = data["switches"]
    rows = []
    worst = None
    permitted = np.zeros(len(a), dtype=bool)
    missing_ranges = np.flatnonzero(np.diff(np.r_[False, ~covered, False])).reshape(-1, 2)
    allowed_missing = 0
    for i, entry in enumerate(commands, 1):
        planned = entry["planned_s"]
        target = 2 if i % 2 else 1
        command = (entry["command_us"] - epoch) / 1e6
        # Require a genuine transition and two consecutive new-tone windows.
        onset = None
        lo = max(1, int(command / 0.004) - 2)
        # A late switch still needs a measured latency. Stop at the next
        # command so a later selection cannot be credited to this one.
        hi = len(active) - 1
        if i < len(commands):
            next_command = (commands[i]["command_us"] - epoch) / 1e6
            hi = min(hi, int(next_command * RATE / WINDOW))
        old = 3 - target
        was_old = False
        for j in range(lo, hi):
            was_old |= active[j - 1] == old
            if was_old and active[j] == active[j + 1] == target:
                onset = j * 0.004
                break
        latency = None if onset is None else onset - command
        quiet_samples, missing = (0, 0) if onset is None else switch_silence(
            covered, silent, missing_ranges, command, onset
        )
        quiet_s = quiet_samples / RATE
        active_delay = None if latency is None else max(0, round(latency * RATE) - quiet_samples) / RATE
        passed = (
            entry["result"] == 0
            and onset is not None
            and latency >= -0.004
            and quiet_s <= MAX_SWITCH_SILENCE
            and active_delay <= MAX_NON_SILENT_LATENCY
            and abs(entry["presented_s"] - planned) <= 1 / FPS
        )
        if (
            entry["result"] == 0
            and latency is not None
            and (worst is None or latency > worst["seconds"])
        ):
            worst = {"seconds": latency, "planned_s": planned, "onset_s": onset}
        if passed:
            permitted |= (ticks + WINDOW / RATE > command) & (ticks < onset + WINDOW / RATE)
            allowed_missing += missing
        rows.append(
            dict(
                **entry,
                command_s=command,
                selection_command_s=command if entry["result"] is not None else None,
                action=(
                    "submitted"
                    if entry["result"] is not None
                    else "track-unavailable"
                    if entry.get("target_id") is None
                    else "already-selected"
                ),
                onset_s=onset,
                onset_player_s=float(media[round(onset / 0.004)]) if onset is not None else None,
                latency_s=latency,
                switch_silence_ms=quiet_s * 1000 if onset is not None else None,
                switch_missing_samples=missing if onset is not None else None,
                non_silent_latency_s=active_delay,
                passed=bool(passed),
            )
        )
    clock_valid = np.isfinite(media)
    expected = np.where((np.nan_to_num(media) // 10).astype(int) % 2, 2, 1)
    present = covered[: count * WINDOW].reshape(-1, WINDOW).all(axis=1)
    detail = {
        "switches": rows,
        "longest_silence": longest(silent),
        "longest_silent_delivered_pcm": longest(silent & present),
        "longest_missing_pcm": longest(~present),
        "longest_overlap": longest(a & b),
        "longest_switch": worst,
        "missing_pcm_samples": int((~covered).sum()),
        "missing_pcm_outside_accepted_switches": int((~covered).sum()) - allowed_missing,
        "longest_silence_outside_accepted_switches": longest(silent & ~permitted),
        "switch_silence_limit_ms": MAX_SWITCH_SILENCE * 1000,
        "non_silent_switch_latency_limit_ms": MAX_NON_SILENT_LATENCY * 1000,
        "overlapping_pcm_samples": overlaps,
        "unexpected_windows": int(((active != expected) & ~permitted).sum()),
        "audio_ids": data["audio_ids"],
        "missing_clock_windows": int((~clock_valid).sum()),
    }
    if len(rows) == 59 and all(r["result"] == 0 and r["onset_s"] is not None for r in rows):
        # Keep the whole-window observations and a separate tone inventory
        # for blocks lying wholly outside the command-to-onset intervals.
        between = np.ones(len(ticks), dtype=bool)
        for row in rows:
            between &= (ticks + WINDOW / RATE <= row["command_s"]) | (ticks >= row["onset_s"])
        selections = np.searchsorted([row["command_s"] for row in rows], ticks, side="right")
        wanted = 1 + selections % 2  # Initially A, alternating requests for B and A.
        detail["between_switches"] = {
            "clock": "whole 4 ms presentation blocks outside actual command-to-onset intervals",
            "blocks": int(between.sum()),
            "wrong_or_absent_tone": int((between & (active != wanted)).sum()),
            "silent": int((between & silent).sum()),
            "dual_tone": int((between & a & b).sum()),
            "incomplete_pcm": int((between & ~present).sum()),
        }
    dump(out / "audio.json", detail)
    passed = (
        data["complete"]
        and clock_valid.all()
        and len(data.get("initial_audio_ids", [])) == 2
        and data.get("initial_audio_id") == data["initial_audio_ids"][0]
        and len(rows) == 59
        and all(r["passed"] for r in rows)
        and len(data["audio_ids"]) == 2
        and not detail["unexpected_windows"]
        and detail["missing_pcm_outside_accepted_switches"] == 0
        and not overlaps
        and not any(
            e[1] == "pause" and epoch <= e[0] < epoch + len(pcm) / RATE * 1e6
            for e in data.get("audio_events", [])
        )
        and detail["longest_silence_outside_accepted_switches"]["duration_ms"] <= 20
        and detail["longest_overlap"]["duration_ms"] <= 4
    )
    return bool(passed)


def subtitle_labels(out, frames, ffmpeg):
    # Build an independent visual vocabulary from the source's real 888 PES.
    # Never label a received image using its expected position in the run.
    source = next((out / "library").glob("*.ts"))
    graph = "[0:s:0]scale=640:360,format=gray,select='eq(mod(n,20),4)',showinfo"
    command = [
        ffmpeg,
        "-hide_banner",
        "-nostdin",
        "-fix_teletext_pts",
        "0",
        "-txt_page",
        "888",
        "-txt_format",
        "bitmap",
        "-i",
        source,
        "-filter_complex",
        graph,
        "-fps_mode",
        "passthrough",
        "-an",
        "-f",
        "rawvideo",
        "pipe:1",
    ]
    ref = subprocess.run(command, capture_output=True, check=True)
    stamps = [
        float(x) for x in re.findall(rb" n:\s*\d+ pts:\s*-?\d+ pts_time:([\d.]+)", ref.stderr)
    ]
    images = np.frombuffer(ref.stdout, np.uint8).reshape(-1, 360, 640)
    if len(stamps) != len(images):
        raise ValueError("subtitle template timestamps do not match decoded images")

    def glyphs(mask):
        # Teletext digits occupy separate columns. One digit can acquire an
        # extra antialiased top row in VLC without moving its neighbour.
        columns = np.flatnonzero(mask.any(axis=0))
        if not len(columns):
            return []
        result = []
        for xs in np.split(columns, np.flatnonzero(np.diff(columns) > 1) + 1):
            char = Image.fromarray(np.uint8(mask[:, xs[0] : xs[-1] + 1]) * 255)
            char = char.crop(char.getbbox())
            pixels = np.asarray(char.resize((30, 36), Image.Resampling.BILINEAR), dtype=float) / 255
            result.append((pixels, char.width / char.height))
        return result

    vocabulary = set()
    templates = {}
    for stamp, img in zip(stamps, images):
        value = int(stamp // 5) * 5
        if not 0 < value < 300:
            continue
        word = str(value)
        parts = glyphs(img > 128)
        if len(parts) != len(word):
            raise ValueError("ambiguous digit boundaries in independent source template")
        vocabulary.add(word)
        for digit, (pixels, aspect) in zip(word, parts):
            templates.setdefault(digit, {})[(pixels.tobytes(), aspect)] = (
                np.pad(pixels, ((3, 3), (2, 2))), aspect
            )
    if len(vocabulary) != 59 or len(templates) != 10:
        raise ValueError("incomplete independent source subtitle vocabulary")
    labels = {}
    confidence = {}
    for key in {r[3] for r in frames}:
        mask = np.asarray(Image.open(out / "subtitle-masks" / (key + ".png")).convert("1"))
        digits = []
        confidence[key] = []
        for pixels, aspect in glyphs(mask):
            scores = []
            for digit, variants in templates.items():
                # Allow a shift of approximately one native raster pixel.
                distance = min(
                    np.abs(pixels - ref[y : y + 36, x : x + 30]).mean()
                    + 0.12 * abs(np.log(aspect / ar))
                    for ref, ar in variants.values()
                    for y in (0, 3, 6)
                    for x in (0, 2, 4)
                )
                scores.append((float(distance), digit))
            best, second = sorted(scores)[:2]
            confidence[key].append([best, second])
            digits.append(best[1] if best[0] < 0.24 and second[0] - best[0] > 0.006 else "?")
        word = "".join(digits)
        labels[key] = word if not word or word in vocabulary else None
    dump(out / "subtitle-template-distances.json", confidence)
    dump(out / "subtitle-labels.json", labels)
    return labels


def subtitle_checks(out, data, frames, ffmpeg):
    labels = subtitle_labels(out, frames, ffmpeg)
    wrong = []
    changes = []
    displayed = {}
    media_changes = []
    media = player_seconds(data, [row[0] for row in frames])
    previous = None
    for (_, frame, _, key), timestamp in zip(frames, media):
        if frame < 0:
            continue
        actual = labels[key]
        displayed[frame] = actual
        if actual != previous:
            changes.append((frame / FPS, actual))
            if np.isfinite(timestamp):
                media_changes.append((float(timestamp), actual))
            previous = actual
        n = frame % FRAMES
        expected = str(n // (5 * FPS) * 5) if n >= 5 * FPS else ""
        # Allow one presentation frame after a cue boundary to show the old
        # text, including clearing 295 at the second airing.
        if actual != expected:
            prior = ((n - 1) % FRAMES) // (5 * FPS) * 5
            if frame == 0 or n % (5 * FPS) or actual != (str(prior) if prior else ""):
                wrong.append((frame, expected, actual))
    boundaries = []
    for second in range(0, 600, 5):
        frame = second * FPS
        expected = str(second % 300) if second % 300 else ""
        onsets = [t for t, value in changes if value == expected and abs(t - second) < 2.5]
        onset = min(onsets, key=lambda t: abs(t - second), default=None)
        delta = None if onset is None else round((onset - second) * FPS)
        native = [t for t, value in media_changes if value == expected and abs(t - second) < 2.5]
        native_onset = min(native, key=lambda t: abs(t - second), default=None)
        native_frames = None if native_onset is None else round((native_onset - second) * FPS)
        boundaries.append(
            {
                "planned_s": second,
                "expected": expected,
                "boundary": displayed.get(frame),
                "next_frame": displayed.get(frame + 1),
                "first_visible_s": onset,
                "offset_frames": delta,
                "player_media_s": native_onset,
                "player_offset_frames": native_frames,
                "within_one_frame": delta in (0, 1) and native_frames in (0, 1),
            }
        )
    dump(
        out / "subtitle-events.json",
        {"changes": changes, "boundaries": boundaries, "wrong_frames": wrong},
    )
    if (
        not data["page_selected"]
        or not data.get("initial_page_selected", False)
        or data.get("page_losses")
        or any(row[2] is not None for row in wrong)
    ):
        return "FAIL"
    if any(s is None for s in labels.values()):
        return "NIET GEMETEN"
    return (
        "PASS"
        if (
            data["complete"]
            and np.isfinite(media).all()
            and all(b["within_one_frame"] for b in boundaries)
        )
        else "FAIL"
    )


def video_checks(out, data, frames):
    previous = -1
    issues = []
    end_position = None
    if data.get("end_us") and data.get("media_epoch_ms") is not None:
        measured = player_seconds(data, [data["end_us"]])[0]
        if np.isfinite(measured):
            end_position = float(measured)
    for i, (now, n, timer, _) in enumerate(frames):
        gap_ms = (now - frames[i - 1][0]) / 1000 if i else 0
        if n != previous + 1 or n < 0 or timer != counters(n % FRAMES)[1] or gap_ms > 100:
            issues.append(
                {"index": i, "frame": n, "previous": previous, "time_text": timer, "gap_ms": gap_ms}
            )
        previous = n
    numbers = np.asarray([r[1] for r in frames])
    holds = []
    for n, group in itertools.groupby(enumerate(frames), key=lambda item: item[1][1]):
        rows = list(group)
        if n >= 0 and len(rows) > 1:
            first = rows[0][1][0]
            following = rows[-1][0] + 1
            end = (
                frames[following][0]
                if following < len(frames)
                else data.get("end_us", rows[-1][1][0])
            )
            holds.append(
                {
                    "frame": n,
                    "start_s": (first - frames[0][0]) / 1e6,
                    "duration_ms": (end - first) / 1000,
                }
            )
    dump(
        out / "video.json",
        {
            "presentations": len(frames),
            "player_duration_s": end_position,
            "errors": issues,
            "holds": holds,
            "repeated": int((np.diff(numbers) == 0).sum()),
            "reversed": int((np.diff(numbers) < 0).sum()),
            "max_presentation_interval_ms": max(
                (b[0] - a[0]) / 1000 for a, b in itertools.pairwise(frames)
            )
            if len(frames) > 1
            else None,
            "missing": sorted(set(range(FRAMES * 2)) - {r[1] for r in frames}),
            "buffering": data["events"],
            "http_connections": data["http_connections"],
        },
    )
    return (
        data["complete"]
        and end_position is not None
        and abs(end_position - DURATION) <= 1 / FPS
        and len(frames) == 2 * FRAMES
        and not issues
        and not data["events"]
        and data["http_connections"] == 1
    )


def transition_clip(out, ffmpeg, frames, epoch, pcm):
    pics = sorted((out / "transition").glob("*.png"))
    if len(pics) < 2:
        return False
    times = [frames[int(p.stem)][0] for p in pics]
    start = max(0, round((times[0] - epoch) * RATE / 1e6))
    stop = min(len(pcm), round((times[-1] + 1e6 / FPS - epoch) * RATE / 1e6))
    if stop <= start:
        return False
    folder = out / "transition"
    (folder / "audio.s16le").write_bytes(pcm[start:stop].tobytes())
    lines = ["ffconcat version 1.0"]
    for i, p in enumerate(pics):
        lines += [
            f"file {p.name}",
            "option framerate 1000",
            "duration %.6f" % ((times[i + 1] - times[i]) / 1e6 if i + 1 < len(times) else 1 / FPS),
        ]
    lines += ["file " + pics[-1].name, "option framerate 1000"]
    (folder / "frames.ffconcat").write_text("\n".join(lines) + "\n")
    subprocess.run(
        [
            ffmpeg,
            "-y",
            "-v",
            "error",
            "-safe",
            "0",
            "-f",
            "concat",
            "-i",
            "frames.ffconcat",
            "-f",
            "s16le",
            "-ar",
            str(RATE),
            "-ac",
            "1",
            "-i",
            "audio.s16le",
            "-fps_mode",
            "vfr",
            "-c:v",
            "ffv1",
            "-c:a",
            "pcm_s16le",
            "transition.mkv",
        ],
        cwd=folder,
        check=True,
    )
    dump(
        folder / "measurements.json",
        {
            "frames": [frames[int(p.stem)] for p in pics],
            "switch": next(
                (
                    s
                    for s in json.loads((out / "audio.json").read_text())["switches"]
                    if s["planned_s"] == 300
                ),
                None,
            ),
        },
    )
    return True


def capture_bounds(path, probe, ffmpeg):
    # Decode the same received bytes, recover frame zero from pixels, and use
    # its video PES position to cut a TS interval without a second HTTP client.
    cmd = [
        probe,
        "-v",
        "error",
        "-select_streams",
        "v:0",
        "-show_frames",
        "-show_entries",
        "frame=best_effort_timestamp_time,pkt_pos",
        "-of",
        "json",
        path,
    ]
    meta = json.loads(run(cmd))["frames"]
    glyphs = make_templates()
    prev = None
    starts = []
    visible = []
    cmd = [
        ffmpeg,
        "-v",
        "error",
        "-i",
        path,
        "-map",
        "0:v:0",
        "-an",
        "-sn",
        "-vf",
        "scale=640:360,crop=112:44:0:0,format=gray",
        "-fps_mode",
        "passthrough",
        "-f",
        "rawvideo",
        "pipe:1",
    ]
    size = 112 * 44
    with subprocess.Popen(cmd, stdout=subprocess.PIPE) as child:
        for row in meta:
            raw = child.stdout.read(size)
            if len(raw) != size:
                raise ValueError("decoded frame count differs from frame timestamp inventory")
            img = np.frombuffer(raw, np.uint8).reshape(44, 112)
            text = read_line(img, 10, 4, glyphs)
            if text.isdigit():
                n = int(text)
                if n == 0 and prev is not None and prev > FRAMES - 2 * FPS:
                    starts.append((float(row["best_effort_timestamp_time"]), int(row["pkt_pos"])))
                prev = n
            if 1 <= len(starts) <= 2:
                visible.append((len(starts) - 1) * FRAMES + int(text) if text.isdigit() else -1)
        if child.stdout.read(1) or child.wait():
            raise ValueError("extra decoded frames or decoder failure")
    video_pts = {}
    if len(starts) >= 3:
        values = [
            float(row["best_effort_timestamp_time"])
            for row in meta
            if starts[0][1] <= int(row["pkt_pos"]) < starts[2][1]
        ]
        steps = np.diff(values)
        video_pts = {
            "frames": len(values),
            "min_step_s": float(min(steps, default=0)),
            "max_step_s": float(max(steps, default=0)),
            "irregular_steps": int(((steps < 0.5 / FPS) | (steps > 1.5 / FPS)).sum()),
            "missing_counters": sorted(set(range(2 * FRAMES)) - set(visible)),
            "repeated_counters": int((np.diff(visible) == 0).sum()),
            "reversed_counters": int((np.diff(visible) < 0).sum()),
        }
    return starts, video_pts


def inspect_hls_pair(out, tools):
    snapshot_hls_pair(out, out / "hls-pair")
    segments = []
    for stage in ("hls-start", "hls-pair"):
        folder = out / stage
        for file in sorted(folder.glob("live*.ts")):
            _, _, page, audio = inspect_ts(file, folder, file.stem, tools)
            segments.append(
                {
                    "stage": stage,
                    "segment": file.name,
                    "ac3_pids": [s["id"] for s in audio],
                    "page888": page,
                }
            )
    return segments


def declared_tracks(pmt):
    audio, subtitles = set(), set()
    for component in pmt.findall("component"):
        pid = int(component.attrib["elementary_PID"], 0)
        stream_type = int(component.attrib["stream_type"], 0)
        ac3 = component.find("AC3_descriptor") is not None or any(
            int(reg.attrib["format_identifier"], 0) == 0x41432D33
            for reg in component.findall("registration_descriptor")
        )
        if stream_type == 0x81 or (stream_type == 0x06 and ac3):
            audio.add(pid)
        if stream_type == 0x06 and any(
            page.get("page_number") == "888" and int(page.attrib["teletext_type"], 0) in (2, 5)
            for page in component.findall("teletext_descriptor/teletext")
        ):
            subtitles.add(pid)
    return audio, subtitles


def transport_checks(out, tools):
    source = out / "received.ts"
    try:
        diagnostic = inspect_hls_pair(out, tools)
    except (OSError, ValueError, KeyError, IndexError, subprocess.SubprocessError) as exc:
        diagnostic = {"status": "NIET GEMETEN", "error": str(exc)}
    dump(out / "hls-segments.json", diagnostic)
    _probe, analysis, page, tracks = inspect_ts(source, out, "received", tools)
    starts, video_pts = capture_bounds(source, tools["ffprobe"], tools["ffmpeg"])
    dump(out / "loop-pts.json", starts)
    if len(starts) < 3:
        return "FAIL" if not page or len(tracks) != 2 else "NIET GEMETEN"
    lo, hi = starts[0][1], starts[2][1]
    if not (0 <= lo < hi <= source.stat().st_size and lo % 188 == hi % 188 == 0):
        raise ValueError("invalid transport window offsets")
    window = out / "window.ts"
    with source.open("rb") as f, window.open("wb") as dst:
        f.seek(lo)
        remaining = hi - lo
        while remaining:
            part = f.read(min(remaining, 4 * 1024**2))
            if not part:
                raise EOFError("short transport window")
            dst.write(part)
            remaining -= len(part)
    probe, analysis, page, tracks = inspect_ts(window, out, "window", tools)
    video = next((s for s in probe["streams"] if s.get("codec_type") == "video"), {})
    expected_video = {
        "codec_name": "h264",
        "width": 1920,
        "height": 1080,
        "field_order": "progressive",
        "r_frame_rate": "25/1",
        "color_space": "bt709",
        "color_transfer": "bt709",
    }
    video_format = {k: video.get(k) for k in expected_video}
    # Inventory alone cannot prove that the same audio PIDs stayed declared.
    ids = {int(s["id"], 0) for s in tracks}
    # Keep the compact version inventory as well as a packet-indexed record
    # of every declaration, including same-version mapping changes.
    all_pmts = out / "window-pmt-all.xml"
    run(
        [
            "tstables",
            "--pack-all-sections",
            "--packet-index",
            "--pid",
            str(probe["programs"][0]["pmt_pid"]),
            "--xml-output",
            all_pmts,
            window,
        ]
    )
    tables = ET.parse(all_pmts).getroot().findall("PMT")
    declared = [declared_tracks(table) for table in tables]
    page_ids = declared[0][1] if declared else set()
    page = page and bool(page_ids) and all(subs == page_ids for _, subs in declared)
    audio_stable = (
        len(ids) == 2
        and len({s.get("channel_layout") for s in tracks}) == 1
        and all(s.get("sample_rate") == "48000" for s in tracks)
        and bool(tables)
        and all(audio == ids for audio, _ in declared)
    )
    audio_timestamps = {}
    ts_clock = 90_000
    first_tick, end_tick = (round(starts[j][0] * ts_clock) for j in (0, 2))
    for i, track in enumerate(tracks):
        if track["time_base"] != "1/90000":
            raise ValueError("unexpected MPEG-TS audio time base")
        packets = json.loads(
            run(
                [
                    tools["ffprobe"],
                    "-v",
                    "error",
                    "-select_streams",
                    f"a:{i}",
                    "-show_packets",
                    "-show_entries",
                    "packet=pts",
                    "-of",
                    "json",
                    source,
                ]
            )
        )["packets"]
        all_ticks = [p["pts"] for p in packets if "pts" in p]
        ticks = [pts for pts in all_ticks if first_tick <= pts < end_tick]
        steps = np.diff(np.asarray(ticks, dtype=np.int64))
        # AC-3 carries 1,536 samples per frame at the declared 48 kHz.
        # A monotone one-tick DTS repair can still duplicate a whole audio
        # frame at the playout seam. TSDuck's leap counter does not catch it.
        # ffprobe's decimal pts_time rounds to microseconds. Subtracting two
        # rounded values can misclassify a permitted two-tick deviation.
        expected_step = 1536 * ts_clock // RATE
        irregular = np.flatnonzero(np.abs(steps - expected_step) > 2)
        # A packet starting just before frame zero can still overlap audio in
        # the measured interval. A final packet can also leave a gap before
        # the exclusive end. Check the portions inside the window only.
        boundary_errors = []
        if all_ticks:
            for left, right in ((first_tick, all_ticks[0]), (all_ticks[-1] + expected_step, end_tick)):
                start, stop = max(left, first_tick), min(right, end_tick)
                if stop - start > 2:
                    boundary_errors.append({
                        "kind": "gap", "start_tick_relative": start - first_tick,
                        "duration_ticks": stop - start,
                    })
        for left, right in itertools.pairwise(all_ticks):
            if left < first_tick <= right or left < end_tick <= right:
                early, late = sorted((left + expected_step, right))
                inside = min(late, end_tick) - max(early, first_tick)
                if inside > 2:
                    boundary_errors.append({
                        "kind": "gap" if right > left + expected_step else "overlap",
                        "start_tick_relative": max(early, first_tick) - first_tick,
                        "duration_ticks": inside,
                    })
        audio_timestamps[track["id"]] = {
            "packets": len(ticks),
            "min_step_ticks": int(min(steps, default=0)),
            "max_step_ticks": int(max(steps, default=0)),
            "min_step_s": float(min(steps, default=0)) / ts_clock,
            "max_step_s": float(max(steps, default=0)) / ts_clock,
            "nonmonotonic": int((steps <= 0).sum()),
            "boundary_errors": boundary_errors,
            "irregular_steps": [
                {"at_s": (ticks[j] - first_tick) / ts_clock, "step_s": int(steps[j]) / ts_clock}
                for j in irregular
            ],
        }
        audio_stable &= bool(
            len(ticks) > 1
            and ticks[0] - first_tick < ts_clock // 10
            and end_tick - ticks[-1] < ts_clock // 10
            and not len(irregular)
            and not boundary_errors
        )
    errors = {
        k: sum(p["packets"].get(k, 0) for p in analysis["pids"])
        for k in ("discontinuities", "pts-leap", "dts-leap", "pcr-leap")
    }
    errors.update(
        {k: analysis["ts"]["packets"].get(k, 0) for k in ("invalid-syncs", "transport-errors")}
    )
    findings = []
    profile = json.loads((out / "profile.json").read_text())["profile"]
    reference_bps = (profile["VideoBitrate"] + profile["AudioBitrate"]) * 1200
    pcr_counts = None
    for plugin in ("continuity", "pcrverify"):
        args = (
            ["--bitrate", str(reference_bps), "--jitter-max", "8000"]
            if plugin == "pcrverify"
            else []
        )
        result = subprocess.run(
            ["tsp", "-v", "-I", "file", window, "-P", plugin, *args, "-O", "drop"],
            capture_output=True,
            text=True,
            check=True,
        )
        (out / (plugin + ".log")).write_text(result.stderr)
        if plugin == "continuity":
            findings.extend(
                line
                for line in result.stderr.splitlines()
                if re.search(
                    r"continuity:.*(discontinuity|missing|duplicate|error)", line, re.IGNORECASE
                )
            )
        else:
            counts = re.search(r"([\d,]+) (?:PCR )?OK, ([\d,]+) with jitter", result.stderr)
            if counts:
                pcr_counts = [int(x.replace(",", "")) for x in counts.groups()]
    # Verify source-time text as well as the PMT. Preserve raw extracted cue
    # receipts so subtitle timing and renderer timing can be reviewed separately.
    cue_pass = False
    if page:
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
                window,
                "-map",
                "0:s:0",
                "-c:s",
                "srt",
                "-f",
                "srt",
                "pipe:1",
            ]
        )
        (out / "received-888.srt").write_text(text)
        changes = []
        for h, m, s, ms, v in re.findall(r"(\d+):(\d+):(\d+),(\d+) --> [^\n]+\n(\d+)", text):
            at = int(h) * 3600 + int(m) * 60 + int(s) + int(ms) / 1000 - starts[0][0]
            if 0 <= at < 600 and (not changes or changes[-1][1] != int(v)):
                changes.append((at, int(v)))
        cue_pass = len(changes) == 118 and all(
            v == j and abs(t - (cycle * 300 + j)) <= 1 / FPS
            for (t, v), (cycle, j) in zip(
                changes, ((c, j) for c in range(2) for j in range(5, 300, 5))
            )
        )
    pmt_offsets = [
        int(tab.find("metadata").attrib["first_ts_packet"].replace(",", "")) for tab in tables
    ]
    pmt_intervals = np.diff([0, *pmt_offsets, (hi - lo) // 188]) * 188 * 8 / reference_bps
    max_pmt_gap = float(max(pmt_intervals, default=600))
    dump(
        out / "transport-verdict.json",
        {
            "errors": errors,
            "ac3_pids": [s["id"] for s in tracks],
            "pmt_repetitions": len(tables),
            "stable_ac3_declarations": bool(declared) and all(audio == ids for audio, _ in declared),
            "page888_pids": sorted(hex(pid) for pid in page_ids),
            "max_pmt_interval_s": max_pmt_gap,
            "video_timestamps": video_pts,
            "video_format": video_format,
            "audio_timestamps": audio_timestamps,
            "mux_reference_bps": reference_bps,
            "pcr_ok_jitter": pcr_counts,
            "page888": page,
            "timed_cues": cue_pass,
            "caption_pts_status": ("PASS" if cue_pass else "FAIL") if page else "NIET GEMETEN",
            "network_arrival_jitter": "NIET GEMETEN",
            "programme_starts": starts,
            "continuity_findings": findings,
        },
    )
    ok = (
        audio_stable
        and video_format == expected_video
        and page
        and cue_pass
        and pcr_counts is not None
        and pcr_counts[0] > 0
        and pcr_counts[1] == 0
        and not any(errors.values())
        and not findings
        and max_pmt_gap <= 0.5
        and video_pts["frames"] == 2 * FRAMES
        and not video_pts["irregular_steps"]
        and all(abs(starts[i][0] - starts[0][0] - i * 300) <= 1 / FPS for i in (1, 2))
    )
    return "PASS" if ok else "FAIL"


def switch_flushes(out, data):
    flushes = [event for event in data.get("audio_events", []) if event[1] == "flush"]
    offsets = {event[3] for event in flushes}
    ends = {}
    with (out / "pcm.csv").open() as stream:
        for row in csv.DictReader(stream):
            count = int(row["sample_count"])
            offset = int(row["sample_offset"]) + count
            if offset in offsets:
                ends[offset] = int(row["presentation_us"]) + count * 1e6 / RATE
    timings = []
    for command in data["switches"]:
        if command["result"] != 0:
            continue
        issued = command["command_us"]
        event = next((e for e in flushes if 0 <= e[0] - issued <= 1e6 / FPS), None)
        until = ends.get(event[3]) if event else None
        timings.append({
            "planned_s": command["planned_s"],
            "command_to_flush_ms": (event[0] - issued) / 1000 if event else None,
            "last_queued_buffer_end_after_flush_ms": (until - event[0]) / 1000 if until is not None else None,
        })
    return timings


def broadcast_audio(out, ffmpeg, data):
    """Locate audible gaps upstream or downstream of the receiver's track selector."""
    starts = json.loads((out / "loop-pts.json").read_text())
    selections = json.loads((out / "audio.json").read_text())["switches"]
    transport = json.loads((out / "transport-verdict.json").read_text())
    pids = transport["ac3_pids"]
    code = json.loads((out / "source-validation.json").read_text()).get("audio_timecode", {})
    if len(pids) != 2:
        return {"status": "NIET GEMETEN", "reason": "two received AC-3 tracks required", "pids": pids}
    report = []
    source = out / "received.ts"
    for track, hz in enumerate((750, 1500)):
        decoded = subprocess.run(
            [
                ffmpeg, "-nostdin", "-nostats", "-v", "info", "-copyts", "-i", source,
                "-map", f"0:a:{track}", "-af",
                "aformat=sample_fmts=s16:sample_rates=48000:channel_layouts=mono,"
                "asettb=1/48000,ashowinfo",
                "-f", "s16le", "pipe:1",
            ],
            check=True,
            capture_output=True,
        )
        samples = np.frombuffer(decoded.stdout, dtype="<i2")
        # Keep the decoded frames' own PTS. Appending PCM and assuming a
        # uniform byte clock would hide overlapping timestamps at a handoff.
        frames = re.findall(
            rb"Parsed_ashowinfo[^\n]*?\bpts:(-?\d+) [^\n]*?nb_samples:(\d+)", decoded.stderr
        )
        times = []
        for pts, size in frames:
            if int(size) % WINDOW:
                raise ValueError("partial 4 ms block in received AC-3 frame")
            times.append(int(pts) + np.arange(0, int(size), WINDOW))
        if not times or sum(len(t) for t in times) * WINDOW != len(samples):
            raise ValueError("decoded broadcast PCM and frame timestamps disagree")
        time_s = (np.concatenate(times) - round(starts[0][0] * RATE)) / RATE
        tones, silent = detect_tones(samples.reshape(-1, WINDOW))
        expected = tones[:, track] & ~tones[:, 1 - track]
        whole = (time_s >= 0) & (time_s + WINDOW / RATE <= DURATION)
        rows = []
        for cmd in selections:
            if cmd.get("onset_player_s") is None or cmd.get("result") != 0:
                continue
            span = (time_s >= cmd["presented_s"]) & (time_s + WINDOW / RATE <= cmd["onset_player_s"])
            rows.append({
                "planned_s": cmd["planned_s"], "blocks": int(span.sum()),
                "expected_tone_blocks": int((span & expected).sum()),
                "silent_blocks": int((span & silent).sum()),
            })
        report.append({
            "pid": pids[track], "frequency_hz": hz, "blocks": int(whole.sum()),
            "unexpected_block_starts_s": time_s[whole & ~expected].tolist(),
            "silent_block_starts_s": time_s[whole & silent].tolist(), "switches": rows,
        })
        # The two-second silence allowance belongs to the VLC receiver.
        # Retain the original short interruption limits on both broadcast PIDs.
        unexpected = longest((~expected)[whole])["duration_ms"]
        dual = longest((tones[:, 0] & tones[:, 1])[whole])["duration_ms"]
        wrong = int((whole & tones[:, 1 - track] & ~tones[:, track]).sum())
        report[-1].update(longest_unexpected_ms=unexpected, longest_dual_ms=dual,
                          wrong_tone_blocks=wrong, passed=bool(whole.any() and unexpected <= 20 and dual <= 4 and wrong == 0))
        if code.get("verified") and transport["video_timestamps"]["irregular_steps"] == 0:
            report[-1]["av_offset"] = transport_offsets(samples, time_s, expected)
    return {
        "status": "PASS" if all(track["passed"] for track in report) else "FAIL",
        "clock": "decoded received-TS sample PTS relative to video frame zero",
        "block_ms": 4, "tone_dbfs": -36, "silence_rms_dbfs": -48, "tracks": report,
        "vlc_switch_flushes": switch_flushes(out, data),
    }


def evaluate(out, tools, data, result, max_av_offset_ms=None):
    frames = data["frames"]
    epoch = data["epoch_us"]
    errors = {}
    result["broadcast_audio"] = "NIET GEMETEN"

    def measure(name, check, *args):
        try:
            verdict = check(*args)
            result[name] = verdict if isinstance(verdict, str) else ("PASS" if verdict else "FAIL")
        except (OSError, ValueError, KeyError, IndexError, subprocess.SubprocessError) as exc:
            errors[name] = repr(exc)
            result[name] = "NIET GEMETEN"

    pcm = covered = active = None
    sync = {}

    def audio():
        nonlocal pcm, covered, active
        if not (out / "vlc.s16le").stat().st_size:
            return False
        pcm, covered, overlaps = pcm_clock(out, data, epoch)
        count = len(pcm) // WINDOW
        tones, silent = detect_tones(pcm[: count * WINDOW].reshape(-1, WINDOW))
        a, b = tones.T
        active = np.where(a & ~b, 1, np.where(b & ~a, 2, 0))
        return audio_checks(out, data, epoch, pcm, covered, overlaps, tones, silent, active)

    def av_offset():
        nonlocal sync
        if active is None:
            return "NIET GEMETEN"
        switches = json.loads((out / "audio.json").read_text())["switches"]
        sync = measure_offset(out, data, pcm, covered, active, switches, max_av_offset_ms)
        dump(out / "av-offset.json", sync)
        return offset_verdict(sync)

    def transition():
        if pcm is None or not transition_clip(out, tools["ffmpeg"], frames, epoch, pcm):
            return "NIET GEMETEN"
        report = out / "transition" / "measurements.json"
        detail = json.loads(report.read_text())
        detail["av_offset_after_switch"] = next(
            (s for s in sync.get("switches", []) if s["planned_s"] == 300), None
        )
        info = json.loads((out / "subtitle-labels.json").read_text())
        seam = [r for r in frames if FRAMES - 1 <= r[1] <= FRAMES + 1]
        detail["boundary_frames"] = [
            {
                "frame": r[1],
                "clip_time": r[2],
                "subtitle": info[r[3]],
                "elapsed_s": (r[0] - epoch) / 1e6,
            }
            for r in seam
        ]
        dump(report, detail)
        return (
            data["page_selected"]
            and [r[1] for r in seam] == [FRAMES - 1, FRAMES, FRAMES + 1]
            and bool((detail["switch"] or {}).get("passed"))
            and info[seam[0][3]] == "295"
            and info[seam[-1][3]] == ""
            and seam[1][2] == "00:00.00"
        )

    if epoch is not None and frames:
        measure("video", video_checks, out, data, frames)
        measure("audio", audio)
        measure("av_offset_measurement", av_offset)
        measure("teletext", subtitle_checks, out, data, frames, tools["ffmpeg"])
        measure("transition", transition)
    else:
        result["video"] = "FAIL"
    measure("transport", transport_checks, out, tools)
    if all((out / name).is_file() for name in ("audio.json", "loop-pts.json", "transport-verdict.json")):
        try:
            diagnostic = broadcast_audio(out, tools["ffmpeg"], data)
        except (ValueError, OSError, IndexError, subprocess.SubprocessError) as exc:
            diagnostic = {"status": "NIET GEMETEN", "error": str(exc)}
        dump(out / "broadcast-audio.json", diagnostic)
        result["broadcast_audio"] = diagnostic["status"]
    if errors:
        result["analysis_errors"] = errors


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("results", type=Path)
    parser.add_argument("--ffmpeg", default="ffmpeg")
    parser.add_argument("--ffprobe", default="ffprobe")
    parser.add_argument("--max-av-offset-ms", type=offset_limit)
    a = parser.parse_args()
    out = a.results.resolve()
    result = dict.fromkeys(
        ("video", "audio", "teletext", "transport", "transition", "av_offset_measurement"), "NIET GEMETEN"
    )
    evaluate(out, vars(a), json.loads((out / "receiver.json").read_text()), result, a.max_av_offset_ms)
    result["total"] = (
        "PASS"
        if all(v == "PASS" for v in result.values())
        else ("FAIL" if "FAIL" in result.values() else "NIET GEMETEN")
    )
    dump(out / "result.json", result)
    print(json.dumps(result))
    sys.exit(result["total"] != "PASS")
