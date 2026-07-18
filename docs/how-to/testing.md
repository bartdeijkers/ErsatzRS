# Run tests

Use the narrowest test that proves the behavior.

## Running tests

Use cargo-nextest as the preferred focused/local runner:

```bash
cargo nextest run -p <crate>
cargo nextest run -p <crate> <test-filter>
```

The repository profile in `.config/nextest.toml` runs through all failures,
does not retry failures, and reports tests that exceed 60 seconds. This keeps
flaky or hanging Phase 7 tests visible rather than hiding them behind retries.

For closeout, run the canonical Cargo gate:

```bash
cargo test --workspace --all-features
```

Nextest does not run doctests, so it supplements rather than replaces the
closeout command and current CI runner.

## Unit tests

Place unit tests next to the module under `#[cfg(test)]`. Use them for pure
domain logic, small parsers, builder invariants, and error mapping.

## Integration tests

Use crate-level `tests/` directories for behavior that crosses module
boundaries. Keep fixtures small and deterministic.

Ignored integration tests remain part of the contract when they depend on
local tooling or large fixtures. Run them explicitly with `cargo test -- --ignored`.

## Snapshot tests

Use `insta` for generated output that must not drift silently:

- FFmpeg argv builders.
- FFmpeg filter-complex builders.
- YAML scheduling timelines.
- VGS/ASS graphics template rendering and `drawvg` filter fragments.

See [snapshot-tests.md](snapshot-tests.md).

## End-to-end tests

Put browser workflows under `tests/e2e/` and run them through:

```bash
scripts/e2e/playwright-e2e.sh
```

The wrapper boots the real web host and runs serially by default because the
specs share a seeded database. Use the TSDuck playback gate under `scripts/e2e/`
for transport-stream correctness that browser tests cannot prove.

## Database tests

- Use an in-memory `sqlx::SqlitePool` per test where possible.
- Use MySQL through testcontainers only at integration tier.
- Gate MySQL integration tests behind a feature such as
  `mysql-integration`.

## Fixtures

- Keep fixtures minimal.
- Do not include copyrighted media.
- Do not include private media paths or personal library metadata.
- Prefer generated sample data.
