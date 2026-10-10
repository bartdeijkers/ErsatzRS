# Synthetic teletext fixture

`teletext.ts` contains original, redistributable synthetic text, not NOS or
operator media. The fixture data is dedicated to the public domain under
CC0-1.0; the generator uses the repository's normal source license.

Regenerate it from the repository root:

```bash
python3 scripts/calibration/generate-teletext-fixture.py /tmp/teletext.ts
cmp /tmp/teletext.ts crates/ersatzrs-ffmpeg/src/fixtures/teletext.ts
```

Run the bounded functional probe against an explicitly selected package:

```bash
ERSATZRS_TELETEXT_TEST_FFMPEG="<exact-package-ffmpeg-executable>" \
  ./run.sh test -p ersatzrs-ffmpeg real_runtime_decodes_exact_pages \
  --run-ignored ignored-only
```

Record the package manifest and archive/executable digests before this check.
For ARM testing under QEMU, the test wrapper must `exec` the selected emulator
and exact package executable, using a matching target-architecture sysroot with
the host glibc/libgcc baseline. Report emulated execution separately from native
platform proof; do not change the production runtime selection to the wrapper.

The teletext-only MPEG-TS has programme 1, PMT PID 4096 and teletext/PCR PID
256. Its descriptor distinguishes initial page 200 and subtitle page 888.
Page 201 is an independently transmitted ordinary page. Eight cycles exercise
explicit page selection, full-page cyan/green text and boxed subtitles.
PCR establishes the clock, and page headers precede body rows by 40 ms.

The fixture producer is independent of the future runtime authoring encoder.
Use FFmpeg/libzvbi for text/bitmap decoding and TSDuck for descriptor/signalling
validation. It is a small decoder probe, not audiovisual timing, all languages,
subpage navigation, client compatibility or full Phase 9 acceptance evidence.
