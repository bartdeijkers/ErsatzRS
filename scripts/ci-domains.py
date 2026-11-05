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
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

# Paths that define the build, the toolchain, CI itself or shared test
# infrastructure: any change here selects everything.
EVERYTHING = (
    "Cargo.toml", "Cargo.lock", "rust-toolchain*", ".cargo/*", ".config/*", "deny.toml",
    "package.json", "package-lock.json", "playwright.config.ts", "tsconfig*.json",
    ".github/workflows/ci.yml", ".github/actions/*", "scripts/ci-*", "scripts/e2e/*", "scripts/release/*", "scripts/calibration/*",
    "run.sh", "run.ps1", "run.cmd", "assets/*", "migrations/*",
    # Foundation crates every other crate builds on.
    "crates/ersatzrs-domain/*", "crates/ersatzrs-core/*",
    # Schema changes affect every repository and page.
    "crates/*/migrations/*",
)

# Always run for a code change: these prove that the SSR/WASM bundle still
# hydrates and that every advertised admin route answers.
SMOKE_SPECS = ("admin-route-contract", "hydration-startup", "island-hydration")

# Admin page areas (crates/ersatzrs-web/src/admin/pages/<area>) and other crates
# mapped to the Playwright specs that exercise them (fnmatch on the spec name).
AREA_SPECS = {
    "addons": ("addon-*", "addons"),
    "channels": ("channel-*", "channels-*", "block-preview"),
    "ffmpeg_profiles": ("ffmpeg-*", "mpegts-script-settings"),
    "graphics_elements": ("graphics-*",),
    "libraries": ("libraries-*", "local-library-*", "television-discovery", "artwork-cache-refresh"),
    "media": ("media-*", "collection*", "content-variants", "search-see-all", "tag-ownership",
              "dependent-group-selections"),
    "media_sources": ("plex-reauthentication", "local-library-*", "libraries-*", "remote-streams"),
    "scheduling": ("scheduling-*", "schedule-*", "playouts-*", "block-preview"),
    "search": ("search-see-all",),
    "settings": ("settings-*", "hdhr-jwt", "mpegts-script-settings"),
    "system": ("health-*", "logs-*", "support-pages", "troubleshooting-playback", "next-version",
               "sponsorblock-host-log", "toast-parity"),
    "watermarks": ("watermark-*",),
    "dev": (),
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
# Files of the shared service crates are mapped by keywords in their path to
# the admin area they serve; a file that matches none selects every spec.
AREA_KEYWORDS = (
    ("media_source", "media_sources"), ("plex", "media_sources"), ("jellyfin", "media_sources"),
    ("emby", "media_sources"), ("remote_stream", "media_sources"),
    ("librar", "libraries"), ("scan", "libraries"), ("artwork", "libraries"),
    ("schedul", "scheduling"), ("playout", "scheduling"), ("block", "scheduling"),
    ("channel", "channels"), ("addon", "addons"), ("ffmpeg", "ffmpeg_profiles"),
    ("watermark", "watermarks"), ("graphics", "graphics_elements"),
    ("health", "system"), ("log", "system"), ("troubleshoot", "system"),
    ("search", "search"), ("collection", "media"), ("media_item", "media"), ("tag", "media"),
    ("setting", "settings"), ("config", "settings"),
)
KEYWORD_CRATES = {"ersatzrs-infra", "ersatzrs-app"}

# Crates whose change can reach any page: every general spec.
CRATE_ALL_GENERAL = {"ersatzrs-infra", "ersatzrs-app", "ersatzrs-platform", "ersatzrs-addon-contract"}
# Crates whose tests include Teletext Rust regressions (cheap job; run it for them).
TELETEXT_RUST_CRATES = {"ersatzrs-infra", "ersatzrs-web", "ersatzrs-app", "ersatzrs-ffmpeg",
                        "ersatzrs-platform", "ersatzrs-addon-contract"}

TELETEXT_PATH = re.compile(r"teletext|drcs|zvbi|tti_", re.I)
NEXT_PATH = re.compile(r"(^|[/_.-])next([/_.-]|$)", re.I)
NEXT_CRATES = {"ersatzrs-ffmpeg"}
NON_CODE = re.compile(r"^(docs|plans|\.claude|\.codex)/|\.(md|png)$")


def workspace_members(root=ROOT):
    text = (root / "Cargo.toml").read_text()
    block = re.search(r"members\s*=\s*\[(.*?)\]", text, re.S)[1]
    return {Path(m).name for m in re.findall(r'"([^"]+)"', block)}


def importers(path, root=ROOT):
    """Spec names importing a shared Playwright module, or None if a non-spec
    module imports it too (then its reach is unknown)."""
    module = path.removeprefix("tests/e2e/").rsplit(".", 1)[0]
    pattern = re.compile(r"""from\s+['"]\./""" + re.escape(module) + r"""['"]""")
    found = set()
    for candidate in (root / "tests/e2e").rglob("*.ts"):
        if candidate.as_posix().endswith(path) or not pattern.search(candidate.read_text()):
            continue
        if not candidate.name.endswith(".spec.ts") or candidate.parent != root / "tests/e2e":
            return None
        found.add(candidate.name.removesuffix(".spec.ts"))
    return found


def spec_names(root=ROOT):
    return sorted(p.name.removesuffix(".spec.ts") for p in (root / "tests/e2e").glob("*.spec.ts"))


def matches(path, patterns):
    return any(fnmatch.fnmatch(path, pattern) for pattern in patterns)


def expand(patterns, specs):
    return {s for s in specs if matches(s, patterns)}


def keyword_areas(path):
    name = path.split("/src/", 1)[-1].lower()
    return sorted({area for keyword, area in AREA_KEYWORDS if keyword in name})


def select(paths, members=None, specs=None, importers=importers):
    """Return the work for a change.

    Keys: everything (bool), rust_packages (sorted list), teletext_rust (bool),
    teletext_browser (bool), next (bool), general_specs ("all" or sorted list),
    reasons (list of short strings).
    """
    members = workspace_members() if members is None else members
    specs = spec_names() if specs is None else specs
    general = [s for s in specs if not TELETEXT_PATH.search(s)]
    out = dict(everything=False, rust_packages=set(), teletext_rust=False, teletext_browser=False,
               next=False, general_specs=set(), reasons=[])

    def everything(reason):
        out["everything"] = True
        out["reasons"].append(reason)

    for path in paths:
        if not path or NON_CODE.search(path):
            continue
        if matches(path, EVERYTHING):
            everything(f"{path} is shared build/CI input")
            continue
        if TELETEXT_PATH.search(path):
            # A Teletext spec or helper needs only the Teletext browser suite.
            if not path.startswith("tests/e2e/"):
                out["teletext_rust"] = True
            out["teletext_browser"] = True
        if NEXT_PATH.search(path):
            out["next"] = True
        if path.startswith("tests/e2e/"):
            name = Path(path).name
            if name.endswith(".spec.ts"):
                spec = name.removesuffix(".spec.ts")
                if not TELETEXT_PATH.search(spec) and out["general_specs"] != "all":
                    out["general_specs"].add(spec)
            else:
                users = importers(path)
                # No known importer (data fixture, dynamic import, nested
                # helper): its reach is unknown, so run every spec.
                if not users:
                    out["general_specs"] = "all"
                    out["teletext_browser"] = True
                    out["reasons"].append(f"{path} is shared Playwright infrastructure")
                else:
                    if any(TELETEXT_PATH.search(u) for u in users):
                        out["teletext_browser"] = True
                    if out["general_specs"] != "all":
                        out["general_specs"] |= {u for u in users if not TELETEXT_PATH.search(u)}
            continue
        parts = path.split("/")
        if path.startswith(".github/"):
            # Other workflows, templates and bot config do not change what CI tests.
            continue
        if parts[0] == "crates" and len(parts) > 2 or parts[0] == "xtask":
            crate = "xtask" if parts[0] == "xtask" else parts[1]
            if crate not in members:
                everything(f"{path} is in an unknown crate")
                continue
            out["rust_packages"].add(crate)
            if crate in TELETEXT_RUST_CRATES:
                out["teletext_rust"] = True
            if crate in NEXT_CRATES:
                out["next"] = True
            if out["general_specs"] == "all":
                continue
            if crate == "ersatzrs-web":
                page = re.match(r"crates/ersatzrs-web/src/admin/pages/([a-z_]+)", path)
                area = page[1] if page else None
                if area in AREA_SPECS:
                    out["general_specs"] |= expand(AREA_SPECS[area], general)
                else:
                    out["general_specs"] = "all"
                    out["reasons"].append(f"{path} is shared admin UI code")
            elif crate in KEYWORD_CRATES and (areas := keyword_areas(path)):
                for area in areas:
                    out["general_specs"] |= expand(AREA_SPECS[area], general)
            elif crate in CRATE_SPECS:
                out["general_specs"] |= expand(CRATE_SPECS[crate], general)
            elif crate in CRATE_ALL_GENERAL:
                out["general_specs"] = "all"
                out["reasons"].append(f"{path} can reach every admin page")
            continue
        # Any other path (root files, scripts, tools) is not mapped: be safe.
        everything(f"{path} is not mapped to a domain")

    if out["everything"]:
        out.update(rust_packages=sorted(members), teletext_rust=True, teletext_browser=True,
                   next=True, general_specs="all")
        return out
    code_changed = bool(out["rust_packages"] or out["general_specs"] or out["teletext_browser"])
    if out["general_specs"] != "all":
        if code_changed:
            out["general_specs"] |= set(SMOKE_SPECS) & set(general)
        out["general_specs"] = sorted(out["general_specs"])
    out["rust_packages"] = sorted(out["rust_packages"])
    return out


def archive_packages(selection):
    """Workspace packages whose test binaries the nextest archive must hold:
    the touched crates (the build adds their dependents) plus the crates the
    Teletext and Next jobs run tests from. Empty means the whole workspace."""
    if selection["everything"]:
        return []
    packages = set(selection["rust_packages"])
    if selection["teletext_rust"] or selection["next"]:
        packages |= {"ersatzrs-infra", "ersatzrs-web"}
    # xtask is always built; an archive of it alone keeps the Test job's
    # filter valid when no Rust test is selected.
    return sorted(packages or {"xtask"})


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


def nextest_filter(packages, members):
    """Filterset narrowing the general Rust selection to the touched crates."""
    if not packages or set(packages) >= set(members):
        return "all()" if packages else "none()"
    return " | ".join(f"rdeps({p})" for p in packages)


def spec_filter(specs):
    """Playwright file filter for the selected general specs."""
    # Spec names contain only [a-z0-9-], so they need no regex escaping.
    assert all(re.fullmatch(r"[a-z0-9-]+", s) for s in specs), specs
    return r"(^|/)(" + "|".join(specs) + r")\.spec\.ts$"


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("command", choices=("select", "closure"))
    parser.add_argument("files", type=Path, help="changed paths (select) or cargo metadata JSON (closure)")
    parser.add_argument("packages", nargs="*", help="closure: the packages to close over")
    args = parser.parse_args()
    if args.command == "closure":
        print(" ".join(reverse_closure(args.packages, json.loads(args.files.read_text()))))
        return
    paths = [line.strip() for line in args.files.read_text().splitlines() if line.strip()]
    members = workspace_members()
    result = select(paths, members)
    result["nextest_filter"] = nextest_filter(result["rust_packages"], members)
    result["archive_packages"] = archive_packages(result)
    general = result["general_specs"]
    result["general_filter"] = "" if general == "all" else (spec_filter(general) if general else "")
    json.dump(result, sys.stdout, sort_keys=True)
    print()


if __name__ == "__main__":
    main()
