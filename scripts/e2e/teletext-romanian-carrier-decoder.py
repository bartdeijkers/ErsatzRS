#!/usr/bin/env python3
"""Fixed Romanian four-owner capture oracle, using the unchanged pinned imager.

Usage: teletext-romanian-carrier-decoder.py UNITS.json day|night, with the existing
ERSATZRS_TELETEXT_TEST_IMAGER_DIR checkout. Inspect chronological CLOSED complete
acquisitions from the actual <=512-unit prefix; never synthesize an EOF header.
Day receives Normal1A0/S1=0 slots4..7 for the exact national5D/7D originals and
their underline variants. Night retains original A, underlined on125/127, and
has no bank. The literal masks are the existing pinned national font policy;
no breve/caron conversion or modern Unicode alias is inferred.

Received banks are explicitly supplied, not automatically routed by X27/MOT.
This checks fixed codes, effects, fragments and DRCS bitmaps, not rendered font
pixels, client navigation or full Level2.5 acceptance. Level1/1.5 use receiver
region0 and literal fallback codes. EOF acquisition tails remain outside scope.
Negatives: both profile swaps and four separate valid-parity PTU pixel toggles,
one per slot in every received bank repetition. Retain original units unchanged.
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
spec = importlib.util.spec_from_file_location("romanian_national_oracle", path)
fixed = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixed)
imager, require = fixed.imager, fixed.require
OWNERS = {0x124: 4, 0x125: 5, 0x126: 6, 0x127: 7}
BANK = 0x1A0
CAPITAL = (0x0D8, 0x070, 0x1FC, 0x306, 0x306, 0x3FE, 0x306, 0x306, 0, 0)
SMALL = (0x0D8, 0x070, 0, 0x3F8, 0x00C, 0x3FC, 0x60C, 0x3FE, 0, 0)


def mask(slot):
    rows = list(CAPITAL if slot < 6 else SMALL)
    if slot % 2:
        rows[9] = 0xFFF
    return rows


def received_bank(pages, theme):
    banks = []
    decoder_type = sys.modules["teletextimager.teletextdrcs"].TeletextDRCSDecode
    for page in pages:
        number = page.get("number")
        require(number in OWNERS or number == BANK,
                "Unexpected page in fixed Romanian capture")
        if number in OWNERS:
            require(page.get("subcode") == 32,
                    f"Display {number:X}: expected subcode32")
            continue
        require(theme == "day", "Night must have no generated resource bank")
        require(page["subcode"] & 15 == 0, "Normal1A0: expected S1=0")
        if (28, 3) not in page or 3 not in page or 4 not in page:
            continue
        require(page[(28, 3)][0] & 15 == 5, "Normal1A0/S0: function differs")
        decoder = decoder_type()
        decoder.set_page([page])
        for slot in range(4, 8):
            require(decoder.ptu_mode(slot, 0) == 0
                    and list(decoder.ptu_l2p5(slot, 0)) == fixed.bitmap(mask(slot)),
                    f"Normal1A0/S0 slot{slot}: fixed mode/bitmap differs")
        banks.append(page)
    require(bool(banks) == (theme == "day"),
            "Complete Normal1A0/S0 required on Day and absent on Night")
    return [banks[0]] if banks else [], len(banks)


def night_cell(decoder, row, column, fragment, underlined):
    where = f"cell {row},{column}"
    require(decoder.get_char_code(row, column) == 65,
            f"{where}: original A code differs")
    require(decoder.get_fragment(row, column).value == fragment,
            f"{where}: fragment differs")
    require(fixed.colour(decoder, decoder.get_foreground(row, column)) == [255, 255, 255]
            and fixed.colour(decoder, decoder.get_background(row, column)) == [0, 0, 0],
            f"{where}: palette colours differ")
    require(not decoder.get_conceal(row, column) and not decoder.get_invert(row, column)
            and decoder.get_flash_mode(row, column) == 0
            and bool(decoder.get_und_sep(row, column)) == underlined,
            f"{where}: original A effects differ")
    require(decoder.get_drcs_ptu(row, column) is None,
            f"{where}: original A must not become DRCS")


def check_display(decoder_type, page, bank, theme):
    slot = OWNERS[page["number"]]
    code = (0x5D if slot < 6 else 0x7D) if theme == "day" else 65
    for level in ("1", "1.5", "2.5"):
        decoder = decoder_type()
        try:
            decoder.decode(page, level=level, ndrcs_page=bank)
            if level == "2.5":
                if theme == "day":
                    fixed.cell(decoder, 1, 7, code, 3, (0, slot, mask(slot)))
                    fixed.cell(decoder, 1, 8, code, 4, (0, slot, mask(slot)))
                else:
                    night_cell(decoder, 1, 7, 3, bool(slot % 2))
                    night_cell(decoder, 1, 8, 4, bool(slot % 2))
            else:
                fixed.cell(decoder, 1, 7, code, 0)
                fixed.cell(decoder, 1, 8, 32, 0)
                fixed.cell(decoder, 2, 8, 101, 0)
            fixed.cell(decoder, 3, 1, 66, 0)
        except imager.resource.CalibrationError as error:
            raise imager.resource.CalibrationError(
                f"Display {page['number']:X}/S32 Level{level}: {error}") from error


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
    bank, bank_count = received_bank(pages, sys.argv[2])
    displays = [page for page in pages if page.get("number") in OWNERS
                and page.get("subcode") == 32 and 1 in page and (26, 0) in page]
    require({page["number"] for page in displays} == set(OWNERS),
            "All four complete received display owners are required")
    counts = {number: 0 for number in OWNERS}
    for page in displays:
        check_display(decoder_type, page, bank, sys.argv[2])
        counts[page["number"]] += 1
    print(json.dumps({
        "decoder": "teletextimager", "version": imager.VERSION, "commit": imager.COMMIT,
        "frozen_national_oracle_sha256": ORACLE_SHA,
        "frozen_loader_sha256": fixed.LOADER_SHA, "external_source_sha256": imager.HASHES,
        "input_sha256": hashlib.sha256(raw).hexdigest(),
        "input_byte_limit": imager.resource.MAX_INPUT, "unit_limit": 512, "units": len(units),
        "fixture": "romanian-four-originals-compiler-slots4-7", "theme": sys.argv[2],
        "closed_displays_checked": len(displays),
        "closed_displays_by_page": {f"{page:X}": count for page, count in counts.items()},
        "levels": ["1", "1.5", "2.5"], "complete_closed_banks_checked": bank_count,
        "received_bank_inventory": [["1A0", 0]] if bank else [],
        "normal_indexed_s1": [0] if bank else [],
        "fixed_generated_slots": [4, 5, 6, 7] if bank else [],
        "literal_national_masks_and_fragments": sys.argv[2] == "day",
        "original_night_A_underlined_pages": ["125", "127"] if sys.argv[2] == "night" else [],
        "fallback_receiver_region": 0, "only_closed_acquisitions_checked": True,
        "automatic_resource_routing_proven": False, "direct_national_recipe_proven": False,
        "font_pixels_or_actual_client_proven": False, "full_engine_acceptance": False,
    }, sort_keys=True))


if __name__ == "__main__":
    try:
        main()
    except (imager.resource.CalibrationError, OSError, ValueError, AttributeError,
            RecursionError, KeyError, IndexError, TypeError) as error:
        message = (str(error) if isinstance(error, imager.resource.CalibrationError)
                   else "Independent Romanian calibration could not run")
        print(json.dumps({"error": message}), file=sys.stderr)
        sys.exit(1)
