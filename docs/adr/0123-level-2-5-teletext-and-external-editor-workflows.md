# ADR 0123: Support Level 2.5 teletext and external-editor workflows

## Status

Accepted (2026-10-11): the maintainer explicitly extended Phase 9 to authored
EN 300 706 Level 2.5 and QTeletextMaker interchange. The subsequent amendment
requires a demonstrably suitable client; Kodi is a candidate, not a dependency.
This supersedes ADR 0121's Level 1/1.5-only authored target. All other source,
playback, output, profile/Lua and engine-first obligations remain in force.

## Implementation

Partial — checked colour references, four-bit RGB, editable CLUT2/3 declarations
and monochrome DRCS bitmap/slot/subtable values exist. Checked raw resource
identities and a DRCS catalog resolve supplied display references with normative
subtable/glyph limits. Ordered enhancement programs, checked charset designations
and local/public/global object identities now support checked definition closure
and bounded transitive reference collection. Per-display primary/X26 pair and
per-type invocation limits are enforced; collected DRCS failures retain their
source command. Checked presentation declarations retain page/magazine presence,
colour remapping and raw panel metadata. A semantic service composes the unchanged
base set with explicitly supplied resolved resource selections and validates their
aggregate ownership, budgets and effective dependencies. The native presentation
body encoder now carries complete checked X28/0 or M29/0 fields; the existing v1
encoder uses it with byte-identical declarations. A bounded, consuming v1 lift
retains explicit page presentation and empty local-X26 presence before composition.
Checked X27/4 declarations retain fixed roles, exact targets, validity and masks;
the semantic service checks their required Level 2.5 resource inventory. A native
body encoder preserves those fields with independently checked packet coding.
Same-target effective explicit calls and glyph references now require announced
S1 values, with source-located failures. This does not derive MOT/X27 selection.
Default roots retain their original MOT resource independently of explicit links.
Checked Level 2.5 MOT declarations now retain explicit ownership, associations,
full link banks, original defaults and ignored raw fields. Native MOT body coding
checks supplied default allocations before emitting any of its seventeen rows.
MOT resolution, resolved display semantics, physical allocation, complete
enhanced-service compilation and editor/publication workflows remain unimplemented. Existing
Western Latin Level 1/1.5 encoding and unpublished channel composition are
reusable baseline evidence, not Level 2.5 acceptance.
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

Lift v1 contributions with the original encoder's explicit page presentation and
present local-X26 semantics, including blank pages. A present empty authored
program suppresses defaults but must not suppress character-enhancement triplets
derived from the preserved base text. Keep these per-producer values before merged
magazine defaults can affect the service; lifting alone does not prove encoding.

The native encoder and browser preview consume the same immutable validated model
and generation. Preview offers enhanced and base views. Diagnostics identify page,
raw subcode and position, adding resource/source-file locations where applicable.
A preview never replaces native encoding, transport or actual client evidence.

### Enhanced model and wire boundary

Keep decimal display identities and the closed v1 model unchanged. Enhanced
documents contain a mandatory base display set, separately typed enhancements
and non-display resources. Resource addresses carry a magazine and raw page byte;
they are not decimal display pages. Resource kind and logical subpage identity
are authored values; physical update and last-packet header bits belong to the
compiler. Validate reserved-purpose addresses, collisions and dependencies over
the complete service, including transitively invoked objects and DRCS.

Raw resource magazines are 1–8 and data-page bytes 00–FE. Respect Annex A.10's
exact reserved purposes: 1BE (ACI), 1F0 (Basic TOP), each magazine's FD (MIP), FE
(MOT) and FF (terminator/null). MIP fixes logical S1 to zero; MOT's zero convention
is a recommendation, so retain other valid S1 values. Object/DRCS addresses may
contain only decimal digits; a hexadecimal digit is recommended, not mandatory.
Derive function/coding from kind and keep logical S1 separate from physical
update/last-packet bits. Exact resource page/S1 collisions cannot be hidden by
different kind tags. Do not infer whole-page-number function exclusivity across
different logical subpages without a normative or actual encoding constraint.

Use checked five-bit colour references and four-bit RGB components. Only CLUT2/3
are editable in Level 2.5; fixed CLUT0/1 and transparency retain their standard
meaning. DRCS glyphs have ten rows of twelve monochrome pixels, checked glyph
slots 0–47 and logical subtable indices 0–15. Representation validation does not
replace the per-display limit of two selected subtables and 24 distinct referenced
glyphs. Objects retain local/public/global identity and active/adaptive/passive
semantics; structured non-spacing operations retain their position and character
designation. Side panels and page/magazine defaults remain explicit model data,
not browser-only styling. Packet ordering and header synthesis are compiler work.
DRCS links resolve normal/global scope before checking exact subtable and slot.
Repeated use of the same glyph does not consume additional distinct-glyph budget.
A catalog validator supplied with references does not itself prove complete
transitive reference collection, object expansion or display/resource allocation.

Represent enhancements as ordered semantic commands, with object definitions
separate from local page data. Object identities carry local display ownership or
checked public/global resource ownership and semantic IDs; the compiler assigns
physical pointer slots. Invocation includes the expected object type and optional
origin modifier, never a detached modifier that can affect an unrelated command.
Active objects may invoke adaptive/passive, adaptive may invoke passive, and
passive may invoke none. Resource objects cannot invoke local definitions; local
references retain exact page/subcode ownership. Resolve every declared target and
type before traversing the effective page program.

Preserve a complete X27/4 declaration separately from packet absence and resolved
selection. An explicit FF/FFFF target retains its magazine; a real target's zero
mask requires no subpages. Keep assigned level applicability and fixed GPOP/POP/
GDRCS/DRCS roles. Check required Level 2.5 resource kind/page/S1 even when no
effective command currently uses it. When the applicable declaration matches
the supplied selected owner, check used-S1 membership for every effective
explicit invocation and DRCS reference, including dependencies inside defaults.
Only synthesized direct default-object roots retain their independent exemption.
A different selected owner or null declaration requires separate authored
resolution/provenance; these checks cannot infer a missing MOT policy.
Encoder-reserved fields use a documented
writer convention, not an invented universal input validity rule.

Default-object roots keep their independently declared original MOT resource,
checked catalog identity, scope and type. Do not rebind them to another X27-selected
page. Only synthesized default roots bypass explicit-invocation link matching;
local and nested calls retain their supplied-selection checks and shared budgets.
Preserve the explicitly selected MOT identity, including nonzero logical S1.
Retain all 256 raw association slots; reserved FF is not a usable association.
Keep one global and seven public/normal links per object/DRCS bank, contiguous
maximum-S1 extents, supersedable flags and independently owned default slots.
Inactive pointer bytes and reserved GPOP fallback bits remain raw metadata.
Coverage masks and page totals declare accounting, not resource availability
or cross-magazine inheritance. Actual default definition existence/type and
physical pointer realization require separate catalog/compiler proof.
Canonical reserved padding in generated packet bodies is a writer convention;
lossless raw import must preserve ignored bytes externally rather than normalize
them silently.
The mixed-default/X27 nested-call policy still requires authored resolution proof.

Collect DRCS references and charset-pair designations over all invoked programs
under caller-owned work/output budgets and per-type invocation limits. Preserve
the source command/object location for diagnostics. An explicit DRCS glyph
reference includes scope, subtable and slot; the compiler emits scoped selection
packets rather than inheriting a caller's selection. Separate page and X/26 charset
designation validation: X/26 Latin aliases omit national substitution. Secondary
page designation changes G0 only and contributes no new G2 or X/26 pair. Count the
primary default plus transitively designated X/26 pairs, preserving raw aliases.
Complete reference collection does not establish resolved geometry, side-panel
placement, attribute inheritance/overlap, glyph composition or encoding readiness.

Keep a complete checked presentation declaration shared by page X/28/0 and
magazine M/29/0, but retain their presence separately. Page presence overrides
magazine presence; absent both, use Annex D region-zero header selection rather
than choosing a repertoire from language metadata. Header option seven alone is
reserved; an explicit assigned primary alias ending in seven remains valid.
Preserve all raw panel flag/width/status combinations, including disabled panels
and a zero-width side of a two-panel declaration. Level 3.5-only panel applicability
is valid metadata suppressed at Level 2.5, not a claim of Level 3.5 rendering.

The preparatory semantic service receives explicitly resolved page resource
selections. This is a compilation boundary, not the authored v2 wire contract or
an implementation of MOT/X27 parsing/precedence. Retain future source declarations
and resolve them explicitly before this boundary. Keep local X26 presence distinct
from an empty program: any local declaration suppresses supplied default slots.
Local definitions destined for carriage require that declaration. Defaults retain
ordered GPOP1/GPOP2/POP1/POP2 slots and semantic identities; the compiler later
allocates their physical selectors. Validate declared links and default targets
even when suppressed, but charge only effective traversal against per-display
character, glyph and invocation limits. Check every effective transitive resource
object against the display's selected POP/GPOP and each glyph against selected
DRCS links. Repeated calls and separate display subpages consume aggregate work.

Host input, unique resource-record, object, structural-item, content-byte, authored
command and expanded-work/output budgets are independent of serialized producer
content. A successful semantic aggregate is immutable and exposes no publication
receipt. Display raw subcodes and resource logical S1 remain distinct: physical
header collisions are checked after compiler synthesis, not by fabricating a
resource subcode or banning all shared page numbers. The legacy Latin encoder
cannot compile this aggregate or replace its effective presentation with its own
X/28 packet; add a complete enhanced compilation result rather than discarding
resources into the existing base-only result.

Resolve inheritable presentation/resource selections within each producer's
owned contribution before channel composition. One addon's magazine declaration
must not restyle another addon's pages. When producer defaults differ within the
same magazine, materialize equivalent page overrides in the composed service;
retain original declarations in design/export metadata. Package/profile receipts,
ownership and stale-selection fences still belong to the application boundary.
This composition step remains unimplemented; a standalone semantic service does
not establish multi-producer publication or profile acceptance.

Introduce `teletext.pages.v2`, retaining the existing `pages` operation. Its request
uses `schema_version = 2` with the same guide input fields and limits as v1;
its response uses version 2 for the enhanced document. Give v1 request/response
independent version constants without changing their existing values, shapes or
parser behavior. Choose the highest declared
host-supported contract from the same installed-manifest/settings snapshot,
retain that choice in the invocation receipt and require the exact response
version. A v2 failure never triggers a v1 retry or lossy downgrade. Existing
installation hash/version and selection fences apply; a capability string alone
does not require a manifest schema migration. No producer may choose publication
identity or turn a structurally valid document into a ready generation.

Mixed v1/v2 producers receive identical guide values and generation with the
appropriate request version. Convert v1 losslessly into base-only contributions
to the shared enhanced domain model. Both invocation preflights and response
validation enforce the selected version; response contents cannot negotiate it.

Existing payload/time/service budgets remain host resource policy, not claims
about normative teletext limits. Enhanced validation must bound resource counts,
dependency traversal, object expansion and encoded output before publication.
The full enhanced aggregate and encoder implementation remain separate work.

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
Edit-policy validation includes indirect effects: a shared palette, object or
DRCS change can modify protected/generated cells even when its definition lies
outside their geometric bounds. Checking edited coordinates alone is insufficient.

### Durable design ownership

Existing graphics upserts have no draft revisions or export compare-and-swap;
addon-list lifecycle records have different ownership. Reuse their immutable
snapshot and transactional revision patterns, not their tables or `ConfigElement`.
Add a `TeletextDesign` head/tombstone, immutable `TeletextDesignRevision` records
and server-owned `TeletextDesignExportReceipt` records. Revisions retain a bounded,
versioned document, digest and original selection/edit-policy context. Receipts
retain the exact export revision, stable file mapping and resource baselines.
Keep these records when a profile or package is removed; do not cascade designs
from `AddonInstallation`.

Saving/importing requires the expected head revision and one immediate write
transaction; overflow, stale input or any failure preserves the previous head.
Validate the complete import before writing. The supplied sidecar cannot broaden
the receipt's edit permissions. Export only permitted design metadata, never a
general settings bag containing secret or local-file references. Package/profile
changes make the original selection context stale and require explicit resolution.

Explicit publication references an immutable `(design_id, revision, digest)` and
a fresh complete channel-selection fence. Saving/importing never sets an active
flag. Add durable pending/publication state with the actual service promotion
owner, not as a substitute for it. After restart, drafts and receipts remain
readable without automatic transmission; publication/restart must separately
restore only accepted generations under the existing profile/Lua timing contract.

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
