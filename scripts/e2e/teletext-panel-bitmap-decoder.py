#!/usr/bin/env python3
"""Fixed Main+RightPanel singleton oracle using unmodified teletextimager.

Usage: teletext-panel-bitmap-decoder.py UNITS.json day|night, with the existing
ERSATZRS_TELETEXT_TEST_IMAGER_DIR checkout. Inspect every genuinely CLOSED
124/125/S32 acquisition and Normal1C0/S0 bank in the unchanged <=512-unit
chronological prefix. Never synthesize a header or accept an EOF partial page.

This fixture carries presentation in actual M29/0, NOT display X28/0. The frozen
reader discards M29, so its own pinned Hamming converters extract those captured
words for the decoder's existing m29_page API. Every observed context must agree,
and must precede dependent display headers. Actual received banks are explicitly
supplied too. This proves fixed bitmap cells/palette/received geometry, not
automatic magazine/MOT/X27 acquisition, a PNG renderer, font or suitable-client
navigation. The original MOT is checked only for fixed encoded correspondence.

Bounded negatives: swap day/night; separately toggle a valid-parity PTU0 pixel
in EVERY Normal1C0/S0 acquisition; move the local Passive invocation by changing
its captured OriginModifier data32 to31 with valid Hamming24/18, or replace that
invocation with a valid reserved/no-effect triplet. Every closed display must
retain both exact raw cells. Keep positive inputs immutable and never inject
native bank/context bytes, fabricate X28 or repair the external decoder.
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
spec = importlib.util.spec_from_file_location("panel_bitmap_national_oracle", path)
fixed = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixed)
imager, require = fixed.imager, fixed.require
DISPLAYS = {0x124, 0x125}
NIGHT = (0x801, 0x800, 0x400, 0x200, 0x100, 0x080, 0x040, 0x020, 0x010, 0x009)
DAY = (0x803, *NIGHT[1:])
HAMMING_8_4 = (0x15, 0x02, 0x49, 0x5E, 0x64, 0x73, 0x38, 0x2F,
               0xD0, 0xC7, 0x8C, 0x9B, 0xA1, 0xB6, 0xFD, 0xEA)
MOT_ROWS = (1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 19, 20, 21)


def received_m29(reader_type, units):
    """Decode only actual magazine1/M29/0 using already authenticated APIs."""
    hamming = sys.modules["teletextimager.bits.hamming_8_4"]
    reader, contexts, display_headers = reader_type(), [], []
    for index, unit in enumerate(units):
        packet = bytearray(imager.resource.BIT_REVERSE[byte] for byte in unit[4:])
        lo, hi = (hamming.decode(byte) for byte in packet[:2])
        require(lo != 0xFF and hi != 0xFF, f"Unit{index}: invalid MRAG")
        magazine, row = (lo & 7) or 8, (lo >> 3) | (hi << 1)
        if row == 29:
            designation = hamming.decode(packet[2])
            require(magazine == 1 and designation == 0,
                    f"Unit{index}: unexpected magazine/M29 designation")
            words = reader.convert_18bit_packet(packet[2:])
            require(len(words) == 13
                    and all(type(word) is int and 0 <= word <= 0x3FFFF for word in words),
                    f"Unit{index}: uncorrectable actual M29/0 triplet")
            contexts.append((index, words))
        elif row == 0:
            low, high = (hamming.decode(byte) for byte in packet[2:4])
            require(low != 0xFF and high != 0xFF, f"Unit{index}: invalid header page")
            if (magazine << 8) | (high << 4) | low in DISPLAYS:
                display_headers.append(index)
    require(bool(contexts) and bool(display_headers), "Captured M29 and display headers required")
    first, words = contexts[0]
    require(all(current == words for _, current in contexts),
            "Contradictory actual M29 contexts are unsupported")
    require(all(first < index for index in display_headers),
            "Actual M29 must precede every dependent display header")
    return {(29, 0): words}, [index for index, _ in contexts]


def check_mot(page):
    """Fixed source declaration; encoded correspondence, not MOT resolution."""
    require(page["subcode"] & 15 == 0 and page["subcode"] >> 8 == 21,
            "MOT1FE: logical S1/last body row differs")
    require(set(page) == {"number", "subcode", "control", 0, (28, 0), *MOT_ROWS},
            "MOT1FE: incomplete or unexpected closed body")
    require(page[0] == b" " * 40 and page[(28, 0)] == [0xFFB6] + [0] * 12,
            "MOT1FE: header text/function6/coding3 differs")
    # EN300706 Figure20/Tables20-23; original drcs_companion::mot():
    # associations124/125 choose Normal link1, no objects or defaults; 1C0/S0.
    expected = {row: [0] * 40 for row in MOT_ROWS}
    expected[2][9] = expected[2][11] = 1
    expected[19] = expected[20] = [1, 15, 15, 0, 0, 0, 0, 0, 0, 0] * 4
    expected[21] = ([1, 15, 15, 0, 1, 12, 0, 0] + [1, 15, 15, 0] * 6
                    + [0, 0, 1, 0, 0, 0, 1, 0])
    for row in MOT_ROWS:
        require(list(page[row]) == [HAMMING_8_4[value] & 0x7F for value in expected[row]],
                f"MOT1FE/S0 row{row}: original encoded body differs")


def check_bank(page, theme):
    require(page["subcode"] & 15 == 0 and page["subcode"] >> 8 == 1,
            "Normal1C0: logical S1/last body row differs")
    require(set(page) == {"number", "subcode", "control", 0, (28, 3), 1}
            and page[0] == b" " * 40,
            "Normal1C0/S0: incomplete or unexpected closed body/header")
    # Table14: slot0 mode0; all other slots undefinedF, reserved bits zero.
    bits = sum(15 << (4 * slot) for slot in range(1, 48))
    modes = [5] + [(bits >> (18 * index)) & 0x3FFFF for index in range(12)]
    require(page[(28, 3)] == modes, "Normal1C0/S0: exact function/coding/modes differ")
    rows = DAY if theme == "day" else NIGHT
    expected_bytes = [byte for row in rows for byte in (0x40 | (row >> 6), 0x40 | (row & 63))]
    require(list(page[1]) == expected_bytes + [32] * 20,
            "Normal1C0/S0: literal PTU0 or undefined second half differs")
    decoder = sys.modules["teletextimager.teletextdrcs"].TeletextDRCSDecode()
    decoder.set_page([page])
    require(decoder.ptu_mode(0, 0) == 0
            and list(decoder.ptu_l2p5(0, 0)) == fixed.bitmap(rows),
            "Normal1C0/S0 slot0: literal120 decoded pixels differ")


def received_pages(pages, theme):
    banks, displays, mots = [], [], 0
    for page in pages:
        number = page.get("number")
        require(number in DISPLAYS | {0x1C0, 0x1FE}, "Unknown closed acquisition")
        if number == 0x1C0:
            check_bank(page, theme)
            banks.append(page)
        elif number == 0x1FE:
            check_mot(page)
            mots += 1
        else:
            require(page.get("subcode") == 32 and 1 in page and (26, 0) in page,
                    f"Display{number:X}: incomplete closed acquisition or wrong subcode")
            require((28, 0) not in page,
                    f"Display{number:X}: unexpected X28 would override original M29")
            displays.append(page)
    require(bool(banks) and mots > 0 and {page["number"] for page in displays} == DISPLAYS,
            "Both complete displays, actual singleton bank and original MOT required")
    return [banks[0]], displays, len(banks), mots


def check_display(decoder_type, page, banks, context, theme):
    rows = DAY if theme == "day" else NIGHT
    rgb = (255, 0, 0) if theme == "day" else (0, 0, 255)
    require(not ({5, 6, 12, 13, 14} & page["control"])
            and len(page[0]) == 40, "Original full-page header/NOS0 differs")
    for level in ("1", "1.5", "2.5"):
        decoder = decoder_type()
        decoder.decode(page, level=level, ndrcs_page=banks, m29_page=context)
        try:
            for column, code in ((8, 90), (9, 90), (10, 32)):
                if level == "2.5" and column == 8:
                    fixed.cell(decoder, 1, column, code, 0, (0, 0, rows), rgb)
                elif level != "2.5":
                    fixed.cell(decoder, 1, column, code, 0)
            if level == "2.5":
                require((decoder.left_side_panel, decoder.right_side_panel) == (4, 12),
                        "Actual M29 panel widths must be4/12 (56cells)")
                require(context[(29, 0)][1] & 0x3F8 == 0x138,
                        "Actual M29 panel flags/status/width differ")
                fixed.cell(decoder, 1, 40, 32, 0, (0, 0, rows), rgb)
                for column in (8, 40):
                    require(decoder.get_drcs_mode(1, column) == 0,
                            f"Cell1,{column}: expected monochrome mode0")
                    foreground = fixed.colour(decoder, decoder.get_foreground(1, column))
                    background = fixed.colour(decoder, decoder.get_background(1, column))
                    pixels = [foreground if bit else background
                              for bit in decoder.get_drcs_bitmap(1, column)]
                    require(pixels == [list(rgb) if bit else [0, 0, 0]
                                       for bit in fixed.bitmap(rows)],
                            f"Cell1,{column}: literal120 palette-mapped RGB pixels differ")
            else:
                require((decoder.left_side_panel, decoder.right_side_panel) == (0, 0),
                        "Level1/1.5 must not enable enhanced panels")
                fixed.cell(decoder, 1, 40, 32, 0)
            for row in range(1, 25):
                for column in range(72):
                    expected_code = 90 if row == 1 and column in (8, 9) else 32
                    require(decoder.get_char_code(row, column) == expected_code
                            and decoder.get_fragment(row, column).value == 0,
                            f"Cell{row},{column}: unchanged literal Base/fragment differs")
                    expected_drcs = level == "2.5" and (row, column) in {(1, 8), (1, 40)}
                    require((decoder.get_drcs_ptu(row, column) is not None) == expected_drcs,
                            f"Cell{row},{column}: missing/moved/additional DRCS invocation")
        except imager.resource.CalibrationError as error:
            raise imager.resource.CalibrationError(
                f"Display{page['number']:X}/S32 Level{level}: {error}") from error


def main():
    require(len(sys.argv) == 3 and sys.argv[2] in ("day", "night"),
            "Expected captured units file and fixed day/night profile")
    source = Path(sys.argv[1])
    require(source.is_file() and not source.is_symlink(), "Expected regular captured units file")
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
    context, context_indices = received_m29(reader_type, units)
    pages = fixed.closed_pages(reader_type, units)
    banks, displays, bank_count, mot_count = received_pages(pages, sys.argv[2])
    counts = {number: 0 for number in DISPLAYS}
    for page in displays:
        check_display(decoder_type, page, banks, context, sys.argv[2])
        counts[page["number"]] += 1
    print(json.dumps({
        "decoder": "teletextimager", "version": imager.VERSION, "commit": imager.COMMIT,
        "frozen_national_oracle_sha256": ORACLE_SHA,
        "frozen_loader_sha256": fixed.LOADER_SHA, "external_source_sha256": imager.HASHES,
        "input_sha256": hashlib.sha256(raw).hexdigest(), "input_byte_limit": imager.resource.MAX_INPUT,
        "unit_limit": 512, "units": len(units), "theme": sys.argv[2],
        "fixture": "retained-singleton-bank-main-and-right-panel",
        "only_closed_acquisitions_checked": True, "closed_displays_checked": len(displays),
        "closed_displays_by_page": {f"{number:X}": count for number, count in counts.items()},
        "complete_closed_normal_1c0_s0_banks_checked": bank_count,
        "complete_closed_original_mots_checked": mot_count,
        "encoded_mot_correspondence_only": True, "levels": ["1", "1.5", "2.5"],
        "supplied_received_bank": "Normal1C0/S0/slot0",
        "supplied_received_m29_magazine": 1, "supplied_received_m29_designation": 0,
        "actual_m29_unit_indices": context_indices, "contexts_identical_and_before_displays": True,
        "display_x28_present": False, "panel_widths": [4, 12], "visual_width": 56,
        "raw_cells": [[1, 8], [1, 40]], "visual_cells": [[1, 12], [1, 44]],
        "literal_bitmap_rows": list(DAY if sys.argv[2] == "day" else NIGHT),
        "literal_pixels_per_cell": 120, "foreground_rgb8": [255, 0, 0] if sys.argv[2] == "day" else [0, 0, 255],
        "background_rgb8": [0, 0, 0], "fallback_receiver_region": 0,
        "automatic_magazine_mot_x27_resource_acquisition_proven": False,
        "font_renderer_or_actual_client_proven": False, "full_engine_acceptance": False,
    }, sort_keys=True))


if __name__ == "__main__":
    try:
        main()
    except (imager.resource.CalibrationError, OSError, ValueError, AttributeError,
            RecursionError, KeyError, IndexError, TypeError) as error:
        message = (str(error) if isinstance(error, imager.resource.CalibrationError)
                   else "Independent fixed Main+RightPanel calibration could not run")
        print(json.dumps({"error": message}), file=sys.stderr)
        sys.exit(1)
