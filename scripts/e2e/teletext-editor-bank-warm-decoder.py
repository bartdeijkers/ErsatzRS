#!/usr/bin/env python3
"""Two-bank warm acquisition using the unchanged fixed DRCS literal oracle.

This opt-in leaf fills the singleton-only warm driver's concrete gap. Both
prefix/suffix captures remain unchanged; one decoder checks both S1 identities,
MAIN consumers and Level1/1.5 fallback. No panel or full conformance claim.
"""

import hashlib
import json
from pathlib import Path
import sys
import types

sys.dont_write_bytecode = True
ORACLE_SHA256 = "fe250705ddc7665a5012d2bb207d1702cfd3bf765716b0c1d4022265d1959a47"


def frozen_oracle():
    path = Path(__file__).with_name("teletext-editor-drcs-decoder.py")
    if not path.is_file() or path.is_symlink():
        raise RuntimeError("Expected regular frozen DRCS oracle source")
    with path.open("rb") as stream:
        body = stream.read(65537)
    if len(body) > 65536 or hashlib.sha256(body).hexdigest() != ORACLE_SHA256:
        raise RuntimeError("Frozen DRCS literal oracle changed")
    module = types.ModuleType("authenticated_bank_warm_drcs_oracle")
    module.__file__ = str(path)
    exec(compile(body, str(path), "exec"), module.__dict__)
    return module


ORACLE = frozen_oracle()
base, require = ORACLE.base, ORACLE.require


def check_indexed_warm_pages(lib, decoder, theme, edited):
    pages = []
    for number, subpage in ((0x124, 0), (0x125, 1)):
        page = ORACLE.check_page(lib, decoder, number, theme, subpage,
                                 edited and theme == "day" and subpage == 0, columns=(8, 10))
        page["checked_columns"] = [8, 10]
        page["fallback"] = ORACLE.check_indexed_fallback(lib, decoder, number)
        pages.append(page)
    return pages


def main():
    require(len(sys.argv) == 5 and sys.argv[2] in ("day", "night") and sys.argv[3] == "indexed-warm-edited",
            "Expected prefix units, day/night, indexed-warm-edited and suffix units")
    raw, units = ORACLE.warm_suffix(sys.argv[1])
    suffix_raw, suffix = ORACLE.warm_suffix(sys.argv[4])
    lib, version = base.load_decoder()
    decoder = lib.vbi_decoder_new()
    require(bool(decoder), "Decoder allocation failed")
    callback = base.CALLBACK(lambda event, user: None)
    try:
        require(lib.vbi_event_handler_register(decoder, 2, callback, None), "Registration failed")
        lib.vbi_teletext_set_level(decoder, 2)
        ORACLE.feed(lib, decoder, units, 0)
        original = check_indexed_warm_pages(lib, decoder, sys.argv[2], False)
        ORACLE.feed(lib, decoder, suffix, len(units))
        updated = check_indexed_warm_pages(lib, decoder, sys.argv[2], True)
        print(json.dumps({"decoder_version": version, "mode": "indexed-warm-edited",
                          "frozen_oracle_sha256": ORACLE_SHA256,
                          "theme": sys.argv[2], "single_decoder": True,
                          "decoder_input_clock_continuous": True,
                          "prefix_sha256": hashlib.sha256(raw).hexdigest(),
                          "suffix_sha256": hashlib.sha256(suffix_raw).hexdigest(),
                          "prefix_units": len(units), "suffix_units": len(suffix),
                          "max_units_per_file": 512, "max_bytes_per_file": base.MAX_INPUT,
                          "original_pages": original, "updated_pages": updated,
                          "panels_proven": False, "full_level25_proven": False}, sort_keys=True))
    finally:
        lib.vbi_decoder_delete(decoder)


if __name__ == "__main__":
    try:
        main()
    except (base.CalibrationError, OSError, ValueError, AttributeError, TypeError, RecursionError, RuntimeError) as error:
        message = str(error) if isinstance(error, base.CalibrationError) else "DRCS bank warm calibration failed"
        print(json.dumps({"error": message}), file=sys.stderr)
        sys.exit(1)
