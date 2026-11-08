# Native text subtitle preparation

Native MPEG-TS, HLS segmenter and playback troubleshooting normalize ordinary
burned text subtitles to UTF-8. SRT, ASS/SSA and WebVTT use the same decoder.
Source files are read only; prepared files live in the subtitle seek cache.
Publication-verified retained subtitles keep strict Unicode validation.
Next subtitle delivery has its own source-bound preparation contract.

## Character-set selection

Selection uses this precedence:

1. UTF-8 or UTF-16 BOM, decoded strictly. A conflicting declaration is an error.
2. An explicit character-set declaration.
3. Valid UTF-8 without a BOM.
4. Assessed legacy detection when at least four non-ASCII bytes are present.

Detection is a heuristic. It cannot establish the intended encoding for every
source. An unassessed or short legacy source needs a declaration. Decoder
errors and binary control characters are refused; tabs and line endings are
allowed. Conversion never introduces replacement characters; literal glyphs
already present in valid Unicode are preserved. ISO-2022-JP needs an explicit declaration.

## Explicit declarations

For a local source named `example.nl.srt`, put a UTF-8 text file named
`example.nl.srt.charset` beside it. Its complete contents are one encoding label,
for example:

```text
windows-1252
```

Labels are case-insensitive and surrounding whitespace is ignored. Common labels
include `utf-8`, `windows-1252`, `windows-1251`, `shift_jis` and `iso-2022-jp`.
The supported set is the Encoding Standard's labels implemented by `encoding_rs`;
unknown and replacement encodings fail. `latin1` follows that standard's mapping
to Windows-1252. Do not use it to declare a different character set.

For HTTP(S), a single `charset` parameter on `Content-Type` declares the source
encoding, for example `application/x-subrip; charset=windows-1252`. Invalid,
duplicate and BOM-conflicting declarations fail. HTTP credentials and source
URLs are not included in preparation diagnostics.

Changing the source bytes or the resolved encoding changes the cache identity.
Equivalent aliases for the same encoding share an identity. Starting at zero
and seeking use the same normalized text; cues spanning a seek are retained
for their remaining display duration.

## Limits and failure behavior

| Work | Limit |
| --- | --- |
| Source and decoded text | 8 MiB each |
| Local `.charset` companion | 128 bytes |
| Source retrieval | 20 seconds |
| FFmpeg syntax compatibility | 10 seconds |
| Health local preflight | Up to 256 source/file pairs, 5 seconds |
| Health scheduling window | Active and upcoming native programmes within 24 hours |

Health never fetches remote subtitles. A limited check reports unchecked sources
instead of claiming complete coverage. Sources beyond its window are checked at
native playback admission. Invalid preparation reaches the HLS error-slate path,
an explicit MPEG-TS request error, or the troubleshooting result. It never
silently disables a selected subtitle or passes undecodable bytes to the burner.
