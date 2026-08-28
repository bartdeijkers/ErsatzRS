# Admin UI component evidence inventory

This inventory is a static source baseline captured on 2026-08-26. Counts
include primitive definitions and are evidence of repetition, not proof that
every match has the same purpose or behavior.

| Cluster | Static evidence | Shared surface | Classification | Adoption |
|---|---|---|---|---|
| Action controls | 54 Rust files contain raw `<button>` markup; shared `Button`, `LinkButton`, or `IconButton` markup appears only in the developer showcase and shared dialog | `button.rs` | Implemented | Limited |
| Data tables | 26 Rust files contain raw `<table>` markup; `<PagedTable>` appears in four production pages and the developer showcase | `table.rs` | Implemented | Limited |
| Pagination | One shared Rust/CSS primitive owns offset metrics plus button/link navigation; media consumers use it directly or through `PagedTable` and `MediaCardPager` | `pagination.rs` | Implemented | Broad across `/media/*`; scheduling remains local |
| Dialogs | Nine page CSS modules contain modal or dialog selectors; shared `Modal` appears across 16 consumer files and `DeleteDialog` across 21 | `dialog.rs` | Implemented | Broad, with local variants |
| Resource states | Resource-state symbols appear in 36 Rust files including the primitive definition | `resource_state.rs` | Implemented | Broad |
| Navigation drawer | One global `AppShell` renders the primary admin drawer and one native button disclosure; no page-local drawer variants were found | `layout/mod.rs`, `layout/layout.module.css` | Implemented | Global shell |

The source paths in the table are under
[`crates/ersatzrs-web/src/admin/ui/primitives/`](../../../crates/ersatzrs-web/src/admin/ui/primitives/).

## Interpretation

- A raw-element count is a discovery queue. It does not require every match to
  migrate to one abstraction.
- `Implemented` means a shared Rust/CSS surface exists. Adoption and quality
  evidence are separate fields.
- Pagination separates button and URL-link navigation over one shared offset
  contract; selection and bulk actions remain outside the pagination landmark.
- Dialog reuse is already broad, but page-local modal CSS indicates remaining
  variants that need semantic comparison before consolidation.
- The inventory does not claim runtime, responsive, browser, or WCAG proof.
  Each component entry lists known limitations.

Future audits update this baseline when source evidence materially changes and
add a component entry only after purpose, semantics, state, keyboard behavior,
responsive behavior, localization, and hydration boundaries have been compared.
