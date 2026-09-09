# ADR 0090: Run CI Tests Through Nextest With Per-Test Timings

## Status

Accepted

## Implementation

Implemented — `.github/workflows/ci.yml` (`test` and `test_hosted` jobs) and
the `ci` profile in `.config/nextest.toml`.

## Context

Local development has run on nextest for some time: `./run.sh test` and
`./run.sh test-closeout` are the required runners, agent hooks block direct
`cargo test` invocations, and `.config/nextest.toml` disables retries, runs through
failures, and reports tests slower than 60 seconds.

CI did not follow. BUILD.md, TEST.md, and AGENTS.md each stated that "CI
intentionally remains on its direct `cargo test --workspace --lib --bins --tests`
selection". That sentence entered the tree in `fd98c527`
("refactor(db): standardize runtime on SQLite") as a note on what that commit
did *not* change; nextest has never appeared in `ci.yml`. No ADR, issue, or
commit records a reason for keeping the two runners apart, so the wording
described a scope boundary that later read as a decision.

The split has a measurable cost. The `test` job grew from ~12.5 minutes in late
August to ~26.6 minutes on 2026-09-09, and the full CI run from ~31 to ~48
minutes. `cargo test` reports only a per-binary total, so attributing that growth
means reading raw log timestamps and guessing across parallel threads — the
slowest names looked like the recent add-on media list work, but nothing in CI
output could confirm it. The information needed to act already exists in the
local profile and was simply not being produced by CI.

## Decision

Run the CI test jobs through nextest, with the same target selection as before.

- `test` runs `cargo nextest run --profile ci --workspace --lib --bins
  --tests`. The selection is unchanged, so doctest coverage is unchanged too:
  neither the old nor the new command runs doctests. `./run.sh test-closeout`
  remains the only path that adds them.
- The `ci` profile keeps the default semantics (no retries, run through
  failures, slow tests reported after 60 seconds) and adds a JUnit report,
  uploaded as a build artifact with a 14-day retention. The report carries a
  duration per test, which makes a runtime regression attributable to
  individual tests instead of one wall-clock number.
- The `test_hosted` shard matrix uses the same runner and profile, so the
  manual GitHub-hosted fallback cannot drift from the self-hosted path.

While aligning the two paths, `web-bins` was found to run only
`-p ersatzrs-web --bins`, leaving the crate's `tests/*.rs` integration targets
unrun on the hosted path even though the self-hosted `test` job covers them
through `--tests`. That shard now runs `--bins --tests`.

## Consequences

CI reports which tests are slow, in the run summary and as a machine-readable
artifact, so the growth in test time can be attributed and fixed rather than
absorbed. The two runners no longer disagree, so a test that passes locally and
fails in CI is a real difference rather than a runner artifact.

Nextest executes each test in its own process, which is a real semantic change
from `cargo test`'s shared-process threads: tests that depend on shared process state
behave differently. This is low risk here because `./run.sh test-closeout`
already runs the entire workspace under nextest as the canonical local gate, so
the suite is known to pass under this runner.

Process-per-test also changes the parallelism profile. Wall-clock time is
expected to improve, but this ADR does not claim a number; the first runs after
this change establish the new baseline.

The three documentation statements asserting the old behavior are updated, as
is the agent guard hook's message, which told agents that CI retained the
direct command.

## Alternatives Considered

- Keep `cargo test` and parse timings from log timestamps. This is what produced the
  original triage, and it cannot separate a slow test from a test that merely
  printed last on a busy thread.
- Add `--report-time` to libtest. It is unstable, requires nightly, and still
  reports through a format the workflow would have to parse itself.
- Split the slow tests into a separate job first. Without per-test timings
  there is no evidence for where to draw the split.

## References

- `.config/nextest.toml`
- `.github/workflows/ci.yml`
- [BUILD.md](../../BUILD.md) — inner-loop strategy and runner split
- [TEST.md](../../TEST.md) — closeout gate
- `fd98c527` — commit that introduced the "CI intentionally remains" wording
