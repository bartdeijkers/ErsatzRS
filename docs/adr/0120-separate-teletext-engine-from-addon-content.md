# ADR 0120: Separate the teletext engine from add-on content

## Status

Accepted (2026-10-10). The maintainer selected engine-first delivery followed
by one new tvguide teletext add-on in ErsatzRS-Addons. The maintainer clarified
that only TV-guide-related NOS Teletekst pages in the 200–299 range serve as
the example for page presentation and navigation.

Maintainer amendment (2026-10-28), ADR0123: engine-first9A acceptance now requires
the fully verified Level1/1.5 engine and every applicable shared gate. Existing
Level2.5 remains available as experimental; its remaining acceptance/client work
is Deferred until a new maintainer task and does not block9A or subsequent9B.
This preserves the engine/content split; no experimental2.5 acceptance is claimed.

Maintainer amendment (2026-11-07): besides add-ons, ErsatzRS itself delivers
operator-made custom pages from the teletext element library as a host-owned
page source. See "Host-delivered custom pages" below. This adds a source; it
does not move TV-guide content into the host or change the two-stage order.

## Implementation

Partial. The revised generic9A boundary is independently accepted at checkpoint
802c8a10 (2026-10-31), with full required general/Level1.5 CI on exact218d4500
and the canonical source/runtime/client proof join in P9A-04. Implemented2.5
remains experimental under ADR0123. P9B-01 now owns the sole production TV-guide
addon candidate; P9B-02 owns its actual guide/publication/stream/client integration.
Neither the candidate nor generic fixture evidence accepts production9B.
See the current milestones and `teletext-support` parity record.

## Context

The previous Phase 9 plan left authored pages optional and primarily described
source teletext subtitle ingest and playback. The requested product needs a
reusable teletext engine followed by independently maintained page content.
The existing add-on host has no teletext capability yet.

## Decision

Deliver Phase 9 in two ordered stages:

1. ErsatzRS implements and verifies the generic teletext engine: typed pages
   and services, character and layout semantics, navigation, validation,
   publication lifecycle, supported rendering/delivery, management and health.
   Prove it with synthetic redistributable pages before implementing the addon.
2. One new tvguide teletext add-on in the separate ErsatzRS-Addons repository
   supplies the actual page structure, navigation links, presentation choices
   within the engine contract, and TV-guide content. It consumes read-only
   programme context supplied by ErsatzRS. Product page numbers, editorial
   layout and guide composition belong to the add-on, not hard-coded host pages.

The design reference is limited to TV-guide-related pages within
[NOS Teletekst 200–299, starting at the TV-guide index](https://nos.nl/teletekst/200):
numbered guide pages, index/subpage navigation, compact programme listings and
colour hierarchy. Other NOS sections and unrelated information pages within
that range are outside the reference scope. The range identifies the reference
material; the add-on's exact page map remains a later design deliverable.
This decision does not select
NOS as a live data source or authorize copying its content or branding. The
initial content is the operator's ErsatzRS TV guide; the detailed page map is
an add-on design deliverable after engine acceptance.

Extend the existing versioned add-on contract and bounded child-process host
only as needed. ErsatzRS validates add-on page output and owns its publication,
rendering and transport. The add-on must not become an external graphics
renderer or gain direct database access. Reuse the read-only programme
occurrence projection from ADR 0112 for current/next/continuing and Later Today,
including filler and timezone/DST behavior.

P9A-01 still blocks implementation until an accepted technical ADR defines the
page model, add-on input/output and compatibility contract, lifecycle limits,
and native/Next source/output matrix. This decision settles ownership and
sequence, not an encoder, transport, API schema or capability version. Source
DVB teletext subtitles retain their existing scan, selection, accessibility and
runtime acceptance requirements. Generated full-page support is now required;
decoder availability or a page preview alone cannot prove service carriage.

Use current explicit checkout/ZIP acquisition and install/enable boundaries
for the add-on. Phase 9 must not depend on the later automatic official
repository distribution work in ADR 0118. Phase 10 and OAD retain their
`p9b-02` dependency, which now includes the working tvguide add-on.

The accepted extension in [ADR 0122](0122-channel-teletext-profiles-themes-and-lua-control.md)
adds channel/add-on bindings and profiles, add-on-owned themes/custom zones and
shared admin/Lua controls with explicit timing and restart/error rules. Generic
synthetic acceptance includes these controls before the same single production
TV-guide add-on supplies actual themes and content. This supplements ownership
and acceptance without changing the two-stage order or NOS reference scope.

### Host-delivered custom pages (amendment 2026-11-07)

Operators upload page designs (TTI, T42, EP1, HTT, for example from
QTeletextMaker) to the teletext element library and link them to a channel
with a page number. These pages are the operator's own content, so no add-on is
needed to deliver them. ErsatzRS acts as their producer under these rules:

- **Source.** One host-owned source per channel with the reserved producer id
  `ersatzrs.host.custom-pages`. It is not an installable add-on, runs no child
  process and has no manifest or settings. Each enabled channel link
  contributes one page: the link's page number, or the element's own number
  when the link has none. Rows 1–24 and all subpages come from the element;
  row 0 (the header) stays the channel's. The subpage cycle time is the
  element's. Only Level 1/1.5 content is broadcast; imported enhancement
  packets are not, until enhanced import is accepted under ADR 0123.
- **Same lifecycle as add-on pages.** Custom pages pass the same validation,
  Check (draft preparation), preview, explicit publication, capacity policy and
  transport as add-on output. Saving a link or an element never publishes. A
  page set that exceeds the capacity policy is refused, never truncated.
- **Fields.** Field markers (`%NAME%%%`; braces are national characters) are
  filled by the host from fixed text, channel name/number, date, time and the
  read-only programme projection of ADR 0112 (now/next title and start). Values
  pass the Level 1 safe-text mapping and are always clipped or padded to the
  field width, so the layout never shifts. Missing guide data renders as spaces,
  never as an older title. A field without a binding blocks Check, so marker
  text never reaches a receiver.
- **Refresh.** Now/next fields refresh at each programme transition, date
  fields at local midnight and time fields at most once per minute, and only
  when such a field is bound. Fixed text and channel fields change only when
  the element, link or channel changes. Refreshes should replace only the
  affected page rather than the whole page set.
- **Conflicts.** Page numbers are unique per channel among custom pages. A custom
  page inside the declared page range of an enabled add-on on the same channel,
  or on a page the add-on actually produced, blocks Check and publication with
  a per-page reason. Neither side silently takes precedence.
- **Failures.** At Check, an unreadable element or a field that cannot be filled
  blocks Check with the page named; the operator can switch that link off. After
  publication, a failed refresh blanks the affected dynamic fields of that page
  only and reports it per page; add-on pages, other custom pages and audio/video
  continue. The last published generation stays live until a new one is
  explicitly published.
- **Revisions.** Publication records each element's revision. Editing or
  replacing an element takes effect at the next publication. Removing a link or
  deleting an element removes the page at the next publication; deleting an
  element in use requires confirmation in the admin.

The add-on contract, add-on ranges and the TV-guide add-on are unchanged.
Lua and channel profiles do not control custom pages in this amendment.

Implementation (partial, 2026-11-07): the host producer takes part in Check,
stored revisions, preview, publication and encoding. While at least one link
is on, the channel selection carries one constant-identity member for
`ersatzrs.host.custom-pages` (installable manifests cannot claim the id), so element
and link edits do not disturb live output; switching the first page on or the
last page off changes the selection like an add-on binding does. Its response
carries exact seven-bit rows that the engine applies as physical base rows on
empty Base pages. The engine's no-hidden-row rule applies unchanged: a page
with text under a double-height line, or double height on row 23 or 24, is a
per-page Check issue rather than having those bytes dropped. Refresh after publication, custom pages during published
Scripted intervals, the element's subpage cycle time and the acceptance
evidence remain open in the `teletext-support` parity record.
Acceptance needs unit and native publication evidence, one independently decoded
stream that carries a custom page with refreshed fields next to the TV-guide
pages, and the Playwright flows for upload, field binding, channel linking and
the conflict refusal.

## Consequences

- Generic engine acceptance is separately observable before content work starts.
- TV-guide layouts and content can evolve in one add-on without host changes.
- Engine-only proof does not complete Phase 9; exact compatible host and add-on
  versions need integrated page, refresh, failure and client-visible evidence.
- Missing, disabled, incompatible or failing add-ons need a visible bounded
  outcome without breaking unrelated playback. P9A-01 decides stale-page and
  last-good publication policy before implementation.
- No runtime architecture or user capability is claimed by this planning change.
- Host-delivered custom pages add a second source next to add-ons. Page-set
  views must show each page's source, and Check must explain conflicts between
  sources instead of resolving them.

## Alternatives Considered

- Hard-code a NOS-inspired TV guide in ErsatzRS: couples content to the engine.
- Build the engine and production content together: prevents independent engine
  acceptance before committing to the content package.
- Deliver only source subtitle decoding: does not meet the authored-page goal.
- Wait for automatic official repository distribution: creates a dependency
  cycle because that work is already gated behind Phase 9 and Phase 10.
- Deliver custom pages through a separate "static pages" add-on: adds a package,
  install step and child process for content the operator already uploaded to
  ErsatzRS, and would need host data access the add-on contract does not grant.

## References

- [Phase 9 implementation plan](../../plans/teletext-support.md)
- [Milestone acceptance](../progress/milestones.toml)
- [Teletext debt](../progress/parity/teletext-support.toml)
- [ADR 0112: Programme-aware graphics context](0112-programme-aware-graphics-context.md)
- [ADR 0118: Official repository distribution](0118-bundle-and-refresh-official-addon-repository.md)
- [Current add-on contract](../reference/addons.md)
- [Teletext admin UX conventions](../reference/teletext-admin-ux.md)
