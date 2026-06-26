#!/usr/bin/env python3
"""Block repository-content writes that violate CONTRIBUTING.md safety rules."""

from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Any


LOCAL_PATH_PATTERNS: tuple[tuple[str, re.Pattern[str]], ...] = (
    (
        "maintainer Linux home absolute path",
        re.compile(r"(?<![A-Za-z0-9_.-])/(?:home|Users)/[A-Za-z0-9._-]+(?:/[^\s'\"`)<>]*)?"),
    ),
    (
        "maintainer Windows user absolute path",
        re.compile(
            r"(?i)\b[A-Z]:\\Users\\(?!(?:<user>|\.\.\.))"
            r"[^\\\s'\"`)<>]+(?:\\[^\s'\"`)<>]*)?"
        ),
    ),
    (
        "WSL-mounted Windows user absolute path",
        re.compile(r"(?<![A-Za-z0-9_.-])/mnt/[A-Za-z]/Users/[A-Za-z0-9._-]+(?:/[^\s'\"`)<>]*)?"),
    ),
    (
        "private media path",
        re.compile(
            r"(?i)(?<![A-Za-z0-9_.-])(?:"
            r"/mnt/[A-Za-z]/[^\s'\"`)<>]*(?:media|movies|tv|music|plex|jellyfin|emby|nastv)[^\s'\"`)<>]*"
            r"|\\\\[^\\\s'\"`)<>]+\\(?:media|movies|tv|music|plex|jellyfin|emby|nastv)(?:\\[^\s'\"`)<>]*)?"
            r")"
        ),
    ),
)

PERSONAL_MEDIA_PATTERNS: tuple[tuple[str, re.Pattern[str]], ...] = (
    (
        "personal media title",
        re.compile(
            r"(?i)\b2010\s*(?::|-)?\s*the\s+year\s+we\s+ma(?:k|d)e\s+contact\b"
        ),
    ),
)

SECRET_PATTERNS: tuple[tuple[str, re.Pattern[str]], ...] = (
    ("private key material", re.compile(r"-----BEGIN [A-Z ]*PRIVATE KEY-----")),
    ("OpenAI-style API key", re.compile(r"\bsk-(?:proj-)?[A-Za-z0-9_-]{20,}\b")),
    ("GitHub token", re.compile(r"\bgh[pousr]_[A-Za-z0-9_]{20,}\b")),
    ("GitLab token", re.compile(r"\bglpat-[A-Za-z0-9_-]{20,}\b")),
    ("Slack token", re.compile(r"\bxox[baprs]-[A-Za-z0-9-]{20,}\b")),
    ("AWS access key id", re.compile(r"\b(?:AKIA|ASIA)[A-Z0-9]{16}\b")),
)

GENERIC_SECRET_ASSIGNMENT = re.compile(
    r"(?i)\b("
    r"api[_-]?key|access[_-]?token|refresh[_-]?token|id[_-]?token|"
    r"client[_-]?secret|issuer[_-]?signing[_-]?key|password|passwd|secret"
    r")\b\s*[:=]\s*[\"']([A-Za-z0-9_./+=:-]{16,})[\"']"
)

PLACEHOLDER_WORDS = (
    "changeme",
    "dummy",
    "example",
    "placeholder",
    "redacted",
    "replace",
    "sample",
    "test",
    "token",
    "your",
)

CONTENT_KEYS = {"content", "new_string", "patch", "input"}


def patch_added_text(patch: str) -> str:
    """Return only added content from an apply_patch payload."""
    if "*** Begin Patch" not in patch:
        return patch

    added: list[str] = []
    for line in patch.splitlines():
        if line.startswith("+") and not line.startswith("+++"):
            added.append(line[1:])
    return "\n".join(added)


def iter_content_strings(value: Any, parent_key: str = ""):
    if isinstance(value, str):
        if parent_key in CONTENT_KEYS or "*** Begin Patch" in value:
            yield patch_added_text(value) if parent_key in {"patch", "input"} else value
        return

    if isinstance(value, list):
        for item in value:
            yield from iter_content_strings(item, parent_key)
        return

    if isinstance(value, dict):
        for key, item in value.items():
            key_text = str(key)
            if isinstance(item, str) and key_text == "old_string":
                continue
            yield from iter_content_strings(item, key_text)


def content_from_stdin(raw: str) -> str:
    try:
        payload = json.loads(raw)
    except json.JSONDecodeError:
        return patch_added_text(raw)

    tool_input = payload.get("tool_input") if isinstance(payload, dict) else None
    source = tool_input if tool_input is not None else payload
    return "\n".join(iter_content_strings(source))


def is_placeholder_secret(value: str) -> bool:
    lowered = value.lower()
    if any(word in lowered for word in PLACEHOLDER_WORDS):
        return True
    return value.startswith(("$", "${", "<")) or "..." in value or "xxx" in lowered


def find_violation(content: str) -> str | None:
    for name, pattern in LOCAL_PATH_PATTERNS:
        if pattern.search(content):
            return name

    for name, pattern in PERSONAL_MEDIA_PATTERNS:
        if pattern.search(content):
            return name

    for name, pattern in SECRET_PATTERNS:
        if pattern.search(content):
            return name

    for match in GENERIC_SECRET_ASSIGNMENT.finditer(content):
        if not is_placeholder_secret(match.group(2)):
            return "literal credential assignment"

    return None


def scan_repo() -> int:
    try:
        raw_files = subprocess.check_output(["git", "ls-files", "-z"])
    except (OSError, subprocess.CalledProcessError) as exc:
        print(f"Unable to list tracked files: {exc}", file=sys.stderr)
        return 2

    failures: list[str] = []
    for raw_name in raw_files.split(b"\0"):
        if not raw_name:
            continue

        path = Path(raw_name.decode())
        try:
            content = path.read_text(encoding="utf-8")
        except (FileNotFoundError, UnicodeDecodeError):
            continue

        for line_no, line in enumerate(content.splitlines(), start=1):
            violation = find_violation(line)
            if violation is not None:
                failures.append(f"{path}:{line_no}: {violation}")

    if failures:
        print("CONTRIBUTING.md repository-content violations:", file=sys.stderr)
        print("\n".join(failures), file=sys.stderr)
        return 2

    return 0


def main() -> int:
    if sys.argv[1:] == ["--scan-repo"]:
        return scan_repo()

    raw = sys.stdin.read()
    if not raw.strip():
        return 0

    violation = find_violation(content_from_stdin(raw))
    if violation is None:
        return 0

    print(
        f"Blocked edit: CONTRIBUTING.md forbids {violation} in repository content. "
        "Use repo-relative paths, $ERSATZTV_SRC, environment variables, or redacted placeholders.",
        file=sys.stderr,
    )
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
