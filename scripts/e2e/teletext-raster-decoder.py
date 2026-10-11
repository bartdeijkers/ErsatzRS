#!/usr/bin/env python3
"""Independent opaque DRCS raster calibration with installed libzvbi 0.2.42.

This opt-in Linux x86-64 diagnostic imports the frozen public ABI helper. It
compares production DVB units and RGB4 rectangles with a literal authored
bitmap, including enlargement. It does not compare borrowed fonts, certify
transparency/video, or exercise flash timing.
"""
import ctypes as C
import importlib.util
import json
from pathlib import Path
import re
import sys

sys.dont_write_bytecode = True
_SPEC = importlib.util.spec_from_file_location(
    "teletext_resource_calibration", Path(__file__).with_name("teletext-resource-decoder.py")
)
if _SPEC is None or _SPEC.loader is None:
    raise RuntimeError("fixed calibration helper is unavailable")
BASE = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(BASE)
SIZES = {"normal": (1, 1, 0), "double_width": (2, 1, 1),
         "double_height": (1, 2, 2), "double_size": (2, 2, 3)}
CASE_FIELDS = {"name", "units", "page", "subcode", "row", "column", "slot", "s1",
               "normal", "pixels", "size", "foreground", "background", "invert",
               "conceal", "reveal", "raster_rgb4"}


def rgb4(value):
    return (type(value) is list and len(value) == 3
            and all(BASE.integer(component, 0, 15) for component in value))


def literal_raster(case):
    scale_x, scale_y, _ = SIZES[case["size"]]
    foreground, background = case["foreground"], case["background"]
    if case["invert"]:
        foreground, background = background, foreground
    visible = not case["conceal"] or case["reveal"]
    return [foreground if visible and ((case["pixels"][y // scale_y]
            >> (11 - x // scale_x)) & 1) else background
            for y in range(10 * scale_y) for x in range(12 * scale_x)]


def validate_input(value):
    require = BASE.require
    require(type(value) is dict and set(value) == {"expected_version", "cases"},
            "invalid request members")
    require(value["expected_version"] == "0.2.42", "unsupported expected decoder version")
    cases = value["cases"]
    require(type(cases) is list and 1 <= len(cases) <= BASE.MAX_CASES, "invalid case count")
    names = set()
    for case in cases:
        require(type(case) is dict and set(case) == CASE_FIELDS, "invalid case members")
        name = case["name"]
        require(type(name) is str and re.fullmatch(r"[A-Za-z0-9_-]{1,64}", name),
                "invalid synthetic case name")
        require(name not in names, "duplicate case name")
        names.add(name)
        require(type(case["size"]) is str and case["size"] in SIZES, "invalid size")
        require(rgb4(case["foreground"]) and rgb4(case["background"]), "invalid RGB4 colours")
        require(case["foreground"] != case["background"], "indistinguishable calibration colours")
        require(all(type(case[key]) is bool for key in ("invert", "conceal", "reveal")),
                "invalid observation flags")
        # Reuse unchanged strict wire/address/DRCS validation before native load.
        legacy = {key: case[key] for key in ("name", "units", "page", "subcode", "row",
                  "column", "slot", "s1", "normal", "pixels")}
        legacy["expect_glyph"] = True
        BASE.validate_input({"expected_version": "0.2.42", "cases": [legacy]})
        scale_x, scale_y, _ = SIZES[case["size"]]
        require(case["column"] + scale_x <= 40 and case["row"] + scale_y <= 25,
                "enlargement rectangle exceeds display")
        require(any(case["pixels"]) and any(row != 4095 for row in case["pixels"]),
                "vacuous calibration bitmap")
        raster = case["raster_rgb4"]
        require(type(raster) is list and len(raster) == 120 * scale_x * scale_y
                and all(rgb4(pixel) for pixel in raster), "invalid bounded raster geometry")
        require(raster == literal_raster(case), "production raster differs from independent bitmap oracle")
    return cases


def check_page(lib, page, case):
    require = BASE.require
    row, column = case["row"], case["column"]
    require(page.pgno == case["page"] and page.subno == case["subcode"]
            and page.columns == 41 and page.rows == 25, "unexpected fetched page geometry")
    require(page.text[page.columns * 2 + 1].bits2 >> 16 == ord("B"),
            "base calibration character is missing")
    cell = page.text[page.columns * row + column]
    table_index = (16 if case["normal"] else 0) + case["s1"]
    require(cell.bits2 >> 16 == 0xF000 + (table_index << 6) + case["slot"]
            and bool(page.drcs[table_index]), "referenced DRCS glyph was not decoded")
    scale_x, scale_y, expected_size = SIZES[case["size"]]
    require((cell.bits1 >> 8) & 255 == expected_size, "decoded size differs from authored size")
    # Every selected cell must have the expected continuation. Otherwise a
    # second independent DW glyph at the region's right edge could overrun the
    # canvas even though the requested rectangle itself fits the page.
    for dy in range(scale_y):
        for dx in range(scale_x):
            expected_part = (expected_size if dy == 0 and dx == 0 else
                             4 if dy == 0 else
                             5 if dx == 1 else
                             7 if scale_x == 2 else 6)
            part = page.text[page.columns * (row + dy) + column + dx]
            require((part.bits1 >> 8) & 255 == expected_part,
                    "decoded enlargement continuation differs from bounded region")
    require(bool(cell.bits1 & 16) == case["conceal"] and cell.bits1 & 8 == 0,
            "decoded conceal or steady flash differs from authored state")
    require((cell.bits1 >> 16) & 255 == 3, "calibration requires an opaque origin")
    address = C.cast(page.drcs[table_index], C.c_void_p).value + case["slot"] * 60
    packed = C.string_at(address, 60)
    actual_bits = [(packed[index // 2] >> (4 * (index % 2))) & 15 for index in range(120)]
    expected_bits = [(bits >> (11 - x)) & 1 for bits in case["pixels"] for x in range(12)]
    require(actual_bits == expected_bits, "decoded DRCS pixels differ from authored geometry")
    foreground, background = case["foreground"], case["background"]
    if case["invert"]:
        foreground, background = background, foreground
    for colour_index, expected in ((cell.bits1 >> 24, foreground), (cell.bits2 & 255, background)):
        require(colour_index < 40, "decoded colour index exceeds palette")
        rgba = int(page.color_map[colour_index]).to_bytes(4, "little")
        require(list(rgba) == [component * 17 for component in expected] + [255],
                "decoded display colours differ from literal RGB4")
    # Draw complete enlargement: a DW glyph can exceed a one-cell canvas.
    width, height = 12 * scale_x, 10 * scale_y
    byte_count, guard = width * height * 4, 16
    canvas = (C.c_uint8 * (byte_count + 2 * guard))(*([0xA5] * (byte_count + 2 * guard)))
    lib.vbi_draw_vt_page_region(C.byref(page), 32, C.byref(canvas, guard), width * 4,
                               column, row, scale_x, scale_y, int(case["reveal"]), 1)
    require(list(canvas[:guard]) == [0xA5] * guard
            and list(canvas[-guard:]) == [0xA5] * guard, "native renderer exceeded bounded canvas")
    expected = bytes(component for pixel in case["raster_rgb4"]
                     for component in [*(value * 17 for value in pixel), 255])
    require(bytes(canvas[guard:guard + byte_count]) == expected,
            "independent rendered RGBA differs from production raster and literal bitmap")


def decode_case(lib, case):
    decoder = lib.vbi_decoder_new()
    BASE.require(bool(decoder), "decoder allocation failed")
    callback = BASE.CALLBACK(lambda event, user: None)
    page = BASE.Page()
    fetched = False
    try:
        BASE.require(lib.vbi_event_handler_register(decoder, 2, callback, None),
                     "teletext event registration failed")
        lib.vbi_teletext_set_level(decoder, 2)
        for index, unit in enumerate(case["units"]):
            sliced = BASE.Sliced()
            sliced.id, sliced.line = 3, unit[2] & 31
            sliced.data[:42] = bytes(BASE.BIT_REVERSE[byte] for byte in unit[4:])
            lib.vbi_decode(decoder, C.byref(sliced), 1, 1.0 + index * 0.04)
        fetched = bool(lib.vbi_fetch_vt_page(decoder, C.byref(page), case["page"],
                                           case["subcode"], 2, 25, 0))
        BASE.require(fetched, "display page was not fetched")
        check_page(lib, page, case)
    except BASE.CalibrationError as error:
        raise BASE.CalibrationError(f"{case['name']}: {error}") from error
    finally:
        if fetched:
            lib.vbi_unref_page(C.byref(page))
        lib.vbi_decoder_delete(decoder)


def main():
    raw = sys.stdin.buffer.read(BASE.MAX_INPUT + 1)
    BASE.require(0 < len(raw) <= BASE.MAX_INPUT, "invalid bounded input size")
    cases = validate_input(json.loads(raw, object_pairs_hook=BASE.unique_object))
    lib, version = BASE.load_decoder()
    for case in cases:
        decode_case(lib, case)
    print(json.dumps({"decoder_version": version, "cases_passed": len(cases)}, sort_keys=True))


if __name__ == "__main__":
    try:
        main()
    except (BASE.CalibrationError, OSError, ValueError, AttributeError, RecursionError):
        error = sys.exc_info()[1]
        message = str(error) if isinstance(error, BASE.CalibrationError) else "native raster calibration could not run"
        print(json.dumps({"error": message}), file=sys.stderr)
        sys.exit(1)
