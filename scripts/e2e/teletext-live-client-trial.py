#!/usr/bin/env python3
"""Headed cold client trial against the disposable native ordinary stream host.

Supply its bounded ready.json and a NEW evidence directory. Both VLC windows
attach directly to ErsatzRS URLs. No replay server or packet rewrite is used.
Requested page controls and screenshots require independent visual inspection;
this script does not certify navigation, panels or complete Level 2.5 acceptance
and never releases the native completion marker automatically.

Explicit --captured-replay uses two byte-identical finite /synthetic.ts replays
of hash-checked ordinary captures. That mode does not attach to the application
and proves neither live updates nor source provenance on its own.
"""

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time
from urllib.parse import urlsplit

REPO = Path(__file__).resolve().parents[2]
GUI = REPO / "scripts/e2e/teletext-native-gui.py"
WHERE = REPO / "scripts/e2e/teletext-native-where.py"
MAX_READY = 64 * 1024
MAX_IMAGE = 16 * 1024 * 1024
MAX_LOG = 8 * 1024 * 1024
MAX_CAPTURE = 8 * 1024 * 1024
MAX_LIVE_SOURCE = 48 * 1024 * 1024
TRIAL_SECONDS = 120
RECEIPTED = {
    GUI: "2ae2f7d95241027194f1355341a1fe51cde6860bce6a248c5b181194ba3eaf15",
    WHERE: "2b4a7e472eb8121fe42e614836e55432f8880e10151fa13ea66ea27c4f3e0b87",
    Path("/usr/bin/vlc"): "1fbb0287954bfb1669927eb8542e82af01508b1fd96cce0cf1ada05a6980bb43",
    Path("/usr/lib/x86_64-linux-gnu/vlc/plugins/codec/libzvbi_plugin.so"): "b87f42fb841588fe68c0845f4cd56dd1062b287eb92faa5f9a086e5ce5fefd00",
    Path("/usr/lib/x86_64-linux-gnu/libzvbi.so.0.13.2"): "134705a7ef4d982eb57bf71a9378f3eb0d95237c8dcb686d684e35471ae834b0",
}


def bounded_bytes(path, maximum):
    if path.is_symlink() or not path.is_file():
        raise RuntimeError("Expected a bounded regular proof input")
    with path.open("rb") as handle:
        body = handle.read(maximum + 1)
    if not body or len(body) > maximum:
        raise RuntimeError("Proof input is empty or exceeds its byte bound")
    return body


def digest(body):
    return hashlib.sha256(body).hexdigest()


def remaining(deadline, maximum=5):
    value = min(maximum, deadline - time.monotonic())
    if value <= 0:
        raise TimeoutError("Headed trial deadline")
    return value


def command(arguments, deadline):
    output = subprocess.check_output(
        [str(value) for value in arguments], text=True,
        timeout=remaining(deadline), stderr=subprocess.STDOUT,
    )
    if len(output.encode()) > 65_536:
        raise RuntimeError("GUI observation exceeds bounded output")
    return output


def gui(deadline, *arguments):
    return command([sys.executable, GUI, *arguments], deadline)


def windows(deadline):
    return {json.loads(line)["window"] for line in gui(deadline, "list").splitlines()}


def assert_owned_window(window, pid, deadline):
    facts = command(["xprop", "-id", str(window), "_NET_WM_PID"], deadline)
    matched = re.fullmatch(r"_NET_WM_PID\(CARDINAL\) = ([0-9]+)\n?", facts)
    if not matched or int(matched.group(1)) != pid:
        raise RuntimeError("Client window lacks exact owned process identity")


def settle(deadline, seconds):
    if remaining(deadline, seconds + 1) < seconds:
        raise TimeoutError("Insufficient trial time to settle requested controls")
    time.sleep(seconds)


def replay_ready(ready, directory):
    if (set(ready) != {"probe_version", "mode", "expected_pages", "subcode", "viewers"}
            or type(ready["probe_version"]) is not int or ready["probe_version"] != 1
            or ready["mode"] != "captured-ordinary-replay"
            or not exact_integer_list(ready["expected_pages"], [200, 201])
            or type(ready["subcode"]) is not int or ready["subcode"] != 0
            or not isinstance(ready["viewers"], list) or len(ready["viewers"]) != 2):
        raise RuntimeError("Expected closed captured-ordinary replay manifest for pages 200/201")
    by_theme, ports = {}, set()
    for item in ready["viewers"]:
        if (not isinstance(item, dict) or set(item) != {"viewer", "url", "capture"}
                or item["viewer"] not in ["day", "night"] or item["viewer"] in by_theme):
            raise RuntimeError("Expected two distinct replay viewer labels without application IDs")
        provenance = item["capture"]
        if (not isinstance(provenance, dict) or set(provenance) != {"file", "bytes", "sha256"}
                or not isinstance(provenance["file"], str)
                or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]{0,159}", provenance["file"])
                or type(provenance["bytes"]) is not int
                or not isinstance(provenance["sha256"], str)
                or not re.fullmatch(r"[0-9a-f]{64}", provenance["sha256"])):
            raise RuntimeError("Expected bounded capture basename, exact byte count and SHA256")
        body = bounded_bytes(directory / provenance["file"], MAX_CAPTURE)
        if (len(body) != provenance["bytes"] or digest(body) != provenance["sha256"]
                or len(body) % 188 or any(body[offset] != 0x47 for offset in range(0, len(body), 188))):
            raise RuntimeError("Captured ordinary bytes do not match their replay manifest")
        if not isinstance(item["url"], str):
            raise RuntimeError("Expected literal owned replay URL")
        parsed = urlsplit(item["url"])
        if (parsed.scheme != "http" or parsed.hostname != "127.0.0.1"
                or parsed.username is not None or parsed.password is not None
                or not parsed.port or parsed.path != "/synthetic.ts" or parsed.query or parsed.fragment):
            raise RuntimeError("Captured replay requires exact loopback /synthetic.ts URLs")
        ports.add(parsed.port)
        by_theme[item["viewer"]] = item
    if set(by_theme) != {"day", "night"} or len(ports) != 2:
        raise RuntimeError("Expected two distinct owned one-request replay server ports")
    return by_theme


def exact_integer_list(value, expected):
    return isinstance(value, list) and value == expected and all(type(item) is int for item in value)


def exact_cell(value, row):
    return (isinstance(value, dict) and set(value) == {"row", "column"}
            and type(value["row"]) is int and value["row"] == row
            and type(value["column"]) is int and value["column"] == 8)


def positive_identity(value):
    return type(value) is int and 0 < value <= 9223372036854775807


def utc_expiry(value):
    if not isinstance(value, str) or not re.fullmatch(
            r"[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}(?:\.[0-9]{1,9})?(?:Z|\+00:00)", value):
        raise RuntimeError("Expected the original UTC scope expiry")
    try:
        instant = datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError:
        raise RuntimeError("Invalid original UTC scope expiry") from None
    if instant <= datetime.now(timezone.utc):
        raise RuntimeError("The original effective scope has expired")
    return instant


def ordinary_url(value, channel_id):
    if not isinstance(value, str):
        raise RuntimeError("Expected a literal owned ordinary URL")
    try:
        parsed = urlsplit(value)
        port = parsed.port
    except ValueError:
        raise RuntimeError("Invalid owned ordinary URL") from None
    if (parsed.scheme != "http" or parsed.hostname != "127.0.0.1"
            or parsed.username is not None or parsed.password is not None
            or not port or parsed.netloc != f"127.0.0.1:{port}"
            or parsed.path != f"/iptv/hls-direct/{channel_id}.ts" or parsed.query or parsed.fragment):
        raise RuntimeError("Native trial requires exact loopback ordinary URLs")
    return port


def legacy_ready(ready):
    channels = ready.get("channels")
    if not isinstance(channels, list) or len(channels) != 2 or not all(isinstance(v, dict) for v in channels):
        raise RuntimeError("Expected two native ready channels")
    by_theme, ports = {}, set()
    for expected_id, theme in [(1, "day"), (2, "night")]:
        channel = next((v for v in channels if type(v.get("channel_id")) is int and v["channel_id"] == expected_id), None)
        if not channel or channel.get("theme") != theme:
            raise RuntimeError("Native readiness channel/theme mismatch")
        if (not exact_integer_list(channel.get("expected_pages"), [124, 125]) or type(channel.get("subcode")) is not int
                or channel["subcode"] != 32 or not exact_integer_list(channel.get("expected_navigation"), [124, 125, 124])):
            raise RuntimeError("Legacy native trial requires pages124/125, subcode32 and exact navigation")
        ports.add(ordinary_url(channel.get("url"), expected_id))
        by_theme[theme] = channel
    if len(ports) != 1:
        raise RuntimeError("Expected one owned native host port")
    return by_theme


def effective_ready(ready, directory):
    keys = {"probe_version", "scope", "source", "channels", "active_transcode_limit", "completion_file",
            "completion_timeout_seconds", "expected_completion", "client_actions", "limitations"}
    if set(ready) != keys or type(ready["probe_version"]) is not int or ready["probe_version"] != 2:
        raise RuntimeError("Expected closed cumulative effective live readiness")
    if any(not isinstance(ready[key], str) or not ready[key] or len(ready[key]) > 2048
           for key in ["scope", "client_actions", "limitations"]):
        raise RuntimeError("Expected bounded live trial descriptions")
    if (type(ready["active_transcode_limit"]) is not int or ready["active_transcode_limit"] != 4
            or ready["completion_file"] != "client-complete.json"
            or type(ready["completion_timeout_seconds"]) is not int or ready["completion_timeout_seconds"] != 180
            or not isinstance(ready["expected_completion"], dict)
            or type(ready["expected_completion"].get("probe_version")) is not int
            or ready["expected_completion"] != {"probe_version": 2, "client_name": "VLC", "client_version": "3.0.20",
                "decoder_version": "0.2.42", "proof_suffix": "cumulative-effective-live-ordinary"}):
        raise RuntimeError("Unsupported cumulative effective client completion identity")
    source = ready["source"]
    if (not isinstance(source, dict) or set(source) != {"file", "sha256", "bytes", "duration_seconds"}
            or source["file"] != "source.ts" or type(source["bytes"]) is not int
            or not 0 < source["bytes"] <= MAX_LIVE_SOURCE or type(source["duration_seconds"]) is not int
            or source["duration_seconds"] != 60 or not isinstance(source["sha256"], str)
            or not re.fullmatch(r"[0-9a-f]{64}", source["sha256"])):
        raise RuntimeError("Expected bounded sixty-second native source provenance")
    body = bounded_bytes(directory / source["file"], MAX_LIVE_SOURCE)
    if (len(body) != source["bytes"] or digest(body) != source["sha256"] or len(body) % 188
            or any(body[offset] != 0x47 or body[offset + 1] & 0x80 for offset in range(0, len(body), 188))):
        raise RuntimeError("Native source differs from its bounded framing/hash provenance")
    if any(((body[offset + 1] & 0x1f) << 8 | body[offset + 2]) == 512 for offset in range(0, len(body), 188)):
        raise RuntimeError("Native ordinary source already contains the authored PID")
    channel_keys = {"channel_id", "theme", "url", "addon_id", "profile_id", "profile_revision", "theme_rgb4",
        "generation_id", "request_id", "design_id", "design_revision", "scoped_role", "original_design_id",
        "original_revision", "original_digest", "root_receipt_id", "root_receipt_digest", "original_valid_until",
        "expected_pages", "subcode", "expected_navigation", "edited_q_page", "edited_r_page", "edited_q_cell", "edited_r_cell"}
    channels = ready["channels"]
    if not isinstance(channels, list) or len(channels) != 2:
        raise RuntimeError("Expected two actual scoped ordinary channels")
    by_theme, ports = {}, set()
    for item in channels:
        if not isinstance(item, dict) or set(item) != channel_keys or item["theme"] not in ["day", "night"]:
            raise RuntimeError("Expected closed scoped channel identities")
        if item["theme"] in by_theme:
            raise RuntimeError("Duplicate scoped theme")
        for key in ["channel_id", "profile_id", "profile_revision", "generation_id", "request_id", "design_id",
                    "design_revision", "original_design_id", "original_revision", "root_receipt_id"]:
            if not positive_identity(item[key]):
                raise RuntimeError("Invalid scoped original/current identity")
        if (type(item["scoped_role"]) is not int or item["scoped_role"] != 2
                or item["original_design_id"] != item["design_id"] or item["original_revision"] != 1
                or item["design_revision"] != (3 if item["theme"] == "day" else 1)):
            raise RuntimeError("Expected cumulative day revision3 and untouched night revision1 rooted in originals")
        if (not isinstance(item["addon_id"], str) or len(item["addon_id"]) > 255
                or not re.fullmatch(r"[a-z0-9]+(?:-[a-z0-9]+)*(?:\.[a-z0-9]+(?:-[a-z0-9]+)*){2,}", item["addon_id"])):
            raise RuntimeError("Invalid selected addon identity")
        if any(not isinstance(item[key], str) or not re.fullmatch(r"[0-9a-f]{64}", item[key])
               for key in ["original_digest", "root_receipt_digest"]):
            raise RuntimeError("Invalid immutable original/receipt digest")
        if (not exact_integer_list(item["expected_pages"], [200, 201]) or type(item["subcode"]) is not int or item["subcode"] != 0
                or not exact_integer_list(item["expected_navigation"], [200, 201, 200])
                or not exact_integer_list(item["theme_rgb4"], [15, 0, 0] if item["theme"] == "day" else [0, 0, 15])
                or any((type(item[key]) is not int or item[key] != 200) if item["theme"] == "day" else item[key] is not None
                       for key in ["edited_q_page", "edited_r_page"])
                or not exact_cell(item["edited_q_cell"], 2) or not exact_cell(item["edited_r_cell"], 3)):
            raise RuntimeError("Unsupported scoped pages, navigation, theme or Q/R edit ownership")
        utc_expiry(item["original_valid_until"])
        ports.add(ordinary_url(item["url"], item["channel_id"]))
        by_theme[item["theme"]] = item
    if set(by_theme) != {"day", "night"} or len(ports) != 1 or by_theme["day"]["addon_id"] != by_theme["night"]["addon_id"]:
        raise RuntimeError("Expected two themes of one addon on one actual native host")
    for key in ["channel_id", "profile_id", "generation_id", "request_id", "design_id", "root_receipt_id"]:
        if by_theme["day"][key] == by_theme["night"][key]:
            raise RuntimeError("Scoped channel/profile/publication identities are not isolated")
    return by_theme


def validate_readiness(raw, directory, captured_replay=False):
    def unique_pairs(pairs):
        value = {}
        for key, item in pairs:
            if key in value:
                raise RuntimeError("Duplicate native readiness field")
            value[key] = item
        return value
    ready = json.loads(raw, object_pairs_hook=unique_pairs)
    if not isinstance(ready, dict) or type(ready.get("probe_version")) is not int:
        raise RuntimeError("Expected native readiness object/version")
    if captured_replay:
        return ready, replay_ready(ready, directory), "200", "201", 0
    if ready["probe_version"] == 1:
        return ready, legacy_ready(ready), "124", "125", 32
    if ready["probe_version"] == 2:
        return ready, effective_ready(ready, directory), "200", "201", 0
    raise RuntimeError("Unsupported native readiness version")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("native_ready_file", type=Path)
    parser.add_argument("new_output_directory", type=Path)
    parser.add_argument("--captured-replay", action="store_true",
                        help="explicit finite captured-ordinary replay manifest; no live application attachment")
    args = parser.parse_args()
    if not os.environ.get("DISPLAY"):
        raise RuntimeError("A real headed X11 display is required")
    for executable in ["xprop", "import"]:
        if not shutil.which(executable):
            raise RuntimeError("Required existing client observation tool is absent")
    sources = {}
    for path, expected in RECEIPTED.items():
        actual = digest(bounded_bytes(path, MAX_LOG))
        if actual != expected:
            raise RuntimeError("Receipted client/tool source identity changed")
        label = str(path.relative_to(REPO)) if path.is_relative_to(REPO) else str(path)
        sources[label] = actual
    sources["client-trial.py"] = digest(bounded_bytes(Path(__file__), MAX_LOG))
    ready_path = args.native_ready_file.absolute()
    raw_ready = bounded_bytes(ready_path, MAX_READY)
    ready, by_theme, first_page, next_page, subcode = validate_readiness(raw_ready, ready_path.parent, args.captured_replay)
    channels = ready.get("channels")
    effective = not args.captured_replay and ready["probe_version"] == 2
    original_started = time.monotonic()
    deadline = original_started + TRIAL_SECONDS
    if effective:
        # Conservatively clamp the one original trial clock to both immutable
        # UTC expiries; observations and page actions never renew either scope.
        remaining_scope = min((utc_expiry(item["original_valid_until"]) - datetime.now(timezone.utc)).total_seconds()
                              for item in channels)
        deadline = min(deadline, original_started + remaining_scope)

    destination = args.new_output_directory.absolute()
    destination.mkdir()  # Existing directories/files/symlinks are never overwritten.
    record = (destination / "trial.ndjson").open("x", encoding="utf-8")
    clients = []

    def emit(value):
        encoded = json.dumps(value, sort_keys=True)
        record.write(encoded + "\n")
        record.flush()
        print(encoded, flush=True)

    try:
        emit({"event": "identity", "sources": sources,
              **({"replay_manifest_sha256": digest(raw_ready), "viewers": ready["viewers"],
                  "captured_ordinary_replay": True, "live_application_url_attachment": False,
                  "expected_replay_pacing_rate": 0.25, "client_playback_rate": 0.25,
                  "source_provenance_proven": False}
                 if args.captured_replay else {"native_ready_sha256": digest(raw_ready),
                                              "channels": channels, "source": ready.get("source"), "live_application_url_attachment_expected": True,
                                              "effective_cumulative_role": effective}),
              "client": "VLC 3.0.20-3build6", "decoder": "libzvbi 0.2.42-2",
              "scope": "Finite captured-ordinary MAIN palette/AZ and requested page navigation inspection"
                       if args.captured_replay else "MAIN glyph/palette and requested page navigation inspection",
              "panel_display_proven": False, "rendered_navigation_proven": False,
              "expected_navigation": [first_page, next_page, first_page], "expected_subcode": subcode,
              "limitations": "Interactive numeric entry remains unproven; WSLg keyboard input can cross owned windows. Installed formatter has 41 columns; 56-column side panels remain unproven."
                             if args.captured_replay else "Interactive numeric entry, side panels, complete Level2.5, warm changes and whole-host restart remain unproven."})
        for index, theme in enumerate(["day", "night"]):
            before = windows(deadline)
            owned = {"theme": theme}
            clients.append(owned)
            metadata = by_theme[theme]
            env = os.environ.copy()
            env["QT_QPA_PLATFORM"] = "xcb"
            for name in ["XDG_CONFIG_HOME", "XDG_CACHE_HOME"]:
                folder = destination / f"{theme}-{name.lower()}"
                folder.mkdir()
                env[name] = str(folder)
            client_log = (destination / f"{theme}-vlc.log").open("xb")
            owned["client_log"] = client_log
            client = subprocess.Popen(
                ["/usr/bin/vlc", "--no-one-instance", "--no-media-library", "--no-qt-privacy-ask",
                 "--ignore-config", "--no-metadata-network-access", "--aout=adummy", "--vout=xcb_x11",
                 "--network-caching=100", "--rate=0.25" if args.captured_replay else "--rate=1",
                 "--vbi-opaque", f"--vbi-page={first_page}", "--key-subtitle-track=v",
                 "--no-video-title-show", "--no-fullscreen", "--no-qt-video-autoresize",
                 "--vbi-level=2", "--no-vbi-text", "--play-and-exit", "--verbose=2", metadata["url"]],
                env=env, stdout=client_log, stderr=client_log, cwd=REPO,
            )
            owned["client"] = client
            window_deadline = time.monotonic() + remaining(deadline, 10)
            while True:
                fresh = windows(deadline) - before
                owned_windows = []
                for window in fresh:
                    assert_owned_window(window, client.pid, deadline)
                    owned_windows.append(window)
                if owned_windows:
                    if len(owned_windows) != 1:
                        raise RuntimeError("Ambiguous owned client window")
                    window = owned_windows[0]
                    break
                if client.poll() is not None:
                    raise RuntimeError("Client exited before headed window")
                if time.monotonic() >= window_deadline:
                    raise TimeoutError("Owned client window deadline")
                settle(deadline, 0.1)
            gui(deadline, "move", window, 30 + 640 * index, 30, 600, 440)
            settle(deadline, 0.5)
            origin = json.loads(command([sys.executable, WHERE, str(window)], deadline))
            owned.update(window=window, origin=origin, metadata=metadata)
            emit({"event": "started", "theme": theme, "window": window, "origin": origin,
                  "client_pid": client.pid, **metadata})
        settle(deadline, 3)
        for owned in clients:
            gui(deadline, "focus", owned["window"])
            x, y = owned["origin"]["x"], owned["origin"]["y"]
            gui(deadline, "click", x + 249, y + 10)
            settle(deadline, 0.5)
            menu_image = destination / f"{owned['theme']}-subtitle-menu.png"
            subprocess.run(["import", "-window", str(owned["window"]), str(menu_image)], check=True, timeout=remaining(deadline, 10))
            gui(deadline, "key", "Escape")
            gui(deadline, "click", x + 39, y + 394)
            activation_deadline = time.monotonic() + remaining(deadline, 8)
            while True:
                log = Path(owned["client_log"].name).read_text(errors="replace")
                if 'using spu decoder module "zvbi"' in log:
                    emit({"event": "decoder_active", "theme": owned["theme"],
                          "selection": "native Qt teletext activation button forces spu-es"})
                    break
                if time.monotonic() >= activation_deadline:
                    raise RuntimeError("Native subtitle-track activation did not start zvbi")
                settle(deadline, 0.2)
        for page, stage in [(first_page, "first"), (next_page, "next"), (first_page, "return")]:
            for owned in clients:
                assert_owned_window(owned["window"], owned["client"].pid, deadline)
                x, y = owned["origin"]["x"], owned["origin"]["y"]
                gui(deadline, "focus", owned["window"])
                settle(deadline, 0.8)
                owned["origin"] = json.loads(command([sys.executable, WHERE, str(owned["window"])], deadline))
                x, y = owned["origin"]["x"], owned["origin"]["y"]
                emit({"event": "current_owned_origin", "theme": owned["theme"], "stage": stage,
                      "origin": owned["origin"]})
                # Actual native Qt spin arrows emit valueChanged directly;
                # avoid global keyboard focus crossing the two WSLg windows.
                if stage != "first":
                    gui(deadline, "click-window", owned["window"], 121, 389 if stage == "next" else 400, x + 121, y + (389 if stage == "next" else 400))
                    settle(deadline, 0.3)
            settle(deadline, 4)
            for owned in clients:
                assert_owned_window(owned["window"], owned["client"].pid, deadline)
                image = destination / f"{owned['theme']}-page{page}-{stage}.png"
                subprocess.run(["import", "-window", str(owned["window"]), str(image)],
                               check=True, timeout=remaining(deadline, 10))
                image_hash = digest(bounded_bytes(image, MAX_IMAGE))
                emit({"event": "captured", "theme": owned["theme"], "requested_page": page,
                      "stage": stage, "image": image.name, "sha256": image_hash,
                      "rendered_page_verified": False, "expected_header": f"{page}.{subcode:02X}"})
        for owned in clients:
            client_exit = owned["client"].wait(timeout=remaining(deadline, 80))
            emit({"event": "finished", "theme": owned["theme"], "client_exit": client_exit,
                  **({"replay_url": owned["metadata"]["url"]} if args.captured_replay
                     else {"ordinary_url": owned["metadata"]["url"]})})
            if client_exit != 0:
                raise RuntimeError("Owned live-route client did not exit successfully")
            for key in ["client_log"]:
                owned[key].flush()
                body = bounded_bytes(Path(owned[key].name), MAX_LOG) if Path(owned[key].name).stat().st_size else b""
                emit({"event": "log", "theme": owned["theme"], "name": Path(owned[key].name).name,
                      "bytes": len(body), "sha256": digest(body)})
        emit({"event": "trial_complete",
              **({"captured_ordinary_replay_and_client_exit_proven": True,
                  "ordinary_live_url_attachment_proven": False, "source_provenance_proven": False,
                  "expected_replay_pacing_rate": 0.25, "client_playback_rate": 0.25,
                  "interactive_numeric_entry_verified": False,
                  "limitations": "Interactive numeric entry remains unproven; this trial requests pages through the owned Qt spin arrows.",
                  "next": "Inspect actual 200/201 headers, AZ and distinct palettes; link original admin capture provenance separately."}
                 if args.captured_replay else {"ordinary_live_url_attachment_and_client_exit_proven": True,
                                               "next": ("Inspect actual200/201 headers, AZ, day200-only cumulative Q/R and isolated red/blue themes in six captured windows."
                                                        if effective else "Inspect actual headers, day-only Q and red/blue indexed DRCS/Z in six captured windows."),
                                               "effective_cumulative_role": effective, "interactive_numeric_entry_verified": False}),
              "native_completion_released": False,
              "rendered_navigation_proven": False, "theme_isolation_proven": False,
              "panel_display_proven": False})
    except Exception as failure:
        emit({"event": "failed", "reason": str(failure), "rendered_navigation_proven": False})
        raise
    finally:
        for owned in clients:
            for key in ["client"]:
                process = owned.get(key)
                if process is None:
                    continue
                if process.poll() is None:
                    process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=5)
            for key in ["client_log"]:
                if key in owned:
                    owned[key].close()
        record.close()


if __name__ == "__main__":
    main()
