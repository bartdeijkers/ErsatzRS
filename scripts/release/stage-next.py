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

# Rust startup acquisition and Python release staging share this pin authority.
MANIFEST = json.loads((Path(__file__).resolve().parent / "next-patches" / "manifest.json").read_text())
REPOSITORY = MANIFEST["upstream_repository"]
DISTRIBUTION_REPOSITORY = MANIFEST["distribution_repository"]
DISTRIBUTION_TAG = MANIFEST["distribution_tag"]
RELEASE_URL = f"https://api.github.com/repos/{DISTRIBUTION_REPOSITORY}/releases/tags/{DISTRIBUTION_TAG}"
DOWNLOAD_BASE = f"https://github.com/{DISTRIBUTION_REPOSITORY}/releases/download/{DISTRIBUTION_TAG}"
PINNED_REVISION = MANIFEST["upstream_revision"]
SOURCE_ARCHIVE_SHA256 = MANIFEST["source_archive_sha256"]
SOURCE_LOCK_SHA256 = MANIFEST["source_lock_sha256"]
PREPARED_LOCK_SHA256 = MANIFEST.get("patched_lock_sha256", SOURCE_LOCK_SHA256)
PATCHED_DISTRIBUTION_VERSION = MANIFEST["distribution_version"]
PINNED_PATCHES = [{"file": p["file"], "sha256": p["sha256"]} for p in MANIFEST["patches"]]
TARGETS = {rid: package["target"] for rid, package in MANIFEST["runtime_packages"].items()}
PLATFORMS = {rid: package["platform"] for rid, package in MANIFEST["runtime_packages"].items()}
PINNED_HASHES = MANIFEST["artifact_sha256"]

BINARIES = ("ersatztv-channel",)
WINDOWS_COMPANIONS = ("libvpl.dll", "libvpl-LICENSE.txt")
ARM64_COMPANIONS = {
    "librockchip_mpp.so.1",
    "librockchip_mpp-LICENSE-Apache-2.0.txt",
    "librockchip_mpp-LICENSE-MIT.txt",
    "librockchip_mpp-README.txt",
    "librockchip_mpp-rk_mpi.h",
}
MAX_ARCHIVE = 128 * 1024 * 1024


def distribution_for(rid):
    """Resolve one complete platform identity, including retained releases."""
    package = MANIFEST["runtime_packages"][rid]
    keys = ("distribution_version", "distribution_tag", "patches")
    present = sum(key in package for key in keys)
    if present not in (0, 3):
        raise ValueError("Next platform requires a complete distribution identity")
    version = package.get("distribution_version", PATCHED_DISTRIBUTION_VERSION)
    tag = package.get("distribution_tag", DISTRIBUTION_TAG)
    patches = package.get("patches", PINNED_PATCHES)
    if any(not isinstance(value, str) or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]*", value)
           for value in (version, tag)) or not isinstance(patches, list) or not patches:
        raise ValueError("Invalid Next platform distribution identity")
    return version, tag, [{"file": p["file"], "sha256": p["sha256"]} for p in patches]


def source_for(rid):
    """Resolve the complete original source/compiler identity of retained bytes."""
    source = MANIFEST["runtime_packages"][rid].get("source_identity", {
        "upstream_revision": PINNED_REVISION,
        "source_archive_sha256": SOURCE_ARCHIVE_SHA256,
        "prepared_lock_sha256": PREPARED_LOCK_SHA256,
        "rust_toolchain": MANIFEST.get("rust_toolchain", "1.97.1"),
    })
    if not isinstance(source, dict) or set(source) != {
        "upstream_revision", "source_archive_sha256", "prepared_lock_sha256", "rust_toolchain"
    }:
        raise ValueError("Next platform requires a complete source identity")
    for key, pattern in (("upstream_revision", r"[0-9a-f]{40}"),
                         ("source_archive_sha256", r"[0-9a-f]{64}"),
                         ("prepared_lock_sha256", r"[0-9a-f]{64}"),
                         ("rust_toolchain", r"[0-9]+\.[0-9]+\.[0-9]+")):
        if not isinstance(source[key], str) or not re.fullmatch(pattern, source[key]):
            raise ValueError("Invalid Next platform source identity")
    return source


def package_url(rid):
    version, tag, _ = distribution_for(rid)
    suffix = ".zip" if rid == "win-x64" else ".tar.gz"
    return f"https://github.com/{DISTRIBUTION_REPOSITORY}/releases/download/{tag}/ersatzrs-next-{version}-{rid}{suffix}"


def download(url, limit):
    # gh uses the existing host login or CI token without exposing credentials
    # to this script or forwarding authentication across download redirects.
    releases = {DISTRIBUTION_TAG} | {
        package["distribution_tag"] for package in MANIFEST["runtime_packages"].values()
        if "distribution_tag" in package
    }
    accepted_release = None
    for tag in releases:
        if not isinstance(tag, str) or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]*", tag):
            raise ValueError("Invalid Next distribution tag")
        release_url = f"https://api.github.com/repos/{DISTRIBUTION_REPOSITORY}/releases/tags/{tag}"
        base = f"https://github.com/{DISTRIBUTION_REPOSITORY}/releases/download/{tag}"
        if url == release_url or url.startswith(base + "/"):
            accepted_release = tag, release_url
            break
    if shutil.which("gh") and accepted_release:
        tag, release_url = accepted_release
        endpoint = f"repos/{DISTRIBUTION_REPOSITORY}/releases/tags/{tag}"
        if url != release_url:
            metadata = json.loads(download(release_url, 2 * 1024 * 1024))
            matches = [asset for asset in metadata["assets"] if asset["name"] == url.rsplit("/", 1)[1]]
            if len(matches) != 1:
                raise ValueError("Missing or ambiguous snapshot asset")
            endpoint = f"repos/{DISTRIBUTION_REPOSITORY}/releases/assets/{matches[0]['id']}"
        command = ["gh", "api", endpoint]
        if url != release_url:
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


def arm64_companion_payload(rid):
    required = rid == "linux-arm64" and (
        "linux_arm64_companions_sha256" in MANIFEST
        or any(p["file"] == "0004-private-mpp-driver-readiness.patch"
               for p in MANIFEST["runtime_packages"][rid].get("patches", PINNED_PATCHES))
    )
    if not required:
        return None
    companions = MANIFEST.get("linux_arm64_companions_sha256")
    if not isinstance(companions, dict) or companions.keys() != ARM64_COMPANIONS:
        raise ValueError("Next ARM64 requires exactly the declared MPP library and notices")
    declared = MANIFEST.get("runtime_packages", {}).get(rid, {})
    hashes, sizes = declared.get("payload_sha256", {}), declared.get("payload_bytes", {})
    names = {"ersatztv-channel", "LICENSE"} | ARM64_COMPANIONS
    if not isinstance(hashes, dict) or not isinstance(sizes, dict) or hashes.keys() != names or sizes.keys() != names:
        raise ValueError("Next ARM64 requires exact payload hashes and sizes")
    if any(not isinstance(value, str) or not re.fullmatch(r"[0-9a-f]{64}", value) for value in hashes.values()):
        raise ValueError("Next ARM64 payload requires exact SHA-256 values")
    if any(type(value) is not int or not 0 < value <= MAX_ARCHIVE for value in sizes.values()):
        raise ValueError("Next ARM64 payload requires bounded positive sizes")
    if any(hashes[name] != digest for name, digest in companions.items()):
        raise ValueError("Next ARM64 companion pins differ from the declared payload")
    return declared


def read_binaries(payload, rid, root, extra_names=()):
    names = [n + (".exe" if rid == "win-x64" else "") for n in BINARIES]
    result = {}
    if rid == "win-x64":
        names.extend(WINDOWS_COMPANIONS)
    if arm64_companion_payload(rid) is not None:
        names.extend(sorted(ARM64_COMPANIONS))
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
    version, _, patches = distribution_for(rid)
    source = source_for(rid)
    origin = json.loads(files["origin.json"])
    expected = {
        "kind": "patched-source-build", "repository": REPOSITORY,
        "source_revision": source["upstream_revision"], "rid": rid, "target": TARGETS[rid],
        "source_archive_sha256": source["source_archive_sha256"],
        "cargo_lock_sha256": source["prepared_lock_sha256"],
        "distribution_version": version, "patches": patches,
    }
    if any(origin.get(key) != value for key, value in expected.items()):
        raise ValueError("Next source/patch provenance differs from the accepted snapshot")
    compiler = source["rust_toolchain"]
    if compiler is not None and (
        not isinstance(compiler, str) or not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", compiler)
        or not origin.get("rustc", "").startswith(f"rustc {compiler} ")
    ):
        raise ValueError("Next compiler provenance differs from the accepted snapshot")
    payload = {name: data for name, data in files.items() if name != "origin.json"}
    if origin.get("payload_sha256") != {name: hashlib.sha256(data).hexdigest() for name, data in payload.items()}:
        raise ValueError("Next payload SHA-256 mismatch")
    declared = arm64_companion_payload(rid)
    if declared is not None and (
        origin["payload_sha256"] != declared["payload_sha256"]
        or {name: len(data) for name, data in payload.items()} != declared["payload_bytes"]
    ):
        raise ValueError("Next ARM64 payload differs from declared hashes or sizes")
    if hashlib.sha256(files["LICENSE"]).hexdigest() != PINNED_HASHES["LICENSE"]:
        raise ValueError("Next license SHA-256 mismatch")
    return origin


def stage_patched_snapshot(rid, output, archive_path=None, fetch=download):
    """Stage the reviewed ADR 0117 distribution without executing or extracting it."""
    version, tag, _ = distribution_for(rid)
    output = Path(output)
    if output.exists():
        raise ValueError("Next staging destination already exists")
    root = f"ersatzrs-next-{version}-{rid}"
    name = root + (".zip" if rid == "win-x64" else ".tar.gz")
    expected_hash = PINNED_HASHES.get(name)
    if not expected_hash:
        raise ValueError("No accepted Next archive for this platform")
    payload = Path(archive_path).read_bytes() if archive_path else fetch(package_url(rid), MAX_ARCHIVE)
    if not 0 < len(payload) <= MAX_ARCHIVE or hashlib.sha256(payload).hexdigest() != expected_hash:
        raise ValueError("Next snapshot SHA-256 mismatch")
    files = read_binaries(payload, rid, root, ("LICENSE", "origin.json"))
    origin = verify_patched_payload(files, rid)
    origin.update(distribution_repository=DISTRIBUTION_REPOSITORY,
                  distribution_tag=tag, asset=name, sha256=expected_hash)
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
    print(f"Staged patched Next {version} for {rid}: sha256:{expected_hash}")


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
