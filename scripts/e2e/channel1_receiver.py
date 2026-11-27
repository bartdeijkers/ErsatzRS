"""Single libVLC 3 playback session. Tee its sole HTTP response before demux."""

import csv
import ctypes
import hashlib
import http.server
import json
import threading
import time
import urllib.request

import numpy as np
import vlc
from channel1_analysis import snapshot_hls_pair
from channel1_fixture import FPS, FRAMES, HEIGHT, WIDTH
from PIL import Image


def make_templates():
    # Read the human-visible glyphs after presentation; no private timestamp PID.
    from channel1_fixture import FONT
    from PIL import ImageDraw

    glyphs = {}
    for char in "0123456789:.":
        tile = Image.new("L", (23, 33), 24)
        ImageDraw.Draw(tile).text((0, 0), char, font=FONT, fill=236)
        glyphs[char] = np.asarray(tile) > 128
    return glyphs


def read_line(gray, y, length, glyphs):
    text = ""
    for col in range(length):
        tile = gray[y : y + 33, 12 + col * 23 : 35 + col * 23] > 128
        cost, char = min((np.count_nonzero(tile != g), ch) for ch, g in glyphs.items())
        if cost > 45:
            return ""
        text += char
    return text


def receive(url, out):
    class Tee(http.server.BaseHTTPRequestHandler):
        def log_message(self, *args):
            pass

        def do_GET(self):
            if self.path != "/channel.ts":
                self.send_error(404)
                return
            self.server.requests += 1
            if self.server.requests != 1:
                self.send_error(409)
                return
            try:
                with (
                    urllib.request.urlopen(url, timeout=35) as src,
                    (out / "received.ts").open("xb") as dst,
                ):
                    self.send_response(200)
                    self.send_header("Content-Type", "video/mp2t")
                    self.end_headers()
                    while chunk := src.read(188 * 256):
                        self.wfile.write(chunk)
                        dst.write(chunk)
            except (BrokenPipeError, ConnectionResetError, ConnectionAbortedError):
                pass

    proxy = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Tee)
    proxy.requests = 0
    threading.Thread(target=proxy.serve_forever, daemon=True).start()
    instance = vlc.Instance(
        "--ignore-config",
        "--no-video-title-show",
        "--no-sub-autodetect-file",
        "--avcodec-hw=none",
        "--network-caching=1000",
        "--no-http-reconnect",
    )
    if not instance:
        raise RuntimeError("libVLC initialization failed")
    player = instance.media_player_new()
    player.set_mrl(f"http://127.0.0.1:{proxy.server_port}/channel.ts")
    clock = vlc.libvlc_clock
    glyphs = make_templates()
    buffers = {}
    data = {
        "frames": [],
        "events": [],
        "audio_events": [],
        "switches": [],
        "vlc": vlc.libvlc_get_version().decode(),
        "epoch_us": None,
        "complete": False,
    }
    state = {"last": None, "media_frame": -1, "loop": 0, "mask": None}
    data["media_clock"] = []

    def media_position(now):
        # libVLC 3 publishes its media time a few times per second. Interpolate
        # at normal playback speed only while backed by a recent player event.
        anchor = state.get("clock_anchor")
        if anchor is None or not 0 <= now - anchor[0] <= 750000:
            return None
        return anchor[1] + (now - anchor[0]) / 1000

    masks, seam = out / "subtitle-masks", out / "transition"
    masks.mkdir()
    seam.mkdir()
    pcm = (out / "vlc.s16le").open("xb")
    stamps = (out / "pcm.csv").open("w", newline="")
    writer = csv.writer(stamps)
    writer.writerow(["presentation_us", "sample_offset", "sample_count", "callback_us"])
    offset = 0

    @vlc.CallbackDecorators.AudioPlayCb
    def audio(_, samples, count, pts):
        nonlocal offset
        arrival = clock()
        pcm.write(ctypes.string_at(samples, count * 2))
        writer.writerow((pts, offset, count, arrival))
        offset += count

    @vlc.CallbackDecorators.AudioFlushCb
    def flush(_, _unused_pts):
        # VLC 3's amem flush supplies no timestamp. Use its invocation clock
        # to discard queued samples, as required by the libVLC flush contract.
        data["audio_events"].append((clock(), "flush", None, offset))

    @vlc.CallbackDecorators.AudioPauseCb
    def pause(_, pts):
        data["audio_events"].append((clock(), "pause", pts, offset))

    @vlc.CallbackDecorators.AudioResumeCb
    def resume(_, pts):
        data["audio_events"].append((clock(), "resume", pts, offset))

    @vlc.CallbackDecorators.VideoLockCb
    def lock(_, planes):
        buf = ctypes.create_string_buffer(WIDTH * HEIGHT * 4)
        key = ctypes.addressof(buf)
        buffers[key] = buf
        planes[0] = key
        return key

    @vlc.CallbackDecorators.VideoDisplayCb
    def display(_, picture):
        try:
            now = clock()
            buf = buffers.pop(picture)
            gray = np.frombuffer(buf, dtype=np.uint8).reshape(HEIGHT, WIDTH, 4)[:, :, 1]
            frame = read_line(gray, 10, 4, glyphs)
            number = int(frame) if frame.isdigit() else -1
            previous = state["last"]
            if number >= 0:
                state["last"] = number
            if data["epoch_us"] is None:
                if number != 0 or previous is None or previous < FRAMES - 2 * FPS:
                    return
                data["epoch_us"] = now
                data["initial_audio_ids"] = list(selection or [])
                data["initial_audio_id"] = state.get("audio_id")
                data["initial_page_selected"] = selected_page
                data["media_epoch_ms"] = media_position(now)
                if data["media_epoch_ms"] is None:
                    raise ValueError("no fresh VLC media clock at the first displayed frame")
            elif number >= 0 and previous is not None and previous - number > FRAMES // 2:
                state["loop"] += 1
            if state["loop"] >= 2:
                data["complete"] = True
                data.setdefault("end_us", now)
                return
            timeline = state["loop"] * FRAMES + number if number >= 0 else -1
            if timeline >= 0:
                state["media_frame"] = timeline
            timer = read_line(gray, 43, 8, glyphs)
            # Source background below the two counters is uniform. Preserve the
            # complete subtitle area, and check the counter area independently.
            mask = gray[80:] > 128
            encoded = np.packbits(mask).tobytes()
            key = hashlib.sha256(encoded).hexdigest()[:24]
            if key != state["mask"]:
                dest = masks / (key + ".png")
                if not dest.exists():
                    Image.fromarray(mask).save(dest)
                state["mask"] = key
            index = len(data["frames"])
            if FRAMES - 3 * FPS <= timeline < FRAMES + 3 * FPS:
                Image.fromarray(gray).save(seam / f"{index:06d}.png")
            data["frames"].append((now, timeline, timer, key))
        except Exception as exc:  # noqa: BLE001 - propagate native callback errors to the gate
            data["callback_error"] = repr(exc)

    player.video_set_callbacks(lock, None, display, None)
    player.video_set_format("RV32", WIDTH, HEIGHT, WIDTH * 4)
    player.audio_set_callbacks(audio, pause, resume, flush, None, None)
    player.audio_set_format("S16N", 48000, 1)

    def buffering(event):
        if event.u.new_cache < 100:
            state["clock_anchor"] = None
        if data["epoch_us"] is not None and not data["complete"] and event.u.new_cache < 100:
            data["events"].append((clock(), "buffering", event.u.new_cache))

    def media_time(event):
        anchor = (clock(), event.u.new_time)
        state["clock_anchor"] = anchor
        data["media_clock"].append(anchor)

    events = player.event_manager()
    events.event_attach(vlc.EventType.MediaPlayerBuffering, buffering)
    events.event_attach(vlc.EventType.MediaPlayerTimeChanged, media_time)
    selection = None
    next_switch = 1
    selected_page = False
    started = time.monotonic()
    try:
        if player.play() < 0:
            raise RuntimeError("VLC refused the stream")
        while not data["complete"]:
            now = time.monotonic()
            if data.get("callback_error") or player.get_state() in (
                vlc.State.Ended,
                vlc.State.Error,
            ):
                break
            if (data["epoch_us"] is None and now - started > 80) or now - started > 720:
                break
            poll = int((now - started) * 10)
            if poll != state.get("track_poll"):
                state["track_poll"] = poll
                if data["epoch_us"] is None and not state.get("startup_segments") and poll % 5 == 0:
                    state["startup_segments"] = snapshot_hls_pair(out, out / "hls-start")
                ids = [i for i, _ in (player.audio_get_track_description() or []) if i >= 0]
                if ids and selection is None:
                    player.audio_set_track(ids[0])
                selection = ids or None
                state["audio_id"] = player.audio_get_track()
                subs = [i for i, _ in (player.video_get_spu_description() or []) if i >= 0]
                if selected_page and (not subs or player.video_get_teletext() != 888):
                    selected_page = False
                    if data["epoch_us"] is not None:
                        data.setdefault("page_losses", []).append(clock())
                if subs and not selected_page:
                    player.video_set_spu(subs[0])
                    player.video_set_teletext(888)
                    selected_page = player.video_get_teletext() == 888
            cmd = clock()
            position = media_position(cmd)
            origin = data.get("media_epoch_ms")
            if (
                next_switch < 60
                and position is not None
                and origin is not None
                and position - origin >= next_switch * 10000
            ):
                k = next_switch % 2
                target = selection[k] if selection and len(selection) > k else None
                current = player.audio_get_track()
                result = (
                    player.audio_set_track(target)
                    if target is not None and target != current
                    else None
                )
                data["switches"].append(
                    {
                        "planned_s": next_switch * 10,
                        "command_us": cmd,
                        "presented_s": (position - origin) / 1000,
                        "visible_counter_s": state["media_frame"] / FPS,
                        "vlc_media_ms": player.get_time(),
                        "previous_id": current,
                        "target_id": target,
                        "result": result,
                    }
                )
                next_switch += 1
            time.sleep(0.003)
    finally:
        player.stop()
        data.update(
            http_connections=proxy.requests, audio_ids=selection or [], page_selected=selected_page
        )
        proxy.shutdown()
        proxy.server_close()
        player.release()
        instance.release()
        pcm.close()
        stamps.close()
        (out / "receiver.json").write_text(json.dumps(data), encoding="utf-8")
    return data
