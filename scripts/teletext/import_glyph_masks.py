#!/usr/bin/env python3
"""Reproduce pinned MIT teletext font masks without Pillow or runtime fetching."""

import argparse
import hashlib
import json
from pathlib import Path
import struct
import sys
import zlib

COMMIT = "2b0125b778c45ffdfa2df0b196870c093361f317"
FONT_NAMES = (
    "G0_latin", "G0_sr_hr", "G0_ru_bg", "G0_uk", "G0_el", "G0_ar", "G0_he",
    "G2_latin", "G2_cyr", "G2_el", "G2_ar",
    "G0_NOS_cs_sk", "G0_NOS_en", "G0_NOS_et", "G0_NOS_fr", "G0_NOS_de",
    "G0_NOS_it", "G0_NOS_lv_lt", "G0_NOS_pl", "G0_NOS_pt_es", "G0_NOS_ro",
    "G0_NOS_sr_hr_sl", "G0_NOS_sv_fi_hu", "G0_NOS_tr",
    "G1_con", "G1_sep", "G3", "G0_reduced",
)
# Exact upstream bytes, not a locally supplied manifest or mutable branch.
SOURCE_SHA256 = {
    "LICENSE": "da1e5265a43dfff2b70dde18552458eb1ebb5bec178f8e01d8093026a8881ea0",
    "pyproject.toml": "d2e0bfdcaad32b68db532da67d609b63c0f7627fa61181b515686f125cdf3f40",
    "teletextimager/font-etsi/G0_NOS_cs_sk.pbm": "f93f001dffce2576b60cc7cc23d6a89405e11c2f96717bb214925fd172f844d2",
    "teletextimager/font-etsi/G0_NOS_cs_sk.pil": "ec1af672eb4992baa2294af1991cbbe23f09f3d83324662e4920a6a493053e46",
    "teletextimager/font-etsi/G0_NOS_de.pbm": "c162653d50f54d1564a2bf61cab117e504d5e9dc278b3d58945cdcf18aeaf290",
    "teletextimager/font-etsi/G0_NOS_de.pil": "ec1af672eb4992baa2294af1991cbbe23f09f3d83324662e4920a6a493053e46",
    "teletextimager/font-etsi/G0_NOS_en.pbm": "415c808e58277f9ef112d2f0e3cc65b8f525d02b28747a93d1453f6b2d32196f",
    "teletextimager/font-etsi/G0_NOS_en.pil": "ec1af672eb4992baa2294af1991cbbe23f09f3d83324662e4920a6a493053e46",
    "teletextimager/font-etsi/G0_NOS_et.pbm": "881e801e9e676f00b5ba4e66c07f86334aa152899628c19c5ceeffdc59286157",
    "teletextimager/font-etsi/G0_NOS_et.pil": "ec1af672eb4992baa2294af1991cbbe23f09f3d83324662e4920a6a493053e46",
    "teletextimager/font-etsi/G0_NOS_fr.pbm": "b7f647c6fa9c0978d26553a9a221baefb1911f401c8b930bf13866026dcb313f",
    "teletextimager/font-etsi/G0_NOS_fr.pil": "ec1af672eb4992baa2294af1991cbbe23f09f3d83324662e4920a6a493053e46",
    "teletextimager/font-etsi/G0_NOS_it.pbm": "52031e8a13a780ce4343e429db253294941d6a5df61011db30e245d07af873b6",
    "teletextimager/font-etsi/G0_NOS_it.pil": "ec1af672eb4992baa2294af1991cbbe23f09f3d83324662e4920a6a493053e46",
    "teletextimager/font-etsi/G0_NOS_lv_lt.pbm": "56ea085f7c34d6826dd0b3796f129c729a25f8c07865eda03431fd004a1dfff9",
    "teletextimager/font-etsi/G0_NOS_lv_lt.pil": "ec1af672eb4992baa2294af1991cbbe23f09f3d83324662e4920a6a493053e46",
    "teletextimager/font-etsi/G0_NOS_pl.pbm": "33ddfee1ed1b9dbb9602146cbd504e38c89690ff5ad02da6080e4a8416f3f8f7",
    "teletextimager/font-etsi/G0_NOS_pl.pil": "ec1af672eb4992baa2294af1991cbbe23f09f3d83324662e4920a6a493053e46",
    "teletextimager/font-etsi/G0_NOS_pt_es.pbm": "6d6671700ed02760bbf12c16fd486fd4ec2836014819d1e272bd64fca911d27e",
    "teletextimager/font-etsi/G0_NOS_pt_es.pil": "ec1af672eb4992baa2294af1991cbbe23f09f3d83324662e4920a6a493053e46",
    "teletextimager/font-etsi/G0_NOS_ro.pbm": "be9736d1348ea3378a3f78f9d6d84a650834d924609c1064d4535eaa76011ff7",
    "teletextimager/font-etsi/G0_NOS_ro.pil": "ec1af672eb4992baa2294af1991cbbe23f09f3d83324662e4920a6a493053e46",
    "teletextimager/font-etsi/G0_NOS_sr_hr_sl.pbm": "bc9c3a474bdf93985638edbf3efa937f9819fbf83235478cd78741f9cf172d81",
    "teletextimager/font-etsi/G0_NOS_sr_hr_sl.pil": "ec1af672eb4992baa2294af1991cbbe23f09f3d83324662e4920a6a493053e46",
    "teletextimager/font-etsi/G0_NOS_sv_fi_hu.pbm": "9a56d27480b955f05d5ff7e16f3e238c16d320abfa8bf84eb1a4ad14ea8e17e7",
    "teletextimager/font-etsi/G0_NOS_sv_fi_hu.pil": "ec1af672eb4992baa2294af1991cbbe23f09f3d83324662e4920a6a493053e46",
    "teletextimager/font-etsi/G0_NOS_tr.pbm": "d86da12264dc48ccacb03b8fe75e21ca5d194d158b817f6a486e1d323cd60317",
    "teletextimager/font-etsi/G0_NOS_tr.pil": "ec1af672eb4992baa2294af1991cbbe23f09f3d83324662e4920a6a493053e46",
    "teletextimager/font-etsi/G0_ar.pbm": "ee9beda176417ae63b53f3ba7c24f2ae0306b1dd62e8b69cb6a164e8acf43165",
    "teletextimager/font-etsi/G0_ar.pil": "ec1af672eb4992baa2294af1991cbbe23f09f3d83324662e4920a6a493053e46",
    "teletextimager/font-etsi/G0_el.pbm": "a8ed4ad18ff6068fcfebe1d086fbbcce31e7bb0d3788f6f21b4073d06542a043",
    "teletextimager/font-etsi/G0_el.pil": "ec1af672eb4992baa2294af1991cbbe23f09f3d83324662e4920a6a493053e46",
    "teletextimager/font-etsi/G0_he.pbm": "475f80796607d398d9532e4ff6fbcc14d73c0f08ea2081c63c95aa93fa336a78",
    "teletextimager/font-etsi/G0_he.pil": "ec1af672eb4992baa2294af1991cbbe23f09f3d83324662e4920a6a493053e46",
    "teletextimager/font-etsi/G0_latin.pbm": "acf6fcd224b15d054a7616dd60158df7615f0bba01480aa4453d08877c18569e",
    "teletextimager/font-etsi/G0_latin.pil": "ec1af672eb4992baa2294af1991cbbe23f09f3d83324662e4920a6a493053e46",
    "teletextimager/font-etsi/G0_reduced.pbm": "7a2f54b2735e9e64c71987318f674fa6689e0d61f7123c4ae82742dbbd1b747d",
    "teletextimager/font-etsi/G0_reduced.pil": "7e6ac69478de906d7fe2228f994969b078de878ca4cae13c9fbc33e81411f4b9",
    "teletextimager/font-etsi/G0_ru_bg.pbm": "437524c3f1b2bcd200e16d84ca66d64ff256a69052a0d856f001a191392e2638",
    "teletextimager/font-etsi/G0_ru_bg.pil": "ec1af672eb4992baa2294af1991cbbe23f09f3d83324662e4920a6a493053e46",
    "teletextimager/font-etsi/G0_sr_hr.pbm": "35911b35da7d1baaf7c7678f4e9c4dd1fb6f654c8b24135dc987e492eafe8e41",
    "teletextimager/font-etsi/G0_sr_hr.pil": "ec1af672eb4992baa2294af1991cbbe23f09f3d83324662e4920a6a493053e46",
    "teletextimager/font-etsi/G0_uk.pbm": "19e1770e2be80ae9429288bd8e068ad97b3c56a07961ea9ef9da2b58d0442be0",
    "teletextimager/font-etsi/G0_uk.pil": "ec1af672eb4992baa2294af1991cbbe23f09f3d83324662e4920a6a493053e46",
    "teletextimager/font-etsi/G1_con.pbm": "f5e68fc6bb5eff5cdfeb0982a5e64850c48e7d5c39482076419246806af843ef",
    "teletextimager/font-etsi/G1_con.pil": "899f7da4e825c2a01e90fa6724f3af6f27a12f5c26f29ad0a6ec6407db71ea6f",
    "teletextimager/font-etsi/G1_sep.pbm": "bdb308d55e9d8c8123974693f9c595879cc001197bc1b414f54679c4c7f38fbc",
    "teletextimager/font-etsi/G1_sep.pil": "899f7da4e825c2a01e90fa6724f3af6f27a12f5c26f29ad0a6ec6407db71ea6f",
    "teletextimager/font-etsi/G2_ar.pbm": "364059fe55eb7312d82712fe1dfd3eafc8498ef226cc3546e3fb119246e7b036",
    "teletextimager/font-etsi/G2_ar.pil": "ec1af672eb4992baa2294af1991cbbe23f09f3d83324662e4920a6a493053e46",
    "teletextimager/font-etsi/G2_cyr.pbm": "196c7244528fb2d9a344d4fb091b831ebf0e8e44ce009e69004d8906f166dd8a",
    "teletextimager/font-etsi/G2_cyr.pil": "ec1af672eb4992baa2294af1991cbbe23f09f3d83324662e4920a6a493053e46",
    "teletextimager/font-etsi/G2_el.pbm": "2341088cb74ef5df3d5bbc6dc36cfab4f73bc31c0d89f4ee57a90d7feb400c37",
    "teletextimager/font-etsi/G2_el.pil": "ec1af672eb4992baa2294af1991cbbe23f09f3d83324662e4920a6a493053e46",
    "teletextimager/font-etsi/G2_latin.pbm": "c21978b0c9d99020191de1dd6f07f5551d2acbb83728f4cc442b9250a067808e",
    "teletextimager/font-etsi/G2_latin.pil": "ec1af672eb4992baa2294af1991cbbe23f09f3d83324662e4920a6a493053e46",
    "teletextimager/font-etsi/G3.pbm": "a79bf6bdcffb53109002b668ae4756e84733f0053f0695fc70492555722d953c",
    "teletextimager/font-etsi/G3.pil": "ec1af672eb4992baa2294af1991cbbe23f09f3d83324662e4920a6a493053e46"
}


def read_bounded(path, maximum):
    with path.open("rb") as source:
        data = source.read(maximum + 1)
    if len(data) > maximum:
        raise ValueError("source file exceeds import bound")
    return data


def png_rows(data):
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError("font atlas is not PNG")
    position, compressed, dimensions = 8, bytearray(), None
    while position < len(data):
        if position + 12 > len(data):
            raise ValueError("truncated PNG chunk")
        length = int.from_bytes(data[position:position + 4], "big")
        kind = data[position + 4:position + 8]
        payload = data[position + 8:position + 8 + length]
        end = position + 12 + length
        if end > len(data):
            raise ValueError("truncated PNG payload")
        crc = int.from_bytes(data[end - 4:end], "big")
        if zlib.crc32(kind + payload) != crc:
            raise ValueError("PNG checksum mismatch")
        position = end
        if kind == b"IHDR":
            width, height, depth, colour, compression, filtering, interlace = struct.unpack(
                ">IIBBBBB", payload
            )
            if not (0 < width <= 792 and height in (20, 40)):
                raise ValueError("unexpected atlas dimensions")
            if (depth, colour, compression, filtering, interlace) != (1, 0, 0, 0, 0):
                raise ValueError("unexpected atlas format")
            dimensions = width, height
        elif kind == b"IDAT":
            compressed.extend(payload)
        elif kind == b"IEND":
            if position != len(data):
                raise ValueError("trailing PNG bytes")
            break
    if dimensions is None:
        raise ValueError("missing atlas dimensions")
    width, height = dimensions
    stride = (width + 7) // 8
    expected = height * (stride + 1)
    inflater = zlib.decompressobj()
    raw = inflater.decompress(compressed, expected + 1)
    if len(raw) != expected or not inflater.eof or inflater.unused_data:
        raise ValueError("unexpected decompressed atlas extent")
    rows, previous = [], bytes(stride)
    for y in range(height):
        offset = y * (stride + 1)
        filtering = raw[offset]
        row = list(raw[offset + 1:offset + stride + 1])
        for x, value in enumerate(row):
            left, above = row[x - 1] if x else 0, previous[x]
            diagonal = previous[x - 1] if x else 0
            if filtering == 1:
                value += left
            elif filtering == 2:
                value += above
            elif filtering == 3:
                value += (left + above) // 2
            elif filtering == 4:
                prediction = left + above - diagonal
                distances = [abs(prediction - item) for item in (left, above, diagonal)]
                value += (left, above, diagonal)[distances.index(min(distances))]
            elif filtering != 0:
                raise ValueError("unknown PNG row filter")
            row[x] = value & 255
        previous = bytes(row)
        rows.append(previous)
    if any(rows[y] != rows[y + 1] for y in range(0, height, 2)):
        raise ValueError("atlas rows cannot be losslessly reduced")
    return width, height, rows


def extract(metrics, image):
    prefix = b"PILfont\n;;;;;;20;\nDATA\n"
    if not metrics.startswith(prefix) or len(metrics) != len(prefix) + 256 * 20:
        raise ValueError("unexpected fixed PILfont metrics")
    width, height, rows = png_rows(image)
    output = bytearray()
    for code in range(0x20, 0x80):
        offset = len(prefix) + code * 20
        entry = struct.unpack(">10h", metrics[offset:offset + 20])
        if entry == (0,) * 10:
            output.extend(bytes(20))
            continue
        advance_x, advance_y, dx0, dy0, dx1, dy1, x0, y0, x1, y1 = entry
        if (advance_x, advance_y, dx0, dy0, dx1, dy1) != (12, 0, 0, -16, 12, 4):
            raise ValueError("unexpected character extent")
        if not (x1 - x0 == 12 and y1 - y0 == 20 and 0 <= x0 < x1 <= width
                and 0 <= y0 < y1 <= height and y0 % 2 == 0):
            raise ValueError("character lies outside fixed atlas")
        for y in range(y0, y1, 2):
            mask = 0
            for x in range(x0, x1):
                mask = (mask << 1) | ((rows[y][x // 8] >> (7 - x % 8)) & 1)
            output.extend(struct.pack("<H", mask))
    return output


def produce(source):
    head = read_bounded(source / ".git/HEAD", 128).decode("ascii").strip()
    if head != COMMIT:
        raise ValueError("requires an ordinary detached checkout at the pinned commit")
    inputs = {}
    for name, expected in SOURCE_SHA256.items():
        data = read_bounded(source / name, 65536)
        if hashlib.sha256(data).hexdigest() != expected:
            raise ValueError("pinned source hash mismatch: " + name)
        inputs[name] = data
    masks = bytearray()
    for name in FONT_NAMES:
        prefix = "teletextimager/font-etsi/" + name
        masks.extend(extract(inputs[prefix + ".pil"], inputs[prefix + ".pbm"]))
    masks = bytes(masks)
    manifest = {
        "schema": 1,
        "upstream": "https://github.com/gkthemac/teletextimager",
        "version": "0.2.0",
        "commit": COMMIT,
        "license": "MIT",
        "format": "28 tables x 96 codes (0x20..0x7f) x 10 little-endian u16 rows; bit11 left",
        "tables": list(FONT_NAMES),
        "source_sha256": SOURCE_SHA256,
        "mask_sha256": hashlib.sha256(masks).hexdigest(),
    }
    return {
        "masks.bin": masks,
        "manifest.json": (json.dumps(manifest, sort_keys=True, indent=2) + "\n").encode(),
        "LICENSE": inputs["LICENSE"],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", required=True, type=Path)
    parser.add_argument("--output-dir", required=True, type=Path)
    parser.add_argument("--check", action="store_true", help="compare without modifying outputs")
    args = parser.parse_args()
    try:
        outputs = produce(args.source)
        if args.check:
            for name, data in outputs.items():
                if read_bounded(args.output_dir / name, 65536) != data:
                    raise ValueError("generated output differs: " + name)
        else:
            args.output_dir.mkdir(parents=True, exist_ok=True)
            for name, data in outputs.items():
                (args.output_dir / name).write_bytes(data)
    except (OSError, ValueError, struct.error, zlib.error, UnicodeError) as error:
        print("teletext font import rejected: " + str(error), file=sys.stderr)
        return 1
    print(json.dumps({"tables": len(FONT_NAMES), "mask_bytes": len(outputs["masks.bin"]),
                      "mask_sha256": hashlib.sha256(outputs["masks.bin"]).hexdigest()}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
