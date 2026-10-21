#!/usr/bin/env python3
"""CI artifact handoff regressions; fixtures are never executed."""

import importlib.util
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


if __name__ == "__main__":
    unittest.main()
