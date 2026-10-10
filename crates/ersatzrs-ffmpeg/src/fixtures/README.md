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

The test generates six seconds of MPEG-2 video and two AAC audio streams in
programs 10/11 at 5 Mbit/s. Bounded source discovery finds the selected program's
PAT-advertised PMT; the actual installer grows its single-packet table to two
packets using null capacity for 33 synthetic authored pages. Four complete page
cycles and repeated table counter wraps exercise installation and PES delivery. It preserves the
other program's PMT and each original AV/PCR/PAT/SDT packet at its original
position, then compares decoded video and both audio frame hashes/timestamps. FFmpeg verifies the
page/caption text; TSDuck verifies the service language/types, PES identity,
PCR-derived rate and continuity/timestamp integrity. Optional
`ERSATZRS_TELETEXT_TEST_MULTIPLEX_ARTIFACT="<synthetic-output-ts-path>"` saves
this stream. `teletext::psi_tests` separately covers fragmented large PMTs,
split headers, multiple sections per packet, full PAT/CAT completeness, all
advertised program PID references, malformed input and version/continuity
fencing. `teletext::pmt_install_tests` covers fragmented large tables, local
capacity, persistent input/output continuity, duplicate/multiple sections and
atomic preflight failure. `teletext::psi_monitor_tests` covers continuing
source/table/PID fencing, held partial sections, inherited continuity, duplicate
validation and permanent failure/EOF. The real fixture seals initial discovery
and feeds subsequent source packets through this monitor before installation.
`teletext::clock_tests` covers selected-PCR syntax, ordering/wrap, physical
all-PID packet distances, configured rate/gap limits and sticky failure. The
real fixture checks identical PCR/CBR evidence before and after installation
and detects a removed null slot. Its 1,000 ppm, 14-tick interval-jitter and
40 ms/134-packet PCR gap limits are explicit fixture policy, not production
defaults. Neither checker measures wall-clock delivery or owns held output.
`teletext::buffer_tests` exercises bounded arbitrary byte reads, partial tails,
first-byte age, monotonic time, prefix release, idle deadlines and permanent
failure. `teletext::output::tests` checks composed source/clock admission,
physical interval boundaries, atomic preparation, total raw/prepared capacity,
immutable schedules, epoch-bound acknowledgements, delivery expiry and strict
EOF, including absolute fractional packet timing, capacity queries and the
earliest deadline before preparation. `teletext::output::tail_tests` additionally
checks actual EOF sealing, complete fractional tails, acknowledged cycle evidence,
terminal partial TS/PSI/clock failures and PMT growth without tail capacity. The additional
ignored `real_runtime_output_gate_produces_valid_preserved_prefix`
uses bounded incremental input and simulated monotonic sink acknowledgement
for two complete 33-page cycles. AV/PCR/other-program packets and decoded video/
both audio hashes/timestamps remain identical; independent teletext decoding
and TSDuck signalling/continuity checks pass on exact Linux x64/Windows and
emulated ARM64/ARMv7 runtimes. Its 1,024-packet retention, 500 ms hold, 100 ms
playout delay and 20 ms lateness are explicit fixture policy. Its acknowledgement
is simulated. The web host's `transport/iptv/teletext_output_tests.rs` separately
tests actual asynchronous forwarding against an independently serialized
synthetic CBR source. Twenty tests cover absolute packet pacing, fragmented
reads, short writes, source/sink failures, read/write/flush deadlines and
cancellation. Real Tokio duplex streams additionally prove bounded backpressure
recovery and cancellation during a blocked write. This establishes writer
acceptance and local pacing. `transport/iptv/teletext_binding_tests.rs` checks
immutable binding validation, explicit final/internal/HLS/container/slate roles
and actual-format admission. Unix owned-child tests in
`transport/iptv/stream_teletext_tests.rs` exercise the actual stdout supervisor,
fresh gates on zero-output hardware fallback, cancellation/reaping and visible
body failures. Paired-producer regressions preserve cleanup ownership across
cancelled waits and retain uncertain cleanup failures. Temporary subtitle files
are preserved when consuming-process termination cannot be confirmed. These
synthetic shell/duplex tests establish supervisor behavior, not packaged client
receipt or the complete native/Next output matrix. Finite EOF tests retain exact
source extent and physical packet times, reject incomplete TS/PSI and exercise
tail-only write/flush faults, deadlines and cancellation. Immutable repeated
headers omit C4 erase; first-cycle and replacement headers still erase. The
ignored `real_runtime_output_gate_drains_finite_source_without_changing_av`
composes the complete natural FFmpeg EOF with the same AV/PCR, decoder and
TSDuck checks, using simulated acknowledgement. The retained web forwarder
uses one deferred-start gate for reserve, original arrival age, refill and actual
paced write/flush acknowledgement. Explicit policy admission bounds requested
allocation payload and checks actual muxrate; first-byte and empty-source
deadlines prevent indefinitely retained sessions. Unix retained-response tests
exercise raw child ownership, single final injection, body failure after reaping,
producer-completion waiting and cancellation. Deferred hardware-retry errors
retain cleanup uncertainty. Two ignored `real_runtime_retained_*` web tests use
the same exact FFmpeg/TSDuck environment variables for six-second short EOF and
nine-second reserve/refill through actual Tokio duplex writes:

```bash
ERSATZRS_TELETEXT_TEST_FFMPEG="<exact-package-ffmpeg-executable>" \
ERSATZRS_TELETEXT_TEST_TSANALYZE="<tsanalyze-executable>" \
  ./run.sh test -p ersatzrs-domain -p ersatzrs-ffmpeg -p ersatzrs-web \
  -E 'test(real_runtime_retained_)' --run-ignored ignored-only
```

They compare
source packet positions, decoded video/both audio hashes/timestamps and source
subtitle page 888, decode authored page 200 and validate TSDuck signalling.
These are synthetic borrowed-output and Unix process tests; packaged client
receipt and the complete native/Next source/output matrix remain separate gates.
Slate/programme handover, production policies,
automatic publication selection and full Phase 9 acceptance remain open.

The `real_legacy_wrapper_keeps_one_authored_epoch_across_two_distinct_programmes`
fixture serves distinct finite programmes through chunked internal HTTP bodies.
It uses the production looping concat playlist/command and final stdout
supervisor, with a fixture-only six-second output limit. One authored binding
remains on the final wrapper while plain internal responses reopen A, B and A.
Decoded video and both English/Dutch audio tracks must contain both programmes;
distinct source captions on page 888 and authored page 200 must remain readable.
Clock and TSDuck checks cover PCR rate, PSI/PIDs, language, PES and continuity.
The fixture retains the production input readrate. Host pacing reads bounded
lookahead from the same stdout during packet waits, writes and flushes; raw and
pending bytes share capacity and retain their first-arrival age. Sink deadlines
remain unchanged, and observed EOF is sealed after pending acknowledgement.

```bash
ERSATZRS_TELETEXT_TEST_FFMPEG="<exact-package-ffmpeg-executable>" \
ERSATZRS_TELETEXT_TEST_TSANALYZE="<tsanalyze-executable>" \
  ./run.sh test -p ersatzrs-ffmpeg -p ersatzrs-web \
  -E 'test(real_legacy_wrapper_)' --run-ignored ignored-only
```

This requires permission to bind temporary loopback HTTP. It checks content
survival through the existing wrapper, not sample-exact caption handover,
full routed admission, capacity-slate transitions, Next or native packaged
client acceptance. The source programmes and temporary server contain only
original synthetic material; no runtime database or installed service is used.
The fixture supplies its own item transport streams. The ordinary host item
builder selects one audio track and does not yet map generic source DVB teletext
passthrough; wrapper preservation cannot restore tracks omitted upstream.
