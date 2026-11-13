#!/usr/bin/env python3
"""Tests for the host-wide browser lock (scripts/e2e/host-lock.py)."""

import importlib.util
import re
import socket
import subprocess
import sys
import unittest
from pathlib import Path
from types import SimpleNamespace

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

    def test_scheduler_queue_keeps_waiting_suites_off_runners_without_replacement(self):
        workflow = (SCRIPT.parents[2] / ".github/workflows/ci.yml").read_text()
        browser = workflow.split("  hydration:\n", 1)[1].split("    strategy:\n", 1)[0]
        group = re.search(r"^      group: (.+)$", browser, re.M)[1]
        self.assertRegex(browser, r"(?m)^      queue: max$")
        self.assertRegex(browser, r"(?m)^      cancel-in-progress: false$")
        prefix, expression = group.split("${{", 1)
        expression = expression.removesuffix("}}").strip().replace("&&", " and ").replace("||", " or ")

        def key(runner="", pool="", run=1, attempt=1, port="18409", workflow="PR checks", ref="topic"):
            return prefix + str(eval(expression, {"__builtins__": {}}, {
                "inputs": SimpleNamespace(runner=runner),
                "needs": SimpleNamespace(preflight=SimpleNamespace(outputs=SimpleNamespace(pool=pool))),
                "github": SimpleNamespace(run_id=run, run_attempt=attempt, workflow=workflow, ref=ref),
                "matrix": SimpleNamespace(port=port), "format": lambda text, *args: text.format(*args),
            }))

        # Different branches, callers and matrix entries on the shared host
        # must contend at the scheduler, before acquiring a worker or TCP lock.
        shared = key()
        for runner in ("", "self-hosted"):
            for run, workflow in ((1, "PR checks"), (2, "Full verification"), (3, "CI (Browser recovery)")):
                for port in ("18409", "18412"):
                    self.assertEqual(key(runner=runner, run=run, workflow=workflow, ref=str(run), port=port), shared)
        # Explicit hosted dispatch and automatic fallback are both isolated.
        for runner, pool in (("github-hosted", ""), ("", "github-hosted")):
            hosted = {key(runner=runner, pool=pool, run=run, attempt=attempt, port=port)
                      for run in (1, 2) for attempt in (1, 2) for port in ("18409", "18412")}
            self.assertEqual(len(hosted), 8)
            self.assertNotIn(shared, hosted)

    def test_workflow_serializes_every_browser_suite_on_the_host_lock(self):
        workflow = (SCRIPT.parents[2] / ".github/workflows/ci.yml").read_text()
        self.assertIn('python3 scripts/e2e/host-lock.py -- scripts/e2e/playwright-e2e.sh "${playwright_args[@]}"',
                      workflow)
        self.assertNotIn('\n          scripts/e2e/playwright-e2e.sh "${playwright_args[@]}"', workflow)


if __name__ == "__main__":
    unittest.main()
