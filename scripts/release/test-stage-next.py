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
import zipfile

spec = importlib.util.spec_from_file_location("stage_next", Path(__file__).with_name("stage-next.py"))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


def fixture(rid, missing=False, link=False):
    root = f"ersatztv-next-v0.1.0-abcdef1-{module.PLATFORMS[rid]}"
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
    asset = {"name": name, "id": 2, "size": len(data),
             "digest": "sha256:" + hashlib.sha256(data).hexdigest(),
             "browser_download_url": f"https://github.com/ErsatzTV/next/releases/download/develop/{name}"}
    return {"id": 1, "assets": [asset]}, data


class StageNextTests(unittest.TestCase):
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

    def test_six_platforms_stage_exact_binaries_and_provenance(self):
        for rid in module.PLATFORMS:
            with self.subTest(rid=rid), tempfile.TemporaryDirectory() as temp:
                folder = Path(temp)
                output = self.stage(folder, rid)
                suffix = ".exe" if rid == "win-x64" else ""
                self.assertEqual({p.name for p in output.iterdir()},
                                 {"ersatztv-channel" + suffix, "LICENSE", "origin.json"})
                self.assertEqual(json.loads((output / "origin.json").read_text())["rid"], rid)
                self.assertFalse((folder / "outside").exists())
                self.assertTrue((output / ("ersatztv-channel" + suffix)).stat().st_mode & 0o100)

    def test_tampered_artifact_leaves_no_destination(self):
        metadata, data = fixture("linux-x64")
        with tempfile.TemporaryDirectory() as temp:
            folder = Path(temp)
            with self.assertRaisesRegex(ValueError, "SHA-256 mismatch"):
                self.stage(folder, metadata=metadata, data=data + b"changed")
            self.assertFalse((folder / "next").exists())

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

    def test_existing_destination_is_preserved(self):
        with tempfile.TemporaryDirectory() as temp:
            folder = Path(temp)
            (folder / "next").mkdir()
            (folder / "next" / "keep").write_text("existing")
            with self.assertRaisesRegex(ValueError, "already exists"):
                self.stage(folder)
            self.assertEqual((folder / "next" / "keep").read_text(), "existing")


if __name__ == "__main__":
    unittest.main()
