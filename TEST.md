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
| Hydration smoke  | `@playwright/test` under `tests/e2e/`.                   | Admin UI islands actually attach handlers and the round-trip server fn fires.   |
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
- **Hydration smoke**: `npm run test:e2e` runs `@playwright/test` against a
  live ErsatzRS server. The Playwright config starts `./run.sh build` when no
  server is already listening on `127.0.0.1:8409`; set `E2E_BASE_URL` to point
  at an existing server. The runner uses an isolated SQLite db at
  `tests/e2e/.tmp/ErsatzRS.db` unless `DATABASE_URL` / `ERSATZRS_DB_PATH` are
  already set.

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

## Admin UI E2E

The old Markdown playbooks have been migrated to `@playwright/test`:

- Config: [playwright.config.ts](playwright.config.ts)
- Shared fixtures: [tests/e2e/fixtures.ts](tests/e2e/fixtures.ts)
- Admin hydration suite: [tests/e2e/admin-hydration.spec.ts](tests/e2e/admin-hydration.spec.ts)
- Seed fixture: [tests/fixtures/seed_admin.sql](tests/fixtures/seed_admin.sql)

Commands:

```bash
# Install the local Playwright dependency once.
npm install
npx playwright install chromium

# Run headless.
npm run test:e2e

# Run headed for debugging.
npm run test:e2e:headed
```

The Playwright `page` fixture records browser console errors and page errors,
then fails the test on hydration-related failures such as
`failed_to_cast_marker_node`. Tests call `seedAdmin()` before each destructive
case so each scenario starts from the same SQLite fixture.

Current baseline: `npx playwright test` starts the server and runs the migrated
suite, but the specs fail on Leptos hydration marker mismatches before some
handlers attach. Treat those failures as product bugs in the affected page
islands, not as harness gaps.

## Per-page E2E index

Add a row when a slice lands. Link to the owning spec and keep the status in
sync with [STATUS.md](STATUS.md)'s Admin UI Hydration Parity table.

| Route                   | Status   | Playbook                                                                                  |
| ----------------------- | -------- | ----------------------------------------------------------------------------------------- |
| `/media/libraries`      | ❌       | `tests/e2e/admin-hydration.spec.ts`; hydration mismatch.                                  |
| `/search`               | ❌       | `tests/e2e/admin-hydration.spec.ts`; hydration mismatch.                                  |
| `/channels`             | ❌       | `tests/e2e/admin-hydration.spec.ts`; hydration mismatch.                                  |
| `/channels/:id`         | ❌       | `tests/e2e/admin-hydration.spec.ts`; hydration mismatch.                                  |
| `/channels/add`         | ❌       | `tests/e2e/admin-hydration.spec.ts`; hydration mismatch.                                  |
| `/ffmpeg/profiles`      | ❌       | `tests/e2e/admin-hydration.spec.ts`; hydration mismatch.                                  |
| `/ffmpeg/profiles/:id`  | ❌       | `tests/e2e/admin-hydration.spec.ts`; hydration mismatch.                                  |
| `/media/sources/local`  | ❌       | `tests/e2e/admin-hydration.spec.ts`; hydration mismatch.                                  |
| `/media/sources/jellyfin`| TODO    | Awaits mock server.                                                                       |
| `/media/sources/emby`   | TODO     | Awaits mock server.                                                                       |
| `/playouts`             | TODO     | Needs URL-driven selection refactor.                                                      |

## Fixture strategy

- **Media files** — reuse `tests/fixtures/media/` for E2E fixtures. Do not
  commit copyrighted media; tiny silent samples only.
- **Admin database seed** — `tests/fixtures/seed_admin.sql` is the canonical
  fixture for hydration tests. It applies after `sqlx::migrate!` against
  the SQLite db selected by `ERSATZRS_DB_PATH`. `./run.sh seed` runs it.
  Keep it minimal:
  - 1 local library pointing at `tests/fixtures/media/`.
  - 1 channel.
  - 1 ffmpeg profile.
  - 1 searchable movie row.
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
4. **Admin UI behaviour** → add or extend a `tests/e2e/*.spec.ts` test plus
   the matching row in the E2E index above.
