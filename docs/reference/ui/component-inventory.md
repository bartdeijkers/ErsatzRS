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
| Settings route tabs | One shared route inventory renders eight localized links across every top-level Settings route while the drawer exposes one Settings destination | `settings_navigation.rs` | Implemented | Broad across `/settings/*` |
| System route tabs | One shared route inventory renders three localized links across the top-level Support routes while the drawer exposes one badged Support destination | `system_navigation.rs` | Implemented | Every `/system/*` page |

The source paths in the table are under
[`crates/ersatzrs-web/src/admin/ui/primitives/`](../../../crates/ersatzrs-web/src/admin/ui/primitives/).

## Interpretation

The [section disclosure](components/disclosure.md) remains a shared-component candidate
implemented locally in the add-on list task pages from the locked
`docs/reference/ui/design-artifacts/admin-ux-organize/` reference: bulk actions, publication details,
and optional metadata use native details/summary semantics. Routine settings,
storage thresholds and managed locations stay visible. Compact Help and
row-command popovers remain distinct patterns. This
does not change the production adoption counts above.

The locked [official Sources v0.1](design-artifacts/addon-sources/LOCK.md)
reuses action, resource-state, confirmation and native disclosure semantics.
Its repository/policy panel is page-local composition, not an implemented new
generic component. Proposed contrast tokens and switch styling remain scoped
to the artifact; production adoption counts are unchanged.

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
