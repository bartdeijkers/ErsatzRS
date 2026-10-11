# Teletext design interchange

## Delivery state

Planned contract; not an operational import/export interface. Level 2.5 authored
support, durable designs/drafts, editor roundtrips and real-client acceptance are
open. Existing Western Latin Level 1/1.5 candidates are preparatory. No new CLI,
admin route, metadata schema implementation or editor binary is claimed here.

The domain now provides checked colour references/RGB, editable CLUT2/3 and
monochrome DRCS bitmap/slot/subtable values. They reject malformed construction
and JSON; they do not validate resource graphs, per-display usage or native
encoding. No current addon interface accepts these new values yet.
Checked raw resource identities and a DRCS catalog now validate definitions and
supplied display references. Ordered enhancement programs and checked object
closure now collect transitive DRCS and charset references from the supplied
effective page program. Resolved display semantics, complete service allocation
and enhanced carriage remain open. Presentation declarations and an explicitly
resolved semantic service now compose those values with the unchanged base set;
they are not operational v2 payloads or readiness/publication tokens.

## Resource identity and DRCS lookup

The implemented authored resource identity uses a raw magazine/page byte and
logical S1, not the display page number or a physical resource-header subcode:

```json
{"kind":"drcs","page":{"magazine":2,"page":160},"logical_subpage":15}
```

Magazines are 1–8; page bytes 0–254 and logical S1 values 0–15. Resource kinds
are `pop`, `gpop`, `drcs`, `gdrcs`, `mot` and `mip`. Identity objects reject unknown,
duplicate or missing fields and positional arrays. Kinds require strings.
Page function/coding is derived, not independently editable.

Annex A.10 reserves 1BE for ACI, 1F0 for Basic TOP, every magazine's FD for MIP,
FE for MOT and FF as a terminator. Object/DRCS resources cannot use those addresses;
other magazines' BE/F0 and decimal-looking resource addresses remain available.
MIP requires S1 zero (§11.3.2); nonzero MOT S1 remains valid because Annex A.1's
zero convention is advisory. Physical update and last-packet metadata remain
compiler-owned.

DRCS subtables preserve sparse, unique slots and monochrome bitmaps. The catalog
checks exact page/S1 collisions independently of kind and the host record budget.
Supplied per-display references resolve through normal/global links to exact
subtables/glyphs. §14.4.1 permits at most two selected subtables and 24 distinct
glyphs; repeats do not increase the count. Failures retain display/raw subcode
and reference ordinal. The enhancement collector can supply complete references
from an effective program and its invoked objects; full service/display allocation
and encoding remain separate checks.

## Enhancement programs and object references

`TeletextEnhancementProgram` preserves ordered commands: relative position,
screen/row colour, column operations and typed object invocation with a required
nullable origin modifier. Column operations represent colour, flash/rate,
non-spacing attributes, G0/G1/G2/G3, diacritics, charset designation and explicit
normal/global DRCS references. Constructors and JSON enforce local bounds; they
do not certify assigned glyphs or final display positions. In particular, relative
row zero is valid and does not imply a writable final header cell.

Page designations accept the 36 assigned table-32 codes; X/26 accepts the 46
table-34 codes. Raw aliases remain distinct. X/26 designations do not substitute
national characters. `secondary_from_code(127)` means no secondary G0; the primary
constructor rejects it. A secondary G0 does not designate G2 or add an enhancement
pair. Diacritic indices 0–15 are retained; composition requires the actual G0/G2
context. The mode-0x10/data-0x2a `@` case remains compiler/renderer work.

Object IDs are semantic host `u16` values, not physical pointer slots. Local
objects retain their exact display page/raw subcode; public/global objects require
POP/GPOP resources. `TeletextEnhancementObjectCatalog::new` accepts definitions
and trusted object/aggregate-command budgets. The catalog is not deserializable:
parse bounded definitions, then apply host budgets explicitly. Every declared
edge is checked, including unused definitions. Active may invoke adaptive/passive;
adaptive may invoke passive; passive may invoke none. Resource objects cannot
invoke local definitions, and local targets must retain their exact owner.

`collect_display_references` takes a checked effective program, display address,
primary repertoire pair and trusted command/output budgets. Each invocation is
visited again; repeats consume work and the 127-invocations-per-type limit. The
primary pair and transitive X/26 pairs may name at most two distinct repertoires;
aliases do not consume another pair. Collected glyph references preserve their
object/command source, and `validate_drcs` maps dependency failures back to it.
Budgets and pair/invocation counters are fresh for each display; failures expose
no partial collected result.

These checks do not select MOT/page defaults, implement local-X/26 default-object
suppression, resolve origins/side panels, compose glyphs, apply object inheritance
or validate spatial overlap/open-object limits. They neither allocate physical
resources nor establish an encoded or publishable generation. The two 15-object
spatial limits in annex A are distinct from the strictly descending invocation
type graph and remain compiler work.

## Local enhancement packet bodies

`compile_teletext_local_program` takes one exact display owner, a borrowed ordered
program and a trusted command budget. It checks work, lowering requirements,
active-position order and the 207-payload-triplet capacity before allocating any
packet bodies. Successful output is immutable: at most sixteen X26 bodies, each
with a Hamming-coded designation and thirteen Hamming 24/18 triplets. The payload
count excludes the mandatory local-only terminator; unused final triplets repeat
that terminator, including the last triplet. No DVB transport or publication
permission is attached to these bodies.

The directly lowered operations are screen/row colour, positions in rows 1–24,
all typed column attributes/repertoires/diacritics and explicit normal/global
DRCS invocations. Row-zero colour uses Address Display Row 0, which also sets
both active coordinates to zero. Row 24 uses address 40. Screen colour uses the
Annex A.5 compatibility recommendation of address 41. The normative initial
active position is zero/zero; commands changing it must remain in screen order.
Equal-position operations retain authored order; backwards moves are rejected
at their original command. Normal and global DRCS selections use independent
local latches; even the first selection of subtable zero is explicit. Usage bits
select Level 2.5 and glyph slots retain their complete checked values.

Explicit position-to-row-zero lowering still needs resolved row colour, and
object invocation still needs checked physical pointer allocation. These produce
located `NeedsRowZeroLowering`/`NeedsObjectAllocation` diagnostics, never invented
triplets or truncated host IDs. They are outstanding required compiler steps,
not permanent feature restrictions. This primitive also does not certify glyph
assignment, header writability, repertoire limits, underlying spacing attributes,
object geometry/inheritance, side panels or effective default resolution.

The base encoder derives its own Level 1.5 glyph enhancements. A complete
service compiler must compose those with authored enhancements without duplicate
designations, backwards positions, glyph loss or altered fallback. Standalone
bodies are not safe to append blindly to existing encoded pages. Synthetic native
resource calibration replaces the known empty base X26 body explicitly; it does
not establish general base/enhancement composition or complete Level 2.5 readiness.

### Positioned base glyphs

The private base layout now retains a single immutable stream of positioned
Western Latin glyph replacements alongside its unchanged fallback rows. Each
event carries its exact checked row/column and a closed G2 or G0-with-diacritic
operation. Diacritic zero retains ambiguous national-subset positions, including
the special `*` code for authored `@`. Direct national characters and mosaics
do not invent replacement events. No Unicode normalization or cell shifting occurs.

The legacy packet builder derives its original words from that stream: one
selector per populated row, unchanged row-zero/row-24 coding and unchanged
data-zero termination/padding. Capacity still counts selectors plus glyphs and
reserves termination; aggregate rejection remains after located character/control
failures. Seven new tests cover literal word vectors, exact fallback cells,
207/208 payload boundaries and preserved diagnostics. The existing complete
wire snapshot and actual pinned FFmpeg Unicode/subtitle/national-subset proof pass.

These positioned events are a composition prerequisite, not a second independent
glyph source or a ready enhanced page. The composer must preserve authored order,
AP and invocation origins, explicit offsets and DRCS latches; restore charset
context with row-end resets; include injected Latin pairs/work in existing budgets;
and resolve header-row colour before neutral positioning. Compatible authored
glyph overrides should replace redundant derived Level 1.5 glyph triplets, while
Level 2.5-only replacements retain their compatible fallback. Generating X26 must
also realize original default-object behavior when the local program was absent;
`Some(empty)` continues suppressing defaults. Definition allocation follows final
merged sizing. Blind prefixing or copying the legacy black header selector is unsafe.

Annex A.3 requires a separate compatibility-row tracker: row-address words for
DRCS selection or object offsets can change a Level 1.5 receiver's row while the
Level 2.5 AP remains unchanged. Restore compatible row context without a backward
screen address. Preserve original display priority when derived local glyphs
overlap Active objects; inserting a local glyph must not silently promote the
underlying base character above an authored replacement. Realized default roots
must retain their original MOT owners, with declaration-to-selection consistency
before using the selected-resource lowerer. These remain composer obligations.

### Ordered main-program intermediate

`compose_teletext_main_program` combines the exact owner's original effective
program with positioned Western Latin replacements. Original commands retain
their values and order. A stable merge flushes earlier base cells before an
authored AP advance without moving an object invocation's original anchor.
Authored G2, G0-with-diacritic and Level-1.5 G3 replacements suppress the derived
replacement at that same cell; duplicate authored compatible replacements fail.
Level-2.5-only G0, G1, G3 and DRCS glyphs retain their compatible base replacement
before the authored glyph. Temporary Latin designations restore the original
same-row designation, with the primary pair reselected on actual row changes.

Trusted raw-input and exact-output budgets precede indexing and output allocation;
a counting pass and an identical emitting pass retain exact command cardinality.
The existing expanded collector checks injected pairs, repeated object work and
located references. Each command has original-page, original-default-slot or
base-cell provenance; nested errors retain full object identity and body ordinal.
An immutable original colour context accompanies the plan. Explicit empty local
programs and absent programs retain their different default-selection behavior.

This is an unpublished lowering intermediate, not a validated final display or
packet sequence. It does not settle dual-receiver row restoration, header wire
positioning, Active/base overlap, non-Latin fallback reinterpretation, glyph
footprints, object attribute inheritance or side panels. Final realization must
retain original default-resource ownership, resolve those semantics and size the
shared main/definition sequence before pointer allocation. The enhanced layout
defers only the legacy standalone X26 capacity check; representability and
fallback errors remain checked. No successful plan grants publication authority.

Twelve focused merge tests cover exact original commands, offsets/anchors,
compatible suppression versus enhanced fallback, G1 precedence, charset aliases
and row resets, injected third-pair rejection, sparse original-default ownership,
explicit empty overrides, header colour context and budget/error provenance.
An oversized semantic plan is accepted here but rejected by the physical packer
at 209 triplets against its 208-slot limit. This proves separation of gates.

The separate pinned composition oracle independently decodes four actual
production-composed/lowered cases at Levels 1.5 and 2.5: derived pound/acute,
authored compatible replacements, G0/G1 with retained fallback and an anchored
full-ID Passive invocation whose definition marker moves to five after insertion.
Synthetic pages explicitly select German national option one, so pound requires
an enhancement rather than occupying an English national-subset base cell.
Exact code/repertoire/diacritic/foreground/background tuples and unchanged
adjacent cells are checked. All original base X26 bodies are replaced once;
other original header/presentation/fallback bytes remain unchanged.

The four-case fixture SHA256 is
`bf4584486120137270f3a5ed62a7830ee5b1af81923445c0b87013b8ae91c603`;
the separate helper SHA256 is
`e1cb2ad2b16e78a0f56baaae14434aa6d78f146d7e0173c7ade9f19db97228eb`.
It reuses the unchanged teletextimager 0.2.0 loader at
`2b0125b778c45ffdfa2df0b196870c093361f317`, with eight verified upstream
source hashes, bounded input and the existing deadline/output-bounded child
runner. Wrong goldens, unknown members and duplicate cells fail. Existing frozen
fixtures/helpers remain unchanged. The
[export/replay workflow](../../scripts/e2e/README.md#teletext-ordered-main-composition-decoder-calibration)
uses the actual producer path rather than constructing expected bytes separately.
This is selected independent cell decoding, not pixels, transport or client
navigation. Inline non-Latin repertoire restoration remains unproven by this
decoder; its same-cell designation handling selects the restored repertoire for
an earlier glyph. That observation neither changes the required engine scope nor
establishes correctness of the final wire compiler.

### Unresolved Unicode/Active wire priority

The opt-in `base_unicode_active_override_decodes_with_pinned_imager` regression
uses the actual `compile_teletext_composed_main_program` output, including final
local definitions and retained DVB units. German national option one forces the
authored pound at page 200/subcode zero, row 1, column 8 into derived G2 fallback.
An original full-ID `ffff` Active object writes G0 `A` there; adjacent `Z` must
remain unchanged. The correct tuples are pound/G2 at Level 1.5 and `A`/G0 at
Level 2.5, both white on black. The expected tuples are independent literals.

Pinned teletextimager 0.2.0 at `2b0125b778c45ffdfa2df0b196870c093361f317`
passes the fallback check but reports Level 2.5 `[35,7,0,7,0]` instead of
`[65,0,0,7,0]`. Nextest `4a620ac7-13ef-469e-ae09-ee707435676c` fails; it is
defect evidence, not successful decoder acceptance. The production request SHA256
is `6ad7b72e9d33b3d339fb7f4b61bb3c0b708fd805b1f9f68f7324425048a83d7b`.
The existing main-composition decoder helper and earlier positive oracles are
unchanged. Export or replay the regression with:

```bash
ERSATZRS_TELETEXT_TEST_IMAGER_DIR=<pinned-checkout> \
ERSATZRS_TELETEXT_TEST_UNICODE_ACTIVE_IMAGER_FIXTURE=<request.json> \
./run.sh test -p ersatzrs-ffmpeg --lib --run-ignored ignored-only \
  -E 'test(base_unicode_active_override_decodes_with_pinned_imager)'
```

[EN 300 706 section 13.2](https://www.etsi.org/deliver/etsi_en/300700_300799/300706/01.02.01_60/en_300706v010201p.pdf#page=92)
places Local glyphs above Active glyphs. Reordering the same generated Local
fallback therefore cannot fix the priority. Physical realization must preserve
fallback and original final fragments, without promoting a semantic Base glyph
or editing the authored catalogue. A transient bounded plan retains affected
origin footprints, final owners and original source identities. It is not a wire
compiler or readiness certificate: unresolved geometry/ownership and whole-glyph
precedence cycles need explicit evidence; actual generated calls, open intervals,
words, pointers, resources and two receiver views still require validation.

`plan_teletext_base_compatibility_realization` borrows the exact immutable
`TeletextMainComposition`. Its host budget bounds command/candidate/origin input
and seed/edge/update work. Seeds use actually generated BaseCell glyph provenance,
not retained replacements already suppressed by an authored compatible glyph.
Known final Active owners initiate closure over their whole effective rectangles;
each covered final owner contributes its whole rectangle in turn. Final fragments,
origin and candidate identities remain associated with the original composition,
including its charset, glyph, DRCS, attribute and colour evidence. Precedence
constraints retain overlapping whole-origin dependencies; cycles remain pending.
Original display pending survives even with no seeds: an unpainted unresolved
Active origin can otherwise leave Base as the apparent final owner. The plan
neither generates synthetic objects nor establishes their placement, historical
paint order, budgets or wire equivalence. Those are the next lowering gates.
Nine planning tests cover exact original sources, a double-height bottom seed,
higher partial ownership, closure extending outside the initial origin, selected
and disabled left panels, unresolved unpainted Active content, noncollision,
budget cutoffs and cycle-blocked graph nodes. They and the existing semantic
Unicode/Active raster test pass in `9c2240d4-3d42-412e-be0a-117d9ef0e761`;
all nine affected tests pass after equivalent lint corrections in
`c9ec93bc-985b-4ab7-b82d-5a2aa6c3524a`. Strict FFmpeg all-target Clippy and
formatting pass. This does not turn the failing independent wire oracle green.

Physical replay must reuse the existing `collect_display_references` on its
actual augmented program/catalog before returning bytes. A.8(1)'s 127 limit is
already counted per type over one entire exact display, including side panels,
every repeated/nested call and all selected sparse default roots. The service
constructor selects one effective program per page/subcode; an explicit empty
local program suppresses defaults without deleting their declarations. The
composed-main factory also collects its complete derived program. A second
geometric counter is unnecessary; raw lowerer direct-call counts alone are
insufficient for any future generated calls. This fact does not settle A.8
open/successive limits, word/resource budgets or physical equivalence.

Three service-boundary regressions prove combined sparse defaults, nested calls
across independent POP/GPOP roots and independent counters for two exact
subcodes, with located constructor rejection at 128. Existing collector tests
already cover each type's 127/128 boundary and repeated nested traversal. Reuse
those tests; checked service construction supplies the located normative rejection
before geometry. P9A-01 owns current verification.

`resolve_teletext_spatial_program` supplies the first physical-replay seam: it
traces one exact host-supplied program against its checked catalogue using the
same counting/emitting walker as `resolve_teletext_spatial_trace`. No layout or
Base-compatible glyph merge runs here. Main/root command ordinals refer to the
supplied Local program, including added Base replacements and carrier calls;
full object identities/body ordinals and actual instances remain exact. The
existing service entry still uses its original effective selection and sparse
DefaultSlot tags. Original semantic provenance belongs in a separate mapping,
never in place of physical command ordinals.

The caller still owns complete-program reference collection under the original
primary repertoire, resource/DRCS dependency checks and original default-owner
authorization. This trace is a bounded diagnostic, not accepted service data,
full A.8 admission, transmission evidence or publication readiness. The next
step must replay glyph/attribute/charset/DRCS/geometry/display/colour resolution
from this actual trace, bind evidence to final packed words and compare the whole
page with the immutable original through the generated-source mapping. Reusing
ordinary main composition would add Base fallback again and is inappropriate.

`resolve_teletext_drcs_program` adds resource replay for an actual augmented
program/catalog. Raw candidate/reference budgets precede fresh complete-program
collection under the original selected primary; an explicit collection budget
bounds that traversal. Source checks retain exact main/body ordinals and reject
stale operations. All collected DRCS dependencies are validated, including a
reference omitted from the supplied intent. Original service display links,
resource identities and bitmaps supply both Normal and Global glyphs; newly
generated carrier objects are checked in the actual catalog instead of being
required in the original catalog. The original service resolver retains its
behavior through shared preflight/source/lookup helpers.

Typed errors preserve collection, dependency and original execution failures
with page and available command locations. Missing exact subcodes, missing
targets/bitmaps, changed operations and exhausted budgets fail without mutation
or partial output. This is diagnostic resource resolution: matching operations
and IDs do not certify intent generation, positions or instances. Full spatial
correspondence, object/default-owner authorization, geometry, physical evidence
and whole-page equivalence remain separate required gates.

`replay_teletext_physical_display` consumes a composition, its same immutable
original service and the actual supplied program/catalog under explicit
collection bounds. It shares the original post-spatial glyph/attribute/charset/
intent/DRCS/geometry execution and display/colour resolvers. Base cells, spacing
footprints and colour planes are borrowed from the original composition; layout
and Base-compatible insertion never run again. The owned diagnostic exposes
each actual projection while leaving the original target untouched.
An actual-program colour prepass validates the screen/row plane invariant before
spatial projection. The retained Base and spacing footprints were prepared with
the original planes, so changed effective planes are rejected. Original service
presentation defaults remain authoritative, while direct Active bodies are read
from the actual catalog. This is a replay boundary; authored colour changes use
a freshly composed original design.

Failures distinguish physical `Main` ordinals from full object identity/body
ordinals. Source/attribute/geometry pending remains visible. The original and
physical projections may differ: the unnormalized Unicode/Active case still
has that exact discrepancy. Matching page identities do not certify the
service/composition pairing. Final packed-word evidence, whole-page comparison,
selected links/default-owner realization, raster and publication remain open.

Do not bypass selected-resource checks merely to pack synthesized MOT roots.
The explicit invocation word has scope/type and N/S1, without a resource-page
address. A default owned by a different original POP/GPOP page therefore needs
proven materialization or another correct wire strategy. The existing composed
factory rejection protects against calling the selected page's wrong object.

`compile_teletext_physical_display_program` binds actual replay and main/local
packing in one host call. It first enforces the colour-plane and whole-program
DRCS checks, then packs that exact program/catalog with original selected links,
resource allocations and header-row colour. It does not compose another main or
relax resource-page selection. The immutable result exposes separate physical
replay and encoded main/local sequence; the caller keeps its original target.

Sparse transmission evidence uses the packer's actual source ordinals/full
object identities and final semantic-word positions. Compatibility insertions
are included in those positions. An original-provenance mapping cannot replace
this physical matching. Domain evidence preserves exact executing instances;
repeated calls do not establish cross-instance chronology. Resource bodies
without emitted word positions remain unknown. Actual attributes/display/colour
are rebuilt from consumed evidence through the shared resolvers; other physical
projections and the original target remain unchanged. Packing/word fit and these
C.2 facts do not establish whole-page equivalence, suitable-client rendering or
publication readiness.

`render_teletext_physical_display` takes one immutable physical replay, pixel
options and a raster budget. Its actual display, glyph intent, DRCS and colours
share the original renderer's fragment/mask/pixel and diagnostic paths. It does
not borrow authored replacement lookup: absent X26 restoration renders raw Base
SPACE for `£` and ASCII `e` for `é`, whereas the original design renderer retains
its authored replacement. Present enhancement identities render normally. The
known original Active `A` versus physical Local `£` discrepancy stays visible.
Diagnostics retain actual source ordinals, object identities and instances;
pixel/diagnostic limits reject without returning partial output. This host
diagnostic is not an independent receiver, a glyph-lineage comparison, whole-page
equivalence or publication authority.

`compare_teletext_physical_display` borrows the original composition and actual
replay under aggregate record, cell, diagnostic and work budgets. It scans the
entire 25-row visual plane, including cells outside a prospective carrier's
footprint. Findings retain each side's own source ordinals, full object identities,
origin/candidate indices and invocation IDs; those ID spaces are not equated.
The comparison observes effective fragments, origin anchors/sizes, logical
foreground/background references, all palette values, composed screen/row planes,
full flash mode/rate, boxing/conceal/invert, canonical glyph masks and effects.
Base/enhanced G1 use their respective separation rules; undefined G3/DRCS effects
retain exact D5 and spacing flags. C.1 observes both reveal states and flash phases
only over ink/background coverage present in each actual fragment, preserving
consumed transparency/adjacent-palette diagnostics without manufacturing pixels.

Pending display histories remain accessible alongside per-origin, charset/glyph
and selected-cell uncertainty. Superseded history is not a readiness predicate.
Zero findings cannot establish authored glyph identity, source lineage, service
generation, independent receiver equivalence, admission or publication: equal font
masks may represent different coded identities. Plane getters expose RGB/video,
while the replay factory guards its original raw plane context. Generated carriers
still need producer-owned exact origin/source mappings and whole-page validation.
Carrier derivation must also preserve coded identity rather than copy a Base
national-subset byte into enhancement G0, and establish temporal context for
incremental/decremental flash. The current model retains raw flash rate; copying
it to a fresh one-glyph Passive object does not yet prove equivalent timing.
Validate original colour-writer dependencies before materializing explicit carrier
backgrounds. These are remaining derivation/verification tasks, not accepted
authoring restrictions.

`build_teletext_origin_carrier` borrows a composition, exact display-origin index
and allocation catalog. It returns one private Passive recipe with that original
origin/candidate/source/root/instance and aligned body-command roles; callers
cannot supply its lineage map. Relative column zero awaits separately proven main
anchor/offset placement. Candidate/origin/catalog/ID-scan/output bounds precede
allocation; deterministic lowest-free Local IDs use the full `u16` namespace for
the exact owner, including subcode, with no reserved range.

The recipe materializes known pre-invert foreground/background, complete flash
mode/rate, effective size axes, boxing/conceal/invert and correct D5. Typed enhanced
glyph operations preserve resolved charset or complete DRCS context. Fixed glyphs
do not invent charset dependencies; special `@` remains independent of pending
G0 selection. Checked authored Base replacements use Latin enhancement context;
national-subset conversion remains an explicit gap. Base G1 materializes D5 OR
spacing while enhanced G1 uses D5 only. Unproven undefined G3/DRCS spacing and
incremental/decremental timing yield located gaps. Original owned fragments use
shared masks/C.3 coordinates and all actual pixels in both reveal/flash states to
validate C.1 before explicit background replacement. Uncertainty is never replaced
by guessed glyphs, colours or geometry.

The representative derived Active-`A` recipe at the retained `£` collision has
actual factory packing, an actual replay anchor and complete selected-model
comparison. Pinned teletextimager checks unchanged literal Level 1.5 `£` and
Level 2.5 `A`, adjacent `Z` and calibration `B`. This representative component
now consumes its derived unchanged-main gap; it is not automatic normalization,
a global lineage proof, full receiver
equivalence, display/navigation or publication acceptance. The original automatic
composition path's correct Unicode/Active conformance oracle remains failing.

`plan_teletext_origin_carrier_placement` privately borrows the exact compatibility
plan and supplied catalog. Connected whole rectangles and undirected precedence
edges form components; deterministic topological order remains distinct from
cyclic/pending membership. Trace the actual composed program, including generated
Base words, through the shared spatial executor. Only root main events provide
gaps, using their unchanged absolute AP after the exact command; recursive child
events never become main insertion positions. Include the initial `(0,0)` gap.

Map known visual origins back to raw main/right/left coordinates using selected
Level 2.5 panel widths and preserve receiver-owned header cells. Choose the latest
unchanged gap whose AP is componentwise no greater than every raw origin, with
nonnegative checked row `0..23`/column `0..71` offsets. Calls within one component
remain contiguous in its DAG order. Unknown rectangles, original selected
pending, cycles and missing representable gaps stay typed pending; even zero
components retain global original histories. Raw counts/storage and explicit
placement scans/graph work are bounded before allocation; expanded tracing has
its own budgets. Spatial failures retain their actual composed-main namespace.

`build_teletext_augmented_origin_program` borrows that exact placement and owns a
private checked candidate. Retain all original commands and object bodies; insert
component calls contiguously in DAG order. Two recipe passes obtain exact body
sizes before copying originals and then allocate distinct lowest-free Local IDs
against the growing catalog. Both passes charge catalog visits and ID probes;
assembly has explicit command, carrier, storage and scan budgets. Reject pending
components with their blocking origin rather than the first component member.

Private emitter maps distinguish retained main provenance/object body ordinals
from generated calls/body roles. They retain original origin/source/root/instance
facts separately and cannot certify actual invocation ancestry or a service
generation. The pinned representative fixture now consumes this producer through
actual allocation, encoding and replay; its complete 1,000-cell selected-model
comparison and literal dual-level decoding still pass. Later retained Passive
paints can redraw fragments.

`compile_teletext_augmented_origin_program` calls the unchanged physical factory
with that private candidate and owns the encoded replay beside bounded ancestry
maps. It accepts no detached trace or source map. Retained roots match original
main commands or exact sparse default slots; nested calls match the already-mapped
original parent and full body identity/ordinal. Check target/type/origin/caller
and absent versus explicit offset, plus actual parent/root/source context. Each
original invocation and generated carrier must occur exactly once. Generated
carriers must be root Passive calls at their exact private gap/raw origin/offset;
generated descendants have no permitted mapping. Separate actual-instance facts
from the carrier's original display-origin facts; never infer a numeric ID shift.

Raw original/carrier fences precede physical compilation, whose replay/packing
retain their own budgets. Actual count, checked storage/work arithmetic and minimum
unavoidable visits are fenced before ancestry allocation; detailed scans charge
visited work. Eight regressions cover repeated three-deep calls, sparse slot3,
offsets, empty/absent programs, no-seed pending, altered geometry and budgets.
The same pinned native fixture now consumes this paired factory and proves its
separate retained/carrier mappings before the unchanged model/literal oracles.
Matching service/composition generations remain a caller invariant. Ancestry is
not typed glyph/display equivalence, receiver acceptance or admission. Next check
resolved typed identities against exact original display origins; automatic
normalization, physical admission and full client navigation remain open.

### Shared base display trace

Main composition retains an immutable, domain-owned base display trace compiled
from the same checked fallback rows as packet encoding. Its fixed 25-by-40 cells
keep coded data, spacing controls and blank cells distinct from authored Unicode
or mosaic intent. The first eight header cells remain host-owned. An authored
Unicode replacement retains base provenance; retaining this trace does not make
its current physical X26 insertion correct when an Active object replaces it.

The trace records the spacing controls emitted by the existing layout compiler,
including Set-At versus Set-After attribute sources, row resets and paired box
entry. Raw Level 1 colour indices remain separate from Level 2.5 remapped colour
references. New Background selects the current raw foreground entry in the
background CLUT, rather than copying its remapped foreground reference. Implicit
and Black Background substitution use the original effective row colour.
Double-height origin information is retained for subsequent footprint resolution.

This is an internal composition intermediate, without serialized authoring or
publication authority. It is not a decoded enhanced page or a browser renderer.
Complete object inheritance and attribute priority, lower-row suppression,
panels, glyph rendering and equivalent Level 1.5/2.5 physical realization remain
required. Controls not emitted by the checked layout are not silently interpreted
as supported authored input. The final shared renderer must consume the resolved
enhanced and fallback views of the same immutable validated service.

Four production-encoded synthetic cases pass guarded teletextimager 0.2.0 at
Levels 1.5 and 2.5, checking literal code/repertoire/diacritic/colour tuples for
control spaces, row reset, mosaics, asymmetric remapping and existing Unicode
replacements. The separate fixture SHA256 is
`2100d9bd163221cfa57617a5936ae780d6b174a895b48508c70bf536666e5d30`.
The frozen helper remains
`e1cb2ad2b16e78a0f56baaae14434aa6d78f146d7e0173c7ade9f19db97228eb`;
its decoder commit and source guards are unchanged. The earlier four-case and
two-case composition fixtures retain their hashes below. A deliberately wrong
Level 2.5 background is rejected. Internal trace regressions separately prove
source integrity, paired-box confirmation, exact-subcode context rejection and
same-service retention; these are not independent pixel or object-priority proof.
Use the [base display calibration](../../scripts/e2e/README.md#teletext-base-display-calibration)
to replay the cell evidence.

### Original object coordinates

`resolve_teletext_spatial_trace` produces an immutable domain-owned trace from
one exact owner's original effective program and checked object catalog. It
retains original commands and full source identities, sparse default slots,
distinct repeated invocation IDs, parent relationships and immediate offsets.
Each object starts at relative zero; nested origins add the caller's absolute
position and immediate offset. Returning from a child preserves the caller's AP.
Screen colour leaves AP unchanged; row colour selects its original row/column
zero. Relative address order is checked independently for every program type.

Counting precedes exact event/invocation allocation under separate trusted
limits. Both passes use the same immutable selection and catalog. Located errors
retain the executing instance and root source, including a second invocation of
the same full object ID. Coordinates use checked wider arithmetic without
clipping or wrapping. Out-of-area coordinates remain unresolved input for the
subsequent geometry validator, rather than becoming accepted display cells.

Main composition retains this trace from the same service as its checked base
display, independently of generated base replacement commands. It reuses its
existing expanded-command limit for both counters and preserves earlier
reference-collection error precedence. Logical event preorder is not physical
packet transmission order. Separate glyph priority, per-attribute inheritance
and arbitration, footprints, panel areas, header normalization and equivalent
receiver views remain required. The trace has no serialized authoring, rendering
or publication authority.

### Original glyph layers

`build_teletext_glyph_layers` combines a checked base display and original
spatial trace without interpreting generated compatibility instructions as
original Local content. The main composition factory supplies both inputs from
one immutable service. The standalone function checks exact page/subcode
identity; equal owners alone do not certify generation identity.

Every non-header-owned base cell remains a candidate, including coded blanks
and spacing controls. Its original authored Unicode or mosaic intent and base
attributes stay separate from the fallback byte. Original glyph commands retain
their raw typed operation, full source, root source, executing invocation and
event ordinal. Colour, designation, position and call commands add no glyphs.
The character ranks are Base, Active, Local, Adaptive and Passive, in ascending
order (EN 300 706 clause 13.2). Among allowed same-type object overlaps, the later
invocation takes precedence (Annex A.7); raw glyph intent still needs repertoire
execution and glyph lookup.

Counting all enhancement candidates precedes allocation under a trusted limit.
One global candidate vector and sparse spans retain losing candidates as well
as the highest ranked contender at each exact coordinate. Sorting is by logical
position, character rank, invocation and original event order; it is not packet
transmission order. Wider coordinates, broadcast-owned header positions and
potential panel positions remain visible. Logical addressability is only a
coordinate classification, not acceptance of enabled widths, object areas or
footprints. No candidate is silently clipped or discarded.

`highest_priority_candidate` is a diagnostic character comparison, not a final
renderable cell. It does not approve prohibited Adaptive overlap or overlapping
Active attribute types, choose attributes, execute character-set lifetimes,
resolve DRCS pixels or apply size occlusion. Base attributes retained in a
candidate remain base attributes; the winning character does not select an
attribute bundle. Level 1.5 interpretation and physical equivalence still need
separate proof. Full per-attribute inheritance/arbitration, geometry and both
receiver views remain required before encoding/publication acceptance.

### G1 alphabet positions and independent decoding

EN 300 706 tables 29 and 47 distinguish G1 alphabetic positions `0x40..0x5F`
from six-bit mosaics. `TeletextG1AlphaCode` checks that complete assigned range;
`G1Alpha` preserves the original mode `0x01`. It is ignored at Level 1.5, uses
the current G0 without national substitution at Level 2.5 and uses D5 for
underline. It must not become a compatible G0/diacritic operation. Existing
`G1 { mosaic }` payloads and their six-bit meaning remain unchanged.

Glyph layers and Passive attribute execution retain the original alphabet
contender, source and invocation. Main composition keeps the existing Base
Unicode replacement before this enhanced-only glyph. These primitives do not
resolve current G0/G2 lifetimes, rasterize non-Latin alphabets or prove D5
underline. Those requirements remain open without restricting the engine target.

The production-composed fixture uses a German national subset, Base `é£Z` and
original G1 `0x40`/`0x5F`. The unchanged Linux x64 libzvbi 0.2.42 object oracle
returns exact Latin `@`/`_`, their colours and unchanged `Z`. Its public ABI and
version guards remain in effect; this is selected-cell decoding, not client
navigation, pixels, non-Latin repertoire or underline acceptance.

Pinned teletextimager 0.2.0 routes mode `0x01` to font 24/25 even at alphabetic
positions; both G1 font files have zero glyph metrics throughout `0x40..0x5F`.
Its renderer therefore cannot display these alphabet characters. The original
positive oracle failed (Nextest `54866d6e-6df0-42be-992d-78c28f73b090`); accepting
font 24 as an equivalent alphabet would hide that defect. The separate named
limitation test exports the unchanged conformant expectations, then reproduces
exact observed font-24 cells with **identical production units**. Level 1.5 still
checks Base `é£Z`. That diagnostic pass proves no Level 2.5 alphabet rendering.

Native replay passes five tests (Nextest
`b080b056-cccc-4fad-8a1e-a478583f49bb`): the new libzvbi positive case, the named
imager limitation diagnostic and all three existing base/composition tests.
Their original fixture hashes remain unchanged. New request SHA256 values are:

| Request | SHA256 |
| --- | --- |
| Correct imager conformance oracle | `9c45fb44798ae6c1331e53f2b496ac03fe4bdd3be7322f86a918eb1ed0f147e1` |
| Imager limitation diagnostic | `6d900b0f584628a8d4948e9c2f1a13e7b5742f7d7cab66cf1e5fc946b9cb76f9` |
| libzvbi positive oracle | `d4fd38a296113767068525a162678d5bd2e515d8f044d76a30b4ac73f6d3843d` |

Use the existing pinned imager checkout and a chosen temporary evidence directory
with the existing composed-page replay command, adding these optional exports:

```sh
export ERSATZRS_TELETEXT_TEST_G1_ALPHA_CONFORMANCE_FIXTURE="$TELETEXT_EVIDENCE_DIR/g1-conformance.json"
export ERSATZRS_TELETEXT_TEST_G1_ALPHA_IMAGER_DIAGNOSTIC_FIXTURE="$TELETEXT_EVIDENCE_DIR/g1-imager-diagnostic.json"
export ERSATZRS_TELETEXT_TEST_G1_ALPHA_LIBZVBI_FIXTURE="$TELETEXT_EVIDENCE_DIR/g1-libzvbi.json"
```

The unmodified main-composition helper can replay either imager request; only
the diagnostic is expected to pass on that pinned version. The unmodified
object helper replays the libzvbi request. All three requests carry identical
production packet bytes and exact owner identities. Neither decoder is a runtime
dependency or mandatory acceptance client. Full versioned suitable-client
rendering/navigation and transport checks remain separate required evidence.

### Original attribute execution

`resolve_teletext_attribute_execution` keeps character precedence separate from
the four attribute groups: foreground, background, flash and Display Attributes.
Main composition supplies the checked base cells, original spatial trace, glyph
layers and original full-row colours from the same immutable service. Standalone
owner checks are exact page/subcode checks, not generation certification.

Active and original Local attributes use the lower context at each affected
cell, continuing to the end of that row's area even without an object glyph.
Adaptive objects preserve only explicitly changed attribute types within their
own addressed row extent; other types still inherit the lower cell context.
Passive objects start with fresh standard defaults for each invocation, keep
their private attributes across row changes and affect only their explicit
character cells. Selecting a character contender never chooses an Active or
Local attribute bundle.

C2 compares where an attribute first takes effect, then where it is placed.
Equal keys require actual transmission evidence: logical event order is not a
substitute. The final main/local-definition packer retains each command's actual
word position after compatibility selectors and restores have been inserted.
The same-service compiled factory maps those positions back to original sources
and exact executing instances, then rebuilds attribute execution, display
resolution and colour composition before returning the immutable result.
Only comparisons within the same original Local main or the same exact object
invocation use this evidence. Resource order, definition storage order, pointer
values and recursive preorder do not establish cross-instance transmission order.
Standalone composition has no physical evidence and keeps equal-key contention
pending. Typed pending diagnostics retain the original contenders and their
source, root and invocation rather than guessing a winner. Adaptive endpoints
whose interpretation depends on AP-only or invocation-only addressing also
remain explicit pending work. These diagnostics do not impose permanent authored
feature restrictions.

The bounded logical plane includes potential main, right-panel and left-panel
areas independently. Panel defaults use CLUT 0 white and the pertaining full-row
background; main-area attributes do not spill into either panel. Header-owned
cells and header panels have no executable cell. Wider original positions remain
located diagnostics rather than clipped content. Actual enabled panel widths,
object-area validity and size footprints are separate required validation.

Display Attributes retain separate field provenance for spacing size, boxing and
conceal controls. Spacing Normal Size and Double Height also terminate earlier
non-spacing width changes. Base spacing mosaic separation stays distinct from
non-spacing D5; the latter also controls enhanced character underline/separation.
Logical colours retain their sources when the diagnostic colour helper applies
inversion. Flash mode and rate are retained without claiming timed animation.

This intermediate does not execute character-set lifetimes, rasterize characters
or DRCS, validate prohibited object overlap/open-object limits, normalize physical
instructions or authorize rendering/publication. Unresolved C2 transmission ties,
Adaptive endpoints, geometry and both receiver views remain mandatory work.

### Final packed attribute-order proof

The portable physical-order regressions independently read Hamming data bits and
check parity. They cover a Local tie across X26/0 triplet12 and X26/1 triplet0,
one exact Adaptive instance, compatibility words shifting the semantic command,
sparse/stale/duplicate evidence and preservation of an older cross-instance
unknown. Both original attributes and final display/colour are checked.

Two opt-in production fixtures use the unchanged frozen external helpers:
teletextimager 0.2.0 at `2b0125b778c45ffdfa2df0b196870c093361f317` checks
Local and Adaptive magenta cells and unchanged Level 1.5 fallback; libzvbi 0.2.42
checks the Local cell. This does not claim libzvbi's local-Adaptive support or
interactive client navigation. The prior eight-case DRCS raster request remains
byte-identical. Exact run results belong to P9A-01 evidence.

| Request | SHA256 |
| --- | --- |
| Local/Adaptive C.2 imager | `be4fc2735981f664c22058a9219fc78cc598d40be0ae9b9c5ec61b29b9cca9e3` |
| Local C.2 libzvbi | `5d49e42cf8b1d53da849712c0dd5a8257f48c59ef1f929f867e283f5d295fd99` |
| Reused eight-case raster | `7ecb654a044afe4d47019c8dab82be7eef1e3517deccac7d97533986ab12afb2` |

```sh
ERSATZRS_TELETEXT_TEST_IMAGER_DIR="$TELETEXT_IMAGER_DIR" \
ERSATZRS_TELETEXT_TEST_COMPILED_COLOUR_TIE_IMAGER_FIXTURE="$TELETEXT_EVIDENCE_DIR/c2-imager.json" \
ERSATZRS_TELETEXT_TEST_COMPILED_COLOUR_TIE_LIBZVBI_FIXTURE="$TELETEXT_EVIDENCE_DIR/c2-libzvbi.json" \
ERSATZRS_TELETEXT_TEST_RASTER_FIXTURE="$TELETEXT_EVIDENCE_DIR/c2-raster.json" \
./run.sh test -p ersatzrs-ffmpeg --lib --run-ignored all \
  -E 'test(physical_order_tests::) | test(compiled_same_emitter_colour_ties_decode_with_pinned_imager) | test(compiled_local_colour_tie_decodes_with_libzvbi) | test(composed_drcs_raster_matches_independent_native_pixels)'
```

### Coded-base height fragments and suppression

The same immutable main composition retains `base_footprints()` alongside its
exact coded `base_display()`. This fixed 25-by-40 intermediate references original
coded positions and fragment origins; it does not replace original cells or
interpret the after-Adaptive attribute projection as a base rendering layer.

The existing authored fallback emits normal-height and double-height spacing
controls. Double Height is SetAfter; Normal Height is SetAt. Any processed spacing
Double Height control suppresses all ordinary data and controls in the next main
row, including when immediately cancelled or placed in column 39. Suppressed
lower-row controls do not trigger further suppression. Double-height lower
fragments reference their exact upper origin. Remaining lower cells are
background-only and reference the upper background origin. Final background
still requires the upper Base/Active/Local layer; Adaptive/Passive background-only
overrides must not be inferred from these base facts.

Each row separately retains its real mapped start defaults, supplied from the
original presentation and colour context. This includes full-row black-background
substitution independently of spacing CLUT remapping. Ignored lower-row coded
foreground, background, flash and size do not become enhancement inheritance.
Nonspacing-only height does not create whole-row spacing suppression. Panels are
not ordinary lower-row data and are resolved separately by final layer replay.

Illegal height rows 0/23/24 retain located pending boundary facts, including
affected lower-row uncertainty, without choosing universal clipping. Independent
imager 0.2.0 and libzvbi 0.2.42 agree on legal suppression corner cases but differ
at those illegal boundaries. Their public fragment-tag numbers also differ.
libzvbi 0.2.42 disagrees with Table 4's independent black-background substitution
under asymmetric remapping; its output is not a substitution-conformance oracle.

The bounded `teletext-base-footprint-decoder.py` oracle consumes actual producer
DVB units and literal code/repertoire/mark/foreground/background/fragment tuples
at both Level 1.5 and 2.5. It uses the existing pinned imager loader and source
guards without changing the original composition oracle or its requests. Native
production proof distinguishes spacing height fragments from nonspacing-only
height preserving uncovered lower coded data. This proves selected decoded
fragments, not pixels, full C.3 overlap, suitable-client navigation or publication.

Replay the production fragment gate with the existing pinned checkout:

```bash
ERSATZRS_TELETEXT_TEST_IMAGER_DIR=/path/to/pinned-teletextimager \
./run.sh test -p ersatzrs-ffmpeg --lib --run-ignored ignored-only \
  -E 'test(teletext::base_footprint_runtime_tests)'
```

Set `ERSATZRS_TELETEXT_TEST_BASE_FOOTPRINT_FIXTURE` to a file in an existing
temporary directory to archive the exact request. Its SHA-256 is
`d7a29971f30296d98df958b6e7bebb906f2c27b584f9668f5a1ca159dc9c3d32`;
the new helper is `a0273b4755de05bacc39e5e8c57624842a045891b1f46e944fb8933524678a49`.
The original composition helper remains
`e1cb2ad2b16e78a0f56baaae14434aa6d78f146d7e0173c7ade9f19db97228eb`.

### Original candidate attribute observations

`TeletextAttributeExecution.records()` retains one immutable pre-geometry
observation per original glyph-layer candidate, with identical index, candidate,
source/root/instance and Base coded/Unicode role. Original losing contenders are
not removed. The execution also retains fixed shared cell stages after
Base/Active/Local tails and after Adaptive application through
`before_adaptive_cell()` and `before_passive_cell()`. The existing `cell()`
projection still includes the winning Passive overlay; it does not replace either
shared stage.

Base/Local/Active/Adaptive observations use the shared stage before Passive
application. Every Passive glyph instead captures its actual invocation's private
state at that original event, before diagnostic winner filtering. A later blue,
normal-size Passive glyph cannot erase an earlier red, double-size Passive
observation or overwrite the shared lower-stage data. Fresh invocation defaults,
private row lifetime and all per-field origins/pending handles survive. Original
header/wide Passive observations retain private state with an explicit
unaddressable reason; other unaddressable observations have no fabricated shared
cell. Attributes do not grant placement or make invalid original positions legal.

The same immutable registry owns every copied pending handle. All candidate and
original command counts, checked candidate/stage/private-slot/scratch storage and
work bounds precede allocation. Shared original correspondence validation checks
exact Base cells and one-to-one original glyph events, including full
source/root/instance/type/priority/position. It does not run object-overlap or open
interval admission on behalf of this observational boundary. The same-service
composition supplies the unchanged authoritative inputs; matching IDs alone are
not a generation certificate.

These observations are not final candidate rendering attributes. Adaptive has
already affected the shared before-Passive stage; its individual overrides and
original writers remain authoritative for stage-aware replay. Final C.3
resolution must process Base/Active/Local, Adaptive and Passive layers with
instance/fragment provenance, rather than enlarge a flattened highest-ranked
glyph. Preserve requested/effective size, origin attributes across every fragment
and obscured writers deferred to the next free position unless superseded.
Spacing double-height suppresses lower-row main-page Base data and controls;
nonspacing-only double-height leaves uncovered Base data active. Re-execute those
baselines and enhancement inheritance; do not copy an unsuppressed final cell
snapshot or erase all selected panel columns. Boundary violations need located
authoring diagnostics; discretionary decoder responses are not a clipping rule.
Full C.3, A.7/A.8, physical realization and engine/client/editor/publication
acceptance remain required and open.

### Layer-aware display resolution

Main composition retains `display_resolution()` from its exact original Base,
spacing footprints, commands, glyph candidates, attribute observations, glyph
intent, DRCS and selected geometry. The fixed visual plane refers to compact
immutable origin, attribute-selection and writer pools. Original losing,
obscured and unplaced candidates remain indexed rather than being discarded.
Additional execution origins can reference the same original candidate when an
Adaptive attribute-only change re-renders a shared-layer character. This keeps
older uncovered fragments and their attributes intact instead of changing their
original context in place.
This result is not a generation certificate or publication authorization.

Base, Active and Local share execution state. Original spacing writers and
enhancement writers execute over visual left/main/right areas. Spacing-suppressed
lower main rows discard coded controls and glyphs; an actual enhancement glyph
there inherits that row's original mapped defaults. Background-only cells use
the upper shared-layer background. Attribute-only Adaptive or Passive work does
not repaint those cells. Nonspacing-only height preserves uncovered lower data.

Adaptive instances inherit each unchanged attribute group from the shared
Base/Active/Local cell at the current position; an explicit Display Attributes
command replaces the whole six-flag group. Passive instances start with fresh white-on-black Normal
private state and paint only their own glyphs. A Normal Passive glyph can overlay
one lower enlarged fragment while preserving the other fragments and origins.

Every occupied part references its origin's attributes. Obscured writers retain
their original source and placed/requested position while their effective start
moves to the next free position within their valid area and lifetime. C2 still
compares effective start and original placement; equal cross-source keys retain
typed pending transmission evidence rather than choosing traversal order.
Requested attributes remain distinct from effective footprint geometry. Use
`effective_size()` and fragment identity for occupancy, never re-expand a cell
from its requested Double Width/Height writer flags. A double-size request whose
width conflicts with an earlier occupied part can retain legal double height.
When every retained contending size flag agrees, requested/effective geometry
can be known despite unresolved writer priority. Both predecessor chains are
checked without selecting a fictitious winner: original pending field values,
writer sources and diagnostics remain unchanged. Mixed or missing evidence
still prevents a size decision. Checked iterative traversal charges the existing
work budget before scratch allocation and retains original-candidate failures.
Illegal boundaries and ambiguous inherited Adaptive sizing on a non-origin
fragment retain located pending evidence rather than a universal clipping rule.

The opt-in `display_resolution_runtime_tests` fixture uses actual producer DVB
units with literal Level 1.5/2.5 tuples for covered foreground deferral and
supersession, Adaptive origin size inheritance/explicit Normal, an explicit
Normal overlay on an Adaptive right fragment, and Passive right/bottom fragment
overlays. It reuses the pinned imager 0.2.0 fragment helper.
Separate temporary comparison showed libzvbi 0.2.42's global recopy erasing Normal
Passive overlays; both decoders also failed the tested legal double-size
axis-reduction case. These observations are decoder limitations, not permission
to change normative wire output or positive proof of those disputed functions.

```bash
ERSATZRS_TELETEXT_TEST_IMAGER_DIR=/path/to/pinned-teletextimager \
./run.sh test -p ersatzrs-ffmpeg --lib --run-ignored ignored-only \
  -E 'test(teletext::display_resolution_runtime_tests)'
```

Set `ERSATZRS_TELETEXT_TEST_DISPLAY_RESOLUTION_FIXTURE` to an existing temporary
directory's file to archive the request. Fragment decoding does not establish
glyph pixels, full A.7/A.8 admission, physical receiver equivalence, suitable-
client navigation, editor interchange or engine/publication acceptance. These
remain required; pending evidence is not a permanent feature restriction.

The final focused run passes all 724 teletext tests
(`d4a1b06a-a342-40f0-9b82-d918515a9658`), including 26 new domain cases and two
same-service composition regressions. All eight native checks pass
(`bdf1a632-11a9-47b5-a4b7-3ea5a9112234`); the seven prior requests plus the
spacing/nonspacing footprint request retain their exact previous bytes. The new
six-case request SHA-256 is
`14d2f71a71c5cbb86cde2e500924830543768d07ae90a5d257bb9e6f79b1858c`.
The unchanged fragment helper SHA-256 is
`a0273b4755de05bacc39e5e8c57624842a045891b1f46e944fb8933524678a49`.
Strict domain/FFmpeg all-target Clippy, workspace formatting and architecture
checks pass. Independent review covers normative stage behavior, candidate/source
retention, bounded work and the behavior-preserving helper extraction.

### Original charset contexts

`resolve_teletext_charset_execution` retains exactly one immutable record per
original glyph-layer candidate, in the same order, including losing glyphs.
Main composition supplies Base, spatial events and candidates from the same
immutable service. Exact owner and candidate/event checks reject inconsistent
inputs; matching identities alone do not certify a generation.

Local and Active mode-eight declarations share the absolute row. Spatial
column order, including the standard's main/right/left address order, determines
which preceding declaration applies; returning from a call does not restore a
private caller charset. In particular, an Active glyph at absolute column 20
uses a main declaration at column 10 even when recursive event preorder lists
the glyph first. A new row starts with the selected primary. Adaptive and
Passive instances instead start fresh at primary, retain their own declaration
across row changes and affect only their own glyphs.

Each glyph retains its own original definition order. An earlier glyph at the
same position cannot acquire a later declaration in its own definition. Equal
positions involving different instances retain a typed pending choice with all
original source/root/invocation contenders and previous context; logical event
preorder is not physical transmission evidence. Known enhancement choices retain
the exact original declaration index. Shared spans and linked pending choices
avoid a contender vector per cell. Trusted original-event and checked storage/
search bounds precede allocation; no input mutation or partial result escapes.

The record distinguishes G0, G1 alphabet, G2, G0-with-diacritic and independent
mosaic/G3/DRCS usage. Enhanced G0/G1 alphabet/diacritic glyphs omit national
substitution. Diacritics retain current G0/G2 and the explicit zero-diacritic
`0x2A` exception. Secondary G0 metadata never designates G2; no ESC is inferred
from its presence. Base retains its exact coded role, authored Unicode intent,
selected primary, extension presence and separate header national bits. National
substitution is applicable only to actual coded Base G0 glyphs, including G0
alphabet positions in graphics mode, rather than mosaics or spacing controls.

This is repertoire-context execution, not character-table lookup or certification
that non-Latin presentation preserves authored Unicode in the current Western
Latin fallback bytes. ESC authoring, glyph/mark assignment, DRCS selector
execution, footprint/panel geometry, final physical normalization and complete
Level 1.5/2.5 receiver proof remain open. No partial result grants preview,
publication or engine acceptance authority, or imposes a permanent feature limit.

### Original glyph assignment and render intent

`resolve_teletext_glyph_intent` consumes the original charset execution and
retains one record per candidate, in unchanged order, including losers and Base
blank/spacing roles. The main-composition factory retains that exact immutable
charset execution alongside the lookup; copied pending indexes refer to its
shared registry. Equality of page identities alone is not a generation proof.
The trusted count includes all 992 Base cells and enhancement candidates; exact
count and checked linear-storage arithmetic precede allocation. Factory limits
add that fixed Base count to the original command bound with checked arithmetic.

Closed identities preserve repertoire, code and actual national subset without
Unicode or font substitution. G0 covers Latin, Cyrillic 1/2/3, Greek, Arabic and
Hebrew; G2 covers Latin, Cyrillic, Greek and Arabic. Base national substitution
uses all Table 32 aliases and the thirteen Table 36 positions, not a designation's
low three bits. The explicit Level 2.5 primary selection, extension presence and
original header national bits remain separate. Legacy clients that combine
region/header bits can differ; this does not prove both receiver views. Enhanced
G0/G1 alphabet positions never acquire national substitution. Fixed G1 mosaics,
G3 and raw DRCS references need no fictitious G0/G2 selection.

The normative assignment audit uses EN 300 706 V1.2.1 tables 35–48. These literal
exceptions are independent of fonts, blank glyph metrics and decoder mappings:

| Table | Explicitly prohibited positions | Unmarked empty positions |
| --- | --- | --- |
| Arabic G0, Table 44 | None | `0x7E` |
| Latin G2, Table 37 | `0x59`, `0x5A`, `0x5B`, `0x65` | `0x40` |
| Cyrillic G2, Table 41 | `0x26`, `0x28` | `0x40` |
| Greek G2, Table 43 | None | `0x40` |
| Arabic G2, Table 45 | None | `0x7F` |
| G3, Table 48 | `0x6E`, `0x6F`, `0x7E`, `0x7F` | None |

G3's four positions are explicitly unassigned and should not be transmitted;
`0x5F` is an assigned background-only glyph, and `0x20` is not a SPACE. Explicit
SPACE remains assigned in G0/G2. Unmarked cells retain exact identity with
`Pending/UnmarkedTableCell`, without guessing SPACE or imposing a transmission
ban. `ShouldNotTransmit` preserves the exact code and prohibition reason.

Nonzero diacritics retain exact current G0 and current-G2 column-4 entry.
Latin/Cyrillic/Greek marks include dot below at index 9 and low line at index C;
Arabic's column 4 contains letters and is not a Latin mark table. Proven
nonalphabetic bases produce `Unassigned/NonAlphabeticBase`. Known alphabetic
G0 with an actual Latin/Cyrillic/Greek G2 diacritic is assigned, retaining the
exact current pair, including Hebrew and supplied mixed identities. The font
primitive cannot invent a reachable Table 34 designation. Arabic alphabetic
eligibility and Arabic G2 letter composition remain precise
`Pending/UnverifiedComposition` requirements, without Latin substitution or
a permanent authoring exclusion.
Mode `0x10`/code `0x2A` instead produces `AtSymbol` independently of charset
ordering; other zero-diacritic codes use current G0. Ordinary direct G0 `0x2A`
remains its actual table position. Unknown charset order cannot manufacture one
repertoire identity, and keeps the original pending registry index.

Assignment is a diagnostic semantic intermediate, not renderer or validation
acceptance. Font shapes, Unicode projection, Arabic joining/composition, DRCS
selector resolution, object/size footprints, panels, physical order and complete
receiver/client/editor/publication proof remain open. No lookup result grants
preview readiness or authorizes publication.

The assignment checkpoint passes 93 selected tests
(`c5aec45c-04ad-48fd-85a5-578786348008`) and 37 final affected tests after cosmetic
semicolon/factory-extraction fixes (`b7e7cf19-851a-45c2-965b-be5f4e457282`). Ten
new domain tests exhaust G0/G2/G3 positions and all national aliases, with literal
primary-table fixtures, original losing sources, pending order and exact budgets.
Three consumer proofs retain the same immutable charset context and Base intent.
Strict affected all-target Clippy, formatting and independent reviews pass.

Six final native tests pass (`f24b1766-4f60-4b54-afda-64eed9fffb68`). The new
`original_non_latin_g0_assignments_decode_with_libzvbi` case uses production
composition/packing with original X26 G0 selections. Primary tables 39, 42 and 46
independently identify Cyrillic capital A (`0x24/0x41`), Greek capital Alpha
(`0x37/0x41`) and Hebrew Aleph (`0x55/0x60`); literal Unicode scalar expectations
are `U+0410`, `U+0391` and `U+05D0`. Frozen libzvbi 0.2.42 Linux-x64 formatted-cell
decoding returns each with exact colours and unchanged Base calibration B. This
is no pixel/navigation/client, complete repertoire or faithful non-Latin Base
encoding proof. Existing frozen helper scripts are unchanged; all six previous
requests retain their prior hashes. The new three-case request SHA-256 is
`777570132d17e63020d4deee8bf60024bf0f6097f2ac01a21f3b1f1c398a3abe`.
Replay/export uses the existing native suite plus:

```sh
ERSATZRS_TELETEXT_TEST_G0_ASSIGNMENTS_LIBZVBI_FIXTURE=/tmp/teletext-g0-assignments.json \
  ./run.sh test -p ersatzrs-ffmpeg --lib --run-ignored ignored-only \
  -E 'test(original_non_latin_g0_assignments_decode_with_libzvbi)'
```

### Original DRCS resource execution

`resolve_teletext_drcs_execution` consumes the original glyph intent and the same
immutable semantic service. A sparse result preserves every DRCS candidate,
including losers, repeated invocations and raw out-of-area coordinates, with its
unchanged candidate index/provenance, explicit scope/S1/slot, exact logical
resource identity and checked twelve-by-ten monochrome bitmap. Defined all-zero
bitmaps remain defined glyphs; they are not missing resources.

The trusted budget fences all intent records before scanning and exact DRCS
reference count before output allocation. The checked service already validates
resolved display links, catalog identity uniqueness, slots, at most two subtables
and 24 distinct glyphs. Resolution uses the selected display's supplied Normal/
Global links and that service's current catalog; it never guesses MOT/X27
precedence, another display's links or a different S1. Missing resources reject
the complete result with original page/position/source/root/instance diagnostics.
Original source-operation correspondence is checked without re-expanding the
program; it does not certify original AP/position or invocation generation.
Matching identifiers and commands are not a generation certificate: the owning
composition supplies and retains all projections from one immutable service.
Equal resource identities in a different service can carry different pixels.

The authored `Drcs { reference }` operation already carries resolved Normal/
Global scope, subtable and glyph slot. There is no separate authored raw selector
command to interpret. EN 300 706 Table 28/Table 29 and Annex C.4 require independent
private Normal/Global selector state: both start at S1 zero in main and every
actual object invocation, including Active; children start fresh, returning does
not modify their caller, and state persists across rows. Selectors do not move AP.
The existing lowerer synthesizes each required explicit selector from the logical
reference and starts each definition fresh. Existing literal selector and object
reset regressions cover that path; the resource projection does not introduce
charset-style shared row state or new transmission-order ambiguity.

Resolved pixels are input to the remaining renderer, not completed display
semantics. Colour/size application, glyph footprints, panels, overlap, physical
receiver equivalence and the operational preview/client/editor/publication gates
remain open. No successful bitmap lookup authorizes publication or establishes
full Level 2.5 acceptance.

Eight new domain and two consumer tests pass in the final 37-test selection
(`a11961bb-4d4d-41dc-86d0-f1678573e762`). They prove asymmetric Normal/Global
pixels, nested/repeated identity/provenance, losing and wide-coordinate retention,
service-specific pixel changes behind identical resource IDs, defined blank
versus missing slots, exact budgets and foreign owner/program isolation. The
initial 60-test selection passed 59 (`72201132-af47-4d3d-b537-9ed00cd46656`);
a fixture's reserved subcode bit (`0x4321`) was corrected to legal `0x2321`,
without changing production behavior. The final selection includes the corrected
test; unchanged additional catalog/glyph/selector reset evidence remains valid.

Six native checks pass (`1b9a5ece-33be-4127-a87d-b70e9d8faade`); all seven prior
positive/conformant/diagnostic requests are byte-identical to the glyph-intent
checkpoint's recorded hashes. Frozen helpers and renderer/encoder/lowerer bytes
are unchanged. This replay checks that the extended factory preserves existing
fixed-cell proofs; it is no new DRCS placement or full rendering proof. Existing
independent DRCS resource decoding remains applicable. Strict affected all-target
Clippy, formatting, architecture and independent reviews pass.

### Selected display coordinates and original object regions

`resolve_teletext_display_geometry` consumes Base presentation, original spatial
trace and glyph layers from the same immutable service. Main composition retains
this shared geometry alongside the original attribute, charset, glyph-intent and
DRCS projections. The geometry result is the boundary to extend with occupied
footprints and geometry-aware final attributes; point placement is not a separate
renderer or publication certificate.

The actual `level25_widths()` determines visible columns without changing raw
panel flags/status/width. Visual order is left panel, forty main columns, right
panel, while original address order is main/right/left. For left width L and
right width R, main raw columns 0–39 map to visual L–L+39; right raw 40–40+R-1
map to L+40–L+39+R; left raw 72-L–71 map to visual 0–L-1. Total width is at most
56. Absent presentation and Level-3.5-only panel metadata select no Level 2.5
panels. Raw unused metadata survives; no Level 3.5 support is implied.

Every candidate retains its original index, content and source/root/instance,
including losing Base/object candidates and authored Unicode intent. Header-owned
columns, header panels, disabled panels, inactive-width gaps and positions beyond
the display remain different explicit nonplacements. No clipping, wrapping,
rebinding or promotion of generated compatibility words occurs. All candidate,
event and invocation counts and checked linear-storage bounds precede allocation;
exact Base equality and one-to-one original glyph-event correspondence reject
mismatched projections. Matching identifiers alone cannot certify generation.

Per-instance region facts separate direct AP-addressing instructions from definite
own glyph/four-attribute-group writes. Pure position and charset setup is not a
written glyph. Annex A.4 nevertheless applies to all own AP-addressing commands:
column functions, position and row-colour addressing. Known raw addresses crossing
main/right/left areas reject with the offending original source/instance, including
charset-only bodies and disabled panels. Descendant addresses/writes never become
parent extents; a main-area parent can invoke a separate panel child. Screen colour,
private DRCS selector state and invocation/origin modifiers do not create additional
AP addresses. Out-of-display facts remain explicit and cannot silently create a
certified occupied rectangle. The existing `CrossAreaWrite` diagnostic now covers
address-only violations too.

Annex A.7 compares cells directly addressed by each original invocation, rather
than enlarged glyph rectangles. The validator uses the same AP-address membership
as A.4, including setup and charset addresses; this is the host's explicit reading
of A.7's addressed-cell rule. Raw inactive panels are included. Each invocation
retains a whole-instance mask of foreground, background, flash and compound
display attributes; Double Height and Double Width belong to the same type.

| Original object types | Shared addressed cell | Same-row A/B/A interleaving |
| --- | --- | --- |
| Active / Active | Allowed with disjoint attribute-type masks | Allowed |
| Adaptive / Adaptive | Rejected | Allowed only if neither instance redefines attributes |
| Passive / Passive | Allowed; later invocation has priority | Allowed |
| Different types | Allowed under normal layer precedence | Allowed |

Nested and repeated calls retain distinct instance identities. Character-size
fragments are resolved separately under C.3; a fragment collision does not create
an A.7 addressed-cell overlap. These checks do not settle pending A.8 intervals,
physical normalization or complete publication/client acceptance.

Known Annex A.8 intervals use raw row/column order, starting at the caller AP
before the invocation offset and ending inclusively at the last direct explicit
addressing event, including non-glyph setup. An address-only body can establish an
endpoint even without a written glyph. Missing, out-of-display and backwards
endpoints remain typed pending evidence. Child endpoints are excluded. These
interval records alone do not prove all A.8 admission limits, A.7 overlap
admissibility or C.3 double-size footprint/attribute behavior.
A bounded inclusive endpoint sweep now checks the known intervals across all
three object types together. Starts count before ends at a shared raw position;
raw inactive panels and header AP are included, independently of visible writes.
The sixteenth known open instance is rejected at its original call source/root
and raw caller AP. Empty, out-of-plane and backwards intervals stay pending; they
cannot hide a definite violation by sixteen known intervals. The immutable result
reports known/pending interval counts and maximum known overlap. A maximum below
sixteen alone does not certify full A.8 admission when unresolved intervals exist,
and complete successive-opening admission remains required. Final A.7 overlap,
C.3 footprints and complete A.8 admission remain required before full geometry,
renderer and engine acceptance.

The separate bounded successive-opening scan combines original calls at caller
AP before offset with actual glyph/four-group writes in raw main/right/left
scan order. Sixteen known successive openings reject at the original call/root/
child source even when all object endpoints are pending. Pure AP/charset setup
is not a reset. A descendant write strictly between successive caller locations
can reset a run; a later-row child write cannot erase earlier header calls merely
because recursive preorder visited it. Blank characters, unresolved glyph
assignments and inactive panels are not discarded as absent writes.

Within one same-position emitter, original command order is known. A mixed-source
position containing a potential reset retains all original witnesses and breaks
the proven run conservatively instead of assigning transmission priority.
Screen/row colour writes have planes rather than invented column addresses;
these and out-of-display potential writes retain pending witnesses and disable
the combined hard count claim. Unpositioned openings retain separate evidence.
The result is a lower-bound check, not full A.8 or publication acceptance.

The size-consensus/successive-opening checkpoint passes all 736 selected
teletext tests (`b9084b38-ce41-4e0a-a166-cc9646b1e45e`), including two new
multi-contender size tables and ten original-position call/write oracles. The
existing twenty increasing-position calls with intervening child writes remain
valid. The pending-interval fixture now includes an explicit original writer
between fifteen calls and the additional empty call, preserving its separate
simultaneous-count purpose. A row-colour test's backwards setup was corrected
to a later legal row; production ordering checks were retained.
All eight native checks pass (`8ab7c09b-1951-4403-88b2-f43db1e2c114`) and all nine
existing producer requests remain byte-identical. Strict domain/FFmpeg all-target
Clippy, formatting, architecture/progress checks and independent production/test
review pass. These results do not resolve mixed-source transmission order,
plane-reset placement, complete A.8 admission or full renderer/editor/client/
publication acceptance.

### Diagnostic raster from the original composition

`render_teletext_composition` consumes the same immutable unpublished
`TeletextMainComposition` as composed packet lowering. Its host-only
`TeletextRasterBudget` bounds pixels and diagnostics before output allocation.
Known cells yield RGB4 or video pixels; unresolved cells refer to one original
located diagnostic per visual cell. Page/subcode, visual and original positions,
candidate, command/root and invocation remain separate. Receiver-owned header
cells carry `ReceiverHeader`, distinct from authored errors and RGB/video pixels.
Section 9.3.1 transmits only columns 8..39 as display data; informative E.7(1)
notes receiver use of the first eight columns without prescribing identifier glyphs.
Section 12.5.2 permits wider-header reformatting. The preview can present the exact
page/subcode from its owner in navigation; it must not copy libzvbi's low-byte
subpage abbreviation. This marker supplies no clock/search/C7 runtime state or
header-object admission. Authored header cells still rasterize normally. This
diagnostic output is not an admitted browser page or publication capability.

The domain mask lookup embeds 28 pinned MIT bitmap tables: seven G0 families,
thirteen Latin national subsets, four G2 families, G3, contiguous/separated G1
and reduced Latin capitals. All masks are twelve by ten, matching exact DRCS
bitmap geometry. G1 alphabetic identities use current G0, never a mosaic font.
Base Unicode replacement masks reuse the existing Latin encoder mapping at Base
priority; an Active glyph still overrides them. The selected fragment determines
normal/double-width/double-height/double-size pixel sampling, independently of
requested size flags. Sparse DRCS records must match the exact scope/subtable/
slot from this composition. Coincident identities in another service cannot
change retained masks or palettes.

`TeletextColourComposition` retains fixed CLUT0/1, selected CLUT2/3, row and screen
planes and page kind. CLUT1 uses seven-fifteenths intensity; its first entry is
transparent. C.1 chooses foreground/background, row colour or video based on
boxing and subtitle/newsflash flags; transparent is not an unconditional alpha
clear. Colours already remapped by original execution are not remapped again.
Conceal/reveal and supplied flash phase select pixels deterministically, without
inventing a clock epoch. Table 29 D4 swaps final logical references before
foreground flash selection and C.1. Adjacent-CLUT flashing therefore selects the
exchanged foreground's paired CLUT. Pinned QTeletextMaker decode/render source
corroborates this order; imager's adjacent-plus-invert path instead pairs the
unswapped foreground, a recorded decoder discrepancy rather than an encoder rule.
Enlarged bottom fragments use their exact retained origin's full-row plane,
including appended repaint origins. C.3(4) and both pinned decoder sources agree;
ordinary BackgroundOnly cells continue to use their own row plane.
Table 4's explicit-background rule now checks two definite parameter uses in
this consumed diagnostic output. Selected BBS plus a winning Base row default or
Black Background (1C) writer identifies substituted original background. A
transmitted 1C is still substituted: C.2 writer explicitness is not sufficient.
Explicit New Background and non-spacing background remain distinct even when
they yield identical colour indexes. Panel/private defaults are not classified
as BBS-derived. Pending attributes are reported first.

A FullPage window diagnoses only an actually consumed transparent original-BG
pixel; ordinary foreground-only pixels and Subtitle/Newsflash Boxing survive.
Inverted Adjacent CLUT uses original BG as its paired-foreground parameter, so
that authored condition is checked in **both phases**, including a current phase
that does not yet pair the CLUT. The located typed failure retains full
background, consumer and inversion writers; later explicit background overrides
can restore valid output. These two checks do not certify all Table 4 parameters
or provide whole-service/publication admission. Equal RGB tuples alone cannot
prove compliant authoring.

Table 29 D5 separates G1 mosaics and underlines alphanumerics, including SPACE
and spacing cells displayed as SPACE. This raster chooses the full bottom mask
row before enlargement. Table 26 notes 11/12 restrict spacing mosaic separation
to Base Level 1/1.5 characters; enhanced G1 depends on D5 alone. An explicit
Display Attributes triplet with D5=0 also writes spacing separation false,
retaining the same original source, physical C.2 order, C.3 deferral and object
scope. Row/private defaults do not cancel spacing. A later spacing Separated
Mosaics can restore separation; spacing Contiguous Mosaics cannot clear D5=1.
Carrier materialization must preserve this Base/enhanced distinction rather than
unconditionally combining spacing with D5. G3/DRCS remain unchanged for their undefined D5 response;
Annex G.2 permits decoder-specific non-DRCS/underline appearance. Exact atlas
shapes are a font policy, not normative pixel geometry.

The D5 correction is covered by four original-writer and two actual-raster
regressions, including all original execution scopes, exact pending sources,
hidden-writer deferral and later spacing restoration. Three production compiled
X26 cases also pass independent libzvbi 0.2.42 decoding and guarded normal-cell
RGBA rendering: enhanced G1 with default D5 over separated Base data, explicit
D5 separation and explicit D5=0 cancellation of Base separation. The host atlas
uses separated rows `0,1950,1950,0,1950,1950,1950,0,1950,1950`; the pinned native
font uses `4030,4030,0,4030,4030,4030,0,4030,4030,0`. Both contiguous masks are
`4095` on all ten rows. These separate literal font expectations follow Annex
G.2's decoder font freedom; native decoding still checks exact mosaic identity,
normal size, opaque white/black colours and unchanged Base calibration `B`.
Production raster must match its own literal before native loading.

The normative later-spacing oracle remains positive and fails on libzvbi
0.2.42: a later Base `1A` after explicit D5=0 remains contiguous in that decoder.
Its test is explicitly ignored as a known decoder regression, with no changed
golden or accepted restriction on authoring. The same four production-wire cases
all pass pinned teletextimager 0.2.0 at both Level 1.5 and 2.5, including that
later spacing restoration. Literal tuples check full mosaic code 127,
contiguous/separated set 24/25, white/black and the unchanged calibration `B`.
The existing helper's eight upstream source guards remain unchanged. This
offscreen calibration is
Linux x86-64 only and establishes no suitable-client navigation, complete
Level 2.5, editor or publication acceptance. P9A-01 owns current test evidence.

| D5/G1 evidence | SHA-256 |
| --- | --- |
| Passing three-case request | `6b3e56c7d6309971912f770bfe2b54f8cc080b164a43d8470709f79ec0aea17f` |
| Correct later-spacing request | `c8d8b0e2dbbf14fa730fab54a11a644d38b572aa7b63ca7a4ba68bb7346fabcb` |
| Native mosaic helper | `94619eb91da173536b41ddc4ce4da05d63061dcd7898fe304b29e8cfd98336bf` |
| Passing four-case imager request | `8621d2e1f401e4ce82311a30d114f64d77b7131771f8c71581f39cb63ca26ef9` |

```bash
ERSATZRS_TELETEXT_TEST_MOSAIC_FIXTURE=/tmp/teletext-mosaic.json \
  ./run.sh test -p ersatzrs-ffmpeg --lib --run-ignored ignored-only \
  -E 'test(composed_mosaic_d5_raster_matches_independent_native_g1_pixels)'
python3 scripts/e2e/teletext-mosaic-decoder.py < /tmp/teletext-mosaic.json
python3 scripts/e2e/test_teletext_mosaic_decoder.py
# Explicit pinned detached checkout, as in the other imager calibrations.
ERSATZRS_TELETEXT_TEST_IMAGER_DIR="$TELETEXT_IMAGER_SRC" \
  ./run.sh test -p ersatzrs-ffmpeg --lib --run-ignored ignored-only \
  -E 'test(composed_mosaic_spacing_decodes_with_pinned_imager)'
# Expected failure on libzvbi 0.2.42; retain the standard-conforming oracle.
./run.sh test -p ersatzrs-ffmpeg --lib --run-ignored ignored-only \
  -E 'test(composed_mosaic_later_spacing_reenables_separation_with_libzvbi)'
```

Known alphabetic composite masks use the actual current G0 base and current G2
mark. Below-letter marks 9/B/C/E retain the full base, including Latin capitals;
above-letter Latin keeps the existing reduced-capital policy. Other known
repertoires, including supplied mixed/Hebrew identities, reserve the actual
mark's occupied top rows. The ring needs three rows. Occupied base rows translate
when they fit; otherwise bounded row-union vertical fitting uses every original
row and retains horizontal bits. This deliberately changes the font's vertical
shape; it is not lossless atlas reproduction or Latin glyph borrowing. The exact
mark mask remains unchanged and visible. Table 29, section 15.6 and informative
E.10/G.2 distinguish current repertoires, support advice and decoder font freedom.
Arabic/mixed-letter composition, unknown assignments and actual client proof
remain required and open. No permanent authoring restriction is added.

Six colour regressions, four literal mask/policy tests, two original-intent tests
and two actual-composition raster checks extend the passing focused suite to 785
(`0410bf54-f84c-4f61-9f26-b3ceb8530efb`). The 23 affected colour/consumer tests pass
after equivalent Clippy method-reference/Copy corrections
(`d143bfd7-5928-4f0d-b468-bd49d48c6207`); the other valid results are reused.
The real service fixture distinguishes equal explicit/substituted window colours
and preserves its parameter writers. A second fixture proves the actual current
Cyrillic2/Cyrillic pair reaches literal dot-above raster pixels without Latin
substitution. Eighteen synthetic base/mark/result contact-sheet cases were
visually checked for legibility, separately from Rust execution and independent
native rendering. Eight actual-production DRCS pixel cases still pass with
libzvbi 0.2.42 (`b17a54d7-c035-46e3-9f7f-087b6b57c459`), retaining the identical
request hash recorded below. Strict all-target Clippy and independent review pass.
This accepts the specified font policy and definite diagnostic
checks, not full Level 2.5, Arabic, transport or suitable-client/publication gates.

The [bitmap provenance](../../crates/ersatzrs-domain/src/teletext/font_data/PROVENANCE.md)
records pin, input hashes, table ordering, license and reproducible stdlib import.
The immutable 53,760-byte binary has SHA-256
`34a8859adad3c488a03c4f8e53abe45bd7f25cb4926594f179d74bffc0e83c7a`.
`import_glyph_masks.py --check` reproduces it without installing the decoder,
Pillow or any production runtime dependency. Changed pinned source bytes reject
before output creation. The root distribution license carries the complete MIT
notice alongside the unchanged ErsatzRS zlib notice.

The earlier raster checkpoint's 771 selected domain/FFmpeg teletext tests pass
(`0d88004d-4369-4c8e-82de-88ec096d245b`), including sixteen colour tests and ten
actual-composition raster tests. Literal regressions cover joint invert/flash,
origin-row transparency and receiver-owned header cells under a zero diagnostic
budget. Earlier eight native assignment/fragment results and their nine request
hashes remain valid; no affected producer logic changed.

An additional actual-production pixel check passes with libzvbi 0.2.42
(`579b2a42-45f6-41ea-986e-df8f3bee6fe2`): eight opaque steady DRCS rectangles
cover Normal/Global scope, Normal/DW/DH/DS enlargement, invert, concealed and
revealed output. The same compiled composition supplies the raster and production
X26; its service supplies X28 palette, MOT and complete resource extent. The
independent helper checks exact decoded identity, scope/S1/slot and 120 bitmap
bits before comparing native RGBA32_LE and production RGB4 to a literal bitmap,
colour and enlargement oracle. Full enlarged rectangles and continuation sizes
are checked before native drawing; bounded canvases retain canaries. This does
not use borrowed font shapes as an independent oracle.

Replay the focused native check with:

```sh
ERSATZRS_TELETEXT_TEST_RASTER_FIXTURE=/tmp/teletext-raster.json \
  ./run.sh test -p ersatzrs-ffmpeg --lib --run-ignored ignored-only \
  -E 'test(composed_drcs_raster_matches_independent_native_pixels)'
python3 scripts/e2e/teletext-raster-decoder.py < /tmp/teletext-raster.json
python3 scripts/e2e/test_teletext_raster_decoder.py
```

The opt-in ABI calibration is Linux x86-64 only; portable raster tests remain on
all supported platforms. The request SHA-256 is
`7ecb654a044afe4d47019c8dab82be7eef1e3517deccac7d97533986ab12afb2`;
helper SHA-256 is
`7e0ff9475a01cb9f583f4c8de5b02e6499365e5eaa295f393d0ae10c53626cb5`.
Four reproducible tests cover 35 malformed request rejection paths before native
loading, with a valid-fixture control reaching the mocked native boundary. Strict all-target Clippy,
formatting, architecture/progress and independent source/test review pass.
This proves the named opaque steady rectangles, not transparency/video, flash
clock/rates, non-DRCS shape conformance, TSDuck transport or suitable-client
navigation. Full A.7/A.8/C.3, physical normalization, remaining composite and
background-parameter evidence, suitable-client display/navigation, localized
headed preview, TTI editor/publication and engine acceptance remain open. No
milestone contract or permanent restriction changes.

### Compatibility-aware composed packing

`compile_teletext_composed_main_program` composes and packs one exact owner from
the same immutable semantic service. Its catalog, selected resource links and
original header-row colour cannot come from a detached, different context.
The returned semantic plan and single main/local-definition sequence remain
unpublished. Packing failures map through authored/default/base provenance;
definitions retain their original full identity and command ordinal.

An opt-in lowering policy separately tracks the historical Level 1.5 row and
enhanced AP. Every emitted row-address word updates the former, including screen
colour, DRCS selection and object modifier/invocation words. Before a compatible
body-row glyph, a same-position mode-four selector restores the intended row if
necessary. Atomic modifier/invocation and DRCS-selector/glyph pairs remain intact;
normal/global latches and enhanced invocation anchors do not change. A first
one-word header operation can establish row zero with its original resolved colour
while AP is still zero/zero. Initial compound DRCS remains an unchanged atomic
pair; it would immediately disturb that row again.

Local invocation addresses depend on their eventual definition markers. After
every local invocation the opt-in policy therefore marks the compatibility row
unknown, even if a preliminary pointer appears to address the desired row. The
next compatible body glyph emits a restoration independent of that preliminary
pointer. Both sizing and emission use this same policy, with every actual selector
charged before body allocation and final forward-pointer placement. High marker
104/N80 and 206/NFB regressions cover rows one and 24 and the exact 208/209 limit.
Legacy compilers and individual definition states keep the original policy and
unchanged packet bytes.

Table 28 provides no neutral header selector: 63/07 also resets AP to zero/zero
and selects colour. Repeating it after column advancement violates Annex A.4.
Explicit header positions and header attributes followed by a row-disturbing
call/DRCS selector and a later compatible glyph still need semantic instruction
normalization. Current packing returns a source-located unresolved-lowering error
rather than reserving a feature exclusion or emitting backwards/reserved words.
Deferred AP and exact invocation-offset adjustment can resolve some cases; full
attribute inheritance, object/base priority and final display realization must
be established before choosing a general normalization. Equal-looking charset
or colour replay is not a generic neutral column operation.

Two additional actual-production cases decode through the unchanged pinned
teletextimager oracle at both Levels 1.5 and 2.5: a body glyph after a Passive
invocation, and an initial header glyph after explicit screen colour. The separate
two-case fixture SHA256 is
`a8e677edf49a3141a5f5a3c2e412d1e6dae29db653b41e42a88f621b7dbb3774`.
The original four-case composition fixture remains byte-identical. The
[same-service packing workflow](../../scripts/e2e/README.md#teletext-compatibility-aware-main-calibration)
uses the same guarded loader and dedicated helper, without new runtime dependencies.
Pinned imager's Level 1.5 path does not model every historical any-row-address
disturbance, and its 63/07 handler advances directly to column eight. Its green
cells do not prove all historic receiver transitions. Literal wire vectors and
independent protected-pointer decoding prove inserted fields and physical sizing;
full suitable-client/decoder coverage remains required. Full header normalization,
non-Latin decoding, object/base/attribute geometry, original default realization,
complete service lifecycle and publication remain open.

## Resource object allocation and invocation

`allocate_teletext_resource_objects` uses the checked object catalog, separately
supplied original MOT default targets and a trusted object/input budget. It
checks raw counts before scanning and all defaults before assigning slots.
Repeated defaults are validated but deduplicated; distinct defaults retain
supplied order, then remaining definitions retain authored catalog order.
One immutable map covers every resource invocation and MOT default. Each key
retains complete scope/page/S1, the entire host `u16` ID and exact type. The
allocator never truncates IDs, rebinds owners or spills into another subpage.
Local definitions remain in the input budget but require separate shared-page
packing; they do not acquire resource pointers.

Each resource/type/S1 admits at most 32 objects, of which MOT defaults occupy
only the first 16. The nine-bit number is
`S1 | half<<4 | group<<5 | pointer_packet_index<<7`.
Pointer packets 1–4 each contain four groups of even/odd pointers per type.
`mot_default_allocation()` returns an exact eight-bit selector only for the
first two pointer packets, preserving target/type. Later pointers require
explicit invocation. Invalid targets/types/default scopes, budgets or capacity
return located identifiers and no partial map. This allocation does not prove
packet-body capacity, pointer values or emitted definition pages.

`compile_teletext_local_program_with_resource_objects` takes the same map and
explicit resolved public/global links. It checks selected resource page and exact
allocated target/type before writing the nine-bit invocation fields. Any supplied
origin modifier immediately precedes its invocation, including explicit zero
modifiers; invocation leaves the caller's active position and DRCS selections
unchanged. Direct invocations are bounded at 127 per type and all inserted
modifiers consume the existing 207-payload-triplet capacity. The semantic service
still owns transitive counts, type/ownership closure and selected dependencies.
This resource-only entry point retains `NeedsObjectAllocation` for local targets.
The separate local-object compiler packs their actual definitions into the same
owner's X26 sequence; no host ID becomes a wire pointer.

### Complete public/global object pages

`encode_teletext_object_page` encodes an explicitly supplied POP/GPOP resource
from the original catalog and the same immutable allocation map. It checks the
resource command aggregate before lowering, then requires exact catalog/map
completeness and types. Each definition is emitted once in physical assignment
order, with fresh active-position, DRCS and direct-invocation state. Nested calls
retain their checked resource pointers rather than expanding duplicated bodies.
Shared definitions do not own a display's selected links: the semantic service
must check each consuming display's complete transitive resource closure.

Pointer destinations address absolute triplets 0–506 relative to the first
triplet of packet 3, independently of pointer-table extensions. Pointer and
definition data cannot share a packet. Canonical contiguous pointer tables use
two/three/four packets and leave 507/494/481 definition-area triplets, including
every definition marker and the final terminator. Unused halves are 511; the
reserved first pointer triplet is zero. This contiguous policy does not exclude
other standard layouts from future import support.

The complete sequence contains a resource header, mandatory X28/0 Format 1,
pointer rows and content rows, with at most 43 DVB units. Header `nn` is derived
from the last content encoding row, including X26 designations; X28 does not
inflate it. Explicit transmission metadata chooses the full update state,
counter, serial/parallel mode and final-subpage termination data one or three.
An explicitly declared empty resource has unused X1/X2 and a termination-only
X3, without an invented object. It can represent a companion subpage required by
a declared extent. No next-page/FF closing header is added by this page primitive.

Budget, capacity, type, missing definition/allocation and lowering failures retain
the resource, original object and command ordinal when applicable. Adaptive and
passive objects reject screen/row-colour operations. Row-zero positioning still
requires context-dependent lowering. The result is immutable complete page data,
not service inventory, lifecycle history, carousel timing or publication authority.

Local definitions share all 208 X26 slots with authored and base-derived
enhancements. Original MOT banks must be supplied separately; the effective
per-display four default slots cannot reconstruct their provenance. Combined
base/enhancement packing,
final display semantics, complete service composition and publication remain
required acceptance work.

### Local object packing

`compile_teletext_local_program_with_objects` takes an explicitly present main
program, a checked catalog, the resource allocation map, selected links and
trusted object/command budgets. It checks the raw catalog count before scanning
and charges the main program plus every exact-owner local definition before
lowering. Owner identity includes the complete raw subcode. Unused owned
definitions retain authored order; foreign owners cannot satisfy invocations.

One sequence of at most 208 triplets contains the main program, its termination
data six, definition markers/bodies and final termination data five. Adjacent
markers terminate preceding definitions; final termination fills padding. Without
local definitions, the existing data-seven local-only output remains byte-identical.
An empty supplied program remains present; this primitive does not infer absence.

Two-pass sizing accounts for all inserted DRCS selectors and origin modifiers
before body allocation. Only the sizing pass uses private provisional selectors;
they never reach output. Final selectors use the exact definition marker index:
designation `i/13`, slot `i%13`, number `(designation<<4)|slot`. Slots 13–15 are
not invented, host IDs are not masked and forward/nested calls retain exact type.
Each definition resets active position, invocation counts and both DRCS latches.
Resource calls in local definitions retain this display's selected links.

Eleven new independent byte-oracle tests cover literal marker boundaries,
full IDs/types/owners, nested forward references, state isolation, inserted-word
capacity and exact 208/209 rejection. Final geometry/inheritance, effective
repertoires/expanded work and composition with base-derived glyph enhancements
remain service obligations. This compiler does not replace or append to an
arbitrary existing X26 sequence and carries no publication authority.

### Compiled physical page inventory

`validate_teletext_physical_page_inventory` fences an explicitly supplied list of
unique compiled display addresses and resource header metadata. The trusted raw
record count is checked before allocating collision indexes. Every physical key
is logical magazine 1–8, the exact BCD/raw page byte and the complete transmitted
subcode. Purpose, controls and lifecycle flags cannot distinguish colliding pages.
Resources additionally retain one logical raw-page/S1 owner, independent of kind,
update counter or last-packet `nn`. Different kinds at different S1 remain legal.

The distinction matters after body compilation: an empty DRCS resource on raw
page 200/S1=0 derives `nn=1` and physical subcode `0100`, colliding with display
200:0100. Empty POP/GPOP derives `nn=3`, colliding with display 200:0300. Checking
only logical S1 would miss both. MOT `nn=21` similarly produces `1500` at counter/
S1 zero, rather than physical subcode zero. Nonzero MOT S1 remains supported.

Callers supply each unique page once per generation, including MOTs and explicitly
emitted empty companions; object references, retransmissions and continuation
fragments are not additional inventory entries. Located diagnostics retain the
two original record indices and exact collision identity without authored text.
Eleven regressions use actual DRCS/POP/MOT writer headers and the independent
header reader, covering exact subcodes, counters/last rows, logical owners,
magazine eight, BCD and input-budget precedence.

The fence does not verify body/header correspondence, dependency completeness,
generation ownership, ordering or carousel timing, and returns no publication or
readiness token. Wiring a complete compiled service through this fence remains
required; the standalone semantic service does not synthesize these headers.

## Presentation and supplied service selection

`TeletextLevel25PresentationDefaults` carries an assigned primary designation,
required nullable secondary G0, CLUT2/3, default screen/row colours, black-background
substitution, spacing-colour remapping and panel metadata. All fields are required;
JSON preserves disabled fields and rejects unknown/duplicate/positional shapes.
There is no implicit receiver palette or second-language default.

The eight foreground/background CLUT pairs are 0/0, 0/1, 0/2, 1/1, 1/2, 2/1,
2/2 and 2/3. The raw panel width N is 0–15. For enabled-level panels:

| Left flag | Right flag | Left width | Right width |
|---|---|---|---|
| Off | Off | 0 | 0 |
| On | Off | 16 when N=0, otherwise N | 0 |
| Off | On | 0 | 16−N |
| On | On | N | 16−N |

No flag/width combination is reserved. A right-only panel with N>0 or two enabled
flags with N=0 remains valid. `Level35Only` status suppresses both at Level 2.5;
raw width/status remains available. This supports the metadata, not Level 3.5
rendering. Actual object placement, size and area boundaries remain unresolved.

`TeletextSemanticService::new` receives `TeletextResolvedServiceDraft` and trusted
`TeletextServiceBudget`. It embeds the base pages unchanged, with exact page/raw
subcode metadata, present magazine defaults, object definitions and DRCS subtables.
Page presentation precedes magazine presentation. With neither, header national
options 0–6 select primary codes 0x00–0x06; reserved fallback 0x07 is diagnosed.
Explicit assigned aliases ending in seven are accepted. Language codes do not
silently select another repertoire. Unknown page lookups return no presentation.

The supplied resource selection is already resolved. Optional `compositional_links`
retain a checked X27/4 declaration on the exact display metadata owner, independently
of that selection. Every set mask bit on a real, Level-2.5-applicable target requires
the matching catalog kind/page/S1, including unused records. Null and zero masks
do not invent inventory; Level-3.5-only metadata is retained without requiring
Level 3.5 resources. For an applicable real declaration matching the supplied
selected owner, every effective explicit object invocation and DRCS reference
must use an announced S1. Failures retain the exact display, role, resource S1
and source object/command. Only synthesized direct MOT default-object roots
are exempt; their nested calls and glyph references are checked. Different
selected owners and null links still require MOT/X27 resolution and provenance.
These checks do not derive that selection or establish encoding readiness.
Default slots retain GPOP1,
GPOP2, POP1, POP2 order and sparse ordinals. A present local program, including an
empty one, suppresses every default. Local definitions require an actual base
owner and explicit local declaration. Declared links/default targets are checked
even when suppressed; their unused glyphs/charset designations do not count as
part of the effective page. Default roots retain their original resource owner;
they are not rebound to explicit links. Catalog identity/type and slot scope still
apply. Local and nested explicit calls must match the supplied selected links;
DRCS uses the existing complete-reference validator. Shared traversal budgets
include default roots, without charging suppressed programs.

`TeletextSemanticService::effective_program(address)` exposes that same selection
policy for an exact existing base owner. Local programs remain borrowed, including
empty overrides; defaults and base-only programs are owned synthesis. Unknown
pages or different raw subcodes return `MissingPage`, never an invented empty
program. Retain the source tag: synthesized MOT roots keep their independent
owners and must not be relabelled as ordinary explicit invocations.

`resolve_teletext_colour_context` computes original screen and all 25 row colours
before a composer inserts positioning-only selectors. It uses this effective
program and page-before-magazine presentation. Main colours outrank direct Active
object colours; Active calls retain invocation order and exact row origins/offsets.
Table 28 and clauses 13.2/13.3 permit these colours only in main/Active data, and
valid object type edges prevent nested Active calls. This narrow prepass does not
validate inapplicable colour declarations in Adaptive/Passive bodies; the complete
compiler must still reject them.

Annex A.5 distinguishes explicitly invoked screen colour from default screen
colour. Missing row colours use explicit screen colour, then the selected default
row colour, then black; default screen colour alone does not colour the rows.
Real down-to-bottom assignments update the tail, while this-only assignments
override one row. Main length and every repeated direct Active body are charged
against the trusted inspected-command budget before colour processing. Absolute
colour rows and relative screen order retain original command/object locations
and invocation ordinals on error; they are never clamped. The result is immutable
original colour context, not packet coding, glyph/panel geometry or readiness.
Synthetic selectors must not feed back into this prepass.

Service budgets cover unique resource records, definitions, structural items,
base content bytes, authored commands and aggregate expanded commands/glyph
references across all display subpages. Multiple objects in one resource consume
one resource record; different kinds cannot hide an exact raw-page/S1 collision.
Input bytes remain bounded before parsing. Constructors return no partial service,
and the checked result has no mutable/deserialization/publication bypass.

MOT/X27 selection and default derivation, final display semantics,
complete physical realization/inventory and enhanced encoding remain required later
steps. A display subcode is not a resource logical S1. The existing base encoder
must not be used to silently discard this aggregate's resources or override its
non-Latin presentation. Operational v2 input, editor, preview and publication
interfaces remain open.

## Presentation packet coding and v1 lifting

The native `teletext::presentation` encoder packs the checked presentation into
the 234 data bits shared by X28/0 and M29/0. It fixes the function/coding prefix,
retains primary/secondary aliases and the no-second-G0 sentinel, raw panel fields,
CLUT2/3 RGB order, screen/row colours, substitution and remapping. It is a packet
body encoder, not enhanced-service compilation or rendering. Palette construction
uses the explicitly named `TeletextLevel25Palette::standard()` for table 30;
there is still no implicit presentation `Default`.

`lift_teletext_v1_page_set` consumes the unchanged base draft. Shared record,
structural-item and content-byte preflight precedes structure checks and derived
metadata allocation. Each exact page/subcode receives the old encoder's explicit
Western Latin X28 declaration and `Some(empty)` authored local program, with no
resource links/defaults or invented resources. The final service constructor
rechecks the complete candidate. National option seven retains a located error;
non-representable characters still require later encoding validation.

The old encoder's literal X28 bytes match for all seven national options. Empty
local presence prevents inherited default objects from altering these pages;
explicit page presentation wins over a conflicting magazine theme. Base text,
styles, exact identities and navigation remain intact. Derived base-character
X26 triplets remain necessary even with an empty authored program. Compiler
convergence for mixed v1/v2 producers remains open; using the old base encoder
on an arbitrary enhanced service would silently discard its authored data.

## Monochrome DRCS body coding

`encode_teletext_drcs_bodies` receives a checked `TeletextDrcsSubtable` and returns
its exact resource identity, twenty-four optional row bodies and X28/3. Slots
0–47 select row one plus slot/2 and the corresponding twenty-byte half; input
vector order never renumbers glyphs. Each of the ten twelve-pixel scanlines has
two odd-parity D-bytes in left-to-right order with bit seven set. Defined blank
pixels use 0x40; an undefined half of a present row uses SPACE 0x20. Wholly
undefined rows are omitted. This preserves sparse absence rather than inventing
blank definitions.

X28/3 derives function four/GDRCS or five/DRCS and coding zero. Its forty-eight
mode nibbles retain zero for defined monochrome glyphs and F for undefined slots,
including cross-triplet boundaries. Remaining bits are canonical reserved zero.
Independent parity/data-position tests decode literal corner/asymmetric fixtures,
all 4,096 scanlines, every slot for both purposes, sparse/blank distinctions,
empty resources and unchanged results after reversing input order. These are
body/geometry checks, not actual decoder, header, transport or client acceptance.

Pinned QTeletextMaker 1.0 decodes valid monochrome geometry but its Level 2.5 path
ignores X28/3 modes and does not validate parity or undefined SPACE halves. Use
it only for the applicable valid-glyph proof. The inspected libzvbi source is
0.2.45; the currently installed independent decoder reports 0.2.42. Source audit
and exact-runtime execution remain distinct. The calibration below records
completed MOT-based pixel/fallback proof; full independent feature decoding remains open.
The official pinned editor AppImage was downloaded and extracted for later GUI
verification (SHA256 `d2d77e8285fbb8147fe5014fb7f592f333dfd6bdcbad4b831197f1ad7a3a04a7`);
this preparation does not prove an operational editor or display workflow.

## Resource headers and complete MOT/DRCS sequences

EN 300 706 Annex A.1 represents physical resource subcodes as
`(nn << 8) | (S2 << 4) | S1`. Logical S1 remains the authored resource identity;
S2 is an explicit modulo-eight update indicator. `nn` names the last data packet
in required transmission order: rows 1–25 or X26 designation 0–15 as values
26–41. X27/X28 precede these and do not increase `nn`. POP/GPOP permit 1–41;
DRCS/GDRCS/MOT/MIP use rows 1–24. A partial update declares its own final packet;
continuations retain that declaration. MIP requires S1 zero; MOT zero is advisory.
Checked metadata preserves all six Annex B table B.1 states:

| Transmission | C4 | C8 | C12 | C13 |
| --- | --- | --- | --- | --- |
| Unchanged full first | 0 | 0 | 0 | 0 |
| Unchanged continuation | 0 | 0 | 1 | 0 |
| Changed full first | 1 | 1 | 0 | 0 |
| Changed continuation | 0 | 1 | 1 | 0 |
| Partial first | 0 | 1 | 0 | 1 |
| Partial continuation | 0 | 1 | 1 | 1 |

C14 is reserved here; C12/C13 are not national-option bits. First changed/partial
transmissions advance S2; continuation fragments retain it. Metadata construction
alone cannot validate previous content/counter, fragment continuity or actual body
extent. Service C11 and other controls/text remain outside this domain type.

The native full-page writers derive actual `nn`, preserve exact raw identities
and emit bounded immutable DVB-unit sequences. MOT emits its header, optional
X28/0 and seventeen rows ending in row 21. X28/0 declares function 6/coding 3 and
Table 6 compatibility bits 8–18 as `0x0ff80`; the descending bit notation in
[ETSI TR 100 287 V1.2.1 section 10.6.3.1](https://www.etsi.org/deliver/etsi_TR/100200_100299/100287/01.02.01_60/tr_100287v010201p.pdf)
clarifies their order. DRCS emits header, X28/3, then ascending defined rows.
An empty authored subtable deliberately emits one SPACE-only row and all-F modes,
giving legal `nn=1` without inventing a blank glyph. At most 19 MOT or 26 DRCS
units are allocated. Supplied default-allocation failures return no encoded page.
The writer uses C5=C6=0, C7=C9=C10=1 and blank header text as suppression policy;
these are not universal import restrictions. Serial/parallel C11 is explicit.
A literal changed DRCS 8A0:S1=F, `nn=24`, S2=0, serial header has wire bytes
`A8 31 57 0B 0B 40 57 40`, physical subcode `180F` and raw controls `F9`.

Sequences exclude the closing next-page/FF header and carry no publication
capability. Complete service compilation must enforce one service-wide C11,
content/counter transitions, resource allocation and Annex B update ordering:
MOT, DRCS, objects, then display. Full enhancement/M29 repetition is at most
20 seconds; successive packets within a page are at most 100 ms apart. MIP's
20 ms erasure spacing is separate. Fragment assembly retransmits X27/X28 before
remaining ascending content. Current writers assemble full MOT/monochrome DRCS;
partial/fragment scheduling remains open, without declaring legal states unsupported.

Static inspection of libzvbi 0.2.45 identified full-subcode partial-update lookup
and decimal-looking resource cache behavior that need exact-version calibration.
These static cache findings still need additional exact-runtime calibration and
do not justify restricting legal resource addresses. The full-resource proof below
does not settle partial/multi-version cache behavior. Full independent feature
decoding, suitable-client display, enhanced compilation/carousel/transport and
editor/publication acceptance remain open.

## Pinned independent resource decoding

The opt-in Linux x64 calibration uses the installed public libzvbi API at
version 0.2.42, checked through `vbi_version`. A fresh decoder per case registers
a retained teletext event callback, feeds the actual authored resource units,
fetches the exact display page at Level 2.5 and checks baseline text independently.
The fetched page stays alive while its referenced glyph buffer is checked and
rendered through public PAL8 drawing; it is unreferenced before decoder deletion.
Native ABI sizes/offsets and Linux x64 little-endian layout are explicitly guarded.
The existing bounded subprocess runner limits time to ten seconds and output to
16 KiB, with separate bounded stderr. The helper bounds input to 2 MiB, sixteen
cases and 256 units per case, with exact fields/byte/bitmap validation.

| Cases | Exact evidence |
| --- | --- |
| Global slots 0/1, including S1=15 | Corner/asymmetric pixels, exact scope/S1/slot and native cell rendering |
| Global slot 46 / normal slot 47 | Final packet/half, left/right pixel orientation and native rendering |
| Defined blank | A real DRCS character and blank glyph buffer, distinguished from base fallback |
| Missing row / SPACE-only half | Mode zero retained; missing bitmap alone preserves the base character |
| Bad parity | Defined mode zero; only one D-byte parity bit changes |
| Undefined mode | Valid bitmap bytes retained; mode F alone preserves the base character |

Five positive cases check every one of the 120 pixels and corresponding PAL8
output. Four negative cases require exact fallback T; all nine also require the
independent base calibration B. MOT extents, transmitted subpages and advertised
counts agree, including all sixteen S1 companions where S1=15 is selected.
Resources and local display-invocation bodies now come from production encoders.
The regenerated nine-case fixture is byte-identical to the previously calibrated
synthetic fixture. This proves these lowering paths, not complete service compilation.
The test replaces the old base X26/0, uses Level 2.5 DRCS selection and local
termination, and closes each resource within its own magazine.

Exact source audit: official libzvbi tag v0.2.42,
commit `3785481f51f41a49e28f2b7f6fd5bd9187d24ae1`. Audited files are
[packet.c](https://github.com/zapping-vbi/zvbi/blob/3785481f51f41a49e28f2b7f6fd5bd9187d24ae1/src/packet.c),
[teletext.c](https://github.com/zapping-vbi/zvbi/blob/3785481f51f41a49e28f2b7f6fd5bd9187d24ae1/src/teletext.c)
and [the public header](https://github.com/zapping-vbi/zvbi/blob/3785481f51f41a49e28f2b7f6fd5bd9187d24ae1/src/libzvbi.h).
Their SHA256 hashes are respectively
`e41fa3c60308599ab6a008da21b793f18e86a5011fac32fe6f84befc9dd6db18`,
`c0920093077fe80924e483bd6a61c76418b7c20f0418dcfc6b148b1986fae4b8` and
`dc3cd31dbf5fab7a6d3d238fc3e3bc1a249546d87e3195cdd2f4678d72db61f0`.
Source-tag correspondence is audit evidence, not certification of the installed
binary's build. The public header layout matches audited 0.2.45 apart from version.

Two limitations are separately observed/audited. Version 0.2.42 does not finalize
a C4-set resource on a direct cross-magazine transition; own-magazine FF:0 closure
makes MOT/DRCS decoding pass without changing resource bytes or lifecycle flags.
Normative X27 linking still fails after that closure. Table 15's zero-based field
shifts are 6/11/14/2 for units/relative magazine/tens/S1 mask; inspected 0.2.42
and 0.2.45 extraction uses 7/12/15/3. The literal global link `29406/00004`
means 8A0/mask1 but the inspected decoder reads 358/mask0. Inspected normal DRCS
rendering also uses the POP link slot. Preserve the normative encoder; these
observations do not restrict legal authored addresses or declare X27 unsupported
by the engine. X27 external decoding, partial/multi-version cache behavior and
full suitable-client feature coverage remain acceptance work.

Run/export/replay instructions are in the
[resource calibration workflow](../../scripts/e2e/README.md#teletext-resource-decoder-calibration).
This proves representative independent decoding and offscreen native glyph
rendering. It proves neither actual client display/navigation nor TS delivery,
full Level 2.5 support, the QTeletextMaker GUI workflow or design publication.

### Pinned independent object decoding

The separate object helper reuses the frozen resource helper's public ABI and
runtime-version guards. Eight fresh-decoder cases consume complete production
POP/GPOP pages, production resource-aware local invocations and normative MOT
links. They cover all six public/global active/adaptive/passive combinations and
two active-to-adaptive-to-passive chains. Full semantic IDs `FFFF`, `0100` and
`0000` remain distinct; logical S1 endpoints zero and fifteen are represented
with complete declared companion extents. Own-magazine FF closing headers retain
the unmodified serial resource bytes.

The oracle independently checks exact formatted Unicode, foreground/background
colours, nested relative origins and an unchanged base B. Adjacent base T cells
distinguish active colour propagation from adaptive/passive containment. This
checks actual decoder formatting, not font pixels or full object inheritance.
Incorrect glyph/colour expectations fail; unknown/duplicate input members,
duplicate cells, boolean integer fields and oversized input are rejected.

The final eight-case fixture SHA256 is
`77f16d8e7723a171b4847c94a20667fe3054a61f8daf26a5f08d6468b1347ce6`;
the helper SHA256 is
`d8d7001f6df374614b95c59a413b07f60af4038089bed1811c12abbe2cf6c6f1`.
Thirty-four focused local/allocation/object-page regressions include eleven new
independent byte-oracle tests, all pointer banks/types/S1 values and exact body
capacities. Native object proof covers the first pointer bank only. Higher-bank
independent decoding, local objects, full composition, transport and actual
suitable-client navigation remain required acceptance work. The separate
[object calibration workflow](../../scripts/e2e/README.md#teletext-object-decoder-calibration)
exports and replays the synthetic cases; it has no production runtime dependency.

### Local-object decoder limitation

The correct local-marker conformance oracle fails with actual libzvbi 0.2.42
for all three direct local object types. At row 4, column 8 it expects authored
A/red/blue but retrieves base T/white/black; the separate base B still decodes.
The pinned fault reproducer requires that exact fallback for both checked cells.
Its successful diagnostic means the decoder limitation was reproduced, not that
local-object rendering was accepted. Correct conformance expectations are exported
separately, with byte-identical production units. Neither compiler pointers nor
the required rendering acceptance criterion are changed.

The diagnostic fixture SHA256 is
`850bf27926b478e254ec44737f34e86e4d0373762f2d2b2821686035a094eb6e`;
the correct conformance fixture SHA256 is
`11bfc143b8ad25d66433b7e8235cdede3fc39ab4cddae685547eb2c6c09bfe2c`.
Both frozen decoder helpers remain unchanged. The existing nine DRCS and eight
resource-object positive cases still pass with their previous fixture hashes.

The audited .42 `teletext.c` local branch passes the definition marker to the
enhancer, whose definition handler stops; the resource resolver instead advances
past its checked marker. This explains the observed direct-local failure. The
same source has an incorrect high-designation shift and rejects nested local
invocations. Those additional limits are source findings, not runtime acceptance
evidence. The engine does not ban these standard features.

### Second independent local-object decoder

The unchanged correct three-case conformance fixture also runs through
[teletextimager 0.2.0](https://github.com/gkthemac/teletextimager/tree/2b0125b778c45ffdfa2df0b196870c093361f317),
pinned to commit `2b0125b778c45ffdfa2df0b196870c093361f317`. Its own T42 reader
decodes packet addresses and Hamming triplets before its independent Level 2.5
formatter executes the local definitions. The adapter only removes DVB framing
and reverses transmitted bits; it does not supply decoded words, object positions
or host semantics to the oracle. Each case uses a fresh reader and decoder.

All three direct local types produce A/red/blue at row 4, column 8. The adjacent
T inherits red/blue only for active objects; adaptive/passive objects retain its
white/black base attributes. The separate B remains unchanged. The original
conformance request and production unit hashes are preserved, including its
`expected_version: "0.2.42"` provenance field. This field identifies the original
libzvbi fixture; the second helper independently guards and reports its own
0.2.0 source version and exact commit. Both frozen libzvbi helpers remain unchanged.

The opt-in helper requires an explicitly supplied external checkout and verifies
SHA256 hashes of every imported upstream source before loading it. It never
downloads, installs, edits or activates a package. The cell decoder needs only
Python's standard library; no Pillow renderer is used in this proof. The existing
bounded subprocess runner retains its ten-second deadline and output limits.
Wrong glyph/colour expectations and changed decoder sources must fail.

Three additional production-generated cases now decode a passive definition at
absolute marker 104 (designation 8/slot 0), marker 195 (designation 15/slot 0),
and a local Active-to-Adaptive-to-Passive chain with full IDs `FFFF/0100/0000`.
The chain's relative origins place A/red/blue at row 4, column 8 while the adjacent
T and separate B retain base attributes. High-marker fixtures use AP-neutral screen
commands before the original invocation; no encoded selectors or definition bytes
are patched by the test. The advanced fixture SHA256 is
`57bf567c2dd894bf884d2435a9932738ca972c7fb3cfd16a77509801eceaece3`.
The original three-case conformance fixture and both libzvbi helpers remain unchanged.

This is representative independent local-object decoding, not interactive client
or font-pixel evidence. The upstream README documents missing non-Latin adaptive/
passive G0/G2 support; inspected code also leaves side-panel sizing work and uses
explicitly supplied external resources rather than automatic MOT/X27 routing.
Complete local coverage, resource pointer extensions, all required
repertoires, base/enhancement composition, transport and actual suitable-client
display/navigation remain separate acceptance work. The libzvbi fault diagnostic
retains its original meaning. See the
[second-decoder workflow](../../scripts/e2e/README.md#teletext-second-local-object-decoder-calibration).

## MOT/X27 preservation audit

EN 300 706 sections 9.6.2, 10.6 and 13.8 require distinct source declarations
before adapting to supplied resolved selections. A present X27/4 carries four
fixed roles (GPOP, POP, GDRCS, DRCS), level applicability and individual 16-bit S1
requirement masks. A real target with mask zero requires no S1; it never means
all subpages. Relative magazine coding XORs three-bit magazine addresses; absolute
code zero denotes magazine eight, while relative zero leaves the owner magazine
unchanged. Packet absence and an explicit FF/FFFF null record remain distinct.
MOT link selectors, contiguous maximum-S1 extents, supersedable flags, fallback
fields and two default-object slots retain their original MOT/resource ownership.
Semantic object IDs are not limited to physical default pointer indices.

Do not erase masks, automatically rebind defaults to an overridden resource or
select the first/S1-zero MOT record implicitly. Supplied explicit object calls
check selected page equality and, when the applicable real X27 target matches,
used-S1 mask membership. Authored resolution must still derive and prove the
binding while retaining independently sourced default provenance. Any local
X26 presence suppresses MOT defaults.

The implemented `TeletextX27Level25Links` has four required named links. Each
checked link carries an explicit target, validity and `TeletextSubpageRequirementMask`;
strict map-only JSON rejects unknown, duplicate, positional or missing fields.
Null masks must be FFFF; valid real masks include every 16-bit combination.
Address-purpose checks apply even to unused or Level-3.5-only metadata.

`encode_teletext_compositional_links` encodes the complete pre-bit-reversal X27/4
body. First-four roles, compatibility bits, target nibbles, XOR magazines,
validities and all mask bits are independently decoded in tests. Reserved bits
and triplet 13 are zero by writer convention. The two additional Level 3.5 slots
use pinned-QTeletextMaker-calibrated null GPOP/POP declarations; this encodes no
authored Level 3.5 extension. Body coding is not carousel integration, resource
allocation, transport conformity or actual display/navigation proof.

`TeletextMotLevel25Declaration` preserves the explicitly selected MOT resource
identity, including nonzero S1, ordered unique raw associations and the complete
Level 2.5 link bank. Up to 256 raw slots are checked before map construction;
FF remains preserved reserved metadata and `association(0xff)` returns none.
This validated-record bound is not a raw JSON parsing/RSS bound. Each object and
DRCS bank has one global and seven public/normal links. `TeletextMotSubpageExtent`
retains a contiguous zero-to-maximum range, separately from arbitrary X27 masks.
Selectors retain independent global-use bits, describing enhancement resource
use rather than defaults alone. The declaration performs no resource selection.
Coverage bit zero denotes magazine eight;
other bits denote magazines one through seven and affect accounting, not inheritance.

Default slots distinguish active semantic identities from dormant FF-link
selectors and type-zero ignored pointer bytes. Active targets must match the
original linked scope/kind/page and extent; semantic IDs retain their full 16
bits. Ignored selector bytes, raw POP protocol/panel/substitution bits and
reserved GPOP fallback bits are preserved, as are explicit coverage/page totals.
Nested map-only JSON requires every declared field and nullable selector;
unknown, duplicate, positional, malformed or ownership-breaking input is rejected.
No default definition existence/type, inventory or computed-count proof is implied.

`encode_teletext_mot_bodies` emits seventeen complete pre-bit-reversal,
Hamming8/4-protected bodies: rows 1–14, 19, 20 and 21. Figure 20 uses twenty
association pairs in rows 1–8, eighteen in rows 9–13 and six in row 14; the FF
pair remains reserved metadata. Tables 21/23 fix absolute magazine addresses,
original default types/selectors, extents, flags and counts. Supplied active
allocations must match exact semantic identity/type and logical S1 before any
body is returned. Missing, unexpected and mismatched allocations report the
MOT identity, link ordinal and default slot. Selectors never truncate host IDs.
All 256 selector bytes are physically assigned; their lower four bits are S1.
Actual emitted POP pointer/program realization is a separate compiler obligation.

Missing sparse associations and reserved padding use canonical zero nibbles.
This authoring output is not a byte-lossless raw importer; ignored padding needs
an explicit external preservation format during interchange. Level 3.5 banks
and optional MOT X28 function-six/coding-three declarations are not emitted here.
Header/update/partial lifecycle, MOT selection, transport/carousel and actual
client proof remain open. A body-only API carries no publication authority.

`TeletextMotLevel25Resolution::resolve` derives four fixed role results for an
explicitly selected same-magazine MOT and exact display/subcode, retaining its
logical S1, copied full bank, actual optional BCD association and original X27.
Missing sparse association means no MOT selection; raw hexadecimal/FF metadata
cannot shadow a decimal display. Absent or Level-3.5-only X27 retains associated
MOT selection. Applicable real X27 selects its exact target/mask when no active
associated MOT exists or its supersedable flag is set. Same-page flag zero
preserves the MOT owner and exact X27 mask with the original extent retained
separately, never intersected.

Different-page flag zero and null over associated active MOT return `Unresolved`
with both original candidates. Cleared global association diagnoses only active
original global defaults, including when X27 selects a global page. Ignored and
dormant selectors stay preserved without false conflicts. Global and associated
public default sources remain independently owned; even an associated unused
POP link retains dormant defaults and conditional fallback flags. These flags
assert no activation; effective application still requires valid linkage and
presentation context. No local-X26/X28/M29 presence is supplied, so the result
cannot decide default suppression or presentation. No catalog acquisition,
readiness or publication authority is granted. Role results and two default
conflict slots are fixed-size; association lookup scans at most 256 records.
The immutable authored declaration remains authoritative for interchange.

Null-X27 fallback, conflicting MOT supersedable flags and global-default
association and mixed-default nested-call edge cases still need a documented,
independently calibrated policy.
The inspected libzvbi 0.2.45 source falls back to MOT on null X27 and lacks complete
global-default handling; the pinned QTeletextMaker source can remove an all-null
packet. Neither is a complete oracle for these cases. Preserve the declarations
while resolving this uncertainty; do not count transport or partial decoder
checks as complete authored/display acceptance.

## Standards audit

Basis: [ETSI EN 300 706 V1.2.1 (2003-04)](https://www.etsi.org/deliver/etsi_en/300700_300799/300706/01.02.01_60/en_300706v010201p.pdf).
Clause references below identify encoding/decoding requirements, not delivered
capability. Optional use in an individual page does not limit the engine target.

| Feature | Relevant clauses | Required implementation |
|---|---|---|
| Base/fallback | 10.1, 12.1–2, annexes A–D | Correct base rows, G0/national/G1, spacing and update/order semantics. |
| Level 1.5 | 12.3.4, annex E.3 | G2/diacritics and applicable G3 fallback. |
| Extended repertoires | 12.3.4, 15 | G0/G2 designation: Latin, Cyrillic, Greek, Arabic and Hebrew. |
| Non-spacing attributes | 12.3 | Colour, flash, size, boxing, conceal, inversion, underline and row/screen attributes. |
| Palettes | 9.4.2, 9.5.1, 12.4 | 32 colours; fixed CLUT0/1, editable CLUT2/3, remapping/substitution and page/magazine precedence. |
| Objects | 9.6.2, 10.5–6, 13 | Local/public/global, active/adaptive/passive, links and MOT precedence. |
| DRCS | 14.1, 14.4.1 | 12×10×1 glyphs; two 48-glyph subtables, at most 24 selected per display page. |
| Side panels | 12.5 | Object-based 16-column side-panel extension and fallback. |
| Level 3.5 rejection | 12.1, 14.4.2 | Targeted rejection of multicolour DRCS, font styles and CLUT0/1 redefinition. |

Annex E is informative; its encoding guidance limit is not a normative service
limit. Charset, resource, fallback and client coverage must have explicit test
cases. A report that FFmpeg contains libzvbi does not prove this matrix.

## Pinned QTeletextMaker source audit

Audit target: [QTeletextMaker 1.0](https://github.com/gkthemac/QTeletextMaker/releases/tag/1.0),
commit `cac2167bac933a6160cca45a45d5d4ee95355898`. This is source inspection,
not a downloaded-artifact check or actual editor roundtrip.

The [loader](https://github.com/gkthemac/QTeletextMaker/blob/cac2167bac933a6160cca45a45d5d4ee95355898/src/qteletextmaker/loadformats.cpp)
interprets DE/PN/PS/RE/CT/FL/OL. SC handling is disabled. It uses the first page
number, treats later PN records as ordinal subpages and ignores their addresses.
Custom records and PF/DS/SP are not retained. Blank lines stop loading; short
rows/packets can be padded. M/29 is imported as X/28.

The [writer](https://github.com/gkthemac/QTeletextMaker/blob/cac2167bac933a6160cca45a45d5d4ee95355898/src/qteletextmaker/saveformats.cpp)
uses CRLF and assigns ordinal subcodes: zero for one subpage, 1…N for several.
FLOF X/27/0 becomes page-only FL links, losing exact-subpage targeting. Unknown
records disappear. These behaviors require host identity/navigation reconciliation.

[Document conversion](https://github.com/gkthemac/QTeletextMaker/blob/cac2167bac933a6160cca45a45d5d4ee95355898/src/qteletextmaker/document.cpp)
rebuilds enhancement fields; semantic equivalence does not imply byte-preserving
packet export. Palette/side-panel/local-object editing exists, but
[documented limitations](https://github.com/gkthemac/QTeletextMaker/blob/cac2167bac933a6160cca45a45d5d4ee95355898/README.md)
include skipped POP/GPOP invocations. The
[DRCS workflow](https://github.com/gkthemac/QTeletextMaker/blob/cac2167bac933a6160cca45a45d5d4ee95355898/examples/Level2p5-DRCS/README-drcs.md)
loads dependencies separately and does not provide bitmap editing. Resource-page
classification and unsupported editor transformations must be checked explicitly.
QTeletextMaker alone cannot establish the full enhanced broadcast-client gate.

## Planned project bundle

Export selection includes a page, selected subpages or a supported complete set.
The bundle contains `ersatzrs-teletext-project.json`, editable TTI documents and
required object/DRCS assets. One editable TTI file maps to one stable page/subpage;
the editor may write subcode zero while the manifest retains the exact identity.
Do not embed the only copy of metadata in custom TTI commands.

Sidecar `schema_version = 1` and `editor_profile = "qteletextmaker-1.0"` must declare:

- Model/interchange/editor profile versions and a host-verifiable export receipt.
- Exact page/subcode-to-file mapping, entry/language/region and navigation.
- Resource kinds, dependencies and semantic baseline hashes.
- Addon/template/package identity and version, profile revision and selected theme.
- Supported overrides, named zone boundaries, dynamic field bindings and generated
  regions, with distinct free/zone/generated edit permissions.

All uploaded metadata is untrusted. Validate paths, file/count/byte/resource bounds,
identities, schema versions and dependencies before allocation or publication.
Compare authority/revisions with the host, not the sidecar's own declarations.
Missing sidecar data cannot confer addon-template edit permission. Ambiguous new/
reordered subpages or changed exact links need explicit resolution, never guessed
matching. Resource editing that the pinned editor cannot preserve must be refused
or use a separately validated structured resource file; never claim a lossy export.

The host export receipt binds the design ID, immutable revision/digest, exact
relative file mapping, resource baseline and edit policy. A changed sidecar cannot
grant new permissions. Durable draft saves require the expected head revision;
stale imports and validation failures retain the previous head and broadcast.
Removing an addon/profile retains designs and receipts while making their
configuration context stale. Ordinary playback does not depend on draft storage.

The enhanced addon boundary is `teletext.pages.v2`: version-2 requests retain the
same bounded guide fields, and version-2 responses carry mandatory base pages plus
typed enhancements/resources. Existing version 1 remains unchanged. The host
selects the highest mutually supported declared version from its installation
snapshot and checks the exact response version without fallback after failure.
Mixed-version producers receive identical guide values with their selected
request version; v1 contributes base pages without enhancement loss. Semantic
resource identities remain separate from compiler-owned physical header fields.

## Planned validation and evidence sequence

The same application functions serve admin and structured agent workflows:

1. Select design/profile and exact export scope; save TTI plus sidecar/dependencies.
2. Verify and record the actual editor artifact/version; visibly open/edit/save
   editable files in QTeletextMaker, retaining the sidecar and protected assets.
3. Import into a separate draft; report normalization, unsupported features,
   identity/dependency conflicts and edits outside free/declared-zone boundaries.
4. Validate the complete typed model and native encoding, with page/subcode/position
   and relative file/line diagnostics. Preview enhanced and base views from it.
5. Persist the draft separately. Publish only by explicit action with fresh
   configuration, package and generation checks. Import cannot alter transmission.
6. Capture the actual published output. Run TSDuck, independent decoding and real
   display/subpage/navigation in a demonstrably suitable client.

Agent inputs/outputs must be bounded files and structured diagnostics using the
same model and edit permissions. Provide documented validation/export/import/
preview entry points when implemented; do not invent currently runnable commands.
Template/package edits use the explicit addon version workflow. Dynamic guide
content remains generated, and profiles/Lua retain their existing scope/timing.

Record host/package/model/editor/client/decoder versions, artifact hashes, feature
fixtures, results and limitations. Localized headed Chromium/Firefox, hydrated/
no-JS and light/dark evidence is required for changed admin. A valid file, a draft,
a browser preview, a transport check and a published client result are separate
claims. Kodi is optional; TSDuck alone does not prove Level 2.5 display.
