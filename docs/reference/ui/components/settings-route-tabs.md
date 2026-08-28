# Settings route tabs

**Status:** Implemented
**Adoption:** Broad across Settings

## Sources

- [`settings_navigation.rs`](../../../../crates/ersatzrs-web/src/admin/ui/primitives/settings_navigation.rs)
- [`settings_navigation.module.css`](../../../../crates/ersatzrs-web/src/admin/ui/primitives/settings_navigation.module.css)
- [`nav.rs`](../../../../crates/ersatzrs-web/src/admin/layout/nav.rs) contains
  the single global Settings destination.

## Purpose and contract

The global navigation exposes one Settings link. Once an operator enters the
Settings area, route tabs navigate Add-ons, FFmpeg, Logging, HDHomeRun,
Scanner, Playout, UI, and XMLTV. This deliberately adds one navigation step for
some destinations so future settings pages do not expand the primary drawer.

The component renders a native navigation landmark and links. The active route
uses `aria-current="page"` as well as a border and text-color change. Add-on
detail routes keep the Add-ons tab selected. Long localized labels remain on
one line inside a bounded horizontal tab scroller instead of widening the page.

## Implementation boundary

`SettingsPageLayout` server-renders the route tabs around regular settings
forms, the hydrated add-on manager, and add-on details. The tabs require no
client state or hydration island. Settings forms retain their raw POST
fallbacks, while interactive add-on behavior remains inside its existing
islands.

## Evidence and limitations

- Every top-level `/settings/*` route consumes the shared layout.
- Unit coverage verifies the route inventory and add-on-detail active state.
- The settings Playwright regression verifies one global drawer link, eight
  route tabs, selected state, and tab navigation.
- Horizontal scrolling is intentional when future pages or long translations
  exceed the available width. A future overflow menu is not part of the
  current contract.
