#!/usr/bin/env python3
"""Choose the CI work a ready pull request needs from the paths it changes.

The pull request lane runs only what a change can break: the Rust tests of the
touched crates and every crate that depends on them, the Teletext and Next jobs
when their domain is touched, and the Playwright specs of the touched admin
pages plus a small hydration smoke set. Page ownership is read from the
sources (scripts/ci-browser-evidence.py): a spec belongs to the page modules
whose routes it names, and a shared module to the pages that use its public
items. A server-function module without a reviewed area selects the pages
that call it, and the application shell the specs that name its elements.
Anything unknown or build-defining selects everything. The full suite
still runs on `main` (weekly) and on manual dispatch, so a missed mapping is
caught there.

Usage: ci-domains.py select <changed-files.txt>   (one repo-relative path per line)
Prints one JSON object; see `select()` for its keys.
"""

import argparse
import fnmatch
import importlib.util
import json
import posixpath
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
_suite_spec = importlib.util.spec_from_file_location(
    "ci_teletext_selection", ROOT / "scripts/ci-teletext-selection.py")
_suites = importlib.util.module_from_spec(_suite_spec)
_suite_spec.loader.exec_module(_suites)
_evidence_spec = importlib.util.spec_from_file_location(
    "ci_browser_evidence", ROOT / "scripts/ci-browser-evidence.py")
_evidence = importlib.util.module_from_spec(_evidence_spec)
_evidence_spec.loader.exec_module(_evidence)

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

# Admin areas mapped to the Playwright specs that exercise them (fnmatch on the
# spec name). Backend changes (server functions, repositories, crates) select a
# whole area. A changed page file instead selects the specs that name its
# routes; this table then only supplies the specs without any route evidence.
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


def select(paths, members=None, specs=None, importers=importers, root=ROOT, added_paths=(),
           base_text=None):
    """Return the work for a change.

    All selected domains carry a reason. Full/unknown selections retain the
    complete required base suite; enhanced checks remain explicit dispatches.
    `base_text(path)` returns a file's content before the change (or None);
    without it, translation and re-export changes keep their broad selection.
    """
    evidence = _evidence.Evidence(root)
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

    def shell():
        """The application shell: the specs naming the elements only it renders.

        The hydration smoke set joins every browser selection, so it proves
        that each page still renders inside the changed shell.
        """
        out["domains"].add("admin-shell")
        found = evidence.shell_specs() & set(general) if evidence.available() else set()
        if not found:
            all_browsers()
            out["reasons"].append("no spec names an application shell element; all required browsers")
            return
        out["general_specs"].update(found)

    def page(module, native):
        """Select the specs that drive one page module."""
        local = module.removeprefix(WEB_SRC + "admin/pages/")
        name = local.split("/")[0].split(".")[0]
        if _evidence.shell(module):
            shell()
            return
        if not module.startswith(WEB_SRC + "admin/pages/"):
            # The home page lives in the application shell.
            all_browsers()
            return
        if local.startswith("channels/teletext"):
            if local.startswith("channels/teletext_drafts"):
                teletext(("authoring", "editor"), native)
            elif local.startswith("channels/teletext_current_guide"):
                teletext(("authoring", "runtime"), native)
            else:
                teletext(TELETEXT_SPECS, native)
            return
        out["domains"].add(name)
        found = evidence.page_specs(module)
        # Specs that name no route at all keep their reviewed area ownership.
        unrouted = {s for s in expand(AREA_SPECS.get(name, ()), specs) if not evidence.spec_pages().get(s)}
        if not found and name not in AREA_SPECS:
            # No spec and no reviewed owner: the reach of this page is unknown.
            all_browsers()
            return
        out["general_specs"].update((found | unrouted) & set(general))
        out["teletext_specs"].update(found & set(base))
        if native and name in ("channels", "scheduling", "ffmpeg_profiles", "settings", "system"):
            # Channel, schedule and settings behavior feeds teletext delivery;
            # its Rust contracts stay selected even when no base spec drives
            # the changed page.
            out["teletext_rust"] = True

    followed = set()

    def reached(found, server, known, native):
        """Apply a consumer trace; unknown reach keeps every browser spec."""
        if not known:
            all_browsers()
            out["domains"].add("shared-ui")
            out["reasons"].append(f"unknown reach ({evidence.unknown}); all required browsers")
            return
        for module in sorted(found):
            page(module, native)
        for module in sorted(server):
            name = module.removeprefix(_evidence.SERVER_FNS).split("/")[0].split(".")[0]
            if name.startswith("teletext"):
                teletext(TELETEXT_SPECS, native)
            elif name in SERVER_AREAS:
                area(SERVER_AREAS[name])
            elif module not in followed:
                # No reviewed area: the pages calling this module own it.
                followed.add(module)
                out["domains"].add("server-functions")
                reached(*evidence.reach((module,)), native=native)

    def pages(changed, presentation):
        """Changed page sources: themselves plus every page using their items."""
        if not evidence.available():
            all_browsers()
            out["domains"].add("shared-ui")
            return
        for path in changed:
            if path.startswith(_evidence.TELETEXT_PAGES):
                page(path, not presentation)
            else:
                reached(*evidence.reach((path,)), native=not presentation)

    def shared(path):
        """A shared interface module selects the pages that use it."""
        if not evidence.available():
            all_browsers()
            out["domains"].add("shared-ui")
            return
        before = base_text(path) if base_text else None
        if path == _evidence.ROUTES or evidence.declaration_only(path):
            # Only the mounted or re-exported items on the changed lines matter.
            current = (root / path).read_text()
            if before is None:
                all_browsers()
                out["domains"].add("shared-ui")
                return
            if path == _evidence.ROUTES:
                components = {component: module for _, component, module in evidence.routes()}
                names = _evidence.changed_names(before, current, set(components))
                if not names and before != current:
                    all_browsers()
                for name in sorted(names):
                    page(components[name], False)
            else:
                reached(*evidence.reach(names=_evidence.changed_names(before, current)), native=False)
            out["domains"].add("shared-ui")
            return
        reached(*evidence.reach((path,)), native=False)
        out["domains"].add("shared-ui")

    def translations(path):
        """Changed text keys select the pages that show them."""
        before = base_text(path) if base_text else None
        out["domains"].add("translations")
        browsers(("settings-i18n",))
        if before is None or not evidence.available():
            all_browsers()
            return
        current = (root / path).read_text()
        keys, functions, known = _evidence.text_changes(before, current)
        if not known:
            all_browsers()
            return
        names = set(functions)
        if keys:
            names |= _evidence.key_helpers(evidence.sources()[_evidence.I18N], keys)
        users = set()
        if keys:
            wanted = re.compile(r"\bTextKey::(?:" + "|".join(sorted(keys)) + r")\b")
            users = {name for name, text in evidence.sources().items()
                     if name != _evidence.I18N and not name.startswith(_evidence.LANGUAGES)
                     and wanted.search(text)}
        reached(*evidence.reach(users, names), native=False)

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
        # A new admin interface module is reached only through a declaration,
        # route or caller that is itself part of this change, so it is
        # classified by its location like a modified file.
        owned_addition = path.startswith(WEB_SRC + "admin/") and not path.startswith(
            (WEB_SRC + "admin/server_fns/",) + _evidence.SHELL_DIRS) and path not in _evidence.SHELL
        if path in added_paths and not path.startswith("tests/e2e/") and not owned_addition:
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
                pages((path,), presentation)
            elif local.startswith("admin/server_fns/"):
                module = local.removeprefix("admin/server_fns/").split("/")[0].split(".")[0]
                if module.startswith("teletext"):
                    teletext(TELETEXT_SPECS, True)
                elif module in SERVER_AREAS:
                    area(SERVER_AREAS[module])
                    if module in ("channels", "schedules", "playouts", "ffmpeg_profiles"):
                        playback()
                elif module == "mod" or not evidence.available():
                    everything(f"{path}: shared/unmapped server function")
                else:
                    found, server, known = evidence.reach((path,))
                    if not known:
                        everything(f"{path}: server function without a visible caller")
                    else:
                        followed.add(path)
                        out["domains"].add("server-functions")
                        reached(found, server, True, native=True)
            elif path == _evidence.I18N or path.startswith(_evidence.LANGUAGES):
                translations(path)
                out["teletext_rust"] = True
            elif _evidence.shell(path):
                shell()
                out["teletext_rust"] |= not presentation
            elif local.startswith("admin/"):
                shared(path)
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


def revision_text(revision):
    """Read a file as it was at `revision`; None when it did not exist."""
    def read(path):
        done = subprocess.run(["git", "-C", str(ROOT), "show", f"{revision}:{path}"],
                              capture_output=True, text=True, check=False)
        return done.stdout if done.returncode == 0 else None
    if subprocess.run(["git", "-C", str(ROOT), "cat-file", "-e", f"{revision}^{{commit}}"],
                      capture_output=True, check=False).returncode != 0:
        raise ValueError(f"base revision {revision!r} is not available")
    return read


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("command", choices=("select", "closure", "archive-targets"))
    parser.add_argument("files", type=Path, help="changed paths (select) or cargo metadata JSON (closure)")
    parser.add_argument("packages", nargs="*", help="closure: the packages to close over")
    parser.add_argument("--added-files", type=Path, help="new paths from the complete PR diff")
    parser.add_argument("--base-rev", help="revision holding the files as they were before the change")
    args = parser.parse_args()
    if args.command in ("closure", "archive-targets"):
        query = reverse_closure if args.command == "closure" else archive_target_flags
        print(" ".join(query(args.packages, json.loads(args.files.read_text()))))
        return
    paths = [line.strip() for line in args.files.read_text().splitlines() if line.strip()]
    members = workspace_members()
    added = args.added_files.read_text().splitlines() if args.added_files else []
    result = select(paths, members, added_paths=added,
                    base_text=revision_text(args.base_rev) if args.base_rev else None)
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
