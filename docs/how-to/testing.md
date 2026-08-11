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

## Close out the Rust test suite

Run the canonical local closeout once after focused tests pass:

```bash
./run.sh test-closeout
```

The wrapper runs workspace unit, integration, and binary tests through nextest,
then uses Cargo only for doctests.
