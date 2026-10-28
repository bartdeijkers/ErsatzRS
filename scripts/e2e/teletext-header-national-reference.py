#!/usr/bin/env python3
"""Fixed AB5 header reference, not a patched decoder or general renderer.

EN 300 706 V1.2.1 Table 25 restricts national substitution to X/1..X/25;
Table 35 therefore leaves Latin X/0 code 2/3 as NUMBER SIGN. Decode the
original acquisition with the existing pinned, unmodified external reader,
then apply that rule only to its two original cells. The external decoder's
incorrect header/font selection is never called, patched or normalized.
This is separate normative encoded-unit/reference-font proof, not a live HTTP
capture or client rendering. Keep the factory's original filling header.
"""
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import sys

sys.dont_write_bytecode = True
SPEC = importlib.util.spec_from_file_location(
    "header_reference_imager", Path(__file__).with_name("teletext-imager-decoder.py")
)
IMAGER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(IMAGER)
require = IMAGER.resource.require
MAX_INPUT = 128 * 1024
POINTS = [{"column": 8, "row": 0, "unicode": 35},
          {"column": 8, "row": 1, "unicode": 163}]
# Original literal masks in AB5 independent-final.json. Neither is supplied by
# the request or read from the product's font atlas/compositor.
HASH_MASK = (0, 408, 408, 2046, 408, 408, 2046, 408, 408, 0)
POUND_MASK = (0, 252, 390, 192, 1008, 192, 998, 828, 0, 0)
FONT_SHA = {
    "G0_latin.pbm": "acf6fcd224b15d054a7616dd60158df7615f0bba01480aa4453d08877c18569e",
    "G0_NOS_en.pbm": "415c808e58277f9ef112d2f0e3cc65b8f525d02b28747a93d1453f6b2d32196f",
    "G0_latin.pil": "ec1af672eb4992baa2294af1991cbbe23f09f3d83324662e4920a6a493053e46",
    "G0_NOS_en.pil": "ec1af672eb4992baa2294af1991cbbe23f09f3d83324662e4920a6a493053e46",
}


def validate(value):
    require(type(value) is dict and set(value) == {"name", "page", "subcode", "points", "units"},
            "only the original AB5 header fixture is supported")
    require(value["name"] == "header" and type(value["page"]) is int
            and value["page"] == 0x200 and type(value["subcode"]) is int
            and value["subcode"] == 0 and value["points"] == POINTS,
            "original identity and normative points required")
    require(all(type(p) is dict and all(type(v) is int for v in p.values())
                for p in value["points"]), "numeric point aliases are forbidden")
    units = value["units"]
    require(type(units) is list and len(units) == 27, "original 27-unit acquisition required")
    for unit in units:
        require(type(unit) is list and len(unit) == 46
                and all(type(b) is int and 0 <= b <= 255 for b in unit), "invalid unit bytes")
        require(unit[:4] == [2, 44, 0xE0, 0xE4], "original DVB framing required")
    return units


def wire_rows(units, hamming):
    rows = []
    for unit in units:
        packet = bytes(IMAGER.resource.BIT_REVERSE[b] for b in unit[4:])
        lo, hi = hamming(packet[0]), hamming(packet[1])
        require(lo != 0xFF and hi != 0xFF, "invalid packet address")
        require(lo & 7 == 2, "original magazine required")
        row = (lo >> 3) | (hi << 1)
        rows.append(row)
        if row == 0:
            fields = [hamming(b) for b in packet[2:10]]
            expected = [0, 0, 0, 8, 0, 0, 0, 1] if len(rows) == 1 else [15, 15, 0, 0, 0, 0, 1, 1]
            require(fields == expected, "original page/subcode/control/filling header differs")
        if row == 0 or 1 <= row <= 24:
            # X/0's first eight data bytes use Hamming, not odd parity.
            start = 10 if row == 0 else 2
            require(all(b.bit_count() % 2 == 1 for b in packet[start:]),
                    "invalid character odd parity")
    require(rows == [0, 28, *range(1, 25), 0], "original chronological packet inventory required")
    return rows


def reference_cells(page):
    require(page.get("number") == 0x200 and page.get("subcode") == 0,
            "independent acquisition identity differs")
    require(page.get("control") == {4, 11}, "original English national/header controls differ")
    require(set(page) == {"number", "subcode", "control", *range(25)}
            or set(page) == {"number", "subcode", "control", (28, 0), *range(25)},
            "original complete base acquisition required")
    if (28, 0) in page:
        words = page[(28, 0)]
        require(len(words) == 13 and all(type(w) is int for w in words), "invalid X28 triplets")
        require(words[0] & 0x3FFF == 0, "original function/coding/primary Latin designation differs")
    require(page[0] == b" " * 8 + b"#" + b" " * 31,
            "original raw header characters differ")
    require(page[1] == b" " * 8 + b"#" + b" " * 31,
            "original English body characters differ")
    # Shared semantics at Levels 1/1.5: this acquisition has no X26 replacing
    # either cell. Keep the English body counterexample; never flatten all NOS.
    return [(0, 8, page[0][8], "G0_latin", 35, HASH_MASK),
            (1, 8, page[1][8], "G0_NOS_en", 163, POUND_MASK)]


def font_mask(directory, name, code):
    from PIL import Image, ImageDraw, ImageFont

    for suffix in ("pil", "pbm"):
        source = directory / f"{name}.{suffix}"
        with source.open("rb") as file:
            data = file.read(64 * 1024 + 1)
        require(len(data) <= 64 * 1024
                and hashlib.sha256(data).hexdigest() == FONT_SHA[source.name],
                "pinned external font changed")
    image = Image.new("1", (12, 20))
    ImageDraw.Draw(image).text((0, 0), chr(code), fill=1,
                              font=ImageFont.load(directory / f"{name}.pil"))
    pixels = list(image.getdata())
    rows = [sum(1 << (11 - x) for x in range(12) if pixels[y * 12 + x]) for y in range(20)]
    require(all(rows[y] == rows[y + 1] for y in range(0, 20, 2)), "external glyph geometry differs")
    return tuple(rows[::2])


def qualify(value):
    units = validate(value)
    reader_type, _ = IMAGER.load_decoder()
    from teletextimager.bits import hamming_8_4

    rows = wire_rows(units, hamming_8_4.decode)
    fonts = Path(os.environ["ERSATZRS_TELETEXT_TEST_IMAGER_DIR"]) / "teletextimager/font-etsi"
    records = []
    for extension in (True, False):
        selected = [u for u, row in zip(units, rows) if extension or row != 28]
        raw = bytes(IMAGER.resource.BIT_REVERSE[b] for unit in selected for b in unit[4:])
        pages = reader_type().read(io.BytesIO(raw))
        require(type(pages) is list and len(pages) == 1, "unexpected independent acquisition count")
        require(((28, 0) in pages[0]) == extension, "X28 presence differs")
        cells = reference_cells(pages[0])
        for row, column, code, font, scalar, expected in cells:
            observed = font_mask(fonts, font, code)
            require(observed == expected, "original independent glyph mask differs")
            for level in ("1", "1.5"):
                records.append({"level": level, "x28": extension, "row": row, "column": column,
                                "code": code, "font": font, "unicode": scalar,
                                "mask": list(observed), "passed": True})
    return {"criterion": "AB5 original four base header observations and body counterexample",
            "method": "Unmodified pinned external acquisition reader plus separate EN300706 Table25/35 reference selection and pinned external PIL fonts",
            "decoder_commit": IMAGER.COMMIT, "reference_version": 1,
            "header_observations": 4, "body_counterexamples": 4, "records": records,
            "limit": "Normative encoded-unit/reference-font proof only; original factory filling header retained, no live HTTP capture, external decoder repair, full renderer/client equivalence, broadcast transport or enhanced-level acceptance"}


def main():
    raw = sys.stdin.buffer.read(MAX_INPUT + 1)
    require(0 < len(raw) <= MAX_INPUT, "invalid bounded input size")
    result = qualify(json.loads(raw, object_pairs_hook=IMAGER.resource.unique_object))
    print(json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    try:
        main()
    except (IMAGER.resource.CalibrationError, OSError, ValueError, KeyError, TypeError,
            AttributeError, IndexError, ImportError) as error:
        message = str(error) if isinstance(error, IMAGER.resource.CalibrationError) else "header reference could not run"
        print(json.dumps({"error": message}), file=sys.stderr)
        sys.exit(1)
