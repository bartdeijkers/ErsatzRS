"""Stage the Linux worker from this CI run for application runtime checks."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path


def stage_candidate(manifest_path, packages, output):
    spec = importlib.util.spec_from_file_location("stage_next", Path(__file__).with_name("stage-next.py"))
    stage = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(stage)
    review = json.loads(manifest_path.read_text())
    rid = "linux-x64"
    root = f"ersatzrs-next-{review['distribution_version']}-{rid}"
    name = root + ".tar.gz"
    receipt = json.loads((packages / (name + ".json")).read_text())
    data = (packages / name).read_bytes()
    if receipt["archive"] != name or hashlib.sha256(data).hexdigest() != receipt["sha256"]:
        raise ValueError("candidate archive differs from the package receipt")
    files = stage.read_binaries(data, rid, root, ("origin.json", "LICENSE"))
    origin = json.loads(files.pop("origin.json"))
    expected = {
        "repository": review["upstream_repository"], "rid": rid, "target": "x86_64-unknown-linux-gnu",
        "source_revision": review["upstream_revision"], "source_archive_sha256": review["source_archive_sha256"],
        "distribution_version": review["distribution_version"], "cargo_lock_sha256": review["patched_lock_sha256"],
        "patches": [{k: patch[k] for k in ("file", "sha256")} for patch in review["patches"]],
        "payload_sha256": {k: hashlib.sha256(v).hexdigest() for k, v in files.items()},
    }
    if origin != receipt["origin"] or any(origin[key] != value for key, value in expected.items()):
        raise ValueError("candidate provenance or payload does not match the reviewed source")
    output.mkdir()
    for filename, content in files.items():
        (output / filename).write_bytes(content)
    (output / "ersatztv-channel").chmod(0o755)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("manifest_path", "packages", "output"):
        parser.add_argument(name, type=Path)
    stage_candidate(**vars(parser.parse_args()))
