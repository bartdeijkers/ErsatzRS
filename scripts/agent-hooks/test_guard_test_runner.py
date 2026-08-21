#!/usr/bin/env python3
"""Regression tests for the agent test-runner command guard."""

from __future__ import annotations

import json
import unittest
from pathlib import Path

from guard_test_runner import contains_direct_cargo_test


class GuardTestRunnerTests(unittest.TestCase):
    def test_agent_hook_configs_wire_the_guard(self) -> None:
        root = Path(__file__).resolve().parents[2]
        for relative in (Path(".codex/hooks.json"), Path(".claude/settings.json")):
            config = json.loads((root / relative).read_text(encoding="utf-8"))
            commands = [
                hook["command"]
                for group in config["hooks"]["PreToolUse"]
                for hook in group["hooks"]
            ]
            self.assertTrue(
                any("guard_test_runner.py" in command for command in commands),
                f"{relative} must wire guard_test_runner.py",
            )

    def test_codex_hooks_use_sandbox_safe_relative_paths(self) -> None:
        root = Path(__file__).resolve().parents[2]
        config = json.loads((root / ".codex/hooks.json").read_text(encoding="utf-8"))
        commands = [
            hook["command"]
            for phase in ("PreToolUse", "PostToolUse")
            for group in config["hooks"][phase]
            for hook in group["hooks"]
        ]
        for command in commands:
            self.assertTrue(command.startswith('python "scripts/agent-hooks/'))
            self.assertNotIn("git rev-parse", command)
            self.assertNotIn("python3", command)

    def test_blocks_direct_focused_cargo_test(self) -> None:
        self.assertTrue(contains_direct_cargo_test("cargo test -p ersatzrs-core filter"))

    def test_blocks_toolchain_and_environment_wrappers(self) -> None:
        self.assertTrue(
            contains_direct_cargo_test("SQLX_OFFLINE=true cargo +stable test -p app")
        )

    def test_blocks_nested_shell_command(self) -> None:
        self.assertTrue(contains_direct_cargo_test("bash -lc 'cargo test --workspace'"))

    def test_blocks_multiline_and_common_command_wrappers(self) -> None:
        self.assertTrue(contains_direct_cargo_test("echo ready\ncargo test -p app"))
        self.assertTrue(contains_direct_cargo_test("timeout 30 cargo test -p app"))
        self.assertTrue(contains_direct_cargo_test("cargo --locked test -p app"))

    def test_blocks_active_command_substitutions(self) -> None:
        self.assertTrue(contains_direct_cargo_test('rg -n "`cargo test -p app`" .'))
        self.assertTrue(contains_direct_cargo_test("echo $(cargo test -p app)"))

    def test_allows_nextest_wrapper(self) -> None:
        self.assertFalse(
            contains_direct_cargo_test("./run.sh test -p ersatzrs-core filter")
        )

    def test_allows_closeout_wrapper(self) -> None:
        self.assertFalse(contains_direct_cargo_test("./run.sh test-closeout"))

    def test_allows_searching_for_documented_command(self) -> None:
        self.assertFalse(contains_direct_cargo_test("rg -n 'cargo test' AGENTS.md"))
        self.assertFalse(contains_direct_cargo_test("rg -n '`cargo test`' AGENTS.md"))


if __name__ == "__main__":
    unittest.main()
