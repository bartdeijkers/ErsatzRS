#!/usr/bin/env python3
"""Stage public ErsatzTV Next artifacts for the optional engine (ADR 0095)."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import re
import shutil
import tarfile
import tempfile
import urllib.request
import zipfile

REPOSITORY = "ErsatzTV/next"
RELEASE_URL = f"https://api.github.com/repos/{REPOSITORY}/releases/tags/develop"
# Update only after source review and runtime acceptance, then rerun candidate CI.
PINNED_REVISION = "4ad282fae7f49d488aa40e5b6bb092dbe9c4018f"
PLATFORMS = {
    "linux-x64": "linux-x64", "linux-arm64": "linux-arm64",
    "linux-arm": "linux-arm", "win-x64": "windows-x64",
    "osx-x64": "macos-x64", "osx-arm64": "macos-arm64",
}
BINARIES = ("ersatztv-channel",)
WINDOWS_COMPANIONS = ("libvpl.dll", "libvpl-LICENSE.txt")
MAX_ARCHIVE = 128 * 1024 * 1024


def download(url, limit):
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
    expected_url = f"https://github.com/{REPOSITORY}/releases/download/develop/{asset['name']}"
    if asset["browser_download_url"] != expected_url:
        raise ValueError("Unexpected Next artifact origin")
    if not 0 < asset["size"] <= MAX_ARCHIVE:
        raise ValueError("Invalid Next artifact size")
    return asset, match[2], asset["name"][:-len(suffix)]


def read_binaries(payload, rid, root):
    names = [n + (".exe" if rid == "win-x64" else "") for n in BINARIES]
    result = {}
    if rid == "win-x64":
        names.extend(WINDOWS_COMPANIONS)
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


def stage(metadata, rid, output, archive_path=None, fetch=download):
    output = Path(output)
    if output.exists():
        raise ValueError("Next staging destination already exists")
    asset, revision, root = select_asset(metadata, rid)
    payload = Path(archive_path).read_bytes() if archive_path else fetch(asset["browser_download_url"], MAX_ARCHIVE)
    digest = hashlib.sha256(payload).hexdigest()
    if len(payload) != asset["size"] or "sha256:" + digest != asset["digest"]:
        raise ValueError("Next artifact size or SHA-256 mismatch")
    binaries = read_binaries(payload, rid, root)
    license_url = f"https://raw.githubusercontent.com/{REPOSITORY}/{PINNED_REVISION}/LICENSE"
    license_data = fetch(license_url, 128 * 1024)
    if not license_data.strip():
        raise ValueError("Next license is empty")
    output.parent.mkdir(parents=True, exist_ok=True)
    temporary = Path(tempfile.mkdtemp(prefix=".next-", dir=output.parent))
    try:
        for name, data in binaries.items():
            path = temporary / name
            path.write_bytes(data)
            path.chmod(0o755)
        (temporary / "LICENSE").write_bytes(license_data)
        (temporary / "origin.json").write_text(json.dumps({
            "repository": REPOSITORY, "release_id": metadata["id"],
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
    args = parser.parse_args()
    rid = args.rid
    if args.docker_arch:
        if args.docker_arch == "arm" and args.docker_variant != "v7":
            parser.error("Only Docker arm/v7 is supported")
        rid = {"amd64": "linux-x64", "arm64": "linux-arm64", "arm": "linux-arm"}[args.docker_arch]
    metadata = json.loads(args.release_metadata.read_text(encoding="utf-8") if args.release_metadata
                          else download(RELEASE_URL, 2 * 1024 * 1024))
    stage(metadata, rid, args.output, args.archive)


if __name__ == "__main__":
    main()
