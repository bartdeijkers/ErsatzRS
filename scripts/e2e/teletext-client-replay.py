"""Bounded byte-identical loopback replay for an opt-in headed client trial.

No remuxing, TS loops, range seeking, publication or runtime configuration.
Pacing leaves time to select a teletext track and navigate before finite EOF.
"""

import argparse
import hashlib
import http.server
import json
from pathlib import Path
import socket
import threading
import time

MAX_BYTES = 8 * 1024 * 1024
PACKET = 188
CHUNK = PACKET * 32


def capture(path):
    # Bound the read itself, including files that grow after stat.
    with path.open("rb") as source:
        body = source.read(MAX_BYTES + 1)
    if not body or len(body) > MAX_BYTES or len(body) % PACKET:
        raise ValueError("capture must contain complete TS packets within 8 MiB")
    if any(body[offset] != 0x47 for offset in range(0, len(body), PACKET)):
        raise ValueError("capture has an invalid TS sync byte")
    return body


def serve(body, muxrate, rate):
    duration = len(body) * 8 / (muxrate * rate)
    if duration > 60:
        raise ValueError("paced capture exceeds the 60-second delivery bound")

    class Server(http.server.HTTPServer):
        def get_request(self):
            connection, address = super().get_request()
            # Also bound request-header reads before do_GET is dispatched.
            connection.settimeout(5)
            return connection, address

    class Handler(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_args):
            pass

        def handle(self):
            def close_headers():
                try:
                    self.connection.shutdown(socket.SHUT_RDWR)
                except OSError:
                    pass

            # Socket timeout alone is inactivity-only: trickled headers must
            # also finish within an absolute five seconds after acceptance.
            self.header_timer = threading.Timer(5, close_headers)
            self.header_timer.daemon = True
            self.header_timer.start()
            try:
                super().handle()
            finally:
                self.header_timer.cancel()
                self.header_timer.join()

        def do_GET(self):
            self.header_timer.cancel()
            self.header_timer.join()
            self.connection.settimeout(5)
            if self.path != "/synthetic.ts":
                self.send_error(404)
                return
            self.send_response(200)
            self.send_header("Content-Type", "video/mp2t")
            self.send_header("Accept-Ranges", "none")
            self.send_header("Connection", "close")
            # No Content-Length: consume sequentially instead of probing the tail.
            self.end_headers()
            start = time.monotonic()
            delivered = 0
            try:
                for offset in range(0, len(body), CHUNK):
                    remaining = start + 60 - time.monotonic()
                    if remaining <= 0:
                        raise TimeoutError("delivery deadline")
                    self.connection.settimeout(min(5, remaining))
                    chunk = body[offset : offset + CHUNK]
                    self.wfile.write(chunk)
                    self.wfile.flush()
                    delivered += len(chunk)
                    delay = start + delivered * 8 / (muxrate * rate) - time.monotonic()
                    if delay > 0:
                        time.sleep(delay)
            except (BrokenPipeError, ConnectionResetError, TimeoutError):
                pass
            print(json.dumps({"delivered_bytes": delivered, "complete": delivered == len(body)}), flush=True)

    with Server(("127.0.0.1", 0), Handler) as server:
        server.timeout = 120
        print(json.dumps({
            "url": f"http://127.0.0.1:{server.server_port}/synthetic.ts",
            "sha256": hashlib.sha256(body).hexdigest(),
            "bytes": len(body),
            "delivery_seconds": duration,
        }), flush=True)
        # One request, no request worker, close after EOF/disconnect/timeout.
        server.handle_request()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("capture", type=Path)
    parser.add_argument("--muxrate", type=int, required=True, help="verified source bits/second")
    parser.add_argument("--rate", type=float, default=0.25, help="client replay speed (0.25 to 1)")
    args = parser.parse_args()
    if args.muxrate <= 0 or not 0.25 <= args.rate <= 1:
        parser.error("positive muxrate and a rate from 0.25 to 1 are required")
    serve(capture(args.capture), args.muxrate, args.rate)


if __name__ == "__main__":
    main()
