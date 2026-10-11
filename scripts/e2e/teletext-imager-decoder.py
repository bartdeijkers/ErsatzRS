#!/usr/bin/env python3
"""Opt-in independent local-object cells from unmodified teletextimager 0.2.0.

The external checkout must be supplied explicitly; nothing is downloaded or
installed. This checks three authored local-object goldens, not resource routing,
font pixels, navigation, complete Level 2.5 or client acceptance. Input version
0.2.42 retains the original fixture provenance; output identifies this decoder.
"""
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import sys
import types

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location(
    "teletext_object_calibration",
    Path(__file__).with_name("teletext-object-decoder.py"),
)
objects = importlib.util.module_from_spec(spec)
spec.loader.exec_module(objects)
resource = objects.resource
require = resource.require

COMMIT = "2b0125b778c45ffdfa2df0b196870c093361f317"
VERSION = "0.2.0"
HASHES = {
    "pyproject.toml": "d2e0bfdcaad32b68db532da67d609b63c0f7627fa61181b515686f125cdf3f40",
    "teletextimager/__init__.py": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    "teletextimager/reader/__init__.py": "b614888201aa0cd1faa9a9f1bb9abeebb3122a136d0414673d9e71605e184752",
    "teletextimager/bits/hamming_8_4.py": "0ff5f3e4a250b28e37e40858725fc4718ddf25b21ecb35b0b0ded6b2a5cc83f9",
    "teletextimager/bits/hamming_24_18.py": "0a1d227ffd7c593c5ce487602235abb380287c284e4da35cc1460be7ddc717d6",
    "teletextimager/teletextdrcs.py": "f0602f15ae6b0a214592e02ff7a8d0ef4c5862ea91284b4f0dbf6be24f4a8c82",
    "teletextimager/reader/readt42.py": "79778bd60439e27535decf5c587dce2eb8448fc1d79499d47f71b937aaa2cd2d",
    "teletextimager/teletextdecoder.py": "293b716e6ce0b1628ecf941748f02c24f48b4008d20fb20c38af0bafb53388be",
}


def load_decoder():
    directory = os.environ.get("ERSATZRS_TELETEXT_TEST_IMAGER_DIR", "")
    require(bool(directory), "explicit external decoder checkout is required")
    checkout = Path(directory)
    require(checkout.is_dir(), "external decoder checkout is unavailable")
    git_directory = checkout / ".git"
    require(git_directory.is_dir() and not git_directory.is_symlink(),
            "external decoder requires an ordinary detached checkout")
    head_path = git_directory / "HEAD"
    require(not head_path.is_symlink(), "external decoder requires a regular detached HEAD")
    with head_path.open("rb") as head_file:
        revision = head_file.read(129)
    require(len(revision) <= 128 and revision.strip() == COMMIT.encode("ascii"),
            "external decoder commit does not match calibration pin")
    sources = {}
    for relative, digest in HASHES.items():
        with (checkout / relative).open("rb") as source:
            data = source.read(256 * 1024 + 1)
        require(len(data) <= 256 * 1024 and hashlib.sha256(data).hexdigest() == digest,
                f"external decoder source hash differs: {relative}")
        sources[relative] = data
    # The exact pyproject hash also fences the declared package version. Loading
    # only verified byte strings avoids import-path substitution and cached pyc.
    require(b"version = '0.2.0'" in sources["pyproject.toml"], "external decoder package version differs")

    def module(name, relative=None, package=False):
        value = types.ModuleType(name)
        value.__package__ = name if package else name.rpartition(".")[0]
        if package:
            value.__path__ = []
        sys.modules[name] = value
        if "." in name:
            parent, _, child = name.rpartition(".")
            setattr(sys.modules[parent], child, value)
        if relative is not None:
            exec(compile(sources[relative], relative, "exec"), value.__dict__)
        return value

    module("teletextimager", "teletextimager/__init__.py", package=True)
    # Upstream bits is a namespace package: it has no __init__.py to execute.
    module("teletextimager.bits", package=True)
    module("teletextimager.reader", "teletextimager/reader/__init__.py", package=True)
    module("teletextimager.bits.hamming_8_4", "teletextimager/bits/hamming_8_4.py")
    module("teletextimager.bits.hamming_24_18", "teletextimager/bits/hamming_24_18.py")
    module("teletextimager.teletextdrcs", "teletextimager/teletextdrcs.py")
    reader = module("teletextimager.reader.readt42", "teletextimager/reader/readt42.py")
    decoder = module("teletextimager.teletextdecoder", "teletextimager/teletextdecoder.py")
    return reader.TeletextReadT42, decoder.TeletextDecode


def decode_case(reader_type, decoder_type, case):
    # This bounded calibration only assigns repertoire expectations to the two
    # original golden locations: X26 Latin G0 A, and underlying English-NOS T.
    for cell in case["cells"]:
        require((cell["row"], cell["column"]) in {(4, 8), (4, 9)}
                and 0x20 <= cell["unicode"] <= 0x7E,
                "unsupported local calibration cell or repertoire")
    require(len(case["cells"]) == 2, "both local calibration cells are required")
    packets = bytes(resource.BIT_REVERSE[byte] for unit in case["units"] for byte in unit[4:])
    pages = reader_type().read(io.BytesIO(packets))
    require(type(pages) is list and len(pages) == 1, "unexpected decoded page count")
    page = pages[0]
    require(page.get("number") == case["page"] and page.get("subcode") == case["subcode"],
            "decoded display identity differs")
    decoder = decoder_type()
    decoder.decode(page, level="2.5")

    def actual(row, column):
        require(decoder.get_drcs_ptu(row, column) is None, "unexpected DRCS calibration glyph")
        return (decoder.get_char_code(row, column), decoder.get_char_set(row, column),
                decoder.get_char_diacritic(row, column), decoder.get_foreground(row, column),
                decoder.get_background(row, column))

    require(actual(2, 1) == (ord("B"), 12, 0, 7, 0), "base Latin calibration character differs")
    for expected in case["cells"]:
        row, column = expected["row"], expected["column"]
        wanted = (expected["unicode"], 0 if column == 8 else 12, 0,
                  expected["foreground"], expected["background"])
        observed = actual(row, column)
        require(observed == wanted,
                f"cell {row},{column} differs: actual {observed}, expected {wanted}")


def main():
    raw = sys.stdin.buffer.read(resource.MAX_INPUT + 1)
    require(0 < len(raw) <= resource.MAX_INPUT, "invalid bounded input size")
    cases = objects.validate(json.loads(raw, object_pairs_hook=resource.unique_object))
    reader_type, decoder_type = load_decoder()
    for case in cases:
        try:
            decode_case(reader_type, decoder_type, case)
        except resource.CalibrationError as error:
            raise resource.CalibrationError(f"{case['name']}: {error}") from error
    print(json.dumps({"decoder": "teletextimager", "decoder_version": VERSION,
                      "decoder_commit": COMMIT, "fixture_provenance_version": "0.2.42",
                      "cases_passed": len(cases)}, sort_keys=True))


if __name__ == "__main__":
    try:
        main()
    except (resource.CalibrationError, OSError, ValueError, AttributeError, RecursionError,
            KeyError, IndexError, TypeError) as error:
        message = str(error) if isinstance(error, resource.CalibrationError) else "independent local calibration could not run"
        print(json.dumps({"error": message}), file=sys.stderr)
        sys.exit(1)
