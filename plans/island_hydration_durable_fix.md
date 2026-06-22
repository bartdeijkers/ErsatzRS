# Durable fix: island-list hydration marker mismatch

## Current Status

This plan supersedes the earlier "wrapper-owned seeded island" conclusion in
this file. The newer whole-app smoke work split the recurring admin hydration
breakage into two separate bugs:

- **Bug A: wasm checksum / build inconsistency - fixed and gated.** Release
  builds now pin cargo-leptos file hashing off in both the SSR binary and WASM
  bundle paths, and `scripts/release/verify-web-bundle.sh` is wired into release
  CI to fail if the assembled bundle references missing `/pkg` assets. This was
  the recurring release breaker where the admin UI never hydrated at all.
- **Bug B: island-list marker mismatch - still open.** With a clean, consistent
  build after Bug A was fixed, `scripts/e2e/hydration-smoke.sh` still reported
  **17 pass / 39 fail**. The failure signature is the Leptos island hydration
  panic "expected a marker node", followed by dead handlers. `/channels` Delete
  was browser-confirmed dead; `/playouts` refresh/row-select remains part of the
  same live risk.

The old claim that the fix is simply "page wrapper owns
`Resource::new_blocking`, island receives resolved props" is not proven. The
failed attempts recorded in `plans/island-hydration-smoke-gate.md` include:

- converting admin `Resource::new` calls to `Resource::new_blocking`;
- changing `collect_view` list rendering to keyed `<For>`;
- converting the nav badges from islands to components;
- reverting those attempts when they did not clear the clean-build smoke.

A minimal repro island with static lists, `collect_view`,
`collect_view`-inside-`Suspense`, reactive text, and component-with-list variants
hydrated cleanly. That means the next fix should isolate what is specific to the
real admin list rendering rather than enforce a broad resource-ownership rule
that may be unrelated to the root cause.

## Tracker State

The current progress source of truth is:

- `docs/progress/hydration.toml`
  - `WholeAppIslandHydrationSmokeGate` is `⏳`.
  - `ChannelsListPage` is `⏳` because `/channels` Delete was confirmed dead
    in-browser.
  - `TimelinePage` / `/playouts` is `⏳` because the original refresh/row-select
    dead-handler report is not release-green until the whole-app smoke and
    browser spot-check pass.
  - Other page-level `✅` entries are historic focused proof only; they are not
    release-level proof while the whole-app gate is pending.
- `docs/progress/phase.toml`
  - The known TODO now names the still-open island-list marker mismatch and
    distinguishes it from the fixed wasm checksum/build-consistency bug.

## Leading Hypothesis

The strongest untested lead is the shared table/list rendering shape:

- `crates/ersatzrs-web/src/admin/ui/primitives/table.rs` renders each cell
  through boxed dynamic closures such as `Box<dyn Fn(&Row) -> AnyView>`.
- Those closures commonly return `.into_any()` from inside list rendering.
- The interrupted probe in the smoke-gate handoff was going to compare
  `.into_any()`-inside-list and `Box<dyn Fn>` variants on `/dev/hydration`.

If this is the trigger, the durable fix should land in the shared primitive
instead of per-page rewrites. It should reduce or remove boxed `AnyView` cell
rendering in dynamic rows, or otherwise force a deterministic Leptos marker
shape that survives SSR and island hydration.

## Work Plan

### 1. Reproduce From The Current Tree

- Run a consistent build, then `scripts/e2e/hydration-smoke.sh`.
- Save the exact pass/fail route list and one representative console trace.
- Browser-prove the original report again:
  - `/channels`: Delete opens the dialog.
  - `/playouts`: row-select populates detail and refresh controls attach.

Do not start with another broad rewrite. The gate is the arbiter; hydration-lint
alone is not enough for marker-node disputes.

### 2. Isolate The List Renderer Trigger

- Re-add a temporary `/dev/hydration` probe that can be removed before commit.
- Compare these variants in the same clean build:
  - direct typed list rows;
  - rows returned through `.into_any()`;
  - rows returned through `Box<dyn Fn(&Row) -> AnyView>`;
  - `PagedTable`-style columns plus row actions;
  - `LetterBar`-style dynamic item lists;
  - select `<option>` lists if the table probe is clean.
- Inspect SSR markup and browser console output for the first variant that
  reproduces "expected a marker node".

### 3. Patch The Shared Primitive First

If the boxed `AnyView`/table hypothesis reproduces:

- fix `PagedTable` or its column/action API so row and cell views have a stable
  hydration marker shape;
- update call sites only as much as the shared API requires;
- keep the fix narrow and avoid changing unrelated page behavior;
- add or update a focused Rust/static regression only if it can actually catch
  the bad shape; otherwise rely on the browser smoke gate.

If the hypothesis is wrong, keep the probe notes in this plan and move to the
next smallest shared renderer candidate before touching pages individually.

### 4. Update The Contract After The Root Cause Is Proven

Only after a browser-proven fix:

- Update `TEST.md` hydration rule 3 so it describes the proven invariant. Do not
  keep the current stale advice if it remains wrong, and do not replace it with
  the old unproven "no island owns an SSR Resource" rule unless the browser proof
  actually establishes that.
- Update `xtask/src/hydration_lint.rs` messages and rules to guard the proven
  anti-pattern. If the root cause is dynamic boxed `AnyView` in island list
  rows, the lint should target that pattern or document why the browser smoke is
  the only practical guard.
- Update `docs/progress/hydration.toml`, `docs/progress/phase.toml`, and the
  session log with the final route count and proof.
- Update `CHANGELOG.md` under `[Unreleased]` if the fix changes release behavior
  or the pre-release gate.

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
