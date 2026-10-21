#!/usr/bin/env python3
"""CI artifact handoff regressions; fixtures are never executed."""

import importlib.util
import hashlib
from contextlib import redirect_stdout
import io
import json
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest
from unittest.mock import patch


SPEC = importlib.util.spec_from_file_location(
    "artifacts", Path(__file__).with_name("ci-build-artifacts.py")
)
artifacts = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(artifacts)
SHA = "a" * 40


class BuildArtifacts(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.commands = patch.object(artifacts.subprocess, "check_output", side_effect=(
            lambda command, **_: SHA + "\n" if command[0] == "git" else "rustc fixture\n"
        ))
        self.commands.start()
        self.addCleanup(self.commands.stop)

    def fixture(self, kind="rust"):
        root = self.base / f"input-{kind}"
        root.mkdir()
        for name in artifacts.REQUIRED[kind]:
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b"synthetic CI handoff fixture; never executed")
            if name.startswith("bin/"):
                path.chmod(0o755)
        archive = self.base / f"{kind}.tar.gz"
        artifacts.pack(root, archive, SHA, kind)
        return root, archive

    def test_both_products_roundtrip_with_independent_environment_and_modes(self):
        for kind in ("rust", "web"):
            with self.subTest(kind=kind):
                root, archive = self.fixture(kind)
                output = self.base / f"restored-{kind}"
                env = self.base / f"{kind}.env"
                artifacts.restore(archive, output, SHA, env, kind)
                artifacts.verify(output, SHA, kind)
                self.assertEqual((output / "bin/xtask").read_bytes(), (root / "bin/xtask").read_bytes())
                values = dict(line.split("=", 1) for line in env.read_text().splitlines())
                self.assertEqual("CI_NEXTEST" in values, kind == "rust")
                self.assertEqual("ERSATZRS_E2E_PREBUILT_DIR" in values, kind == "web")

    def test_wrong_source_or_product_refuses_before_exporting_paths(self):
        _, archive = self.fixture()
        for sha, kind in [("b" * 40, "rust"), (SHA, "web")]:
            output = self.base / "refused"
            env = self.base / "refused.env"
            with self.assertRaisesRegex(ValueError, "schema or source SHA"):
                artifacts.restore(archive, output, sha, env, kind)
            self.assertFalse(output.exists())
            self.assertFalse(env.exists())

    def test_corrupt_missing_extra_and_non_executable_products_are_refused(self):
        root, _ = self.fixture()
        product = root / "bin/xtask"
        original = product.read_bytes()
        product.write_bytes(b"corrupt")
        with self.assertRaisesRegex(ValueError, "hash/size"):
            artifacts.verify(root, SHA, "rust")
        product.write_bytes(original)
        product.chmod(0o644)
        with self.assertRaisesRegex(ValueError, "permission"):
            artifacts.verify(root, SHA, "rust")
        product.chmod(0o755)
        extra = root / "unexpected"
        extra.write_text("extra")
        with self.assertRaisesRegex(ValueError, "inventory"):
            artifacts.verify(root, SHA, "rust")
        extra.unlink()
        product.unlink()
        with self.assertRaisesRegex(ValueError, "inventory"):
            artifacts.verify(root, SHA, "rust")

    def test_platform_mismatch_is_not_treated_as_a_cache_miss(self):
        root, _ = self.fixture()
        path = root / "build.json"
        manifest = json.loads(path.read_text())
        manifest["host"]["machine"] = "different-architecture"
        path.write_text(json.dumps(manifest))
        with self.assertRaisesRegex(ValueError, "platform/libc"):
            artifacts.verify(root, SHA, "rust")

    def test_existing_destination_is_preserved(self):
        _, archive = self.fixture()
        output = self.base / "existing"
        output.mkdir()
        marker = output / "keep"
        marker.write_text("keep")
        with self.assertRaisesRegex(ValueError, "already exists"):
            artifacts.restore(archive, output, SHA, None, "rust")
        self.assertEqual(marker.read_text(), "keep")

    def test_browser_wrapper_uses_verified_bundle_without_any_build_tools(self):
        root, _ = self.fixture("web")
        server = root / "bin/ersatzrs-web"
        server.write_text('''#!/usr/bin/env python3
import json, os, time
from pathlib import Path
proof = Path(os.environ['HANDOFF_PROOF'])
ready = proof.with_suffix('.tmp')
ready.write_text(json.dumps({
    'manifest': os.environ['CARGO_MANIFEST_DIR'],
    'site': os.environ['LEPTOS_SITE_ROOT'],
}))
ready.replace(proof)
time.sleep(20)
''')
        (root / "build.json").unlink()
        artifacts.pack(root, self.base / "wrapper.tar.gz", SHA, "web")
        commands = self.base / "commands"
        commands.mkdir()
        for name in ("cargo", "cargo-leptos", "stylance"):
            path = commands / name
            path.write_text("#!/bin/sh\nexit 99\n")
            path.chmod(0o755)
        for name, content in {
            "git": f"#!/bin/sh\nprintf '%s\\n' '{SHA}'\n",
            "sqlite3": "#!/bin/sh\nexit 0\n",
            "curl": '#!/bin/sh\ntest -f "$HANDOFF_PROOF"\n',
            "npx": "#!/bin/sh\nprintf '%s\\n' '{}' > \"$PLAYWRIGHT_JSON_OUTPUT_NAME\"\n",
        }.items():
            path = commands / name
            path.write_text(content)
            path.chmod(0o755)
        # The shell protocol is mocked; this test never binds a network port.
        port = 18489
        proof = self.base / "host-environment.json"
        repo = Path(__file__).resolve().parent.parent
        env = dict(os.environ, PATH=f"{commands}{os.pathsep}{os.environ['PATH']}",
                   ERSATZRS_E2E_PREBUILT_DIR=str(root), ERSATZRS_E2E_PORT=str(port),
                   ERSATZRS_E2E_RESULT_DIR=str(self.base / "results"),
                   ERSATZRS_FFMPEG_DIR=str(self.base / "no-runtime"),
                   ERSATZRS_SKIP_PW_INSTALL="1", HANDOFF_PROOF=str(proof))
        result = subprocess.run(["bash", "scripts/e2e/playwright-e2e.sh", "--project=chromium"],
                                cwd=repo, env=env, capture_output=True, text=True, timeout=30)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        recorded = json.loads(proof.read_text())
        self.assertEqual(recorded["manifest"], str(repo / "crates/ersatzrs-web"))
        self.assertNotEqual(recorded["site"], str(root / "site"))
        artifacts.verify(root, SHA, "web")
        # Corruption must stop before starting another host or invoking tools.
        proof.unlink()
        (root / "site/bundle.css").write_text("corrupt")
        refused = subprocess.run(["bash", "scripts/e2e/playwright-e2e.sh"], cwd=repo,
                                 env=env, capture_output=True, text=True, timeout=15)
        self.assertNotEqual(refused.returncode, 0)
        self.assertFalse(proof.exists())

    def test_unsafe_or_duplicate_tar_entries_never_escape_destination(self):
        for kind in ("parent", "symlink", "duplicate"):
            archive = self.base / "unsafe.tar.gz"
            with tarfile.open(archive, "w:gz") as handle:
                entry = tarfile.TarInfo("../escape" if kind == "parent" else "entry")
                if kind == "symlink":
                    entry.type = tarfile.SYMTYPE
                    entry.linkname = "../escape"
                    handle.addfile(entry)
                else:
                    entry.size = 1
                    handle.addfile(entry, io.BytesIO(b"x"))
                    if kind == "duplicate":
                        entry.name = "./entry"
                        handle.addfile(entry, io.BytesIO(b"y"))
            output = self.base / "unsafe-output"
            with self.assertRaises(ValueError):
                artifacts.restore(archive, output, SHA, None, "rust")
            self.assertFalse(output.exists())
            self.assertFalse((self.base / "escape").exists())

    def test_local_cache_both_products_fetch_restore_and_verify(self):
        cache = self.base / "cache"
        for kind in ("rust", "web"):
            with self.subTest(kind=kind):
                root, archive = self.fixture(kind)
                expected = hashlib.sha256(archive.read_bytes()).hexdigest()
                key = f"run100-1-{kind}"
                self.assertEqual(artifacts.publish_local(archive, SHA, kind, key, cache), expected)
                fetched = self.base / f"fetched-{kind}.tar.gz"
                self.assertTrue(artifacts.fetch_local(fetched, expected, key, cache))
                self.assertEqual(fetched.read_bytes(), archive.read_bytes())
                output = self.base / f"cached-{kind}"
                env = self.base / f"cached-{kind}.env"
                artifacts.restore(fetched, output, SHA, env, kind)
                artifacts.verify(output, SHA, kind)
                self.assertEqual((output / "bin/xtask").read_bytes(), (root / "bin/xtask").read_bytes())
                values = dict(line.split("=", 1) for line in env.read_text().splitlines())
                self.assertEqual("CI_NEXTEST" in values, kind == "rust")
                self.assertEqual("ERSATZRS_E2E_PREBUILT_DIR" in values, kind == "web")
        self.assertEqual({path.name for path in cache.iterdir()}, {"run100-1"})

    def test_local_cache_miss_preserves_download_fallback_and_destination(self):
        destination = self.base / "download.tar.gz"
        destination.write_bytes(b"existing download; preserve on miss")
        self.assertFalse(artifacts.fetch_local(destination, "c" * 64, "missing-rust", self.base / "cache"))
        self.assertEqual(destination.read_bytes(), b"existing download; preserve on miss")

    def test_local_cache_cli_emits_explicit_hit_or_miss_for_fallback(self):
        _, archive = self.fixture()
        cache = self.base / "cache"
        output = self.base / "step-output"

        def invoke(mode, path, key, expected=None):
            arguments = ["ci-build-artifacts.py", mode, "--archive", str(path),
                         "--sha", SHA, "--kind", "rust", "--cache-key", key,
                         "--output-file", str(output)]
            if expected:
                arguments.extend(["--expected-digest", expected])
            printed = io.StringIO()
            with patch("sys.argv", arguments), patch.dict(os.environ, CI_LOCAL_ARTIFACT_CACHE=str(cache)), redirect_stdout(printed):
                artifacts.main()
            return printed.getvalue()

        expected = hashlib.sha256(archive.read_bytes()).hexdigest()
        self.assertEqual(invoke("publish-local", archive, "run100-rust"), f"digest={expected}\n")
        fetched = self.base / "download.tar.gz"
        self.assertEqual(invoke("fetch-local", fetched, "run100-rust", expected), "hit=true\n")
        self.assertEqual(fetched.read_bytes(), archive.read_bytes())
        missing = self.base / "missing-download.tar.gz"
        self.assertEqual(invoke("fetch-local", missing, "missing-rust", expected), "hit=false\n")
        self.assertFalse(missing.exists())
        self.assertEqual(output.read_text().splitlines(), [f"digest={expected}", "hit=true", "hit=false"])

    def test_local_cache_wrong_hash_key_and_corruption_preserve_destination(self):
        _, archive = self.fixture()
        cache = self.base / "cache"
        expected = artifacts.publish_local(archive, SHA, "rust", "run100-rust", cache)
        destination = self.base / "download.tar.gz"
        destination.write_bytes(b"keep")
        with self.assertRaises(ValueError):
            artifacts.fetch_local(destination, "0" * 64, "run100-rust", cache)
        self.assertFalse(artifacts.fetch_local(destination, expected, "other-rust", cache))
        entry = cache / "run100/rust.tar.gz"
        entry.write_bytes(b"corrupt cached archive")
        with self.assertRaises(ValueError):
            artifacts.fetch_local(destination, expected, "run100-rust", cache)
        self.assertEqual(destination.read_bytes(), b"keep")

    def test_local_cache_cannot_bypass_requested_source_validation(self):
        _, archive = self.fixture()
        cache = self.base / "cache"
        expected = artifacts.publish_local(archive, SHA, "rust", "run100-rust", cache)
        fetched = self.base / "fetched.tar.gz"
        self.assertTrue(artifacts.fetch_local(fetched, expected, "run100-rust", cache))
        output, env = self.base / "refused-source", self.base / "refused-source.env"
        with self.assertRaisesRegex(ValueError, "schema or source SHA"):
            artifacts.restore(fetched, output, "b" * 40, env, "rust")
        self.assertFalse(output.exists())
        self.assertFalse(env.exists())

    def test_local_cache_rejects_unsafe_keys_and_invalid_digests(self):
        _, archive = self.fixture()
        cache = self.base / "cache"
        for key in ("../escape-rust", "/escape-rust", "group/path-rust", "-group-rust",
                    "a" * 200 + "-rust", "gr\u00f6up-rust", "group-other"):
            with self.subTest(key=key), self.assertRaises(ValueError):
                artifacts.publish_local(archive, SHA, "rust", key, cache)
            with self.subTest(key=key), self.assertRaises(ValueError):
                artifacts.fetch_local(self.base / "download", "c" * 64, key, cache)
        for bad in ("", "z" * 64, "a" * 63, "A" * 64):
            with self.subTest(digest=bad), self.assertRaises(ValueError):
                artifacts.fetch_local(self.base / "download", bad, "group-rust", cache)
        with self.assertRaises(ValueError):
            artifacts.publish_local(archive, SHA, "web", "group-rust", cache)
        with self.assertRaises(ValueError):
            artifacts.publish_local(archive, "not-a-source-sha", "rust", "group-rust", cache)
        self.assertFalse((self.base / "escape").exists())

    def test_local_cache_reuses_identical_immutable_entry_and_refuses_different_bytes(self):
        _, archive = self.fixture()
        cache = self.base / "cache"
        expected = artifacts.publish_local(archive, SHA, "rust", "run100-rust", cache)
        entry = cache / "run100/rust.tar.gz"
        original, modified = entry.read_bytes(), entry.stat().st_mtime_ns
        self.assertEqual(artifacts.publish_local(archive, SHA, "rust", "run100-rust", cache), expected)
        self.assertEqual(entry.stat().st_mtime_ns, modified)
        archive.write_bytes(b"different producer bytes")
        with self.assertRaises(ValueError):
            artifacts.publish_local(archive, SHA, "rust", "run100-rust", cache)
        self.assertEqual(entry.read_bytes(), original)

    def test_local_cache_symlink_entries_groups_and_roots_are_refused(self):
        _, archive = self.fixture()
        expected = hashlib.sha256(archive.read_bytes()).hexdigest()
        for part in ("entry", "group", "root"):
            with self.subTest(part=part):
                cache = self.base / f"cache-{part}"
                artifacts.publish_local(archive, SHA, "rust", "run100-rust", cache)
                selected = {"entry": cache / "run100/rust.tar.gz", "group": cache / "run100", "root": cache}[part]
                outside = self.base / f"outside-{part}"
                selected.rename(outside)
                selected.symlink_to(outside, target_is_directory=part != "entry")
                destination = self.base / f"download-{part}"
                destination.write_bytes(b"keep")
                with self.assertRaises(ValueError):
                    artifacts.fetch_local(destination, expected, "run100-rust", cache)
                with self.assertRaises(ValueError):
                    artifacts.publish_local(archive, SHA, "rust", "run100-rust", cache)
                self.assertEqual(destination.read_bytes(), b"keep")
        cache = self.base / "cache-destination"
        artifacts.publish_local(archive, SHA, "rust", "run100-rust", cache)
        target = self.base / "destination-target"
        target.write_bytes(b"keep")
        destination = self.base / "destination-symlink"
        destination.symlink_to(target)
        with self.assertRaises(ValueError):
            artifacts.fetch_local(destination, expected, "run100-rust", cache)
        self.assertEqual(target.read_bytes(), b"keep")

    def test_local_cache_interrupted_copy_never_publishes_partial_bytes(self):
        _, archive = self.fixture()
        cache = self.base / "cache"
        expected = hashlib.sha256(archive.read_bytes()).hexdigest()

        def interrupt(_source, destination, *args, **kwargs):
            Path(destination).write_bytes(b"partial")
            if Path(destination).parent == cache / "run100":
                self.assertFalse((cache / "run100/rust.tar.gz").exists())
            else:
                self.assertEqual((self.base / "download.tar.gz").read_bytes(), b"keep")
            raise OSError("synthetic interrupted local cache copy")

        with patch.object(artifacts.shutil, "copyfile", side_effect=interrupt):
            self.assertEqual(artifacts.publish_local(archive, SHA, "rust", "run100-rust", cache), expected)
        self.assertFalse((cache / "run100/rust.tar.gz").exists())
        files = {path.relative_to(cache).as_posix(): path.read_bytes()
                 for path in cache.rglob("*") if path.is_file()}
        self.assertEqual(files, {f"run100/{artifacts.LOCAL_GROUP_MARKER}": artifacts.LOCAL_GROUP_OWNER})
        self.assertFalse(artifacts.fetch_local(self.base / "miss", expected, "run100-rust", cache))
        artifacts.publish_local(archive, SHA, "rust", "run100-rust", cache)
        destination = self.base / "download.tar.gz"
        destination.write_bytes(b"keep")
        before = set(self.base.iterdir())
        with patch.object(artifacts.shutil, "copyfile", side_effect=interrupt):
            self.assertFalse(artifacts.fetch_local(destination, expected, "run100-rust", cache))
        self.assertEqual(destination.read_bytes(), b"keep")
        self.assertEqual(set(self.base.iterdir()), before, "interrupted fetch must remove its temporary file")

    def test_local_cache_retention_keeps_four_producer_groups_and_current_products(self):
        _, rust = self.fixture("rust")
        _, web = self.fixture("web")
        cache = self.base / "cache"
        first = None
        for index in range(5):
            expected = artifacts.publish_local(rust, SHA, "rust", f"run{index}-rust", cache)
            if first is None:
                first = expected
            os.utime(cache / f"run{index}", (1000 + index, 1000 + index))
        web_digest = artifacts.publish_local(web, SHA, "web", "run4-web", cache)
        self.assertEqual(len(list(cache.iterdir())), 4)
        self.assertTrue((cache / "run4/rust.tar.gz").is_file())
        self.assertTrue(artifacts.fetch_local(self.base / "current-web", web_digest, "run4-web", cache))
        destination = self.base / "old-download"
        destination.write_bytes(b"keep")
        self.assertFalse(artifacts.fetch_local(destination, first, "run0-rust", cache))
        self.assertEqual(destination.read_bytes(), b"keep")

    def test_local_cache_retention_and_collision_preserve_foreign_folders(self):
        _, archive = self.fixture()
        cache = self.base / "shared-cache"
        cache.mkdir()
        for name in ("foreign", "foreign-marked", "occupied"):
            group = cache / name
            group.mkdir()
            (group / "keep").write_bytes(f"unrelated {name}".encode())
            if name == "foreign-marked":
                (group / artifacts.LOCAL_GROUP_MARKER).write_bytes(b"another owner's marker\n")
            if name == "occupied":
                # Even byte-identical content does not grant ownership of a
                # pre-existing nonempty directory to this helper.
                (group / "rust.tar.gz").write_bytes(archive.read_bytes())
            os.utime(group, (1, 1))
        before = {path.relative_to(cache): path.read_bytes()
                  for path in cache.rglob("*") if path.is_file()}
        for index in range(5):
            artifacts.publish_local(archive, SHA, "rust", f"run{index}-rust", cache)
            os.utime(cache / f"run{index}", (1000 + index, 1000 + index))
        self.assertEqual({path.name for path in cache.iterdir()},
                         {"foreign", "foreign-marked", "occupied", "run1", "run2", "run3", "run4"})
        for relative, content in before.items():
            self.assertEqual((cache / relative).read_bytes(), content)
        occupied_before = {path.name: path.read_bytes() for path in (cache / "occupied").iterdir()}
        with self.assertRaises(ValueError):
            artifacts.publish_local(archive, SHA, "rust", "occupied-rust", cache)
        self.assertEqual({path.name: path.read_bytes() for path in (cache / "occupied").iterdir()}, occupied_before)


if __name__ == "__main__":
    unittest.main()
