#!/usr/bin/env python3
"""Verify and prepare the temporary Next source exception (ADR 0116)."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

MANIFEST = Path(__file__).with_name("next-patches") / "manifest.json"


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def reviewed_manifest(path=MANIFEST, revision=None):
    path = Path(path)
    manifest = json.loads(path.read_text(encoding="utf-8"))
    revision = revision or manifest["upstream_revision"]
    if revision != manifest["upstream_revision"]:
        raise ValueError("New Next revision requires unpatched regression review and patch retirement assessment")
    for patch in manifest["patches"]:
        if patch["upstream_review_revision"] != revision:
            raise ValueError("Patch has not been reviewed against this upstream revision")
        if not patch.get("retire_when") or not patch.get("unpatched_regression"):
            raise ValueError("Patch requires an explicit retirement condition and unpatched regression evidence")
        name = Path(patch["file"])
        if name.name != str(name) or name.is_absolute():
            raise ValueError("Patch must be an adjacent file")
        file = path.parent / name
        if file.is_symlink() or digest(file) != patch["sha256"]:
            raise ValueError("Next patch SHA-256 mismatch")
    return manifest


def prepare(archive, output, manifest_path=MANIFEST, revision=None, unpatched=False):
    manifest_path = Path(manifest_path).resolve()
    manifest = reviewed_manifest(manifest_path, revision)
    archive, output = Path(archive).resolve(), Path(output).resolve()
    if digest(archive) != manifest["source_archive_sha256"]:
        raise ValueError("Next source archive SHA-256 mismatch")
    if output.exists():
        raise ValueError("Next source destination already exists")
    output.mkdir(parents=True)
    # The archive is the checksum-pinned upstream source. Keep its symlinks and
    # permissions; never extract an unverified response or arbitrary archive.
    subprocess.run(["tar", "-xzf", str(archive), "--strip-components=1", "-C", str(output)], check=True)
    if digest(output / "Cargo.lock") != manifest["source_lock_sha256"]:
        raise ValueError("Next source Cargo.lock SHA-256 mismatch")
    subprocess.run(["git", "init", "-q", str(output)], check=True)
    if not unpatched:
        for patch in manifest["patches"]:
            file = str(manifest_path.parent / patch["file"])
            # git apply has no fuzzy context fallback. Any source drift requires
            # review instead of silently carrying our patch onto another version.
            subprocess.run(["git", "-C", str(output), "apply", "--check", file], check=True)
            subprocess.run(["git", "-C", str(output), "apply", file], check=True)
    if digest(output / "Cargo.lock") != manifest["source_lock_sha256"]:
        raise ValueError("Next patch changed the upstream lockfile")
    provenance = {
        "source_revision": manifest["upstream_revision"],
        "source_archive_sha256": manifest["source_archive_sha256"],
        "cargo_lock_sha256": manifest["source_lock_sha256"],
        "distribution_version": manifest["distribution_version"],
        "patches": [] if unpatched else [
            {"file": patch["file"], "sha256": patch["sha256"]}
            for patch in manifest["patches"]
        ],
    }
    (output / "ersatzrs-source.json").write_text(json.dumps(provenance, indent=2) + "\n", encoding="utf-8")
    return provenance


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-archive", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--manifest", type=Path, default=MANIFEST)
    parser.add_argument("--revision", help="Expected upstream revision; changes require a retirement review")
    parser.add_argument("--unpatched", action="store_true", help="Prepare the upstream comparison without our fix")
    args = parser.parse_args()
    result = prepare(args.source_archive, args.output, args.manifest, args.revision, args.unpatched)
    print(json.dumps(result, indent=2))
