#!/usr/bin/env python3
"""Disjoint CI ownership; these buckets do not establish level acceptance.

Shared/base tests default to 1.5. Enhanced feature families (including their
base fallback assertions) belong to 2.5. Binary matching retains integration
tests whose individual names do not contain 'teletext'.
"""

import argparse
import re


TELETEXT = r"teletext|drcs|zvbi|tti_"
ENHANCED = (
    r"drcs|object|resource|palette|panel|mot_|m29|x28|colour_composition|"
    r"composed_main|main_composition|origin_carrier|normalized_editorial|"
    r"augmented_origin|display_resolution|attribute_execution|attribute_observation|"
    r"enhancement_program|enhancement_objects|editorial_tti|editorial_effects|"
    r"compatibility_lowering|physical_spacing|level_?2_?5"
)
BASE_REQUIRED = r"(^|::)standalone_base::"
ENHANCED_SPECS = (
    "teletext-bank-dependencies",
    "teletext-definition-add",
    "teletext-definition-copy",
    "teletext-object-selection",
    "teletext-resource-bank-flow",
    "teletext-resource-diagnostics",
    "teletext-retained-bank",
    "teletext-secondary-default",
)
RECOVERY_SPECS = (
    "block-preview.spec.ts", "teletext-authoring-flow.spec.ts", "teletext-editor-flow.spec.ts",
)
PREVIEW_RECOVERY_CASES = (
    "shows the real transcode error when decoded preview video is an error slate",
    "plays a real H.264/AAC preview derivative through Hls.js",
)
TELETEXT_RUNTIME_RECOVERY_SPECS = (
    "channels-source-teletext-delivery.spec.ts", "teletext-restart-flow.spec.ts",
)
TELETEXT_RUNTIME_RECOVERY_CASES = (
    "localized source-caption choice persists through native forms and rejects incompatible output atomically",
    "standalone Level 1.5 active Next viewers release source processes and leases on shutdown",
    "built-in MPEG-TS choice persists through localized settings and host restart (en-us/JS)",
    "built-in MPEG-TS choice persists through localized settings and host restart (nl/no-JS)",
    "shared profile generations match browser and ordinary streams (en-us/JS)",
    "shared profile generations match browser and ordinary streams (nl/no-JS)",
    "standalone Level 1.5 ordinary URL is held for an external client qualification",
    "per-output teletext delivery status (en-us/ts-legacy/v1)",
    "per-output teletext delivery status (en-us/ts-legacy/v2)",
    "per-output teletext delivery status (nl/ts/v1)",
    "per-output teletext delivery status (nl/ts/v2)",
)


def rust_filter(suite, next_runtime=False):
    teletext = f"(test(/{TELETEXT}/) | binary(/{TELETEXT}/))"
    # Generic authored-service and integration-binary names also contain base
    # and shared guards. Classify features, retaining the explicit standalone
    # family even when its test name mentions forbidden optional resources.
    enhanced = f"(test(/{ENHANCED}/) & not test(/{BASE_REQUIRED}/))"
    next_engine = "(test(next_engine::) | test(transport::iptv::next_media::))"
    if suite == "general":
        return f"not test(next_engine::) & not {teletext}"
    if suite == "next":
        return f"{next_engine} & not test(fixture_child) & not {teletext}"
    if suite == "all":
        return teletext
    selected = f"{teletext} & {'not ' if suite == '1.5' else ''}{enhanced}"
    if next_runtime:
        return f"{selected} & {next_engine} & not test(fixture_child)"
    return f"{selected} & not {next_engine}"


def next_recovery_filter(case_set):
    if case_set == "all":
        return rust_filter("next")
    if case_set == "subtitle-nul":
        return ("test(=next_engine::process::runtime_tests::subtitle_nul_runtime_tests::"
                "real_native_next_nul_extraction_and_existing_cache_preserve_selected_captions)")
    raise ValueError("unknown Next recovery case set")


def browser_filter(suite):
    teletext = r"[^/]*teletext[^/]*\.spec\.ts$"
    enhanced = "(" + "|".join(ENHANCED_SPECS) + r")\.spec\.ts$"
    if suite == "general":
        return r"(^|/)(?![^/]*teletext)[^/]+\.spec\.ts$"
    if suite == "2.5":
        return enhanced
    return rf"(^|/)(?!(?:{'|'.join(ENHANCED_SPECS)})\.spec\.ts$){teletext}"


def browser_recovery_args(case_set):
    if case_set == "fixtures":
        return list(RECOVERY_SPECS)
    if case_set == "channel-preview":
        return ["channel-watch.spec.ts", "--grep",
                "(?:" + "|".join(re.escape(case) for case in PREVIEW_RECOVERY_CASES) + ")$"]
    if case_set == "teletext-1.5":
        return [browser_filter("1.5")]
    if case_set == "teletext-runtime":
        return [*TELETEXT_RUNTIME_RECOVERY_SPECS, "--grep",
                "(?:" + "|".join(re.escape(case) for case in TELETEXT_RUNTIME_RECOVERY_CASES) + ")$"]
    raise ValueError("unknown browser recovery case set")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("kind", choices=("rust", "browser", "browser-recovery", "next-recovery"))
    parser.add_argument("suite", choices=("general", "1.5", "2.5", "all", "next",
                                          "fixtures", "channel-preview", "teletext-1.5", "teletext-runtime", "subtitle-nul"))
    parser.add_argument("--next-runtime", action="store_true")
    args = parser.parse_args()
    if args.kind == "next-recovery":
        if args.suite not in ("all", "subtitle-nul") or args.next_runtime:
            parser.error("Next recovery requires all or subtitle-nul without --next-runtime")
        print(next_recovery_filter(args.suite))
        return
    if args.suite == "subtitle-nul":
        parser.error("subtitle-nul requires Next recovery")
    if args.kind == "browser" and args.suite not in ("general", "1.5", "2.5"):
        parser.error("browser selection requires general, 1.5 or 2.5")
    if args.kind == "browser-recovery":
        if args.suite not in ("fixtures", "channel-preview", "teletext-1.5", "teletext-runtime") or args.next_runtime:
            parser.error("browser recovery requires a bounded case set without --next-runtime")
        print("\n".join(browser_recovery_args(args.suite)))
        return
    if args.kind == "rust" and args.suite in ("fixtures", "channel-preview", "teletext-1.5", "teletext-runtime"):
        parser.error("browser recovery case sets cannot select Rust tests")
    print(rust_filter(args.suite, args.next_runtime) if args.kind == "rust" else browser_filter(args.suite))


if __name__ == "__main__":
    main()
