#!/usr/bin/env python3
"""Disjoint CI ownership; these buckets do not establish level acceptance.

Shared/base tests default to 1.5. Enhanced feature families (including their
base fallback assertions) belong to 2.5. Binary matching retains integration
tests whose individual names do not contain 'teletext'.
"""

import argparse


TELETEXT = r"teletext|drcs|zvbi|tti_"
ENHANCED = (
    r"drcs|object|resource|palette|panel|mot_|m29|x28|colour_composition|"
    r"composed_main|main_composition|origin_carrier|normalized_editorial|"
    r"augmented_origin|display_resolution|attribute_execution|attribute_observation|"
    r"enhancement_program|enhancement_objects|editorial_tti|editorial_effects|"
    r"compatibility_lowering|physical_spacing|authored_service|level_?2_?5"
)
ENHANCED_SPECS = (
    "teletext-bank-dependencies",
    "teletext-definition-add",
    "teletext-definition-copy",
    "teletext-object-selection",
    "teletext-resource-bank-flow",
    "teletext-resource-diagnostics",
    "teletext-retained-bank",
)


def rust_filter(suite, next_runtime=False):
    teletext = f"(test(/{TELETEXT}/) | binary(/{TELETEXT}/))"
    enhanced = f"(test(/{ENHANCED}/) | binary(=teletext_profile_preparation))"
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


def browser_filter(suite):
    teletext = r"[^/]*teletext[^/]*\.spec\.ts$"
    enhanced = "(" + "|".join(ENHANCED_SPECS) + r")\.spec\.ts$"
    if suite == "general":
        return r"(^|/)(?![^/]*teletext)[^/]+\.spec\.ts$"
    if suite == "2.5":
        return enhanced
    return rf"(^|/)(?!(?:{'|'.join(ENHANCED_SPECS)})\.spec\.ts$){teletext}"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("kind", choices=("rust", "browser"))
    parser.add_argument("suite", choices=("general", "1.5", "2.5", "all", "next"))
    parser.add_argument("--next-runtime", action="store_true")
    args = parser.parse_args()
    if args.kind == "browser" and args.suite not in ("general", "1.5", "2.5"):
        parser.error("browser selection requires general, 1.5 or 2.5")
    print(rust_filter(args.suite, args.next_runtime) if args.kind == "rust" else browser_filter(args.suite))


if __name__ == "__main__":
    main()
