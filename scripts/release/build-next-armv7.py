#!/usr/bin/env python3
"""Build the accepted, unmodified Next ARMv7 source with a Bookworm toolchain."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

spec = importlib.util.spec_from_file_location("stage_next", Path(__file__).with_name("stage-next.py"))
stage_next = importlib.util.module_from_spec(spec)
spec.loader.exec_module(stage_next)
TARGET = "armv7-unknown-linux-gnueabihf"


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def build(work, output, archive=None):
    work, output = Path(work).resolve(), Path(output).resolve()
    if output.exists():
        raise ValueError("Next output already exists")
    work.mkdir(parents=True, exist_ok=True)
    source = work / "next-source"
    source.mkdir(exist_ok=True)
    if any(path.name != "target" for path in source.iterdir()):
        raise ValueError("Next source directory must be clean; only the target cache may exist")
    url = f"https://codeload.github.com/{stage_next.REPOSITORY}/tar.gz/{stage_next.PINNED_REVISION}"
    data = Path(archive).read_bytes() if archive else stage_next.download(url, stage_next.MAX_ARCHIVE)
    if hashlib.sha256(data).hexdigest() != stage_next.SOURCE_ARCHIVE_SHA256:
        raise ValueError("Pinned Next source archive SHA-256 mismatch")
    archive_path = work / "source.tar.gz"
    archive_path.write_bytes(data)
    # Extraction only follows successful verification of the immutable upstream
    # archive. Preserve its internal template symlinks and file permissions.
    subprocess.run(["tar", "-xzf", str(archive_path), "--strip-components=1", "-C", str(source)], check=True)
    if sha(source / "Cargo.lock") != stage_next.SOURCE_LOCK_SHA256:
        raise ValueError("Pinned Next Cargo.lock SHA-256 mismatch")
    rustc = subprocess.check_output(["rustc", "--version"], text=True).strip()
    if not rustc.startswith("rustc 1.97.1 "):
        raise ValueError("Next ARMv7 packaging requires Rust 1.97.1")
    compiler = subprocess.check_output(["arm-linux-gnueabihf-gcc", "--version"], text=True).splitlines()[0]
    command = ["cargo", "build", "--locked", "--release", "--all-features", "--target", TARGET,
               "--bin", "ersatztv-channel"]
    environment = dict(os.environ)
    environment.update({
        "ETV_VERSION": "0.1.0-" + stage_next.PINNED_REVISION[:7],
        "CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABIHF_LINKER": "arm-linux-gnueabihf-gcc",
        "CC_armv7_unknown_linux_gnueabihf": "arm-linux-gnueabihf-gcc",
        "AR_armv7_unknown_linux_gnueabihf": "arm-linux-gnueabihf-ar",
    })
    subprocess.run(command, cwd=source, env=environment, check=True)
    if sha(source / "Cargo.lock") != stage_next.SOURCE_LOCK_SHA256:
        raise ValueError("Next build changed the upstream lockfile")
    binary = source / "target" / TARGET / "release" / "ersatztv-channel"
    subprocess.run(["arm-linux-gnueabihf-strip", str(binary)], check=True)
    versions = subprocess.check_output(["arm-linux-gnueabihf-readelf", "--version-info", str(binary)], text=True)
    import re
    required = {tuple(map(int, value.split('.'))) for value in re.findall(r"GLIBC_([0-9.]+)", versions)}
    if not required or max(required) > (2, 36):
        raise ValueError("Next ARMv7 executable exceeds the GLIBC 2.36 runtime baseline")
    with tempfile.TemporaryDirectory(dir=work) as temporary:
        stage = Path(temporary)
        shutil.copyfile(binary, stage / "ersatztv-channel")
        shutil.copyfile(source / "LICENSE", stage / "LICENSE")
        (stage / "origin.json").write_text(json.dumps({
            "kind": "source-build", "repository": stage_next.REPOSITORY,
            "source_revision": stage_next.PINNED_REVISION, "revision": stage_next.PINNED_REVISION[:7],
            "rid": "linux-arm", "target": TARGET, "source_url": url,
            "source_archive_sha256": stage_next.SOURCE_ARCHIVE_SHA256,
            "cargo_lock_sha256": stage_next.SOURCE_LOCK_SHA256,
            "binary_sha256": sha(binary), "license_sha256": sha(source / "LICENSE"),
            "rustc": rustc, "compiler": compiler, "build_command": command,
            "glibc_required": '.'.join(map(str, max(required))),
        }, indent=2) + "\n", encoding="utf-8")
        stage_next.stage_source_build(stage, "linux-arm", output)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--work-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--source-archive", type=Path)
    args = parser.parse_args()
    build(args.work_dir, args.output, args.source_archive)
