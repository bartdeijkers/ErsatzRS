# Channel packages — Phase 11 (first draft)

Status: draft for refinement. Nothing implemented. The enabling work (ADR 0072
source routing, ADR 0073 per-item classification and the seven library
projections) is on `main`; this plan is what those were built toward.

## The idea

An operator uploads one `.zip` under `/channels` and gets a working channel.
The package carries the channel's identity — name, artwork, graphics elements —
and a feed URL. The feed lists the programming: items that are Beeld & Geluid,
YouTube, or any other add-on source. ErsatzRS routes each item to the add-on
that handles its host, imports it into a library, and fills the metadata gaps
the feed left open by asking the add-on.

The point is that nobody fills in a form. Today every add-on list entry is added
by hand through the admin UI; a channel package is the same machinery driven by
a document instead of an operator.

## What already exists

| Capability | Where |
|---|---|
| URL → add-on routing, provider-neutral | `ersatzrs_infra::addon_source_routing::resolve_source_addon` (ADR 0072) |
| Fallback when nothing claims a host | `addons.default_remote_stream_addon` |
| Per-item classification from provider metadata | `AddonMediaListItem.ContentKind` (ADR 0073) |
| Projection into any library kind | seven presentations behind `reconcile_projection` (ADR 0073) |
| Metadata enrichment through an add-on | `remote-stream.item.v2` (ADR 0070) |
| Bounded, safe ZIP installation | `ersatzrs_infra::addons` package install path (ADR 0066) |
| Durable operator metadata | managed NFO sidecars (ADR 0068/0071) |

A channel package should reuse all of it rather than grow a parallel path. The
ZIP handling in particular already solves bounded extraction, path traversal,
and content hashing for add-on packages; a channel package is a different
payload through the same discipline.

## Open questions to settle before implementing

These are the decisions the phase-opening ADR has to make. They are listed as
questions on purpose — this draft does not pre-empt them.

1. **What is the package, exactly?** A manifest plus assets plus a feed URL, or
   may it also carry the feed inline? Inline is reproducible and offline-
   installable; a URL stays current. Probably both, with one winning.
2. **What is the feed format?** The operator asked for "as lean as possible".
   The obvious candidates are a line-per-item NDJSON like the add-on contracts
   already use, or M3U/XMLTV for familiarity. Reusing the NDJSON shape means the
   same validator, the same bounds, and the same error vocabulary.
3. **Who owns the channel?** Is an imported channel editable afterwards like a
   hand-made one, or does it stay managed and get overwritten on refresh? The
   add-on media list answer — provider owns the snapshot, operator edits survive
   in NFO — is the precedent worth following.
4. **What happens when a feed item has no add-on?** The resolver already returns
   `None` and `Ambiguous` distinctly. A package import cannot stop and ask, so
   it needs a policy: skip and report, fail the whole import, or import as a
   direct URL.
5. **Where do the graphics elements go?** ErsatzRS has watermarks and `drawvg`
   graphics elements; a package that ships its own must land in those tables
   without colliding with operator-made ones.
6. **Refresh semantics.** Does the feed re-import on a schedule like an add-on
   list with `auto_refresh`, and what happens to items that disappear from it?
   `AddonMediaListItem` already has tombstone behavior worth mirroring.
7. **Trust.** Add-on packages have signed repositories and a tiered trust model
   (ADR 0066). A channel package executes nothing, but it does fetch a feed and
   write into libraries. Decide whether it needs signing or whether the
   unknown-sources gate covers it.

## Rough shape of the work

Sequenced so each step is reviewable and the risky decisions come first.

1. **P11A — product and format ADR.** Blocking, like `p9a-01` and `p10a-01`.
   Answers the questions above and freezes the feed contract with a version
   number from day one.
2. **P11B — package contract and validation.** A `channel-package` manifest
   type in `ersatzrs-addon-contract` (or a sibling crate if it does not belong
   with add-ons), bounded like the add-on manifest, plus the standalone
   validator so a publisher can check a package before shipping it.
3. **P11C — import.** Upload, safe extraction, asset placement, channel
   creation. No feed yet: prove the identity half end to end.
4. **P11D — feed ingestion.** Fetch, validate, route each item through
   `resolve_source_addon`, import, project, and enrich. This is where the
   existing add-on-list machinery is reused rather than reimplemented.
5. **P11E — refresh and lifecycle.** Scheduled refresh, disappearing items,
   removal, and what happens to a channel whose package is deleted.
6. **P11F — administration.** The `/channels` surface: upload, state, per-item
   failures, and the same three-way availability grouping the add-on list editor
   already uses.

Health checks belong with each step, not at the end: a channel whose feed cannot
be fetched, whose add-on is missing, or whose assets went away is exactly what
the health engine exists to report.

## Parity note

ErsatzTV has nothing like this — it has no add-on platform and no packaged
channels. This is an ErsatzRS extension in the same category as the add-on
platform itself, so it needs the same treatment: an ADR that states the
deviation, and parity entries that record it as an extension rather than as
ErsatzTV coverage.

## Tracking

The motivating debt is `channel-package-feed-import` in
`docs/progress/parity/addon-platform.toml`. When this phase opens, it becomes
the phase entry and the debt should point at the milestones instead.
