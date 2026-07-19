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
| MySQL backend    | `--features mysql-integration`                           | Same integration tests, run against MariaDB via testcontainers.                 |
| Hydration smoke  | CI-gated lint plus Playwright specs or browser probes.    | Admin UI islands actually attach handlers and the round-trip server fn fires.   |
| Runtime smoke    | `cargo xtask smoke`                                      | Real web host, seeded SQLite fixture, IPTV MPEG-TS and HLS startup playback.    |
| CI               | `.github/workflows/ci.yml`                               | fmt, clippy, test, hydration lint, scoped hydration smoke, cargo-deny, commitlint, docs. |

References:

- [docs/explanation/test-strategy.md](docs/explanation/test-strategy.md) — why
  the layers exist.
- [docs/how-to/testing.md](docs/how-to/testing.md) — fixture rules, in-memory
  SQLite vs testcontainers MySQL, ignored tests.
- [docs/how-to/snapshot-tests.md](docs/how-to/snapshot-tests.md) — `cargo
  insta review` workflow and snapshot naming.

## What runs where

- **CI (every push and PR)**: layers 1–4 plus `cargo xtask hydration-lint -p
  ersatzrs-web`. The browser island hydration smoke also runs in CI when the
  diff touches web, Leptos, E2E, workflow, Cargo, Node, or hydration policy
  files. The `mysql-integration` feature is enabled there, so the
  testcontainers MariaDB run is part of CI.
- **Focused/local dev**: use `./run.sh test -p <crate>` as the required runner
  and append a test filter when one regression is sufficient. Agent command
  hooks reject direct `cargo test` invocations. The
  repository profile runs through all failures, retries nothing, and reports
  tests slower than 60 seconds. Add the relevant feature flags when the change
  crosses a gated path.
- **Closeout**: use `./run.sh test-closeout`. It runs all workspace
  unit/integration/binary tests through nextest, then Cargo doctests because
  nextest cannot execute them. The MySQL integration feature may pull a MariaDB
  image on a cold cache. CI remains on its existing direct Cargo test selection.
- **Hydration smoke**: prefer a runnable Playwright spec under `tests/e2e/` and
  run it with `E2E_BASE_URL=http://127.0.0.1:8409 npx playwright test ...`.
  For exploratory work, a browser probe through Playwright MCP or another
  attached browser tool is acceptable, but it is not the only documented path.
  See the per-page index below for expected interactions.
- **Runtime smoke**: `cargo xtask smoke` builds `ersatzrs-web`, starts it
  against a temporary SQLite database, seeds a tiny local media library, probes
  `/iptv/channel/<n>.ts?mode=ts`, `/iptv/channel/<n>.m3u8?mode=segmenter`, and
  `/iptv/session/<n>/hls.m3u8`, then scans the server log for known
  FFmpeg/HLS failure signatures. Use `--channel N`, `--media PATH`,
  `--skip-ts`, `--skip-hls`, and `--keep-temp` for focused runtime triage.
  Add `--hls-lifecycle` when changing HLS StopOnDisconnect behavior; it seeds a
  short segmenter idle timeout, fetches a real HLS segment, waits for session
  cleanup, then repeats the cleanup proof through the `mode=ts` wrapper.

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
5. **Server-state polling pattern.** When the UI needs a server-driven signal
   (e.g. lock state), copy the wasm32-gated `Effect` from
   `libraries/mod.rs:81-95`: `gloo_timers::future::TimeoutFuture::new(1_500)`
   in a `loop`, behind `#[cfg(target_arch = "wasm32")]`.

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
- A row in the per-page E2E index below, with status `TODO`.
- A hand-off checklist of the gates the human should run.

The **human** runs, before committing:

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings` and
  `cargo clippy --workspace --all-targets --features mysql -- -D warnings`
  (mirror CI: two passes, neither with `--all-features`).
- `./run.sh test-closeout`
- `cargo leptos build` from `crates/ersatzrs-web` (rebuilds hydration assets).
- A runnable Playwright spec for the touched page when one exists or is added;
  otherwise a documented browser smoke using Playwright MCP or an equivalent
  browser tool.
- Flips the per-page index row from `TODO` to `✅` on success.

This split keeps the porter's edit loop short — full-workspace gates and
hydration-asset rebuilds happen once at commit time, not on every iteration.

## Admin UI E2E

Scripted admin parity checks live under `tests/e2e/`. CI runs the full
configured Playwright suite by default through:

```bash
scripts/e2e/playwright-e2e.sh
```

The wrapper defaults Playwright to one worker because the full suite mutates
shared application state in its isolated test database. Set
`ERSATZRS_E2E_WORKERS=<n>` or pass an explicit `--workers` argument only for
spec subsets that are known to be independent.

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
An FFmpeg operator override (`~/.local/ffmpeg-8.1`, like `run.sh`) is used when
present so startup skips the managed runtime download. Useful env knobs:
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

Support page parity criteria are captured from the live ErsatzTV Legacy
Support pages in `tests/e2e/support-page-criteria.ts`. The support spec can
seed the persisted health snapshot with `E2E_SEED_SUPPORT=1` when
`E2E_DB_PATH` or `ERSATZRS_DB_PATH` points at the SQLite test database.

## Per-page E2E index

Add a row when a slice lands. Link to the owning spec and keep the status in
sync with [STATUS.md](STATUS.md)'s Admin UI Hydration Parity table.

| Route                    | Status | Notes                                                                   |
| ------------------------ | ------ | ----------------------------------------------------------------------- |
| `/media/libraries`       | ✅     | Stable DOM shape, no `<Show>` in island.                                |
| `/search`                | ✅     | Stable DOM shape, no `<Show>` in island.                                |
| `/media/playlists`       | TODO   | Browser smoke covered add group, add playlist, edit/save redirect, preview empty state, delete playlist/group, and console warning/error check; add a durable Playwright spec when the route gets automated admin UI coverage. |
| `/media/rerun-collections` | ✅ | List route desktop/mobile hydration and overflow checked with Playwright MCP; add-editor POST save/delete fallback covered by `tests/e2e/hydration-pending-routes.spec.ts`. |
| `/channels`              | ✅     | Stable DOM shape, `DeleteDialog` always rendered. Stop-transcode button always in DOM, visibility toggled via `style:visibility` from the live HLS-session poll. |
| `/channels/numbers`      | ✅     | SSR channel-number editor; `tests/e2e/channels-edit-numbers.spec.ts` covers Edit Numbers navigation, validation, atomic swap persistence, mobile actions, and no-JS POST fallback. |
| `/channels/:id`          | ✅     | SSR multipart form; mode-specific fields visible without hydration.     |
| `/media/trakt/lists`     | ✅     | Add dialog, row Search/Match/Delete, delete persistence, editor save POST fallback, and console check covered by `tests/e2e/hydration-pending-routes.spec.ts`. |
| `/channels/add`          | ✅     | Same SSR form as `/channels/:id` (id=0), with logo upload/remove smoke. |
| `/ffmpeg/profiles`       | ✅     | Stable DOM shape, `DeleteDialog` always rendered.                       |
| `/ffmpeg/profiles/:id`   | ✅     | Island owns blocking resource, no Suspense wrapper.                     |
| `/media/sources/local`   | ✅     | Stable DOM shape, `DeleteDialog` always rendered.                       |
| `/media/sources/jellyfin`| ✅     | Stable DOM shape, `DeleteDialog` always rendered.                       |
| `/media/sources/emby`    | ✅     | Stable DOM shape, `DeleteDialog` always rendered.                       |
| `/playouts`              | TODO   | Island list+detail port of `Playouts.razor`. Browser-smoke: Add Playout dropdown, Reset All, search filter, row select → detail table, Show Filler toggle, Reset/Schedule Reset/Delete/Edit JSON dialogs, Troubleshoot context dialog, and a console warning/error check; add a durable Playwright spec when the route gets automated coverage. |
| `/schedules`             | Covered | `scheduling-route-family.spec.ts` covers hydrated list/add/items workflows, ordered Multiple-then-One save/reload persistence, filler-scope guidance, and screenshots; ADR 0019 adds localized cooldown controls plus desktop/mobile save/load proof. |
| `/templates`             | TODO   | Browser smoke covered list geometry, template group/template add/copy/delete confirmations, and console warning/error check; add a durable Playwright spec when the route gets automated admin UI coverage. |
| `/templates/:id`         | TODO   | Browser smoke covered editor geometry, browser-navigation edit link, block add/remove/drag/save, and no-JS item form POST fallback; add a durable Playwright spec when the route gets automated admin UI coverage. |
| `/media/sources/plex`    | ✅     | Initial source/token state resolves before the island boundary; seeded desktop/mobile hydration and console sweep clean. |
| `/media/sources/plex/:id/libraries` | ✅ | Seeded library preference toggle/save persistence covered by `tests/e2e/hydration-pending-routes.spec.ts`. |
| `/media/sources/plex/:id/paths` | ✅ | Seeded path add/save persistence plus desktop/mobile overflow checks covered by `tests/e2e/hydration-pending-routes.spec.ts` and Playwright MCP. |
| `/media/sources/jellyfin/:id/libraries` | ✅ | Seeded library preference toggle/save persistence covered by `tests/e2e/hydration-pending-routes.spec.ts`. |
| `/media/sources/jellyfin/:id/paths` | ✅ | Seeded path add/save persistence plus desktop/mobile overflow checks covered by `tests/e2e/hydration-pending-routes.spec.ts` and Playwright MCP. |
| `/media/sources/emby/:id/libraries` | ✅ | Seeded library preference toggle/save persistence covered by `tests/e2e/hydration-pending-routes.spec.ts`. |
| `/media/sources/emby/:id/paths` | ✅ | Seeded path add/save persistence plus desktop/mobile overflow checks covered by `tests/e2e/hydration-pending-routes.spec.ts` and Playwright MCP. |
| `/blocks`                | ✅     | Stable DOM shape, two `DeleteDialog`s always rendered; copy modal, copy redirect, and 3840x1918 visual comparison covered in `plans/blocks-parity-slice.md`. |
| `/blocks/:id`            | ✅     | Stable DOM shape, preview modal via `style:display`; hydrated add-item/preview/save plus no-JS item fallback verified in `plans/blocks-parity-slice.md`. |
| `/playouts/sequential/:id`| ✅    | Stable DOM shape, validation error banner always rendered.              |
| `/watermarks`            | ✅     | List body is an island; hydrated copy/delete, pager state, and clean final console verified. |
| `/watermarks/:id`        | ✅     | Stable DOM shape, all conditional sections via `style:display`.         |
| `/media/collections`     | ✅     | Stable DOM shape, `DeleteDialog` always rendered.                       |
| `/media/collections/:id` | TODO   | Detail page port: poster grid, hover-checkbox multi-select, per-card trashcan, inline rename pencil, "Use Custom Playback Order" toggle with Move Up/Down (drag-and-drop deferred). Island owns blocking resource; sections rendered conditionally over stable signal state. |
| `/media/collections/:id/edit` | ✅ | Rename-only form (port of `CollectionEditor.razor`).                  |
| `/media/filler/presets` | ✅ | List route desktop/mobile hydration and overflow checked with Playwright MCP; add-editor POST save/delete fallback covered by `tests/e2e/hydration-pending-routes.spec.ts`. |
| `/media/filler/presets/:id/edit` | ✅ | Editor initial form/options resolve before island emission; save posts to `/api/filler-presets/save` without hydration and is covered by `tests/e2e/hydration-pending-routes.spec.ts`. |
| `/system/health`         | ✅     | Live/persisted health rows match scripted Support parity criteria.       |
| `/system/logs`           | ✅     | Newest OS-local app-data log file renders in ErsatzTV-style table surface. |
| `/system/troubleshooting`| ✅     | ErsatzTV-style tools/report tabs match scripted Support parity criteria. |
| `/media/tv/shows/:id`   | ✅     | Detail page renders metadata facts and hydrated Add To Collection/Playlist/Schedule plus special-feature row actions without console/page errors; covered by `tests/e2e/content-variants.spec.ts`. Plex Scan buttons call targeted `scan_show`, Jellyfin/Emby/Local fall back to library scan. |
| `/media/tv/seasons/:id` | ✅     | Detail page renders fanart/poster header and hydrated Add To Collection/Playlist/Schedule plus special-feature row media-info/troubleshooting actions without console/page errors; covered by `tests/e2e/content-variants.spec.ts`. |
| `/media/movies`          | ✅     | Seeded URL-driven query plus item overflow menu, Add To Collection dialog, Show Media Info, and Troubleshoot Playback covered by `tests/e2e/hydration-pending-routes.spec.ts`. |
| `/media/music/songs`     | ✅     | Shared media-browser shell with right-aligned Select All toolbar, item overflow menu, Add To Collection dialog persistence, Show Media Info, Troubleshoot Playback, and bulk Add To Playlist collection-type stamping covered by `tests/e2e/media-browser-actions.spec.ts`. |
| `/media/browser/images`  | ✅     | Shared media-browser shell with right-aligned Select All toolbar, item overflow menu, Show Media Info, Troubleshoot Playback, and bulk Add To Playlist image collection-type stamping covered by `tests/e2e/media-browser-actions.spec.ts`. |
| `/media/trash`           | ✅     | Seeded Delete Selected dialog cancel, Empty Trash confirmation, and delete persistence covered by `tests/e2e/hydration-pending-routes.spec.ts`. |
| `/media/movies/:id`      | ✅     | Movie extras row treatment, Add To Collection/Playlist extra actions, alternate-edition cards, commentary audio/subtitle cards, and troubleshooting form selection preservation covered by `tests/e2e/content-variants.spec.ts`. |
| `/media/remote/streams`  | ✅     | Dedicated `RemoteStreamList.razor` parity route; card geometry, Add To Collection persistence, no-JS SSR HTML, and mobile menu covered by `tests/e2e/remote-streams.spec.ts`. |
| `/media/music/artists/:id` | ✅   | Artist detail resolves initial data before the island boundary; artist thumbnail/fanart, music-video thumbnail rendering, hydrated artist Add To Collection persistence/redirect, and no console/page errors are covered by `tests/e2e/media-browser-actions.spec.ts`. |
| `/system/troubleshooting/playback` | TODO | Media-item entry route accepts `?mediaItem=ID` and loads backend media-info context. |

## Fixture strategy

- **External services (Plex/Jellyfin/Emby/Trakt)** — `httpmock` helper used in
  `ersatzrs-infra` and `ersatzrs-scanner` integration tests.

## Adding a new test

1. **Pure logic** → `#[cfg(test)] mod tests` next to the code.
2. **Generated FFmpeg argv / filter graph / YAML round-trip** → snapshot test
   in the relevant crate, follow `docs/how-to/snapshot-tests.md`.
3. **Repository or external client behaviour** → integration test under
   `crates/<crate>/tests/`, reuse `httpmock` patterns.
4. **Admin UI behaviour** → add or run a focused Playwright spec when practical;
   otherwise document the browser probe used and add/update the matching row in
   the per-page E2E index above.
