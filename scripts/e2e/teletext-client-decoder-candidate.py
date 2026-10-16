"""Authenticate a positive sparse decoder candidate and owned VLC mappings.

Pure file/receipt checks only: no decoder loading, process creation or GUI work.
The stock path remains a distinct qualification from a test-corrected library.
"""

import hashlib
import json
import os
from pathlib import Path
import re
import types

SPARSE_PROOF_SHA256 = "721ccf8702bc86be049b184142ff1ee577371a776920415f0054db3a7195d174"
MAX_RECEIPT = 65536
MAX_FILE = 16 * 1024 * 1024


def require(condition, message):
    if not condition:
        raise ValueError(message)


def bounded(path, maximum=MAX_FILE):
    require(path.is_file() and not path.is_symlink(), "Expected regular decoder qualification input")
    with path.open("rb") as stream:
        body = stream.read(maximum + 1)
    require(0 < len(body) <= maximum, "Decoder qualification input exceeds byte bound")
    return body


def digest(path):
    return hashlib.sha256(bounded(path)).hexdigest()


def sparse_verifier():
    path = Path(__file__).with_name("teletext-zvbi-sparse-proof.py")
    body = bounded(path, MAX_RECEIPT)
    require(hashlib.sha256(body).hexdigest() == SPARSE_PROOF_SHA256, "Sparse proof verifier source changed")
    module = types.ModuleType("authenticated_sparse_decoder_verifier")
    module.__file__ = str(path)
    exec(compile(body, str(path), "exec"), module.__dict__)
    return module


def qualify(receipt_path, receipt_sha256, build, proof_input, build_sha256):
    verifier = sparse_verifier()
    require(not build.is_symlink() and not proof_input.is_symlink(), "Expected real decoder build/input directories")
    build, proof_input = build.resolve(), proof_input.resolve()
    raw = bounded(receipt_path, MAX_RECEIPT)
    require(verifier.hash_shape(receipt_sha256) and hashlib.sha256(raw).hexdigest() == receipt_sha256,
            "Externally pinned positive sparse proof mismatch")
    base_raw, sparse_raw, variants = verifier.verify_build(build, proof_input, build_sha256)
    entry = variants["sparse"]
    library = build / entry["library"]
    expected = {
        "schema": 1, "variant": "sparse", "decoder_label": verifier.LABELS["sparse"],
        "source": verifier.SOURCE, "recipes": verifier.RECIPES,
        "build_receipt_sha256": hashlib.sha256(base_raw).hexdigest(),
        "sparse_build_receipt_sha256": hashlib.sha256(sparse_raw).hexdigest(),
        "oracle_sha256": verifier.ORACLE_SHA256, "helper_sha256": verifier.HELPER_SHA256,
        "units_sha256": verifier.UNITS_SHA256,
        "loaded_library": {"path": str(library), "sha256": entry["library_sha256"]},
        "exit_code": 0, "conformance_passed": True, "report": verifier.expected_report(),
        "decoder_library_test_only": True, "actual_client_proven": False,
        "panels_proven": False, "full_level25_proven": False,
    }
    actual = verifier.closed_json(raw)
    # Canonical JSON comparison also distinguishes bool/int and int/float;
    # ordinary Python dictionary equality would accept True in an integer slot.
    require(json.dumps(actual, sort_keys=True) == json.dumps(expected, sort_keys=True),
            "Positive sparse proof does not match the verified combined candidate")
    return {"path": str(library), "sha256": entry["library_sha256"],
            "label": verifier.LABELS["sparse"], "test_only": True,
            "positive_proof_sha256": receipt_sha256,
            "sparse_build_receipt_sha256": hashlib.sha256(sparse_raw).hexdigest(),
            "source_verifier_sha256": SPARSE_PROOF_SHA256,
            "independent_main_columns": 41, "panels_proven": False,
            "full_level25_proven": False, "full_client_acceptance": False}


def verify_process(pid, decoder, plugin, plugin_sha256, proc_root=Path("/proc")):
    require(type(pid) is int and pid > 0, "Expected an owned positive client PID")
    maps = bounded(proc_root / str(pid) / "maps", 4 * 1024 * 1024).decode()
    libraries, plugins, records = set(), set(), []
    for line in maps.splitlines():
        fields = line.split(maxsplit=5)
        if len(fields) != 6:
            continue
        path = re.sub(r"\\([0-7]{3})", lambda match: chr(int(match[1], 8)), fields[5])
        if "/libzvbi.so." in path:
            libraries.add(path)
            records.append((path, fields[3], fields[4]))
        elif "/libzvbi_plugin.so" in path:
            plugins.add(path)
            records.append((path, fields[3], fields[4]))
    require(libraries == {decoder["path"]}, "Owned client did not map exactly the qualified decoder")
    require(plugins == {str(plugin)}, "Owned client did not map exactly the receipted VLC decoder plugin")
    for path, device, inode in records:
        stat = Path(path).stat()
        expected_device = tuple(int(part, 16) for part in device.split(":"))
        require(expected_device == (os.major(stat.st_dev), os.minor(stat.st_dev))
                and int(inode) == stat.st_ino, "Mapped decoder/plugin file identity changed")
    require(digest(Path(decoder["path"])) == decoder["sha256"], "Mapped decoder content changed")
    require(digest(plugin) == plugin_sha256, "Mapped VLC decoder plugin content changed")
    return {"client_pid": pid, "decoder": dict(decoder),
            "plugin": {"path": str(plugin), "sha256": plugin_sha256},
            "exact_mapped_paths_and_file_identities_verified": True}
