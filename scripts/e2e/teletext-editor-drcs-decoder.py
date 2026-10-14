#!/usr/bin/env python3
"""Independent pixels for the fixed synthetic QTM DRCS flow, Linux x64 only.

Reads ordered units captured from the ordinary stream, not native semantic
objects. Reuses the hash-pinned libzvbi ABI helper. Literal bitmap/palette
expectations cover the MAIN cell only; 56-column panels remain unproven.
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


def check_page(lib, decoder, number, theme, subpage):
    page, fetched = base.Page(), False
    try:
        fetched = bool(lib.vbi_fetch_vt_page(decoder, C.byref(page), number, 0x20, 2, 25, 0))
        require(fetched and page.pgno == number and page.subno == 0x20
                and page.rows == 25 and page.columns == 41, "Unexpected MAIN page geometry")
        cell = page.text[page.columns + 8]
        bank, unicode = 16 + subpage, 0xF400 + subpage * 64
        require(cell.bits2 >> 16 == unicode and bool(page.drcs[bank]), "Normal indexed DRCS/slot0 missing")
        packed = C.string_at(C.cast(page.drcs[bank], C.c_void_p).value, 60)
        expected = [(row >> (11 - x)) & 1 for row in PATTERNS[subpage] for x in range(12)]
        require([(packed[x // 2] >> (4 * (x % 2))) & 15 for x in range(120)] == expected,
                "Independent decoded DRCS bitmap differs from literal")
        foreground, background = cell.bits1 >> 24, cell.bits2 & 255
        rgba = list(int(page.color_map[foreground]).to_bytes(4, "little"))
        require(rgba == ([255, 0, 0, 255] if theme == "day" else [0, 0, 255, 255]),
                "Independent decoded theme palette differs")
        require(list(int(page.color_map[background]).to_bytes(4, "little")) == [0, 0, 0, 255],
                "Independent decoded background differs")
        canvas = (C.c_uint8 * 120)()
        lib.vbi_draw_vt_page_region(C.byref(page), 6, canvas, 12, 8, 1, 1, 1, 1, 1)
        require(list(canvas) == [foreground if bit else background for bit in expected],
                "Independent rendered DRCS pixels differ from literal")
        return {"page": f"{number:03X}", "raw_page": number, "subcode": 32, "unicode": unicode, "logical_subpage": subpage, "pixels": 120,
                "foreground_rgba": rgba}
    finally:
        if fetched:
            lib.vbi_unref_page(C.byref(page))


def main():
    require(len(sys.argv) in (3, 4) and sys.argv[2] in ("day", "night"), "Expected units file, day/night and optional indexed/sparse mode")
    mode = sys.argv[3] if len(sys.argv) == 4 else "singleton"
    require(mode in ("singleton", "indexed", "sparse"), "Unknown synthetic bitmap oracle")
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
        for index, unit in enumerate(units):
            sliced = base.Sliced()
            sliced.id, sliced.line = 3, unit[2] & 31
            sliced.data[:42] = bytes(base.BIT_REVERSE[v] for v in unit[4:])
            lib.vbi_decode(decoder, C.byref(sliced), 1, 1.0 + index * .04)
        pages = [check_page(lib, decoder, number, sys.argv[2],
                            1 if mode == "sparse" or (mode == "indexed" and number == 0x125) else 0)
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
