# ADR 0123: Support Level 2.5 teletext and external-editor workflows

## Status

Accepted (2026-10-11): the maintainer explicitly extended Phase 9 to authored
EN 300 706 Level 2.5 and QTeletextMaker interchange. The subsequent amendment
requires a demonstrably suitable client; Kodi is a candidate, not a dependency.
This supersedes ADR 0121's Level 1/1.5-only authored target. All other source,
playback, output, profile/Lua and engine-first obligations remain in force.

## Implementation

Not implemented. Existing Western Latin Level 1/1.5 encoding and unpublished
channel composition are reusable baseline evidence, not Level 2.5 acceptance.
The standards/editor source audits do not establish real editor or client proof.

## Context

Decoder capability cannot establish authored-page design, validation, encoding,
publication or display. QTeletextMaker's actual TTI loader/writer also normalizes
identity and discards metadata, so arbitrary raw TTI roundtripping is insufficient.
The pinned source audit and standards matrix live in the
[interchange reference](../reference/teletext-design-interchange.md).

## Decision

### One model through Level 2.5

The generic engine must support designing, validating, encoding, publishing and
displaying authored pages through Level 2.5. Include palettes/remapping, extended
character and mosaic repertoires, non-spacing attributes, DRCS, local/public/global
objects and side panels. Optional use by a page or receiver does not remove these
engine obligations. Preserve a readable, correctly designated Level 1/1.5 base;
enhancements cannot substitute for its fallback content.

Extend the typed model with checked enhancement/resource identities, dependencies
and bounds, including non-display resources distinct from display pages. Resolve
objects/DRCS and reject broken references, invalid codes and unbounded expansion
before publication. The addon contract must be explicitly versioned for enhanced
pages; the existing closed v1 contract remains base-only compatibility, never a
silent lossy downgrade. Retain supported structure, attributes and navigation.
Reject Level 3.5-only features with a targeted explanation and location.

The native encoder and browser preview consume the same immutable validated model
and generation. Preview offers enhanced and base views. Diagnostics identify page,
raw subcode and position, adding resource/source-file locations where applicable.
A preview never replaces native encoding, transport or actual client evidence.

### External editing, drafts and publication

Support selection/export of a page, its subpages or a supported page set as TTI
plus `ersatzrs-teletext-project.json`, an explicitly versioned external sidecar.
Keep the sidecar outside TTI because QTeletextMaker does not preserve custom
records. It records identity mapping, exact navigation, source/template/package
and profile revisions, themes/overrides, zones, dynamic fields and dependencies.
Compare it with authoritative host export/configuration state; supplied metadata
does not grant package, profile or publication authority.

For pinned QTeletextMaker 1.0, one editable TTI document per stable page/subpage
avoids ordinal renumbering. Restore identities through explicit mapping, not
guessed content matching. Ambiguous additions/reordering, missing dependencies or
navigation conflicts require explicit resolution before accepting a draft.
Report semantic normalization; do not silently lose supported information.
Editor limitations do not narrow the engine's Level 2.5 target.

Import creates a separate concept with validation and preview. Design edits,
durable draft storage and explicit publication are distinct application actions.
Import never changes the broadcast or installed package. A stale export/profile
cannot overwrite a newer draft or publish without current revision checks.

Mark free authored content, declared fillable zones and generated guide content
separately. Validate edits against addon-owned theme/zone/template declarations;
generated content remains generated. Template changes use explicit addon versions
and installation workflow, never package-file mutation or script autoactivation.
Reuse existing application commands, configuration, addon trust and Lua controls.

### Evidence and delivery order

P9A-01 is reopened for the expanded standards/interchange contract. P9A-04 accepts
the complete independent engine with synthetic pages before P9B-01 starts the
sole production TV-guide addon in ErsatzRS-Addons, inspired only by NOS guide
pages 200–299. Existing platform, subtitle, audio, lifecycle and output gates stay.

Require TSDuck transport checks, independent decoding of representative enhanced
functions and actual display/page/subpage navigation in a demonstrably suitable
client. Record exact client/decoder versions, settings, functions and limitations.
Kodi or an equivalent suitable client is allowed. TSDuck alone does not establish
Level 2.5 rendering; an editor preview is not a broadcast-client substitute.

Pin and verify the actual QTeletextMaker artifact for export → visibly open/edit/
save → import → preview → explicit publication evidence. Localize all changed
admin in every supported culture and retain headed browser, both-theme and
hydrated/no-JS proof. Provide a structured file/validation/preview workflow for
AI agents using the same application validation and publication boundary.

## Consequences

Existing transport and channel-isolation evidence remains valid for unchanged
behavior. Enhanced resource carriage, base fallback, draft persistence, editor
interchange and client display need additional implementation and acceptance.
Full Level 2.5 cannot be claimed with missing required parts. Any necessary engine
restriction must be explicitly proposed to the maintainer, not hidden in a
support label or accepted by substituting decoder availability.

## Alternatives Considered

- Retain a Level 1/1.5 authored target: rejected by the authorized scope extension.
- Treat QTeletextMaker as a lossless TTI validator: contradicted by its source.
- Store host metadata as custom TTI records: discarded by the editor's writer.
- Require Kodi at runtime or for acceptance: replaced by the maintainer's suitable
  client amendment.

## References

- [ADR 0121](0121-teletext-pages-transport-and-addon-contract.md)
- [ADR 0122](0122-channel-teletext-profiles-themes-and-lua-control.md)
- [Interchange and audit reference](../reference/teletext-design-interchange.md)
- [Phase 9 plan](../../plans/teletext-support.md)
- [Authoritative milestone acceptance](../progress/milestones.toml)
