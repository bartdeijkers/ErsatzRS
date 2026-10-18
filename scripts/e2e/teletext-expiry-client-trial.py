#!/usr/bin/env python3
"""Observe genuine scoped expiry on two original headed VLC responses.

Automatic post-expiry frames are captured before any control interaction. Later
explicit teletext selection/navigation is recorded separately. Screenshot markers
never certify pixels, release the native completion, or prove full Level 2.5.
This opt-in Linux/X11 trial reuses the receipted ordinary-stream client helpers.
"""

import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import re
import runpy
import signal
import subprocess
import time

HELPER = Path(__file__).with_name("teletext-live-update-client-trial.py")
W = runpy.run_path(str(HELPER))
H = W["H"]
PROVENANCE = Path(__file__).with_name("teletext-expiry-provenance.py")
utc_ns = runpy.run_path(str(PROVENANCE))["utc_ns"]
# Imported capture/ownership functions retain their receipted implementations;
# expiry readiness independently restricts the shared loader to this protocol.
PINNED_WARM = "807ca2360265964eb56c569e96b31a322c400d87361c19d116903243fec57b37"
STATES = ["base", "scoped_q", "expired_base"]
CHANNEL_KEYS = {"channel_id", "theme", "url", "addon_id", "profile_id", "profile_revision",
                "base_generation_id", "base_request_id", "base_design_id", "base_design_revision",
                "base_digest", "theme_rgb4"}
FACT_KEYS = {"channel_id", "generation_id", "request_id", "design_id", "design_revision",
             "design_digest", "document_version", "teletext_visible", "edited_q_present", "theme_rgb4"}
SCOPE_KEYS = {"design_id", "revision", "digest", "generation_id", "request_id", "original_design_id",
              "original_revision", "original_digest", "root_receipt_id", "root_receipt_digest", "start", "finish"}


def ready(value, directory):
    W["closed"](value, {"probe_version", "protocol", "completion_timeout_seconds", "source", "channels",
                       "expected_pages", "subcode", "stages", "stage_files", "final_completion", "limitations"},
                "natural-expiry readiness")
    if (value["probe_version"] != 1 or value["protocol"] != "natural-expiry-live-ordinary-v1"
            or type(value["completion_timeout_seconds"]) is not int or value["completion_timeout_seconds"] != 180
            or value["stages"] != STATES or not H["exact_integer_list"](value["expected_pages"], [200, 201])
            or type(value["subcode"]) is not int or value["subcode"] != 0
            or value["stage_files"] != "stage-N-ready.json and stage-N-captured.json, N from 1 through 3"
            or not W["text_value"](value["limitations"])):
        raise RuntimeError("Unsupported natural-expiry observation protocol")
    if value["final_completion"] != {"probe_version": 1, "client_name": "VLC", "client_version": "3.0.20",
                                     "decoder_version": "0.2.42", "proof_suffix": "natural-expiry-live-ordinary"}:
        raise RuntimeError("Unexpected root-only completion identity")
    W["source_provenance"](value["source"], directory)
    if not isinstance(value["channels"], list) or len(value["channels"]) != 2:
        raise RuntimeError("Expected two isolated native channel descriptions")
    channels, ports = {}, set()
    for item in value["channels"]:
        W["closed"](item, CHANNEL_KEYS, "accepted Base channel")
        theme = item["theme"]
        if theme not in ["day", "night"] or theme in channels:
            raise RuntimeError("Ambiguous native viewer")
        for key in CHANNEL_KEYS - {"theme", "url", "addon_id", "base_digest", "theme_rgb4"}:
            if not H["positive_identity"](item[key]):
                raise RuntimeError("Invalid accepted Base identity")
        if (item["channel_id"] != (1 if theme == "day" else 2)
                or item["base_design_revision"] != 1 or not W["hash_value"](item["base_digest"])
                or not isinstance(item["addon_id"], str)
                or not re.fullmatch(r"[a-z0-9]+(?:-[a-z0-9]+)*(?:\.[a-z0-9]+(?:-[a-z0-9]+)*){2,}", item["addon_id"])
                or not H["exact_integer_list"](item["theme_rgb4"], [15, 0, 0] if theme == "day" else [0, 0, 15])):
            raise RuntimeError("Invalid channel/addon/theme association")
        ports.add(W["queried_url"](item["url"], item["channel_id"], theme))
        channels[theme] = item
    if len(ports) != 1 or channels["day"]["addon_id"] != channels["night"]["addon_id"]:
        raise RuntimeError("Expected the same addon and owned origin")
    for key in ["channel_id", "profile_id", "base_generation_id", "base_request_id", "base_design_id"]:
        if channels["day"][key] == channels["night"][key]:
            raise RuntimeError("Accepted Base identities are not channel-isolated")
    return channels


def stage(value, number, channels, previous):
    W["closed"](value, {"probe_version", "stage", "state", "scope", "snapshot_utc", "day", "night", "scoped",
                       "expected_pages", "subcode", "wire_offsets"}, "natural-expiry stage")
    if (value["probe_version"] != 1 or type(value["stage"]) is not int or value["stage"] != number
            or value["state"] != STATES[number - 1] or not W["text_value"](value["scope"])
            or not H["exact_integer_list"](value["expected_pages"], [200, 201])
            or type(value["subcode"]) is not int or value["subcode"] != 0
            or not isinstance(value["wire_offsets"], list) or len(value["wire_offsets"]) != 3
            or any(type(n) is not int or n < 0 or n % 188 for n in value["wire_offsets"])):
        raise RuntimeError("Invalid natural-expiry stage/page/packet identity")
    observed = utc_ns(value["snapshot_utc"])
    scope = value["scoped"]
    if number == 1:
        if scope is not None or previous is not None:
            raise RuntimeError("Initial stage must precede the new scoped original")
    else:
        if previous is None or previous["stage"] != number - 1:
            raise RuntimeError("Skipped natural-expiry stage")
        W["closed"](scope, SCOPE_KEYS, "immutable scoped original")
        for key in SCOPE_KEYS - {"digest", "original_digest", "root_receipt_digest", "start", "finish"}:
            if not H["positive_identity"](scope[key]):
                raise RuntimeError("Invalid new Scoped identity")
        if (scope["revision"] != 2 or scope["original_revision"] != 1
                or scope["design_id"] != scope["original_design_id"]
                or any(not W["hash_value"](scope[key]) for key in ["digest", "original_digest", "root_receipt_digest"])
                or utc_ns(scope["finish"]) - utc_ns(scope["start"]) != 35_000_000_000):
            raise RuntimeError("Expected a genuine new thirty-five-second scope")
        if number == 2 and not utc_ns(scope["start"]) <= observed < utc_ns(scope["finish"]):
            raise RuntimeError("Q readiness lies outside its original scope")
        if number == 3 and (scope != previous["scoped"] or observed < utc_ns(scope["finish"])):
            raise RuntimeError("Expiry changed immutable scope or was observed early")
    for theme in ["day", "night"]:
        item, initial = value[theme], channels[theme]
        W["closed"](item, FACT_KEYS, "actual stage generation")
        for key in ["channel_id", "generation_id", "request_id", "design_id", "design_revision", "document_version"]:
            if not H["positive_identity"](item[key]):
                raise RuntimeError("Invalid actual stage identity")
        if (item["channel_id"] != initial["channel_id"] or not W["hash_value"](item["design_digest"])
                or type(item["teletext_visible"]) is not bool or not item["teletext_visible"]
                or type(item["edited_q_present"]) is not bool
                or not H["exact_integer_list"](item["theme_rgb4"], initial["theme_rgb4"])):
            raise RuntimeError("Actual stage presentation lost isolation")
        scoped = number == 2 and theme == "day"
        expected = ({"generation_id": scope["generation_id"], "request_id": scope["request_id"],
                     "design_id": scope["design_id"], "design_revision": scope["revision"], "design_digest": scope["digest"]}
                    if scoped else {key: initial["base_" + key] for key in
                                    ["generation_id", "request_id", "design_id", "design_revision"]}
                    | {"design_digest": initial["base_digest"]})
        if (any(item[key] != expected[key] for key in expected) or item["document_version"] != (2 if scoped else 1)
                or item["edited_q_present"] != scoped):
            raise RuntimeError("Scoped publication or exact accepted Base restoration mismatch")
    return value


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("native_ready_file", type=Path)
    args = parser.parse_args()
    if not os.environ.get("DISPLAY"):
        raise RuntimeError("A real headed X11 display is required")
    if H["digest"](H["bounded_bytes"](HELPER, H["MAX_LOG"])) != PINNED_WARM:
        raise RuntimeError("Receipted warm helper source changed")
    sources = {}
    for path, expected in H["RECEIPTED"].items():
        actual = H["digest"](H["bounded_bytes"](path, H["MAX_LOG"]))
        if actual != expected:
            raise RuntimeError("Receipted client/decoder/observation identity changed")
        sources[str(path.relative_to(H["REPO"])) if path.is_relative_to(H["REPO"]) else str(path)] = actual
    for path in [HELPER, W["HELPERS"], PROVENANCE, Path(__file__)]:
        sources[str(path.relative_to(H["REPO"]))] = H["digest"](H["bounded_bytes"](path, H["MAX_LOG"]))
    path = args.native_ready_file.absolute()
    raw, value = W["load"](path)
    directory = path.parent
    channels = ready(value, directory)
    deadline = time.monotonic() + value["completion_timeout_seconds"]
    clients, previous = [], None
    with (directory / "headed-expiry-trial.ndjson").open("x", encoding="utf-8") as record:
        def emit(item):
            item["observed_utc"] = datetime.now(timezone.utc).isoformat()
            record.write(json.dumps(item, sort_keys=True) + "\n")
            record.flush()
            print(json.dumps(item, sort_keys=True), flush=True)

        emit({"event": "identity", "sources": sources, "ready_sha256": H["digest"](raw),
              "client": "VLC 3.0.20-3build6", "decoder": "libzvbi 0.2.42-2",
              "rendered_navigation_proven": False, "full_level25_proven": False})
        try:
            for index, theme in enumerate(["day", "night"]):
                client = W["start_client"](theme, channels[theme]["url"], directory, index, deadline, 200)
                clients.append(client)
                emit({"event": "started", "viewer": theme, "pid": client["process"].pid,
                      "window": client["window"], "url": client["url"]})
            for number in [1, 2, 3]:
                raw_stage, actual = W["wait_stage"](directory, number, deadline)
                stage(actual, number, channels, previous)
                emit({"event": "native_stage", "stage": number, "ready": actual})
                images = []
                if number == 3:
                    # No focus, activation, page selection or decoder inspection
                    # that could change rendering precedes these automatic frames.
                    H["settle"](deadline, 3)
                    for client in clients:
                        image = W["capture"](client, directory, number, None, 0, deadline)
                        images.append(image)
                        emit({"event": "automatic_post_expiry", "stage": number, **image,
                              "decoder_active": W["decoder_active"](client), "controls_used": False})
                for client in clients if number == 1 else clients[:1]:
                    was_active = W["decoder_active"](client)
                    W["activate"](client, deadline)
                    emit({"event": "teletext_selection", "stage": number, "viewer": client["viewer"],
                          "decoder_was_active": was_active, "explicit_selection_required": not was_active})
                for visit, page in enumerate([200, 201, 200] if number == 3 else [200]):
                    if number == 3 and visit:
                        for client in clients:
                            position = W["origin"](client, deadline)
                            y = 389 if page == 201 else 400
                            H["gui"](deadline, "click-window", client["window"], 121, y,
                                     position["x"] + 121, position["y"] + y)
                    H["settle"](deadline, 3)
                    if number == 2 and time.time_ns() >= utc_ns(actual["scoped"]["finish"]):
                        raise RuntimeError("Q frame observation missed its genuine original scope")
                    for client in clients:
                        image = W["capture"](client, directory, number, page, visit + (1 if number == 3 else 0), deadline)
                        images.append(image)
                        emit({"event": "captured", "stage": number, **image, "rendered_page_verified": False})
                W["closed_new"](directory / f"stage-{number}-captured.json",
                                {"probe_version": 1, "stage": number, "ready_sha256": H["digest"](raw_stage),
                                 "clients": [{"viewer": c["viewer"], "pid": c["process"].pid} for c in clients],
                                 "screenshots": images})
                previous = actual
            emit({"event": "captures_complete", "native_completion_released": False,
                  "next": "Inspect automatic frames and requested navigation before root-only completion."})
        finally:
            cleanup_failed = False
            for client in clients:
                process = client["process"]
                try:
                    if process.poll() is None:
                        process.terminate()
                    try:
                        status = process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        status = process.wait(timeout=5)
                    controlled = status in [0, -signal.SIGTERM]
                    cleanup_failed |= not controlled
                    emit({"event": "finished", "viewer": client["viewer"], "client_exit": status,
                          "controlled_exit": controlled})
                except (OSError, subprocess.TimeoutExpired):
                    cleanup_failed = True
                    emit({"event": "cleanup_failed", "viewer": client["viewer"], "controlled_exit": False})
                finally:
                    client["log"].close()
            if cleanup_failed:
                raise RuntimeError("Owned client teardown did not complete as expected")


if __name__ == "__main__":
    main()
