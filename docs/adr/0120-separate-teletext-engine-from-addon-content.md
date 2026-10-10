# ADR 0120: Separate the teletext engine from add-on content

## Status

Accepted (2026-10-10). The maintainer selected engine-first delivery followed
by one new tvguide teletext add-on in ErsatzRS-Addons. The maintainer clarified
that only TV-guide-related NOS Teletekst pages in the 200–299 range serve as
the example for page presentation and navigation.

## Implementation

Not implemented. Phase 9 milestones and the `teletext-support` parity record
own the remaining standards decision, engine, add-on and integrated acceptance.

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

## Consequences

- Generic engine acceptance is separately observable before content work starts.
- TV-guide layouts and content can evolve in one add-on without host changes.
- Engine-only proof does not complete Phase 9; exact compatible host and add-on
  versions need integrated page, refresh, failure and client-visible evidence.
- Missing, disabled, incompatible or failing add-ons need a visible bounded
  outcome without breaking unrelated playback. P9A-01 decides stale-page and
  last-good publication policy before implementation.
- No runtime architecture or user capability is claimed by this planning change.

## Alternatives Considered

- Hard-code a NOS-inspired TV guide in ErsatzRS: couples content to the engine.
- Build the engine and production content together: prevents independent engine
  acceptance before committing to the content package.
- Deliver only source subtitle decoding: does not meet the authored-page goal.
- Wait for automatic official repository distribution: creates a dependency
  cycle because that work is already gated behind Phase 9 and Phase 10.

## References

- [Phase 9 implementation plan](../../plans/teletext-support.md)
- [Milestone acceptance](../progress/milestones.toml)
- [Teletext debt](../progress/parity/teletext-support.toml)
- [ADR 0112: Programme-aware graphics context](0112-programme-aware-graphics-context.md)
- [ADR 0118: Official repository distribution](0118-bundle-and-refresh-official-addon-repository.md)
- [Current add-on contract](../reference/addons.md)
