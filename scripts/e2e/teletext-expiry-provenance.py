#!/usr/bin/env python3
"""Qualify owner-bound ACK coordinates in the synthetic native expiry trial.

This is an offline, resource-free pages 200/201 transport and provenance check.
It does not decode Level 2.5 presentation, certify client pixels/navigation, move
the immutable UTC expiry or add a grace period. Independent suffix decoding and
the headed-client trial remain separate acceptance evidence.
"""

import argparse
from bisect import bisect_right
import calendar
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import re
import sys
import uuid

TARGET = "ersatzrs_web::transport::iptv::teletext_provenance:"
MAX_LOG = 4 * 1024 * 1024
MAX_EXPIRY = 16 * 1024
MAX_CAPTURE = 48 * 1024 * 1024
PACKET = 188
VIEWERS = (("day", 1), ("observer", 1), ("night", 2))
HAM = (0x15, 0x02, 0x49, 0x5e, 0x64, 0x73, 0x38, 0x2f,
       0xd0, 0xc7, 0x8c, 0x9b, 0xa1, 0xb6, 0xfd, 0xea)
REVERSE = bytes(int(f"{value:08b}"[::-1], 2) for value in range(256))
FIELD = re.compile(r'(?<!\w)([a-z_][a-z_0-9]*)=("[^"\\]*(?:\\.[^"\\]*)*"|Some\([^)]*\)|[^ \t}\r\n]+)')
UTC = re.compile(r"([0-9]{4})-([0-9]{2})-([0-9]{2})T([0-9]{2}):([0-9]{2}):([0-9]{2})(?:\.([0-9]{1,9}))?(?:Z|\+00:00)\Z")


class ProofError(RuntimeError):
    """Content-free proof rejection; no log lines or authored payloads escape."""


def require(condition, message):
    if not condition:
        raise ProofError(message)


def integer(value, minimum=0):
    return type(value) is int and minimum <= value <= (1 << 64) - 1


def digest(body):
    return hashlib.sha256(body).hexdigest()


def unique_object(pairs):
    value = {}
    for key, item in pairs:
        require(key not in value, "duplicate JSON field")
        value[key] = item
    return value


def bounded(path, maximum, reject_cap=False):
    with path.open("rb") as stream:
        body = stream.read(maximum + 1)
    require(0 < len(body) <= maximum and (not reject_cap or len(body) < maximum),
            "evidence input exceeds its non-truncating bound")
    return body


def utc_ns(value):
    require(type(value) is str, "UTC timestamp is not a string")
    match = UTC.fullmatch(value)
    require(match is not None, "UTC timestamp is not exact nanosecond RFC3339")
    try:
        instant = datetime(*(int(match[index]) for index in range(1, 7)), tzinfo=timezone.utc)
    except ValueError as error:
        raise ProofError("UTC timestamp components are invalid") from error
    fraction = int((match[7] or "").ljust(9, "0"))
    return calendar.timegm(instant.utctimetuple()) * 1_000_000_000 + fraction


def canonical_uuid(value):
    require(type(value) is str, "owner UUID is not a string")
    try:
        parsed = uuid.UUID(value)
    except ValueError as error:
        raise ProofError("owner UUID is invalid") from error
    require(parsed.int != 0 and str(parsed) == value, "owner UUID is not canonical")
    return value


def scalar(raw):
    if raw.startswith('"'):
        try:
            return json.loads(raw)
        except ValueError as error:
            raise ProofError("invalid quoted log field") from error
    if raw.startswith("Some("):
        return scalar(raw[5:-1])
    if raw == "None":
        return None
    if raw in ("true", "false"):
        return raw == "true"
    if re.fullmatch(r"[0-9]+", raw):
        require(len(raw) <= 20, "log integer exceeds its bound")
        value = int(raw)
        require(integer(value), "log integer exceeds its bound")
        return value
    return raw


def parse_log(body):
    require(0 < len(body) < MAX_LOG, "log reached its truncation cap")
    try:
        text = body.decode("utf-8")
    except UnicodeError as error:
        raise ProofError("log is not complete UTF-8") from error
    require(text.endswith("\n") and "output truncated" not in text.lower(),
            "log is incomplete or explicitly truncated")
    events = []
    for line_number, line in enumerate(text.splitlines(), 1):
        if TARGET not in line:
            continue
        require(line.count(TARGET) == 1, "ambiguous provenance log target")
        fields = {}
        for key, raw in FIELD.findall(line):
            value = scalar(raw)
            require(not key.startswith("_"), "log uses a reserved parser field")
            require(key not in fields or (type(fields[key]) is type(value) and fields[key] == value),
                    "span and event fields disagree")
            fields[key] = value
        require(type(fields.get("event")) is str, "provenance event lacks its explicit kind")
        for key in ("request_trace", "intent_id", "proposal_slot", "output_epoch"):
            if key in fields:
                canonical_uuid(fields[key])
        require("observed_utc" in fields or fields["event"] in
                ("ordinary_live_intent", "monitor_owner_link"),
                "timed provenance event lacks observed UTC")
        if "observed_utc" in fields:
            fields["_utc_ns"] = utc_ns(fields["observed_utc"])
        if "viewer_index" in fields:
            require(integer(fields["viewer_index"]) and fields["viewer_index"] < 3,
                    "trial viewer index is outside its exact bound")
        if "channel_id" in fields:
            require(integer(fields["channel_id"], 1), "channel identity is not a positive integer")
        fields["_line"] = line_number
        events.append(fields)
    require(events, "log contains no exact provenance events")
    return events


def one(values, message):
    require(len(values) == 1, message)
    return values[0]


def owners(events, expiry):
    """Join explicit request -> intent -> slot -> epoch; hashes never pick an owner."""
    result = []
    for index, (viewer, channel) in enumerate(VIEWERS):
        intent = one([event for event in events if event["event"] == "ordinary_live_intent"
                      and event.get("viewer_index") == index], "viewer lacks one original request/intent")
        require(intent.get("channel_id") == channel, "viewer intent has the wrong channel")
        canonical_uuid(intent.get("request_trace"))
        canonical_uuid(intent.get("intent_id"))
        linked = one([event for event in events if event["event"] == "monitor_owner_link"
                      and event.get("intent_id") == intent["intent_id"]], "intent lacks one monitor owner link")
        require(linked.get("channel_id") == channel, "monitor owner link changes channel")
        for key in ("viewer_index", "request_trace"):
            require(key not in linked or linked[key] == intent[key], "monitor link changes request identity")
        slot = canonical_uuid(linked.get("proposal_slot"))
        group = [event for event in events if event.get("proposal_slot") == slot]
        initial = one([event for event in group if event["event"] == "monitor_initial"],
                      "owner lacks one original initial candidate")
        require(initial.get("channel_id") == channel and initial.get("generation_id") ==
                expiry["base_generation_ids"][channel - 1], "owner initial candidate is not the actual Base")
        epochs = {event["output_epoch"] for event in group if "output_epoch" in event}
        require(len(epochs) == 1, "owner has missing or multiple output epochs")
        epoch = next(iter(epochs))
        result.append({"viewer": viewer, "viewer_index": index, "channel_id": channel,
                       "request_trace": intent["request_trace"], "intent_id": intent["intent_id"],
                       "proposal_slot": slot, "output_epoch": epoch,
                       "initial_generation_id": initial["generation_id"], "events": group})
    for key in ("request_trace", "intent_id", "proposal_slot", "output_epoch"):
        require(len({owner[key] for owner in result}) == 3, "viewers collide on an explicit owner identity")
    owned_slots = {owner["proposal_slot"] for owner in result}
    require(all(event.get("proposal_slot") in owned_slots for event in events
                if "proposal_slot" in event), "log contains an unassigned foreign owner")
    return result


def qualified_windows(owner, raw):
    events = owner["events"]
    count = len(raw) // PACKET
    prepared = [event for event in events if event["event"] == "sink_window_prepared"]
    written = [event for event in events if event["event"] == "sink_window_first_packet_written"]
    flushed = [event for event in events if event["event"] == "sink_window_flushed"]
    require(prepared, "owner has no prepared windows")
    for events_of_kind in (prepared, written, flushed):
        positions = [event.get("first_packet") for event in events_of_kind]
        require(all(integer(position) for position in positions) and len(set(positions)) == len(positions),
                "owner repeats or omits an exact window coordinate")
    writes = {event["first_packet"]: event for event in written}
    flushes = {event["first_packet"]: event for event in flushed}
    require(set(writes) <= {event["first_packet"] for event in prepared}
            and set(flushes) <= {event["first_packet"] for event in prepared},
            "foreign write or ACK lacks a prepared window")
    windows, ignored = [], 0
    expected = 0
    terminal = False
    for event in sorted(prepared, key=lambda event: event["first_packet"]):
        first, end = event["first_packet"], event.get("end_packet")
        require(integer(end, 1) and first < end and integer(event.get("active_sequence")),
                "prepared window extent or sequence is invalid")
        require(first == expected, "prepared windows overlap or leave a coordinate gap")
        expected = end
        write, flush = writes.get(first), flushes.get(first)
        if first >= count and flush is None:
            terminal = True
            ignored += 1
            continue
        fully_retained = end <= count
        require(not terminal and write is not None and (flush is not None or not fully_retained),
                "retained window lacks a first write or complete flush")
        terminal = not fully_retained
        for joined in (write, flush):
            if joined is None:
                continue
            require(joined.get("output_epoch") == owner["output_epoch"]
                    and joined.get("active_sequence") == event["active_sequence"],
                    "write/ACK changes window epoch or active sequence")
        require(event["_line"] < write["_line"] and event["_utc_ns"] <= write["_utc_ns"],
                "window first write precedes its preparation")
        require(flush is None or (flush.get("end_packet") == end and write["_line"] < flush["_line"]
                and write["_utc_ns"] <= flush["_utc_ns"]),
                "window prepared/write/flushed extents or ordering disagree")
        expected_hash = write.get("first_packet_sha256")
        require(type(expected_hash) is str and re.fullmatch(r"[0-9a-f]{64}", expected_hash),
                "first-written packet digest is missing")
        require(digest(raw[first * PACKET:(first + 1) * PACKET]) == expected_hash,
                "assigned viewer packet hash fails at its exact unadjusted offset")
        if flush is not None:
            for flag in ("cycle_boundary", "complete_cycle", "signalling_announced", "authored_selected"):
                require(type(flush.get(flag)) is bool, "ACK lacks an explicit boundary flag")
        windows.append({"first": first, "end": end, "sequence": event["active_sequence"],
                        "prepared": event, "written": write, "flushed": flush,
                        "fully_retained": fully_retained})
    require(windows and windows[-1]["end"] >= count, "retained capture is not wholly associated with owned windows")
    return windows, ignored


def transitions(owner, windows, expiry):
    group = owner["events"]
    proposals = [event for event in group if event["event"] == "monitor_proposal"]
    installations = [event for event in group if event["event"] == "transition_installed"]
    sequences = [event.get("sequence") for event in proposals]
    require(all(integer(sequence, 1) for sequence in sequences) and len(set(sequences)) == len(sequences),
            "proposal sequences are missing or ambiguous")
    by_sequence = {event["sequence"]: event for event in proposals}
    require(len({event.get("sequence") for event in installations}) == len(installations),
            "sequence has multiple actual installations")
    for event in installations:
        proposal = by_sequence.get(event.get("sequence"))
        require(proposal is not None and event.get("action") == proposal.get("action")
                and event["_line"] > proposal["_line"], "installation lacks its actual proposal")
        matching = [window for window in windows if window["fully_retained"] and window["end"] == event.get("end_packet")]
        ack = one(matching, "installation ACK is foreign or outside retained capture")
        require(event.get("output_epoch") == owner["output_epoch"]
                and event.get("cycle_boundary") is True
                and ack["flushed"]["cycle_boundary"] is True
                and event.get("complete_cycle") == ack["flushed"]["complete_cycle"]
                and event.get("signalling_announced") == ack["flushed"]["signalling_announced"]
                and integer(event.get("accepted_elapsed_ns"))
                and event["accepted_elapsed_ns"] >= ack["flushed"].get("accepted_elapsed_ns", -1)
                and event["_utc_ns"] >= ack["flushed"]["_utc_ns"]
                and event["_line"] > ack["flushed"]["_line"],
                "installation does not share its exact completed flush ACK")
    installed = {event["sequence"]: event for event in installations}
    for window in windows:
        prior = [event for event in installations if event["end_packet"] <= window["first"]]
        latest = max(prior, key=lambda event: event["end_packet"]) if prior else None
        require(window["sequence"] == (latest["sequence"] if latest else 0),
                "window active sequence is not the actual installed sequence")
    if owner["channel_id"] == 2:
        require(not proposals and not installations, "night observer acquired an unrelated transition")
        return None
    source_ns = utc_ns(expiry["original_valid_until"])
    scoped = one([event for event in proposals if event.get("generation_id") == expiry["scoped_generation_id"]
                  and event.get("action") == "replace"], "owner lacks one actual Scoped proposal")
    require(scoped.get("channel_id") == 1 and scoped.get("design_id") == expiry["scoped_design_id"]
            and scoped.get("revision") == expiry["scoped_revision"], "Scoped proposal changes saved identity")
    recovery = [event for event in group if event["event"] == "monitor_recovered"
                and event.get("generation_id") == expiry["scoped_generation_id"]]
    require(recovery and all(utc_ns(event.get("original_valid_until")) == source_ns for event in recovery),
            "Scoped recovery changes immutable expiry")
    scoped_install = installed.get(scoped["sequence"])
    require(scoped_install is not None and scoped_install["_utc_ns"] < source_ns,
            "Scoped proposal lacks an installation before immutable expiry")
    boundary = one([event for event in group if event["event"] == "scope_boundary_expired"],
                   "owner lacks one original UTC boundary observation")
    require(boundary.get("generation_id") == expiry["scoped_generation_id"]
            and boundary["_utc_ns"] >= source_ns, "observed expiry boundary changes generation or UTC")
    base = one([event for event in proposals if event.get("generation_id") == expiry["base_generation_ids"][0]
                and event.get("action") == "replace" and event["_utc_ns"] >= source_ns],
               "owner lacks one actual post-expiry Base proposal")
    base_install = installed.get(base["sequence"])
    require(base_install is not None, "Base proposal lacks its actual installation ACK")
    require(base_install["_utc_ns"] >= source_ns and base_install["end_packet"] > scoped_install["end_packet"],
            "Base installation does not follow immutable Scoped expiry")
    base_ack = one([window for window in windows if window["fully_retained"] and window["end"] == base_install["end_packet"]],
                   "Base installation lacks its exact retained ACK")
    scoped_cut = base_install["end_packet"]
    path = "direct_base_at_complete_scoped_ack"
    if base_ack["sequence"] != scoped["sequence"] or not base_ack["flushed"]["complete_cycle"]:
        withdrawn = one([event for event in installations if event.get("action") == "withdraw"
                         and scoped_install["end_packet"] < event["end_packet"] < base_install["end_packet"]
                         and event["_utc_ns"] >= source_ns], "Base requires one actual preceding Scoped withdrawal")
        withdrawal_ack = one([window for window in windows if window["fully_retained"] and window["end"] == withdrawn["end_packet"]],
                             "Scoped withdrawal lacks its exact retained ACK")
        require(withdrawal_ack["sequence"] == scoped["sequence"] and withdrawal_ack["flushed"]["complete_cycle"]
                and base_ack["sequence"] == withdrawn["sequence"],
                "Base installation does not follow complete Scoped withdrawal closure")
        absent = [window for window in windows if window["fully_retained"]
                  and withdrawn["end_packet"] <= window["first"]
                  and window["end"] <= base_install["end_packet"]]
        require(absent and absent[0]["first"] == withdrawn["end_packet"]
                and absent[-1]["end"] == base_install["end_packet"]
                and all(window["sequence"] == withdrawn["sequence"]
                        and window["flushed"]["authored_selected"] is False for window in absent)
                and all(before["end"] == after["first"] for before, after in zip(absent, absent[1:]))
                and any(window["flushed"]["signalling_announced"] for window in absent),
                "Base installation lacks contiguous physically absent windows with a preceding PSI ACK")
        scoped_cut = withdrawn["end_packet"]
        path = "scoped_complete_ack_then_absent_psi_ack_then_base"
    return {"scoped_sequence": scoped["sequence"], "scoped_install": scoped_install,
            "base_sequence": base["sequence"], "base_install": base_install,
            "scoped_ack_cut_packet": scoped_cut, "ack_path": path}


def nibble(byte):
    reversed_byte = REVERSE[byte]
    require(reversed_byte in HAM, "noncanonical Hamming address or header byte")
    return HAM.index(reversed_byte)


def pes_units(body, first, end, complete=True):
    require(len(body) >= 184 and len(body) <= 65_504 and len(body) % 184 == 0
            and body[:4] == b"\x00\x00\x01\xbd"
            and (int.from_bytes(body[4:6], "big") + 6 == len(body) if complete else
                 len(body) < int.from_bytes(body[4:6], "big") + 6 <= 65_504),
            "teletext PES framing or declared length is invalid")
    require(body[6] == 0x84 and body[7] == 0x80 and body[8] == 0x24
            and body[9] & 0xf1 == 0x21 and body[11] & 1 and body[13] & 1
            and body[14:45] == b"\xff" * 31 and body[45] == 0x10,
            "synthetic teletext PES header, PTS or identifier is invalid")
    require((len(body) - 46) % 46 == 0, "teletext PES has a partial data unit")
    result = []
    for ordinal, offset in enumerate(range(46, len(body), 46)):
        unit = body[offset:offset + 46]
        require(unit[1] == 44, "synthetic teletext data unit length is invalid")
        if unit[0] == 0xff:
            require(unit[2:] == b"\xff" * 44, "teletext stuffing unit is malformed")
            continue
        require(unit[0] == 2 and unit[2] == 0xe0 and unit[3] == 0xe4,
                "trial data unit is not ordinary teletext framing")
        coded_address = nibble(unit[4]) | nibble(unit[5]) << 4
        magazine, row = coded_address & 7, coded_address >> 3
        require(magazine == 2 and row <= 29, "trial contains a foreign magazine or packet")
        value = {"pes_first": first, "pes_end": end, "unit_ordinal": ordinal,
                 "row": row, "magazine": magazine}
        if row == 0:
            header = [nibble(byte) for byte in unit[6:14]]
            number = header[0] | header[1] << 4
            subcode = header[2] | (header[3] & 7) << 4 | header[4] << 8 | (header[5] & 3) << 12
            require((number in (0x00, 0x01) and subcode == 0)
                    # packets::terminator uses the non-wildcard mFF/0000
                    # closing header, suppressed rolling header and serial mode.
                    or (number == 0xff and header == [15, 15, 0, 0, 0, 0, 1, 1]),
                    "trial contains a resource or foreign page/subpage")
            value.update(page=200 + number if number != 0xff else None,
                         terminator=number == 0xff, c4=bool(header[3] & 8))
        elif 1 <= row <= 25:
            decoded = bytes(REVERSE[byte] for byte in unit[6:])
            require(all(byte.bit_count() % 2 == 1 for byte in decoded), "base row odd parity is invalid")
            value["cells"] = bytes(byte & 0x7f for byte in decoded)
        result.append(value)
    return result


def decode_capture(raw, allow_terminal_partial=False):
    """Strict PID512 PUSI/continuity/PES framing with exact source packet coordinates."""
    require(raw and len(raw) <= MAX_CAPTURE and len(raw) % PACKET == 0,
            "capture does not contain bounded complete TS packets")
    units, packets, pending = [], [], bytearray()
    expected_cc = None
    first = expected_length = None
    for index, offset in enumerate(range(0, len(raw), PACKET)):
        packet = raw[offset:offset + PACKET]
        require(packet[0] == 0x47 and packet[1] & 0x80 == 0, "capture TS sync or transport error")
        if ((packet[1] & 31) << 8 | packet[2]) != 512:
            continue
        require(packet[3] & 0xf0 == 0x10, "synthetic PID512 must use clear payload-only TS")
        cc = packet[3] & 15
        require(expected_cc is None or cc == expected_cc, "PID512 continuity counter is discontinuous")
        expected_cc = (cc + 1) % 16
        if packet[1] & 0x40:
            require(not pending, "PUSI interrupted an incomplete teletext PES")
            first = index
            expected_length = None
        else:
            require(pending, "PID512 continuation precedes its exact PUSI")
        pending.extend(packet[4:])
        if expected_length is None:
            require(pending[:4] == b"\x00\x00\x01\xbd", "PID512 PUSI is not private-stream PES")
            expected_length = int.from_bytes(pending[4:6], "big") + 6
            require(184 <= expected_length <= 65_504 and expected_length % 184 == 0,
                    "teletext PES length is outside its strict bounded framing")
        require(len(pending) <= expected_length, "PID512 packet exceeds declared PES length")
        if len(pending) == expected_length:
            units.extend(pes_units(bytes(pending), first, index + 1))
            packets.append({"first": first, "end": index + 1})
            pending.clear()
    partial = None
    if pending:
        require(allow_terminal_partial, "capture ends with an incomplete teletext PES")
        partial = {"first": first, "end": len(raw) // PACKET,
                   "retained_pes_bytes": len(pending), "declared_pes_bytes": expected_length}
        units.extend(pes_units(bytes(pending), first, partial["end"], complete=False))
        packets.append({"first": first, "end": partial["end"], "complete": False})
    require(packets, "capture contains no teletext PES")
    return packets, units, partial


def cycle_proof(owner, windows, packets, units, transition, expiry, partial_pes=None):
    starts = [window["first"] for window in windows]
    def containing(packet):
        index = bisect_right(starts, packet) - 1
        require(index >= 0 and packet < windows[index]["end"], "raw PES lacks an exact prepared window")
        return windows[index]

    expiry_ns = utc_ns(expiry["original_valid_until"])
    cut = transition["base_install"]["end_packet"] if transition else None
    if cut is not None:
        for boundary in (cut, transition.get("scoped_ack_cut_packet", cut)):
            require(all(not (pes["first"] < boundary < pes["end"]) for pes in packets), "teletext PES crosses qualified ACK cut")
    if partial_pes is not None:
        require(cut is None or partial_pes["first"] >= cut, "partial retained PES precedes qualified Base cut")
        require(containing(partial_pes["first"])["sequence"] == (transition["base_sequence"] if transition else 0),
                "partial retained PES is not the actual terminal Base sequence")
    cycles, current, page = [], None, None
    terminal_cycle = None
    for ordinal, unit in enumerate(units):
        window = containing(unit["pes_first"])
        sequence = window["sequence"]
        require(containing(unit["pes_end"] - 1)["sequence"] == sequence,
                "raw teletext PES crosses installed active sequences")
        if current is None:
            current = {"first_pes_packet": unit["pes_first"], "first_prepared_window": window["first"],
                       "prepared_utc": window["prepared"]["observed_utc"],
                       "prepared_utc_ns": window["prepared"]["_utc_ns"],
                       "sequence": sequence, "pages": [], "first200_packet": None,
                       "first200_c4": None, "first200_prepared_utc": None, "markers": []}
        require(sequence == current["sequence"], "logical cycle crosses actual installed sequences")
        if unit["row"] == 0 and unit["terminator"]:
            require(current["pages"] == [200, 201], "trial cycle lacks ordered complete pages200/201")
            matching = [window for window in windows if window["fully_retained"] and window["first"] <= unit["pes_first"]
                       and unit["pes_end"] <= window["end"] and window["flushed"]["complete_cycle"]
                       and window["flushed"]["cycle_boundary"]]
            if not matching:
                require(ordinal == len(units) - 1 and (cut is None or current["first_pes_packet"] >= cut)
                        and sequence == (transition["base_sequence"] if transition else 0),
                        "raw FF terminator lacks its exact complete-cycle flush boundary")
                terminal_cycle = {**current, "raw_ff_seen": True, "complete_ack_retained": False}
                current, page = None, None
                continue
            ack = one(matching, "raw FF terminator has ambiguous complete-cycle ACK")
            current["end_packet"] = ack["end"]
            current["ff_pes_packet"] = unit["pes_first"]
            cycles.append(current)
            current, page = None, None
            continue
        if unit["row"] == 0:
            page = unit["page"]
            current["pages"].append(page)
            if page == 200:
                current["first200_packet"] = unit["pes_first"]
                current["first200_c4"] = unit["c4"]
                current["first200_prepared_utc"] = window["prepared"]["observed_utc"]
        elif unit["row"] in (2, 3):
            require(page is not None, "base row has no original display header")
            marker = unit["cells"][8]
            if marker in (ord("Q"), ord("R")):
                current["markers"].append({"page": page, "row": unit["row"], "column": 8,
                                            "code": chr(marker), "pes_packet": unit["pes_first"]})
                marker_cut = transition.get("scoped_ack_cut_packet", cut) if transition else None
                require(marker_cut is None or unit["pes_first"] < marker_cut, "old Q/R marker survives qualified Base ACK cut")
    if current is not None:
        require((cut is None or current["first_pes_packet"] >= cut)
                and current["sequence"] == (transition["base_sequence"] if transition else 0)
                and not current["markers"], "terminal incomplete cycle is not marker-free Base after qualified cut")
        terminal_cycle = {**current, "raw_ff_seen": False, "complete_ack_retained": False}
    require(len(cycles) >= 2, "trial capture lacks repeated complete cycles")
    scoped_cycles = []
    if transition:
        absence_start = transition.get("scoped_ack_cut_packet", cut)
        require(absence_start == cut or all(pes["end"] <= absence_start or pes["first"] >= cut for pes in packets),
                "authored PES survives the actual absent-PSI interval")
        require(any(cycle["sequence"] == transition["scoped_sequence"]
                    and any(marker["code"] == "Q" and marker["row"] == 2 for marker in cycle["markers"])
                    for cycle in cycles), "actual Scoped sequence never carries its Q marker")
        scoped_cycles = [cycle for cycle in cycles if cycle["sequence"] == transition["scoped_sequence"]]
        require(scoped_cycles and all(cycle["prepared_utc_ns"] < expiry_ns for cycle in scoped_cycles),
                "a new Scoped logical cycle was prepared at or after immutable expiry")
        restored = [cycle for cycle in cycles if cycle["first_pes_packet"] >= cut]
        require(len(restored) >= 2 and all(cycle["sequence"] == transition["base_sequence"]
                                         and not cycle["markers"] for cycle in restored),
                "post-cut capture lacks repeated original Base pages without Q/R")
        require(restored[0]["first200_c4"] is True, "first restored Base200 header lacks C4 erasure")
    else:
        require(all(cycle["sequence"] == 0 and not cycle["markers"] for cycle in cycles),
                "isolated night capture contains a Scoped marker or foreign sequence")
        restored = cycles
    # No boundary is inferred from the first200 header: M29 may begin one
    # window earlier. Every repeated logical start follows its prior raw FF/ACK.
    for previous, following in zip(cycles, cycles[1:]):
        require(following["first_pes_packet"] >= previous["end_packet"],
                "next logical cycle starts before the prior FF complete-cycle ACK")
    return {"scoped_cycles": scoped_cycles, "restored_complete_cycles": len(restored),
            "first_restored200_packet": restored[0]["first200_packet"],
            "first_restored200_c4": restored[0]["first200_c4"],
            "no_pes_crosses_ack_cut": True, "no_old_markers_after_ack_cut": True,
            "partial_pes": partial_pes, "partial_base_cycle": terminal_cycle,
            "full_capture_eof_claimed": False}


def qualify(directory):
    directory = Path(directory)
    log = bounded(directory / "stream-terminal.log", MAX_LOG, reject_cap=True)
    expiry_body = bounded(directory / "expiry.json", MAX_EXPIRY)
    try:
        expiry = json.loads(expiry_body, object_pairs_hook=unique_object)
    except (ValueError, UnicodeError) as error:
        raise ProofError("expiry evidence is not complete JSON") from error
    require(type(expiry) is dict and type(expiry.get("probe_version")) is int and expiry["probe_version"] == 1,
            "unsupported native expiry evidence")
    for key in ("scoped_generation_id", "scoped_design_id", "scoped_revision"):
        require(integer(expiry.get(key), 1), "expiry evidence lacks positive original Scoped identity")
    require(type(expiry.get("base_generation_ids")) is list and len(expiry["base_generation_ids"]) == 2
            and all(integer(value, 1) for value in expiry["base_generation_ids"])
            and len(set(expiry["base_generation_ids"] + [expiry["scoped_generation_id"]])) == 3,
            "Base/Scoped original generation identities are not distinct")
    expiry_ns = utc_ns(expiry.get("original_valid_until"))
    require(utc_ns(expiry.get("observed_utc")) >= expiry_ns,
            "raw UTC fragment observation precedes immutable expiry")
    require(type(expiry.get("source_requests_per_viewer")) is int and expiry["source_requests_per_viewer"] == 1
            and expiry.get("immutable_scope_and_publication_state_across_expiry") is True,
            "original response or immutable saved state evidence is missing")
    offsets = expiry.get("expiry_offsets")
    require(type(offsets) is list and len(offsets) == 3 and all(integer(value) and value % PACKET == 0 for value in offsets),
            "raw UTC fragment offsets are not exact packet coordinates")
    events = parse_log(log)
    assigned = owners(events, expiry)
    report = {"probe_version": 1, "scope": "owner-qualified synthetic resource-free pages200201 expiry",
              "original_valid_until": expiry["original_valid_until"], "original_expiry_utc_ns": expiry_ns,
              "raw_utc_fragment_observed_utc": expiry["observed_utc"],
              "log_sha256": digest(log), "expiry_json_sha256": digest(expiry_body),
              "immutable_utc_expiry_unchanged": True, "expiry_grace_added": False,
              "viewers": [], "limitations": ["No full Level 2.5 acceptance or independent presentation decoding",
                                               "No actual client rendering/navigation proof",
                                               "Only synthetic resource-free pages200/201 and markers row2/3,column8"]}
    for owner in assigned:
        name = owner["viewer"] + "-ordinary.ts"
        raw = bounded(directory / name, MAX_CAPTURE)
        require(len(raw) % PACKET == 0 and offsets[owner["viewer_index"]] <= len(raw),
                "retained raw UTC fragment is outside complete capture packets")
        windows, ignored = qualified_windows(owner, raw)
        transition = transitions(owner, windows, expiry)
        packets, units, partial_pes = decode_capture(raw, allow_terminal_partial=True)
        proof = cycle_proof(owner, windows, packets, units, transition, expiry, partial_pes)
        item = {key: value for key, value in owner.items() if key != "events"}
        item.update(capture_file=name, capture_bytes=len(raw), capture_sha256=digest(raw),
                    packet_count=len(raw) // PACKET, first_written_matches=len(windows),
                    flushed_windows=sum(window["fully_retained"] for window in windows), ignored_terminal_unflushed_windows=ignored,
                    retainer_tail_bytes=(len(raw) - windows[-1]["first"] * PACKET) if not windows[-1]["fully_retained"] else 0,
                    terminal_window_flushed_in_log=windows[-1]["flushed"] is not None if not windows[-1]["fully_retained"] else None,
                    raw_utc_fragment_offset_bytes=offsets[owner["viewer_index"]],
                    qualified_ack_cut_packet=transition["base_install"]["end_packet"] if transition else None,
                    qualified_ack_cut_bytes=transition["base_install"]["end_packet"] * PACKET if transition else None,
                    qualified_ack_cut_utc=transition["base_install"]["observed_utc"] if transition else None,
                    scoped_sequence=transition["scoped_sequence"] if transition else None,
                    base_sequence=transition["base_sequence"] if transition else 0,
                    scoped_ack_cut_packet=transition["scoped_ack_cut_packet"] if transition else None,
                    ack_path=transition["ack_path"] if transition else "unchanged_night_base")
        item.update(proof)
        report["viewers"].append(item)
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("evidence_directory", type=Path)
    parser.add_argument("create_new_report", type=Path)
    args = parser.parse_args()
    report = qualify(args.evidence_directory)
    encoded = (json.dumps(report, indent=2, sort_keys=True) + "\n").encode()
    with args.create_new_report.open("xb") as output:
        output.write(encoded)
    print(json.dumps({"probe_version": 1, "qualified_viewers": len(report["viewers"])}))


if __name__ == "__main__":
    try:
        main()
    except (ProofError, OSError, ValueError, KeyError, TypeError, IndexError, RecursionError) as error:
        message = str(error) if isinstance(error, ProofError) else "bounded expiry provenance inspection failed"
        print(json.dumps({"error": message}), file=sys.stderr)
        sys.exit(1)
