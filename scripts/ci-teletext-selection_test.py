#!/usr/bin/env python3
"""Guard complete, non-overlapping browser ownership as CI suites evolve."""

import importlib.util
from pathlib import Path
import re
import subprocess
import sys
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


class NextRecovery(unittest.TestCase):
    def test_default_preserves_full_next_filter_and_recovery_names_one_exact_test(self):
        self.assertEqual(selection.next_recovery_filter("all"), selection.rust_filter("next"))
        expected = ("test(=next_engine::process::runtime_tests::subtitle_nul_runtime_tests::"
                    "real_native_next_nul_extraction_and_existing_cache_preserve_selected_captions)")
        self.assertEqual(selection.next_recovery_filter("subtitle-nul"), expected)
        result = subprocess.run([sys.executable, str(ROOT / "scripts/ci-teletext-selection.py"),
                                 "next-recovery", "subtitle-nul"], text=True, capture_output=True, check=True)
        self.assertEqual(result.stdout.strip(), expected)


class BrowserRecovery(unittest.TestCase):
    def test_existing_fixture_selection_is_unchanged(self):
        self.assertEqual(selection.browser_recovery_args("fixtures"), [
            "block-preview.spec.ts", "teletext-authoring-flow.spec.ts", "teletext-editor-flow.spec.ts",
        ])

    def test_preview_selection_matches_only_the_two_complete_case_names(self):
        args = selection.browser_recovery_args("channel-preview")
        self.assertEqual(args[:2], ["channel-watch.spec.ts", "--grep"])
        self.assertEqual(len(args), 3)
        for name in selection.PREVIEW_RECOVERY_CASES:
            self.assertIsNotNone(re.search(args[2], f"chromium channel preview modal and watch route {name}"))
            self.assertIsNone(re.search(args[2], f"{name} unrelated suffix"))
        self.assertIsNone(re.search(args[2], "plays a real Hx264/AAC preview derivative through Hls.js"))
        self.assertIsNone(re.search(args[2], "Next converted captions are authenticated and toggle real browser cues"))

    def test_level15_recovery_reuses_the_complete_existing_suite_filter(self):
        args = selection.browser_recovery_args("teletext-1.5")
        self.assertEqual(args, [selection.browser_filter("1.5")])
        for spec in (ROOT / "tests/e2e").rglob("*.spec.ts"):
            with self.subTest(spec=spec.name):
                self.assertEqual(bool(re.search(args[0], spec.as_posix())),
                                 owners(spec.as_posix()) == ["1.5"])

    def test_runtime_recovery_retains_only_failed_cases_and_interrupted_followers(self):
        args = selection.browser_recovery_args("teletext-runtime")
        self.assertEqual(args[:3], ["channels-source-teletext-delivery.spec.ts",
                                   "teletext-restart-flow.spec.ts", "--grep"])
        self.assertEqual(len(selection.TELETEXT_RUNTIME_RECOVERY_CASES), 11)
        self.assertEqual(len(set(selection.TELETEXT_RUNTIME_RECOVERY_CASES)), 11)
        for name in selection.TELETEXT_RUNTIME_RECOVERY_CASES:
            self.assertIsNotNone(re.search(args[3], f"chromium {name}"))
            self.assertIsNone(re.search(args[3], f"chromium {name} unrelated suffix"))
        for name in (
            "standalone Level 1.5 active Legacy viewers release source processes and leases on shutdown",
            "standalone Level 1.5 active Hybrid viewers release source processes and leases on shutdown",
            "standalone Level 1.5 Lua scopes survive real host restarts and expire to accepted base",
            "per-output teletext delivery status (en-us/ts-legacy/v3)",
            "built-in MPEG-TS choice persists through localized settings and host restart (en-us/no-JS)",
        ):
            self.assertIsNone(re.search(args[3], name))
        for spec in args[:2]:
            self.assertTrue((ROOT / "tests/e2e" / spec).is_file())
            self.assertEqual(owners(spec), ["1.5"])

    def test_cli_refuses_unknown_and_mixed_selections_without_output(self):
        for args in (("next-recovery", "next"), ("next-recovery", "subtitle-nul", "--next-runtime"),
                     ("rust", "subtitle-nul"), ("browser", "subtitle-nul"),
                     ("browser-recovery", "subtitle-nul"), ("next-recovery", "unknown"),
                     ("browser-recovery", "unknown"), ("browser-recovery", "general"),
                     ("rust", "channel-preview"), ("browser", "channel-preview"),
                     ("rust", "teletext-1.5"), ("browser", "teletext-1.5"),
                     ("rust", "teletext-runtime"), ("browser", "teletext-runtime"),
                     ("browser-recovery", "teletext-runtime", "--next-runtime"),
                     ("browser-recovery", "teletext-1.5", "--next-runtime"),
                     ("browser-recovery", "channel-preview", "--next-runtime")):
            with self.subTest(args=args):
                result = subprocess.run([sys.executable, str(ROOT / "scripts/ci-teletext-selection.py"), *args],
                                        text=True, capture_output=True)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(result.stdout, "")


if __name__ == "__main__":
    unittest.main()
