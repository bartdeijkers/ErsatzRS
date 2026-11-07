#!/usr/bin/env python3
"""CI artifact handoff regressions; fixtures are never executed."""

import importlib.util
import hashlib
from contextlib import redirect_stdout
from copy import deepcopy
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
        # Fixtures own their synthetic manifests and publish/verify scenarios;
        # a real recovery dispatch must not replace them with its producer.
        environment = patch.dict(os.environ, CI_REUSE_ARTIFACTS="")
        environment.start()
        self.addCleanup(environment.stop)
        self.commands = patch.object(artifacts.subprocess, "check_output", side_effect=(
            lambda command, **_: SHA + "\n" if command[0] == "git" else "rustc fixture\n"
        ))
        self.commands.start()
        self.addCleanup(self.commands.stop)

    def fixture(self, kind="rust", capabilities=None):
        root = self.base / f"input-{kind}"
        root.mkdir()
        names = {name for cap in (capabilities or artifacts.CAPABILITIES[kind])
                 for name in artifacts.CAPABILITIES[kind][cap]}
        for name in names:
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b"synthetic CI handoff fixture; never executed")
            if name.startswith("bin/"):
                path.chmod(0o755)
        archive = self.base / f"{kind}.tar.gz"
        artifacts.pack(root, archive, SHA, kind, capabilities)
        return root, archive

    def test_test_only_artifact_omits_smoke_and_refuses_runtime_consumer(self):
        root, archive = self.fixture(capabilities=["rust-tests"])
        self.assertFalse((root / "bin/ersatzrs-web-smoke").exists())
        output, env = self.base / "tests", self.base / "tests.env"
        artifacts.restore(archive, output, SHA, env, "rust", required_capabilities=["rust-tests"])
        values = dict(line.split("=", 1) for line in env.read_text().splitlines())
        self.assertIn("CI_NEXTEST", values)
        self.assertNotIn("CI_SMOKE_SERVER", values)
        before = env.read_bytes()
        refused = self.base / "runtime"
        with self.assertRaisesRegex(ValueError, "consumer capabilities"):
            artifacts.restore(archive, refused, SHA, env, "rust", required_capabilities=["smoke-server"])
        self.assertFalse(refused.exists())
        self.assertEqual(env.read_bytes(), before)

    def test_capabilities_require_payloads_and_cannot_hide_or_invent_products(self):
        root, _ = self.fixture()
        manifest_path = root / "build.json"
        original = json.loads(manifest_path.read_text())
        for capabilities in ([], None, "rust-tests", ["unknown"], ["smoke-server"],
                             ["matched-web"], ["rust-tests", "rust-tests"], [True]):
            manifest_path.write_text(json.dumps(dict(original, capabilities=capabilities)))
            with self.subTest(capabilities=capabilities), self.assertRaisesRegex(ValueError, "capabilities"):
                artifacts.verify(root, SHA, "rust")
        manifest_path.write_text(json.dumps(dict(original, capabilities=["rust-tests"])))
        with self.assertRaisesRegex(ValueError, "undeclared"):
            artifacts.verify(root, SHA, "rust")
        # Even a manifest with internally consistent hashes cannot claim a
        # capability whose payload is missing.
        (root / "bin/ersatzrs-web-smoke").unlink()
        del original["files"]["bin/ersatzrs-web-smoke"]
        manifest_path.write_text(json.dumps(original))
        with self.assertRaisesRegex(ValueError, "missing or empty"):
            artifacts.verify(root, SHA, "rust", required_capabilities=["smoke-server"])

    def test_legacy_manifest_keeps_the_entire_original_contract(self):
        root, _ = self.fixture()
        path = root / "build.json"
        manifest = json.loads(path.read_text())
        manifest["schema_version"] = 1
        path.write_text(json.dumps(manifest))
        with self.assertRaisesRegex(ValueError, "legacy"):
            artifacts.verify(root, SHA, "rust")
        del manifest["capabilities"]
        path.write_text(json.dumps(manifest))
        self.assertIn("smoke-server", artifacts.verify(root, SHA, "rust", required_capabilities=["smoke-server"]))
        (root / "bin/ersatzrs-web-smoke").unlink()
        del manifest["files"]["bin/ersatzrs-web-smoke"]
        path.write_text(json.dumps(manifest))
        with self.assertRaisesRegex(ValueError, "missing or empty"):
            artifacts.verify(root, SHA, "rust")

    def test_full_artifact_exports_smoke_only_after_validating_capability(self):
        _, archive = self.fixture()
        env = self.base / "full.env"
        artifacts.restore(archive, self.base / "full", SHA, env, "rust",
                          required_capabilities=["rust-tests", "smoke-server"])
        self.assertIn("CI_SMOKE_SERVER=", env.read_text())
        with self.assertRaisesRegex(ValueError, "consumer capabilities"):
            artifacts.restore(archive, self.base / "wrong-kind", SHA, None, "rust",
                              required_capabilities=["matched-web"])

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

    def test_full_reuse_requires_scope_proof_before_returning_artifacts(self):
        with patch.dict(os.environ, {key: "0" if key == "CARGO_PROFILE_TEST_DEBUG" else ""
                                     for key in artifacts.BUILD_FLAG_KEYS}):
            root, archive = self.fixture()
        reference = self.reference(root, archive, "rust")
        cache = self.base / "cache"
        artifacts.publish_local(archive, SHA, "rust", reference["cache_key"], cache)
        with patch.object(artifacts, "verify_source_equivalence", return_value=[]), \
             patch.object(artifacts, "verify_full_workspace_archive", return_value=4) as coverage:
            result = artifacts.reuse_local(self.base / "reused.tar.gz", "b" * 40,
                                           "rust", reference, cache, full_workspace=True)
            coverage.assert_called_once()
            self.assertEqual(result["rust_source_sha"], SHA)
        with patch.object(artifacts, "verify_source_equivalence", return_value=[]), \
             patch.object(artifacts, "verify_full_workspace_archive",
                          side_effect=ValueError("omits workspace test targets")):
            with self.assertRaisesRegex(ValueError, "omits workspace"):
                artifacts.reuse_local(self.base / "refused.tar.gz", "b" * 40,
                                      "rust", reference, cache, full_workspace=True)

    def test_full_reuse_also_requires_the_runtime_smoke_server(self):
        with patch.dict(os.environ, {key: "0" if key == "CARGO_PROFILE_TEST_DEBUG" else ""
                                     for key in artifacts.BUILD_FLAG_KEYS}):
            root, archive = self.fixture(capabilities=["rust-tests"])
        reference = self.reference(root, archive, "rust")
        cache = self.base / "cache"
        artifacts.publish_local(archive, SHA, "rust", reference["cache_key"], cache)
        with patch.object(artifacts, "verify_source_equivalence", return_value=[]), \
             patch.object(artifacts, "verify_full_workspace_archive") as coverage:
            with self.assertRaisesRegex(ValueError, "capabilities"):
                artifacts.reuse_local(self.base / "refused.tar.gz", "b" * 40,
                                      "rust", reference, cache, full_workspace=True)
            coverage.assert_not_called()

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


class FullWorkspaceCoverage(unittest.TestCase):
    def setUp(self):
        self.metadata = {
            "workspace_members": ["a", "b"],
            "resolve": {"nodes": [{"id": "a", "features": []}, {"id": "b", "features": ["ssr"]}]},
            "packages": [
                {"id": "a", "name": "a", "targets": [
                    {"name": "a", "kind": ["lib"], "test": True},
                    {"name": "a-cli", "kind": ["bin"], "test": True}]},
                {"id": "b", "name": "b", "targets": [
                    {"name": "b", "kind": ["cdylib", "rlib"], "test": True},
                    {"name": "live", "kind": ["test"], "test": True, "required-features": ["ssr"]},
                    {"name": "gpu", "kind": ["test"], "test": True, "required-features": ["gpu"]},
                    {"name": "server", "kind": ["bin"], "test": False}]},
            ],
        }
        self.binaries = {"rust-binaries": {
            "a-lib": {"package-id": "a", "binary-name": "a", "kind": "lib"},
            "a-bin": {"package-id": "a", "binary-name": "a-cli", "kind": "bin"},
            "b-lib": {"package-id": "b", "binary-name": "b", "kind": "lib"},
            "b-test": {"package-id": "b", "binary-name": "live", "kind": "test"},
        }}

    def test_full_selection_includes_libraries_binaries_and_integration_tests(self):
        self.assertEqual(artifacts.verify_workspace_test_coverage(
            self.metadata, self.metadata, self.binaries), 4)

    def test_subset_cannot_claim_full_coverage(self):
        for missing in self.binaries["rust-binaries"]:
            with self.subTest(missing=missing):
                partial = deepcopy(self.binaries)
                del partial["rust-binaries"][missing]
                with self.assertRaisesRegex(ValueError, "omits workspace test targets"):
                    artifacts.verify_workspace_test_coverage(self.metadata, self.metadata, partial)

    def test_changed_membership_and_feature_graph_are_refused(self):
        for field in ("membership", "features"):
            with self.subTest(field=field):
                archived = deepcopy(self.metadata)
                if field == "membership":
                    archived["workspace_members"].pop()
                else:
                    archived["resolve"]["nodes"][1]["features"] = []
                with self.assertRaisesRegex(ValueError, "different workspace"):
                    artifacts.verify_workspace_test_coverage(self.metadata, archived, self.binaries)

    def test_binary_kind_does_not_substitute_for_a_missing_target(self):
        changed = deepcopy(self.binaries)
        changed["rust-binaries"]["b-test"]["kind"] = "bin"
        with self.assertRaisesRegex(ValueError, "omits workspace"):
            artifacts.verify_workspace_test_coverage(self.metadata, self.metadata, changed)

    def test_enabled_required_feature_needs_its_actual_test_binary(self):
        enabled = deepcopy(self.metadata)
        enabled["resolve"]["nodes"][1]["features"].append("gpu")
        with self.assertRaisesRegex(ValueError, "b::test/gpu"):
            artifacts.verify_workspace_test_coverage(enabled, enabled, self.binaries)

    def test_nextest_catalog_does_not_enable_optional_test_targets(self):
        catalog = deepcopy(self.metadata)
        catalog["resolve"]["nodes"][1]["features"].append("gpu")
        self.assertEqual(artifacts.verify_workspace_test_coverage(
            self.metadata, catalog, self.binaries, catalog), 4)
        altered = deepcopy(catalog)
        altered["resolve"]["nodes"][0]["features"].append("unexpected")
        with self.assertRaisesRegex(ValueError, "different workspace feature graph"):
            artifacts.verify_workspace_test_coverage(self.metadata, altered, self.binaries, catalog)


class Outputs(SimpleNamespace):
    """Job outputs: an output a job did not set reads as '' on GitHub."""

    def __getattr__(self, name):
        if name.startswith("__"):
            raise AttributeError(name)
        return ""


def preflight(**outputs):
    return SimpleNamespace(preflight=SimpleNamespace(outputs=Outputs(**outputs)))


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

    def scheduled(self, lane="full", selection=None, **selected):
        inputs = dict(audit_only=False, test_only=False, next_engine_only=False,
                      teletext_only="none", browser_only="none", browser_case_set="fixtures", reuse_artifacts="", next_case_set="all")
        inputs.update(selected)
        scheduled = set()
        for name, body in self.jobs.items():
            condition = re.search(r"^    if: (.+)$", body, re.M)
            condition = condition[1] if condition else "true"
            dependencies = re.search(r"^    needs: (.+)$", body, re.M)
            dependencies = [x.strip() for x in dependencies[1].strip("[]").split(",")] if dependencies else []
            if "always()" not in condition and any(x not in scheduled for x in dependencies):
                continue
            # A skipped Preflight (focused modes) has empty outputs on GitHub.
            selected_lane = lane if "preflight" in scheduled else ""
            needs = SimpleNamespace(**{
                key: SimpleNamespace(result="success" if key in scheduled else "skipped",
                                     outputs=Outputs(skip="false", lane=selected_lane, tree="",
                                                     **(selection if selected_lane and selection else {})))
                for key in self.jobs
            })
            if self.evaluate(condition, inputs, needs):
                scheduled.add(name)
        return scheduled

    def test_default_manual_run_keeps_every_job(self):
        self.assertEqual(self.scheduled(), set(self.jobs))

    def test_full_reuse_keeps_every_gate_and_requires_complete_scope(self):
        self.assertEqual(self.scheduled(reuse_artifacts='{"rust":{},"web":{}}'), set(self.jobs))
        provider = self.jobs["build"].split("name: Reuse verified CI artifacts", 1)[1].split("      - uses:", 1)[0]
        self.assertIn("REQUIRE_FULL_WORKSPACE:", provider)
        self.assertIn('"$kind" == \'rust\' && "$REQUIRE_FULL_WORKSPACE" == \'true\'', provider)
        self.assertIn("coverage+=(--require-full-workspace)", provider)
        self.assertIn('"${coverage[@]}"', provider)

    def test_runtime_jobs_match_the_managed_ffmpeg_default(self):
        runtime = (Path(__file__).resolve().parents[1]
                   / "crates/ersatzrs-ffmpeg/src/runtime.rs").read_text()
        default = re.search(r'DEFAULT_MANAGED_FFMPEG_TAG: &str = "([^"]+)"', runtime)
        self.assertIsNotNone(default, "the managed runtime default must be explicit")
        for job in ("next_engine", "teletext", "hydration"):
            with self.subTest(job=job):
                pins = re.findall(r"^      ERSATZRS_FFMPEG_RELEASE_TAG: (\S+)$",
                                  self.jobs[job], re.M)
                self.assertEqual(pins, [default[1]],
                                 "runtime CI must exercise the accepted application default")

    def test_browser_health_declares_independently_qualified_runtime(self):
        # Health must be checked against the published payload qualification,
        # not used to derive its own expected capability or executable identity.
        environment = self.jobs["hydration"].split("    steps:\n", 1)[0]
        self.assertRegex(environment,
                         r'(?m)^      ERSATZRS_TELETEXT_E2E_ORIGINAL_PTS: "true"$')
        for binary in ("FFMPEG", "FFPROBE"):
            self.assertRegex(environment,
                             rf'(?m)^      ERSATZRS_TELETEXT_E2E_{binary}_SHA256: [a-f0-9]{{64}}$')

    def test_required_ci_ok_check_aggregates_every_gate_job(self):
        # `CI ok` is the single required check on main. A gate job missing
        # from its needs could fail without blocking a merge.
        body = self.jobs["ci_ok"]
        self.assertIn("    name: CI ok\n", body)
        needs = re.search(r"^    needs: \[(.+)\]$", body, re.M)[1]
        self.assertEqual({x.strip() for x in needs.split(",")}, set(self.jobs) - {"ci_ok"})
        self.assertRegex(body, r"(?m)^    if: \$\{\{ always\(\) && ")

    def test_pull_requests_always_start_the_workflow(self):
        # A path-filtered pull_request never reports the required check.
        trigger = self.workflow.split("\n  pull_request:\n", 1)[1].split("\n  push:\n", 1)[0]
        self.assertNotRegex(trigger, r"(?m)^    paths(-ignore)?:")

    def preflight_diff(self, files, **overrides):
        """Execute the actual github-script with controlled API responses."""
        code = re.search(r"      - id: duplicate\n.*?          script: \|\n(.*?)^      - uses:",
                         self.jobs["preflight"], re.S | re.M)[1]
        code = "\n".join(line[12:] for line in code.splitlines())
        harness = r"""
        const input = JSON.parse(require('fs').readFileSync(0, 'utf8'));
        const expected = { number: 42, draft: false, head: {sha: 'head'}, base: {sha: 'base'},
                           changed_files: input.files.length, ...input.expected };
        const output = {}, written = {};
        let reads = 0;
        const github = {rest: {
          pulls: {listFiles: 'listFiles', get: async () => ({data: {
            ...expected, ...(input.revisions?.[reads++] || {})
          }})}, git: {getCommit: async () => ({data: {tree: {sha: 'merge-tree'}}})}
        }, paginate: async (method, args) => {
          if (method !== 'listFiles' || args.per_page !== 100) throw new Error('Expected paginated PR files');
          return input.files;
        }};
        const core = {setOutput: (key, value) => output[key] = value, info: () => {}, notice: () => {}};
        const context = {eventName: 'pull_request', repo: {owner: 'fixture', repo: 'fixture'},
                         payload: {pull_request: expected}, sha: 'merge-result'};
        const fakeRequire = name => {
          if (name !== 'fs') throw new Error('Unexpected module');
          return {writeFileSync: (path, data) => written[path.split('/').pop()] = data};
        };
        const AsyncFunction = Object.getPrototypeOf(async function(){}).constructor;
        new AsyncFunction('github', 'context', 'core', 'require', 'process', input.code)(
          github, context, core, fakeRequire, {env: {RUNNER_TEMP: '/fixture'}}
        ).then(() => console.log(JSON.stringify({output, written})))
         .catch(error => {console.error(error.message); process.exitCode = 1;});
        """
        return subprocess.run(["node", "-e", harness], input=json.dumps(dict(code=code, files=files, **overrides)),
                              text=True, capture_output=True)

    def test_complete_diff_preserves_added_and_old_paths_and_runtime_assets(self):
        for path in ("CHANGELOG.md", "crates/ersatzrs-web/public/channel-icon.png", "nested/runtime.md"):
            result = self.preflight_diff([{"filename": path, "status": "modified"}])
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads(result.stdout)["output"]["lane"], "pr")
        result = self.preflight_diff([
            {"filename": "docs/moved.md", "previous_filename": "crates/old.rs", "status": "renamed"},
            {"filename": "crates/new.rs", "status": "added"},
        ])
        self.assertEqual(result.returncode, 0, result.stderr)
        data = json.loads(result.stdout)
        self.assertEqual(data["output"], {"lane": "pr", "skip": "false", "tree": "merge-tree"})
        self.assertEqual(set(data["written"]["changed-files.txt"].splitlines()),
                         {"docs/moved.md", "crates/old.rs", "crates/new.rs"})
        self.assertEqual(data["written"]["added-files.txt"], "crates/new.rs")
        result = self.preflight_diff([{"filename": "plans/INDEX.md", "status": "modified"}])
        self.assertEqual(json.loads(result.stdout)["output"]["lane"], "skip")

    def test_incomplete_malformed_or_changed_diff_cannot_skip_checks(self):
        normal = [{"filename": "plans/INDEX.md", "status": "modified"}]
        cases = [(normal, {"expected": {"changed_files": 2}}), ([], {}),
                 (normal * 2, {}),
                 ([{"filename": "docs/../production.rs", "status": "modified"}], {}),
                 ([{"filename": "docs/new.md", "status": "renamed"}], {}),
                 ([{"filename": "docs/new.md", "status": "invented"}], {}),
                 ([{"filename": "docs/new\nfile.md", "status": "modified"}], {}),
                 (normal, {"revisions": [{"head": {"sha": "new-head"}}]}),
                 (normal, {"revisions": [{}, {"base": {"sha": "new-base"}}]})]
        for files, options in cases:
            with self.subTest(files=files, options=options):
                self.assertNotEqual(self.preflight_diff(files, **options).returncode, 0)

    def test_actual_selection_outputs_match_gate_and_matrix(self):
        code = re.search(r"          python3 - .*? <<'PY'\n(.*?)^          PY$",
                         self.jobs["preflight"], re.S | re.M)[1]
        code = "\n".join(line[10:] for line in code.splitlines())
        root = Path(__file__).resolve().parents[1]
        paths = ("crates/ersatzrs-web/src/admin/pages/libraries/libraries.module.css",
                 "xtask/src/affected_checks.rs", "crates/ersatzrs-web/src/admin/pages/channels/teletext_drafts.rs")
        with tempfile.TemporaryDirectory() as tmp:
            changed, selection = Path(tmp, "changed"), Path(tmp, "selection.json")
            for path in paths:
                changed.write_text(path + "\n")
                result = subprocess.run([sys.executable, str(root / "scripts/ci-domains.py"), "select", str(changed)],
                                        text=True, capture_output=True, check=True)
                selection.write_text(result.stdout)
                result = subprocess.run([sys.executable, "-c", code, str(selection)], text=True, capture_output=True)
                self.assertEqual(result.returncode, 0, result.stderr)
                outputs = dict(line.split("=", 1) for line in result.stdout.splitlines())
                omitted = json.loads(outputs["deselected"])
                self.assertEqual(self.ci_ok_exit("pr", {job: "skipped" for job in omitted}, outputs=outputs), 0)
                self.assertEqual(set(json.loads(outputs["required_jobs"])), set(self.jobs) - {"ci_ok", *omitted})
                for row in json.loads(outputs["pr_matrix"])["include"]:
                    self.assertEqual(row["project"], "chromium")
                    self.assertIn(row["suite"], {"general", "1.5"})
                if "teletext_drafts" in path:
                    self.assertIn("teletext-editor-flow", outputs["teletext_filter"])
                    self.assertNotIn("teletext-restart-flow", outputs["teletext_filter"])
            data = json.loads(selection.read_text())
            data["products"]["web"] = False
            selection.write_text(json.dumps(data))
            result = subprocess.run([sys.executable, "-c", code, str(selection)], capture_output=True)
            self.assertNotEqual(result.returncode, 0)

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
        unfocused = preflight(lane="")
        for key, expected in (("BUILD_RUST", True), ("BUILD_WEB", False)):
            expression = re.search(rf"^      {key}: (.+)$", self.jobs["build"], re.M)[1]
            self.assertEqual(self.evaluate(expression, inputs, unfocused), expected)
        expression = re.search(r"^        level: (.+)$", self.jobs["teletext"], re.M)[1]
        expression = expression.removeprefix("${{").removesuffix("}}").strip()
        expression = expression.replace("&&", " and ").replace("||", " or ")
        for mode, expected in (("1.5-rust", ["1.5"]), ("1.5", ["1.5"]),
                               ("2.5", ["2.5"]), ("all", ["1.5", "2.5"]), ("none", ["1.5"]),
                               ("", ["1.5"]), (None, ["1.5"])):
            actual = eval(expression, {"__builtins__": {}}, {
                "inputs": SimpleNamespace(teletext_only=mode), "fromJSON": json.loads,
            })
            self.assertEqual(actual, expected)
        self.assertIn('TELETEXT_LEVEL: ${{ matrix.level }}', self.jobs["teletext"])
        self.assertIn('scripts/ci-teletext-selection.py rust "$TELETEXT_LEVEL")', self.jobs["teletext"])
        self.assertIn('scripts/ci-teletext-selection.py rust "$TELETEXT_LEVEL" --next-runtime)', self.jobs["teletext"])
        self.assertIn("format('-teletext-{0}', inputs.teletext_only)", self.workflow)

    def test_experimental_browser_level25_requires_explicit_dispatch(self):
        expression = re.search(r"^      matrix: (.+)$", self.jobs["hydration"], re.M)[1]
        expression = expression.removeprefix("${{").removesuffix("}}").strip()
        expression = expression.replace("&&", " and ").replace("||", " or ")
        for mode, expected in ((None, {"general", "1.5"}), ("", {"general", "1.5"}),
                               ("none", {"general", "1.5"}), ("1.5", {"1.5"}),
                               ("2.5", {"2.5"}), ("all", {"1.5", "2.5"})):
            with self.subTest(teletext_only=mode):
                matrix = self.browser_matrix(expression, teletext_only=mode)
                self.assertEqual({row["suite"] for row in matrix}, expected)
                self.assertEqual(len(matrix), 2 * len(expected))
                self.assertEqual(len({row["port"] for row in matrix}), len(matrix))
                for suite in expected:
                    self.assertEqual({row["project"] for row in matrix if row["suite"] == suite},
                                     {"chromium", "firefox"})

    @staticmethod
    def browser_matrix(expression, lane="full", pr_matrix="", **inputs):
        inputs = dict(dict(teletext_only="none", browser_only="none"), **inputs)
        return eval(expression, {"__builtins__": {}}, {
            "inputs": SimpleNamespace(**inputs), "fromJSON": json.loads,
            "needs": preflight(lane=lane, pr_matrix=pr_matrix),
        })["include"]

    def test_lanes_schedule_only_their_jobs(self):
        everything = set(self.jobs)
        self.assertEqual(self.scheduled(lane="pr"), everything)
        self.assertEqual(self.scheduled(lane="light"),
                         everything - {"next_engine", "teletext", "hydration"})
        # A pull request that touches no Teletext, Next or browser domain.
        narrow = dict(run_next="false", run_teletext="false", run_browser="false")
        self.assertEqual(self.scheduled(lane="pr", selection=narrow),
                         everything - {"next_engine", "teletext", "hydration"})
        self.assertEqual(self.scheduled(lane="pr", selection=dict(run_next="false")),
                         everything - {"next_engine"})
        # Focused manual modes never see a selection.
        self.assertEqual(self.scheduled(lane="pr", selection=narrow, teletext_only="all"),
                         {"build", "teletext", "hydration"})

    def test_build_flags_follow_pr_consumers_and_full_manual_keeps_every_product(self):
        inputs = dict(test_only=False, next_engine_only=False, browser_only="none",
                      teletext_only="none", reuse_artifacts="")
        cases = [("pr", (False, True, False)), ("pr", (True, False, False)),
                 ("pr", (True, True, False)), ("pr", (True, True, True)),
                 ("full", (True, True, True))]
        for lane, flags in cases:
            outputs = dict(zip(("build_rust", "build_web", "build_smoke"), map(lambda x: str(x).lower(), flags)))
            for key, expected in zip(("BUILD_RUST", "BUILD_WEB", "BUILD_SMOKE"), flags):
                with self.subTest(lane=lane, flags=flags, key=key):
                    expression = re.search(rf"^      {key}: (.+)$", self.jobs["build"], re.M)[1]
                    self.assertEqual(self.evaluate(expression, inputs, preflight(lane=lane, **outputs)), expected)
        selected = dict(run_test="false", run_next="false", run_teletext="false", run_browser="true")
        self.assertEqual(self.scheduled(lane="pr", selection=selected),
                         set(self.jobs) - {"test", "next_engine", "teletext"})

    def test_artifact_consumers_require_their_executable_capabilities(self):
        for job, capability in (("test", "rust-tests"), ("next_engine", "smoke-server"),
                                ("teletext", "rust-tests"), ("hydration", "matched-web")):
            self.assertIn(f"--require-capability {capability}", self.jobs[job])
        build = self.jobs["build"]
        self.assertIn('if [[ "$BUILD_SMOKE" == \'true\' ]]; then\n            cargo build --locked --bin ersatzrs-web', build)
        self.assertIn('capabilities+=(--capability smoke-server)', build)

    def test_pull_request_lane_uses_the_selected_chromium_matrix(self):
        expression = re.search(r"^      matrix: (.+)$", self.jobs["hydration"], re.M)[1]
        expression = expression.removeprefix("${{").removesuffix("}}").strip()
        expression = expression.replace("&&", " and ").replace("||", " or ")
        selected = json.dumps({"include": [{"suite": "general", "project": "chromium", "port": "18409"}]})
        self.assertEqual(self.browser_matrix(expression, lane="pr", pr_matrix=selected),
                         json.loads(selected)["include"])
        full = {(row["suite"], row["project"]) for row in self.browser_matrix(expression)}
        self.assertEqual(full, {(suite, project) for suite in ("general", "1.5")
                                for project in ("chromium", "firefox")})
        # Focused dispatches keep their own matrix whatever the lane.
        self.assertEqual({row["project"] for row in self.browser_matrix(
            expression, lane="pr", pr_matrix=selected, browser_only="firefox")}, {"firefox"})

    def test_browser_suites_wait_for_a_passing_test_job(self):
        body = self.jobs["hydration"]
        needs = re.search(r"^    needs: \[(.+)\]$", body, re.M)[1]
        self.assertIn("test", {x.strip() for x in needs.split(",")})
        condition = re.search(r"^    if: (.+)$", body, re.M)[1]
        inputs = dict(test_only=False, next_engine_only=False, teletext_only="none", browser_only="none")
        for test_result, expected in (("success", True), ("skipped", False),
                                      ("failure", False), ("cancelled", False)):
            needs = preflight(lane="pr")
            needs.build = SimpleNamespace(result="success")
            needs.test = SimpleNamespace(result=test_result)
            self.assertEqual(self.evaluate(condition, inputs, needs), expected)
        needs = preflight(lane="pr", run_test="false")
        needs.build = SimpleNamespace(result="success")
        needs.test = SimpleNamespace(result="skipped")
        self.assertTrue(self.evaluate(condition, inputs, needs))

    def ci_ok_exit(self, lane, results, event="pull_request", skip="false", deselected="[]", outputs=None):
        code = re.search(r"python3 - <<'PY'\n(.*?)^          PY$", self.jobs["ci_ok"], re.S | re.M)[1]
        code = "\n".join(line[10:] for line in code.splitlines())
        needs = {job: {"result": results.get(job, "success"), "outputs": {}}
                 for job in self.jobs if job != "ci_ok"}
        needs["preflight"]["outputs"] = {"skip": skip, "lane": lane, "tree": "", "deselected": deselected}
        if lane == "pr":
            omitted = set(json.loads(deselected))
            flags = {key: job not in omitted for job, key in (("test", "run_test"), ("next_engine", "run_next"),
                     ("teletext", "run_teletext"), ("hydration", "run_browser"))}
            flags.update(build_rust=any(flags[key] for key in ("run_test", "run_next", "run_teletext")),
                         build_web=flags["run_browser"], build_smoke=flags["run_next"])
            needs["preflight"]["outputs"].update({key: str(value).lower() for key, value in flags.items()})
            needs["preflight"]["outputs"]["required_jobs"] = json.dumps(sorted(set(needs) - omitted))
        needs["preflight"]["outputs"].update(outputs or {})
        result = subprocess.run([sys.executable, "-c", code], text=True, capture_output=True,
                                env=dict(os.environ, NEEDS=json.dumps(needs), EVENT_NAME=event))
        return result.returncode

    def test_ci_ok_accepts_only_the_jobs_a_lane_leaves_out(self):
        light = {"next_engine": "skipped", "teletext": "skipped", "hydration": "skipped"}
        self.assertEqual(self.ci_ok_exit("pr", {}), 0)
        self.assertNotEqual(self.ci_ok_exit("pr", {"hydration": "skipped"}), 0)
        self.assertNotEqual(self.ci_ok_exit("pr", {"test": "failure"}), 0)
        # Domain selection: exactly the deselected jobs may be skipped.
        self.assertEqual(self.ci_ok_exit("pr", {"next_engine": "skipped"},
                                         deselected='["next_engine"]'), 0)
        self.assertNotEqual(self.ci_ok_exit("pr", {"next_engine": "skipped", "teletext": "skipped"},
                                            deselected='["next_engine"]'), 0)
        self.assertNotEqual(self.ci_ok_exit("pr", {"next_engine": "failure"},
                                            deselected='["next_engine"]'), 0)
        self.assertNotEqual(self.ci_ok_exit("full", {"next_engine": "skipped"},
                                            event="push", deselected='["next_engine"]'), 0)
        self.assertEqual(self.ci_ok_exit("full", {"commitlint": "skipped"}, event="push"), 0)
        self.assertEqual(self.ci_ok_exit("full", {"commitlint": "skipped"}, event="schedule"), 0)
        # A draft never reports a passing required check, even when green.
        self.assertNotEqual(self.ci_ok_exit("light", light), 0)
        self.assertEqual(self.ci_ok_exit("skip", {job: "skipped" for job in self.jobs
                                                  if job != "preflight"}, skip="true"), 0)

    def test_ci_ok_refuses_absent_contradictory_or_overbroad_selection(self):
        for lane in ("", "unknown"):
            self.assertNotEqual(self.ci_ok_exit(lane, {}), 0)
        for outputs in ({"run_test": None}, {"run_browser": ""}, {"build_smoke": "false"},
                        {"required_jobs": "[]"}, {"required_jobs": None}, {"deselected": "null"},
                        {"skip": "true"}):
            with self.subTest(outputs=outputs):
                self.assertNotEqual(self.ci_ok_exit("pr", {}, outputs=outputs), 0)
        self.assertNotEqual(self.ci_ok_exit("pr", {"fmt": "skipped"}, deselected='["fmt"]'), 0)
        omitted = ["next_engine", "teletext", "test"]
        for result, expected in (("skipped", 0), ("failure", 1), ("cancelled", 1)):
            self.assertEqual(bool(self.ci_ok_exit("pr", {job: result for job in omitted},
                                                 deselected=json.dumps(omitted))), bool(expected))

    def test_green_runs_record_the_tree_they_proved(self):
        body = self.jobs["ci_ok"]
        self.assertIn("name: ci-tree-${{ needs.preflight.outputs.tree }}", body)
        self.assertIn("name: ci-full-${{ needs.preflight.outputs.tree }}", body)
        self.assertEqual(body.count("if: ${{ github.event_name == 'pull_request' && needs.preflight.outputs.tree != '' }}"), 2)
        self.assertEqual(body.count("if: ${{ needs.preflight.outputs.lane == 'full' && needs.preflight.outputs.tree != '' }}"), 2)
        self.assertIn("provenBy('ci-tree', tree, ['pull_request'])", self.workflow)
        self.assertIn("provenBy('ci-full', tree, ['push', 'schedule', 'workflow_dispatch'])", self.workflow)

    def test_main_push_reuses_only_a_successful_exact_full_tree(self):
        code = re.search(r"      - id: duplicate\n.*?          script: \|\n(.*?)^      - uses:",
                         self.jobs["preflight"], re.S | re.M)[1]
        code = "\n".join(line[12:] for line in code.splitlines())
        harness = r"""
        const input = JSON.parse(require('fs').readFileSync(0, 'utf8'));
        const output = {};
        const github = {rest: {git: {getCommit: async () => ({data:{tree:{sha:'current-tree'}}})},
          actions: {listWorkflowRuns:'runs', listArtifactsForRepo:'artifacts',
            getWorkflowRun:async () => ({data:{path:'.github/workflows/ci.yml',
              event:'workflow_dispatch', head_repository:{full_name:'fixture/fixture'},
              conclusion:'success', html_url:'fixture', ...input.run}})}},
          paginate:async (method, args) => method === 'artifacts' && args.name === 'ci-full-current-tree'
            && input.match !== false ? [{expired:false,workflow_run:{id:1},...input.artifact}] : []};
        const core = {setOutput:(key,value)=>output[key]=value,info:()=>{},notice:()=>{}};
        const context = {eventName:'push',sha:'new-merge',runId:2,
                         repo:{owner:'fixture',repo:'fixture'}};
        const AsyncFunction = Object.getPrototypeOf(async function(){}).constructor;
        new AsyncFunction('github','context','core',input.code)(github,context,core)
          .then(()=>console.log(JSON.stringify(output)))
          .catch(error=>{console.error(error.message);process.exitCode=1});
        """
        cases = [({}, "skip"), ({"match": False}, "full"),
                 ({"artifact": {"expired": True}}, "full"),
                 ({"run": {"conclusion": "failure"}}, "full"),
                 ({"run": {"event": "pull_request"}}, "full"),
                 ({"run": {"path": "other.yml"}}, "full"),
                 ({"run": {"head_repository": {"full_name": "foreign/repo"}}}, "full")]
        for case, lane in cases:
            with self.subTest(case=case):
                result = subprocess.run(["node", "-e", harness],
                                        input=json.dumps({"code": code, **case}),
                                        text=True, capture_output=True)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(json.loads(result.stdout)["lane"], lane)

    def test_test_job_narrows_to_the_selected_crates(self):
        body = self.jobs["test"]
        self.assertIn("NEXTEST_FILTER: ${{ needs.preflight.outputs.nextest_filter }}", body)
        self.assertIn('& (${NEXTEST_FILTER:-all()})', body)
        self.assertIn("--no-tests fail", body)

    def test_audit_run_cannot_cancel_or_prove_a_full_run(self):
        self.assertIn("inputs.audit_only && '-audit-only'", self.workflow)
        self.assertIn("&& run.display_title !== 'CI (Audit only)'", self.workflow)
        self.assertIn("&& run.display_title !== 'CI (Next subtitle-NUL recovery)'", self.workflow)

    def test_mixed_audit_modes_are_explicitly_rejected(self):
        step = self.workflow.split("- name: Reject mixed audit mode\n", 1)[1]
        expression = step.splitlines()[0].strip().removeprefix("if: ")
        base = dict(audit_only=True, test_only=False, next_engine_only=False,
                    teletext_only="none", browser_only="none", browser_case_set="fixtures", reuse_artifacts="", next_case_set="all")
        self.assertFalse(self.evaluate(expression, base, SimpleNamespace()))
        for key, value in (("test_only", True), ("next_engine_only", True),
                           ("teletext_only", "all"), ("browser_only", "both"),
                           ("browser_case_set", "channel-preview"), ("browser_case_set", "teletext-1.5"),
                           ("browser_case_set", "teletext-runtime"), ("teletext_only", "1.5-rust"), ("reuse_artifacts", "{}"), ("next_case_set", "subtitle-nul")):
            with self.subTest(mode=key):
                self.assertTrue(self.evaluate(expression, dict(base, **{key: value}), SimpleNamespace()))

    def test_browser_case_sets_are_bounded_and_cannot_mix_focused_modes(self):
        step = self.jobs["build"].split("      - name: Validate dispatch mode\n", 1)[1]
        code = re.search(r"python3 - <<'PYCODE'\n(.*?)^          PYCODE$", step, re.S | re.M)[1]
        code = "\n".join(line[10:] for line in code.splitlines())
        base = dict(os.environ, TEST_ONLY="false", NEXT_ONLY="false", TELETEXT_ONLY="none",
                    BROWSER_ONLY="none", BROWSER_CASE_SET="fixtures", REUSE_ARTIFACTS="",
                    RUNNER_POOL="self-hosted", RESET_TARGET_DIR="false")
        cases = [({"NEXT_ONLY": "true", "NEXT_CASE_SET": "subtitle-nul"}, True),
                 ({"NEXT_CASE_SET": "subtitle-nul"}, False),
                 ({"NEXT_ONLY": "true", "NEXT_CASE_SET": "unknown"}, False),
                 ({"NEXT_ONLY": "true", "NEXT_CASE_SET": "all"}, True), ({}, True), ({"BROWSER_ONLY": "chromium", "BROWSER_CASE_SET": "channel-preview"}, True),
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
            ({"REUSE_ARTIFACTS": '{"rust":{},"web":{}}'}, True),
            ({"REUSE_ARTIFACTS": '{"rust":{}}'}, False),
            ({"REUSE_ARTIFACTS": '{"rust":{},"web":{}}', "RUNNER_POOL": "github-hosted"}, False),
            ({"REUSE_ARTIFACTS": '{"rust":{},"web":{}}', "RESET_TARGET_DIR": "true"}, False),
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
        for mode in ({}, {"teletext_only": "1.5-rust"}, {"browser_only": "both", "browser_case_set": "teletext-runtime"}):
            inputs = dict(test_only=False, next_engine_only=False, browser_only="none", teletext_only="none",
                          runner="self-hosted", reset_target_dir=False, reuse_artifacts='{"explicit":"reference"}')
            inputs.update(mode)
            build_env = {}
            for key in ("BUILD_RUST", "BUILD_WEB"):
                expression = re.search(rf"^      {key}: (.+)$", self.jobs["build"], re.M)[1]
                build_env[key] = str(self.evaluate(expression, inputs, preflight())).lower()
            self.assertEqual(build_env, {"BUILD_RUST": "false", "BUILD_WEB": "false"})
            steps = re.split(r"^      - ", self.jobs["build"], flags=re.M)[1:]
            active = []
            for step in steps:
                condition = re.search(r"^        if: (.+)$", step, re.M)
                if not condition or self.evaluate(condition[1], inputs, preflight(), SimpleNamespace(**build_env)):
                    active.append(step.splitlines()[0])
            self.assertEqual(active, ["name: Validate dispatch mode", "uses: actions/checkout@v7", "name: Reuse verified CI artifacts"])
        self.assertIn("CI_REUSE_ARTIFACTS: ${{ inputs.reuse_artifacts }}", self.workflow)
        self.assertIn("REUSE_ARTIFACTS: ${{ inputs.reuse_artifacts }}", self.jobs["build"])

    def test_runner_pool_falls_back_to_github_hosted(self):
        runs_on = {name: re.search(r"^    runs-on: (.+)$", body, re.M)[1]
                   for name, body in self.jobs.items() if name != "preflight"
                   and re.search(r"^    runs-on: .*self-hosted", body, re.M)}
        self.assertGreater(len(runs_on), 5)
        for name, expression in runs_on.items():
            with self.subTest(job=name):
                self.assertIn("needs.preflight.outputs.pool == 'github-hosted'", expression)
                needs = re.search(r"^    needs: (.+)$", self.jobs[name], re.M)
                self.assertTrue(needs and "preflight" in needs[1], f"{name} must need preflight")
                pick = re.search(r"fromJSON\((.+?) && '\"", expression)[1]
                for runner, pool, hosted in (("", "", False), ("", "github-hosted", True),
                                             ("self-hosted", "", False), ("github-hosted", "", True)):
                    self.assertEqual(self.evaluate(pick, dict(runner=runner), preflight(pool=pool)), hosted)
        step = self.jobs["preflight"].split("name: Choose the runner pool", 1)[1]
        self.assertIn("if: ${{ !inputs.runner }}", step)
        self.assertIn("secrets.CI_RUNNER_STATUS_TOKEN", step)
        self.assertIn("continue-on-error: true", step.split("script:", 1)[0])
        self.assertIn("github-authentication-token-expiration", step)
        self.assertIn("pool: ${{ steps.pool.outputs.pool }}", self.jobs["preflight"])

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
