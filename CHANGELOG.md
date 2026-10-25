# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
this project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html),
and commit messages use
[Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/).

Entries are curated for people, not generated from raw git logs. Keep
`[Unreleased]` current during development, then move its entries into a dated
release section when tagging a release.

## [Unreleased]

### Security

- Use JWT 11's supported AWS-LC backend to remove the vulnerable RustCrypto
  dependency reported by RUSTSEC-2026-0387, and remove the obsolete RSA advisory
  exception. Preserve HS256-only validation and existing token compatibility.

- Update the application Tokio runtime to 1.53.3, resolving I/O-safety advisory
  [RUSTSEC-2026-0386](https://rustsec.org/advisories/RUSTSEC-2026-0386.html)
  without changing dependency features or audit policy.

- Pin the published Next 0.2.3 worker (rs.6 on Linux/Windows, retained rs.4 on
  macOS) with fixed TLS and
  Tokio dependencies and a non-yanked crypto crate. Require the original and
  patched lockfile checksums during source preparation and packaging. Managed
  runtime adoption remains subject to compatibility and platform acceptance.

- Update certificate validation to rustls-webpki 0.103.16 and webpki-roots
  1.0.10, and ZIP handling to 9.0.5. Resolve RUSTSEC-2026-0381,
  RUSTSEC-2026-0382 and RUSTSEC-2026-0367 through RUSTSEC-2026-0369,
  retaining strict package path checks.

- Update the locked HTTPS client runtime to Rustls 0.23.46, addressing
  [RUSTSEC-2026-0346](https://rustsec.org/advisories/RUSTSEC-2026-0346.html),
  [RUSTSEC-2026-0347](https://rustsec.org/advisories/RUSTSEC-2026-0347.html),
  [RUSTSEC-2026-0348](https://rustsec.org/advisories/RUSTSEC-2026-0348.html) and
  [RUSTSEC-2026-0349](https://rustsec.org/advisories/RUSTSEC-2026-0349.html).
- Require Ammonia 4.2.4 for release-note HTML sanitization, addressing
  RUSTSEC-2021-0157, RUSTSEC-2021-0158 and CSS URL-filtering advisory
  RUSTSEC-2026-0373 without weakening sanitizer policy.

### Added

- Add bounded, playback-owned teletext inspection and observed-page selection
  for an already-running producer. Native Live add-on caption Burn now uses
  that same producer on FFmpeg 2.0.7, preserving selected audio, ordinary Off
  playback and joined cancellation. One-shot and Next source-output adoption
  retain their separate acceptance gates.

- Add owned finite source-video frame-end qualification for subtitle preparation.
  Exact decoded clocks and complete-frame bookends are checked against the selected
  outpoint; uncertain endpoints and native error diagnostics are refused. Runtime
  integration is negotiated only with compatible Next candidates; managed-runtime
  and platform adoption remain separately gated.

- Preserve bounded source teletext page transmissions and their exact body,
  subcode, update and character-context provenance through managed scans and
  SQLite database reopen. Unknown context, incomplete pages and capacity exhaustion
  stay explicit; captured data grants no decoded-page or playback authority.
  Inconsistent stored wire/context facts are rejected without losing the
  previous snapshot or ordinary audio/subtitle metadata.

- Add shared source storage admission and retained HTTP inputs for provider
  teletext caption preparation. Configurable defaults reserve at most 20 GiB
  total and 8 GiB per source while keeping 2 GiB free. Native-qualified inputs
  can supply both captions and Next AV without a second provider download;
  refused preparation preserves ordinary AV. Actual workers retain their files
  and storage claims through cancellation and cleanup. Source changes remain
  guarded; presentation-only channel edits keep existing file grants readable.

- Import a first standalone teletext Main enhancement program into a versioned
  concept. Preserve the distinction between absent and explicitly empty Main,
  retain checked edits through re-export and restart, and undo to exact original
  absence. First Main creation can retain an authenticated prefix of generated
  supplementary characters without converting those source words into authored
  commands. Publication remains explicit; original templates, declarations and
  protected page content retain their existing safeguards.

- Add localized teletext draft controls for creating empty POP/GPOP resource
  banks and announcing retained subpages on already selected pages. Explicit
  lifecycle values and current-draft checks protect each edit; multiple display
  pages can be announced together. Versioned concepts retain these changes through
  preview, replay and separate publication. The engine can also change selected
  bank pages and checked resource calls atomically while preserving MOT defaults.
  Receipt-bound native actions now save that selection as a versioned concept,
  retaining it through later TTI edits, preview, explicit publication and restart.
  Protected generated consumers remain immutable. Localized admin controls now
  select raw bank pages and explicitly retarget existing resource calls together,
  including call-only edits, without changing a running publication. Complete
  editor/client acceptance remains open; ordinary TTI permissions stay intact.
  Native-model validation can also coordinate checked Main, Local and resource
  consumers with bank selection in one complete transaction. Its explicit Main
  retarget intent preserves existing edit permissions. Receipt-bound saved concepts
  now retain these coordinated consumers through TTI edits, preview, exact root
  undo, separate publication and restart. Root undo also restores originally absent
  X27 declarations and their exact MOT provenance. Full engine/editor/client
  acceptance remains open.
  The same draft controls can select retained Normal and Global DRCS bank pages,
  preserving glyph slots, subtables and protected shared consumers, including
  equal-pixel bank changes. Exact undo restores the original design; saving a
  choice still requires separate publication.
  Named draft forms can also attach retained POP/GPOP and Normal/Global DRCS
  banks to previously unselected roles. Retargeting and partial undo preserve
  sibling choices; exact undo restores the original absent or unused links.

- Add atomic native validation for new POP/GPOP resource sets, preserving existing
  pages, defaults, fallback and dependencies. New banks and simultaneous
  definitions in original banks are validated as one complete graph. Private
  versioned concept storage
  now preserves complete new-bank definitions, source ownership and explicit
  transmission data alongside unchanged originals. Explicit receipt-bound native
  actions now save unpublished bank-set revisions and retain them through later
  TTI edits, shared preview, explicit publication and cold reload. Complete
  external-editor/client acceptance remains open.

- Accept physical teletext Alpha/Mosaic Black and Double Width/Size controls
  in imported designs. Preserve their coded rows, attribute origins, protected
  zones and basic fallback content while using the shared enhanced preview
  and encoder. Complete fallback/client and editor acceptance remains open.

- Show per-output teletext delivery observations separately from publication
  selection, including waiting, acknowledged delivery, rejection and historical
  last delivery. Observations expire with their output and do not survive a
  host restart; incomplete diagnostic coverage is reported explicitly.
- Encode explicitly authored Arabic teletext glyph forms and supplementary text
  in their original declared charset, preserving cell order and Level 1 fallback
  without requiring optional resource banks.

- Encode qualified supplementary Unicode symbols and letters in authored
  Cyrillic and Greek teletext without optional object banks. Use their actual
  G2 repertoires while preserving Level 1 fallback and Local charset changes.

- Allow native Cyrillic and Hebrew Unicode text in matching authored teletext
  pages without optional object banks. Preserve the supplied cell order,
  character context and Level 1 fallback, with explicit errors for unsupported
  characters or mismatched contexts.

- Offer a localized built-in MPEG-TS output choice in FFmpeg settings for
  authored teletext on Hybrid streams. Preserve the explicit choice across
  settings saves and restarts while retaining existing default/custom scripts
  and their output behavior.

- Let native MPEG-TS and Hybrid channels carry selected source teletext captions
  as DVB bitmap subtitles while copying video. Preserve the active caption,
  updates, clearing and selected alternate audio when capacity waiting hands
  over to the programme within the same stream. The localized delivery choice
  is isolated per output channel and rejects incompatible outputs; Burn remains
  the default. Qualified finite Local source pages can also reach Next Convert
  as page-bound text, preserving copied video, selected alternate audio and
  caption timing/clearing across seeks. Incomplete source windows and uncertain
  final cues refuse explicitly; broader source/output and full engine acceptance
  remain open.

- Show saved teletext template, theme, Free/custom-zone/generated regions and
  dynamic-field bindings beside the selected draft preview in every admin
  language. Boundaries come from the immutable original and remain read-only;
  viewing them does not change packages, concepts or the broadcast. Explicit
  addon versions can introduce linked POP/GPOP resources into new drafts while
  preserving historical originals and export receipts. New capture and explicit
  publication stay separate; complete resource/set editing remains open.

- Add receipt-bound native teletext object creation within existing POP/GPOP
  resources. Private version18 concepts retain complete added definitions and
  their later body edits without changing originals or versions1–17. The localized
  channel admin can create a first empty definition in an existing POP/GPOP
  resource with an explicit type and new ID, or copy an existing definition while
  preserving its resource, type and complete body. Both save unpublished concepts.
  A first empty added definition can now receive explicit row positions and
  glyphs through a pinned QTeletextMaker save, retaining exact complete native
  output through concept import, repeat edits, reopen and separate publication.
  Later concept imports can append rows or insert complete rows between existing
  rows while retaining the complete saved body and its position anchors.
  Pinned QTeletextMaker interior-row saves now round-trip through added objects
  and byte-identical original companions with separate current receipts, native
  preview, explicit publication and cold recovery.
  Header object calls preserve foreground and base fallback when local Active
  or nested objects share a page, including an initial header position before
  a same-instance Active call. Unrelated pointer relocation no longer causes
  false validation errors. A forward header position immediately before a
  permitted nested call now encodes inside a local Active or Adaptive definition
  when every use has the same known background, retaining the saved source and
  caller state. Forward glyphs after that nested call also preserve their original
  source, priority and Level 1.5 fallback. Editable Local child glyphs now import
  correctly beside an unchanged normalized header wrapper; compiler-owned source correspondence
  keeps derived background writes and every protected shared consumer guarded.
  Concept preview and cold reopen retain the original definitions and metadata.
  Existing Local object calls can now be moved or retargeted to another original
  definition on the same page through concept import. Supported Local definitions
  and existing page Main programs also accept same-page Local call insertion and
  removal, retaining complete original catalogues and position anchors while
  keeping generated header attributes out of authored edits.
  Main imports can combine these call edits with existing single-word scalar
  changes, preserving source lineage, protected content and explicit publication.
  Repeat edits, undo and cold recovery preserve unrelated edits; publication stays
  explicit and protected shared consumers remain guarded even when pixels match.
  Native preview,
  publication and recovery qualify new allocations and every protected shared
  consumer. A pinned actual QTeletextMaker save of an added definition now passes
  unchanged-byte import, preview, separate publication and cold reopen. Complete
  resource/set editing and full engine acceptance remain open.
- Import complete positive rows into original POP/GPOP teletext objects, including
  first, appended and interior rows. Private version19 concepts preserve original
  anchors and complete object identities separately from added-definition edits.
  Fresh-receipt restoration of an exact original body uses checked prior edit
  history and restores retained scalar changes together with inserted rows.
  Imports remain unpublished until explicit acceptance. Complete editor and
  Level 2.5 engine acceptance remain open.
- Edit original POP/GPOP teletext objects through receipt-bound QTeletextMaker
  companions, including full-capacity command fragments. Private version14 concepts
  retain scalar changes; version15 adds bounded glyph/style insertion, deletion and
  column changes with root-relative repeat/undo. Version16 separately supports
  retained object-call offsets and existing catalogue target/type changes, with
  full dependency validation and protected-writer checks even for identical pixels.
  Version17 separately records inserted/omitted resource calls. Receipt-bound V8
  import combines call addition/deletion with bounded glyph/style insertion and
  deletion, glyph column changes and same-family G0/G1 value edits. Complete callee
  identities, fixed anchors and every shared consumer remain checked.
  Existing foreground/background/attribute values can also change beside call
  additions/removals; changed styles retain no protected-writer authority.
  Rejoined version7 fragments now also support native resource-call addition and
  deletion in large objects, retaining complete original identities and protected
  consumers without reducing native capacity. Actual pinned editor saves now also
  prove native call addition/deletion in a nonfirst large-object fragment through
  unchanged-byte concepts, shared previews and explicit publication/cold reopen. A pinned
  QTeletextMaker 1.0 save combining a G0 change with a new nested call passes
  unchanged-byte concept import, shared previews, unpublished reopen and separate
  explicit publication/cold reopen. An actual compound save now also proves
  glyph/style insertion, a glyph column change and an added call through the same
  boundaries, preserving the complete original callee context. Dependent
  display carriers can be repacked after authenticated resource edits; uploaded
  display words remain bound to their original export and rejected words retain
  their exact X26 location. New/retired subtrees cannot inherit protected content
  authority. Original invisible row-zero position anchors now survive bounded
  resource glyph/call edits, including leading, multiple and known-position
  call-adjacent anchors, without inventing broadcast words. Remaining position/
  context semantics, full definition/set changes and oversized editor context
  remain open.
  Pinned QTeletextMaker 1.0 context companions now render nested resource objects
  through protected editor-only Local aliases. Origin-modifier edits preserve
  original native identities through concept preview, explicit publication and
  database reopen; copied callee context remains readonly. Complete oversized
  context and coordinated resource/link/set editing remain open. Cross-resource
  targets must agree with the selected POP/GPOP links; equal native call selectors
  do not grant authority to choose another resource page.
  Complete native reconstruction, exact resource/subpage identities and permissions
  for every shared consumer protect generated content. Import remains separate from explicit publication;
  earlier receipts and native capacities retain their contracts. Complete dependency
  views, resource/set interchange and full Level2.5 acceptance remain open.
- Import same-family G0 text, G1 mosaics/alphabetic characters, G2, G3, DRCS references,
  flash/charset settings and G0-with-diacritic changes in existing page-local and
  Local-object teletext commands into versioned concepts. Local colours/attributes
  use version 5,
  extended characters version 6, G1-alphabetic/G3 edits version 7 and DRCS slot
  edits version 8, flash/charset edits version 9 and Normal/Global subtable
  selection edits version 10; earlier formats keep their original contracts.
  Shared DRCS selectors update every affected reference, with full resource and
  protected-consumer validation. G3 compatibility stays fixed. Exact command
  provenance, immutable object/invocation structure and permissions for every
  shared panel consumer protect generated content. Repeated imports and undo
  preserve originals and remain separate from explicit publication. Protected
  charset context is checked even when a change produces identical pixels. Actual
  pinned
  QTeletextMaker G0/G1/G2/G3 glyph, diacritic-selector, Normal DRCS slot,
  Local foreground, flash/charset, DRCS scope/subtable, Local G0/G1 structure
  mixed DRCS deletion/G2 insertion and Local foreground-command insertion saves
  pass concept import,
  shared preview and reopen.
  Prohibited G2/G3 positions remain refused without changing publication; full
  editor/Level 2.5 acceptance is open.
  Exact-original Local G0/G1 insertion, deletion and column changes now use
  private version 11 concepts with bounded survivor correspondence, authenticated
  marker/pointer relocation and protected shared-consumer lineage. Explicit version
  12 extends structural Local edits to G1 alphabetic, G2, G3, diacritics and DRCS
  glyphs while preserving earlier formats. Version 13 adds inserted/deleted
  foreground, background and display-attribute commands against original
  Free/Zone permissions, retaining earlier concept contracts and shared-consumer
  protection. DRCS selectors retain complete checked
  resource references and independent definition-local scope. Complete native
  reconstruction preserves original documents and keeps import separate from
  publication. Complete command, resource/object and editor/Level 2.5 acceptance
  remain open.
- Import existing local teletext colour/attribute edits into versioned concepts,
  with exact command correspondence, complete native validation and protected-zone
  checks. Repeated edits, previews and reload preserve original documents and
  fallback packets. A pinned QTeletextMaker colour-edit trial now connects
  localized concept previews, explicit publication, ordinary-stream delivery and
  two-theme client navigation. An additional recorded DRCS bitmap edit/save and
  reload calibration preserves singleton resource identity and function. Checked
  bitmap concepts now retain authenticated original resources through native
  import, shared preview, explicit publication and undo. Singleton V3/V4 resource
  edits protect every shared display consumer and preserve fallback packets;
  retained actual QTeletextMaker output now passes the native publication flow.
  Receipt-bound resource-bank imports now reconcile editor numbering while
  retaining native identities, protected packets and undefined slots. Actual
  dense and padded editor saves reach shared preview, explicit publication and
  reopen without modifying installed packages. Dense bank saves now have headed
  nine-culture admin import/preview/publication proof through the same ordinary
  streams, independent bitmap/fallback decoding and two-theme client navigation.
  An isolated test decoder now qualifies unchanged sparse normal-DRCS packets
  after two audited decoder corrections; stock client and complete resource,
  external-editor and Level 2.5 acceptance remain open.
  Sparse ordinary-stream client trials now support an explicitly receipted
  decoder candidate and verify the decoder actually loaded by each client;
  actual sparse-original and genuinely edited padded-bank flows now connect
  localized admin import/preview/explicit publication to ordinary streams,
  independent pixels/fallback and two-theme navigation with that candidate.
  Explicit page-X28 presentation now has the same genuine bank-edit,
  localized admin/publication, ordinary-stream and two-theme stock-client proof;
  native M29-to-X28 handover now preserves existing ordinary viewers and actual
  VLC decoder instances through separate-design bank publication, with verified
  loaded-library identities and before/after page navigation.
  Complete Level 2.5 and editor acceptance remain open.
  Ordinary-stream G1 calibration now proves fixed-vector fallback/enhanced
  decoding and navigation with a separate qualified test decoder, retaining the
  actual stock-decoder discrepancy. Complete object, panel and client conformance
  remain open.
- Add exact-page teletext bitmap input support to the native FFmpeg command
  builder, preserving the selected page when video, audio and captions share
  one input. A bounded native decode preflight retains the exact local source
  association through cancellation and decodes only a completed private byte
  window checked for stable PSI, exact program/PID membership and complete PES.
  Late table changes cannot hide behind truncation; real bitmap events are required. Unknown timestamps and charset provenance remain
  explicit. Exact-file source candidates now retain page language, wire roles and
  service identity; qualified decode receipts join shared native subtitle ranking
  without changing ordinary metadata. Shared native preparation releases its
  short work permit before playback; actual consumers retain exact-source
  receipts and playback admission through confirmed cleanup or quarantine.
  Qualified source captions now reach ordinary native legacy and Hybrid routes;
  finite Local-source proof preserves both audio preferences, Off/stale fallback,
  authored pages and default-capacity cleanup. Owned legacy caption producers use
  the existing pacing opt-out while the final wrapper retains its clock and bounds.
  Explicit bounded packet spans can decode pages beyond the initial scan prefix
  from the same held source, with exact offsets and omission accounting. Reaching
  EOF from a later offset does not qualify the whole source or a playback cue.
  Complete AV/PCR tails remain in captured non-EOF ranges; only an unfinished
  selected PES requires trimming. Both native consumers now retain the exact
  requested seek/end separately from source-clock and decoder timestamps.
  A bounded-memory held-source walk checks current PSI and raw PCR across
  continuing windows and rechecks the retained decoder-byte hash. Raw clock
  observations do not establish media-relative cue coverage.
  Native format and all AV start times now retain exact signed decimal/rational
  precision and their raw PES-header witnesses under that same ownership,
  without choosing a first-track origin or enabling unqualified later cues.
  Native source preparation now precedes bitmap decoding; later explicit windows
  reuse the same held source and require the requested page's actual header in
  the exact decoder bytes. Missing headers reject before launching the decoder,
  and native probe paths cannot silently substitute a different lossy filename.
  A bounded original-source demux trace now preserves native packet order,
  positions, flags and signed timestamps independently of ordinary metadata.
  Held raw witnesses now corroborate the tested default-probe program/PCR epoch;
  unknown or corrupt first packets cannot borrow an initializer from later AV.
  Exact requested times remain separate from effective native microseconds.
  Effective native-format intervals can now locate real held PCR bookends and
  decode current page headers from their bounded spans, rechecking exact request
  identity and original source/decoder hashes. Actual preceding page resets,
  PES timestamps and PAT/PMT bytes now preserve the first bitmap in tested cue
  windows; filtering must retain the exact protected reset header and timestamp.
  Descriptor discovery retains pages absent from the initial scan without
  declaring them ready. Explicit enabled-stream observations now include subtitle
  starts and distinguish the input seek origin from later presentation correction,
  using the existing source/process owner and native builder format flags.
  Missing post-seek timestamps remain unknown even if muxing emits zero.
  Final source-caption commands now carry an optional normalized physical-input
  plan through native opening and retries, sharing ownership rules with filter
  labels. Command/page mismatches retain the existing ordinary playback fallback.
  Native opening now checks generated stream selections and caption PID against
  the same held source; stale stream roles also preserve ordinary playback.
  Generated caption plans retain separate input/output seeks, duration and
  transport offset without interpreting custom options as native timing proof.
  Effective-time observations reject arguments outside FFmpeg's generated-hour
  syntax instead of claiming an interval the native parser cannot accept.
  Comparable generated seek/duration mismatches now reject before opening or
  retrying a source-caption consumer, preserving ordinary playback fallback.
  Qualified source captions now open on a separate native input aligned to the
  selected AV clock, preserving ordinary audio when playback seeks. Initial
  opening and retries validate the exact completed request; unknown policies
  retain ordinary playback. Complete inherited page context and general
  source/seek acceptance remain open.
- Add a representative two-channel teletext acceptance flow through ordinary
  legacy MPEG-TS routes, from Lua profiles and explicit publication to visible
  client navigation with isolated themes and edits. Independent transport,
  decoding and cleanup checks accompany the client proof; complete engine
  acceptance remains open.
- Let Scripted Lua validate existing channel-bound teletext profiles and On/Off
  choices through shared application controls without changing the admin base
  or activating packages. Publish cursor intervals atomically with the schedule,
  preserving target revisions, rollback, expiry and restart observations.
  Fresh Continue builds reload the channel base after reopening while retaining
  the earlier profile intervals and programme checkpoints.
  Durable Lua rebuild checks now cover Refresh, preserving and replacing Reset,
  exact On/Off boundaries and unchanged published schedules after script errors.
  Bound the actual worker and snapshot reads; active overrides cannot emit a
  stale base artifact. Localized Health explains unavailable selections and pages
  that are not ready, with headed proof in every culture and both themes. Scoped
  native preparation now combines exact published Lua and unoverridden base
  profiles, retains Off membership and transition expiry, and uses the same
  page validation, encoder and preview model. Abandoned requests retain actual
  worker admission. Exact effective originals now use independent scoped pending
  and accepted pointers, explicit Off absence and original-expiry confirmation.
  Native recovery connects synthetic scoped acceptance to version-2 live direct
  streams while retaining independent accepted base history. The channel admin
  now prepares, previews and explicitly selects effective-scope originals,
  including Lua On over base Off and genuine absence concepts. Historical
  previews survive expiry; fresh publication retains whole-scope checks and
  separate base/effective pointers. The synthetic flow is now verified through
  localized hydrated/no-JS admin forms, ordinary AV-safe streams and independently
  decoded client navigation with contrasting channel themes. Current guide
  qualification and complete admin/Lua/client delivery remain open. Scoped
  originals now share protected native editor export/import-as-draft and preview,
  retaining real Lua origins, Off membership and original expiry through versioned
  metadata. Imported concepts do not publish or change packages; explicit
  publication still requires the original current scope. A representative scoped
  QTeletextMaker edit now reaches validated preview, explicit publication and
  independently decoded client navigation without changing the other channel.
  Repeated concept imports retain earlier free/zone rows and explicit undo over
  one immutable original, including edits across separate export subsets. Each
  import remains unpublished and rechecks protected boundaries. Representative
  repeated edits now pass localized headed admin, ordinary-stream validation and
  isolated two-theme captured and direct live-client navigation. Representative
  cumulative replacement and genuine Lua Off now pass on the same warm streams,
  retaining channel isolation and ordinary playback; VLC requires teletext
  reselection after PMT replacement. Complete editor coverage, actual natural-
  expiry client clearing and engine acceptance remain open.
  Private opt-in diagnostics now connect original scoped expiry, monitor
  proposals and actual flushed output boundaries to exact written-packet hashes
  for retained-byte qualification. Native expiry now qualifies original request
  ownership, full scoped-cycle closure and independent post-ACK Base decoding,
  retaining raw UTC diagnostics and the agreed playback policy. Page acquisition
  no longer borrows matching rows from a different page. A headed natural-expiry
  trial now records old-Q removal before controls, isolated themes and navigation
  on both original clients. An unchanged valid Base now returns automatically
  after a temporary On profile ends, retaining the original MAIN-page clients,
  decoder, themes and navigation. Rejected expiry successors withdraw at the
  same safe output boundary. A real temporary Off now has qualified transport
  recovery on the original responses, with complete teletext absence in between;
  pinned VLC still needs explicit reselection after Off. Automatic Off display,
  slow recovery and broader resource/client coverage remain open, alongside
  full engine acceptance.
- Let generic teletext transport epochs begin without authored content, preserving
  original audio/video and PSI until later activation on the same clock. Require
  acknowledged PMT announcement before page data. An explicit version-2 policy
  connects bounded accepted-state recovery to existing direct stream responses,
  including initially Off, without restarting the source. Invalid selection or
  rejected replacement withdraws authored output while ordinary playback continues;
  non-transient withdrawal faults terminate the epoch. Warm two-channel client
  screens, navigation, controls, transport and source playback pass with explicit
  client reselection. Full client/resource and retained/Next acceptance remain open.
- Preserve exact physical teletext editor rows through the shared native compiler
  and preview raster, with per-page/subpage isolation and existing bounded work.
  A separate versioned private concept codec retains original addon/request bytes,
  resources and metadata. A bounded native import now authenticates retained exports,
  protects free/zone/generated effects and saves immutable concepts for the same
  preview and separate explicit publication. An opt-in headed QTeletextMaker 1.0
  probe connects an edit within a synthetic two-page set to native import,
  preview, explicit publication, reopen and ordinary stream output. Transport,
  independent decoding and headed client navigation prove two-channel theme
  isolation. A bounded live-URL probe now verifies published/reopened originals
  through two actual ordinary streams and headed clients with separate themes.
  Capacity-wait timestamp resets remain a recorded playback defect. Nonzero
  charset loss is explicitly rejected; full editor/admin and Level 2.5 client
  acceptance remain open.
- Export saved native teletext originals through a bounded host API to exact
  per-page TTI mappings and versioned sidecars, backed by retained server receipts.
  Preserve full resource/magazine packets, original subcodes/navigation and
  addon-owned metadata without changing broadcast state. Editor-only palette
  context is explicit. A versioned protected DRCS TTI companion now lets the
  external editor load indexed monochrome resource sets while retaining exact
  native identities and bytes. Explicit unavailable editor slots preserve sparse
  native subtable indices without adding broadcast resources. Modified/missing
  dependencies reject without publication; Legacy/V2/V3 receipts remain
  recoverable. Full resource editing/interchange remains open.
  Channel previews now offer localized
  ZIP export and bounded complete-set import as an unpublished draft, with stored
  channel ownership and protected-edit validation before save. Preview and
  publication retain their separate native actions.
- Capture addon-owned teletext template revisions, named themes, typed overrides,
  free areas, custom zones and generated-field bindings through negotiated v3.
  Validate exact page coverage and actual side-panel bounds; preserve metadata
  with immutable original designs without granting import or publication authority.
  Existing v1/v2 contributions remain compatible; full editor/admin acceptance
  remains open.
- Add preparatory immutable teletext export-receipt storage with exact revision,
  digest and current-head fences, required application confirmation and retained
  history after deletion or uninstall. Add a bounded TTI record codec that preserves
  payloads, editor ordinals and magazine scope without silent repair. Native
  export/import, addon-owned edit policies and the full editor workflow remain open.
- Add separate optional whole-service teletext quotas for resolution, planning,
  placement, assembly, ancestry, comparison, collection, spatial traversal,
  attribute cell updates and carrier-ID search work. Both spatial passes share credit; failed recipes retain
  their actual admitted searches. Failed work retains its
  admitted charges; exhaustion preserves the prior accepted generation. Existing
  model and wire identities remain unchanged. Production policy and complete
  compiler CPU/memory bounds remain open.
- Add bounded optional teletext startup admission that preserves exact raw
  playback when authored admission rejects before any sink-write attempt.
  Source discovery derives actual program/PCR identity, retains original arrival
  ages and shares the retained stream's memory/deadline bounds. Explicit accepted
  generation adapters combine checked host limits with real command hints and
  PSI, recheck current channel selection and retain the exact shared native
  service. An explicit direct stream route now carries recovered accepted
  generations through the actual producer, with checked channel identity and
  command ownership across retries. Ordinary final direct MPEG-TS responses can
  now select accepted originals under explicit versioned host policy, with
  isolated per-channel profiles and bounded recovery. Invalid, missing, busy or
  exhausted optional recovery preserves ordinary playback; pending drafts do
  not enter broadcasts. Health reports fresh policy validity in all nine
  languages without claiming delivery or full Level 2.5 acceptance. Automatic
  attachment also falls back for unsupported retained sources and capacity-slate
  joins. Retained accepted-generation routing and full live acceptance remain open.
- Add durable, independently fenced teletext generation selection and native
  reconstruction of exact frozen originals. Drafts and pending requests remain
  separate from accepted output; dynamic guide freshness and ongoing generation
  activation remain open.
- Strengthen preparatory teletext guide fingerprints with the exact accepted
  External JSON document bytes, including changes outside the projected window.
  Preserve historical semantic-only fingerprints without relabelling them and
  keep accepted content authoritative until guarded publication. Runtime
  activation and current guide-role freshness remain open.
- Add bounded teletext output proposals for shared complete services. Replace only
  after a confirmed cycle boundary; acknowledge changed PMT signalling before
  new page packets. Withdrawal and re-enable preserve the continuing AV clock
  and packet counters. Invalid proposals preserve the existing stream. Durable
  runtime publication, immediate-disable policy and full warm-client acceptance
  remain open.
- Preparatory teletext guide capture retains exact addon requests and a versioned
  source-context association through immutable save/reload. Source-aware pending
  requests atomically reject changed historical guide context; legacy designs stay
  readable. Live-source ownership, current Now/Next freshness, runtime promotion
  and complete engine acceptance remain open.
- Add transaction-owned XMLTV metadata reads with aggregate row/value limits
  before retrieval, including oversized data in corrupted numeric fields.
  Existing guide hydration is preserved; complete guide admission remains open.
- Add bounded accepted guide-document reads within a caller-owned database
  snapshot, checking source bytes and programme counts before allocation.
  Complete guide acquisition remains to be connected.
- Add an opt-in synthetic teletext capture and bounded client replay workflow,
  verifying unchanged video/audio plus enhanced pages and numeric navigation.
  Live generation promotion and full Level 2.5 acceptance remain open.
- Preserve the exact complete teletext service when transferring a pending
  publication request to shared stream output; live promotion remains open.
- Add bounded native teletext previews for exact page/subpage addresses, keeping
  validated navigation, located diagnostics and RGB/video/unresolved pixel roles.
- Add durable pending teletext publication requests for exact design revisions,
  with atomic checks against the live design head, complete channel profiles and
  previous request. Keep historical revision selection explicit and preserve
  requests across restart; live generation promotion remains to be connected.
- Add durable teletext original-design revisions with bounded, exact source
  preservation, atomic revision checks and retained deletion tombstones. Restore
  historical previews through the same complete native compiler without running
  removed packages or changing the broadcast. This preparatory API grants no
  editor permissions; localized editing and explicit publication remain open.
- Add typed teletext page/subpage drafts with checked identities, literal text
  and mosaic runs, style and navigation. Validate layout and internal references
  without truncating content. Encode bounded Western Latin DVB page packets with
  explicit national subsets, accents and spacing controls; reject unsupported
  text or attributes without replacing content. Add bounded serial page
  carousels, timestamped DVB PES/TS packetization, validated PMT service
  signalling, bounded source PAT/CAT/PMT discovery and atomic null-packet
  replacement. Install growing PMTs using owned and null slots with atomic
  preflight and persistent continuity. Continue checking admitted source PSI
  and PID ownership across packet windows; reject changed tables and incomplete
  sections before releasing output. Validate the declared constant muxrate
  against selected-program PCR with explicit gap and tolerance limits.
  Compose bounded byte retention and atomic output preparation with first-byte
  holding deadlines, physical packet schedules and epoch-bound sink
  acknowledgements; reject stale, late or undelivered output.
  Add bounded asynchronous encoded-output forwarding with paced packet writes,
  source/write/flush deadlines and writer acknowledgement before advancing.
  Drain complete packets at finite EOF without padding or shifting their timing;
  require an acknowledged complete cycle and reject incomplete transport/PSI.
  Repeat immutable pages without erasing previously acquired content; a new
  generation retains first-cycle erasure.
  Bind authored pages only to explicit final MPEG-TS output roles in the
  existing process supervisor. Preserve zero-output hardware fallback with fresh
  transport state; expose transport failures through the response body and
  preserve capacity/assets when process cleanup cannot be confirmed.
  Compose retained MPEG-TS output through one reserve and teletext packet clock:
  preserve original arrival ages, defer delivery until reserve/PSI/PCR readiness,
  refill between writes and drain validated short-source EOF. Enforce first-byte,
  idle-source and sink deadlines, and retain cleanup uncertainty through startup
  and hardware retries. Reject incompatible rate, holding or allocation policies.
  Admit native legacy wrappers and their first encoders atomically with distinct
  leases. Bind private concat/item callbacks to one wrapper owner and transfer
  its encoder lease once. Fence abandoned startup, body disposal and hardware
  retries while awaiting cleanup; quarantine uncertain processes independently.
  Report insufficient limit-one legacy capacity in Health with localized guidance.
  Synthetic multi-program and real looping-wrapper audiovisual tests preserve
  programme packets, incoming audio languages, source captions and decoded
  frames/timestamps. The Linux production-route fixture verifies owner-bound
  admission, cancellation, selected audio and authored pages. Full routed source
  captions/alternate audio, compatible slate/programme handover, native platform
  acceptance and generation publication remain pending.
  Prepare bounded interpretation of explicitly supplied packed teletext
  descriptors, preserving page addresses, language order and raw types. Report
  stale unpaired metadata without altering ordinary tracks; automatic acquisition,
  page discovery and scanner integration remain pending.
- Prepare explicit MPEG-TS teletext metadata associations from supplied FFprobe
  JSON, retaining global stream indexes, reported PIDs and every program
  membership alongside packed page descriptors. Missing or conflicting auxiliary
  metadata remains visible without changing ordinary audio and subtitle tracks;
  automatic acquisition and source-byte verification remain pending.
- Prepare typed, report-only source teletext metadata with additive SQLite
  storage for exact stream/file/version associations, reported programs and
  descriptors. Bound writes to the current managed scan lease and invalidate
  reports on existing file or stream identity changes while preserving ordinary
  tracks. Actual page acquisition and selection remain pending; an adapter
  revision does not certify immutable source bytes.
- Prepare checked raw teletext header observations from explicitly supplied DVB
  units, preserving page/subcode/control bits, correction counts and independent
  unit classification. Distinguish non-headers, termination and invalid metadata;
  complete source acquisition, character-set context and page readiness remain
  pending.
- Prepare bounded assembly on one explicitly selected teletext PID and a borrowed
  view of each complete supplied PES, checking continuity, exact duplicates,
  finite truncation, framing and optional PTS before exposing data units.
  Supported transport/header profiles remain explicit; these checks do not
  establish source acquisition, PSI conformity, complete pages, character sets
  or readiness.
- Prepare strict typed teletext guide requests and page responses, with host
  conversion to complete, structurally validated page drafts. Reject oversized
  input, duplicate JSON keys, missing required fields and mismatched generations
  while preserving literal content. Bound read-only guide requests, channel and
  occurrence counts without truncation; retain explicit nullable metadata and
  precise UTC instants. Build requests from supplied grouped programmes with
  exact host locale membership, actual segments and shared current/next/continuing
  roles. Keep full-guide limits separate from Later Today; reject inexact source
  offsets without changing content. Prepare civil guide windows through the
  shared programme clock across daylight-saving boundaries and skipped dates. Guide
  acquisition, encoding validation and publication remain pending. Add a
  preparatory owned command runner with one execution deadline, bounded pipes
  and cancellation cleanup across caller abandonment. Keep uncertain cleanup
  distinct. A dedicated adapter checks enabled installed packages under the same
  deadline and retains native resolution jobs after cancellation; generic invokers
  still refuse teletext.
- Prepare named teletext profiles and channel bindings with isolated settings,
  revision checks and retained unavailable selections. Shared application
  controls can turn a binding Off even when its package or profile is unavailable.
  Profile-specific execution uses the effective settings before native-tool
  resolution, checks selection identity around execution and returns only complete
  structurally validated unpublished page drafts. Compose multiple bindings with
  an explicit entry page, one shared producer deadline and complete membership
  checks; reject page collisions and channel-wide resource overflow. Native
  preparation encodes the whole candidate and retains the same immutable logical
  pages, with no partial result after a later encoding failure. Saved channels now
  expose localized native profile forms: manifest-defined overrides, separate
  inherited and empty values, revision-fenced selection and unavailable-profile
  Off. An explicit static-page action now validates selected profiles, saves new
  immutable originals and previews exact saved page/subpage identities through
  the native compiler, with located failures and preserved publication/playback.
  Fresh authoring policy and input remain separate; absent/invalid limits disable
  only authoring and Health reports configuration without broadcast claims.
  Separate native actions now request pending publication and explicitly select
  that exact original for new compatible streams, preserving observed revision
  and pending/accepted-pointer fences. Historical previews retain their pages
  when the original profile selection becomes unavailable, with publication
  controls disabled. Synthetic restart checks retain both accepted themes and a
  newer pending-only design across real process replacement and reloaded admin
  views. Complete open-client shutdown remains unproven. Guide/editor interchange,
  Effective Lua originals now save their complete base/interval origins, Off
  membership, producer associations and exact source/theme metadata as distinct
  immutable drafts. Historical native preview restores them after expiry or
  profile removal without producer or Lua replay; existing base originals and
  publication/export roles remain separate. Scoped Lua publication/recovery,
  guide/editor interchange, warm activation and complete live acceptance remain
  pending.
- Functionally probe the selected FFmpeg runtime's optional teletext decoder
  with synthetic full-page and subtitle fixtures, checking exact page selection,
  text and colour bitmap output within bounded processes. Missing support is
  recorded without rejecting unrelated playback; full teletext delivery remains
  in progress.
- Show teletext decoder availability and failed checks in Health with translated
  guidance for every supported culture. Include the probe outcome, runtime
  generation and executable checksums in the troubleshooting report.

### Changed

- Allow UCU-12 acceptance with explicitly deferred macOS runtime work. Keep
  outstanding macOS FFmpeg/Next builds and playback acceptance unverified;
  retain older macOS package identities when adopting qualified corrections on
  Linux and Windows.
- Add focused CI recovery selections for the Level 1.5 Rust runtime job and
  the failed source-caption/Next-viewer browser cases with their interrupted
  serial followers. Retain existing test assertions and unrelated passing CI.

- Make the Native source-caption browser fixture explicit about its engine,
  preserving its localized save/rejection checks when fresh channels use Next.

- Add an audit-only CI recovery mode with an independent concurrency group.
  It reruns dependency checks and their fast workflow safeguards without
  cancelling a full run or treating partial success as full CI acceptance.

- Select the verified ErsatzRS-ffmpeg 2.0.7 package line for managed-runtime
  fallback and pinned runtime CI jobs. Retain existing operator pin behavior;
  macOS builds and approved hardware acceptance remain explicitly unverified.

- Retain local troubleshooting Download Results without adding diagnostic
  uploads, under the approved ADR 0129 parity deviation. Native remains a
  supported engine; users choose whether and where to share the archive.

- Allow the local closeout wrapper to run its doctest component separately
  after focused failure recovery, without repeating unaffected unit tests.
  The default closeout still runs both stages.

- Clarify the debug frontend/server component-erasure requirement: successful
  asset downloads alone do not prove that native admin controls hydrate.

- Enforce an exact compiler version for each reviewed Next source candidate.
  Next 0.2.3 packaging requires Rust 1.99.0; existing source packages retain
  their Rust 1.97.1 requirement. The application's toolchain is unchanged.

- Add a bounded regional-canvas renderer for the Next graphics integration.
  Watermarks retain full-output placement and opacity timing; mutable authored
  graphics are clipped to the published rectangle. Runtime adoption remains gated
  by the UCU-12 integration checks.

- Record the remaining Next one-shot video-copy failure: keyframe preflight can
  consume the single-use source before playback. Successful delivery remains an
  acceptance gate; verified caption refusal does not establish playback safety.

- Keep the experimental Next teletext Burn runtime unaccepted after a reproduced
  source-outpoint overrun; stream, selected-audio and storage cleanup checks
  remain separately qualified.

- Track Phase 9A acceptance through finite, separately closable milestone
  dependencies, with preserved handoff history separate from current work.

- Separate teletext Level 1.5/shared and Level 2.5 Rust CI checks, and split
  Playwright into general and both teletext suites for Chromium and Firefox.
  Teletext-only recovery can rerun either level without rerunning unrelated
  checks; focused results remain distinct from complete green CI.
  Build the selected CI test executables and matched server/WASM site once,
  then run each suite from verified same-revision artifacts without rebuilding.
  Relocatable fixture and runtime-manifest paths preserve execution on another
  runner; missing or corrupt build products fail explicitly.
  Same-host runners copy digest-verified artifacts through a bounded local
  cache, retaining GitHub downloads as a fallback and all existing validation.
  Completed oversized Cargo caches are cleared before consumers extract tests.
  Native Next acquisition tests receive the same pinned archive and manifest
  as runtime staging, preserving their identity and health checks.
  Source-caption runtime tests use independent retained evidence directories,
  preserving parallel isolation and the existing exact timing assertions.
  Native export and workload checks follow the versioned TTI companion and
  complete object-header work contracts while retaining strict boundaries.
  Browser checks use repository-owned disposable databases and retire only
  fixture-owned Scripted playouts while retaining immutable publication history.

- Require finite agent work packages with explicit closure conditions, scoped
  completion, evidence reuse and separate external blockers, preserving all
  acceptance requirements.

- Define separate acceptance for standalone Level 1/1.5 teletext before completing
  Level 2.5, including applicable X/26 data and the full editor-to-viewer flow.
  Preserve shared safety gates and full Phase 9A requirements; production guide
  content still waits for complete independent engine acceptance.
- Allow the existing macOS Next candidate workflow to select the reviewed
  caption source-out correction without changing accepted runtime downloads.
  Candidate building and verified runtime adoption remain separate gates.
- Allow local Linux Next candidate builds to select a reviewed source manifest
  while preserving the accepted runtime defaults. Keep exact patch and package
  provenance; candidate builds do not activate or publish a runtime.

- Adopt the published Linux/Windows Next rs.6 packages. They cap copied-video
  captions at the original subtitle source outpoint while retaining keyframe
  and pass alignment, and preserve the selected audio stream's metadata through
  copy and filtering.
  Keep macOS on its original rs.4 package identity, without claiming these
  fixes there. Verify each platform's version, release tag and patch provenance
  together during acquisition and staging.
- Clarify the [Phase 9A/9B authoring boundary](docs/adr/0123-level-2-5-teletext-and-external-editor-workflows.md#phase-9a9b-ownership-boundary)
  and F2 scope while preserving all generic engine acceptance requirements.

- Store local DVB teletext descriptors and bounded actual header observations
  alongside ordinary film, video, music-video, television and extras metadata.
  Keep reader ownership through cancellation, bind headers to stable current
  PSI, and recheck the held source and scan claim inside each media transaction.
  Auxiliary failure preserves ordinary tracks; lease loss rolls back the leaf.
  Video discovery includes MPEG/TS suffixes. Source-page bodies, charset,
  complete inventory and playback selections remain open.
- Allow manually dispatched Next-only and bounded browser recovery CI for
  corrective commits without cancelling a running full check. Focused runs
  include complete affected serial groups and remain distinct from complete
  CI evidence.

- Start HLS-to-MPEG-TS playback with a six-second read burst in the generated
  wrapper and bundled Unix/Windows scripts, then resume realtime pacing.

- Replace the archived YAML parser with the maintained YAML organization fork
  across scheduling, media scanning, graphics and script handling (ADR 0125),
  preserving existing YAML models and persistent state.

- Pin the supported Rust compiler explicitly in reviewed Next source builds,
  and verify the updated upstream Windows dispatcher checksum for Next v0.2.1.

- Adopt the published, checksum-pinned six-platform Next v0.2.2 dependency,
  preserving converted-caption cue intervals, timestamp maps and WebVTT settings
  across HLS segments. Retain the demonstrated RTSP/image corrections and source
  provenance. Linux/Windows startup and decoded playback pass; approved
  hardware/ARM/macOS application acceptance remains explicitly unverified.

- Rebase the reviewed Next runtime to v0.2.1 across all six platforms, retaining
  the demonstrated RTSP and scheduled-image corrections and exact package pins.
  Verified public packages support automatic acquisition, bundled startup and
  managed restart on Linux and Windows. Approved hardware and ARM/macOS
  application acceptance remains explicitly unverified.

- Read structured Next diagnostics with severity, channel and FFmpeg context,
  retaining older text logs, bounded retention and credential/path redaction.
  Flatten control characters so a message cannot forge another diagnostic line.

- Expand Phase 9 to authored EN 300 706 Level 2.5 with correct base fallback and
  QTeletextMaker TTI import/export, versioned template/theme/zone metadata and
  separate drafts and explicit publication. Require independent decoding,
  TSDuck and real display/navigation in a suitable client; Kodi is optional.
  Checked colour/resource values, ordered enhancement programs and bounded
  object-derived DRCS/charset dependency checks provide the model foundation.
  Page/magazine presentation and supplied-selection service validation now retain
  panel metadata, default suppression and aggregate dependency budgets. Authenticate
  complete service candidates against actual MOT/X27 declarations, one MOT bank
  per magazine, all sparse defaults and required resource subpages. Preserve
  genuine empty companions and original POP presentation fallback. Compile every
  display/resource with shared pointer reservations, explicit lifecycle records,
  aggregate bounds and final physical inventory; emit M29 once. Independent full
  compiler decoding retains both the original X27 defect and corrected test-only
  result. Reuse the bounded carousel/output epoch for complete services, with
  magazine/resource acquisition order, immutable C4/C8 repeats, exact cycle
  timing and display-only signalling. Independent carousel decoding and TSDuck
  checks pass. Refresh complete resource/M29 sweeps between whole display pages
  with preserved display order, repeat controls and generation boundaries. Larger
  display cycles now use bounded resource deadlines with a physical-window margin;
  paced-output and independent TSDuck checks include shifted null slots and
  premature EOF rejection. Check resource replacements against completed wire
  content and modulo-eight counters, preserving pending candidates, retired owners
  and legacy-page reuse without advancing history on rejection. Independent
  warm-cache DRCS decoding now verifies changed and unchanged complete generations,
  exact glyph pixels and counter wraparound. Add strict unpublished v2 authored
  input with mandatory fallback, original resource/presentation declarations,
  trusted work budgets and authenticated resolved assertions. Pin the highest
  supported installed profile contract through exact request, process capability,
  manifest and receipt checks; base-only preparation rejects enhanced selections
  before any producer starts. Add bound authored validation and complete native
  encoding with explicit host lifecycle, shared deadlines and fresh selection
  checks; the same checked model remains available for preview. Preserve complete
  producer-local presentation/provenance and decoder-dependent second-language
  defaults beside native candidates. Prepare channel-wide original v1/v2 documents
  under one membership snapshot/deadline with cumulative raw response bounds;
  retain explicit Off without interpreting an unused guide. Add bounded cold
  producer composition with complete derived page themes, preserved originals,
  shared resource authentication and unchanged preview. Prepare complete native
  channel candidates with original entry/language, one worker/deadline and fresh
  whole-channel selection checks. Complete authored bindings now retain M29,
  displays and resources through existing paced/retained final MPEG-TS forwarding,
  sharing immutable content with independent output epochs. Normal-route generation
  lookup, warm replacement, live publication, durable restart and client acceptance
  remain open.
  Breaking Rust API: `TeletextOutputBinding::pages()` now returns optional base
  content; use `content()` for complete authored bindings.
  Retain exact validated producer response bytes and schema in original binding
  order beside native candidates, preserving the actual V1 lift region without
  reserialization. Durable concept storage and explicit publication remain open.
  Add a pinned persistent-decoder diagnostic confirming that M29 palette defaults
  survive omission despite page erasure; safe replacement remains required.
  Final resource command spans now settle same-instance attributes
  in native preview and both normalization passes, with exact body/allocation
  correspondence and original default provenance. Resolve glyphs when all
  applicable charset aliases share one exact repertoire pair, retaining raw
  source uncertainty and prior history. Complete encoding/raster and independent
  decoding pass; different repertoires still reject. Cross-instance uncertainty,
  hosted transport, durable update history and full acceptance remain open. Headed
  pinned-QTeletextMaker edit/save/reopen calibration records actual identity
  normalization and unknown-record loss; full host interchange remains open. Verify
  the existing whole-display 127-invocations-per-type admission across sparse
  defaults, nested calls and independent subcodes; physical replay must reuse it.
  Trace a supplied physical program with the shared bounded spatial executor,
  preserving augmented command ordinals and original service default tags.
  Resolve augmented-program DRCS sources with fresh bounded dependency checks
  while retaining original service links and bitmap authority.
  Replay physical page projections through shared resolution, preserving the
  original target and explicit encoding discrepancies for later comparison.
  Bind physical replay to final packed words for same-emitter attribute order;
  absent resource positions and cross-instance ordering stay unresolved.
  Render physical replay with shared glyph/fragment/colour helpers, showing raw
  Base fallback when authored Unicode instructions are absent and retaining
  actual located diagnostics; whole-page equivalence remains unproven.
  Compare full original/physical visual planes under host budgets, retaining
  fragment/colour/flash/glyph differences and separate uncertainty histories;
  equal host masks do not certify authored identity or receiver equivalence.
  Derive bounded Passive recipes from exact original display origins and typed
  context while preserving original colour dependencies. A representative
  retained-anchor placement passes dual-level independent decoding. Derive
  whole-component DAG order and unchanged main gaps from actual composed root
  events, with checked panel/offset mapping and explicit pending components;
  build bounded candidates retaining original commands/bodies, distinct generated
  IDs and private emitter maps. The representative dual-level decoder proof now
  consumes this producer and a paired physical factory now checks complete unique
  original/carrier invocation ancestry across nested calls and sparse defaults.
  Compare selected typed identities and exact origins independently of mask
  equality, including full DRCS resources and Unicode replacement context.
  An explicit bounded same-service normalization entry now owns both models and
  requires complete typed and selected-model comparisons before returning bytes.
  The original Unicode/Active decoder oracle passes with unchanged dual-level
  literal expectations. Foreign-default ownership, row-zero lowering, live
  integration and full standards/client/editor/publication acceptance stay open.
  Elide exact already-at-`(0,0)` position identities without colour or timing
  side effects, preserving raw budgets, pointers, source ordinals and both DRCS
  latches. Other row-zero lowering remains required. Document native MOT versus
  X26 foreign-default realization obligations without weakening owner checks.
  Build complete normalized display packets from checked models, retaining actual
  metadata and original fallback, with shared MOT reservations and independent
  dual-level decoding. Whole-service and publication acceptance remain open.
  Realize proven national-option glyphs through explicit standard-table carrier
  operations while preserving original identities, provenance and fallback bytes.
  Validate an enlarged German section-sign overlap through complete model checks
  and independent dual-level decoding. English double vertical has exact Hebrew
  carrier realization with pair-budget enforcement; its pinned decoder limitation
  is recorded explicitly. Four national positions remain pending.
  Add an offline, versioned comparison of original and test-corrected libzvbi
  decoding against unchanged native-default fixtures, with source/build receipts
  and actual loaded-library verification; client acceptance remains separate.
  Preserve native no-X26 default activation, original sparse owners and actual
  page/magazine declaration presence through the shared packet writer. Reject
  X26-dependent inputs with located diagnostics; retain the mixed nested-resource
  conformance oracle despite the known libzvbi X27/4 decoder limitation. Whole
  service generation and full client/editor acceptance remain open.
  Native presentation-body coding and bounded v1 lifting preserve existing packet bytes
  and isolate page defaults. Checked X27 declarations, body coding and required
  inventory validation retain exact masks and original default-object ownership.
  Effective same-target object/glyph references require announced S1 values with
  source-located diagnostics; original default roots retain their exemption.
  Checked MOT declarations and packet-body coding preserve original resource
  ownership, full link banks/defaults, ignored fields and raw counts, with checked
  supplied selectors and located failures. Monochrome DRCS body coding preserves
  exact slots/pixels and sparse undefined glyphs with explicit X28/3 modes.
  Checked resource headers preserve exact physical identities/lifecycle states;
  complete bounded MOT/DRCS sequences derive actual last rows and keep empty
  DRCS undefined. A bounded local X26 compiler lowers typed colour, attribute,
  glyph and DRCS commands with located failures and unchanged input. Required
  row-zero lowering and base/enhancement composition remain open. Resource-object
  allocation now preserves full IDs/shared defaults, exact resource/type selection
  and invocation offsets. Complete bounded POP/GPOP pages now retain exact
  pointers, definition markers, resource metadata and independent object state;
  local objects now share one bounded X26 sequence with exact forward selectors
  and independent definition state. A bounded compiled-page inventory check now
  rejects exact display/resource header collisions and repeated logical resource
  owners while preserving distinct-S1 identities. Complete service compilation
  remains open. Exact-owner effective program access and bounded original colour
  resolution preserve default suppression and main/Active colour priority before
  composition. Private positioned base glyphs now prepare context-aware
  composition while preserving existing fallback rows and packet bytes.
  An ordered bounded main-program intermediate now retains original commands,
  invocation anchors and provenance, with compatible-glyph suppression and
  row-scoped charset restoration. Final wire/display semantics remain open.
  Same-service packing now retains original header colour, restores compatible
  body-row positioning and includes added selectors in exact local pointers and
  capacity. Complex header normalization and full receiver acceptance remain open.
  Main composition now retains checked base cells, authored glyph intent and
  spacing-attribute provenance with separate raw/remapped colours, preparing
  shared display resolution without claiming enhanced rendering or publication.
  Original object tracing now preserves exact repeated/nested call coordinates
  and sparse default provenance under bounded allocation; glyph and attribute
  resolution remain required before display or publication acceptance.
  Glyph layers now keep original base intent separate from object/local
  character precedence, retaining losing and out-of-area contenders explicitly
  for subsequent attribute, geometry and receiver-equivalence validation.
  Separate original attribute execution now preserves per-cell inheritance,
  private object state and independent panel defaults, retaining unresolved
  transmission and Adaptive endpoint diagnostics without granting readiness.
  Checked G1 alphabet positions preserve enhanced-only mode and base fallback;
  selected Latin positions decode with libzvbi. A separately recorded imager
  font limitation does not count as enhanced rendering acceptance. Full charset
  execution, non-Latin alphabet and underline proof remain open.
  Original charset contexts now retain shared row and private object lifetimes,
  exact declaration sources and every losing glyph. Ambiguous source order stays
  pending; character lookup and physical receiver equivalence remain open.
  Finite glyph lookup now preserves original repertoire positions and national
  subsets, distinguishing prohibited codes and uncertain cells/compositions.
  Glyph shapes, complete preview and publication acceptance remain open.
  Original DRCS glyphs now retain exact resource identities and checked bitmaps
  from the selected display's service, including losing object candidates.
  Full placement, rendering and publication acceptance remain open.
  Original attribute execution now retains every candidate's pre-geometry
  observation, losing Passive private state and separate shared stages; complete
  layer-aware size/obscured-attribute rendering remains required.
  Coded-base fragments now retain spacing-driven lower-row suppression and exact
  upper origins beside independent row defaults; ignored controls do not cascade.
  Layer-aware replay now retains shared and private execution, fragment origins,
  effective size and deferred writers without changing their original sources.
  Adaptive attribute-only repaint and partial higher-layer overlays remain
  distinct. Agreeing contending size flags now establish geometry while keeping
  unresolved writer provenance. A bounded raw-position call/write scan rejects
  proven successive-opening overflow, retaining intervening child writes and
  pending reset evidence. Known cells now rasterize with pinned MIT teletext fonts,
  exact DRCS, original fragments and selected palette/video planes. Authored
  Unicode keeps Base priority; unresolved cells retain located diagnostics.
  Receiver-owned headers use a distinct marker; invert precedes flash and
  enlarged bottom parts retain origin-row colours. Eight production DRCS
  rectangles match independent libzvbi pixels, including enlargement and reveal.
  Alphabetic composite fonts preserve current letter/mark families; defined
  substituted-background parameter failures retain their original writers.
  Object area and overlap checks now include original position/charset setup
  and inactive panels, retaining permitted object types and separate size fragments.
  Actual packed command positions now resolve known same-emitter attribute ties
  in the shared display/colour result; uncertain cross-instance order stays visible.
  Record a production-wire Unicode/Active priority regression with its unchanged
  Level 1.5/2.5 oracle. Bounded realization planning retains actual fallback
  overlaps, whole origin footprints and final fragments; wire correction remains
  required before acceptance.
  Complete rendering, physical equivalence, client/editor workflows and
  publication acceptance stay open.
  Known inclusive raw object intervals now reject sixteen simultaneous opens
  across all object types with original call diagnostics; unresolved intervals
  remain visible and full overlap/admission validation remains open.
  Selected panel geometry now preserves original addresses and visible positions,
  retaining unplaced candidates and original object-region provenance.
  Occupied footprints and complete rendering acceptance remain open.
  Nine pinned independent MOT/DRCS cases now prove exact pixels,
  native cell rendering and isolated invalid-glyph fallback. Eight additional
  object cases prove representative public/global types, nested origins and
  exact glyph/colour behavior. A separate pinned diagnostic records a local-object
  decoder limitation; it does not establish local rendering acceptance. The same
  correct three-case oracle also passes a second pinned independent decoder,
  with guarded sources and exact local glyph/colour inheritance. This is an
  opt-in verification tool, with no production dependency or client acceptance.
  Additional cases prove high local designations and nested local object origins.
  X27 decoder limits,
  full feature/client proof and service scheduling remain open.
  Bounded MOT/X27 derivation preserves provenance,
  original defaults and conflicting candidates. Effective defaults/presentation,
  complete enhanced-service compilation, editor
  workflows and client acceptance remain open.
- Extend Phase 9 acceptance with per-channel teletext add-on bindings and named
  profiles, add-on-owned themes/overrides/custom zones, and shared admin/Lua
  controls. Specify override timing, expiry, rollback and restart, channel
  isolation and identical validated browser/stream pages. Implementation remains
  pending; generic engine acceptance still precedes the single TV-guide add-on.
- Plan Phase 9 as a generic teletext engine followed by one new TV-guide
  teletext add-on in ErsatzRS-Addons, with pages and navigation inspired only
  by the TV-guide-related NOS Teletekst pages 200–299.
  Require independent engine acceptance before content work and integrated
  host/add-on proof before phase closeout; runtime behavior is unchanged.

- Update 21 Rust dependencies, including Leptos hydration, JSON Schema
  validation, scheduling parsers and platform process support; refresh CI
  actions while preserving the pinned Unicode case-mapping dependency.
- Default new add-on lists to Adaptive automatic refresh when the selected
  add-on supports source-change checks. Preserve explicit cadence choices and
  existing lists; providers without adaptive support retain the Daily default.
- Record the deferred official add-on distribution plan after teletext and video
  jukebox: a bundled repository, automatic repository updates enabled for new
  profiles, and retained installation from both ZIP formats. Include the
  maintainer-locked Sources design v0.1; runtime behavior is unchanged.

### Fixed

- Use software encoder controls when a refused hardware backend falls back to
  CPU encoding, so an NVENC profile's `p5` preset cannot prevent libx264 previews
  from starting. Preserve hardware controls when only filtering falls back and
  leave saved profiles unchanged.

- Supply isolated app data and the verified published-package inventory to Next
  CI runtime cases, serialize their shared report path, and retain existing
  session and FFmpeg diagnostics. Generate the literal-clock fixture for finite-cut
  tests when no explicit incident input is supplied, retaining the exact
  cutoff, cache, stale-source and installation-fence assertions.

- Stage security-patched Next packages using their prepared lockfile identity,
  and verify the private ARM64 MPP library and all four companion notices
  against exact payload hashes and sizes. Reject missing or modified companions
  before launch while preserving verification of existing runtime packages.

- Keep all Next runtime companions beside the application in release archives
  and Docker layouts, including ARM64's private MPP files and Windows' VPL files.
  Preserve the application's licence and Next's separate provenance and licence.

- Let the local TSDuck gate reuse an explicitly selected, separately verified
  server candidate when checking runtime packages, avoiding a redundant build.

- Keep requested RKMPP channels on software when the private MPP companion has
  no initialized hardware codecs. Refuse unpinned system libraries and retain
  selected audio, subtitles and stored acceleration choices during fallback.

- Preserve remote troubleshooting callbacks without requiring a saved channel
  when optional provider source retention is enabled. Preview source identity,
  track selection and revocation checks remain in effect.

- Keep native teletext test fixtures portable and explicitly document read-only
  transaction checks, restoring Windows compilation and architecture verification
  without disabling assertions or treating Linux-only decoder calibration as
  portable runtime evidence.

- Create the first channel on a fresh installation with Next, matching the
  accepted new-channel default and verified runtime acquisition. Existing Native
  selections and shared profiles retain their settings. Keep native playback
  gates independent of that default by selecting their fixture engine explicitly.

- Remove NUL characters from extracted text subtitles before admitting them for
  Native or Next playback. Repair existing subtitle-cache files once and
  regenerate older Next artifacts; retain Unicode text, selected audio and
  original sidecars. Failed repairs remain retryable with localized Health
  guidance rather than being reported as complete.

- Require a valid UTC date and time before channel troubleshooting playback,
  with localized field feedback and an explanation while Play is unavailable.
  Stop remains available when a running preview's time field is invalid.

- Keep missing stored HDR10 metadata unknown so Next can inspect PQ frames.
  Decode Emby's Dolby Vision profile labels, ignore absent-profile sentinels,
  and refresh possible provider HDR/Dolby Vision records once without replacing
  valid streams, local HDR metadata or disabled-library settings.

- Exclude Jellyfin external audio files from a movie or episode's physical
  stream list. Request a one-time rescan of affected stored items, preserving
  valid audio/subtitle selections and cached subtitle data until replacement.

- Avoid video-sized blank PGS canvases in Native playback before the first
  subtitle cue. Bind the initial canvas hint to the selected source stream,
  preserving later presentation dimensions and selected audio.

- Preserve native teletext fallback cells for qualified Romanian, Polish,
  Serbian/Croatian/Slovenian and Latvian/Lithuanian text and the Greek numeral
  sign. Support standard filled rectangles and native supplementary alpha in
  their declared character sets, retaining established Latin G2 spelling.
  Encode assigned spacing accents without optional object banks; keep original
  text, explicit G0/G2 spellings and distinct script/diacritic identities.

- Compile an Adaptive teletext object over the right half of an inherited
  double-width character without a false unresolved-display error. Preserve
  both lower fragments, the complete object definition and Level 1/1.5 fallback.

- Enforce teletext's limit of 15 simultaneously open objects even when their
  known source coordinates fall outside the visible page. Preserve valid
  intervals and source-located validation errors.
  A colour instruction can no longer hide sixteen successive object openings
  in the same source; uncertain cross-source ordering remains explicit.
  A later child-object write also cannot hide its invoking Main call from
  the limit.
  Preserve a known opening prefix inside that child even when a later write
  falls outside the page, retaining its original source-located refusal.

- Encode Main header context with original POP/GPOP objects using the existing
  coherent resource-before-display acquisition order. Preserve original object
  identities, complete fallback packets and strict standalone validation.

- Preserve native teletext colour order when one original Local object is
  invoked on separate rows. Both instances keep one exact definition and the
  Level 1/1.5 fallback; ambiguous or foreign word mappings remain rejected.

- Preserve specific packet positions when rejecting teletext resource edits
  after alternative validation fails. Admin feedback retains the resource and
  offending word without changing validation or publication safeguards.

- Preserve original Local-object references when importing a Main charset change
  around generated Base characters. Complete native context validation now
  handles retained Local definitions and pointer relocation during repacking,
  while preserving protected objects, fallback, exact root undo and explicit
  publication safeguards.

- Correct the teletext reference to describe the operational export, concept
  import, validation, preview and explicit-publication workflow for admin users
  and agents. Preserve the distinction between completed editor actions and
  remaining engine, client and platform acceptance.

- Prevent a qualifying Next candidate from consuming a one-shot producer during
  Copy keyframe preflight. Use facts from its already-owned prefix with explicit
  runtime negotiation, preserve one consuming pass and joined cleanup, and retain
  source-bound captions through the existing published prepared-media workflow.
  Managed-runtime adoption remains separately gated.

- Preserve every complete source-video frame before a selected finite outpoint
  in compatible Next caption-Burn candidates, including the previously truncated
  seek2164/out9200 interval. The source-qualified cutoff precedes framerate
  conversion and preserves the selected encoder, audio, full-source caption
  cache, exact caption clear and bounded ownership/cleanup. Older callbacks and
  unqualified observations retain the existing output policy; managed-runtime
  adoption remains a separate acceptance gate.

- Retain source teletext observations when DVB PES headers carry valid optional
  fields, including DTS and MPEG extensions. Preserve the exact PTS and page
  units while rejecting malformed fields outside the fixed header boundary.
  Discover every program on a shared PMT PID with independent versions and
  complete membership checks, retaining ordinary audio/subtitle metadata.
  Fresh held-source inventory accepts legal initial discontinuity flags without
  resetting later continuity or treating interior decoder windows as new sources.
  Read M2TS sources without stripping their four-byte packet prefixes: inventory,
  caption snapshots, source hashes and native clock positions retain exact frames.

- Reject unqualified teletext stream selections before Next Burn preparation,
  matching Convert's existing page-bound refusal. Qualified source captions and
  ordinary subtitles retain their routes; explicit Off preserves selected AV.

- Provide an opt-in Next candidate that preserves the incoming video clock
  while burning text subtitles after a seek. The qualified retained-source HLS
  interval preserves exact caption transitions and selected alternate audio.
  Another seek still omits a complete final frame before the video outpoint;
  correction and managed-runtime adoption remain separate.

- Keep Health results in the selected admin language after a hydrated refresh,
  including recovery from invalid teletext source-storage settings.

- Keep Next channel capacity, session resources and output reserved when process
  or prepared-producer termination cannot be confirmed. Revoke further callbacks
  while retaining ownership, and release confirmed failures normally after the
  final playback checkpoint.

- Retain the selected Jellyfin/Emby media source with its exact scanned file and
  use it for authenticated Next playback instead of requesting the provider's
  default variant. Different versions can share a provider path without taking
  over each other's file identity. Source changes invalidate caption observations,
  prepared cache identity and stale media requests; existing files without a
  selector retain ordinary playback.

- Preserve native teletext descriptors and observed page headers during managed
  Jellyfin, Emby and Plex scans of explicitly mapped files, including targeted
  show refresh and Plex Other Video. Bind reports to the current source, stream
  and acquired scan lease, replace stale observations on deep rescan, and retain
  ordinary subtitles and alternate audio when native enrichment is unavailable.
  Page-qualified caption preparation can use those mapped files without changing
  their provider identity. Current mapping and held-file checks reject stale
  sources while preserving ordinary playback and alternate-audio selection.

- Preserve source-derived steady teletext header context when removing and
  restoring Local object calls through TTI concepts. Exact root undo and
  publication isolation remain enforced; generated Flash stays protected.
- Compile the qualified Local-object header layout with changing row colours,
  preserving original object positions, steady presentation and Level 1.5
  fallback. Generated position writers use only proven unchanged attributes.
- Compile teletext header calls with competing Main and Local attributes when
  their actual transmitted words establish a definite order within one page.
  Preview and encoded output retain the original source and Level 1.5 fallback;
  unavailable or ambiguous transmission order keeps its existing safeguards.
- Preserve a retained leading Main teletext position when deleting its last Local
  calls or restoring the original calls from TTI. Authenticate the complete
  generated colour/context span without granting it authored edit permissions;
  genuine authored styles, protected consumers and explicit publication
  retain their existing safeguards.

- Import authored teletext Main charset edits together with their generated Base
  context. Preserve supplementary source text when restoration words change or
  disappear, retain exact root undo, and reject stale context even alongside
  ordinary row edits. Generated words remain compiler-owned; publication stays
  explicit.
- Restore original and newly added teletext object bodies together from a fresh
  export. Undo retains deliberately added empty definitions, exact original
  anchors and protected consumers; publication remains an explicit action.
- Retire teletext MOT banks on the same output source. Previously delivered
  populated banks receive complete neutral updates before the successor starts;
  preserve exact subpage counters and require sink acknowledgement of the clearing
  transfer separately from delivery of the successor.
- Check realtime and rolling stream delivery in the disposable TSDuck playback
  gate as well as its live-URL mode, so transport validity alone cannot hide starvation.
- Make the default TSDuck playback capture include periodic delivery samples
  after its measurement window warms up, while retaining the same capture
  duration, measurement window and transport failure thresholds.
- Reject authored teletext references to subpages outside the effective MOT
  announcement, including later concept edits. Preserve applicable X27 selection,
  original default provenance and the running publication on refusal. Complete
  coordinated bank-selection editing remains open.

- Allow Add/Copy in explicitly created teletext banks, preserving complete
  definitions and lifecycle through drafts, preview, separate publication and
  restart. Stale, occupied and invalid targets still leave existing output intact.

- Support authored hold/release mosaics while retaining original control bytes,
  glyph sources and mosaic spacing. Keep caption-box visibility and protected
  zone boundaries enforced when a held glyph depends on an earlier cell.

- Preserve row-local primary/secondary G0 selection and original ESC controls in
  authored teletext, without changing supplementary G2 or X26 context. Protect
  zone boundaries even when a charset change leaves a cell visually blank, and
  keep header glyphs separate from body national-option substitution. Complete
  independent receiver qualification remains open.

- Render the assigned Arabic teletext G0 character at position 7/E in the
  validated preview instead of reporting it as undefined. Preserve its original
  charset and coded identity without substituting a Latin glyph.

- Preserve supported native teletext glyph spellings when a QTeletextMaker G2
  edit would otherwise become canonical G0 text. Keep exact fallback and source
  context through concept import, preview, re-export, repeat/undo and explicit
  publication, without changing original packages or protected content.

- Import qualified generated supplementary teletext Text edits as concepts while
  retaining their Base role, original producer data, region permissions and
  channel isolation. Preview, re-export, repeated edits, undo and explicit
  publication use the same validated page model.

- Preserve selected audio-language metadata through copy and filtering in the
  locally qualified Next compatibility candidate; managed adoption remains pending.

- Deliver already-published teletext in built-in Next MPEG-TS output through
  bounded final-wrapper recovery.

- Bound HTTP shutdown when a client stops reading a response, including large
  admin assets, using the existing drain grace before closing stuck connections.
  Preserve normal response bytes and trusted peer addresses.

- Accept declared Greek teletext G0 Unicode text and exact coded Greek/Arabic
  header-code seven pages without optional object banks. Preserve original
  glyph identities and reject unsupported or cross-script text at its exact cell.

- Preserve valid Romanian teletext pages with explicit national charset context and
  matching header bits through shared preview and authored-service encoding, without
  requiring optional object banks or weakening legacy-context validation.
- Select teletext Unicode national tables from the complete assigned Latin charset
  context, preserving native fallback cells and explicit X/26 replacements instead
  of silently changing characters such as a pound sign into a Polish hash sign.
- Preserve a finite Local teletext caption in Next without a final clear through the
  proven selected-video presentation end, clipped by earlier programme cuts.
  Keep prior text and clocks exact, isolate caches by video and clock policy,
  and retain bounded preparation and cleanup. Linux Next copy/alternate-audio
  evidence is scoped; remaining native/output/platform acceptance stays open.
- Compile declared teletext pages whose MOT defaults belong to a different
  object bank than the selected explicit link when Base compatibility requires
  X/26. Preserve original defaults, object identities, full effects and fallback
  through bounded private physical realizations; standalone explicit calls still
  enforce their selected resource links. Broader context and client acceptance
  remain open.
- Preserve late selected teletext captions when Next converts a finite Local
  source larger than one observation window. Decode a complete owned snapshot
  with bounded memory, unchanged source clocks, selected audio and cleanup;
  reject incomplete or changing transport instead of publishing partial text.
  Select preferred pages whose first header occurs beyond the scanner prefix.
  When the complete checked source never transmits an announced page, retain
  ordinary programme/audio/subtitle selection without inventing a ready caption.
  Source, publication, deadline and cleanup failures still refuse explicitly.
  Reviewed Next candidates preserve full caption source cutoffs for either
  keyframe-landing direction while retaining shifted intermediate pass limits.
  Accepted runtime adoption remains separate from candidate verification.
- Reject native source-teletext burn-in with video stream-copy during command
  construction, avoiding an incompatible FFmpeg filter/copy command. Verified
  caption carriage for copied video remains open.
- Preserve generated MPEG-TS clocks, continuity and changed stream signalling
  across repeated capacity-wait screens and programme admission. Native Hybrid
  startup reserves its wrapper and first encoder together while continuing
  waiting output; cached playback and ordinary failure screens remain available.
  Scale capacity-wait and HLS error screens to the configured channel resolution
  using the embedded background image's actual dimensions. Complete codec/client
  and platform acceptance remains open.
- Accept structural teletext resource edits that change packet extent without
  mistaking the derived `nn` subcode for a different original resource. Candidate
  mapping preserves exact identity, update counter and transmission; original
  receipt metadata and complete native validation remain authoritative.
- Support forward teletext header positions before glyphs and object calls when
  full page validation preserves authored attributes. Keep generated position
  attributes protected during TTI import and preserve source locations, fallback,
  DRCS selections and packet bounds through concept preview and publication.
  Resolve same-Main header colour conflicts from actual encoded writer order before
  positioning an object call. Local, public and global object definitions can
  defer forward relative positions until a real column operation, preserving
  original source ordinals and pointer allocation. Uncertain cross-object context,
  unmaterialized calls and backward addressing retain validation errors.
- Preserve original unencoded AP00 and forward row-zero teletext position anchors
  during structural editor imports. Authenticate their original zero-word spans,
  retain glyph correspondence across insertions/deletions and enforce shared
  Free/Zone permissions. Concepts, re-export and undo keep original sources and
  broadcast state intact; position-order, deadline and capacity checks remain.
- Preserve original POP-fallback side panels and background substitution in
  standalone QTeletextMaker exports through an explicit versioned editor view.
  Receipt-bound import protects that presentation while keeping native X28/M29
  absence, decoder-dependent secondary selection and historical exports intact.
- Honor captured editable left- and right-panel teletext zones during bitmap imports.
  Shared glyph edits still require permission for every changed consumer; generated
  zones, Main header bytes and the original panel geometry remain protected.
  The retained genuine editor-bank flow reaches two ordinary streams with fresh
  permissions, isolated profiles and independently checked inherited presentation
  and bitmap pixels. Actual editor Main edits now preserve representative panel
  geometry and themes; full editor and client acceptance remain open.
- Preserve MOT-inherited teletext resource links when deriving exact national
  glyphs, including a free subpage behind an occupied bank. The physical page
  retains every original role and subpage requirement without rewriting authored
  MOT/resource data or granting editor access to generated glyphs.
- Realize assigned national glyphs through a free DRCS subpage when both resource
  links are occupied, preserving authored banks, fallback and link identities.
  Independent linked targets can each receive a bank within the existing caller
  budgets, preserving per-page DRCS limits instead of imposing a two-bank service cap.
  Indexed TTI banks retain separate authored and compiler editing permissions
  across independently linked pages through native concepts and explicit publication.
  Three pinned actual QTeletextMaker saves now reach receipt-bound concept import,
  shared preview, explicit publication and two ordinary streams, with independent
  decoding of each linked bank and fallback. Suitable-client and full engine
  acceptance remain open.
- Prevent teletext stream-task panics when authored DRCS banks coexist with
  compiler-generated banks; acquisition orders the actual encoded resource headers.
- Rebuild generated teletext repaint packets after authorized Base-cell edits,
  retaining exact authored operands and protected zones. A pinned QTeletextMaker
  national-cell save now reaches native concept import, shared preview, explicit
  publication and database reopen without changing installed packages or the
  other channel. Ordinary delivery now passes source audio/video, transport and
  independent glyph, fragment and theme checks. Headed localized Chromium and
  Firefox trials connect the actual editor save to native forms and the same
  ordinary streams. Hydration and actual teletext-client proof remain open.
- Preserve the assigned Romanian national Base `0x5D`/`0x7D` originals through
  enlarged-page repaint with exact resource-backed glyphs, including underline.
  Keep the original character identities and fallback bytes; no guessed breve
  or caron conversion is introduced. Four actual QTeletextMaker saves now reach
  concept preview, explicit publication and ordinary streams with independent
  bitmap/fallback checks; suitable-client rendering and navigation remain open.
- Realize the Turkish national Base `0x23` TL symbol through enlarged-page repaint
  with exact plain or underlined DRCS, retaining its original identity and
  fallback under both primary designations. Mixed Polish/Turkish pages keep
  stable compiler slots, and native editorial validation retains zone and
  resource ownership. Both resource scopes can carry all four variants together
  while retaining caller page, output and work limits. An eight-page actual editor
  trial now preserves both scopes through receipt-bound concept validation, shared
  preview, separate publication and restart recovery. Both ordinary streams now
  pass transport, source audio/video, full-model and independent bitmap/fallback
  checks; suitable-client display and navigation remain open. The earlier single-cell Turkish editor flow
  reaches receipt-bound concepts, shared preview, explicit publication and two
  ordinary streams with transport and independent bitmap checks. Localized
  headed Chromium and Firefox now prove receipt-bound import, exact preview,
  separate publication and the same ordinary streams. Hydration and suitable-
  client acceptance for this variant remain open.
- Preserve the Polish national Base `0x5B` identity through enlarged-page repaint
  using its exact plain or underlined glyph. Reuse matching authored definitions
  or provision bounded compiler-owned DRCS resources through unused links, keeping
  original declarations, fallback, existing resources and lifecycle intact.
  Authenticated MOT-inherited roles now retain those originals through prepared
  concepts, shared preview, explicit publication and indexed reexport; modified
  MOT bytes or generated glyphs are rejected without changing either channel.
  Three actual QTeletextMaker saves now reach both ordinary streams with
  transport and independent glyph/fallback and encoded-MOT checks; automatic
  resource acquisition and suitable-client navigation remain open.
  MOT-selected default glyphs also survive actual QTeletextMaker edits, concepts,
  shared preview, explicit publication, ordinary streams and reopen. Original
  POP/GPOP/MOT packets and fallback remain protected; scoped independent object
  and glyph checks pass. Automatic acquisition and suitable-client proof remain
  open for this default variant.
  Enlarged English double-vertical glyphs now retain valid Latin/Greek pages
  through exact generated resources when their direct carrier would exceed the
  two-charset-pair limit. Plain and underlined forms preserve fallback; existing
  direct routes remain unchanged when a pair is free or Hebrew is already used.
  The same exact realization now supports raw English primary designation `0x10`,
  preserving its transmitted designation, fallback and original page identity.
  Fresh editor saves retain this raw designation through concept preview, explicit
  publication and isolated ordinary streams with independent glyph/fallback checks.
  Genuine editor saves now reach isolated channel publication and ordinary
  streams with independent mask/Greek/fallback checks; generated glyph slots
  remain protected on reimport. Suitable-client proof remains open.
  Mixed plain/underlined glyphs retain stable generated slots and their painted
  attributes while keeping undefined DRCS underline bits disabled. Known Base
  spacing latches also survive the genuine editor/import/publication/stream flow,
  preserving assigned G0 paint, literal fallback and protected mosaics.
  Receipt-bound concepts can create or retire the generated carrier without
  granting permission to edit its resource or X27 links. Fresh recorded
  QTeletextMaker saves cover plain and underlined variants through native concept
  import, shared preview, explicit publication and reopen without predefined banks.
  Two ordinary channel streams preserve audio/video and separate themes, with
  transport and independent glyph/fallback checks. Headed Chromium and Firefox
  connect plain and underlined raw-latch variants to localized native forms,
  exact previews and the same streams. Hydration and full editor/client
  acceptance remain open.

- Permit acknowledged teletext updates from magazine defaults to complete page
  declarations while retaining bounded magazine cache history. Later incomplete
  pages and Off/resume cannot silently borrow stale palette or charset defaults.
- Keep teletext resource refresh counters aligned with each stream's completed
  output using explicitly validated header choices. Preserve immutable designs,
  page bodies and compatibility with older accepted publications. Representative
  ordinary-stream updates preserve source audio/video. Authorized content-only
  replacements now retain the active decoder during bounded recovery; Off,
  expired or changed selections and failed validation still withdraw teletext.
  An actual singleton bitmap update retains two client decoders and page
  navigation with different channel themes. Broader warm-client and authored
  Level 2.5 acceptance remain open.

- Preserve imported teletext resource kind, raw hexadecimal page and logical
  subpage in validation diagnostics, using the existing localized admin display.
  Actual singleton QTeletextMaker bitmap edits now have ordinary-stream,
  independent-pixel and two-theme live-client navigation proof. Broader resource
  updates and complete Level 2.5/editor acceptance remain open.

- Keep the basic playback smoke fixture on explicit software encoding with a
  matching preset instead of inheriting a startup-selected GPU preset.
- Distinguish teletext source-read, window-preparation and packet-delivery
  failures in bounded DEBUG diagnostics, retaining the actual forwarding error
  when a stream is stopped. Delivery, buffering and cleanup rules are unchanged.
- Restore selected accepted teletext on generated native legacy and hybrid MPEG-TS
  wrappers without rerunning addons. Missing or invalid policies and unavailable
  profiles preserve ordinary playback, selected audio language and explicit
  subtitle Off. Keep wrapper startup within its existing
  deadline and clean up waiting responses even when SQLite configuration is busy.
  Full client, caption, platform and engine acceptance remains open.
- Keep browser fixtures isolated from retained teletext revision/publication
  identities, and verify valid midnight overlaps in real Block preview tests.

- Allow callers to omit Next's HLS-only subtitle rendition with `subtitles=false`
  and advertise it only for Convert, without marking it as the default track.
  MPEG-TS continues to use the authenticated audio/video media playlist directly.

- Limit simultaneous compilation on shared self-hosted CI runners to avoid
  exhausting memory during workspace and Next acceptance. Preserve all test
  selections and capture existing Next lifecycle errors in the isolated preview
  fixture instead of reporting only its failed readiness state.
- Clean up owned browser-test playouts after each case so synthetic Scripted
  failures do not contaminate later Health acceptance checks.
- Prevent Next Copy playback verification from failing on startup database
  writers; seed its isolated engine/profile settings with a bounded wait and
  one transaction.
- Recognize native Windows YAML schedule-file paths and resolve their imports
  instead of decoding the path itself as an inline schedule.

- Resolve smart collection list data before rendering its SSR response so saved
  search links work with JavaScript disabled.

- Specify the bounded ffprobe diagnostic byte-buffer type explicitly so native
  Windows builds remain valid with their Unicode iterator dependencies.

- Honor repeat counts and inherited custom titles in nested Sequential YAML
  pre-roll, mid-roll and post-roll sequences while retaining shared selection
  state and existing continuity checks.

- Preserve relative paths in troubleshooting ZIPs so Native and Next reports
  with the same filename remain separate entries instead of losing a report.

- Detect APNG graphics by their animation-control chunks, including animations
  stored with a `.png` extension, and select the animation loop option. This
  prevents FFmpeg startup failures. Reuse bounded image timing and file reopening
  for finite and infinite APNG canvas animation, preserving still PNG and JPEG XL
  behavior without rebuilding the FFmpeg runtime.
- Preserve ordinary MPEG-TS playback when a new teletext generation arrives
  after the finite source has ended. Reject late activation before it can install
  an incomplete cycle, retain withdrawal and full-cycle checks, and load accepted
  base output in the next response. Keep typed terminal failures visible.
- Recheck accepted teletext selection after source discovery before its first
  authored write. Late disable or generation replacement preserves exact ordinary
  playback in both direct and retained output.
- Preserve native fallback header controls while adapting new teletext exports to
  QTeletextMaker's effective primary charset. Explicitly version the editor
  carrier and retain legacy receipts and qualified concepts without rewriting
  their bytes. Reject protected status/enhancement changes as before.

- Keep video-copy streams free of encoder-only profile, preset, tune and hardware
  options left in a saved transcoding profile. A retained H.264 `high` profile
  could otherwise make FFmpeg reject ordinary playback before producing AV.

- Retain teletext work capacity until the actual addon-resolution and native
  workers finish, including after requester cancellation or an early cleanup
  error. Nested publication/recovery shares one slot and the original deadline;
  saturated or invalid resource policy leaves accepted output and playback intact.
  Unconfirmed process termination retains its teletext slot until restart; Health
  reports invalid concurrency settings in every admin language.
- Enforce teletext producer limits before addon invocation and original-document
  validation, including saved-design reconstruction. Over-limit work leaves the
  stored revision, accepted generation and ordinary playback unchanged.
- Honor the original teletext compilation deadline between resource writers,
  display-normalization stages and packet assembly. Expiry returns no partial
  candidate and cannot replace an accepted generation. Checkpoints do not
  preempt a running bounded stage or establish aggregate CPU/memory limits.
- Bound concurrent large teletext test fixtures and release complete services
  between lifecycle cases, preventing test runs from exhausting runner memory.

- Align teletext overlap and scan-continuity test fixtures with panel-colour
  validation and managed probe cleanup ownership. Allow explicitly selected
  Rust-only CI runs on repaired revisions without treating them as full CI proof.
- Reject full-screen and full-row colour selectors in authored teletext objects
  addressing either side panel, with original command and invocation diagnostics.
  Preserve legal main-page colours and independent panel glyph/cell attributes;
  complete Level 2.5 client acceptance remains pending.
- Fix enhanced teletext G1 mosaic separation and explicit D5 cancellation in the
  shared display model and raster. Keep original writer order and object scope;
  retain independent decoder limitations and full engine acceptance as open.
- Retain managed scan ownership through local FFprobe cancellation and cleanup
  in the web worker, startup replay and standalone scanner. Gate terminal scan
  updates on confirmed cleanup; stop the worker and reject new scans when
  cleanup remains uncertain. Report stopped workers in cached Health results
  and badges, with guidance in every supported language.
- Bound local FFprobe stdout content to 8 MiB while concurrently retaining a
  16 KiB raw diagnostic tail from stderr. Reject overflow and keep process
  cleanup owned after cancellation, preserving existing probe arguments and
  ordinary track metadata.
- Preserve every incoming stream and its language metadata through the legacy
  MPEG-TS channel wrapper, including alternate audio and DVB teletext subtitles.
  Keep source stream metadata separate from channel service metadata.
- Admit FFmpeg's startup discontinuity markers only for a fresh teletext output
  epoch. Keep later timeline resets terminal without weakening PSI, clock or
  capacity validation.
- Read bounded final-output lookahead while teletext packets wait for delivery,
  preventing pipe backpressure from starving FFmpeg's realtime concat wrapper.
  Preserve source ages, shared capacity, sink deadlines and acknowledged EOF.
- Preserve paired Remote Stream producer cleanup across cancelled waits and
  report unconfirmed reaping. Retain direct-stream capacity, prepared external
  subtitles and manual artifacts when consuming-process termination remains
  uncertain, including hardware respawn failures.

- Keep Health links readable in both themes by using the table's text colour
  and an underline instead of an accent colour with insufficient dark contrast.
- Keep XMLTV guides readable when metadata contains XML-forbidden characters.
  Preserve rich guide fields, strictly validate custom template fragments before
  appending them, sanitize per-item fallbacks, and validate the complete document
  before returning a successful XML response.
- Keep the on-demand checkpoint regression test's virtual clock stationary
  during SQLite I/O, avoiding intermittent CI write timeouts while preserving
  its checkpoint timing and viewer-position assertions.
- Treat manual exclusion from add-on-list publication as acknowledgement of an
  unavailable Remote Stream in Health. Keep warnings for automatically filtered
  items, streams still included in another list, and missing local files;
  including an item again restores its unavailable warning.
- Render the admin logo with a transparent vector TV/R-cog mark and theme-aware
  lettering, keeping it sharp and readable in both light and dark mode.
- Resolve Classic, Block and Scripted editor data before mounting their
  hydration islands, preventing missing-marker crashes and restoring their
  interactive editing controls.
- Allow Classic Refresh to discard expired legacy rows before checking a
  restoration boundary, so old overlaps do not block checkpoint recovery. Keep
  current programmes and delayed-channel retention protected.
- Classify unavailable add-on episodes through their owned Remote Stream even
  when the episode has no media version, avoiding false missing-disk warnings
  and counting each unavailable stream once.
- Keep scans running while their lease heartbeat waits for SQLite, preventing
  the worker from blocking its own transaction and losing progress. Retain
  cancellation and transaction rollback on lease loss or shutdown.
- Count all required operations in add-on list preparation progress when one
  video needs both interval metadata and subtitles, so completed task counts
  cannot exceed the displayed total.
- Reuse installed Python 3.12 or newer on Windows release runners, avoiding
  installer failures under restricted PowerShell execution policies. Recover a
  failed Windows draft archive from its accepted immutable source and web bundle
  without rebuilding successful Linux archives or changing the release tag.
- Verify Next staging bytes and provenance on Windows without assuming POSIX
  execute bits; retain those permission checks on POSIX hosts. Windows recovery
  runs the corrected tests against the unchanged accepted staging implementation.

## [1.1.0] - 2026-10-09

This stable release includes Windows x64, Linux x64, ARM64 and ARMv7 archives.
Docker is excluded with maintainer approval after its build timed out. Linux
application archives require glibc 2.39 or newer. Approved hardware/platform
acceptance debt remains explicitly unverified.

### Security

- Update the transitive CI tooling dependency `fast-uri` to 3.1.8 to fix URI
  authority injection, host confusion and host normalization vulnerabilities.

- Add expiring, source-bound media-part tickets on the private callback listener
  for external JSON playback integration, without publishing Plex credentials.
  Resolve current credentials at each open and preserve byte-range requests.

- Disable HDHomeRun HTTP endpoints and UDP discovery while IPTV JWT protection
  is enabled. Explain the inactive settings in the selected UI language and
  reject changes to them until JWT protection is disabled.

- Reject invalid artwork-cache, watermark and channel-logo filenames before reading files,
  while preserving registered local artwork and existing uploaded logos.
  Provider artwork URLs now resolve persisted artwork IDs; callers cannot select
  provider paths or override stored resize settings. Follow bounded image
  redirects without forwarding provider credentials across origins.
  Keep browser artwork within the management authentication boundary and IPTV
  artwork under the separate IPTV token policy (ADR 0103).

- Compare internal Next media capabilities and credits tickets in constant time,
  preserving rejection of altered bindings and credentials from an earlier host
  process.
  Keep URL query credentials out of troubleshooting sample filenames.

- Require `X-Ers-Api-Key` for machine API requests, with a persistent random
  credential in the application state directory's `api-secrets.json` file.
  **Breaking change:** browser action and server-function paths move to
  `/admin-api`; machine clients must send the key. Browser troubleshooting and
  IPTV retain their separate policies. No machine key is sent to the browser
  (ErsatzTV #2991; ADR 0103).

- Isolate internal FFmpeg and Next callbacks on a loopback listener and build
  their URLs from the bound address, preventing client-supplied Host or proxy
  headers from selecting callback origins. Preserve Next capability checks.
  Add an optional streaming-only listener that does not expose management
  pages, APIs, server functions or assets (ErsatzTV #2989; ADR 0102).

- Update the bundled SQLite used by the server and tooling to 3.51.3, above
  the 3.50.2 fix for CVE-2025-6965, without requiring a system SQLite install.
  Verify the linked runtime also retains the WAL-reset corruption fix.

### Fixed

- Use the checksum-verified managed FFmpeg for browser fixture commands and
  PATH lookup, avoiding runner-local binaries with missing shared libraries.
- Measure live playback throughput with a monotonic clock so host clock
  synchronization cannot create false delivery stalls in browser acceptance.
- Wait for the add-on import landing to finish its automatic navigation before
  testing the prepared workspace, preserving the overview and task-count checks.

- Preserve consumed Classic programme and filler history when repairing future
  overlaps or retaining prefetched programmes during Refresh/Reset. Keep history
  cleanup anchored to the originally requested start time.

- Preview the complete 30-second Next channel troubleshooting window, including
  scheduled handoffs and configured gap filler. Keep historical callbacks valid,
  preserve per-programme audio/subtitle choices and apply the selected preview
  profile without changing saved channel or playout state (upstream #3065).

- Keep profile applicability help readable on narrow screens without JavaScript.
  Preserve a failed preview's startup diagnostic instead of attaching logs from
  an earlier producer that did not belong to that preview.

- Keep cached IPTV URLs working after a channel switches to Next: legacy
  MPEG-TS modes use its hybrid wrapper and old HLS modes use its segmenter,
  preserving authentication and native-engine isolation (upstream #3062).

- Keep failed local deep scans retryable when source timestamps are unchanged.
  Render the Libraries table without JavaScript and preserve the selected
  language for hydrated scan controls and confirmation dialogs. Revalidate
  mutable local artwork so refreshed images appear even when file timestamps
  stay unchanged, and bypass images retained under the former cache policy.

- Generate profile-compatible silent audio for native playback of media with
  confirmed missing audio streams. Preserve disabled and selected audio, source
  seeking, programme handoff and unprobed input behavior.

- Use selected video or complete audio-stream duration when scheduling local
  media, so subtitles extending beyond the content do not delay the next
  programme. Guard FFmpeg 9's conflicting Matroska duration estimates
  (ErsatzTV #3058).

- Start troubleshooting playback at the requested seek point using the
  generated playlist and its first segment. Preserve normal live-preview
  starting behavior and successful short previews without a Next ready marker
  (ErsatzTV #3057).

- Preserve Windows UNC paths in the generated language registry, and return
  opaque 404s for private callback paths on the management listener while
  retaining the existing FFmpeg profile pages.

- Include song subtitles in playback selection and validate generated-picture
  and song-sidecar pins accurately in health checks. Continue rejecting stale
  media versions and unavailable tracks.

- Prevent Block playout skips and repeats after collection changes by removing
  orphaned future cursor history before restoration and deleting all duplicate
  history at replaced programme starts. Preserve consumed history, current
  programmes and restart-safe rotation during preserving Refresh.

- Use hardware VAAPI padding only with the compatible Intel iHD driver,
  preserving effective color tags and range. Keep NV12/P010 depth during
  software fallback and subsequent subtitle/graphics composition; hardware
  execution remains explicitly unverified (ADR 0117).

- Use the selected content stream's duration when scanning media, excluding
  attached artwork and preserving audio/subtitle metadata. For Matroska/WebM,
  use a valid shorter stream duration tag when FFmpeg has copied a longer
  container estimate from subtitles; preserve numeric duration precedence
  elsewhere (ADR 0117).

- Hold static WebP and repeat complete GIF/WebP animations for the scheduled
  duration in Next. Preserve fractional seeking, local/HTTP image handoff and
  live-input EOF, with decoded Linux/Windows and production HTTP regression proof.

- Use image metadata and remote-stream fallback durations consistently for
  scheduling, troubleshooting and Next input hints; identify BMP, TIFF, WebP,
  GIF and AVI inputs explicitly. Decoded playback and production HTTP lifecycle
  checks pass with the published runtime.

- Refresh Next projections after profile copy/resolution changes, omit graphics
  from copied video and stop advertising a fixed copied-video HLS resolution.
  Runtime checks verify unchanged compressed video/audio with configured overlays.

- Pin a temporary, revision-verified Next source correction for live RTSP
  startup buffering (ADR 0116). It preserves padded-audio termination and
  requires removal when an equivalent upstream correction passes acceptance;
  all six platform packages retain verified source, patch and payload hashes.

- Support Next copy-mode configuration with independent audio/video selection,
  transcode fallback settings and audio-copy loudness suppression. Runtime
  integration passes Linux/Windows playback and transport acceptance.
  **Breaking for existing Next configuration overlays:** each overlay requires
  a supported channel schema version; legacy `format: null` copy requests must
  be reviewed and expressed with `mode: copy`. Health reports incompatible
  versions before playback startup.

- Reopen upstream catch-up for 23 newer ErsatzTV commits and correct pending
  ledger ordering while preserving the previously accepted baseline.

- Restore complete Chromium and Firefox CI coverage by isolating saved add-on
  selection policies, following the current playout update dialog, checking
  native graphics placement controls, and releasing test-owned preview sessions.
  Snapshot each matched server/site bundle so concurrent local builds cannot
  remove hydration assets from a running suite.

- Apply exact Image/Motion pixel placement in native rendering and previews,
  overriding anchors, percentage margins and source-content padding. Preserve
  authored Motion alpha/timing through YAML and show long titles with independent
  heading/title fonts and truthful fixed-size text behavior.

- Play compact animated JPEG XL graphics with decoded duration, bounded native
  repeat and programme-relative phase seeking. Preserve source alpha, evaluate
  constant and Boolean opacity without per-pixel processing, and apply Motion
  timing to regular playback as well as previews. Support JPEG XL uploads and
  native Image/Motion previews with localized animation guidance.

- Author every native graphics kind with placement, timing, layering and guide
  controls; validate YAML and missing fonts, media and variables before saving.
  Preview unsaved definitions through the real native canvas and explain each
  text renderer's font and width behavior. Resolve subtitle programme variables
  through the shared parser, bound native Lua drawing and include PNG encoding
  in runtime health checks.

- Select add-on list items with saved rolling age, newest-count, duration and
  title filters, manual pins and exclusions, and an accepted/rejected preview.
  Keep archived media and the last complete publication while the existing
  durable workers apply the selection; pause polling without losing its cadence.

- Resolve native graphics programme labels through the same scheduler-specific
  guide grouping as XMLTV, including split films, filler, historical timestamps
  and local calendar days across DST. Provide bounded defaults and a channel-aware
  text preview; keep raw Epg templates compatible. Carry explicit item variables
  through Classic and Block schedules, copies and continuation, and offer bounded
  opt-in Drawtext metadata refresh separate from clock updates and file reload.

- Escape generated drawvg file paths through both FFmpeg parsers, including
  Windows drive letters, separators, spaces and quotes. Channel previews now
  identify error-slate segments and show the original redacted transcode failure,
  even while the browser successfully decodes the replacement video.

- Let Classic schedule items save compatible filler, graphics, guide and track
  changes for the next automatic extension without replacing published programmes
  or resetting rotation. Show the retained horizon and persisted application/build
  status; reject incompatible source/order edits and concurrent builds safely.

- Reject overlapping, backward or non-positive Classic programme timelines
  before publication, including unsafe Continue boundaries and anchors. Report
  existing overlaps in Health and show localized rejection guidance. Allow a
  verified programme-preserving Refresh to repair an invalid future while
  retaining current programmes, guide settings and consumed history; refuse
  ambiguous retained programmes without changing published state.

- Offer explicit Extend, Refresh and Reset playout actions with programme
  preservation enabled by default. Validate the stored scheduler kind, retain
  complete programmes beyond producer buffering and leading mirrors, and restore
  Classic, Scripted, YAML and Block source rotation after refresh or restart.
  Reset can start a fresh future rotation without changing retained programmes;
  incomplete or incompatible continuation state fails safely with localized
  Health guidance. Correct Block source repetition and Classic midnight rewinds.
- Automatically adapt Scripted playouts after successful scans change their
  actual library/tag selections, including previously empty selections. Ignore
  unchanged or unrelated sources; coalesce work, reject stale candidates and
  retain bounded retries across failures, cancellation and restart. Show source
  revision, pending work, failures and the last successful build in the admin UI.
- Preserve accepted Next playback during rebuild preparation and recoverable
  publication failures. Stage complete audio/subtitle/graphics projections for
  active channels, mirrors and previews before committing a preserving build;
  restore previous files when finalization fails or is cancelled. Enforce the
  safe publication deadline, retry transient file contention and report failed
  recovery without acknowledging success. Persistent storage failure can still
  require recovery; database/file publication is not a single atomic operation.
  Fix leading-mirror schedule reads and cancellation before HLS playback starts.
- Add explicit External JSON adoption controls and retain accepted planning
  across restart or source-file loss. Playback and guide use the same revision;
  Extend only appends, preserving actions retain prefetched programmes and
  reject invalid, stale or expired candidates without changing authored clocks.
  Show adoption status and localized recovery guidance. Existing direct-file
  readers remain compatible until explicit adoption.

- Continue Scripted playouts from the persisted future timeline and restore
  series history at that cursor. Expose Lua build context, reject backward or
  overlapping candidates before atomic publication, and show failures in
  Activity, Health and the retained timeline preview.

- Discover MPEG, transport-stream and other supported video-container candidates
  in local television libraries. Preview folder counts, ignored extensions,
  unsupported nesting and empty shows before saving, with bounded read-only
  inspection and localized diagnostics; retain episode/extra separation.

- Own diagnostic previews with separate IDs and visible lifecycle states. Stop
  cancels and reaps the process tree and graphics controllers before releasing
  capacity; stale preview links cannot play a replacement. Preserve the finite
  deadline and live-channel isolation, and support programme-relative seeking
  with explicit UTC timestamps and server-rendered Play/Stop forms.

- Support diagnostic playback for all channel delivery modes, including HLS
  Segmenter, through the shared persisted-mode decoder. Show localized,
  actionable startup failures without exposing private runtime details.

- Identify ErsatzRS correctly in unavailable-media health warnings and localize
  known disk/provider diagnostics, including cached messages using the old name.
- Reconcile Windows local-TV folder aliases after case-only directory changes,
  clearing false `Local television identity` warnings while preserving media IDs,
  stream selections and scheduling references. Reuse the existing episode
  when a changed path spelling still identifies the same file.
- Include the add-on contract and platform crates in Docker dependency-cache
  stages so release images can build the complete workspace. Add Docker-only
  recovery that preserves the accepted application tag and web bundle while
  recording the corrected build recipe.
- Allow Docker recovery to inspect unpublished drafts and validate release
  evidence with Python on runners without `jq`.

- Select managed FFmpeg packages by the complete platform suffix so 32-bit ARM
  installations cannot download the ARM64 executable from a mixed release.

- Restore shared browser-test language and theme after localization checks, and
  keep manually seeded playback fixtures independent of the external JSON engine.
  Wait for hydrated pages before subsequent test navigation, preserving Firefox
  page-error checks.

- Run Next CI against the validated FFmpeg v2.0.6 release and supply the
  packaged executable to the graphics-canvas runtime test. Keep fallback and
  on-demand subtitle fixtures eligible under the extracted-text selection
  contract, without bypassing the playback assertions. Keep routed fixtures
  alive through the host's startup deadline while proving authenticated
  decoding and explicit producer cleanup. Stage the pinned Next and FFmpeg
  packages for browser CI so its real Next cases run against verified runtimes.

- Preserve AV1 profiles and use fMP4 for native AV1 HLS output. Reject AV1
  MPEG-TS output before startup and report incompatible channel settings in
  health checks; Next continues to reject AV1 (ADR 0106). Software SVT-AV1
  uses compatible target-bitrate rate control instead of unsupported CBR flags.

- Stop FFmpeg hardware-capability probe processes when the check times out or
  is cancelled, preventing abandoned probes from accumulating.

- Restore Windows compilation of Next subtitle runtime tests by sharing the
  portable caption helpers outside the Unix-only producer fixtures.

- Show this build's unreleased changelog on development home and health pages;
  tagged releases continue to show their exact version's notes.

- Include Next's Windows oneVPL DLL and license beside its executable in both
  release packaging paths, rejecting archives with missing companions.

- Check out Windows batch scripts with CRLF line endings, including the bundled
  MPEG-TS script, while retaining LF for shell scripts.

- Respect Use Embedded Subtitles for Next automatic selection. Automatic and
  custom segmenter choices skip embedded text without a usable extracted path
  before applying preferences or selector rules. Preserve bitmap eligibility
  and exact pins.

- Publish plain Next watermarks as direct media layers, avoiding a graphics
  canvas when no graphics elements or opacity expressions require it. The pinned
  Next runtime now also applies configured opacity correctly to RGB images.

- Keep Jellyfin collection members as their real media types during scans by
  disabling collection collapsing and rejecting unexpected response types. For
  affected show libraries, scan again, empty the trash to remove bogus collection
  shows, then deep scan collections to restore collection search results.

- Let libva select the Next VAAPI driver when the profile uses Default, instead
  of forcing Intel iHD on AMD devices. Explicit driver and device choices remain.

- Preserve Plex, Jellyfin and Emby video rotation metadata, expose it in Media Info,
  and publish Next playouts using the rotation-aware 0.0.5 schema. Correct the
  orientation health check so copied rotated video is reported.

- Require absolute filesystem paths for local libraries, keep percent characters
  literal, and reject overlaps with other libraries while retaining the Images /
  Other Videos exception. Show localized offending-path errors with correction
  inputs retained. Saving unchanged paths preserves their scanned media.

- Clear dependent playlist and template selections when changing groups in
  Deco and Playout Templates editors, so hidden previous choices cannot be saved.
  Show persisted template selections correctly when opening the editor.
  Resolve the Playout Templates form before hydration so its controls attach reliably.

- Validate schedule files before saving or queuing a playout build, rejecting
  empty paths and directories. Use the same quoted script path during validation,
  policy loading and execution, and pass supplied arguments to Lua's `arg` table.

- Discover current MPEG-TS scripts in FFmpeg settings and recover missing selections
  to Default or the first available script. Settings and stream startup share fresh
  definitions, so edits, removals and corrected invalid files take effect on reload.
  Preserve operator-defined script names and render the FFmpeg settings form
  completely when JavaScript is disabled.

- Keep movie collection poster and keyboard ordering consistent, prevent
  overlapping order saves and restore rejected orders. Reject stale or foreign
  collection rows atomically and repair legacy non-movie custom-order settings
  while retaining memberships and audio/subtitle overrides.

- Continue independent scheduler maintenance after a job fails, while reporting
  the failure and retaining its cadence for retry. Stop before starting another
  maintenance job when shutdown is requested.

- Return typed errors from authenticated FFmpeg-profile, smart-collection and
  empty-trash machine APIs: invalid requests use 400, missing records 404,
  profile and smart-collection conflicts 409 and operational failures 500.
  Prevent profile deletion from removing dependent channels or playouts, and show localized feedback
  when the default or an in-use profile cannot be deleted. Keep loaded profile
  row actions and format labels in the selected UI language.

- End troubleshooting readiness promptly when native playback exits, retaining
  completed diagnostics and process-owned cleanup. Bound media-sample capture
  and remove temporary samples after failures or disconnected requests. Remove
  URI credentials from exported environment diagnostics.

- Recover the last motion-graphics frame when seeking near EOF and stop Hold
  at its exact expiry, including zero hold time. Anchor media troubleshooting
  graphics at the selected seek in both engines and expose the numeric
  `MediaItem_SeekSeconds` template variable while preserving subtitle and audio
  timing.

- Restore Plex Other Video folder tags across Windows, UNC and Unix server
  paths. Refresh existing items on their next scan while preserving subtitles,
  labels and independently owned collection tags.

- Omit unavailable automatic Next sidecars while preserving errors for exact
  selections. Validate prepared-media sidecars against the materialized file,
  retain valid remote sources, normalize HTTP(S) schemes for FFmpeg and preserve
  subtitle seek offsets.

- Exclude unsupported media-server closed captions before subtitle selection in
  native playback, Next and troubleshooting, including Plex Other Videos. Keep
  local captions and stored metadata intact, and reject unsupported pinned tracks.

- Fetch Plex sidecar subtitles, including Other Videos, through their current
  owning server with header authentication. Prepare bounded local artifacts
  instead of forwarding persisted token-bearing URLs to playback.

- Preserve YAML post-roll, mid-roll and graphics settings across Continue builds
  and restarts, including graphics variables and explicitly cleared settings.
  Resolve graphics paths in SQL-backed YAML builds, including imported sequences
  and named schedules.

- Advance timed graphics and watermark selection with each Next channel-preview
  chunk, including mirrored historical previews, while keeping the selected
  programme and media version unchanged.

- Keep motion graphics visible on the first Next canvas frame when a chunk
  starts between source frames. Normalize the sought motion timestamps before
  compositing, preserving delayed starts and loop/hold/disappear timing.

- Keep graphics template programme stop times tied to the scheduled finish,
  independently of seek offsets and rendering chunk durations, including
  mirrored and preview playback.

- Decode Plex metadata using the documented `Media`, `Part` and `Stream` array
  names while retaining compatibility with lowercase responses.

- Restore random fallback with an explicit warning when a Classic marathon has
  no grouping mode, preserving normal Random continuation and track preferences.
- Read external JSON programme metadata directly for XMLTV, including mirror
  offsets and minimum guide duration, with explicit invalid-start-time errors.

- Refresh unchanged local PQ/HLG media once so stored HDR/Dolby Vision metadata
  is populated, and expose Dolby Vision profile and HDR10 metadata presence in
  Media Info JSON while preserving non-video field omission.

- Expose native and Next playlist-worker reports through authenticated
  `GET /api/sessions`, including last access and a nullable transcode cursor.
  Next reports `state: "next"`; reading session reports does not keep streams
  alive. Document the response and API-key requirement in OpenAPI.

- Hourly and manual artwork cleanup remove unreferenced cached images and restore
  missing cache entries from local image sources, updating all shared references.
  Preserve original artwork, referenced blur images and uploaded watermarks/logos;
  failed repairs remain eligible for the next pass.

- Share health checks across page visits and scheduled maintenance, cache complete
  results for five minutes, and add a localized Refresh action with a no-JavaScript
  fallback. Leaving the page no longer cancels checks for other readers. Bound
  check execution and persist results and warning/error counts together so a
  failed run or write retains the previous complete snapshot.

- Provider episode scans repair changed season membership even when metadata is
  unchanged, exclude Emby specials from unrelated seasons, and retain valid
  episodes after filtered pages. Season pages fall back to the show poster.

- Local television scans retain retry eligibility after episode write failures or
  malformed NFO sidecars, instead of caching an incomplete season as unchanged.

- Tie HLS startup readiness to the registered worker, so a removed worker cannot
  report a successful tune-in and replacement sessions must become ready themselves.

- Honor “Probe For Interlaced Frames” during native playback and troubleshooting:
  sample the selected local video stream, preserve multi-day seek positions, and
  cache eligible version ratios without overwriting scan updates. Health reports
  a missing `idet` filter when this option is enabled.

- Keep replacement HLS workers registered when an older native or Next worker
  finishes cleanup; direct viewers retain their independent registrations.
  Enforce the HLS readiness deadline during playlist reads as well as polling.

- Preserve collection, label, network and country tags when an ordinary library
  metadata refresh replaces its own untyped tags.
  Reconcile Plex movie, show, season, episode and other-video labels by provider ID and name,
  preserving other scanners' tags when labels are renamed or removed.
  Plex, Jellyfin and Emby collection membership updates now maintain separately
  owned collection tags in the same transaction.
  Scope provider collection identities to their server, preserving unambiguous
  legacy IDs and leaving ambiguous collections untouched. Same-name collections
  receive distinct display names without changing their provider tags.
  Backfill legacy tag ownership within its media source, preserving existing
  types and ambiguous rows instead of assigning another server's collection.
  Reconcile removed provider collections only after complete root discovery,
  retaining collection references and other roots' memberships. Failed scans
  preserve published state; repeated scans preserve membership IDs and selected
  audio/subtitle overrides. Persist affected Classic playout refresh work with
  collection publication so restart recovery retains both old and new dependency
  matches; unchanged scans create no refresh work.
  Movie NFO countries persist separately and match `country:` searches; ordinary
  tag searches and media detail tags exclude countries and networks while
  retaining provider labels and collection tags.
  Plex TV-library scans now refresh network memberships within the owning
  library; failed or incomplete responses preserve existing network tags.
  Network changes also persist affected episode-playout refreshes, and collection
  scans clean stranded owned tags even outside prior membership. Explicit admin
  deletion also removes owned tags and persists affected refresh work atomically.
  Search results render completely when JavaScript is disabled.

- Render HDHomeRun and UI settings forms completely on the server so they
  remain usable with JavaScript disabled.

- Fill the complete remaining YAML schedule interval with fallback content,
  keeping its playback duration, history and guide grouping aligned with the
  scripted scheduler.

- Include the show poster image alongside the icon and episode still in the
  bundled XMLTV episode template, following the upstream artwork correction.
  Preserve shared show artwork, genres and guide identifiers for every episode
  in a guide batch.

- Preserve shuffled playlist and marathon cycle seeds across rebuilds, including
  independently wrapping children and repeated collection slots. Deduplicate
  overlapping marathon sources and reject invalid Lua marathon grouping.

- Preserve every scripted-playlist child position in build history and restore
  the playlist cycle before advancing, preventing replayed or skipped items on
  continuation. Load persisted scripted history on rebuild and retain structured
  block-deco playlist children when restoring filler positions. Resolve Lua
  sources and named overlays from the database during scheduling, including
  dynamically constructed names; reject failed or cancelled source lookups without
  saving a partial timeline.

- Preserve shuffled YAML sequence order across continuation builds and restarts,
  keep nested instructions in their original slots, and reset only affected
  list positions when saved orders or schedule definitions are invalidated.

- Keep block-scheduler cleanup at or before the build start when the next block
  begins later, preserving current and future playout items between blocks.

- Resolve configured FFmpeg and FFprobe commands to absolute executables before
  publishing runtime paths, preventing Next from rebasing bare commands against
  its configuration directory and falling back to black video and silence.

- Make Plex server discovery transactional and retain the first missing-server
  timestamp until the server returns, preserving synchronized content and path
  overrides when a server disappears from plex.tv (ErsatzTV #2990).
  Bound sign-in attempts to two minutes, validate replacement credentials before
  switching the active account, and prevent stale authorization responses from
  erasing newer credentials. Missing servers skip scans without changing stored
  library sync preferences.
  Add localized reauthentication, missing-server guidance, bounded library
  refresh, and explicit removal with a named confirmation that preserves files
  on Plex. Reject removal if the server has returned since confirmation opened.

- Keep direct MPEG-TS responses separate from HLS worker lookup, preventing
  legacy-to-Hybrid switching from reusing a closing direct stream. Channel Stop
  still reaches every viewer, and one response's cleanup preserves other streams.

- Keep playback troubleshooting controls within narrow screens when option
  labels would otherwise expand the form's grid columns.

- Resume Next on-demand channels from their saved viewer position. Shift the
  timeline before publishing it and checkpoint viewer demand periodically and
  on shutdown, without advancing normal playback from previews or heartbeat time.

- Include the Vulkan loader and supported Mesa drivers in the minimal Docker
  images, and expose NVIDIA graphics capabilities alongside CUDA/video support.
  Recheck cached hardware results when the container's driver capabilities
  change. Hardware smoke checks use the current managed FFmpeg package and
  valid NVENC frame dimensions, and download Vulkan frames in their actual
  pixel format before conversion.
  Remove the obsolete MySQL feature from the alternate Docker build command.
- Probe NVIDIA Vulkan HDR support with the same CUDA transfer used by playback,
  avoiding false capability failures from an unsupported direct mapping.
  Recheck cached results from the old probe when the server starts.
- Preserve optional Next subtitle style overrides in the channel-config model,
  including empty values, without changing the default subtitle appearance.
- Keep custom stream selectors available for Next playback troubleshooting and
  show measured completion speed for both engines. Capture native FFmpeg
  progress without blocking either output pipe, and read Next reports with
  bounded memory and a deadline. Include sanitized Next output with diagnostic
  levels and targets, plus troubleshooting stream-selection decisions, in the
  completed log; mark when older lines exceed the retained-log limit.
- Apply explicit subtitle candidates before custom Next troubleshooting rules
  choose audio, preserving stored subtitle metadata and explicit subtitle-off
  selection. Retain custom selectors submitted to the preview API.
- Initialize the shared HLS player correctly when a preview has no caption
  toggle, including Firefox playback troubleshooting.
- Render playback troubleshooting settings without requiring JavaScript to
  replace the loading placeholder.
- Decode stored media durations correctly when starting playback troubleshooting
  previews, including fractional seconds. Accept empty subtitle selections and
  multiple watermark or graphics selections, and honor Start From Beginning.
- Keep playback troubleshooting output reserved until its process and diagnostic
  writes finish, preventing overlapping previews from clearing active files.
  Bound stalled previews to two minutes and retain the reservation when process
  cleanup cannot be confirmed.
- Keep Remote Stream troubleshooting callbacks bound to their preview selection
  and revoke them before releasing producer resources and transcode capacity.
- Preserve subtitle metadata during explicit Next troubleshooting selection so
  retained-interval caption provenance remains valid.
- Include Next FFmpeg reports and outcome diagnostics from immediate report
  subdirectories in playback troubleshooting downloads.
- Preserve provider external subtitle tracks during scans and hourly cleanup,
  including tracks with shared or missing paths. Affected Jellyfin and Emby
  movies and episodes refresh on their next ordinary library scan.
- Preserve Unicode channel names and FFmpeg paths in MPEG-TS wrapper scripts
  through the upstream-compatible `ETV_*` environment variables; upgrade
  unchanged bundled scripts while preserving custom templates.

- Sample complete observation windows in Next fallback playback tests so short
  programme-boundary segments do not cause false audio/video decoding failures.
- Keep excluded add-on list items individually selectable for bulk inclusion;
  select-visible still skips them and the on-page selection count stays accurate.
- Choose a free loopback port for each Next CI smoke run so an existing local
  server on a shared runner cannot fail the playback gate before it starts.
- Keep the maintenance scheduler running when a new library scan starts just
  after startup replay, so add-on publication cannot trigger a false replay error.
- Exclude add-on list videos when later detail import identifies them as unavailable, while preserving explicit inclusion choices and retained media; show detail progress before preparation and wait for upload dates before grouping seasons by year.
- Remove stale projected media when an item is excluded from a published
  add-on list, so the next publication can complete verification while keeping
  the source stream.
- Save add-on list numbering previews that group seasons by year; the review
  control is no longer mistaken for an item season field.
- Exclude newly discovered unavailable add-on list videos by default while
  retaining them for review and optional local replacement.
- Preserve normal chunked HTTP end-of-stream handling between legacy MPEG-TS
  programmes, preventing FFmpeg 9 from aborting the channel at the first item.
- Make the playback validation gate follow HLS direct's session playlist,
  reject failed or prematurely ended captures, and report channel-worker failures.

- Publish playout gap rows with their completed build and move them with
  on-demand playback, so the detail table keeps gaps aligned with scheduled items
  (upstream PR #2968).

- Resolve Classic fixed starts in local wall time, including tomorrow's DST
  offset. Flexible starts wait at most 12 hours across midnight (upstream PR #2967).

- Keep distinct rerun collections separate in Classic scheduling progress and
  Refresh checkpoints. Ambiguous legacy state is reported by Classic Playout
  State health and requires Reset before rebuilding (upstream PR #2966).

- Preserve external channel-logo URLs as watermark inputs for native and Next
  playback instead of discarding them as missing local files. Channels without
  artwork use a cached, playable image of their generated logo (upstream PR #2961).

- Pass the extracted-font cache to Next subtitle rendering for normal playback
  and browser previews (upstream PR #2960).

- Render General troubleshooting diagnostics without requiring JavaScript.
- Stop Block default filler at items with zero or negative duration, with a
  diagnostic instead of an endless build loop. Exclude expired playout rows
  from filler-gap detection while retaining rows needed by delayed mirrors.
  Reset replaces existing filler and moves retained primary content together
  with its history when block-start filler changes length. Shutdown now cancels
  Block filler generation without publishing a partial timeline (upstream PR #2957).
- Keep playlist cycles open until every playable item has been visited, exclude
  season-zero episodes from Season/Episode playlist children, and reconstruct
  shuffled group order from its saved seed. Classic now loads the saved
  Shuffle Playlist Items setting and structured playlist children. Classic and
  YAML marathon Continue preserve complete child and cycle state. Repeating
  YAML playback no longer exhausts the empty-loop guard while adding content. Repeated playlist sources share playback progress. Legacy Classic Continue
  requires complete state or a safe Refresh checkpoint; Refresh refuses older
  playlist checkpoints without cycle state before clearing items; Reset creates a
  complete checkpoint (upstream PR #2956).
- Restore YAML playlist and Marathon progress after history cleanup and a
  restart. Structured SQL loading now preserves playlist children. Versioned
  snapshots retain cycle state without copying the full playlist on every
  item; incompatible legacy history refuses Refresh and requires Reset.
- Honor Classic Marathon grouping by show, season, artist, album or first
  director, including batch size and group/item shuffle settings. Split builds
  retain the same playback order and guide groups as uninterrupted builds.
- Preserve Classic collection progress for inactive alternate schedules across
  Continue builds; a saved shuffle index remains valid against the full item
  count (upstream PR #2955).
- Keep a Classic Refresh checkpoint for every day in a continued playout and
  preserve the already built prefix when Refresh rewinds to it, including a
  media item crossing midnight (upstream PR #2954).
- Drop NVIDIA and AMD crop-metadata workarounds on FFmpeg 9 so overlay output
  retains its full picture height (upstream PR #2953).
- Preserve Classic playlist Random Start Point offsets across Continue builds
  so resumed playback follows the uninterrupted sequence (upstream PR #2950).
- Keep Classic random starts eligible for previously unused collections while
  preserving the progress of collections whose saved index wrapped to zero
  (upstream PR #2949).
- Apply Classic Random Start Point to playlist children on fresh builds
  (upstream PR #2948).
- Update Plex movie and Other Video titles on ordinary scans when Plex changes
  only the title, including the first scan after upgrading (upstream PR #2947).
- Extract embedded text subtitles for upcoming playout items when a channel
  uses a custom stream selector, even if its general subtitle mode is off.
  Next can select embedded text directly; native transcoding still requires
  extraction outside HLS Direct (upstream PR #2938).
- Remove already-played on-demand playout items at the saved checkpoint and
  retain four hours of continuous scripted playout history for delayed viewing.
- Keep the existing HLS segments and fMP4 init map available when a realtime
  transcode falls behind the playlist trim cutoff, instead of returning an
  empty session playlist before the next append.
- Match NCalc 6.2 when Classic filler expressions compare NaN values or
  preserve negative zero through `IEEERemainder`, keeping selected breaks
  consistent with the updated upstream evaluator.
- Include a matched Intel VAAPI/QSV userspace stack in Linux x64 Docker images,
  with pinned source checksums and licenses, so passing an Intel render device
  no longer leaves the container without its media driver.
- Reconcile local sidecar subtitles by filename without colliding with embedded
  stream zero; rescans remove duplicate identities and update moved sidecar paths
  while preserving unchanged embedded extraction caches. Existing collisions trigger
  a local folder rescan after upgrade; hourly cleanup removes ownerless subtitles
  and empty-path sidecars, with pending cleanup reported in health checks.
- Match configured MPEG-TS script IDs without regard to case, including Unicode
  letters, and use the lowercase built-in default in troubleshooting (upstream PR #2916).

- Preserve clean EOF for Next's one-shot producer HTTP streams by retaining
  FFmpeg's chunked-reader state, while keeping reconnection disabled. The
  separate Next CI check now covers full-frame decoding through the production
  HTTP response.

- Initialize rawvideo pixel-format metadata while probing a retained producer's
  bounded NUT prefix, so Next can consume that same producer without reopening it
  or rejecting otherwise valid media as incomplete.

- Bind Next playback callbacks to their originating normal or preview session,
  preserving each session's tools and path replacements and rejecting callbacks
  from stopped or replaced instances. Retained-source preflight also observes
  cancellation and deadlines while waiting for database or sidecar preparation.
- Make the synthetic add-on option test provider consume runtime requests before
  replying on Unix and Windows, preventing a process-input race in browser tests.

- Clean orphaned actor and artwork database records at startup and hourly, with
  a bounded manual cleanup API, while retaining referenced images and artwork
  files (upstream PR #2885).

- Preserve merged deco watermarks when combining them with item, channel or
  global watermarks, including a merged watermark with no fallback selection.

- Repair empty or whitespace-only embedded-subtitle cache references while
  reusing completed cached tracks (upstream PR #2873). Partial extraction retains
  each subtitle's absolute stream index, preserving the selected language.

- Select Next embedded subtitles from the resolved media version before applying
  language, forced/default or custom-selector preferences, so stale metadata
  cannot hide a valid image track (upstream PR #2868).

- Fill Classic fallback gaps and post-roll padding with complete clips, trimming only the
  final clip instead of stretching one item. Preserve subsecond source positions
  when saving clips, so a short final fallback cannot collapse to zero length.
  Honor chapter-as-media filler
  offsets and titles (upstream PR #2867). Place mid-roll filler between source
  chapters, keep post-roll after the final chapter, and align padding to clock
  boundaries. Preserve chapter identity and track preferences across filler
  membership changes and database restarts; honor exact media-version pins.
  Keep pre/post-roll padding expressions effective when mid-roll selects no
  chapter points. Refuse destructive Refresh when older filler checkpoints
  cannot establish identity. Evaluate chapter expressions consistently across host
  languages with ICU Unicode comparison, invariant numbers and explicit date/time
  formats. Reject invalid literals and expressions above 64 KiB or 32 nesting
  levels without replacing the existing playout (ADRs 0097 and 0098).

- Preserve negative mirror playout offsets through channel repository upsert,
  keeping XMLTV actual and guide-window times consistent (upstream PR #2863).

- Render channel add/edit forms in order so they remain usable with JavaScript
  disabled, including engine selection and server-side mode normalization.

- Correct the HDHomeRun discovery manufacturer link to the ErsatzRS repository,
  following the repository-link correction in ErsatzTV PR #2848.

### Added

- Normalize Next output frame rates to the lowest scheduled rate above 23 fps,
  preserving rational rates and selected audio/subtitles. Copy compatible video
  at the target rate and transcode mismatched sources (upstream #3063).

- Deep-scan local libraries to refresh unchanged television episodes and
  cached track metadata. Confirm library and external-collection deep scans
  before queuing them, with localized warnings about longer scan times.

- Play songs through the optional Next engine with occurrence-stable artwork,
  metadata and channel-sized pictures served by authenticated private callbacks.
  Preserve alternate audio, subtitle selection, seeking and scheduled handoff;
  use finite black video for other audio-only programmes.

- Support MPEG-2 video output with the Next engine, using 8-bit output and an
  explicit positive video bitrate. Preserve selected audio and subtitles;
  omit the HLS `CODECS` attribute when either codec identifier is unknown.
- Playback troubleshooting adds a music-video credits template selector for both
  engines and includes the template and generated ASS in diagnostic downloads.

- Show Next emergency playback and schedule-gap reasons on fallback video by
  default, including when subtitles normally use Convert mode. Channel
  configuration overlays can explicitly disable the error card.

- Add shared track selection for transient external programmes, preserving
  language preferences, custom YAML rules, JavaScript audio metadata and
  engine-specific subtitle eligibility without requiring library records.
  Carry transient programme metadata into graphics and dynamic text, and derive
  graphics EPG variables from the selected channel's external timeline.
  Prepare selected embedded text and Plex sidecar subtitles with bounded work,
  cancellation cleanup and temporary files owned by the playback caller.
  Connect native stream preparation to these bindings, return unavailable-programme
  responses for missing Plex prerequisites, and retain subtitle ownership through
  stream cleanup. Preserve optional interlace sampling and mixed H.264 profile
  detection for transient native inputs without creating library records.
  Native remote HTTP playback, XMLTV and MPEG-TS/fMP4 HLS are verified on Linux.
  HLS retains transient subtitle files through encoder completion and programme
  identity across transitions. Native streaming smoke and TSDuck checks pass;
  external JSON playback follows upstream native-engine support.

- Render image graphics in Next alongside ordered watermarks, using shared
  template and decoration selections. Allow graphics selections in Next
  playback troubleshooting, including the form without JavaScript.
- Support ordered permanent and intermittent watermarks together in Next
  playback, including all selected troubleshooting watermarks.

- Support the Next engine in the playback troubleshooting API for local and
  media-server items, with isolated finite previews, selected profiles and
  tracks, exact-version diagnostics, and engine-specific playlist responses.
- Sort add-on list review items by clicking the playlist, item date, or name
  heading in either direction; select or deselect the visible page from the
  checkbox above item selections while skipping excluded videos. Pagination
  sits above the items, and excluded and unavailable rows have visual status cues.
- Show detected Next and FFmpeg versions alongside the ErsatzRS version in
  General troubleshooting diagnostics and playback troubleshooting archives.
- Show sanitized release notes from the bundled changelog on the home and
  health pages, with an offline full-changelog link (upstream PR #2942).
- Support named, priority-ordered date ranges in Sequential YAML playouts. Each
  schedule resumes its own instruction position and in-build graphics, watermark,
  filler and roll state; continuation survives a database restart. Scheduled
  sequences expand and shuffle within their active list, and the YAML editor
  validates the new `schedules` structure. YAML `wait_until` and `pad_until`
  now use the server's local clock, matching the date selector (upstream PR #2935).
- Support `epg_entries` in imported image graphics definitions so image paths
  can use programme guide template variables (upstream PR #2923).
- Supply stored video, alternate-audio and embedded-subtitle probe hints to
  optional Next playback when media metadata is complete. This avoids an extra
  probe before local or provider playback; Next still probes incomplete sources.
  A configured fallback can now fill the interval before the first programme
  (upstream PR #2920).
- Fill internal gaps in optional Next playback with ephemeral local, provider
  and Remote Stream fallback using decoration, channel and global policy.
  Direct HTTP, ready artifacts, ordinary producers and retained intervals preserve
  selected audio and supported Burn/Convert captions, then return to the next
  programme without changing the persisted timeline. Generated credits and exact
  playlist selections, including audio/subtitles off, remain in control
  (upstream PR #2919).
- Generate music-video credits for optional Next playback in Burn and Convert
  subtitle modes, preserving playout in-points and selected alternate audio
  (upstream PR #2917).
- Scheduled Next playback preserves in/out points across selected tracks,
  including video-only media with generated silence (upstream PR #2912).
- Explicitly scheduled still images are supported by the optional Next engine,
  retaining the scheduled display interval and supplying silent audio (upstream PR #2911).
- Show the optional Next engine version in the admin navigation drawer, with
  a localized unavailable fallback when startup discovery fails (upstream PR #2910).

- Resolve direct Remote Streams and ready published local artifacts through
  session-bound Next callbacks, preserving track selection and rejecting changed
  publications. Ordinary and retained-interval add-ons use a session-owned
  producer and one-shot HTTP transfer with fresh audio/video metadata, preserving
  producer-applied seek. Finite and live ordinary streams support absolute and relative
  external subtitles in Burn and Convert modes; finite and live producers retain
  selected embedded image subtitles. Retained-interval playback preserves mapped
  text captions across cuts. Independently readable live text sidecars use bounded,
  publication-validated preparation to preserve caption timing when joining a programme.

- Retain Next error reports under the application-data `ffmpeg-reports`
  directory even when native FFmpeg reports are disabled (upstream PR #2907).
  Successful Next transcodes discard temporary reports; configuration overlays
  retain precedence.
- Apply optional Next channel configuration overlays after the FFmpeg profile:
  `next/channel-config-overlays/default.json` followed by `{channel_number}.json`
  under the application data root (upstream PR #2891). Next owns the merge and
  resolves relative paths from each overlay's directory; changes apply at the
  next engine session start. Health reports unreadable or malformed active overlays
  and distinguishes incomplete bounded inspection from engine failure.
- Display one permanent or intermittent watermark with the optional Next engine,
  preserving selected placement, actual or scaled size and margins, and forwarding configured
  opacity (upstream PR #2878), including demand-time embedded subtitle Burn.
  Intermittent overlays follow wall-clock cycles with the configured visible
  duration and Next's default one-second fades (upstream PR #2879); invalid
  timing is rejected instead of becoming a permanent overlay.
  Respect placement inside the source image when output has padding (upstream
  PR #2882); crop and stretch retain frame-relative geometry.

### Changed

- Refresh the README, installation overview, and release requirements for the
  stable Linux, Windows, and Docker release line. Explain bundled Next,
  managed FFmpeg, and the platform/hardware acceptance that remains unverified.

- Refresh the queued teletext plan for native and Next playback, exact page and
  source identity, SQLite persistence and current runtime/admin boundaries.
  Preserve the product decision and platform/runtime acceptance gates after UCU.

- Use Next for newly created channels and standalone playback previews, with
  the exact engine bundled in packages or downloaded and verified automatically
  before startup. Preserve existing channel engines and explicit Native choices.
  Keep verified installations usable offline and label native-only profile
  settings in every supported admin language. Distribute the pinned engine from
  the public package-only ErsatzRS-next repository without GitHub credentials.

- Share song artwork, BlurHash and multilingual metadata rendering between
  native and Next playback. Keep the native progress bar, preserve literal
  titles, and use default artwork when an optional cover is unavailable.

- Update the optional Next engine to the reviewed six-platform
  `next-0362692-ersatzrs.1` snapshot, including upstream colour/padding and
  legacy QSV loudness/canvas memory fixes, complete GIF/WebP programme repetition
  and bounded live RTSP buffering.
  Pin exact source, patch and archive checksums, preserve licenses and platform
  companions, and retain independent removal requirements for both local
  corrections (ADR 0117). Available Linux/Windows application checks and macOS
  package CI pass; existing hardware/platform acceptance debt remains explicit.

- Update the managed FFmpeg runtime to the published v2.0.6 packages for Linux
  x64, Windows x64, ARM64 and ARMv7, with animated WebP seeking and ICC/EXIF
  read safety (ADR 0117). All four package CI jobs pass and published checksums
  match the tested archives. Native VAAPI padding and application acceptance
  remain tracked separately.

- Use one accepted approval policy for temporary Next compatibility corrections
  during upstream catch-up (ADR 0117), retaining verified package provenance,
  acceptance gates and mandatory removal when upstream fixes each defect.
  Close UCU-09 at upstream v26.11.1; later releases are separate work.

- Preserve the accepted Next dependency in a checksum-pinned snapshot, so
  upstream replacement of its develop release cannot change or break packaging.
  Reuse the tested ARMv7 executable across archives and Docker images.
- Stage stable application archives in an unpublished draft until package
  acceptance is complete, then publish the verified bytes.

- Build the missing ARMv7 Next executable from pinned, unmodified upstream
  source for release archives and Docker images, with checksum-verified source
  and retained compiler provenance. Keep the Bookworm GLIBC 2.36 baseline and
  distinguish this build from official upstream binary downloads (ADR 0100).

- Report the stable application version as 1.1.0 in diagnostics, provider
  requests and add-on compatibility checks. Update the official yt-dlp add-on
  to 0.11.6 before upgrading the host; older packages only declare support for
  the development host version.

- Pin the accepted Next source revision during archive and Docker staging,
  rejecting upstream development-release drift after candidate CI has passed.

- Match Next's optional AMF adapter-index protocol, validating its unsigned
  range while retaining automatic device selection in generated profiles.
  AMD hardware playback remains explicitly unverified.

- Include a stable `startedAt` clock in native and Next session reports, and use
  that session clock for graphics independently of programme changes.

- Render every graphics kind in Next playback through a private authenticated
  FFmpeg canvas (ADR 0105), including scripts, both text renderers, images,
  motion and subtitle graphics. Preserve transparency, layer order, programme
  timing, content-box geometry, selected audio and ordinary subtitles across
  scheduled, dynamic, remote, fallback, mirror and preview playback.
  Session-bound callbacks validate programme identity and observe current
  graphics edits; disconnect and stop reap producers and generated artifacts.
  Check FFV1 encoding/decoding, NUT input/output support and canvas timing
  filters at startup and report missing support through FFmpeg health checks.

- Use the checksum-verified ErsatzRS-ffmpeg v2.0.5 release by default. It adds
  Linux ARMv7 and ARM64 packages and retains the exact v2.0.4 Linux x64 and
  Windows packages. ARMv7 requires GLIBC 2.36 (Debian Bookworm or newer).
  The runtime includes
  SVG decoding with correct translucent overlays and the upstream QSV/AMF
  runtime patches, including legacy Intel decoder-pool retention, D3D11 frame
  transfers and oneVPL device discovery. Preserve existing codecs and subtitle/audio playback;
  Intel QSV, AMD AMF and native Linux NVIDIA interop validation remain
  explicitly approved hardware acceptance debt.

- Add Legacy/Next engine selection to playback troubleshooting, including
  engine-specific controls and in-page preview playback while retaining the
  plain HTML playlist submission fallback. Display bounded completion logs
  after the preview process and diagnostic writes finish.
- Keep saved Classic filler expressions on the accepted deterministic NCalcSync
  6.2 profile while upstream advances to 6.4, preserving their existing
  meaning across upgrades (upstream PR #2940).
- Check new Clippy exemptions in CI and provide a local pre-commit check,
  requiring item-level scope and an explicit reason without running a crate
  build after each edit.
- Keep the official Next build and explicitly refuse unsupported live embedded
  text captions and one-shot producer embedded text without an independently
  prepared caption artifact (ADR 0100). These accepted alpha limitations remain
  visible parity debt; there is no silent subtitle loss or engine fallback.

- Add the optional Next engine's per-channel text subtitle mode: Burn or
  Convert If Possible (upstream PR #2869), with authenticated WebVTT playlists
  and preview captions.
  Conversion can read embedded text without native pre-extraction. Burn prepares
  the exact selected embedded track through a bounded, version-validated cache.
  Preview captions stay disabled after stopping playback.
  Verify selected-language conversion and seek-relative captions through
  authenticated Plex, Jellyfin and Emby inputs. Exclude provider EIA-608 tracks
  from Next selection, including locally resolved provider media; explicit pins
  fail instead of silently selecting another track (upstream PR #2882).

- Report daily gap goal-prompts in routine output instead of maintaining a
  generated prompt file; use reconciliation commits to track the change window.

- Raise the local `target/` `cargo clean` threshold from 40 GiB to 60 GiB in
  the build policy and in the `./run.sh` and `cargo xtask` size warnings
  (still overridable with `ERSATZRS_TARGET_CLEAN_THRESHOLD_GIB`).
- Bundle the optional ErsatzTV Next channel executable beside the server in
  release archives and Docker images (upstream PRs #2850, #2851), excluding
  unused Next executables while retaining checksum, license and provenance.
  Add persisted per-channel engine selection (upstream PR #2852); existing channels retain the
  native engine and Next selections use HLS Segmenter. Support the upstream
  `ETV_NEXT_FOLDER` override. Prepare executable discovery, JSON exchange and
  atomic playout publication that retains the previous schedule on failure.
  Add profile-to-Next configuration and managed process startup with bounded
  readiness, heartbeat and process-tree cleanup; reject unsupported explicit
  output codecs and HDR settings instead of silently changing them.
  Respect the profile's fit-with-bars, stretch and crop scaling choices while
  retaining selected subtitles and audio (upstream PR #2880).
  Reuse persisted audio/subtitle selection for Next item projection, retaining
  preference precedence, exact pins, source seek points and explicit silence.
  Read persisted schedules with mirror offsets and deterministic media-file
  binding while retaining scheduler locks through projection/publication.
  Serialize refreshes per channel and retain write ownership through cancellation
  so a newer schedule cannot be overwritten by an abandoned publication.
  Add the Next HLS session adapter with bounded startup, heartbeat, demand-based
  idle expiry, concurrent rebuild notification and managed shutdown.
  Connect Next local-video HLS startup to the shared session registry and
  transcode limit, retaining capacity through process/output cleanup. Preserve
  playlist tokens and support Next MPEG-TS Hybrid through the existing HLS
  wrapper (upstream PR #2883). Retain Hybrid or Segmenter when saving a Next
  channel; explicit native legacy playback stays unavailable for Next.
  Avoid a stalled MPEG-TS wrapper when Next Convert is selected: converted text
  stays available through HLS; choose Burn for subtitles in MPEG-TS, as explained
  in the localized editor (ADR 0099). Report missing
  Next executables and unsupported profiles through health checks. Use Next for
  channel previews with browser-compatible H.264/AAC and independent capacity
  and cleanup; forward playout rebuilds to preview-only sessions as well.
  Add media-server input resolution with server-specific local paths and
  authenticated Plex/Jellyfin/Emby HTTP playback, preserving selected audio and
  subtitles (upstream PR #2865). Resolve current provider credentials through a
  capability-protected internal media route when Next opens remote media,
  retaining byte-range responses without buffering whole media files.
  Verify Plex identity before failover and preserve media redirects without
  forwarding credentials across origins. Add bounded JavaScript movie/episode audio
  selectors shared by native playback and Next, with upstream callback metadata,
  authoritative stream pins, YAML precedence, safe fallback and redacted health
  reporting. CLR interop is unavailable; execution uses time, heap and stack
  limits under accepted ADR 0096 (upstream PR #2866). ARM and macOS application
  acceptance remains explicit maintainer-approved debt; these platforms have
  not been marked verified.
  Publish continued schedules to active Next channels and mirrors while preserving
  native playback continuity (upstream PR #2854). Verify existing mirror offsets,
  channel-update propagation and bundled runtime discovery against upstream PR #2855.
  Verify executable-adjacent Next archive staging for both macOS architectures
  (upstream PR #2856); active macOS release and runtime acceptance remain open.
  Verify atomic Next playout publication on Windows/NTFS, preserving the previous
  generation when sharing blocks replacement and allowing publication after release
  (upstream PR #2864).
  Add a separate Next engine check inside CI, with runtime provenance, decoded
  playback tests and HTTP/preview smoke diagnostics. Use Python 3 explicitly
  on Linux runners and propagate failures through diagnostic log pipelines.
  Remove the obsolete MySQL migration-directory build watch to prevent
  unnecessary recompilation on successive builds.

- Retain form-action selection counts and Select all under Edit multiple items
  > Items to change as an explicitly accepted design choice; Save-button and row
  markers distinguish retained changes from saved data.
- Accept editable numbering reviews: Apply saves the current Items task,
  including edits made after the proposal; Cancel restores the pre-proposal draft.
- Reconcile 183 upstream commits through `960421e8`, with accepted architectural
  decisions for the optional Next engine, graphics canvas, management boundaries
  and retained AV1 support. Preserve case-insensitive `.yml`/`.yaml` Remote Stream
  discovery, blank credits-template fallback and streaming process cleanup.

- Keep detailed future work in `plans/`, completion history and evidence in
  progress TOMLs, and reusable design artifacts under documentation. Milestones
  may omit a standalone plan; supplied plan links are still validated. The
  generated Clippy ledger now lives under `docs/reference/`.

## [1.0.37-beta] - 2026-09-22

Windows-only beta accepted under AMM-N-03: the published archive is byte-for-byte
identical to the tested draft. Linux and Windows acceptance, native recovery,
SponsorBlock and primary/fallback subtitle evidence are recorded in the milestone.
Version 1.0.36-beta failed its isolation gate and was not published.

### Added

- Add a "New season each calendar year" option to add-on list episode
  auto-detection, numbering each year's items as one season in first-aired
  order (for example, one season per year of a web series).

### Changed

- The add-on list Items page starts with its items: the publication state now
  sits beside the list title, and finished preparation counters and completed
  publication jobs are shown only under Activity & recovery. Work that is still
  running or failed stays visible as a short progress note. The Save button now
  shows whether the page has unsaved changes (highlighted only when it does),
  replacing the separate saved/unsaved text and the "Saved." banner.
- Add-on list rows use S and E for season and episode in every language (Dutch
  previously showed "A" for episode), mark rows with unsaved numbering or
  inclusion changes, and offer an "open in a new tab" link beside each title.
- The add-on list title can be edited directly in the Items heading and is
  saved with Save changes.
- The add-on list bulk tools are split into three panels: Items to change
  (scope, range and length filter), Number episodes and Detect seasons. Season
  detection now has its own first season and ignores the manual numbering
  Count, and media kind changes no longer use that Count either. A numbering
  preview is reviewed inside the tools with "Apply numbering changes", which
  saves the current Items task, including later row edits, for automatic
  publication, and "Cancel preview", which restores your earlier unsaved edits.
- The add-on list Items page shows the applied filter and search as chips with
  one "Clear filters" button, and the pager gains First page, Last page and a
  "Page X of Y" position. With JavaScript, picking a filter category or page
  size with the mouse applies it straight away and keeps focus on that control;
  keyboard changes still apply with Enter or the button. An item's page offers
  Save item at the top as well as below the form, and a chosen local
  replacement can be saved from the replacement panel itself.
- Add-on list items are now stored as `{title}-{id}` files (for example
  `My Episode-dQw4w9WgXcQ.yml` with its `.nfo`, artwork, and subtitles), so
  episodes can be found by name on disk. Titles are reduced to characters that
  are safe on Windows, macOS, and Linux. Existing `{id}` files are renamed once
  on the next sync, keeping operator edits and scheduling history; after that a
  file name no longer changes when the title does.
- Saving add-on list edits such as episode numbering no longer repeats the
  provider subtitle check for every item. An identical earlier check is reused
  until the next manual refresh of the list; scheduled refreshes reuse it too,
  and newly imported items are always checked. Adds a database migration.

### Fixed

- Release bundle verification uses a dedicated port, refuses an occupied port,
  and cleans up the native Windows candidate process tree after verification.

- Give add-on list detail progress its localized task name for screen readers.
- Native Windows acceptance tests compare canonical paths before checking
  backup containment and wait for Save/install/enable and disk-warning reloads
  to hydrate before navigating. Chapter assertions use JSON to preserve stored
  newlines across Windows SQLite output. These test corrections retain isolation
  safeguards, persisted-content checks and browser-error assertions.
- The add-on list publication status on Items and Activity & recovery is live
  again: a hydration error on page load stopped it from updating in the
  browser.
- The Unavailable health check no longer claims that add-on Remote Streams are
  missing from disk. Streams the provider reports as not playable (for example
  age-restricted videos) get their own message and are counted once, even when
  they also appear as TV episodes.
- A provider thumbnail that cannot be downloaded is now a health warning instead
  of an error, because the item is still published without it. Missing list
  artwork and uploaded artwork remain errors.
- The item "Test access" panel no longer shows a past "Next attempt" time for a
  check that has already finished.
- Finish add-on list publication when existing artwork is reused. A rescan
  after the publication commit no longer re-stamps unchanged thumbnails with a
  newer revision, which made verification fail until recovery gave up with
  `publication_recovery_exhausted`.
- Number episodes oldest first with "Auto-detect seasons from first-air dates",
  also for lists the provider returns newest first (such as channel uploads).
  Previously the newest upload became episode 1 and no season breaks were
  detected.

## [1.0.35-beta] - 2026-09-16

### Fixed

- Keep add-on list publication working when an optional remote thumbnail is
  missing or unavailable. Reuse verified local artwork before contacting the
  image server, preserve operator replacements, and log artwork fetch failures
  without rolling back otherwise valid media.

- Automatically apply saved add-on imports, refreshes, metadata enrichment and
  edits, including pending drafts from older versions, without a review or
  publication step. Keep source-removed items and usable local media, retain
  manual metadata overrides, and report technical failures through existing
  recovery and health views.
- Keep add-on NFO metadata refreshable after media projection, adopt external
  field edits, derive missing show facets from included episodes, and preserve
  content ratings when projecting television episodes.

- Start admin interactivity without waiting for browser idle time, which could
  leave Chromium pages rendered but unresponsive. Keep existing browser
  deadlines and cover startup with idle callbacks withheld.

- Isolate synthetic add-on E2E provider hosts so unrelated scenarios do not
  inherit each other's admission waits. Preserve import deadlines and exact
  counts, add safe job-state diagnostics, await import-page navigation before
  playback editing, and verify applied readiness updates instead of sleeping.

## [1.0.34-beta] - 2026-09-14

### Security

- Update `rustls` to 0.23.45 and its required `rustls-webpki` patch for
  [RUSTSEC-2026-0285](https://rustsec.org/advisories/RUSTSEC-2026-0285), which
  fixes acceptance of TLS 1.3 handshake messages at the wrong encryption level.

### Changed

- Withdraw the O-01 folder portability program and restore the preceding
  add-on-list implementation, including database-owned drafts/publications,
  existing removal behavior and historical revision actions. Preserve the
  preparation/publication fixes and unrelated parity debt cleanup.

- Withdraw the manual anamorphic regression-source release check by maintainer
  decision; retain automated NVENC command-level regression coverage and all
  other accepted Windows package gates.

### Fixed

- Keep add-on installation dialogs usable in short viewports; distinguish adding
  a repository from installing an add-on and link successful imports to their
  repository offers. Explain permissions and prioritize first-time setup.
- Link item publication blockers to review, separate unknown availability, and
  report preparation categories, task/item totals and durable activity separately.
- Preserve complete filter labels on narrow screens, show unknown durations and
  stable thumbnail fallbacks, and limit fresh-cutoff notices to a new selection.
  Display save status from native form input; remove ignored Leptos metadata.

- Allow Playback & access saves after metadata-only draft updates when the saved
  task baseline still matches, while retaining conflicts for competing playback
  edits. Preserve editor input when ancillary reads fail and report save failures
  by category. Keep subsequent saves usable after undoing playback options.

- Bind scanned items before a new add-on list's first typed-media publication,
  prevent parent metadata edits from being silently bypassed when storage paths differ,
  and distinguish a failed publication with successful recovery from a
  successful publication in the live overview and Activity.
- Avoid stale generated-file scan work when republishing exclusions, and allow
  recovery verification after the original scan has terminally failed.
- Show first-import progress until the list is ready for review, add bulk
  inclusion/exclusion through the existing Save/Discard workflow, and clarify
  Add guidance, validation links and accessible item evidence labels.
- Clarify empty add-on setup and repository-versus-add-on ZIP installation;
  label admin controls and retain Check/Save feedback without discarding edits
  in another setting.

- Retain add-on option defaults when creating a list, so Download ahead works
  without first changing a playback option. Explicit Prepare/Publish also binds
  defaults for older unbound drafts in every delivery mode; automatic refresh
  does not adopt them, and published revisions remain unchanged.
- Align the add-on-list publication heading with required preparation, show
  safe failure reasons and retries in the overview and Activity, and distinguish
  invalid preparation requests from changed inputs. A settings failure after
  adding a list now opens the created list with recovery guidance.

## [1.0.33-beta] - 2026-09-11

This section also records the changes shipped in `1.0.32-beta`, which had no
separate changelog section.

### Added

- Add permission-reviewed yt-dlp self-update: startup/daily jobs invoke the
  configured `yt-dlp -U`, defer while the executable is busy without interrupting
  playback, and show bounded version results and remediation in Add-ons/Health.
  Uncertain process cleanup after an abrupt crash requires explicit tool-scoped
  offline recovery after stopping all related processes; normal confirmed
  shutdown/restart recovers automatically. Recovery does not repair executable
  bytes; a broken executable may require manual repair. Add-on readiness warns
  when native updating failed and a newer release for the configured channel is
  confirmed, naming the program and installed/latest versions through generic
  localized text with manual-update guidance. Unknown latest versions do
  not count as stale. Replacement backup/rollback, quarantine and new manual
  update/history controls are withdrawn from scope.

- Enable opt-in Adaptive refresh in yt-dlp package 0.10.2 through a bounded,
  provider-neutral normalized-JSON probe. Matching source-bound validators avoid
  imports; changed samples enter the existing refresh workflow. Checks sample
  the first 50 entries, with a 12-second deadline and 1 MiB output caps;
  changes outside that window require a full refresh/reconciliation.

- Add advisory Health warnings for add-on-list disk usage, with editable
  thresholds of 25 GB per list and 250 GB combined by default. Zero disables
  each warning independently. Settings work with and without JavaScript;
  warnings count list-owned file sizes and uploaded artwork without changing
  download admission, deleting files or introducing retention policies.

- Acquire human-authored yt-dlp subtitles only for the configured global primary
  and fallback languages, without automatic captions. Caption artifacts use the
  existing bounded file worker and are bound to publication revisions; streaming
  maps their cues through the frozen cuts without downloading the video, while
  Download ahead uses yt-dlp's native processing with a package-local caption
  timing correction. Captions remain aligned across scheduled joins and cuts
  in MPEG-TS and transcoded HLS playback.

- Add generic list-level Stream, Download ahead, and new-videos-only delivery
  selection. New-videos-only shows its saved cutoff beside the selector;
  re-enabling it establishes a fresh cutoff while current published playback
  remains unchanged until publication. Download ahead honors optional
  SponsorBlock removal using native yt-dlp processing.

- Extend provider-neutral add-on-list option contracts with finite preparation
  requirements and an explicit access-check target while preserving existing
  schema bytes. Add native list/item editors with inherited values, explicit
  bulk preview/apply/undo and undo of first-time schema activation. Versioned
  item-operation contracts, a revision-fenced access worker, and bounded
  processed-file admission are in place. Cached files can be reused when a
  removed item reappears without taking ownership from another item.
  Enrichment checkpoints retain their saved option context across restarts;
  publication journals and retained revisions bind each item to its selected
  processed file without changing legacy snapshot digests.
  Published playback selects that file by list and provider ownership; saved
  options use playback v3 without silently falling back to legacy behavior.
  Scanning preserves the prepared duration, including milliseconds and items
  whose provider duration is missing. Preparation workers retain unfinished
  storage until process cleanup is confirmed, with a localized recovery health
  notice. The existing Prepare publication action queues required processing outside
  the publication lease, shows localized progress with and without JavaScript,
  and preserves pending attempts on repeated clicks. Managed add-on cleanup now
  checks process-group/job membership separately from leader exit before releasing
  preparation resources. Streaming and Download ahead both honor enabled
  SponsorBlock removal; the list's delivery selector is independent of
  SponsorBlock. New-videos-only applies Download ahead only to videos first
  imported after its visible cutoff; existing videos stream after publication.
  Shared timeline/input generation and immutable interval-plan storage are in
  place. Scanning uses the frozen edited duration, including milliseconds, and
  rejects conflicting or corrupt playback selections. Publication-only
  playback selections no longer mark an unchanged saved draft as edited. A small,
  bounded initial startup delay is permitted to establish continuity reserve,
  targeting ten seconds with a twenty-second maximum, including source opening
  and reserve establishment. SponsorBlock cut continuity remains mandatory.
  Metadata-only interval jobs now use the ordinary host coordinator and existing
  durable queue, without file storage or the download worker. They preserve
  source/chapter context, share provider cooldowns and await child cleanup on
  shutdown; unresolved cleanup remains blocked across restart. Normal publication
  now admits and selects timing-bound interval metadata instead of requiring a
  processed file. Automatic refresh waits for this evidence without resetting
  failed attempts or cooldowns, and frozen playback selections no longer cause
  false draft-change holds. The official yt-dlp adapter now provides bounded
  metadata-only intervals without media downloads or an FFmpeg probe, matching
  Download ahead's default removal categories. The unpublished custom CDN Range
  adapter and resolve-only operation are replaced by yt-dlp-owned transfer.
  Native yt-dlp capabilities are preferred across the remaining add-on work.
  Retained sections are decoded to one edited-clock raw-video/PCM stream for
  the host's final encoder; the same producer supplies startup metadata and
  playback. Frozen publication intent, managed cancellation and bounded reserve
  remain host-owned. Native playback metadata keeps caption and format listings
  intact within a dedicated bounded envelope; incomplete audio codec labels do
  not override yt-dlp's selection before the actual media probe. The required
  real-video MPEG-TS/HLS gate passes under delay/jitter after removing repeated
  native NUT identifiers and resolving fractional video timestamp collisions
  with retained-only VFR. It checks decoded/player startup, cut continuity,
  source-content correspondence and concealed host failures.
  Both channel encoders consume owned retained inputs and restart the producer
  for hardware retries, preserving explicit audio choices and redacting
  private diagnostics. Retained playback now has a bounded paced MPEG-TS output
  reserve and completed-segment HLS startup gating, with shared-deadline graphics
  and watermark probing. Selected subtitles with an unmapped edited clock are
  explicitly refused. Bounded buffering and actual-host cancellation are
  verified for scheduled MPEG-TS/HLS; direct MP4 reserve support is not implemented.
  Streaming completion now explicitly requires the maintainer-selected real
  yt-dlp video to pass scheduled MPEG-TS and HLS SponsorBlock cut tests;
  synthetic playback proofs alone are insufficient.

- Add an opt-in automatic publication policy for scheduled add-on-list
  refreshes after the first explicit Publish. Only validated additions use
  the existing exact-revision publication lease; changes to current items,
  removals, conflicts, access restrictions, unavailable items, or failed
  preparation remain review drafts with localized Activity and Health notices.
  Manual Refresh, Reconcile, and Reset and reload still require review.

- Add non-destructive full-source reconciliation for add-on media lists. A
  complete validated scratch import previews additions, provider changes,
  reordering, reappearances, and missing items while retaining operator edits
  and the current publication. Missing items require a second complete
  observation at least 24 hours later or explicit confirmation before Publish
  removes them, and Publish atomically accepts the matching staged archive.

- Add transactionally checkpointed incremental discovery for add-on media
  lists. The host gives providers only an invocation-private archive, commits
  the archive and page together, preserves the last complete checkpoint across
  cancellation and restart, and exposes exact examined, skipped-known, and
  emitted counts. The official yt-dlp package uses this contract on Windows
  and POSIX without unsafe early stop, so unchanged playlists emit no duplicate
  items and reordered playlists cannot hide a later unseen item. A separate
  bounded playlist-header probe refreshes rich provider metadata even when the
  item delta is empty, while preserving operator overrides, conflicts, and
  richer fields omitted by flat extraction.

- Add localized fixed and opt-in adaptive refresh cadences for add-on media
  lists. One restart-safe coordinator persists due, retry, success-anchor,
  conditional-validator, and bounded source-change history state; unchanged
  probes avoid full imports, failures honor bounded backoff and Retry-After,
  and Health reports disabled or capability-lost adaptive configurations and
  terminal refresh failures without exposing provider diagnostics.

- Add durable non-mutating source tests, exact saved-draft and reset-impact
  previews, and digest-validated publication history for add-on media lists.
  Operators can compare retained revisions and restore an older snapshot through
  the normal publication lease as a new monotonic revision; revision-owned files
  remain pinned, list sorting is keyboard accessible, and destructive dialogs
  now contain and restore focus for hydrated and no-JavaScript workflows.

- Give Remote Stream search results canonical detail links and an SSR-first,
  localized detail page showing effective metadata, artwork, tracks, liveness,
  duration, availability, owning add-on lists and their published options,
  playback health, and applicable channel actions. Add-on-list-filtered search
  now indexes only memberships from the committed published snapshot.

- Add a durable typed-confirmation Reset and reload action for add-on media
  lists. It removes derived state before a first-generation full import while
  retaining approved configuration and the empty generated-playlist identity;
  list and item metadata stay visible and editable with hydrated/no-JavaScript
  parity, and absent episode numbering is non-blocking and shown as Unnumbered.

- Add a provider-neutral add-on-list lifecycle persistence foundation for
  versioned declarative options, explicit refresh cadence and publication
  policy, durable reset/reconcile and incremental checkpoint state, retained
  revisions, tombstones, bounded artifact ownership, and redacted tool-update
  history while preserving legacy add-on request contracts.

- Allow administrators to remove an uploaded offline add-on repository bundle
  from Sources without uninstalling package snapshots or deleting their
  settings and data. Checkout sources remain operator-owned and cannot be
  removed through ErsatzRS; both hydrated confirmation and no-JavaScript form
  submission are supported.

- Allow editable playlist items to be reordered with localized Move Up and
  Move Down submit buttons or JavaScript-enabled drag-and-drop. Both paths save
  the same persisted item indexes, while generated playlists remain read-only.

### Changed

- Windows beta tags now create an unpublished draft for exact-archive
  acceptance. Publication promotes that same tested ZIP; workflow reruns refuse
  to overwrite a published beta. Exact-SHA green CI remains required before
  tagging, and failed candidates consume their beta version.
  Windows beta compilation is limited to two Cargo jobs on the shared runner.
  Changelog headings keep the immutable tag/candidate date when publication
  follows later.

- Dependencies updated: quick-xml 0.42, process-wrap 10 and jsonschema 0.52,
  plus minor and patch updates to uuid, mlua, flate2, base64, futures-util,
  minijinja, open and syn. NFO and XMLTV guide parsing adapt to quick-xml's
  string-based element names; Windows add-on job tracking keeps its spawn
  ordering under process-wrap 10.

- Scope this add-on lifecycle closeout's platform verification to Linux x64
  and Windows x64. ARM/macOS verification is deferred; declared platform
  support is unchanged.

- CI runs the Chromium and Firefox Playwright suites one after the other
  instead of side by side. Both self-hosted runners share one machine, so
  overlapping browser suites competed for its CPU and memory. A run takes
  roughly ten minutes longer; the second suite still runs when the first
  fails. Each project has its own result, temporary database and web host,
  with one worker per suite because scenarios share application state.
  Separate concurrent workflow runs can still overlap.
- CI no longer retests a commit it has already proved. A `Preflight` job asks
  the API whether the pushed SHA already has a successful CI run and skips the
  gate when it does, so a direct fast-forward merge to `main` stops rerunning
  the feature branch's green run on an identical tree. Pull requests and manual
  dispatches always run in full, and a PR merge commit is a new SHA, so neither
  is ever skipped. The release-candidate gate in `BUILD.md` now cites the run
  that executed the gate: a skipped push run is recorded as provenance, and the
  tag requires every gate job to report success rather than a green workflow
  alone.
- CI skips docs-only pull requests, matching the non-code `paths-ignore` that
  already applied to `main` pushes. This is safe only while the repository has
  no branch protection; if required status checks are ever enabled, docs-only
  PRs need a sentinel job reporting under the required names instead.

- Run the CI test jobs through nextest with the same target selection as before
  (ADR 0090), under a new `ci` profile that keeps the no-retry,
  run-through-failures and slow-test-after-60s semantics and adds a JUnit
  report. Per-test durations are uploaded as a build artifact, so growth in CI
  test time is attributable to individual tests instead of one wall-clock
  number. Doctest coverage is unchanged: neither selection ran them, and
  `./run.sh test-closeout` remains the only path that adds them.

- Adapt Add-on Lists to the locked task layout with separate item review,
  settings, playback/access, activity and artwork pages. Native forms retain
  unsaved edits and selection across navigation, and task saves use coherent
  transactions with revision and metadata checks. Add 10/25/50/100/All paging,
  exact bulk selection, fixed adjacent-item controls and vertically stacked
  access settings on wide displays. Artwork uploads remain
  attached to unsaved edits, and publishing keeps newer input unsaved.
  Item review separates editable fields from saved-item tools, keeps First aired
  visible, makes numbered/unnumbered choices explicit, and groups optional
  metadata and replacement selection behind restrained disclosures. Shorter
  option guidance, contextual action labels and clearer light-theme field edges
  make the form easier to scan; existing dates and unsaved replacements are
  preserved through native navigation and Save. Publication readiness updates
  after background preparation without reloading the page or losing unsaved input.

- Move Settings to the bottom of the admin navigation drawer and collapse the
  expanded Support group into a single badged `Support` link beside it
  (ADR 0089). Health checks, Logs, and Troubleshooting now render as in-page
  route tabs, reusing the pattern Settings already uses for its seven pages, so
  the health warning/error badge stays visible no matter which drawer groups are
  expanded. Every `/system/*` URL is unchanged; Logs and Troubleshooting cost one
  extra click from the drawer, and the `/system/troubleshooting/*` detail pages
  carry the same tab strip.

- Reopen the add-on-list atomic-publication contract after Windows dogfood
  showed that a failed post-scan directory rollback can leave an otherwise
  valid recovery journal permanently blocked. AMM-J now owns durable restart
  recovery, Windows/POSIX filesystem retry, localized operator actions, and
  item-scoped continuation for provider-restricted entries. Its integrated
  packaged Windows restart repetition remains part of the AMM-N closeout
  instead of blocking the focused recovery-UI milestone.

- Accept ADR 0085 and broaden the open Add-on Metadata Management roadmap
  through AMM-N: coordinate publication with every scan origin, make blocked
  and reset/reload workflows explicit, retain redacted causal diagnostics,
  preview source/change/downstream impact, compare/restore bounded revisions,
  reconcile without erasing edits, expose inline metadata and Remote Stream
  details, add fixed and opt-in cadence-aware adaptive refresh plus observable
  safe yt-dlp incremental discovery, carry inherited/bulk
  browser-cookie/SponsorBlock/subtitle behavior into
  playback with advisory storage warnings, and run permission-reviewed native
  `yt-dlp -U` at startup and daily. Replacement backup, rollback, quarantine,
  retention policies and expanded update administration are outside this scope.

- Refresh the dependency rollup, including `base64` 0.23 and
  `ed25519-dalek` 3.0 for signed add-on repositories, while preserving the
  lowercase boolean and `none` output of existing XMLTV and graphics templates
  across the MiniJinja 2.23 update.

### Fixed

- Preserve the Windows application-data locations needed by yt-dlp to find
  the selected browser profile, without forwarding unrelated environment
  values or handling cookie contents in ErsatzRS.

- Isolate package verification from inherited database settings and application
  state. Application logs now honor the existing application-data override,
  keeping disposable verification logs with the rest of its test data; normal
  default log locations are unchanged.

- Stop the `ersatzrs-web` build script from failing when a Cargo target
  directory is reused from a different checkout path. It read
  `CARGO_MANIFEST_DIR` through `env!`, which bakes the checkout path into the
  compiled build script; Cargo reuses that binary after the target directory
  moves, so a CI runner whose work folder was relocated failed with
  `read admin language directory: NotFound`. The path is now read when the build
  script runs.

- Run the `ersatzrs-web` integration tests on the manual GitHub-hosted CI
  fallback. Its `web-bins` shard ran only `--bins`, leaving the crate's
  `tests/*.rs` targets unrun on that path while the self-hosted job covered
  them through `--tests`.

- Stop an add-on list from getting stuck in an endless publication recovery.
  The publication verification scan rewrites its own scanner sidecars inside the
  managed folder it is about to verify, so the sealed revision fingerprint no
  longer matched itself and recovery retried the same failure forever, leaving
  the list permanently "Publish blocked". The fingerprint now covers published
  content only, ignoring scanner state files and empty generated folders;
  intact revisions sealed by earlier versions are verified and upgraded before
  the scan rewrites their sidecars.

- Keep a list in `Publishing` once its publication has selected the revision it
  verifies, including after retry exhaustion until its recovery journal is
  finalized. An ordinary edit landing between verification retries could
  overwrite that state and leave recovery unable to converge. A publication that
  never reached verification still releases the draft when its lease expires.

- Give publication recovery a retry budget. A permanent failure now ends as a
  terminal, operator-actionable state with its recovery actions available,
  instead of retrying on a timer indefinitely.

- Keep the add-on preparation worker alive while the host monitors its exit;
  release its liveness pipe only on shutdown so queued preparation can run.

- Add-on list publication verifies artwork in its owning media folder, including
  nested playlist folders, while retaining managed-folder and content-hash checks.
- Saved add-on lists with provider-supplied person ordering or thumbnails no
  longer appear edited just by opening the metadata editor and hide publication
  preparation. Unchanged people fields retain that metadata in both hydrated
  and HTML-only saves.

- Use the retained prepared file for add-on-list items in HLS-backed Hybrid
  playback, matching native TS. Missing or invalid published artifacts fail
  safely instead of invoking raw provider playback.

- Keep unchanged generated playlist sidecars byte-stable during publication
  verification, so a timestamp-only rewrite cannot strand a sealed publication
  in recovery. Normal refreshes still update their synchronization time.

- Render the add-on-list index and Remote Stream details without JavaScript,
  including native sorting and channel links, instead of leaving a loading
  placeholder while their content remains in hidden streamed fragments.
  Keep JavaScript-only revision-restore buttons hidden when scripting is off.

- Use familiar `S01E01` episode labels throughout the admin UI in every culture,
  preserving localized Unnumbered labels, season-zero specials, and media filenames.

- Repair add-on-list CI coverage for refresh controls, source-probe fixture
  isolation, save/reload synchronization, supported option schemas, and retained
  checkpoint invalidation without changing the intended runtime behavior.
  Isolate reset lifecycle jobs, identities, and directories between browser
  scenarios so queued work cannot mutate the next fixture, and preserve the
  expected draft revision after tombstone confirmation in storage fixtures.

- Retain a bounded, ordered, redacted activity history for each add-on-list
  publication and recovery job. The localized editor, Health, and read-only
  runtime audit now distinguish original failures from later recovery failures;
  hydrated Copy and no-JavaScript Download actions expose only typed phases,
  counts, outcomes, and sanitized operating-system error details.

- Show durable add-on-list publication phase, attempts, last progress, and
  redacted failure codes in administration and health, with generation-fenced
  Retry recovery and Restore previous revision actions for hydrated and
  no-JavaScript use. Draft settings and item edits now lock only while a live
  publication lease is executing; expired work is shown as recovery-required
  instead of indefinitely Publishing.

- Add the `media-list.list.v6` typed availability reason contract. Official
  providers can classify authentication and content restrictions without
  exposing cookies or raw provider diagnostics; restricted entries remain in
  review, preserve the current published snapshot through refresh, and are
  excluded item-by-item at the next explicit publication unless a valid Local
  replacement is selected.

- Keep add-on-list Save and Publish actions disabled until item edits have
  refreshed publication readiness, preventing a fast follow-up publication
  from submitting the previous saved revision.

- Run exact-revision add-on-list publication and recovery as durable background
  work behind one shared managed-library lease. Publication now survives
  request cancellation and process restart, deduplicates repeated Publish
  requests and provider refreshes, transactionally fences expired workers,
  waits for startup replay before admitting dependent refreshes, cancels scans
  that lose their heartbeat, preserves concurrent provider changes as a later
  draft, and adopts stranded pre-upgrade publication journals for recovery.

- Make add-on-list filesystem publication resumable across process exits and
  Windows sharing violations with revision-owned integrity markers, immutable
  byte-copy backups, bounded retry, pre-upgrade terminal-state adoption,
  pre-sealed full-tree fingerprints, a post-swap Local verification scan,
  exact logical projection/cardinality checks, active-publication writer
  fencing, and journal-last terminal cleanup after filesystem and SQLite
  convergence.

### Removed

- Retire official Beeld & Geluid support and its live acceptance requirements.
  Current packages offer yt-dlp only; existing operator-managed installations,
  saved media and provider-neutral add-on features are not automatically removed.

- Remove the remaining hard-coded Trakt assumptions from the core playlist and
  add-on-list admin pages. Provider-specific list discovery and parsing remain
  add-on-owned, with an architecture guard preventing them from leaking back
  into these host pages.

## [1.0.31-beta] - 2026-08-30

### Added

- Add named advanced-search drafts to the smart-collection composer. Queries
  can be saved, recalled later, modified, and updated without creating a smart
  collection or triggering collection and playout refresh side effects.

- Add a repository-local Context7 documentation skill that targets current,
  version-specific external contracts, keeps retrieval to the smallest useful
  query set, and avoids calls when local repository evidence is authoritative.

### Changed

- Separate media discovery from durable smart-collection authoring. Global
  search now stays results-first with a compact common-filter disclosure and
  exact query handoff; the smart-collection route uses localized
  Match all/Match any/Exclude any cards, focused nested-group editing, a live
  result count, and the same canonical query for hydrated and no-JavaScript
  saves. Search result counts and bulk actions now reflow as distinct,
  higher-contrast controls, and the mobile navigation cannot obscure the
  search or composer after a desktop-to-mobile transition. See ADR 0083.

- Move Remote Stream playlist filesystem health inspection onto Tokio's
  blocking pool and persist validated effective liveness in version 4 playlist
  sidecars. Health verifies that metadata against the generated-definition hash
  instead of synchronously YAML-parsing every durationless entry.

- Skip a Remote Stream whose playback behavior or duration is unknown instead
  of refusing the whole playout build, matching ErsatzTV's `PlayoutBuilder`,
  which drops zero-duration items from a collection and keeps scheduling. A
  single add-on episode that resolved without a duration previously left its
  channel with no playout items and nothing to stream. Dropped items are now
  reported as playout build warnings.

### Fixed

- Recover severely under-delivering live HLS hardware output by trying one
  software H.264 restart before lowering source quality. Temporary software
  decisions are isolated by channel and the complete FFmpeg profile, verified
  720p decisions can still escalate, and startup jitter retains a 20-second
  observation floor. See ADR 0084.

- Recover live add-on HLS playback that cannot sustain 1080p by requesting one
  provider-neutral 720p rendition fallback, verifying it against playlist
  delivery, remembering it for 24 hours, and reporting the temporary quality
  reduction through localized health guidance. See ADR 0084.

- Keep the Windows beta web-bundle gate independent of runner-owned FFmpeg
  drift. Auto-discovered overrides are now accepted only when they report
  FFmpeg major version 9; otherwise the gate uses the normal managed
  ErsatzRS-ffmpeg acquisition path instead of failing on an older PATH tool.

- Preserve canonical search semantics across structured edits, including
  multiword text, wildcard, phrase, and fuzzy values; reject incomplete queries
  during initial projection and durable saves; wait for the current composer
  model before saving; keep the editor locked until projection completes; and
  preserve the exact `/search` return URL across no-JavaScript save failures.
  Composer validation is localized in every supported admin culture.

- Validate managed Remote Stream definitions through the production parser
  before treating them as duration-exempt live entries in health, preventing an
  invalid definition with no playback URL from producing a passing result.

- Preserve an existing Classic playout when Remote Stream filtering leaves a
  required schedule source empty. Reset and Refresh now refuse the destructive
  rebuild instead of clearing the materialized timeline without replacement.

- Detect live HLS segment under-runs even when every received MPEG-TS packet is
  structurally valid, expose recent events through health, and make the manual
  TSDuck gate compare PCR media time with wall time without misclassifying
  variable-rate HLS from packet count and nominal bitrate.

- Stop health checks from reporting durationless live Remote Stream playlist
  entries, excluded add-on detail failures, and unpublished television
  projections as actionable warnings.

- Play sidecar subtitles whose filename contains an apostrophe, comma or
  semicolon. Those characters reached the ffmpeg `subtitles` filter unescaped,
  where an apostrophe opened an unterminated quote that swallowed the rest of
  the filter graph — including the `fontsdir` drive-colon escape behind it — and
  killed the channel with "No option name near ..." at the moment the affected
  episode started. See ADR 0082.

- Keep Classic playouts refreshable after the first build. Checkpoints are now
  captured wholly at the 100-nanosecond tick precision used by persisted
  timestamp columns, including nested Smart Shuffle release times and duration
  boundaries. A checkpoint therefore preserves both its scheduler continuation
  time and cooldown selection after a database round trip. Previously a
  sub-tick seam could either refuse every Refresh after a Reset or resume a
  duration cooldown early and change the future media order.

## [1.0.29-beta] - 2026-08-28

### Changed

- Rework desktop Settings around one global navigation entry, localized route
  tabs, responsive section cards, and a standardized sticky save action. Logs
  now uses the full content width with a compact search toolbar, accessible
  sorting, and the shared localized pagination controls.

- Replace public HTTPS add-on catalogs, signing keys, and automatic refreshes
  with operator-managed local Git checkouts and complete offline repository
  ZIPs. Add-ons are still installed and updated individually as immutable
  snapshots behind the off-by-default Unknown Sources gate; existing catalog
  installations remain runnable as legacy unmanaged packages and can be
  explicitly adopted by a local source without losing settings or data.

- Standardize `/media/*` pagination with a built-in localized `Rows per page`
  option list, range and page status, accessible previous/next actions,
  44-pixel targets, responsive wrapping, and SSR-safe link navigation.

- Require a completed, successful CI run for the exact release-candidate
  commit before creating any alpha, beta, or normal release tag.

### Fixed

- Keep add-on detail pages hydrated while they render the shared Settings
  route tabs inside the island boundary.

## [1.0.28-beta] - 2026-08-27

### Changed

- Rework add-on-list review around the collection: publication readiness now
  uses a non-sticky secondary rail on wide screens and returns to normal flow
  before the review workspace becomes cramped.

- Reject Windows beta publication when the packaged server and hydration
  assets fail the structural web-bundle consistency gate.

### Fixed

- Keep large add-on-list validation sets usable by grouping repeated blockers
  in the publication-readiness rail, showing the blocking reason on affected
  rows, opening episode numbering directly, and refreshing collection metadata
  when overview import becomes ready without overwriting operator edits.

- Keep the Proposal B publication-readiness flow within the add-on-list review
  workspace at intermediate viewport widths.

## [1.0.27-beta] - 2026-08-26

### Added

- Add repository-scoped admin UX design and audit skills with a Tunarr-inspired
  semantic HTML/CSS visual contract, WCAG 2.2 and responsive review, deceptive-
  pattern checks, and an evidence-backed UI component catalog.

- Add undoable direct and bulk add-on-list review controls, cursor-based
  episode numbering, date-gap season detection, and responsive bulk media-kind
  and playback-behavior editing.

### Changed

- Made every canonical repository skill discoverable through the current
  `.agents/skills/<name>/SKILL.md` layout while retaining the existing Codex and
  Claude compatibility pointers, with cross-platform mirror validation to
  prevent skill contracts from drifting.

### Fixed

- Keep the add-on-list publication-readiness heading in normal document flow
  with quieter typography while preserving its information, contrast, and
  accessible section relationship.

- Make add-on-list import, review, and publication self-explanatory with
  automatic Ready materialization, authoritative saved-draft and blocker state,
  faithful no-JavaScript create/save/review/publish forms, actionable readiness,
  explicit per-row save/error feedback, keyboard-safe mobile navigation, and a
  compact responsive review layout proven from 320 CSS pixels through 4K.

- Queue playback-required detail enrichment before publishing finite add-on
  streams with missing durations, clarify item-inclusion controls, and keep
  provider-distinct rows stable while the review table updates.

## [1.0.26-beta] - 2026-08-25

### Added

- Accepted ADR 0077 and added a typed FFmpeg 9 `drawtext` filter/pipeline leg
  beside drawvg, with inline or reloadable file text, explicit font resolution,
  layout and styling controls, cross-platform escaping, z-ordered composition,
  hardware-frame bridging, runtime capability enforcement, and first-party
  package gates for the complete text-shaping stack. Text graphics now expose a
  localized drawvg/drawtext choice while preserving drawvg for existing JSON;
  the bounded minijinja controller atomically refreshes live UTF-8 text in the
  same FFmpeg process, is wired through IPTV/HLS/retry/preview cleanup, reports
  invalid configurations through health, and has hydrated plus no-JavaScript
  editor coverage.

- Added focused repository skills for fast Rust iteration, read-only runtime
  incident analysis, stateful Refresh continuity, SQL cardinality/idempotency,
  durable background work, and Windows dogfood acceptance. New cross-platform
  `affected-checks` and read-only SQLite `runtime-audit` developer commands
  turn changed paths and runtime state into bounded, redacted evidence without
  executing broad gates or mutating an operator database.

### Changed

- Accepted ADR 0080 and standardized Runtime, Cargo, migrations, current
  documentation, repository skills, agent checklists, developer scripts,
  affected-check recommendations, and CI on SQLite as the sole supported
  database backend. Removed MySQL/MariaDB URLs now fail with migration guidance.
  The local closeout exercises one workspace graph plus doctests. The
  maintainer confirmed that the retired optional backend had no user
  deployments, so no operator conversion procedure or release blocker is
  required.

- Reopened the Classic Refresh and add-on television projection parity
  contracts after Windows dogfood exposed two release-correctness defects:
  scan-triggered Refresh can rewind a shuffled Classic playout when no dated
  checkpoint exists, and duplicate thumbnail projection can multiply episode
  rows in the television browser. Corrective milestones now require safe
  existing-database handling, scoped scan invalidation, idempotent artwork,
  and SQLite, MySQL, browser, restart, and health proof.

### Fixed

- Official yt-dlp 0.7.1 and Beeld & Geluid 0.9.2 add-ons now accept the full
  direct-video, playlist, shared-list, dated-search, series, and nested-episode
  URL matrix on Windows and POSIX. Direct yt-dlp objects become one-item lists
  with their finite/live behavior intact, provider query parameters and order
  survive aggregate pagination, and invalid output cannot become a successful
  empty draft. Existing failed or empty drafts expose a localized retry that
  replaces only the incomplete generation while preserving operator state;
  health links directly to that recovery path.

- Beeld & Geluid add-on 0.9.1 now preserves every series, shared-list, and
  saved-search query parameter while replacing only pagination; merges HTML
  cards with embedded provider JSON by stable episode identity; and lets
  explicit `isPlayable=false` evidence override optimistic card defaults. The
  exact eight-item Strip- en cartoontekenen overview now retains “Les 7” as
  `unavailable` / `not_playable` without a detail request on Windows and POSIX,
  so a Local replacement can be selected and published. Reimport also repairs
  affected seven-item drafts without duplicates or provider-order drift.

- Made canonical skill-mirror validation handle both real Git symlinks on
  Linux and flattened pointer files in Windows checkouts, eliminating a
  platform-specific `xtask` CI failure.

- Restored the scoped `cargo-deny` exception for the RSA timing advisory that
  is bundled by `jsonwebtoken`'s RustCrypto provider. ErsatzRS IPTV tokens are
  restricted to HS256 shared secrets and never execute the affected RSA
  private-key operation; the corrected policy replaces the obsolete MySQL
  rationale and keeps dependency CI enforceable.

- Add-on television publication now canonicalizes repeated artwork candidates,
  reads one deterministic thumbnail per projected item, repairs only safe exact
  duplicates in existing SQLite/MySQL databases, and reports aggregate
  projection-cardinality problems through localized health diagnostics.

- Remote Stream membership scans now refresh only Classic playouts whose
  current schedules can consume the changed media or managed playlist. Stable
  membership deltas produce one reasoned, coalesced refresh intent per playout;
  the intent commits atomically with playlist reconciliation, survives restart
  before queue admission, and exposes failed or stale work through health.

- Classic Refresh now restores complete, production-generated day checkpoints
  across process restarts instead of rewinding shuffled media. Checkpoints
  preserve schedule, source, fill-group, Smart Shuffle, Random, and Random
  Rotation state; membership changes reconcile by stable playback identity,
  and legacy continuation-only databases refuse the destructive rebuild with
  an actionable Reset/health diagnostic while leaving the playout window
  untouched.

- Toast lifecycles now use measured elapsed time, so delayed browser timer
  callbacks cannot stretch the configured display and fade durations after a
  hover or touch pause.

### Removed

- Removed the retired MySQL/MariaDB migration tree, live MySQL CI job,
  MySQL-specific check graph, and the obsolete RSA advisory exception carried
  only for SQLx MySQL authentication.

## [1.0.25-beta] - 2026-08-23

### Changed

- Add-on Lists now preserve provider-neutral unknown/finite/live playback
  behavior through review, publication, scheduling, IPTV, HLS, health, and
  yt-dlp. Remote Stream remains the collection type for both live and on-demand
  media, while unsupported or unknown live behavior is refused safely.

- Add-on list imports now publish a verified paginated overview before optional
  item-detail requests and resume both discovery and enrichment from durable
  checkpoints. The fair-use scanner serializes automatic and manual requests
  per provider host using a randomized global range configurable under Add-ons
  (2–16 seconds by default), without shortening an existing cooldown. Lists can
  stay overview-only, every row can fetch details manually, and the editor shows
  durable progress, item number, first-air date, season/episode columns, and
  exact accessible bulk-numbering previews. Beeld & Geluid and yt-dlp share the
  provider-neutral workflow on POSIX and Windows.

## [1.0.24-beta] - 2026-08-22

### Added

- Add-on list imports now open as a reviewable two-step Import/Publish draft
  using one localized metadata editor for Beeld & Geluid, yt-dlp, and future
  providers. The shared `media-list.list.v4` document covers list and item
  titles, dates and numbering, descriptions, genres, large tag sets, credits,
  languages, GUIDs, and role-aware poster/fanart/thumbnail candidates. The
  editor supports per-item inclusion, a filterable review count, inline
  accordion editing, movie primary/extra roles independent of source order,
  original-order playlist creation, and pagination across every collection
  type before explicit publication. The official yt-dlp package keeps the
  frequently updated `yt-dlp` executable in charge of all provider extraction;
  its shared POSIX/PowerShell mapper only translates the complete JSON result.

- The standalone add-on validator can now validate captured media-list NDJSON
  through the exact runtime conformance parser, allowing provider adapters to
  reject invalid metadata types and formats before publication.

- Television add-on drafts can now assign one season and consecutive episode
  numbers to all missing entries in original provider order, optionally in
  bounded batches for multi-season lists. Existing numbering is preserved by
  default, occupied numbers are skipped, and an explicit override remains
  available alongside per-episode editing and pagination.

- New built-in music-video-credits preset `_MusicFactory_ArtistTitle`, a
  "Music Factory" (TMF) styled artist/song-title card alongside the MTV-styled
  `_default.ass`. It renders two equal-width stacked bars bottom-left — a black
  bar with the gold artist name flush on top of a gold bar with the black song
  title — faded in and out over the same 9s–16s window; the TMF brand gold is
  `#EFB832`. The "MUSIC FACTORY" wordmark and TMF channel bug are intended as
  separate always-on image graphics/watermark overlays rather than baked into
  this per-song card.

- Music-video-credits templates can opt into real `minijinja` rendering with a
  leading `{# ersatzrs:minijinja #}` marker, so a template's whole layout —
  sizes, margins and positions derived from the output resolution — lives in
  the template instead of a per-preset Rust renderer (ADR 0076). The
  `_MusicFactory_ArtistTitle` preset uses this, and a render error falls back to
  the built-in default card. Existing token/interval presets are unchanged.

### Changed

- Re-running a Windows beta tag now replaces its existing release archive
  after rebuilding and validating it, while first-time beta tags continue to
  create a prerelease normally.

- Windows release packaging now tests startup registration through a small
  platform adapter instead of compiling the complete web test harness in a
  second Cargo profile. The release still performs the isolated real-HKCU
  registry round trip on its Windows runner and exact source revision, and the
  packaging gate verifies the ZIP contains a non-empty `ErsatzRS.exe` startup
  launcher at the package root.

- Scheduled an additive FFmpeg `drawtext` overlay leg after add-on metadata
  closeout and before Phase 9 teletext. ADR 0077 keeps `drawvg` as the default
  static/vector-rich backend while defining reloadable text files for live
  clocks, countdowns, and changing now/next or video-jukebox presentation.

### Fixed

- Existing add-on lists can now be reclassified after import to any supported
  library type. Save and Publish replay the durable provider snapshot instead
  of invoking the add-on again, while preserving source identity, original
  order, inclusion choices, and operator metadata edits; the list detail page
  shows both the current library type and any pending type change.

- Publishing a reviewed add-on list now bypasses a fresh empty draft cache, so
  the staged item selection is materialized into its managed library on the
  first publish; existing published lists with that stale empty state repair
  themselves on their next scan. Remote Stream health also understands v3
  sidecars and reports successful synchronizations with no active entries.
  Shared artwork downloads now identify ErsatzRS to providers that reject
  anonymous HTTP clients, preventing Beeld & Geluid episode stills from
  failing publication with HTTP 403. Reviewed NFO generation also retains
  list-wide tags and metadata defaults alongside item metadata.

- Replaying a reviewed publication snapshot now treats a legacy nullable extra
  artwork list as empty across both SQLite and MySQL instead of failing the
  publication during row decoding.

- Make the Windows `test-closeout` wrapper run both the default and
  all-features nextest matrices, matching Linux and preserving the
  SQLite-only integration coverage hidden when MySQL is enabled.

- Make the structural web-bundle and Playwright verifiers use native
  drive-letter paths when Git Bash launches Windows processes, and force their
  temporary hosts to loopback, so the hydration gates can open their temporary
  SQLite databases without triggering a Windows Firewall prompt.

- Keep Windows browser-gate artwork and runtime caches inside the throwaway
  test root by explicitly overriding both ErsatzRS state directories; Windows
  known-folder resolution otherwise ignores an isolated `HOME`.

- Generate a managed list-level `tvshow.nfo` or `artist.nfo` from the
  add-on's title and description before applying an automatic library
  presentation, including before the first library scan. An empty untracked
  placeholder is healed instead of being reported as malformed XML, while real
  user-authored NFO files keep their ownership protection.

- Project-local Codex hooks now invoke the real Windows Python installation
  through repository-relative paths, avoiding repeated `PostToolUse hook
  (failed)` reports caused by the inaccessible Windows Store `python3` alias
  and Git's sandbox-user ownership check.

- Workspace test binaries and the native `ersatzrs-web` dev package omit debug
  information, preventing the Windows web test harness from exceeding MSVC's
  program-database limit and its RLIB from exceeding the 4-GiB archive limit.

- Bind the runtime smoke host exclusively to loopback, avoiding an interactive
  Windows Firewall prompt for its ephemeral debug executable.

- Enforce the selected FFmpeg profile's scaling behavior, output frame, and
  audio channel count for managed Remote Stream items whose media properties
  are unavailable until FFmpeg opens the add-on stream, instead of silently
  preserving the source geometry or channel layout.

- Decode escaped Beeld & Geluid descriptions consistently in the official
  Windows add-on package, including embedded quotation marks and newlines,
  instead of rejecting otherwise valid series metadata as malformed JSON.

### Removed

- Dropped the unread `pin_id` field from the Plex sign-in island's polling
  state; the poll loop already used its own cloned copy.

### Security

- Updated `h2` to 0.4.16 for
  [RUSTSEC-2026-0258](https://rustsec.org/advisories/RUSTSEC-2026-0258): the
  version reached through `hyper`/`axum` accepted and queued empty HTTP/2 DATA
  frames without limit, so a client that did not drain its streams could drive
  unbounded memory use, or a panic on length overflow, against the admin UI and
  the IPTV/HLS endpoints. Low severity.

### Added

- Managed add-on-list Remote Stream entries now fetch their first provider
  image beside the generated definition. The bounded, manifest-host-validated
  download accepts only JPEG, PNG, or WebP; it keeps operator-replaced files,
  skips unchanged generated artwork on refresh, and records per-item failures
  without aborting the rest of a list.

- Add-on-list library projections now carry their NFO genres, tags, studios,
  cast, credits, identifiers, and artwork into the projected metadata rows for
  TV, Movie, Music Video, Other Video, Song, and Image presentations. Refreshes
  replace those facets instead of accumulating duplicate rows.

- Each add-on list entry now owns its media kind. The classification the add-ons
  already derived from provider metadata was stored but never read, so it was
  invisible and the list-wide value decided each entry's NFO root — a mixed list
  wrote every entry under one wrong root. The list-wide control remains as an
  explicit **Apply to all items** bulk default that confirms before overwriting.

- The add-on list collection type now offers every library kind — Auto, TV Shows,
  Movies, Music, Other Videos, Songs, Images, and Remote Streams — and every one
  of them is projected. **Auto** derives the presentation from the entries and
  only enters a projection every entry agrees on; a mixed list stays on Remote
  Streams rather than being guessed into a hierarchy.

- An add-on list can be projected as **Movies**, **Other Videos**, **Songs**,
  **Images**, or **Music Videos**. Music Videos gets one managed artist above the
  list, named by `artist.nfo`, because a music video cannot exist without an
  artist; switching away removes that artist too. For the parentless kinds each
  entry becomes a normal media item over the same Remote Stream version, with its
  metadata from its own NFO. Switching between presentations detaches the
  previous one, reverting detaches the projection instead of deleting the
  provider items, and a revert is refused while a projected item is still
  referenced by a collection, another playlist, or a playout.

- The add-on list editor groups entries by availability into counted
  **non-playable**, **unknown** and **playable** sections, each paged on its own
  (10/25/50/100/all) and each offering local replacement. Non-playable is
  expanded by default; unknown is kept separate because "could not determine"
  is not the same as "cannot play".

- Add-on packages can declare `default_source_hosts` (manifest schema v4): the
  hosts they offer to process, as a statement separate from the hosts they are
  permitted to contact. ErsatzRS uses it to select an add-on for an
  operator-supplied source URL automatically, preferring the most specific claim
  and asking the operator when two packages claim a host equally well.

- **Settings > Add-ons > Sources** gained a **Default remote stream processor**.
  It supplies the add-on for HTTP and HTTPS source URLs that no installed
  package claims, and Health Checks now report a configured default that is
  missing, disabled, or unable to play remote streams. Remote Stream definitions
  without `addon:` are still played directly by FFmpeg; the default does not
  change playback.

- The add-on of a saved add-on list can now be changed from the list editor.
  ErsatzRS enumerates through the newly selected package first, refuses a move
  that would collide with an existing list, and confirms with the operator that
  item metadata, replacements, and chapter corrections are derived again.

- Streaming add-ons can now supply bounded chapter markers through
  `remote-stream.item.v2`. ErsatzRS keeps provider and operator text separate,
  gives manual corrections precedence, offers **Restore provider chapters**,
  and derives stable provider-neutral fragment items from the existing
  enrichment call instead of invoking the add-on once per chaptered item.

- Health Checks now report an enabled add-on whose most recent readiness check
  did not succeed. The reported readiness state is persisted alongside its
  diagnostic code, so an add-on that stopped being usable after it was enabled —
  for example because a required external tool was removed — is reported instead
  of appearing healthy until playback fails.

- Add-on media lists can now be presented as a normal television show with
  Show/Season/Episode identities backed by the existing Remote Stream playback
  versions. Every list item has a dedicated metadata and chapter editor;
  descriptive edits are written to optimistic-concurrency-protected Kodi NFO
  files and then projected into the database. The list-wide chapters control
  has been removed, externally edited NFO files are preserved as user-owned,
  and failed television projections are reported by health diagnostics.

- The official Beeld & Geluid integration now recognizes personal lists,
  series and programme pages, individual episodes, and filtered Schatkamer
  searches through one normalized source grammar. It traverses every result
  page without a fixed page ceiling, preserves provider order, deduplicates by
  stable episode ID, imports editorial introductions and artwork, and reports
  incomplete synchronization clearly. Individual episodes accept validated
  YouTube-style chapter markers: the full episode remains available while each
  chapter becomes a structured, independently selectable Remote Stream fragment
  with exact `start` and optional `end` bounds. The new v3 add-on list contracts
  carry fragment parentage, duration, and additional artwork without changing
  the frozen v1/v2 wire shapes.
- Storage-backed add-on lists can now use v2 availability and content-kind
  contracts. Their rich metadata is written to deterministic Kodi-style `.nfo`
  sidecars beside operational Remote Stream YAML, and the scanner applies the
  same sidecar precedence as other Local media. Add-on Lists offers a per-link
  television/movie/music-video/other-video override and lets an explicitly
  unavailable provider item pin one exact Local media version as its fallback;
  unknown availability never triggers it, and provider recovery wins
  automatically. Interrupted YAML/NFO publication is detected through managed
  hashes and healed, while a provider item omitted from a later snapshot is
  retained as an unavailable tombstone so its replacement intent survives.
  An invalidated Local target is now shown explicitly as an invalid
  replacement in Add-on Lists instead of looking like an unconfigured item.
  Remote Stream cards and XMLTV now expose the descriptive content kind without
  changing Remote Stream playback identity. Provider classification rules,
  localized package resources, icons, and fixtures remain owned by each add-on
  rather than the host.
- Importing an add-on list now takes an optional storage folder name next to
  the URL. Leaving it empty keeps the current behaviour — the folder is derived
  from the list name the provider reports — while a supplied name is sanitized
  the same way and refused when another list of the same add-on already owns
  it. The check runs before the provider is invoked, so a rejected name costs
  no provider call and leaves no half-imported list behind.

### Changed

- **Settings > Add-ons > Sources** was rebuilt. Four blocks of equal weight —
  a risk toggle, a routing preference, an unlabelled upload row, and the
  repository list — became three labelled sections: **Unknown sources**,
  **Add-on repositories**, and **Install from file**. The repository list is now
  a table in the same house style as the media-source pages, and it finally
  shows what an operator opens this page for: how many add-ons each source
  offers and when its catalogue was last refreshed (both were already loaded and
  discarded). Actions that cannot run explain themselves on hover instead of
  being silently greyed out, the add/edit form moved into a dialog that names
  which flow you are in and keeps an existing repository's ID read-only, the
  delete confirmation names the repository instead of its ID, the file pickers
  show the chosen file in the UI language rather than the browser's own chrome,
  and the unknown-sources button is red only for the click that accepts risk.
  The **Default remote stream processor** moved to the **Installed add-ons**
  tab, with the add-ons it chooses between.

- Updating an add-on no longer takes it out of service. A package update landed
  disabled with its readiness result cleared, so a working add-on stopped
  serving until an operator noticed and ran **Check** and **Enable** by hand.
  The update now runs that readiness check itself and returns the add-on to
  service when it passes. It stays disabled, as before, when the check fails —
  and when the new version declares a network host, external command, media read
  access, or managed FFmpeg access the previous version did not, because
  granting a package new privileges stays the operator's decision. First
  installs still start disabled. See
  [ADR 0074](docs/adr/0074-keep-updated-add-ons-in-service.md).

- The Settings > Add-ons page dropped its four-card summary band. It spent the
  top of the viewport restating what the tab strip already names, so the counts
  moved into the tab titles: `Installed add-ons (2/3)` reads as enabled over
  installed, and `Updates` and `Sources` carry their own totals.

- The add-on list editor leads with the source URL and the add-on, followed by
  the fields the resolved add-on populates.

- Adding an add-on list now starts with the URL. The add-on selection moved
  below it and became optional: leave it on *Determine from the URL* and
  ErsatzRS fills it in from the URL host, visibly and overridably. The plain
  HTML form path resolves the same way and reports an unroutable URL with its
  own message instead of a generic failure.

- `plans/` now holds only work that still needs doing. Sixteen fully
  implemented plans were pruned once their substance lived in
  `docs/progress/`, and the two remaining blockers of the add-on NFO closeout
  plan are recorded as open parity debts instead of only in a plan header. A
  milestone that is still open must name a live plan; a completed milestone may
  omit the pointer, and `cargo xtask progress check` enforces both halves.
  Contributor-facing TSDuck install steps moved from the pruned MPEG-TS plan to
  [the troubleshooting how-to](docs/how-to/troubleshoot-startup-and-playback.md).

- The official yt-dlp add-on now requires an operator-installed JavaScript
  runtime (deno) alongside yt-dlp, and reports its absence as unavailable rather
  than failing later during playback. Without one, some providers return a media
  URL bound to a restricted player client that the managed FFmpeg runtime is
  refused when it fetches that URL. The add-on also prefers adaptive manifests
  over progressive renditions for the same reason. Both programs must be on the
  `PATH` of the account that runs ErsatzRS, which is not necessarily the same
  environment as an interactive login shell.

- Add-on package manifests, repository-package manifests, and generated
  repository catalogs can now be checked through one shared Rust validation
  path exposed by `cargo xtask validate-addon-manifest` and the thin
  `ersatzrs-addon-validator` binary. Context-independent catalog validation now
  lives in `ersatzrs-addon-contract`, while repository identity, rollback,
  current-time expiry, and signature checks remain host trust concerns.
- The official add-on repository now constructs deterministic packages, icons,
  and unsigned catalogs with a repository-owned Rust publisher. It invokes the
  pinned host validator instead of duplicating manifest rules, publishes from
  a rollback-safe staging tree, and keeps detached signing and deployment as
  separate downstream CI steps.
- Add-on media-list and Remote Stream list output now share one strict parsing
  policy: unknown fields reject the complete enumeration with a bounded error
  naming the field, while omitted and `null` optional arrays both mean absent.
  Existing capability versions are frozen; new fields require a capability
  version bump instead of speculative emission.
- The independently versioned `ersatzrs-addon-contract` crate now owns the
  media-list and Remote Stream NDJSON records, their wire enums, validation,
  and reusable stdout conformance checks. A runnable reference add-on ships as
  a Cargo example. Registry publication is disabled and guarded; the crate
  remains private to the ErsatzRS GitHub repository.
- Failed add-on `list` and `play` operations now carry a stable, bounded JSON
  diagnostic code on stderr. Host-detected missing settings and secrets use the
  same vocabulary, the admin UI localizes recognized codes in every supported
  culture, and unknown third-party codes retain a generic localized fallback.
  Provider-specific error prose and setting keys no longer live in the host;
  environment secrets are redacted and file-secret diagnostics stay suppressed.
- Availability vocabulary is now owned by each add-on package. The official
  yt-dlp package normalizes provider results to `available`, `unavailable`, or
  `unknown` on POSIX and Windows; the host rejects every other explicit value
  atomically with a bounded provider-neutral error instead of silently
  coercing it. Architecture lint prevents provider tokens from returning to
  host crates.

### Removed

- The official Trakt add-on and its native cross-platform build matrix have
  been retired. The public add-ons repository no longer needs credentials for
  the private ErsatzRS repository and publishes only Beeld & Geluid and yt-dlp.
  Provider-neutral Add-on Lists remain available, but Trakt list parity is
  deliberately dropped and the legacy built-in Trakt implementation is not
  restored.

### Fixed

- Installing an add-on failed on Windows with "installed package path escaped
  its root". The package content hash walked the staged package from its plain
  `C:\...` path but re-resolved every file with `std::fs::canonicalize`, which
  returns a `\\?\`-prefixed verbatim path on Windows, so each file looked like
  an escape from its own package root and no add-on could be installed. The
  add-on package paths now resolve through `dunce`, which keeps the ordinary
  path form Windows and `cmd.exe` batch entrypoints accept, and the hasher
  canonicalizes its root the same way it canonicalizes the files under it. The
  same defect made every install fail on macOS, where `/var` resolves to
  `/private/var`. The add-on media storage root also no longer persists a
  verbatim path. `dunce` keeps the compatible form only where Windows accepts
  it and falls back to the verbatim form per path, so every containment check
  now compares raw canonical paths, which cannot disagree on form, and only the
  paths handed to a child process stay on `dunce`. A rejected path is logged
  with the path and root that failed the check, because one message covered
  three different rejections and none of them said which path was at fault.
- `./run.sh test-closeout` ran only the `--all-features` configuration, which
  enables `mysql` and compiles away every `#![cfg(not(feature = "mysql"))]`
  integration test. That pass was not a stricter superset but a smaller set:
  322 tests never ran, and two had been failing on a fixture that seeded a
  `MediaStreamKind` of `0` where the enum is 1-based, leaving the generated
  playlist empty. The closeout now runs both configurations.
- The per-item add-on media list metadata editor is now covered by the admin
  route contract. Its route shipped with the television projection without a
  hydration entry, so the page had no recorded browser evidence and the route
  contract check failed; the entry is recorded as outstanding rather than
  claimed, and browser proof for the editor is still to come.
- MySQL databases can be migrated past the add-on television projection again.
  Its `AddonMediaListItemEpisode` foreign key was named with 68 characters,
  four over MySQL's 64-character identifier limit, so the migration aborted and
  every MySQL install stopped at that point. The name now carries the same
  EF-style truncation the rest of the MySQL schema uses; the SQLite spelling is
  unchanged.
- Choosing a Local replacement for an add-on list item now finds candidates.
  The search required a video stream recorded with a kind the scanner never
  writes, so it matched nothing in any library and the feature could not be
  used at all.
- Presenting an add-on list as a television show no longer fails with a
  database lock. The projection read before it wrote inside a deferred
  transaction, which SQLite refuses to upgrade — immediately, and regardless of
  the configured busy timeout — whenever another connection has written in the
  meantime. Renaming a projected show failed for the same reason.
- Every other write path is now protected against the same failure. All 98
  writing transactions across the repositories, the scanners, and the Plex,
  Jellyfin, and Emby persistence layers now take the write lock up front, so
  concurrent writers queue for the configured busy timeout instead of failing
  outright. This removes a class of intermittent `database is locked` errors
  that could surface anywhere in the admin UI while a library scan or another
  editor was writing. Genuinely read-only transactions deliberately keep the
  concurrent deferred begin, and a new `cargo xtask architecture-lint` rule
  rejects a newly added transaction that writes without it.
- Seasons projected from an add-on list are now listed on the show page.
  They were created without the metadata row every other season source writes,
  and the page requires it, so a projected show appeared to have no seasons or
  episodes at all. Existing projections repair themselves on the next
  synchronization.
- The Local replacement picker is no longer laid out as a bare inline block.
  It is now a dialog that names the item being replaced, sizes itself to the
  viewport instead of a fixed form width, keeps its search box and buttons
  reachable while only the result list scrolls, and reports an empty result
  instead of showing nothing. Its row actions match the icon buttons used
  elsewhere in the table.
- Synchronizing a Remote Stream playlist now refreshes its panel. **Sync Now**,
  and removing an entry (which triggers a synchronization), previously left the
  last-synchronized time and item count stale until the page was reloaded.
- Failed add-on media list operations are now recorded in the log. Importing,
  matching, updating, or deleting a list previously surfaced only a generic HTTP
  failure line, so the bounded provider diagnostic naming the offending record
  never reached **Support > Logs**.
- Finite add-on streams now keep one HLS Segmenter producer alive for the
  complete play item. A bounded startup burst fills the normal work-ahead
  buffer before the producer continues at realtime speed, preventing
  keyframe-aligned source seeking from replaying video at the former 44-second
  process boundary. The TSDuck URL gate now also recognizes public HLS master
  playlists instead of falsely requiring media segments in the top-level
  variant list.
- Channels configured for Transport Stream Hybrid, and explicit `mode=ts`
  requests, now dispatch the modern HLS segmenter wrapper promised by the
  streaming parity contract instead of silently falling through to the legacy
  concat branch. This prevents the wrong pipeline from terminating at Remote
  Stream programme boundaries.
- Beeld & Geluid chapter input now requires the matching v3 list capability,
  preventing older add-on packages from exposing chapter titles that play the
  complete episode. Remote Stream playlist scans validate that every selected
  fragment retains its full episode and link all chapter parents only after
  the complete source-ordered batch has been materialized.
- GitHub-hosted Playwright fallback runs now reuse the FFmpeg 9 runtime that
  server startup acquires for their calibration-media fixtures, instead of
  requiring an unrelated `ffmpeg` executable on the runner's `PATH`.
- Cargo package metadata now points to the canonical ErsatzRS repository and
  declares the same Zlib license as the repository `LICENSE`, so packaged
  add-on contract metadata carries accurate source and licensing links.
- Operator-supplied Add-on List and Remote Stream playlist URLs are now
  restricted to absolute HTTP(S) URLs without embedded credentials. Before a
  provider call, the host also enforces the installed add-on's declared exact,
  suffix, or wildcard `network_hosts`; rejected saved lists remain visible
  with a localized failure instead of disappearing from administration. The
  add-on receives the same canonical URL serialization used for validation,
  preventing authority parser differences from bypassing the host restriction.
- Add-on list failures reached the operator wrapped in transport plumbing —
  "error running server function: conflict: …" — because the localized
  messages match the repository wording and nothing stripped the server
  function's own prefix, so every message dispatched through a server function
  fell through to the raw string. The prefix is now removed before matching,
  which also lands the storage-folder rejections (taken, or unusable as a
  folder name) as ordinary sentences in every supported culture.
- No add-on that declares managed FFmpeg could serve as a media-list provider.
  Resolving an add-on's environment requires both managed paths whenever its
  manifest declares `managed_ffmpeg`, but list enumeration was the one
  invocation path that passed an empty runtime context — so adding a Beeld &
  Geluid list failed with "managed FFmpeg path is unavailable" before a single
  request left the machine. List invocations now supply the managed FFmpeg and
  ffprobe paths, the way every other invocation path already did.
- Local Libraries rendered the storage root each add-on manages as a loose
  block of text above the library table, with its own Settings link, so
  add-on-owned storage read as something other than a media source. Those
  roots are now ordinary rows in the table, sorted in with the libraries and
  labelled "Add-on" as their media kind. Their edit action opens the owning
  add-on's settings page rather than the library editor, and their delete
  action stays visible but disabled, because the add-on — not the operator —
  owns the registration; add-on-managed list libraries now show the same
  disabled delete instead of no delete action at all.
- The Add-ons Updates view reported "No add-ons match these filters." when
  nothing was filtered at all, which reads like the view is broken rather than
  like good news. With no search term and no state filter it now says
  everything is up to date; with either applied it still reports the filter
  miss, because a filter can hide a pending update.
- Two rendering defects in the Settings > Add-ons manager. An "Add-on
  repositories" caption sat in the page header, where it labelled all four
  views instead of the repository list it describes; it now heads that list in
  the Sources view. And on an add-on's detail page the link to its public
  source-code URL had no styling of its own, so it inherited body text colour
  and read as a label rather than a link — directly below a "Source" fact
  naming the repository the add-on came from, leaving two different things
  wearing the same word. The link is now styled as one and reads "View source
  code", translated in every supported culture.
- Connecting to MySQL 8 failed outright with "RSA auth backend disabled".
  MySQL 8 defaults to the `caching_sha2_password` plugin, whose first
  password exchange on a connection needs either TLS or an RSA public-key
  exchange, and sqlx 0.9 moved its RSA backend behind an opt-in feature that
  the `mysql` build did not enable. The feature is now enabled, so a plaintext
  MySQL 8 connection authenticates instead of being refused at startup.
  MariaDB and MySQL 5.7, which default to `mysql_native_password`, were
  unaffected.
- Saving almost anything could silently delete related data. Repository writes
  used SQL `REPLACE INTO`, which is not an update: on both SQLite and MySQL it
  deletes the conflicting row before re-inserting it, and that delete fires
  `ON DELETE CASCADE`. Seventeen statements wrote to a table other rows cascade
  off, so editing an FFmpeg profile destroyed every channel using it (and their
  playouts), saving a channel destroyed its playout and artwork, and — worst —
  every library rescan erased the child rows of each media item it touched,
  because 21 tables cascade off `MediaItem`. All seventeen now use a real
  upsert, which never deletes the row. Statements still using `REPLACE INTO`
  write only to tables nothing cascades off.
- A Plex rescan deleted and recreated a media file row whenever its path hash
  already existed, taking the linked Plex file record with it. The write now
  updates the existing row in place.
- Saving a library discarded every item scanned into it and blanked its stored
  paths. The save deleted all of the library's path rows before re-inserting
  them with an empty path, and media items cascade off those rows. Library save
  now reconciles paths instead: a path that stays keeps its row, its stored
  path, and its scanned items, while a path genuinely removed from the library
  is still deleted along with its items.
- A new database migration did not run until the crate was rebuilt for some
  other reason. `sqlx::migrate!` embeds migrations at compile time but cannot
  register the directory as a build input, so adding a `.sql` file left the
  cached build — and its stale migration set — in place, and restarting the
  server changed nothing. A build script now tracks both migration directories.
- The official add-on repository could never refresh, reporting "the repository
  has no Ed25519 public key". Its production signing key is now pinned, so a
  fresh database verifies the signed catalog instead of failing every refresh.
- Four Italian admin UI error branches never fired, because the English text
  they match against had itself been translated. Overlapping block and deco
  reports, the pinned-stream-selection error, and the movie-not-found error
  now match the message the backend actually emits and render in Italian.

### Added

- Schema-v3 add-ons can now declare file-backed media-list storage. Enabling
  Beeld & Geluid or yt-dlp creates its profile-relative media root and managed
  local source; each imported list becomes a separately named Remote Streams
  library backed by the existing safe playlist synchronizer. Add-on Lists now
  appears under Media Sources immediately below Local, and configured storage
  paths remain portable across Linux, Windows, and macOS.
- Repository-installed add-ons can now expose bounded provider lists through
  `media-list.list.v1`. The new `/media/addon-lists` manager imports and
  refreshes those links, matches movies/shows/seasons/episodes to local media,
  supports `addon_list:<id>` search, and can maintain a playlist in provider
  order. The official Trakt add-on owns Trakt API access and configures its
  client ID only through an environment/file secret reference; Beeld & Geluid
  supports the same workflow for Schatkamer series, shared-list, and saved
  `/zoeken?...` links.

- The Beeld & Geluid streaming add-on can now synchronize public, user-made
  Schatkamer `/lijst/<uuid>` links as ordered Remote Stream playlists alongside
  series links. Duplicate programmes keep their first position, unavailable
  programmes are skipped with a bounded diagnostic, and the standalone POSIX
  and Windows implementations preserve the existing rich metadata projection.
- German for Germany (`de-de`) is now a complete, selectable admin UI language
  (ADR 0022). Every admin text key and every stable workflow error message is
  translated, terminology is unified on one German term per ErsatzTV concept
  (Sendeplan, Sendeplanelement, Playout, Block, Deko, Filler, Sammlung,
  Element), and product nouns, provider/codec/protocol identifiers, routes, and
  configuration keys stay verbatim. The settings-route browser case passes in
  Chromium against a live host with no English fallback leakage.
- Italian for Italy (`it-it`) is now a complete, selectable admin UI language
  (ADR 0025). Terminology is unified on one Italian term per ErsatzTV concept —
  Playout keeps the product noun so it no longer collides with Playback
  ("riproduzione"), Collection settles on "raccolta", Playlist on "playlist",
  and Watermark on "filigrana" — the remaining English passthrough messages are
  translated, and the settings-route browser case passes in Chromium against a
  live host with no English fallback leakage.
- Spanish for Latin America (`es-419`) and Spain (`es-es`) are now complete,
  selectable admin UI languages (ADRs 0028 and 0029). Playout keeps the product
  noun so it no longer collides with Playback ("reproducción"), the remaining
  English passthrough messages are translated, episode subtitles read
  "temporada X, episodio Y", and both settings-route browser cases pass in
  Chromium against a live host with no English fallback leakage. The regional
  split between the two ("vídeo"/"video", "ajustes"/"configuración") is
  preserved.
- A signed, repository-backed add-on platform now provides trusted-source
  replacement, immutable package install/update, manifest-rendered permissions,
  settings and panels, readiness checks, content-integrity health, automatic
  catalog refresh, dependency guards, and localized management under
  **Settings > Add-ons**. The separate official-repository source builds
  independent Beeld & Geluid and yt-dlp Remote Stream packages for all six
  release RIDs; publishing it and pinning its production public key remain
  release work.
- Remote Stream playlist manifests can now enumerate an upstream series through
  an installed add-on, synchronize stable generated definitions, report health
  and bounded troubleshooting, expose localized management and tag facets, and
  optionally maintain a read-only playlist in upstream order with affected
  playout refresh.
- The FFmpeg profile editor now offers the full encoder preset ladders FFmpeg 9
  accepts instead of the single `veryfast` value inherited from ErsatzTV:
  `ultrafast` through `veryslow` for software `libx264`/`libx265`, and the
  complete `veryfast` through `veryslow` target-usage ladder for QSV.
  `placebo` is deliberately not offered, since it cannot sustain realtime
  encoding at any resolution.
- NVENC profiles for HEVC and AV1 can now select FFmpeg 9's `uhq` ("ultra high
  quality") tune. H.264 NVENC does not advertise it, so the option is gated by
  codec.
- Dolby Vision profile 7 sources with a complete BL/EL/RPU record can now
  transcode from their HDR10-compatible base layer to SDR or HDR10 MPEG-TS and
  HLS output. The production path directly software-decodes only the exact
  characterized HEVC Main 10/yuv420p10le, compatibility-id-6 tuple;
  `dovi_split` remains a fixture oracle rather than an optional runtime
  dependency. Dolby Vision signalling is stripped, and incomplete or
  imprecisely identified sources remain rejected. Existing items whose
  base-layer state predates the additive SQLite/MySQL field require a rescan.
- Dolby Vision Profile 5 can now convert through two exact, capability-gated
  NVIDIA operations: SDR/H.264 and HDR10/HEVC. Both use the selected CUDA
  device, RPU-aware Vulkan/libplacebo processing, a software colour-contract
  conversion, and NVENC. ErsatzRS selects the path only after a positive
  functional proof for the current runtime identity; missing proof or a
  learned live failure rejects the source instead of spawning a guessed-colour
  fallback.
- Jellyfin, Emby, and Plex scans now retain rotation, mastering-display,
  MaxCLL/MaxFALL, and Dolby Vision configuration fields. Complete provider
  tuples round-trip to playback, while positive partial metadata is persisted
  as explicitly incomplete with a provider hint so rescans and restarts cannot
  silently downgrade known Dolby Vision to ordinary HEVC/PQ.

### Changed

- Trakt lists are no longer a built-in ErsatzRS feature (ADR 0067). Upgrading
  deliberately deletes legacy Trakt list/item/GUID rows, their generated
  system playlists and page-size preference, then drops all three Trakt tables;
  no existing Trakt data is imported into the provider-neutral add-on schema.

- The tag facets on the remote-streams browser no longer push the card grid off
  the screen. On a tag-heavy library the wrapped chip cloud grew without limit
  and filled most of the viewport before a single stream was visible. Tags now
  render through a shared facet rail: one fixed-height row of chips that
  scrolls sideways, with a CSS-only disclosure that expands the same chips into
  a height-capped cloud. The rail also shows how many tags exist, marks the tag
  currently filtered on, and offers an "All" chip to clear the filter — which
  previously required editing the URL. No hydration is involved, so it behaves
  identically in the `<noscript>` fallback. Facet links on the media detail
  pages (tags, genres, studios, languages, people) now share the same chip
  palette and focus ring, and pick up the light theme instead of a hard-coded
  dark-theme green.

- Add-on trust is now tiered rather than all-or-nothing (ADR 0066). Verified
  repositories keep detached Ed25519 catalog signatures; unverified HTTPS
  repositories and sideloaded add-on or repository ZIPs require one
  off-by-default Unknown Sources opt-in. The permanent trust tier is visible in
  the manager and health report, while the official source still fails closed
  without its pinned key. The localized Settings > Add-ons manager now provides
  Installed, Browse, Updates, and Sources views plus a dedicated detail page for
  every add-on, guarded lifecycle actions, source offers, permissions,
  configuration, readiness, and optional bounded package artwork. Updates and
  source ownership remain manual by design.
- Documentation now has explicit Diátaxis navigation, a complete reference
  index, evidence-backed current System Context and Container views, and
  separate add-on package, repository, runtime, trust, and operator guidance.
  Add-on pages distinguish the implemented development host from the still
  unpublished official repository and production key. Goal-oriented how-to
  guides now also cover building and refreshing a Classic playout, verifying
  local and Remote Stream ingestion, scheduling content variants, managing an
  add-on lifecycle, building manual collections from media results, keeping
  channel content updated with smart collections, and confirming IPTV
  client imports. Release how-to guides now separate normal multi-platform
  tags from Windows-only beta tags and use unmistakable version placeholders.
  Backup and troubleshooting procedures now also cover external databases and
  state, the documented systemd service home, IPTV JWT probes, and redacted
  diagnostic downloads; Jellyfin guidance reflects its Movies/TV-only,
  one-way synchronization contract.
- Remote Stream YAML now uses `addon:` identities for provider playback and
  playlist enumeration. The legacy raw `script:`, `extractor:`, and
  `settings.remote_stream_extractors` contracts are intentionally rejected so
  media files cannot bypass repository trust with arbitrary commands.
- The remaining FFmpeg multi-vendor hardware and Dolby Vision fixture/client
  evidence gaps are explicitly deferred beyond Phase 8 and no longer block
  Phase 9; their unresolved acceptance evidence remains tracked for later
  opportunistic closure.
- CI now executes the Output Dynamic Range health classification against an
  isolated MySQL 8 service instead of allowing that backend regression to skip
  when no external test URL is configured.
- Release guidance now distinguishes Windows-only beta artifacts from the
  normal Linux/Windows/GHCR release matrix, and the remaining pre-publication
  Windows candidate, workflow-hardening, and live-acceptance work is tracked in
  one redacted plan instead of an uncommitted beta test diary.
- FFmpeg settings now publish as one validated runtime generation. ErsatzRS
  stages binaries and graph probes before one transactional settings/cache
  commit, then atomically swaps the shared capability report and paired
  FFmpeg/ffprobe paths; a failed candidate leaves the previous runtime and side
  effects untouched. Cache reuse and learned denials are bound to both binary
  digests plus the selected CUDA device UUID/model and driver, and late child
  failures from an older generation cannot poison its replacement.
- NVIDIA native capability probing and generated CUDA/Vulkan graphs now bind to
  device index zero instead of unioning encoder/decoder support across all
  installed GPUs. Dolby Vision admission is also stricter: only characterized
  Profile 8.1 and Profile 7 tuples are accepted, and Dolby Vision stream copy
  remains rejected until a source/container/output/client matrix is proven.
- The FFmpeg profiles seeded into a **new** database are retuned for FFmpeg 9.
  NVENC seeds now use preset `p5` with tune `hq` instead of `p7` with `ll`, and
  QSV seeds use `medium` (target usage 4) instead of `veryfast` (target usage
  7). The old NVENC pair came from the legacy `llhq` compatibility mapping,
  which pairs the slowest preset with a low-latency tune that a 4-second-segment
  HLS pipeline never benefits from. Software seeds keep `veryfast`. Existing
  databases are not modified, and profiles migrated from `llhp`/`llhq` still
  keep their exact original values.

### Fixed

- Signed add-on catalog refresh now uses verified `ETag`/`Last-Modified`
  revalidation and compare-and-swap trust/catalog revisions, preventing stale
  refreshes, installs, configuration changes, or readiness checks from
  overwriting a newer repository or installation state.
- Add-on execution now gates managed FFmpeg paths on the declared permission,
  owns and terminates managed Unix process groups or Windows Job Objects,
  bounds probe output and drain time, strips unsafe Unix package write modes,
  redacts or suppresses secret-bearing diagnostics, and isolates retained data
  by repository ID.

- Beeld & Geluid playlist-generated episode definitions now retain plot,
  original broadcast timestamp, Kijkwijzer advice, genre and subject labels,
  collection, role-aware people, producer, original broadcasters, and current
  broadcasters. Remote Stream scans persist and index those facets, while
  XMLTV/Kodi output now includes the compatible date, categories, rating, and
  credits instead of dropping the metadata after enumeration.

- Channel mirror source and playout offset controls are now available only
  when the editor's Playout Source is Mirror; server-side normalization still
  discards forged mirror-only values from Generated channels.
- A runtime hardware-encoder demotion no longer reuses the hardware profile's
  encoder preset for the software encode. The QSV and `libx264` ladders share
  several names (`medium`, `slow`, `veryfast`) while meaning entirely different
  cost points, so a demoted QSV profile could hand `libx264` a far more
  expensive preset than the hardware encoder was using and lose realtime.
- Remote Stream producer failures now include the playback surface, channel,
  producer name, effective seek, failure class, exit status, stderr byte count,
  and a bounded diagnostic tail. URLs, credentials, cookies, authorization
  values, and local paths are sanitized before logging, so add-on failures
  are actionable without exposing signed media URLs or private configuration.

## [1.0.21-beta.1] - 2026-08-09

### Fixed

- Runtime-learned hardware graph and decode denials now merge through one
  mandatory process-wide FFmpeg report before the complete snapshot is
  persisted. Long-lived HLS and preview workers read that shared report before
  every FFmpeg command build, and immediate IPTV retries receive the complete
  merged denial set, so concurrent workers neither erase restart state nor
  carry another worker's already-rejected graph into a later spawn or retry.
- Finite Remote Stream producers now treat a bounded consumer closing its input
  as normal debug-level teardown while preserving warnings for producer
  failures and unrelated pipe errors.
- The optional Windows YouTube wrapper keeps successful `yt-dlp` extraction
  quiet while retaining source-side `{seek}` through the managed FFmpeg
  runtime, preventing temporary signed media URLs from reaching normal console
  or captured-log output.

## [1.0.21-beta] - 2026-08-08

### Added

- Added a **Source Orientation** health check. Rotated video — most phone
  recordings — carries its orientation as a container display matrix, and
  MPEG-TS has no field for one, so a channel that *copies* video delivers
  portrait content sideways. The check names the affected items and channels and
  points at the two real fixes: transcode the channel, or use the fMP4 HLS
  variant. Transcoding channels are unaffected, because FFmpeg autorotates and
  the rotation ends up in the pixels. ErsatzRS reports rather than silently
  forcing a transcode, since copy is an explicit choice with a large CPU cost
  attached. The measured behavior of every output format is documented in the
  troubleshooting guide.

- FFmpeg profiles set for HDR10 output can now carry their own **mastering
  display** and **MaxCLL/MaxFALL** values (ADR 0064). ErsatzRS has always had to
  signal something when an HDR10 encode has no source metadata to carry
  forward — converting HLG, or mapping SDR in — and until now that was a fixed
  documented constant. Leaving the fields blank keeps that constant, which the
  editor shows as the placeholder so the effective value is visible rather than
  implied. The override never displaces metadata a source carried itself: a
  graded source still wins, because it describes the display the content was
  actually mastered on. Values are validated at save time, so a typo is refused
  instead of failing every stream later, and the Output Dynamic Range health
  check reports profiles whose stored values are being ignored.

- Added an **Output Dynamic Range** choice to FFmpeg profiles (ADR 0064), so
  SDR and HDR10 output is now an explicit operator decision instead of being
  inferred from a profile name, codec, or bit depth. `SDR (BT.709)` keeps the
  existing behavior of tone-mapping HDR sources to BT.709. `HDR10 (BT.2020 PQ)`
  produces a stable BT.2020/PQ stream: PQ sources are passed through with their
  own mastering-display and MaxCLL/MaxFALL metadata intact, HLG sources are
  normalized to PQ, and SDR sources are mapped in at ITU-R BT.2408 diffuse white
  (203 cd/m²) without inventing highlight detail. The signalling is identical
  for every item, so a schedule that mixes SDR and HDR content does not change
  the stream's colour signalling at a programme boundary.

  Existing profiles are migrated to SDR and keep streaming exactly what they
  streamed before; only a freshly created database seeds its two `hdr` profiles
  as HDR10. HDR10 requires hevc or av1 at 10-bit and is rejected at save time
  rather than silently downgraded, a new **Output Dynamic Range** health check
  reports profiles that cannot produce what they promise, and the control is
  localized in every supported culture. Profiles that copy video are unaffected
  and say so.

  NVENC produces HDR10 too, keeping a PQ source entirely on the GPU. Backends
  without a characterized HDR10 graph — QSV, VAAPI, AMF, VideoToolbox, RKMPP,
  and V4L2M2M — encode HDR10 profiles in software instead of quietly emitting
  SDR, and the health check warns when that happens.

  Scanning now records each source's mastering-display and MaxCLL/MaxFALL
  values, so an HLG or SDR item converted to HDR10 carries the metadata its own
  grade implies rather than a generic default.

- Added uniform content-rating/Kijkwijzer, studio, music-label, language, genre, and
  tag metadata for movies, television, music videos, other videos, songs, and
  images across local/Plex scanning, SQLite/MySQL persistence, search and smart
  collections, Smart Shuffle grouping, and localized detail pages. Music
  videos, songs, and images now have read-only detail routes with linked facet
  searches and the established media actions.

- Added an opt-in **Settings** -> **UI** control that starts the packaged
  `ErsatzRS.exe` tray launcher when the current Windows user signs in. The UI
  reads the live per-user registry state, can repair a stale release path, and
  reports a missing launcher or inaccessible registration without persisting a
  misleading database flag. Linux remains service-manager owned and now has a
  systemd installation guide modeled on Jellyfin's package behavior.

- Added Dolby Vision input support. Profile 8.1 sources — the common case for
  streaming-service 4K releases — now play from their HDR10-compatible base
  layer to both SDR and HDR10 channels, verified against a real file. Profile 5
  and dual-layer profile 7 are rejected with a health entry naming the reason
  rather than played with visibly wrong colour, because neither has a
  characterized conversion. Scanning records each source's Dolby Vision
  configuration. Dolby Vision output is not supported.

### Changed

- Re-sequenced the long-term roadmap: FFmpeg 9 HDR/Dolby Vision, Vulkan, and
  pipeline hardening is Phase 8; teletext capability, persistence, delivery,
  management, and runtime proof is Phase 9; and the "The Box" video-jukebox
  extension moves to Phase 10. This changes planning contracts only, not
  current runtime behavior.

- Defined the future FFmpeg profile dynamic-range contract: transcoding will
  explicitly target stable BT.709 SDR or BT.2020/PQ HDR10 instead of inferring
  HDR from HEVC Main 10 or a profile name. The accepted plan also establishes
  source-derived HDR metadata, fixture-gated Dolby Vision input handling,
  operation-scoped Vulkan/libplacebo fallback, safe long-filtergraph transport,
  and benchmark gates for AMF, VAAPI, libplacebo, and possible Vulkan encoding.
  This is an architecture and delivery contract; the runtime implementation is
  still tracked as not started/partial in `docs/progress/`.

- FFmpeg 9.x is now the explicit runtime contract. Managed startup defaults to
  `ErsatzRS-ffmpeg` v2, rejects FFmpeg 8 and unverified future majors, and keeps
  downloaded paths updateable while preserving administrator-owned overrides.
  NVENC profiles now store and edit presets (`p1`-`p7`) separately from tunes
  (`hq`, `ll`, `ull`, `lossless`), with automatic `llhp`/`llhq` migration. The
  migration is verified to be bit-exact: a profile that used `llhq` or `llhp`
  before the upgrade produces pixel-identical output afterwards.
  One upstream behavior change comes with FFmpeg 9 and is intentionally not
  overridden: `h264_nvenc` now defaults to the **High** profile instead of
  **Main**. This affects only NVENC H.264 profiles that leave the video profile
  unset; set it explicitly on the FFmpeg profile if a client device requires
  Main.

- The nightly Clippy-exemption cleanup now batches individually verified fixes
  behind one push and self-tunes its batch cap from durable run trailers. Its
  fixed 40-minute budget now terminates each verification command using the
  remaining time, rolls back a timed-out target, and records all-skipped or
  pre-success time-box outcomes in an empty state commit so later runs reduce
  the cap instead of retrying the same workload unchanged.

### Removed

- Removed the unused `.empty-cell` CSS rule from the rerun collections admin
  page's module stylesheet (dead-code cleanup, no visible change).
- Removed the unused `.empty` CSS rule from the blocks admin page's module
  stylesheet (dead-code cleanup, no visible change).

### Fixed

- Fixed hardware-graph fallback state and unsupported Dolby Vision handling in
  live streams. A graph probe that ran but produced no classifiable result now
  remains non-selectable instead of being mistaken for an older, never-probed
  runtime; live failures are learned only from operation-specific evidence and
  are published to future HLS and IPTV sessions immediately. Hardware-decode
  failures now use the same process-wide publication path. Concurrent HLS and
  IPTV failures persist merged graph and decode denial snapshots, so one worker
  cannot erase another worker's learned fallback on restart. The one-retry
  limit no longer suppresses learning a failed CUDA fallback. Unsupported
  Dolby Vision HLS items now emit an error slate and advance instead of looping
  on the same item, and early HLS/IPTV refusal removes any rendered graphics.

- Fixed NVENC playback turning anamorphic sources into a solid-green frame on
  FFmpeg 9 when CUDA first shrank the video by its sample-aspect ratio and then
  scaled it to the channel. ErsatzRS now performs one CUDA scale directly to
  the computed square-pixel size, preserving the intended aspect ratio without
  the broken intermediate surface.

- Fixed late tune-in for script-backed Remote Streams by adding opt-in
  source-side seeking. Extractors that accept a standalone `{seek}` argument now
  receive the current playout offset instead of forcing FFmpeg to drain a
  non-seekable pipe from the start; unmarked scripts retain their existing
  behavior. The bundled Beeld & Geluid wrappers use this contract, so tuning
  into a long Schatkamer episode starts promptly even well after its beginning.
  A canonical Windows YouTube wrapper and YAML template now apply the same
  contract while keeping `yt-dlp` an optional manual install, preserving its
  request headers, and forcing downloads through ErsatzRS's managed FFmpeg.

- Fixed HDR content keeping its HDR metadata after being converted to SDR. The
  mastering-display and MaxCLL/MaxFALL values survived tone-mapping and were
  written onto the BT.709 output, so a display was told to expect a 1000-nit
  HDR grade for a picture that had already been converted away from it. This
  affected every HDR source and every encoder.

- Fixed portrait and other rotated recordings being scaled as though they were
  landscape. FFmpeg rotates such sources from their display matrix before any
  filter runs, so ErsatzRS now sizes them from their displayed shape — a
  portrait item letterboxes into a 1080p channel instead of filling it. Scanning
  records the rotation, stream copy is untouched, and no rotation filter is
  added, so the picture is never rotated twice.

- Fixed a graphics element pointing at an animated WebP or GIF taking its whole
  channel down. The loop option differs per demuxer, and the wrong one makes
  FFmpeg fail to open the input at all; it is now chosen from the file's own
  header.

- Fixed NVIDIA HDR playback selecting a Vulkan/libplacebo graph that cannot run.
  ErsatzRS chose that path whenever `ffmpeg -hwaccels` listed `vulkan`, which is
  not evidence that the graph works — on a WSL2 host with an RTX 3080 Ti the
  only Vulkan device is a software rasterizer, so every HDR NVENC transcode died
  with `Could not derive CUDA device`. A bounded functional probe now runs the
  real graph at capability refresh, classifies the failure, remembers the denial
  per FFmpeg binary and GPU, and falls back once to a proven path that still
  encodes on the GPU. Platforms without Vulkan stay healthy, and a new
  **Hardware Graph Capability** health row reports when a fallback is in effect.

  Two related runtime failures were fixed while proving the fallback: the
  previous "non-Vulkan" NVENC HDR chain still ran libplacebo and therefore still
  needed a Vulkan device, and downloading a 10-bit HDR CUDA surface as nv12 is
  rejected by FFmpeg.

- Removed the redundant **Apply** button from the Windows startup section in
  **Settings** -> **UI**. The existing **Save Settings** action now saves the
  theme, language, and Windows startup choice together.

- Fixed the programme guide on the channel watch route never scrolling. Its
  section is a plain grid item, so the page's implicit `auto` column sized
  itself to the guide's full zoomed timeline (~9700px) and left the guide's own
  scroller as wide as its content — the **Now**, **-2h**/**-1h** and
  **+2h**/**+4h** controls had nothing to move. The standalone EPG preview was
  already correct and is unchanged.

- Fixed FFmpeg 9 playback failures by replacing removed `-vsync` arguments with
  `-fps_mode`, translating legacy NVENC low-latency presets at the playback
  boundary, and validating the required output/NVENC options at startup. The
  shared fix also covers script-backed Remote Streams, including the Beeld &
  Geluid extractor.

- Fixed the uniform metadata-facet slice so new detail-page actions hydrate,
  existing local-TV seasons are forced through their first facet backfill,
  Plex movie and episode facets are persisted, Plex artwork uses its owning
  server and token, television episodes use field-local embedded fallbacks,
  and embedded-only scans retain `Embedded` source provenance.
- Fixed script-backed Remote Streams failing when a producer emitted a
  streamable MP4, WebM, or another FFmpeg-supported container instead of
  MPEG-TS. Playback and scanner probing now let FFmpeg detect the producer
  container from its bytes, matching ErsatzTV while preserving MPEG-TS script
  support.
- Fixed the cross-browser channel-watch E2E proof measuring EPG zoom geometry
  before the asynchronous preview failure UI had settled, which made slower CI
  runs report a false center-preservation regression.
- Fixed intermittent Linux `ETXTBSY` failures in FFmpeg capability tests by
  serializing every test that spawns a tool while another test may be writing
  a temporary fake executable.
- Fixed local TV remakes and reboots with the same title being merged into one
  show. Full local-library scans now distinguish folder, NFO provider-ID, and
  year identities; transactionally split existing mixed seasons and extras;
  preserve the largest hierarchy's IDs and references; invalidate affected
  folder ETags; clean empty parents without deleting extras-only parents; and
  report any remaining cross-folder ownership through health checks.
- Fixed Windows release validation so the targeted startup-registration tests
  use the normal test profile, where cross-crate test support is available,
  instead of failing to compile before reaching the Windows registry checks.
- Fixed Classic playout corruption across multiple channels by letting SQLite
  and MySQL allocate globally unique identities for collection and fill-group
  enumerator state. Runtime-state writes can no longer replace another
  playout's rows on a primary-key collision; startup audits and Refreshes only
  affected Classic playouts (including legacy textual schedule-kind rows),
  later Continue builds retain the same guard, and the localized health report
  exposes any state that cannot be repaired safely.
- Fixed music video cards showing the letter-placeholder instead of a
  thumbnail for the common same-basename-image sidecar convention (e.g.
  `Song.jpg` next to `Song.mkv`, with no `-thumb` suffix). Local scanning now
  recognizes the bare same-basename image as Thumbnail artwork, matching
  ErsatzTV's `MusicVideoFolderScanner.LocateThumbnail` behavior.
- Fixed local artist scans creating a duplicate artist (with its own
  duplicated artwork) on rescans once an `artist.nfo` name diverged from the
  disk folder name or its disambiguation changed. Artist re-identification now
  matches on the freshly parsed title and disambiguation, and the trailing
  scan pass removes artists that no longer own any music videos, matching
  ErsatzTV's `GetArtistByMetadata` and `DeleteEmptyArtists` behavior.
- Fixed a remastered or upscaled music video (e.g. an "Official Music Video
  (Remastered)" reupload) being classified as an alternate version instead of
  the primary video. Local scanning treats `upscaled` as a `remaster` alias and
  prefers an available enhanced rendition as the canonical music video: a lone
  rendition is promoted outright, and one found alongside a plain/official
  original moves that original into the artist's Versions section. Missing
  enhanced renditions are excluded from canonical selection so a remaining
  playable original becomes primary again, while movie and song renditions
  retain their existing alternate-version behavior.
- Fixed burned-in text subtitle (SRT/ASS/WebVTT) lines silently disappearing
  partway through HLS Segmenter playback whenever a work-ahead chunk restart
  (roughly every 44 seconds) landed in the middle of a subtitle line already
  on screen. The per-chunk subtitle rebase now rewrites cue timestamps
  directly instead of an ffmpeg seek-and-copy pass, and clamps a straddling
  cue to keep showing instead of dropping it. HTTP(S), UTF-8/UTF-16,
  dot-separated SubRip, tab-separated WebVTT timing lines, padded ASS, and
  extensionless provider ASS inputs retain their prior FFmpeg compatibility,
  including a fallback for unsupported syntax. Remote fetch failures also
  redact credential-bearing subtitle URLs from logs (ADR 0061).

## [1.0.17-beta] - 2026-08-02

### Added

- Added rich XMLTV guide content: `/iptv/xmltv.xml` now renders plot, genre
  categories, content rating (mapped to `MPAA`/`VCHIP` per ErsatzTV's `us:`
  convention), artwork icons, episode numbering (`onscreen` and `xmltv_ns`
  systems), and per-kind titles for movies, episodes, music videos, songs,
  other videos, and remote streams, matching ErsatzTV's own field set.
  Channel elements gain the three-way `<display-name>`, category tags, and
  the same logo fallback chain as the M3U playlist. Guide templates are
  customizable per media kind via `minijinja`, following the same bundled-
  default/override-folder convention as the existing Music Video Credits
  templates (`<data>/templates/channel-guide/<name>.jinja`); a broken custom
  override falls back to the bundled default for that one kind, and a
  template that renders invalid XML falls back to a minimal element for
  that one item, rather than either failing the whole guide. Movie icons
  deliberately fall back to correctly sized thumbnail artwork when no poster
  exists, while bundled Remote Stream entries emit persisted genre categories
  for better filtering and discovery. Locally-scanned
  library artwork icons are now actually reachable (see Security) — this
  previously 404'd because the artwork route only understood Jellyfin/Emby
  paths and a local-cache hash convention nothing ever populated.
- Added standalone Linux/macOS and Windows Beeld & Geluid Schatkamer Remote
  Stream extractors and ErsatzRS library instructions.
- Added complete Remote Stream YAML scanning and URL/script probing, direct
  producer piping for IPTV and HLS, live/reconnect timing, structural health
  checks, and per-card Add To Playlist support.
- Added Jellyfin-inspired Version, Video, Audio, and Subtitle selection to
  movie details for exact Add To Schedule and Add To Playlist entries. The
  selected media version and stream indexes persist through editors and
  scheduling into MPEG-TS/HLS playback; stale selections block playback and
  appear in health reporting instead of silently falling back.
- Added a top-level `/epg` preview and a muted-autoplay channel preview modal
  plus dedicated `/channels/:id/watch` experience. The shared guide presents
  two hours of history and 24 hours ahead in accessible, scrollable
  multi-channel lanes; browser playback uses an isolated H.264 eight-bit/AAC
  derivative with Hls.js-first selection, localized failures, scoped preview
  tokens, responsive uncropped desktop/mobile sizing, and explicit local-preview
  versus shared-session controls. Normal IPTV profiles and output remain
  unchanged.
- Added an exhaustive production-admin route contract for Playwright coverage,
  with seeded collection-detail interaction proof for custom playback order and
  item removal. The contract excludes only the feature-gated developer gallery
  and fails when the production router and hydration evidence drift apart.
- Added on-demand GitHub-hosted CI through a manual runner selector. Pull
  requests and `main` pushes still use self-hosted Linux by default, while
  hosted Test work is sharded by web target, uses a lower-memory test profile
  plus LLVM's `lld` linker, and caches pinned build tools to stay within the
  hosted job lifetime. Hosted and self-hosted runs use independent concurrency
  groups so either pool remains available as a fallback.
- Added optional default and fallback subtitle languages to `/settings/ffmpeg`.
  Built-in MPEG-TS and HLS playback now applies item and channel preferences
  first, then the global default, and tries the global fallback before
  selecting no subtitle.

### Changed

- FFmpeg playback-language settings now use Kodi's common ISO-639-1-backed
  language list instead of exposing historical ISO-639-3 entries. The `und`
  value is presented as Original for audio and Undefined for subtitles,
  selects missing-language streams consistently in MPEG-TS and HLS, and is
  persisted for local sidecar subtitles without a language suffix. Stored
  language-code case and surrounding whitespace are matched
  case-insensitively and normalized without changing the selected language.
- Windows beta tags may now use either the terminal `-beta` form or a numbered
  `-beta.N` suffix; both publish only the win-x64 prerelease archive and skip
  the normal multi-platform and Docker release jobs.
- Release archives and the Docker image now ship a single merged `site/`
  directory for admin-UI assets instead of duplicating them across separate
  `site/` and `public/` folders. Release packaging explicitly merges the
  generated `public/` assets with the hydration `pkg/` bundle because
  frontend-only `cargo-leptos` builds do not consistently copy `assets-dir`
  content into `site-root` on every platform. `crates/ersatzrs-web/public/`
  remains the source layout; only the packaged/runtime duplication was removed.
- Replaced the EPG Visible window scrubber with a 50–200% Zoom control that
  rescales the horizontal timeline and preserves its temporal center. The
  default desktop scale shows about three hours, and the channel-watch layout
  now left-aligns its bounded video while giving the guide full width.
- Self-hosted CI jobs now share a persistent `CARGO_TARGET_DIR` outside the
  auto-cleaned checkout instead of restoring `target/` through a remote cache
  round-trip on every run; `clippy`, `test`, and `hydration` no longer pay
  that round-trip on the self-hosted path (the GitHub-hosted fallback is
  unaffected). Also fixes a `test_hosted` matrix cache-key collision, adds a
  manual `reset_target_dir` dispatch input, and adds idempotency guards and
  npm caching for a few smaller redundant CI steps.
- Split the largest media-browser transport, YAML and scripted scheduling
  registries, and Jellyfin/Emby adapters into enumerable feature modules.
  Exhaustive route, instruction, Lua-global, and media-source gates now prevent
  new variants from silently falling through while preserving public imports,
  server-function endpoints, serialized YAML, scheduler output, provider
  requests and persistence, hydration, and visible admin behavior.
- Search and per-kind media browser cards now preserve ErsatzTV's artwork
  semantics: poster cards request provider height 440, thumbnail cards request
  height 220, and existing provider resize parameters are replaced instead of
  duplicated. Season and episode cards keep valid upstream navigation, music
  videos no longer receive fabricated detail links, and See All routes preserve
  the active query through pagination and letter navigation. The pinned
  upstream routes have no library-filter combobox, so ErsatzRS does not invent
  one.
- Watermark editing now renders the selected custom asset in a truthful
  Full-HD reference preview. Actual Size uses native image dimensions, the
  preview includes a centered 4:3 safe-frame guide without an outer 16:9
  guideline, and localized Previous/Next controls provide deterministic
  comparison navigation with explicit list ends and unsaved-edit protection.
- Split Trakt maintenance into capability-honest local and remote paths. Local
  matching, generated playlists, settings, deletion, and hourly rematching no
  longer require a placeholder API client; interactive and scheduled remote
  refresh require real configuration and retain typed failures without
  preventing independent local matching.
- Completed the admin application-layer extraction. Collection, playlist,
  channel, schedule, settings, Trakt, media, playout, template, deco, filler,
  watermark, troubleshooting, artwork, and layout transports now call
  storage-neutral application commands and queries while infrastructure owns
  SQL, typed row decoding, transaction boundaries, and SQLite/MySQL details.
  Shared localized resource states no longer turn storage or decode failures
  into submittable forms or plausible empty/default data, and architecture
  lint now forbids raw admin pool access alongside direct SQL and silent
  fallback patterns.
- Unsaved block previews now use the production block scheduling engine in
  Reset mode with temporary template, playout, and reference state instead of a
  simplified duration projection. The editor shows the actual enriched rows
  produced from every supported block collection kind, preserves drafts
  without saving, and distinguishes loading, valid empty, and localized
  failure states.
- Updated futures-util to 0.3.33, Tokio to 1.53.1, leptos_axum to 0.8.10,
  async-trait to 0.1.91, and clap to 4.6.4. Scheduled Cargo and GitHub Actions
  updates now arrive in one weekly multi-ecosystem Dependabot rollup instead of
  separate pull requests.
- All playout-build requests now enter through one application command with
  typed accepted, queued, building, unavailable, and rejected outcomes. Admin
  and API side effects share the same FIFO admission, coalescing, and activity
  state while the scheduler retains Refresh/Reset processing and persisted
  build behavior. Stronger modes now upgrade queued work, and mutations that
  arrive during an active build retain one mode-upgradable follow-up instead
  of being silently discarded.
- Library and targeted-show scans now enter through one application-owned FIFO
  command boundary and operation registry. Web and periodic refresh producers
  share persisted provider validation, queued/running/progress/terminal state,
  Local show fallback, provider-specific show synchronization, and retained
  failure diagnostics while the native composition root keeps concrete
  provider, local scanner, and embedded-subtitle execution. Periodic refresh
  coalesces a library that is already queued or running without changing manual
  FIFO repeat behavior, and rejected scan requests now surface as failures
  instead of false queued confirmations.
- The Phase 7B application-service foundation now gives `ersatzrs-app`
  documented command, query, typed-error, port, and operation-coordination
  boundaries. Core no longer exposes SQLx errors, infrastructure maps concrete
  storage failures explicitly, and the native web composition root injects
  concrete repositories into one `ApplicationServices` container shared with
  scheduler tasks.
- Development-only component-gallery and sample-stream routes now require a
  non-default build feature plus explicit runtime opt-in. Structured hydration
  data is now the canonical admin route-verification source, with validation
  against duplicate or mismatched TEST.md route tables and durable browser
  coverage for the detailed playout workflow.
- Admin settings and library storage failures now render a localized, stable
  `data-state="error"` resource instead of a plausible default form or empty
  list. Their reads use typed infrastructure models and the shared repository
  error vocabulary; architecture lint now also ratchets alternate awaited
  result fallbacks, form-rendering Suspense fallbacks, and row-decode defaults.
  Library polling errors clear after the affected load or a complete polling
  cycle recovers instead of leaving the page permanently in an error state.
- Established the storage-neutral application-service boundary and native web
  composition root in ADR 0051. A baseline-aware architecture gate now blocks
  an empty app crate, forbidden adapter dependencies, new direct admin SQL, and
  new server-error-to-default loaders while existing debt is migrated by
  vertical slice; raw Executor SQL, SQLx glob imports, and production-capable
  cfg forms cannot bypass its debt accounting.

- Added dependency-aware structural milestone tracking through
  `cargo xtask progress`, with validated acceptance/evidence records for
  restoring the application layer, closing audited stopgaps, and preparing the
  future "The Box" video-jukebox add-on API.
- Renamed the FFmpeg profile audio/video transcoding controls and database
  fields from the ambiguous `NormalizeAudio`/`NormalizeVideo` to
  `TranscodeAudio`/`TranscodeVideo`. Disabling them still copies the source
  streams; LoudNorm, framerate, and color normalization remain separate.
- Made Dutch (nl) admin UI terminology consistent. "Shuffle" now uses the common
  loanword everywhere instead of the uncommon "schudden", and the "Scripted",
  playout, deco, and multi-collection wording is unified (one rendering each,
  matching the surrounding labels).
- Saving a channel no longer rebuilds its playouts automatically, matching
  ErsatzTV. Manual Refresh/Reset and Reset All requests now use a reliable
  unbounded worker queue, acknowledge queued work immediately, and show
  queued/building activity until the persisted build outcome is available.
- Classic schedule items now present Smart Shuffle as a Playback Order option.
  Selecting it keeps the compatible persisted Shuffle order and enables the
  item policy; its spacing controls appear inline only while selected. Existing
  Smart Shuffle policies attached to other randomized Classic orders normalize
  to Shuffle when next saved.

### Removed

- Removed the Smart Shuffle Overview preview panel from the schedule items page
  (`/schedules/{id}/items`) and the block editor, along with its server
  functions, response types, translations, and the underlying
  `smart_shuffle_candidates` repository query. Building the preview had to
  enumerate every candidate of every item in the draft, which was too slow to
  be useful. The Smart Shuffle item settings themselves are unchanged, as is
  Smart Shuffle behavior during playout building.

### Fixed

- Fixed rich XMLTV guide rendering for metadata and provider URLs containing
  XML metacharacters, localized non-ASCII ratings, custom-title overrides,
  episode poster/still artwork parity, and literal percent sequences in local
  artwork paths. Custom guide templates now receive ErsatzTV's complete
  per-kind context, and guide metadata is loaded in bounded batches instead of
  issuing sequential subtype and relationship queries for every distinct
  scheduled item. Token-complete custom fragments with unclosed elements now
  take the per-item fallback path instead of corrupting the complete guide.
- Fixed the XMLTV `<channel id>`/`<programme channel>` attributes, which used
  the bare channel number and could mismatch the M3U playlist's checksummed
  `tvg-id`, breaking guide-to-channel association for IPTV clients that match
  strictly by id. The `xmltv.days_to_build` and `xmltv.time_zone` settings
  now take effect (previously always rendered a hardcoded 48-hour UTC
  window regardless of the persisted setting). Jellyfin and Emby movie sync
  now persist genres, tags, actors, directors, and writers, matching show
  and episode sync (previously silently dropped for movies only).
- Delayed mirror channels now reuse their source channel's playout across
  MPEG-TS, HLS, browser preview, admin EPG, and XMLTV while keeping the mirror's
  own channel settings. Signed offsets round-trip correctly, source history is
  retained until delayed broadcasts finish, and the admin UI prevents valid
  mirrors from receiving an independent playout.
- Beeld & Geluid Remote Stream extractors now select one video/audio rendition
  pair instead of mapping every HLS rendition into the MPEG-TS pipe, preserving
  the stream order required by channel HLS transcoding.
- Windows Remote Stream batch producers now receive their quoted script path
  and arguments through the exact `cmd.exe` command line, so paths containing
  spaces and URL arguments no longer become literal quoted executable names
  during scanning or IPTV/HLS playback.
- Scanner settings now identify the library refresh interval as hours in every
  supported language, matching the unit used by the scheduler and ErsatzTV's
  field adornment.
- Admin toasts now match ErsatzTV's pinned MudBlazor 8.15 snackbar behavior and
  appearance: filled light/dark severity colors, top-right placement, semantic
  icons, accessible inline controls, five-item oldest-first queueing, duplicate
  suppression, timed entry/visibility/exit, and lifecycle pause during pointer
  or touch interaction.
- Playwright E2E now explicitly regenerates the Stylance CSS bundle and
  synchronizes the authoritative web `public/` assets into the built Leptos
  site root before booting the server, preventing clean checkouts paired with
  persistent Cargo targets from omitting generated CSS or producing broad 404,
  MIME-type, hydration, and geometry failures.
- HLS work-ahead now evaluates Remote Stream liveness at the actual transcode
  cursor, preventing a following live item from being opened before its
  scheduled start. Script producers are cancelled and reaped whenever IPTV,
  HLS, or ffprobe stops consuming them, including stalled producers with no
  stdout, and producer stderr now retains only a bounded rolling tail.
- Exact movie stream selections now preserve an explicitly silent audio choice
  and Subtitle Off instead of falling through to channel selectors, resolve
  pinned subtitles against the selected media version, and fail closed on
  ambiguous subtitle metadata. EPG zoom also keeps the temporal viewport
  center instead of jumping to the start, and disappearing movies report a
  localized error in every supported culture.
- Classic-scheduled randomized playouts no longer freeze on the same order
  after every `Reset`: the operation now regenerates the playout seed,
  matching ErsatzTV's `ResetPlayout()`. `Refresh` now rejects the old
  end-of-horizon schedule state and undated Continue collection anchors,
  rebuilding only from a dated checkpoint aligned with the requested window
  when one exists.
- The Classic engine's shuffle seed arithmetic no longer silently collapses a
  negative seed to a constant `0`, and "Shuffle Schedule Items" reshuffling on
  a cycle wraparound now avoids repeating the same schedule item across the
  reshuffle boundary, matching the equivalent protection Classic collection
  shuffles already had.
- `Random Start Point` now only affects `Chronological`, `Season, Episode`,
  and `Shuffle In Order` schedule items, matching ErsatzTV. Previously it was
  applied to every playback order, including ones that are already
  randomized by construction.
- Classic scheduling's `Shuffle`, `Shuffle In Order`, `Random`, and `Random
  Rotation` playback orders are no longer aliases of each other. `Shuffle` is
  now a plain flat shuffle; `Shuffle In Order` (and the `Multi-Episode
  Shuffle` placeholder) keep the group-distance spread that previously
  applied to `Shuffle` too, now generalized to spread by show as well as
  artist; `Random` performs independent deterministic draws with replacement;
  and `Random Rotation` randomizes within show/artist groups while preserving
  group separation across cycle and persisted-build boundaries, including
  collections that repeat one media item with distinct audio or subtitle
  overrides. Rotation uses a direct singleton-group path and heap-backed
  grouped interleave so large movie libraries no longer trigger quadratic
  scheduling work.
- Browser-preview E2E fixtures now preserve the channel-number and integer
  schedule-kind database contracts, so the shared Playwright database cannot
  invalidate later channel-number and playout coverage.
- Manual CI runs on the default branch now lint the selected head commit
  instead of passing an empty range to commitlint, restoring the
  GitHub-hosted fallback when the self-hosted runner is unavailable.
- Hardened the EPG and browser-preview slice after correctness review. Preview
  and normal HLS workers now use distinct admission identities while sharing
  the same global capacity limit; guide labels use local time; aggregate
  selection matches live playback when playouts overlap; large title lookups
  are deduplicated and batched; and modal close, Stop Preview, and channel
  changes invalidate pending asynchronous responses. New HLS workers also
  withhold playlists until their session directory is prepared, preventing a
  restart from briefly publishing stale segments.
- Custom channel stream selectors are now read from one canonical folder,
  `<data>/scripts/channel-stream-selectors`, matching ErsatzTV's
  `ScriptsFolder/channel-stream-selectors`. Playback and playback
  troubleshooting previously looked one level higher than the channel editor
  listed them, so a selector that appeared in the channel dropdown was never
  applied. The folder is created at startup, an absent or unreadable folder is
  reported as "no selectors" instead of an error, and every file in it is
  offered (not only `.yml`/`.yaml`), so the channel editor and the
  troubleshooting page can no longer disagree. This fixes
  `/system/troubleshooting/playback` rendering only an error message on installs
  that never created the folder.
- The Add Smart Collection page no longer breaks admin-UI interactivity. It
  queried for collection id 0, and reporting that failure through the toast
  channel during server rendering left the client unable to hydrate, disabling
  every button and dialog on the page. Toasts are now client-only by
  construction, and `cargo xtask hydration-lint` gained a rule that fails the
  build when a server-side load reaches for the toast channel.
- Admin pages now report a failed initial load instead of quietly showing empty
  or default data. Media browsers and collection member lists render the shared
  error state; the Plex, Jellyfin, and Emby library and path editors show the
  error and disable Save so a failed load cannot overwrite real settings; and
  the collection and smart-collection editors surface it with the form disabled
  while still server-rendering the no-JS fallback.
- Preserved typed failures and consistency across extracted admin mutations.
  Collection media actions now validate referenced records and create a new
  collection with its first item in one transaction. Initial playout creation
  removes the unbuilt record when scheduler admission fails, automatic
  collection, schedule, settings, and playout rebuild failures are surfaced,
  and FFmpeg profile reads no longer mask decode or storage failures as
  missing profiles.
- Pull-request labels no longer trigger full CI runs. Hosted-runner fallback is
  now manual-only, preventing dependency automation from spawning duplicate
  workflows while applying routine labels.
- fMP4 HLS init-segment deduplication now selects the earliest generation
  deterministically instead of depending on filesystem directory order.
- Pull-request commit lint now runs the repository's pinned npm dependencies
  directly, so it no longer requires Docker on the self-hosted WSL runner.
- GitHub Actions and release builds now use the repository's supported Rust
  1.97.1 toolchain again, and Dependabot no longer proposes updates for the
  toolchain action whose tags can name Rust releases that do not exist.
- The global preferred audio language now drives built-in MPEG-TS and HLS
  stream selection when an item and channel do not override it. HLS Direct
  retains all source audio streams only when both the explicit audio language
  and title are blank, matching ErsatzTV.
- Playout rebuild confirmations now use the shared success-toast surface instead
  of rendering an unstyled message between the `/playouts` action bar and page
  content. Queued and active rebuilds remain visible through the row spinner.
- Local `dev`, `build`, and `serve` wrappers now build the scanner companion
  beside the web binary before startup, so scheduled automatic library
  refreshes no longer get skipped when only the Leptos web target existed.
- Playback and HLS data access now distinguishes valid missing metadata or
  configuration from database and decode failures. IPTV, HLS, capacity slates,
  and troubleshooting previews refuse to build altered FFmpeg commands or use
  raw paths, default capacity, or different audio, subtitle, deinterlace,
  tonemap, and frame-rate behavior after an operational failure; correlated
  logs and health reporting expose the cause.

### Security

- Fixed an unauthenticated arbitrary local file read in `/artwork/posters`,
  `/artwork/fanart`, and `/artwork/thumbnails`: a percent-encoded or
  double-slash absolute path in the URL replaced the intended local-cache
  base directory entirely (`PathBuf::join` overrides its base when the
  joined segment is absolute), letting any remote client read any file
  readable by the server process. Local artwork is now only ever served
  when the requested path exactly matches a `Artwork.Path` row the scanner
  already persisted, and the local-cache-hash lookup path now rejects any
  path component that isn't an ordinary filename.
- Preview and normal HLS segment routes now reject unsafe channel path
  components before resolving session files, preventing percent-decoded route
  captures from escaping their session-owned temporary directories when IPTV
  token enforcement is disabled or an unscoped token is used.

## [1.0.15-beta.9] - 2026-07-19

### Changed

- Renamed Similar Video Cooldown to Smart Shuffle and moved its opt-in policy
  from schedule/playout-wide settings onto each Classic schedule item, reusable
  Block item, Sequential YAML content entry, and Scripted content key. Legacy
  policies and anchor JSON remain readable and migrate without changing active
  schedules. Classic and Block editors now provide draft-aware per-source
  summaries plus searchable, tag-filtered, 50-row candidate details.
- Rebuilt the end-user documentation around Diátaxis with a first-channel
  tutorial; goal-oriented Windows, Docker, local-library, Jellyfin, IPTV,
  backup, content-variant, and troubleshooting guides; and current operator
  references for configuration, endpoints, navigation, media-source support,
  system requirements, release bundles, channel architecture, and the managed
  FFmpeg runtime. Contributor, parity, testing, and release-maintainer material
  now has a separate navigation map so it does not obscure the new-user path.
- Raised the required Rust toolchain from 1.85 to the latest stable release,
  Rust 1.97.1, across workspace manifests, local development, CI, release
  builds, and Docker builders.
- Enforced cargo-nextest for local Rust testing through cross-platform
  `run.sh` / `run.cmd` wrappers and Codex/Claude command guards. The closeout
  wrapper runs nextest before Cargo doctests, while CI retains its direct Cargo
  test selection.
- SQLite startup and lazy pools now enforce foreign keys, a 30-second busy
  timeout, WAL journaling, and NORMAL synchronization on every connection;
  the matching migration enables WAL for existing databases, and health
  reporting detects any runtime drift from those defaults.

### Fixed

- Smart Shuffle now isolates Classic policies and filter tags by schedule-item
  identity even when multiple items share one content source or reuse a filter
  rule ID for different searches, preserves explicit YAML and reusable Block
  item Off overrides during legacy-policy migration, hydrates tags from
  multiline Scripted Lua declarations, localizes overview warnings, and
  advances playout-scoped item spacing across intervening randomized-Off and
  chronological sources.
- Schedule item editing now explains that pre-, mid-, and post-roll filler
  applies to every media item selected by that schedule item. Regression
  coverage preserves ordered `Multiple (3)` then `One` schedules through
  save/reload and Classic Refresh rebuilds, protecting three-videos/one-bumper
  channel loops from collapsing into per-video post-roll filler.
- NVENC channels can now downscale HDR sources and composite alpha watermarks
  without feeding an `nv12` main surface into a `yuva420p` CUDA overlay. The
  pipeline tracks the real post-scale CUDA format and inserts the required
  `yuv420p` conversion before `overlay_cuda`, preventing HLS segmenter startup
  failures on affected Windows/NVIDIA systems.
- Unchanged Plex, Jellyfin, and Emby items in `RemoteOnly` state now honor the
  provider ETag short-circuit instead of refetching metadata and overwriting
  persisted media-file data on every scan. Content-variant classification still
  reapplies from provider page data, while `FileNotFound` and `Unavailable`
  items continue through a full rescan so they can recover.

## [1.0.15-beta.8] - 2026-07-17

### Fixed

- Windows beta archives now generate and verify the public Stylance bundle
  before packaging. They stage Leptos JavaScript/WASM under `site/pkg` only,
  avoiding duplicate public fonts and images under `site/`.

## [1.0.15-beta.7] - 2026-07-17

### Fixed

- Windows-only beta releases now select Git Bash before the Rust toolchain
  action, so its temporary scripts execute on the self-hosted Windows runner.

## [1.0.15-beta.6] - 2026-07-17

### Added

- French (France) admin UI localization (ADR 0021): the admin UI can now be
  used entirely in French. Selecting `français (France)` on `/settings/ui`
  persists `fr-fr` (aliases `fr`, `fr-FR`, `fr_FR`), and every admin page,
  dynamic label, and stable workflow/error message renders in French while
  technical identifiers (FFmpeg, Plex, codec names, routes, configuration
  keys) stay verbatim. This includes ADR 0019's Smart Shuffle presets, thresholds,
  rules, and tag-preview states. Covered by i18n coverage tests and the
  settings-i18n browser proof.
- Collection and playlist items can now persist item-level audio/subtitle
  playback overrides, so admins can add a movie with a commentary or alternate
  audio track and see the saved override on collection/playlist inspection.
- The same movie can now be added to one collection several times when each
  row carries a different playback override, so every commentary or alternate
  audio track of a movie can be represented in a single collection. Duplicate
  rows show their own override summary on the collection detail page, can be
  removed or reordered independently, and schedule separately with their own
  audio/subtitle preferences. (`CollectionItem` now uses a surrogate primary
  key in SQLite and MySQL; adding an item that exactly matches an existing
  row, including default adds without an override, remains a no-op.)
- Artist detail pages now support music-video multi-select: each listed music
  video (including alternate versions) has a checkbox, and the shared
  selection toolbar bulk-adds the selected videos to a collection or playlist.
  Per-video Add To Collection keeps working.
- Schedules can now opt into ADR 0019's deterministic Smart Shuffle
  policy across Classic, Block, Sequential YAML, and Scripted playouts. Shared
  count/duration thresholds diversify artist, show, season, parent variant,
  collection, tag, genre, studio, and operator-filter groups, persist through
  Continue builds, and relax impossible constraints without dead air; Off
  remains byte- and behavior-compatible with imported ErsatzTV schedules.
  Localized admin controls include presets, custom thresholds and rules, and a
  deterministic effective-tag preview. This feature was originally named
  Similar Video Cooldown.
- ADR 0020 proposes rebuilding impacted playouts when channels, schedules, or
  referenced library content change, so already-scheduled items pick up new
  settings (e.g. subtitle mode) instead of only being appended past — including
  porting ErsatzTV's item-clearing `Refresh` semantics to the classic engine.

### Changed

- Add To Collection and Add To Playlist now keep you on the current media
  browse, detail, or search page after a successful add instead of navigating
  to the target collection or playlist. The success notification includes an
  explicit, localized "View Collection" / "View Playlist" link for when you do
  want to jump there.
- Updated the locked Leptos package family to `leptos` 0.8.20.
- Updated the locked `leptos_router` dependency to 0.8.14.
- Updated the locked `jsonschema` dependency to 0.46.9.
- Updated the locked `open` dependency to 5.3.6.
- The channel editor's Subtitle Mode dropdown now shows helper text (all
  locales) clarifying that "Forced" only selects forced-narrative tracks and
  that "Any" is the mode to always show subtitles in the preferred language,
  reducing a common misconfiguration where sidecar subtitles never appear.
- Progress tracking now keeps reopened media-browser, ADR 0018 movie-detail, and
  Zero Duration health-check parity debts visible for follow-up work.
- Progress tracking now also records reopened channel-add default group, ADR
  0017 default FFmpeg profile bitrate, and FFmpeg profile editor spacing debts.
- Dutch admin UI now translates "watermark" as "zenderlogo" (station logo)
  instead of the literal "watermerk", matching common Dutch broadcast usage.
- Fresh-database default FFmpeg profiles now seed halved video bitrates
  (4000, 8000, 10000, 22500, and 30000 kbit/s for the 720p through 4K HDR
  profiles) while keeping the previous video buffer sizes, per ADR 0017 v3.
  Existing databases keep their saved profiles unchanged.

### Fixed

- Classic, YAML, and scripted playout `Refresh` builds now clear and rebuild the
  scheduled window instead of appending past stale items; classic refresh also
  preserves same-day checkpoint anchors so enumerator/shuffle continuity
  survives while per-item settings are re-stamped.
- Saving a channel now enqueues a playout `Refresh` for each channel playout
  when playback-impacting fields change, including subtitle/audio preferences,
  watermark, streaming mode, FFmpeg profile, and playout offset.
- Reset Playout and Reset All Playouts now rewrite near-term classic playout
  rows instead of only appending future items, so reset-applied playback
  settings are visible on the next scheduled item.
- Collection detail pages now hydrate cleanly after adding items from media
  pages and following the explicit "View Collection" toast action.
- Creating a channel at `/channels/add` now pre-fills the Group field with
  `ErsatzRS` again instead of `ErsatzTV`; existing channels keep their saved
  group values.
- The FFmpeg profile editor's Video and Audio sections now use the same
  vertical spacing between fields as the General section, in desktop and
  mobile layouts.
- Release Docker image builds now compile the native `ersatzrs-web` crate as an
  `rlib` only, avoiding the unused native web `cdylib` link that caused ARMv7
  QEMU aborts during multi-arch GHCR publication.
- Movie detail pages now render linked extras as TV-style action rows, allow
  those extras to be added to collections and playlists, and expose alternate
  movie editions plus commentary audio/subtitle playback options.
- Songs and images now use the shared media-browser action shell with the
  right-side selection toolbar, item actions, media-info/troubleshooting links,
  and correct playlist item types; artist and show detail pages now hydrate
  reliably with restored artist collection and preview-artwork behavior.
- The Zero Duration health check now ignores ordinary static image media while
  continuing to warn for zero-duration video/audio media and animated-capable
  image paths that still need duration metadata.

### Security

- Updated `quick-xml` to 0.41 to resolve RUSTSEC-2026-0194 (quadratic
  duplicate-attribute checking) and RUSTSEC-2026-0195 (unbounded
  namespace-declaration allocation), both denial-of-service vectors in XML
  parsing.

## [1.0.15-beta.4] - 2026-07-01

### Fixed

- Release Docker image builds now compile only the runtime binaries instead of
  the extra web library target, avoiding ARMv7 QEMU memory exhaustion while
  publishing the multi-arch GHCR image.

## [1.0.15-beta.3] - 2026-07-01

### Fixed

- Release Docker image builds now isolate Cargo target caches per target
  platform, avoiding concurrent multi-arch BuildKit cleanup races that could
  remove active Cargo fingerprints during GHCR publication.

## [1.0.15-beta.2] - 2026-06-30

### Fixed

- Release Docker image builds now reuse the prebuilt Leptos web-assets artifact
  instead of compiling `cargo-leptos` inside every target image, avoiding ARMv7
  QEMU out-of-memory failures during GHCR publication.

## [1.0.15-beta.1] - 2026-06-30

### Added

- In-app MPEG-TS troubleshooting for ADR 0013 at
  `/system/troubleshooting/mpeg`: the Support page can capture bounded
  channel MPEG-TS samples, run optional read-only TSDuck `tsp`/`tsbitrate`
  analysis, write a redacted report archive, and surface TSDuck availability
  as a non-blocking health row.
- A purpose-built `ersatzrs-healthcheck` binary now probes `/healthz` without
  curl or shell tooling, ships in release archives, and backs the Docker image
  `HEALTHCHECK`.
- Project-local Codex and Claude hooks now block maintainer-machine absolute
  paths, private media paths, and likely secrets before repository writes, and
  run `rustfmt` on a single edited Rust file after edit tools complete.
- Repo-local `hydration-islands` skill for Leptos WASM island and SSR HTML
  development, capturing the project-specific boundary rules, fallback
  expectations, and browser smoke gates that protect admin UI hydration.
- Content-variant search foundation for ADR 0018: `MediaItem` can now store
  primary/extra/alternate-version identity, subtype, and parent linkage, and
  smart-collection/search evaluation can query `content_kind`, `sub_kind`,
  `parent_title`, `audio_title`, `subtitle_title`, and `has_commentary` while
  defaulting existing searches to primary items.
- Content-variant UI surfacing for ADR 0018: media browse pages now always show
  special features / versions with localized badges, TV show/season specials
  render as episode-style rows, detail pages expose linked Extras / Special
  Features and Versions sections, and search/smart-collection editing shows the
  new token reference.
- Content-variant local scanner import for ADR 0018: movie/show library extras
  matching ErsatzTV's extra suffix and directory taxonomy are now imported as
  typed `OtherVideo` rows with normalized sub-kinds and parent linkage while
  remaining hidden from default search/scheduling behavior.
- Content-variant alternate-version linking for ADR 0018: local music-video,
  song, and movie scans now classify live, unplugged, acoustic, remix,
  remaster, extended, and director-cut renditions, keep deterministic canonical
  primaries, link same-work alternates through `ParentMediaItemId`, and leave
  orphan alternates typed but unlinked.
- Content-variant commentary render acceptance for ADR 0018: smart collections
  can feed commentary-bearing items into channels whose preferred audio title
  or custom stream selector pins commentary audio and matching subtitles in
  direct IPTV/MPEG-TS and HLS segmenter playback.
- Content-variant scheduling examples for ADR 0018: built-in templates and
  operator docs now cover unplugged rendition smart collections, commentary
  movie channels, and featurette pre-roll filler attached to primary movie
  schedule items without changing default schedules.
- Content-variant provider imports for ADR 0018: Plex, Jellyfin, and Emby movie
  library sync now classify server-reported extras and alternate versions into
  the same hidden-by-default search/scheduling fields used by local libraries.
- Release gate against the recurring "wasm checksum mismatch" that has broken
  several releases (`scripts/release/verify-web-bundle.sh`, wired into the
  release workflow). The Leptos SSR binary and the WASM bundle are built in
  separate jobs; if they disagree on cargo-leptos file hashing the server
  references `/pkg` asset URLs the bundle does not contain and the admin UI
  never hydrates. The gate boots the assembled bundle and fails the release if
  any hydration asset 404s. Hashing is now pinned off deterministically
  (`hash-files = false` in `crates/ersatzrs-web/Cargo.toml`, `LEPTOS_HASH_FILES`
  on both release jobs).
- Whole-app island hydration smoke (`scripts/e2e/hydration-smoke.sh` +
  `tests/e2e/island-hydration.spec.ts`): a browser-level pre-release check that
  boots the host and verifies every route rendering a Leptos `#[island]`
  actually hydrates (sets `window.__ersatzrs_hydrated`, no marker-mismatch
  panics). Documents the build-consistency requirement in BUILD.md.
- CI now runs `cargo xtask hydration-lint -p ersatzrs-web` and the full
  Playwright admin E2E suite by default, booting `ersatzrs-web` against an
  isolated SQLite database with matched Leptos SSR/WASM assets.

### Changed

- ErsatzRS is now a beta-stage project, and the release line publishes Linux
  and Windows archives plus GHCR Docker images while still intentionally
  omitting macOS archives for now.
- Startup database URL resolution now flows through the startup configuration
  helper, preferring `ErsatzRS_DATABASE__URL` / `ERSATZRS_DATABASE__URL` while
  preserving the legacy Docker `DATABASE_URL` override and default SQLite path.
- Infra health checks are split into registry submodules under
  `crates/ersatzrs-infra/src/health/`, with a completeness test covering the
  registered title set and order.

### Fixed

- Existing app-data MPEG-TS wrapper defaults that were seeded with `MuxRate` but
  without `PcrPeriod` now upgrade on startup, so direct `.ts` playback uses the
  same 20 ms PCR cadence as the current bundled script after a restart.
- HLS Direct MPEG-TS output now transcodes TS-incompatible copy sources such as
  WebM/Opus to the channel profile instead of emitting private/data streams,
  and normalizes hardware-only H.264 presets when falling back to software x264.
- Default `.ts` requests for MPEG-TS hybrid channels now use the continuous
  MPEG-TS path unless the hybrid wrapper is requested explicitly, avoiding
  HLS-wrapper segment resets in VLC.
- HLS segmenter startup now waits for a stable media-segment floor and serves
  normalized MPEG-TS playlists without ordinary item-boundary discontinuity
  tags, so VLC no longer turns item changes into PCR resets or rebuffer stalls.
- HLS Segmenter MPEG-TS appends now advance from the worker's successful append
  duration instead of a lagging PCR probe, preventing overlapping 4-second
  timestamp ranges that made VLC, Kodi, and MPC-HC freeze until pause/resume.
- The startup FFmpeg runtime downloaded/updated toast now appears once per
  browser tab for a specific runtime event instead of reappearing on every admin
  page switch.
- Foreground MPEG-TS viewers now reclaim stale HLS transcode slots before
  emitting the all-streams-in-use slate, preventing a recently closed HLS
  session from injecting a slate into direct channel playback.
- MPEG-TS troubleshooting reports now include a per-second stability meter and
  persist the same samples as `stability.json`, making the 20-second TSDuck
  analysis usable for spotting empty captures, continuity failures, and PCR
  warnings without reading raw logs first.
- Multi-collection editor hydration now keeps the save button DOM stable while
  preserving the ErsatzTV-style action bar icon in both hydrated and no-JS
  fallback rendering.
- Pending admin hydration gaps for Plex/Jellyfin/Emby source pages, Movies,
  Trash, Trakt lists, Filler Presets, and Rerun Collections now have durable
  seeded Playwright coverage; the affected islands resolve initial SSR data
  before hydration and provider path replacement tables stay within mobile
  layouts.
- CI Playwright E2E now runs on its own port, and `ersatzrs-web` honors the
  configured Leptos bind address so the self-hosted runner can coexist with a
  live maintainer admin server on the default `8409` port.
- Schedule item HTML fallback saves now parse repeated raw row fields before
  reconstructing schedule items, so no-JS `/api/schedules/items/save` submits
  preserve multiple rows and their filler, watermark, graphics, language, and
  playback fields instead of rejecting repeated `item_index` values.
- Admin-triggered local library scans now keep retrying SQLite busy/locked
  contention at each import/link/finalization write, so a long Music Videos
  scan can finish ADR 0018 alternate-version classification instead of replaying
  the whole scan and leaving documented smart-collection selectors empty.
- Classic Duration schedule items with count-based post-roll filler now advance
  through the filler before scheduling the next primary item, preventing
  overlapping playout rows in generated schedules.
- Classic schedules with dynamic programming before a future fixed block now
  start at the dynamic item, so channels have active playout rows before the
  fixed anchor instead of going offline until that block.
- Classic shuffled music-video scheduling now carries artist IDs through
  reference-data loading and spaces repeated artists when possible, preventing
  same-artist videos from clustering in MTV-style blocks.
- Admin route-family Playwright coverage now has stable hydrated startup state:
  library scan actions expose queued/failure status, media collection editors
  keep their SSR data through hydration, and scheduling editors load their
  initial data before the island boundary instead of panicking on marker-node
  mismatches.
- FFmpeg profile editing now serves its no-JS fallback from already-resolved
  SSR data and keeps `<noscript>` free of hydration resources, so the H.264
  profile select remains available without client JavaScript.
- The Playwright E2E wrapper now runs serially by default unless
  `ERSATZRS_E2E_WORKERS` or an explicit `--workers` argument is supplied,
  preventing shared-database route specs from racing in the default CI suite.
- Docker runtime images now include the admin UI public assets, Leptos
  hydration bundle, and runtime assets so the UI renders and hydrates from the
  image instead of starting with missing `/public` and `/pkg` files.
- Automatic maintenance health runs now reuse the managed FFmpeg paths resolved
  at startup instead of probing bare `ffmpeg` / `ffprobe` from `PATH`.
- Docker containers no longer crash during startup after the NVIDIA native
  hardware probe unloads CUDA/NVENC driver libraries.
- Docker runtime image builds now cache Rust dependency compilation behind a
  manifest/lockfile layer, so source-only edits rebuild workspace crates
  without recompiling the full third-party dependency graph while preserving
  the `ersatzrs-web` and `ersatzrs-healthcheck` runtime layout.
- The `/channels` Edit Numbers action now opens a real `/channels/numbers`
  editor with localized desktop/mobile actions, SSR/no-JS save fallback,
  validation, and atomic channel-number swaps that preserve `SortNumber`.
- Release publishing now restores GHCR Docker images for the beta line while
  keeping the macOS archive omission in place for the active release line.
- Managed FFmpeg startup on Windows now copies downloaded first-party
  ffmpeg/ffprobe files next to `ErsatzRS.exe` and persists those paths so
  `/settings/ffmpeg` shows the acquired runtime instead of a generic PATH tool.
- Fresh databases now seed the first channel name and group as `ErsatzRS`.
- Library scan clicks now queue in FIFO order and run one library scan at a
  time instead of racing concurrent scans or dropping already-locked requests.
- The hydrated Add Schedule Item action on `/schedules/:id/items` now opens a
  new editable row reliably.
- Remote stream placeholders with zero duration no longer trigger Zero Duration
  warnings on `/system/health`.
- MPEG-TS playback timing is stable across direct `.ts`, legacy concat, hybrid
  wrapper, HLS-direct, and MPEG-TS HLS segmenter paths: FFmpeg outputs now share
  profile-derived `-muxrate` / 20 ms PCR-period settings, bundled wrapper
  scripts render the same PCR period, and the HLS segment normalizer preserves
  continuity-counter rules for PCR-only adaptation packets.
- Windows launcher builds made from Linux/WSL now embed the tray icon resources
  correctly, preventing `ErsatzRS.exe` from failing at startup with
  `Error setting icon from resource: 1812`.
- Database schema health reporting now distinguishes real downgrade detection
  from dirty migrations and migration checksum mismatches, so `/system/health`
  no longer labels every schema metadata issue as an ErsatzTV downgrade.
- Admin island hydration is release-green again: the whole-app smoke now passes
  all 56 no-fixture island routes, and browser spot-checks prove `/channels`
  Delete plus `/playouts` row-select/refresh controls attach without marker
  mismatches.
- CI hydration smoke installs Node tools from a committed `package-lock.json`
  so `npm ci` no longer fails on clean runners.
- Linux-hosted clippy now accepts the Windows launcher build script target
  detection without weakening the repository-wide environment-access lint.

## [1.0.14-alpha] - 2026-06-21

### Changed

- Release archives now intentionally skip macOS targets until a future
  macOS-targeted release, preserving GitHub-hosted runner credits while the
  active release line builds Linux and Windows archives on self-hosted runners.

### Fixed

- Windows upgrades from older bundled-FFmpeg archives now clear stale stored
  `ffmpeg.exe` / `ffprobe.exe` paths from the previous release folder when those
  files are no longer present, allowing managed `ErsatzRS-ffmpeg` startup
  acquisition to run instead of failing before download.

## [1.0.13-alpha] - 2026-06-21

### Added

- ErsatzRS now manages its FFmpeg 8.1 runtime at startup: explicit operator
  overrides still win, otherwise the app validates a managed
  `ErsatzRS-ffmpeg` install, downloads the matching first-party release when
  needed, auto-updates managed installs when a newer release exists, verifies
  checksums, runs drawvg/zscale smoke validation, and reports successful
  downloads or failed update attempts through `/system/health` and an admin
  toast.
- Calibration clip generation now includes a `CAL_SIZE_PRESET=cinema-1920x800`
  mode for synthetic scope-ratio proof geometry (`1920x800`, SAR `1:1`, DAR
  `12:5`) plus a live Scale and Pad proof script that boots ErsatzRS, streams
  the generated source through a 1080p FFmpeg profile, and verifies black
  top/bottom padding in the captured output.

### Changed

- Release archives and Docker images no longer package FFmpeg/ffprobe. The
  Windows launcher and Docker runtime now rely on the managed startup
  `ErsatzRS-ffmpeg` download/update path, while offline deployments can still
  use explicit FFmpeg/ffprobe path overrides or a pre-placed managed runtime.

### Fixed

- Over-capacity HLS and MPEG-TS viewers now receive a playable, looping
  "all streams in use" slate that does not consume a transcode slot and switches
  to real content once capacity frees, instead of immediately getting HTTP 503.
- HLS segmenter active transcode limits now enforce a hard ceiling instead of
  letting recently watched channels bypass the cap. Over-capacity HLS sessions
  reclaim work-ahead/cooling/stale watched transcodes first, otherwise wait on
  the slot-free all-streams-in-use slate; the FFmpeg settings copy now describes
  the hard limit.
- FFmpeg command-generation parity is complete: the NVENC encoder-only
  CUDA-filter gate is now live-proven against an FFmpeg 8.1 build with `cuda`
  and `h264_nvenc` but without `scale_cuda`, `yadif_cuda`, or `overlay_cuda`,
  confirming ErsatzRS keeps NVENC encode while demoting decode/filter work away
  from unavailable CUDA filters.
- FFmpeg profile scaling now preserves the selected `Scale and Pad` behavior for
  near-16:9 sources with odd fitted dimensions instead of rounding the fit up
  into a full-frame stretch; `Stretch` and `Crop` command generation now have
  dedicated regression snapshots alongside the NVENC scale-and-pad path.
- FFmpeg profile command generation now honors VAAPI `HardwareIfPossible`
  padding with `pad_vaapi`, and respects the profile `DeinterlaceVideo` flag
  so interlaced sources are only deinterlaced when the selected profile enables
  it.

## [1.0.12-alpha] - 2026-06-19

### Fixed

- Media collection route-family parity is complete for multi-collections,
  rerun collections, playlists, and filler presets. Multi-collections now have
  HTML POST fallback save/delete routes matching the other media collection
  editors, and durable browser coverage proves the route family renders the
  localized list/editor workflows without console errors.
- Scanner parity is complete: local music-video NFO parsing now accepts
  ErsatzTV-style XML fragment streams while preserving fallback behavior for
  invalid sidecars, local scans retry SQLite busy/locked contention instead of
  aborting immediately, and the media-server `Unavailable` state branch has
  regression coverage.
- Admin media scan results now refresh hydrated views without a manual reload:
  `/system/health` recomputes current health checks, `/media/trash`,
  `/media/libraries`, and the media browser lists refetch when scanner-updated
  media state changes. `/media/trash` now includes the same `FileNotFound`
  media families as ErsatzTV, including missing TV episodes that previously
  appeared in health checks but not Trash. The non-ErsatzTV green scan-queued
  snackbar on `/media/libraries` was removed.
- Release builds now serve every Leptos server function again, fixing the
  Windows release-build failures where **Scan Library** (`/media/libraries`) and
  **Show Media Info** for TV episodes reported
  `error deserializing server function results: Invalid format: missing delimiter in ""`.
  Root cause: `server_fn` derives each `/api/<fn><hash>` route from
  `xxh64(CARGO_MANIFEST_DIR + ":" + module_path)`, so the absolute build path
  leaked into the URL. The release pipeline builds the hydration WASM on a Linux
  runner and the per-RID server binary on its own runner, so the two halves
  computed different hashes; the hydrated client POSTed to routes the server
  never registered and every server-function call returned an empty `404`. The
  build now pins `SERVER_FN_OVERRIDE_KEY` (in `.cargo/config.toml`) so paths are
  identical regardless of where each half is compiled. This supersedes the
  1.0.11-alpha attempt, which changed the scan return type but did not address
  the path mismatch.

### Changed

- The Windows (`win-x64`) release now bundles the first-party
  [`ErsatzRS-ffmpeg`](https://github.com/bartdeijkers/ErsatzRS-ffmpeg) **8.1.2**
  package (pinned to release `v1.0.0`) instead of a third-party Gyan build. This
  guarantees the `drawvg` filter (built with `--enable-cairo`) and the rest of
  the ErsatzRS-required capability set, and aligns the bundled runtime with
  ADR 0008 (first-party builds, no `--enable-nonfree`). The drawvg presence
  assertion is retained.
- New installations now seed **five** ready-to-use FFmpeg profiles instead of
  one — 720p, 1080p, 1080p HDR, 4K, and 4K HDR, all with AC-3 6-channel
  640 kbps audio — and apply the best detected hardware encoder to that fresh
  seed at startup with hardware-compatible presets. HLS segmenter output also
  adds `-max_delay 500000` for
  multichannel AC-3 to tighten A/V interleaving at segment boundaries.
- HDR-to-SDR software tone mapping now converts and tags the full BT.709 SDR
  color state instead of leaving BT.2020 primaries/matrix on the output. A new
  manual `scripts/e2e/hdr-tone-map-proof.sh` gate live-proves HDR10 PQ and HLG
  calibration streams against generated SDR references.
- A new manual `scripts/e2e/nvenc-filter-gate-proof.sh` gate codifies the
  remaining NVENC encoder-only proof prerequisite: FFmpeg must report
  `cuda` + `h264_nvenc` while omitting `scale_cuda`, `yadif_cuda`, and
  `overlay_cuda`.
- ADRs now track decision state and implementation state as two independent
  sections. `## Status` records only the decision lifecycle
  (Proposed/Accepted/Superseded); a new `## Implementation` section records
  whether the code exists (`Not implemented` / `Partial` / `Implemented` /
  `N/A`), so an `Accepted {date}` ADR can no longer be mistaken for shipped
  code. `cargo xtask progress adr` surfaces a "Built" column, and
  `cargo xtask progress check` fails if an ADR is missing the section or claims
  `Implemented` while a parity debt linked to it (`adr = "NNNN"`) is still open.

## [1.0.11-alpha] - 2026-06-17

### Fixed

- Library and TV show scan actions now return a non-empty server-function
  payload, fixing the release-build scan toast failure
  `error deserializing server function results: Invalid format: missing delimiter in ""`.

## [1.0.10-alpha] - 2026-06-17

### Fixed

- Windows release UI interactions are restored for the `/media/libraries` scan
  button, `/system/logs` pager controls, and TV season episode **Show Media
  Info** actions. The libraries page now renders its toast container inside the
  hydrated island, and the Logs pager no longer leaks Leptos macro text into the
  DOM or loops log-page reloads after loading the saved page size.

## [0.0.9-alpha] - 2026-06-17

### Fixed

- Missing library items are now detected during scans. Previously nothing ever
  flagged moved/deleted media, so such items stayed `Normal` forever and never
  appeared in `/media/trash`, the **File Not Found** / **Unavailable** health
  checks, or anywhere else. Local scans now flag items whose file disappeared as
  `FileNotFound`; media-server scans (Plex/Jellyfin/Emby, movies and TV) trash
  items no longer present on the server and flag items whose local file is
  missing as `RemoteOnly`. The `Skip Missing Items` setting
  (`playout.skip_missing_items`) is now honored: when enabled, `FileNotFound`
  and `Unavailable` items are excluded from playout scheduling, matching
  ErsatzTV's `PlayoutBuilder`.

### Changed

- HLS Segmenter parity tracking now documents the open calibration playback
  deviation debt: structurally clean HLS captures can still drift at decoded
  content boundaries, so TSDuck transport checks alone are no longer treated as
  sufficient proof of playout continuity.

### Added

- Admin UI languages are now registered from the language files themselves, so
  adding a supported admin language only requires adding a new module under
  `crates/ersatzrs-web/src/admin/languages/` with the standard metadata block
  and translations.
- Remote Streams now has an ErsatzTV-style admin browser at
  `/media/remote/streams`, including search-index filtering, card paging,
  letter jumps, collection/playlist add actions, no-JS list HTML, and
  Playwright coverage.
- Playback calibration and verification tooling for streaming work.
  `scripts/calibration/` generates three 1080p60 clips (per-clip background
  colour, large clip number, millisecond ABS/per-clip timecodes, frame
  counters, a per-second sweep bar, a centred geometry square, and a per-second
  audio pip) as MKV sources plus MPEG-TS, for visually tracking playout order
  and spotting jitter/skips. `scripts/e2e/` adds a TSDuck end-to-end playback
  gate that validates served streams — continuity-counter/PCR/PTS integrity,
  PSI structure, and audio/video PIDs — across the MPEG-TS (legacy and hybrid)
  and HLS (segmenter and direct) pipelines. The gate is a required manual check,
  not part of CI; the generated clips are gitignored and regenerated on demand.

### Fixed

- Support pages parity is now closed for Health, Logs, and Troubleshooting:
  dynamic Support health badges render hydrated warning/error counts, Logs
  supports ErsatzTV-style search, Timestamp/Level sorting, persisted page size,
  and pager navigation, and durable browser coverage exercises the support
  route-family fallbacks.
- Scheduling admin route-family parity is now closed for schedules, blocks,
  templates, decos, and deco templates, with durable browser coverage for the
  hydrated route workflows, editor fallback forms, localized labels, and
  console-clean visual geometry.
- Classic scheduling now preserves imported schedule-item filler preset IDs and
  schedules ErsatzTV-style per-item pre-roll, post-roll, tail, and fallback
  filler around Classic playout items while keeping fill-group state intact.
- Opacity-expression watermarks now loop still image inputs so FFmpeg evaluates
  ErsatzTV clock variables over live direct `.ts` and HLS segment timelines
  instead of freezing the expression at the first image frame.
- HLS Segmenter now enforces a configurable active transcode limit
  (`ffmpeg.segmenter.active_transcode_limit`, default 2) in addition to the
  existing work-ahead limit, so rapid channel switching does not leave many old
  idle sessions spawning FFmpeg processes while the current viewer keeps
  priority.
- HLS Segmenter FFmpeg seeks now preserve sub-second playout and in-point
  offsets through the worker, command builders, direct IPTV stream path, and
  troubleshooting previews instead of flooring to whole seconds before `-ss`.
- The TSDuck playback gate now converts shell paths to host-native paths when
  needed, isolates the server temp directory per run across platforms, waits
  for transient SQLite bootstrap locks, and checks HLS Segmenter audio-pip
  cadence in the decoded capture.
- The Windows launcher build script now consumes the `embed_resource`
  compilation result so the workspace clippy gate stays clean with
  `-D warnings`.
- FFmpeg playback now uses persisted media stream indexes for video and audio
  inputs, fixing watermarked channels whose files store audio before video,
  such as the MTV channel stream failing with a filtergraph stream-specifier
  error.
- HLS Segmenter `StopOnDisconnect` sessions now stop after the configured idle
  timeout even while FFmpeg is still running, segment-file requests refresh the
  session like ErsatzTV, and the MPEG-TS hybrid wrapper waits for initial HLS
  segments before wrapping a restarted session.
- Settings route-family i18n parity is now browser-proven across `en-us`, `pl`,
  `pt-br`, and `nl` for `/settings/ffmpeg`, `/settings/logging`,
  `/settings/hdhr`, `/settings/scanner`, `/settings/playout`, `/settings/ui`,
  and `/settings/xmltv`, with translated headings, labels, save actions, and
  no English fallback leakage outside intentional technical values.
- FFmpeg profile editing now exposes ErsatzTV-style H.264 profile choices,
  persists NVENC `high444p`, and shows the unsupported-format warning when
  NVENC is paired with MPEG-2.
- Legacy MPEG-TS concat streams now stop each internal stream at the active
  `PlayoutItem` boundary, honoring short `InPoint`/`OutPoint` durations so
  `mode=ts-legacy` advances through cyclic playout items instead of continuing
  the same media file past the seam.
- Classic scheduling now avoids immediate repeats across shuffled collection
  reshuffle boundaries and persists continue-build anchors at the last
  generated item finish when a media item overshoots a build hard stop.
- Channel music-video credit subtitles now extract the bundled ErsatzTV fonts
  before playback, so `_default.ass` renders with the matching title style, and
  the `/channels` Troubleshoot action now only appears for enabled channels
  with playouts.
- Windows release builds now discover bundled Leptos hydration assets from the
  `site/pkg` directory next to the server binary even when `LEPTOS_*` runtime
  variables are absent, restoring hydrated admin actions such as watermark
  saves and `/channels` stop-session visibility.
- Watermark placement previews now avoid Windows drive-letter escaping in the
  FFmpeg `drawvg` filter, fixing preview refreshes in the Windows admin build.

## [0.0.8-alpha] - 2026-06-13

### Added

- AV1 hardware-decode fallback now learns unsupported CUDA/NVDEC paths at
  runtime and falls back to software `libdav1d` decode instead of repeatedly
  attempting a known-bad hardware path.
- Dynamic watermark overlays now route through the graphics overlay plan,
  including loop options and NCalc opacity-expression translation for
  ErsatzTV-style watermark timing.
- Music video credit templates now render in the admin UI.
- Workflow review gates now validate parity progress, runtime smoke coverage,
  and completed-prompt hygiene before status work is considered closed.
- ADR 0014 now records the hardware capability fallback contract for FFmpeg
  decode behavior.

### Changed

- Top-gap automation was hardened, then removed with its agent documentation
  once the workflow moved back to explicit curated gap prompts.
- Dependabot updates for `chrono`, `stylance`, `uuid`, and
  `download-artifact` were batched into one dependency-maintenance slice.
- Progress and status documents now narrow completed work to the exact proven
  parity scope and keep unfinished hardware-decode debt visible.

### Fixed

- Smart-collection search now covers the documented ErsatzTV Lucene fields,
  relative date helpers, and nested `smart_collection` expansion used by
  smart collections, fixing the `Music Videos` profile query from 699 to the
  expected 377 results.
- Windows release packaging now prefers Git Bash on the self-hosted runner so
  bash-based release steps receive valid GitHub Actions temp script paths.
- Collections and smart collections now restore the pinned ErsatzTV admin
  layout contract, including the manual add-page save path, no-JS collection
  form fallback, detail-page custom ordering, and browser parity coverage.
- HLS segmenter playlists now keep program-date-time and MPEG-TS continuity
  counters stable across appended FFmpeg process boundaries, fixing the
  parallel live HLS playback freeze debt.
- The four ADR 0014 hardware-decode parity debts are closed in the FFmpeg
  capability and fallback path.
- FFmpeg fake-tool snapshot tests serialize script execution to avoid
  `ETXTBSY` flakes.
- Multiple parity documents were corrected from completed-looking status back
  to `In progress` where open debt remained.

## [0.0.7-alpha] - 2026-06-10

### Changed

- Release packaging documentation now describes the actual web asset and
  archive paths used by the release workflow.

### Fixed

- Docker release builds now use the workspace context and no longer leak host
  target-directory paths into build stages.
- Release jobs run on the intended self-hosted runners, with Linux checks kept
  on Linux hosts.
- Windows target-directory configuration was removed from committed config so
  release builders can use their own platform paths.
- FFmpeg subtitle filter paths are escaped correctly for both parser levels,
  including Windows paths.
- Subtitle burn-in is skipped when a seeked subtitle has no active cues.
- Scanner writes `VideoScanKind` values with ErsatzTV-compatible numbering.
- NVENC and CUDA FFmpeg paths keep still images and CUDA surface tracking on
  the correct software/hardware boundary.
- Platform-dependent FFmpeg snapshots were made Windows-neutral.

## [0.0.6-alpha] - 2026-06-10

### Fixed

- Windows run configuration now reads `.cargo/config.toml`, escapes
  target-directory backslashes correctly, and strips verbatim path prefixes
  from normalized library paths.
- The Windows launcher sets `LEPTOS_OUTPUT_NAME` for the bundled server.

## [0.0.5-alpha] - 2026-06-09

### Added

- FFmpeg and FFprobe path resolution now goes through the `FFmpegLocator` port.
- FFmpeg command generation parity was extended across runtime filter
  requirements, NVIDIA decoder probing, VideoToolbox probing, playback metadata
  options, encoder capability probes, QSV extra frames, HLS PTS offsets, MP4
  flags, realtime still-image filters, HDR/zscale smoke coverage, and
  zscale/libzimg runtime requirements.
- Channels gained a `Stop Transcode Session` action.
- The HLS, IPTV, and streaming runtime paths gained transport-parity coverage
  for direct stream cancellation, HLS segment lifecycle handling, playout
  updates on live time, and FFmpeg stream-failure logging.
- Bundled template presets were added as application assets.
- Release packaging gained web asset reuse for Docker publishing, frontend-only
  web asset builds, lower-memory release settings, and Docker disk-pressure
  reductions.
- CI gained path filtering, run cancellation, and fail-fast behavior to reduce
  wasted runs.
- Daily progress routines, pinned parity baselines, and gap prompt refresh
  documents were added to the progress workflow.
- Cargo workspace with the initial domain, core, infra, app, FFmpeg, web, and
  scanner crates.
- Initial documentation contracts for long-term solo development and future
  public open-source collaboration, including Diataxis docs, ADRs, porting
  contracts, GitHub templates, CI, release scaffolding, dependency-update
  scaffolding, and repo-local agent skills.
- Application artwork in SVG, PNG, ICO, and ICNS formats.
- FFmpeg 8.1 runtime version detection and the initial `axum` plus Leptos SSR
  web host with `/healthz`.
- SQLite startup bootstrap, the collapsed initial schema, repository traits,
  SQLx repository implementations, offline SQLx metadata, and integration
  tests.
- Domain models for configuration, channels, playout, scheduling, media,
  collections, libraries, media sources, FFmpeg profiles, filler, graphics,
  and metadata.
- Persistence design documentation and a persistence gap matrix covering the
  ErsatzTV SQLite tables.
- Real-world ErsatzTV SQLite compatibility coverage gated by
  `ERSATZRS_REAL_WORLD_SQLITE`.
- FFmpeg foundation with typed inputs, command generation, complex filter
  routing, software/NVENC/VAAPI pipeline builders, drawvg SVG template
  rendering, ffconcat playlist writing, and snapshot coverage.
- Scheduling engines and persistence through Phase 3, including block, YAML,
  Lua scripted scheduling, SQL-backed content resolution, smart collection
  search, history parity, and playout build persistence.
- Media source scanning for local files, Plex, Jellyfin, and Emby, including
  ffprobe metadata, provider credentials, provider library bootstrap writes,
  Plex collections, Emby shows/seasons/episodes, and ErsatzTV-compatible media
  file path hashing.
- IPTV and HDHomeRun protocol surfaces, including M3U, XMLTV, HDHomeRun HTTP
  endpoints, UDP discovery, MPEG-TS streaming, and persisted device UUIDs.
- Leptos admin UI foundation, layout/navigation, channel editor, media library
  and browser pages, collection editors, block scheduling UI, and Channels
  visual parity work.
- HLS segmenter hardening, playlist trimming, fMP4 support, direct HLS
  passthrough, and runtime FFmpeg/Docker multi-arch support.
- SQLite or MySQL initialization from `DATABASE_URL` when built with the
  `mysql` feature, ErsatzTV-compatible MySQL migrations, and MySQL integration
  coverage.
- Dependency and CI hardening for `cargo deny`, including CSS and test HTTP
  tooling updates, lockfile refreshes, license policy updates, and
  duplicate-version policy alignment.

### Changed

- The AGPL `evalexpr` dependency was replaced with an in-tree condition
  evaluator.
- Cargo dependency maintenance updated `windows-sys`, `embed-resource`, and
  related Dependabot-managed packages.
- Snapshot review documentation now records that `cargo-insta` is already
  installed.
- Project branding, crates, binaries, Docker wiring, documentation, and
  snapshots were renamed from NasTv/`nastv-*` to ErsatzRS/`ersatzrs-*`.
- Persistence moved to an expanded normalized SQLite schema derived from the
  final ErsatzTV model instead of the earlier hybrid JSON/scalar mapping.
- SQLite repository decoding and writes now use ErsatzTV-compatible integer
  enum values, EF-style UTC `DateTime` text, and ErsatzTV `TimeSpan` parsing.
- FFmpeg runtime detection moved into `ersatzrs-ffmpeg` and is reused by
  `ersatzrs-web`.
- Cargo deny metadata and policy now cover workspace crates and accepted
  transitive license/advisory exceptions.
- Workspace package metadata, lints, binary declarations, Rust version,
  README, `.gitignore`, and SQLite migration path documentation were expanded
  into the current project contracts.

### Removed

- `docs/003_ersatztv_sqlite3_schema.sql` was removed after the normative
  `0001_initial.sql` migration and persistence gap matrix superseded it.

### Fixed

- Release packaging now includes web assets for all archives, repairs packaged
  build targets, and reuses built web assets in Docker publish jobs.
- FFmpeg runtime parity was hardened across NVENC CUDA initialization, CUDA
  overlay filters, required runtime filters, zscale/libzimg requirements,
  VideoToolbox probing, NVIDIA encoder/decoder probing, HEVC options, bitrate
  and frame-rate options, audio output options, GOP spelling, and SD
  colorspace normalization.
- HLS segmenter transport parity now preserves fMP4 init segments and closes
  segmenter transport gaps.
- Streaming now keeps HLS playout updates on live time.
- Scheduling persists continue-build anchors.
- Cargo deny policy was updated for `RUSTSEC-2026-0173`.
- Web stream-seek test assertions were corrected.
- Admin libraries and Plex source buttons are hydrated islands with stable DOM
  structure so click handlers do not fall back to inert SSR markup.
- Admin shell sidebar visual parity was tightened around logo placement, top
  app bar width, sidebar surface color, and default collapsed groups.
- Template repository upserts no longer write placeholder child rows that
  violate normalized foreign keys.
- Docker health checks no longer depend on `wget`.
- Web startup tracing, listen address handling, UTF-8 FFmpeg output validation,
  and FFmpeg version test coverage were tightened.

[Unreleased]: https://github.com/bartdeijkers/ErsatzRS/compare/v1.1.0...HEAD
[1.1.0]: https://github.com/bartdeijkers/ErsatzRS/compare/v1.0.37-beta...v1.1.0
[1.0.37-beta]: https://github.com/bartdeijkers/ErsatzRS/compare/v1.0.35-beta...v1.0.37-beta
[1.0.35-beta]: https://github.com/bartdeijkers/ErsatzRS/compare/v1.0.34-beta...v1.0.35-beta
[1.0.34-beta]: https://github.com/bartdeijkers/ErsatzRS/compare/v1.0.33-beta...v1.0.34-beta
[1.0.33-beta]: https://github.com/bartdeijkers/ErsatzRS/compare/v1.0.32-beta...v1.0.33-beta
[1.0.28-beta]: https://github.com/bartdeijkers/ErsatzRS/compare/v1.0.27-beta...v1.0.28-beta
[1.0.27-beta]: https://github.com/bartdeijkers/ErsatzRS/compare/v1.0.26-beta...v1.0.27-beta
[1.0.26-beta]: https://github.com/bartdeijkers/ErsatzRS/compare/v1.0.25-beta...v1.0.26-beta
[1.0.25-beta]: https://github.com/bartdeijkers/ErsatzRS/compare/v1.0.24-beta...v1.0.25-beta
[1.0.24-beta]: https://github.com/bartdeijkers/ErsatzRS/compare/v1.0.23-beta...v1.0.24-beta
[1.0.21-beta.1]: https://github.com/bartdeijkers/ErsatzRS/compare/v1.0.21-beta...v1.0.21-beta.1
[1.0.21-beta]: https://github.com/bartdeijkers/ErsatzRS/compare/v1.0.17-beta...v1.0.21-beta
[1.0.17-beta]: https://github.com/bartdeijkers/ErsatzRS/compare/v1.0.15-beta.9...v1.0.17-beta
[1.0.15-beta.9]: https://github.com/bartdeijkers/ErsatzRS/compare/v1.0.15-beta.8...v1.0.15-beta.9
[1.0.15-beta.8]: https://github.com/bartdeijkers/ErsatzRS/compare/v1.0.15-beta.7...v1.0.15-beta.8
[1.0.15-beta.7]: https://github.com/bartdeijkers/ErsatzRS/compare/v1.0.15-beta.6...v1.0.15-beta.7
[1.0.15-beta.6]: https://github.com/bartdeijkers/ErsatzRS/compare/v1.0.15-beta.5...v1.0.15-beta.6
[1.0.15-beta.1]: https://github.com/bartdeijkers/ErsatzRS/compare/v1.0.14-alpha...v1.0.15-beta.1
[1.0.14-alpha]: https://github.com/bartdeijkers/ErsatzRS/compare/v1.0.13-alpha...v1.0.14-alpha
[1.0.13-alpha]: https://github.com/bartdeijkers/ErsatzRS/compare/v1.0.12-alpha...v1.0.13-alpha
[1.0.12-alpha]: https://github.com/bartdeijkers/ErsatzRS/compare/v1.0.11-alpha...v1.0.12-alpha
[1.0.11-alpha]: https://github.com/bartdeijkers/ErsatzRS/compare/v1.0.10-alpha...v1.0.11-alpha
[1.0.10-alpha]: https://github.com/bartdeijkers/ErsatzRS/compare/v0.0.9-alpha...v1.0.10-alpha
[0.0.9-alpha]: https://github.com/bartdeijkers/ErsatzRS/compare/v0.0.8-alpha...v0.0.9-alpha
[0.0.8-alpha]: https://github.com/bartdeijkers/ErsatzRS/compare/v0.0.7-alpha...v0.0.8-alpha
[0.0.7-alpha]: https://github.com/bartdeijkers/ErsatzRS/compare/v0.0.6-alpha...v0.0.7-alpha
[0.0.6-alpha]: https://github.com/bartdeijkers/ErsatzRS/compare/v0.0.5-alpha...v0.0.6-alpha
[0.0.5-alpha]: https://github.com/bartdeijkers/ErsatzRS/releases/tag/v0.0.5-alpha
