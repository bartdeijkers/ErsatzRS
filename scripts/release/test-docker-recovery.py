#!/usr/bin/env python3
"""Offline regressions for the actual Docker-recovery workflow's CI guard."""

from contextlib import redirect_stdout
import io
import json
import os
from pathlib import Path
import re
import textwrap
import unittest
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[2]
WORKFLOW = (ROOT / ".github/workflows/release-docker-recovery.yml").read_text()
GUARD = textwrap.dedent(re.search(
    r"          python3 - <<'PY'\n(.*?)\n          PY", WORKFLOW, re.S
).group(1))
CI_WORKFLOW = (ROOT / ".github/workflows/ci.yml").read_text()
SOURCE = "a" * 40
TAG = "v1.2.0"
ENDPOINT = "repos/example/ersatzrs/actions/runs/123"
REQUIRED = {
    "Preflight", "Format", "Documentation links", "Cargo deny", "Clippy",
    "Build CI artifacts", "Next engine", "Test", "Teletext 1.5",
    "Playwright E2E (general, chromium)", "Playwright E2E (general, firefox)",
    "Playwright E2E (1.5, chromium)", "Playwright E2E (1.5, firefox)",
}
ENHANCED = {"Teletext 2.5", "Playwright E2E (2.5, chromium)",
            "Playwright E2E (2.5, firefox)"}


def passed_jobs(names):
    return [{"name": name, "status": "completed", "conclusion": "success"}
            for name in sorted(names)]


class DockerRecoveryGuard(unittest.TestCase):
    def setUp(self):
        self.release = {"isDraft": True, "isPrerelease": False}
        self.asset_run = {"head_sha": SOURCE, "head_branch": TAG,
                          "status": "completed", "path": ".github/workflows/release.yml"}
        self.asset_jobs = passed_jobs({"Build web assets"})
        self.ci_runs = [{"databaseId": 456, "headSha": SOURCE, "event": "push",
                         "status": "completed", "conclusion": "success"}]
        self.ci_jobs = passed_jobs(REQUIRED)
        self.ci_jobs.append({"name": "Commit lint", "status": "completed",
                             "conclusion": "skipped"})

    def run_guard(self, ci_workflow=CI_WORKFLOW):
        def check_output(command, **kwargs):
            self.assertEqual(command[0], "gh")
            self.assertTrue(kwargs.get("text"))
            args = command[1:]
            if args[:2] == ["release", "view"]:
                response = self.release
            elif args == ["api", ENDPOINT]:
                response = self.asset_run
            elif args == ["api", ENDPOINT + "/jobs?per_page=100"]:
                response = {"jobs": self.asset_jobs}
            elif args[:2] == ["run", "list"]:
                self.assertEqual(args[args.index("--commit") + 1], SOURCE)
                response = self.ci_runs
            elif args[:3] == ["run", "view", "456"]:
                response = {"jobs": self.ci_jobs}
            else:
                self.fail(f"Unexpected external command: {command}")
            return json.dumps(response)

        output = io.StringIO()
        with patch.dict(os.environ, {"TAG": TAG, "SOURCE_SHA": SOURCE,
                                     "ASSETS_RUN": "123", "GITHUB_REPOSITORY": "example/ersatzrs"}), \
                patch("subprocess.check_output", side_effect=check_output), \
                patch.object(Path, "read_text", return_value=ci_workflow), redirect_stdout(output):
            exec(compile(GUARD, "release-docker-recovery.yml:guard", "exec"), {})
        return output.getvalue()

    def test_current_full_push_accepts_thirteen_jobs_without_optional_enhanced(self):
        self.assertEqual(len(REQUIRED), 13)
        self.assertIn("Accepted source CI: 456", self.run_guard())

    def test_each_required_job_remains_mandatory(self):
        for missing in sorted(REQUIRED):
            with self.subTest(missing=missing):
                self.ci_jobs = passed_jobs(REQUIRED - {missing})
                with self.assertRaisesRegex(SystemExit, "No complete successful source CI"):
                    self.run_guard()

    def test_enhanced_only_dispatch_cannot_replace_full_ci(self):
        self.ci_runs[0]["event"] = "workflow_dispatch"
        self.ci_jobs = passed_jobs(ENHANCED | {"Format", "Commit lint"})
        with self.assertRaisesRegex(SystemExit, "No complete successful source CI"):
            self.run_guard()

    def test_non_push_runs_still_require_commit_lint(self):
        for event in ("pull_request", "workflow_dispatch"):
            with self.subTest(event=event):
                self.ci_runs[0]["event"] = event
                self.ci_jobs = passed_jobs(REQUIRED)
                with self.assertRaisesRegex(SystemExit, "No complete successful source CI"):
                    self.run_guard()
                self.ci_jobs += passed_jobs({"Commit lint"})
                self.assertIn("Accepted source CI: 456", self.run_guard())

    def test_historical_split_contract_still_requires_enhanced_jobs(self):
        historical = "teletext_only:\nname: Preflight\nname: Build CI artifacts\n"
        self.ci_jobs = passed_jobs(REQUIRED | {"Commit lint"})
        with self.assertRaisesRegex(SystemExit, "No complete successful source CI"):
            self.run_guard(historical)
        self.ci_jobs += passed_jobs(ENHANCED)
        self.assertIn("Accepted source CI: 456", self.run_guard(historical))

    def test_historical_two_browser_contract_is_retained(self):
        self.ci_jobs = passed_jobs({"Format", "Clippy", "Test", "Cargo deny",
                                   "Documentation links", "Commit lint", "Next engine",
                                   "Playwright E2E (chromium)", "Playwright E2E (firefox)"})
        self.assertIn("Accepted source CI: 456", self.run_guard("historical unsplit workflow"))

    def test_wrong_ci_source_and_incomplete_or_failed_runs_are_rejected(self):
        for change in ({"headSha": "b" * 40}, {"status": "in_progress"},
                       {"conclusion": "failure"}):
            with self.subTest(change=change):
                original = self.ci_runs[0].copy()
                self.ci_runs[0].update(change)
                with self.assertRaisesRegex(SystemExit, "No complete successful source CI"):
                    self.run_guard()
                self.ci_runs[0] = original

    def test_wrong_original_release_identity_or_live_run_is_rejected(self):
        for change in ({"head_sha": "b" * 40}, {"head_branch": "v1.1.0"},
                       {"path": ".github/workflows/ci.yml"}, {"status": "in_progress"}):
            with self.subTest(change=change):
                original = self.asset_run.copy()
                self.asset_run.update(change)
                with self.assertRaisesRegex(SystemExit, "Web-assets run does not match"):
                    self.run_guard()
                self.asset_run = original

    def test_published_or_prerelease_is_rejected(self):
        for change in ({"isDraft": False}, {"isPrerelease": True}):
            with self.subTest(change=change):
                original = self.release.copy()
                self.release.update(change)
                with self.assertRaisesRegex(SystemExit, "unpublished stable draft"):
                    self.run_guard()
                self.release = original

    def test_matching_web_producer_must_have_completed_successfully(self):
        for change in ({"status": "in_progress"}, {"conclusion": "failure"},
                       {"name": "Unrelated producer"}):
            with self.subTest(change=change):
                original = self.asset_jobs[0].copy()
                self.asset_jobs[0].update(change)
                with self.assertRaisesRegex(SystemExit, "Matching web-assets job did not succeed"):
                    self.run_guard()
                self.asset_jobs[0] = original


if __name__ == "__main__":
    unittest.main()
