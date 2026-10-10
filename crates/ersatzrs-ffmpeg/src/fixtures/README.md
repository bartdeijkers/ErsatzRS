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

The fixture producer is independent of the runtime authoring encoder.
Use FFmpeg/libzvbi for text/bitmap decoding and TSDuck for descriptor/signalling
validation. It is a small decoder probe, not audiovisual timing, all languages,
subpage navigation, client compatibility or full Phase 9 acceptance evidence.

Probe engine-authored packets using the independent fixture transport:

```bash
ERSATZRS_TELETEXT_TEST_FFMPEG="<exact-package-ffmpeg-executable>" \
ERSATZRS_TELETEXT_TEST_PYTHON="<python3-executable>" \
  ./run.sh test -p ersatzrs-ffmpeg real_runtime_decodes_authored \
  --run-ignored ignored-only
```

`scripts/calibration/wrap-authored-teletext.py` preserves the encoder's supplied
data units and supplies only calibration PES/TS timing and signalling. The
bounded test verifies accented Unicode, all seven Western Latin national
repertoires with default decoder settings, boxed subtitles and cyan/green
bitmap output. This does not validate production multiplexing, editorial-link
CRC, older clients or the remaining character regions.

Check the serial carousel and PES/TS packetizer against the selected decoder:

```bash
ERSATZRS_TELETEXT_TEST_FFMPEG="<exact-package-ffmpeg-executable>" \
ERSATZRS_TELETEXT_TEST_PYTHON="<python3-executable>" \
  ./run.sh test -p ersatzrs-ffmpeg real_runtime_decodes_carousel \
  --run-ignored ignored-only
```

The wrapper's `--packetized` mode supplies only calibration PAT/PMT/PCR and
retains the Rust-generated PES/TS packets. An optional
`ERSATZRS_TELETEXT_TEST_ARTIFACT="<synthetic-output-ts-path>"` preserves this
synthetic stream for inspection. Run the independent TSDuck regression:

```bash
ERSATZRS_TELETEXT_TEST_TSANALYZE="<tsanalyze-executable>" \
ERSATZRS_TELETEXT_TEST_PYTHON="<python3-executable>" \
  ./run.sh test -p ersatzrs-ffmpeg real_tsduck_checks \
  --run-ignored ignored-only
```

It checks PES identity, continuity, timestamp leaps, sync and referenced PID
structure, then proves continuity damage is detected. Calibration PSI/PCR is
independent fixture code; the test does not prove production PMT rewriting,
null-capacity reservation, actual decoder-buffer pacing, programme preservation
or the audiovisual playback gate.

Verify PMT signalling and null-slot replacement against real synthetic AV:

```bash
ERSATZRS_TELETEXT_TEST_FFMPEG="<exact-package-ffmpeg-executable>" \
ERSATZRS_TELETEXT_TEST_TSANALYZE="<tsanalyze-executable>" \
  ./run.sh test -p ersatzrs-ffmpeg real_runtime_adds_signalled \
  --run-ignored ignored-only
```

The test generates one second of MPEG-2 video/AAC audio at 5 Mbit/s, rewrites
its small single-packet PMT and replaces available null slots with four authored
page cycles. It preserves each original AV/PCR/PAT/SDT packet at its original
position and compares decoded AV frame hashes/timestamps. FFmpeg verifies the
page/caption text; TSDuck verifies the service language/types, PES identity,
PCR-derived rate and continuity/timestamp integrity. Optional
`ERSATZRS_TELETEXT_TEST_MULTIPLEX_ARTIFACT="<synthetic-output-ts-path>"` saves
this stream. This finite fixture does not prove input PSI assembly, all-program
PID ownership, growing-PMT installation, asynchronous output pacing/buffer/rate
admission, live application integration or full Phase 9 acceptance.
