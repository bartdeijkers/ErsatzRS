# Testing

Use the narrowest test that proves the behavior.

## Unit Tests

Place unit tests next to the module under `#[cfg(test)]`. Use them for pure
domain logic, small parsers, builder invariants, and error mapping.

## Integration Tests

Use crate-level `tests/` directories for behavior that crosses module
boundaries. Keep fixtures small and deterministic.

Ignored integration tests remain part of the contract when they depend on
local tooling or large fixtures. Run them explicitly with `cargo test -- --ignored`.

## Snapshot Tests

Use `insta` for generated output that must not drift silently:

- FFmpeg argv builders.
- FFmpeg filter-complex builders.
- YAML scheduling timelines.
- SVG template rendering and `drawvg` filter fragments.

See [snapshot-tests.md](snapshot-tests.md).

## End-to-End Tests

Put cross-crate HTTP and streaming tests under `ersatzrs-web/tests/` once the web
crate exists. E2E tests may be slower and should focus on critical user flows.

## Database Tests

- Use an in-memory `sqlx::SqlitePool` per test where possible.
- Use MySQL through testcontainers only at integration tier.
- Gate MySQL integration tests behind a feature such as
  `mysql-integration`.

## Fixtures

- Keep fixtures minimal.
- Do not include copyrighted media.
- Do not include private media paths or personal library metadata.
- Prefer generated sample data.
