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
Native monochrome DRCS bodies preserve exact slots/pixels and sparse undefined
glyphs, with an explicit X28/3 mode declaration. Checked resource-header metadata
preserves all six lifecycle states, exact physical subcodes and kind-specific
packet bounds. Full MOT/DRCS writers derive actual last rows and emit bounded DVB
sequences, including a legal SPACE-only empty DRCS. Previous-content/update
consistency and service carousel ordering remain open. Nine opt-in MOT-based
libzvbi 0.2.42 cases now prove exact glyph pixels, native offscreen cell rendering
and independent invalid-glyph fallback. A bounded local X26 compiler now lowers
typed colour/attribute/glyph/DRCS commands without mutation or partial output;
twelve independent regressions cover fields, order, physical limits and located
errors. The nine native cases retain identical bytes through that compiler.
Bounded resource-object allocation now preserves full IDs and shared MOT defaults;
resource invocations check exact selection/type and retain immediate offsets.
Thirty-four focused regressions cover the local compiler, allocation and full
object-page paths.
Complete bounded POP/GPOP page writers now emit checked pointer tables,
definition markers, mandatory function metadata and actual last-row headers.
Original definitions are encoded once with independent lowering state; explicitly
declared empty resources remain legal without invented objects. Full capacity,
catalog/map matching and located colour/lowering checks precede body allocation.
Eight additional libzvbi 0.2.42 cases independently decode all public/global
active/adaptive/passive combinations, nested relative origins and exact glyph/
colour behavior, with unchanged base cells. This covers the first pointer bank,
not external decoding of every legal pointer or complete display semantics.
Local-object compilation now allocates exact marker positions and packs the main
program plus all owned definitions into one bounded X26 sequence. Forward and
nested references preserve full IDs/types; each definition has fresh state.
Eleven new byte-oracle regressions bring the focused compiler total to 45.
Actual .42 decoding leaves base text for all three correctly pointed local types;
the separate pinned fault diagnostic reproduces this with unchanged encoded units
and a separately exported correct conformance oracle. This is a decoder limitation,
not local rendering acceptance or an engine restriction. The same correct
three-case oracle now passes independent local decoding through pinned
teletextimager 0.2.0, with exact glyph/colour inheritance and unchanged base cells.
The opt-in adapter verifies imported source hashes and uses the decoder's own
packet reader; it adds no production dependency. This establishes representative
direct local decoding, not full feature or interactive-client acceptance. Three
additional cases now prove high local designations and nested full-ID local
origins with exact glyph/colour behavior. Private positioned base-glyph events
now retain checked operations and fallback cells for the future composer; original
legacy packet bytes, termination and error priority remain unchanged. Seven new
regressions, the existing wire snapshot and pinned runtime Unicode/subtitle/
national-subset proof pass. Charset/header-colour/default context and final merged
definition sizing remain required; events alone do not constitute composition.
Context-dependent row-zero positioning and base/enhancement composition remain
required work. The compiler now reuses exact-owner effective program selection
and a bounded original-colour prepass, preserving local empty overrides, sparse
MOT roots, main/Active priority and explicit-screen versus default-row fallback.
These prerequisites do not establish merged glyph/object semantics or readiness.
An ordered, bounded main-program intermediate now preserves original commands,
invocation anchors and source locations while merging compatible base glyphs
and restoring row-scoped repertoire. It retains original effective-program and
colour context. Final dual-receiver positioning, base/object priority, geometry,
default-resource realization and shared packet sizing remain required; the
intermediate does not authorize publication or redefine display semantics.
Reuse the existing complete-program collector for A.8(1): one exact display has
one counter per object type, shared across repeated/nested calls and all selected
default roots, with a maximum of 127. Explicit empty local data suppresses default
execution while preserving declarations. Any physical augmented program must
pass that same collection against its actual catalogue; direct lowerer counts
cannot replace it. Open/successive-object and word/resource admission remain
separate. Service admission remains the owner of the whole-display type limit.
Supplied-program spatial replay shares the existing two-pass walker without
rerunning Base composition. It retains physical Local main ordinals and full
body identities/instances; the service entry retains its original sparse default
tags. Keep original provenance in a separate mapping, and bind transmission
evidence to the actual final words. Complete reference/resource admission and
whole-page equivalence remain required; the replay trace grants no readiness.
Supplied-program DRCS replay now collects the actual main/catalog afresh under
the original service's selected primary repertoire and explicit collection
budget. It validates all DRCS dependencies, including references absent from the
glyph intent, and checks each retained DRCS operation against actual main/body
ordinals. Original display links and service bitmaps remain authoritative;
generated carrier IDs need not exist in the original catalog. Candidate/reference
preflight, source checks and bitmap lookup share the original resolver's paths.
This diagnostic does not certify placements, instances, complete object selection,
geometry or publication. Physical projection and whole-page equivalence remain
required before accepting generated carriers.
Physical-display replay now shares the complete post-spatial glyph, attribute,
charset, DRCS and geometry chain, plus display and colour resolution. It borrows
the original composition's Base/footprints/colour planes without inserting
fallback again, and retains its separate original target. Actual supplied main
ordinals have physical diagnostic labels; exact object IDs/body ordinals survive.
The caller must use the same immutable original service/composition. This is
analysis, not final-word ordering, packing, equivalence or readiness authority.
Before replay, resolve actual screen/row colours with the shared bounded prepass
and require them to match those used to prepare the retained Base/footprints.
Changed planes require a fresh design composition; they cannot silently reuse
the original planes or supply evidence of physical equivalence.
Default-root ownership still requires a proven physical strategy: an explicit
POP/GPOP invocation contains scope/type and allocated N/S1, not a resource-page
address. A selected-page mismatch must keep failing until materialization or
another proven encoding preserves the original MOT target; authorization alone
cannot fix receiver routing.
An actual-program packing diagnostic now binds replay and X26 packing within
one call, using unchanged selected resource links and the original header colour.
Its sparse C.2 evidence matches actual main/object sources directly to final
semantic words, including compatibility insertions. Repeated definition calls
retain distinct instances; unavailable resource words and cross-instance order
remain unknown. Consuming that evidence updates only actual attributes, display
and colour, leaving original targets and retained glyph/DRCS/geometry unchanged.
This binds final-word evidence; it does not prove full wire-page equivalence.
Physical raster diagnostics now consume only the immutable actual replay through
the same fragment, glyph-mask, DRCS and colour helpers as the original renderer.
The original design may lift authored Unicode at Base priority; physical replay
must render the transmitted Base fallback when its enhancement is absent. Do not
restore original replacements or target ownership into the physical projection.
Located unresolved cells and host budgets remain explicit. Raster diagnostics
alone do not establish authored identity, whole-page equivalence or readiness.
Whole-page diagnostic comparison now observes every visual cell, effective
origin/fragment geometry, palette and plane response, logical colour references,
full flash mode/rate and glyph effects through shared sided mask helpers. Test
both reveal states and flash phases over coverage actually present in each
fragment. Retain full pending histories separately from selected uncertainties.
Equal masks cannot certify authored identities or producer lineage; a generated
carrier must retain an exact original display-origin mapping before equivalence
can be accepted. Same-owner pairing alone cannot certify a service generation.
One-origin Passive recipes now derive typed glyph/context, effective attributes
and private command-role lineage from the exact borrowed display-origin, including
repaint origins. Allocate the lowest free full Local ID for the exact owner under
bounded scans. Check original owned-fragment C.1 dependencies before making
backgrounds explicit. Unproven national conversion, temporal phase and undefined
spacing remain located derivation gaps; they are not permanent authoring limits.
Recipes alone do not prove augmented source correspondence, unchanged-anchor/DAG
placement, whole-program admission, receiver equivalence or publication.
Carrier candidate placement now borrows the exact compatibility plan/catalog,
forms whole-overlap components and preserves DAG order. Derive unchanged main
gaps from root-only events in the actual composed program, including Base words;
child events are not caller anchors. Inverse selected panel geometry and checked
componentwise offsets choose the latest viable gap without changing original
commands. Unknown/cyclic/unrepresentable components remain pending, with original
global histories retained even without seeds. Generated order alone cannot
settle later original paints. Full augmented source/instance correspondence,
distinct generated IDs and whole-page typed/display equivalence remain required;
placement does not certify admission or readiness.
A same-service packing seam now uses original header colour and separately tracks
historical compatibility rows. Final main/definition sizing includes inserted body
restoration selectors; local invocation counts are independent of provisional
pointer high bits. Complex header cases still require instruction normalization,
not authoring exclusions. Selected native cells and literal wire/high-pointer
evidence remain distinct from full decoder/client acceptance (see the reference).
A domain-owned bounded base-cell trace is now retained by main composition from
the actual checked fallback rows. Coded cells, authored Unicode/mosaic intent and
spacing-attribute sources remain distinct, with separate raw and remapped colours
and double-height origins. This supports the shared semantic resolver; it does
not settle object priority, full footprints or browser rendering. Base-derived
Unicode must still retain base priority in final semantic and physical views.
A production-wire regression now confirms that generated Local G2 fallback can
obscure an original Active glyph, despite correct semantic Base priority. EN
300 706 section 13.2 makes packet reordering insufficient. Physical normalization
must preserve the Level 1.5 fallback and the original final fragment ownership,
repertoire, attributes and resource references. Its transient plan must retain
original provenance and unresolved geometry, including partially obscured enlarged
origins; it cannot silently mutate authored objects or claim publication readiness.
Actual lowering must separately prove precedence and charge generated objects,
invocations, open intervals, words and resources against existing limits. The
interchange reference records the unchanged positive oracle and observed failure.
A pure-domain original spatial trace now preserves exact command sources and
repeated invocation identities, origins, offsets and caller AP under two-pass
work/allocation limits. Main composition retains it from the same immutable
service, without treating generated base commands as original Local semantics.
Glyph priority and per-attribute arbitration remain separate required resolution;
logical traversal order is not physical packet transmission evidence.
Original glyph layers now retain all base and original enhancement contenders
with separate character ranks and invocation provenance. The diagnostic highest
character contender does not choose an attribute bundle or approve overlap,
panels, character-set execution, footprints or physical equivalence. Generated
compatibility words cannot acquire original Local provenance in this model.
Original attribute execution now keeps foreground, background, flash and compound
Display Attributes separate from character precedence. The same-service factory
supplies original row colours and checked spacing provenance. Active/Local
row-area scope, Adaptive per-type inheritance and fresh Passive invocation state
are retained independently; C2 ties requiring actual transmission order and
uncertain Adaptive endpoints remain typed pending diagnostics. Potential panel
areas use independent standard defaults. This is a bounded semantic intermediate,
not character-set execution, geometry/overlap acceptance, physical normalization
or publication authority; see the interchange reference and milestone evidence.
The same execution retains every original candidate's pre-geometry attribute
observation, including losing Passive private states, alongside fixed shared
stages before Adaptive and Passive application. Neither an observation nor a
flattened cell winner supplies final size/overlap semantics. C.3 resolution must
replay original layer/instance writers, spacing lower-row suppression, origin
fragments and deferred obscured attributes before rendering and publication.
The same composition now retains fixed coded-base fragments and the original
spacing Double Height cause for whole lower-main-row suppression. Suppressed
controls cannot cascade; background-only cells reference their upper origin,
separately from the ignored row's mapped default baseline. Illegal height rows
retain located pending boundaries rather than choosing a decoder-specific clip.
These base facts feed a shared layer-aware display resolver. Base/Active/Local,
per-instance Adaptive inheritance and private Passive overlays retain separate
execution semantics. Compact origin/writer pools preserve every original
candidate, requested/effective size and deferred obscured attribute source;
effective fragments alone define occupancy. Equal contending size values can
establish geometry while preserving unresolved writer provenance. A separate
bounded raw-position call/write scan rejects proven successive-opening overflow;
child writes can intervene, while plane writes and mixed-source reset ties retain
pending evidence without invented cells or ordering. Physical-order ties, illegal size
boundaries and ambiguous inherited Adaptive non-origin sizing remain located
pending evidence. The interchange reference owns the precise API and decoder
limitations. This does not accept glyph rasterization, full A.7/A.8 admission,
physical equivalence, suitable-client display, editor workflow or publication.
A consumed diagnostic raster now uses that exact unpublished composition's
original glyph identities, fragments, DRCS records and selected colour planes.
Pinned MIT font bitmap data supplies twelve-by-ten masks without an external
renderer or runtime font acquisition. Base Unicode replacements retain Base
priority; they never become Local contenders. Fixed/selected CLUTs and C.1
transparency are distinct from video alpha. Pixel and diagnostic budgets are host
policy; unresolved cells carry original locations instead of guessed glyphs.
Annex G.2 permits font/underline shape choices; D5 separates G1 mosaics and
underlines ordinary alphanumerics, with G3/DRCS unchanged as an explicit decoder
policy for their undefined response. Table 26 notes 11/12 restrict spacing mosaic
separation to Level 1/1.5 characters; enhanced G1 uses D5 alone. Explicit Table 29
D5=0 also cancels an
earlier Base spacing separation through the original writer's C.2/C.3 ordering,
scope and deferral. Defaults do not perform that cancellation. Later spacing
separation can restore it; spacing Contiguous Mosaics cannot clear D5=1. This
correction is required in the common model and raster before physical replay.
Receiver-owned header cells retain a distinct marker; exact identity belongs in
preview navigation, without invented header errors or a decoder-specific subpage
abbreviation. Invert exchanges references
before supplied flash phase and C.1; enlarged bottom fragments retain the exact
origin row plane. Eight actual-production opaque DRCS rectangles now match
independent libzvbi 0.2.42 bitmap and RGBA rendering, covering enlargement,
scope, invert and conceal/reveal. This is bounded representative pixel evidence,
not full client/transport acceptance. Known alphabetic composites now preserve
actual current G0/G2 masks, fitting above marks into the chosen twelve-by-ten
font while below marks retain full bases. This is explicit vertical font policy,
not lossless atlas reproduction; Arabic/unknown assignments stay pending.
Two definite Table 4 dependencies now retain background/consumer/invert source:
consumed window transparency from substituted BG and inverted Adjacent CLUT's
paired-BG parameter in both phases. Explicit backgrounds remain distinct; no
whole-service admission follows. Remaining Arabic/background parameter evidence
and complete client/editor/publication gates stay open. Borrowed
atlas/imager shape equality remains self-consistency only. The retained MIT
notice is included in the root distribution license. This adds no publication
permission, permanent feature restriction or milestone acceptance waiver.
G1 alphabetic positions now have a separate checked operation, preserving mode
0x01 rather than promoting them to a Level 1.5-compatible character command.
Original glyph/Passive attribute execution retains this intent; composition keeps
base Unicode fallback. Independent libzvbi 0.2.42 proves selected Latin positions
without national substitution. A separate teletextimager 0.2.0 diagnostic records
its empty-G1-font limitation while proving unchanged Level 1.5 fallback. Current
G0 selection, non-Latin alphabets and D5 underline still require full execution
and receiver proof; the interchange reference records the distinct evidence.
Original charset contexts are now consumed alongside the same-service Base,
spatial, glyph and attribute projections. Local/Active declarations share spatial
absolute-row state; Adaptive/Passive instances keep fresh private state across
rows. Every contender retains its own context, original writer provenance and
pending equal-position source order. Primary/secondary/header national metadata,
Base Unicode intent and actual coded role remain separate. This does not implement
ESC authoring, resolve full geometry or prove physical receiver equivalence;
the interchange reference defines the boundary. Full acceptance remains open.
The same immutable composition now retains finite glyph identities from the
original charset execution. All seven G0 tables, all four G2 tables, G1 mosaics
and alphabetic positions, G3 and raw DRCS references remain distinct. Table 32
selects the actual Table 36 national subset for Base glyphs; enhancement glyphs
never gain national substitution. Explicit prohibited positions and unmarked
empty cells remain different outcomes. Current-G0/current-G2 composite intent
retains unverified non-Latin composition, rather than substituting Latin marks.
The zero-diacritic 2/A exception is always @, even when charset order is pending.
This assignment boundary establishes no Unicode/font/raster, DRCS resolution,
geometry, preview or publication authority. Temporary missing evidence is not a
permanent engine restriction; full authored Level 2.5 acceptance remains open.
Original DRCS execution now resolves each explicit logical scope/S1/slot against
the same service's supplied display links and checked catalog, preserving exact
bitmap/resource identities, losing candidates and invocation provenance. Authored
references already contain selector choices; the existing lowerer emits private
Normal/Global selections with fresh definition state. The interchange reference
owns this boundary. Bitmap resolution cannot certify placement, size/colour,
overlap, receiver-equivalent rendering or preview/publication readiness.
Selected display geometry now maps original candidates through actual Level 2.5
panel widths while preserving raw addressing, metadata, losers and nonplacements.
Per-instance direct-address/write facts distinguish own area violations from legal
separate child placement. A.4 applies to known raw AP addresses, including position,
charset setup and inactive panels. A.7 checks those same directly addressed cells
with whole-instance attribute-type masks; enlarged footprints remain a separate
C.3 concern. Repeated and nested calls retain exact identities. Caller AP and
direct endpoints retain raw interval semantics. The same immutable composition
owns these inputs. Extend this shared
result with occupied footprints and geometry-aware attributes. A bounded raw
inclusive sweep rejects sixteen known simultaneous open intervals across all
object types, preserving unresolved intervals and original call diagnostics.
Known-overlap counts grant no full admission certificate; A.7/A.8 admission,
C.3, physical equivalence and complete renderer/preview/publication remain open.
Known same-emitter C.2 ties may use the actual final packed word positions. The
compiled factory must map them to original sources and executing instances and
rebuild the shared immutable attribute/display/colour result. Standalone analysis
and unsupported cross-instance comparisons stay pending; source ordinals, pointer
values and definition storage order do not certify transmission order.
A bounded compiled-page identity fence now rejects exact physical
display/resource header collisions and repeated logical resource owners, without
reducing display subcodes to S1 or banning distinct-S1 kinds. Eleven regressions
use actual empty DRCS/POP/MOT headers, counters/last rows and magazine-eight/BCD
identities. The eventual complete compiler must supply every unique page; this
primitive does not prove inventory completeness, lifecycle or publication.
X27 native decoding has a separately
reproduced version-specific gap; normative encoding is retained. Full external
feature decoding and actual client display/navigation remain open.
A bounded per-display MOT/X27 derivation now preserves selected context, exact
mask/provenance and original default sources. Audited conflicting declarations
remain explicit unresolved candidates. Effective default/presentation derivation,
resolved display semantics, physical allocation, complete
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
Retain each role's original MOT extent when selecting a real superseding X27
page/mask. For a non-supersedable same-page declaration, preserve the owner and
use the exact X27 mask rather than intersecting it with the MOT extent. Different
non-supersedable pages and null-over-associated-MOT combinations remain explicit
unresolved candidates until their policy is calibrated; do not erase either.
Cleared global associations preserve active default declarations with separate
diagnostics. No derivation without local-X26/X28/M29 presence may claim effective
default application or presentation. These intermediate diagnostics do not declare
legal authored inputs permanently unsupported.
The mixed-default/X27 nested-call policy still requires authored resolution proof.

Keep stable logical S1 separate from physical last-packet `nn` and modulo-eight
S2. Preserve all six Annex B lifecycle declarations without confusing C12/C13
with display national-option bits. Actual compiled content determines `nn`; X27
and X28 precede content and do not inflate it. Header metadata alone cannot
prove previous content/counter or fragment continuity. Keep service-wide C11
and opaque imported controls/text separately owned. Canonical writer suppression
flags are not import restrictions. Empty authored DRCS can use a SPACE-only row
with undefined modes without creating glyph definitions. Complete compilation
and scheduling must enforce resource update order, repetition/packet gaps and
closing headers before publication; full-page assembly is preparatory evidence.

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
