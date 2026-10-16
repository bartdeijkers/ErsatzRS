"""Pure receipt qualification for the separate test-only G1 client candidate.

The frozen G1 capture verifier owns the complete build chain and six-mosaic/B
oracle. A positive receipt qualifies only that vector, never full conformance.
Process/library/plugin map qualification remains the existing cold driver seam.
"""

import hashlib
import json
from pathlib import Path
import re
import types

G1_PROOF_SHA256 = "597769f27264aa015450563d31dbe94edbbcc0a4dd8ac6e0c9729cb1ccdd775c"
MAX_RECEIPT = 65536


def require(condition, message):
    if not condition:
        raise ValueError(message)


def bounded(path):
    require(path.is_file() and not path.is_symlink(), "Expected regular G1 qualification input")
    with path.open("rb") as stream:
        body = stream.read(MAX_RECEIPT + 1)
    require(0 < len(body) <= MAX_RECEIPT, "G1 qualification input exceeds byte bound")
    return body


def g1_verifier():
    path = Path(__file__).with_name("teletext-g1-candidate-decoder.py")
    body = bounded(path)
    require(hashlib.sha256(body).hexdigest() == G1_PROOF_SHA256, "G1 capture verifier source changed")
    module = types.ModuleType("authenticated_g1_capture_verifier")
    module.__file__ = str(path)
    exec(compile(body, str(path), "exec"), module.__dict__)
    return module


def qualify(receipt_path, receipt_sha256, build, proof_input, build_sha256):
    require(not build.is_symlink() and not proof_input.is_symlink(), "Expected real G1 build/input directories")
    build, proof_input = build.resolve(), proof_input.resolve()
    raw = bounded(receipt_path)
    require(isinstance(receipt_sha256, str) and re.fullmatch(r"[0-9a-f]{64}", receipt_sha256)
            and hashlib.sha256(raw).hexdigest() == receipt_sha256,
            "Externally pinned positive G1 proof mismatch")
    verifier = g1_verifier()
    actual = json.loads(raw, object_pairs_hook=unique_object)
    require(type(actual) is dict, "Expected a G1 proof object")
    require(type(actual.get("unit_count")) is int and 0 < actual["unit_count"] <= 512,
            "Invalid positive G1 unit count")
    require(isinstance(actual.get("units_sha256"), str)
            and re.fullmatch(r"[0-9a-f]{64}", actual["units_sha256"]), "Invalid positive G1 units pin")
    build_raw, entry = verifier.verify_build(build, proof_input, build_sha256)
    library = build / entry["library"]
    expected = {
        "schema": 1, "variant": "g1", "decoder_label": verifier.LABEL,
        "decoder_version": "0.2.42", "units_sha256": actual["units_sha256"],
        "unit_count": actual["unit_count"], "source_sha256": verifier.SOURCE_SHA256,
        "recipe_sha256": verifier.RECIPE_SHA256,
        "build_receipt_sha256": hashlib.sha256(build_raw).hexdigest(),
        "library": {"path": str(library), "sha256": entry["library_sha256"],
                    "mapped_file_identity_verified": True},
        "pages": verifier.expected_report(), "candidate_conformance": True,
        "imager_rerun": False, "actual_client_proven": False, "panels_proven": False,
        "full_level25_proven": False, "full_client_acceptance": False,
    }
    # JSON equality distinguishes bool/int and int/float and rejects extra keys.
    require(json.dumps(actual, sort_keys=True) == json.dumps(expected, sort_keys=True),
            "Positive G1 proof does not match the verified fourth-variant candidate")
    return {"path": str(library), "sha256": entry["library_sha256"],
            "label": verifier.LABEL, "test_only": True,
            "positive_proof_sha256": receipt_sha256,
            "g1_build_receipt_sha256": hashlib.sha256(build_raw).hexdigest(),
            "source_verifier_sha256": G1_PROOF_SHA256,
            "candidate_scope": "G1 later-spacing six-mosaic/B MAIN vector on124/125 at1.5/2.5 only",
            "independent_main_columns": 41, "panels_proven": False,
            "full_level25_proven": False, "full_client_acceptance": False,
            "prior_object_provenance_proven": False}


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, "Duplicate G1 proof member")
        result[key] = value
    return result
