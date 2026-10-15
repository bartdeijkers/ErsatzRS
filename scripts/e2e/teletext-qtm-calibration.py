#!/usr/bin/env python3
"""Reproduce and verify one synthetic QTeletextMaker 1.0 edit/save trial.

Pinned source: cac2167bac933a6160cca45a45d5d4ee95355898. The fixtures cover
G0 mode 9 enhancement words and a singleton mode-0 DRCS bitmap carrier, without
private engine sidecars. Verification covers only the recorded synthetic editor
trials, not arbitrary TTI, import permissions or Rust roundtrips.
"""

import argparse
import hashlib
import json
from pathlib import Path
import sys

MAX_BYTES = 8192
EDITOR_COMMIT = "cac2167bac933a6160cca45a45d5d4ee95355898"
ORIGINAL_SHA256 = "261ec4c6a90c5467cf497802c58f577563ff0b42db4ca61485dcd8dd5ba625b0"
EDITED_SHA256 = "1cfa16ca74a88898fef95593858d3acedcc6c118db752414b2e33da7b35209db"
DESCRIPTION = b"DE,Synthetic ErsatzRS external-editor audit"
LINKS = b"FL,201,202,203,204,205,206"
ENHANCEMENT = b"OL,26,@iD@Ei`" + b"\x7f" * 33
SIDECAR = b"XX,not-an-ErsatzRS-sidecar"
DRCS_ORIGINAL_SHA256 = "208ee7a257e7300d7de8f8b413d943ce2c4b71a36c3d565f42895bec3d1ab0cf"


class CalibrationError(Exception):
    """The bounded synthetic trial differs from its recorded oracle."""


def require(condition, message):
    if not condition:
        raise CalibrationError(message)


def row(number, text):
    require(len(text) <= 40, "invalid literal row width")
    return b"OL," + str(number).encode("ascii") + b"," + text.ljust(40, b" ")


def original_bytes():
    lines = [DESCRIPTION]
    for subcode, title in [(b"0007", b"FIRST EDITOR AUDIT"),
                           (b"0030", b"SECOND EDITOR AUDIT")]:
        lines.extend([
            b"PN,2" + subcode, b"SC," + subcode, b"PS,8000", b"RE,0", b"CT,8,T",
            LINKS, row(0, b"ERSATZRS AUDIT"), row(1, title), row(2, b"EDIT THIS ROW"),
            ENHANCEMENT, SIDECAR,
        ])
    payload = b"\r\n".join(lines) + b"\r\n"
    require(len(payload) == 619 and digest(payload) == ORIGINAL_SHA256,
            "internal synthetic fixture does not match its recorded hash")
    return payload


def digest(payload):
    return hashlib.sha256(payload).hexdigest()


def bounded_read(path):
    require(0 < path.stat().st_size <= MAX_BYTES, "input exceeds the 8 KiB bound")
    with path.open("rb") as handle:
        payload = handle.read(MAX_BYTES + 1)
    require(0 < len(payload) <= MAX_BYTES, "input changed beyond the 8 KiB bound")
    return payload


def lines(payload):
    require(payload.endswith(b"\r\n"), "missing final CRLF")
    require(b"\n" not in payload.replace(b"\r\n", b"")
            and b"\r" not in payload.replace(b"\r\n", b""), "non-CRLF line ending")
    return payload[:-2].split(b"\r\n")


def edited_pages(records):
    require(records[0] == DESCRIPTION, "description changed")
    starts = [index for index, record in enumerate(records) if record.startswith(b"PN,")]
    require(starts == [1, 11], "unexpected page count or record layout")
    return [records[1:11], records[11:]]


def verify(original_path, edited_path):
    original = bounded_read(original_path)
    edited = bounded_read(edited_path)
    original_records = lines(original)
    edited_records = lines(edited)
    require(original == original_bytes(), "original is not the pinned synthetic fixture")
    require(digest(edited) == EDITED_SHA256 and len(edited) == 563,
            "saved file differs from the pinned edit/save trial")
    pages = edited_pages(edited_records)
    for index, page in enumerate(pages):
        subcode = [b"0001", b"0002"][index]
        title = [b"FIRST EDITOR AUDIT", b"SECOND EDITOR AUDIT"][index]
        changed_row = [b"SAVED QTM ROW!", b"EDIT THIS ROW"][index]
        require(page == [
            b"PN,2" + subcode, b"SC," + subcode, b"PS,8000", b"RE,0", b"CT,8,T",
            row(0, b"ERSATZRS AUDIT"), ENHANCEMENT, row(1, title), row(2, changed_row), LINKS,
        ], "page metadata, padded rows, enhancement words or links changed")
    require([record for record in original_records if record.startswith(b"OL,26,")]
            == [record for record in edited_records if record.startswith(b"OL,26,")]
            == [ENHANCEMENT, ENHANCEMENT], "enhancement payload changed")
    require(original_records.count(SIDECAR) == 2
            and not any(record.startswith(b"XX,") for record in edited_records),
            "unknown XX record was not dropped as recorded")
    return {
        "calibration_passed": True,
        "engine_roundtrip_proven": False,
        "editor": "QTeletextMaker",
        "editor_version": "1.0",
        "editor_commit": EDITOR_COMMIT,
        "original_sha256": digest(original),
        "edited_sha256": digest(edited),
        "original_bytes": len(original),
        "edited_bytes": len(edited),
        "pages_verified": 2,
    }


def drcs_original_bytes():
    """The singleton mode-0 resource exported by the native companion fixture."""
    payload = b"\r\n".join([
        b"PN,1C000", b"SC,0000", b"PS,C07C",
        b"OL,28,CE@@p" + b"\x7f" * 31 + b"@@@@",
        row(1, b"`A`@P@H@D@B@A@@`@P@I"),
    ]) + b"\r\n"
    require(digest(payload) == DRCS_ORIGINAL_SHA256,
            "internal DRCS fixture differs from the native export")
    return payload


def verify_drcs(original_path, edited_path):
    """Qualify one saved bitmap change, not generic TTI or engine admission."""
    original = bounded_read(original_path)
    edited = bounded_read(edited_path)
    require(original == drcs_original_bytes(), "original is not the pinned DRCS fixture")
    records = lines(original)
    saved = lines(edited)
    # Case-normalized identities and added normal-editor timing records are
    # explicit observations. No packet, undefined glyph or header change is lost.
    expected = [b"PN,1c000", b"SC,0000", b"PS,c07c", b"RE,0", b"CT,20,T",
                records[3], row(1, b"`C`@P@H@D@B@A@@`@P@I")]
    require(saved == expected, "saved DRCS identity, function, modes or bitmap bytes differ")
    before = records[4].split(b",", 2)[2]
    after = saved[6].split(b",", 2)[2]
    require([i for i, pair in enumerate(zip(before, after)) if pair[0] != pair[1]] == [1],
            "DRCS edit changed more than OL1 column1")
    # Two consecutive six-bit characters encode each 12-bit PTU row; validate
    # all ten rows independently of the external writer and Rust implementation.
    def bitmap(payload):
        return [((payload[2*i] & 0x3f) << 6) | (payload[2*i+1] & 0x3f)
                for i in range(10)]
    old_bitmap = [0x801, 0x800, 0x400, 0x200, 0x100,
                  0x080, 0x040, 0x020, 0x010, 0x009]
    new_bitmap = [0x803, *old_bitmap[1:]]
    require(bitmap(before) == old_bitmap and bitmap(after) == new_bitmap,
            "DRCS PTU0 bitmap differs from the literal oracle")
    return {
        "calibration_passed": True, "engine_roundtrip_proven": False,
        "editor": "QTeletextMaker", "editor_version": "1.0",
        "editor_commit": EDITOR_COMMIT,
        "original_sha256": digest(original), "edited_sha256": digest(edited),
        "original_bytes": len(original), "edited_bytes": len(edited),
        "resource_page_hex": "1C0", "resource_s1": 0, "glyph_slot": 0,
        "changed_row": 1, "changed_column": 1,
        "bitmap_before": old_bitmap, "bitmap_after": new_bitmap,
        "normalizations": ["PN/PS hexadecimal case", "added RE,0 and CT,20,T"],
        "protected_packets_preserved": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    fixture = commands.add_parser("fixture", help="write the exact synthetic input; refuse overwrite")
    fixture.add_argument("destination", type=Path)
    verifier = commands.add_parser("verify", help="verify the recorded original and saved files")
    verifier.add_argument("original", type=Path)
    verifier.add_argument("edited", type=Path)
    drcs_fixture = commands.add_parser("fixture-drcs", help="write the singleton DRCS trial input")
    drcs_fixture.add_argument("destination", type=Path)
    drcs_verifier = commands.add_parser("verify-drcs", help="verify the single DRCS bitmap edit")
    drcs_verifier.add_argument("original", type=Path)
    drcs_verifier.add_argument("edited", type=Path)
    args = parser.parse_args()
    if args.command in ("fixture", "fixture-drcs"):
        payload = original_bytes() if args.command == "fixture" else drcs_original_bytes()
        with args.destination.open("xb") as handle:
            handle.write(payload)
        result = {"fixture_bytes": len(payload), "fixture_sha256": digest(payload),
                  "engine_roundtrip_proven": False}
    elif args.command == "verify":
        result = verify(args.original, args.edited)
    else:
        result = verify_drcs(args.original, args.edited)
    print(json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    try:
        main()
    except (CalibrationError, OSError, ValueError) as error:
        print(json.dumps({"error": str(error)}), file=sys.stderr)
        sys.exit(1)
