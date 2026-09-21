#!/usr/bin/env python3
"""Offline port-isolation regressions; set BUNDLE_TEST_BASH for native Git Bash."""
import os
from pathlib import Path
import socket
import subprocess
import unittest


class BundlePortIsolation(unittest.TestCase):
    def run_gate(self, port):
        env = dict(os.environ, ERSATZRS_VERIFY_PORT=str(port),
                   ERSATZRS_WEB_BIN="missing-candidate-must-not-be-launched")
        script = Path(__file__).with_name("verify-web-bundle.sh")
        return subprocess.run([os.environ.get("BUNDLE_TEST_BASH", "bash"),
                               script.as_posix()], env=env, capture_output=True,
                              text=True, timeout=10)

    def test_occupied_port_is_rejected_before_candidate_lookup(self):
        with socket.socket() as listener:
            listener.bind(("127.0.0.1", 0))
            listener.listen()
            result = self.run_gate(listener.getsockname()[1])
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("already in use", result.stderr)
            self.assertNotIn("server binary not found", result.stderr)
            self.assertGreaterEqual(listener.fileno(), 0)

    def test_invalid_ports_are_rejected(self):
        for port in ("0", "65536", "-1", "abc", "18479/path"):
            with self.subTest(port=port):
                result = self.run_gate(port)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("must be between 1 and 65535", result.stderr)


if __name__ == "__main__":
    unittest.main()
