#!/usr/bin/env python3
"""Strict actual-stream G1 tuples and the known stock later-spacing failure.

Exit zero means every complete acquisition passed the independent imager and
stock libzvbi reproduced its exact known negative alongside checked native
pixels. It does not mean stock G1 conformance, suitable-client acceptance,
side-panel rendering or full Level2.5 acceptance. No packets are filtered,
synthesized or rewritten; DVB bit order is converted directly to T42 order.
"""

import argparse
import ctypes as C
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import re
import sys
import types

sys.dont_write_bytecode = True
SOURCES = {
    "teletext-resource-decoder.py": "615bc0ceb7b24738e7cb7e8a6df076c8b58eb9968ecdc7e07d226f76f5d7a76d",
    "teletext-object-decoder.py": "d8d7001f6df374614b95c59a413b07f60af4038089bed1811c12abbe2cf6c6f1",
    "teletext-imager-decoder.py": "45bd284daf621915d3e718ad536d2b4ea69a55a38c2c923712fd2cf4e79c225c",
    "teletext-main-composition-decoder.py": "e1cb2ad2b16e78a0f56baaae14434aa6d78f146d7e0173c7ade9f19db97228eb",
    "teletext-mosaic-decoder.py": "94619eb91da173536b41ddc4ce4da05d63061dcd7898fe304b29e8cfd98336bf",
}
STOCK = Path("/usr/lib/x86_64-linux-gnu/libzvbi.so.0.13.2")
STOCK_SHA256 = "134705a7ef4d982eb57bf71a9378f3eb0d95237c8dcb686d684e35471ae834b0"
IMAGER_ENV = "ERSATZRS_TELETEXT_TEST_IMAGER_DIR"
MAX_SOURCE = 65536


def require(condition, message):
    if not condition:
        raise ValueError(message)


def bounded(path, maximum):
    require(path.is_file() and not path.is_symlink(), "Expected regular bounded G1 proof input")
    with path.open("rb") as stream:
        body = stream.read(maximum + 1)
    require(0 < len(body) <= maximum, "G1 proof input exceeds byte bound")
    return body


def frozen_helpers():
    # Existing sibling helpers import one another through spec loaders. Supply
    # their authenticated source bytes for those exact five paths as well, so
    # cached bytecode cannot detach executed helper code from its source pin.
    bodies = {}
    for name, expected in SOURCES.items():
        path = Path(__file__).with_name(name)
        body = bounded(path, MAX_SOURCE)
        require(hashlib.sha256(body).hexdigest() == expected, "Frozen G1 helper source changed")
        bodies[str(path)] = body
    original = importlib.util.spec_from_file_location

    class CheckedSource:
        def __init__(self, path):
            self.path = path

        def create_module(self, _spec):
            return None

        def exec_module(self, module):
            module.__file__ = self.path
            exec(compile(bodies[self.path], self.path, "exec"), module.__dict__)

    def checked_spec(name, path, *args, **kwargs):
        if str(path) in bodies:
            return importlib.util.spec_from_loader(name, CheckedSource(str(path)))
        return original(name, path, *args, **kwargs)

    try:
        importlib.util.spec_from_file_location = checked_spec
        modules = []
        for name in ("teletext-mosaic-decoder.py", "teletext-main-composition-decoder.py"):
            path = Path(__file__).with_name(name)
            module = types.ModuleType("authenticated_" + name.replace("-", "_"))
            module.__file__ = str(path)
            exec(compile(bodies[str(path)], str(path), "exec"), module.__dict__)
            modules.append(module)
        return tuple(modules)
    finally:
        importlib.util.spec_from_file_location = original


MOSAIC, COMPOSITION = frozen_helpers()
BASE = MOSAIC.BASE


def captured_units(path):
    raw = bounded(path, BASE.MAX_INPUT)
    units = json.loads(raw, object_pairs_hook=BASE.unique_object)
    require(type(units) is list and 1 <= len(units) <= 512, "Invalid captured G1 unit count")
    for unit in units:
        require(type(unit) is list and len(unit) == 46
                and all(BASE.integer(value, 0, 255) for value in unit)
                and unit[:2] == [2, 44] and unit[3] == 0xE4, "Invalid captured G1 DVB unit")
    return raw, units


def terminal_header(units, hamming):
    packet = [BASE.BIT_REVERSE[value] for value in units[-1][4:]]
    n = [hamming.decode(value) for value in packet[:10]]
    require(all(type(value) is int and 0 <= value <= 15 for value in n), "Terminal G1 header is not valid Hamming")
    number = ((n[0] & 7) << 8) | (n[3] << 4) | n[2]
    subcode = ((n[7] & 3) << 12) | (n[6] << 8) | ((n[5] & 7) << 4) | n[4]
    require((n[0] >> 3) | (n[1] << 1) == 0 and number in (0x124, 0x125) and subcode == 32,
            "G1 input must end immediately after a transmitted124/125 subcode32 header; trailing partial acquisition rejected")
    return {"raw_page": number, "subcode": subcode, "packet": 0,
            "scope": "Final actual header completes prior acquisition; prefetched empty page is not decoded"}


def expected_cells(level):
    return [(1, column, [127, 25 if column >= 20 or level == "1.5" and column < 12 else 24, 0, 7, 0])
            for column in (8, 9, 12, 13, 20, 21)] + [(2, 1, [66, 15, 0, 7, 0])]


def check_imager_page(decoder_type, page):
    require(type(page) is dict and type(page.get("number")) is int
            and page["number"] in (0x124, 0x125) and type(page.get("subcode")) is int
            and page["subcode"] == 32, "Unexpected imager page/subcode; other returned pages are rejected")
    levels = []
    for level in ("1.5", "2.5"):
        decoder = decoder_type()
        decoder.decode(page, level=level)
        cells = []
        for row, column, expected in expected_cells(level):
            require(decoder.get_drcs_ptu(row, column) is None, "Unexpected DRCS alias in G1 proof")
            observed = [decoder.get_char_code(row, column), decoder.get_char_set(row, column),
                        decoder.get_char_diacritic(row, column), decoder.get_foreground(row, column),
                        decoder.get_background(row, column)]
            require(observed == expected and all(type(value) is int for value in observed),
                    f"Imager {page['number']:03X} Level{level} cell{row},{column}: actual{observed}, expected{expected}")
            cells.append({"row": row, "column": column, "tuple": observed})
        levels.append({"level": level, "cells": cells})
    return {"raw_page": page["number"], "subcode": 32, "levels": levels}


def imager_decode(units, root):
    previous = os.environ.get(IMAGER_ENV)
    try:
        os.environ[IMAGER_ENV] = str(root.resolve())
        reader_type, decoder_type = COMPOSITION.imager.load_decoder()
    finally:
        if previous is None:
            os.environ.pop(IMAGER_ENV, None)
        else:
            os.environ[IMAGER_ENV] = previous
    terminal = terminal_header(units, reader_type.read.__globals__["hamming_8_4"])
    payload = bytes(BASE.BIT_REVERSE[value] for unit in units for value in unit[4:])
    stream, reader, reports = io.BytesIO(payload), reader_type(), []
    counts = {0x124: 0, 0x125: 0}
    # Pinned readt42 returns completed acquisitions incrementally and prefetches
    # the following header. Calling it after byteEOF would expose an empty page.
    while stream.tell() < len(payload):
        start = stream.tell()
        pages = reader.read(stream)
        require(stream.tell() > start and type(pages) is list and 1 <= len(pages) <= len(units),
                "Imager reader did not return bounded completed acquisitions")
        for page in pages:
            require(len(reports) < len(units), "Imager acquisition count exceeds input bound")
            report = check_imager_page(decoder_type, page)
            counts[report["raw_page"]] += 1
            reports.append(report)
    require(all(counts.values()), "Both actual124/125 G1 acquisitions are required")
    return {"version": COMPOSITION.imager.VERSION, "commit": COMPOSITION.imager.COMMIT,
            "upstream_source_sha256": COMPOSITION.imager.HASHES,
            "acquisition_counts": {f"{number:03X}": count for number, count in counts.items()},
            "pages": reports, "terminal_header": terminal,
            "padding_policy": "Pinned reader ignores time-filling X/FF headers and whole-magazine packets29 and above; every returned page must be124/125 subcode32"}


def stock_mapping(mappings):
    records = []
    for line in mappings.splitlines():
        fields = line.split(maxsplit=5)
        if len(fields) == 6 and "/libzvbi.so." in fields[5]:
            path = re.sub(r"\\([0-7]{3})", lambda match: chr(int(match[1], 8)), fields[5])
            records.append((path, fields[3], fields[4]))
    require({path for path, _, _ in records} == {str(STOCK)}, "Expected only the qualified mapped stock libzvbi")
    stat = STOCK.stat()
    for _, device, inode in records:
        require(tuple(int(part, 16) for part in device.split(":")) == (os.major(stat.st_dev), os.minor(stat.st_dev))
                and int(inode) == stat.st_ino, "Mapped stock decoder file identity changed")
    require(hashlib.sha256(bounded(STOCK, 16 * 1024 * 1024)).hexdigest() == STOCK_SHA256,
            "Mapped stock decoder hash changed")
    return {"path": str(STOCK), "sha256": STOCK_SHA256, "mapped_file_identity_verified": True}


def stock_base(page):
    cell = page.text[page.columns * 2 + 1]
    require(cell.bits2 >> 16 == 66 and cell.bits1 >> 24 == 7 and cell.bits2 & 255 == 0,
            "Stock B control glyph or white/black colours differ")
    require((cell.bits1 >> 8) & 255 == 0 and (cell.bits1 >> 16) & 255 == 3 and cell.bits1 & 0x18 == 0,
            "Stock B control must remain normal opaque steady unconcealed")
    require(int(page.color_map[7]) == 0xFFFFFFFF and int(page.color_map[0]) == 0xFF000000,
            "Stock B control white/black palette differs")


def stock_page(lib, decoder, number, level):
    page, fetched = BASE.Page(), False
    cells = []
    try:
        fetched = bool(lib.vbi_fetch_vt_page(decoder, C.byref(page), number, 32, level, 25, 0))
        require(fetched and page.pgno == number and page.subno == 32 and page.columns == 41 and page.rows == 25,
                "Stock G1 page identity/geometry differs")
        stock_base(page)
        for column in (8, 9, 12, 13, 20, 21):
            cell = page.text[41 + column]
            require(cell.bits1 >> 24 == 7 and cell.bits2 & 255 == 0,
                    "Stock G1 literal white7/black0 indices differ")
            normative = "separated" if column >= 20 or level == 1 and column < 12 else "contiguous"
            case = {"page": number, "subcode": 32, "row": 1, "column": column, "pattern": normative}
            failure = None
            if level == 2 and column >= 20:
                try:
                    MOSAIC.check_page(lib, page, case)
                except MOSAIC.BASE.CalibrationError as error:
                    failure = str(error)
                require(failure == "decoded full G1 mosaic is incorrect", "Stock later-spacing failure did not reproduce the exact normative Unicode negative")
                observed = "contiguous"
                MOSAIC.check_page(lib, page, {**case, "pattern": observed})
            else:
                observed = normative
                MOSAIC.check_page(lib, page, case)
            cells.append({"row": 1, "column": column, "normative": normative,
                          "observed": observed, "unicode": cell.bits2 >> 16,
                          "foreground": 7, "background": 0,
                          "pixels_checked": 120, "normative_error": failure})
        return {"raw_page": number, "subcode": 32, "level": "1.5" if level == 1 else "2.5",
                "cells": cells, "B_control": {"row": 2, "column": 1, "unicode": 66, "foreground": 7,
                                             "background": 0, "normal_opaque_steady_unconcealed": True}}
    finally:
        if fetched:
            lib.vbi_unref_page(C.byref(page))


def stock_decode(units):
    require(not os.environ.get("LD_LIBRARY_PATH"), "Stock G1 proof rejects inherited LD_LIBRARY_PATH overrides")
    require(hashlib.sha256(bounded(STOCK, 16 * 1024 * 1024)).hexdigest() == STOCK_SHA256,
            "Qualified stock decoder hash changed")
    lib, version = BASE.load_decoder()
    decoder = lib.vbi_decoder_new()
    require(bool(decoder), "Stock G1 decoder allocation failed")
    callback = BASE.CALLBACK(lambda event, user: None)
    try:
        require(lib.vbi_event_handler_register(decoder, 2, callback, None), "Stock G1 registration failed")
        lib.vbi_teletext_set_level(decoder, 2)
        for index, unit in enumerate(units):
            sliced = BASE.Sliced()
            sliced.id, sliced.line = 3, unit[2] & 31
            sliced.data[:42] = bytes(BASE.BIT_REVERSE[value] for value in unit[4:])
            lib.vbi_decode(decoder, C.byref(sliced), 1, 1.0 + index * .04)
        pages = [stock_page(lib, decoder, number, level) for number in (0x124, 0x125) for level in (1, 2)]
        mapped = stock_mapping(bounded(Path("/proc/self/maps"), 4 * 1024 * 1024).decode())
        return {"version": version, "mapped_library": mapped, "pages": pages,
                "exact_known_negative": "decoded full G1 mosaic is incorrect"}
    finally:
        lib.vbi_decoder_delete(decoder)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("units", type=Path)
    parser.add_argument("--imager-root", type=Path, required=True)
    arguments = parser.parse_args()
    raw, units = captured_units(arguments.units)
    imager = imager_decode(units, arguments.imager_root)
    stock = stock_decode(units)
    require(bounded(arguments.units, BASE.MAX_INPUT) == raw, "Captured G1 input changed during proof")
    print(json.dumps({"schema": 1, "units_sha256": hashlib.sha256(raw).hexdigest(), "unit_count": len(units),
                      "source_sha256": SOURCES, "imager": imager, "stock": stock,
                      "imager_conformance": True, "stock_g1_conformance": False,
                      "known_later_spacing_failure_reproduced": True,
                      "actual_client_proven": False, "panels_proven": False, "full_level25_proven": False}, sort_keys=True))


if __name__ == "__main__":
    try:
        main()
    except (BASE.CalibrationError, COMPOSITION.resource.CalibrationError, OSError, ValueError, KeyError, IndexError,
            TypeError, AttributeError, RuntimeError, RecursionError) as error:
        print(json.dumps({"error": str(error)}), file=sys.stderr)
        sys.exit(1)
