#!/usr/bin/env python3
"""Independent pixels for the fixed synthetic QTM DRCS flow, Linux x64 only.

Reads ordered units captured from the ordinary stream, not native semantic
objects. Reuses the hash-pinned libzvbi ABI helper. Literal bitmap/palette
expectations cover MAIN cells only; 56-column panels remain unproven.
The fixed indexed-edited mode checks both consumers: only day page124 S1=0
changes 0x801 to 0x803; page125 S1=1 and the night channel stay unchanged.
"""
import ctypes as C
import hashlib
import json
from pathlib import Path
import sys
import types

sys.dont_write_bytecode = True
HELPER_SHA = "615bc0ceb7b24738e7cb7e8a6df076c8b58eb9968ecdc7e07d226f76f5d7a76d"
PATTERNS = (
    (0x801, 0x800, 0x400, 0x200, 0x100, 0x080, 0x040, 0x020, 0x010, 0x009),
    (0x801, 0x001, 0x002, 0x004, 0x008, 0x010, 0x020, 0x040, 0x080, 0x900),
)
path = Path(__file__).with_name("teletext-resource-decoder.py")
body = path.read_bytes()
if len(body) > 65536 or hashlib.sha256(body).hexdigest() != HELPER_SHA:
    raise RuntimeError("Frozen DRCS ABI helper changed")
base = types.ModuleType("fixed_editor_drcs_abi")
base.__file__ = str(path)
exec(compile(body, str(path), "exec"), base.__dict__)
require = base.require


def check_page(lib, decoder, number, theme, subpage, edited=False, columns=(8,)):
    page, fetched = base.Page(), False
    try:
        fetched = bool(lib.vbi_fetch_vt_page(decoder, C.byref(page), number, 0x20, 2, 25, 0))
        require(fetched and page.pgno == number and page.subno == 0x20
                and page.rows == 25 and page.columns == 41, "Unexpected MAIN page geometry")
        bank, unicode = 16 + subpage, 0xF400 + subpage * 64
        require(bool(page.drcs[bank]), "Normal indexed DRCS/slot0 missing")
        packed = C.string_at(C.cast(page.drcs[bank], C.c_void_p).value, 60)
        pattern = (0x803, *PATTERNS[0][1:]) if edited else PATTERNS[subpage]
        expected = [(row >> (11 - x)) & 1 for row in pattern for x in range(12)]
        require([(packed[x // 2] >> (4 * (x % 2))) & 15 for x in range(120)] == expected,
                "Independent decoded DRCS bitmap differs from literal")
        for column in columns:
            cell = page.text[page.columns + column]
            require(cell.bits2 >> 16 == unicode, "Normal indexed DRCS/slot0 missing")
            foreground, background = cell.bits1 >> 24, cell.bits2 & 255
            rgba = list(int(page.color_map[foreground]).to_bytes(4, "little"))
            require(rgba == ([255, 0, 0, 255] if theme == "day" else [0, 0, 255, 255]),
                    "Independent decoded theme palette differs")
            require(list(int(page.color_map[background]).to_bytes(4, "little")) == [0, 0, 0, 255],
                    "Independent decoded background differs")
            canvas = (C.c_uint8 * 120)()
            lib.vbi_draw_vt_page_region(C.byref(page), 6, canvas, 12, column, 1, 1, 1, 1, 1)
            require(list(canvas) == [foreground if bit else background for bit in expected],
                    "Independent rendered DRCS pixels differ from literal")
        return {"page": f"{number:03X}", "raw_page": number, "subcode": 32, "unicode": unicode, "logical_subpage": subpage, "pixels": 120,
                "foreground_rgba": rgba}
    finally:
        if fetched:
            lib.vbi_unref_page(C.byref(page))


def check_indexed_fallback(lib, decoder, number):
    """The authored base contains ZZ at 8/9 and a space at the second consumer."""
    results = []
    for level, name in ((0, "1"), (1, "1.5")):
        page, fetched = base.Page(), False
        try:
            fetched = bool(lib.vbi_fetch_vt_page(decoder, C.byref(page), number, 0x20, level, 25, 0))
            require(fetched and page.pgno == number and page.subno == 0x20
                    and page.rows == 25 and page.columns == 41, "Unexpected fallback MAIN page identity/geometry")
            for column, unicode in ((8, 90), (9, 90), (10, 32)):
                cell = page.text[page.columns + column]
                require(cell.bits2 >> 16 == unicode and cell.bits1 >> 24 == 7
                        and cell.bits2 & 255 == 0, "Independent decoded Level 1/1.5 fallback differs")
            require(int(page.color_map[7]) == 0xFFFFFFFF and int(page.color_map[0]) == 0xFF000000,
                    "Independent decoded fallback palette differs")
            results.append({"level": name, "columns": [8, 9, 10], "unicode": [90, 90, 32],
                            "foreground_rgba": [255, 255, 255, 255], "background_rgba": [0, 0, 0, 255]})
        finally:
            if fetched:
                lib.vbi_unref_page(C.byref(page))
    return results


def warm_suffix(path):
    """Read a second exact ordered acquisition under the existing fixed bounds."""
    file = Path(path)
    require(file.is_file() and not file.is_symlink(), "Expected regular warm suffix units file")
    with file.open("rb") as stream:
        raw = stream.read(base.MAX_INPUT + 1)
    require(0 < len(raw) <= base.MAX_INPUT, "Warm suffix input byte bound exceeded")
    units = json.loads(raw, object_pairs_hook=base.unique_object)
    require(type(units) is list and 1 <= len(units) <= 512, "Warm suffix unit count bound exceeded")
    for unit in units:
        require(type(unit) is list and len(unit) == 46
                and all(base.integer(v, 0, 255) for v in unit)
                and unit[:2] == [2, 44] and unit[3] == 0xE4,
                "Invalid warm suffix captured DVB unit")
    return raw, units


def feed(lib, decoder, units, first_index):
    """Retain one receiver and a continuous explicit decoder input clock."""
    for index, unit in enumerate(units, first_index):
        sliced = base.Sliced()
        sliced.id, sliced.line = 3, unit[2] & 31
        sliced.data[:42] = bytes(base.BIT_REVERSE[v] for v in unit[4:])
        lib.vbi_decode(decoder, C.byref(sliced), 1, 1.0 + index * .04)


def main():
    require(len(sys.argv) in (3, 4, 5) and sys.argv[2] in ("day", "night"), "Expected units file, day/night and optional fixed mode/suffix")
    mode = sys.argv[3] if len(sys.argv) >= 4 else "singleton"
    require(mode in ("singleton", "singleton-edited", "indexed", "indexed-edited", "sparse", "singleton-warm-edited"), "Unknown synthetic bitmap oracle")
    require((mode == "singleton-warm-edited") == (len(sys.argv) == 5), "Warm mode alone requires an explicit suffix file")
    file = Path(sys.argv[1])
    require(file.is_file() and not file.is_symlink(), "Expected regular captured units file")
    with file.open("rb") as stream:
        raw = stream.read(base.MAX_INPUT + 1)
    require(0 < len(raw) <= base.MAX_INPUT, "Input byte bound exceeded")
    units = json.loads(raw, object_pairs_hook=base.unique_object)
    require(type(units) is list and 1 <= len(units) <= 512, "Unit count bound exceeded")
    for unit in units:
        require(type(unit) is list and len(unit) == 46 and all(base.integer(v, 0, 255) for v in unit)
                and unit[:2] == [2, 44] and unit[3] == 0xE4, "Invalid captured DVB unit")
    lib, version = base.load_decoder()
    decoder = lib.vbi_decoder_new()
    require(bool(decoder), "Decoder allocation failed")
    callback = base.CALLBACK(lambda event, user: None)
    try:
        require(lib.vbi_event_handler_register(decoder, 2, callback, None), "Registration failed")
        lib.vbi_teletext_set_level(decoder, 2)
        if mode == "singleton-warm-edited":
            suffix_raw, suffix = warm_suffix(sys.argv[4])
            feed(lib, decoder, units, 0)
            original = [check_page(lib, decoder, number, sys.argv[2], 0, False)
                        for number in (0x124, 0x125)]
            feed(lib, decoder, suffix, len(units))
            updated = [check_page(lib, decoder, number, sys.argv[2], 0, sys.argv[2] == "day")
                       for number in (0x124, 0x125)]
            print(json.dumps({"decoder_version": version, "mode": mode,
                              "theme": sys.argv[2], "single_decoder": True,
                              "decoder_input_clock_continuous": True,
                              "prefix_sha256": hashlib.sha256(raw).hexdigest(),
                              "suffix_sha256": hashlib.sha256(suffix_raw).hexdigest(),
                              "prefix_units": len(units), "suffix_units": len(suffix),
                              "max_units_per_file": 512, "max_bytes_per_file": base.MAX_INPUT,
                              "original_pages": original, "updated_pages": updated,
                              "panels_proven": False}, sort_keys=True))
            return
        for index, unit in enumerate(units):
            sliced = base.Sliced()
            sliced.id, sliced.line = 3, unit[2] & 31
            sliced.data[:42] = bytes(base.BIT_REVERSE[v] for v in unit[4:])
            lib.vbi_decode(decoder, C.byref(sliced), 1, 1.0 + index * .04)
        if mode == "indexed-edited":
            pages = []
            for number, subpage in ((0x124, 0), (0x125, 1)):
                page = check_page(lib, decoder, number, sys.argv[2], subpage,
                                  sys.argv[2] == "day" and subpage == 0, columns=(8, 10))
                page["checked_columns"] = [8, 10]
                page["fallback"] = check_indexed_fallback(lib, decoder, number)
                pages.append(page)
        else:
            pages = [check_page(lib, decoder, number, sys.argv[2],
                                1 if mode == "sparse" or (mode == "indexed" and number == 0x125) else 0,
                                mode == "singleton-edited" and sys.argv[2] == "day")
                     for number in (0x124, 0x125)]
        print(json.dumps({"decoder_version": version, "units_sha256": hashlib.sha256(raw).hexdigest(),
                          "theme": sys.argv[2], "mode": mode, "pages": pages, "panels_proven": False}, sort_keys=True))
    finally:
        lib.vbi_decoder_delete(decoder)


if __name__ == "__main__":
    try:
        main()
    except (base.CalibrationError, OSError, ValueError, AttributeError, TypeError, RecursionError) as error:
        message = str(error) if isinstance(error, base.CalibrationError) else "DRCS pixel calibration failed"
        print(json.dumps({"error": message}), file=sys.stderr)
        sys.exit(1)
