#!/usr/bin/env python3
"""Guard complete, non-overlapping browser ownership as CI suites evolve."""

import importlib.util
from pathlib import Path
import re
import unittest


ROOT = Path(__file__).resolve().parent.parent
SPEC = importlib.util.spec_from_file_location(
    "selection", ROOT / "scripts/ci-teletext-selection.py"
)
selection = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(selection)


def owners(path):
    return [suite for suite in ("general", "1.5", "2.5")
            if re.search(selection.browser_filter(suite), path)]


class BrowserOwnership(unittest.TestCase):
    def test_every_current_spec_has_exactly_one_owner(self):
        specs = list((ROOT / "tests/e2e").rglob("*.spec.ts"))
        self.assertTrue(specs)
        counts = {suite: 0 for suite in ("general", "1.5", "2.5")}
        for spec in specs:
            with self.subTest(spec=spec.name):
                selected = owners(spec.as_posix())
                self.assertEqual(len(selected), 1)
                counts[selected[0]] += 1
        self.assertTrue(all(counts.values()), counts)

    def test_new_shared_teletext_specs_cannot_leak_to_general(self):
        self.assertEqual(owners("tests/e2e/teletext-new-flow.spec.ts"), ["1.5"])
        self.assertEqual(owners("tests/e2e/channels-source-teletext.spec.ts"), ["1.5"])
        self.assertEqual(owners("tests/e2e/channels.spec.ts"), ["general"])

    def test_declared_enhanced_specs_exist_and_remain_exclusive(self):
        for name in selection.ENHANCED_SPECS:
            path = ROOT / "tests/e2e" / f"{name}.spec.ts"
            with self.subTest(spec=name):
                self.assertTrue(path.is_file())
                self.assertEqual(owners(path.as_posix()), ["2.5"])


if __name__ == "__main__":
    unittest.main()
