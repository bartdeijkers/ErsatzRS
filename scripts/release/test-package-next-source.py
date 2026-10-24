#!/usr/bin/env python3
"""Offline package provenance, retention and companion regression checks."""
import importlib.util
import hashlib
import json
import shutil
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

    def test_default_compiler_pin_is_not_relaxed(self):
        with self.assertRaisesRegex(ValueError, "require Rust 1.97.1"):
            module.package(self.payload, self.provenance, "linux-x64", self.root / "out",
                           "rustc 1.99.0 (fixture)")

    def test_candidate_compiler_pin_is_exact_and_scoped(self):
        review = self.root / "review"
        shutil.copytree(module.prepare_next.MANIFEST.parent, review)
        path = review / "manifest.json"
        manifest = json.loads(path.read_text())
        manifest["rust_toolchain"] = "1.99.0"
        path.write_text(json.dumps(manifest))
        for invalid in ["rustc 1.97.1 (fixture)", "rustc 1.99.1 (fixture)",
                        "rustc 1.99.0-nightly (fixture)"]:
            with self.subTest(compiler=invalid):
                with self.assertRaisesRegex(ValueError, "require Rust 1.99.0"):
                    module.package(self.payload, self.provenance, "linux-x64",
                                   self.root / "candidate-output", invalid, path)
        receipt = module.package(self.payload, self.provenance, "linux-x64",
                                 self.root / "candidate-output", "rustc 1.99.0 (fixture)", path)
        self.assertEqual(receipt["origin"]["rustc"], "rustc 1.99.0 (fixture)")

    def test_package_requires_the_reviewed_post_patch_lock(self):
        review = self.root / "review"
        shutil.copytree(module.prepare_next.MANIFEST.parent, review)
        path = review / "manifest.json"
        manifest = json.loads(path.read_text())
        manifest["patched_lock_sha256"] = "b" * 64
        path.write_text(json.dumps(manifest))
        with self.assertRaisesRegex(ValueError, "provenance differs"):
            module.package(self.payload, self.provenance, "linux-x64", self.root / "out",
                           "rustc 1.97.1 (fixture)", path)
        self.source["cargo_lock_sha256"] = manifest["patched_lock_sha256"]
        self.provenance.write_text(json.dumps(self.source))
        receipt = module.package(self.payload, self.provenance, "linux-x64", self.root / "out",
                                 "rustc 1.97.1 (fixture)", path)
        self.assertEqual(receipt["origin"]["cargo_lock_sha256"], manifest["patched_lock_sha256"])

    def test_arm64_mpp_requires_complete_exact_companions(self):
        review = self.root / "review"
        shutil.copytree(module.prepare_next.MANIFEST.parent, review)
        path = review / "manifest.json"
        manifest = json.loads(path.read_text())
        patch_file = review / "0004-private-mpp-driver-readiness.patch"
        patch_file.write_bytes(b"fixture MPP source patch")
        manifest["patches"].append({
            "file": patch_file.name, "sha256": module.sha(patch_file),
            "upstream_review_revision": manifest["upstream_revision"],
            "retire_when": "fixture retirement", "unpatched_regression": "fixture refusal",
        })
        self.source["patches"] = [{"file": p["file"], "sha256": p["sha256"]}
                                  for p in manifest["patches"]]
        self.provenance.write_text(json.dumps(self.source))
        output = self.root / "arm64-output"

        def package():
            path.write_text(json.dumps(manifest))
            return module.package(self.payload, self.provenance, "linux-arm64",
                                  output, "rustc 1.97.1 (fixture)", path)

        with self.assertRaisesRegex(ValueError, "name exactly"):
            package()
        companions = {}
        for name in module.ARM64_COMPANIONS:
            (self.payload / name).write_bytes(("fixture " + name).encode())
            companions[name] = module.sha(self.payload / name)
        manifest["linux_arm64_companions_sha256"] = companions
        missing = companions.copy()
        missing.pop("librockchip_mpp-LICENSE-MIT.txt")
        manifest["linux_arm64_companions_sha256"] = missing
        with self.assertRaisesRegex(ValueError, "name exactly"):
            package()
        manifest["linux_arm64_companions_sha256"] = companions
        (self.payload / "librockchip_mpp.so.1").write_bytes(b"tampered companion")
        with self.assertRaisesRegex(ValueError, "companion checksum"):
            package()
        (self.payload / "librockchip_mpp.so.1").write_bytes(b"fixture librockchip_mpp.so.1")
        receipt = package()
        for name, digest in companions.items():
            self.assertEqual(receipt["origin"]["payload_sha256"][name], digest)

    def test_windows_requires_exact_companions(self):
        (self.payload / "ersatztv-channel.exe").write_bytes(b"fixture executable")
        with self.assertRaisesRegex(ValueError, "Missing or unsafe"):
            self.package("win-x64")
        for name in module.WINDOWS_COMPANIONS:
            (self.payload / name).write_text("wrong companion")
        with self.assertRaisesRegex(ValueError, "companion checksum"):
            self.package("win-x64")

    def test_candidate_windows_companions_are_scoped_and_cannot_expand_payload(self):
        review = self.root / "review"
        shutil.copytree(module.prepare_next.MANIFEST.parent, review)
        path = review / "manifest.json"
        manifest = json.loads(path.read_text())
        (self.payload / "ersatztv-channel.exe").write_bytes(b"fixture executable")
        companions = {}
        for name in module.WINDOWS_COMPANIONS:
            data = ("reviewed candidate " + name).encode()
            (self.payload / name).write_bytes(data)
            companions[name] = hashlib.sha256(data).hexdigest()
        manifest["windows_companions_sha256"] = companions
        path.write_text(json.dumps(manifest))
        with self.assertRaisesRegex(ValueError, "companion checksum"):
            self.package("win-x64")  # The production pin still rejects candidate bytes.
        output = self.root / "candidate-output"
        for invalid in [{"libvpl.dll": companions["libvpl.dll"]},
                        {**companions, "unexpected.dll": "0" * 64}]:
            manifest["windows_companions_sha256"] = invalid
            path.write_text(json.dumps(manifest))
            with self.assertRaisesRegex(ValueError, "name exactly"):
                module.package(self.payload, self.provenance, "win-x64", output,
                               "rustc 1.97.1 (fixture)", path)
        manifest["windows_companions_sha256"] = companions
        path.write_text(json.dumps(manifest))
        receipt = module.package(self.payload, self.provenance, "win-x64", output,
                                 "rustc 1.97.1 (fixture)", path)
        for name, digest in companions.items():
            self.assertEqual(receipt["origin"]["payload_sha256"][name], digest)


if __name__ == "__main__":
    unittest.main()
