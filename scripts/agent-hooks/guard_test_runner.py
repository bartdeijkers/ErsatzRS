#!/usr/bin/env python3
"""Block direct agent cargo-test commands in favor of repository wrappers."""

from __future__ import annotations

import json
import os
import shlex
import sys
from typing import Any


COMMAND_KEYS = {"cmd", "command"}
SEPARATORS = {";", "&&", "||", "|", "&", "(", ")"}
ASSIGNMENT_PREFIXES = ("CARGO_", "RUST", "SQLX_", "LD_", "ERSATZRS_")


def command_strings(value: Any, parent_key: str = ""):
    """Yield shell commands from Codex and Claude hook payloads."""
    if isinstance(value, str):
        if parent_key in COMMAND_KEYS:
            yield value
        return
    if isinstance(value, list):
        for item in value:
            yield from command_strings(item, parent_key)
        return
    if isinstance(value, dict):
        for key, item in value.items():
            yield from command_strings(item, str(key))


def tokenize(command: str) -> list[str]:
    command = command.replace("\\\n", " ").replace("\n", " ; ")
    try:
        lexer = shlex.shlex(command, posix=True, punctuation_chars=";&|()")
        lexer.whitespace_split = True
        lexer.commenters = ""
        return list(lexer)
    except ValueError:
        # A malformed shell command will fail on its own. Do not turn this
        # policy guard into a generic shell parser.
        return []


def command_segments(tokens: list[str]) -> list[list[str]]:
    segments: list[list[str]] = []
    current: list[str] = []
    for token in tokens:
        if token in SEPARATORS or set(token).issubset({"&", "|", ";"}):
            if current:
                segments.append(current)
                current = []
        else:
            current.append(token)
    if current:
        segments.append(current)
    return segments


def active_subcommands(command: str):
    """Yield shell substitutions that execute outside single-quoted text."""
    index = 0
    quote: str | None = None
    while index < len(command):
        char = command[index]
        if char == "\\":
            index += 2
            continue
        if quote == "'":
            if char == "'":
                quote = None
            index += 1
            continue
        if char == "'":
            quote = "'"
            index += 1
            continue
        if char == '"':
            quote = None if quote == '"' else '"'
            index += 1
            continue
        if char == "`":
            end = index + 1
            while end < len(command):
                if command[end] == "\\":
                    end += 2
                    continue
                if command[end] == "`":
                    yield command[index + 1 : end]
                    index = end + 1
                    break
                end += 1
            else:
                return
            continue
        if char == "$" and index + 1 < len(command) and command[index + 1] == "(":
            depth = 1
            end = index + 2
            nested_quote: str | None = None
            while end < len(command) and depth:
                nested = command[end]
                if nested == "\\":
                    end += 2
                    continue
                if nested_quote == "'":
                    if nested == "'":
                        nested_quote = None
                    end += 1
                    continue
                if nested == "'":
                    nested_quote = "'"
                elif nested == '"':
                    nested_quote = None if nested_quote == '"' else '"'
                elif nested_quote is None and nested == "(":
                    depth += 1
                elif nested_quote is None and nested == ")":
                    depth -= 1
                end += 1
            if depth == 0:
                yield command[index + 2 : end - 1]
                index = end
                continue
            return
        index += 1


def strip_command_prefixes(segment: list[str]) -> list[str]:
    tokens = list(segment)
    while tokens:
        token = tokens[0]
        if "=" in token and (
            token.split("=", 1)[0].isidentifier()
            or token.startswith(ASSIGNMENT_PREFIXES)
        ):
            tokens.pop(0)
            continue
        if os.path.basename(token) in {"command", "env", "sudo"}:
            tokens.pop(0)
            while tokens and (tokens[0].startswith("-") or "=" in tokens[0]):
                tokens.pop(0)
            continue
        if os.path.basename(token) in {"nice", "nohup", "time"}:
            tokens.pop(0)
            while tokens and tokens[0].startswith("-"):
                tokens.pop(0)
            continue
        if os.path.basename(token) == "timeout":
            tokens.pop(0)
            while tokens and tokens[0].startswith("-"):
                tokens.pop(0)
            if tokens:
                tokens.pop(0)
            continue
        break
    return tokens


def segment_runs_cargo_test(segment: list[str]) -> bool:
    tokens = strip_command_prefixes(segment)
    if not tokens:
        return False

    program = os.path.basename(tokens[0]).lower()
    if program in {"bash", "sh", "zsh"}:
        for index, token in enumerate(tokens[1:], start=1):
            if token == "-c" or (token.startswith("-") and "c" in token[1:]):
                if index + 1 < len(tokens):
                    return contains_direct_cargo_test(tokens[index + 1])
                return False

    if program in {"cargo-test", "cargo-test.exe"}:
        return True
    if program not in {"cargo", "cargo.exe"}:
        return False

    args = tokens[1:]
    if args and args[0].startswith("+"):
        args = args[1:]
    options_with_values = {"--color", "--config", "-Z"}
    index = 0
    while index < len(args):
        arg = args[index]
        if arg == "test":
            return True
        if not arg.startswith("-"):
            return False
        index += 2 if arg in options_with_values else 1
    return False


def contains_direct_cargo_test(command: str) -> bool:
    if any(contains_direct_cargo_test(nested) for nested in active_subcommands(command)):
        return True
    return any(
        segment_runs_cargo_test(segment)
        for segment in command_segments(tokenize(command))
    )


def main() -> int:
    raw = sys.stdin.read()
    if not raw.strip():
        return 0
    try:
        payload = json.loads(raw)
    except json.JSONDecodeError:
        payload = {"command": raw}

    if not any(contains_direct_cargo_test(command) for command in command_strings(payload)):
        return 0

    print(
        "Blocked direct cargo test invocation. Use './run.sh test -p <crate> "
        "[filter]' for focused/local Rust tests or './run.sh test-closeout' "
        "for the nextest plus doctest closeout gate. CI runs the same "
        "nextest selection (ADR 0090).",
        file=sys.stderr,
    )
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
