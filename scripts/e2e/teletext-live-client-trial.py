#!/usr/bin/env python3
"""Headed cold client trial against the disposable native ordinary stream host.

Supply its bounded ready.json and a NEW evidence directory. Both VLC windows
attach directly to ErsatzRS URLs. No replay server or packet rewrite is used.
Requested page controls and screenshots require independent visual inspection;
this script does not certify navigation, panels or complete Level 2.5 acceptance
and never releases the native completion marker automatically.
"""

import argparse
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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("native_ready_file", type=Path)
    parser.add_argument("new_output_directory", type=Path)
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
    def unique_pairs(pairs):
        value = {}
        for key, item in pairs:
            if key in value:
                raise RuntimeError("Duplicate native readiness field")
            value[key] = item
        return value
    ready = json.loads(raw_ready, object_pairs_hook=unique_pairs)
    if not isinstance(ready, dict):
        raise RuntimeError("Expected native readiness object")
    if ready.get("probe_version") != 1:
        raise RuntimeError("Unsupported native readiness version")
    channels = ready.get("channels")
    if (not isinstance(channels, list) or len(channels) != 2
            or not all(isinstance(v, dict) for v in channels)):
        raise RuntimeError("Expected two native ready channels")
    by_theme, ports = {}, set()
    for expected_id, theme in [(1, "day"), (2, "night")]:
        channel = next((v for v in channels if v.get("channel_id") == expected_id), None)
        if not channel or channel.get("theme") != theme:
            raise RuntimeError("Native readiness channel/theme mismatch")
        parsed = urlsplit(channel.get("url", ""))
        if (parsed.scheme != "http" or parsed.hostname != "127.0.0.1"
                or parsed.username is not None or parsed.password is not None
                or not parsed.port or parsed.path != f"/iptv/hls-direct/{expected_id}.ts"
                or parsed.query or parsed.fragment):
            raise RuntimeError("Native trial requires exact loopback ordinary URLs")
        ports.add(parsed.port)
        by_theme[theme] = channel
    if len(ports) != 1:
        raise RuntimeError("Expected one owned native host port")
    destination = args.new_output_directory.absolute()
    destination.mkdir()  # Existing directories/files/symlinks are never overwritten.
    record = (destination / "trial.ndjson").open("x", encoding="utf-8")
    clients = []
    deadline = time.monotonic() + TRIAL_SECONDS

    def emit(value):
        encoded = json.dumps(value, sort_keys=True)
        record.write(encoded + "\n")
        record.flush()
        print(encoded, flush=True)

    try:
        emit({"event": "identity", "sources": sources, "native_ready_sha256": digest(raw_ready),
              "channels": channels, "source": ready.get("source"),
              "client": "VLC 3.0.20-3build6", "decoder": "libzvbi 0.2.42-2",
              "scope": "MAIN glyph/palette and requested page navigation inspection",
              "panel_display_proven": False, "rendered_navigation_proven": False,
              "limitations": "Installed formatter has 41 columns; 56-column side panels remain unproven."})
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
                 "--network-caching=100", "--rate=1", "--vbi-opaque", "--vbi-page=124", "--key-subtitle-track=v",
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
        for page, stage in [("124", "first"), ("125", "next"), ("124", "return")]:
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
                      "rendered_page_verified": False, "expected_header": f"{page}.20"})
        for owned in clients:
            client_exit = owned["client"].wait(timeout=remaining(deadline, 80))
            emit({"event": "finished", "theme": owned["theme"], "client_exit": client_exit,
                  "ordinary_url": owned["metadata"]["url"]})
            if client_exit != 0:
                raise RuntimeError("Owned live-route client did not exit successfully")
            for key in ["client_log"]:
                owned[key].flush()
                body = bounded_bytes(Path(owned[key].name), MAX_LOG) if Path(owned[key].name).stat().st_size else b""
                emit({"event": "log", "theme": owned["theme"], "name": Path(owned[key].name).name,
                      "bytes": len(body), "sha256": digest(body)})
        emit({"event": "trial_complete", "ordinary_live_url_attachment_and_client_exit_proven": True,
              "native_completion_released": False,
              "rendered_navigation_proven": False, "theme_isolation_proven": False,
              "panel_display_proven": False, "next": "Inspect actual headers, day-only Q and red/blue indexed DRCS/Z in six captured windows."})
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
