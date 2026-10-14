#!/usr/bin/env python3
"""Capture headed warm transitions from the disposable native TCP fixture.

The same owned VLC processes and URLs survive all five stages. Capture barriers
only synchronize the native actor; they do not certify rendered content. Inspect
the actual images independently before writing the root-only completion marker.
Requires the previously receipted Linux x64/X11 client and observation tools.
"""

import argparse
import json
import os
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
    if not isinstance(value, dict) or value.get("probe_version") != 1:
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
    if value.get("stage") != stage:
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


def start_client(viewer, url, directory, index, deadline):
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
         "--network-caching=100", "--rate=1", "--vbi-opaque", "--vbi-page=124",
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
    selected_urls = urls(ready)
    directory = ready_path.parent
    deadline = time.monotonic() + 150
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
                client = start_client(viewer, selected_urls[viewer], directory, index, deadline)
                clients.append(client)
                emit({"event": "started", "viewer": viewer, "pid": client["process"].pid,
                      "window": client["window"], "url": client["url"]})
            for stage in range(1, 6):
                raw_stage, stage_ready = wait_stage(directory, stage, deadline)
                emit({"event": "native_stage", "stage": stage, "ready": stage_ready})
                if stage == 1:
                    activate(clients[1], deadline)
                elif stage in [2, 3, 5]:
                    activate(clients[0], deadline)
                images = []
                for visit, page in enumerate([124, 125, 124] if stage == 2 else [124]):
                    if stage == 2 and visit:
                        for client in clients:
                            position = origin(client, deadline)
                            y = 389 if page == 125 else 400
                            gui(deadline, "click-window", client["window"], 121, y,
                                position["x"] + 121, position["y"] + y)
                    settle(deadline, 3)
                    for client in clients:
                        requested = None if client["viewer"] == "day" and stage in [1, 4] else page
                        image = capture(client, directory, stage, requested, visit, deadline)
                        images.append(image)
                        emit({"event": "captured", "stage": stage, **image,
                              "rendered_page_verified": False})
                marker = {"probe_version": 1, "stage": stage, "ready_sha256": digest(raw_stage),
                          "clients": [{"viewer": c["viewer"], "pid": c["process"].pid} for c in clients],
                          "screenshots": images}
                closed_new(directory / f"stage-{stage}-captured.json", marker)
            for client in clients:
                owned_window(client["window"], client["process"].pid, deadline)
                # Target the owned process directly; a global shortcut could
                # cross WSLg focus and close a different application.
                client["process"].terminate()
                status = client["process"].wait(timeout=remaining(deadline, 10))
                emit({"event": "finished", "viewer": client["viewer"], "client_exit": status,
                      "stop": "owned SIGTERM after all screenshots"})
                if status not in [0, -signal.SIGTERM]:
                    raise RuntimeError("Owned warm client failed controlled teardown")
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
