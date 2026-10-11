#!/usr/bin/env python3
"""Bounded public libzvbi 0.2.42 resource calibration for Linux x86-64.

This ignored native test helper consumes synthetic DVB units from stdin. It
loads the installed decoder, not a mocked implementation or bundled library.
The public page ABI is deliberately pinned to the native little-endian GCC
Linux x86-64 layout. It is not a cross-platform renderer or runtime dependency.
"""

import ctypes as C
import ctypes.util
import json
import platform
import re
import sys

MAX_INPUT = 2 * 1024 * 1024
MAX_CASES = 16
MAX_UNITS = 256
BIT_REVERSE = bytes(int(f"{value:08b}"[::-1], 2) for value in range(256))


class CalibrationError(Exception):
    """A bounded synthetic input or native decoder failed calibration."""


class Sliced(C.Structure):
    _fields_ = [("id", C.c_uint32), ("line", C.c_uint32), ("data", C.c_uint8 * 56)]


class Char(C.Structure):
    # Public vbi_char: flags/size/opacity/foreground, background/CLUT/Unicode.
    _fields_ = [("bits1", C.c_uint32), ("bits2", C.c_uint32)]


class Dirty(C.Structure):
    _fields_ = [("y0", C.c_int), ("y1", C.c_int), ("roll", C.c_int)]


class Link(C.Structure):
    _fields_ = [("pgno", C.c_int), ("subno", C.c_int)]


class Page(C.Structure):
    _fields_ = [
        ("vbi", C.c_void_p), ("nuid", C.c_uint), ("pgno", C.c_int),
        ("subno", C.c_int), ("rows", C.c_int), ("columns", C.c_int),
        ("text", Char * 1056), ("dirty", Dirty), ("screen_color", C.c_int),
        ("screen_opacity", C.c_int), ("color_map", C.c_uint32 * 40),
        ("drcs_clut", C.POINTER(C.c_uint8)),
        ("drcs", C.POINTER(C.c_uint8) * 32), ("nav_link", Link * 6),
        ("nav_index", C.c_char * 64), ("font", C.c_void_p * 2),
        ("double_height_lower", C.c_uint), ("page_opacity", C.c_int * 2),
        ("boxed_opacity", C.c_int * 2),
    ]


CALLBACK = C.CFUNCTYPE(None, C.c_void_p, C.c_void_p)


def require(condition, message):
    if not condition:
        raise CalibrationError(message)


def integer(value, minimum, maximum):
    return type(value) is int and minimum <= value <= maximum


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, "duplicate input member")
        result[key] = value
    return result


def validate_input(value):
    require(type(value) is dict and set(value) == {"expected_version", "cases"}, "invalid request members")
    require(value["expected_version"] == "0.2.42", "unsupported expected decoder version")
    cases = value["cases"]
    require(type(cases) is list and 1 <= len(cases) <= MAX_CASES, "invalid case count")
    names = set()
    fields = {"name", "units", "page", "subcode", "row", "column", "slot", "s1", "normal", "pixels", "expect_glyph"}
    for case in cases:
        require(type(case) is dict and set(case) == fields, "invalid case members")
        name = case["name"]
        require(type(name) is str and re.fullmatch(r"[A-Za-z0-9_-]{1,64}", name), "invalid synthetic case name")
        require(name not in names, "duplicate case name")
        names.add(name)
        require(integer(case["page"], 0x100, 0x899), "invalid display page")
        require((case["page"] & 15) <= 9 and ((case["page"] >> 4) & 15) <= 9, "non-decimal display page")
        require(integer(case["subcode"], 0, 0x3F7F) and case["subcode"] & 0xC080 == 0, "invalid display subcode")
        require(integer(case["row"], 1, 24) and integer(case["column"], 0, 39), "invalid display cell")
        require(integer(case["slot"], 0, 47) and integer(case["s1"], 0, 15), "invalid DRCS location")
        require(type(case["normal"]) is bool and type(case["expect_glyph"]) is bool, "invalid case flags")
        pixels = case["pixels"]
        require(type(pixels) is list and len(pixels) == 10 and all(integer(row, 0, 4095) for row in pixels), "invalid bitmap geometry")
        units = case["units"]
        require(type(units) is list and 1 <= len(units) <= MAX_UNITS, "invalid data-unit count")
        for unit in units:
            require(type(unit) is list and len(unit) == 46 and all(integer(byte, 0, 255) for byte in unit), "invalid data-unit bytes")
            require(unit[0] == 2 and unit[1] == 44 and unit[3] == 0xE4, "invalid teletext data-unit framing")
    return cases


def load_decoder():
    require(sys.platform == "linux" and platform.machine() == "x86_64" and sys.byteorder == "little", "unsupported native calibration ABI")
    require(C.sizeof(Sliced) == 64 and C.sizeof(Char) == 8 and C.sizeof(Page) == 9072, "unexpected native structure size")
    require(Page.text.offset == 28 and Page.drcs.offset == 8664, "unexpected native page offsets")
    library = ctypes.util.find_library("zvbi")
    require(library is not None, "installed decoder is unavailable")
    lib = C.CDLL(library)
    signatures = {
        "vbi_version": ([C.POINTER(C.c_uint)] * 3, None),
        "vbi_decoder_new": ([], C.c_void_p),
        "vbi_decoder_delete": ([C.c_void_p], None),
        "vbi_event_handler_register": ([C.c_void_p, C.c_int, CALLBACK, C.c_void_p], C.c_int),
        "vbi_teletext_set_level": ([C.c_void_p, C.c_int], None),
        "vbi_decode": ([C.c_void_p, C.POINTER(Sliced), C.c_int, C.c_double], None),
        "vbi_fetch_vt_page": ([C.c_void_p, C.POINTER(Page)] + [C.c_int] * 5, C.c_int),
        "vbi_unref_page": ([C.POINTER(Page)], None),
        "vbi_draw_vt_page_region": ([C.POINTER(Page), C.c_int, C.c_void_p] + [C.c_int] * 7, None),
    }
    for name, (arguments, result) in signatures.items():
        function = getattr(lib, name)
        function.argtypes, function.restype = arguments, result
    version = [C.c_uint() for _ in range(3)]
    lib.vbi_version(*(C.byref(part) for part in version))
    actual = ".".join(str(part.value) for part in version)
    require(actual == "0.2.42", "installed decoder version does not match calibration pin")
    return lib, actual


def decode_case(lib, case):
    decoder = lib.vbi_decoder_new()
    require(bool(decoder), "decoder allocation failed")
    callback = CALLBACK(lambda event, user: None)  # Retain until decoder deletion.
    page = Page()
    fetched = False
    try:
        require(lib.vbi_event_handler_register(decoder, 2, callback, None), "teletext event registration failed")
        lib.vbi_teletext_set_level(decoder, 2)  # Public VBI_WST_LEVEL_2p5.
        for index, unit in enumerate(case["units"]):
            sliced = Sliced()
            sliced.id = 3  # Public VBI_SLICED_TELETEXT_B.
            sliced.line = unit[2] & 31
            sliced.data[:42] = bytes(BIT_REVERSE[byte] for byte in unit[4:])
            # Public decode timestamps must advance by a video-frame interval.
            lib.vbi_decode(decoder, C.byref(sliced), 1, 1.0 + index * 0.04)
        fetched = bool(lib.vbi_fetch_vt_page(decoder, C.byref(page), case["page"], case["subcode"], 2, 25, 0))
        require(fetched, "display page was not fetched")
        require(page.pgno == case["page"] and page.subno == case["subcode"] and page.columns == 41 and page.rows == 25, "unexpected fetched page geometry")
        require(page.text[page.columns * 2 + 1].bits2 >> 16 == ord("B"), "base calibration character is missing")
        cell = page.text[page.columns * case["row"] + case["column"]]
        unicode = cell.bits2 >> 16
        if not case["expect_glyph"]:
            require(unicode == ord("T"), "invalid DRCS did not preserve the base calibration character")
            return
        table_index = (16 if case["normal"] else 0) + case["s1"]
        expected_unicode = 0xF000 + (table_index << 6) + case["slot"]
        require(unicode == expected_unicode and bool(page.drcs[table_index]), "referenced DRCS glyph was not decoded")
        address = C.cast(page.drcs[table_index], C.c_void_p).value + case["slot"] * 60
        packed = C.string_at(address, 60)
        pixels = [(packed[index // 2] >> (4 * (index % 2))) & 15 for index in range(120)]
        expected = [(row >> (11 - column)) & 1 for row in case["pixels"] for column in range(12)]
        require(pixels == expected, "decoded DRCS pixels differ from authored geometry")
        canvas = (C.c_uint8 * 120)()
        lib.vbi_draw_vt_page_region(C.byref(page), 6, canvas, 12, case["column"], case["row"], 1, 1, 1, 1)  # Public PAL8.
        foreground, background = cell.bits1 >> 24, cell.bits2 & 255
        require(foreground != background, "calibration foreground and background are identical")
        require(list(canvas) == [foreground if pixel else background for pixel in expected], "rendered DRCS pixels differ from authored geometry")
    except CalibrationError as error:
        raise CalibrationError(f"{case['name']}: {error}") from error
    finally:
        if fetched:
            lib.vbi_unref_page(C.byref(page))
        lib.vbi_decoder_delete(decoder)


def main():
    raw = sys.stdin.buffer.read(MAX_INPUT + 1)
    require(0 < len(raw) <= MAX_INPUT, "invalid bounded input size")
    cases = validate_input(json.loads(raw, object_pairs_hook=unique_object))
    lib, version = load_decoder()
    for case in cases:
        decode_case(lib, case)
    print(json.dumps({"decoder_version": version, "cases_passed": len(cases)}, sort_keys=True))


if __name__ == "__main__":
    try:
        main()
    except (CalibrationError, OSError, ValueError, AttributeError):
        # Synthetic identifiers are bounded above; no library paths or logs escape.
        error = sys.exc_info()[1]
        if isinstance(error, CalibrationError):
            print(json.dumps({"error": str(error)}), file=sys.stderr)
        else:
            print(json.dumps({"error": "native calibration could not run"}), file=sys.stderr)
        sys.exit(1)
