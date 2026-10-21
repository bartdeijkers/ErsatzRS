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
import stat
import subprocess
import sys
import tarfile
import tempfile
from contextlib import suppress


REQUIRED = {
    "rust": ("nextest.tar.zst", "bin/cargo-nextest", "bin/xtask", "bin/provision_runtime", "bin/ersatzrs-web-smoke"),
    "web": ("bin/xtask", "bin/provision_runtime", "bin/ersatzrs-web",
            "site/pkg/ersatzrs-web.js", "site/pkg/ersatzrs-web.wasm",
            "site/bundle.css", "site/channel-preview.js", "site/vendor/hls/hls.mjs",
            "site/vendor/hls/LICENSE"),
}
LOCAL_GROUP_MARKER = ".ersatzrs-ci-artifacts"
LOCAL_GROUP_OWNER = b"ersatzrs-ci-artifacts-group-v1\n"


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


def local_entry(cache, key):
    """Separate producer groups from their two immutable artifact kinds."""
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]{0,199}", key):
        raise ValueError("unsafe local CI artifact cache key")
    group, separator, kind = key.rpartition("-")
    if not separator or not group or kind not in REQUIRED:
        raise ValueError("local CI artifact cache key must end in -rust or -web")
    group_path = cache / group
    return group_path, group_path / f"{kind}.tar.gz", kind


def check_cache_directories(cache, group):
    if cache.is_symlink() or group.is_symlink():
        raise ValueError("local CI artifact cache directories must not be symlinks")


def regular_entry(path):
    """Reject symlinks and other non-file entries instead of following them."""
    try:
        mode = path.lstat().st_mode
    except FileNotFoundError:
        return False
    if not stat.S_ISREG(mode):
        raise ValueError("local CI artifact cache entry must be a regular file")
    return True


def owned_local_group(group, strict=True):
    marker = group / LOCAL_GROUP_MARKER
    try:
        if not regular_entry(marker):
            return False
        if marker.stat().st_size != len(LOCAL_GROUP_OWNER) or marker.read_bytes() != LOCAL_GROUP_OWNER:
            raise ValueError("local CI artifact cache ownership marker differs")
        return True
    except ValueError:
        if strict:
            raise
        return False


def initialize_local_group(cache, group):
    """Claim only an empty group, using one complete atomic ownership marker."""
    if owned_local_group(group):
        return
    if any(group.iterdir()):
        # Another producer may have published its marker after our first read.
        if owned_local_group(group):
            return
        raise ValueError("refusing nonempty unowned local CI artifact cache group")
    descriptor, name = tempfile.mkstemp(prefix=".owner-", dir=cache)
    temporary = Path(name)
    try:
        with os.fdopen(descriptor, "wb") as handle:
            handle.write(LOCAL_GROUP_OWNER)
            handle.flush()
            os.fsync(handle.fileno())
        try:
            os.link(temporary, group / LOCAL_GROUP_MARKER)
        except FileExistsError:
            if not owned_local_group(group):
                raise ValueError("local CI artifact cache ownership marker differs") from None
    finally:
        with suppress(OSError):
            temporary.unlink()


def prune_local(cache, current):
    """Keep at most four producer groups, preserving the just-published group."""
    groups = [path for path in cache.iterdir()
              if re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]{0,199}", path.name)
              and not path.is_symlink() and path.is_dir()
              and owned_local_group(path, strict=False)]
    old = sorted((path for path in groups if path != current),
                 key=lambda path: path.stat().st_mtime_ns)
    for path in old[:max(0, len(groups) - 4)]:
        shutil.rmtree(path)


def publish_local(archive: Path, sha: str, kind: str, key: str, cache: Path) -> str:
    """Publish digest-bound bytes; pack/restore retain source and host authority."""
    if not re.fullmatch(r"[0-9a-f]{40}", sha) or kind not in REQUIRED:
        raise ValueError("local CI artifact publication needs a source SHA and kind")
    group, entry, key_kind = local_entry(cache, key)
    if kind != key_kind:
        raise ValueError("local CI artifact cache key has the wrong kind")
    if not regular_entry(archive):
        raise ValueError("local CI artifact archive is missing")
    expected = digest(archive)
    temporary = None
    try:
        check_cache_directories(cache, group)
        group.mkdir(parents=True, exist_ok=True)
        initialize_local_group(cache, group)
        if regular_entry(entry):
            if digest(entry) != expected:
                raise ValueError("immutable local CI artifact cache entry differs")
        else:
            descriptor, name = tempfile.mkstemp(prefix=".publish-", dir=group)
            os.close(descriptor)
            temporary = Path(name)
            shutil.copyfile(archive, temporary)
            if digest(temporary) != expected:
                raise ValueError("local CI artifact changed during publication")
            with temporary.open("rb") as handle:
                os.fsync(handle.fileno())
            try:
                os.link(temporary, entry)
            except FileExistsError:
                if not regular_entry(entry) or digest(entry) != expected:
                    raise ValueError("immutable local CI artifact cache entry differs") from None
        os.utime(group, None)
        prune_local(cache, group)
    except OSError:
        print("Local CI artifact cache unavailable; using GitHub artifact fallback", file=sys.stderr)
    finally:
        if temporary is not None:
            with suppress(OSError):
                temporary.unlink()
    return expected


def fetch_local(archive: Path, expected_digest: str, key: str, cache: Path) -> bool:
    """Expose a cache hit only after a complete temporary copy matches its hash."""
    if not re.fullmatch(r"[0-9a-f]{64}", expected_digest):
        raise ValueError("a full CI artifact SHA256 is required")
    group, entry, _ = local_entry(cache, key)
    if archive.is_symlink():
        raise ValueError("local CI artifact destination must not be a symlink")
    temporary = None
    try:
        check_cache_directories(cache, group)
        if not owned_local_group(group):
            return False
        if not regular_entry(entry):
            return False
        archive.parent.mkdir(parents=True, exist_ok=True)
        descriptor, name = tempfile.mkstemp(prefix=".fetch-", dir=archive.parent)
        os.close(descriptor)
        temporary = Path(name)
        shutil.copyfile(entry, temporary)
        if digest(temporary) != expected_digest:
            raise ValueError("local CI artifact cache hash mismatch")
        os.replace(temporary, archive)
        return True
    except OSError:
        print("Local CI artifact cache unavailable; using GitHub artifact fallback", file=sys.stderr)
        return False
    finally:
        if temporary is not None:
            with suppress(OSError):
                temporary.unlink()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("pack", "restore", "verify", "publish-local", "fetch-local"))
    parser.add_argument("--directory", type=Path)
    parser.add_argument("--kind", choices=("rust", "web"), required=True)
    parser.add_argument("--archive", type=Path)
    parser.add_argument("--sha", required=True)
    parser.add_argument("--env-file", type=Path)
    parser.add_argument("--cache-key")
    parser.add_argument("--expected-digest")
    parser.add_argument("--output-file", type=Path)
    args = parser.parse_args()
    if not re.fullmatch(r"[0-9a-f]{40}", args.sha):
        parser.error("a full source SHA is required")
    local = args.mode in ("publish-local", "fetch-local")
    if not local and not args.directory:
        parser.error("pack/restore/verify requires --directory")
    if args.mode != "verify" and not args.archive:
        parser.error("this mode requires --archive")
    if local and not args.cache_key:
        parser.error("local cache modes require --cache-key")
    if args.mode == "fetch-local" and not args.expected_digest:
        parser.error("fetch-local requires --expected-digest")
    actual_sha = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    if args.sha != actual_sha:
        parser.error("execution checkout differs from the requested source SHA")
    if local:
        cache = Path(os.environ.get("CI_LOCAL_ARTIFACT_CACHE", "")) if os.environ.get(
            "CI_LOCAL_ARTIFACT_CACHE") else Path.home() / ".cache/ersatzrs/ci-artifacts"
        if local_entry(cache, args.cache_key)[2] != args.kind:
            parser.error("local CI artifact cache key has the wrong kind")
        if args.mode == "publish-local":
            value = "digest=" + publish_local(args.archive, args.sha, args.kind, args.cache_key, cache)
        else:
            value = "hit=" + str(fetch_local(args.archive, args.expected_digest, args.cache_key, cache)).lower()
        if args.output_file:
            with args.output_file.open("a") as handle:
                handle.write(value + "\n")
        print(value)
        return
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
