#!/usr/bin/env python3
"""Qualify an ordinary guide response's expiry/PMT boundary (ADR 0124).

Request log offsets assign the owner before packet hashes validate its windows.
All raw PMT sections, AV descriptors and sink ACKs remain checked. This grants
no caption timing waiver, client acceptance or full-response EOF claim.
"""

import argparse
import json
from pathlib import Path
import re
from types import ModuleType

SOURCE = Path(__file__).with_name("teletext-expiry-provenance.py")
H = ModuleType("expiry_provenance")
H.__file__ = str(SOURCE)
exec(compile(SOURCE.read_bytes(), str(SOURCE), "exec"), H.__dict__)
require, one = H.require, H.one
SECTION = re.compile(r"Packet ([0-9,]+) to ([0-9,]+), PID 0x([0-9A-F]+), TID 0x02, TIDext 0x([0-9A-F]+), V([0-9]+), Sec 0/0: .+")
WINDOW_EVENTS = {"sink_window_prepared", "sink_window_first_packet_written", "sink_window_flushed"}


def candidate(event, metadata, channel):
    require(event.get("channel_id") == channel and event.get("authored_selected") is True
            and event.get("original_valid_until") is None, "candidate is not this authored Base guide")
    row = one([row for row in metadata if row["channel_id"] == channel and row["seed_id"] == event.get("guide_seed_id")
               and row["runtime_id"] == event.get("guide_runtime_id")
               and row["generation_id"] == event.get("generation_id")], "guide candidate lacks exact saved metadata")
    until = H.utc_ns(event.get("guide_valid_until"))
    require(until == H.utc_ns(row["valid_until"]), "guide expiry differs from its saved metadata")
    return until


def qualify(raw, log, request, tables, sections, metadata):
    require(raw and len(raw) <= 8 * 1024 * 1024 and len(raw) % H.PACKET == 0, "capture exceeds its unchanged byte bound")
    count = len(raw) // H.PACKET
    require(H.integer(request.get("channel_id"), 1), "request lacks its actual channel")
    start, ready, end = (request.get(key) for key in ("log_start", "log_first_chunk", "log_end"))
    require(all(H.integer(value) for value in (start, ready, end)) and start < ready <= end <= len(log), "request log offsets are invalid")
    events = H.parse_log(log)
    offsets = [0]
    for line in log.splitlines(keepends=True):
        offsets.append(offsets[-1] + len(line))
    intent = one([event for event in events if event["event"] == "ordinary_live_intent"
                  and event.get("channel_id") == request["channel_id"]
                  and start <= offsets[event["_line"] - 1] and offsets[event["_line"]] <= ready],
                 "request interval does not identify exactly one channel intent")
    link = one([event for event in events if event["event"] == "monitor_owner_link"
                and event.get("intent_id") == intent.get("intent_id")], "request intent lacks one monitor link")
    require(link.get("channel_id") == request["channel_id"], "monitor link changes channel")
    group = [event for event in events if event.get("proposal_slot") == link["proposal_slot"]]
    epochs = {event["output_epoch"] for event in group if "output_epoch" in event}
    require(len(epochs) == 1, "request has missing or multiple sink epochs")
    # A capped client may leave later flushed windows in the server log. Retain
    # and count them, but claim ACK proof only for the captured prefix.
    suffix = [event for event in group if event["event"] in WINDOW_EVENTS and event.get("first_packet", -1) >= count]
    retained = [event for event in group if event not in suffix]
    owner = {"events": retained, "output_epoch": next(iter(epochs))}
    windows, ignored = H.qualified_windows(owner, raw)
    candidates = [event for event in group if event["event"] in ("monitor_initial", "monitor_recovered")
                  and event.get("authored_selected") is True]
    require(candidates, "request never acquires a guide candidate")
    initial = min(candidates, key=lambda event: event["_line"])
    expiry = candidate(initial, metadata, request["channel_id"])
    require(initial["_utc_ns"] < expiry, "initial guide is already expired")
    proposals = [event for event in group if event["event"] == "monitor_proposal"]
    replacements = [event for event in proposals if event.get("action") == "replace"]
    startup = initial["event"] == "monitor_recovered"
    require(len(replacements) == (2 if startup else 1), "guide lacks its initial or refreshed proposal")
    replacement = replacements[-1]
    withdrawals = [event for event in proposals if event.get("action") == "withdraw"]
    require(len(withdrawals) <= 1 and len(proposals) == len(withdrawals) + len(replacements), "guide has unexpected proposals")
    require(all(H.integer(event.get("sequence"), 1) for event in proposals)
            and len({event["sequence"] for event in proposals}) == len(proposals), "proposal sequences are ambiguous")
    recovered = one([event for event in group if event["event"] == "monitor_recovered"
                     and initial["_line"] < event["_line"] < replacement["_line"]
                     and event.get("guide_runtime_id") != initial.get("guide_runtime_id")], "replacement lacks one fresh runtime observation")
    fresh_expiry = candidate(recovered, metadata, request["channel_id"])
    require(recovered.get("guide_seed_id") == initial.get("guide_seed_id")
            and H.integer(recovered.get("guide_runtime_id"), 1) and fresh_expiry > expiry,
            "replacement does not refresh the original guide lineage")
    for key in ("channel_id", "generation_id", "request_id", "design_id", "revision"):
        require(replacement.get(key) == recovered.get(key) == initial.get(key), "guide handover changes manual identity")
    boundaries = [event for event in group if event["event"] == "scope_boundary_expired"]
    require(len(boundaries) <= 1, "guide has unexpected expiry boundaries")
    if boundaries:
        boundary = boundaries[0]
        require(boundary["_utc_ns"] >= expiry and boundary.get("generation_id") == initial["generation_id"], "expiry changes the original UTC or generation")
        withdrawal = one(withdrawals, "expiry lacks its fail-closed withdrawal proposal")
        require(boundary["_line"] < withdrawal["_line"] < recovered["_line"]
                and withdrawal["sequence"] < replacement["sequence"], "expiry recovery precedes its withdrawal proposal")
    else:
        require(not withdrawals and recovered["_utc_ns"] < expiry, "unexplained guide withdrawal or late renewal")

    installations = [event for event in group if event["event"] == "transition_installed"]
    require(len({event.get("sequence") for event in installations}) == len(installations), "duplicate actual installation")
    def ack(event):
        require(event.get("output_epoch") == owner["output_epoch"], "transition belongs to another sink")
        window = one([window for window in windows if window["fully_retained"] and window["end"] == event.get("end_packet")], "transition ACK is outside the captured prefix")
        flushed = window["flushed"]
        require(event["_line"] > flushed["_line"] and event["_utc_ns"] >= flushed["_utc_ns"]
                and event.get("accepted_elapsed_ns", -1) >= flushed.get("accepted_elapsed_ns", 0)
                and all(event.get(key) == flushed.get(key) for key in ("cycle_boundary", "complete_cycle", "signalling_announced")), "transition does not match its exact flushed ACK")
        return window
    for event in installations:
        proposal = one([proposal for proposal in proposals if proposal["sequence"] == event.get("sequence")], "installation lacks its actual proposal")
        require(event.get("action") == proposal["action"] and event["_line"] > proposal["_line"] and event.get("cycle_boundary") is True, "installation changes action or precedes proposal")
        ack(event)
    for window in windows:
        prior = [event for event in installations if event["end_packet"] <= window["first"]]
        latest = max(prior, key=lambda event: event["end_packet"]) if prior else None
        require(window["sequence"] == (latest["sequence"] if latest else 0), "sink window does not use its actual installed sequence")
    startup_install = None
    if startup:
        first_proposal = replacements[0]
        require(initial["_line"] < first_proposal["_line"] < replacement["_line"]
                and all(first_proposal.get(key) == initial.get(key) for key in ("channel_id", "generation_id", "request_id", "design_id", "revision")), "startup guide changes its observed identity")
        startup_install = one([event for event in installations if event.get("sequence") == first_proposal["sequence"] and event.get("action") == "replace"], "startup guide was not installed")
        startup_applied = one([event for event in group if event["event"] == "transition_applied"
                               and event.get("sequence") == first_proposal["sequence"] and event.get("action") == "replace"], "startup guide lacks actual delivery")
        ack(startup_applied)
        require(startup_applied.get("complete_cycle") is True and startup_install["_line"] < startup_applied["_line"]
                and startup_install["_utc_ns"] < expiry, "startup guide was not installed within its original authority")
    replace = one([event for event in installations if event.get("action") == "replace" and event.get("sequence") == replacement["sequence"]], "fresh guide was not installed")
    applied = one([event for event in group if event["event"] == "transition_applied" and event.get("sequence") == replace["sequence"] and event.get("action") == "replace"], "fresh guide lacks actual complete-cycle delivery")
    ack(applied)
    require(applied.get("complete_cycle") is True and replace["_line"] < applied["_line"] and applied["_utc_ns"] < fresh_expiry, "fresh guide cycle is incomplete or expired")
    if startup:
        require(startup_applied["_line"] < replace["_line"], "fresh replacement precedes complete startup delivery")

    pmts = [table for table in tables if table.get("#name") == "PMT"]
    require(pmts, "raw capture has no PMT")
    versions = []
    reference_av = reference_text = None
    for table in pmts:
        nodes = table["#nodes"]
        meta = one([node for node in nodes if node["#name"] == "metadata"], "PMT lacks exact packet metadata")
        require(table.get("current") is True and H.integer(table.get("version")) and table["version"] < 32
                and 0 <= meta["first_ts_packet"] <= meta["last_ts_packet"] < count, "PMT version or extent is invalid")
        components = [node for node in nodes if node["#name"] == "component"]
        authored = [node for node in components if node["elementary_pid"] == 512]
        require(len(authored) <= 1, "PMT repeats authored PID")
        av = [node for node in components if node["elementary_pid"] != 512]
        require(len(av) == 3 and len({node["elementary_pid"] for node in av}) == 3, "PMT lost AV or selected captions")
        if reference_av is None:
            reference_av = av
        if authored and reference_text is None:
            reference_text = authored
        require(av == reference_av and (not authored or authored == reference_text)
                and table["service_id"] == pmts[0]["service_id"] and table["pcr_pid"] == pmts[0]["pcr_pid"], "PMT changes AV, captions or authored descriptors")
        versions.append({"version": table["version"], "first": meta["first_ts_packet"], "last": meta["last_ts_packet"], "pid": meta["pid"], "present": bool(authored)})
    require(any(version["present"] for version in versions) and versions[-1]["present"], "authored service is missing before or after handover")
    first_present = next(index for index, version in enumerate(versions) if version["present"])
    require(first_present == (1 if startup else 0), "initial absent PMT has no exact startup recovery")
    handover_versions = versions[first_present:]
    if startup:
        require(startup_install["end_packet"] <= handover_versions[0]["first"]
                and handover_versions[0]["last"] < startup_applied["end_packet"], "initial PMT announcement is outside its actual ACK interval")
    raw_versions, previous_end = [], -1
    for line in sections.splitlines():
        match = SECTION.fullmatch(line)
        require(match is not None, "unrecognized raw PMT section observation")
        first, last = (int(match[index].replace(',', '')) for index in (1, 2))
        pid, service, version = int(match[3], 16), int(match[4], 16), int(match[5])
        require(previous_end < first <= last < count and pid == versions[0]["pid"] and service == pmts[0]["service_id"], "raw PMT section changes programme or bounds")
        previous_end = last
        if not raw_versions or version != raw_versions[-1]["version"]:
            raw_versions.append({"version": version, "first": first, "first_last": last, "last": last})
        else:
            raw_versions[-1]["last"] = last
    require(len(raw_versions) == len(versions) and all(raw["version"] == table["version"] and raw["first"] == table["first"]
            and raw["first_last"] == table["last"] for raw, table in zip(raw_versions, versions)), "raw PMT repeats an obsolete or unreported version")
    require(all(b["version"] == (a["version"] + 1) % 32 for a, b in zip(versions, versions[1:])), "PMT version does not advance once per transition")
    next_table, announced = 0, False
    for ordinal in range(count):
        while next_table < len(versions) and versions[next_table]["first"] <= ordinal:
            announced = versions[next_table]["present"]
            next_table += 1
        packet = raw[ordinal * 188:(ordinal + 1) * 188]
        require(announced or ((packet[1] & 31) << 8 | packet[2]) != 512, "authored payload has no current PMT announcement")
    withdrawn = [event for event in installations if event.get("action") == "withdraw"]
    if withdrawn:
        removed = one(withdrawn, "multiple withdrawals installed")
        if startup:
            require(startup_applied["_line"] < removed["_line"], "startup delivery was not closed at the withdrawal ACK")
        require(boundaries and len(handover_versions) == 3 and [version["present"] for version in handover_versions] == [True, False, True], "PMT withdrawal lacks original expiry or recovery")
        require(removed["_utc_ns"] >= expiry and removed["end_packet"] < replace["end_packet"], "withdrawal precedes expiry or follows replacement")
        absent = one([event for event in group if event["event"] == "transition_applied" and event.get("action") == "withdraw" and event.get("sequence") == removed["sequence"]], "withdrawal lacks absent-signalling ACK")
        absent_window = ack(absent)
        require(absent_window["sequence"] == removed["sequence"] and absent_window["flushed"].get("authored_selected") is False
                and absent.get("signalling_announced") is True and absent["_line"] < replace["_line"], "replacement precedes withdrawal signalling ACK")
        require(removed["end_packet"] <= handover_versions[1]["first"] and handover_versions[1]["last"] < absent["end_packet"]
                and replace["end_packet"] <= handover_versions[2]["first"] and handover_versions[2]["last"] < applied["end_packet"], "raw PMT signalling is outside its actual ACK interval")
        for ordinal in range(removed["end_packet"], handover_versions[2]["first"]):
            packet = raw[ordinal * 188:(ordinal + 1) * 188]
            require(((packet[1] & 31) << 8 | packet[2]) != 512, "authored payload survives the withdrawn interval")
        cut = handover_versions[2]["first"]
        path = "expiry_withdrawal_ack_then_reannouncement"
    else:
        require(len(handover_versions) == 1, "PMT changes without an installed withdrawal")
        cut, path = replace["end_packet"], "confirmed_replacement_without_withdrawal"
    return {"capture_sha256": H.digest(raw), "log_sha256": H.digest(log), "proposal_slot": link["proposal_slot"],
            "intent_id": intent["intent_id"], "output_epoch": owner["output_epoch"], "path": path,
            "original_valid_until": initial["guide_valid_until"], "fresh_valid_until": recovered["guide_valid_until"],
            "recovered_packet": cut, "packet_count": count, "qualified_windows": len(windows),
            "unretained_suffix_window_events": len(suffix), "ignored_unflushed_windows": ignored,
            "startup_recovery": startup, "request_provenance": request,
            "full_response_eof_claimed": False, "caption_timing_waived": False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("capture", "log", "request", "pmt", "sections", "metadata", "report"):
        parser.add_argument("--" + name, type=Path, required=True)
    args = parser.parse_args()
    load = lambda path: json.loads(H.bounded(path, 65_536), object_pairs_hook=H.unique_object)
    try:
        result = qualify(H.bounded(args.capture, 8 * 1024 * 1024), H.bounded(args.log, H.MAX_LOG, reject_cap=True),
                         load(args.request), load(args.pmt), H.bounded(args.sections, 262_144).decode(), load(args.metadata))
        args.report.write_text(json.dumps(result, indent=2) + "\n")
    except (H.ProofError, ValueError, KeyError, TypeError, OSError) as error:
        parser.exit(1, f"Guide boundary qualification failed: {error}\n")


if __name__ == "__main__":
    main()
