#!/usr/bin/env python3
"""Reproduce and verify one synthetic QTeletextMaker 1.0 edit/save trial.

Pinned source: cac2167bac933a6160cca45a45d5d4ee95355898. This fixture uses
G0 mode 9 enhancement words and no private engine sidecar. Verification covers
only the recorded synthetic editor trial, not arbitrary TTI or Rust roundtrips.
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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    fixture = commands.add_parser("fixture", help="write the exact synthetic input; refuse overwrite")
    fixture.add_argument("destination", type=Path)
    verifier = commands.add_parser("verify", help="verify the recorded original and saved files")
    verifier.add_argument("original", type=Path)
    verifier.add_argument("edited", type=Path)
    args = parser.parse_args()
    if args.command == "fixture":
        payload = original_bytes()
        with args.destination.open("xb") as handle:
            handle.write(payload)
        result = {"fixture_bytes": len(payload), "fixture_sha256": digest(payload),
                  "engine_roundtrip_proven": False}
    else:
        result = verify(args.original, args.edited)
    print(json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    try:
        main()
    except (CalibrationError, OSError, ValueError) as error:
        print(json.dumps({"error": str(error)}), file=sys.stderr)
        sys.exit(1)
