#!/usr/bin/env python3
"""Regressions for source identity and mandatory temporary-patch review."""
import importlib.util
import json
from pathlib import Path
import shutil
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("prepare_next", Path(__file__).with_name("prepare-next-source.py"))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class SourceReviewTests(unittest.TestCase):
    def test_current_patch_is_verified(self):
        value = module.reviewed_manifest()
        self.assertTrue(value["patches"])
        self.assertEqual(value["adr"], "0117")

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


if __name__ == "__main__":
    unittest.main()
