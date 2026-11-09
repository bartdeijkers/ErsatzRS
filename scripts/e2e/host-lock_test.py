#!/usr/bin/env python3
"""Tests for the host-wide browser lock (scripts/e2e/host-lock.py)."""

import importlib.util
import socket
import subprocess
import sys
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().with_name("host-lock.py")
SPEC = importlib.util.spec_from_file_location("host_lock", SCRIPT)
host_lock = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(host_lock)


def free_port():
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        return probe.getsockname()[1]


class HostLock(unittest.TestCase):
    def test_second_holder_waits_and_times_out(self):
        port = free_port()
        first = host_lock.acquire(port, timeout=1)
        try:
            now = [0.0]
            messages = []
            with self.assertRaises(TimeoutError):
                host_lock.acquire(port, timeout=10, poll=5, clock=lambda: now[0],
                                  sleep=lambda s: now.__setitem__(0, now[0] + s),
                                  log=lambda *a, **k: messages.append(a[0]))
            self.assertEqual(len(messages), 1)
        finally:
            first.close()
        # Released: the next holder gets it at once.
        host_lock.acquire(port, timeout=0).close()

    def test_runs_the_command_and_returns_its_exit_code(self):
        port = free_port()
        code = subprocess.call([sys.executable, str(SCRIPT), "--port", str(port), "--",
                                sys.executable, "-c", "import sys; sys.exit(3)"])
        self.assertEqual(code, 3)
        host_lock.acquire(port, timeout=0).close()

    def test_workflow_serializes_every_browser_suite_on_the_host_lock(self):
        workflow = (SCRIPT.parents[2] / ".github/workflows/ci.yml").read_text()
        self.assertIn('python3 scripts/e2e/host-lock.py -- scripts/e2e/playwright-e2e.sh "${playwright_args[@]}"',
                      workflow)
        self.assertNotIn('\n          scripts/e2e/playwright-e2e.sh "${playwright_args[@]}"', workflow)


if __name__ == "__main__":
    unittest.main()
