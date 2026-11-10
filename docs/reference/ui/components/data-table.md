# Data table

**Status:** Implemented
**Adoption:** Limited

## Sources

- [`table.rs`](../../../../crates/ersatzrs-web/src/admin/ui/primitives/table.rs)
- [`table.module.css`](../../../../crates/ersatzrs-web/src/admin/ui/primitives/table.module.css)

## Interface

`PagedTable<T>` receives the current optional row set, optional total count,
reactive `PageParams`, column definitions, and optional row actions.
`PageParams` contains a zero-based page, page size, and `SortState`.

## Current behavior

- The component renders native table, heading, row, and cell elements.
- Clicking a column heading selects that column or reverses its sort direction.
- Loading and empty results occupy a full-width table row.
- A trailing action column is present when a row-action renderer is supplied.
- The table has a 78rem minimum width inside a bounded horizontal scroll
  container. The page itself need not scroll horizontally.
- The footer reports rows per page, the visible range, total items, and
  previous/next controls.

## Current evidence and limitations

- The shared table is used by four production list pages and the developer
  showcase, while raw table markup exists in 26 Rust files.
- Sortable headings attach click behavior directly to `<th>`. They are not
  keyboard-operable and do not expose `aria-sort`.
- Previous and next controls have localized `title` text but no explicit visible
  labels or `aria-label`.
- The shared page-size selector offers numeric sizes and All; choosing a size
  resets the page. All is represented by zero and consumers fetch every row.
- Loading and empty states are covered locally; an error state belongs to the
  surrounding resource boundary.
- The horizontal-scroll model preserves columns but has not been established as
  the right narrow-screen interaction for every table.
