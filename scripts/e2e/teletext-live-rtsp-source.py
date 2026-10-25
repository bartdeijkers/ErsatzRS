#!/usr/bin/env python3
"""Opt-in synthetic TS/RTP/RTSP origin; no decoding, rewriting or source EOF.

Requires the qualified GStreamer 1.24.2 RTSP-server/PyGObject test toolchain.
This is a test source, not a product dependency or a teletext display client.
"""

import argparse
import hashlib
import json
from pathlib import Path
import signal
import threading

import gi

gi.require_version("Gst", "1.0")
gi.require_version("GstRtsp", "1.0")
gi.require_version("GstRtspServer", "1.0")
from gi.repository import GLib, Gst, GstRtsp, GstRtspServer

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("source", type=Path)
parser.add_argument("--port", required=True)
parser.add_argument("--status", required=True, type=Path)
args = parser.parse_args()
payload = args.source.read_bytes()
assert 0 < len(payload) <= 4 * 1024 * 1024 and len(payload) % 188 == 0
assert all(payload[index] == 0x47 for index in range(0, len(payload), 188))

Gst.init(None)
assert Gst.version()[:3] == (1, 24, 2), "qualify a different tool version first"
lock = threading.Lock()
status = {
    "ready": False,
    "version": Gst.version_string(),
    "source_sha256": hashlib.sha256(payload).hexdigest(),
    "connections": 0,
    "active_clients": 0,
    "source_sends": 0,
    "active_media": 0,
    "source_eof": False,
    "transport": "TCP",
}


def update(**changes):
    with lock:
        for name, value in changes.items():
            if isinstance(value, bool):
                status[name] = value
            else:
                status[name] += value
        temporary = args.status.with_suffix(".new")
        temporary.write_text(json.dumps(status) + "\n", encoding="utf-8")
        temporary.replace(args.status)


def connected(_server, client):
    update(connections=1, active_clients=1)
    client.connect("closed", lambda _client: update(active_clients=-1))


def configure(_factory, media):
    update(active_media=1)
    active = True

    def unprepared(_media):
        nonlocal active
        if active:
            active = False
            update(active_media=-1)

    media.connect("unprepared", unprepared)
    source = media.get_element().get_by_name("source")
    sent = False

    def needed(source, _length):
        nonlocal sent
        if sent:
            return
        sent = True
        buffer = Gst.Buffer.new_allocate(None, len(payload), None)
        buffer.fill(0, payload)
        buffer.pts = 0
        assert source.emit("push-buffer", buffer) == Gst.FlowReturn.OK
        update(source_sends=1)

    source.connect("need-data", needed)


server = GstRtspServer.RTSPServer.new()
server.set_address("127.0.0.1")
server.set_service(args.port)
server.connect("client-connected", connected)
factory = GstRtspServer.RTSPMediaFactory.new()
# Reuse the independently qualified TCP carrier; inner TS clocks stay intact.
factory.set_protocols(GstRtsp.RTSPLowerTrans.TCP)
factory.set_stop_on_disconnect(True)
factory.set_launch(
    "( appsrc name=source is-live=true format=time block=true "
    "caps=video/mpegts,systemstream=true,packetsize=188 "
    "! rtpmp2tpay name=pay0 pt=33 )"
)
factory.connect("media-configure", configure)
server.get_mount_points().add_factory("/fixture", factory)
assert server.attach(None)
loop = GLib.MainLoop()
signal.signal(signal.SIGTERM, lambda *_: loop.quit())
update(ready=True)
try:
    loop.run()
finally:
    update(ready=False)
