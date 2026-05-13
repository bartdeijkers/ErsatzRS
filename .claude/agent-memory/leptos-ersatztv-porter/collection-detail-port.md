---
name: collection-detail-port
description: Port of /media/collections/:id (CollectionItems.razor) — multi-kind poster grid, inline rename, multi-select trashcan, custom-order toggle.
metadata:
  type: project
---

The collection-detail port (May 2026) replaced the rename-only modal on
`/media/collections/:id` with a faithful port of
`ErsatzTV/Pages/CollectionItems.razor`.

**Why:** the previous wiring forced users into a rename-only modal and gave
no way to view a collection's contents, remove items, or set custom
playback order — the original "edit collection" surface in ErsatzTV is the
full detail page, not the rename form. The rename form lives at
`/media/collections/:id/edit` only.

**How to apply (next time you port a "card grid + multi-select + inline
state" admin page):**

1. **Hydration shape — copy the movies.rs pattern.** Page wrapper is a
   plain `#[component]` that reads `:id` from `use_params_map()` and renders
   a single island carrying only a scalar `i64` (or other plain) prop.
   The island owns `Resource::new_blocking` keyed on `(id, refresh)`,
   renders `<Suspense>` internally, and creates all `RwSignal`s after the
   resource resolves. Sub-views are plain `#[component]`s reading those
   signals. This is the same shape used by `MovieMultiSelectIsland` and
   passes `hydration-lint` with no findings.

2. **One-shot multi-kind SQL.** ErsatzTV's `GetCollectionCardsHandler` does
   a giant EF `Include` graph across every kind (movies, shows, seasons,
   episodes, artists, music videos, other videos, songs, images, remote
   streams). Port as a single `UNION ALL` over each kind joined to its
   metadata + first artwork row. Carry a `Kind` discriminant column and
   resolve the href client-side. Two round-trips total: collection header
   row + the UNION query. See `get_collection_detail` in
   `crates/ersatzrs-web/src/admin/server_fns/media_browser.rs`.

3. **Drag-and-drop is the deferral lever.** ErsatzTV uses a JS interop
   (`sortableCollection` / `enableSorting`) backed by SortableJS for the
   custom-order DnD. Replacing it with Move Up / Move Down buttons that hit
   the same `update_collection_custom_order` server fn preserves the
   toggle, the persistence, and the rendered order — only the input device
   differs. Note this as a "Deferred" item with a TODO; do not silently
   drop the toggle itself.

4. **Selection / trashcan / hover-check CSS.** The `.card-wrap` +
   `.card-checkbox` hover-reveal pattern from
   `crates/ersatzrs-web/src/admin/pages/media/browser.module.css` works
   for both top-left selection checkboxes and bottom-corner action
   buttons. Stylance scopes class names per-file, so duplicate the
   `.card-wrap:hover .card-actions` selector locally rather than trying
   to `:global()` across modules.

5. **`view!` macro pitfalls hit during this port:**
   - `<Foo show_move=a && b && *kind == "movie" />` fails to parse — lift
     complex boolean expressions to a `let` binding before the `view!`.
   - `<Foo can_move_down=idx + 1 < len />` also fails because `< len` is
     parsed as start of a tag. Same fix: bind to a local first.

6. **Playout refresh side-effect.** ErsatzTV's
   `RemoveItemsFromCollectionHandler` and `UpdateCollectionHandler` enqueue
   `BuildPlayout(Refresh)` for every playout that uses the collection.
   ErsatzRS does not yet have an equivalent background channel — the
   persistence is identical, only the side-effect is missing. Same gap
   exists for `set_collection_use_custom_order`. Add as TODO until the
   scheduling channel exists.

Files:
- `crates/ersatzrs-web/src/admin/pages/media/collection_detail.rs`
- `crates/ersatzrs-web/src/admin/pages/media/collection_detail.module.css`
- `crates/ersatzrs-web/src/admin/server_fns/media_browser.rs`
  (`get_collection_detail`, `set_collection_use_custom_order`,
  `remove_items_from_collection`, `update_collection_custom_order`,
  `CollectionDetail`, `CollectionItemCard`).
- Route in `crates/ersatzrs-web/src/admin/routes.rs`:
  `/media/collections/:id` now points to `CollectionDetailPage`;
  `/media/collections/:id/edit` still points to `CollectionEditorPage`
  (rename only).
