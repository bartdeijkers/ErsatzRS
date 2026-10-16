#!/usr/bin/env python3
"""Fixed three-linked-target oracle using the unchanged pinned imager decoder.

Usage: teletext-linked-targets-decoder.py UNITS.json day|night [--implicit-mot],
with the existing ERSATZRS_TELETEXT_TEST_IMAGER_DIR checkout. Inspect only chronological CLOSED
acquisitions from the actual <=512-unit prefix; never synthesize an EOF header.
Owners124/125/126 explicitly receive Normal2A0/2A2/2A3 respectively, preserving
logical S1 indices [authored0, generated1] on Day and [authored0] on Night.
All three receive Global1A1/[authored0]. Every complete received bank and display
is checked. Supplying these fixed banks does not prove automatic X27/MOT cache
routing, font pixels, actual client navigation or complete Level2.5 acceptance.
Level1/1.5 checks use receiver region0 and literal codes, not national fonts.
The explicit implicit-MOT mode additionally checks fixed encoded MOT bodies,
not semantic MOT resolution, automatic acquisition or lifecycle conformance.

Bounded negatives: both wrong-profile swaps must reject the fixed inventory.
For each FULL Normal page+S1 identity, separately toggle a valid-parity PTU0
pixel in every acquisition of that identity: S1=1 must reject the fixed Polish
mask; S1=0 must reject the authored zero bitmap. Keep captures immutable and
retain each derived negative independently; do not patch the external decoder.
"""

import hashlib
import importlib.util
import json
from pathlib import Path
import sys

sys.dont_write_bytecode = True
ORACLE_SHA = "2887bf80a370636d81126ac00ac90770e62fb3f5606a341027841ddeb93ee884"
path = Path(__file__).with_name("teletext-polish-carrier-decoder.py")
if hashlib.sha256(path.read_bytes()).hexdigest() != ORACLE_SHA:
    raise RuntimeError("Frozen national-carrier oracle changed")
spec = importlib.util.spec_from_file_location("linked_targets_national_oracle", path)
fixed = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixed)
imager, require = fixed.imager, fixed.require
NORMAL_BY_DISPLAY = {0x124: 0x2A0, 0x125: 0x2A2, 0x126: 0x2A3}
GLOBAL = 0x1A1


# EN300706 Figure20/Tables20-23 and this fixed fixture: primary identity1FE/S9,
# associations124/125/126, unused objects retaining rawNoObject77, original
# Global1A1 and Normal2A0/2A2/2A3 links with extent0 and no superseding flag.
# The unchanged pinned reader leaves coding3 rows as Hamming codewords &0x7F;
# compare that exact representation rather than claiming semantic MOT parsing.
HAMMING_8_4 = (0x15, 0x02, 0x49, 0x5E, 0x64, 0x73, 0x38, 0x2F,
               0xD0, 0xC7, 0x8C, 0x9B, 0xA1, 0xB6, 0xFD, 0xEA)
MOT_ROWS = (1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 19, 20, 21)


def check_implicit_mot(page):
    require(page.get("number") == 0x1FE and page.get("subcode") & 15 == 9,
            "Fixed MOT1FE logical S1=9 required")
    require(set(page) == {"number", "subcode", "control", 0, (28, 0), *MOT_ROWS},
            "Closed MOT1FE must contain exactly all seventeen body rows and X28/0")
    require(page[0] == b" " * 40, "Original MOT1FE header text differs")
    require(page[(28, 0)] == [0xFFB6] + [0] * 12,
            "MOT1FE X28/0 function6/coding3 or reserved fields differ")
    expected = {row: [0] * 40 for row in MOT_ROWS}
    expected[2] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 9, 0, 10, 0, 11,
                   0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                   0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
    expected[19] = [9, 15, 15, 0, 0, 0, 13, 4, 13, 4] * 4
    expected[20] = [9, 15, 15, 0, 0, 0, 13, 4, 13, 4] * 4
    expected[21] = ([1, 10, 1, 0, 2, 10, 0, 0, 2, 10, 2, 0, 2, 10, 3, 0]
                    + [9, 15, 15, 0] * 4 + [0] * 8)
    for row in MOT_ROWS:
        encoded = [HAMMING_8_4[nibble] & 0x7F for nibble in expected[row]]
        require(list(page[row]) == encoded, f"MOT1FE/S9 row{row}: encoded body differs")


def implicit_mot_pages(pages):
    mot_pages = [page for page in pages if page.get("number") == 0x1FE]
    require(bool(mot_pages), "At least one complete closed MOT1FE/S9 required")
    for page in mot_pages:
        check_implicit_mot(page)
    return [page for page in pages if page.get("number") != 0x1FE], len(mot_pages)


def check_bank(decoder_type, page, identity):
    # Keep the received S1 at its actual list index; index0 is unused for S1=1.
    subpage = identity[1]
    decoder = decoder_type()
    decoder.set_page([page] if subpage == 0 else [None, page])
    expected = [0] * 120 if subpage == 0 else fixed.bitmap(fixed.POLISH)
    require(decoder.ptu_mode(0, subpage) == 0
            and list(decoder.ptu_l2p5(0, subpage)) == expected,
            f"Received {identity[0]:X}/S1={subpage} PTU0 bitmap differs")


def received_banks(pages, theme):
    expected = {(number, 0) for number in NORMAL_BY_DISPLAY.values()} | {(GLOBAL, 0)}
    if theme == "day":
        expected |= {(number, 1) for number in NORMAL_BY_DISPLAY.values()}
    banks, counts = {}, {}
    decoder_type = sys.modules["teletextimager.teletextdrcs"].TeletextDRCSDecode
    for page in pages:
        number = page.get("number")
        require(number in NORMAL_BY_DISPLAY or number in NORMAL_BY_DISPLAY.values()
                or number == GLOBAL, "Unexpected page in fixed linked-target capture")
        if number in NORMAL_BY_DISPLAY:
            require(page.get("subcode") == 32, "Unexpected display subcode")
            continue
        identity = (number, page["subcode"] & 15)
        require(identity in expected, "Unexpected received resource identity")
        if (28, 3) in page and 1 in page:
            function = 4 if number == GLOBAL else 5
            require(page[(28, 3)][0] & 15 == function,
                    "Received resource function differs")
            check_bank(decoder_type, page, identity)
            banks.setdefault(identity, page)
            counts[identity] = counts.get(identity, 0) + 1
    require(set(banks) == expected,
            "Complete authored banks and exact profile-specific generated inventory required")
    normals = {owner: [banks[(target, subpage)]
                       for subpage in range(2 if theme == "day" else 1)]
               for owner, target in NORMAL_BY_DISPLAY.items()}
    return normals, [banks[(GLOBAL, 0)]], counts


def check_display(decoder_type, page, normal, glob, theme):
    code = 0x5B if theme == "day" else 65
    for level in ("1", "1.5", "2.5"):
        decoder = decoder_type()
        decoder.decode(page, level=level, ndrcs_page=normal, gdrcs_page=glob)
        try:
            if level == "2.5":
                glyph = (1, 0, fixed.POLISH) if theme == "day" else None
                fixed.cell(decoder, 1, 7, code, 3, glyph)
                fixed.cell(decoder, 1, 8, code, 4, glyph)
            else:
                fixed.cell(decoder, 1, 7, code, 0)
                fixed.cell(decoder, 1, 8, 32, 0)
                fixed.cell(decoder, 2, 8, 101, 0)
            fixed.cell(decoder, 3, 1, 66, 0)
        except imager.resource.CalibrationError as error:
            raise imager.resource.CalibrationError(
                f"Display {page['number']:X}/S32 Level{level}: {error}") from error


def main():
    require(len(sys.argv) in (3, 4) and sys.argv[2] in ("day", "night")
            and (len(sys.argv) == 3 or sys.argv[3] == "--implicit-mot"),
            "Expected captured units file and fixed day/night profile [--implicit-mot]")
    implicit_mot = len(sys.argv) == 4
    source = Path(sys.argv[1])
    require(source.is_file() and not source.is_symlink(),
            "Expected regular captured units file")
    with source.open("rb") as stream:
        raw = stream.read(imager.resource.MAX_INPUT + 1)
    require(0 < len(raw) <= imager.resource.MAX_INPUT, "Input byte bound exceeded")
    units = json.loads(raw, object_pairs_hook=imager.resource.unique_object)
    require(type(units) is list and 1 <= len(units) <= 512, "Invalid unit count")
    for unit in units:
        require(type(unit) is list and len(unit) == 46 and unit[:2] == [2, 44]
                and unit[3] == 0xE4
                and all(type(byte) is int and 0 <= byte <= 255 for byte in unit),
                "Invalid actual DVB unit")
    reader_type, decoder_type = imager.load_decoder()
    pages = fixed.closed_pages(reader_type, units)
    if implicit_mot:
        pages, mot_count = implicit_mot_pages(pages)
    normals, glob, bank_counts = received_banks(pages, sys.argv[2])
    displays = [page for page in pages if page.get("number") in NORMAL_BY_DISPLAY
                and page.get("subcode") == 32 and 1 in page and (26, 0) in page]
    require({page["number"] for page in displays} == set(NORMAL_BY_DISPLAY),
            "All three complete received displays are required")
    counts = {number: 0 for number in NORMAL_BY_DISPLAY}
    for page in displays:
        check_display(decoder_type, page, normals[page["number"]], glob, sys.argv[2])
        counts[page["number"]] += 1
    report = {
        "decoder": "teletextimager", "version": imager.VERSION, "commit": imager.COMMIT,
        "frozen_national_oracle_sha256": ORACLE_SHA,
        "frozen_loader_sha256": fixed.LOADER_SHA, "external_source_sha256": imager.HASHES,
        "input_sha256": hashlib.sha256(raw).hexdigest(),
        "input_byte_limit": imager.resource.MAX_INPUT, "unit_limit": 512, "units": len(units),
        "fixture": "three-distinct-linked-targets-authored0-compiler1", "theme": sys.argv[2],
        "closed_displays_checked": len(displays),
        "closed_displays_by_page": {f"{page:X}": count for page, count in counts.items()},
        "levels": ["1", "1.5", "2.5"],
        "received_bank_inventory": [[f"{page:X}", subpage] for page, subpage in sorted(bank_counts)],
        "complete_closed_banks_checked": [
            {"page": f"{page:X}", "s1": subpage, "count": count}
            for (page, subpage), count in sorted(bank_counts.items())],
        "normal_bank_by_display": {f"{owner:X}": f"{target:X}"
                                   for owner, target in NORMAL_BY_DISPLAY.items()},
        "normal_indexed_s1": [0, 1] if sys.argv[2] == "day" else [0],
        "global_indexed_s1": [0], "all_authored_slot0_bitmaps_zero": True,
        "all_three_literal_polish_s1_1_slot0_and_fragments": sys.argv[2] == "day",
        "fallback_receiver_region": 0, "only_closed_acquisitions_checked": True,
        "automatic_resource_routing_proven": False, "font_pixels_or_actual_client_proven": False,
        "full_engine_acceptance": False,
    }
    if implicit_mot:
        report.update({
            "fixture": "implicit-mot-three-distinct-linked-targets-authored0-compiler1",
            "closed_mots_checked": mot_count, "mot_page": "1FE", "mot_logical_s1": 9,
            "encoded_mot_body_correspondence": True,
            "semantic_mot_resolution_proven": False,
            "mot_lifecycle_independently_proven": False,
        })
    print(json.dumps(report, sort_keys=True))


if __name__ == "__main__":
    try:
        main()
    except (imager.resource.CalibrationError, OSError, ValueError, AttributeError,
            RecursionError, KeyError, IndexError, TypeError) as error:
        message = (str(error) if isinstance(error, imager.resource.CalibrationError)
                   else "Independent linked-target calibration could not run")
        print(json.dumps({"error": message}), file=sys.stderr)
        sys.exit(1)
