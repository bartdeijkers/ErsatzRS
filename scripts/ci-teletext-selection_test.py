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


class RustOwnership(unittest.TestCase):
    @staticmethod
    def selected(suite, name, binary, next_runtime=False):
        # Evaluate the actual emitted nextest predicates on real and intentionally synthetic shared
        # identities, without compiling or executing those Rust test bodies.
        expression = selection.rust_filter(suite, next_runtime=next_runtime)
        expression = re.sub(r"(test|binary)\(/(.*?)/\)", lambda m: str(bool(
            re.search(m[2], name if m[1] == "test" else binary))), expression)
        expression = re.sub(r"(test|binary)\(([^()]*)\)", lambda m: str(
            (name if m[1] == "test" else binary) == m[2][1:] if m[2].startswith("=")
            else m[2] in (name if m[1] == "test" else binary)), expression)
        return bool(eval(expression.replace("&", " and ").replace("|", " or "),
                         {"__builtins__": {}}, {}))

    def test_native_shared_and_standalone_checks_remain_required(self):
        cases = (
            ("native_profile_preparation_keeps_simultaneous_channels_isolated", "teletext_profile_preparation"),
            ("client_capture::installed_originals_pending_request_capture_actual_direct_mpegts_for_client_trial", "teletext_profile_preparation"),
            ("teletext::authored_service_tests::standalone_base::standalone_spacing_and_level15_x26_compile_without_optional_resource_banks", "ersatzrs_ffmpeg"),
            ("teletext::authored_service_tests::standalone_base::standalone_conceal_reveal_and_flash_phases_preserve_essential_base_pixels", "ersatzrs_ffmpeg"),
            ("teletext::authored_service_tests::new_shared_base_guard", "ersatzrs_ffmpeg"),
        )
        for name, binary in cases:
            with self.subTest(name=name):
                self.assertTrue(self.selected("1.5", name, binary))
                self.assertFalse(self.selected("2.5", name, binary))
                self.assertFalse(self.selected("general", name, binary))
                self.assertTrue(self.selected("all", name, binary))

    def test_named_enhanced_families_stay_optional_and_disjoint(self):
        for name, binary in (
            ("editorial_imports::resource_interchange::original_pop_fragments_import_publish_repeat_undo_reopen_preserves_all_consumers", "teletext_profile_preparation"),
            ("teletext::authored_service_tests::drcs::normal_table", "ersatzrs_ffmpeg"),
            ("teletext::authored_service_tests::physical_spacing::original_width", "ersatzrs_ffmpeg"),
        ):
            with self.subTest(name=name):
                self.assertFalse(self.selected("1.5", name, binary))
                self.assertTrue(self.selected("2.5", name, binary))
                self.assertFalse(self.selected("general", name, binary))
                self.assertTrue(self.selected("all", name, binary))

    def test_ordinary_and_next_runtime_ownership_is_preserved(self):
        self.assertTrue(self.selected("general", "media_is_unchanged", "ersatzrs_web"))
        name = "next_engine::process::runtime_tests::teletext_caption_output"
        self.assertFalse(self.selected("1.5", name, "ersatzrs_infra"))
        self.assertTrue(self.selected("1.5", name, "ersatzrs_infra", next_runtime=True))
        self.assertFalse(self.selected("next", name, "ersatzrs_infra"))


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
