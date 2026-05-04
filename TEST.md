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
| Hydration smoke  | This file — Playwright MCP playbooks (see below).        | Admin UI islands actually attach handlers and the round-trip server fn fires.   |
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
- **Hydration smoke (this file)**: agent-driven, runs from a Claude Code
  session against a live `./run.sh dev` server. Not in CI yet — see
  "Future: scripted Playwright runner" below.

## Hydration rules (read before writing islands)

The hydration smoke layer exists because Leptos islands have sharp edges that
unit/snapshot tests cannot catch. These rules come from real fixes shipped on
the libraries page (commits 5484132, 4190407) and the channel editor
stabilisation. Apply them every time you wire interactive UI:

1. **Page wrapper stays `#[component]`.** The interactive body becomes
   `#[island]`. `hydrate_islands()` only attaches handlers declared inside
   `#[island]`. Reference: `crates/ersatzrs-web/src/admin/pages/libraries/mod.rs:11-17`.
2. **Stable DOM shape.** Replace `cond.then(|| view!{...})` with always-render
   plus `style:display=move || if cond { "" } else { "none" }` and
   `disabled=move || !cond`. Conditional sub-trees crash hydration with
   `failed_to_cast_marker_node`. Reference:
   `crates/ersatzrs-web/src/admin/pages/libraries/mod.rs:104-135`.
3. **No prop-bearing islands across `Suspense` boundaries.** If the parent
   page hydrates props from a server resource, fetch the data inside the
   island via its own server fn or a blocking SSR resource, *or* lift the
   selection state to a URL query param (`use_query_map`) so children
   re-derive from the URL.
4. **Server-state polling pattern.** When the UI needs a server-driven signal
   (e.g. lock state), copy the wasm32-gated `Effect` from
   `libraries/mod.rs:39-56`: `gloo_timers::future::TimeoutFuture::new(1500)`
   in a `loop`, behind `#[cfg(target_arch = "wasm32")]`.

## Playbook format

A playbook is a numbered Markdown sequence the agent runs through the
`mcp__plugin_playwright_playwright__browser_*` MCP tools. Each playbook is a
small reproducible script that asserts hydration works and the server fn
fires.

Template:

```markdown
## Page: <route>

**Setup:** seed `<fixture-name>` (or `none` if the page does not need data).

**Steps:**
1. `browser_navigate` → `http://127.0.0.1:8409/<route>`
2. `browser_snapshot` → assert <element> is present and in the expected state.
3. `browser_click` → <accessibility-name of the control>.
4. `browser_console_messages` → assert no `failed_to_cast_marker_node`
   and no uncaught JS errors.
5. `browser_wait_for` → assert <user-visible side effect> within <N> seconds.

**Acceptance:**
- Server log (`./run.sh log`) shows <server fn name> invocation.
- <UI state assertion>.
- No hydration errors in console.
```

## Per-page playbook index

Add a row when a slice lands. Link the playbook source if it grows past the
short form below — long playbooks live in `docs/playbooks/<page>.md`.

| Route                   | Status   | Playbook                                                                                  |
| ----------------------- | -------- | ----------------------------------------------------------------------------------------- |
| `/media/libraries`      | ✅       | Inline below.                                                                             |
| `/search`               | ✅       | Inline below.                                                                             |
| `/channels`             | ✅       | Inline below.                                                                             |
| `/channels/:id`         | ✅       | Inline below.                                                                             |
| `/channels/add`         | ✅       | Same playbook as `/channels/:id`.                                                         |
| `/ffmpeg/profiles`      | ✅       | Inline below.                                                                             |
| `/ffmpeg/profiles/:id`  | ✅       | Inline below.                                                                             |
| `/media/sources/local`  | ✅       | Inline below.                                                                             |
| `/media/sources/jellyfin`| TODO    | Awaits mock server.                                                                       |
| `/media/sources/emby`   | TODO     | Awaits mock server.                                                                       |
| `/playouts`             | TODO     | Needs URL-driven selection refactor.                                                      |

### Page: `/media/libraries`

**Setup:** seed `local-library-testdata` (`./run.sh seed`).

**Steps:**
1. `browser_navigate` → `http://127.0.0.1:8409/media/libraries`.
2. `browser_snapshot` → assert table renders one local library row with the
   "Scan" button enabled and "Deep Scan" hidden (local library only).
3. `browser_click` → "Scan" on row 1.
4. `browser_console_messages` → assert no `failed_to_cast_marker_node`.
5. `browser_wait_for` → toast text "Scan queued" within 2s.
6. `browser_snapshot` → assert spinner appears on the row (lock polling).

**Acceptance:**
- `ersatzrs.log` shows `scan_library` invocation with `elapsed_ms` tracing.
- Spinner clears within ~30s for the small fixture; library row shows
  updated item count after refetch.
- No hydration errors.

### Page: `/channels`

**Setup:** seed `local-library-testdata` (`./run.sh seed`).

**Steps:**
1. `browser_navigate` → `http://127.0.0.1:8409/channels`.
2. `browser_snapshot` → assert table renders the seeded channel row with
   "Edit" / "Delete" controls in the actions cell.
3. `browser_click` → "Delete channel" on row 1.
4. `browser_snapshot` → assert the `DeleteDialog` is visible and shows the
   channel name.
5. `browser_click` → "Delete" inside the dialog.
6. `browser_console_messages` → assert no `failed_to_cast_marker_node`.
7. `browser_wait_for` → row count decremented and the row no longer present
   within 2s (the resource refetches via the `delete_action` effect).

**Acceptance:**
- `ersatzrs.log` shows `delete_channel` invocation.
- Counter at the top of the table reflects the new total.
- `Show Disabled` toggle still flips visibility (proves checkbox `on:change`
  fires post-hydration).

### Page: `/search`

**Setup:** seed `local-library-testdata` (`./run.sh seed`).

**Steps:**
1. `browser_navigate` → `http://127.0.0.1:8409/search`.
2. `browser_type` → search input "test".
3. `browser_press_key` → Enter.
4. `browser_console_messages` → assert no `failed_to_cast_marker_node`.
5. `browser_wait_for` → result list renders within 2s.

**Acceptance:**
- `ersatzrs.log` shows `search_media` invocation.
- Result list contains the seeded item titles.

### Page: `/ffmpeg/profiles`

**Setup:** seed `local-library-testdata` (`./run.sh seed`) — includes one
seeded ffmpeg profile.

**Steps:**
1. `browser_navigate` → `http://127.0.0.1:8409/ffmpeg/profiles`.
2. `browser_snapshot` → assert table renders the seeded profile row with
   "Edit" / "Delete" buttons.
3. `browser_click` → "Delete" on row 1.
4. `browser_snapshot` → assert `DeleteDialog` shows the profile name.
5. `browser_click` → "Delete" inside the dialog.
6. `browser_console_messages` → assert no `failed_to_cast_marker_node`.
7. `browser_wait_for` → row disappears within 2s.

**Acceptance:**
- `ersatzrs.log` shows `delete_ffmpeg_profile` invocation.
- Total count above the table reflects the new total.

### Page: `/ffmpeg/profiles/:id`

**Setup:** seed `local-library-testdata` (`./run.sh seed`).

**Steps:**
1. `browser_navigate` → `http://127.0.0.1:8409/ffmpeg/profiles/1`.
2. `browser_snapshot` → assert form fields populate from the seeded profile;
   the hardware-acceleration accordion is hidden when `normalize_video` is
   off and visible when on.
3. `browser_click` → toggle "Normalize Video" checkbox.
4. `browser_snapshot` → assert hardware accordion appears (style:display
   toggles, no DOM remount).
5. `browser_click` → "Save".
6. `browser_console_messages` → assert no `failed_to_cast_marker_node`.
7. `browser_wait_for` → toast text "Saved" within 2s.

**Acceptance:**
- `ersatzrs.log` shows `save_ffmpeg_profile` invocation.
- Reloading the page preserves the new `normalize_video` value.
- "Copy Profile" button opens the modal and creates a new profile that
  navigates to its own editor at `/ffmpeg/profiles/{new_id}`.

### Page: `/channels/:id`

**Setup:** seed `local-library-testdata` (`./run.sh seed`) — includes one
seeded channel + ffmpeg profile.

**Steps:**
1. `browser_navigate` → `http://127.0.0.1:8409/channels/1`.
2. `browser_snapshot` → assert form fields populate (number, name, ffmpeg
   profile dropdown contains seeded profile).
3. `browser_type` → change "Name" field.
4. `browser_select_option` → change "Streaming Mode" to "HLS Segmenter".
5. `browser_click` → "Save".
6. `browser_console_messages` → assert no `failed_to_cast_marker_node`.
7. `browser_wait_for` → redirect to `/channels` within 2s.
8. `browser_snapshot` → assert the renamed channel appears in the list.

**Acceptance:**
- `ersatzrs.log` shows the channel form POST handler invocation.
- Required-field validation fires before submit if Number, Name, or FFmpeg
  Profile are empty (no DOM rebuild — error spans toggle via signal).
- The same playbook applies to `/channels/add` (use `LocalLibraryForm
  ::new_defaults` defaults; expect a new row to appear in `/channels`
  after save).

### Page: `/media/sources/local`

**Setup:** none — the page is the entry point for adding a library.

**Steps:**
1. `browser_navigate` → `http://127.0.0.1:8409/media/sources/local`.
2. `browser_click` → "+ Add Library".
3. `browser_type` → fill "Name" with `Test Library`, "Library Paths" with
   the absolute path to `testdata/`.
4. `browser_click` → "Add".
5. `browser_wait_for` → redirect to `/media/sources/local` within 2s.
6. `browser_snapshot` → assert the new row is present in the table.
7. `browser_click` → "Delete" on the new row.
8. `browser_click` → "Delete" inside the dialog.
9. `browser_console_messages` → assert no `failed_to_cast_marker_node`.
10. `browser_wait_for` → row disappears within 2s.

**Acceptance:**
- The add flow uses the static HTML form (no JS required); the delete flow
  goes through the `LocalLibrariesIsland` and the `DeleteDialog`.
- `ersatzrs.log` shows `delete_local_library` invocation.
- After add, `/media/libraries` shows the new library (overlap with the
  libraries playbook).

## Fixture strategy

- **Media files** — reuse `testdata/` already in the repo. Do not commit
  copyrighted media; tiny silent samples only.
- **Admin database seed** — `tests/fixtures/seed_admin.sql` is the canonical
  fixture for hydration playbooks. It applies after `sqlx::migrate!` against
  the SQLite db at `~/.ersatzrs/ErsatzRS.db` (override with
  `ERSATZRS_DB_PATH`). `./run.sh seed` runs it. Keep it minimal:
  - 1 local library pointing at `testdata/`.
  - 1 channel.
  - 1 ffmpeg profile.
  - 1 sequential playout.
- **External services (Plex/Jellyfin/Emby/Trakt)** — `httpmock` helper used in
  `ersatzrs-infra` and `ersatzrs-scanner` integration tests. Add a shared
  helper crate when the first browser-side test needs a mocked external API
  (deferred until Slice E).

## Adding a new test

1. **Pure logic** → `#[cfg(test)] mod tests` next to the code.
2. **Generated FFmpeg argv / filter graph / YAML round-trip** → snapshot test
   in the relevant crate, follow `docs/how-to/snapshot-tests.md`.
3. **Repository or external client behaviour** → integration test under
   `crates/<crate>/tests/`, reuse `httpmock` patterns.
4. **Admin UI behaviour** → add a playbook entry above plus the matching row
   in the index.

## Future: scripted Playwright runner

When the agent-driven loop becomes a bottleneck, migrate the playbooks to
`@playwright/test` under `tests/e2e/`. The numbered steps are already in the
right shape — each becomes a `test()` block, the MCP tool calls map 1:1 to
the JS Playwright API, and `browser_console_messages` becomes
`page.on('console', ...)`. Until then, this file is the source of truth.
