#!/usr/bin/env python3
"""Run rustfmt for one edited Rust file when a hook payload identifies it."""

from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Any


PATCH_FILE_RE = re.compile(r"^\*\*\* (?:Add|Update) File: (.+)$", re.MULTILINE)
PATH_KEYS = {"file", "file_path", "path", "target_file"}


def repository_root() -> Path:
    """Resolve the checkout without invoking Git under the sandbox account."""
    return Path(__file__).resolve().parents[2]


def iter_values(value: Any, parent_key: str = ""):
    if isinstance(value, str):
        yield parent_key, value
        return

    if isinstance(value, list):
        for item in value:
            yield from iter_values(item, parent_key)
        return

    if isinstance(value, dict):
        for key, item in value.items():
            yield from iter_values(item, str(key))


def candidate_paths(raw: str) -> set[str]:
    paths: set[str] = set()

    try:
        payload = json.loads(raw)
    except json.JSONDecodeError:
        payload = None

    if payload is not None:
        tool_input = payload.get("tool_input") if isinstance(payload, dict) else None
        source = tool_input if tool_input is not None else payload
        for key, value in iter_values(source):
            if key in PATH_KEYS:
                paths.add(value)
            if "*** Begin Patch" in value:
                paths.update(PATCH_FILE_RE.findall(value))
    else:
        paths.update(PATCH_FILE_RE.findall(raw))

    return paths


def normalize(path_text: str, root: Path) -> Path | None:
    path = Path(path_text)
    if not path.is_absolute():
        path = root / path

    try:
        resolved = path.resolve()
        resolved.relative_to(root)
    except (OSError, ValueError):
        return None

    if resolved.suffix != ".rs" or not resolved.is_file():
        return None
    if "target" in resolved.relative_to(root).parts:
        return None

    return resolved


def main() -> int:
    raw = sys.stdin.read()
    if not raw.strip():
        return 0

    root = repository_root()
    rust_files = {
        resolved
        for path_text in candidate_paths(raw)
        if (resolved := normalize(path_text, root)) is not None
    }

    if len(rust_files) != 1:
        return 0

    try:
        result = subprocess.run(
            ["rustfmt", "--edition", "2021", str(next(iter(rust_files)))],
            cwd=root,
            text=True,
        )
    except FileNotFoundError:
        print("rustfmt not found; skipped single-file formatting", file=sys.stderr)
        return 0

    return result.returncode


if __name__ == "__main__":
    raise SystemExit(main())
