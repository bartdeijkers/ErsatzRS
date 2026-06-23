# Durable fix: island hydration smoke closure

## Current Status

This plan is closed as of 2026-06-23. It superseded the earlier
"wrapper-owned seeded island" conclusion and split the recurring admin
hydration breakage into two separate bugs:

- **Bug A: wasm checksum / build inconsistency - fixed and gated.** Release
  builds now pin cargo-leptos file hashing off in both the SSR binary and WASM
  bundle paths, and `scripts/release/verify-web-bundle.sh` is wired into release
  CI to fail if the assembled bundle references missing `/pkg` assets. This was
  the recurring release breaker where the admin UI never hydrated at all.
- **Bug B: island-list marker mismatch - closed from current HEAD.** The stale
  2026-06-22 state was **17 pass / 39 fail** with "expected a marker node" and
  dead handlers. Reproducing from the current tree with a clean, consistent
  cargo-leptos build now passes all **56 / 56** routes in
  `scripts/e2e/hydration-smoke.sh`. Browser spot-checks also proved
  `/channels` Delete opens the confirmation dialog and seeded `/playouts`
  row-select plus Refresh Playout attach without hydration/page errors.

The old claim that the fix is simply "page wrapper owns `Resource::new_blocking`,
island receives resolved props" remains too broad. Current proof supports the
more precise invariant documented in `TEST.md`: do not render prop-bearing
islands across unresolved `Suspense` boundaries, and use the whole-app browser
smoke for marker-shape disputes that lexical lint cannot prove.

## Tracker State

The current progress source of truth is:

- `docs/progress/hydration.toml`
  - `WholeAppIslandHydrationSmokeGate` is `✅` with the 56/56 route count.
  - `ChannelsListPage` is `✅` after the Delete dialog browser spot-check.
  - `TimelinePage` / `PlayoutsPage row selection` is `✅` after the seeded
    row-select/detail/Refresh Playout browser spot-check.
  - Other page-level entries keep their focused proof state; the whole-app gate
    is the release-level no-fixture hydration proof.
- `docs/progress/phase.toml`
  - The stale island-list marker mismatch known TODO is removed; HLS segmenter
    playback proof remains the active Phase 7 hardening TODO.

## Disposition Of The Leading Hypothesis

The strongest stale lead was the shared table/list rendering shape:

- `crates/ersatzrs-web/src/admin/ui/primitives/table.rs` renders each cell
  through boxed dynamic closures such as `Box<dyn Fn(&Row) -> AnyView>`.
- Those closures commonly return `.into_any()` from inside list rendering.
- The interrupted probe in the smoke-gate handoff planned to compare
  `.into_any()`-inside-list and `Box<dyn Fn>` variants on `/dev/hydration`.

Because the current tree no longer reproduces the marker mismatch, this plan did
not add a temporary `/dev/hydration` route and did not rewrite `PagedTable`.
`PagedTable` still appears in green `/channels` and `/watermarks` smoke routes,
so current evidence does not justify changing that shared primitive.

## Completed Work

### 1. Reproduce From The Current Tree

- Ran a consistent cargo-leptos build through
  `scripts/e2e/hydration-smoke.sh`.
- Result: **56 passed / 0 failed**.
- No representative console trace was saved because the current smoke produced
  no marker-mismatch, page-error, hydration-error, or not-found failure.
- Browser-proved the original reports:
  - `/channels`: Delete opens and closes the `Confirm Delete` dialog.
  - `/playouts`: a seeded row selection reveals `Playout Detail`, and
    `Refresh Playout` attaches and dispatches without hydration/page errors.

### 2. Isolate The List Renderer Trigger

- Skipped. The gate from step 1 is the arbiter and did not reproduce the marker
  mismatch from current HEAD.
- The boxed `AnyView`/`PagedTable` hypothesis is recorded as stale, not proven.

### 3. Patch The Shared Primitive First

- No shared primitive patch was made for this plan. Current browser evidence is
  green with the existing `PagedTable` API.

### 4. Update The Contract After The Root Cause Is Proven

- `TEST.md` hydration rule 3 now describes the proven boundary invariant without
  requiring the old unproven "island owns every SSR resource" rule.
- `xtask/src/hydration_lint.rs` wording now matches that invariant and makes the
  browser smoke the guard for marker-shape disputes.
- `docs/progress/hydration.toml`, `docs/progress/phase.toml`, and the session
  log record the final 56/56 route count and spot-check proof.
- `CHANGELOG.md` records the release-green island hydration state under
  `[Unreleased]`.

## Verification

The durable fix is not complete until all of these pass from a clean,
consistent build:

1. `scripts/release/verify-web-bundle.sh` still passes for the checksum/build
   consistency gate.
2. `scripts/e2e/hydration-smoke.sh` reports all listed routes green
   (`window.__ersatzrs_hydrated === true`, no marker-mismatch console errors,
   no page errors, no `[data-not-found]` fallback).
3. `/channels` Delete opens the dialog in a browser.
4. `/playouts` row-select populates the detail table and refresh controls attach
   in a browser.
5. `cargo xtask progress check` passes after tracker updates.
6. The focused web gate used for the implementation slice passes. Prefer
   `cargo xtask parity-check web` unless the slice deliberately documents a
   narrower gate.
