#!/usr/bin/env python3
"""Package a reviewed Next source build with payload and patch provenance."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import tarfile
import tempfile
import zipfile

spec = importlib.util.spec_from_file_location("prepare_next", Path(__file__).with_name("prepare-next-source.py"))
prepare_next = importlib.util.module_from_spec(spec)
spec.loader.exec_module(prepare_next)

TARGETS = {
    "linux-x64": "x86_64-unknown-linux-gnu",
    "linux-arm64": "aarch64-unknown-linux-gnu",
    "linux-arm": "armv7-unknown-linux-gnueabihf",
    "osx-x64": "x86_64-apple-darwin",
    "osx-arm64": "aarch64-apple-darwin",
    "win-x64": "x86_64-pc-windows-msvc",
}
LICENSE_SHA256 = "63e3fc4abdd9da3cb50217660ef8331984ac034330ef48ae6f1cf768dd210a55"
# Default companions from the accepted Windows source distribution. A reviewed
# successor can pin its exact companion hashes in its source manifest.
WINDOWS_COMPANIONS = {
    "libvpl.dll": "ab22b28f67f5b443d0e022b797675314d650669374247766ea91373e2c67a8e7",
    "libvpl-LICENSE.txt": "bf1cfac2e2792b6e1e995ce103d70796aecaf2ec7e4c5fe5474f7acec7b4a677",
}
ARM64_COMPANIONS = {
    "librockchip_mpp.so.1",
    "librockchip_mpp-LICENSE-Apache-2.0.txt",
    "librockchip_mpp-LICENSE-MIT.txt",
    "librockchip_mpp-README.txt",
    "librockchip_mpp-rk_mpi.h",
}


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def package(payload, source_provenance, rid, output, rustc, manifest_path=prepare_next.MANIFEST):
    manifest = prepare_next.reviewed_manifest(manifest_path)
    source = json.loads(Path(source_provenance).read_text(encoding="utf-8"))
    expected = {
        "source_revision": manifest["upstream_revision"],
        "source_archive_sha256": manifest["source_archive_sha256"],
        "cargo_lock_sha256": manifest.get("patched_lock_sha256", manifest["source_lock_sha256"]),
        "distribution_version": manifest["distribution_version"],
        "patches": [{"file": p["file"], "sha256": p["sha256"]} for p in manifest["patches"]],
    }
    if source != expected:
        raise ValueError("Next source provenance differs from the reviewed manifest")
    toolchain = manifest.get("rust_toolchain", "1.97.1")
    if not rustc.startswith(f"rustc {toolchain} "):
        raise ValueError(f"Next source builds require Rust {toolchain}")
    payload, output = Path(payload), Path(output)
    binary = "ersatztv-channel.exe" if rid == "win-x64" else "ersatztv-channel"
    files = [binary, "LICENSE"] + (list(WINDOWS_COMPANIONS) if rid == "win-x64" else [])
    arm64_companions = {}
    if rid == "linux-arm64" and (
        "linux_arm64_companions_sha256" in manifest
        or any(p["file"] == "0004-private-mpp-driver-readiness.patch" for p in manifest["patches"])
    ):
        arm64_companions = manifest.get("linux_arm64_companions_sha256")
        if not isinstance(arm64_companions, dict) or arm64_companions.keys() != ARM64_COMPANIONS:
            raise ValueError("Next ARM64 companions must name exactly the reviewed MPP library and notices")
        files.extend(sorted(arm64_companions))
    for name in files:
        if not (payload / name).is_file() or (payload / name).is_symlink():
            raise ValueError(f"Missing or unsafe Next payload: {name}")
    if sha(payload / "LICENSE") != LICENSE_SHA256:
        raise ValueError("Next license checksum mismatch")
    for name, digest in arm64_companions.items():
        if sha(payload / name) != digest:
            raise ValueError(f"Next ARM64 companion checksum mismatch: {name}")
    if rid == "win-x64":
        companions = manifest.get("windows_companions_sha256", WINDOWS_COMPANIONS)
        if not isinstance(companions, dict) or companions.keys() != WINDOWS_COMPANIONS.keys():
            raise ValueError("Next Windows companions must name exactly the reviewed DLL and license")
        for name, digest in companions.items():
            if sha(payload / name) != digest:
                raise ValueError(f"Next Windows companion checksum mismatch: {name}")
    name = f"ersatzrs-next-{manifest['distribution_version']}-{rid}"
    archive = output / (name + (".zip" if rid == "win-x64" else ".tar.gz"))
    if archive.exists():
        raise ValueError("Next package already exists")
    output.mkdir(parents=True, exist_ok=True)
    origin = {
        "kind": "patched-source-build", "repository": manifest["upstream_repository"],
        **source, "rid": rid, "target": TARGETS[rid], "rustc": rustc,
        "payload_sha256": {name: sha(payload / name) for name in files},
        "signing": "No upstream publisher signature is claimed for the rebuilt executable.",
    }
    with tempfile.TemporaryDirectory(dir=output) as temporary:
        root = Path(temporary) / name
        root.mkdir()
        for file in files:
            shutil.copyfile(payload / file, root / file)
            (root / file).chmod(0o755 if file == binary else 0o644)
        (root / "origin.json").write_text(json.dumps(origin, indent=2) + "\n", encoding="utf-8")
        staged = Path(temporary) / archive.name
        if rid == "win-x64":
            with zipfile.ZipFile(staged, "w", zipfile.ZIP_DEFLATED) as bundle:
                for file in sorted(root.iterdir()):
                    bundle.write(file, arcname=f"{name}/{file.name}")
        else:
            with tarfile.open(staged, "w:gz") as bundle:
                bundle.add(root, arcname=name)
        staged.rename(archive)
    receipt = {"archive": archive.name, "sha256": sha(archive), "origin": origin}
    archive.with_name(archive.name + ".json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    return receipt


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--payload", type=Path, required=True)
    parser.add_argument("--source-provenance", type=Path, required=True)
    parser.add_argument("--rid", choices=TARGETS, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--rustc", required=True, help="Exact rustc --version from the build")
    parser.add_argument("--manifest", type=Path, default=prepare_next.MANIFEST,
                        help="Reviewed source manifest; defaults to the production distribution")
    args = parser.parse_args()
    print(json.dumps(package(args.payload, args.source_provenance, args.rid, args.output, args.rustc, args.manifest), indent=2))
