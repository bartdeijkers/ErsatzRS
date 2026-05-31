# Emby/Plex Media-Source Pages Hydration Anomaly

## Summary

Several Emby and Plex media-source admin pages declare interactive handlers
(`Action`s, `on:click`, `on:change`, `RwSignal`s, `DeleteDialog`) directly
inside a plain `#[component]` with no enclosing `#[island]`. The app hydrates
with `leptos::mount::hydrate_islands()` (`crates/ersatzrs-web/src/lib.rs:25`),
so **only handlers declared inside an `#[island]` (or inside plain components
rendered within an island) attach on the client.** `App` → `AppShell` →
`AdminRoutes` are all plain `#[component]`s, so a page that is itself a plain
`#[component]` has no island ancestor and its wiring is inert after hydration.

This is the same defect that affected the Jellyfin pages and was fixed on
2026-05-30 by the canonical wrapper→island refactor. Use the corrected Jellyfin
pages as the reference implementation:

- `media_sources/jellyfin.rs`: `JellyfinSourcesPage` → `JellyfinSourcesIsland`,
  `JellyfinLibrariesPage` → `JellyfinLibrariesIsland`,
  `JellyfinPathsPage` → `JellyfinPathsIsland` (the last passes the `source_id`
  route param as a plain prop into the island).
- Already-correct siblings: `JellyfinConnectionPage` /
  `EmbyConnectionPage` (→ `*ConnectionEditorIsland`), and `PlexSourcesPage` /
  `PlexSignInIsland`.

## Affected pages (current state)

`crates/ersatzrs-web/src/admin/pages/media_sources/emby.rs`:

- `EmbySourcesPage` (plain `#[component]`) — row **Delete** `on:click` +
  `DeleteDialog` are inert. Mirror `JellyfinSourcesPage` → `EmbySourcesIsland`.
- `EmbyLibrariesPage` (plain `#[component]`) — **Save** `on:click` and per-row
  sync **checkbox** `on:change` are inert. Mirror
  `JellyfinLibrariesPage` → `EmbyLibrariesIsland(source_id: i64)`.
- `EmbyPathsPage` (plain `#[component]`) — **Save** `on:click`, plus **Add** /
  **Remove** in the shared `PathsEditor`, are inert. The page also calls
  `source_id()` as a closure in the Save dispatch. Mirror
  `JellyfinPathsPage` → `EmbyPathsIsland(source_id: i64)` and replace
  `source_id()` with the `source_id` prop.

`crates/ersatzrs-web/src/admin/pages/media_sources/plex.rs`:

- `PlexLibrariesPage` (plain `#[component]`) — **Save** `on:click` and sync
  **checkbox** `on:change` are inert. Mirror
  `PlexLibrariesPage` → `PlexLibrariesIsland(source_id: i64)`.
- `PlexPathsPage` (plain `#[component]`) — **Save** `on:click` + `PathsEditor`
  Add/Remove inert; `source_id()` closure in Save dispatch. Mirror
  `PlexPathsPage` → `PlexPathsIsland(source_id: i64)`.
- `PlexSourcesPage` is **already** a thin wrapper over `PlexSourcesIsland` — no
  change needed.

The shared `PathsEditor` sub-component (imported via
`use super::jellyfin::PathsEditor;` in both `emby.rs` and `plex.rs`) is a plain
`#[component]`; it becomes interactive automatically once rendered inside an
island. It does **not** use `<Show>`, so TEST.md rule 2 is satisfied.

## Why it matters

- **Save / Add / Remove / Delete and sync checkboxes silently do nothing** on
  these pages after hydration. The page renders correctly via SSR (dialogs are
  present-but-hidden via `style:display`), which is exactly why it slips past a
  shallow check — the same way the Jellyfin Paths Save button looked fine but
  was dead.
- It contradicts TEST.md rule 1 and is a latent trap for anyone copying these
  files as a template.

## Confirm first (one browser probe per page)

Per TEST.md's containment playbook, capture the regression target before
editing:

1. `./run.sh serve` (rebuilds Leptos hydration assets).
2. Playwright MCP `browser_navigate` to each page in a fresh context:
   `/media/sources/emby`, `/media/sources/emby/{id}/libraries`,
   `/media/sources/emby/{id}/paths`, `/media/sources/plex/{id}/libraries`,
   `/media/sources/plex/{id}/paths`.
3. Exercise the one interaction (Add a path + Save, toggle a sync checkbox +
   Save, or row Delete) and capture `browser_console_messages` (watch for
   `failed_to_cast_marker_node`). **Expected current (buggy) behaviour:** no
   server fn fires and no UI state changes.

## Fix

For each affected page, apply the canonical wrapper→island refactor exactly as
done for Jellyfin:

1. Rename the page body to `#[island] fn <Name>Island(...)`. For pages that read
   the `id` route param, keep `use_params_map()` in the thin `#[component]`
   wrapper and pass the parsed `source_id: i64` as a plain prop into the island
   (route-param props are not the prop-bearing-island-across-`Suspense` hazard
   of rule 3 — see `JellyfinPathsPage`/`PlayoutEditorPage`).
2. Replace any `source_id()` closure calls inside the island with the
   `source_id` prop value.
3. Add the thin wrapper:
   ```rust
   #[component]
   pub fn EmbyPathsPage() -> impl IntoView {
       let params = use_params_map();
       let source_id =
           params.with(|p| p.get("id").and_then(|s| s.parse::<i64>().ok()).unwrap_or(0));
       view! { <EmbyPathsIsland source_id=source_id/> }
   }
   ```
4. Do **not** introduce `<Show>` inside an island (rule 2). `DeleteDialog`/
   `Modal` already render always with `style:display` (rule 4).

## Verification

- `cargo check -p ersatzrs-web --no-default-features --features ssr`
- `cargo check -p ersatzrs-web --lib --no-default-features --features hydrate
  --target wasm32-unknown-unknown` (island codegen gate).
- `cargo xtask hydration-lint -p ersatzrs-web` if available (expect no new
  findings), and `cargo xtask parity-check web`.
- Playwright MCP smoke per page: path Add + Save persists (verify DB row),
  library sync toggle + Save, Emby source Delete (cancel + confirm), and a
  console warning/error check.

## Scope / risk

- Edits confined to `media_sources/emby.rs` and `media_sources/plex.rs`; no
  server-fn, schema, CSS, or shared-primitive changes. Low risk, but each
  touches a hydrated page, so the before/after browser probe is mandatory per
  TEST.md.
- Independent of the playback FFmpeg-profile work
  (`plans/playback-ffmpeg-profile-loading.md`).
