#!/usr/bin/env python3
"""Regression tests for the pull request domain selection (scripts/ci-domains.py)."""

import importlib.util
import json
import re
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("ci_domains", ROOT / "scripts/ci-domains.py")
domains = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(domains)

MEMBERS = domains.workspace_members()
SPECS = domains.spec_names()


def select(*paths, importers=lambda path: None):
    return domains.select(list(paths), MEMBERS, SPECS, importers=importers)


class DomainSelection(unittest.TestCase):
    def test_build_and_ci_inputs_select_everything(self):
        for path in ("Cargo.toml", "Cargo.lock", ".github/workflows/ci.yml", "scripts/ci-domains.py",
                     "playwright.config.ts", "crates/ersatzrs-core/src/lib.rs",
                     "crates/ersatzrs-domain/src/teletext/page.rs", "unmapped-root-file.txt",
                     "crates/ersatzrs-unknown/src/lib.rs"):
            with self.subTest(path=path):
                result = select(path)
                self.assertTrue(result["everything"], result)
                self.assertEqual(result["general_specs"], "all")
                self.assertEqual(result["rust_packages"], sorted(MEMBERS))
                self.assertTrue(result["next"] and result["teletext_rust"] and result["teletext_browser"])

    def test_docs_and_other_workflows_select_nothing(self):
        result = select("docs/adr/0001.md", "README.md", ".github/workflows/cleanup-actions.yml",
                        ".github/ISSUE_TEMPLATE/bug.yml")
        self.assertFalse(result["everything"])
        self.assertEqual(result["rust_packages"], [])
        self.assertEqual(result["general_specs"], [])
        self.assertFalse(result["next"] or result["teletext_rust"] or result["teletext_browser"])

    def test_admin_page_selects_its_area_specs_and_the_smoke_set(self):
        result = select("crates/ersatzrs-web/src/admin/pages/libraries/list.rs")
        self.assertEqual(result["rust_packages"], ["ersatzrs-web"])
        self.assertIn("local-library-deletion", result["general_specs"])
        self.assertNotIn("scheduling-route-family", result["general_specs"])
        for smoke in domains.SMOKE_SPECS:
            self.assertIn(smoke, result["general_specs"])
        self.assertFalse(result["next"] or result["teletext_browser"])

    def test_shared_admin_code_selects_every_general_spec(self):
        result = select("crates/ersatzrs-web/src/admin/ui/button.rs")
        self.assertEqual(result["general_specs"], "all")
        self.assertFalse(result["teletext_browser"])

    def test_teletext_and_next_paths_select_their_jobs(self):
        teletext = select("crates/ersatzrs-infra/src/addon_teletext_command.rs")
        self.assertTrue(teletext["teletext_rust"] and teletext["teletext_browser"])
        self.assertFalse(teletext["next"])
        nxt = select("crates/ersatzrs-infra/src/next_engine/coordinator.rs")
        self.assertTrue(nxt["next"])
        self.assertFalse(nxt["teletext_browser"])
        self.assertTrue(select("tests/e2e/next-version.spec.ts")["next"])

    def test_service_crate_files_map_by_keyword_or_select_everything_general(self):
        mapped = select("crates/ersatzrs-infra/src/repositories/media_source.rs")
        self.assertIn("plex-reauthentication", mapped["general_specs"])
        self.assertNotIn("graphics-elements", mapped["general_specs"])
        unmapped = select("crates/ersatzrs-infra/src/db.rs")
        self.assertEqual(unmapped["general_specs"], "all")

    def test_changed_spec_runs_itself_and_teletext_specs_run_their_suite(self):
        result = select("tests/e2e/watermark-preview.spec.ts")
        self.assertEqual(result["rust_packages"], [])
        self.assertIn("watermark-preview", result["general_specs"])
        teletext = select("tests/e2e/teletext-editor-flow.spec.ts")
        self.assertTrue(teletext["teletext_browser"])
        self.assertNotIn("teletext-editor-flow", teletext["general_specs"])

    def test_shared_playwright_module_selects_its_importers(self):
        result = select("tests/e2e/helpers/scanned-library.ts",
                        importers=lambda path: {"local-library-deletion"})
        self.assertIn("local-library-deletion", result["general_specs"])
        self.assertFalse(result["teletext_browser"])
        unknown = select("tests/e2e/hydration.ts", importers=lambda path: None)
        self.assertEqual(select("tests/e2e/fixtures/clip.mp4", importers=lambda path: set())["general_specs"], "all")
        self.assertEqual(unknown["general_specs"], "all")
        self.assertTrue(unknown["teletext_browser"])
        teletext = select("tests/e2e/helpers/teletext-native-editor.ts",
                          importers=lambda path: {"teletext-editor-flow"})
        self.assertTrue(teletext["teletext_browser"])

    def test_importers_reads_real_spec_imports(self):
        users = domains.importers("tests/e2e/helpers/teletext-channel-fixture.ts")
        self.assertTrue(users is None or all(u in SPECS for u in users))

    def test_every_area_mapping_names_existing_specs(self):
        for area, patterns in {**domains.AREA_SPECS, **domains.CRATE_SPECS}.items():
            for pattern in patterns:
                with self.subTest(area=area, pattern=pattern):
                    self.assertTrue(domains.expand((pattern,), SPECS), f"{pattern} matches no spec")
        for _, area in domains.AREA_KEYWORDS:
            self.assertIn(area, domains.AREA_SPECS)
        for crate in (*domains.CRATE_SPECS, *domains.CRATE_ALL_GENERAL, *domains.TELETEXT_RUST_CRATES):
            self.assertIn(crate, MEMBERS)
        for smoke in domains.SMOKE_SPECS:
            self.assertIn(smoke, SPECS)

    def test_filters(self):
        self.assertEqual(domains.nextest_filter([], MEMBERS), "none()")
        self.assertEqual(domains.nextest_filter(sorted(MEMBERS), MEMBERS), "all()")
        self.assertEqual(domains.nextest_filter(["ersatzrs-scanner", "ersatzrs-web"], MEMBERS),
                         "rdeps(ersatzrs-scanner) | rdeps(ersatzrs-web)")
        pattern = re.compile(domains.spec_filter(["collections", "hydration-startup"]))
        self.assertTrue(pattern.search("tests/e2e/collections.spec.ts"))
        self.assertFalse(pattern.search("tests/e2e/collection-detail.spec.ts"))

    def test_command_line_and_workflow_selection_step(self):
        workflow = (ROOT / ".github/workflows/ci.yml").read_text()
        code = re.search(r"python3 - \"\$RUNNER_TEMP/selection.json\" >> \"\$GITHUB_OUTPUT\" <<'PY'\n(.*?)^          PY$",
                         workflow, re.S | re.M)[1]
        code = "\n".join(line[10:] for line in code.splitlines())
        cases = {
            "crates/ersatzrs-web/src/admin/pages/watermarks/edit.rs":
                dict(lane="pr", run_next="false", run_teletext="true", run_browser="true"),
            "docs/readme-only.md": dict(lane="skip", run_browser="false"),
            "Cargo.toml": dict(lane="pr", run_next="true", run_teletext="true", run_browser="true"),
        }
        with tempfile.TemporaryDirectory() as tmp:
            for path, expected in cases.items():
                with self.subTest(path=path):
                    files = Path(tmp, "files.txt")
                    files.write_text(path + "\n")
                    selection = subprocess.run(
                        [sys.executable, str(ROOT / "scripts/ci-domains.py"), "select", str(files)],
                        check=True, text=True, capture_output=True).stdout
                    Path(tmp, "selection.json").write_text(selection)
                    out = subprocess.run([sys.executable, "-c", code, str(Path(tmp, "selection.json"))],
                                         check=True, text=True, capture_output=True).stdout
                    outputs = dict(line.split("=", 1) for line in out.splitlines())
                    for key, value in expected.items():
                        self.assertEqual(outputs[key], value, outputs)
                    matrix = json.loads(outputs["pr_matrix"])["include"]
                    self.assertEqual(bool(matrix), outputs["run_browser"] == "true")
                    self.assertTrue(all(row["project"] == "chromium" for row in matrix))
                    deselected = set(json.loads(outputs["deselected"]))
                    self.assertEqual("next_engine" in deselected, outputs["run_next"] == "false")


if __name__ == "__main__":
    unittest.main()
