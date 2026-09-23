# Run repository tests

Use the repository wrappers for local Rust tests. Agent hooks reject direct
`cargo test` commands so focused runs and closeout cannot silently use different
test sets.

## Run a focused Rust test

Select the narrowest crate and filter that prove the change:

```bash
./run.sh test -p <crate>
./run.sh test -p <crate> <test-filter>
```

The checked-in nextest profile runs through all failures, performs no retries,
and reports tests that exceed 60 seconds.

For ignored integration tests that depend on local tooling or large fixtures:

```bash
./run.sh test -p <crate> --run-ignored ignored-only
```

Supply any documented environment-gated fixture or database setting before the
command. Never place private paths, credentials, or copyrighted media in the
repository.

## Run browser workflows

Use the Playwright wrapper for the real web host and hydrated bundle:

```bash
scripts/e2e/playwright-e2e.sh
```

Pass a focused spec through the wrapper while iterating:

```bash
scripts/e2e/playwright-e2e.sh tests/e2e/<feature>.spec.ts
```

The command is successful only when the requested spec reports no failed test
and the wrapper exits zero. The suite runs serially by default because its
specs share a seeded database.

## Run playback gates

Use the runtime smoke test for basic IPTV and HLS startup:

```bash
cargo xtask smoke
```

Use the manual TSDuck gate when transport continuity, PCR/PTS behavior, PSI, or
audio/video PIDs are part of the change:

```bash
scripts/e2e/tsduck-playback-gate.sh gate
```

### Check Next HTTP playback

Set `ETV_NEXT_FOLDER` to the directory containing `ersatztv-channel` (include
`target/release` when using a locally built Next checkout). Provide the FFmpeg 9
and ffprobe paths through the normal `FFMPEG_PATH` and `FFPROBE_PATH` overrides.
The CI job provisions these using the production runtime manager before testing.
Then run:

```bash
cargo xtask smoke --next --keep-temp
```

This uses a temporary database and generated local media. It checks Next HLS
playlists and a served segment, MPEG-TS refusal, the separate browser-preview
playlist/segment, and idle removal of both session folders. `--next` implies the
lifecycle check and cannot be combined with `--skip-hls`. The default native smoke
remains `cargo xtask smoke`. `ERSATZRS_SMOKE_PORT` selects an unused loopback port
(default 8409); an occupied port fails without touching the existing server.
Use a dedicated `TMPDIR` on Unix when running beside another playback test.
These checks exercise transport, not admin hydration or cross-platform playback.
The CI workflow has a dedicated **Next engine** job for this gate and the decoded
track-selection tests below.

### Check the optional Next process boundary

Set `ERSATZRS_NEXT_TEST_BIN` to the Next channel executable,
`ERSATZRS_NEXT_TEST_FFMPEG` to the FFmpeg 9 executable, and
`ERSATZRS_NEXT_TEST_FFPROBE` to its matching ffprobe executable. These are
test-only variables; use executable paths, not source or installation folders.
On Windows, include the `.exe` filename.

```bash
./run.sh test -p ersatzrs-infra next_engine::process::runtime_tests::real_next_hls_preserves_selected_audio_and_burns_external_subtitles --run-ignored ignored-only
```

Embedded bitmap selection has a separate synthetic PGS regression:

```bash
./run.sh test -p ersatzrs-infra real_next_hls_burns_preferred_embedded_bitmap_and_respects_subtitles_off --run-ignored ignored-only
```

It generates two embedded image tracks: default English draws a left-hand marker,
preferred Dutch draws a right-hand marker. Decoded HLS must show only the selected
marker; an explicit subtitles-off pin must show neither while retaining Dutch
audio. No downloaded subtitle media is needed. The dedicated Next CI job includes
this test through its existing `next_engine::` filter. Next starts a loopback
proxy even for local media, so these runtime tests require local socket access.

The external-subtitle test creates disposable blue video with 440-Hz and 880-Hz audio tracks and
an external subtitle. It publishes the playout through the real adapter, starts
Next with profile-derived configuration, waits for readiness, decodes a generated
HLS segment and checks the selected 880-Hz audio and visible subtitle glyphs.
Schedule items and media-file bindings are read from an isolated migrated
database under scheduler locks; the per-channel publisher reloads channel
preferences, runs native track selection and atomically publishes on its blocking
worker. Separate coordinator regressions cover queued reads, retained old
publications, empty schedules and cancellation before/during filesystem work.
A second startup uses an exact silent/subtitles-off pin and verifies
decoded silence and the absence of glyphs. The production Next host reads the persisted profile and schedule. The test
first occupies its only shared transcode slot and proves no process/output
starts; releasing the slot admits Next. A duplicate request reuses the session.
Each process is touched and stopped through the shared registry; process/output
cleanup releases capacity and removes the handle. Its playlist is unavailable
after stop. A simultaneous normal/preview phase uses a stored AC3 profile:
normal output remains AC3, preview output is AAC, both retain selected audio and
subtitles, and stopping preview releases only its own capacity/output. The stored
profile remains unchanged. A registry regression covers rebuild notifications
without a normal session. Separate session regressions use portable child fixtures and a test clock
for idle expiry, startup deadlines, refresh responsiveness and terminal states.
It uses no personal library or production database.
This is engine-boundary proof; it does not replace web endpoint, authentication,
scheduling, native smoke or platform acceptance gates.

## Close out the Rust test suite

Run the canonical local closeout once after focused tests pass:

```bash
./run.sh test-closeout
```

The wrapper runs workspace unit, integration, and binary tests through nextest,
then uses Cargo only for doctests.
