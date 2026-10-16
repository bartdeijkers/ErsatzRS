"""Pure G1 qualification negatives; no decoder/client/build is executed."""

import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

SCRIPT = Path(__file__).with_name("teletext-client-g1-candidate.py")
SPEC = importlib.util.spec_from_file_location("g1_client_candidate_contract", SCRIPT)
G1 = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(G1)


def sha(body):
    return hashlib.sha256(body).hexdigest()


class CandidateTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.build, self.proof = self.root / "build", self.root / "input"
        self.build.mkdir()
        self.proof.mkdir()
        self.file = self.root / "positive.json"
        entry = {"library": "g1/libzvbi.so.0.13.2", "library_sha256": "a" * 64}
        self.build_raw = b"externally pinned complete G1 build chain"
        # The shared build verifier owns chain validation; this test seam checks
        # its invocation and exact positive-proof association, not fake decoding.
        self.verifier = SimpleNamespace(
            LABEL="test-only-g1-candidate", SOURCE_SHA256={"oracle": "b" * 64}, RECIPE_SHA256="c" * 64,
            verify_build=Mock(return_value=(self.build_raw, entry)),
            expected_report=lambda: [{"page": page, "level": level,
                                      "columns": [8, 9, 12, 13, 20, 21], "later_spacing_separated": True}
                                     for page in [124, 125] for level in ["1.5", "2.5"]],
        )
        self.original = {
            "schema": 1, "variant": "g1", "decoder_label": self.verifier.LABEL,
            "decoder_version": "0.2.42", "units_sha256": "d" * 64, "unit_count": 150,
            "source_sha256": self.verifier.SOURCE_SHA256, "recipe_sha256": self.verifier.RECIPE_SHA256,
            "build_receipt_sha256": sha(self.build_raw),
            "library": {"path": str(self.build / entry["library"]), "sha256": entry["library_sha256"],
                        "mapped_file_identity_verified": True},
            "pages": self.verifier.expected_report(), "candidate_conformance": True,
            "imager_rerun": False, "actual_client_proven": False, "panels_proven": False,
            "full_level25_proven": False, "full_client_acceptance": False,
        }

    def qualify(self, value=None, external=None):
        raw = json.dumps(self.original if value is None else value).encode()
        self.file.write_bytes(raw)
        with patch.object(G1, "g1_verifier", return_value=self.verifier):
            return G1.qualify(self.file, sha(raw) if external is None else external,
                              self.build, self.proof, "e" * 64)

    def test_positive_g1_proof_binds_separate_build_chain_and_scope(self):
        result = self.qualify()
        self.verifier.verify_build.assert_called_once_with(self.build, self.proof, "e" * 64)
        self.assertEqual(result["path"], self.original["library"]["path"])
        self.assertEqual(result["sha256"], self.original["library"]["sha256"])
        self.assertTrue(result["test_only"])
        self.assertFalse(result["full_client_acceptance"])
        self.assertFalse(result["prior_object_provenance_proven"])

    def test_external_positive_receipt_pin_is_required(self):
        for pin in ["f" * 64, "wrong", True]:
            with self.subTest(pin=pin), self.assertRaisesRegex(ValueError, "Externally pinned"):
                self.qualify(external=pin)
        self.verifier.verify_build.assert_not_called()

    def test_unbound_build_or_nonpositive_or_overclaiming_proofs_reject(self):
        changes = [
            {"variant": "sparse"}, {"schema": True}, {"decoder_version": "0.2.45"},
            {"candidate_conformance": False}, {"actual_client_proven": True},
            {"full_client_acceptance": True}, {"extra": "not closed"},
            {"build_receipt_sha256": "f" * 64}, {"recipe_sha256": "f" * 64},
            {"source_sha256": {}}, {"imager_rerun": True},
            {"library": {**self.original["library"], "path": str(self.build / "sparse/libzvbi.so.0.13.2")}},
            {"library": {**self.original["library"], "mapped_file_identity_verified": False}},
            {"pages": self.original["pages"][:-1]},
        ]
        for change in changes:
            with self.subTest(change=change), self.assertRaisesRegex(ValueError, "Positive G1 proof"):
                self.qualify({**copy.deepcopy(self.original), **change})

    def test_both_pages_both_levels_and_normative_later_cells_are_exact(self):
        value = copy.deepcopy(self.original)
        value["pages"][3]["later_spacing_separated"] = False
        with self.assertRaisesRegex(ValueError, "Positive G1 proof"):
            self.qualify(value)
        for key, invalid in [("unit_count", True), ("unit_count", 0), ("unit_count", 513),
                             ("units_sha256", "short")]:
            with self.subTest(key=key, invalid=invalid), self.assertRaisesRegex(ValueError, "Invalid positive G1"):
                self.qualify({**copy.deepcopy(self.original), key: invalid})

    def test_changed_verifier_source_is_never_executed(self):
        with patch.object(G1, "bounded", return_value=b"raise AssertionError('must not execute')"):
            with self.assertRaisesRegex(ValueError, "verifier source changed"):
                G1.g1_verifier()


if __name__ == "__main__":
    unittest.main()
