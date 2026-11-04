#!/usr/bin/env python3
"""Run a command while holding a host-wide lock on a loopback TCP port.

Both self-hosted runners share one machine (and, under WSL2, one network
namespace even across distributions), but not one filesystem. Two Playwright
suites running at once on that machine compete for CPU and memory, which made
results unreliable, and their fixed E2E ports collide. Binding a fixed loopback
port is the one lock both runners can see: whoever holds the bind runs, the
other waits until it is released (also when the holder dies, because the
kernel frees the socket).

Usage: host-lock.py [--port N] [--timeout SECONDS] -- command [args...]
"""

import argparse
import socket
import subprocess
import sys
import time

DEFAULT_PORT = 18399


def acquire(port, timeout, poll=5.0, clock=time.monotonic, sleep=time.sleep, log=print):
    deadline = clock() + timeout
    announced = False
    while True:
        lock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        try:
            # No SO_REUSEADDR: a second bind must fail while the lock is held.
            lock.bind(("127.0.0.1", port))
            lock.listen(1)
            return lock
        except OSError:
            lock.close()
            if clock() >= deadline:
                raise TimeoutError(f"host lock 127.0.0.1:{port} still held after {timeout:.0f}s")
            if not announced:
                log(f"Waiting for the host browser lock 127.0.0.1:{port}: another suite runs on this machine.",
                    flush=True)
                announced = True
            sleep(poll)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--port", type=int, default=DEFAULT_PORT)
    parser.add_argument("--timeout", type=float, default=4 * 3600)
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args(argv)
    command = args.command[1:] if args.command[:1] == ["--"] else args.command
    if not command:
        parser.error("a command is required after --")
    started = time.monotonic()
    lock = acquire(args.port, args.timeout)
    waited = time.monotonic() - started
    print(f"Holding the host browser lock after waiting {waited:.0f}s.", flush=True)
    try:
        return subprocess.call(command)
    finally:
        lock.close()


if __name__ == "__main__":
    sys.exit(main())
