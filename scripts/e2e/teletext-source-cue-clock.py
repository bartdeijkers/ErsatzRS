#!/usr/bin/env python3
"""Source-coupled qualification for the fixed page888 recovery boundary trial.

The maintainer approved this 8ms cue criterion on 2026-11-13. It replaces only
constant subtitle packet/PCR lead in this opt-in trial. Raw transport checks,
native timestamp repair, capture bounds and receiver-deadline limitations stay.
"""

import argparse
import collections
import csv
import hashlib
import json
import subprocess
from fractions import Fraction
from pathlib import Path

SOURCE_SHA256 = "e70b7cf1f0c18cd5c1b8316f4b1e6f8a8bca45282a30514f451796d9debfa05e"
LIMIT_TICKS = 720  # Existing 8ms, never widened by caller configuration.


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def read(path, maximum):
    with path.open("rb") as stream:
        raw = stream.read(maximum + 1)
    require(len(raw) <= maximum, "input exceeds its fixed byte bound")
    return raw


def run(command):
    result = subprocess.run(command, capture_output=True, timeout=30, check=False)
    require(result.returncode == 0, "native diagnostic failed")
    require(len(result.stdout) <= 4 * 1024 * 1024, "native diagnostic exceeds its bound")
    return result.stdout


def packets(probe, path, selector):
    return json.loads(run([probe, "-v", "error", "-fix_teletext_pts", "0",
                          "-select_streams", selector, "-show_packets", "-show_entries",
                          "packet=pts,dts,data_hash,size", "-show_data_hash", "sha256",
                          "-of", "json", str(path)]))["packets"]


def frames(probe, path, selector, source=False):
    options = ["-txt_page", "888", "-txt_format", "bitmap", "-txt_default_region", "-1"] if source else []
    return json.loads(run([probe, "-v", "error", "-fix_teletext_pts", "1" if source else "0",
                          *options, "-select_streams", selector, "-show_frames",
                          "-show_entries", "frame=pts,num_rects", "-of", "json", str(path)]))["frames"]


def native_tick(microseconds):
    tick = (microseconds * 90000 + 500000) // 1000000
    require((tick * 1000000 + 45000) // 90000 == microseconds,
            "native cue does not identify one source clock tick")
    return tick


def subtitle_pes(raw, pid):
    result, pending = [], None
    for ordinal in range(len(raw) // 188):
        packet = raw[ordinal * 188:(ordinal + 1) * 188]
        require(packet[0] == 0x47 and not packet[1] & 0x80, "invalid raw transport framing")
        if ((packet[1] & 31) << 8 | packet[2]) != pid or not packet[3] & 0x10:
            continue
        offset = 4 + (1 + packet[4] if packet[3] & 0x20 else 0)
        require(offset < 188, "invalid subtitle payload offset")
        if packet[1] & 0x40:
            require(pending is None, "subtitle PES was truncated")
            pending = {"first_packet": ordinal, "body": bytearray()}
        require(pending is not None, "subtitle capture starts inside a PES")
        pending["body"].extend(packet[offset:])
        body = pending["body"]
        if len(body) < 14:
            continue
        require(body[:4] == b"\x00\x00\x01\xbd", "invalid subtitle PES start")
        size = 6 + int.from_bytes(body[4:6], "big")
        require(14 <= size <= 65541 and body[7] & 0x80 and body[8] >= 5,
                "invalid subtitle PES length or timestamp flags")
        if len(body) < size:
            continue
        stamp = body[9:14]
        require(stamp[0] >> 4 in (2, 3) and all(stamp[i] & 1 for i in (0, 2, 4)),
                "invalid subtitle PTS marker bits")
        pts = ((stamp[0] >> 1 & 7) << 30) | (stamp[1] << 22) | ((stamp[2] >> 1) << 15) | (stamp[3] << 7) | (stamp[4] >> 1)
        result.append({"first_packet": pending["first_packet"], "last_packet": ordinal,
                       "pts": pts, "pes_sha256": digest(body[:size])})
        pending = None
    require(pending is None and result, "missing or terminal incomplete subtitle PES")
    return result


def clock_rows(path, pid, count, flags, first, final):
    rows = []
    for row in csv.reader(read(path, 4 * 1024 * 1024).decode().splitlines()):
        row = [value.strip() for value in row]
        if row and row[0] == str(pid):
            require(len(row) == 8 and row[3] == "PTS", "invalid subtitle clock row")
            rows.append((int(row[1]), int(row[5]), int(row[7])))
    require(len(rows) == count and count > 1, "subtitle clock count differs from raw analysis")
    require(rows[0][1] == first and rows[-1][1] == final, "subtitle clock endpoints differ")
    require(all(abs(offset) <= 270000 for _, _, offset in rows), "subtitle exceeds existing 3s PCR bound")
    gaps = 0
    for previous, current in zip(rows, rows[1:]):
        require(current[1] >= previous[1], "subtitle PTS runs backwards")
        gaps += current[1] - previous[1] > 270000
    require(gaps == flags, "raw subtitle cadence flags are not fully explained")
    return rows


def match_epochs(source_video, capture_video, source_audio, capture_audio):
    by_hash = collections.defaultdict(list)
    for index, packet in enumerate(source_video):
        by_hash[packet["data_hash"]].append((index, packet))
    epochs, dts_differences = [], []
    for ordinal, packet in enumerate(capture_video):
        matches = by_hash[packet["data_hash"]]
        require(len(matches) == 1, "video access unit has no unique source identity")
        index, original = matches[0]
        shift = packet["pts"] - original["pts"]
        if not epochs or epochs[-1]["shift"] != shift:
            epochs.append({"shift": shift, "first_index": index, "last_index": index - 1, "count": 0})
        epoch = epochs[-1]
        require(index == epoch["last_index"] + 1, "source video units skip or reorder")
        epoch.update(last_index=index, count=epoch["count"] + 1)
        if packet["dts"] - original["dts"] != shift:
            dts_differences.append({"capture_index": ordinal, "pts_shift": shift,
                                    "dts_shift": packet["dts"] - original["dts"]})
    require(len(epochs) == 2 and epochs[0]["first_index"] > 0 and epochs[1]["first_index"] == 0,
            "capture does not prove the required source programme rollover")
    by_pts = {packet["pts"]: packet for packet in source_audio}
    seam = min(by_pts) + epochs[1]["shift"]
    audio_counts = [0, 0]
    for packet in capture_audio:
        epoch = int(packet["pts"] >= seam)
        shift = epochs[epoch]["shift"]
        original = by_pts.get(packet["pts"] - shift)
        require(original and original["data_hash"] == packet["data_hash"]
                and packet["dts"] - original["dts"] == shift, "selected audio differs from its source clock")
        audio_counts[epoch] += 1
    require(all(audio_counts) and any(packet["pts"] == seam for packet in capture_audio),
            "selected audio does not identify the actual next-programme start")
    return epochs, seam, audio_counts, dts_differences


def qualify_cues(events, source_events, epochs, seam, first_av, final_av):
    require([event["num_rects"] > 0 for event in source_events] == [True, True, False],
            "fixed source does not contain A, B and its explicit clear")
    source_cues = [(native_tick(event["pts"]), event["num_rects"] > 0) for event in source_events]
    require(first_av - epochs[0]["shift"] >= source_cues[-1][0],
            "old programme is not independently clear at capture start")
    expected = []
    for index, epoch in enumerate(epochs):
        start, end = (first_av, seam) if index == 0 else (seam, final_av + 1)
        expected.extend(tick + epoch["shift"] for tick, visible in source_cues
                        if visible and start <= tick + epoch["shift"] < end)
    actual = [event["pts"] for event in events if event["num_rects"] > 0]
    require(len(actual) == len(expected) == 1, "visible source cue coverage differs at the rollover")
    require(all(abs(a - b) <= LIMIT_TICKS for a, b in zip(actual, expected)),
            "visible cue differs from native-effective source timing by more than 8ms")
    visible = False  # The old programme has already explicitly cleared page888.
    for event in events:
        if event["num_rects"] > 0:
            visible = True
            continue
        epoch = epochs[int(event["pts"] >= seam)]
        source_time = event["pts"] - epoch["shift"]
        state = False
        for tick, source_visible in source_cues:
            if tick <= source_time:
                state = source_visible
        require(not state and not visible, "empty PES clears a visible or still-required source cue")
    return {"expected_visible_pts": expected, "actual_visible_pts": actual,
            "visible_error_us": [float(Fraction(a - b, 90000) * 1000000) for a, b in zip(actual, expected)]}


def main(args):
    source, raw, complete = read(args.source, 40 * 1024 * 1024), read(args.capture, 8 * 1024 * 1024), read(args.complete_av, 8 * 1024 * 1024)
    require(digest(source) == SOURCE_SHA256, "source is not the unchanged qualified burst fixture")
    require(raw and len(raw) <= 8 * 1024 * 1024 and len(raw) % 188 == 0, "raw capture exceeds its unchanged bound")
    require(raw.startswith(complete) and len(complete) % 188 == 0 and len(complete) > len(raw) * 9 // 10,
            "AV proof is not the original complete-PES capture prefix")
    rows = clock_rows(args.clock_csv, args.pid, args.count, args.flags, args.first, args.final)
    pes = subtitle_pes(raw, args.pid)
    streams = json.loads(run([args.ffprobe, "-v", "error", "-show_streams", "-of", "json", str(args.capture)]))["streams"]
    captions = [stream for stream in streams if int(stream["id"], 0) == args.pid and stream["codec_name"] == "dvb_subtitle"]
    require(len(captions) == 1, "raw PID is not one selected DVB subtitle stream")
    decoded = frames(args.ffprobe, args.capture, str(captions[0]["index"]))
    require(len(pes) == len(rows) == len(decoded), "subtitle PES/clock/decoder counts differ")
    for packet, row, frame in zip(pes, rows, decoded):
        require((packet["first_packet"], packet["pts"]) == row[:2], "subtitle clock rows differ from raw PES")
        require(native_tick(frame["pts"]) == packet["pts"],
                "decoded subtitle time differs from raw PES")
        packet["num_rects"] = frame["num_rects"]
        packet["raw_pts_minus_pcr_ticks"] = row[2]
    video, audio = packets(args.ffprobe, args.complete_av, "v:0"), packets(args.ffprobe, args.complete_av, "a:0")
    epochs, seam, audio_counts, dts = match_epochs(packets(args.ffprobe, args.source, "v:0"), video,
                                                 packets(args.ffprobe, args.source, "3"), audio)
    proof = qualify_cues(pes, frames(args.ffprobe, args.source, "2", True), epochs, seam,
                         min(packet["pts"] for packet in video + audio), max(packet["pts"] for packet in video + audio))
    report = {"schema": "ersatzrs.fixed-source-cue-clock/1", "source_sha256": digest(source),
              "checker_sha256": digest(Path(__file__).read_bytes()),
              "capture_sha256": digest(raw), "raw_bytes": len(raw), "complete_av_prefix_bytes": len(complete),
              "pid": args.pid, "raw_cadence_flags": args.flags, "qualified_cadence_flags": args.flags,
              "limit_us": 8000, "native_timestamp_policy_unchanged": True, "epochs": epochs,
              "video_packets": len(video), "audio_epoch_counts": audio_counts,
              "existing_dts_shift_differences": dts, "subtitle_pes": pes, **proof,
              "scope": "Source-coupled cue timing only; raw flags retained, no client delivery-deadline conformance claim"}
    args.report.write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("source", "capture", "complete-av", "clock-csv", "report"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--ffprobe", required=True)
    for name in ("pid", "count", "flags", "first", "final"):
        parser.add_argument("--" + name, type=int, required=True)
    options = parser.parse_args()
    try:
        main(options)
    except (ValueError, KeyError, OSError, subprocess.SubprocessError) as error:
        options.report.write_text(json.dumps({"qualified": False, "failure": str(error)}) + "\n")
        raise SystemExit("source cue qualification failed: " + str(error)) from error
