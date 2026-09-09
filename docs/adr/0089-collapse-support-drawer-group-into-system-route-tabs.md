# ADR 0089: Collapse the Support Drawer Group Into System Route Tabs

## Status

Accepted

## Implementation

Implemented (2026-09-09) — `crates/ersatzrs-web/src/admin/layout/nav.rs`,
`crates/ersatzrs-web/src/admin/ui/primitives/system_navigation.rs`, and
`SystemPageLayout` on all eight `/system/*` pages.

## Context

ErsatzTV renders its support destinations as an always-expanded `MudNavGroup`
in the navigation drawer (`ErsatzTV/Shared/MainLayout.razor:195-244`): a
"Support" title carrying an error/warning icon, with `system/health`,
`system/logs`, and `system/troubleshooting` as child links. The health-check
counts appear twice — once as an icon on the group title, once as a `MudBadge`
on the health-checks link.

ErsatzRS mirrored that structure. Two problems followed from it in the ErsatzRS
drawer, which is narrower and denser than ErsatzTV's:

1. The group occupies four permanent rows at the bottom of the drawer. With the
   Media and Lists groups expanded, the support rows — and with them the health
   badge, the drawer's only always-on parity/runtime signal — scroll out of
   view.
2. ErsatzRS already resolved the same problem for Settings. ErsatzTV lists seven
   settings pages as drawer children; ADR-era ErsatzRS instead exposes one
   Settings drawer destination and renders the seven routes as in-page tabs via
   `SettingsRouteTabs`. Support was the last multi-page section still expanded
   into the drawer, so the drawer applied two different patterns to the same
   problem.

## Decision

Collapse the Support drawer group into a single badged drawer link, and render
the three support routes as in-page tabs, reusing the established Settings
pattern rather than inventing a second one.

- The drawer shows one `Support` link targeting `/system/health`, with
  `match_prefix="/system/"` so it stays active across every support page. It
  carries the existing `HealthBadge` through a new optional trailing slot on
  `NavLink`. The badge markup, its `data-health-*` hooks, and its counts are
  unchanged.
- A new `SystemRouteTabs` / `SystemPageLayout` primitive renders
  `/system/health`, `/system/logs`, and `/system/troubleshooting` as a route tab
  strip, directly mirroring `SettingsRouteTabs` / `SettingsPageLayout`. Tabs sit
  outside `Suspense` in the page wrapper, per the hydration rules in TEST.md.
- `/system/troubleshooting/*` detail routes keep the Troubleshooting tab
  selected, matching how `/settings/addons/<id>` keeps the Addons tab selected.

The drawer link order at the bottom is Support, then Settings. This preserves
ErsatzTV's relative ordering of the two sections.

## Consequences

The health badge is now permanently visible in the drawer regardless of which
groups are expanded, and the drawer's bottom block is two rows instead of five.
The drawer applies one consistent rule: a multi-page section gets one
destination plus in-page route tabs.

This is a deliberate divergence from ErsatzTV. Logs and Troubleshooting are no
longer reachable in one click from the drawer; they cost one extra click via the
Support link's tab strip. Deep-linked URLs, route definitions, and every
existing `/system/*` link elsewhere in the admin UI are unaffected.

The `/system/troubleshooting/*` detail pages (MPEG, sequential schedule, block
playout, block playout history, playback) render the same tab strip, so a user
who deep-links into one — for example from the media browser's
playback-troubleshooting links — reaches Logs without going back through the
drawer. This matches how `/settings/addons/<id>` keeps its Settings tabs.

The drawer no longer renders the error/warning icon that ErsatzTV puts on the
Support group title; the badge on the Support link carries the same counts and
is the single signal, removing ErsatzTV's duplicate display.

## Alternatives Considered

- Keep the Support group and pin it to the drawer bottom alongside Settings.
  Fixes badge visibility without diverging from ErsatzTV, but keeps two
  competing drawer patterns and still spends four permanent rows.
- Move the health signal to a topbar status pill and flatten the three links.
  Solves the mobile blind spot too — the drawer is closed by default under
  767px, so the health signal is invisible there — but needs a new topbar island
  and is the furthest from ErsatzTV. Worth revisiting on its own merits; it is
  independent of this decision.

## References

- `$ERSATZTV_SRC/ErsatzTV/Shared/MainLayout.razor:186-245`
- `crates/ersatzrs-web/src/admin/ui/primitives/settings_navigation.rs`
- [TEST.md](../../TEST.md) — page wrapper vs `#[island]` hydration rules
- [ADR 0012](0012-admin-ui-nl-localization.md) — admin UI translation boundary
