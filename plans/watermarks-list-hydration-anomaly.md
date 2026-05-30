# Watermarks List Hydration Anomaly

## Summary

`crates/ersatzrs-web/src/admin/pages/watermarks/list.rs` declares
`WatermarksListPage` as a plain `#[component]` that *directly* contains
interactive handlers (`Action`s, `on:click` delete/copy, reactive `RwSignal`s,
`DeleteDialog`/`Modal`). This violates the project's hydration contract.

The app hydrates with `leptos::mount::hydrate_islands()` (see
`crates/ersatzrs-web/src/lib.rs:25`). In islands mode, **only handlers declared
inside an `#[island]` (or inside plain components rendered *within* an island)
attach on the client.** `App` → `AppShell` → `AdminRoutes` are all plain
`#[component]`s with no enclosing island, so a page that is itself a plain
`#[component]` has **no island ancestor** and its `on:click`/`Action` wiring is
inert after hydration.

Every other interactive list page follows the canonical pattern from TEST.md
rule 1 — a thin `#[component]` wrapper that renders a single `#[island]` body:

- `channels/list.rs`: `ChannelsListPage` (`#[component]`) → `ChannelsListIsland`
  (`#[island]`).
- `ffmpeg_profiles/list.rs`: `FfmpegProfilesListPage` → `FfmpegProfilesListIsland`.
- `libraries/mod.rs`: `LibrariesPage` → `LibrariesIsland` (the reference cited
  by TEST.md rule 1).

`watermarks/list.rs` is the lone exception. The `/watermarks` row in the
TEST.md per-page index is marked `✅`, which most likely reflects a manual smoke
of the *editor* (`/watermarks/:id`, which **is** a proper island) and SSR render
of the list — not a verified hydrated delete/copy on the **list** page.

## Why it matters

- **Delete / Copy buttons silently do nothing** on `/watermarks` after
  hydration: the `on:click` closures never attach, so `delete_open`/`copy_open`
  never flip, the `DeleteDialog`/`Modal` never open, and `delete_action`/
  `copy_action` never dispatch. The page looks correct (SSR renders the table,
  the dialogs are present-but-hidden) which is exactly why this slips through a
  shallow check.
- It contradicts TEST.md rule 1 and the hydration-lint intent, so it is a
  latent trap for the next person who copies this file as a template.

## Confirm first (one browser probe)

Before editing, capture the regression target per TEST.md's containment
playbook:

1. `./run.sh serve` (rebuilds Leptos hydration assets + `/public/bundle.css`).
2. Playwright MCP: `browser_navigate` to `/watermarks` in a fresh context.
3. Click a row Delete button. **Expected current (buggy) behaviour:** no dialog
   appears and no network request fires. Capture `browser_console_messages`
   (look for `failed_to_cast_marker_node` too).
4. Repeat for Copy.

If the dialog *does* open and the server fn *does* fire, the anomaly is benign
on this Leptos version (components inside the routed tree are being hydrated
more broadly than rule 1 assumes) — in that case downgrade this to a
consistency cleanup rather than a bug fix, and note the finding in TEST.md.

## Fix

Apply the canonical wrapper→island refactor, mirroring `channels/list.rs`:

1. Rename the current `WatermarksListPage` body to
   `#[island] fn WatermarksListIsland()` (move all signals, actions, effects,
   `columns`, `row_actions`, dialogs, and the `view!` into it unchanged).
2. Add a thin wrapper:
   ```rust
   #[component]
   pub fn WatermarksListPage() -> impl IntoView {
       view! { <WatermarksListIsland/> }
   }
   ```
3. Keep the DOM shape stable: `DeleteDialog`/`Modal` already render always with
   `style:display` (rule 4) — no change needed there. Do **not** introduce
   `<Show>` inside the island (rule 2). `PagedTable` stays a `#[component]`
   rendered inside the island, which is correct.
4. Re-run `cargo xtask hydration-lint -p ersatzrs-web` (expect no new findings).
5. Re-run the browser probe: Delete opens the confirm dialog and dispatches;
   Copy opens the rename modal, dispatches, toasts, and redirects to the new id.

## Verification

- `cargo xtask parity-check web`
- Playwright MCP smoke on `/watermarks`: row Delete (cancel + confirm), row Copy
  (rename + redirect), pager, and a console warning/error check.
- Flip the TEST.md `/watermarks` row note to record that the **list** page is
  now a proper island and that hydrated delete/copy are verified.

## Scope / risk

- Single-file refactor (`watermarks/list.rs`); no server-fn, schema, CSS, or
  shared-primitive changes. Low risk, but it touches a hydrated page, so the
  browser probe before/after is mandatory per TEST.md.
- Not bundled into the `/playouts` slice: that slice deliberately uses the
  correct wrapper→island pattern from the start, so it neither depends on nor is
  blocked by this fix.
