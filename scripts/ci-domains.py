#!/usr/bin/env python3
"""Choose the CI work a ready pull request needs from the paths it changes.

The pull request lane runs only what a change can break: the Rust tests of the
touched crates and every crate that depends on them, the Teletext and Next jobs
when their domain is touched, and the Playwright specs of the touched admin
areas plus a small hydration smoke set. Anything shared, unknown or
build-defining selects everything. The full suite still runs on `main`
(nightly) and on manual dispatch, so a missed mapping is caught there.

Usage: ci-domains.py select <changed-files.txt>   (one repo-relative path per line)
Prints one JSON object; see `select()` for its keys.
"""

import argparse
import fnmatch
import importlib.util
import json
import posixpath
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
_suite_spec = importlib.util.spec_from_file_location(
    "ci_teletext_selection", ROOT / "scripts/ci-teletext-selection.py")
_suites = importlib.util.module_from_spec(_suite_spec)
_suite_spec.loader.exec_module(_suites)

# Paths that define the build, the toolchain, CI itself or shared test
# infrastructure: any change here selects everything.
EVERYTHING = (
    "Cargo.toml", "Cargo.lock", "rust-toolchain*", ".cargo/*", ".config/*", "deny.toml",
    "package.json", "package-lock.json", "playwright.config.ts", "tsconfig*.json",
    ".github/workflows/ci.yml", ".github/workflows/release.yml", ".github/actions/*", "scripts/ci-*", "scripts/e2e/*", "scripts/release/*", "scripts/calibration/*",
    "run.sh", "run.ps1", "run.cmd", "assets/*", "migrations/*",
    # Foundation crates every other crate builds on.
    "crates/ersatzrs-domain/*", "crates/ersatzrs-core/*",
    # Schema changes affect every repository and page.
    "crates/*/migrations/*",
)

# Run with every selected browser domain: prove that the SSR/WASM bundle still
# hydrates and that every advertised admin route answers.
SMOKE_SPECS = ("admin-route-contract", "hydration-startup", "island-hydration")

# Admin page areas (crates/ersatzrs-web/src/admin/pages/<area>) and other crates
# mapped to the Playwright specs that exercise them (fnmatch on the spec name).
AREA_SPECS = {
    "addons": ("addon-*", "addons"),
    "channels": ("channel-*", "channels-*", "block-preview"),
    "ffmpeg_profiles": ("ffmpeg-*", "mpegts-script-settings", "profile-deletion"),
    "graphics_elements": ("graphics-*",),
    "libraries": ("libraries-*", "local-library-*", "television-discovery", "artwork-cache-refresh",
                  "windows-release-regressions"),
    "media": ("media-*", "collection*", "content-variants", "search-see-all", "tag-ownership",
              "dependent-group-selections", "remote-streams"),
    "media_sources": ("plex-reauthentication", "local-library-*", "libraries-*", "remote-streams"),
    "scheduling": ("scheduling-*", "schedule-*", "playouts-*", "block-preview"),
    "search": ("search-see-all",),
    "settings": ("settings-*", "hdhr-jwt", "mpegts-script-settings"),
    "system": ("health-*", "logs-*", "support-pages", "troubleshooting-playback", "next-version",
               "sponsorblock-host-log", "toast-parity"),
    "watermarks": ("watermark-*",),
    "dev": SMOKE_SPECS,
}
CRATE_SPECS = {
    "ersatzrs-scanner": AREA_SPECS["libraries"] + AREA_SPECS["media"],
    "ersatzrs-scheduler": AREA_SPECS["scheduling"],
    "ersatzrs-ffmpeg": ("ffmpeg-*", "channel-*", "troubleshooting-playback", "watermark-*",
                        "mpegts-script-settings", "hdhr-jwt", "health-*"),
    "ersatzrs-hwprobe": ("ffmpeg-*", "health-*"),
    "ersatzrs-healthcheck": ("health-*",),
    "ersatzrs-launcher": ("windows-release-regressions",),
    "xtask": (),
}
TELETEXT_PATH = re.compile(r"teletext|drcs|zvbi|tti_", re.I)
WEB_SRC = "crates/ersatzrs-web/src/"

# Whole specs may own several families: retain their serial groups and fixtures.
# Required/experimental ownership still comes from ci-teletext-selection.py.
TELETEXT_SPECS = {
    "authoring": ("teletext-authoring-flow", "teletext-effective-authoring-flow",
                  "teletext-guide-authoring-flow", "teletext-channel-profiles",
                  "teletext-polish-flow", "teletext-definition-removal"),
    "editor": ("teletext-editor-flow", "teletext-authoring-flow", "teletext-channel-profiles",
               "teletext-polish-flow", "teletext-definition-removal"),
    "runtime": ("teletext-restart-flow", "channels-source-teletext-delivery",
                "teletext-scripted-health", "teletext-output-policy-health", "teletext-decoder-health",
                "teletext-guide-authoring-flow", "teletext-authoring-flow",
                "teletext-effective-authoring-flow", "teletext-channel-profiles"),
}
PLAYBACK_SPECS = ("channel-watch", "channel-streaming-engine", "troubleshooting-playback",
                  "hdhr-jwt", "mpegts-script-settings", "ffmpeg-*", "watermark-*", "health-*",
                  "addon-prepared-playback")
PUBLIC_SCRIPT_SPECS = {
    "channel-preview.js": ("channel-watch", "troubleshooting-playback"),
    "troubleshooting-preview.js": ("troubleshooting-playback",),
    "workspace-auto-apply.js": ("addon-*",),
    "workspace-draft-status.js": ("addon-*",),
}
# Only these tiny wrappers have been reviewed as pure presentation. The source
# grammar below invalidates this classification if code/import structure grows.
PRESENTATION_WRAPPERS = {"media/images.rs", "media/songs.rs", "media/see_all_lists.rs"}
# These three helpers emit native -af text. Other filter modules retain Next
# coverage until their exported types and consumers have also been audited.
NATIVE_FILTER_INPUTS = {"filter/audio.rs"}
SERVER_AREAS = {
    "libraries": ("libraries",), "media_sources": ("media_sources", "libraries"),
    "media_browser": ("media",), "search": ("search",),
    "watermarks": ("watermarks",), "ffmpeg_profiles": ("ffmpeg_profiles",),
    "graphics_elements": ("graphics_elements",),
    "schedules": ("scheduling",), "playouts": ("scheduling",),
    "channels": ("channels",), "addons": ("addons",),
    "addon_media_lists": ("addons",), "addon_list_options": ("addons",),
}
REPOSITORY_AREAS = {
    "library": ("libraries",), "media_source": ("media_sources", "libraries"),
    "collection": ("media",), "tag": ("media",),
    "watermark": ("watermarks",), "search": ("search",),
}


def documentation_path(path):
    # Runtime images, embedded Markdown and fixtures must not be extension skips.
    return path != "CHANGELOG.md" and (path.startswith(("docs/", "plans/", ".claude/", ".codex/"))
            or "/" not in path and path.endswith(".md"))


def presentation_wrapper(path, root=ROOT):
    relative = path.removeprefix(WEB_SRC + "admin/pages/")
    if relative not in PRESENTATION_WRAPPERS:
        return False
    try:
        source = (root / path).read_text()
    except OSError:
        return False
    source = re.sub(r"(?m)^\s*//[^\n]*", "", source)
    # No native helpers, tests, conditional compilation, macros other than
    # component/view, or additional imports can enter the narrow lane.
    header = (r"\s*use leptos::prelude::\*;\s*"
              r"use crate::admin::pages::media::multi_select_browser::"
              r"\{BrowserKindConfig, MultiSelectBrowserPage\};\s*")
    component = (r"#\[component\]\s*pub fn [A-Za-z]+\(\) -> impl IntoView\s*\{\s*"
                 r"view!\s*\{\s*<MultiSelectBrowserPage config=BrowserKindConfig::"
                 r"[A-Z_]+\s*/>\s*\}\s*\}\s*")
    return re.fullmatch(header + "(?:" + component + ")+", source) is not None


def workspace_members(root=ROOT):
    text = (root / "Cargo.toml").read_text()
    block = re.search(r"members\s*=\s*\[(.*?)\]", text, re.S)[1]
    return {Path(m).name for m in re.findall(r'"([^"]+)"', block)}


def importers(path, root=ROOT):
    """Trace static relative imports/re-exports transitively, or fail broad.

    Non-TS fixtures and unresolved/dynamic imports have unknown reach. Never
    infer a safe empty selection from an importer search with no result.
    """
    if not path.endswith(".ts") or not (root / path).is_file():
        return None
    reverse = {}
    for candidate in (root / "tests/e2e").rglob("*.ts"):
        source = candidate.read_text()
        candidate_name = candidate.relative_to(root).as_posix()
        imports = re.findall(r"(?:\bfrom\s*|\bimport\s*)['\"]([^'\"]+)['\"]", source)
        for expression in re.findall(r"\b(?:import|require)\s*\(([^)]*)\)", source):
            literal = re.fullmatch(r"\s*['\"]([^'\"]+)['\"]\s*", expression)
            if literal:
                imports.append(literal[1])
            else:
                # Its dynamic import might reach the changed helper. Include
                # this module's transitive consumers, even without a static edge.
                reverse.setdefault(path, set()).add(candidate_name)
        for module in imports:
            if not module.startswith("."):
                continue
            stem = posixpath.normpath(posixpath.join(posixpath.dirname(candidate_name), module))
            possibilities = [stem, stem + ".ts", stem + "/index.ts"]
            if stem.endswith(".js"):
                possibilities.insert(0, stem[:-3] + ".ts")
            target = next((p for p in possibilities if (root / p).is_file()), None)
            if target is None:
                return None
            reverse.setdefault(target, set()).add(candidate_name)
    seen, pending, found = set(), [path], set()
    while pending:
        current = pending.pop()
        if current in seen:
            continue
        seen.add(current)
        for consumer in reverse.get(current, ()):
            if consumer.endswith(".spec.ts"):
                if posixpath.dirname(consumer) != "tests/e2e":
                    return None
                found.add(Path(consumer).name.removesuffix(".spec.ts"))
            else:
                pending.append(consumer)
    return found or None


def spec_names(root=ROOT):
    return sorted(p.name.removesuffix(".spec.ts") for p in (root / "tests/e2e").glob("*.spec.ts"))


def matches(path, patterns):
    return any(fnmatch.fnmatch(path, pattern) for pattern in patterns)


def expand(patterns, specs):
    return {s for s in specs if matches(s, patterns)}


def select(paths, members=None, specs=None, importers=importers, root=ROOT, added_paths=()):
    """Return the work for a change.

    All selected domains carry a reason. Full/unknown selections retain the
    complete required base suite; enhanced checks remain explicit dispatches.
    """
    members = workspace_members() if members is None else members
    specs = spec_names() if specs is None else specs
    general = [s for s in specs if re.search(_suites.browser_filter("general"), s + ".spec.ts")]
    base = [s for s in specs if re.search(_suites.browser_filter("1.5"), s + ".spec.ts")]
    out = dict(everything=False, rust_packages=set(), teletext_rust=False, teletext_browser=False,
               next=False, general_specs=set(), teletext_specs=set(), domains=set(), reasons=[])
    if not paths:
        raise ValueError("changed-file selection must not be empty")
    if not set(added_paths) <= set(paths):
        raise ValueError("added paths must be included in the complete changed-file set")

    def everything(reason):
        out["everything"] = True
        out["reasons"].append(reason)

    def browsers(patterns):
        out["general_specs"].update(expand(patterns, general))

    def all_browsers():
        out["general_specs"].update(general)
        out["teletext_specs"].update(base)

    def teletext(families, native=False):
        known = set().union(*(set(names) for names in TELETEXT_SPECS.values()))
        if set(base) != known:
            everything("base teletext spec inventory changed; family ownership needs review")
        for family in families:
            out["domains"].add("teletext-" + family)
            out["teletext_specs"].update(set(TELETEXT_SPECS[family]) & set(base))
        out["teletext_rust"] |= native

    def area(names):
        for name in names:
            out["domains"].add(name)
            browsers(AREA_SPECS[name])
            if name == "media_sources":
                # This file also owns provider-source storage Health scenarios;
                # its name alone does not describe all of its dependencies.
                out["teletext_specs"].add("teletext-decoder-health")

    def playback(native=True, next_engine=True):
        browsers(PLAYBACK_SPECS)
        teletext(("runtime",), native)
        out["next"] |= next_engine
        out["domains"].add("playback")

    for path in sorted(set(paths)):
        if not path or path.startswith("/") or ".." in path.split("/") or "\\" in path:
            raise ValueError(f"invalid repository-relative changed path: {path!r}")
        if documentation_path(path):
            out["reasons"].append(f"{path}: documentation")
            continue
        if path == "CHANGELOG.md":
            browsers(("support-pages",))
            out["domains"].add("release-notes")
            out["reasons"].append(f"{path}: embedded release notes and /changelog.md response")
            continue
        if matches(path, EVERYTHING):
            everything(f"{path} is shared build/CI input")
            continue
        if path.startswith(".github/"):
            out["reasons"].append(f"{path}: independent repository automation")
            continue
        if path in added_paths and not path.startswith("tests/e2e/"):
            everything(f"{path}: new production input requires ownership review")
            continue
        if not (root / path).is_file():
            everything(f"{path}: deleted or unavailable source; reach is unknown")
            continue
        before = set(out["domains"])
        if path.startswith("tests/e2e/"):
            name = Path(path).name
            if name.endswith(".spec.ts"):
                spec = name.removesuffix(".spec.ts")
                if spec not in specs or Path(path).parent.as_posix() != "tests/e2e":
                    everything(f"{path}: unrecognized browser spec")
                    continue
                users = {spec}
            else:
                users = importers(path)
            if not users or not set(users) <= set(specs):
                all_browsers()
                out["reasons"].append(f"{path}: unknown fixture/import reach; all required browsers")
            else:
                out["general_specs"].update(set(users) & set(general))
                out["teletext_specs"].update(set(users) & set(base))
                # Enhanced-only edits still prove required base fallback; the
                # optional enhanced dispatch contract is not silently changed.
                if set(users) & set(_suites.ENHANCED_SPECS):
                    teletext(("authoring", "editor"))
                out["reasons"].append(f"{path}: browser spec/import consumers {', '.join(sorted(users))}")
            out["domains"].add("browser-tests")
            continue
        if path.startswith("crates/ersatzrs-web/public/"):
            name = path.removeprefix("crates/ersatzrs-web/public/")
            if name in PUBLIC_SCRIPT_SPECS:
                browsers(PUBLIC_SCRIPT_SPECS[name])
            else:
                all_browsers()
            out["domains"].add("browser-assets")
            out["reasons"].append(f"{path}: served frontend asset")
            continue
        parts = path.split("/")
        crate = "xtask" if parts[0] == "xtask" else parts[1] if parts[0] == "crates" and len(parts) > 2 else None
        if crate not in members:
            everything(f"{path}: unmapped production/build input")
            continue
        if path.endswith("Cargo.toml") or path.endswith("build.rs"):
            everything(f"{path}: crate build contract")
            continue
        css = path.startswith(WEB_SRC + "admin/") and path.endswith(".module.css")
        presentation = css or presentation_wrapper(path, root)
        if not presentation:
            out["rust_packages"].add(crate)

        local = path.removeprefix(f"crates/{crate}/src/")
        if crate == "ersatzrs-web":
            if local.startswith("admin/pages/"):
                page = local.removeprefix("admin/pages/")
                if page.startswith("channels/teletext"):
                    if page.startswith("channels/teletext_drafts"):
                        teletext(("authoring", "editor"), not presentation)
                    elif page.startswith("channels/teletext_current_guide"):
                        teletext(("authoring", "runtime"), not presentation)
                    else:
                        teletext(TELETEXT_SPECS, not presentation)
                else:
                    name = page.split("/")[0].split(".")[0]
                    if name in AREA_SPECS:
                        area((name,))
                        if name in ("channels", "scheduling", "ffmpeg_profiles", "settings", "system"):
                            # Shared channel/settings changes affect teletext
                            # delivery even when no teletext filename is edited.
                            teletext(("runtime",), not presentation)
                    else:
                        all_browsers()
                        out["domains"].add("shared-ui")
            elif local.startswith("admin/server_fns/"):
                module = local.removeprefix("admin/server_fns/").split("/")[0].split(".")[0]
                if module.startswith("teletext"):
                    teletext(TELETEXT_SPECS, True)
                elif module in SERVER_AREAS:
                    area(SERVER_AREAS[module])
                    if module in ("channels", "schedules", "playouts", "ffmpeg_profiles"):
                        playback()
                else:
                    everything(f"{path}: shared/unmapped server function")
            elif local.startswith("admin/"):
                all_browsers()
                out["domains"].add("shared-ui")
                # Full base Rust contracts stay available for shared Rust UI.
                out["teletext_rust"] |= not presentation
            elif local.startswith("transport/"):
                playback()
                if not TELETEXT_PATH.search(local):
                    out["domains"].add("shared-transport")
            elif local.startswith("api/") and TELETEXT_PATH.search(local):
                teletext(TELETEXT_SPECS, True)
            else:
                everything(f"{path}: shared/unmapped web runtime")
        elif crate == "ersatzrs-ffmpeg":
            playback(next_engine=local not in NATIVE_FILTER_INPUTS)
            if TELETEXT_PATH.search(local) and not re.search(r"/(?:output|buffer|pmt|packet)", local):
                teletext(TELETEXT_SPECS, True)
        elif crate in ("ersatzrs-infra", "ersatzrs-app"):
            if TELETEXT_PATH.search(local):
                teletext(TELETEXT_SPECS, True)
                if local.startswith(("playback/", "next_engine/")):
                    playback()
            elif local.startswith("next_engine/"):
                playback()
            elif local.startswith("repositories/"):
                module = local.removeprefix("repositories/").split("/")[0].split(".")[0]
                if module in REPOSITORY_AREAS:
                    area(REPOSITORY_AREAS[module])
                else:
                    everything(f"{path}: unmapped repository/shared state")
            else:
                everything(f"{path}: unmapped service/shared state")
        elif crate in CRATE_SPECS:
            if crate == "xtask" and Path(path).name in ("main.rs", "smoke.rs", "smoke_next.rs"):
                everything(f"{path}: shared CI command routing/runtime smoke input")
                continue
            browsers(CRATE_SPECS[crate])
            out["domains"].add(crate)
            if crate == "ersatzrs-scheduler":
                playback()
            elif crate == "ersatzrs-scanner":
                # Source-track metadata reaches ordinary and authored output.
                playback()
        else:
            everything(f"{path}: shared crate/unknown consumer reach")
        owners = sorted(out["domains"] - before)
        out["reasons"].append(f"{path}: {'presentation' if presentation else 'Rust and consumers'}; "
                              + (", ".join(owners) if owners else "existing domain union"))

    if out["everything"]:
        out.update(rust_packages=set(members), teletext_rust=True,
                   next=True, general_specs=set(general), teletext_specs=set(base))
        out["domains"].add("full-required")
    if out["general_specs"] or out["teletext_specs"]:
        out["general_specs"].update(set(SMOKE_SPECS) & set(general))
    out["teletext_browser"] = bool(out["teletext_specs"])
    out["general_specs"] = "all" if set(general) == out["general_specs"] else sorted(out["general_specs"])
    out["teletext_specs"] = sorted(out["teletext_specs"])
    out["domains"] = sorted(out["domains"])
    out["rust_packages"] = sorted(out["rust_packages"])
    return out


def archive_packages(selection):
    """Workspace packages whose test binaries the nextest archive must hold:
    the touched crates (the build adds their dependents) plus the crates the
    Teletext and Next jobs run tests from. An empty list means workspace scope
    only when build_products says an archive is required."""
    if selection["everything"]:
        return []
    packages = set(selection["rust_packages"])
    if selection["teletext_rust"] or selection["next"]:
        packages |= {"ersatzrs-infra", "ersatzrs-web"}
    return sorted(packages)


def build_products(selection):
    """Demand follows consumers, including helpers inside each required product."""
    return {
        "rust": bool(selection["rust_packages"] or selection["next"] or selection["teletext_rust"]),
        "web": bool(selection["general_specs"] or selection["teletext_specs"]),
        "smoke_server": selection["next"],
    }


def reverse_closure(packages, metadata):
    """`packages` plus every workspace member depending on them (any kind)."""
    members = {p["name"]: p for p in metadata["packages"] if p["id"] in metadata["workspace_members"]}
    selected = set(packages)
    changed = True
    while changed:
        changed = False
        for name, package in members.items():
            if name not in selected and any(d["name"] in selected for d in package["dependencies"]):
                selected.add(name)
                changed = True
    return sorted(selected)


def archive_target_flags(packages, metadata):
    """Keep lib/bin/integration coverage without requesting nonexistent targets."""
    members = {p["name"]: p for p in metadata["packages"] if p["id"] in metadata["workspace_members"]}
    if set(packages) - members.keys():
        raise ValueError("unknown workspace package in archive scope")
    selected = packages or members.keys()
    kinds = {kind for name in selected for target in members[name]["targets"] for kind in target["kind"]}
    libraries = {"lib", "rlib", "cdylib", "dylib", "staticlib", "proc-macro"}
    return (["--lib"] if kinds & libraries else []) + (["--bins"] if "bin" in kinds else []) + ["--tests"]


def nextest_filter(packages, members):
    """Filterset narrowing the general Rust selection to the touched crates."""
    if not packages or set(packages) >= set(members):
        return "all()" if packages else "none()"
    return " | ".join(f"rdeps({p})" for p in packages)


def spec_filter(specs):
    """Playwright file filter for the selected general specs."""
    # Spec names contain only [a-z0-9-], so they need no regex escaping.
    if not specs or not all(re.fullmatch(r"[a-z0-9-]+", s) for s in specs):
        raise ValueError("browser filter requires a nonempty list of valid spec names")
    return r"(^|/)(" + "|".join(specs) + r")\.spec\.ts$"


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("command", choices=("select", "closure", "archive-targets"))
    parser.add_argument("files", type=Path, help="changed paths (select) or cargo metadata JSON (closure)")
    parser.add_argument("packages", nargs="*", help="closure: the packages to close over")
    parser.add_argument("--added-files", type=Path, help="new paths from the complete PR diff")
    args = parser.parse_args()
    if args.command in ("closure", "archive-targets"):
        query = reverse_closure if args.command == "closure" else archive_target_flags
        print(" ".join(query(args.packages, json.loads(args.files.read_text()))))
        return
    paths = [line.strip() for line in args.files.read_text().splitlines() if line.strip()]
    members = workspace_members()
    added = args.added_files.read_text().splitlines() if args.added_files else []
    result = select(paths, members, added_paths=added)
    result["nextest_filter"] = nextest_filter(result["rust_packages"], members)
    result["archive_packages"] = archive_packages(result)
    result["products"] = build_products(result)
    general = result["general_specs"]
    result["general_filter"] = "" if general == "all" else (spec_filter(general) if general else "")
    result["teletext_filter"] = spec_filter(result["teletext_specs"]) if result["teletext_specs"] else ""
    json.dump(result, sys.stdout, sort_keys=True)
    print()


if __name__ == "__main__":
    main()
