#!/usr/bin/env python3
"""Fixed Polish Base5B carrier oracle for actual two-channel ordinary captures.

Reuse the unchanged eight-source-pinned teletextimager loader. Unlike the main
composition oracle, this vector requires DRCS. Received resource pages are
explicitly supplied to the decoder; this does not prove automatic X27/MOT cache
routing, rendered font pixels, headed client navigation or full conformance.
Level1/1.5 checks use receiver default region0, not Polish-font client evidence.
The explicit --compiler-owned vector expects generated Normal1A0/S1=0slot0 on
Day and no resource bank on Night; the original authored-bank vector is unchanged.
With --compiler-owned --underlined, Day uses slot1 and the fixed full bottom
underline row. Its physical DRCS D5 remains false; Night stays unchanged.
The separate --compiler-owned --turkish vector requires the Table36 TL
identity's literal Base23, fixed slot2 and mask; Polish vectors stay unchanged.
"""
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import sys

sys.dont_write_bytecode = True
LOADER_SHA = "45bd284daf621915d3e718ad536d2b4ea69a55a38c2c923712fd2cf4e79c225c"
POLISH = (0, 0x3FC, 0x18, 0x30, 0x1F8, 0xC0, 0x180, 0x3FC, 0, 0)
POLISH_UNDERLINED = (0, 0x3FC, 0x18, 0x30, 0x1F8, 0xC0, 0x180, 0x3FC, 0, 0xFFF)
TURKISH = (0, 0, 0x3F0, 0xC0, 0xD8, 0xD8, 0xD8, 0xDE, 0, 0)
MIRRORED = (0x801, 0x001, 0x002, 0x004, 0x008, 0x010, 0x020, 0x040, 0x080, 0x900)
path = Path(__file__).with_name("teletext-imager-decoder.py")
if hashlib.sha256(path.read_bytes()).hexdigest() != LOADER_SHA:
    raise RuntimeError("Frozen imager loader changed")
spec = importlib.util.spec_from_file_location("fixed_polish_imager", path)
imager = importlib.util.module_from_spec(spec)
spec.loader.exec_module(imager)
require = imager.require


def closed_pages(reader_type, units):
    # Strip DVB framing and undo on-wire bit order only. Preserve every packet
    # and its order; do not synthesize a page-closing header for an EOF tail.
    packets = bytes(imager.resource.BIT_REVERSE[b] for unit in units for b in unit[4:])
    stream, reader, pages = io.BytesIO(packets), reader_type(), []
    for _ in range(512):
        current = reader.read(stream)
        if current is None:
            return pages
        require(type(current) is list and len(current) == 1, "Unexpected acquisition count")
        if stream.tell() == len(packets):
            return pages
        if current[0]:
            pages.append(current[0])
    raise imager.resource.CalibrationError("Acquisition bound exceeded")


def bitmap(rows):
    return [int(bool(row & (1 << (11 - x)))) for row in rows for x in range(12)]


def colour(decoder, index):
    palette = decoder.get_palette()
    return palette[index * 3:index * 3 + 3]


def cell(decoder, row, column, code, fragment, drcs=None, foreground=(255, 255, 255)):
    where = f"cell {row},{column}"
    require(decoder.get_char_code(row, column) == code, f"{where}: original code differs")
    require(decoder.get_fragment(row, column).value == fragment, f"{where}: fragment differs")
    require(colour(decoder, decoder.get_foreground(row, column)) == list(foreground)
            and colour(decoder, decoder.get_background(row, column)) == [0, 0, 0],
            f"{where}: palette colours differ")
    require(not decoder.get_conceal(row, column) and not decoder.get_invert(row, column)
            and not decoder.get_und_sep(row, column) and decoder.get_flash_mode(row, column) == 0,
            f"{where}: neutral effects differ")
    if drcs is None:
        require(decoder.get_drcs_ptu(row, column) is None, f"{where}: unexpected DRCS")
        return
    subtable, slot, rows = drcs
    require(decoder.get_drcs_type(row, column) == decoder.DRCSType.DRCS_NORMAL
            and decoder.get_drcs_subtable(row, column) == subtable
            and decoder.get_drcs_ptu(row, column) == slot,
            f"{where}: normal DRCS reference differs")
    actual = decoder.get_drcs_bitmap(row, column)
    require(actual is not None and list(actual) == bitmap(rows), f"{where}: literal DRCS mask differs")


def check_display(decoder_type, page, banks, theme, compiler_owned=False, underlined=False,
                  turkish=False):
    number = page["number"]
    for level in ("1", "1.5", "2.5"):
        decoder = decoder_type()
        decoder.decode(page, level=level, ndrcs_page=banks)
        if number == 0x124:
            code = (0x23 if turkish else 0x5B) if theme == "day" else 65
            if level == "2.5":
                slot = (1 if underlined else 0) if compiler_owned else 5
                rows = POLISH_UNDERLINED if underlined else POLISH
                if turkish:
                    slot, rows = 2, TURKISH
                glyph = (0, slot, rows) if theme == "day" else None
                cell(decoder, 1, 7, code, 3, glyph)
                cell(decoder, 1, 8, code, 4, glyph)
            else:
                cell(decoder, 1, 7, code, 0)
                cell(decoder, 1, 8, 32, 0)
            cell(decoder, 3, 1, 66, 0)
        elif level == "2.5":
            foreground = (255, 0, 0) if theme == "day" else (0, 0, 255)
            for column, code in ((8, 90), (10, 32)):
                if compiler_owned:
                    cell(decoder, 1, column, 65, 0, None, foreground)
                else:
                    cell(decoder, 1, column, code, 0, (1, 0, MIRRORED), foreground)
        else:
            for column, code in ((8, 90), (9, 90), (10, 32)):
                cell(decoder, 1, column, code, 0)


def received_banks(pages, theme, compiler_owned, turkish=False):
    banks = {}
    owner = 0x1A0 if compiler_owned else 0x1C0
    # Slot2's actual PTU starts on row2; the compiler need not send an empty
    # row1. Existing Polish vectors still require their original PTU row1.
    required_row = 2 if turkish else 1
    for page in pages:
        if page.get("number") == owner and (28, 3) in page and required_row in page:
            banks.setdefault(page["subcode"] & 15, page)
    if compiler_owned:
        expected = {0} if theme == "day" else set()
        require(set(banks) == expected, "Expected fixed compiler-owned bank inventory")
        require(all(page.get("number") in (0x124, 0x125, owner) for page in pages),
                "Unexpected page in fixed compiler-owned capture")
        return [banks[0]] if theme == "day" else []
    require(set(banks) == {0, 1}, "Expected both actual normal DRCS banks")
    return [banks[0], banks[1]]


def main():
    require(len(sys.argv) in (3, 4, 5) and sys.argv[2] in ("day", "night")
            and (len(sys.argv) == 3 or sys.argv[3] == "--compiler-owned")
            and (len(sys.argv) < 5 or sys.argv[4] in ("--underlined", "--turkish")),
            "Expected units file, fixed theme and optional --compiler-owned [--underlined|--turkish]")
    compiler_owned = len(sys.argv) >= 4
    underlined = len(sys.argv) == 5 and sys.argv[4] == "--underlined"
    turkish = len(sys.argv) == 5 and sys.argv[4] == "--turkish"
    source = Path(sys.argv[1])
    require(source.is_file() and not source.is_symlink(), "Expected regular captured units file")
    with source.open("rb") as stream:
        raw = stream.read(imager.resource.MAX_INPUT + 1)
    require(0 < len(raw) <= imager.resource.MAX_INPUT, "Input byte bound exceeded")
    units = json.loads(raw, object_pairs_hook=imager.resource.unique_object)
    require(type(units) is list and 1 <= len(units) <= 512, "Invalid unit count")
    for unit in units:
        require(type(unit) is list and len(unit) == 46 and unit[:2] == [2, 44]
                and unit[3] == 0xE4 and all(type(b) is int and 0 <= b <= 255 for b in unit),
                "Invalid actual DVB unit")
    reader_type, decoder_type = imager.load_decoder()
    pages = closed_pages(reader_type, units)
    banks = received_banks(pages, sys.argv[2], compiler_owned, turkish)
    displays = [page for page in pages if page.get("number") in (0x124, 0x125)
                and page.get("subcode") == 32 and 1 in page and (26, 0) in page]
    require({p["number"] for p in displays} == {0x124, 0x125}, "Both complete displays are required")
    for page in displays:
        check_display(decoder_type, page, banks, sys.argv[2], compiler_owned, underlined, turkish)
    report = {"decoder": "teletextimager", "version": imager.VERSION,
                      "commit": imager.COMMIT, "input_sha256": hashlib.sha256(raw).hexdigest(),
                      "theme": sys.argv[2], "units": len(units), "closed_displays_checked": len(displays),
                      "levels": ["1", "1.5", "2.5"], "literal_drcs_and_fragments": True,
                      "automatic_resource_routing_proven": False, "font_pixels_or_actual_client_proven": False,
                      "fallback_receiver_region": 0}
    if compiler_owned:
        report.update({"fixture": "compiler-owned-underline" if underlined else "compiler-owned",
                       "normal_resource_page": "1A0",
                       "subtable": 0, "polish_slot": 1 if underlined else 0,
                       "resource_present": sys.argv[2] == "day"})
        if turkish:
            del report["polish_slot"]
            report.update({"fixture": "compiler-owned-turkish", "turkish_slot": 2})
    print(json.dumps(report, sort_keys=True))


if __name__ == "__main__":
    try:
        main()
    except (imager.resource.CalibrationError, OSError, ValueError, AttributeError,
            RecursionError, KeyError, IndexError, TypeError) as error:
        message = str(error) if isinstance(error, imager.resource.CalibrationError) else "Independent Polish calibration could not run"
        print(json.dumps({"error": message}), file=sys.stderr)
        sys.exit(1)
