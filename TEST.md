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
| Hydration smoke  | Manual Playwright MCP playbooks (see below).             | Admin UI islands actually attach handlers and the round-trip server fn fires.   |
| CI               | `.github/workflows/ci.yml`                               | fmt, clippy, test, cargo-deny, commitlint, docker bake.                         |

References:

- [docs/explanation/test-strategy.md](docs/explanation/test-strategy.md) — why
  the layers exist.
- [docs/how-to/testing.md](docs/how-to/testing.md) — fixture rules, in-memory
  SQLite vs testcontainers MySQL, ignored tests.
- [docs/how-to/snapshot-tests.md](docs/how-to/snapshot-tests.md) — `cargo
  insta review` workflow and snapshot naming.

## What runs where

- **CI (every push and PR)**: layers 1–4. The `mysql-integration` feature is
  enabled there, so the testcontainers MariaDB run is part of CI.
- **Local dev**: layers 1–3 by default (`cargo test --workspace`). Add
  `--all-features` to include the MySQL integration locally — it pulls a
  MariaDB image so it is slow on a cold cache.
- **Hydration smoke**: run manually via the Playwright MCP tool. Start the
  server with `./run.sh dev`, then use the browser MCP playbook for the page
  under test. See the per-page index below for the expected interactions.

## Hydration rules (read before writing islands)

The hydration smoke layer exists because Leptos islands have sharp edges that
unit/snapshot tests cannot catch. These rules come from real fixes shipped on
the libraries page (commits 5484132, 4190407) and later admin island
stabilisation. Apply them every time you wire interactive UI:

1. **Page wrapper stays `#[component]`.** The interactive body becomes
   `#[island]`. `hydrate_islands()` only attaches handlers declared inside
   `#[island]`. Reference: `crates/ersatzrs-web/src/admin/pages/libraries/mod.rs:11-17`.
2. **Stable DOM shape.** Replace `cond.then(|| view!{...})` and `<Show when=...>` with
   always-render plus `style:display=move || if cond { "" } else { "none" }` and
   `disabled=move || !cond`. Conditional sub-trees crash hydration with
   `failed_to_cast_marker_node`. **Never use `<Show>` inside `#[island]`.**
   This applies to `#[component]`s called inside islands too (e.g. `ItemEditor` inside
   `BlockEditorIsland`). The `Modal` and `DeleteDialog` primitives use `style:display`
   internally — never reintroduce `<Show>` in them.
   Reference: `crates/ersatzrs-web/src/admin/pages/libraries/mod.rs`.
3. **No prop-bearing islands across `Suspense` boundaries.** If the parent
   page hydrates props from a server resource, have the island own a
   `Resource::new_blocking` internally and read from `data.get_untracked()` to
   seed signals — the blocking resource is resolved before HTML streams on SSR
   and deserialized immediately on the client. Pass only a plain id (e.g. `i64`)
   from the page wrapper to the island. Reference:
   `FfmpegProfileEditorIsland`.
   `ChannelEditorPage` is intentionally an SSR multipart form now because the
   broader app still has an island marker mismatch; do not use it as an island
   reference pattern.
4. **`DeleteDialog` always rendered.** `DeleteDialog` takes `entity_name: Signal<String>`
   (a reactive signal, not a `String`). Keep the dialog in the DOM at all times — control
   visibility via the `open: RwSignal<bool>` signal. Never conditionally render a dialog via
   `delete_target.get().map(|...| view!{<DeleteDialog .../>})`. Derive the name as
   `Signal::derive(move || delete_target.get().map(|t| t.name.clone()).unwrap_or_default())`.
5. **Server-state polling pattern.** When the UI needs a server-driven signal
   (e.g. lock state), copy the wasm32-gated `Effect` from
   `libraries/mod.rs:39-56`: `gloo_timers::future::TimeoutFuture::new(1500)`
   in a `loop`, behind `#[cfg(target_arch = "wasm32")]`.

## Hydration and UI bug containment

Use this playbook before changing Leptos hydration, islands, shared UI
primitives, router setup, or Cargo dependencies for an admin UI bug. Hydration
failures often surface far away from the first mismatch, so treat them as a
boundary diagnosis problem before refactoring.

1. **Classify the failure first.** Decide whether the bug is SSR HTML shape,
   island hydration/runtime, server function/API, CSS/layout, or data/query
   behavior. Record the current evidence in the handoff or final response.
2. **Create one browser probe before editing.** Use Playwright MCP to navigate
   to the route, capture current console errors, and exercise exactly one
   failing interaction. That probe becomes the regression target.
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
  `<Suspense>` region (rule 3 above).

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

- `cargo fmt --all -- --check`
- `cargo xtask hydration-lint -p ersatzrs-web`
- `cargo test -p ersatzrs-web`
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
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-features`
- `cargo leptos build` from `crates/ersatzrs-web` (rebuilds hydration assets).
- Playwright MCP smoke per the playbook for the touched page.
- Flips the per-page index row from `TODO` to `✅` on success.

This split keeps the porter's edit loop short — full-workspace gates and
hydration-asset rebuilds happen once at commit time, not on every iteration.

## Admin UI E2E

Hydration smoke tests can be run manually via the Playwright MCP tool. Scripted
admin parity checks live under `tests/e2e/` and run with `npm run test:e2e`.
Point them at a running server with `E2E_BASE_URL=http://127.0.0.1:8409`.

Workflow:
1. Start the server: `./run.sh dev` (or `cargo run --bin ersatzrs-web`).
2. Use the Playwright MCP `browser_navigate` / `browser_snapshot` / `browser_click`
   tools to navigate to the page and exercise interactive islands.
3. Check browser console for `failed_to_cast_marker_node` or other hydration
   errors; any such error is a product bug in the affected island.

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
| `/media/rerun-collections` | TODO | Browser smoke covered list render, add/save redirect, delete dialog, cleanup, route geometry screenshot, and console warning/error check; add a durable Playwright spec when the route gets automated admin UI coverage. |
| `/channels`              | ✅     | Stable DOM shape, `DeleteDialog` always rendered.                       |
| `/channels/:id`          | ✅     | SSR multipart form; mode-specific fields visible without hydration.     |
| `/media/trakt/lists`     | TODO   | Browser smoke should cover Add dialog, row Search/Match/Delete, pager, editor save POST fallback, visual geometry, and console warning/error check; see `plans/trakt-lists-parity-slice.md`. |
| `/channels/add`          | ✅     | Same SSR form as `/channels/:id` (id=0), with logo upload/remove smoke. |
| `/ffmpeg/profiles`       | ✅     | Stable DOM shape, `DeleteDialog` always rendered.                       |
| `/ffmpeg/profiles/:id`   | ✅     | Island owns blocking resource, no Suspense wrapper.                     |
| `/media/sources/local`   | ✅     | Stable DOM shape, `DeleteDialog` always rendered.                       |
| `/media/sources/jellyfin`| ✅     | Stable DOM shape, `DeleteDialog` always rendered.                       |
| `/media/sources/emby`    | ✅     | Stable DOM shape, `DeleteDialog` always rendered.                       |
| `/playouts`              | TODO   | Needs URL-driven selection refactor.                                    |
| `/schedules`             | TODO   | Browser smoke covered list/add/items route geometry, screenshots, no-JS schedule and item form POST fallbacks, and hydrated item editing; add a durable Playwright spec when the route gets automated admin UI coverage. |
| `/templates`             | TODO   | Browser smoke covered list geometry, template group/template add/copy/delete confirmations, and console warning/error check; add a durable Playwright spec when the route gets automated admin UI coverage. |
| `/templates/:id`         | TODO   | Browser smoke covered editor geometry, browser-navigation edit link, block add/remove/drag/save, and no-JS item form POST fallback; add a durable Playwright spec when the route gets automated admin UI coverage. |
| `/media/sources/plex`    | ✅     | Stable DOM shape, `PlexSignInIsland` all-arms always rendered.          |
| `/blocks`                | ✅     | Stable DOM shape, two `DeleteDialog`s always rendered; copy modal, copy redirect, and 3840x1918 visual comparison covered in `plans/blocks-parity-slice.md`. |
| `/blocks/:id`            | ✅     | Stable DOM shape, preview modal via `style:display`; hydrated add-item/preview/save plus no-JS item fallback verified in `plans/blocks-parity-slice.md`. |
| `/playouts/sequential/:id`| ✅    | Stable DOM shape, validation error banner always rendered.              |
| `/watermarks`            | ✅     | Stable DOM shape, `DeleteDialog` always rendered.                       |
| `/watermarks/:id`        | ✅     | Stable DOM shape, all conditional sections via `style:display`.         |
| `/media/collections`     | ✅     | Stable DOM shape, `DeleteDialog` always rendered.                       |
| `/media/collections/:id` | TODO   | Detail page port: poster grid, hover-checkbox multi-select, per-card trashcan, inline rename pencil, "Use Custom Playback Order" toggle with Move Up/Down (drag-and-drop deferred). Island owns blocking resource; sections rendered conditionally over stable signal state. |
| `/media/collections/:id/edit` | ✅ | Rename-only form (port of `CollectionEditor.razor`).                  |
| `/media/filler/presets` | TODO | Stable DOM shape, `DeleteDialog` always rendered; HTML POST fallback delete route. |
| `/media/filler/presets/:id/edit` | TODO | Editor island owns blocking resources; save also posts to `/api/filler-presets/save` without hydration. |
| `/system/health`         | ✅     | Live/persisted health rows match scripted Support parity criteria.       |
| `/system/logs`           | ✅     | Newest OS-local app-data log file renders in ErsatzTV-style table surface. |
| `/system/troubleshooting`| ✅     | ErsatzTV-style tools/report tabs match scripted Support parity criteria. |
| `/media/tv/shows/:id`   | TODO   | Detail page renders metadata facts including Languages; Plex Scan buttons call targeted `scan_show`, Jellyfin/Emby/Local fall back to library scan; needs manual Add-To/Scan hydration smoke. |
| `/media/tv/seasons/:id` | TODO   | Detail page renders fanart/poster header plus vertical episode cards; needs manual Add-To, Show Media Info, and Troubleshoot Playback hydration smoke. |
| `/media/movies`          | TODO   | URL-driven pager (`?query=…&page=N`) + Jellyfin/Emby artwork proxy; item overflow menu opens Add To Collection, Show Media Info, and Troubleshoot Playback. |
| `/media/movies/:id`      | ✅     | Blocking resource details page with fanart, poster, plot, metadata.       |
| `/media/music/artists/:id` | ✅ | Detail page renders fanart/thumbnail header, linked facts, artist Add-To actions, and first 100 music videos; modal-open hydration smoke clean. |
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
4. **Admin UI behaviour** → verify manually with the Playwright MCP tool and
   add/update the matching row in the per-page E2E index above.
