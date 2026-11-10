# Pagination

**Status:** Implemented
**Adoption:** Broad across `/media/*`; scheduling retains local variants

## Sources

- [`pagination.rs`](../../../../crates/ersatzrs-web/src/admin/ui/primitives/pagination.rs)
- [`pagination.module.css`](../../../../crates/ersatzrs-web/src/admin/ui/primitives/pagination.module.css)

## Interface

`PaginationControls` receives reactive zero-based page, optional total, and
page-size signals plus previous/next and page-size-change callbacks. The
localized `Rows per page` option list is part of the component contract and
cannot be omitted. `PaginationLinks` accepts the same resolved offset state,
previous/next URLs, and an SSR-safe GET form contract for changing page size.
Both variants share range calculation, localized labels, markup, and CSS.

## Current behavior

- The visible range, total, and current page are localized.
- Client-state navigation uses native buttons; URL-state navigation uses links
  that remain useful before hydration and omit their destination when disabled.
- Previous and next controls have explicit localized accessible names, native
  focus behavior, visible focus indicators, and 44 by 44 CSS-pixel targets.
- Loading, empty, first-page, last-page, and `All` page-size states use the same
  stable outer navigation markup.
- Page-size choices are always present and separate from the range/status
  region; the current size is added when it is not in the default choices.
  All is always the last choice, including for route-specific option lists.
  Its value is zero; preserve that value in client state, URLs and saved
  preferences. It selects every matching row, resets the page to the start,
  reports one page and disables both navigation controls. The existing `All`
  translation supplies its label in every supported culture.
- Media-card selection and bulk actions remain outside the pagination landmark.
- The layout wraps by content pressure and left-aligns at narrow widths without
  requiring horizontal page scrolling.

## Adoption and evidence

`tests/e2e/pagination-all.spec.ts` exercises shared tables and collection,
schedule and playout lists through their existing islands and server functions.
Their application queries translate the zero sentinel to the existing complete
repository read convention without changing filters or sorting.
`media-browser-actions.spec.ts` and `remote-streams.spec.ts` cover URL-owned
media pagination, including the native GET-form fallback. `logs-workbench.spec.ts`
covers the saved log-view preference. These boundaries reuse the existing
`PaginationControls`, `PaginationLinks`, `PagedTable` and `MediaCardPager`;
no new island or endpoint is introduced.

The representative rendering paths are `ChannelsListPage` /
`ChannelsListIsland` and `FfmpegProfilesListPage` /
`FfmpegProfilesListIsland` through `PagedTable`; `CollectionsPage` /
`CollectionsIsland`, `SchedulesPage` / `SchedulesIsland` and `PlayoutsPage` /
`PlayoutsIsland` use the controls directly. URL-owned media pages keep their
`MultiSelectBrowserPage` / `MultiSelectIsland` and Remote Stream wrappers,
GET forms and existing media-list server functions. Logs retain `LogsPage` /
`LogsIsland` and `load_recent_log_entries`. Rows, totals, queries and sort state
stay owned by those consumers.

The shared primitive owns pagination for `PagedTable`, `MediaCardPager`, the
URL-driven multi-select and Remote Stream browsers, manual/smart/multi/rerun
collections, filler presets, and both add-on-list paging surfaces. This covers
the paginated `/media/*` route families, including `/media/sources/local`
through `PagedTable`.

Pure Rust tests cover empty, final-partial-page, and `All` range math.
`tests/e2e/media-collection-route-family.spec.ts` checks the shared control's
button/select target geometry on representative table routes;
`tests/e2e/media-browser-actions.spec.ts` changes the built-in option list on a
card browser; `tests/e2e/remote-streams.spec.ts` exercises the same control
through its no-JavaScript form; and
`tests/e2e/addon-metadata-editor.spec.ts` exercises keyboard paging in the
review editor.

Scheduling playout/schedule pagers remain page-local because they were outside
the `/media/*` migration. Cursor-based pagination remains a future semantic
variant and must not be forced through the offset contract without a separate
interface.
