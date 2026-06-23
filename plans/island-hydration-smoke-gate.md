# Island hydration smoke gate — WIP handoff

**Status update (2026-06-23):** the WIP failure state below is historical.
`plans/island_hydration_durable_fix.md` now records the closed audit: a clean
cargo-leptos build passed `scripts/e2e/hydration-smoke.sh` for all 56 listed
island routes, and browser spot-checks proved `/channels` Delete plus seeded
`/playouts` row-select/refresh controls.

**Status:** historical WIP handoff; the gate is now green from the
2026-06-23 durable-fix audit.
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

## Historical Open Bug: nav badge hydration mismatch

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

---

# Day 2 findings (2026-06-22) — systemic island hydration failure

**Badge bug: FIXED.** `badges.rs` `PlayoutWarningBadge`/`HealthBadge` converted
from `#[island]` to plain `#[component]` (server-rendered, no hydration
boundary). Root cause was an `#[island]` nested inside `<details>/<summary>`,
which breaks the islands hydration walk. SSR confirmed: no more
`data-component="…Badge"` wrappers, badge `data-*-badge` spans still present.

**The gate then exposed a much larger, PRE-EXISTING problem.** With badges
fixed: **13 routes pass, 43 fail.** Every failure is the same class —
`A hydration error occurred … expected a marker node, but found this instead`
+ an `unreachable` wasm pageerror — and it is **fatal**: handlers never attach.

Proven in a real browser (Playwright, `/tmp/probe3.cjs`): on `/channels`,
clicking Delete does NOT open the dialog (overlay `display` stays `none`). So
the original `/channels` report was NEVER actually fixed by the badge /
StartupNotifications work; it is part of this systemic failure.

**Discriminator (clear):** islands that render a **dynamic node list**
(`collect_view`/`<For>`/Suspense list — table rows, `<option>` lists, the
`LetterBar`) FAIL; islands with only static markup + reactive *attributes*
(settings forms, empty lists) PASS. The 13 passing are settings/troubleshooting
pages and pages whose lists are empty in the seed DB.

**Not a Leptos version regression.** leptos has been 0.8.19 for 30+ commits
(since mid-May). Latest 0.8.x IS 0.8.19 (next is 0.9.0-alpha — too risky). So
no version-bump escape.

**`Resource::new_blocking` is NOT a reliable fix.** TEST.md rule 3 says islands
should own `Resource::new_blocking`; the failing list islands used non-blocking
`Resource::new`. Converting ALL 42 admin `Resource::new(` → `new_blocking(`
(done, in tree) did NOT clear the gate — still 13 pass / 43 fail, `/channels`
still fatal in-browser. (A one-off channels-only build appeared to work, but
that was an unreliable/stale-wasm artifact; the reliably-rebuilt state is
broken.) Biggest single source: `LetterBar` (`letter_bar.rs:127`, 8 routes).

**Conclusion:** this is a deep Leptos 0.8.19 islands hydration bug with
dynamic-node content, latent until the gate caught it. It is NOT a quick fix and
NOT safely releasable. Build-cycle guessing has not converged.

**Open next steps to try (each needs a ~5-min wasm build to verify):**
- Minimal repro: one island that renders `vec.into_iter().map(..).collect_view()`
  vs `<For>` with keys — determine if `<For>`+keys emits correct markers where
  `collect_view` does not. If so, the fix is `collect_view` → `<For>` in
  `LetterBar`, `PagedTable` rows, and select-option lists (shared primitives →
  fixes most routes at once).
- Check the Leptos `islands` example's list rendering and copy it verbatim.
- Inspect SSR markup of a working dynamic list (if any) vs a failing one for the
  `<!>`/`<!--…-->` marker differences.
- Consider whether `.into_any()` on per-row/cell views drops markers.

**State of tree (branch docs/island-hydration-smoke-plan, uncommitted):**
badges.rs (components), 21 admin files + channels/list.rs (Resource→new_blocking,
UNVERIFIED — consider reverting), lib.rs/routes.rs (signals), spec/runner/docs.
Gate currently RED (correctly). DO NOT cut a release — admin UI list pages have
non-functional controls in the browser.

---

# Day 2 RESOLUTION (2026-06-22) — two distinct bugs

The maintainer identified the real recurring **release breaker**: a **wasm
checksum mismatch** (cargo-leptos `LEPTOS_HASH_FILES`/`hash.txt`). It is SEPARATE
from the island-list hydration bug below.

## Bug A — wasm checksum / build inconsistency — FIXED + GATED
Release builds the WASM (`web-assets` job, `LEPTOS_HASH_FILES=false`) and the SSR
binary (`binaries` job) separately; the binaries job never set `LEPTOS_HASH_FILES`,
so the running server can reference `/pkg/*.<hash>.{js,wasm}` URLs the bundle
lacks → no hydration. Fixes:
- `crates/ersatzrs-web/Cargo.toml`: `hash-files = false` (deterministic).
- `.github/workflows/release.yml`: `LEPTOS_HASH_FILES: "false"` on the binaries
  job + a "Verify web bundle (wasm checksum gate)" step.
- `scripts/release/verify-web-bundle.sh`: boots binary+assets, fails if any
  `/pkg` asset 404s. Tested PASS (consistent) and FAIL (LEPTOS_HASH_FILES=true,
  surfaced the leptos `failed to read hash file` panic).
- Documented: BUILD.md, AGENTS.md, memory `project-wasm-checksum-hydration-consistency`.

## Historical Bug B — island-list hydration marker mismatch
With a CLEAN, CONSISTENT build (`cargo clean` + `cargo leptos build`, gate-verified
consistent), the browser smoke is still **17 pass / 39 fail**. Islands that render
a non-empty dynamic list fail with "expected a marker node"; `/channels` delete
confirmed dead in-browser. This is the "app-wide island marker issue" the session
log notes from 2026-05-22 — pre-existing, already in shipped releases.
NONE of these fixed it (all reverted): `Resource::new` → `new_blocking`,
`collect_view` → `<For>`, badges island → component. A MINIMAL repro island
(static list / collect_view / collect_view-in-Suspense / reactive text /
component-with-list) hydrated CLEANLY — so the trigger is something specific to
the real list rendering.
**Strongest untested lead:** `PagedTable` renders each cell via a
`Box<dyn Fn(&Row) -> AnyView>` returning `.into_any()` (columns + row_actions);
the probe variant testing `.into_any()` + `Box<dyn Fn>` lists was built but never
observed (interrupted). Re-add that probe (`/dev/hydration`) and test
`.into_any()`-in-list vs not. If that's it, the fix is in the shared `PagedTable`
/ column-renderer primitive (fixes most of the 39 at once).

## Final tree state (uncommitted, branch docs/island-hydration-smoke-plan)
Kept (verified): checksum fix (Cargo.toml, release.yml, verify-web-bundle.sh),
hydration smoke (spec + runner using the cargo-leptos binary), lib.rs flag,
routes.rs `data-not-found`, docs (BUILD/AGENTS/TEST/README/CHANGELOG/plan/memory).
Reverted (did not fix Bug B): all `new_blocking`, `<For>`, badge, and
StartupNotifications code changes. Tree compiles (`cargo check -p ersatzrs-web
--lib --features ssr`).
