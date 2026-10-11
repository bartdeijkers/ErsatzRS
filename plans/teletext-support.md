# Teletext support plan

## Status

In progress. The 2026-10-10 maintainer decision in
[ADR 0120](../docs/adr/0120-separate-teletext-engine-from-addon-content.md)
sets two stages: first the generic ErsatzRS teletext engine, then one new
TV-guide content add-on in ErsatzRS-Addons. Six milestones include a separate
engine acceptance gate (`p9a-04`). P9A-01 is reopened for the authorized Level 2.5
and external-editor contract in
[ADR 0123](../docs/adr/0123-level-2-5-teletext-and-external-editor-workflows.md).
The earlier [ADR 0121](../docs/adr/0121-teletext-pages-transport-and-addon-contract.md)
baseline acceptance remains historical evidence;
P9A-02 has bounded selected-runtime decoder probes, with packaging and platform
acceptance still open. Implementation follows the completed `ucu-09` and other
recorded prerequisites. UCU-09
has a fixed upstream v26.11.1 boundary; later upstream releases do not extend
that gate. No page engine, source metadata, UI or content add-on acceptance is
claimed. The authoritative execution state
lives in
[`docs/progress/milestones.toml`](../docs/progress/milestones.toml) and
[`docs/progress/parity/teletext-support.toml`](../docs/progress/parity/teletext-support.toml);
this plan owns future sequencing and acceptance boundaries.

## Goal

Deliver a reusable teletext engine and then an add-on-authored TV guide inspired
by NOS Teletekst. Authored support must reach EN 300 706 Level 2.5 for design,
validation, encoding, publication and display, with correct Level 1/1.5 fallback
and QTeletextMaker TTI interchange. Keep three distinct concerns explicit: DVB teletext subtitle
pages embedded in source transport streams,
interactive full-page teletext browsing, and an authored teletext service
carried in ErsatzRS output. Scan and persist the accepted source metadata,
provide predictable selection and playback behavior, and expose capability or
failure through health and troubleshooting on every supported platform.

## Ownership and content scope

ADR 0120 owns the engine/content split. The planned boundary is:

| Owner | Responsibility |
|---|---|
| ErsatzRS engine | Generic page/service model, validation, character/layout primitives, navigation, page publication, supported rendering/transport, capability, admin and health. |
| Existing add-on boundary | Versioned read-only guide input and typed page output, compatibility, bounded invocation, cancellation and failure handling; concrete schemas are a P9A-01 decision. |
| One new tvguide teletext add-on in ErsatzRS-Addons | Page map, index and navigation links, TV-guide layout and actual page content using host-supplied programme context. |

The first stage uses synthetic pages to prove the engine without production
content. The second stage defines the guide's numbered index, channel/programme
pages and compact colour/text presentation using only the TV-guide-related
[NOS Teletekst pages 200–299](https://nos.nl/teletekst/200) as the design example.
This includes guide index/subpage navigation and programme listings. Other NOS
sections and unrelated information pages within that range are outside scope.
It does not include scraping NOS, republishing its news or copying its branding.
The reference range does not fix the add-on's page numbering; detailed page
numbers and layouts belong to the add-on design.

Reuse existing add-on packaging, trust, explicit installation/enablement and
bounded process execution. No new renderer process, direct database access,
general-purpose public add-on HTTP API or automatic repository distribution is
required by this plan. Current checkout/ZIP installation avoids a dependency
cycle with OAD-01 through OAD-04, which remain after Phase 9 and Phase 10.

## Level 2.5 authoring and external editing

ADR 0123 owns the accepted scope. The
[standards/editor audit and interchange reference](../docs/reference/teletext-design-interchange.md)
records normative clauses, the pinned QTeletextMaker 1.0 source, actual TTI
normalization/metadata losses and the planned structured file workflow.

The technical boundary retains the closed v1 contract and adds
`teletext.pages.v2` with exact request/response version checks from the installed
selection snapshot. Both versions converge on one domain model; v1 contributes
its unchanged base content. Checked palette/monochrome DRCS values, raw resource
identities, ordered enhancement programs and checked object closure form the
model foundation. Bounded effective-program traversal collects transitive DRCS
references and checks primary/X26 pair and invocation limits. Presentation
declarations and the explicitly resolved semantic service retain page/magazine
presence, panels and default suppression while checking aggregate dependencies.
The presentation packet-body encoder and bounded v1 lift preserve explicit
page/X26 presence and unchanged v1 bytes. Full mixed-v1/v2 compiler convergence
remains open. Checked X27/4 declarations and body coding preserve masks,
null/absent distinctions and applicability; the service checks required inventory
and used-S1 membership when the applicable target matches the supplied selection.
Source-located checks cover explicit calls and glyphs inside default programs;
only original synthesized MOT roots are exempt. Different-owner/null resolution
remains open. Default roots retain their original owner instead of rebinding.
Checked Level 2.5 MOT declarations and independently decoded body coding now
retain explicit owners, full banks, extents, original default slots and ignored
fields. Supplied physical selectors are checked without truncating semantic IDs;
this does not prove object allocation or carousel lifecycle.
Native monochrome DRCS bodies preserve slot/pixel geometry and sparse undefined
glyphs; X28/3 declares modes explicitly. Checked resource-header metadata retains
all six lifecycle states and exact physical subcodes. Complete MOT/DRCS writers
derive actual last rows, emit bounded DVB sequences and handle empty DRCS without
defining glyphs. Body/history consistency, service ordering/repetition and actual
client proof remain open. Nine opt-in libzvbi 0.2.42 MOT-based cases prove exact
glyph pixels/native offscreen rendering and isolated invalid-glyph fallback.
X27 decoding limitations are recorded without altering normative wire output.
Full independent feature decoding remains open. A bounded local X26 compiler
now lowers typed commands with independent DRCS latches and located order/limit
errors; twelve regressions cover every operation family and packet boundaries.
The nine native cases pass with byte-identical production-compiled invocations.
Bounded POP/GPOP allocation and resource invocation now preserve complete IDs,
default priority, exact selection/type and offsets under normative capacities.
Thirty-four focused regressions pass. Complete POP/GPOP writers now retain exact
pointer destinations, mandatory X28 function metadata, original definition order
and independent per-object state. They preflight the full body capacity and allow
explicit empty companion resources. Eight pinned native cases independently
decode all public/global object types and nested relative origins, including
exact glyph/colour behavior and unchanged base cells. External decoding beyond
the first pointer bank remains open. Local-object compilation now packs the main
program and exact-owner definitions into one 208-slot sequence, including forward
and nested references; eleven new regressions bring the focused total to 45.
Actual .42 local-object decoding fails the correct glyph/colour oracle. A separate
fault diagnostic reproduces fallback without changing normative encoder bytes or
rendering acceptance. The unchanged correct three-case oracle now passes a second
independent decoder, teletextimager 0.2.0 at the pinned commit recorded in the
interchange reference. Its own T42 reader and formatter verify glyphs, colours,
type-specific inheritance and unaffected base cells. Imported source hashes are
guarded; it is an opt-in proof tool, not a runtime dependency or navigation client.
Full external feature decoding and suitable-client display/navigation remain due.
Three further independent cases cover designation 8/15 and nested local origins.
Private positioned base-glyph events now support the upcoming context-aware main
composer while retaining exact legacy fallback/packet bytes and diagnostics.
Seven new regressions, the unchanged wire snapshot and actual pinned Unicode/
subtitle/national-subset decoding pass. Merge authored AP/charset/header-colour/
default context before final local-definition sizing; do not concatenate bodies.
Row-zero position lowering and base/enhancement composition remain required work,
without permanent scope restrictions.
The compiler now reuses exact-owner effective program selection, retaining local
empty overrides and sparse original MOT roots. Original colour context is resolved
before insertion, with main/Active priority, exact row origins and bounded repeated
work. An ordered main-program intermediate now merges positioned replacements
with unchanged original commands, preserves invocation anchors and source
provenance, suppresses duplicate base/compatible glyphs and restores row-scoped
charset context under input/output/expanded budgets. It carries original colour
context but does not grant readiness. Final dual-receiver wire positions,
Active/base priority, geometry/default ownership and shared definition sizing
remain required before complete service composition.
The same-service composed packer now includes opt-in compatibility-row restoration
and safe initial header colour selection in final shared sizing/pointer allocation.
Local-call restoration counts are independent of provisional pointer values.
Complex header normalization remains required: no neutral selector preserves both
receiver states after header column advancement. Pinned imager proves selected
cells, not all historical Level 1.5 transitions; retain separate wire and final
client/decoder acceptance evidence in the interchange reference.
Main composition now retains a bounded domain-owned base display trace derived
from the actual checked layout. It preserves authored intent separately from
coded fallback, spacing-control timing and attribute provenance, raw/remapped
colours and double-height origins. Next resolve original object invocations and
attribute inheritance against this trace, then footprints and both receiver
views before normalizing physical instructions. A base-derived Unicode glyph
must not gain local-enhancement priority over an authored Active object. This
intermediate grants neither rendering nor publication acceptance.
Original spatial tracing now preserves repeated/full-ID calls, nested caller
origins and sparse original defaults under bounded two-pass allocation. Main
composition consumes it independently of generated base glyphs. Next combine
base intent and original object events: prove base `é` versus Active `A` at both
receiver levels, then the original Local/Adaptive/Passive glyph tiers and the
standard's distinct colour-inheritance behavior. Glyph rank alone must not choose
attributes. Resolve footprints, panel areas and physical realization before
granting full display or publication authority.
The retained glyph layers now compare original character candidates by their
standard-defined rank and invocation identity while keeping losing base intent.
Resolve all four attribute groups under their distinct inheritance and scope
rules, preserving C2 source evidence and unresolved transmission ties. Do not
infer attributes from the highest character rank. The diagnostic layers
retain raw out-of-area/header/panel candidates for explicit geometry validation;
no overlap, pixel rendering or wire equivalence acceptance is implied.
The consumed original attribute executor now retains all four groups separately
from character rank, with per-cell lower-context inheritance, independent panel
defaults and located pending C2/Adaptive-extent diagnostics. Next settle pending
transmission and endpoint rules, execute character-set lifetimes, validate object
overlap/areas and size footprints, and normalize the physical instructions against
both receiver views. Full display/publication acceptance remains open.
All original candidates now retain pre-geometry attribute observations, including
losing private Passive state, with shared stages before Adaptive and Passive.
The layer-aware resolver now consumes original writers separately from these
diagnostic stages: spacing lower-row suppression, shared and per-instance
execution, origin fragments and deferred obscured attributes. It does not resize
a flattened glyph winner or use those observations as final attributes.
Coded-base fragments now retain spacing Double Height suppression, exact upper
origins and separate mapped lower-row default baselines. Ignored lower controls
do not cascade. Replay retains requested/effective size and each obscured
writer's original placement/source, with compact immutable fragment origins.
Illegal height boundaries, physical-order ties and inherited Adaptive non-origin
sizing remain pending. A.4 now checks known raw own AP addresses including setup,
charset and disabled panels; children retain separate areas. A.7 uses that same
address membership and whole-instance attribute-type masks, preserving permitted
Passive/mixed pairs and separate C.3 size fragments. Next settle remaining
physical realization by reusing the existing full-program per-type 127 counter,
which already includes repeated/nested calls and all selected default roots per
exact page/subcode. Validate the actual augmented catalogue/program, preserving
default ownership and physical/source ordinals. Then settle remaining
glyph/attribute/charset/DRCS and whole-page display/colour equivalence from the
new supplied-program spatial replay, which shares the existing bounded walker
without composing Base fallback again. Use supplied-program DRCS replay with
fresh bounded collection and complete DRCS dependency validation, checking
actual main/body sources while retaining the
original service's links and pixels. This covers new carrier IDs and shifted
main ordinals without treating detached collected references as authority;
placement, instance and full object/default-owner validation remain separate.
Shared physical-display replay now executes the complete resolution chain on
that actual program, retaining the original target separately and identifying
physical main ordinals in diagnostics. Compare both whole-page projections;
reject changed actual screen/row planes before reusing original Base/footprints.
the current Unicode/Active discrepancy remains. Preserve original MOT owners
through a proven encoding strategy: explicit resource calls carry no page
address, so relaxing selected-page checks alone would route to the wrong page.
The actual-program packing diagnostic now binds supplied replay to final main/
local semantic words in one call, preserving selected links and header colour.
Same-emitter C.2 uses exact physical sources and instances; unencoded resource
words and cross-instance order remain pending. Original targets stay separate.
Physical raster now uses that actual replay through the shared renderer, retaining
raw transmitted Base fallback instead of restoring absent authored Unicode.
Located unresolved cells and budgets remain explicit; host pixel comparison alone
cannot prove glyph lineage, whole-page semantics or independent receiver behavior.
Bounded whole-page comparison now observes every cell, effective fragment/origin
geometry, colour references and both reveal/flash responses through shared mask
helpers. Histories remain separate from selected uncertainty; zero findings cannot
certify authored identity or matching generations. Next bind generated carriers to
exact original display origins, then prove the actual normalization with decoding.
Carrier derivation still needs identity-preserving Base national-subset conversion,
unchanged-anchor placement and temporal flash context. Preserve original colour
dependencies before explicit carrier backgrounds; no permanent exclusions are
accepted for these remaining tasks.
One-origin Passive recipes now retain exact borrowed origin/source lineage,
effective typed attributes/glyph context and bounded full-ID allocation, validating
original C.1 before explicit backgrounds. A representative retained-AP placement
passes complete selected-model comparison and pinned dual-level literal decoding.
Bounded candidate placement now preserves whole-component DAG order and derives
latest unchanged main gaps from actual composed root events, with selected-panel
inverse mapping and checked componentwise offsets. The pinned representative
wire oracle uses this derived gap; general later-paint equivalence remains open.
Bounded augmented assembly now preserves originals, inserts contiguous component
calls and allocates distinct lowest-free IDs. Private emitter maps retain original
source/root/instance facts separately from generated calls/body roles. Exact body
sizes and storage bounds precede copies; both recipe passes and aggregate ID scans
are charged. Pending components report the blocking origin. The representative
native oracle consumes this producer and still passes complete selected-model and
dual-level literal comparisons. A paired factory now owns actual encoding/replay
and bounded original/actual ancestry: sparse roots, nested parent/callsite and
target/type/origin/caller/offset facts match with complete unique coverage.
Generated carriers retain separate exact-placement instances; numeric ID shifts
are not evidence. Next check resolved typed glyph identities and exact original
display origins before whole-page equivalence; these components do not correct
the automatic composition path yet. Bounded typed comparison now checks actual
assigned identities against exact original origins and paired provenance, including
Unicode replacement context and full DRCS resources. Equal-mask false positives
are rejected; complete structural/colour comparison remains separate. The native
representative passes both full1000-cell views and literal decoding. Next connect
the same-service normalization entry with original final-word C2 settlement;
foreign MOT-default activation and row-zero lowering remain required/open.
Preserve original-source mapping and bind C.2 evidence to final physical words.
Complete remaining physical-order, A.7/A.8 and glyph evidence before normalizing
both receiver views. Fragment tests alone
cannot accept the engine, client, editor or publication workflow.
The final main/local packer supplies actual word positions for known same-emitter
C.2 ties; compiled composition maps original sources/instances and rebuilds the
shared attribute/display/colour result. Standalone and cross-instance ties remain
pending, with no inferred resource or recursive-preorder chronology.
Known selected cells now rasterize from the same unpublished encoder
composition: pinned MIT masks, exact original DRCS, effective C.3 fragments,
fixed/selected CLUTs and C.1 video/row/window planes. Authored Unicode lifts at
Base priority and retains higher-layer overrides. Bounded output retains located
unresolved pixels; it does not authorize publication. Receiver-owned header
cells use a distinct marker and exact identity in preview navigation. Invert
precedes supplied flash phase; enlarged bottom fragments retain origin-row planes.
Eight opaque DRCS rectangles match independent libzvbi 0.2.42 pixels. Physical
replay must use the corrected separation contract: enhanced
G1 uses D5 alone, Base G1 also uses surviving spacing separation, and explicit
D5=0 cancels an earlier spacing writer with its original C.2/C.3 scope and
deferral. Defaults do not cancel; later spacing separation can restore it.
Before full engine acceptance, retain independent wire/raster evidence for these
cases alongside the unchanged Unicode/Active positive oracle. Next settle
remaining source/extent, Arabic composite and complete background-parameter
evidence. Known alphabetic composites now fit exact current masks under explicit
font policy; definite consumed window/inverted-adjacent BBS parameters retain
located failures while explicit backgrounds remain valid. No whole-service
admission follows. Next consume this raster in the localized preview and prove
physical equivalence and independent client display/navigation. Engine,
external-editor and add-on acceptance stay open.
The production-wire Unicode/Active regression currently fails: page 200 with
German national option one displays pound instead of the original Active `A` at Level
2.5, while Level 1.5 remains correct. Correct this before engine acceptance.
First retain a bounded physical-realization plan from final fragment owners and
whole effective origin footprints, with original candidate/source identities and
explicit unresolved evidence. Then lower a proven realization without changing
authored packages or losing Level 1.5 fallback. Preserve higher partial overlays,
all glyph families, charset lifetimes, background provenance and origin-row planes;
packet or candidate order alone cannot establish equivalent output. Revalidate
actual generated invocation/open/word/resource limits and independently decode
both receiver views. An empty or successful plan does not certify the wire.
The checked G1 alphabet operation now preserves mode 0x01 and base fallback;
selected Latin positions decode correctly with libzvbi 0.2.42. Pinned imager's
alphabet-font gap is a separate diagnostic, not positive Level 2.5 evidence.
Complete current G0/G2 resolution for every original contender. Prove
non-Latin G1 and D5 underline separately; do not rewrite G1 as a compatible glyph.
The consumed original charset executor now keeps a context for every contender,
with shared spatial Local/Active state, fresh private Adaptive/Passive state and
exact declaration provenance. Equal-position cross-source order stays pending;
secondary metadata creates no ESC or new G2. Next settle source-order questions,
execute glyph/mark tables and supported ESC semantics, then prove non-Latin Base
fallback and normalize both receiver views with geometry/footprint validation.
Finite original glyph lookup now preserves all G0/G2 table identities, G1/G3
and raw DRCS references in the same immutable composition. Base national aliases
use the actual Table 32/36 subset; enhanced glyphs omit national substitution.
Explicit bans, unmarked cells, nonalphabetic bases and unverified composition
remain distinct. Next implement verified glyph shapes/Unicode projection and
non-Latin composition, then DRCS selectors, size/object/panel geometry and final
receiver-equivalent normalization. This lookup cannot authorize preview or
publication; the interchange reference owns exact table exceptions and proof.
Original logical DRCS references now resolve to exact resource identities and
bitmaps from the same immutable service, preserving losers and original sources.
Reuse the existing explicit-reference lowerer and fresh private selectors; do not
invent a second authored selector interpreter. Next apply validated glyph shapes
and DRCS pixels under complete colour/size/footprint/panel rules, then normalize
physical order against both receiver views. Preview and publication remain gated.
Selected geometry now preserves raw/visual panel placement and original instance
address/write facts in the same composition model. Definite own cross-area writes
are rejected; separate panel children and pending endpoint evidence stay distinct.
Known raw inclusive intervals now reject sixteen simultaneous open instances
across all types; unresolved endpoints remain explicit. A combined raw-position
call/write scan also rejects sixteen known successive openings, including empty
objects. Descendant writes can reset between call positions; plane writes and
mixed-source ties remain explicit rather than granting complete admission.
Agreeing contending size flags establish geometry without discarding unresolved
writer provenance. Next complete A.8 pending
interval and successive-opening rules, A.7 admissibility and C.3 footprints,
origin/deferred attributes, then validated glyph rasterization and receiver-
equivalent normalization. Point placement alone cannot close engine acceptance.
A bounded physical inventory fence now compares exact compiled display/resource
headers and stable logical resource owners. Eleven regressions cover actual
empty-resource collisions, counter/last-row changes, magazine eight and MOTs.
The final compiler must feed every unique page, including empty companions, into
this check; service completeness and activation remain unimplemented.
Bounded per-display MOT/X27 derivation now preserves role provenance, exact
masks, original defaults/fallback and unresolved candidates for audited conflicts.
It does not decide effective defaults or presentation.
Next integrate derivation with catalog/effective-default checks, finish audited
edge policies and prove declaration-to-selection consistency, then final display/object
semantics, physical service allocation and the native compiler.
These preparatory checks cannot close P9A-01 or the engine acceptance gate.
Durable designs use immutable revisions, head compare-and-swap and host export
receipts; pending/publication state belongs to the eventual promotion owner.

- Extend the typed model, versioned addon payloads, native packets/resources and
  browser renderer through Level 2.5: palettes, extended characters/mosaics,
  non-spacing attributes, DRCS, local/public/global objects and side panels.
  Preserve readable, correctly designated base content. Reject Level 3.5-only
  features with precise diagnostics; no decoder-only or partial-feature acceptance.
- Export a page, subpages or supported set as editable TTI plus versioned external
  metadata/dependencies. Pin and verify the real editor version. Restore exact
  identities/navigation through explicit mapping; reject ambiguous or unsupported
  changes without silent loss. QTeletextMaker's own limitations do not reduce
  the engine target.
- Import only into a separate concept. Validate and preview from the same model
  used by the broadcast; report page/subpage/position and encoding problems.
  Keep durable drafts and explicit publication separate. Import cannot change
  an active generation, profile selection or installed package.
- Preserve addon-owned templates, theme/override settings, declared zones and
  dynamic fields in the sidecar. Distinguish free content, fillable zones and
  automatically generated guide regions. Validate edit permissions against host
  state, including indirect palette/object/DRCS effects on protected regions;
  template changes require an explicit addon version workflow.
- Prove export → actual editor open/edit/save → import → validation/preview →
  explicit publication. Provide bounded file inputs and structured diagnostics
  for AI agents through the same validation/application functions. Localize all
  changed admin with headed, light/dark, hydrated/no-JS browser evidence.
- Prove representative enhanced functions with TSDuck transport checks,
  independent decoding and actual display/navigation in a demonstrably suitable
  client. Record client/decoder versions, settings, coverage and limitations.
  Kodi is a candidate, not mandatory or a runtime dependency; an equivalent
  suitable client is allowed. TSDuck alone cannot prove Level 2.5 display.

First prove these with independent synthetic engine pages at P9A-04. Only then
start the sole production TV-guide addon. Existing source-subtitle, alternate
audio, native/Next, platform and failure gates remain. Necessary engine limits
must be proposed explicitly to the maintainer; missing functions remain blockers.

## Channel profiles, themes and Lua control

The maintainer's scope extension is accepted in
[ADR 0122](../docs/adr/0122-channel-teletext-profiles-themes-and-lua-control.md).
It is required Phase 9 work, with implementation and independent acceptance open.

- Each `(channel, add-on)` binding has an enable state and selected named profile.
  A channel supports several add-ons; the same installed add-on serves multiple
  channels with different profiles simultaneously. A profile references that
  add-on and its separately scoped, manifest-validated settings. Profile changes
  do not rewrite global installation settings or activate packages.
  Persist profile/settings/binding revisions atomically, retain unavailable
  selections after profile deletion or package uninstall, and allow a binding
  to be switched Off even when its profile or package is unavailable. ADR 0122
  owns storage bounds, inheritance and revision-fence details.
- Prepare multiple bindings as one channel candidate with an explicitly selected
  entry add-on for its initial page/service language. Reject overlapping page/
  subpage addresses and more than 1,024 records across all producers. Capture
  and recheck the complete enabled membership and effective settings around
  serial invocation under one shared deadline. Validate the whole native encoding
  and retain the same immutable logical pages for future browser delivery;
  service admission, persisted entry policy and atomic publication remain gates.
- The TV-guide add-on declares named colour/layout themes, permitted overrides
  and defined page zones accepting custom values. A profile selects those values.
  Presentation and zone semantics stay in the add-on; the host validates complete
  standard pages and rejects collisions or unsupported content without truncation.
  Stream and browser use the same immutable validated channel generation.
- Admin persists base selections through application commands. Lua controls
  existing profiles and toggles through the same application validation, via a
  core port. During generation, commands create candidate channel/binding
  intervals at the scheduling cursor, until the next successful command or the
  candidate timeline end. At expiry, restore the current admin base. Continue
  starts from base unless its script selects an override again.
- Publish override intervals atomically with the schedule. An uncaught script
  error rolls back the entire candidate. Restart restores current published
  intervals without replaying Lua; expired or stale revisions cannot return.
  Refresh/Reset preserve or replace overrides at the programme boundary, and
  mirrors resolve independently by output channel identity.
- Profile/enable changes publish at a safe carousel/output boundary after complete
  validation. Preparation delay is visible as pending/unavailable; it cannot pass
  off another profile as current. Disable withdraws the channel's binding safely.
  An active explicit Lua override takes precedence over base edits until expiry;
  profile edits invalidate its snapshot without extending that expiry.
- Missing, deleted, wrong-add-on, invalid, disabled or incompatible profiles have
  typed visible outcomes. Rejected commands leave the selection intact. Existing
  selections that become invalid follow bounded stale/withdrawal policy without
  damaging ordinary playback or other channels. Scripts install/enable no packages.

Use existing installation/settings definitions, channel application commands,
Scripted resolution and atomic schedule publication. Add only the necessary
profile/binding/interval transactional relationships; ConfigRepository's
independent get/set operations cannot substitute for their atomic revision checks.

P9A-04 proves controls with synthetic producers before production content:
channel and mirror isolation, multiple bindings/page collisions, simultaneous
profile presentation, shared-profile edits, Lua transitions/expiry/rollback,
restart inside/after overrides, Refresh/Reset and missing/invalid profile safety.
All changed admin surfaces require every supported culture and matched no-JS/
hydrated browser proof. P9B-01 then supplies the single production TV-guide
add-on's themes/overrides/zones; P9B-02 repeats the scenarios with its exact
installed package, actual guide pages and browser/stream generation comparison.
Existing source-subtitle, alternate-audio, native/Next, six-RID and output gates
remain required. NOS reference scope stays limited to TV-guide pages 200–299.

## Evidence from the 2026-10-07 planning review

- ErsatzRS currently has no teletext-specific domain model, scanner branch,
  selection policy, FFmpeg capability requirement, admin surface, or runtime
  proof.
- Embedded subtitle extraction recognizes SubRip, ASS, WebVTT and MOV text and
  distinguishes selected bitmap codecs, but does not classify or decode
  `dvb_teletext`.
- The first-party FFmpeg 9 runtime contract does not currently require or probe
  `libzvbi_teletextdec` or the `--enable-libzvbi` build flag.
- FFmpeg documents DVB teletext as demux/decode support supplied by the optional
  libzvbi dependency. The decoder can select subtitle pages or explicit page
  numbers and can produce bitmap or text/ASS output. FFmpeg does not document a
  DVB teletext encoder, so decoding support must not be presented as proof that
  ErsatzRS can author a teletext service.
- Native FFmpeg and Next are now integrated engines. Next publishes selected
  tracks and subtitle probe hints, but its current host wire types carry no
  teletext page or decoder options. Native support cannot establish Next support.
- [ADR 0095](../docs/adr/0095-native-pipeline-instead-of-ersatztv-next-engine.md)
  conditionally accepts Next as the new-channel and standalone-preview default
  only with automatic verified runtime availability. That acquisition/default
  implementation is still a UCU-09 candidate at this planning checkpoint.
- Existing selection and preparation include exact media versions, provider
  stream identity, bounded Next subtitle artifacts, and edited-source subtitle
  timing. Reuse those boundaries rather than adding a parallel extraction path.
- The current managed FFmpeg pin is v2.0.6. P9A-02 must inspect the actual pinned
  FFmpeg and Next artifacts at implementation time; version or package presence
  alone does not prove teletext capability.
- The 2026-10-07 search of the local ErsatzTV checkout at `df6ae228` found no
  teletext-specific policy, as in the initial 2026-08-07 search. P9A-01 must
  repeat the source audit at the completed UCU baseline and classify each
  accepted behavior as parity or an ErsatzRS extension.

## Source map and technical decision

[ADR 0121](../docs/adr/0121-teletext-pages-transport-and-addon-contract.md)
owns the technical page, DVB/browser delivery, add-on and lifecycle contract.
The 2026-10-10 audit uses ErsatzTV legacy commit
`46118d43313b7828780fbfcfb46e980390ea8b0d`:

- ErsatzTV UI/components/validators: no teletext-specific policy found in C#,
  project, JSON or Markdown sources; authored-page UI is an ErsatzRS extension.
- Existing selection: `$ERSATZTV_SRC/ErsatzTV.Core/FFmpeg/FFmpegStreamSelector.cs`
  preserves language, forced/default and embedded-source constraints.
- Existing metadata: `$ERSATZTV_SRC/ErsatzTV.Core/Domain/Metadata/Subtitle.cs`
  and `Domain/MediaItem/MediaStream.cs` have generic codec/index/language fields,
  but no page/service identity or teletext extraction format.
- Existing side effects: extraction/preparation, source ownership and retained
  interval timing stay in their established owners; authored pages do not
  modify schedules or consume media inputs.
- ErsatzRS targets: domain page/service values; core selection/guide rules;
  add-on-contract DTOs; infrastructure scanner/persistence/invocation; FFmpeg
  encoding/probes; application coordination; web viewer/admin/transport.
- Visual reference for the future content add-on: only TV-guide-related NOS
  200–299 pages. Shared SSR forms, links, localization and hydration primitives
  govern host management; P9B-01 owns the detailed content-page visual map.
- Intentional deviation: authored page generation is the extension accepted by
  ADRs 0120/0121, not a claim of existing ErsatzTV teletext parity.

## Non-goals before the Phase 9 ADR

- Do not assume that “teletext support” means subtitles only.
- Do not promise full-page authoring or MPEG-TS teletext carriage from decoder
  availability.
- Do not treat DVB bitmap subtitles, closed captions, ordinary text subtitles,
  and DVB teletext as interchangeable codecs.
- Do not make an optional external FFmpeg library a startup requirement for
  users who have no teletext media until the packaging decision is accepted.
- Do not expose raw page bytes or undecoded character-set data through public
  APIs as a temporary contract.

## Phase 9A: Generic teletext engine

### P9A-01: Product and standards decision

Accept an ADR before schema, runtime-package, or pipeline work. It must decide:

- how required generic full-page browsing and authored-page delivery coexist
  with the retained source subtitle requirements under ADR 0120;
- page-number, magazine/service, subtitle flag, language, national-option/
  default-region, character-set, transparency, and duration semantics;
- whether source teletext is burned into video, extracted to a cached text/ASS
  derivative, carried or stream-copied in MPEG-TS, exposed as a browser sidecar,
  or rejected per output mode;
- how channel, selected-item, global language, forced/default, and accessibility
  preferences interact with multiple teletext pages;
- HLS behavior, because browser HLS clients do not gain teletext-page support
  merely because an MPEG-TS input contains teletext;
- the authenticated full-page browsing surface and distinct generic authored
  page model, with synthetic fixture proof before production content;
- the versioned host/add-on input and page-output contract, read-only programme
  context, page ownership, layout limits, compatibility and validation;
- refresh and atomic publication, restart, expiry/last-good behavior, resource
  bounds, and visible missing/disabled/incompatible/failing add-on outcomes;
- the six-RID libzvbi packaging, licensing, capability, and operator-runtime
  fallback contract; and
- health behavior for unsupported pages, missing decoder capability, malformed
  streams, and character-set failures.

Use ETS 300 706/EN 300 706 terminology where it applies and ground output
carriage decisions in MPEG-TS/client evidence. Record unsupported combinations
explicitly instead of allowing FFmpeg's automatic stream mapping to decide.

The ADR must fill an engine/source/output matrix using these independent axes:

| Axis | Cases requiring a disposition |
|---|---|
| Engine | Native FFmpeg; Next, including its Burn and Convert modes |
| Source | Reopenable local files; authenticated provider/finite remote sources; prepared add-on files; one-shot producers; live HTTP/RTSP |
| Output | Native MPEG-TS and HLS modes; Next HLS and hybrid MPEG-TS; browser/client delivery |
| Video processing | Transcode/burn-in; video-copy policy and its effect on selected captions |
| Teletext behavior | Subtitle pages; full-page browsing; authored services, with explicit supported delivery |

For each accepted combination, identify page selection, decoder/preparation
ownership, delivery, failure behavior and required runtime proof; explicitly
reject or mark inapplicable the remaining combinations. Determine whether the
pinned Next protocol can express the selection or needs an accepted adapter or
protocol change before promising support. Preserve [ADR 0100](../docs/adr/0100-next-alpha-embedded-text-subtitle-limits.md):
never silently discard an exact selection, switch engines, consume a one-shot
producer twice, or await whole-source extraction from an endless stream. Existing
alpha limitations are constraints to evaluate, not proof of teletext behavior.

### P9A-02: Runtime packaging and capability

Build or acquire a small redistributable teletext fixture before changing the
managed runtime. For every first-party RID:

1. establish whether libzvbi can be built, linked, licensed, and packaged;
2. verify `ffmpeg -decoders` exposes `libzvbi_teletextdec` and the accepted
   decoder options;
3. add a bounded functional decode probe rather than checking the name alone;
4. record operator-runtime capability without rejecting unrelated playback;
5. surface a missing required capability when a saved channel actually selects
   teletext; and
6. verify runtime archives and container images contain any required shared
   libraries or remain genuinely static.

Use the existing managed-runtime acquisition and capability-reporting paths,
recording exact artifact identity separately from application decode evidence.
UCU's scoped ARM/macOS and hardware acceptance exceptions do not waive Phase 9
requirements; any teletext platform exception needs its own accepted decision.

### P9A-03: Scan and persistence

Persist a typed teletext stream/page model rather than overloading the existing
text/image-subtitle heuristic. At minimum carry the ADR-approved stream index,
codec, service/page number, subtitle flag/type, language, region/character set,
forced/default disposition, and source provenance.

Bind page identity to the exact source revision, media version and stream. Pages
within one stream must remain independently selectable and must not collide in
cache identity. Preserve provider subtitle ownership under
[ADR 0101](../docs/adr/0101-provider-subtitle-stream-identity.md); a provider path
is metadata, not sufficient identity. Extend existing reconciliation and
preparation contracts to invalidate stale page selections and artifacts without
replacing unrelated tracks.

Scanner work covers local and supported remote providers, exact media-version
selection, rescan reconciliation, unavailable files, SQLite persistence, and
search/detail projections. Preserve alternate-audio and ordinary subtitle
behavior while adding teletext; a codec that cannot be decoded must remain
visible rather than disappear from selection.

### P9A-04: Engine delivery and independent acceptance

Thread exact stream/page selection through collection/schedule entries and the
native and Next adapters. Implement the P9A-01 matrix through the existing
application ports, selection rules and source owners:

- burn-in uses explicit decoder page/format/region options and preserves
  transparency, timing, seek/restart, scaling, and HDR/SDR behavior;
- extraction reuses bounded, source-bound preparation with page-aware cache keys,
  deadlines, cancellation and atomic publication; failure follows the accepted
  selection policy without admitting partial or stale artifacts;
- MPEG-TS passthrough or carriage is enabled only after PMT/PID and client proof;
- HLS/browser playback uses the accepted burn-in or sidecar fallback and never
  advertises native teletext controls that do not exist; and
- required full-page browsing and authored pages use a stable generic
  application/add-on boundary with typed page validation and host rendering.

Preserve subtitle timing through retained source intervals, mid-cue seeks,
programme handoffs and chunk restarts. Define video-copy behavior explicitly;
copying video must not accidentally promise burned-in captions or carry a
teletext stream solely through automatic mapping.

For the host-supplied add-on input, reuse the read-only programme occurrence
projection from [ADR 0112](../docs/adr/0112-programme-aware-graphics-context.md)
for current/next/continuing programmes and Later Today, including filler and
DST boundaries. Reuse the native preview boundary from
[ADR 0114](../docs/adr/0114-author-graphics-with-the-native-renderer.md) where
appropriate. A rendered page preview does not prove an encoded teletext service;
the ADR must still select that service's authoring and transport contract.

Before P9B-01 starts, demonstrate synthetic multi-page publication, navigation,
character/layout fidelity, update/restart and invalid-page/failure handling
through the accepted outputs. Prove the versioned add-on boundary with a test
fixture producer, not the production TV-guide implementation. Localized engine
management, health, supported-platform runtime proof, source-subtitle regressions
and applicable browser, smoke and TSDuck gates must pass at this checkpoint.

## Phase 9B: TV-guide add-on and integrated acceptance

### P9B-01: One tvguide teletext content add-on

After P9A-04 is Complete, implement one new add-on in ErsatzRS-Addons. Define
its exact package identity and NOS-inspired page map, then produce typed pages
through the engine's accepted contract. The add-on owns the index, channel and
programme page composition and navigation links. Consume the host's read-only
programme occurrence context rather than querying SQLite or duplicating
scheduling logic. Cover current/next/continuing and Later Today, filler,
empty guides, timezone/DST boundaries, long titles and multilingual text.

Verify explicit package installation and enablement, compatible host/add-on
versions, settings and bounded refresh. Show actual guide pages changing after
playout/guide changes without embedding their content rules in ErsatzRS. Prove
disabled/missing packages, incompatible output, timeouts and failed refresh
follow P9A-01's visible stale/last-good policy and preserve unrelated playback.
Record exact host and package revisions for integrated acceptance; publication
or automatic distribution is not implied by this implementation milestone.

### P9B-02: Admin, health, and end-to-end acceptance

Localize teletext type, page/service, language/region, selection, capability,
and error presentation in all supported cultures (currently nine). Extend the
existing troubleshooting and health surfaces for selected-but-undecodable streams,
invalid page metadata, missing runtime support, extraction/decode failures, and
unsupported output combinations.

Keep browser actions on the management surface, machine operations under the
existing API policy, and decoder/media callbacks on the internal listener under
[ADRs 0102](../docs/adr/0102-isolate-http-listener-route-surfaces.md) and
[0103](../docs/adr/0103-separate-machine-api-from-browser-actions.md). Any browsing
or sidecar endpoint needs an explicit route and access-policy classification;
do not expose management through the streaming-only listener.

Use licensed or redistributable fixtures for subtitle pages and full-page
services. Install the exact tvguide add-on through the existing package flow;
prove its guide-driven page content, navigation, refresh/restart, capability
and failure behavior through the actual engine and supported client outputs.
Engine-only or mock-provider evidence cannot close this milestone. Runtime
acceptance covers scan/rescan, exact selection,
seek and chunk restart, mixed subtitle types, direct MPEG-TS, HLS fallback,
HDHomeRun clients where applicable, malformed input, runtime refresh, and
decoded visual/text comparison. Include multiple pages in one stream,
multilingual character sets, preserved alternate audio, source replacement,
stale cache rejection, cancellation, edited timelines, and native/Next copy-mode
and unsupported-selection outcomes from the accepted matrix. Prove the served
output, not only serialized selection or successful process startup. TSDuck
must verify PSI/PID structure whenever the output carries a separate teletext
or DVB-subtitle stream.

## Delivery order

1. `p9a-01`: after UCU-09 closes, revalidate the source baseline and accept the
   expanded Level 2.5/interchange contract, product/runtime and engine/source/output matrix.
2. `p9a-02`: package and functionally probe the decoder.
3. `p9a-03`: persist source services/pages and expose exact metadata.
4. `p9a-04`: implement and accept the generic engine and selection/output matrix
   using synthetic pages, including channel profiles, shared admin/Lua controls,
   full Level 2.5/base fallback, QTeletextMaker TTI/sidecar roundtrip, localized
   management, health, independent decoder and suitable-client/runtime proof.
5. `p9b-01`: implement the single tvguide teletext add-on in ErsatzRS-Addons,
   including its NOS-inspired page structure, host-supplied guide content,
   named themes, permitted overrides and custom zones.
6. `p9b-02`: close integrated host/add-on management, health and end-to-end proof.

Phase 10 video-jukebox work remains dependency-gated behind `p9b-02`, including
the working content add-on. Official add-on distribution remains later work.

## Closeout gates

Each slice updates the owning parity debt, health contract, changelog, and
session log. Phase 9 closes only after:

- runtime packaging/probing is verified for all six RIDs or an ADR records an
  intentional supported-platform exception;
- SQLite scanner and persistence tests pass;
- P9A-04 independently proves the engine, channel/profile isolation, shared
  admin/Lua lifecycle and synthetic presentation before production content starts;
- the exact compatible tvguide add-on is installed and its real guide pages,
  navigation, refresh, restart and failure behavior pass integrated proof;
- FFmpeg argv snapshots and decoded subtitle/page fixtures pass;
- changed admin surfaces are localized and hydration/browser-proven;
- `./run.sh test-closeout`, the workspace Clippy configuration,
  `cargo leptos build`, and `cargo xtask parity-check` pass;
- `cargo xtask smoke` and the TSDuck playback gate cover every changed output
  mode; and
- `cargo xtask progress check` reports no tracker drift.

## References

- [FFmpeg codec documentation: libzvbi teletext](https://ffmpeg.org/ffmpeg-codecs.html#libzvbi_002dteletext)
- [FFmpeg supported subtitle formats](https://ffmpeg.org/general.html#Subtitle-Formats)
- [FFmpeg 9 performance and support progress](../docs/progress/parity/ffmpeg-9-performance-and-support.toml)
- [ADR 0003: Invoke FFmpeg through the CLI](../docs/adr/0003-ffmpeg-cli-spawn.md)
- [ADR 0052: Global subtitle language preferences](../docs/adr/0052-global-subtitle-language-preferences.md)
- [ADR 0056: Exact media-version and stream selections](../docs/adr/0056-exact-media-version-and-stream-selection.md)
