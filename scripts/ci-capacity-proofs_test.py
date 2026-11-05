#!/usr/bin/env python3
"""Regression tests for the capacity-proof gate (scripts/ci-capacity-proofs.py)."""

import importlib.util
import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("ci_capacity", ROOT / "scripts/ci-capacity-proofs.py")
capacity = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(capacity)


class CapacityProofs(unittest.TestCase):
    def test_four_cpus_skip_and_more_run(self):
        self.assertFalse(capacity.capable(4))
        self.assertTrue(capacity.capable(5))
        self.assertTrue(capacity.capable(24))

    def test_every_listed_case_exists(self):
        sources = "\n".join(path.read_text() for path in (ROOT / "crates/ersatzrs-infra/src").rglob("*.rs"))
        for name in capacity.NEXT_TESTS:
            with self.subTest(name=name):
                self.assertIn(f"fn {name.rsplit('::', 1)[1]}(", sources)
        specs = "\n".join(path.read_text() for path in (ROOT / "tests/e2e").glob("*.spec.ts"))
        for title in capacity.BROWSER_TITLES:
            with self.subTest(title=title):
                self.assertIn(f"test('{title}'", specs)

    def test_filters_name_exactly_the_listed_cases(self):
        self.assertEqual(capacity.nextest_exclude(),
                         "not (" + " | ".join(f"test(={n})" for n in capacity.NEXT_TESTS) + ")")
        pattern = re.compile(capacity.browser_grep())
        for title in capacity.BROWSER_TITLES:
            self.assertTrue(pattern.search(f"channel-watch.spec.ts {title}"))
        self.assertFalse(pattern.search("finding 09 decodes compact PNG in a full native broadcast graph"))

    def test_report_names_the_skipped_cases_and_cpu_count(self):
        text = capacity.report("next", cpus=4)
        self.assertIn("4 CPUs", text)
        for name in capacity.NEXT_TESTS:
            self.assertIn(name, text)
        self.assertIn(capacity.BROWSER_TITLES[0], capacity.report("browser", cpus=4))

    def test_workflow_applies_the_gate_only_when_incapable(self):
        workflow = (ROOT / ".github/workflows/ci.yml").read_text()
        next_step = workflow.split("name: Next components, real-engine decoding and HTTP EOF", 1)[1].split("\n      - name:", 1)[0]
        self.assertIn("if ! python3 scripts/ci-capacity-proofs.py capable; then", next_step)
        self.assertIn('filter="($filter) & $(python3 scripts/ci-capacity-proofs.py nextest-exclude)"', next_step)
        self.assertIn('-E "$filter"', next_step)
        browser_step = workflow.split("name: Playwright E2E suite or bounded fixture recovery", 1)[1].split("\n      - name:", 1)[0]
        self.assertIn("! python3 scripts/ci-capacity-proofs.py capable", browser_step)
        self.assertIn('--grep-invert "$(python3 scripts/ci-capacity-proofs.py browser-grep)"', browser_step)
        self.assertLess(browser_step.index("--grep-invert"), browser_step.index("host-lock.py"))


if __name__ == "__main__":
    unittest.main()
