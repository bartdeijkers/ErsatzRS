#!/usr/bin/env python3
"""Regressions for source identity and mandatory temporary-patch review."""
import importlib.util
import hashlib
import json
from pathlib import Path
import shutil
import tarfile
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("prepare_next", Path(__file__).with_name("prepare-next-source.py"))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class SourceReviewTests(unittest.TestCase):
    def test_current_patch_is_verified(self):
        value = module.reviewed_manifest()
        self.assertTrue(value["patches"])
        self.assertEqual(value["adr"], "0130;0131")

    def test_new_revision_cannot_silently_reuse_patch(self):
        with self.assertRaisesRegex(ValueError, "retirement assessment"):
            module.reviewed_manifest(revision="0" * 40)

    def test_changed_source_pin_requires_a_new_review(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder) / "patches"
            shutil.copytree(module.MANIFEST.parent, root)
            path = root / "manifest.json"
            value = json.loads(path.read_text())
            value["upstream_revision"] = "0" * 40
            path.write_text(json.dumps(value))
            with self.assertRaisesRegex(ValueError, "not been reviewed"):
                module.reviewed_manifest(path)

    def test_changed_patch_is_rejected(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder) / "patches"
            shutil.copytree(module.MANIFEST.parent, root)
            value = json.loads((root / "manifest.json").read_text())
            (root / value["patches"][0]["file"]).write_text("tampered")
            with self.assertRaisesRegex(ValueError, "SHA-256 mismatch"):
                module.reviewed_manifest(root / "manifest.json")

    def test_compiler_pin_rejects_aliases_partial_versions_and_wrong_types(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder) / "patches"
            shutil.copytree(module.MANIFEST.parent, root)
            path = root / "manifest.json"
            value = json.loads(path.read_text())
            for invalid in ["stable", "1.99", "1.99.0-nightly", None, 199]:
                with self.subTest(toolchain=invalid):
                    value["rust_toolchain"] = invalid
                    path.write_text(json.dumps(value))
                    with self.assertRaisesRegex(ValueError, "exact stable Rust version"):
                        module.reviewed_manifest(path)

    def test_patched_lock_pin_requires_a_complete_hash(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder) / "patches"
            shutil.copytree(module.MANIFEST.parent, root)
            path = root / "manifest.json"
            value = json.loads(path.read_text())
            for invalid in [None, 123, "", "f" * 63, "g" * 64]:
                with self.subTest(lock=invalid):
                    value["patched_lock_sha256"] = invalid
                    path.write_text(json.dumps(value))
                    with self.assertRaisesRegex(ValueError, "exact SHA-256"):
                        module.reviewed_manifest(path)


class PreparedLockTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        upstream = self.root / "upstream"
        upstream.mkdir()
        self.original = b"version = 3\n"
        self.corrected = b"version = 4\n"
        (upstream / "Cargo.lock").write_bytes(self.original)
        self.archive = self.root / "source.tar.gz"
        with tarfile.open(self.archive, "w:gz") as archive:
            archive.add(upstream, arcname="source")
        patch = self.root / "lock.patch"
        patch.write_text("diff --git a/Cargo.lock b/Cargo.lock\n"
                         "--- a/Cargo.lock\n+++ b/Cargo.lock\n"
                         "@@ -1 +1 @@\n-version = 3\n+version = 4\n")
        self.manifest = {
            "upstream_revision": "a" * 40,
            "source_archive_sha256": module.digest(self.archive),
            "source_lock_sha256": hashlib.sha256(self.original).hexdigest(),
            "distribution_version": "fixture",
            "patches": [{"file": patch.name, "sha256": module.digest(patch),
                         "upstream_review_revision": "a" * 40,
                         "retire_when": "Equivalent upstream source passes",
                         "unpatched_regression": "Known vulnerable lock fixture"}],
        }
        self.path = self.root / "manifest.json"

    def prepare(self, name="prepared", unpatched=False):
        self.path.write_text(json.dumps(self.manifest))
        return module.prepare(self.archive, self.root / name, self.path, unpatched=unpatched)

    def test_a_lock_patch_without_an_explicit_pin_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "exact reviewed hash"):
            self.prepare()

    def test_exact_post_lock_is_recorded_and_unpatched_control_retains_upstream_lock(self):
        corrected_hash = hashlib.sha256(self.corrected).hexdigest()
        self.manifest["patched_lock_sha256"] = corrected_hash
        prepared = self.prepare()
        self.assertEqual(prepared["cargo_lock_sha256"], corrected_hash)
        self.assertEqual((self.root / "prepared/Cargo.lock").read_bytes(), self.corrected)
        control = self.prepare("unpatched", unpatched=True)
        self.assertEqual(control["cargo_lock_sha256"], self.manifest["source_lock_sha256"])
        self.assertEqual(control["patches"], [])
        self.assertEqual((self.root / "unpatched/Cargo.lock").read_bytes(), self.original)

    def test_wrong_post_lock_pin_is_rejected(self):
        self.manifest["patched_lock_sha256"] = "0" * 64
        with self.assertRaisesRegex(ValueError, "exact reviewed hash"):
            self.prepare()

    def test_post_lock_cannot_replace_original_archive_lock_verification(self):
        corrected_hash = hashlib.sha256(self.corrected).hexdigest()
        self.manifest["source_lock_sha256"] = corrected_hash
        self.manifest["patched_lock_sha256"] = corrected_hash
        with self.assertRaisesRegex(ValueError, "source Cargo.lock"):
            self.prepare()


if __name__ == "__main__":
    unittest.main()
