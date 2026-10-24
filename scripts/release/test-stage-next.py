#!/usr/bin/env python3
"""Offline regressions for Next release staging; no credentials or network."""
import copy
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import tarfile
import tempfile
import unittest
import warnings
import zipfile
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("stage_next", Path(__file__).with_name("stage-next.py"))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


def fixture(rid, missing=False, link=False):
    root = f"ersatztv-next-v0.1.0-{module.PINNED_REVISION[:7]}-{module.PLATFORMS[rid]}"
    buffer = io.BytesIO()
    names = ["ersatztv", "ersatztv-channel", "ersatztv-playout-generator"]
    if missing:
        names.remove("ersatztv-channel")
    if rid == "win-x64":
        with zipfile.ZipFile(buffer, "w") as archive:
            for name in names:
                info = zipfile.ZipInfo(f"{root}/{name}.exe")
                info.external_attr = (0o120777 if link else 0o100755) << 16
                archive.writestr(info, b"MZ-fixture")
            archive.writestr(f"{root}/libvpl.dll", b"MZ-oneVPL-fixture")
            archive.writestr(f"{root}/libvpl-LICENSE.txt", b"oneVPL license fixture\n")
            archive.writestr("../../outside", b"must not extract")
        suffix = ".zip"
    else:
        with tarfile.open(fileobj=buffer, mode="w:gz") as archive:
            for name in names:
                data = b"ELF-fixture"
                info = tarfile.TarInfo(f"{root}/{name}")
                if link:
                    info.type = tarfile.SYMTYPE
                    info.linkname = "../../outside"
                else:
                    info.size = len(data)
                archive.addfile(info, io.BytesIO(data))
            info = tarfile.TarInfo("../../outside")
            info.size = 4
            archive.addfile(info, io.BytesIO(b"evil"))
        suffix = ".tar.gz"
    data = buffer.getvalue()
    name = root + suffix
    module.PINNED_HASHES[name] = hashlib.sha256(data).hexdigest()
    asset = {"name": name, "id": 2, "size": len(data),
             "digest": "sha256:" + hashlib.sha256(data).hexdigest(),
             "browser_download_url": f"{module.DOWNLOAD_BASE}/{name}"}
    return {"id": 1, "assets": [asset]}, data


class AcceptedSourceTests(unittest.TestCase):
    def test_active_patch_pin_matches_reviewed_manifest(self):
        if module.PATCHED_DISTRIBUTION_VERSION is None:
            self.skipTest("No active source patch")
        manifest = json.loads(Path(__file__).with_name('next-patches').joinpath('manifest.json').read_text())
        self.assertEqual(module.PINNED_REVISION, manifest['upstream_revision'])
        self.assertEqual(module.SOURCE_ARCHIVE_SHA256, manifest['source_archive_sha256'])
        self.assertEqual(module.SOURCE_LOCK_SHA256, manifest['source_lock_sha256'])
        self.assertEqual(module.PREPARED_LOCK_SHA256,
                         manifest.get('patched_lock_sha256', manifest['source_lock_sha256']))
        self.assertEqual(module.PATCHED_DISTRIBUTION_VERSION, manifest['distribution_version'])
        self.assertEqual(module.PINNED_PATCHES,
                         [{key: item[key] for key in ('file', 'sha256')} for item in manifest['patches']])
        for rid in module.PLATFORMS:
            suffix = '.zip' if rid == 'win-x64' else '.tar.gz'
            name = f'ersatzrs-next-{module.PATCHED_DISTRIBUTION_VERSION}-{rid}{suffix}'
            self.assertRegex(module.PINNED_HASHES[name], r'^[0-9a-f]{64}$')


class StageNextTests(unittest.TestCase):
    def setUp(self):
        # Keep the official-artifact path covered for ADR 0116 patch retirement.
        legacy = copy.deepcopy(module.MANIFEST)
        legacy.pop("linux_arm64_companions_sha256", None)
        legacy.pop("patched_lock_sha256", None)
        legacy["patches"] = []
        for name, value in (("PATCHED_DISTRIBUTION_VERSION", None), ("MANIFEST", legacy),
                            ("PINNED_PATCHES", []), ("PREPARED_LOCK_SHA256", module.SOURCE_LOCK_SHA256)):
            official = patch.object(module, name, value)
            official.start()
            self.addCleanup(official.stop)
        original = module.PINNED_HASHES.copy()
        self.addCleanup(lambda: (module.PINNED_HASHES.clear(), module.PINNED_HASHES.update(original)))
        module.PINNED_HASHES["LICENSE"] = hashlib.sha256(b"MIT License\nSynthetic copyright notice\n").hexdigest()

    def stage(self, folder, rid="linux-x64", metadata=None, data=None, fail_license=False):
        if metadata is None:
            metadata, data = fixture(rid)
        def fetch(url, limit):
            if url.endswith("/LICENSE"):
                if fail_license:
                    raise OSError("license unavailable")
                return b"MIT License\nSynthetic copyright notice\n"
            return data
        output = folder / "next"
        module.stage(metadata, rid, output, fetch=fetch)
        return output

    def test_official_platforms_stage_exact_binaries_and_provenance(self):
        for rid in (rid for rid in module.PLATFORMS if rid != "linux-arm"):
            with self.subTest(rid=rid), tempfile.TemporaryDirectory() as temp:
                folder = Path(temp)
                output = self.stage(folder, rid)
                suffix = ".exe" if rid == "win-x64" else ""
                companions = {"libvpl.dll", "libvpl-LICENSE.txt"} if rid == "win-x64" else set()
                self.assertEqual({p.name for p in output.iterdir()},
                                 {"ersatztv-channel" + suffix, "LICENSE", "origin.json"} | companions)
                if rid == "win-x64":
                    self.assertEqual((output / "libvpl.dll").read_bytes(), b"MZ-oneVPL-fixture")
                    self.assertEqual((output / "libvpl-LICENSE.txt").read_bytes(), b"oneVPL license fixture\n")
                self.assertEqual(json.loads((output / "origin.json").read_text())["rid"], rid)
                self.assertEqual(json.loads((output / "origin.json").read_text())["source_revision"],
                                 module.PINNED_REVISION)
                self.assertFalse((folder / "outside").exists())
                binary = output / ("ersatztv-channel" + suffix)
                self.assertEqual(binary.read_bytes(), b"MZ-fixture" if rid == "win-x64" else b"ELF-fixture")
                # Windows uses executable formats and ACLs, not POSIX execute
                # bits. Retain the owner-execute check on every POSIX host.
                if os.name == "posix":
                    self.assertTrue(binary.stat().st_mode & 0o100)

    def test_tampered_artifact_leaves_no_destination(self):
        metadata, data = fixture("linux-x64")
        with tempfile.TemporaryDirectory() as temp:
            folder = Path(temp)
            with self.assertRaisesRegex(ValueError, "SHA-256 mismatch"):
                self.stage(folder, metadata=metadata, data=data + b"changed")
            self.assertFalse((folder / "next").exists())

    def test_invalid_windows_companions_reject_entire_stage(self):
        for name in ("libvpl.dll", "libvpl-LICENSE.txt"):
            for defect in ("missing", "empty", "duplicate", "link"):
                with self.subTest(name=name, defect=defect), tempfile.TemporaryDirectory() as temp:
                    metadata, data = fixture("win-x64")
                    buffer = io.BytesIO()
                    with zipfile.ZipFile(io.BytesIO(data)) as original, zipfile.ZipFile(buffer, "w") as archive:
                        for info in original.infolist():
                            content = original.read(info)
                            if info.filename.endswith("/" + name):
                                if defect == "missing":
                                    continue
                                if defect == "empty":
                                    content = b""
                                if defect == "link":
                                    info.external_attr = 0o120777 << 16
                                if defect == "duplicate":
                                    archive.writestr(info, content)
                            with warnings.catch_warnings():
                                warnings.simplefilter("ignore", UserWarning)
                                archive.writestr(info, content)
                    data = buffer.getvalue()
                    metadata["assets"][0].update(size=len(data), digest="sha256:" + hashlib.sha256(data).hexdigest())
                    module.PINNED_HASHES[metadata["assets"][0]["name"]] = hashlib.sha256(data).hexdigest()
                    folder = Path(temp)
                    with self.assertRaises(ValueError):
                        self.stage(folder, "win-x64", metadata, data)
                    self.assertFalse((folder / "next").exists())
                    self.assertEqual(list(folder.iterdir()), [])

    def test_missing_or_linked_binary_rejected_for_zip_and_tar(self):
        for rid in ("linux-x64", "win-x64"):
            for option in ({"missing": True}, {"link": True}):
                with self.subTest(rid=rid, option=option), tempfile.TemporaryDirectory() as temp:
                    metadata, data = fixture(rid, **option)
                    with self.assertRaises(ValueError):
                        self.stage(Path(temp), rid, metadata, data)
                    self.assertFalse((Path(temp) / "next").exists())

    def test_selection_rejects_missing_ambiguous_or_unverified_artifact(self):
        base, _ = fixture("linux-x64")
        for change in ("missing", "ambiguous", "digest", "origin"):
            metadata = copy.deepcopy(base)
            if change == "missing":
                metadata["assets"] = []
            elif change == "ambiguous":
                metadata["assets"] *= 2
            else:
                metadata["assets"][0]["digest" if change == "digest" else "browser_download_url"] = "invalid"
            with self.subTest(change=change), self.assertRaises(ValueError):
                module.select_asset(metadata, "linux-x64")

    def test_license_failure_does_not_publish_partial_directory(self):
        with tempfile.TemporaryDirectory() as temp:
            folder = Path(temp)
            with self.assertRaises(OSError):
                self.stage(folder, fail_license=True)
            self.assertFalse((folder / "next").exists())

    def test_changed_snapshot_revision_is_rejected_before_download(self):
        metadata, _ = fixture("linux-x64")
        asset = metadata["assets"][0]
        for field in ("name", "browser_download_url"):
            asset[field] = asset[field].replace(module.PINNED_REVISION[:7], "abcdef1")
        with tempfile.TemporaryDirectory() as temp:
            output = Path(temp) / "next"
            def unexpected_download(url, limit):
                self.fail("Unreviewed Next artifacts must not be downloaded")
            with self.assertRaisesRegex(ValueError, "differs from accepted pin"):
                module.stage(metadata, "linux-x64", output, fetch=unexpected_download)
            self.assertFalse(output.exists())

    def test_replaced_asset_and_matching_metadata_are_rejected(self):
        metadata, data = fixture("linux-x64")
        metadata["assets"][0]["digest"] = "sha256:" + hashlib.sha256(data + b"replacement").hexdigest()
        with self.assertRaisesRegex(ValueError, "pinned archive"):
            module.select_asset(metadata, "linux-x64")

    def test_arm_snapshot_rejects_tampering_before_extraction(self):
        with tempfile.TemporaryDirectory() as temp:
            output = Path(temp) / "next"
            with self.assertRaisesRegex(ValueError, "snapshot SHA-256"):
                module.stage_arm_snapshot(output, fetch=lambda url, limit: b"tampered")
            self.assertFalse(output.exists())

    def test_arm_snapshot_preserves_source_build_identity(self):
        with tempfile.TemporaryDirectory() as temp:
            folder = Path(temp)
            source, origin = self.source_fixture(folder)
            buffer = io.BytesIO()
            name = f"ersatzrs-next-{module.PINNED_REVISION[:7]}-linux-arm.tar.gz"
            with tarfile.open(fileobj=buffer, mode="w:gz") as archive:
                for path in source.iterdir():
                    archive.add(path, arcname=name[:-7] + "/" + path.name)
            payload = buffer.getvalue()
            module.PINNED_HASHES[name] = hashlib.sha256(payload).hexdigest()
            module.stage_arm_snapshot(folder / "next", fetch=lambda url, limit: payload)
            self.assertEqual(json.loads((folder / "next/origin.json").read_text()), origin)

    def test_existing_destination_is_preserved(self):
        with tempfile.TemporaryDirectory() as temp:
            folder = Path(temp)
            (folder / "next").mkdir()
            (folder / "next" / "keep").write_text("existing")
            with self.assertRaisesRegex(ValueError, "already exists"):
                self.stage(folder)
            self.assertEqual((folder / "next" / "keep").read_text(), "existing")

    def source_fixture(self, folder):
        source = folder / "source"
        source.mkdir()
        binary = b"\x7fELF\x01\x01" + bytes(12) + b"\x28\x00" + b"synthetic executable"
        license_data = b"MIT fixture\n"
        (source / "ersatztv-channel").write_bytes(binary)
        (source / "LICENSE").write_bytes(license_data)
        origin = {
            "kind": "source-build", "repository": module.REPOSITORY,
            "source_revision": module.PINNED_REVISION, "rid": "linux-arm",
            "target": "armv7-unknown-linux-gnueabihf",
            "source_archive_sha256": module.SOURCE_ARCHIVE_SHA256,
            "cargo_lock_sha256": module.SOURCE_LOCK_SHA256,
            "binary_sha256": hashlib.sha256(binary).hexdigest(),
            "license_sha256": hashlib.sha256(license_data).hexdigest(),
        }
        (source / "origin.json").write_text(json.dumps(origin))
        return source, origin

    def test_source_build_keeps_provenance_and_executable(self):
        with tempfile.TemporaryDirectory() as temp:
            folder = Path(temp)
            source, _ = self.source_fixture(folder)
            module.stage_source_build(source, "linux-arm", folder / "next")
            for name in ("ersatztv-channel", "LICENSE", "origin.json"):
                self.assertEqual((source / name).read_bytes(), (folder / "next" / name).read_bytes())
            # The byte/provenance checks above apply on every host; Windows
            # does not expose POSIX executable permissions for this ARM file.
            if os.name == "posix":
                self.assertTrue((folder / "next/ersatztv-channel").stat().st_mode & 0o111)

    def test_source_build_rejects_changed_origin_or_payload_atomically(self):
        for changed in ("source_revision", "source_archive_sha256", "cargo_lock_sha256",
                        "binary_sha256", "license_sha256", "target"):
            with self.subTest(changed=changed), tempfile.TemporaryDirectory() as temp:
                folder = Path(temp)
                source, origin = self.source_fixture(folder)
                origin[changed] = "unaccepted"
                (source / "origin.json").write_text(json.dumps(origin))
                with self.assertRaises(ValueError):
                    module.stage_source_build(source, "linux-arm", folder / "next")
                self.assertFalse((folder / "next").exists())

    def test_source_build_rejects_wrong_elf_even_with_matching_checksum(self):
        with tempfile.TemporaryDirectory() as temp:
            folder = Path(temp)
            source, origin = self.source_fixture(folder)
            wrong = b"MZ" + bytes(30)
            (source / "ersatztv-channel").write_bytes(wrong)
            origin["binary_sha256"] = hashlib.sha256(wrong).hexdigest()
            (source / "origin.json").write_text(json.dumps(origin))
            with self.assertRaisesRegex(ValueError, "ARM ELF32"):
                module.stage_source_build(source, "linux-arm", folder / "next")

    def test_source_build_cannot_replace_official_platforms_or_existing_output(self):
        with tempfile.TemporaryDirectory() as temp:
            folder = Path(temp)
            source, _ = self.source_fixture(folder)
            with self.assertRaises(ValueError):
                module.stage_source_build(source, "linux-x64", folder / "next")
            (folder / "next").mkdir()
            (folder / "next/keep").write_text("existing")
            with self.assertRaises(ValueError):
                module.stage_source_build(source, "linux-arm", folder / "next")
            self.assertEqual((folder / "next/keep").read_text(), "existing")

    def test_armv7_does_not_claim_an_official_artifact(self):
        with tempfile.TemporaryDirectory() as temp:
            with self.assertRaisesRegex(ValueError, "accepted source build"):
                self.stage(Path(temp), rid="linux-arm")


class PatchedSnapshotTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        values = {
            "PATCHED_DISTRIBUTION_VERSION": "0.1.0-fixture-ersatzrs.1",
            "PINNED_PATCHES": [{"file": "fix.patch", "sha256": "a" * 64}],
            "PINNED_HASHES": {"LICENSE": hashlib.sha256(b"fixture license").hexdigest()},
        }
        for name, value in values.items():
            mock = patch.object(module, name, value)
            mock.start()
            self.addCleanup(mock.stop)
        if "linux_arm64_companions_sha256" in module.MANIFEST:
            self.configure_mpp_review(prepared_lock=module.PREPARED_LOCK_SHA256)

    def archive(self, rid, defect=None):
        binary = "ersatztv-channel.exe" if rid == "win-x64" else "ersatztv-channel"
        files = {binary: b"fixture binary", "LICENSE": b"fixture license"}
        if rid == "linux-arm64":
            files.update(getattr(self, "mpp_files", {}))
        if rid == "win-x64":
            files.update({name: b"fixture companion" for name in module.WINDOWS_COMPANIONS})
        origin = {
            "kind": "patched-source-build", "repository": module.REPOSITORY,
            "source_revision": module.PINNED_REVISION, "rid": rid, "target": module.TARGETS[rid],
            "source_archive_sha256": module.SOURCE_ARCHIVE_SHA256,
            "cargo_lock_sha256": module.PREPARED_LOCK_SHA256,
            "distribution_version": module.PATCHED_DISTRIBUTION_VERSION,
            "patches": module.PINNED_PATCHES,
            "payload_sha256": {name: hashlib.sha256(data).hexdigest() for name, data in files.items()},
            "rustc": f"rustc {module.MANIFEST.get('rust_toolchain', '1.97.1')} (fixture)",
        }
        if defect in ("patches", "source_revision", "cargo_lock_sha256", "rid", "target"):
            origin[defect] = "unreviewed"
        elif defect == "payload":
            files[binary] = b"changed payload"
        elif defect == "license":
            files["LICENSE"] = b"changed license"
            origin["payload_sha256"]["LICENSE"] = hashlib.sha256(files["LICENSE"]).hexdigest()
        elif defect == "missing-companion":
            del files["libvpl.dll"]
        elif defect == "original-lock":
            origin["cargo_lock_sha256"] = module.SOURCE_LOCK_SHA256
        elif defect == "compiler":
            origin["rustc"] = "rustc 1.97.1 (unreviewed)"
        elif defect == "mpp-missing":
            del files[self.mpp_defect]
        elif defect == "mpp-tampered":
            files[self.mpp_defect] = b"changed companion"
        files["origin.json"] = json.dumps(origin).encode()
        prefix = f"ersatzrs-next-{module.PATCHED_DISTRIBUTION_VERSION}-{rid}"
        buffer = io.BytesIO()
        if rid == "win-x64":
            with zipfile.ZipFile(buffer, "w") as bundle:
                for name, data in files.items():
                    info = zipfile.ZipInfo(prefix + "/" + name)
                    info.external_attr = (0o120777 if defect == "link" and name == binary else 0o100755) << 16
                    bundle.writestr(info, data)
        else:
            with tarfile.open(fileobj=buffer, mode="w:gz") as bundle:
                for name, data in files.items():
                    info = tarfile.TarInfo(prefix + "/" + name)
                    info.size = len(data)
                    if defect == "link" and name == binary:
                        info.type = tarfile.SYMTYPE
                        info.linkname = "../../outside"
                    bundle.addfile(info, io.BytesIO(data))
        name = prefix + (".zip" if rid == "win-x64" else ".tar.gz")
        data = buffer.getvalue()
        module.PINNED_HASHES[name] = hashlib.sha256(data).hexdigest()
        return data

    def configure_mpp_review(self, prepared_lock="e" * 64):
        self.mpp_files = {name: ("fixture " + name).encode() for name in module.ARM64_COMPANIONS}
        payload = {"ersatztv-channel": b"fixture binary", "LICENSE": b"fixture license", **self.mpp_files}
        manifest = copy.deepcopy(module.MANIFEST)
        manifest["rust_toolchain"] = "1.99.0"
        manifest["linux_arm64_companions_sha256"] = {
            name: hashlib.sha256(data).hexdigest() for name, data in self.mpp_files.items()
        }
        manifest["runtime_packages"]["linux-arm64"] = {
            "payload_sha256": {name: hashlib.sha256(data).hexdigest() for name, data in payload.items()},
            "payload_bytes": {name: len(data) for name, data in payload.items()},
        }
        patches = [p for p in module.PINNED_PATCHES if p["file"] != "0004-private-mpp-driver-readiness.patch"]
        patches.append({"file": "0004-private-mpp-driver-readiness.patch", "sha256": "b" * 64})
        for name, value in (("MANIFEST", manifest), ("PREPARED_LOCK_SHA256", prepared_lock),
                            ("PINNED_PATCHES", patches)):
            mock = patch.object(module, name, value)
            mock.start()
            self.addCleanup(mock.stop)
        return manifest

    def test_prepared_lock_and_exact_compiler_are_required(self):
        self.configure_mpp_review()
        for defect in ("original-lock", "compiler"):
            with self.subTest(defect=defect):
                data = self.archive("linux-x64", defect)
                with self.assertRaisesRegex(ValueError, "provenance"):
                    module.stage_patched_snapshot("linux-x64", self.root / "next", fetch=lambda url, limit: data)
                self.assertEqual(list(self.root.iterdir()), [])

    def test_private_mpp_stages_exact_library_notices_and_prepared_lock(self):
        self.configure_mpp_review()
        data = self.archive("linux-arm64")
        out = self.root / "next"
        module.stage_patched_snapshot("linux-arm64", out, fetch=lambda url, limit: data)
        self.assertEqual({p.name for p in out.iterdir()},
                         {"ersatztv-channel", "LICENSE", "origin.json"} | module.ARM64_COMPANIONS)
        for name, value in self.mpp_files.items():
            self.assertEqual((out / name).read_bytes(), value)
        origin = json.loads((out / "origin.json").read_text())
        self.assertEqual(origin["cargo_lock_sha256"], "e" * 64)
        self.assertEqual(origin["payload_sha256"],
                         module.MANIFEST["runtime_packages"]["linux-arm64"]["payload_sha256"])

    def test_private_mpp_patch_cannot_omit_or_expand_companion_inventory(self):
        manifest = self.configure_mpp_review()
        data = self.archive("linux-arm64")
        for defect in ("absent", "missing", "extra"):
            with self.subTest(defect=defect):
                changed = copy.deepcopy(manifest)
                if defect == "absent":
                    del changed["linux_arm64_companions_sha256"]
                elif defect == "missing":
                    del changed["linux_arm64_companions_sha256"]["librockchip_mpp.so.1"]
                else:
                    changed["linux_arm64_companions_sha256"]["undeclared.so"] = "f" * 64
                with patch.object(module, "MANIFEST", changed), self.assertRaisesRegex(ValueError, "ARM64"):
                    module.stage_patched_snapshot("linux-arm64", self.root / "next", fetch=lambda url, limit: data)
                self.assertEqual(list(self.root.iterdir()), [])

    def test_every_missing_or_tampered_mpp_companion_is_refused(self):
        self.configure_mpp_review()
        for name in module.ARM64_COMPANIONS:
            for defect in ("mpp-missing", "mpp-tampered"):
                with self.subTest(companion=name, defect=defect):
                    self.mpp_defect = name
                    data = self.archive("linux-arm64", defect)
                    with self.assertRaises(ValueError):
                        module.stage_patched_snapshot("linux-arm64", self.root / "next", fetch=lambda url, limit: data)
                    self.assertEqual(list(self.root.iterdir()), [])

    def test_mpp_payload_hash_size_and_inventory_must_match_manifest(self):
        manifest = self.configure_mpp_review()
        data = self.archive("linux-arm64")
        for defect in ("hash", "size", "boolean-size", "missing-size", "extra-payload"):
            with self.subTest(defect=defect):
                changed = copy.deepcopy(manifest)
                declared = changed["runtime_packages"]["linux-arm64"]
                if defect == "hash":
                    declared["payload_sha256"]["librockchip_mpp.so.1"] = "f" * 64
                    changed["linux_arm64_companions_sha256"]["librockchip_mpp.so.1"] = "f" * 64
                elif defect == "size":
                    declared["payload_bytes"]["librockchip_mpp.so.1"] += 1
                elif defect == "boolean-size":
                    declared["payload_bytes"]["librockchip_mpp.so.1"] = True
                elif defect == "missing-size":
                    del declared["payload_bytes"]["librockchip_mpp.so.1"]
                else:
                    declared["payload_sha256"]["undeclared.so"] = "f" * 64
                    declared["payload_bytes"]["undeclared.so"] = 12
                with patch.object(module, "MANIFEST", changed), self.assertRaisesRegex(ValueError, "ARM64"):
                    module.stage_patched_snapshot("linux-arm64", self.root / "next", fetch=lambda url, limit: data)
                self.assertEqual(list(self.root.iterdir()), [])

    def test_all_six_platforms_preserve_patch_and_payload_identity(self):
        for rid in module.PLATFORMS:
            with self.subTest(rid=rid):
                data = self.archive(rid)
                out = self.root / rid
                module.stage_patched_snapshot(rid, out, fetch=lambda url, limit: data)
                origin = json.loads((out / "origin.json").read_text())
                self.assertEqual(origin["patches"], module.PINNED_PATCHES)
                self.assertEqual(origin["rid"], rid)
                self.assertEqual(origin["distribution_tag"], module.DISTRIBUTION_TAG)
                for name, digest in origin["payload_sha256"].items():
                    self.assertEqual(hashlib.sha256((out / name).read_bytes()).hexdigest(), digest)
                with self.assertRaisesRegex(ValueError, "already exists"):
                    module.stage_patched_snapshot(rid, out, fetch=lambda url, limit: data)

    def test_invalid_archive_or_provenance_never_publishes_partial_stage(self):
        for rid in ("linux-arm", "win-x64"):
            for defect in ("patches", "source_revision", "cargo_lock_sha256", "rid", "target", "payload", "license", "link", "checksum"):
                with self.subTest(rid=rid, defect=defect):
                    data = self.archive(rid, defect)
                    if defect == "checksum":
                        data += b"changed"
                    out = self.root / rid
                    with self.assertRaises(ValueError):
                        module.stage_patched_snapshot(rid, out, fetch=lambda url, limit: data)
                    self.assertEqual(list(self.root.iterdir()), [])

    def test_missing_windows_companion_is_rejected(self):
        data = self.archive("win-x64", "missing-companion")
        with self.assertRaises(ValueError):
            module.stage_patched_snapshot("win-x64", self.root / "next", fetch=lambda url, limit: data)
        self.assertEqual(list(self.root.iterdir()), [])

    def test_patched_pin_cannot_use_unpatched_staging_paths(self):
        with self.assertRaisesRegex(ValueError, "requires the patched snapshot"):
            module.stage({}, "linux-x64", self.root / "next")
        with self.assertRaisesRegex(ValueError, "requires the patched snapshot"):
            module.stage_arm_snapshot(self.root / "next")
        with self.assertRaisesRegex(ValueError, "requires the patched snapshot"):
            module.stage_source_build(self.root, "linux-arm", self.root / "next")


if __name__ == "__main__":
    unittest.main()
