#!/usr/bin/env python3
"""Stage public ErsatzTV Next artifacts for the optional engine (ADR 0095)."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import re
import shutil
import subprocess
import tarfile
import tempfile
import urllib.request
import zipfile

REPOSITORY = "ErsatzTV/next"
DISTRIBUTION_REPOSITORY = "bartdeijkers/ErsatzRS"
DISTRIBUTION_TAG = "next-0362692-ersatzrs.1"
RELEASE_URL = f"https://api.github.com/repos/{DISTRIBUTION_REPOSITORY}/releases/tags/{DISTRIBUTION_TAG}"
DOWNLOAD_BASE = f"https://github.com/{DISTRIBUTION_REPOSITORY}/releases/download/{DISTRIBUTION_TAG}"
# Update only after source review and runtime acceptance, then rerun candidate CI.
PINNED_REVISION = "03626929089a438245cfc77cbedda6e92d98dc9f"
SOURCE_ARCHIVE_SHA256 = "06e4f530f0985a8c8cde4380f9abb43d4352b7ab07917272b33b75394fa891e7"
SOURCE_LOCK_SHA256 = "abf9a7d03c93a4e1602d421f2cad4f1295edadaae9f5e50a13b7b24be0072fd8"
# Set with the accepted six-platform snapshot; None retains official staging.
PATCHED_DISTRIBUTION_VERSION = "0.1.0-0362692-ersatzrs.1"
PINNED_PATCHES = [
    {
        "file": "0001-bound-live-rtsp-shortest-buffer.patch",
        "sha256": "75c2b4d7128e140f65322978c8fc6d144638aee24c22e19512f9b746ba8b88f0"
    },
    {
        "file": "0002-repeat-programme-images.patch",
        "sha256": "b62884eae96f03a2ea14fc25338ec76ccbac0134e16d9babb637bdf6023a3156"
    }
]
TARGETS = {
    "linux-x64": "x86_64-unknown-linux-gnu", "linux-arm64": "aarch64-unknown-linux-gnu",
    "linux-arm": "armv7-unknown-linux-gnueabihf", "win-x64": "x86_64-pc-windows-msvc",
    "osx-x64": "x86_64-apple-darwin", "osx-arm64": "aarch64-apple-darwin",
}
PLATFORMS = {
    "linux-x64": "linux-x64", "linux-arm64": "linux-arm64",
    "linux-arm": "linux-arm", "win-x64": "windows-x64",
    "osx-x64": "macos-x64", "osx-arm64": "macos-arm64",
}
PINNED_HASHES = {
    "LICENSE": "63e3fc4abdd9da3cb50217660ef8331984ac034330ef48ae6f1cf768dd210a55",
    "ersatzrs-next-0.1.0-0362692-ersatzrs.1-linux-arm.tar.gz": "a597770522f383418aa75ec22d9f3f5b93801589e9471951176969ac3f27096e",
    "ersatzrs-next-0.1.0-0362692-ersatzrs.1-linux-arm64.tar.gz": "4a108d1d7d7b08d73cd00cce1753a7929a3d557b453e6e11281c551e2d7db4eb",
    "ersatzrs-next-0.1.0-0362692-ersatzrs.1-linux-x64.tar.gz": "365373706913e099c0609f8e447d041a0f1e9b3deb4556066a4dc91024d5f9b3",
    "ersatzrs-next-0.1.0-0362692-ersatzrs.1-osx-arm64.tar.gz": "36ff3f5fca69bded87759621b192348bd3785a89dde398ff2ded13199e9bcfcf",
    "ersatzrs-next-0.1.0-0362692-ersatzrs.1-osx-x64.tar.gz": "278032d1470f2b891d69bf22e00c601cfcc380f5583f28b198015839142c624d",
    "ersatzrs-next-0.1.0-0362692-ersatzrs.1-win-x64.zip": "4f14f94f8a904a61d3b69f9d50ba072aa54db36939cd8a50a692a2276427611e",
    "provenance.json": "511ec0d2bf8d142433bb5ac4410702e84f0ec4bea3cdbd739b7ae3150bc6bd6a"
}

BINARIES = ("ersatztv-channel",)
WINDOWS_COMPANIONS = ("libvpl.dll", "libvpl-LICENSE.txt")
MAX_ARCHIVE = 128 * 1024 * 1024


def download(url, limit):
    # gh uses the existing host login or CI token without exposing credentials
    # to this script or forwarding authentication across download redirects.
    if shutil.which("gh") and (url == RELEASE_URL or url.startswith(DOWNLOAD_BASE + "/")):
        endpoint = f"repos/{DISTRIBUTION_REPOSITORY}/releases/tags/{DISTRIBUTION_TAG}"
        if url != RELEASE_URL:
            metadata = json.loads(download(RELEASE_URL, 2 * 1024 * 1024))
            matches = [asset for asset in metadata["assets"] if asset["name"] == url.rsplit("/", 1)[1]]
            if len(matches) != 1:
                raise ValueError("Missing or ambiguous snapshot asset")
            endpoint = f"repos/{DISTRIBUTION_REPOSITORY}/releases/assets/{matches[0]['id']}"
        command = ["gh", "api", endpoint]
        if url != RELEASE_URL:
            command += ["-H", "Accept: application/octet-stream"]
        with tempfile.TemporaryFile() as output:
            subprocess.run(command, stdout=output, check=True, timeout=60)
            output.seek(0)
            data = output.read(limit + 1)
        if len(data) > limit:
            raise ValueError("Next download exceeds the size limit")
        return data
    request = urllib.request.Request(url, headers={"User-Agent": "ErsatzRS-release"})
    with urllib.request.urlopen(request, timeout=60) as response:
        data = response.read(limit + 1)
    if len(data) > limit:
        raise ValueError("Next download exceeds the size limit")
    return data


def select_asset(metadata, rid):
    platform = PLATFORMS[rid]
    suffix = ".zip" if rid == "win-x64" else ".tar.gz"
    pattern = re.compile(r"ersatztv-next-(v[^/]+-([0-9a-f]{7,40}))-" +
                         re.escape(platform + suffix))
    matches = [(a, pattern.fullmatch(a["name"])) for a in metadata["assets"]]
    matches = [(a, m) for a, m in matches if m]
    if len(matches) != 1:
        raise ValueError(f"Expected exactly one Next artifact for {rid}")
    asset, match = matches[0]
    if not PINNED_REVISION.startswith(match[2]):
        raise ValueError(f"Next artifact revision differs from accepted pin {PINNED_REVISION}")
    digest = asset.get("digest", "")
    if not re.fullmatch(r"sha256:[0-9a-f]{64}", digest):
        raise ValueError("Next artifact requires a SHA-256 digest")
    if digest != "sha256:" + PINNED_HASHES.get(asset["name"], ""):
        raise ValueError("Next artifact differs from the pinned archive SHA-256")
    expected_url = f"{DOWNLOAD_BASE}/{asset['name']}"
    if asset["browser_download_url"] != expected_url:
        raise ValueError("Unexpected Next artifact origin")
    if not 0 < asset["size"] <= MAX_ARCHIVE:
        raise ValueError("Invalid Next artifact size")
    return asset, match[2], asset["name"][:-len(suffix)]


def read_binaries(payload, rid, root, extra_names=()):
    names = [n + (".exe" if rid == "win-x64" else "") for n in BINARIES]
    result = {}
    if rid == "win-x64":
        names.extend(WINDOWS_COMPANIONS)
    names.extend(extra_names)
    if rid == "win-x64":
        with zipfile.ZipFile(io.BytesIO(payload)) as archive:
            for name in names:
                matches = [i for i in archive.infolist() if i.filename == f"{root}/{name}"]
                if len(matches) != 1 or not 0 < matches[0].file_size <= MAX_ARCHIVE:
                    raise ValueError(f"Missing, duplicate or oversized Next binary: {name}")
                mode = matches[0].external_attr >> 16
                if mode & 0o170000 == 0o120000:
                    raise ValueError("Next binaries must not be symbolic links")
                result[name] = archive.read(matches[0])
    else:
        with tarfile.open(fileobj=io.BytesIO(payload), mode="r:gz") as archive:
            for name in names:
                matches = [i for i in archive.getmembers() if i.name == f"{root}/{name}"]
                if len(matches) != 1 or not matches[0].isfile() or not 0 < matches[0].size <= MAX_ARCHIVE:
                    raise ValueError(f"Missing, duplicate or invalid Next binary: {name}")
                with archive.extractfile(matches[0]) as source:
                    result[name] = source.read()
    # Only exact expected members are read; archive paths are never extracted.
    return result


def verify_patched_payload(files, rid):
    origin = json.loads(files["origin.json"])
    expected = {
        "kind": "patched-source-build", "repository": REPOSITORY,
        "source_revision": PINNED_REVISION, "rid": rid, "target": TARGETS[rid],
        "source_archive_sha256": SOURCE_ARCHIVE_SHA256,
        "cargo_lock_sha256": SOURCE_LOCK_SHA256,
        "distribution_version": PATCHED_DISTRIBUTION_VERSION, "patches": PINNED_PATCHES,
    }
    if any(origin.get(key) != value for key, value in expected.items()):
        raise ValueError("Next source/patch provenance differs from the accepted snapshot")
    payload = {name: data for name, data in files.items() if name != "origin.json"}
    if origin.get("payload_sha256") != {name: hashlib.sha256(data).hexdigest() for name, data in payload.items()}:
        raise ValueError("Next payload SHA-256 mismatch")
    if hashlib.sha256(files["LICENSE"]).hexdigest() != PINNED_HASHES["LICENSE"]:
        raise ValueError("Next license SHA-256 mismatch")
    return origin


def stage_patched_snapshot(rid, output, archive_path=None, fetch=download):
    """Stage the reviewed ADR 0117 distribution without executing or extracting it."""
    if not PATCHED_DISTRIBUTION_VERSION or not PINNED_PATCHES:
        raise ValueError("No patched Next snapshot is accepted")
    output = Path(output)
    if output.exists():
        raise ValueError("Next staging destination already exists")
    root = f"ersatzrs-next-{PATCHED_DISTRIBUTION_VERSION}-{rid}"
    name = root + (".zip" if rid == "win-x64" else ".tar.gz")
    expected_hash = PINNED_HASHES.get(name)
    if not expected_hash:
        raise ValueError("No accepted Next archive for this platform")
    payload = Path(archive_path).read_bytes() if archive_path else fetch(f"{DOWNLOAD_BASE}/{name}", MAX_ARCHIVE)
    if not 0 < len(payload) <= MAX_ARCHIVE or hashlib.sha256(payload).hexdigest() != expected_hash:
        raise ValueError("Next snapshot SHA-256 mismatch")
    files = read_binaries(payload, rid, root, ("LICENSE", "origin.json"))
    origin = verify_patched_payload(files, rid)
    origin.update(distribution_repository=DISTRIBUTION_REPOSITORY,
                  distribution_tag=DISTRIBUTION_TAG, asset=name, sha256=expected_hash)
    files["origin.json"] = (json.dumps(origin, indent=2) + "\n").encode()
    output.parent.mkdir(parents=True, exist_ok=True)
    temporary = Path(tempfile.mkdtemp(prefix=".next-", dir=output.parent))
    try:
        for name, data in files.items():
            path = temporary / name
            path.write_bytes(data)
            path.chmod(0o755 if name in ("ersatztv-channel", "ersatztv-channel.exe") else 0o644)
        temporary.rename(output)
    finally:
        if temporary.exists():
            shutil.rmtree(temporary)
    print(f"Staged patched Next {PATCHED_DISTRIBUTION_VERSION} for {rid}: sha256:{expected_hash}")


def stage(metadata, rid, output, archive_path=None, fetch=download):
    if PATCHED_DISTRIBUTION_VERSION:
        raise ValueError("The accepted Next pin requires the patched snapshot")
    if rid == "linux-arm":
        raise ValueError("ARMv7 requires the accepted source build, not an official artifact")
    output = Path(output)
    if output.exists():
        raise ValueError("Next staging destination already exists")
    asset, revision, root = select_asset(metadata, rid)
    payload = Path(archive_path).read_bytes() if archive_path else fetch(asset["browser_download_url"], MAX_ARCHIVE)
    digest = hashlib.sha256(payload).hexdigest()
    if len(payload) != asset["size"] or "sha256:" + digest != asset["digest"]:
        raise ValueError("Next artifact size or SHA-256 mismatch")
    binaries = read_binaries(payload, rid, root)
    license_url = f"{DOWNLOAD_BASE}/LICENSE"
    license_data = fetch(license_url, 128 * 1024)
    if hashlib.sha256(license_data).hexdigest() != PINNED_HASHES["LICENSE"]:
        raise ValueError("Next license SHA-256 mismatch")
    output.parent.mkdir(parents=True, exist_ok=True)
    temporary = Path(tempfile.mkdtemp(prefix=".next-", dir=output.parent))
    try:
        for name, data in binaries.items():
            path = temporary / name
            path.write_bytes(data)
            path.chmod(0o755)
        (temporary / "LICENSE").write_bytes(license_data)
        (temporary / "origin.json").write_text(json.dumps({
            "repository": REPOSITORY, "distribution_repository": DISTRIBUTION_REPOSITORY,
            "distribution_tag": DISTRIBUTION_TAG, "release_id": metadata["id"],
            "asset_id": asset["id"], "asset": asset["name"], "revision": revision,
            "source_revision": PINNED_REVISION,
            "rid": rid, "sha256": digest, "license_url": license_url,
            "license_sha256": hashlib.sha256(license_data).hexdigest(),
        }, indent=2) + "\n", encoding="utf-8")
        temporary.rename(output)
    finally:
        if temporary.exists():
            shutil.rmtree(temporary)
    print(f"Staged Next {revision} for {rid}: {asset['name']} sha256:{digest}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--rid", choices=PLATFORMS)
    group.add_argument("--docker-arch", choices=("amd64", "arm64", "arm"))
    parser.add_argument("--docker-variant", default="")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--release-metadata", type=Path)
    parser.add_argument("--archive", type=Path)
    parser.add_argument("--snapshot-dir", type=Path, help="Local download of the pinned dependency release")
    parser.add_argument("--source-build", type=Path,
                        help="Verified first-party ARMv7 build directory (ADR 0100)")
    args = parser.parse_args()
    rid = args.rid
    if args.docker_arch:
        if args.docker_arch == "arm" and args.docker_variant != "v7":
            parser.error("Only Docker arm/v7 is supported")
        rid = {"amd64": "linux-x64", "arm64": "linux-arm64", "arm": "linux-arm"}[args.docker_arch]
    if args.snapshot_dir:
        if args.source_build or args.archive or args.release_metadata:
            parser.error("Snapshot directory cannot be combined with other artifact inputs")
        def local_fetch(url, limit):
            path = args.snapshot_dir / url.rsplit("/", 1)[1]
            if not path.is_file() or path.is_symlink() or not 0 < path.stat().st_size <= limit:
                raise ValueError("Missing or invalid snapshot file")
            return path.read_bytes()
        if PATCHED_DISTRIBUTION_VERSION:
            stage_patched_snapshot(rid, args.output, fetch=local_fetch)
        elif rid == "linux-arm":
            stage_arm_snapshot(args.output, fetch=local_fetch)
        else:
            metadata = {"id": None, "assets": [
                {"id": None, "name": name, "digest": "sha256:" + digest,
                 "size": (args.snapshot_dir / name).stat().st_size,
                 "browser_download_url": f"{DOWNLOAD_BASE}/{name}"}
                for name, digest in PINNED_HASHES.items()
                if name.startswith("ersatztv-next-") and (args.snapshot_dir / name).is_file()
            ]}
            stage(metadata, rid, args.output, fetch=local_fetch)
        return
    if PATCHED_DISTRIBUTION_VERSION:
        if args.source_build or args.release_metadata:
            parser.error("Patched snapshots require an accepted archive or snapshot directory")
        stage_patched_snapshot(rid, args.output, args.archive)
        return
    if args.source_build:
        if args.release_metadata or args.archive:
            parser.error("Source builds cannot be combined with official archive inputs")
        stage_source_build(args.source_build, rid, args.output)
        return
    if rid == "linux-arm":
        if args.release_metadata:
            parser.error("ARMv7 snapshot does not use official release metadata")
        stage_arm_snapshot(args.output, args.archive)
        return
    metadata = json.loads(args.release_metadata.read_text(encoding="utf-8") if args.release_metadata
                          else download(RELEASE_URL, 2 * 1024 * 1024))
    stage(metadata, rid, args.output, args.archive)



def stage_arm_snapshot(output, archive_path=None, fetch=download):
    """Stage the exact accepted ARMv7 build, preserving source-build provenance."""
    if Path(output).exists():
        raise ValueError("Next staging destination already exists")
    if PATCHED_DISTRIBUTION_VERSION:
        raise ValueError("The accepted Next pin requires the patched snapshot")
    name = f"ersatzrs-next-{PINNED_REVISION[:7]}-linux-arm.tar.gz"
    payload = Path(archive_path).read_bytes() if archive_path else fetch(f"{DOWNLOAD_BASE}/{name}", MAX_ARCHIVE)
    if hashlib.sha256(payload).hexdigest() != PINNED_HASHES.get(name):
        raise ValueError("ARMv7 snapshot SHA-256 mismatch")
    with tempfile.TemporaryDirectory() as temporary:
        source = Path(temporary)
        with tarfile.open(fileobj=io.BytesIO(payload), mode="r:gz") as archive:
            for member in ("ersatztv-channel", "LICENSE", "origin.json"):
                matches = [i for i in archive.getmembers() if i.name == name[:-7] + "/" + member]
                if len(matches) != 1 or not matches[0].isfile() or not 0 < matches[0].size <= MAX_ARCHIVE:
                    raise ValueError("Invalid ARMv7 snapshot member")
                with archive.extractfile(matches[0]) as stream:
                    (source / member).write_bytes(stream.read())
        stage_source_build(source, "linux-arm", output)


def stage_source_build(source, rid, output):
    """Keep the accepted ARMv7 exception explicit and preserve its provenance."""
    if PATCHED_DISTRIBUTION_VERSION:
        raise ValueError("The accepted Next pin requires the patched snapshot")
    source, output = Path(source), Path(output)
    if rid != "linux-arm" or output.exists():
        raise ValueError("Source builds require ARMv7 and a new staging destination")
    origin = json.loads((source / "origin.json").read_text(encoding="utf-8"))
    expected = {
        "kind": "source-build", "repository": REPOSITORY,
        "source_revision": PINNED_REVISION, "rid": "linux-arm",
        "target": "armv7-unknown-linux-gnueabihf",
        "source_archive_sha256": SOURCE_ARCHIVE_SHA256,
        "cargo_lock_sha256": SOURCE_LOCK_SHA256,
    }
    if any(origin.get(key) != value for key, value in expected.items()):
        raise ValueError("ARMv7 source provenance differs from the accepted build")
    for name, key in (("ersatztv-channel", "binary_sha256"), ("LICENSE", "license_sha256")):
        path = source / name
        if path.is_symlink() or not path.is_file() or not 0 < path.stat().st_size <= MAX_ARCHIVE:
            raise ValueError("Missing or invalid ARMv7 build file")
        if hashlib.sha256(path.read_bytes()).hexdigest() != origin.get(key):
            raise ValueError("ARMv7 build file SHA-256 mismatch")
    header = (source / "ersatztv-channel").read_bytes()[:20]
    if len(header) != 20 or header[:6] != b"\x7fELF\x01\x01" or header[18:20] != b"\x28\x00":
        raise ValueError("Next source build is not a little-endian ARM ELF32 executable")
    output.parent.mkdir(parents=True, exist_ok=True)
    temporary = Path(tempfile.mkdtemp(prefix=".next-", dir=output.parent))
    try:
        for name in ("ersatztv-channel", "LICENSE", "origin.json"):
            shutil.copyfile(source / name, temporary / name)
        (temporary / "ersatztv-channel").chmod(0o755)
        temporary.rename(output)
    finally:
        if temporary.exists():
            shutil.rmtree(temporary)
    print(f"Staged source-built Next {PINNED_REVISION} for ARMv7")


if __name__ == "__main__":
    main()
