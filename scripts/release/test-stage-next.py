#!/usr/bin/env python3
"""Offline regressions for Next release staging; no credentials or network."""
import copy
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest
import warnings
import zipfile

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


class StageNextTests(unittest.TestCase):
    def setUp(self):
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
                self.assertTrue((output / ("ersatztv-channel" + suffix)).stat().st_mode & 0o100)

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
            name = "ersatzrs-next-4ad282f-linux-arm.tar.gz"
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


if __name__ == "__main__":
    unittest.main()
