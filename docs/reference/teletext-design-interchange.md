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

The supplied resource selection is already resolved; it does not preserve or
parse MOT/X27 declarations or requirement masks. Default slots retain GPOP1,
GPOP2, POP1, POP2 order and sparse ordinals. A present local program, including an
empty one, suppresses every default. Local definitions require an actual base
owner and explicit local declaration. Declared links/default targets are checked
even when suppressed; their unused glyphs/charset designations do not count as
part of the effective page. Collected transitive POP/GPOP references must match
the selected links, and DRCS uses the existing complete-reference validator.

Service budgets cover unique resource records, definitions, structural items,
base content bytes, authored commands and aggregate expanded commands/glyph
references across all display subpages. Multiple objects in one resource consume
one resource record; different kinds cannot hide an exact raw-page/S1 collision.
Input bytes remain bounded before parsing. Constructors return no partial service,
and the checked result has no mutable/deserialization/publication bypass.

MOT/X27 declaration mapping and default selection, final display semantics,
physical headers/pointers/collisions and enhanced encoding remain required later
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
select the first/S1-zero MOT record implicitly. The supplied-selection service
currently checks linked page equality; authored resolution must also validate
required S1 sets and retain independently sourced default provenance. Any local
X26 presence suppresses MOT defaults.

Null-X27 fallback, conflicting MOT supersedable flags and global-default
association edge cases still need a documented, independently calibrated policy.
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
