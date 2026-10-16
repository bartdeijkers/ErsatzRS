#!/usr/bin/env python3
"""Fixed linked-S1 capture oracle using the unchanged pinned imager decoder.

Usage: teletext-linked-subpage-decoder.py UNITS.json day|night, with the existing
ERSATZRS_TELETEXT_TEST_IMAGER_DIR checkout. Only closed acquisitions from the
unchanged chronological <=512-unit prefix are inspected; EOF is never closed by
synthesizing a header. Received Normal lists preserve S1: [authored0, generated1]
on Day and [authored0] on Night. Global retains its received authored S1=0.
Supplying banks is not automatic X27/MOT routing, font rendering or client proof.
Fallback checks literal codes with receiver region0, not national font pixels.

Negative method: swapping day/night must reject the fixed inventory. Toggle one
valid-parity PTU0 pixel in every received Normal2A0/S1=1 acquisition to reject the
fixed Polish mask; a separate Normal2A0/S1=0 mutation must reject the zero authored
mask. Keep original capture units immutable and retain each negative separately.
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
spec = importlib.util.spec_from_file_location("linked_subpage_national_oracle", path)
fixed = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixed)
imager, require = fixed.imager, fixed.require
DISPLAYS = {0x124, 0x125}


def received_banks(pages, theme):
    expected = {(0x2A0, 0), (0x1A1, 0)}
    if theme == "day":
        expected.add((0x2A0, 1))
    banks = {}
    for page in pages:
        number = page.get("number")
        require(number in DISPLAYS | {0x2A0, 0x1A1},
                "Unexpected page in fixed linked-subpage capture")
        if number in DISPLAYS:
            require(page.get("subcode") == 32, "Unexpected display subcode")
            continue
        identity = (number, page["subcode"] & 15)
        require(identity in expected, "Unexpected received resource identity")
        if (28, 3) in page and 1 in page:
            function = 5 if number == 0x2A0 else 4
            require(page[(28, 3)][0] & 15 == function,
                    "Received resource function differs")
            banks.setdefault(identity, page)
    require(set(banks) == expected,
            "Complete authored banks and exact generated S1 inventory required")
    normal = [banks[(0x2A0, 0)]]
    if theme == "day":
        normal.append(banks[(0x2A0, 1)])
    return normal, [banks[(0x1A1, 0)]], sorted(expected)


def check_bank_bitmaps(normal, glob, theme):
    # This module was loaded only from the unchanged eight-source hash pins.
    decoder_type = sys.modules["teletextimager.teletextdrcs"].TeletextDRCSDecode
    for banks in (normal, glob):
        decoder = decoder_type()
        decoder.set_page(banks)
        require(decoder.ptu_mode(0, 0) == 0
                and list(decoder.ptu_l2p5(0, 0)) == [0] * 120,
                "Authored S1=0 slot0 must retain its zero bitmap")
    if theme == "day":
        decoder = decoder_type()
        decoder.set_page(normal)
        require(decoder.ptu_mode(0, 1) == 0
                and list(decoder.ptu_l2p5(0, 1)) == fixed.bitmap(fixed.POLISH),
                "Generated Normal S1=1 slot0 literal Polish mask differs")


def check_display(decoder_type, page, normal, glob, theme):
    for level in ("1", "1.5", "2.5"):
        decoder = decoder_type()
        decoder.decode(page, level=level, ndrcs_page=normal, gdrcs_page=glob)
        if page["number"] == 0x124:
            code = 0x5B if theme == "day" else 65
            if level == "2.5":
                glyph = (1, 0, fixed.POLISH) if theme == "day" else None
                fixed.cell(decoder, 1, 7, code, 3, glyph)
                fixed.cell(decoder, 1, 8, code, 4, glyph)
            else:
                fixed.cell(decoder, 1, 7, code, 0)
                fixed.cell(decoder, 1, 8, 32, 0)
                fixed.cell(decoder, 2, 8, 101, 0)
            fixed.cell(decoder, 3, 1, 66, 0)
        elif level == "2.5":
            foreground = (255, 0, 0) if theme == "day" else (0, 0, 255)
            for column in (8, 10):
                fixed.cell(decoder, 1, column, 65, 0, None, foreground)
        else:
            for column, code in ((8, 90), (9, 90), (10, 32)):
                fixed.cell(decoder, 1, column, code, 0)


def main():
    require(len(sys.argv) == 3 and sys.argv[2] in ("day", "night"),
            "Expected captured units file and fixed day/night profile")
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
    normal, glob, inventory = received_banks(pages, sys.argv[2])
    check_bank_bitmaps(normal, glob, sys.argv[2])
    displays = [page for page in pages if page.get("number") in DISPLAYS
                and page.get("subcode") == 32 and 1 in page and (26, 0) in page]
    require({page["number"] for page in displays} == DISPLAYS,
            "Both complete received displays are required")
    for page in displays:
        check_display(decoder_type, page, normal, glob, sys.argv[2])
    print(json.dumps({"decoder": "teletextimager", "version": imager.VERSION,
                      "commit": imager.COMMIT, "input_sha256": hashlib.sha256(raw).hexdigest(),
                      "fixture": "occupied-links-authored0-compiler1",
                      "theme": sys.argv[2], "units": len(units),
                      "closed_displays_checked": len(displays),
                      "display_pages": ["124", "125"], "levels": ["1", "1.5", "2.5"],
                      "received_bank_inventory": [[f"{p:X}", s] for p, s in inventory],
                      "normal_indexed_s1": list(range(len(normal))), "global_indexed_s1": [0],
                      "authored_slot0_bitmaps_zero": True,
                      "literal_polish_s1_1_slot0_and_fragments": sys.argv[2] == "day",
                      "page125_g0_foreground_rgb8": [255, 0, 0] if sys.argv[2] == "day" else [0, 0, 255],
                      "automatic_resource_routing_proven": False,
                      "font_pixels_or_actual_client_proven": False,
                      "full_engine_acceptance": False, "fallback_receiver_region": 0}, sort_keys=True))


if __name__ == "__main__":
    try:
        main()
    except (imager.resource.CalibrationError, OSError, ValueError, AttributeError,
            RecursionError, KeyError, IndexError, TypeError) as error:
        message = (str(error) if isinstance(error, imager.resource.CalibrationError)
                   else "Independent linked-subpage calibration could not run")
        print(json.dumps({"error": message}), file=sys.stderr)
        sys.exit(1)
