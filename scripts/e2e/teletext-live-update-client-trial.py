#!/usr/bin/env python3
"""Capture headed warm transitions from the disposable native TCP fixture.

The same owned VLC processes and URLs survive the declared stages. Capture barriers
only synchronize the native actor; they do not certify rendered content. Inspect
the actual images independently before writing the root-only completion marker.
Requires the previously receipted Linux x64/X11 client and observation tools.
"""

import argparse
import json
import os
import re
from datetime import datetime, timezone
from pathlib import Path
import runpy
import signal
import subprocess
import sys
import time
from urllib.parse import urlsplit

HELPERS = Path(__file__).with_name("teletext-live-client-trial.py")
H = runpy.run_path(str(HELPERS))
bounded_bytes, digest = H["bounded_bytes"], H["digest"]
remaining, settle, gui = H["remaining"], H["settle"], H["gui"]
windows, owned_window = H["windows"], H["assert_owned_window"]
command, REPO, WHERE = H["command"], H["REPO"], H["WHERE"]


def unique_pairs(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise RuntimeError("Duplicate readiness or stage field")
        result[key] = value
    return result


def load(path):
    raw = bounded_bytes(path, H["MAX_READY"])
    value = json.loads(raw, object_pairs_hook=unique_pairs)
    if not isinstance(value, dict) or type(value.get("probe_version")) is not int or value["probe_version"] not in [1, 2, 3, 4]:
        raise RuntimeError("Unsupported closed native proof object")
    return raw, value


def closed_new(path, value):
    temporary = path.with_name("." + path.name + ".pending")
    with temporary.open("xb") as output:
        output.write((json.dumps(value, sort_keys=True) + "\n").encode())
        output.flush()
        os.fsync(output.fileno())
    # Publish only a complete closed inode, without replacing another proof file.
    os.link(temporary, path)
    temporary.unlink()


def wait_stage(directory, stage, deadline):
    path = directory / f"stage-{stage}-ready.json"
    while not path.exists():
        settle(deadline, 0.1)
    raw, value = load(path)
    if type(value.get("stage")) is not int or value["stage"] != stage:
        raise RuntimeError("Native stage identity mismatch")
    return raw, value


def urls(ready):
    channels = ready.get("channels")
    if not isinstance(channels, list) or len(channels) != 2:
        raise RuntimeError("Expected two native channel/client descriptions")
    result, ports = {}, set()
    for channel, viewer in [(1, "day"), (2, "night")]:
        candidates = [v for v in channels if isinstance(v, dict)
                      and v.get("channel_id") == channel and v.get("theme") == viewer]
        if len(candidates) != 1:
            raise RuntimeError("Ambiguous native channel/theme")
        url = candidates[0].get("url", "")
        parsed = urlsplit(url)
        if (parsed.scheme != "http" or parsed.hostname != "127.0.0.1"
                or parsed.username is not None or parsed.password is not None
                or not parsed.port or parsed.path != f"/iptv/hls-direct/{channel}.ts"
                or parsed.query != f"teletext_test_viewer={viewer}" or parsed.fragment):
            raise RuntimeError("Expected the exact owned ordinary loopback URL")
        ports.add(parsed.port)
        result[viewer] = url
    if len(ports) != 1:
        raise RuntimeError("Clients must share the owned native origin")
    return result



# The legacy-wrapper extension retains the imported helper functions exactly.
COLD_HELPER_SHA256 = "a743ebb87c212cd2d9e78f1a97fcb7df6f92a6a20d354d4af234919e0c284f0b"
STATES = ["q", "cumulative_qr", "lua_off"]
CHANNEL_KEYS = {"channel_id", "theme", "url", "addon_id", "profile_id", "profile_revision", "theme_rgb4",
    "generation_id", "request_id", "design_id", "design_revision", "scoped_role", "original_design_id",
    "original_revision", "original_digest", "root_receipt_id", "root_receipt_digest", "original_valid_until",
    "expected_pages", "subcode", "expected_navigation", "edited_q_page", "edited_r_page", "edited_q_cell", "edited_r_cell"}
STAGE_CHANNEL_KEYS = {"channel_id", "scoped_role", "design_id", "design_revision", "design_digest", "generation_id",
    "request_id", "valid_until", "original_design_id", "original_revision", "original_digest", "root_receipt_id",
    "root_receipt_digest", "original_valid_until", "teletext_visible", "edited_q_present", "edited_r_present", "theme_rgb4"}


def closed(value, keys, label):
    if not isinstance(value, dict) or set(value) != keys:
        raise RuntimeError(f"Expected closed {label}")


def hash_value(value):
    return isinstance(value, str) and re.fullmatch(r"[0-9a-f]{64}", value) is not None


def text_value(value):
    return isinstance(value, str) and 0 < len(value) <= 2048


def queried_url(value, channel, theme):
    if not isinstance(value, str):
        raise RuntimeError("Expected literal owned ordinary URL")
    parsed = urlsplit(value)
    if parsed.query != f"teletext_test_viewer={theme}":
        raise RuntimeError("Expected exact owned viewer query")
    # Reuse the cold helper's canonical host/path/port/credential checks.
    return H["ordinary_url"](parsed._replace(query="").geturl(), channel)


def source_provenance(value, directory):
    closed(value, {"file", "bytes", "sha256", "duration_seconds"}, "native source provenance")
    if (value["file"] != "source.ts" or type(value["bytes"]) is not int
            or not 0 < value["bytes"] <= H["MAX_LIVE_SOURCE"] or not hash_value(value["sha256"])
            or type(value["duration_seconds"]) is not int or value["duration_seconds"] != 60):
        raise RuntimeError("Expected bounded sixty-second native source")
    body = bounded_bytes(directory / value["file"], H["MAX_LIVE_SOURCE"])
    if (len(body) != value["bytes"] or digest(body) != value["sha256"] or len(body) % 188
            or any(body[n] != 0x47 or body[n + 1] & 0x80 for n in range(0, len(body), 188))
            or any(((body[n + 1] & 31) << 8 | body[n + 2]) == 512 for n in range(0, len(body), 188))):
        raise RuntimeError("Source hash/framing/plain AV identity mismatch")


def scoped_ready(ready, directory):
    closed(ready, {"probe_version", "protocol", "completion_timeout_seconds", "source", "channels",
        "expected_pages", "subcode", "stages", "stage_files", "final_completion", "limitations"}, "warm scoped readiness")
    if (type(ready["probe_version"]) is not int or ready["probe_version"] != 2
            or ready["protocol"] != "warm-scoped-live-ordinary-v2" or ready["stages"] != STATES
            or type(ready["completion_timeout_seconds"]) is not int or ready["completion_timeout_seconds"] != 180
            or not H["exact_integer_list"](ready["expected_pages"], [200, 201])
            or type(ready["subcode"]) is not int or ready["subcode"] != 0
            or ready["stage_files"] != "stage-N-ready.json and stage-N-captured.json, N from 1 through 3"
            or not text_value(ready["limitations"])):
        raise RuntimeError("Unsupported warm scoped stage/page/deadline protocol")
    final = ready["final_completion"]
    if (not isinstance(final, dict) or type(final.get("probe_version")) is not int
            or final != {"probe_version": 2, "client_name": "VLC", "client_version": "3.0.20",
                "decoder_version": "0.2.42", "proof_suffix": "warm-scoped-live-ordinary"}):
        raise RuntimeError("Unsupported root-only client completion identity")
    source_provenance(ready["source"], directory)
    if not isinstance(ready["channels"], list) or len(ready["channels"]) != 2:
        raise RuntimeError("Expected two actual scoped channels")
    channels, ports = {}, set()
    for item in ready["channels"]:
        closed(item, CHANNEL_KEYS, "initial scoped channel")
        theme = item["theme"]
        if theme not in ["day", "night"] or theme in channels:
            raise RuntimeError("Duplicate/unknown scoped theme")
        for key in ["channel_id", "profile_id", "profile_revision", "generation_id", "request_id", "design_id",
                    "design_revision", "original_design_id", "original_revision", "root_receipt_id"]:
            if not H["positive_identity"](item[key]):
                raise RuntimeError("Invalid initial scoped identity")
        if (type(item["scoped_role"]) is not int or item["scoped_role"] != 2
                or item["original_design_id"] != item["design_id"] or item["original_revision"] != 1
                or item["design_revision"] != (2 if theme == "day" else 1)
                or not isinstance(item["addon_id"], str) or len(item["addon_id"]) > 255
                or not re.fullmatch(r"[a-z0-9]+(?:-[a-z0-9]+)*(?:\.[a-z0-9]+(?:-[a-z0-9]+)*){2,}", item["addon_id"])
                or not hash_value(item["original_digest"]) or not hash_value(item["root_receipt_digest"])):
            raise RuntimeError("Invalid scoped role/original/receipt/addon identity")
        if (not H["exact_integer_list"](item["theme_rgb4"], [15, 0, 0] if theme == "day" else [0, 0, 15])
                or not H["exact_integer_list"](item["expected_pages"], [200, 201])
                or type(item["subcode"]) is not int or item["subcode"] != 0
                or not H["exact_integer_list"](item["expected_navigation"], [200, 201, 200])):
            raise RuntimeError("Unsupported initial scoped presentation")
        if ((theme == "day" and (type(item["edited_q_page"]) is not int or item["edited_q_page"] != 200))
                or (theme == "night" and item["edited_q_page"] is not None)
                or item["edited_r_page"] is not None
                or not H["exact_cell"](item["edited_q_cell"], 2) or not H["exact_cell"](item["edited_r_cell"], 3)):
            raise RuntimeError("Initial stage requires Q-only Day and untouched Night")
        H["utc_expiry"](item["original_valid_until"])
        ports.add(queried_url(item["url"], item["channel_id"], theme))
        channels[theme] = item
    if len(ports) != 1 or channels["day"]["addon_id"] != channels["night"]["addon_id"]:
        raise RuntimeError("Expected same addon and owned native host")
    for key in ["channel_id", "profile_id", "generation_id", "request_id", "design_id", "root_receipt_id"]:
        if channels["day"][key] == channels["night"][key]:
            raise RuntimeError("Channel/profile/publication isolation mismatch")
    return channels


def validate_stage(value, number, channels, previous):
    closed(value, {"probe_version", "stage", "state", "scope", "day", "night", "expected_pages", "subcode", "wire_offsets"},
           "warm scoped stage")
    if (type(value["probe_version"]) is not int or value["probe_version"] != 2
            or type(value["stage"]) is not int or value["stage"] != number or number not in [1, 2, 3]
            or value["state"] != STATES[number - 1] or not text_value(value["scope"])
            or not H["exact_integer_list"](value["expected_pages"], [200, 201])
            or type(value["subcode"]) is not int or value["subcode"] != 0
            or not isinstance(value["wire_offsets"], list) or len(value["wire_offsets"]) != 3
            or any(type(n) is not int or n < 0 for n in value["wire_offsets"])):
        raise RuntimeError("Scoped stage epoch/page/framing mismatch")
    if number > 1 and (not isinstance(previous, dict) or previous.get("stage") != number - 1):
        raise RuntimeError("Scoped stage skipped its previous captured epoch")
    for theme in ["day", "night"]:
        item, initial = value[theme], channels[theme]
        closed(item, STAGE_CHANNEL_KEYS, "scoped stage channel")
        for key in ["channel_id", "design_id", "design_revision", "generation_id", "request_id", "original_design_id", "original_revision"]:
            if not H["positive_identity"](item[key]):
                raise RuntimeError("Invalid scoped stage identity")
        if (item["channel_id"] != initial["channel_id"] or type(item["scoped_role"]) is not int or item["scoped_role"] != 2
                or not hash_value(item["design_digest"]) or not hash_value(item["original_digest"])
                or any(type(item[key]) is not bool for key in ["teletext_visible", "edited_q_present", "edited_r_present"])
                or not H["exact_integer_list"](item["theme_rgb4"], initial["theme_rgb4"])
                or item["valid_until"] != item["original_valid_until"]):
            raise RuntimeError("Stage role/scope/presentation mismatch")
        H["utc_expiry"](item["valid_until"])
        absent = number == 3 and theme == "day"
        if absent:
            if (item["teletext_visible"] or item["edited_q_present"] or item["edited_r_present"]
                    or item["design_revision"] != 1 or item["original_revision"] != 1
                    or item["original_design_id"] != item["design_id"] or item["original_digest"] != item["design_digest"]
                    or item["root_receipt_id"] is not None or item["root_receipt_digest"] is not None
                    or any(item[key] == initial[key] or (previous and item[key] == previous[theme][key])
                           for key in ["design_id", "generation_id", "request_id"])):
                raise RuntimeError("Off requires its genuine new absent original/publication without fabricated receipt")
        else:
            for key in ["original_design_id", "original_revision", "original_digest", "root_receipt_id",
                        "root_receipt_digest", "original_valid_until", "design_id"]:
                if item[key] != initial[key]:
                    raise RuntimeError("Stage changed immutable original/root/expiry")
            if (not item["teletext_visible"] or item["edited_q_present"] != (theme == "day")
                    or item["edited_r_present"] != (theme == "day" and number == 2)
                    or item["design_revision"] != (number + 1 if theme == "day" else 1)):
                raise RuntimeError("Stage content/revision mismatch")
            if number == 1:
                if any(item[key] != initial[key] for key in ["generation_id", "request_id", "design_revision"]):
                    raise RuntimeError("Initial accepted identity changed before attachment")
            elif theme == "night":
                if item != previous[theme]:
                    raise RuntimeError("Unaffected Night identity changed")
            elif any(item[key] == previous[theme][key] for key in ["generation_id", "request_id", "design_digest"]):
                raise RuntimeError("Cumulative replacement did not publish a new exact identity")
    for key in ["design_id", "generation_id", "request_id"]:
        if value["day"][key] == value["night"][key]:
            raise RuntimeError("Stage publication identities lost channel isolation")
    return value


def scoped_deadline(ready, channels, started):
    now = time.monotonic()
    wall_now = datetime.now(timezone.utc)
    return min(started + ready["completion_timeout_seconds"],
               *(now + (H["utc_expiry"](item["original_valid_until"]) - wall_now).total_seconds()
                 for item in channels.values()))


DRCS_STATES = ["original_bitmap", "edited_bitmap"]
DRCS_CHANNEL_KEYS = {"channel_id", "theme", "url", "addon_id", "profile_id", "profile_revision",
    "theme_rgb4", "design_id", "design_revision", "design_digest", "generation_id", "request_id"}


def drcs_ready(ready, directory):
    closed(ready, {"probe_version", "protocol", "completion_timeout_seconds", "source", "channels",
        "editor_original", "resource", "expected_pages", "subcode", "expected_navigation", "stages",
        "stage_files", "client_stop_file", "limitations"}, "warm DRCS readiness")
    if (type(ready["probe_version"]) is not int or ready["probe_version"] != 3
            or ready["protocol"] != "warm-drcs-live-ordinary-v1"
            or type(ready["completion_timeout_seconds"]) is not int or ready["completion_timeout_seconds"] != 55
            or ready["stages"] != DRCS_STATES
            or ready["stage_files"] != "stage-N-ready.json and stage-N-captured.json, N from 1 through 2"
            or ready["client_stop_file"] != "clients-stopped.json"
            or not H["exact_integer_list"](ready["expected_pages"], [124, 125])
            or type(ready["subcode"]) is not int or ready["subcode"] != 32
            or not H["exact_integer_list"](ready["expected_navigation"], [124, 125, 124])
            or not text_value(ready["limitations"])):
        raise RuntimeError("Unsupported warm DRCS protocol/page/deadline")
    source_provenance(ready["source"], directory)
    closed(ready["resource"], {"kind", "magazine", "page", "logical_subpage", "slot",
        "original_first_bitmap_row", "edited_first_bitmap_row"}, "fixed DRCS resource")
    resource = ready["resource"]
    if (resource != {"kind": "drcs", "magazine": 1, "page": 192, "logical_subpage": 0,
                    "slot": 0, "original_first_bitmap_row": 2049, "edited_first_bitmap_row": 2051}
            or any(type(value) is not int for key, value in resource.items() if key != "kind")):
        raise RuntimeError("Unsupported fixed DRCS resource or bitmap intent")
    if not isinstance(ready["channels"], list) or len(ready["channels"]) != 2:
        raise RuntimeError("Expected two isolated DRCS channels")
    channels, ports = {}, set()
    for index, item in enumerate(ready["channels"]):
        closed(item, DRCS_CHANNEL_KEYS, "DRCS channel")
        theme = ["day", "night"][index]
        if item["theme"] != theme or item["channel_id"] != index + 1:
            raise RuntimeError("DRCS channel/theme order mismatch")
        for key in ["channel_id", "profile_id", "profile_revision", "design_id", "design_revision", "generation_id", "request_id"]:
            if not H["positive_identity"](item[key]):
                raise RuntimeError("Invalid DRCS channel identity")
        if (item["design_revision"] != 1 or not hash_value(item["design_digest"])
                or not isinstance(item["addon_id"], str)
                or not re.fullmatch(r"[a-z0-9]+(?:-[a-z0-9]+)*(?:\.[a-z0-9]+(?:-[a-z0-9]+)*){2,}", item["addon_id"])
                or not H["exact_integer_list"](item["theme_rgb4"], [15, 0, 0] if theme == "day" else [0, 0, 15])):
            raise RuntimeError("Unsupported DRCS original/palette/addon")
        ports.add(queried_url(item["url"], item["channel_id"], theme))
        channels[theme] = item
    if len(ports) != 1 or channels["day"]["addon_id"] != channels["night"]["addon_id"]:
        raise RuntimeError("DRCS clients must share the owned origin and addon")
    for key in ["channel_id", "profile_id", "design_id", "generation_id", "request_id"]:
        if channels["day"][key] == channels["night"][key]:
            raise RuntimeError("DRCS channels lost profile/publication isolation")
    original = ready["editor_original"]
    closed(original, {"design_id", "revision", "digest", "receipt_id", "receipt_digest"}, "DRCS authenticated original")
    if (not H["positive_identity"](original["design_id"]) or type(original["revision"]) is not int
            or original["revision"] != 1 or original["design_id"] != channels["day"]["design_id"]
            or original["digest"] != channels["day"]["design_digest"]
            or not H["positive_identity"](original["receipt_id"]) or not hash_value(original["receipt_digest"])):
        raise RuntimeError("DRCS editor original/receipt association mismatch")
    return channels


def validate_drcs_stage(value, number, channels, previous, ready_sha256):
    closed(value, {"probe_version", "stage", "state", "scope", "initial_ready_sha256", "day", "night",
        "day_first_bitmap_row", "night_first_bitmap_row", "day_s2", "night_s2", "expected_pages",
        "subcode", "expected_navigation", "wire_offsets"}, "warm DRCS stage")
    if (type(value["probe_version"]) is not int or value["probe_version"] != 3
            or type(value["stage"]) is not int or value["stage"] != number or number not in [1, 2]
            or value["state"] != DRCS_STATES[number - 1] or not text_value(value["scope"])
            or value["initial_ready_sha256"] != ready_sha256 or not hash_value(ready_sha256)
            or not H["exact_integer_list"](value["expected_pages"], [124, 125])
            or type(value["subcode"]) is not int or value["subcode"] != 32
            or not H["exact_integer_list"](value["expected_navigation"], [124, 125, 124])
            or not isinstance(value["wire_offsets"], list) or len(value["wire_offsets"]) != 3
            or any(type(n) is not int or n < 0 for n in value["wire_offsets"])):
        raise RuntimeError("DRCS stage/readiness/navigation mismatch")
    for key, expected in [("day_first_bitmap_row", 2049 if number == 1 else 2051),
                          ("night_first_bitmap_row", 2049), ("day_s2", number), ("night_s2", 1)]:
        if type(value[key]) is not int or value[key] != expected:
            raise RuntimeError("DRCS stage bitmap/counter mismatch")
    if number == 2:
        if (not isinstance(previous, dict) or previous.get("stage") != 1
                or any(a <= b for a, b in zip(value["wire_offsets"], previous["wire_offsets"]))):
            raise RuntimeError("DRCS update skipped its original observed stage")
    for theme in ["day", "night"]:
        item, initial = value[theme], channels[theme]
        closed(item, DRCS_CHANNEL_KEYS, "DRCS stage channel")
        if (any(not H["positive_identity"](item[key]) for key in ["channel_id", "profile_id",
                "profile_revision", "design_id", "design_revision", "generation_id", "request_id"])
                or not H["exact_integer_list"](item["theme_rgb4"], initial["theme_rgb4"])):
            raise RuntimeError("Invalid typed DRCS stage identity/palette")
        if number == 1 or theme == "night":
            if item != initial:
                raise RuntimeError("DRCS unchanged channel/original drifted")
        else:
            changed = {"design_revision", "design_digest", "generation_id", "request_id"}
            if (any(item[key] != initial[key] for key in DRCS_CHANNEL_KEYS - changed)
                    or type(item["design_revision"]) is not int or item["design_revision"] != 2
                    or not hash_value(item["design_digest"]) or item["design_digest"] == initial["design_digest"]
                    or any(not H["positive_identity"](item[key]) or item[key] == initial[key]
                           for key in ["generation_id", "request_id"])):
                raise RuntimeError("DRCS edit changed immutable scope or lacks new publication")
    for key in ["generation_id", "request_id"]:
        if value["day"][key] == value["night"][key]:
            raise RuntimeError("DRCS stage crossed channel publications")
    return value



PAGE_PRESENTATION_STATES = ["original_m29", "published_page_x28_bank"]


def page_presentation_ready(ready, directory):
    keys = {"probe_version", "protocol", "completion_timeout_seconds", "source", "channels",
        "editor_original", "resource", "expected_pages", "subcode", "expected_navigation", "stages",
        "stage_files", "client_stop_file", "limitations", "bank_logical_subpages"}
    closed(ready, keys, "warm page-presentation bank readiness")
    if (type(ready["probe_version"]) is not int or ready["probe_version"] != 4
            or ready["protocol"] != "warm-page-presentation-bank-v1"
            or ready["stages"] != PAGE_PRESENTATION_STATES
            or not H["exact_integer_list"](ready["bank_logical_subpages"], [0, 1])):
        raise RuntimeError("Unsupported page-presentation bank protocol/mode")
    # Only the already-checked new protocol labels are projected. The existing
    # bounded source/channel/resource/original checks retain their exact rules.
    common = {key: value for key, value in ready.items() if key != "bank_logical_subpages"}
    common.update(probe_version=3, protocol="warm-drcs-live-ordinary-v1", stages=DRCS_STATES)
    return drcs_ready(common, directory)


def validate_page_presentation_stage(value, number, channels, previous, ready_sha256, initial_original):
    keys = {"probe_version", "stage", "state", "scope", "initial_ready_sha256", "day", "night",
        "day_first_bitmap_row", "night_first_bitmap_row", "day_s2", "night_s2", "expected_pages",
        "subcode", "expected_navigation", "wire_offsets", "editor_original", "day_presentation_source", "night_presentation_source"}
    closed(value, keys, "warm page-presentation bank stage")
    if (type(value["probe_version"]) is not int or value["probe_version"] != 4
            or type(value["stage"]) is not int or value["stage"] != number or number not in [1, 2]
            or value["state"] != PAGE_PRESENTATION_STATES[number - 1] or not text_value(value["scope"])
            or value["initial_ready_sha256"] != ready_sha256 or not hash_value(ready_sha256)
            or not H["exact_integer_list"](value["expected_pages"], [124, 125])
            or type(value["subcode"]) is not int or value["subcode"] != 32
            or not H["exact_integer_list"](value["expected_navigation"], [124, 125, 124])
            or not isinstance(value["wire_offsets"], list) or len(value["wire_offsets"]) != 3
            or any(type(offset) is not int or offset < 0 for offset in value["wire_offsets"])
            or value["day_presentation_source"] != ("magazine" if number == 1 else "page")
            or value["night_presentation_source"] != "magazine"):
        raise RuntimeError("Page-presentation stage/readiness/source mismatch")
    for key, expected in [("day_first_bitmap_row", 2049 if number == 1 else 2051),
                          ("night_first_bitmap_row", 2049), ("day_s2", number), ("night_s2", 1)]:
        if type(value[key]) is not int or value[key] != expected:
            raise RuntimeError("Page-presentation bitmap/counter mismatch")
    for theme in ["day", "night"]:
        item, initial = value[theme], channels[theme]
        closed(item, DRCS_CHANNEL_KEYS, "page-presentation stage channel")
        if (any(not H["positive_identity"](item[key]) for key in ["channel_id", "profile_id", "profile_revision",
                "design_id", "design_revision", "generation_id", "request_id"])
                or not hash_value(item["design_digest"])
                or not H["exact_integer_list"](item["theme_rgb4"], initial["theme_rgb4"])):
            raise RuntimeError("Invalid typed page-presentation identity/palette")
        if number == 1 or theme == "night":
            if item != initial:
                raise RuntimeError("Unchanged page-presentation channel drifted")
        else:
            changed = {"design_id", "design_revision", "design_digest", "generation_id", "request_id"}
            if (any(item[key] != initial[key] for key in DRCS_CHANNEL_KEYS - changed)
                    or item["design_revision"] != 2
                    or any(item[key] == initial[key] for key in changed)
                    or any(item[key] == channels["night"][key] for key in ["design_id", "generation_id", "request_id", "design_digest"])):
                raise RuntimeError("Page-presentation replacement lacks a distinct isolated concept/publication")
    if number == 1:
        if value["editor_original"] is not None:
            raise RuntimeError("Initial M29 stage cannot claim a new page original")
    else:
        if (not isinstance(previous, dict) or type(previous.get("stage")) is not int or previous["stage"] != 1):
            raise RuntimeError("Page-presentation stage skipped its original observed epoch")
        validate_page_presentation_stage(previous, 1, channels, None, ready_sha256, initial_original)
        if any(after <= before for after, before in zip(value["wire_offsets"], previous["wire_offsets"])):
            raise RuntimeError("Every viewer must advance across page-presentation publication")
        original = value["editor_original"]
        closed(original, {"design_id", "revision", "digest", "receipt_id", "receipt_digest"}, "new page editor original")
        if (not H["positive_identity"](original["design_id"]) or type(original["revision"]) is not int or original["revision"] != 1
                or original["design_id"] != value["day"]["design_id"]
                or original["design_id"] in [channels["day"]["design_id"], channels["night"]["design_id"]]
                or not hash_value(original["digest"])
                or original["digest"] in [initial_original["digest"], channels["night"]["design_digest"], value["day"]["design_digest"]]
                or not H["positive_identity"](original["receipt_id"]) or original["receipt_id"] == initial_original["receipt_id"]
                or not hash_value(original["receipt_digest"]) or original["receipt_digest"] == initial_original["receipt_digest"]):
            raise RuntimeError("New page original/receipt/concept association is stale or crosses channels")
    return value

def drcs_decoder_identity(client):
    """Require the original decoder instance; reactivation would hide cache bugs."""
    if client["process"].poll() is not None:
        raise RuntimeError("Owned DRCS client stopped during the live stages")
    log = bounded_bytes(Path(client["log"].name), H["MAX_LOG"]).decode(errors="replace")
    starts = re.findall(r'^\[([0-9a-fA-F]+)\].*using spu decoder module "zvbi"\s*$', log, re.MULTILINE)
    stops = re.findall(r'removing module "zvbi"', log)
    if len(starts) != 1 or stops:
        raise RuntimeError("DRCS warm proof requires one retained active decoder instance")
    identity = starts[0]
    if client.get("drcs_decoder_identity", identity) != identity:
        raise RuntimeError("DRCS client replaced its decoder instance")
    client["drcs_decoder_identity"] = identity
    return identity


def stock_mapping_qualification(sources):
    """Authenticate the existing owned-process mapper for the v4 trial only."""
    path = REPO / "scripts/e2e/teletext-client-decoder-candidate.py"
    body = bounded_bytes(path, H["MAX_READY"])
    actual = digest(body)
    if actual != "f1acfc1ab867e67b2ca3e1eecb26e1289e0192c62b9f78b383adec6832a0d4ec":
        raise RuntimeError("Client decoder qualification helper source changed")
    qualification = {"__file__": str(path), "__name__": "authenticated_warm_client_decoder_qualification"}
    exec(compile(body, str(path), "exec"), qualification)
    sources[str(path.relative_to(REPO))] = actual
    return qualification


def stock_decoder_mapping(client, qualification):
    """Require the receipted stock decoder/plugin mapped by this owned PID."""
    decoder = {"path": "/usr/lib/x86_64-linux-gnu/libzvbi.so.0.13.2",
               "sha256": "134705a7ef4d982eb57bf71a9378f3eb0d95237c8dcb686d684e35471ae834b0",
               "label": "Ubuntu libzvbi 0.2.42-2", "test_only": False}
    plugin = Path("/usr/lib/x86_64-linux-gnu/vlc/plugins/codec/libzvbi_plugin.so")
    return qualification["verify_process"](client["process"].pid, decoder, plugin,
        "b87f42fb841588fe68c0845f4cd56dd1062b287eb92faa5f9a086e5ce5fefd00")



def start_client(viewer, url, directory, index, deadline, first_page=124):
    before = windows(deadline)
    env = os.environ.copy()
    env["QT_QPA_PLATFORM"] = "xcb"
    for name in ["XDG_CONFIG_HOME", "XDG_CACHE_HOME"]:
        folder = directory / f"{viewer}-{name.lower()}"
        folder.mkdir()
        env[name] = str(folder)
    log = (directory / f"{viewer}-warm-vlc.log").open("xb")
    process = subprocess.Popen(
        ["/usr/bin/vlc", "--no-one-instance", "--no-media-library", "--no-qt-privacy-ask",
         "--ignore-config", "--no-metadata-network-access", "--aout=adummy", "--vout=xcb_x11",
         "--network-caching=100", "--rate=1", "--vbi-opaque", f"--vbi-page={first_page}",
         "--key-subtitle-track=v", "--no-video-title-show", "--no-fullscreen",
         "--no-qt-video-autoresize", "--vbi-level=2", "--no-vbi-text",
         "--play-and-exit", "--verbose=2", url], env=env, stdout=log, stderr=log, cwd=REPO,
    )
    client = {"viewer": viewer, "process": process, "log": log, "url": url}
    try:
        window_deadline = time.monotonic() + remaining(deadline, 10)
        while True:
            matches = []
            for window in windows(deadline) - before:
                try:
                    owned_window(window, process.pid, deadline)
                except RuntimeError:
                    continue
                matches.append(window)
            if matches:
                if len(matches) != 1:
                    raise RuntimeError("Ambiguous owned warm client window")
                client["window"] = matches[0]
                break
            if process.poll() is not None or time.monotonic() >= window_deadline:
                raise RuntimeError("Owned warm client failed to expose its window")
            settle(deadline, 0.1)
        gui(deadline, "move", client["window"], 30 + 640 * index, 30, 600, 440)
        settle(deadline, 0.5)
        return client
    except BaseException:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)
        log.close()
        raise


def origin(client, deadline):
    owned_window(client["window"], client["process"].pid, deadline)
    return json.loads(command([sys.executable, WHERE, client["window"]], deadline))


def decoder_active(client):
    log_path = Path(client["log"].name)
    if log_path.stat().st_size:
        log = bounded_bytes(log_path, H["MAX_LOG"]).decode(errors="replace")
        started = log.rfind('using spu decoder module "zvbi"')
        stopped = log.rfind('removing module "zvbi"')
        return started >= 0 and started > stopped
    return False


def activate(client, deadline):
    if decoder_active(client):
        return  # Existing selected decoder: clicking again would disable it.
    position = origin(client, deadline)
    gui(deadline, "focus", client["window"])
    gui(deadline, "click-window", client["window"], 39, 394,
        position["x"] + 39, position["y"] + 394)
    activation_deadline = time.monotonic() + remaining(deadline, 8)
    while not decoder_active(client):
        if time.monotonic() >= activation_deadline:
            raise RuntimeError("Native teletext control did not select the decoder")
        settle(deadline, 0.2)


def activate_stage(clients, version, stage, deadline):
    if version in [3, 4]:
        if stage == 1:
            for client in clients:
                activate(client, deadline)
        for client in clients:
            drcs_decoder_identity(client)
    elif version == 2:
        if stage == 1:
            for client in clients:
                activate(client, deadline)
        elif stage == 2:
            activate(clients[0], deadline)
    elif stage == 1:
        activate(clients[1], deadline)
    elif stage in [2, 3, 5]:
        activate(clients[0], deadline)


def capture(client, directory, stage, page, visit, deadline):
    owned_window(client["window"], client["process"].pid, deadline)
    file = directory / f"stage-{stage}-{client['viewer']}-{visit}.png"
    if file.exists() or file.is_symlink():
        raise RuntimeError("Warm image destination already exists")
    subprocess.run(["import", "-window", str(client["window"]), str(file)],
                   check=True, timeout=remaining(deadline, 10))
    return {"viewer": client["viewer"], "page": page, "file": file.name,
            "sha256": digest(bounded_bytes(file, H["MAX_IMAGE"]))}


def main():
    started = time.monotonic()
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("native_ready_file", type=Path)
    args = parser.parse_args()
    if not os.environ.get("DISPLAY"):
        raise RuntimeError("A real headed X11 display is required")
    sources = {}
    for path, expected in H["RECEIPTED"].items():
        actual = digest(bounded_bytes(path, H["MAX_LOG"]))
        if actual != expected:
            raise RuntimeError("Receipted client/observation identity changed")
        sources[str(path.relative_to(REPO)) if path.is_relative_to(REPO) else str(path)] = actual
    for path in [HELPERS, Path(__file__)]:
        sources[str(path.relative_to(REPO))] = digest(bounded_bytes(path, H["MAX_LOG"]))
    ready_path = args.native_ready_file.absolute()
    raw_ready, ready = load(ready_path)
    directory = ready_path.parent
    version = ready["probe_version"]
    channels = (page_presentation_ready(ready, directory) if version == 4
                else drcs_ready(ready, directory) if version == 3
                else scoped_ready(ready, directory) if version == 2 else None)
    if version in [2, 3, 4] and digest(bounded_bytes(HELPERS, H["MAX_LOG"])) != COLD_HELPER_SHA256:
        raise RuntimeError("Pinned cold helper source identity changed")
    qualification = stock_mapping_qualification(sources) if version == 4 else None
    selected_urls = {theme: item["url"] for theme, item in channels.items()} if channels else urls(ready)
    deadline = (started + ready["completion_timeout_seconds"] if version in [3, 4]
                else scoped_deadline(ready, channels, started) if channels else time.monotonic() + 150)
    first_page, next_page = (200, 201) if version == 2 else (124, 125)
    stages = 2 if version in [3, 4] else 3 if channels else 5
    previous = None
    clients = []
    with (directory / "headed-warm-trial.ndjson").open("x", encoding="utf-8") as record:
        def emit(value):
            record.write(json.dumps(value, sort_keys=True) + "\n")
            record.flush()
            print(json.dumps(value, sort_keys=True), flush=True)

        emit({"event": "identity", "sources": sources, "ready_sha256": digest(raw_ready),
              "client": "VLC 3.0.20-3build6", "decoder": "libzvbi 0.2.42-2",
              "rendered_navigation_proven": False, "full_level25_proven": False})
        try:
            for index, viewer in enumerate(["day", "night"]):
                client = start_client(viewer, selected_urls[viewer], directory, index, deadline, first_page)
                clients.append(client)
                emit({"event": "started", "viewer": viewer, "pid": client["process"].pid,
                      "window": client["window"], "url": client["url"]})
            for stage in range(1, stages + 1):
                raw_stage, stage_ready = wait_stage(directory, stage, deadline)
                if version == 4:
                    validate_page_presentation_stage(stage_ready, stage, channels, previous, digest(raw_ready), ready["editor_original"])
                elif version == 3:
                    validate_drcs_stage(stage_ready, stage, channels, previous, digest(raw_ready))
                elif channels:
                    validate_stage(stage_ready, stage, channels, previous)
                emit({"event": "native_stage", "stage": stage, "ready": stage_ready})
                activate_stage(clients, version, stage, deadline)
                if version == 4:
                    for client in clients:
                        emit({"event": "decoder_mapping", "stage": stage, "phase": "activated",
                              "viewer": client["viewer"], "mapping": stock_decoder_mapping(client, qualification)})
                images = []
                navigation = version in [3, 4] or stage == (1 if channels else 2)
                for visit, page in enumerate([first_page, next_page, first_page] if navigation else [first_page]):
                    if navigation and visit:
                        for client in clients:
                            position = origin(client, deadline)
                            y = 389 if page == next_page else 400
                            gui(deadline, "click-window", client["window"], 121, y,
                                position["x"] + 121, position["y"] + y)
                    settle(deadline, 3)
                    for client in clients:
                        if version in [3, 4]:
                            drcs_decoder_identity(client)
                        if version == 4:
                            emit({"event": "decoder_mapping", "stage": stage, "phase": "before_capture",
                                  "viewer": client["viewer"], "mapping": stock_decoder_mapping(client, qualification)})
                        absent = False if version in [3, 4] else stage == 3 if channels else stage in [1, 4]
                        requested = None if client["viewer"] == "day" and absent else page
                        image = capture(client, directory, stage, requested, visit, deadline)
                        images.append(image)
                        emit({"event": "captured", "stage": stage, **image,
                              "rendered_page_verified": False})
                marker = {"probe_version": version, "stage": stage, "ready_sha256": digest(raw_stage),
                          "clients": [{"viewer": c["viewer"], "pid": c["process"].pid} for c in clients],
                          "screenshots": images}
                closed_new(directory / f"stage-{stage}-captured.json", marker)
                previous = stage_ready
            stopped_clients = []
            for client in clients:
                owned_window(client["window"], client["process"].pid, deadline)
                decoder_identity = drcs_decoder_identity(client) if version in [3, 4] else None
                if version == 4:
                    emit({"event": "decoder_mapping", "stage": stages, "phase": "before_stop",
                          "viewer": client["viewer"], "mapping": stock_decoder_mapping(client, qualification)})
                # Target the owned process directly; a global shortcut could
                # cross WSLg focus and close a different application.
                client["process"].terminate()
                status = client["process"].wait(timeout=remaining(deadline, 10))
                emit({"event": "finished", "viewer": client["viewer"], "client_exit": status,
                      "stop": "owned SIGTERM after all screenshots"})
                if status not in [0, -signal.SIGTERM]:
                    raise RuntimeError("Owned warm client failed controlled teardown")
                if version in [3, 4]:
                    stopped_clients.append({"viewer": client["viewer"], "pid": client["process"].pid,
                        "exit_code": status, "decoder_identity": decoder_identity,
                        "decoder_starts": 1, "decoder_stops_before_shutdown": 0})
            if version in [3, 4]:
                closed_new(directory / "clients-stopped.json", {"probe_version": version,
                    "ready_sha256": digest(raw_ready), "clients": stopped_clients})
            emit({"event": "captures_complete", "native_completion_released": False,
                  "rendered_navigation_proven": False,
                  "next": "Inspect all stage images before root-only native completion."})
        except BaseException as failure:
            emit({"event": "failed", "reason": str(failure), "rendered_navigation_proven": False})
            raise
        finally:
            for client in clients:
                process = client["process"]
                if process.poll() is None:
                    process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=5)
                client["log"].close()


if __name__ == "__main__":
    main()
