"""Verify the accepted OAD design bytes without modifying the artifact."""

import hashlib
import json
from pathlib import Path


def main():
    root = Path(__file__).resolve().parent
    manifest = json.loads((root / "design-lock.json").read_text())
    failures = []
    for name, expected in manifest["files"].items():
        target = (root / name).resolve()
        if not target.is_relative_to(root) or not target.is_file():
            failures.append(f"Missing or invalid path: {name}")
        elif hashlib.sha256(target.read_bytes()).hexdigest() != expected:
            failures.append(f"Changed locked file: {name}")
    if failures:
        raise SystemExit("\n".join(failures))
    print(f"OAD {manifest['revision']} lock verified: {len(manifest['files'])} files")


if __name__ == "__main__":
    main()
