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
the libraries page (commits 5484132, 4190407) and the channel editor
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
   from the page wrapper to the island. Reference: `ChannelEditorIsland` and
   `FfmpegProfileEditorIsland`.
4. **`DeleteDialog` always rendered.** `DeleteDialog` takes `entity_name: Signal<String>`
   (a reactive signal, not a `String`). Keep the dialog in the DOM at all times — control
   visibility via the `open: RwSignal<bool>` signal. Never conditionally render a dialog via
   `delete_target.get().map(|...| view!{<DeleteDialog .../>})`. Derive the name as
   `Signal::derive(move || delete_target.get().map(|t| t.name.clone()).unwrap_or_default())`.
5. **Server-state polling pattern.** When the UI needs a server-driven signal
   (e.g. lock state), copy the wasm32-gated `Effect` from
   `libraries/mod.rs:39-56`: `gloo_timers::future::TimeoutFuture::new(1500)`
   in a `loop`, behind `#[cfg(target_arch = "wasm32")]`.

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
| `/channels`              | ✅     | Stable DOM shape, `DeleteDialog` always rendered.                       |
| `/channels/:id`          | ✅     | Island owns blocking resource, no Suspense wrapper.                     |
| `/channels/add`          | ✅     | Same island as `/channels/:id` (id=0).                                  |
| `/ffmpeg/profiles`       | ✅     | Stable DOM shape, `DeleteDialog` always rendered.                       |
| `/ffmpeg/profiles/:id`   | ✅     | Island owns blocking resource, no Suspense wrapper.                     |
| `/media/sources/local`   | ✅     | Stable DOM shape, `DeleteDialog` always rendered.                       |
| `/media/sources/jellyfin`| ✅     | Stable DOM shape, `DeleteDialog` always rendered.                       |
| `/media/sources/emby`    | ✅     | Stable DOM shape, `DeleteDialog` always rendered.                       |
| `/playouts`              | TODO   | Needs URL-driven selection refactor.                                    |
| `/media/sources/plex`    | ✅     | Stable DOM shape, `PlexSignInIsland` all-arms always rendered.          |
| `/blocks`                | ✅     | Stable DOM shape, two `DeleteDialog`s always rendered.                  |
| `/blocks/:id`            | ✅     | Stable DOM shape, preview modal via `style:display`.                    |
| `/playouts/sequential/:id`| ✅    | Stable DOM shape, validation error banner always rendered.              |
| `/watermarks`            | ✅     | Stable DOM shape, `DeleteDialog` always rendered.                       |
| `/watermarks/:id`        | ✅     | Stable DOM shape, all conditional sections via `style:display`.         |
| `/media/collections`     | ✅     | Stable DOM shape, `DeleteDialog` always rendered.                       |
| `/media/collections/:id` | TODO   | Detail page port: poster grid, hover-checkbox multi-select, per-card trashcan, inline rename pencil, "Use Custom Playback Order" toggle with Move Up/Down (drag-and-drop deferred). Island owns blocking resource; sections rendered conditionally over stable signal state. |
| `/media/collections/:id/edit` | ✅ | Rename-only form (port of `CollectionEditor.razor`).                  |
| `/system/health`         | ✅     | Live/persisted health rows match scripted Support parity criteria.       |
| `/system/logs`           | ✅     | Newest OS-local app-data log file renders in ErsatzTV-style table surface. |
| `/system/troubleshooting`| ✅     | ErsatzTV-style tools/report tabs match scripted Support parity criteria. |
| `/media/movies`          | TODO   | URL-driven pager (`?query=…&page=N`) + Jellyfin/Emby artwork proxy; item overflow menu opens Add To Collection, Show Media Info, and Troubleshoot Playback. |
| `/media/movies/:id`      | ✅     | Blocking resource details page with fanart, poster, plot, metadata.       |
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
