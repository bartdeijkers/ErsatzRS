# System route tabs

**Status:** Implemented
**Adoption:** Every `/system/*` page

## Sources

- [`system_navigation.rs`](../../../../crates/ersatzrs-web/src/admin/ui/primitives/system_navigation.rs)
- [`system_navigation.module.css`](../../../../crates/ersatzrs-web/src/admin/ui/primitives/system_navigation.module.css)
- [`nav.rs`](../../../../crates/ersatzrs-web/src/admin/layout/nav.rs) contains
  the single global Support destination.
- [ADR 0089](../../../adr/0089-collapse-support-drawer-group-into-system-route-tabs.md)

## Purpose and contract

The global navigation exposes one badged Support link targeting
`/system/health` and active across `/system/*`. Once an operator enters the
Support area, route tabs navigate Health checks, Logs, and Troubleshooting.
This mirrors the Settings decision: a multi-page section costs one drawer row,
not one row per page, so the drawer's bottom block stays two rows and the health
warning/error badge cannot scroll out of view.

The component renders a native navigation landmark and links. The active route
uses `aria-current="page"` as well as a border and text-color change.
`/system/troubleshooting/*` detail routes keep the Troubleshooting tab selected,
the same rule add-on detail routes use for the Add-ons tab.

## Implementation boundary

`SystemPageLayout` server-renders the route tabs around the page body, outside
`Suspense`, so the tab strip is present before the page's resource resolves and
its DOM shape does not change on hydration. The tabs require no client state or
hydration island; `HealthIsland` and `LogsIsland` continue to own their own
interactive behavior inside the layout.

The health badge rides on the drawer link through `NavLink`'s optional trailing
slot. Its markup, `data-health-*` hooks, and counts are unchanged from the
previous group-header placement.

## Evidence and limitations

- All eight `/system/*` routes — the three top-level pages plus the five
  troubleshooting detail routes — serve `data-system-route-tabs` with exactly
  one `aria-current="page"`.
- Unit coverage verifies the route inventory, the troubleshooting-detail active
  state, and that sibling system routes do not match each other.
- The Playwright case `one drawer Support link drives the system route tabs` in
  `tests/e2e/support-pages.spec.ts` verifies the single badged drawer link, the
  three tabs, selected state, tab navigation, and the detail-route active state.
- Horizontal scrolling is intentional when long translations exceed the
  available width, matching the Settings tabs. An overflow menu is not part of
  the current contract.
