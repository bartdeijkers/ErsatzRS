"""Pure positive-receipt and owned-map qualification; no client/decoder run."""

import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock, patch

SCRIPT = Path(__file__).with_name("teletext-client-decoder-candidate.py")
SPEC = importlib.util.spec_from_file_location("client_decoder_candidate_contract", SCRIPT)
CANDIDATE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CANDIDATE)


def sha(body):
    return hashlib.sha256(body).hexdigest()


class CandidateTests(unittest.TestCase):
    def receipt(self, root):
        verifier = CANDIDATE.sparse_verifier()
        build, proof = root / "build", root / "input"
        build.mkdir()
        proof.mkdir()
        library = build / "sparse/libzvbi.so.0.13.2"
        entry = {"library": "sparse/libzvbi.so.0.13.2", "library_sha256": sha(b"candidate")}
        base, sparse = b"base receipt", b"combined receipt"
        verifier.verify_build = Mock(return_value=(base, sparse, {"sparse": entry}))
        report = {"schema": 1, "variant": "sparse", "decoder_label": verifier.LABELS["sparse"],
                  "source": verifier.SOURCE, "recipes": verifier.RECIPES,
                  "build_receipt_sha256": sha(base), "sparse_build_receipt_sha256": sha(sparse),
                  "oracle_sha256": verifier.ORACLE_SHA256, "helper_sha256": verifier.HELPER_SHA256,
                  "units_sha256": verifier.UNITS_SHA256,
                  "loaded_library": {"path": str(library), "sha256": entry["library_sha256"]},
                  "exit_code": 0, "conformance_passed": True, "report": verifier.expected_report(),
                  "decoder_library_test_only": True, "actual_client_proven": False,
                  "panels_proven": False, "full_level25_proven": False}
        return verifier, build, proof, report

    def test_external_positive_and_build_pins_reach_authenticated_verifier(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            verifier, build, proof, report = self.receipt(root)
            file = root / "positive.json"
            raw = json.dumps(report).encode()
            file.write_bytes(raw)
            with patch.object(CANDIDATE, "sparse_verifier", return_value=verifier):
                result = CANDIDATE.qualify(file, sha(raw), build, proof, "a" * 64)
            verifier.verify_build.assert_called_once_with(build, proof, "a" * 64)
            self.assertEqual(result["path"], report["loaded_library"]["path"])
            self.assertEqual(result["sha256"], report["loaded_library"]["sha256"])
            self.assertTrue(result["test_only"])
            self.assertFalse(result["full_client_acceptance"])
            with patch.object(CANDIDATE, "sparse_verifier", return_value=verifier):
                with self.assertRaisesRegex(ValueError, "Externally pinned"):
                    CANDIDATE.qualify(file, "0" * 64, build, proof, "a" * 64)

    def test_negative_or_mismatched_positive_receipt_cannot_qualify(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            verifier, build, proof, original = self.receipt(root)
            changes = [
                {"variant": "corrected"}, {"exit_code": 1}, {"conformance_passed": False},
                {"schema": True}, {"actual_client_proven": True}, {"extra": "not closed"},
                {"sparse_build_receipt_sha256": "b" * 64},
                {"loaded_library": {"path": str(build / "original/libzvbi.so.0.13.2"), "sha256": "c" * 64}},
                {"report": {**original["report"], "pages": []}},
            ]
            file = root / "positive.json"
            for changeset in changes:
                with self.subTest(changes=changeset):
                    report = {**copy.deepcopy(original), **changeset}
                    raw = json.dumps(report).encode()
                    file.write_bytes(raw)
                    with patch.object(CANDIDATE, "sparse_verifier", return_value=verifier):
                        with self.assertRaisesRegex(ValueError, "Positive sparse proof"):
                            CANDIDATE.qualify(file, sha(raw), build, proof, "a" * 64)

    def test_changed_verifier_source_is_not_executed(self):
        with patch.object(CANDIDATE, "bounded", return_value=b"raise AssertionError('must not execute')"):
            with self.assertRaisesRegex(ValueError, "verifier source changed"):
                CANDIDATE.sparse_verifier()


class MappingTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="decoder mapping ")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.library = self.root / "candidate/libzvbi.so.0.13.2"
        self.plugin = self.root / "plugins/libzvbi_plugin.so"
        for path, body in [(self.library, b"qualified library"), (self.plugin, b"stock plugin")]:
            path.parent.mkdir(parents=True)
            path.write_bytes(body)
        self.decoder = {"path": str(self.library), "sha256": CANDIDATE.digest(self.library),
                        "label": "test-corrected", "test_only": True}
        self.plugin_hash = CANDIDATE.digest(self.plugin)
        self.proc = self.root / "proc"
        self.maps = self.proc / "123/maps"
        self.maps.parent.mkdir(parents=True)
        self.lines = [self.line(self.library), self.line(self.plugin)]
        self.write()

    @staticmethod
    def line(path):
        stat = path.stat()
        device = f"{os.major(stat.st_dev):02x}:{os.minor(stat.st_dev):02x}"
        encoded = str(path).replace(" ", r"\040")
        return f"1000-2000 r-xp 00000000 {device} {stat.st_ino} {encoded}\n"

    def write(self):
        self.maps.write_text("".join(self.lines))

    def verify(self, decoder=None):
        return CANDIDATE.verify_process(123, self.decoder if decoder is None else decoder,
                                        self.plugin, self.plugin_hash, proc_root=self.proc)

    def test_exact_library_plugin_paths_hashes_and_inodes_pass(self):
        self.assertTrue(self.verify()["exact_mapped_paths_and_file_identities_verified"])
        # The same function enforces stock mode: an inherited alternate library
        # cannot masquerade as the expected stock path, even with a matching hash.
        stock = {**self.decoder, "path": str(self.root / "stock/libzvbi.so.0.13.2"), "test_only": False}
        with self.assertRaisesRegex(ValueError, "exactly the qualified decoder"):
            self.verify(stock)

    def test_missing_or_additional_decoder_and_plugin_are_rejected(self):
        alternate = self.root / "other/libzvbi.so.0.13.2"
        other_plugin = self.root / "other/libzvbi_plugin.so"
        alternate.parent.mkdir()
        alternate.write_bytes(b"other")
        other_plugin.write_bytes(b"other plugin")
        cases = [[], [self.lines[0]], self.lines + [self.line(alternate)],
                 self.lines + [self.line(other_plugin)]]
        for lines in cases:
            with self.subTest(lines=lines):
                self.lines = lines
                self.write()
                with self.assertRaises(ValueError):
                    self.verify()

    def test_content_or_mapped_inode_changes_are_rejected(self):
        original = self.library.read_bytes()
        self.library.write_bytes(original + b"changed")
        with self.assertRaisesRegex(ValueError, "decoder content changed"):
            self.verify()
        self.library.write_bytes(original)
        self.lines[0] = self.lines[0].replace(f" {self.library.stat().st_ino} ", " 1 ")
        self.write()
        with self.assertRaisesRegex(ValueError, "file identity changed"):
            self.verify()

    def test_plugin_content_change_is_rejected(self):
        self.plugin.write_bytes(b"changed plugin")
        with self.assertRaisesRegex(ValueError, "plugin content changed"):
            self.verify()


if __name__ == "__main__":
    unittest.main()
