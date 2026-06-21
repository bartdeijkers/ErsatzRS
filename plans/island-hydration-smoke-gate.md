# Island hydration smoke gate — WIP handoff

**Status:** in progress, gate currently RED on a real bug (see "Open bug").
**Started:** 2026-06-21. **Goal (user):** "test all island hydrations before
every release" — a Playwright e2e that proves every Leptos `#[island]` route
actually hydrates, run as a manual pre-release gate.

## Why
We keep regressing one class of bug: a change to a shell-level island, a shared
primitive, the router, or a Cargo dep aborts island hydration, and interactive
controls (stop/delete on `/channels`, refresh/row-select on `/playouts`, dialogs,
forms) silently stop working while plain `<a>` links keep working. SSR/unit tests
don't catch it. This gate does.

## What's done (working)

- **Spec:** `tests/e2e/island-hydration.spec.ts` — data-driven, one test per
  no-fixture island route (56 tests). Per route asserts:
  1. `GET route` → HTTP 200
  2. not the SPA 404 catch-all (`[data-not-found]` absent)
  3. `window.__ersatzrs_hydrated === true` (primary signal)
  4. no uncaught pageerror
  5. no hydration-signature console error (`failed_to_cast_marker_node`,
     `panicked`, `hydration`, …)
  Failure messages dump the full captured console + page errors for diagnosis.
  Routes covered: lists/add/settings/system/source routes (no seeded data).
  Out of scope: detail/editor routes needing a real `:id` (own seeded specs).

- **Runner:** `scripts/e2e/hydration-smoke.sh` — self-contained:
  `cargo leptos build` → boot `ersatzrs-web` on throwaway sqlite + isolated
  `HOME` (never touches `~/.ersatzrs`) with absolute `LEPTOS_SITE_ROOT` →
  wait `/healthz` → run spec → teardown. Env knobs: `ERSATZRS_SKIP_LEPTOS_BUILD=1`,
  `ERSATZRS_E2E_KEEP_TEMP=1`, `ERSATZRS_E2E_PORT`, `ERSATZRS_FFMPEG_DIR`,
  `ERSATZRS_SKIP_PW_INSTALL=1`. Uses FFmpeg override at `~/.local/ffmpeg-8.1`
  (like `run.sh`) so startup skips the managed runtime download.
  Verified: boots, builds assets, runs all 56 end-to-end.

- **App signals added:**
  - `crates/ersatzrs-web/src/lib.rs::hydrate` sets `window.__ersatzrs_hydrated =
    true` right after `hydrate_islands()` (uses `web_sys`/`js_sys`, wasm32 only).
    A marker-mismatch panic in any island aborts before this line.
  - `crates/ersatzrs-web/src/admin/routes.rs` 404 fallback `<div>` tagged
    `data-not-found="true"`.
  - Both compile on `ssr` and `wasm32`/`hydrate`. hydration-lint clean.

- **Runner gotcha (already fixed):** `cargo leptos build` writes the site bundle
  to **repo-root** `target/site` (NOT the `.cargo/config.toml` scratch
  `build.target-dir = /mnt/f/ErsatzRS/target`). The script now searches repo-root
  `target/` and pins an absolute `LEPTOS_SITE_ROOT` so the server finds assets.

- **Docs:** TEST.md (new "Whole-app island hydration smoke" subsection +
  workflow), `scripts/e2e/README.md` (new "Island hydration smoke (E2E)"
  section), CHANGELOG `[Unreleased]` Added + Fixed entries.

## Separate fix already made: `/channels` (likely correct, keep)
`crates/ersatzrs-web/src/admin/layout/mod.rs` `StartupNotifications` was a
shell-level island streaming a non-blocking `Resource::new`. Rewrote it to a
wasm32-gated **client-only** fetch (no SSR resource). No smoke error ever pointed
at it, so keep it. (Whether it alone fixed `/channels` is now uncertain — the
badge bug below also breaks every page; see "Reassess".)

## OPEN BUG (blocks green gate): nav badge hydration mismatch

`crates/ersatzrs-web/src/admin/layout/badges.rs` — `PlayoutWarningBadge` and
`HealthBadge` live in the nav drawer, so they hydrate on **every** page → all 56
routes fail. Console error:

```
A hydration error occurred while trying to hydrate an element defined at
crates/ersatzrs-web/src/admin/layout/badges.rs:<span line>.
The framework expected a marker node, but found this instead: node
```

**Regression source:** commit `6065b00` switched these badges from `Resource::new`
(hardened in `b6f4e8b` "eliminate failed_to_cast_marker_node across all admin
islands") to `LocalResource::new`. `LocalResource` doesn't run during SSR, so the
client hydration walk finds a different DOM shape.

### Attempts so far (current uncommitted state of badges.rs)
1. `LocalResource::new` → `Resource::new`, read directly in the view.
   Result: different error — "reading a resource in `hydrate` mode outside a
   `<Suspense>`/`<Transition>`/effect" warning **and** still a marker mismatch.
2. Read the `Resource` only inside an `Effect`, mirror into plain `RwSignal`s,
   render the signals (TEST.md hydration rule 5 pattern). Added `badge_text()`
   helper (empty when 0, `99+` cap). Result: **warnings gone**, but the
   `expected a marker node` **error persists** at the badge `<span>`.
3. Wrapped `HealthBadge`'s two sibling top-level spans in a single
   `<span style="display:contents">` root (islands want one root).
   Result: still the same marker mismatch (now points at `PlayoutWarningBadge`'s
   span after line shift) — so multi-root was NOT the cause either.

### Conclusion / next hypothesis
Not resource-read-location, not multi-root. Most likely the **reactive text-node
child** `{move || badge_text(...)}` at the island root doesn't get a matching SSR
marker comment. Next things to try:
- Compare against a known-good island that renders reactive text inside and
  hydrates cleanly; copy its exact shape.
- Try removing the reactive text child: render the count via a stable nested
  element or an attribute, or keep the text static and only toggle
  `style:display` (the badge value matters though).
- Consider `Resource::new_blocking` so SSR resolves+serializes before streaming
  and the view can read it directly with deterministic markers.
- Worst case, fall back to the exact `b6f4e8b` badges.rs shape verbatim and
  re-verify (it was the last known state that claimed marker-mismatch-free).
- Constraint: `tests/e2e/support-pages.spec.ts` asserts BOTH
  `[data-health-error-badge]` and `[data-health-warning-badge]` exist (count 1
  each) — keep both spans in the DOM, toggle visibility only.

### Reassess once badges hydrate
With badges fixed, the shell is clean; in the last run every per-page console
error pointed ONLY at badges.rs (never at the page island), so page islands
likely already hydrate. After green, confirm `/channels` stop/delete and
`/playouts` refresh/row-select actually work (the original report). If the gate
is green with badges fixed even after reverting the `StartupNotifications`
change, that change may have been unnecessary — decide whether to keep it.

## How to resume
```bash
# fast inner loop (reuse last-built assets if present):
ERSATZRS_SKIP_LEPTOS_BUILD=1 ERSATZRS_E2E_KEEP_TEMP=1 ./scripts/e2e/hydration-smoke.sh
# full gate (rebuilds wasm — ~5 min):
./scripts/e2e/hydration-smoke.sh
# inspect a single failure's console: search /tmp/hsmoke*.log or the kept temp
# dir's ersatzrs-web.log printed at the end of a failing run.
```
Each verify cycle that touches Rust UI code needs a full `cargo leptos build`
(~5 min wasm). `cargo-leptos` is installed (v0.3.6). Note: `cargo clean` was run
at end of 2026-06-21 session (scratch >40 GiB), so the next build is cold.

## Changed files (all uncommitted)
- `crates/ersatzrs-web/src/admin/layout/mod.rs` — StartupNotifications client-only
- `crates/ersatzrs-web/src/admin/layout/badges.rs` — WIP Effect+wrapper (gate red)
- `crates/ersatzrs-web/src/lib.rs` — `window.__ersatzrs_hydrated` flag
- `crates/ersatzrs-web/src/admin/routes.rs` — `data-not-found` on 404 fallback
- `tests/e2e/island-hydration.spec.ts` — new spec
- `scripts/e2e/hydration-smoke.sh` — new runner
- `TEST.md`, `scripts/e2e/README.md`, `CHANGELOG.md` — docs
- (also pre-existing uncommitted: launcher/main.rs, host/bootstrap.rs from before)
