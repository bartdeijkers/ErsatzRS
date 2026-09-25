#!/usr/bin/env python3
"""Regression tests for the diff-based Clippy allow policy."""

from __future__ import annotations

import importlib.util
import os
import subprocess
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("check_new_clippy_allows.py")
SPEC = importlib.util.spec_from_file_location("check_new_clippy_allows", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
POLICY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(POLICY)


class ClippyAllowPolicyTests(unittest.TestCase):
    def test_existing_allow_and_unrelated_edit_do_not_fail(self) -> None:
        source = "#[allow(clippy::too_many_lines)]\nfn old() {}\nfn changed() {}\n"
        self.assertEqual(POLICY.new_allow_violations(source, {3}), [])

    def test_new_unexplained_allow_fails(self) -> None:
        source = "#[allow(clippy::too_many_lines)]\nfn new() {}\n"
        self.assertEqual(
            POLICY.new_allow_violations(source, {1, 2}),
            [(1, "add a reason=... argument or an inline // reason")],
        )

    def test_item_allow_accepts_inline_or_attribute_reason(self) -> None:
        source = (
            "#[allow(clippy::too_many_lines)] // Keep one state machine visible.\n"
            "fn first() {}\n"
            '#[allow(clippy::too_many_arguments, reason = "Existing wire contract")]\n'
            "fn second() {}\n"
        )
        self.assertEqual(POLICY.new_allow_violations(source, {1, 3}), [])

    def test_module_allow_fails_even_with_reason(self) -> None:
        source = '#![allow(clippy::too_many_lines, reason = "Convenient")]\n'
        self.assertEqual(
            POLICY.new_allow_violations(source, {1}),
            [(1, "use an item-level allow instead of a file-wide allow")],
        )

    def test_expect_and_broad_lint_groups_follow_the_policy(self) -> None:
        source = (
            "#[expect(clippy::too_many_lines)]\n"
            "fn first() {}\n"
            '#[allow(clippy::all, reason = "Too broad")]\n'
            "fn second() {}\n"
            "#[allow(warnings)]\n"
            "fn third() {}\n"
        )
        self.assertEqual(
            POLICY.new_allow_violations(source, {1, 3, 5}),
            [
                (1, "add a reason=... argument or an inline // reason"),
                (3, "name specific Clippy lint(s), not a lint group or warnings"),
                (5, "name specific Clippy lint(s), not a lint group or warnings"),
            ],
        )

    def test_reason_text_does_not_count_as_a_lint(self) -> None:
        source = '#[allow(dead_code, reason = "The word clippy::all is explanatory")]\n'
        self.assertEqual(POLICY.new_allow_violations(source, {1}), [])

    def test_multiline_allow_checks_changed_lint_line(self) -> None:
        source = (
            "#[allow(\n"
            "    dead_code,\n"
            "    clippy::too_many_lines,\n"
            ")]\n"
            "fn example() {}\n"
        )
        self.assertEqual(
            POLICY.new_allow_violations(source, {3}),
            [(1, "add a reason=... argument or an inline // reason")],
        )

    def test_git_diff_detects_new_allow_and_ignores_existing(self) -> None:
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            subprocess.run(["git", "init", "-q"], cwd=root, check=True)
            subprocess.run(["git", "config", "user.email", "test@example.invalid"], cwd=root, check=True)
            subprocess.run(["git", "config", "user.name", "Test"], cwd=root, check=True)
            source = root / "example.rs"
            source.write_text("#[allow(clippy::old)]\nfn old() {}\nfn tail() {}\n")
            subprocess.run(["git", "add", "example.rs"], cwd=root, check=True)
            subprocess.run(["git", "commit", "-qm", "baseline"], cwd=root, check=True)
            source.write_text(
                "#[allow(clippy::old)]\n"
                "fn old() {}\n"
                "#[allow(clippy::new)]\n"
                "fn added() {}\n"
                "fn tail() {}\n"
            )
            original = Path.cwd()
            try:
                os.chdir(root)
                self.assertEqual(
                    POLICY.check("HEAD"),
                    ["example.rs:3: add a reason=... argument or an inline // reason"],
                )
            finally:
                os.chdir(original)


if __name__ == "__main__":
    unittest.main()
