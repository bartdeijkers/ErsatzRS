#!/usr/bin/env python3
"""Package and verify exact-source CI executables and a matched Leptos site."""

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import platform
import re
import shutil
import subprocess
import tarfile


REQUIRED = {
    "rust": ("nextest.tar.zst", "bin/cargo-nextest", "bin/xtask", "bin/provision_runtime", "bin/ersatzrs-web-smoke"),
    "web": ("bin/xtask", "bin/provision_runtime", "bin/ersatzrs-web",
            "site/pkg/ersatzrs-web.js", "site/pkg/ersatzrs-web.wasm",
            "site/bundle.css", "site/channel-preview.js", "site/vendor/hls/hls.mjs",
            "site/vendor/hls/LICENSE"),
}


def digest(path):
    with path.open("rb") as handle:
        return hashlib.file_digest(handle, "sha256").hexdigest()


def host():
    return {"system": platform.system(), "machine": platform.machine(),
            "libc": list(platform.libc_ver())}


def check_required(root, kind):
    for name in REQUIRED[kind]:
        path = root / name
        if not path.is_file() or not path.stat().st_size:
            raise ValueError(f"missing or empty CI build product: {name}")
    for name in REQUIRED[kind]:
        if name.startswith("bin/") and not os.access(root / name, os.X_OK):
            raise ValueError(f"CI executable lost its executable permission: {name}")


def pack(root, archive, sha, kind):
    check_required(root, kind)
    actual_sha = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    if sha != actual_sha:
        raise ValueError("build source SHA differs from the requested revision")
    files = {}
    for path in sorted(root.rglob("*")):
        if path.is_symlink():
            raise ValueError("CI build products must not contain symlinks")
        if path.is_file():
            name = path.relative_to(root).as_posix()
            if name == "build.json":
                raise ValueError("refusing to reuse an existing build manifest")
            files[name] = {"sha256": digest(path), "size": path.stat().st_size,
                           "executable": bool(path.stat().st_mode & 0o111)}
    manifest = {"schema_version": 1, "source_sha": sha, "kind": kind, "host": host(), "files": files,
                "toolchain": subprocess.check_output(["rustc", "--version"], text=True).strip(),
                "build_flags": {key: os.environ.get(key, "") for key in (
                    "RUSTFLAGS", "CARGO_PROFILE_TEST_DEBUG", "LEPTOS_OUTPUT_NAME", "LEPTOS_HASH_FILES")}}
    (root / "build.json").write_text(json.dumps(manifest, indent=2) + "\n")
    with tarfile.open(archive, "w:gz", compresslevel=1) as handle:
        handle.add(root, arcname=".", recursive=True)


def verify(root, sha, kind):
    manifest = json.loads((root / "build.json").read_text())
    if manifest.get("schema_version") != 1 or manifest.get("source_sha") != sha or manifest.get("kind") != kind:
        raise ValueError("CI build manifest has the wrong schema or source SHA")
    if manifest.get("host") != host():
        raise ValueError("CI build and execution platform/libc differ")
    if any(p.is_symlink() for p in root.rglob("*")):
        raise ValueError("CI build products must not contain symlinks")
    actual = {p.relative_to(root).as_posix() for p in root.rglob("*") if p.is_file()}
    if actual != set(manifest["files"]) | {"build.json"}:
        raise ValueError("CI build product inventory differs from its manifest")
    for name, expected in manifest["files"].items():
        path = root / name
        if path.is_symlink() or path.stat().st_size != expected["size"] or digest(path) != expected["sha256"]:
            raise ValueError(f"CI build product hash/size mismatch: {name}")
        if bool(path.stat().st_mode & 0o111) != expected["executable"]:
            raise ValueError(f"CI build product permission mismatch: {name}")
    check_required(root, kind)


def restore(archive, root, sha, env_file, kind):
    if root.exists():
        raise ValueError("CI build destination already exists; refusing stale files")
    root.mkdir(parents=True)
    try:
        with tarfile.open(archive, "r:gz") as handle:
            seen = set()
            for entry in handle.getmembers():
                path = PurePosixPath(entry.name)
                if path.is_absolute() or ".." in path.parts or not (entry.isfile() or entry.isdir()):
                    raise ValueError("unsafe CI build archive member")
                if path.as_posix() in seen:
                    raise ValueError("duplicate CI build archive member")
                seen.add(path.as_posix())
            handle.extractall(root, filter="data")
        verify(root, sha, kind)
        if env_file:
            values = {
                "CI_BUILD_DIR": root,
                "CI_XTASK": root / "bin/xtask",
                "CI_PROVISION_RUNTIME": root / "bin/provision_runtime",
            }
            if kind == "rust":
                values.update({"CI_NEXTEST_ARCHIVE": root / "nextest.tar.zst",
                               "CI_NEXTEST": root / "bin/cargo-nextest",
                               "CI_SMOKE_SERVER": root / "bin/ersatzrs-web-smoke"})
            else:
                values["ERSATZRS_E2E_PREBUILT_DIR"] = root
            if any(c in str(value) for value in values.values() for c in "\r\n"):
                raise ValueError("CI build path cannot be represented in an environment file")
            with env_file.open("a") as handle:
                for key, value in values.items():
                    handle.write(f"{key}={value}\n")
    except Exception:
        shutil.rmtree(root)
        raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("pack", "restore", "verify"))
    parser.add_argument("--directory", type=Path, required=True)
    parser.add_argument("--kind", choices=("rust", "web"), required=True)
    parser.add_argument("--archive", type=Path)
    parser.add_argument("--sha", required=True)
    parser.add_argument("--env-file", type=Path)
    args = parser.parse_args()
    if not re.fullmatch(r"[0-9a-f]{40}", args.sha):
        parser.error("a full source SHA is required")
    if args.mode in ("pack", "restore") and not args.archive:
        parser.error("pack/restore requires --archive")
    actual_sha = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    if args.sha != actual_sha:
        parser.error("execution checkout differs from the requested source SHA")
    root = args.directory.resolve()
    if args.mode == "pack":
        pack(root, args.archive, args.sha, args.kind)
    elif args.mode == "restore":
        restore(args.archive, root, args.sha, args.env_file, args.kind)
    else:
        verify(root, args.sha, args.kind)
    print(f"CI build {args.mode}: verified source {args.sha}")


if __name__ == "__main__":
    main()
