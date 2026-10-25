#!/usr/bin/env python3
"""CI artifact handoff regressions; fixtures are never executed."""

import importlib.util
import hashlib
from contextlib import redirect_stdout
import io
import json
import os
import re
from types import SimpleNamespace
from pathlib import Path
import subprocess
import sys
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

    def reference(self, root, archive, kind):
        manifest = json.loads((root / "build.json").read_text())
        return {"source_sha": SHA, "cache_key": f"original-producer-{kind}",
                "sha256": artifacts.digest(archive), "toolchain": manifest["toolchain"],
                "build_flags": manifest["build_flags"]}

    def test_reuse_keeps_original_source_and_refuses_corrupt_or_missing_cache(self):
        with patch.dict(os.environ, {key: "0" if key == "CARGO_PROFILE_TEST_DEBUG" else ""
                                     for key in artifacts.BUILD_FLAG_KEYS}):
            root, archive = self.fixture()
        reference = self.reference(root, archive, "rust")
        cache = self.base / "cache"
        artifacts.publish_local(archive, SHA, "rust", reference["cache_key"], cache)
        output = self.base / "copied.tar.gz"
        with patch.object(artifacts, "verify_source_equivalence", return_value=["BUILD.md"]):
            result = artifacts.reuse_local(output, "b" * 40, "rust", reference, cache)
            self.assertEqual(result["rust_source_sha"], SHA)
            self.assertEqual(result["rust_equivalent_execution_sha"], "b" * 40)
            restored = self.base / "reused"
            artifacts.restore(output, restored, "b" * 40, None, "rust", reference)
            self.assertEqual(json.loads((restored / "build.json").read_text())["source_sha"], SHA)
            output.write_bytes(b"corrupt")
            with self.assertRaisesRegex(ValueError, "archive SHA256"):
                artifacts.restore(output, self.base / "refused", "b" * 40, None, "rust", reference)
            self.assertFalse((self.base / "refused").exists())
            entry = artifacts.local_entry(cache, reference["cache_key"])[1]
            entry.write_bytes(b"corrupt")
            with self.assertRaisesRegex(ValueError, "cache hash mismatch"):
                artifacts.reuse_local(output, "b" * 40, "rust", reference, cache)
            entry.unlink()
            with self.assertRaisesRegex(ValueError, "unavailable; no rebuild"):
                artifacts.reuse_local(output, "b" * 40, "rust", reference, cache)

    def test_reuse_requires_pinned_toolchain_flags_and_truthful_web_defaults(self):
        with patch.dict(os.environ, {key: "" for key in artifacts.BUILD_FLAG_KEYS}):
            root, archive = self.fixture("web")
        reference = self.reference(root, archive, "web")
        manifest = json.loads((root / "build.json").read_text())
        cargo = '[package.metadata.leptos]\noutput-name="ersatzrs-web"\nhash-files=false\n'
        with patch.object(artifacts.subprocess, "check_output", side_effect=lambda cmd, **_: cargo if cmd[0] == "git" else "rustc fixture\n"):
            artifacts.verify(root, "b" * 40, "web", reference)
            self.assertTrue(all(value == "" for value in reference["build_flags"].values()))
            for key, value in (("toolchain", "rustc different"), ("build_flags", dict(reference["build_flags"], RUSTFLAGS="unsafe-change"))):
                changed = dict(reference, **{key: value})
                with self.assertRaisesRegex(ValueError, "compiler/build flags"):
                    artifacts.verify(root, "b" * 40, "web", changed)
            with patch.object(artifacts.subprocess, "check_output", return_value="rustc another\n"), \
                 self.assertRaisesRegex(ValueError, "execution toolchain"):
                artifacts.verify(root, "b" * 40, "web", reference)
            with patch.object(artifacts.subprocess, "check_output", side_effect=lambda cmd, **_: cargo.replace("false", "true") if cmd[0] == "git" else "rustc fixture\n"), \
                 self.assertRaisesRegex(ValueError, "Leptos output"):
                artifacts.verify(root, "b" * 40, "web", reference)
            with self.assertRaisesRegex(ValueError, "native/test flags"):
                artifacts.verify_reuse_options(manifest, reference, "rust")

    def test_reuse_reference_rejects_incomplete_wrong_kind_and_unsafe_values(self):
        root, archive = self.fixture()
        reference = self.reference(root, archive, "rust")
        self.assertEqual(artifacts.reuse_reference(json.dumps({"rust": reference}), "rust"), reference)
        bad = [dict(reference, source_sha="main"), dict(reference, sha256="short"),
               dict(reference, cache_key="../escape-rust"), dict(reference, cache_key="wrong-web"),
               dict(reference, toolchain="rustc fixture\nINJECT=1"), dict(reference, build_flags={})]
        for item in bad:
            with self.subTest(reference=item), self.assertRaises(ValueError):
                artifacts.reuse_reference(json.dumps({"rust": item}), "rust")
        with self.assertRaisesRegex(ValueError, "missing required"):
            artifacts.reuse_reference(json.dumps({"rust": reference}), "web")

    def test_source_reuse_allows_only_explicit_ci_docs_and_fetches_only_missing_commit(self):
        for changed in ("crates/ersatzrs-infra/src/lib.rs", "Cargo.lock", ".cargo/config.toml",
                        "tests/e2e/channels.spec.ts", "scripts/release/next-patches/manifest.json",
                        "docs/arbitrary.md"):
            with self.subTest(path=changed), \
                 patch.object(artifacts.subprocess, "run", return_value=SimpleNamespace(returncode=0)), \
                 patch.object(artifacts.subprocess, "check_output", side_effect=[(changed + "\0").encode(), b""]):
                with self.assertRaisesRegex(ValueError, "source inputs changed"):
                    artifacts.verify_source_equivalence(SHA, "b" * 40)
        with patch.object(artifacts.subprocess, "run", return_value=SimpleNamespace(returncode=0)) as command, \
             patch.object(artifacts.subprocess, "check_output", side_effect=[b"BUILD.md\0", b".github/workflows/ci.yml\0"]):
            self.assertEqual(artifacts.verify_source_equivalence(SHA, "b" * 40), [".github/workflows/ci.yml", "BUILD.md"])
            self.assertEqual(command.call_count, 1)
        with patch.object(artifacts.subprocess, "run", side_effect=[SimpleNamespace(returncode=1), SimpleNamespace(returncode=0)]) as command, \
             patch.object(artifacts.subprocess, "check_output", return_value=b""):
            artifacts.verify_source_equivalence(SHA, "b" * 40)
            self.assertEqual(command.call_args_list[1].args[0], ["git", "fetch", "--no-tags", "--depth=1", "origin", SHA])
        with patch.object(artifacts.subprocess, "run", return_value=SimpleNamespace(returncode=0)), \
             patch.object(artifacts.subprocess, "check_output", side_effect=[b"", b"Cargo.toml\0"]), \
             self.assertRaisesRegex(ValueError, "source inputs changed"):
            artifacts.verify_source_equivalence(SHA, SHA)

    def test_source_reuse_rejects_compiled_changelog_even_when_documentation(self):
        # Release notes embed CHANGELOG.md at compile time; documentation is
        # not automatically independent of the server executable.
        for changes in ([b"CHANGELOG.md\0", b""], [b"", b"CHANGELOG.md\0"]):
            with self.subTest(changes=changes), \
                 patch.object(artifacts.subprocess, "run", return_value=SimpleNamespace(returncode=0)), \
                 patch.object(artifacts.subprocess, "check_output", side_effect=changes), \
                 self.assertRaisesRegex(ValueError, "source inputs changed: CHANGELOG.md"):
                artifacts.verify_source_equivalence(SHA, "b" * 40)

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


class FocusedAuditSelection(unittest.TestCase):
    """Evaluate actual job conditions, including implicit successful-needs gates."""

    def setUp(self):
        self.workflow = (Path(__file__).resolve().parents[1] / ".github/workflows/ci.yml").read_text()
        jobs = self.workflow.split("\njobs:\n", 1)[1]
        parts = re.split(r"^  ([a-z_]+):\n", jobs, flags=re.M)
        self.jobs = dict(zip(parts[1::2], parts[2::2]))

    @staticmethod
    def evaluate(expression, inputs, needs, env=None):
        expression = expression.removeprefix("${{").removesuffix("}}").strip()
        expression = re.sub(r"!(?!=)", " not ", expression)
        expression = expression.replace("&&", " and ").replace("||", " or ")
        return bool(eval(expression.strip(), {"__builtins__": {}}, {
            "inputs": SimpleNamespace(**inputs), "needs": needs,
            "github": SimpleNamespace(event_name="workflow_dispatch"),
            "always": lambda: True, "cancelled": lambda: False, "true": True, "false": False,
            "env": env or SimpleNamespace(),
        }))

    def scheduled(self, **selected):
        inputs = dict(audit_only=False, test_only=False, next_engine_only=False,
                      teletext_only="none", browser_only="none", browser_case_set="fixtures", reuse_artifacts="")
        inputs.update(selected)
        scheduled = set()
        for name, body in self.jobs.items():
            condition = re.search(r"^    if: (.+)$", body, re.M)
            condition = condition[1] if condition else "true"
            dependencies = re.search(r"^    needs: (.+)$", body, re.M)
            dependencies = [x.strip() for x in dependencies[1].strip("[]").split(",")] if dependencies else []
            if "always()" not in condition and any(x not in scheduled for x in dependencies):
                continue
            needs = SimpleNamespace(**{
                key: SimpleNamespace(result="success" if key in scheduled else "skipped",
                                     outputs=SimpleNamespace(skip="false"))
                for key in self.jobs
            })
            if self.evaluate(condition, inputs, needs):
                scheduled.add(name)
        return scheduled

    def test_default_manual_run_keeps_every_job(self):
        self.assertEqual(self.scheduled(), set(self.jobs))

    def test_audit_recovery_runs_only_preflight_format_and_audit(self):
        self.assertEqual(self.scheduled(audit_only=True), {"preflight", "fmt", "deny"})

    def test_existing_focused_modes_keep_their_job_ownership(self):
        self.assertEqual(self.scheduled(test_only=True), {"build", "test"})
        self.assertEqual(self.scheduled(next_engine_only=True), {"build", "next_engine"})
        self.assertEqual(self.scheduled(browser_only="both"), {"build", "hydration"})
        for browser in ("chromium", "firefox", "both"):
            self.assertEqual(self.scheduled(browser_only=browser, browser_case_set="channel-preview"),
                             {"build", "hydration"})
        self.assertEqual(self.scheduled(browser_only="firefox", browser_case_set="teletext-1.5"),
                         {"build", "hydration"})
        for browser in ("chromium", "firefox", "both"):
            self.assertEqual(self.scheduled(browser_only=browser, browser_case_set="teletext-runtime"),
                             {"build", "hydration"})
        for level in ("1.5", "2.5", "all"):
            self.assertEqual(self.scheduled(teletext_only=level), {"build", "teletext", "hydration"})

    def test_rust_level15_recovery_excludes_web_build_and_browsers(self):
        self.assertEqual(self.scheduled(teletext_only="1.5-rust"), {"build", "teletext"})
        inputs = dict(test_only=False, next_engine_only=False, browser_only="none", teletext_only="1.5-rust", reuse_artifacts="")
        for key, expected in (("BUILD_RUST", True), ("BUILD_WEB", False)):
            expression = re.search(rf"^      {key}: (.+)$", self.jobs["build"], re.M)[1]
            self.assertEqual(self.evaluate(expression, inputs, SimpleNamespace()), expected)
        expression = re.search(r"^        level: (.+)$", self.jobs["teletext"], re.M)[1]
        expression = expression.removeprefix("${{").removesuffix("}}").strip()
        expression = expression.replace("&&", " and ").replace("||", " or ")
        for mode, expected in (("1.5-rust", ["1.5"]), ("1.5", ["1.5"]),
                               ("2.5", ["2.5"]), ("all", ["1.5", "2.5"]), ("none", ["1.5", "2.5"])):
            actual = eval(expression, {"__builtins__": {}}, {
                "inputs": SimpleNamespace(teletext_only=mode), "fromJSON": json.loads,
            })
            self.assertEqual(actual, expected)
        self.assertIn('TELETEXT_LEVEL: ${{ matrix.level }}', self.jobs["teletext"])
        self.assertIn('scripts/ci-teletext-selection.py rust "$TELETEXT_LEVEL")', self.jobs["teletext"])
        self.assertIn('scripts/ci-teletext-selection.py rust "$TELETEXT_LEVEL" --next-runtime)', self.jobs["teletext"])
        self.assertIn("format('-teletext-{0}', inputs.teletext_only)", self.workflow)

    def test_audit_run_cannot_cancel_or_prove_a_full_run(self):
        self.assertIn("inputs.audit_only && '-audit-only'", self.workflow)
        self.assertIn("&& run.display_title !== 'CI (Audit only)'", self.workflow)

    def test_mixed_audit_modes_are_explicitly_rejected(self):
        step = self.workflow.split("- name: Reject mixed audit mode\n", 1)[1]
        expression = step.splitlines()[0].strip().removeprefix("if: ")
        base = dict(audit_only=True, test_only=False, next_engine_only=False,
                    teletext_only="none", browser_only="none", browser_case_set="fixtures", reuse_artifacts="")
        self.assertFalse(self.evaluate(expression, base, SimpleNamespace()))
        for key, value in (("test_only", True), ("next_engine_only", True),
                           ("teletext_only", "all"), ("browser_only", "both"),
                           ("browser_case_set", "channel-preview"), ("browser_case_set", "teletext-1.5"),
                           ("browser_case_set", "teletext-runtime"), ("teletext_only", "1.5-rust"), ("reuse_artifacts", "{}")):
            with self.subTest(mode=key):
                self.assertTrue(self.evaluate(expression, dict(base, **{key: value}), SimpleNamespace()))

    def test_browser_case_sets_are_bounded_and_cannot_mix_focused_modes(self):
        step = self.jobs["build"].split("      - name: Validate dispatch mode\n", 1)[1]
        code = re.search(r"python3 - <<'PYCODE'\n(.*?)^          PYCODE$", step, re.S | re.M)[1]
        code = "\n".join(line[10:] for line in code.splitlines())
        base = dict(os.environ, TEST_ONLY="false", NEXT_ONLY="false", TELETEXT_ONLY="none",
                    BROWSER_ONLY="none", BROWSER_CASE_SET="fixtures", REUSE_ARTIFACTS="",
                    RUNNER_POOL="self-hosted", RESET_TARGET_DIR="false")
        cases = [({}, True), ({"BROWSER_ONLY": "chromium", "BROWSER_CASE_SET": "channel-preview"}, True),
                 ({"BROWSER_ONLY": "firefox", "BROWSER_CASE_SET": "teletext-1.5"}, True),
                 ({"BROWSER_ONLY": "both", "BROWSER_CASE_SET": "teletext-runtime"}, True),
                 ({"TELETEXT_ONLY": "1.5-rust"}, True),
                 ({"TELETEXT_ONLY": "1.5-rust", "BROWSER_ONLY": "chromium"}, False),
                 ({"TELETEXT_ONLY": "1.5-rust", "NEXT_ONLY": "true"}, False),
                 ({"TELETEXT_ONLY": "1.5-rust", "TEST_ONLY": "true"}, False),
                 ({"BROWSER_CASE_SET": "teletext-runtime"}, False),
                 ({"BROWSER_ONLY": "firefox", "BROWSER_CASE_SET": "teletext-runtime", "NEXT_ONLY": "true"}, False),
                 ({"BROWSER_ONLY": "chromium", "BROWSER_CASE_SET": "teletext-runtime", "TELETEXT_ONLY": "1.5-rust"}, False),
                 ({"BROWSER_CASE_SET": "channel-preview"}, False),
                 ({"BROWSER_CASE_SET": "teletext-1.5"}, False),
                 ({"BROWSER_ONLY": "firefox", "BROWSER_CASE_SET": "teletext-1.5", "NEXT_ONLY": "true"}, False),
                 ({"BROWSER_ONLY": "firefox", "BROWSER_CASE_SET": "teletext-1.5", "TEST_ONLY": "true"}, False),
                 ({"BROWSER_ONLY": "firefox", "BROWSER_CASE_SET": "teletext-1.5", "TELETEXT_ONLY": "1.5"}, False),
                 ({"BROWSER_ONLY": "chromium", "BROWSER_CASE_SET": "unknown"}, False),
                 ({"BROWSER_ONLY": "chromium", "BROWSER_CASE_SET": "channel-preview", "NEXT_ONLY": "true"}, False),
                 ({"BROWSER_ONLY": "firefox", "BROWSER_CASE_SET": "channel-preview", "TEST_ONLY": "true"}, False),
                 ({"BROWSER_ONLY": "both", "BROWSER_CASE_SET": "channel-preview", "TELETEXT_ONLY": "all"}, False)]
        for changes, valid in cases:
            with self.subTest(changes=changes):
                result = subprocess.run([sys.executable, "-c", code], env=dict(base, **changes),
                                        text=True, capture_output=True)
                self.assertEqual(result.returncode == 0, valid, result.stderr)

        for changes, valid in (
            ({"REUSE_ARTIFACTS": "{}", "TELETEXT_ONLY": "1.5-rust"}, True),
            ({"REUSE_ARTIFACTS": "{}", "BROWSER_ONLY": "both", "BROWSER_CASE_SET": "teletext-runtime"}, True),
            ({"REUSE_ARTIFACTS": "{}"}, False),
            ({"REUSE_ARTIFACTS": "{}", "TELETEXT_ONLY": "1.5-rust", "RUNNER_POOL": "github-hosted"}, False),
            ({"REUSE_ARTIFACTS": "{}", "TELETEXT_ONLY": "1.5-rust", "RESET_TARGET_DIR": "true"}, False),
            ({"REUSE_ARTIFACTS": "not-json", "TELETEXT_ONLY": "1.5-rust"}, False),
            ({"REUSE_ARTIFACTS": "[]", "TELETEXT_ONLY": "1.5-rust"}, False),
        ):
            with self.subTest(reuse=changes):
                result = subprocess.run([sys.executable, "-c", code], env=dict(base, **changes),
                                        text=True, capture_output=True)
                self.assertEqual(result.returncode == 0, valid, result.stderr)

    def test_reuse_provider_never_builds_installs_resets_uploads_or_prunes(self):
        for mode in ({"teletext_only": "1.5-rust"}, {"browser_only": "both", "browser_case_set": "teletext-runtime"}):
            inputs = dict(test_only=False, next_engine_only=False, browser_only="none", teletext_only="none",
                          runner="self-hosted", reset_target_dir=False, reuse_artifacts='{"explicit":"reference"}')
            inputs.update(mode)
            build_env = {}
            for key in ("BUILD_RUST", "BUILD_WEB"):
                expression = re.search(rf"^      {key}: (.+)$", self.jobs["build"], re.M)[1]
                build_env[key] = str(self.evaluate(expression, inputs, SimpleNamespace())).lower()
            self.assertEqual(build_env, {"BUILD_RUST": "false", "BUILD_WEB": "false"})
            steps = re.split(r"^      - ", self.jobs["build"], flags=re.M)[1:]
            active = []
            for step in steps:
                condition = re.search(r"^        if: (.+)$", step, re.M)
                if not condition or self.evaluate(condition[1], inputs, SimpleNamespace(), SimpleNamespace(**build_env)):
                    active.append(step.splitlines()[0])
            self.assertEqual(active, ["name: Validate dispatch mode", "uses: actions/checkout@v7", "name: Reuse verified CI artifacts"])
        self.assertIn("CI_REUSE_ARTIFACTS: ${{ inputs.reuse_artifacts }}", self.workflow)
        self.assertIn("REUSE_ARTIFACTS: ${{ inputs.reuse_artifacts }}", self.jobs["build"])

    def test_reuse_has_no_download_fallback_and_has_an_honest_job_name(self):
        self.assertIn("name: ${{ inputs.reuse_artifacts && 'Verify reused CI artifacts' || 'Build CI artifacts' }}",
                      self.jobs["build"])
        for job in ("test", "next_engine", "teletext", "hydration"):
            download = self.jobs[job].split("uses: actions/download-artifact@v8\n", 1)[1]
            condition = re.search(r"^        if: (.+)$", download, re.M)[1]
            for reference in ("", '{"explicit":"reference"}'):
                for hit in ("true", "false", ""):
                    expression = condition.replace("steps.local_artifact.outputs.hit", repr(hit))
                    enabled = self.evaluate(expression, {"reuse_artifacts": reference}, SimpleNamespace())
                    self.assertEqual(enabled, not reference and hit != "true", (job, reference, hit))

    def test_browser_case_set_keeps_distinct_recovery_concurrency(self):
        self.assertIn("inputs.browser_case_set && inputs.browser_case_set != 'fixtures' && format('-{0}', inputs.browser_case_set) || ''", self.workflow)
        self.assertIn('recovery_args="$(python3 scripts/ci-teletext-selection.py browser-recovery "$BROWSER_CASE_SET")"',
                      self.jobs["hydration"])
        self.assertIn('playwright_args+=("${selected_cases[@]}")', self.jobs["hydration"])

    def next_step_run(self, name):
        step = self.jobs["next_engine"].split(f"      - name: {name}\n", 1)[1]
        step = step.split("\n      - ", 1)[0]
        return "\n".join(line[10:] for line in step.split("        run: |\n", 1)[1].splitlines())

    def test_next_runtime_preparation_exports_existing_contained_app_data(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            tools = root / "tools"
            tools.mkdir()
            git = tools / "git"
            git.write_text("#!/bin/sh\nprintf 'synthetic-ci-sha\\n'\n")
            git.chmod(0o755)
            env_file = root / "github.env"
            env = dict(os.environ, RUNNER_TEMP=str(root / "runtime"), GITHUB_RUN_ID="fixture",
                       GITHUB_RUN_ATTEMPT="1", GITHUB_ENV=str(env_file),
                       CARGO_TARGET_DIR=str(root / "synthetic-target"), PATH=f"{tools}:{os.environ['PATH']}")
            subprocess.run(["bash", "-eu", "-c", self.next_step_run("Prepare isolated runtime and diagnostics")],
                           cwd=root, env=env, check=True)
            values = dict(line.split("=", 1) for line in env_file.read_text().splitlines())
            data = Path(values["ERSATZRS_APP_DATA_DIR"])
            self.assertTrue(data.is_dir())
            self.assertTrue(data.is_relative_to(root / "runtime"))
            self.assertEqual(data.parent, Path(values["TMPDIR"]))

    def test_next_inventory_verifies_every_declared_package_before_export(self):
        run = self.next_step_run("Stage Next and validated FFmpeg runtime")
        code = re.search(r"python3 - <<'PYCODE'[^\n]*\n(.*?)^PYCODE$", run, re.S | re.M)[1]
        rids = ("linux-x64", "linux-arm64", "linux-arm", "win-x64", "osx-x64", "osx-arm64")
        manifest = {"runtime_packages": {rid: {"asset": f"reviewed-{rid}.archive"} for rid in rids}}
        for refused in (None, "osx-arm64"):
            with self.subTest(refused=refused), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                manifest_path = root / "recipe/next-patches/manifest.json"
                manifest_path.parent.mkdir(parents=True)
                manifest_path.write_text(json.dumps(manifest))
                fetched, verified, urls = [], [], []

                def fetch(url, limit):
                    urls.append(url)
                    fetched.append(url.rsplit("/", 1)[1])
                    return fetched[-1].encode()

                def verify(rid, output, archive):
                    if rid == refused:
                        raise ValueError("synthetic reviewed-package verification refusal")
                    self.assertEqual(archive.read_bytes(), manifest["runtime_packages"][rid]["asset"].encode())
                    verified.append(rid)
                    output.mkdir()

                stage = SimpleNamespace(__file__=str(root / "recipe/stage-next.py"), MANIFEST=manifest,
                                        package_url=lambda rid: f"https://example.invalid/{'retained' if rid.startswith('osx-') else 'current'}/{manifest['runtime_packages'][rid]['asset']}", MAX_ARCHIVE=1024,
                                        download=fetch, stage_patched_snapshot=verify)
                spec = SimpleNamespace(loader=SimpleNamespace(exec_module=lambda _: None))
                env_file = root / "github.env"
                with patch.dict(os.environ, TMPDIR=str(root), GITHUB_ENV=str(env_file)), \
                     patch("importlib.util.spec_from_file_location", return_value=spec), \
                     patch("importlib.util.module_from_spec", return_value=stage):
                    if refused:
                        with self.assertRaisesRegex(ValueError, "verification refusal"):
                            exec(code, {})
                        self.assertFalse(env_file.exists(), "refused inventory must not be published")
                        continue
                    exec(code, {})
                self.assertEqual(set(verified), set(rids))
                self.assertEqual(len(fetched), len(rids))
                for rid in rids:
                    release = "retained" if rid.startswith("osx-") else "current"
                    self.assertIn(f"https://example.invalid/{release}/{manifest['runtime_packages'][rid]['asset']}", urls)
                values = dict(line.split("=", 1) for line in env_file.read_text().splitlines())
                packages = Path(values["NEXT_CANDIDATE_PACKAGES"])
                self.assertEqual((packages / "manifest.json").read_bytes(), manifest_path.read_bytes())
                self.assertEqual({p.name for p in packages.iterdir()},
                                 {"manifest.json", *(p["asset"] for p in manifest["runtime_packages"].values())})
                self.assertEqual(Path(values["ERSATZRS_NEXT_TEST_PACKAGE_ARCHIVE"]),
                                 packages / manifest["runtime_packages"]["linux-x64"]["asset"])
                self.assertEqual(Path(values["ERSATZRS_NEXT_TEST_PACKAGE_MANIFEST"]), manifest_path)


if __name__ == "__main__":
    unittest.main()
