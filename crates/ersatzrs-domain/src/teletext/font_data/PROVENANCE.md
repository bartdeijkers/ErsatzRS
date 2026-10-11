# Teletext glyph bitmap provenance

These masks are derived only from the font assets in
[teletextimager](https://github.com/gkthemac/teletextimager), version `0.2.0`,
commit `2b0125b778c45ffdfa2df0b196870c093361f317`. The upstream MIT notice is
retained in [LICENSE](LICENSE). No upstream decoder or renderer was ported.
The package explicitly includes `font-etsi/*`; no separate font-license
exception is present at this pin.

`manifest.json` records each upstream PNG/PILfont input hash, the exact package
metadata/license hashes, table order and generated binary hash. Despite their
`.pbm` names, upstream images are one-bit PNG atlases. Every adjacent image-row
pair is identical. The importer verifies that property before losslessly
reducing each twelve-by-twenty glyph to twelve by ten.

`masks.bin` contains 28 tables, each with 96 code slots (`0x20` through `0x7f`),
each containing ten little-endian `u16` rows. Bit eleven is the left pixel.
Unused atlas slots remain zero bytes; a zero bitmap does not prove assignment,
SPACE semantics or permission to transmit. The checked lookup independently
rejects prohibited/unmarked positions. The final table contains only reduced
Latin capitals and must not substitute for non-Latin capital shapes.

Reproduce from an ordinary detached upstream checkout without installing its
package or Pillow:

```sh
python3 scripts/teletext/import_glyph_masks.py \
  --source "$TELETEXTIMAGER_SRC" \
  --output-dir crates/ersatzrs-domain/src/teletext/font_data
python3 scripts/teletext/import_glyph_masks.py \
  --source "$TELETEXTIMAGER_SRC" \
  --output-dir crates/ersatzrs-domain/src/teletext/font_data --check
```

The importer rejects another HEAD or changed input bytes before producing any
output, uses only Python's standard library, and performs no subprocesses,
network calls or runtime acquisition. Artifact generation is a maintainer
operation; production uses embedded immutable bytes on all supported targets.

Lookup does not establish complete display readiness. Non-Latin/mixed-pair
composite shaping remains an explicit error until independently established.
G1 alphabetic positions use current G0 through the original checked identity;
they are not taken from the G1 mosaic table. Mosaics use separate contiguous
and separated masks. The cell renderer decides underline/flash/colour/visibility
and applies the validated C.3 fragment transforms. `with_underline()` chooses
the full bottom mask row as this font's underline policy, before enlargement.

Base authored Unicode must retain Base priority when associated with its exact
legacy Latin replacement mask; generated compatibility words must not become
a higher-priority Local candidate. Stream and browser readiness still require
the same admitted immutable service, complete semantic validation and wire/
independent-client evidence.
