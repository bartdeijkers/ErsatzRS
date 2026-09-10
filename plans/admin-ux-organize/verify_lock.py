#!/usr/bin/env python3
"""Verify the approved design baseline without rewriting reference files."""

import hashlib
import json
from pathlib import Path
import sys


def main():
    root = Path(__file__).resolve().parent
    manifest = json.loads((root / "design-lock.json").read_text(encoding="utf-8"))
    files = manifest["files"]
    sources = {
        path.relative_to(root).as_posix()
        for pattern in ("*.html", "*.css", "assets/**/*")
        for path in root.glob(pattern)
        if path.is_file()
    } | {"preview_server.py", "html_tree.py"}
    locked_sources = {name for name in files if not name.startswith("screenshots/")}
    errors = []
    if sources != locked_sources:
        errors.append(f"Source inventory changed: {sorted(sources ^ locked_sources)}")
    for name, expected in files.items():
        path = root / name
        if not path.is_file():
            errors.append(f"Missing: {name}")
        elif hashlib.sha256(path.read_bytes()).hexdigest() != expected:
            errors.append(f"Changed: {name}")
    if errors:
        print("Design lock failed; do not regenerate the baseline without approval.")
        print("\n".join(errors))
        return 1
    print(f"Design lock revision {manifest['revision']}: {len(files)} files verified.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
