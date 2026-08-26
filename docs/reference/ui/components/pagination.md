# Pagination

**Status:** Candidate
**Adoption:** Fragmented

## Evidence

Two shared implementations and several page-local variants repeat pagination
behavior:

- [`PagedTable`](../../../../crates/ersatzrs-web/src/admin/ui/primitives/table.rs)
  reports page size, visible range, total, and previous/next actions.
- [`MediaCardPager`](../../../../crates/ersatzrs-web/src/admin/ui/primitives/card.rs)
  reports the visible range, exposes previous/next actions, and may include
  selection actions.
- Nine CSS modules contain pager or pagination selector families.

## Candidate contract

A consolidated pagination component needs:

- a localized visible range and total;
- native button semantics for previous and next actions;
- explicit accessible names independent of tooltip support;
- disabled boundary states and a stable loading state;
- a target size suitable for pointer and touch use;
- predictable placement in tables, card grids, and narrow layouts;
- separation between page navigation and optional selection or bulk actions;
- a server/SSR representation that remains understandable before hydration.

## Unresolved variants

- Table pagination reports page size; card pagination currently does not.
- Selection actions are part of the card pager layout but are not pagination
  semantics.
- Page-local variants may use cursor, offset, or zero-based page state and cannot
  be merged solely from their visual similarity.
- The narrow-screen choice between wrapping, a compact range, and labelled icon
  controls still needs browser and localization evidence.

The candidate becomes `Implemented` only when a shared source surface owns the
settled contract. Existing pagers remain evidence, not a conformance claim.
