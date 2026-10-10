# Synthetic teletext fixture

`teletext.ts` contains original, redistributable synthetic text, not NOS or
operator media. The fixture data is dedicated to the public domain under
CC0-1.0; the generator uses the repository's normal source license.

Regenerate it from the repository root:

```bash
python3 scripts/calibration/generate-teletext-fixture.py /tmp/teletext.ts
cmp /tmp/teletext.ts crates/ersatzrs-ffmpeg/src/fixtures/teletext.ts
```

The teletext-only MPEG-TS has programme 1, PMT PID 4096 and teletext/PCR PID
256. Its descriptor distinguishes initial page 200 and subtitle page 888.
Page 201 is an independently transmitted ordinary page. Eight cycles exercise
explicit page selection, full-page cyan/green text and boxed subtitles.
PCR establishes the clock, and page headers precede body rows by 40 ms.

The fixture producer is independent of the future runtime authoring encoder.
Use FFmpeg/libzvbi for text/bitmap decoding and TSDuck for descriptor/signalling
validation. It is a small decoder probe, not audiovisual timing, all languages,
subpage navigation, client compatibility or full Phase 9 acceptance evidence.
