# TEST

Index of testing conventions for ErsatzRS. Existing deep-dive docs are kept in
`docs/` — this file points at them and adds the parts that span the whole
project (UI hydration playbooks, fixture strategy, what runs where).

For build commands and the dev loop, see [BUILD.md](BUILD.md).

## Pyramid

| Layer            | Where                                                    | What it covers                                                                 |
| ---------------- | -------------------------------------------------------- | ------------------------------------------------------------------------------ |
| Unit             | `#[cfg(test)]` modules in each crate                     | Pure logic — duration math, template parsing, scheduling primitives.            |
| Snapshot         | `insta` in `ersatzrs-ffmpeg`, `ersatzrs-infra`           | Generated FFmpeg argv, filter graphs, playout build round-trips.                |
| Integration      | `tests/` per crate                                       | Database repositories, external clients (Plex/Jellyfin/Emby/Trakt), ffprobe.    |
| Hydration smoke  | CI-gated lint plus Playwright specs or browser probes.    | Admin UI islands actually attach handlers and the round-trip server fn fires.   |
| Runtime smoke    | `cargo xtask smoke`                                      | Real web host, seeded SQLite fixture, IPTV MPEG-TS and HLS startup playback.    |
| CI               | `.github/workflows/ci.yml`                               | fmt, clippy, test, architecture/hydration lint, scoped hydration smoke, cargo-deny, commitlint, docs. |

References:

- [docs/explanation/test-strategy.md](docs/explanation/test-strategy.md) — why
  the layers exist.
- [docs/how-to/testing.md](docs/how-to/testing.md) — focused, browser,
  playback, ignored, and closeout commands.
- [docs/how-to/snapshot-tests.md](docs/how-to/snapshot-tests.md) — wrapper plus
  `cargo insta review` workflow.

## What runs where

- **CI (every push and PR)**: the SQLite test layers,
  `cargo xtask architecture-lint`, and
  `cargo xtask hydration-lint -p ersatzrs-web`. The browser island
  hydration smoke also runs in CI when the
  diff touches web, Leptos, E2E, workflow, Cargo, Node, or hydration policy
  files.
- **Focused/local dev**: use `./run.sh test -p <crate>` as the required runner
  and append a test filter when one regression is sufficient. Agent command
  hooks reject direct `cargo test` invocations. The
  repository profile runs through all failures, retries nothing, and reports
  tests slower than 60 seconds. Add relevant non-database feature flags when a
  change crosses a gated path.
- **Closeout**: use `./run.sh test-closeout`. It runs all workspace
  unit/integration/binary tests through nextest, then Cargo doctests because
  nextest cannot execute them. CI runs the same nextest selection under the
  `ci` profile and uploads a JUnit report with per-test durations (ADR 0090);
  it does not run doctests, so the closeout gate remains the only path that
  covers them.
  To resume just the doctest component after focused failure recovery, use
  `./run.sh test-closeout --doctests-only` or the Windows equivalent documented
  in [BUILD.md](BUILD.md). Report the composed evidence explicitly; this option
  alone does not satisfy closeout.
- **Hydration smoke**: prefer a runnable Playwright spec under `tests/e2e/` and
  run it with `E2E_BASE_URL=http://127.0.0.1:8409 npx playwright test ...`.
  For exploratory work, a browser probe through Playwright MCP or another
  attached browser tool is acceptable, but it is not the only documented path.
  Query `cargo xtask progress routes` for the canonical per-route evidence and
  expected interactions.
- **Runtime smoke**: `cargo xtask smoke` builds `ersatzrs-web`, starts it
  against a temporary SQLite database, seeds a tiny local media library, probes
  `/iptv/channel/<n>.ts?mode=ts`, `/iptv/channel/<n>.m3u8?mode=segmenter`, and
  `/iptv/session/<n>/hls.m3u8`, then scans the server log for known
  FFmpeg/HLS failure signatures. Use `--channel N`, `--media PATH`,
  `--skip-ts`, `--skip-hls`, and `--keep-temp` for focused runtime triage.
  Add `--hls-lifecycle` when changing HLS StopOnDisconnect behavior; it seeds a
  short segmenter idle timeout, fetches a real HLS segment, waits for session
  cleanup, then repeats the cleanup proof through the `mode=ts` wrapper.

Run the `teletext capacity recovery` cases in
`tests/e2e/teletext-channel-profiles.spec.ts` through the isolated Playwright
wrapper with `--grep 'teletext capacity recovery' --headed --workers=1`.
They exercise the native readiness/capacity form in both JavaScript modes:
global-scope consent, protected custom policies, explicit replacement and
invalid-policy repair. The fixture restores global policies after each case. Locale setup also primes
absent date/time defaults before safety snapshots, preserves saved choices and
restores their original values and presence.
This browser scope does not establish transport carriage or native Windows
acceptance; those remain in `teletext-admin-recovery-flow`.

Run the `published custom fields report` cases in
`tests/e2e/teletext-restart-flow.spec.ts` through the isolated Playwright wrapper
with qualified FFmpeg9 and `--grep 'published custom fields report' --headed --workers=1`.
They cover Chromium/Firefox, hydrated English and no-JS Dutch, actual page-local
projection failure, retained Health/recovery, independently decoded pages and AV,
and an actual host replacement after deleting the latest elements and crossing
a real minute. Leave the polling page before deliberately stopping its host;
keep the zero-console/page-error assertion. This Base live-policy-v2 synthetic
producer proof does not substitute for the production guide-worker join,
programme transitions, final transport gates or native Windows acceptance.

The opt-in `ordinary bitrate bursts retain scanned captions` case in
`teletext-guide-authoring-flow.spec.ts` also needs
`RUST_LOG=info,ersatzrs_web::transport::iptv::teletext_provenance=debug` for its
owner-bound expiry proof. Request log offsets assign the response; packet hashes,
saved guide metadata, all raw PMT sections and actual sink ACKs qualify its
handover. Pages 200/201 are decoded from the unmodified recovered suffix.
The raw strict transport check remains independent; an expiry receipt cannot
waive a subtitle timing failure. Source and 8 MiB/30-second capture bounds stay
unchanged.

## Add-on list task workspace

AMM-N-01B task routes render populated native forms through
`AddonListWorkspacePage`; the surrounding application shell hydrates. Run the
six `addon-list-workspace`, `addon-list-workspace-visual`, `addon-list-item-form`,
`addon-metadata-editor`, `addon-list-options` and `addon-list-disk-warnings`
specs through
`scripts/e2e/playwright-e2e.sh` against its isolated database/provider fixtures.
The wrapper builds matched SSR/WASM/CSS assets. Functional specs exercise native
and hydrated workflows in Chromium and Firefox; visual checks cover both themes,
320–3840 CSS pixels and 200% zoom. Persistence assertions query the temporary
database. The item-form spec also covers explicit numbering, noncanonical date
preservation, credits, staged replacement save/discard, option-source labels,
field contrast and inner-scroller captures. Numbering assertions use metadata
overrides, and replacement assertions use `AddonMediaListItemReplacement`;
provider overview columns are not the effective draft projection. The AMM-N-01B
milestone owns acceptance and evidence; prototype reports cannot substitute for
production checks.

The prepared-playback regression also requires the publication action to become
enabled after background preparation without reloading or replacing unsaved form
input. Restart, NSF live, subtitle and SponsorBlock workflows use the same native
task routes and retain their persistence and actual-media assertions.

Run `next-version.spec.ts` through the wrapper for the startup-cached Next
version in the shared navigation drawer. It covers desktop/no-JS HTML, mobile
navigation and hydration diagnostics in both browsers. The default expects no
Next installation; for a staged engine set `ETV_NEXT_FOLDER` and
`ERSATZRS_E2E_NEXT_VERSION` to its expected version without the executable prefix.

Run `addon-list-preparation-feedback.spec.ts` on its own wrapper database for
untouched-default preparation, failure/retry guidance, legacy schema adoption
with retained unsaved input, and partial Add recovery. Its isolated add-on
identities avoid changing schemas used by the other prepared-playback fixtures.
Its import helper waits for the visible workspace after database completion,
so subsequent navigation cannot race the landing page's automatic reload.
Prepared fixture variants isolate both package identity and synthetic provider
host. Changing only the URL path does not isolate host admission: a preceding
scenario's active provider lease can defer an import until its lease expiry,
even when that provider finishes earlier. Keep real same-host pacing intact;
do not reduce its configuration to speed up functional tests. Import assertions
retain their 60-second bound and exact item count, and report only job state,
attempts, eligibility, lease and progress timestamps on failure. The prepared
playback scenario also awaits the visible workspace before opening Playback.

Run `addon-walkthrough-fixes.spec.ts` on its own wrapper database for fresh
default-TV first publication, first-import completion without a manual reload,
bulk include/exclude with explicit Save/Discard and draft/publication isolation,
restored-failure feedback, and live add-on Check/Save without lost sibling edits.
It includes a JavaScript-disabled bulk workflow and independent package identities
for each browser/scenario. Its import helper also awaits the visible workspace
before navigation or fixture mutation. Use `addons.spec.ts` for the existing declarative
normal/secret-reference settings regressions; Save now waits for persistent
feedback rather than a document reload. Its hydration readiness wait uses the
same bounded 15 seconds as the other add-on specs, allowing the cold debug WASM
load without changing the overall test timeout or interaction assertions.
On a hydration timeout it reports document visibility, loader readiness and
completed package-resource timings, without resource URLs or settings values.
Installation waits for the redirected page's Enable action after persistence;
an immediate test-driven reload can race that redirect in Firefox.

The restored-publication-failure scenario checks fresh renders and a live poll.
Settled lists intentionally stop polling, so the live case first queues owned
optional detail work behind a future-eligibility barrier, then observes an
updated attempt counter from its synthetic operation in the UI. It restores that
detail work in `finally`. This proves a real readiness poll was applied within
the existing seven-second bound while failure feedback persists; elapsed time
alone is not evidence of a poll.

The exact-overview case in `addon-metadata-editor.spec.ts` starts on the import
landing with no editable collection. Complete the seeded discovery, then await
automatic landing removal and hydration before asserting the exact item IDs,
order and title. Do not restore a manual navigation that hides a broken import
completion transition.

## Drawvg path and preview slate proof

Finding 05 uses `native_drawvg_paths_render_complete_marker_frames` in
`ersatzrs-ffmpeg/tests/drawvg_paths.rs` with `FFMPEG_PATH` and explicit ignored-test
selection for generated-path rendering. The Windows gate is
`scripts/e2e/windows-drawvg-paths.ps1` (PowerShell 7): prepare an owned temporary
root, emit filters with the `drawvg-paths` example, then render all six manifest
cases and check complete red/blue/white pixels. It also proves following native
Drawtext with a real font and a text-file reload path. No server/database is used.

The `channel-watch.spec.ts` real error-slate case injects invalid synthetic VGS
through a disposable graphics attachment. It requires actual Hls.js playback and
the matching segment's original redacted failure, plus responsive warning layout.
An interrupted status request must show unknown output and recover afterwards.
The ordinary real derivative case requires ordinary output without that warning.
Backend regressions preserve buffered slate provenance across subsequent encoder
success, match served playlist times from the saved anchor independently of the
current clock, reject stale replacement-worker reads and verify read-only polling.

## Programme-aware graphics proof

Finding 04 uses `ersatzrs-core::programme_tests` for scheduler-specific grouping,
split films, filler boundaries, occurrence identity, missing/overlapping context
and local-day/DST selection. `ersatzrs-infra/tests/graphics_programme_context.rs`
checks actual historical repository previews, raw Epg compatibility, explicit
overrides, long split programmes and real database refresh for source/mirror
controllers. Dynamic-text tests prove bounded reads, cancellation and retention
of the last good file. Schedule-copy, migration-upgrade, Block continuation and
the deferred-save fresh-process maintenance fixtures preserve attachment values
and the published programme prefix.

Run the `finding 04 programme graphics acceptance` cases in
`tests/e2e/channel-watch.spec.ts` against the runner's disposable database.
Historical cases cover draft/saved text, both renderers, English/Dutch, mobile
and no-JavaScript operation. The live cases require decoded Hls.js pixels and a
bounded 30-second capture of the regular IPTV endpoint, full audio/video decode,
programme frames on both sides of the boundary and an unchanged stored timeline.
Inspect the saved programme frames alongside `served-transition.json`; a pixel
change alone does not identify the displayed title. The fixture reserves four
bounded transcode slots for both outputs and their boundary handovers, then
restores the setting. The transport-integrity gate remains a separate check.

## Hydration rules (read before writing islands)

The hydration smoke layer exists because Leptos islands have sharp edges that
unit/snapshot tests cannot catch. These rules come from real fixes shipped on
the libraries page (commits 5484132, 4190407) and later admin island
stabilisation. Apply them every time you wire interactive UI:

1. **Page wrapper stays `#[component]`.** The interactive body becomes
   `#[island]`. `hydrate_islands()` only attaches handlers declared inside
   `#[island]`. Reference: `crates/ersatzrs-web/src/admin/pages/libraries/mod.rs:18-21`.
2. **Stable DOM shape.** Replace `cond.then(|| view!{...})` and `<Show when=...>` with
   always-render plus `style:display=move || if cond { "" } else { "none" }` and
   `disabled=move || !cond`. Conditional sub-trees crash hydration with
   `failed_to_cast_marker_node`. **Never use `<Show>` inside `#[island]`.**
   This applies to `#[component]`s called inside islands too (e.g. `ItemEditor` inside
   `BlockEditorIsland`). The `Modal` and `DeleteDialog` primitives use `style:display`
   internally — never reintroduce `<Show>` in them.
   Reference: `crates/ersatzrs-web/src/admin/pages/libraries/mod.rs`.
   A helper that returns different `.into_any()` views depending on a flag
   broke hydration of both add-on list publication islands on every workspace
   load (`preparation_view(readiness, only_attention)`, 2026-09-19); two
   separate functions with concrete view types fixed it. The page flag
   `__ersatzrs_hydrated` was already `true` when that mismatch was logged, so
   seeded island specs must also watch console errors for a few seconds after
   the flag (`tests/e2e/addon-list-publication-island.spec.ts`).
3. **No prop-bearing islands across unresolved `Suspense` boundaries.** A
   wrapper may resolve data before rendering an island, or the island may own a
   `Resource::new_blocking` and seed its signals from `data.get_untracked()`.
   The invariant is the boundary shape: do not render a prop-bearing `#[island]`
   while the parent `Suspense` can swap fallback/resolved DOM around it. When in
   doubt, pass only stable scalar props (for example an `i64` id) and keep the
   async boundary inside the island. Reference: `FfmpegProfileEditorIsland`.
4. **`DeleteDialog` always rendered.** `DeleteDialog` takes `entity_name: Signal<String>`
   (a reactive signal, not a `String`). Keep the dialog in the DOM at all times — control
   visibility via the `open: RwSignal<bool>` signal. Never conditionally render a dialog via
   `delete_target.get().map(|...| view!{<DeleteDialog .../>})`. Derive the name as
   `Signal::derive(move || delete_target.get().map(|t| t.name.clone()).unwrap_or_default())`.
5. **Never report a server-side failure through the toast channel.** The toast
   store is deliberately not hydration-stable: request-scoped context on the
   server, one empty process-wide store on the client (`use_toast`). A toast
   queued while SSR renders would serialize a `ToastContainer` child the client
   cannot reproduce, and hydration aborts with "expected a marker node" the
   moment it reaches the container — killing every island on the page. This bit
   `/media/smart-collections/add` in CI run 30132852456. `ToastStore::push` is
   now inert off-wasm so the crash is unrepresentable — which also means a
   server-side push reports nothing at all. Use the toast channel from `Action`
   callbacks, `Effect`s, and `LocalResource` loaders only. A server-backed load
   that can fail belongs in `admin_result_view`, whose `[data-admin-resource]`
   wrapper is serialized as part of the page. `hydration-lint` rule-3 enforces
   this, and carries no baseline entries — a server-executed loader must render
   its failure in the resource value, either through `admin_result_view` or, when
   the `<form>` has to stay server-rendered for a no-JS fallback, through an
   always-present error element plus a disabled submit control.
6. **Server-state polling pattern.** When the UI needs a server-driven signal
   (e.g. lock state), copy the wasm32-gated `Effect` from
   `libraries/mod.rs:81-95`: `gloo_timers::future::TimeoutFuture::new(1_500)`
   in a `loop`, behind `#[cfg(target_arch = "wasm32")]`.

## Hydration startup regression

`hydration-startup.spec.ts` withholds browser idle callbacks without consuming
CPU or slowing I/O, then requires hydration within the existing 15-second
bound and exercises real settings validation. It must run on Chromium and
Firefox. A second scenario holds the WASM request behind a controllable barrier
and verifies that document load is distinct from hydration readiness. The
affected add-on specs share `tests/e2e/hydration.ts`, which reports
document readiness, visibility, bootstrap phase and package-resource timings
on failure without cookies, request bodies or full resource URLs.

Required admin startup is owned by `admin/hydration.rs` and `hydration.js`.
Leptos 0.8.20 gates its default islands loader on an unbounded
`requestIdleCallback`; a local instrumented Chromium failure reached that gate
but never started the loader. The adapter keeps nonce handling, package paths,
SSR context setup, nested-island order and children callbacks, while starting
module loading directly. It follows the existing unhashed, unsplit bundle
contract and does not enable the islands router. Review this adapter against
Leptos's bootstrap when updating Leptos; retain its MIT notice.

The historical `__ersatzrs_hydrated` marker indicates that Rust initialized its
hydration context, before JavaScript finishes traversing islands. A marker-only
check is not proof that a specific control works: retain interaction assertions.
The bootstrap phase reaches `ready` only after traversal completes. The shared
island smoke requires both signals within the unchanged 15-second bound.

## Hydration and UI bug containment

Use this playbook before changing Leptos hydration, islands, shared UI
primitives, router setup, or Cargo dependencies for an admin UI bug. Hydration
failures often surface far away from the first mismatch, so treat them as a
boundary diagnosis problem before refactoring.

1. **Classify the failure first.** Decide whether the bug is SSR HTML shape,
   island hydration/runtime, server function/API, CSS/layout, or data/query
   behavior. Record the current evidence in the handoff or final response.
2. **Create one browser probe before editing.** Use a Playwright spec when the
   interaction is scriptable; otherwise use Playwright MCP or another attached
   browser tool to navigate to the route, capture current console errors, and
   exercise exactly one failing interaction. That probe becomes the regression
   target.
3. **Patch the narrowest stable boundary.** If a server-rendered detail page
   only needs one broken action, keep the page SSR and prefer a small explicit
   JSON endpoint for that action. Do not move the whole page into an island just
   to attach one handler.
4. **Do not change shared UI without proof.** Avoid edits to `letter_bar`,
   pagers, shared modals, `HydrationScripts`, `hydrate_islands`, route setup, or
   Cargo dependencies unless the browser probe proves that boundary is the
   source. If many pages can be affected, split the work into a dedicated
   hydration-hardening task.
5. **Verify one behavior at a time.** After each change, run the cheapest
   relevant build/check, repeat the browser probe, and inspect the console. Stop
   expanding the patch once the failing behavior is fixed.
6. **Keep parity side effects.** When replacing a Leptos server function with an
   explicit endpoint, preserve the same persistence rules and runtime side
   effects, such as playout refresh enqueueing after collection changes.

For Leptos API questions, use Context7 against the repo's current Leptos 0.8.x
versions. Do not apply generic Leptos examples without checking that they match
the 0.8 API and this repo's SSR/islands setup.

### Hydration lint

Static check that runs in seconds. Catches the patterns above without needing
a browser session:

```bash
cargo xtask hydration-lint -p ersatzrs-web
```

Rules enforced (v1):

- **rule-1** — `<Show>` inside `#[island]` (rule 2 above).
- **rule-2** — Prop-bearing call to a known `#[island]` function inside a
  `<Suspense>` region (rule 3 above). The lint guards only the known lexical
  boundary hazard; whole-app marker mismatches still require the browser smoke
  because list/table DOM shape is not statically provable here.
- **rule-3** — The toast channel (`load_or_notify`, `ToastStore::push`) reached
  from a server-executing `Resource::new` / `Resource::new_blocking` loader
  (rule 5 above). `LocalResource` loaders are exempt: they only run on the
  client, where the store is real.

Known pre-existing findings are listed in `.hydration-lint-baseline` at the
repo root. When you fix one, delete its line — new findings in files not
listed there fail the linter as normal. To run without the baseline (e.g.
to see the full debt): `cargo xtask hydration-lint -p ersatzrs-web --no-baseline`.

### Fast parity check

For admin/media parity slices, run the focused gate ladder with:

```bash
cargo xtask parity-check web
```

This wraps the cheap checks that have repeatedly caught late parity bugs:

- `cargo xtask target-size` warning check before the gate ladder
- `cargo fmt --all -- --check`
- `cargo xtask architecture-lint`
- `cargo xtask hydration-lint -p ersatzrs-web`
- `cargo nextest run -p ersatzrs-web`
- `cargo clippy -p ersatzrs-web --all-targets -- -D warnings`

Use extra scope words for traceability when the slice spans more than web UI,
for example `cargo xtask parity-check web media`. The current command records
those scopes in output; widen tests manually when the slice changes shared
repository, scanner, provider, scheduler, or health behavior.

### Reusable parity regressions

When a parity review finds a repeated defect class, add or update a reusable
regression instead of leaving only a one-off smoke note. Prioritize:

- Card action menus and fallback links.
- Hydrated islands attaching handlers.
- SSR links, fallback modals, and no-JS save/action paths.
- Side effects after add/remove/edit, especially generated playlists and
  playout refresh enqueueing.
- Health/troubleshooting visibility after runtime requirement, provider,
  schema, media-state, or FFmpeg capability changes.

## Verification responsibility split

The porter (`leptos-ersatztv-porter` agent) and the human reviewer share the
verification load on UI port work. Each side runs only what's cheap on their
side, so neither blocks the other.

The **porter** produces:

- Source code that compiles clean (`cargo build -p ersatzrs-web`,
  `cargo clippy -p ersatzrs-web --all-targets -- -D warnings`).
- Zero new `cargo xtask hydration-lint` findings.
- A structured route entry in `docs/progress/hydration.toml`, with status
  `TODO` when browser proof remains.
- A hand-off checklist of the gates the human should run.

The **human** runs, before committing:

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`.
- `./run.sh test-closeout`
- `cargo leptos build` from `crates/ersatzrs-web` (rebuilds hydration assets).
- A runnable Playwright spec for the touched page when one exists or is added;
  otherwise a documented browser smoke using Playwright MCP or an equivalent
  browser tool.
- Updates the structured hydration route entry from `TODO` to `✅` on success.

This split keeps the porter's edit loop short — full-workspace gates and
hydration-asset rebuilds happen once at commit time, not on every iteration.

## Admin UI E2E

Run `schedule-deferred-save.spec.ts` for Classic deferred item saves through
hydrated and no-JavaScript editors, retained horizons, pending/build status,
incompatible-edit rejection and preservation of unsaved fields during polling.
The infrastructure natural-maintenance restart regression supplies the actual
future-application proof; the browser fixture deliberately retains a distant horizon.

Run `local-library-paths.spec.ts` for local-library validation feedback, draft
correction, localized errors, literal percent paths and unchanged scanned-media
identity. It exercises Chromium and Firefox with JavaScript enabled and disabled.

Run `dependent-group-selections.spec.ts` for Deco and Playout Templates group
changes. It verifies cleared child IDs, replacement save/reload persistence and
retained date filters in Chromium and Firefox, using the wrapper's isolated database.

Scripted admin parity checks live under `tests/e2e/`. CI runs the full
configured Playwright suite by default through:

```bash
scripts/e2e/playwright-e2e.sh
```

The wrapper defaults Playwright to one worker because the full suite mutates
shared application state in its isolated test database. Set
`ERSATZRS_E2E_WORKERS=<n>` or pass an explicit `--workers` argument only for
spec subsets that are known to be independent.

After building, the wrapper copies the matched server and site assets into its
temporary runtime. The running suite and any restart fixtures use that copy, so
another local Leptos build cannot replace the candidate's hydration assets.

Because that single worker is a correctness requirement rather than a tuning
choice, CI runs the wrapper separately per selected suite and browser. Ready
PRs run relevant whole specs in Chromium. Weekly/full release-candidate CI
retains general and Level 1.5 suites in Chromium and Firefox; Level 2.5 remains
an explicit optional dispatch. See [CI domain selection](BUILD.md#ci-lanes)
for ownership, conservative fallback and product requirements.
Each job invokes the wrapper
separately, and the wrapper creates its own temporary database and web host
per invocation, so the two projects cannot observe each other's state. The
jobs do not overlap: both CI runners share one machine, and two suites
building and driving browsers side by side compete for its CPU and memory.
Each job still keeps its own port (`ERSATZRS_E2E_PORT`) and result directory
(`ERSATZRS_E2E_RESULT_DIR`), so either runner can take it. Splitting by
project rather than by `--shard` also keeps every
`test.describe.configure({ mode: 'serial' })` group whole inside one job.

Run a single project locally the same way CI does:

```bash
ERSATZRS_E2E_PORT=18409 scripts/e2e/playwright-e2e.sh --project=chromium
```

On Windows, run the same wrapper from Git Bash. It keeps shell-owned paths in
POSIX form while translating the temporary database, home, site, and fixture
paths passed to native Windows processes into drive-letter form. The wrapper
also sets `ERSATZRS_APP_STATE_DIR` and `ERSATZRS_APP_DATA_DIR`, because Windows
known-folder resolution does not honor a temporary `HOME`; artwork and caches
therefore remain inside the throwaway test root on every platform. Its web host
always binds to loopback, so the E2E run does not require a Windows Firewall
approval.

For focused local probes against an already-running server, use
`E2E_BASE_URL=http://127.0.0.1:8409 npx playwright test tests/e2e/<spec>.ts`.
When no spec exists yet, use Playwright MCP or another attached browser tool
for the immediate probe and add a `TODO` row below for durable coverage.

Workflow:
1. Start the server: `./run.sh dev` (or `cargo run --bin ersatzrs-web`).
2. Run or add a focused Playwright spec for the page. For exploratory manual
   probes, use the attached browser tool to navigate, snapshot, click, and
   inspect console output.
3. Check browser console for `failed_to_cast_marker_node` or other hydration
   errors; any such error is a product bug in the affected island.

**Geometry assertions must pin the element they measure.** `admin_result_view`
wraps page content in an unconstrained `<section data-admin-resource>`, so a
bare `locator('section').first()` resolves to that wrapper and measures the full
main-content box instead of the page's max-width container — silently, with no
visual regression to notice. Target the container itself
(`section[class*="content"]:not([data-admin-resource])`) or exclude the wrapper
(`:not([data-admin-resource])`). CI run 30132852456 lost `remote-streams.spec.ts`
this way: 1764px measured against a 1540px bound, while the 1536px grid it meant
to check was still correct.

### Whole-app island hydration smoke (CI/release gate)

`tests/e2e/island-hydration.spec.ts` visits every route that renders an
`#[island]` and proves, per route, that island hydration ran to completion.
This is the durable guard against the recurring class of regression where a
shell-level island, a shared primitive, the router, or a Cargo dependency
aborts hydration and silently kills the click handlers on otherwise-unrelated
pages (e.g. the stop/delete buttons on `/channels`, refresh/row-select on
`/playouts`). The default CI Playwright job includes this spec in both
configured browser projects; run the narrower smoke manually before every
release and before closing broad admin UI hydration work:

```bash
scripts/e2e/hydration-smoke.sh
```

The runner is self-contained: it builds the Leptos hydration assets, boots
`ersatzrs-web` on a throwaway SQLite database (isolated `HOME`, never touches
`~/.ersatzrs`), waits for `/healthz`, runs the spec, and tears everything down.
An FFmpeg operator override (`~/.local/ffmpeg-9.0`, like `run.sh`) is used when
present so startup skips the managed runtime download. If startup acquires a
managed runtime instead, the wrapper also exposes that exact FFmpeg binary to
browser fixtures that generate calibration media. Useful env knobs:
`ERSATZRS_SKIP_LEPTOS_BUILD=1` (reuse existing assets), `ERSATZRS_E2E_KEEP_TEMP=1`
(keep the temp DB/log), `ERSATZRS_E2E_PORT=NNNN`.

Detection signals per route: `window.__ersatzrs_hydrated === true` (set in
`crates/ersatzrs-web/src/lib.rs::hydrate` right after `hydrate_islands()`, so a
marker-mismatch panic in any island leaves it unset), no uncaught page errors,
no hydration-signature console errors, and the route did not fall through to the
404 catch-all (`[data-not-found]`). The spec covers islands reachable without
seeded data (list / add / settings / system / source routes); detail and editor
routes that need a real `:id` belong in their own seeded specs. When you add a
new island route, add it to the `ISLAND_ROUTES` table in the spec.

The gate also asserts the shared navigation badge wrappers on every route and
injects a settings storage failure against its throwaway database. That error
proof requires one stable `[data-admin-resource][data-state="error"]` wrapper,
no form or submit control inside it, completed hydration, and no marker-mismatch
or other hydration-signature error. Server-backed resource primitives must keep
the same outer element across loading, ready, empty, and error states; only the
`data-state` value and contents may change across a `Suspense` boundary.
Failure-injection specs read the throwaway database from `E2E_DB_PATH` or
`ERSATZRS_DB_PATH`; both the focused hydration runner and the normal full-suite
wrapper export that shared contract so CI cannot silently skip the proof.

Support page parity criteria are captured from the live ErsatzTV Legacy
Support pages in `tests/e2e/support-page-criteria.ts`. The support spec can
seed the persisted health snapshot with `E2E_SEED_SUPPORT=1` when
`E2E_DB_PATH` or `ERSATZRS_DB_PATH` points at the SQLite test database.

`teletext-secondary-default.spec.ts` uses the existing owned native-router and
ready/completion markers for receipt-bound context concepts. Run its matching
ignored `secondary_default_headed_localized_native_forms` host with current,
component-erasure-matched frontend assets, then headed Chromium and Firefox
sequentially. It checks nine cultures, both themes, JS/no-JS, exact context undo
and historical controls; native assertions own cold replay and live isolation.
It establishes no external-editor or receiver capability.

`teletext-decoder-health.spec.ts` checks the actual startup-cached decoder in
English/Dutch, both themes and hydrated/no-JS flows. Its default published-runtime
expectation is `Info`: decoding works, but original Live caption timestamps are
unavailable. For an independently qualified candidate, set
`ERSATZRS_TELETEXT_E2E_ORIGINAL_PTS=true` to require `Pass`; this does not infer
the expected capability from the response. Pin both tools with
`ERSATZRS_TELETEXT_E2E_FFMPEG_SHA256` and
`ERSATZRS_TELETEXT_E2E_FFPROBE_SHA256` when proving an exact runtime. The same
flow checks Refresh persistence, localized messages, diagnostics identities
and the exported `original_pts` value. Use the existing wrapper with
`--headed --workers 1` and a matching SSR/WASM bundle.

## Admin route verification

Route and hydration/browser verification state is canonical in
[`docs/progress/hydration.toml`](docs/progress/hydration.toml). Query the live
structured view instead of maintaining another Markdown matrix:

```bash
cargo xtask progress routes
```

Add or update the owning structured entry when a route or its evidence changes.
`cargo xtask progress check` rejects a reintroduced per-route table and reports
route-set mismatches in either direction.

## Fixture strategy

- **External services (Plex/Jellyfin/Emby/Trakt)** — `httpmock` helper used in
  `ersatzrs-infra` and `ersatzrs-scanner` integration tests.
- **Scanned local libraries (Playwright)** — `tests/e2e/helpers/scanned-library.ts`
  generates a tiny TV-show tree with the pinned FFmpeg, adds it through the
  local library form and runs the application's own scanner from the Libraries
  page. Prefer it over hand-inserted rows when a browser spec needs scan-shaped
  data. Only the television scanner writes `LibraryFolder` rows, so the helper
  builds shows rather than movies. `local-library-scan-lifecycle.spec.ts` is
  the reference user.

## Snapshot-test policy

Snapshot tests are required when generated output is the behavior under test,
including FFmpeg argument generators, filter-complex builders, YAML instruction
handlers, and drawvg VGS/ASS template renderers.

Name each snapshot for the scenario, for example
`software_pipeline_single_video_hls`, `yaml_duration_instruction_exact_fit`, or
`drawvg_lower_third_with_title`. Review the output as behavior rather than a
formatting artifact, never commit `.snap.new` files, and explain an intentional
snapshot update in the pull request. A deliberate ErsatzTV behavior divergence
also needs the owning ADR.

## Adding a new test

1. **Pure logic** → `#[cfg(test)] mod tests` next to the code.
2. **Generated FFmpeg argv / filter graph / YAML round-trip** → snapshot test
   in the relevant crate, follow `docs/how-to/snapshot-tests.md`.
3. **Repository or external client behaviour** → integration test under
   `crates/<crate>/tests/`, reuse `httpmock` patterns.
4. **Admin UI behaviour** → add or run a focused Playwright spec when practical;
   otherwise document the browser probe used and update the matching structured
   route entry in `docs/progress/hydration.toml`.
