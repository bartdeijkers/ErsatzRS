#!/usr/bin/env python3
"""Offline package provenance, retention and companion regression checks."""
import importlib.util
import json
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("package_next", Path(__file__).with_name("package-next-source.py"))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class PackageTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.payload = self.root / "payload"
        self.payload.mkdir()
        (self.payload / "LICENSE").write_text("fixture license")
        (self.payload / "ersatztv-channel").write_bytes(b"fixture executable")
        self.manifest = module.prepare_next.reviewed_manifest()
        m = self.manifest
        self.source = {
            "source_revision": m["upstream_revision"],
            "source_archive_sha256": m["source_archive_sha256"],
            "cargo_lock_sha256": m["source_lock_sha256"],
            "distribution_version": m["distribution_version"],
            "patches": [{"file": p["file"], "sha256": p["sha256"]} for p in m["patches"]],
        }
        self.provenance = self.root / "source.json"
        self.provenance.write_text(json.dumps(self.source))
        mocked = patch.object(module, "LICENSE_SHA256", module.sha(self.payload / "LICENSE"))
        mocked.start()
        self.addCleanup(mocked.stop)

    def package(self, rid="linux-x64"):
        return module.package(self.payload, self.provenance, rid, self.root / "out", "rustc 1.97.1 (fixture)")

    def test_archive_retains_exact_payload_patch_provenance_and_permissions(self):
        receipt = self.package()
        archive = self.root / "out" / receipt["archive"]
        self.assertEqual(module.sha(archive), receipt["sha256"])
        with tarfile.open(archive) as bundle:
            members = {Path(m.name).name: m for m in bundle if m.isfile()}
            self.assertEqual(set(members), {"ersatztv-channel", "LICENSE", "origin.json"})
            self.assertEqual(members["ersatztv-channel"].mode, 0o755)
            self.assertEqual(bundle.extractfile(members["ersatztv-channel"]).read(), b"fixture executable")
            origin = json.load(bundle.extractfile(members["origin.json"]))
            self.assertEqual(origin, receipt["origin"])
            self.assertEqual(origin["patches"], self.source["patches"])
        with self.assertRaisesRegex(ValueError, "already exists"):
            self.package()

    def test_unpatched_source_cannot_be_labeled_as_patched(self):
        self.source["patches"] = []
        self.provenance.write_text(json.dumps(self.source))
        with self.assertRaisesRegex(ValueError, "provenance differs"):
            self.package()

    def test_tampered_license_is_rejected(self):
        (self.payload / "LICENSE").write_text("changed")
        with self.assertRaisesRegex(ValueError, "license checksum"):
            self.package()

    def test_windows_requires_exact_companions(self):
        (self.payload / "ersatztv-channel.exe").write_bytes(b"fixture executable")
        with self.assertRaisesRegex(ValueError, "Missing or unsafe"):
            self.package("win-x64")
        for name in module.WINDOWS_COMPANIONS:
            (self.payload / name).write_text("wrong companion")
        with self.assertRaisesRegex(ValueError, "companion checksum"):
            self.package("win-x64")


if __name__ == "__main__":
    unittest.main()
