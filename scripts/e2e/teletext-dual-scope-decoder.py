#!/usr/bin/env python3
"""Independent fixed-vector checks for the eight-page dual-scope capture.

Reuse the frozen imager loader and its unchanged external decoder. Received
banks are supplied explicitly: this does not prove automatic X27/MOT routing,
font rendering, client navigation or complete authored Level 2.5. Fallback uses
receiver region 0 and checks literal codes, not Polish/Turkish font selection.
The input is the existing chronological, at-most-512-unit capture prefix; an
unfinished final acquisition is never closed by synthesizing another header.
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
spec = importlib.util.spec_from_file_location("dual_scope_national_oracle", path)
fixed = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixed)
imager, require = fixed.imager, fixed.require
DISPLAYS = {0x124, 0x125, 0x126, 0x127, 0x128, 0x129, 0x130, 0x131}


def slot(number):
    # Page numbers are BCD here; 12A..12F are not display identities.
    require(number in DISPLAYS, "Unexpected fixed display identity")
    return (int(f"{number:X}") - 124) % 4


def national_code(number):
    return 0x5B if slot(number) < 2 else 0x23


def mask(number):
    rows = list(fixed.POLISH if slot(number) < 2 else fixed.TURKISH)
    if slot(number) % 2:
        rows[9] = 0xFFF
    return rows


def cell(decoder, row, column, code, fragment, underline=False, drcs=None):
    where = f"cell {row},{column}"
    require(decoder.get_char_code(row, column) == code,
            f"{where}: literal character differs")
    require(decoder.get_fragment(row, column).value == fragment,
            f"{where}: enlarged fragment differs")
    require(fixed.colour(decoder, decoder.get_foreground(row, column)) == [255] * 3
            and fixed.colour(decoder, decoder.get_background(row, column)) == [0] * 3,
            f"{where}: neutral colours differ")
    require(not decoder.get_conceal(row, column)
            and not decoder.get_invert(row, column)
            and decoder.get_und_sep(row, column) == underline
            and decoder.get_flash_mode(row, column) == 0,
            f"{where}: original effects differ")
    if drcs is None:
        require(decoder.get_drcs_ptu(row, column) is None,
                f"{where}: unexpected DRCS")
        return
    scope, ptu, rows = drcs
    wanted = (decoder.DRCSType.DRCS_NORMAL if scope == "normal"
              else decoder.DRCSType.DRCS_GLOBAL)
    require(decoder.get_drcs_type(row, column) == wanted
            and decoder.get_drcs_subtable(row, column) == 0
            and decoder.get_drcs_ptu(row, column) == ptu,
            f"{where}: fixed DRCS scope/subtable/slot differs")
    bitmap = decoder.get_drcs_bitmap(row, column)
    require(bitmap is not None and list(bitmap) == fixed.bitmap(rows),
            f"{where}: literal national DRCS mask differs")


def received_banks(pages, theme):
    expected = {(0x2A2, 3)}
    if theme == "day":
        expected |= {(0x1A0, 0), (0x1A1, 0)}
    banks = {}
    for page in pages:
        if page.get("number") in (0x1A0, 0x1A1, 0x2A2):
            identity = (page["number"], page["subcode"] & 15)
            require(identity in expected, "Unexpected received resource identity")
            required_rows = (1, 2) if page["number"] != 0x2A2 else (3,)
            if (28, 3) in page and all(row in page for row in required_rows):
                banks.setdefault(identity, page)
    require(set(banks) == expected,
            "Complete authored and both generated resource banks are required")
    require(all(page.get("number") in DISPLAYS | {p for p, _ in expected}
                for page in pages), "Unexpected page in fixed capture")
    # The decoder indexes the supplied list by S1. The unrelated authored S1=3
    # bank is only inventoried here; no checked cell invokes it. Do not alias
    # that bank to S1=0 or S1=1 by compacting a sparse list.
    normal = []
    glob = []
    if theme == "day":
        normal.append(banks[(0x1A0, 0)])
        glob.append(banks[(0x1A1, 0)])
    return normal, glob, sorted(expected)


def check_display(decoder_type, page, normal, glob, theme):
    number = page["number"]
    code = national_code(number) if theme == "day" else 65
    for level in ("1", "1.5", "2.5"):
        decoder = decoder_type()
        decoder.decode(page, level=level, ndrcs_page=normal, gdrcs_page=glob)
        if level == "2.5":
            scope = "normal" if number < 0x128 else "global"
            glyph = (scope, slot(number), mask(number)) if theme == "day" else None
            underline = theme == "night" and bool(slot(number) % 2)
            for column, fragment in ((7, 3), (8, 4)):
                cell(decoder, 1, column, code, fragment, underline, glyph)
        else:
            cell(decoder, 1, 7, code, 0)
            cell(decoder, 1, 8, 32, 0)
            cell(decoder, 2, 8, 101, 0)
        cell(decoder, 3, 1, 66, 0)


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
    displays = [page for page in pages if page.get("number") in DISPLAYS
                and page.get("subcode") == 32 and 1 in page and (26, 0) in page]
    require({page["number"] for page in displays} == DISPLAYS,
            "All eight complete received displays are required")
    for page in displays:
        check_display(decoder_type, page, normal, glob, sys.argv[2])
    print(json.dumps({"decoder": "teletextimager", "version": imager.VERSION,
                      "commit": imager.COMMIT, "input_sha256": hashlib.sha256(raw).hexdigest(),
                      "fixture": "eight-page-four-variant-dual-scope",
                      "theme": sys.argv[2], "units": len(units),
                      "closed_displays_checked": len(displays),
                      "display_pages": [f"{p:X}" for p in sorted(DISPLAYS)],
                      "received_bank_inventory": [[f"{p:X}", s] for p, s in inventory],
                      "authored_s1_3_bank_inventory_only": True,
                      "levels": ["1", "1.5", "2.5"],
                      "literal_drcs_masks_scopes_slots_and_fragments": True,
                      "automatic_resource_routing_proven": False,
                      "font_pixels_or_actual_client_proven": False,
                      "fallback_receiver_region": 0}, sort_keys=True))


if __name__ == "__main__":
    try:
        main()
    except (imager.resource.CalibrationError, OSError, ValueError, AttributeError,
            RecursionError, KeyError, IndexError, TypeError) as error:
        message = (str(error) if isinstance(error, imager.resource.CalibrationError)
                   else "Independent dual-scope calibration could not run")
        print(json.dumps({"error": message}), file=sys.stderr)
        sys.exit(1)
