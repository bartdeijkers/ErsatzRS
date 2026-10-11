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

### Unicode/Active wire priority: original failure and normalized candidate

The opt-in `base_unicode_active_override_decodes_with_pinned_imager` regression
originally used actual `compile_teletext_composed_main_program` output, including final
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
unchanged. The positive regression now explicitly chooses
`compile_teletext_normalized_main_program`; it retains German option one, the
same original rows/object and the same two literal cells. It passes both levels
in `af44fcf5-1934-4899-8d06-59172060d9f1`. The new request SHA256 is
`9f54ec86a2c7a15927916b83d97493b03842658015b18066c3a8d60379419756`.
The original failing artifact remains historical evidence for the ordinary
composed path; this result supersedes it only for the explicit normalized route.
Export or replay the current positive regression with:

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
composition path was recorded failing above; the later explicit normalized route
passes that unchanged positive oracle, without proving general client acceptance.

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

`compare_teletext_augmented_origin_glyphs` accepts only the private paired compiled
candidate and scans the complete selected visual plane under record/cell/diagnostic/
work/storage bounds. Generated glyph roles map to exact original display origins,
including repaint origins sharing a candidate; retained emitters use original
provenance and checked invocation maps. Compare actual assigned G0/G2 repertoires,
composite base/mark context, special `@`, mosaics, G3 and DRCS identities. DRCS also
requires equal full resolved resource identity and bitmap. Base Unicode uses the
same extracted explicit-Latin replacement identity as the existing mask helper;
physical Base still uses coded fallback. Equal masks are never identity evidence.

Located typed findings keep original/actual source, root, candidate, origin and
instance spaces separate. Current selected pending and full append-only histories
remain distinct. This does not replace the whole-page fragment/attribute/mask/C.1
comparison or certify service generations, nonvisible state or receiver readiness.
Seven tests prove Unicode replacements/Active collision, equal-mask Latin/Cyrillic
failure, changed G2 context, nested/repeated/sparse roots, budgets/history and equal
DRCS pixels with distinct scope/S1/slot/resource. The DRCS identity fixture uses
relative row1 to reach the comparison; row-zero lowering remains required/open.
The pinned native fixture now passes both full1,000-cell typed and selected-model
comparisons before unchanged literal dual-level decoding.

`compile_teletext_normalized_main_program` now accepts only one immutable service,
exact selected owner and per-stage host budgets. It reuses the existing composed
factory and consumes its original final-word C.2-settled target once. Internally
it derives the plan, placement, augmented catalog/program, allocations, physical
encoding and paired ancestry. No caller plan, map or detached trace can substitute
another generation. Both original and augmented allocation/packing passes are
explicitly bounded. Complete typed and selected-model comparisons must succeed
before the owned original/physical result escapes; located failures return no
partial bytes. Histories survive and no-seed pages still undergo both comparisons.
This is an explicit engine candidate, not service or durable publication admission.

Eight tests cover owned lifetime, original fallback bytes, no-collision replacement
and C.2 settlement, exact subcode lookup, no-seed typed/global uncertainty, both
allocation limits, stage cutoffs and foreign sparse-slot default rejection
(`3b726d74-8d3c-4f19-a7d6-572598b41bcf`). The original Unicode/Active positive
oracle now passes via this route; three unchanged native Local/Composed/C.2 cases
also pass (`74289bc4-52de-421f-b409-07cea2f6597c`). The pinned decoder/helper and
literal expectations are unchanged. Foreign MOT/default activation cannot be
fixed by cloning owners or relaxing selected links; row-zero lowering and complete
standards/client/editor/localized preview/publication acceptance remain open.

### Redundant row-zero position lowering

The shared lowerer accepts an exact `SetPosition(0,0)` identity only when the
enhanced active position already equals `(0,0)`. It emits no triplet or colour
writer and preserves legacy compatibility-row state plus both DRCS latches.
Omit its emitted command-position record rather than assigning an earlier word;
retain subsequent authored ordinals. Actual pointer and packet-boundary sizing
use zero emitted words while raw command budgets still count the command.
This does not implement arbitrary header positioning or later row-zero resets.

Eight new tests cover termination-only encoding, all local object types and empty
bodies, packet-boundary pointers, exact sparse source/word records, raw-command
versus physical capacity, header colour and both DRCS latches, unchanged located
failure state and sparse actual-word C.2 settlement. All534 nonignored FFmpeg
teletext tests pass (`3472327d-b9ed-4a62-9b11-77efc0ceb789`), including existing
resource/transport/snapshot/normalization regressions. Strict FFmpeg all-target
Clippy and workspace formatting pass; independent source/oracle review is clean.
The previous architecture-lint evidence remains applicable: no boundary,
dependency, application or admin source changed.

Pinned teletextimager 0.2.0 at `2b0125b778c45ffdfa2df0b196870c093361f317`
passes six implicit/explicit cases across Active/Adaptive/Passive objects after
complete production DVB-unit equality checks and unchanged literal dual-level
`ZZ`/`AZ` cells. The normalized Unicode/Active oracle remains green in the same
run (`a1997691-fe69-4391-836a-e26d51fa28ba`). New request SHA256:
`2b85bc263c7539ede97e632eda8a1a6e0af6c78978721d947a12c1e9e1b5581a`.
Export or replay this synthetic proof with the unchanged pinned helper:

```bash
ERSATZRS_TELETEXT_TEST_IMAGER_DIR=<pinned-checkout> \
ERSATZRS_TELETEXT_TEST_ROW_ZERO_IMAGER_FIXTURE=<request.json> \
./run.sh test -p ersatzrs-ffmpeg --lib --run-ignored ignored-only \
  -E 'test(redundant_initial_object_position_decodes_without_wire_or_colour_changes)'
```

Nonidentity row-zero lowering, foreign defaults and full standards/client/editor/
localized preview/publication acceptance remain required. This test does not
prove client navigation, complete row-zero support or whole-service readiness.

### Foreign defaults and physical activation

[EN 300 706 §13.8 and E.11](https://www.etsi.org/deliver/etsi_en/300700_300799/300706/01.02.01_60/en_300706v010201p.pdf#page=101)
requires suppression of MOT defaults whenever local X26 is present, including
compatibility-only words. Table 28's object invocation carries scope/type/S1 and
pointer selection, not a page number; actual selected resource links determine
its owner. Current rejection of foreign defaults on the explicit-call path is
therefore correct, but does not fulfill authored support for those pages.

The pending implementation needs a no-X26 native activation path and a separate
X26 realization path. Native activation must retain exact MOT slots, full original
owners and nested selected-resource context. X26 may directly call selected roots;
foreign contributions require producer-owned physical realizations with original
root/instance provenance. Do not clone Local semantic owners or waive link checks.
Realization must cover all affected origins, attribute-only and charset effects,
row/screen planes, panels and header contributions, not just Unicode/Active seeds.
Prove sparse/repeated public/global defaults, mixed foreign roots/nested selected
calls, compatibility-only suppression, wrong-link rejection, unchanged fallback,
complete typed/structural/temporal comparisons and independent decoding. These
paths and their whole-service transport/publication acceptance remain open.

### Native default display packets

`encode_teletext_native_default_page` consumes one immutable semantic service and
exact display page/subcode. Its owned result preserves original sparse default
identities, header/kind/language, editorial links and all fallback rows without
inventing local X26. Only actual page declarations emit X27/4 and X28/0. Original
M29/0 is returned separately, including when a page declaration overrides it.
Missing owners, any present local program (even empty) and a first base replacement
requiring X26 reject before output with located diagnostics. These are routing
conditions for this primitive, not accepted limitations on authored Level 2.5.
Reuse the ordinary packet writer; the legacy encoder's units remain identical.
This primitive does not generate MOT/resources or certify complete service
inventory, generation, transport scheduling, admission or publication.

Eight new tests pass (`280e1ba1-d5e9-40ae-9330-baf82a0ea4ff`), including an
independent legacy-byte comparison across seven national options and three page
kinds, sparse foreign defaults, actual declaration presence and failure locations.
The first run passed all534 existing teletext tests plus seven new tests
(`1ee16c40-8a06-4773-ab3c-659ef7f1267d`); its remaining new fixture needed an
actual selected-resource catalog entry. That fixture is corrected without
relaxing resource validation or its wire expectations. The final eight-test run
is the evidence for the corrected fixtures; the unchanged534-test baseline is
reused after independently verified mechanical metadata borrowing.

Pinned Linux x64 libzvbi 0.2.42 independently passes two production root-only
cases: sparse second-slot Active default from original A0 despite actual X27/4
selecting A1, with literal `A/C/SPACE` glyphs/colours; adding one production empty
local X26 packet suppresses defaults and leaves `T/T/SPACE`. Native run
`b2b82489-c621-4fb1-accd-df90c1326ba4` has **one passing test and one failing
conformance test**, not a green suite. The unchanged mixed nested-selected case
still expects `A/C/D`, but the known shifted X27/4 parser reads correct triplets
`0x29445/0x0000c` as page358/mask1 instead of 8A1/mask3. Failed nested lookup
restores the whole fallback page. Keep its normative bytes and positive oracle:
neither a shifted-wire workaround nor a passing root-only case proves this case.
Pinned teletextimager 0.2.0 has no MOT-default activation or selected X27 resource
handling and cannot supply this proof unchanged. The test-corrected independent
C-decoder proof below now covers this mixed case; actual client display/navigation
remains open.

Production root-only request SHA256:
`2759c8a3b45f0df2fe6beab77a39b5af432369b4f89ebdf2b93a72add762c9cd`.
The original mixed request remains exactly:
`98d8416f14be48c760473a39e73f48b5934fcb91e8478a3ea6478fd7b7262db2`.
Direct replay passes the two root-only cases and fails the original mixed case.
The unchanged object helper remains
`d8d7001f6df374614b95c59a413b07f60af4038089bed1811c12abbe2cf6c6f1`;
its unchanged resource/ABI/version helper remains
`615bc0ceb7b24738e7cb7e8a6df076c8b58eb9968ecdc7e07d226f76f5d7a76d`.

```bash
ERSATZRS_TELETEXT_TEST_NATIVE_DEFAULT_ROOT_FIXTURE=<root-request.json> \
./run.sh test -p ersatzrs-ffmpeg --lib --run-ignored ignored-only \
  -E 'test(native_foreign_mot_root_only_and_local_suppression_decode)'
python3 scripts/e2e/teletext-object-decoder.py < <root-request.json>
```

Export the unchanged mixed conformance request with
`ERSATZRS_TELETEXT_TEST_NATIVE_DEFAULT_FIXTURE` and
`test(native_foreign_mot_default_and_nested_selected_resource_decode)`; this
explicit opt-in test fails under 0.2.42 and is not acceptance evidence.
Eight existing resource-object native cases pass
(`3fcd07e7-dfb5-4821-b738-a50c658a4a64`). Three relevant pinned-imager tests
pass (`fa918e08-cece-4492-b651-662df4e35b88`), including the six row-zero
implicit/explicit cases and normalized Unicode/Active oracle. Final strict FFmpeg
all-target Clippy, workspace formatting and independent delta review pass.
No crate boundary, dependency, application or admin source changed; prior
architecture-lint evidence remains applicable. All milestones remain open.

### Test-corrected X27 independent decoder

The maintained [offline build and replay workflow](../../scripts/e2e/README.md#teletext-x27-test-decoder-comparison)
compares unmodified official libzvbi 0.2.42 source at
`3785481f51f41a49e28f2b7f6fd5bd9187d24ae1` with a **test-only corrected**
build. Only three `parse_27` lines change: Table 15 magazine shift 12→11,
masked page-tens shift 11→10, units shift 7→6 and subcode-mask shift 3→2.
Neither production wire bytes, expected cells nor the frozen Python/ABI helpers
change. Retain the complete source archive, license notices, both parser files,
configuration/build logs, tool versions and schema-1 `proof-build.json` receipt.
The recipe builds both variants with identical options; `-D_GNU_SOURCE` and
`LIBS=-lm` make configure detect `sincos` using the actual compiler prototype.
It applies no additional source patch or forced configure-cache result.

The 2026-10-11 replay used Docker Desktop 4.94.0 (241994), engine 29.8.2,
Linux amd64, with existing image
`sha256:c0aee3f2c9c722c2afe67dff563609b63cbbe09f79e97ce8d5debdddbfab302e`.
The image ID is verified by the outer Docker invocation, not self-attested by
the receipt. Builds run offline with a read-only root and temporary output;
replay mounts every input read-only. No host library or runtime dependency changes.

| Decoder | Root and local-X26 suppression | Nested selection and local-X26 suppression |
|---|---|---|
| Original 0.2.42 | Two cases pass | Positive oracle fails at 1,8: fallback T instead of A |
| Test-only X27 correction | Two cases pass | Two cases pass: A/C/D, then suppressed T/T/SPACE |

The replay wrapper exits successfully only for these exact recorded outcomes.
The original nested record explicitly has `conformance_passed: false`; a green
comparison command must never be reported as four conformant runs. Each fresh
process verifies exactly one actually mapped libzvbi path and its receipted hash
through `/proc/self/maps`. Selecting corrected/root while loading original rejects
with `Unexpected mapped decoder path`, even though root cells alone would pass.
This Linux-x64-only manual harness is not a portable product requirement.

Source archive SHA256:
`4ecc6b129d6b8191db86953efec5f50afb6f1d9db1a706bb903ebde0d489df4c`
(1,267,984 bytes; 267 members). Original/corrected `packet.c` hashes:
`e41fa3c60308599ab6a008da21b793f18e86a5011fac32fe6f84befc9dd6db18` /
`9547e54ff9f72999289423d5a85a3ee4cceea93e1858034a82b62e10370d70bd`.
This build's original/corrected loaded-library hashes:
`2991c13dcd1f8c2b694bcce57b5edd2e68ba5bfab1c3cada55f6e088463e9bf9` /
`4b2f8b97bb3847c14b33b80eed70441b6e5607046026cd4b32861cca1af2ea9b`.
Receipt SHA256:
`911f441bcad8e1abf0beee4d6556f61b21ff76655f9586bc838091c7cf4738e6`.
Future builds record their own library/configuration hashes; source, parser,
fixture and helper pins remain fixed. The unchanged request/helper hashes are
listed in the preceding section.

This establishes representative independent decoding with an explicitly corrected
C implementation. It does not establish an official fixed release, complete
Level 2.5 support, transport integrity, suitable-client display/navigation or
QTeletextMaker/editor/admin/publication acceptance. Those requirements remain open.

### Complete normalized display packets

`encode_teletext_normalized_display_page` now builds complete owned display units
from the same immutable service, exact owner and checked normalization result.
The shared packet writer accepts the actual packed X26 bodies once, alongside
original editorial links, actual page X27/4 and X28/0 presence, and unchanged
header/fallback rows. It does not reinsert legacy Unicode X26 or synthesize a
page override from effective magazine/header presentation. M29 remains separate,
including when an actual page override exists. This diagnostic result grants no
whole-service admission, publication or runtime capability.

The public constructor uses the existing ordinary allocation policy. A private
service constructor accepts one bounded list of original MOT-default reservations;
original and augmented allocation passes use that identical list. Resource
pointer changes therefore correspond to exact semantic targets, rather than
truncating IDs or rebinding owners. Raw repeated default inputs still consume
the trusted budget. Final X26 capacity is sixteen bodies, checked before page
storage. Failed owner, normalization, metadata or comparison checks return no
partial owned page and leave the source unchanged.

Seven new regressions cover packet order and literal header/fallback retention,
actual metadata presence, separate M29, real pointer reservation0→16, raw budget
and late-failure isolation. Fifty-one selected regressions pass
(`451724b2-e219-422e-8df8-3398246b4bf1`), including prior native-page, ordinary
packet/transport, allocation and normalization tests. Strict all-target FFmpeg
Clippy and formatting pass. Private budget arguments were mechanically borrowed
after the initial lint identified large by-value copies; public signatures and
semantic control/data flow remain unchanged.

The production complete page—not a fixture loop replacing legacy packets—passes
pinned teletextimager 0.2.0 at `2b0125b778c45ffdfa2df0b196870c093361f317`
(`08c43d55-d7ac-4e43-8f5b-ef5def5aa3ce`). Actual owner200/subcode1234, no
invented X28, the Active `A` over derived pound, ordinary `Z` and `B` sentinel
have literal Level1.5/2.5 tuple checks. This valid native evidence predates only
the equivalent private budget-reference correction. It proves this display
packet seam; it does not certify resource-only transport, full glyph pixels,
client navigation, TTI editing, publication or every Level2.5 feature.

Request SHA256:
`577f0815b8e72e60c832fec6a2ad9aadee98d1c55c59ca7c9bda05786b1b2be3`.
The strict composition helper remains unchanged at
`e1cb2ad2b16e78a0f56baaae14434aa6d78f146d7e0173c7ade9f19db97228eb`.

```bash
ERSATZRS_TELETEXT_TEST_IMAGER_DIR="$decoder_dir" \
ERSATZRS_TELETEXT_TEST_COMPILED_DISPLAY_IMAGER_FIXTURE="$fixture_dir/compiled-display.json" \
  ./run.sh test -p ersatzrs-ffmpeg production_display_units_decode_active_override \
  --lib --run-ignored ignored-only
ERSATZRS_TELETEXT_TEST_IMAGER_DIR="$decoder_dir" \
  python3 scripts/e2e/teletext-main-composition-decoder.py \
  < "$fixture_dir/compiled-display.json"
```

The owned complete-service boundary now authenticates these declarations before
compiling resources and displays; see the candidate contract below. Engine,
transport, client, editor and publication acceptance remain separate.

### Authenticated complete service candidates

`TeletextDeclaredService::new` consumes one immutable semantic service plus
actual MOT declarations, exact display selections and explicit empty POP/GPOP
companions. It compares all four selected resource roles and sparse default
slots, retaining original owners after X27 supersession. Local X26 suppresses
execution without removing declaration obligations. Every advertised MOT extent
and applicable X27 mask requires the exact resource purpose and subpage, even
for unassociated or superseded links. Active defaults in every emitted bank
reserve real pointers and require actual type-matching definitions.

Each magazine has one MOT bank (§10.6), applicable to all display subcodes.
Every host selection must name that actual bank, or explicitly name no bank
when the magazine emits none. Distinct simultaneous same-magazine MOT owners
and a host opt-out reject with located diagnostics. A single nonzero S1 remains
accepted: Annex A.1 recommends zero, rather than making every other raw value
invalid. Individual resource overrides remain X27/4, not host-only routing.

`new_with_object_companions` retains genuinely empty POP/GPOP resources without
inventing objects. Raw record/structure limits precede purpose and collision
checks. The declaration boundary requires the same exact companion inventory.
Reauthentication clears previous derived presentation; actual page X28 precedes
own-magazine M29, then original associated POP flags, then header fallback.
Protocol/panel/BBS flags do not change charset, palette, planes or remapping.
An unused POP link with otherwise eligible nontrivial presentation remains an
explicit unresolved case pending primary evidence; this is not a scope waiver.

`encode_teletext_authored_service` consumes that checked declaration and one
explicit lifecycle record per resource. Missing, duplicate and unexpected
records reject. One bounded list of all original MOT reservations drives the
resource writer and both display allocation passes. Absent Local X26 with no
Base-derived instructions retains native default activation and the original
semantic composition; present X26 uses the complete normalized display writer.
Failure cannot silently switch paths or exempt foreign owners. Every resource,
including empty companions, is encoded once. M29 has one service-wide emission
list; page-local copies are diagnostic only. All headers use serial mode.
Aggregate page/unit/command bounds and actual final physical inventory reject
the entire candidate without returning partial output.

The opt-in native tests feed every produced resource and display unchanged,
adding only explicit closing headers to commit the pages to the independent
decoder. The root A/C and nested A/C/D oracles, contrasting selected-resource
owner and empty-Local T fallback remain identical to the primitive calibration.
Resources include the declared empty S1=0 companion. This is bounded decoder
feeding, not proof of a timed service carousel, client navigation or transport.

Request SHA256 values:

- Authored root: `fbab6be109e59c0c29d30e0936ae4681a094217bb178821ecf3a81c778f31efd`.
- Authored nested: `f837aa540a3ea608f13f2bfe692cabe48b4b731ec13ac8a58be8de8934e7f33f`.

The existing libzvbi 0.2.42 source/configuration/library receipt and unchanged
strict object/resource helpers are reused. Unmodified root succeeds;
unmodified nested retains its exact X27 misrouting failure. The test-only X27
correction succeeds for both two-case fixtures and verifies the actually mapped
library hash. No upstream fixed release or suitable-client acceptance is claimed.
The maintained replay also preserves the four original primitive fixtures.

```bash
ERSATZRS_TELETEXT_TEST_AUTHORED_SERVICE_ROOT_FIXTURE="$fixture_dir/authored-root.json" \
  ./run.sh test -p ersatzrs-ffmpeg \
  full_authored_service_native_root_and_compiled_empty_local_decode \
  --lib --run-ignored ignored-only
ERSATZRS_TELETEXT_TEST_AUTHORED_SERVICE_NESTED_FIXTURE="$fixture_dir/authored-nested.json" \
  ./run.sh test -p ersatzrs-ffmpeg \
  full_authored_service_nested_selected_default_decode \
  --lib --run-ignored ignored-only
```

Exporting nested with unmodified libzvbi intentionally returns nonzero after
writing the unchanged producer fixture. Use the receipted offline replay for
both decoder variants; never change expected cells to obtain a green result.
Full resource-command C2 settlement, lifecycle/update histories, host-delivered
service transport/admission, all remaining national recipes and
Level2.5 client/editor/publication gates remain required and open. P9A-01 owns
current run IDs and verification evidence; no milestone acceptance is weakened.

### Complete service carousel and output planning

`TeletextCarousel::new_authored` owns a complete service candidate. It reuses the
legacy cursor, 20 ms header/body interval, at most31 units per PES batch and
closing mFF header. Actual M29 is emitted in a separate Magazine acquisition
batch, repeated with resources when needed. Resource indexes are bounded and ordered MOT, DRCS/GDRCS, POP/GPOP,
then original display order (Annex B.3/B.7). Owned pages are retained; batches
copy only their scheduled units.
The first header retains supplied resource lifecycle state. Later identical
resource repeats clear C4/C8, preserving physical S1/counter/last-row and every
other header bit. Display C4 repetition remains unchanged. Source units stay
immutable.

Resource and M29 acquisition repetition must remain within twenty seconds
(Annex B.6(1)); this does not bound the whole display carousel. Before order-index
allocation, count a complete acquisition sweep R (magazines plus resource headers
and ceil(bodyunits/31)), the largest whole display D, the sole final FF and one
physical-window margin. `R + D + 2 > 1000` rejects with `CycleTiming` when resources
or M29 exist. This is fixed-rate output capacity, not an approved authored-feature
restriction. Display-only candidates gain no resource repetition limit.

At a complete display boundary, refresh before another whole display plus FF and
the window margin would exceed the sweep-start interval. Retain the exact display
cursor while M29 and the existing B.7-ordered resources repeat; resume it after
the full sweep. Magazine-only and resource-only services use their actual first
transmission as the interval anchor. All immutable sweep components have fixed
relative offsets, so at most999 batch intervals between sweep starts also bounds
each full component repetition. The extra window accommodates different physical
null-slot positions. Actual sink jitter remains a separate delivery acceptance.

Internal refreshes clear resource C4/C8 even in the first long display cycle;
later first-pass displays still retain their supplied erase header. There is one
FF only after all original displays. Overflow or failed timing validation
must not change an existing plan. Replacement requires the complete boundary;
even a first magazine-only batch closes that boundary. Clock and whole-generation
ownership survive legacy/authored transitions.

Seven new pure/output regressions join79 focused passes
(`7209953e-f08c-4448-86da-3bbb19b4bcb9`). The499-display fixture produces1005
batches with acquisition starts at0/997 and one closing FF. Both changed/unchanged
first-cycle resources, M29-only/resource-only paths, immutable bodies, unique
display order, mid-refresh replacement/overflow and intrinsic acquisition-capacity
rejection are checked. An initial large fixture used reserved1BE; correction to
valid2C0–2DF logical-S1 groups changes no production rule.

The actual output epoch streams bounded20ms windows, consumes40 early null slots
with valid adaptation-only packets after frame500 and checks the final byte of
every resource/M29 unit against the physical TS packet schedule. More than10ms
additional in-window displacement still retains the20s repetition bound. Source/
PCR packets, continuity and the sole FF remain intact. EOF after a confirmed
internal refresh is `Unterminated`; preparing/exposing FF does not establish
completion until its exact schedule is acknowledged. Strict affected all-target
Clippy, formatting and independent standards/code review pass; a test-only
`filter().next_back()` lint correction uses equivalent `rfind()`.

TSDuck3.44-4676 independently validates the complete paced output and detects
deliberate PID512 continuity damage
(`f381c795-da3e-4ced-9fd4-da7ebf5a2c90`). Synthetic PSI/PCR are the existing output
fixture; this is not hosted AV, source-subtitle or client acceptance. The capture
has12562912 bytes and SHA256
`37a28ff3e14ce9ba568f5193c0a102cb27ae6169862777ecaf7f2991f6204ea0`.

```bash
ERSATZRS_TELETEXT_TEST_TSANALYZE="$tsanalyze" \
ERSATZRS_TELETEXT_TEST_INTERLEAVED_TS="$fixture_dir/interleaved.ts" \
  ./run.sh test -p ersatzrs-ffmpeg \
  interleaved_paced_service_passes_tsduck_and_detects_counter_damage \
  --run-ignored ignored-only
```

Three unchanged small-service carousel cases also pass original libzvbi0.2.42
(`f97a84f6-de3b-4fa8-adba-0df9f958cf94`): root/empty Local, resource C2 and shared
charset aliases. Root and alias requests retain the exact recorded hashes; valid
earlier receipts/goldens are reused. Full update history, hostv2 admission,
national/carrier gaps and complete engine/editor/client/publication acceptance
remain open. No milestone contracts/statuses/dependencies change.

`TeletextServiceDescriptor::from_authored_service` advertises only original
initial/subtitle display roles, addresses and languages. MOT/DRCS/object pages
and M29 do not create navigation entries. `TeletextOutputGate::new_authored`
and `new_authored_retained` pair this same candidate's descriptor and carousel,
then reuse source PSI/PCR admission, null-slot capacity, packet pacing, retention,
explicit delivery start, sink acknowledgements and EOF fences. The web host
still needs version2 service admission/publication; these constructors grant no
package, profile or live-stream capability by themselves.

### Consecutive full-resource updates

At `replace_authored_at_boundary`, known resource owners use the raw magazine/page
and logical S1. Compare the actual immutable encoded resource kind, last-packet
extent and all units, ignoring only header byte9 (S2/C4) and the C8 bit in header
byte12. Other controls, allocations, pointers, modes and sparse/removed body
packets remain content. M29 has no resource S2 counter.

| Previous state | Requested full transmission | Required S2 |
| --- | --- | --- |
| Same normalized bytes | `FullUnchangedFirst` | Previous counter |
| Same normalized bytes, deliberate refresh | `FullChangedFirst` | Previous counter plus one modulo eight |
| Changed bytes/kind, or retired known owner | `FullChangedFirst` | Previous counter plus one modulo eight |
| Cold or previously unknown owner | Caller-supplied complete lifecycle | No invented previous counter |

These checks implement Annex A.1, B.6(5) and Table B.1's full-transmission path.
Equal final bytes do not prove unseen intermediate receiver history (A.1 note3).
Partial/continuation transmission is not implemented by this complete-service
compiler; that does not waive any outstanding standards requirement.

Use the last completed generation rather than an untransmitted replacement.
For example, completed A at S2=7 followed by pending B at S2=0 still allows an
untransmitted C matching A at Unchanged/S2=7. Only after B's complete cycle may
a later changed update use S2=1. Never-emitted removal can likewise be cancelled;
completed removal keeps a counter-only tombstone and requires Changed on return.
Completed legacy displays retire retained bodies sharing their raw page address,
across all S1, while leaving counters and unrelated resources intact.

History moves resource bodies from the replaced complete candidate without
cloning. It retains only the latest completed resource bodies and retired counters
in at most32,640 raw-page/S1 keys, not every prior generation. Rejected candidates
leave history, immutable current bytes, cursor, boundaries and programme clock
unchanged. Existing timing and mid-cycle fences run first. The owner must confirm
the planned final batch's actual delivery and generation fence before replacement;
this synchronous cursor does not acknowledge the sink.

```bash
./run.sh test -p ersatzrs-ffmpeg \
  -E 'test(service_update::) | test(carousel::authored_service_update_tests::)'
```

These are preparatory in-process checks. Durable restart/publication authority,
cold receiver state and broader resource/client update decoding remain required
and open. Earlier unchanged single-service transport/decoder receipts remain
applicable; they do not prove consecutive receiver-cache updates.

### Warm-cache DRCS update diagnostic

The opt-in Linux x64 diagnostic uses one original libzvbi 0.2.42 decoder across
four complete-service generations, with two actual carousel cycles each:

| Checkpoint | DRCS body | S2 | First C4/C8 | Repeated C4/C8 |
| --- | --- | --- | --- | --- |
| `a_changed` | Fixed asymmetric A | 7 | 1/1 | 0/0 |
| `a_unchanged` | Identical A | 7 | 0/0 | 0/0 |
| `b_changed` | Different fixed asymmetric B | 0 | 1/1 | 0/0 |
| `b_repeat` | Identical B | 0 | 0/0 | 0/0 |

Resource 2C0 retains normal S1=0, slot 0 and nn=1. An actual MOT 2FE declaration
selects it for display 200:0, whose Local X26 references that DRCS glyph at (1,8).
Base T fallback at (1,8) and B calibration at (2,1) are preserved. No X27 or invented
closing header is added: same-magazine resource/display acquisition uses the
unchanged production compiler, carousel and replacement boundary. Actual batches
retain continuous programme ticks across all replacements.

The new helper fetches/unrefs the display after each generation, checking exact
Unicode 0xF400, normal opaque FG7/BG0, all 120 cached DRCS pixels and all 120 PAL8
rendered pixels against fixed independent A/B literals. Base B is also checked
as Unicode 66 and 120 literal font pixels from pinned `wstfont2.xbm`, SHA256
`f6a1fa6714e3604a466eb8d6ca3b496c2dcf217f217985c710eee288137d41ad`.
Native canvases retain guard bytes. One decoder and monotonic synthetic 40 ms
per-unit decode timestamps span every checkpoint; these decoder feeds do not
prove actual sink timing or paced transport.

Input is closed, bounded to 2 MiB, four sequences, four checkpoints and 256 units
per checkpoint. Every header/lifecycle/framing check runs before native loading.
No supplied expected-pixel field is accepted. The frozen ABI helper is read once,
hashed and compiled from those exact source bytes; valid cached bytecode cannot
replace it. The mapped original library must match the existing receipted
`2991c13d…` hash; the corrected X27 variant is not used.

Pinned helper SHA256:
`3d6fc3f37304c23e56559dbec610bbfbd5d3feea54ca37a9a16a73c6a3e5b678`.
Contract-test SHA256:
`b1b885ee425a2209f664cd89da71fd5dff7840a99435803f6d503eec64f6e244`.
The 53,970-byte request has100 units per checkpoint and SHA256
`93d749f9ecd507b3d83ffd71b874ab42c718c09ee904cd032a62da73325d0166`.

```bash
LD_LIBRARY_PATH="$proof_dir/original" \
ERSATZRS_TELETEXT_TEST_PYTHON=python3 \
ERSATZRS_TELETEXT_TEST_RESOURCE_UPDATE_FIXTURE="$fixture_dir/updates.json" \
  ./run.sh test -p ersatzrs-ffmpeg \
  consecutive_complete_carousel_drcs_updates_decode_with_one_warm_cache \
  --lib --run-ignored ignored-only --success-output immediate
LD_LIBRARY_PATH="$proof_dir/original" \
  python3 scripts/e2e/teletext-resource-update-decoder.py < "$fixture_dir/updates.json"
python3 scripts/e2e/teletext-resource-update-decoder_test.py
```

P9A-01 owns the actual run evidence. Eight pure contract tests include malformed
input, duplicate JSON, counter/flag/repetition errors, pre-native bounds,
one-decoder lifetime and a valid malicious cached-bytecode control. An actual
negative probe replaces only B's bitmap units with A's valid bytes, retaining
all expected B headers: header validation succeeds, native decoding rejects the
cached pixel mismatch. The positive fixture and pixel oracles stay unchanged.

This proves the representative normal DRCS warm-cache replacement and unchanged
control. The decoder does not reject every incorrect counter tuple itself;
engine negative tests provide that separate evidence. Object/MOT update history,
removal/reintroduction decoding, restart authority, actual client navigation,
full Level 2.5 and editor/publication acceptance remain open. No cache-eviction or
complete conformance claim follows from this diagnostic.

The complete mixed-service calibration sends four real carousel cycles through
the production packetizer. Independent calibration adds PSI/PCR only and retains
Rust PES/TS bytes. TSDuck3.44-4676 checks sync/transport errors, PID references,
private-stream PES, continuity and PTS/PCR leaps; an intentionally damaged counter
must fail its continuity invariant. This is not a real AV host/playback proof.
Captured synthetic TS SHA256:
`763cee5715f461388214b26d76be954e397838daf517032a7ecc8b8c0ad320b1`.

```bash
ERSATZRS_TELETEXT_TEST_TSANALYZE="$tsanalyze" \
ERSATZRS_TELETEXT_TEST_PYTHON="$python" \
ERSATZRS_TELETEXT_TEST_AUTHORED_CAROUSEL_TS="$fixture_dir/authored-carousel.ts" \
  ./run.sh test -p ersatzrs-ffmpeg \
  complete_authored_carousel_pes_passes_tsduck_and_detects_counter_damage \
  --lib --run-ignored ignored-only
```

Two full carousel cycles also pass the unchanged root A/C and empty-Local T
oracle with libzvbi 0.2.42. Original nested decoding preserves its exact known X27
failure; the same receipted test-only correction passes nested A/C/D and Local
suppression. No page body or closing header is replaced in this fixture.
Request SHA256 values:

- Carousel root: `8636493c5710c59339c14e06f4fb0ccbc33dfa12fc2abd5aadd4716efd60be8d`.
- Carousel nested: `093f973725e0c7a1beeff3ad178af968f24ced16aac1cd77cc5467b522bf5975`.

```bash
ERSATZRS_TELETEXT_TEST_AUTHORED_CAROUSEL_ROOT_FIXTURE="$fixture_dir/carousel-root.json" \
  ./run.sh test -p ersatzrs-ffmpeg \
  full_authored_carousel_root_and_compiled_empty_local_decode \
  --lib --run-ignored ignored-only
ERSATZRS_TELETEXT_TEST_AUTHORED_CAROUSEL_NESTED_FIXTURE="$fixture_dir/carousel-nested.json" \
  ./run.sh test -p ersatzrs-ffmpeg \
  full_authored_carousel_nested_selected_default_decode \
  --lib --run-ignored ignored-only
```

The maintained receipt replay accepts `carousel-root`/`carousel-nested` in
separate original/corrected processes. Original nested must retain
`conformance_passed:false`; both corrected fixtures and original root report
two passing cases. Exporting nested on the original decoder intentionally
returns nonzero after writing the producer fixture. Remaining charset/order ties, update
history, hostv2 served transport, full national
recipes and suitable-client/editor/publication gates remain required/open.

### Final resource command positions and attribute settlement

`EncodedTeletextObjectPage::command_positions()` retains each exact full object
identity and original body ordinal, with `first_word()` and `semantic_word()`.
The final emission pass records these spans after pointer extension, DRCS
selectors and invocation-origin modifiers. Coordinates start at FIRST X3:
rows3–25 occupy the first23 groups of13 triplets, followed by X26 designations.
Packet metadata bytes are not triplets. Definition markers, termination, padding
and genuine zero-output position identities have no command record. Physical
capacity bounds the nonempty record count before sidecar allocation; original
raw command admission still includes every zero-output command.

The complete authored factory encodes resources privately before displays, then
borrows these exact pages together with the original catalogue and shared MOT
reservations. Output ordering stays displays followed by logical resources.
The private resource index requires every emitted command and exact original
definition/type/pointer. Augmented resource definitions and allocations must
match before reuse; generated Local carriers remain separate. Missing resources,
wrong S1 or altered bodies, types, reservations and trace operations reject with
located correspondence errors. Empty contexts invent no diagnostic resource.
Shared lowering classifies zero-output commands; no duplicate AP policy is used.

Combine Local and resource positions into one exact-trace evidence object before
original compatibility planning and before actual model comparison. Native MOT
defaults bind their resource body events while retaining sparse DefaultSlot
provenance and absent X26; their synthetic root calls gain no wire timestamp.
Repeated and nested executions retain separate invocation IDs while reusing
the same definition word. Existing Annex C.2 comparison settles writers only
inside one exact executing object instance. Resource page, pointer, storage and
carousel acquisition order cannot decide cross-instance priority. Attributes,
display resolution and colour are recomputed together; charset and unresolved
cross-instance histories remain separate mandatory work.

Thirteen new pure regressions cover literal pointer/displaced/boundary words,
no-op/raw bounds, exact catalogue/allocation/trace correspondence, independent
repeated instances, native defaults and both normalized attribute models. The
complete two-cycle Active/Adaptive calibration passes unchanged libzvbi 0.2.42:
literal A65, foreground13, background0, plus the frozen unaffected Base B.
The receipted offline replay verifies the actually mapped original library;
no decoder correction is needed for this fixture. Current request SHA256:
`0ee3aff7c8e60610dcbd6fe432080b559a437c57218cb142489ee2ecea4ba8cc`.

```bash
ERSATZRS_TELETEXT_TEST_RESOURCE_C2_FIXTURE="$fixture_dir/resource-c2.json" \
  ./run.sh test -p ersatzrs-ffmpeg \
  complete_carousel_active_and_adaptive_resource_c2_decode \
  --lib --run-ignored ignored-only
```

Use the maintained offline receipt replay with selector `original resource-c2`,
mounting this request as `/proof/resource-c2.json`. Prior carousel root/nested
and TSDuck fixture hashes remain byte-identical, so their corrected/original
receipt outcomes remain valid. The original nested decoder still fails its
unchanged positive oracle. Exact results belong to P9A-01 evidence. These are
synthetic model, native decoding and transport checkpoints, without host-v2
publication, actual client navigation or editor workflow acceptance.

### National-option carrier conversion

`resolve_teletext_national_conversion` is a finite Table 36 identity relation,
read against Tables 35/37 and the Hebrew G0 Table 46. It retains the exact original
national G0 identity and returns an explicit designation, G0/G2/diacritic operation and resulting
enhanced identity. It does not infer meaning from Unicode or equal font masks.
German `§` at Base `0x40` becomes Latin G2 `0x27`; German `#` becomes enhanced
G0 `0x23`; English `$` becomes G2 `0x24`, because enhanced G0 `0x24` is currency
sign, not dollar. Designation aliases resolve through the existing charset and
intent pipeline; graphics-mode alphabetic positions remain distinct from mosaics.

The audited table has 165 established recipes and four explicit pending positions.
English `0x7C` (double vertical) has the exact Hebrew G0 `0x7C` identity in
[EN 300 706 Tables 34/46](https://www.etsi.org/deliver/etsi_en/300700_300799/300706/01.02.01_60/en_300706v010201p.pdf),
using explicit designation `0x55` (Hebrew G0/Arabic G2). Polish `0x5B` (stroked
capital Z) and Turkish `0x23` (TL) still need proven physical representations. Romanian `0x5D/0x7D`
have ambiguous breve/caron marks in the primary table's tiny glyphs and remain
unresolved; do not choose by language expectation or a decoder's Unicode table.
Romanian `0x5E` is the primary table's I-circumflex, not the cached decoder's
I-acute. These are remaining implementation/evidence gaps, not approved authoring
restrictions. Resource-backed realization must respect actual DRCS and pair budgets.

Carrier construction and typed comparison share this same checked relation.
Comparison permits it only after exact original-origin and invocation correspondence
and a private generated `CarrierBody/Glyph` mapping. Retained Base and other
commands keep their literal identities. The original service, Base glyph record,
source, repaint origin and fallback bytes remain immutable. Full selected-page
font/fragment/colour comparison stays independently required, so a semantic mapping
cannot hide a font discrepancy. Explicit Latin and Hebrew designations consume the
normal charset-pair budget; it does not bypass allocation or packing limits.

Six focused domain tests pass (`b9694f9c-2dac-406d-b1ac-68e5b721ba4d`): all
169 literal operands/pending cells, all 164 results through real charset/intent
execution, all 29 page aliases, invalid codes/repertoires and non-national roles.
The combined teletext run (`db7ad83d-8f0c-4e68-b1b4-ecd917a35ff6`) passed
949 of 950 tests; the new overlap fixture alone initially violated the existing
forward-only main-position rule. Using a legal Active call offset `(0,1)` from
main AP `(1,7)` preserves its intended absolute geometry without a backward move.
The corrected focused test passes (`7a3843ae-4fcf-41bd-b67e-473010acd16e`).
The unchanged tests remain valid evidence; this is not a claimed all-green first run.

The overlap has German Base `§` enlarged by Adaptive repaint over columns 7–8,
Active double-size `A` behind it, explicit normal `X` at column 9 and an unaffected
`B` sentinel. A derived pound at row 2 triggers the actual normalization closure.
The recipe preserves the distinct repaint origin and original national Base
candidate. Both typed and full selected-display comparisons inspect all 1,000
cells with zero findings; the original service and ordinary fallback encoder
remain unchanged. Four retained national Base cases separately prove that no
carrier means no canonicalization of their literal identities.

The production-normalized DVB fixture also passes pinned teletextimager 0.2.0
at `2b0125b778c45ffdfa2df0b196870c093361f317`
(`5e8a34ee-1a3f-474b-9c3a-b116a41b58c5`). Literal Level 1.5/2.5 code,
repertoire, diacritic and colour tuples cover both `§` halves, `X`, both bottom
`A` halves and the `B` sentinel. German Base `§` is raw `0x40`/font 15;
the generated enhanced glyph is `0x27`/Latin G2 font 7. Expected tuples came from
the pinned decoder's table identities, not from overwriting observed failures.
This is selected-cell independent decoding, not actual client navigation or pixel
acceptance of every converted national glyph. The helper and its source guards
remain unchanged.

Request SHA256:
`3717dd013e81515fdf3bdcc17c479eeb61d210bd802caf12e70764697791f184`.
Helper SHA256:
`e1cb2ad2b16e78a0f56baaae14434aa6d78f146d7e0173c7ade9f19db97228eb`.

```bash
ERSATZRS_TELETEXT_TEST_IMAGER_DIR="$decoder_dir" \
ERSATZRS_TELETEXT_TEST_NATIONAL_OVERLAP_IMAGER_FIXTURE="$fixture_dir/national-overlap.json" \
  ./run.sh test -p ersatzrs-ffmpeg national_base_enlarged_overlap_decodes \
  --lib --run-ignored ignored-only
ERSATZRS_TELETEXT_TEST_IMAGER_DIR="$decoder_dir" \
  python3 scripts/e2e/teletext-main-composition-decoder.py \
  < "$fixture_dir/national-overlap.json"
```

The Hebrew extension has eight passing domain tests
(`99c39153-383d-4c17-9694-068afa88dc57`), including all 169 literal entries,
165 real charset/intent executions, 29 aliases and explicit rejection of a third
Latin/Greek/Hebrew pair. Ten normalization regressions pass
(`d5fbd8fd-952f-499d-a5e3-5110a84a0eb3`). The enlarged English double vertical
keeps its original Base national identity/repaint origin; its actual generated
carrier is Hebrew G0. Both complete 1,000-cell comparisons have zero findings.
The immutable service and ordinary fallback bytes remain unchanged.

Independent decoding of this new fixture **fails** in pinned teletextimager 0.2.0:
Level 2.5 `(1,7)` is `[124,0,0,7,0]`, expected `[124,6,0,7,0]`.
The reader correctly parses mode08/data55 before mode09/data7C. Its Adaptive and
Passive overlays omit private modified-G0/G2 designation handling, even though
Table 29 and Annex C.4(2) require it. Do not replace the expected Hebrew identity
with Latin or accept equal masks. `teletext-national-carrier-calibration.py`
uses the unchanged guarded oracle and records this exact known mismatch with
`conformance: false`; unexpected failures and unexpected success reject.
Its six selected Level 1.5 cells pass. Three opt-in tests pass
(`1bf0c600-7d9c-430e-a5f0-089827c902c9`): the new **negative calibration**,
the unchanged German positive (request hash unchanged), and existing G1 calibration.
The first new strict positive attempt failed as expected by the later diagnosis
(`f1cec664-cc2a-47f3-b541-8fcd68f09338`). This does not establish independent
Level 2.5 success for the Hebrew carrier. Strict domain/FFmpeg all-target Clippy
and formatting pass; complete decoder/client/service acceptance remains open.

English request SHA256:
`61230e6bdd4e31a059e85aea4632939b908eced0f4d4435073630007b113fe6c`.
Negative calibration helper SHA256:
`6bae3b93ac33d70f02061d024ea6cd488b0fc77e1c6e8b1fedc3f88c730c7a6a`.

```bash
ERSATZRS_TELETEXT_TEST_IMAGER_DIR="$decoder_dir" \
ERSATZRS_TELETEXT_TEST_ENGLISH_NATIONAL_OVERLAP_IMAGER_FIXTURE="$fixture_dir/english-overlap.json" \
  ./run.sh test -p ersatzrs-ffmpeg english_national_overlap_records \
  --lib --run-ignored ignored-only
# Strict positive oracle: reproduces the documented failure, exits nonzero.
ERSATZRS_TELETEXT_TEST_IMAGER_DIR="$decoder_dir" \
  python3 scripts/e2e/teletext-main-composition-decoder.py \
  < "$fixture_dir/english-overlap.json"
# Negative calibration: explicit conformance:false, exits zero only for this gap.
ERSATZRS_TELETEXT_TEST_IMAGER_DIR="$decoder_dir" \
  python3 scripts/e2e/teletext-national-carrier-calibration.py \
  < "$fixture_dir/english-overlap.json"
```

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
remains its actual table position. Unknown charset order keeps the original
pending registry index. Differing possible pairs cannot manufacture one identity.

#### Shared charset alias consensus

Equal-position Local/Active declarations can retain an unresolved raw winner
while every applicable Table 34 designation selects the same exact G0/G2 pair.
Glyph intent now exposes that derived `selected_pair()` without changing the
raw selection, alias values, declaration indexes or source/root/invocation
contenders. It compares complete pairs, never Unicode or equal font masks.
Private Adaptive/Passive state and known raw aliases remain unchanged.

One checked cache entry per pending record avoids recursive history walks or
per-glyph contender allocations. Final declarations supersede the previous
position; same-cell choices retain the applicable previous context, including
an earlier pending choice, and exclude later declarations in the glyph's own
definition. Every remaining contender must agree. Disagreement anywhere in
the applicable chain remains `CharsetOrder`; agreeing last two writers cannot
erase an older possible repertoire. Existing prohibited/unmarked/composite
assignment rules still apply after consensus.

A derived carrier with a known pair but no raw winner emits the first assigned
Table 34 code for that pair. This physical representative never replaces the
original pending choice. Whole-display comparison diagnoses actual unresolved
glyph intent, rather than rejecting harmless retained raw history. Different
Local/POP repertoires still produce a located diagnostic and reject the complete
candidate atomically.

Ten new pure regressions join 98 focused passes
(`616e8bc2-f43e-4d02-823d-aadc6c23192d`), including all 1,000 display cells and
both full raster phases after complete resource-backed normalization. Strict
domain/FFmpeg all-target Clippy, formatting and independent consumer reviews
pass. The unchanged libzvbi 0.2.42 helper decodes two actual carousel cycles at
page 200/subcode 0, retaining Base B and enhanced A/7/0 at row 1, column 10
(`b8a8bada-a4ab-4984-a36f-df24580435f0`: three decoder tests pass; the transport
test initially lacks explicit tool paths). The separately configured TSDuck
test passes (`631b0277-f793-4404-86ec-e72fdefe2820`), including intentional
continuity damage detection. Existing TS, root and resource-C2 fixture hashes
remain byte-identical; their prior receipt evidence remains valid.

New request SHA256:
`3a2ad74aa78a28ac97a944b324c58a8a7cf381f9f56c6391187502531d519af0`.
The maintained offline Docker receipt accepts `charset-consensus`, pins one
successful case, and verifies the actually mapped unchanged library SHA256
`2991c13dcd1f8c2b694bcce57b5edd2e68ba5bfab1c3cada55f6e088463e9bf9`.

```bash
ERSATZRS_TELETEXT_TEST_CHARSET_CONSENSUS_FIXTURE="$fixture_dir/charset-consensus.json" \
  ./run.sh test -p ersatzrs-ffmpeg \
  complete_carousel_shared_charset_aliases_decode --lib --run-ignored ignored-only
```

Use the pinned decoder environment and receipt replay described above. This is
one representative authored-service proof; different-pair priority, complete
Level 2.5 client display/navigation, editor roundtrip and publication stay open.

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

Annex D.2.1(2), printed page133 of
[EN 300 706 V1.2.1](https://www.etsi.org/deliver/etsi_en/300700_300799/300706/01.02.01_60/en_300706v010201p.pdf#page=133),
leaves the second-language default decoder dependent when X28/M29 are absent.
The existing Header/POP engine policy uses no secondary G0; that is not evidence
of an authored Table33 no-second-G0 declaration. Producer-local presentation
snapshots must retain that uncertainty separately from the engine policy, full
palette/panel/charset defaults and original packet presence. They must not turn
it into a transmitted X28 override while combining unrelated M29 declarations.
Equivalent materialization and suitable-client evidence remain open.

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
commit `cac2167bac933a6160cca45a45d5d4ee95355898`. Source inspection is now
supplemented by the bounded, actual editor calibration below. Full ErsatzRS
export/draft import/preview/publication acceptance remains open.

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

### Actual synthetic edit/save calibration

The pinned source was built unchanged with native GCC 15.2.0, CMake 4.2.3 and
Qt 6.10.2 (`qt6-base-dev` 6.10.2+dfsg-7) in a local Ubuntu 26.04.1 container.
The editor reports `QTeletextMaker 1.0`; its binary SHA256 is
`567a669fd0edef20215967a08fe679a6c011fd0d7ca85ebd15f14393540177ee`.
Local tool-image identity is
`sha256:6064a1cfba3673ce1530dd5ea4c98249a5540666e3feea3160f43c122fd81c01`.
Docker Desktop 4.94.0 (241994), engine 29.8.2, Linux amd64, displayed actual
`xcb` windows through WSLg; offscreen version checks are not display evidence.

[The calibration helper](../../scripts/e2e/teletext-qtm-calibration.py) creates
one synthetic two-subpage TTI input and verifies only its recorded edit/save
result. The [test-only widget driver](../../scripts/e2e/teletext-qtm-widget-driver.cpp)
uses the original Qt event loop, real editor input, subpage navigation and the
actual Save As dialog. It leaves the editor decoder and writer unchanged, requires
an exposed X11 window and the exact synthetic file, refuses existing/symlink
destinations, checks the saved path/modified state and exits. It writes no TTI
bytes itself. Input routing through QtTest is recorded explicitly; this is not
an OS keyboard-routing test or an ErsatzRS admin browser test.

The final headed run exited successfully, produced six checked captures and
passed the literal byte/semantic oracle. Input SHA256 is
`261ec4c6a90c5467cf497802c58f577563ff0b42db4ca61485dcd8dd5ba625b0` (619 bytes);
saved SHA256 is
`1cfa16ca74a88898fef95593858d3acedcc6c118db752414b2e33da7b35209db` (563 bytes).
Driver source/library SHA256 are
`ec18ed217de15b1bf981d0fb9d939f612d6c724b5dd56ee75b7ec480c329566b` /
`023d257ada686c82827e24817fd433aca02d84ff622d508aeab9b9e12f2e9d84`;
helper SHA256 is
`d3d9a3637f2f30859176701988778ee5c3cb02bd7ab8e1cd61ed257912881850`.

Observed outcomes:

- The first subpage's row 2 becomes `SAVED QTM ROW!`; navigating to the second
  subpage and back retains the edit. The second row 2 remains unchanged.
- Page 200 subcodes `0007`/`0030` become ordinal `0001`/`0002` in PN and SC.
  Both exact X/26 payloads, padded rows, header/title and FL links survive;
  unknown XX records disappear and the writer uses CRLF.
- The selected first-subpage G0 mode 9 enhancement displays `A` at row 1,
  column 5 in Level 2.5. A separately reopened, unmodified editor instance
  visibly retains both that enhancement and the saved text. This is selected-cell
  evidence, not all-subpage glyph, palette, DRCS, object or side-panel conformance.

The helper rejects an existing fixture destination, oversized files, non-CRLF
input and changed enhancement payloads. The final driver has an independent
review of destination and save-completion fences. Reproduce using the
[opt-in local recipe](../../scripts/e2e/README.md#qteletextmaker-headed-synthetic-calibration).
Screenshots/logs are local test artifacts, not bundled product assets.

This input is hand-authored synthetic calibration, not an ErsatzRS export.
There is no Rust TTI draft import, shared browser preview or explicit publication
proof here (`engine_roundtrip_proven: false`). Identity normalization and unknown
record loss strengthen the existing external-sidecar and strict import contract;
they do not waive supported content/navigation preservation. Engine-first order,
full editor roundtrip and suitable-client acceptance remain required.

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

### Preparatory v2 authored input

The independent addon-contract `TeletextPagesV2Response` and application
`teletext_pages_v2::parse_declared_service` now provide a preparatory unpublished
input path. Mandatory base `pages` reuse the unchanged v1 DTO/conversion. The
additional required collections are `page_enhancements`, `magazines`, `objects`,
`drcs`, `mots`, `mot_selections` and `empty_object_pages`.
Every enhanced object or tagged variant is a closed map; assigned scalar enums
are strings. Missing nullable fields are rejected. In particular, `local_program:
null` and `local_program: {"commands": []}` remain different authored inputs.

Raw resource pages use `{ "magazine": 2, "page": 192 }` for 2C0, independently
of decimal display page numbers. Resource `subpage` is logical S1, never a full
physical header subcode. Local objects retain exact display page/subcode owners;
Public/Global objects retain purpose, resource address, logical S1 and semantic
object ID. The input preserves original charset aliases, command order, palette
components, panel fields, monochrome DRCS bitmaps, X27 masks/FF magazines and
complete MOT banks/associations/defaults, including inactive raw selector bytes.

The parser retains the existing 8 MiB raw response cap and exact bounded
generation echo. Each original top-level collection is limited to 1024 records;
base pages require 1–1024. Metadata is not counted as additional base pages, and
duplicates cannot evade collection limits. Duplicate JSON keys, trailing input
and excessive nesting reject before a typed result escapes. These are host wire
limits, not normative teletext limits or bounds on process RSS.

Conversion receives separate trusted semantic/declaration budgets, accounts for
original content/commands before converted collections or domain indexes, and
uses direct checked constructors. It then calls
`TeletextSemanticService::new_with_object_companions` and
`TeletextDeclaredService::new`. Supplied resolved links, defaults and MOT
selections are checked assertions, never overrides of actual declarations.
Failures return numeric collection/command locations or existing typed
semantic/declaration diagnostics without authored text or correlation values.
No partial service, v1 retry, physical pointer/update choice or publication
identity is returned.

Independent v1 request/response version constants retain value 1, with the
original shared name preserved for callers. `TeletextSelectionSnapshot.contract`
now pins the highest supported declared contract from the exact installed
manifest/settings transaction, preferring v2 over v1. The explicit trusted-schema
request APIs and `build_request_for_schema` retain identical guide fields and
limits, requiring exact version agreement; default APIs remain strictly v1.
Profile invocation preflights the selected version before package resolution,
rechecks the manifest choice and installation identity before launch, sets
`ERSATZRS_ADDON_CAPABILITY` from that choice and returns it within the exact
selection receipt. A response cannot negotiate another contract. The global
legacy invocation API remains explicitly v1.

Existing base-only binding preparation rejects a v2 selection before invoking
the producer. Channel preparation checks every binding before starting any
producer, including a v2 binding later in a mixed selection. These are explicit
unavailable preparation results, without fallback, partial candidate or package
activation. No schema migration or installed package modification is needed.
The additive bound native path below prepares one complete producer document.
Producer-isolated channel composition, durable drafts/editor interchange and
explicit publication remain open.
This input path does not activate packages,
change live output or establish complete engine/client acceptance. P9A-01 owns
the current verification evidence.

### Bound native document preparation

`ApplicationServices::invoke_bound_teletext_document` builds the guide request
under the exact selected contract and invokes the existing profile runtime under
one 30-second absolute deadline. It checks the full returned selection and opaque
generation, then rereads the authoritative binding before returning a private
`BoundTeletextDocumentInput`. Raw producer output is never a preview document.

`BoundTeletextDocumentInput::validate` is synchronous bounded CPU work. Hosts run
it off async workers with trusted `TeletextProfileDocumentBudget`, independently
of producer fields. It validates exactly v1 or v2 without retries: v1 retains its
base draft; v2 retains the complete checked `TeletextDeclaredService`. Deadline
checks surround validation and prevent an expired successful result from escaping;
they do not preempt CPU work. A private prepared value retains the document,
selection, generation and original deadline. Debug output omits authored content,
correlation values and settings values.

The native `prepare_bound_teletext_document_candidate` entrypoint invokes this
application function, then joins one `spawn_blocking` validation/encoding task.
Its explicit `TeletextDocumentEncodingPolicy` supplies validation budgets, the
v1 encoding region, complete compiler budgets and every resource lifecycle record.
V1 is lifted with unchanged fallback, explicit base presentation and empty Local
X26, then authenticated with no MOT/resources/defaults. V2 moves its original
checked declaration into the complete authored service compiler without cloning
or replacing it with base-only pages.

The compiler owns pointers, physical packet fields, complete resource/display
inventory and service-wide M29 emission. Resource counters, Changed/Unchanged
choices and POP termination stay explicit host inputs; opaque producer generation
or package hashes cannot select them. Missing, duplicate or foreign lifecycle
records and encoding failures reject the complete candidate.

The private native result retains the complete encoded service, its original
checked declarations and derived descriptor beside the exact selection,
generation and deadline. Existing original/physical preview APIs borrow that
same compiled candidate. Before returning it, the host calls
`recheck_teletext_document_selection` after CPU work, with deadline checks around
the fresh repository read. Publication must still recheck eligibility and use
trusted generation ownership; these observations grant no live admission.

Native preparation also captures producer-local presentation before encoding,
bounded by the trusted declaration display limit. Its private result retains
immutable `producer_presentations()` beside the compiler-owned original model;
capturing defaults does not insert X28, M29 or Local X26 declarations.

This supports one explicit output-channel/addon binding. The separate channel
bridge below prepares mixed cold candidates. Durable drafts and TTI import/export, localized
admin/browser workflows, source acquisition, refresh/restart, hosted transport
and explicit publication remain open. Base-only preparation continues to reject
v2; it cannot silently discard authored content. P9A-01 owns current evidence.

### Producer-local presentation snapshots

`TeletextProducerPresentation::new` consumes one original checked declaration
under `TeletextProducerPresentationBudget::max_pages`. It preflights count and
addressable snapshot storage before allocation, retaining the original complete
service without cloning it. Snapshots follow original base-page order and exact
raw subcodes; `page(address)` never substitutes another subpage.

Each snapshot retains the complete effective presentation, its original Page,
Magazine, Header or conditional POP source, and Local X26 presence as Absent,
PresentEmpty or PresentNonEmpty. Page and magazine second-language values are
`Declared`, including actual no-second-G0 declarations. Header/POP second-language
selection remains `DecoderDependent`, separately from the existing engine's
no-secondary policy. The source model retains all packet presence, sparse
defaults, object/DRCS/MOT bodies, resource identities and navigation unchanged.
`into_parts` moves that model and its associated snapshots into native work;
`into_original` recovers only the original model. Neither grants materialization
or publication authority. Debug output omits authored content.

These snapshots precede the checked cold composition below. The second-language
materialization gap remains explicit; copying engine policy into an X28 sentinel
does not close it. Warm replacement and publication require separate proof.

Warm replacement also requires a persistent decoder. The pinned original
[libzvbi source](https://github.com/zapping-vbi/zvbi/tree/3785481f51f41a49e28f2b7f6fd5bd9187d24ae1/src)
stores M29 in the magazine extension (`packet.c`, `parse_28_29`), which page
fetch can reuse when X28 is absent (`teletext.c`). Display C4 clears page
extensions, while an explicit channel-switch reset initializes magazine state.
Source inspection therefore does not support assuming that omission of old
M29 clears receiver defaults. MOT removal and page-X28 removal are separate
cases. Required runtime proof feeds prior and replacement generations into one
decoder without a channel reset, retaining exact original producer presentation
and resource ownership. Existing fresh-decoder fixtures and the warm DRCS trial
do not prove these removals; P9A-01 tracks that evidence gap.

### Checked cold producer composition

`TeletextColdProducerComposition::new` consumes an ordered vector of checked
producer snapshots and explicit host language/exact entry address. It retains
every original service unchanged beside the composed declaration and private
page provenance: producer ordinal, original page ordinal, exact address and
whether original Magazine presentation became a derived Page override.
Read-only accessors and `into_parts` preserve that association; callers cannot
forge the original provenance. Debug and errors omit authored text.

One common complete M29 declaration survives per magazine only when all original
declarations agree and no page originally relies on Header or POP fallback.
Otherwise original Magazine-dependent pages receive complete derived X28
presentation, including raw charset selections, all palette colours, remapping,
background substitution and panels. Actual page presentation retains precedence.
Header/POP pages keep absent presentation and decoder-dependent secondary state;
no receiver-dependent value becomes an authored sentinel. Local X26 keeps its
Absent, PresentEmpty or PresentNonEmpty state.

Duplicate exact display addresses fail, including identical bodies. Shared
resources coalesce only with identical complete identities and bodies; collisions
use raw page plus logical S1 independently of kind. Distinct S1 remains valid.
One actual MOT bank binds every display in its magazine; differing banks fail
without renumbering or inventing a receiver selector. A previously unassociated
page can adopt the common bank only when all original resolved effects survive.
The existing semantic and declaration constructors validate the complete result,
then original sparse defaults, roles, conditional POP fallback, presentation and
Local presence are checked again. Failure returns no partial candidate.

`TeletextProducerCompositionBudget` bounds raw producers, nested input items,
content bytes, commands, clone/index storage and composition work before copies;
duplicate banks still consume raw limits. Its separate semantic and declaration
budgets count the actual coalesced output. The complete encoder must subsequently
validate lifecycle records, aggregate encoding limits and actual physical headers.
Logical composition does not guess a resource header's counter/nn subcode or ban
every shared raw page number.

The focused domain/FFmpeg regressions cover contrasting complete M29 themes,
native common M29, Header/POP absence, exact identities/navigation, resource and
MOT conflicts, unchanged sparse defaults and raw/output bounds. Complete encoding
of two synthetic themes preserves each original shared-model raster exactly,
including different palette/panel output, without adding Local X26. A logically
valid shared raw page is rejected by the final physical header collision check.
This is cold, unpublished model/encoder evidence. Independent decoding of the
mixed result, suitable-client display/navigation,
safe warm replacement and explicit publication remain required.

### Warm magazine-default cache diagnostic

The opt-in Linux x64 `teletext-magazine-cache-decoder.py` diagnostic consumes
two exact captures of production-encoded units under a closed 2 MiB input:
`expected_version`, `seed_units` and `target_units`. It accepts no caller oracle,
reset, replacement byte or expected palette. The frozen resource ABI is
authenticated and executed from the same once-read source bytes; the actual
mapped library must be the receipted original libzvbi 0.2.42 (`2991c13d…`).

The seed is the actual magazine-2 M29/0 emitted by
`complete_carousel_active_and_adaptive_resource_c2_decode` from the authored
CLUT2[0] declaration `[1,2,3]`. Its exact 46-byte SHA256 is
`cc165638bc162755447098e3f9f73748a8d80b3d0c1d610e77e4d54edd13e9d6`.
The unchanged 28-unit target comes from
`production_display_units_decode_active_override_and_original_fallback`:
page 200, subcode 0x1234, C4=1, no X28/M29 and its ordinary closing FF header.
Its concatenated-unit SHA256 is
`3ac04da12333307f13f30850a159a1c49dd0dfd7cd490a6a8a9782d0b1ad5ba8`.

One decoder receives these internally fixed checkpoints with increasing decode
timestamps. Each fetch is released before the next feed; no channel reset occurs.

| Checkpoint | Actual units fed | Independent CLUT2[0] RGB literal |
| --- | --- | --- |
| Initial cold page | Target's 28 unchanged units | `[255,0,85]` |
| M29 declaration | Captured M29, then the same target | `[17,34,51]` |
| M29 omitted | The same target, without M29 | Cached `[17,34,51]` |

The default palette literal comes from pinned `packet.c` and the nibble-expanded
M29 literal from the original authored declaration. The diagnostic separately
checks unchanged Base Z at (1,9) and B at (2,1). It does not validate the older
target's enhanced Active-A override: its unchanged positive oracle retains the
known ordinary-compiler gap. No oracle or encoder is relaxed for this diagnostic.

The bounded request is 3,235 bytes, SHA256
`74209e42be63f8f04116834f860a4c72a299041763e46502c3e742a8887f326c`.
Helper SHA256:
`e8f211de3d0ecde0c880210d7ac0ba77dfb20614804dc32f3fac29788bbed55c`.
Contract-test SHA256:
`23f09e5addf8c94ee16eb93884a07c3ba053fb0889685a4ec2a3e70edf85717f`.
The contract suite embeds those exact synthetic captured units. From the repo
root, reconstruct the request without requiring earlier local artifacts:

```python
import json
from pathlib import Path
import runpy

fixture = runpy.run_path("scripts/e2e/teletext-magazine-cache-decoder_test.py")
request = fixture["request"]()
Path("magazine-cache.json").write_text(json.dumps(request, separators=(",", ":")))
```

Alternatively, verify the archived source captures and retain their original bytes:

```python
import hashlib
import json
from pathlib import Path

source = Path("resource-c2-request.json").read_bytes()
target = Path("compiled-display-request.json").read_bytes()
assert hashlib.sha256(source).hexdigest() == "0ee3aff7c8e60610dcbd6fe432080b559a437c57218cb142489ee2ecea4ba8cc"
assert hashlib.sha256(target).hexdigest() == "577f0815b8e72e60c832fec6a2ad9aadee98d1c55c59ca7c9bda05786b1b2be3"
request = {
    "expected_version": "0.2.42",
    "seed_units": [json.loads(source)["cases"][0]["units"][0]],
    "target_units": json.loads(target)["cases"][0]["units"],
}
Path("magazine-cache.json").write_text(json.dumps(request, separators=(",", ":")))
```

```bash
LD_LIBRARY_PATH="$proof_dir/original" \
  python3 scripts/e2e/teletext-magazine-cache-decoder.py < "$fixture_dir/magazine-cache.json"
python3 scripts/e2e/teletext-magazine-cache-decoder_test.py
```

Passing this diagnostic confirms cached M29 survives omission despite page C4.
It does not prove safe removal, full service replacement, MOT/page-X28 removal,
transport timing, client display/navigation or full Level 2.5 acceptance. The
85-unit feed uses a captured M29 seed rather than pretending to publish a prior
complete service. P9A-01 owns measured evidence and the remaining safe-replacement
work; both `removal_safe_proven` and `client_conformance_proven` remain false.

### Channel-wide original document preparation

`ApplicationServices::invoke_channel_teletext_documents` captures one ordered
enabled-channel snapshot under one original 30-second deadline. An On selection
requires an explicitly selected entry addon; an empty selection returns private
Off input. Channel identity and unique addon membership are checked before any
producer starts. At most two guide templates are preflighted under their exact
selected v1/v2 schemas before serial invocation. Correlation remains opaque and
subject to the existing byte limit, including Off preparation.

Each producer uses the existing installed-profile invocation with that original
deadline. Exact binding/contract/installation/settings and generation must match
the returned receipt. The per-response wire cap and trusted cumulative retained
stdout limit (`TeletextChannelDocumentInputBudget::max_response_bytes`) are
checked before retaining a response; failure stops later producers. This raw
response budget is distinct from the separately capped guide templates, semantic
validation budgets and total process memory. A fresh complete channel read fences
membership, including additions, removals and reordered bindings.

The private input's synchronous `validate` returns ordered original documents
under their respective contracts, without lifting or merging them. Hosts run
validation off async workers. Any failure rejects the complete result, with a
numeric binding ordinal and no raw producer text. Prepared values retain the
whole selection, generation, entry, deadline and document correspondence; empty
documents remain Off. `recheck_channel_teletext_document_selection` observes the
complete snapshot again after CPU/native work without resetting the deadline.
The native channel bridge below applies aggregate semantic/encoding bounds.
Generation ownership and explicit publication remain separate required steps.
The existing base-only channel API still rejects v2 before starting any producer.

### Selection-bound native channel documents

`prepare_channel_teletext_document_candidate` invokes the complete enabled
selection through the original application path, then joins one blocking worker
for document validation, lossless v1 lifting or moved v2 declarations, producer
snapshots, checked cold composition and complete encoding. Every stage retains
the original deadline. After CPU work the application rechecks complete channel
membership/settings/contract/installation selection and the host checks expiry
again. Failure returns no partial candidate; preparation writes no configuration
and issues no playout notification. Caller abandonment does not interrupt already
running bounded synchronous work.

`TeletextChannelDocumentEncodingPolicy` keeps cumulative raw response limits,
per-document validation, v1 region, aggregate cold composition, complete encoding
and explicit resource lifecycle inputs separate. Producer count and producer-vector
storage are checked before lifting. The host never infers counters, Changed state
or last-subpage flags from original content or a prior encoded service.

The selected entry addon's original service supplies the language. An optional
exact `entry` must identify one of that original producer's initial-page subcodes;
it cannot select another producer or a different initial page. Without an explicit
address, a unique initial-page member is used with its actual subcode. Multiple
members yield a located `EntryRequired` error rather than choosing subcode zero.

`EncodedChannelTeletextDocumentCandidate` privately retains the observed selection,
opaque generation, entry addon, deadline, ordered original producer contexts,
page provenance and metadata beside the complete encoded service and descriptor.
Producer ordinal still matches binding ordinal. The composed declaration moves
into encoding without cloning originals. Explicit Off retains no producers,
provenance, metadata, encoding or descriptor, ignores unused entry/lifecycle
policy and still receives the final complete-channel fence.

This is an unpublished cold candidate. A later promotion needs its own fresh
selection check, admitted generation ownership and safe output boundary. Warm
M29/MOT/page-X28 replacement, durable drafts, editor interchange, localized UI,
Lua scheduling controls, suitable-client proof and publication remain required.

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
