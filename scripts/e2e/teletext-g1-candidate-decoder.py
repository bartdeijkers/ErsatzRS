#!/usr/bin/env python3
"""Positive fixed-vector G1 oracle for a separately receipted test decoder.

Checks unchanged ordinary captured units against the frozen normative six-cell
G1 vectors and B control on both pages and both levels. This neither reruns nor
claims the separate retained imager proof. Candidate conformance here is limited
to those vectors; client, panel and complete Level2.5 acceptance remain open.
"""

import argparse
import ctypes as C
import hashlib
import json
import os
from pathlib import Path
import re
import sys
import types

sys.dont_write_bytecode = True
LIVE_SHA256 = "e1b0d81ecb3026da714cc49e5fd6766a65ab0be99dd399556503c23bae6ce881"
SPARSE_SHA256 = "721ccf8702bc86be049b184142ff1ee577371a776920415f0054db3a7195d174"
RECIPE_SHA256 = "dec31caae97615081d7e8b2d7f9b70ea199a2a2860e82b9aca4b78176cea8085"
FORMATTER_SHA256 = "9bd5df7a7837ac558f0e14a1d493e8022f76aeb1d3bbd812b3bbd903fef71db7"
LABEL = "test-only-x27-normal-drcs-and-g1-later-spacing-candidate"
MAX_RECEIPT = 128 * 1024
LIMITATIONS = {
    "test_only": True, "production_dependency": False, "independent_main_columns": 41,
    "side_panels_proven": False, "full_client_acceptance": False, "full_level25_acceptance": False,
    "g1_correction_conformance_proven": False, "prior_object_glyph_provenance_proven": False,
    "required_proof": ["unchanged four G1 vectors", "later enhanced G1 ignores Base separation",
                       "D5 true survives contiguous spacing", "explicit D5 false rearms cancellation",
                       "Local and offset Active scope", "Adaptive extent and Passive private state",
                       "D5 false still clears inherited alphabetic underline", "row and invocation reset",
                       "prior-object enhanced G1 provenance"],
}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def frozen(name, expected):
    path = Path(__file__).with_name(name)
    require(path.is_file() and not path.is_symlink(), "Expected regular frozen G1 source")
    with path.open("rb") as stream:
        body = stream.read(65537)
    require(len(body) <= 65536 and hashlib.sha256(body).hexdigest() == expected, "Frozen G1 candidate dependency changed")
    module = types.ModuleType("authenticated_g1_candidate_" + name.replace("-", "_"))
    module.__file__ = str(path)
    exec(compile(body, str(path), "exec"), module.__dict__)
    return module


LIVE = frozen("teletext-g1-live-decoder.py", LIVE_SHA256)
SPARSE = frozen("teletext-zvbi-sparse-proof.py", SPARSE_SHA256)
BASE = LIVE.BASE
SOURCE_SHA256 = {**LIVE.SOURCES, "teletext-g1-live-decoder.py": LIVE_SHA256,
                 "teletext-zvbi-sparse-proof.py": SPARSE_SHA256}


def equal_json(actual, expected):
    return json.dumps(actual, sort_keys=True) == json.dumps(expected, sort_keys=True)


def verify_build(build, proof_root, external_build_sha256):
    """Return exact G1 receipt bytes and entry after authenticating its chain."""
    require(not build.is_symlink() and not proof_root.is_symlink(), "Expected real G1 build/input directories")
    build, proof_root = build.resolve(), proof_root.resolve()
    raw = LIVE.bounded(build / "proof-g1-build.json", MAX_RECEIPT)
    require(SPARSE.hash_shape(external_build_sha256) and hashlib.sha256(raw).hexdigest() == external_build_sha256,
            "Externally pinned G1 build receipt mismatch")
    receipt = SPARSE.closed_json(raw)
    require(type(receipt) is dict and set(receipt) == {
        "schema", "source", "requested_image", "base_receipt", "sparse_receipt", "recipes",
        "original_formatter_sha256", "sparse_formatter_sha256", "preserved_outputs", "configuration",
        "base_tool_versions_sha256", "sparse_tool_versions_sha256", "tool_versions_sha256", "variants", "limitations"},
        "Unexpected G1 build receipt members")
    require(type(receipt["schema"]) is int and receipt["schema"] == 1, "Unsupported G1 build receipt")
    sparse_link = receipt["sparse_receipt"]
    require(type(sparse_link) is dict and set(sparse_link) == {"path", "sha256"}
            and sparse_link["path"] == "proof-sparse-build.json", "Invalid sparse build receipt link")
    base_raw, sparse_raw, _ = SPARSE.verify_build(build, proof_root, sparse_link["sha256"])
    sparse = SPARSE.closed_json(sparse_raw)
    require(receipt["base_receipt"] == {"path": "proof-build.json", "sha256": hashlib.sha256(base_raw).hexdigest()},
            "Original build receipt link mismatch")
    require(equal_json(receipt["source"], sparse["source"]) and receipt["requested_image"] == SPARSE.IMAGE
            and equal_json(receipt["configuration"], SPARSE.CONFIGURATION), "G1 source/configuration provenance mismatch")
    recipes = {**SPARSE.RECIPES, "g1": {"path": "scripts/teletext-zvbi-g1-build.sh", "sha256": RECIPE_SHA256}}
    require(receipt["recipes"] == recipes and SPARSE.digest(proof_root / recipes["g1"]["path"], 65536) == RECIPE_SHA256,
            "G1 build recipe pin mismatch")
    require(receipt["original_formatter_sha256"] == SPARSE.FORMATTERS["original"]
            and receipt["sparse_formatter_sha256"] == SPARSE.FORMATTERS["sparse"], "G1 formatter ancestry mismatch")
    require(receipt["base_tool_versions_sha256"] == sparse["base_tool_versions_sha256"]
            and receipt["sparse_tool_versions_sha256"] == sparse["tool_versions_sha256"]
            and SPARSE.digest(build / "g1-tool-versions.log", 65536) == receipt["tool_versions_sha256"], "G1 tool receipt mismatch")
    require(equal_json(receipt["limitations"], LIMITATIONS), "G1 test-only candidate boundary changed")
    require(type(receipt["variants"]) is dict and set(receipt["variants"]) == {"g1"}, "Expected the separate fourth G1 variant")
    entry = receipt["variants"]["g1"]
    require(type(entry) is dict and set(entry) == {"label", "packet_sha256", "formatter_sha256", "library", "library_sha256",
                                                "configure_log_sha256", "bootstrap_log_sha256", "build_log_sha256"}, "Unexpected G1 variant members")
    require(entry["label"] == LABEL and entry["packet_sha256"] == SPARSE.PARSERS["sparse"]
            and entry["formatter_sha256"] == FORMATTER_SHA256 and entry["library"] == "g1/libzvbi.so.0.13.2", "G1 variant source/path pins mismatch")
    for filename, expected in [("packet.c", SPARSE.PARSERS["sparse"]), ("teletext.c", FORMATTER_SHA256),
                               ("COPYING.md", SPARSE.SOURCE["copying_sha256"]), ("libzvbi.so.0.13.2", entry["library_sha256"]),
                               ("config-details.log", entry["configure_log_sha256"]), ("bootstrap.log", entry["bootstrap_log_sha256"]),
                               ("build.log", entry["build_log_sha256"])]:
        require(SPARSE.hash_shape(expected) and SPARSE.digest(build / "g1" / filename) == expected, "G1 variant content changed")
    # Replay only the bounded preservation manifest already generated by the
    # pinned recipe; do not discover or invent additional build inventory.
    preserved = receipt["preserved_outputs"]
    require(type(preserved) is dict and 1 <= len(preserved) <= 1024, "Invalid prior-output preservation receipt")
    for name, expected in preserved.items():
        relative = Path(name)
        require(not relative.is_absolute() and ".." not in relative.parts and bool(relative.parts), "Invalid preserved output path")
        path = build / relative
        require(type(expected) is dict and set(expected) in ({"sha256"}, {"symlink"}), "Invalid preserved output entry")
        actual = {"symlink": str(path.readlink())} if path.is_symlink() else {"sha256": SPARSE.digest(path)}
        require(actual == expected, "Earlier decoder output changed")
    return raw, entry


def expected_report():
    pages = []
    for number in (0x124, 0x125):
        for level in (1, 2):
            cells = []
            for column in (8, 9, 12, 13, 20, 21):
                separated = column >= 20 or level == 1 and column < 12
                cells.append({"row": 1, "column": column, "pattern": "separated" if separated else "contiguous",
                              "unicode": 0xEE5F if separated else 0xEE7F, "foreground": 7, "background": 0, "pixels_checked": 120})
            pages.append({"raw_page": number, "subcode": 32, "level": "1.5" if level == 1 else "2.5", "cells": cells,
                          "B_control": {"row": 2, "column": 1, "unicode": 66, "foreground": 7, "background": 0,
                                        "normal_opaque_steady_unconcealed": True}})
    return pages


def check_page(lib, decoder, expected):
    page, fetched = BASE.Page(), False
    level = 1 if expected["level"] == "1.5" else 2
    try:
        fetched = bool(lib.vbi_fetch_vt_page(decoder, C.byref(page), expected["raw_page"], 32, level, 25, 0))
        require(fetched and page.pgno == expected["raw_page"] and page.subno == 32 and page.rows == 25 and page.columns == 41,
                "G1 candidate page identity/geometry differs")
        LIVE.stock_base(page)
        for wanted in expected["cells"]:
            cell = page.text[41 + wanted["column"]]
            require(cell.bits1 >> 24 == 7 and cell.bits2 & 255 == 0, "G1 candidate literal white7/black0 indices differ")
            LIVE.MOSAIC.check_page(lib, page, {"page": expected["raw_page"], "subcode": 32, "row": 1,
                                              "column": wanted["column"], "pattern": wanted["pattern"]})
    finally:
        if fetched:
            lib.vbi_unref_page(C.byref(page))


def mapped_library(library, expected_sha256, mappings):
    records = []
    for line in mappings.splitlines():
        parts = line.split(maxsplit=5)
        if len(parts) == 6 and "/libzvbi.so." in parts[5]:
            path = re.sub(r"\\([0-7]{3})", lambda match: chr(int(match[1], 8)), parts[5])
            records.append((path, parts[3], parts[4]))
    require({path for path, _, _ in records} == {str(library)}, "Expected only the explicitly receipted mapped G1 decoder")
    stat = library.stat()
    for _, device, inode in records:
        require(tuple(int(part, 16) for part in device.split(":")) == (os.major(stat.st_dev), os.minor(stat.st_dev))
                and int(inode) == stat.st_ino, "Mapped G1 decoder file identity changed")
    require(SPARSE.digest(library) == expected_sha256, "Mapped G1 decoder content changed")
    return {"path": str(library), "sha256": expected_sha256, "mapped_file_identity_verified": True}


def decode(units, library, library_sha256):
    require(os.environ.get("LD_LIBRARY_PATH") == str(library.parent), "LD_LIBRARY_PATH must select exactly the receipted G1 directory before decoder loading")
    lib, version = BASE.load_decoder()
    decoder = lib.vbi_decoder_new()
    require(bool(decoder), "G1 candidate allocation failed")
    callback = BASE.CALLBACK(lambda event, user: None)
    try:
        require(lib.vbi_event_handler_register(decoder, 2, callback, None), "G1 candidate registration failed")
        lib.vbi_teletext_set_level(decoder, 2)
        for index, unit in enumerate(units):
            sliced = BASE.Sliced()
            sliced.id, sliced.line = 3, unit[2] & 31
            sliced.data[:42] = bytes(BASE.BIT_REVERSE[value] for value in unit[4:])
            lib.vbi_decode(decoder, C.byref(sliced), 1, 1.0 + index * .04)
        pages = expected_report()
        for page in pages:
            check_page(lib, decoder, page)
        mapped = mapped_library(library, library_sha256, LIVE.bounded(Path("/proc/self/maps"), 4 * 1024 * 1024).decode())
        return version, pages, mapped
    finally:
        lib.vbi_decoder_delete(decoder)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("units", type=Path)
    parser.add_argument("--library", required=True, type=Path)
    parser.add_argument("--library-sha256", required=True)
    parser.add_argument("--build-receipt", required=True, type=Path)
    parser.add_argument("--build-receipt-sha256", required=True)
    parser.add_argument("--proof-root", required=True, type=Path)
    args = parser.parse_args()
    require(not args.library.is_symlink() and not args.build_receipt.is_symlink(), "Expected regular explicit G1 library/receipt files")
    receipt_path = args.build_receipt.absolute()
    require(receipt_path.name == "proof-g1-build.json", "Expected the explicit fourth-variant build receipt")
    build = receipt_path.parent.resolve()
    raw_receipt, entry = verify_build(build, args.proof_root, args.build_receipt_sha256)
    library = args.library.absolute()
    require(library == build / entry["library"] and args.library_sha256 == entry["library_sha256"], "Explicit G1 library path/hash does not match the pinned build receipt")
    raw, units = LIVE.captured_units(args.units)
    version, pages, mapped = decode(units, library, args.library_sha256)
    require(LIVE.bounded(args.units, BASE.MAX_INPUT) == raw, "Ordinary G1 captured input changed during decoding")
    print(json.dumps({"schema": 1, "variant": "g1", "decoder_label": LABEL, "decoder_version": version,
                      "units_sha256": hashlib.sha256(raw).hexdigest(), "unit_count": len(units), "source_sha256": SOURCE_SHA256,
                      "recipe_sha256": RECIPE_SHA256, "build_receipt_sha256": hashlib.sha256(raw_receipt).hexdigest(),
                      "library": mapped, "pages": pages, "candidate_conformance": True, "imager_rerun": False,
                      "actual_client_proven": False, "panels_proven": False, "full_level25_proven": False,
                      "full_client_acceptance": False}, sort_keys=True))


if __name__ == "__main__":
    try:
        main()
    except (BASE.CalibrationError, LIVE.COMPOSITION.resource.CalibrationError, OSError, ValueError, KeyError,
            IndexError, TypeError, AttributeError, RuntimeError, RecursionError) as error:
        print(json.dumps({"error": str(error)}), file=sys.stderr)
        sys.exit(1)
