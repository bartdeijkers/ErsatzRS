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
    def test_reviewed_cross_domain_consumers_are_retained(self):
        for path, required in (
            ("crates/ersatzrs-web/src/admin/server_fns/media_browser/kinds/remote_streams.rs", "remote-streams"),
            ("crates/ersatzrs-web/src/admin/server_fns/ffmpeg_profiles.rs", "profile-deletion"),
            ("crates/ersatzrs-web/src/admin/server_fns/libraries.rs", "windows-release-regressions"),
            ("crates/ersatzrs-web/public/channel-preview.js", "troubleshooting-playback"),
            ("crates/ersatzrs-web/src/transport/iptv/stream.rs", "addon-prepared-playback"),
        ):
            with self.subTest(path=path):
                self.assertTrue((ROOT / path).is_file())
                self.assertIn(required, SPECS)
                selection = domains.select([path])
                self.assertFalse(selection["everything"])
                self.assertIn(required, selection["general_specs"])
                self.assertNotIn("teletext-editor-flow", selection["teletext_specs"])

    def test_audited_domain_behavior(self):
        cases = json.loads((ROOT / "scripts/ci-domain-cases.json").read_text())
        for case in cases:
            with self.subTest(case=case["name"]):
                result = domains.select(case["paths"])
                general = set(SPECS) - set(s for s in SPECS if domains.TELETEXT_PATH.search(s)) \
                    if result["general_specs"] == "all" else set(result["general_specs"])
                selected = general | set(result["teletext_specs"])
                self.assertEqual(bool(result["rust_packages"]), case["rust"], result)
                self.assertEqual(result["teletext_browser"], case["teletext"], result)
                self.assertEqual(result["next"], case["next"], result)
                self.assertTrue(set(case["include"]) <= selected, result)
                self.assertFalse(set(case["exclude"]) & selected, result)
                self.assertTrue(result["reasons"])

    def test_base_teletext_inventory_is_complete_without_promoting_enhanced(self):
        base = {s for s in SPECS if re.search(domains._suites.browser_filter("1.5"), s + ".spec.ts")}
        owned = set().union(*map(set, domains.TELETEXT_SPECS.values()))
        self.assertEqual(owned, base)
        self.assertFalse(owned & set(domains._suites.ENHANCED_SPECS))
        result = domains.select(["crates/ersatzrs-web/src/transport/iptv/teletext_output.rs"],
                                specs=SPECS + ["teletext-new-unowned"])
        self.assertTrue(result["everything"])
        self.assertIn("teletext-new-unowned", result["teletext_specs"])

    def test_union_never_drops_required_checks(self):
        paths = ["crates/ersatzrs-web/src/admin/pages/libraries/libraries.module.css",
                 "crates/ersatzrs-web/src/transport/iptv/teletext_output.rs"]
        merged = domains.select(paths)
        for path in paths:
            single = domains.select([path])
            for key in ("rust_packages", "teletext_specs", "general_specs"):
                self.assertTrue(set(single[key]) <= set(merged[key]), key)
            for key in ("next", "teletext_rust", "teletext_browser"):
                self.assertTrue(not single[key] or merged[key], key)
        self.assertEqual(merged, domains.select(list(reversed(paths)) + paths))

    def test_added_deleted_and_invalid_paths_fail_broad_or_closed(self):
        path = "crates/ersatzrs-web/src/admin/pages/libraries/libraries.module.css"
        self.assertTrue(domains.select([path], added_paths=[path])["everything"])
        self.assertTrue(domains.select(["crates/ersatzrs-web/src/removed.rs", path])["everything"])
        for paths in ([], [""], ["../outside.rs"], ["/outside.rs"], ["crates\\outside.rs"]):
            with self.subTest(paths=paths), self.assertRaises(ValueError):
                domains.select(paths)
        with self.assertRaises(ValueError):
            domains.select([path], added_paths=["unlisted.rs"])

    def test_source_health_is_not_missed_by_teletext_filename(self):
        result = domains.select(["crates/ersatzrs-infra/src/repositories/media_source.rs"])
        self.assertIn("teletext-decoder-health", result["teletext_specs"])
        self.assertNotIn("teletext-editor-flow", result["teletext_specs"])

    def test_embedded_changelog_is_frontend_input_not_a_documentation_skip(self):
        result = domains.select(["CHANGELOG.md"])
        self.assertEqual(result["rust_packages"], [])
        self.assertIn("support-pages", result["general_specs"])
        self.assertFalse(result["teletext_browser"])
        self.assertFalse(domains.documentation_path("CHANGELOG.md"))

    def test_empty_or_unescaped_browser_filter_fails_closed(self):
        for names in ([], [".*"], ["missing.spec.ts"]):
            with self.subTest(names=names), self.assertRaises(ValueError):
                domains.spec_filter(names)

    def test_new_file_metadata_reaches_command_line_selection(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = "crates/ersatzrs-web/src/admin/pages/libraries/libraries.module.css"
            changed, added = Path(tmp, "changed"), Path(tmp, "added")
            changed.write_text(path + "\n")
            added.write_text(path + "\n")
            command = [sys.executable, str(ROOT / "scripts/ci-domains.py"), "select", str(changed)]
            normal = json.loads(subprocess.run(command, check=True, text=True, capture_output=True).stdout)
            new = json.loads(subprocess.run(command + ["--added-files", str(added)],
                                            check=True, text=True, capture_output=True).stdout)
            self.assertEqual(normal["rust_packages"], [])
            self.assertTrue(new["everything"])

    def test_presentation_wrapper_invalidates_on_added_native_behavior(self):
        path = "crates/ersatzrs-web/src/admin/pages/media/images.rs"
        source = (ROOT / path).read_text()
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            target = root / path
            target.parent.mkdir(parents=True)
            target.write_text(source)
            self.assertEqual(domains.select([path], root=root)["rust_packages"], [])
            for addition in ("\n#[test] fn regression() {}", "\nuse sqlx::Row;",
                             "\n#[server] pub async fn mutate() {}", "\nmod native;"):
                target.write_text(source + addition)
                self.assertEqual(domains.select([path], root=root)["rust_packages"], ["ersatzrs-web"])

    def test_transitive_helper_imports_cycles_and_dynamic_consumers(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            e2e = root / "tests/e2e"
            (e2e / "helpers").mkdir(parents=True)
            (e2e / "helpers/data.ts").write_text("export const x = 1;")
            (e2e / "helpers/wrapper.ts").write_text("export {x} from './data';")
            (e2e / "selected.spec.ts").write_text("import {x} from './helpers/wrapper';")
            (e2e / "unrelated.spec.ts").write_text("import {test} from '@playwright/test';")
            path = "tests/e2e/helpers/data.ts"
            self.assertEqual(domains.importers(path, root), {"selected"})
            (e2e / "helpers/data.ts").write_text("import './wrapper'; export const x = 1;")
            self.assertEqual(domains.importers(path, root), {"selected"})
            (e2e / "dynamic.spec.ts").write_text("await import(variable);")
            self.assertEqual(domains.importers(path, root), {"selected", "dynamic"})
            (e2e / "broken.spec.ts").write_text("import './missing';")
            self.assertIsNone(domains.importers(path, root))

    def test_audited_domain_case_inventory(self):
        cases = json.loads((ROOT / "scripts/ci-domain-cases.json").read_text())
        self.assertEqual(len({case["name"] for case in cases}), len(cases))
        for case in cases:
            with self.subTest(case=case["name"]):
                for path in case["paths"]:
                    self.assertTrue((ROOT / path).is_file(), path)
                for spec in case["include"] + case["exclude"]:
                    self.assertIn(spec, SPECS)
                self.assertFalse(set(case["include"]) & set(case["exclude"]))

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
        result = select("crates/ersatzrs-web/src/admin/pages/libraries/mod.rs")
        self.assertEqual(result["rust_packages"], ["ersatzrs-web"])
        self.assertIn("local-library-deletion", result["general_specs"])
        self.assertNotIn("scheduling-route-family", result["general_specs"])
        for smoke in domains.SMOKE_SPECS:
            self.assertIn(smoke, result["general_specs"])
        self.assertFalse(result["next"] or result["teletext_browser"])

    def test_shared_admin_code_selects_every_general_spec(self):
        result = select("crates/ersatzrs-web/src/admin/ui/primitives/button.rs")
        self.assertEqual(result["general_specs"], "all")
        self.assertTrue(result["teletext_browser"])

    def test_teletext_and_next_paths_select_their_jobs(self):
        teletext = select("crates/ersatzrs-infra/src/addon_teletext_command.rs")
        self.assertTrue(teletext["teletext_rust"] and teletext["teletext_browser"])
        self.assertFalse(teletext["next"])
        nxt = select("crates/ersatzrs-infra/src/next_engine/coordinator.rs")
        self.assertTrue(nxt["next"])
        self.assertTrue(nxt["teletext_browser"])
        self.assertFalse(select("tests/e2e/next-version.spec.ts")["next"])

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
        self.assertFalse(teletext["teletext_rust"])
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
        for areas in (*domains.SERVER_AREAS.values(), *domains.REPOSITORY_AREAS.values()):
            for area in areas:
                self.assertIn(area, domains.AREA_SPECS)
        for crate in domains.CRATE_SPECS:
            self.assertIn(crate, MEMBERS)
        for smoke in domains.SMOKE_SPECS:
            self.assertIn(smoke, SPECS)

    def test_archive_holds_what_the_selected_jobs_run(self):
        self.assertEqual(domains.archive_packages(select("Cargo.toml")), [])
        self.assertEqual(domains.archive_packages(select("tests/e2e/watermark-preview.spec.ts")), [])
        scanner = select("crates/ersatzrs-scanner/src/lib.rs")
        self.assertEqual(domains.archive_packages(scanner), ["ersatzrs-infra", "ersatzrs-scanner", "ersatzrs-web"])
        nxt = select("crates/ersatzrs-infra/src/next_engine/coordinator.rs")
        self.assertEqual(domains.archive_packages(nxt), ["ersatzrs-infra", "ersatzrs-web"])

    def test_build_products_follow_actual_consumers(self):
        for path, expected in (
            ("tests/e2e/watermark-preview.spec.ts", (False, True, False)),
            ("crates/ersatzrs-web/src/admin/pages/libraries/libraries.module.css", (False, True, False)),
            ("xtask/src/affected_checks.rs", (True, False, False)),
            ("crates/ersatzrs-web/src/admin/pages/libraries/mod.rs", (True, True, False)),
            ("crates/ersatzrs-infra/src/next_engine/coordinator.rs", (True, True, True)),
            ("Cargo.toml", (True, True, True)),
            ("plans/INDEX.md", (False, False, False)),
        ):
            with self.subTest(path=path):
                actual = domains.build_products(select(path))
                self.assertEqual(actual, dict(zip(("rust", "web", "smoke_server"), expected)))

    def test_ci_runtime_commands_and_developer_pages_cannot_select_nothing(self):
        for path in ("xtask/src/main.rs", "xtask/src/smoke.rs", "xtask/src/smoke_next.rs"):
            self.assertTrue(select(path)["everything"])
        self.assertEqual(set(domains.AREA_SPECS["dev"]), set(domains.SMOKE_SPECS))

    def test_reverse_closure_follows_workspace_dependents(self):
        metadata = {
            "workspace_members": ["a", "b", "c", "d"],
            "packages": [
                {"id": "a", "name": "core", "dependencies": []},
                {"id": "b", "name": "infra", "dependencies": [{"name": "core"}]},
                {"id": "c", "name": "web", "dependencies": [{"name": "infra"}, {"name": "serde"}]},
                {"id": "d", "name": "scanner", "dependencies": [{"name": "core"}]},
            ],
        }
        self.assertEqual(domains.reverse_closure(["infra"], metadata), ["infra", "web"])
        self.assertEqual(domains.reverse_closure(["core"], metadata), ["core", "infra", "scanner", "web"])
        self.assertEqual(domains.reverse_closure(["web"], metadata), ["web"])

    def test_archive_target_kinds_preserve_full_coverage_and_binary_only_crates(self):
        metadata = {"workspace_members": ["a", "b"], "packages": [
            {"id": "a", "name": "tool", "targets": [{"kind": ["bin"]}, {"kind": ["test"]}]},
            {"id": "b", "name": "library", "targets": [{"kind": ["rlib", "cdylib"]}]},
        ]}
        self.assertEqual(domains.archive_target_flags([], metadata), ["--lib", "--bins", "--tests"])
        self.assertEqual(domains.archive_target_flags(["tool"], metadata), ["--bins", "--tests"])
        self.assertEqual(domains.archive_target_flags(["library"], metadata), ["--lib", "--tests"])
        with self.assertRaises(ValueError):
            domains.archive_target_flags(["absent"], metadata)

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
            "crates/ersatzrs-web/src/admin/pages/watermarks/list.rs":
                dict(lane="pr", run_next="false", run_teletext="false", run_browser="true"),
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
