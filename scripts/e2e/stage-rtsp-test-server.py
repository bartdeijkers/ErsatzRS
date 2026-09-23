#!/usr/bin/env python3
"""Stage a pinned Linux x64 MediaMTX test server; never a product runtime."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import shutil
import tarfile
import tempfile
import urllib.request

VERSION = "v1.21.1"
ASSET = f"mediamtx_{VERSION}_linux_amd64.tar.gz"
URL = f"https://github.com/bluenviron/mediamtx/releases/download/{VERSION}/{ASSET}"
SHA256 = "653abc672a3e693f8d3b2717752492fdcfb8072291ec108d03d3dd857411b0ee"
MAX_BYTES = 128 * 1024 * 1024


def stage(output):
    if output.exists():
        raise ValueError("Test-server staging destination already exists")
    request = urllib.request.Request(URL, headers={"User-Agent": "ErsatzRS-tests"})
    with urllib.request.urlopen(request, timeout=60) as response:
        payload = response.read(MAX_BYTES + 1)
    if len(payload) > MAX_BYTES or hashlib.sha256(payload).hexdigest() != SHA256:
        raise ValueError("Test-server archive size or SHA-256 mismatch")
    files = {}
    with tarfile.open(fileobj=io.BytesIO(payload), mode="r:gz") as archive:
        for name in ("mediamtx", "LICENSE"):
            matches = [member for member in archive.getmembers() if member.name == name]
            if len(matches) != 1 or not matches[0].isfile() or not 0 < matches[0].size <= MAX_BYTES:
                raise ValueError(f"Missing or invalid test-server archive member: {name}")
            with archive.extractfile(matches[0]) as source:
                files[name] = source.read()
    output.parent.mkdir(parents=True, exist_ok=True)
    pending = Path(tempfile.mkdtemp(prefix=".rtsp-test-", dir=output.parent))
    try:
        for name, content in files.items():
            (pending / name).write_bytes(content)
        (pending / "mediamtx").chmod(0o755)
        origin = {
            "purpose": "isolated RTSP integration test server; not shipped",
            "version": VERSION,
            "platform": "linux-x64",
            "archive_url": URL,
            "archive_sha256": SHA256,
            "binary_sha256": hashlib.sha256(files["mediamtx"]).hexdigest(),
        }
        (pending / "origin.json").write_text(json.dumps(origin, indent=2) + "\n")
        pending.rename(output)
    finally:
        if pending.exists():
            shutil.rmtree(pending)
    print(json.dumps(origin))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    stage(parser.parse_args().output)
