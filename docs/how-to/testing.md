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

### Check the optional Next process boundary

Set `ERSATZRS_NEXT_TEST_BIN` to the Next channel executable,
`ERSATZRS_NEXT_TEST_FFMPEG` to the FFmpeg 9 executable, and
`ERSATZRS_NEXT_TEST_FFPROBE` to its matching ffprobe executable. These are
test-only variables; use executable paths, not source or installation folders.
On Windows, include the `.exe` filename.

```bash
./run.sh test -p ersatzrs-infra next_engine::process::runtime_tests::real_next_hls_preserves_selected_audio_and_burns_external_subtitles --run-ignored ignored-only
```

The test creates disposable blue video with 440-Hz and 880-Hz audio tracks and
an external subtitle. It publishes the playout through the real adapter, starts
Next with profile-derived configuration, waits for readiness, decodes a generated
HLS segment and checks the selected 880-Hz audio and visible subtitle glyphs.
Schedule items and media-file bindings are read from an isolated migrated
database under scheduler locks; the per-channel publisher reloads channel
preferences, runs native track selection and atomically publishes on its blocking
worker. Separate coordinator regressions cover queued reads, retained old
publications, empty schedules and cancellation before/during filesystem work.
A second startup uses an exact silent/subtitles-off pin and verifies
decoded silence and the absence of glyphs. Each process is supervised through
the Next HLS handle, registered before startup, touched and stopped through the
shared registry, then removed after process cleanup. Its playlist is unavailable
after stop. Separate session regressions use portable child fixtures and a test clock
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
