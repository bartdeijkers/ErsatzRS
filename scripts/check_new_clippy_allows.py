#!/usr/bin/env python3
"""Require a reason and item scope for newly added Clippy suppressions."""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path


ATTRIBUTE_START = re.compile(r"^\s*#\s*(!?)\s*\[\s*(?:allow|expect)\s*\(")
CLIPPY_LINT = re.compile(r"\bclippy::[a-z_]+\b")
BROAD_LINT = re.compile(r"\b(?:warnings|clippy::(?:all|pedantic|nursery|restriction|cargo))\b")
HUNK_START = re.compile(r"^@@ -\d+(?:,\d+)? \+(\d+)(?:,\d+)? @@")
INLINE_REASON = re.compile(r"\breason\s*=\s*\"[^\"]+\"")
COMMENT_REASON = re.compile(r"//\s*\S")


def added_line_numbers(diff: str) -> set[int]:
    """Return destination line numbers from a zero-context Git diff."""
    added: set[int] = set()
    line_number: int | None = None
    for line in diff.splitlines():
        hunk = HUNK_START.match(line)
        if hunk:
            line_number = int(hunk.group(1))
        elif line_number is not None and line.startswith("+") and not line.startswith("+++"):
            added.add(line_number)
            line_number += 1
        elif line_number is not None and line.startswith(" "):
            line_number += 1
    return added


def new_allow_violations(source: str, added: set[int]) -> list[tuple[int, str]]:
    """Check only attributes with changed lines; leave existing debt alone."""
    lines = source.splitlines()
    findings: list[tuple[int, str]] = []
    index = 0
    while index < len(lines):
        start = ATTRIBUTE_START.match(lines[index])
        if start is None:
            index += 1
            continue

        first = index
        while index + 1 < len(lines) and ")]" not in lines[index]:
            index += 1
        attribute = "\n".join(lines[first : index + 1])
        lint_tokens = re.sub(r'"(?:\\.|[^"\\])*"', '""', attribute)
        lint_tokens = re.sub(r"//[^\n]*", "", lint_tokens)
        is_new = any(number in added for number in range(first + 1, index + 2))
        if is_new and (CLIPPY_LINT.search(lint_tokens) or BROAD_LINT.search(lint_tokens)):
            if BROAD_LINT.search(lint_tokens):
                findings.append((first + 1, "name specific Clippy lint(s), not a lint group or warnings"))
            elif start.group(1):
                findings.append((first + 1, "use an item-level allow instead of a file-wide allow"))
            elif not INLINE_REASON.search(attribute) and not COMMENT_REASON.search(lines[index]):
                findings.append((first + 1, "add a reason=... argument or an inline // reason"))
        index += 1
    return findings


def git_output(*args: str) -> bytes:
    return subprocess.check_output(["git", *args], stderr=subprocess.PIPE)


def check(base: str) -> list[str]:
    git_output("rev-parse", "--verify", f"{base}^{{commit}}")
    changed = git_output("diff", "--name-only", "-z", "--diff-filter=ACMR", base, "--")
    findings: list[str] = []
    for raw_path in changed.split(b"\0"):
        if not raw_path:
            continue
        path = Path(raw_path.decode("utf-8", errors="surrogateescape"))
        if path.suffix != ".rs":
            continue
        diff = git_output("diff", "--unified=0", "--no-ext-diff", base, "--", str(path))
        added = added_line_numbers(diff.decode("utf-8", errors="surrogateescape"))
        if not added:
            continue
        source = path.read_text(encoding="utf-8")
        for line, reason in new_allow_violations(source, added):
            findings.append(f"{path}:{line}: {reason}")
    return findings


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", default="HEAD", help="commit to compare with the working tree")
    args = parser.parse_args()
    try:
        findings = check(args.base)
    except (OSError, subprocess.CalledProcessError, UnicodeError) as error:
        print(f"Clippy allow policy could not run: {error}", file=sys.stderr)
        return 2
    if findings:
        print("New Clippy allow policy violations:", file=sys.stderr)
        for finding in findings:
            print(f"  {finding}", file=sys.stderr)
        return 1
    print("New Clippy allow policy: passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
