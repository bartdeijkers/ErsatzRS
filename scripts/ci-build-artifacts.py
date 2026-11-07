#!/usr/bin/env python3
"""Package and verify exact-source CI executables and a matched Leptos site."""

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import platform
import re
import shutil
import stat
import subprocess
import sys
import tarfile
import tempfile
import tomllib
from contextlib import suppress


REQUIRED = {
    "rust": ("nextest.tar.zst", "bin/cargo-nextest", "bin/xtask", "bin/provision_runtime", "bin/ersatzrs-web-smoke"),
    "web": ("bin/xtask", "bin/provision_runtime", "bin/ersatzrs-web",
            "site/pkg/ersatzrs-web.js", "site/pkg/ersatzrs-web.wasm",
            "site/bundle.css", "site/channel-preview.js", "site/vendor/hls/hls.mjs",
            "site/vendor/hls/LICENSE"),
}
# Schema 1 required the complete Rust product, including its smoke server.
# Schema 2 makes that optional but keeps every consumer's requirement explicit.
CAPABILITIES = {
    "rust": {"rust-tests": REQUIRED["rust"][:-1],
             "smoke-server": ("bin/ersatzrs-web-smoke",)},
    "web": {"matched-web": REQUIRED["web"]},
}
BASE_CAPABILITY = {"rust": "rust-tests", "web": "matched-web"}
LOCAL_GROUP_MARKER = ".ersatzrs-ci-artifacts"
LOCAL_GROUP_OWNER = b"ersatzrs-ci-artifacts-group-v1\n"
BUILD_FLAG_KEYS = ("RUSTFLAGS", "CARGO_PROFILE_TEST_DEBUG", "LEPTOS_OUTPUT_NAME", "LEPTOS_HASH_FILES")
# Explicit recovery-only edits that cannot change compiled/runtime inputs.
# The qualified browser spec is executed from the checkout, never compiled
# into these products. Its changes invalidate its browser suite's results.
# Rust tests, runtime pins and Cargo configuration still fail closed.
REUSE_SOURCE_EXCEPTIONS = {
    ".github/workflows/ci.yml", "scripts/ci-build-artifacts.py", "scripts/ci-build-artifacts_test.py",
    "scripts/ci-teletext-selection.py", "scripts/ci-teletext-selection_test.py",
    "BUILD.md", "docs/progress/milestones.toml", "docs/progress/phase.toml",
    "docs/progress/sessions.ndjson", "docs/progress/parity/upstream-baseline-catch-up.toml",
    "tests/e2e/ffmpeg-profiles.spec.ts",
}


def digest(path):
    with path.open("rb") as handle:
        return hashlib.file_digest(handle, "sha256").hexdigest()


def host():
    return {"system": platform.system(), "machine": platform.machine(),
            "libc": list(platform.libc_ver())}


def capability_names(kind, names):
    if (not isinstance(names, list) or not names
            or any(not isinstance(name, str) for name in names)
            or len(set(names)) != len(names) or set(names) - CAPABILITIES[kind].keys()
            or BASE_CAPABILITY[kind] not in names):
        raise ValueError("invalid CI artifact capabilities")
    return sorted(names)


def check_required(root, kind, capabilities):
    required = {name for capability in capabilities for name in CAPABILITIES[kind][capability]}
    for name in required:
        path = root / name
        if not path.is_file() or not path.stat().st_size:
            raise ValueError(f"missing or empty CI build product: {name}")
    for name in required:
        if name.startswith("bin/") and not os.access(root / name, os.X_OK):
            raise ValueError(f"CI executable lost its executable permission: {name}")
    for name in set(REQUIRED[kind]) - required:
        if (root / name).exists():
            raise ValueError(f"undeclared CI build capability product: {name}")


def pack(root, archive, sha, kind, capabilities=None):
    capabilities = capability_names(kind, list(CAPABILITIES[kind]) if capabilities is None else capabilities)
    check_required(root, kind, capabilities)
    actual_sha = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    if sha != actual_sha:
        raise ValueError("build source SHA differs from the requested revision")
    files = {}
    for path in sorted(root.rglob("*")):
        if path.is_symlink():
            raise ValueError("CI build products must not contain symlinks")
        if path.is_file():
            name = path.relative_to(root).as_posix()
            if name == "build.json":
                raise ValueError("refusing to reuse an existing build manifest")
            files[name] = {"sha256": digest(path), "size": path.stat().st_size,
                           "executable": bool(path.stat().st_mode & 0o111)}
    manifest = {"schema_version": 2, "capabilities": capabilities,
                "source_sha": sha, "kind": kind, "host": host(), "files": files,
                "toolchain": subprocess.check_output(["rustc", "--version"], text=True).strip(),
                "build_flags": {key: os.environ.get(key, "") for key in (
                    "RUSTFLAGS", "CARGO_PROFILE_TEST_DEBUG", "LEPTOS_OUTPUT_NAME", "LEPTOS_HASH_FILES")}}
    (root / "build.json").write_text(json.dumps(manifest, indent=2) + "\n")
    with tarfile.open(archive, "w:gz", compresslevel=1) as handle:
        handle.add(root, arcname=".", recursive=True)


def verify(root, sha, kind, reference=None, required_capabilities=()):
    manifest = json.loads((root / "build.json").read_text())
    producer = reference["source_sha"] if reference else sha
    if manifest.get("schema_version") not in (1, 2) or manifest.get("source_sha") != producer or manifest.get("kind") != kind:
        raise ValueError("CI build manifest has the wrong schema or source SHA")
    if manifest["schema_version"] == 1:
        if "capabilities" in manifest:
            raise ValueError("legacy CI manifest cannot declare partial capabilities")
        capabilities = list(CAPABILITIES[kind])
    else:
        capabilities = capability_names(kind, manifest.get("capabilities"))
    if set(required_capabilities) - set(capabilities):
        raise ValueError("CI artifact lacks required consumer capabilities")
    if manifest.get("host") != host():
        raise ValueError("CI build and execution platform/libc differ")
    if any(p.is_symlink() for p in root.rglob("*")):
        raise ValueError("CI build products must not contain symlinks")
    actual = {p.relative_to(root).as_posix() for p in root.rglob("*") if p.is_file()}
    if actual != set(manifest["files"]) | {"build.json"}:
        raise ValueError("CI build product inventory differs from its manifest")
    for name, expected in manifest["files"].items():
        path = root / name
        if path.is_symlink() or path.stat().st_size != expected["size"] or digest(path) != expected["sha256"]:
            raise ValueError(f"CI build product hash/size mismatch: {name}")
        if bool(path.stat().st_mode & 0o111) != expected["executable"]:
            raise ValueError(f"CI build product permission mismatch: {name}")
    check_required(root, kind, capabilities)
    if reference:
        verify_reuse_options(manifest, reference, kind)
    return capabilities


def restore(archive, root, sha, env_file, kind, reference=None, required_capabilities=()):
    if reference and digest(archive) != reference["sha256"]:
        raise ValueError("reused CI artifact archive SHA256 differs")
    if root.exists():
        raise ValueError("CI build destination already exists; refusing stale files")
    root.mkdir(parents=True)
    try:
        with tarfile.open(archive, "r:gz") as handle:
            seen = set()
            for entry in handle.getmembers():
                path = PurePosixPath(entry.name)
                if path.is_absolute() or ".." in path.parts or not (entry.isfile() or entry.isdir()):
                    raise ValueError("unsafe CI build archive member")
                if path.as_posix() in seen:
                    raise ValueError("duplicate CI build archive member")
                seen.add(path.as_posix())
            handle.extractall(root, filter="data")
        capabilities = verify(root, sha, kind, reference, required_capabilities)
        if env_file:
            values = {
                "CI_BUILD_DIR": root,
                "CI_XTASK": root / "bin/xtask",
                "CI_PROVISION_RUNTIME": root / "bin/provision_runtime",
            }
            if kind == "rust":
                values.update({"CI_NEXTEST_ARCHIVE": root / "nextest.tar.zst",
                               "CI_NEXTEST": root / "bin/cargo-nextest"})
                if "smoke-server" in capabilities:
                    values["CI_SMOKE_SERVER"] = root / "bin/ersatzrs-web-smoke"
            else:
                values["ERSATZRS_E2E_PREBUILT_DIR"] = root
            if any(c in str(value) for value in values.values() for c in "\r\n"):
                raise ValueError("CI build path cannot be represented in an environment file")
            with env_file.open("a") as handle:
                for key, value in values.items():
                    handle.write(f"{key}={value}\n")
    except Exception:
        shutil.rmtree(root)
        raise


def reuse_reference(value, kind):
    references = json.loads(value)
    if not isinstance(references, dict) or not references or set(references) - set(REQUIRED):
        raise ValueError("reuse references must name only Rust and/or web artifacts")
    for item_kind, item in references.items():
        keys = {"source_sha", "cache_key", "sha256", "toolchain", "build_flags"}
        if not isinstance(item, dict) or set(item) != keys:
            raise ValueError("incomplete CI artifact reuse reference")
        if not isinstance(item["source_sha"], str) or not re.fullmatch(r"[0-9a-f]{40}", item["source_sha"]):
            raise ValueError("reuse requires an exact producer SHA")
        if not isinstance(item["sha256"], str) or not re.fullmatch(r"[0-9a-f]{64}", item["sha256"]):
            raise ValueError("reuse requires an exact archive SHA256")
        if not isinstance(item["cache_key"], str) or local_entry(Path("."), item["cache_key"])[2] != item_kind:
            raise ValueError("reuse cache key has the wrong artifact kind")
        if not isinstance(item["toolchain"], str) or not item["toolchain"].startswith("rustc ") or any(c in item["toolchain"] for c in "\r\n"):
            raise ValueError("reuse requires the exact recorded compiler")
        flags = item["build_flags"]
        if not isinstance(flags, dict) or set(flags) != set(BUILD_FLAG_KEYS) or any(
            not isinstance(v, str) or any(c in v for c in "\r\n") for v in flags.values()
        ):
            raise ValueError("reuse requires every recorded build flag")
    if kind not in references:
        raise ValueError("missing required CI artifact reuse reference")
    return references[kind]


def verify_source_equivalence(producer, execution):
    """Keep the producer identity and refuse any compiled/runtime/test input drift."""
    if any(not re.fullmatch(r"[0-9a-f]{40}", sha) for sha in (producer, execution)):
        raise ValueError("source equivalence requires full commit SHAs")
    available = subprocess.run(["git", "cat-file", "-e", f"{producer}^{{commit}}"],
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
    if available.returncode:
        subprocess.run(["git", "fetch", "--no-tags", "--depth=1", "origin", producer], check=True)
    changes = set()
    for revisions in ((producer, execution), ("HEAD",)):
        changed = subprocess.check_output(["git", "diff", "--name-only", "--no-renames", "-z", *revisions, "--"])
        changes.update(name for name in changed.decode().split("\0") if name)
    forbidden = changes - REUSE_SOURCE_EXCEPTIONS
    if forbidden:
        raise ValueError("CI artifact source inputs changed: " + ", ".join(sorted(forbidden)))
    return sorted(changes)


def verify_reuse_options(manifest, reference, kind):
    if manifest.get("toolchain") != reference["toolchain"] or manifest.get("build_flags") != reference["build_flags"]:
        raise ValueError("reused CI compiler/build flags differ from their pinned reference")
    installed = subprocess.check_output(["rustc", "--version"], text=True,
                                       env=dict(os.environ, RUSTUP_AUTO_INSTALL="0")).strip()
    if installed != reference["toolchain"]:
        raise ValueError("reused CI compiler differs from the execution toolchain")
    flags = reference["build_flags"]
    if flags["RUSTFLAGS"] or (kind == "rust" and flags["CARGO_PROFILE_TEST_DEBUG"] != "0"):
        raise ValueError("reused CI artifact has incompatible native/test flags")
    if kind == "web":
        source = subprocess.check_output(["git", "show", f"{reference['source_sha']}:crates/ersatzrs-web/Cargo.toml"], text=True)
        leptos = tomllib.loads(source)["package"]["metadata"]["leptos"]
        output = flags["LEPTOS_OUTPUT_NAME"] or leptos.get("output-name")
        hashed = flags["LEPTOS_HASH_FILES"] or str(leptos.get("hash-files")).lower()
        if output != "ersatzrs-web" or hashed != "false":
            raise ValueError("reused CI web artifact has incompatible Leptos output settings")


def resume_job_signature(workflow, job):
    """Compare consumer execution, allowing only its scheduling to change."""
    body = re.search(rf"^  {job}:\n(.*?)(?=^  [a-z][a-z0-9_]*:|\Z)",
                     workflow.split("\njobs:\n", 1)[1], re.M | re.S)[1]
    body = re.sub(r"^    if: .*\n", "", body, flags=re.M)
    if job == "hydration":
        body = re.sub(r"^      matrix: .*\n", "", body, flags=re.M)
    return body


def resume_source_guard(changes, old_helper, new_helper, old_workflow, new_workflow):
    """Result reuse is stricter than executable reuse: selectors cannot drift."""
    import ast

    allowed = {
        ".github/workflows/ci.yml", "scripts/ci-build-artifacts.py", "scripts/ci-build-artifacts_test.py",
        "BUILD.md", "docs/progress/milestones.toml", "docs/progress/phase.toml",
        "docs/progress/sessions.ndjson", "docs/progress/parity/upstream-baseline-catch-up.toml",
        "tests/e2e/ffmpeg-profiles.spec.ts",
    }
    if set(changes) - allowed:
        raise ValueError("completed CI check inputs changed")
    def consumer_code(source):
        tree = ast.parse(source)
        # Qualification is independently checked above. Archive validation,
        # restoration and execution must remain byte-for-byte equivalent ASTs.
        tree.body = [node for node in tree.body if not (
            isinstance(node, ast.FunctionDef) and node.name.startswith("resume_")
            and not node.decorator_list) and not (
            isinstance(node, ast.Assign) and any(isinstance(target, ast.Name)
                and target.id == 'REUSE_SOURCE_EXCEPTIONS' for target in node.targets))]
        return ast.dump(tree, include_attributes=False)
    if consumer_code(old_helper) != consumer_code(new_helper):
        raise ValueError("CI artifact consumer implementation changed")
    # Inputs are above concurrency. Global flags, defaults and permissions
    # below it must be identical, independently of individual job signatures.
    def settings(source):
        return source.split("\nconcurrency:\n", 1)[1].split("\njobs:\n", 1)[0]
    if settings(old_workflow) != settings(new_workflow):
        raise ValueError("global CI execution settings changed")


def resume_artifacts_guard(log, references, execution_sha):
    """Bind imported results to the same fully verified immutable products."""
    for kind in ("rust", "web"):
        reference = reuse_reference(references, kind)
        records = []
        for line in log.splitlines():
            try:
                value = json.loads(line[line.index('{'):])
            except (ValueError, json.JSONDecodeError):
                continue
            if isinstance(value, dict) and f"{kind}_cache_key" in value:
                records.append(value)
        if len(records) != 1 or any(records[0].get(f"{kind}_{key}") != expected for key, expected in {
            "cache_key": reference["cache_key"], "digest": reference["sha256"],
            "source_sha": reference["source_sha"], "equivalent_execution_sha": execution_sha,
        }.items()):
            raise ValueError("completed CI check artifact provenance differs")


def resume_browser_cases(rows, logs, changes):
    """Narrow replay only after both original general suites fully reported."""
    selected = {path for path in changes if path == 'tests/e2e/ffmpeg-profiles.spec.ts'}
    evidence = []
    for row in rows:
        project = 'chromium' if row['name'].endswith('chromium)') else 'firefox'
        log = logs.get(row['name'], '')
        plain = re.sub(r'\x1b\[[0-9;]*[A-Za-z]', '', log)
        plain = re.sub(r'^\ufeff?\d{4}-\d\d-\d\dT\S+ ', '', plain, flags=re.M)
        runs = re.findall(r'^Running (\d+) tests? using ', plain, re.M)
        projects = set(re.findall(r'^\[\d+/\d+\] \[([^]]+)\]', plain, re.M))
        def count(word):
            matches = re.findall(rf'^\s*(\d+) {word}(?:\s|$)', plain, re.M)
            return int(matches[-1]) if matches else 0
        passed, failed, skipped = count('passed'), count('failed'), count('skipped')
        failures = re.findall(r'^\s*\d+\) \[' + re.escape(project)
                              + r'\] › (tests/e2e/[^\s:]+\.spec\.ts):', plain, re.M)
        if (len(runs) != 1 or projects != {project} or passed <= 0
                or int(runs[0]) != passed + failed + skipped
                or len(failures) != failed
                or row.get('conclusion') not in ('success', 'failure')
                or (row['conclusion'] == 'failure') != bool(failed)
                or any(not re.fullmatch(r'tests/e2e/[^/]+\.spec\.ts', path) for path in failures)):
            return '', []
        selected.update(failures)
        evidence.append({'job_id': row['id'], 'project': project, 'listed': int(runs[0]),
                         'passed': passed, 'failed': failed, 'skipped': skipped,
                         'failed_specs': sorted(set(failures)),
                         'log_sha256': hashlib.sha256(log.encode()).hexdigest()})
    if not selected or len(evidence) != 2:
        return '', []
    pattern = '(?:' + '|'.join(re.escape(path) for path in sorted(selected)) + ')$'
    return pattern, evidence


def resume_plan(run, jobs, repository, sha, old_workflow, new_workflow,
                old_helper, new_helper, changes, references, producer_log, browser_logs=None):
    """Reuse only actual successful jobs from a completed ordinary full run."""
    if (run.get("status") != "completed" or run.get("conclusion") not in ("success", "failure")
            or run.get("path") != ".github/workflows/ci.yml"
            or run.get("event") not in ("push", "schedule", "workflow_dispatch")
            or run.get("head_repository", {}).get("full_name") != repository
            or not re.fullmatch(r"[0-9a-f]{40}", run.get("head_sha", ""))
            or type(run.get("id")) is not int or run["id"] <= 0
            or not re.fullmatch(r"[0-9a-f]{40}", sha)):
        raise ValueError("resume requires a completed same-repository full CI run")
    rows = jobs.get("jobs", [])
    by_name = {row["name"]: row for row in rows}
    browsers = [
        {"suite": suite, "project": project, "port": str(port)}
        for suite, project, port in (("general", "chromium", 18409),
                                    ("general", "firefox", 18410),
                                    ("1.5", "chromium", 18411), ("1.5", "firefox", 18412))
    ]
    browser_names = [f"Playwright E2E ({row['suite']}, {row['project']})" for row in browsers]
    required = {"Preflight", "Format", "Documentation links", "Clippy", "Commit lint",
                "Cargo deny", "Test", "Next engine", "Teletext 1.5", "CI ok", *browser_names}
    producers = set(by_name) & {"Verify reused CI artifacts"}
    if (len(producers) != 1 or set(by_name) != required | producers
            or len(rows) != len(by_name) or jobs.get("total_count") != len(rows)
            or any(row.get("status") != "completed" for row in rows)
            or any(row.get("run_id") != run["id"] for row in rows)
            or any(type(row.get("id")) is not int or row["id"] <= 0 for row in rows)
            or len({row["id"] for row in rows}) != len(rows)):
        raise ValueError("resume requires the complete ordinary full CI job inventory")
    if by_name['Verify reused CI artifacts'].get('conclusion') != 'success':
        raise ValueError('previous artifact verification did not pass')
    if (by_name['Preflight'].get('conclusion') != 'success'
            or by_name['CI ok'].get('conclusion') not in ('success', 'failure')):
        raise ValueError('previous run did not execute the full gate')
    resume_artifacts_guard(producer_log, references, run['head_sha'])
    resume_source_guard(changes, old_helper, new_helper, old_workflow, new_workflow)
    scopes = {"clippy": ["Clippy"], "test": ["Test"], "next_engine": ["Next engine"],
              "teletext": ["Teletext 1.5"], "hydration": browser_names}
    reused = {}
    passed_browsers = []
    pending_browsers = list(browsers)
    for job, names in scopes.items():
        if resume_job_signature(old_workflow, job) != resume_job_signature(new_workflow, job):
            continue
        invalidated = {name for name in names if job == 'hydration'
                       and 'tests/e2e/ffmpeg-profiles.spec.ts' in changes
                       and name.startswith('Playwright E2E (general,')}
        passed = [by_name[name]["id"] for name in names
                  if by_name[name].get("conclusion") == "success" and name not in invalidated]
        if len(passed) == len(names):
            reused[job] = passed
        if job == "hydration":
            passed_browsers = passed
            pending_browsers = [row for row, name in zip(browsers, names)
                                if by_name[name].get("conclusion") != "success" or name in invalidated]
    general = [by_name[name] for name in browser_names[:2]]
    replay_filter, case_proof = resume_browser_cases(general, browser_logs or {}, changes)
    return {"run_id": run["id"], "source_sha": run["head_sha"], "execution_sha": sha,
            "jobs": reused, "browser_passed": passed_browsers,
            "browser_pending": pending_browsers, "general_filter": replay_filter,
            "browser_case_proof": case_proof}


def resume_checks(run, jobs, repository, sha, references, producer_log, browser_logs=None):
    """Bind the API proof to clean exact source before exporting any skips."""
    if subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip() != sha:
        raise ValueError("resume checkout differs from the execution SHA")
    if subprocess.run(["git", "diff", "--quiet", "HEAD", "--"], check=False).returncode:
        raise ValueError("resume requires a clean tracked source checkout")
    producer = run.get("head_sha", "")
    if (not re.fullmatch(r"[0-9a-f]{40}", producer)
            or run.get("head_repository", {}).get("full_name") != repository):
        raise ValueError("resume source is not a same-repository commit")
    changes = verify_source_equivalence(producer, sha)
    def source(revision, path):
        return subprocess.check_output(["git", "show", f"{revision}:{path}"], text=True)
    return resume_plan(run, jobs, repository, sha,
                       source(producer, ".github/workflows/ci.yml"),
                       source(sha, ".github/workflows/ci.yml"),
                       source(producer, "scripts/ci-build-artifacts.py"),
                       source(sha, "scripts/ci-build-artifacts.py"), changes,
                       references, producer_log, browser_logs)


def workspace_test_targets(metadata):
    """Match Cargo's default lib/bin/integration selection without compiling."""
    packages = {package["id"]: package for package in metadata["packages"]}
    features = {node["id"]: set(node["features"]) for node in metadata["resolve"]["nodes"]}
    targets = set()
    for member in metadata["workspace_members"]:
        package = packages[member]
        for target in package["targets"]:
            if not target["test"] or not set(target.get("required-features", ())) <= features[member]:
                continue
            kinds = set(target["kind"])
            if kinds & {"lib", "rlib", "cdylib", "dylib", "staticlib", "proc-macro"}:
                kind = "lib"
            elif kinds in ({"bin"}, {"test"}):
                kind = target["kind"][0]
            else:
                continue
            targets.add((package["name"], target["name"], kind))
    if not targets:
        raise ValueError("workspace has no test targets")
    return targets


def verify_workspace_test_coverage(current, archived, binaries, catalog=None):
    """A selected-package archive must never stand in for a full workspace."""
    packages = {package["id"]: package["name"] for package in archived["packages"]}
    members = {packages[member] for member in archived["workspace_members"]}
    current_packages = {package["id"]: package["name"] for package in current["packages"]}
    if members != {current_packages[member] for member in current["workspace_members"]}:
        raise ValueError("reused archive has a different workspace membership")
    def workspace_features(metadata):
        nodes = {node["id"]: node["features"] for node in metadata["resolve"]["nodes"]}
        names = {package["id"]: package["name"] for package in metadata["packages"]}
        return {names[member]: sorted(nodes[member]) for member in metadata["workspace_members"]}
    if workspace_features(current if catalog is None else catalog) != workspace_features(archived):
        raise ValueError("reused archive has a different workspace feature graph")
    actual = set()
    for binary in binaries["rust-binaries"].values():
        package = packages[binary["package-id"]]
        if package not in members:
            raise ValueError("reused archive contains a non-workspace test binary")
        kind = "lib" if binary["kind"] == "proc-macro" else binary["kind"]
        actual.add((package, binary["binary-name"], kind))
    expected = workspace_test_targets(current)
    missing = expected - actual
    if missing:
        names = ", ".join(f"{package}::{kind}/{name}" for package, name, kind in sorted(missing))
        raise ValueError("reused archive omits workspace test targets: " + names)
    return len(expected)


def verify_full_workspace_archive(root):
    """Use the verified nextest executable to inspect, never run, its archive."""
    environment = dict(os.environ, RUSTUP_AUTO_INSTALL="0")
    current = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--format-version", "1", "--locked", "--offline"],
        text=True, env=environment))
    # Nextest's archived Cargo metadata is an all-features catalog, even for
    # default-feature test builds. Compare that catalog separately; eligible
    # test targets still come from the current default-feature resolution.
    catalog = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--format-version", "1", "--locked", "--offline", "--all-features"],
        text=True, env=environment))
    with tempfile.TemporaryDirectory(prefix="ci-full-coverage-") as temporary:
        binaries = json.loads(subprocess.check_output([
            str(root / "bin/cargo-nextest"), "nextest", "list",
            "--archive-file", str(root / "nextest.tar.zst"),
            "--extract-to", temporary, "--workspace-remap", str(Path.cwd()),
            "--list-type", "binaries-only", "--message-format", "json",
        ], text=True, env=environment))
        archived = json.loads((Path(temporary) / "target/nextest/cargo-metadata.json").read_text())
        return verify_workspace_test_coverage(current, archived, binaries, catalog)


def reuse_local(archive, sha, kind, reference, cache, full_workspace=False):
    """Verify existing immutable bytes without building, relabelling or publishing."""
    changed = verify_source_equivalence(reference["source_sha"], sha)
    if not fetch_local(archive, reference["sha256"], reference["cache_key"], cache):
        raise ValueError("required reused CI artifact is unavailable; no rebuild was started")
    with tempfile.TemporaryDirectory(prefix="ci-reuse-") as temporary:
        root = Path(temporary) / "verified"
        restore(archive, root, sha, None, kind, reference,
                required_capabilities=("rust-tests", "smoke-server") if full_workspace else ())
        if full_workspace:
            verify_full_workspace_archive(root)
    return {f"{kind}_cache_key": reference["cache_key"], f"{kind}_digest": reference["sha256"],
            f"{kind}_source_sha": reference["source_sha"],
            f"{kind}_equivalent_execution_sha": sha,
            f"{kind}_allowed_source_changes": json.dumps(changed, separators=(",", ":"))}


def local_entry(cache, key):
    """Separate producer groups from their two immutable artifact kinds."""
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]{0,199}", key):
        raise ValueError("unsafe local CI artifact cache key")
    group, separator, kind = key.rpartition("-")
    if not separator or not group or kind not in REQUIRED:
        raise ValueError("local CI artifact cache key must end in -rust or -web")
    group_path = cache / group
    return group_path, group_path / f"{kind}.tar.gz", kind


def check_cache_directories(cache, group):
    if cache.is_symlink() or group.is_symlink():
        raise ValueError("local CI artifact cache directories must not be symlinks")


def regular_entry(path):
    """Reject symlinks and other non-file entries instead of following them."""
    try:
        mode = path.lstat().st_mode
    except FileNotFoundError:
        return False
    if not stat.S_ISREG(mode):
        raise ValueError("local CI artifact cache entry must be a regular file")
    return True


def owned_local_group(group, strict=True):
    marker = group / LOCAL_GROUP_MARKER
    try:
        if not regular_entry(marker):
            return False
        if marker.stat().st_size != len(LOCAL_GROUP_OWNER) or marker.read_bytes() != LOCAL_GROUP_OWNER:
            raise ValueError("local CI artifact cache ownership marker differs")
        return True
    except ValueError:
        if strict:
            raise
        return False


def initialize_local_group(cache, group):
    """Claim only an empty group, using one complete atomic ownership marker."""
    if owned_local_group(group):
        return
    if any(group.iterdir()):
        # Another producer may have published its marker after our first read.
        if owned_local_group(group):
            return
        raise ValueError("refusing nonempty unowned local CI artifact cache group")
    descriptor, name = tempfile.mkstemp(prefix=".owner-", dir=cache)
    temporary = Path(name)
    try:
        with os.fdopen(descriptor, "wb") as handle:
            handle.write(LOCAL_GROUP_OWNER)
            handle.flush()
            os.fsync(handle.fileno())
        try:
            os.link(temporary, group / LOCAL_GROUP_MARKER)
        except FileExistsError:
            if not owned_local_group(group):
                raise ValueError("local CI artifact cache ownership marker differs") from None
    finally:
        with suppress(OSError):
            temporary.unlink()


def prune_local(cache, current):
    """Keep at most four producer groups, preserving the just-published group."""
    groups = [path for path in cache.iterdir()
              if re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]{0,199}", path.name)
              and not path.is_symlink() and path.is_dir()
              and owned_local_group(path, strict=False)]
    old = sorted((path for path in groups if path != current),
                 key=lambda path: path.stat().st_mtime_ns)
    for path in old[:max(0, len(groups) - 4)]:
        shutil.rmtree(path)


def publish_local(archive: Path, sha: str, kind: str, key: str, cache: Path) -> str:
    """Publish digest-bound bytes; pack/restore retain source and host authority."""
    if not re.fullmatch(r"[0-9a-f]{40}", sha) or kind not in REQUIRED:
        raise ValueError("local CI artifact publication needs a source SHA and kind")
    group, entry, key_kind = local_entry(cache, key)
    if kind != key_kind:
        raise ValueError("local CI artifact cache key has the wrong kind")
    if not regular_entry(archive):
        raise ValueError("local CI artifact archive is missing")
    expected = digest(archive)
    temporary = None
    try:
        check_cache_directories(cache, group)
        group.mkdir(parents=True, exist_ok=True)
        initialize_local_group(cache, group)
        if regular_entry(entry):
            if digest(entry) != expected:
                raise ValueError("immutable local CI artifact cache entry differs")
        else:
            descriptor, name = tempfile.mkstemp(prefix=".publish-", dir=group)
            os.close(descriptor)
            temporary = Path(name)
            shutil.copyfile(archive, temporary)
            if digest(temporary) != expected:
                raise ValueError("local CI artifact changed during publication")
            with temporary.open("rb") as handle:
                os.fsync(handle.fileno())
            try:
                os.link(temporary, entry)
            except FileExistsError:
                if not regular_entry(entry) or digest(entry) != expected:
                    raise ValueError("immutable local CI artifact cache entry differs") from None
        os.utime(group, None)
        prune_local(cache, group)
    except OSError:
        print("Local CI artifact cache unavailable; using GitHub artifact fallback", file=sys.stderr)
    finally:
        if temporary is not None:
            with suppress(OSError):
                temporary.unlink()
    return expected


def fetch_local(archive: Path, expected_digest: str, key: str, cache: Path) -> bool:
    """Expose a cache hit only after a complete temporary copy matches its hash."""
    if not re.fullmatch(r"[0-9a-f]{64}", expected_digest):
        raise ValueError("a full CI artifact SHA256 is required")
    group, entry, _ = local_entry(cache, key)
    if archive.is_symlink():
        raise ValueError("local CI artifact destination must not be a symlink")
    temporary = None
    try:
        check_cache_directories(cache, group)
        if not owned_local_group(group):
            return False
        if not regular_entry(entry):
            return False
        archive.parent.mkdir(parents=True, exist_ok=True)
        descriptor, name = tempfile.mkstemp(prefix=".fetch-", dir=archive.parent)
        os.close(descriptor)
        temporary = Path(name)
        shutil.copyfile(entry, temporary)
        if digest(temporary) != expected_digest:
            raise ValueError("local CI artifact cache hash mismatch")
        os.replace(temporary, archive)
        return True
    except OSError:
        print("Local CI artifact cache unavailable; using GitHub artifact fallback", file=sys.stderr)
        return False
    finally:
        if temporary is not None:
            with suppress(OSError):
                temporary.unlink()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("pack", "restore", "verify", "publish-local", "fetch-local", "reuse"))
    parser.add_argument("--directory", type=Path)
    parser.add_argument("--kind", choices=("rust", "web"), required=True)
    parser.add_argument("--archive", type=Path)
    parser.add_argument("--sha", required=True)
    parser.add_argument("--env-file", type=Path)
    parser.add_argument("--cache-key")
    parser.add_argument("--expected-digest")
    parser.add_argument("--output-file", type=Path)
    parser.add_argument("--capability", action="append", help="pack: declare each provided capability")
    parser.add_argument("--require-capability", action="append", default=[],
                        help="restore/verify: refuse before exporting paths unless provided")
    parser.add_argument("--require-full-workspace", action="store_true",
                        help="reuse rust: require every current workspace test target and smoke server")
    args = parser.parse_args()
    if args.capability is not None and args.mode != "pack":
        parser.error("--capability is only valid when packing")
    if args.require_capability and args.mode not in ("restore", "verify"):
        parser.error("--require-capability is only valid when restoring or verifying")
    if args.require_full_workspace and (args.mode != "reuse" or args.kind != "rust"):
        parser.error("--require-full-workspace is only valid for Rust artifact reuse")
    if not re.fullmatch(r"[0-9a-f]{40}", args.sha):
        parser.error("a full source SHA is required")
    local = args.mode in ("publish-local", "fetch-local")
    if not local and args.mode != "reuse" and not args.directory:
        parser.error("pack/restore/verify requires --directory")
    if args.mode != "verify" and not args.archive:
        parser.error("this mode requires --archive")
    if local and not args.cache_key:
        parser.error("local cache modes require --cache-key")
    if args.mode == "fetch-local" and not args.expected_digest:
        parser.error("fetch-local requires --expected-digest")
    actual_sha = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    if args.sha != actual_sha:
        parser.error("execution checkout differs from the requested source SHA")
    reuse = os.environ.get("CI_REUSE_ARTIFACTS", "")
    reference = reuse_reference(reuse, args.kind) if reuse else None
    if reference and args.mode in ("pack", "publish-local"):
        parser.error("reuse cannot build or publish a replacement artifact")
    cache = Path(os.environ.get("CI_LOCAL_ARTIFACT_CACHE", "")) if os.environ.get(
        "CI_LOCAL_ARTIFACT_CACHE") else Path.home() / ".cache/ersatzrs/ci-artifacts"
    if args.mode == "reuse":
        if reference is None:
            parser.error("reuse requires explicit CI_REUSE_ARTIFACTS references")
        values = reuse_local(args.archive, args.sha, args.kind, reference, cache,
                             full_workspace=args.require_full_workspace)
        if args.output_file:
            with args.output_file.open("a") as handle:
                for key, value in values.items():
                    handle.write(f"{key}={value}\n")
        print(json.dumps(values, sort_keys=True))
        return
    if reference:
        verify_source_equivalence(reference["source_sha"], args.sha)
    if local:
        if local_entry(cache, args.cache_key)[2] != args.kind:
            parser.error("local CI artifact cache key has the wrong kind")
        if args.mode == "publish-local":
            value = "digest=" + publish_local(args.archive, args.sha, args.kind, args.cache_key, cache)
        else:
            if reference and (args.cache_key != reference["cache_key"] or args.expected_digest != reference["sha256"]):
                parser.error("reuse fetch differs from its pinned artifact reference")
            hit = fetch_local(args.archive, args.expected_digest, args.cache_key, cache)
            if reference and not hit:
                raise ValueError("required reused CI artifact is unavailable; no rebuild was started")
            value = "hit=" + str(hit).lower()
        if args.output_file:
            with args.output_file.open("a") as handle:
                handle.write(value + "\n")
        print(value)
        return
    root = args.directory.resolve()
    if args.mode == "pack":
        pack(root, args.archive, args.sha, args.kind, args.capability)
    elif args.mode == "restore":
        restore(args.archive, root, args.sha, args.env_file, args.kind, reference, args.require_capability)
    else:
        verify(root, args.sha, args.kind, reference, args.require_capability)
    if reference:
        print(f"CI build {args.mode}: verified producer {reference['source_sha']} for equivalent execution {args.sha}")
    else:
        print(f"CI build {args.mode}: verified source {args.sha}")


if __name__ == "__main__":
    main()
