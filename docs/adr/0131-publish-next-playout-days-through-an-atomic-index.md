# ADR 0131: Publish Next playout days through an atomic index

## Status

Accepted 2026-11-06. The maintainer authorizes local implementation and tests.
Commit/push, GitHub CI, runtime publication, installation and releases retain
their separate authorization boundaries. Runtime candidate work is now authorized locally under accepted ADR0130.

## Implementation

Implemented within the required scope: daily planning/index publication, accepted-SQL recovery,
callback ownership and reader identity admission are implemented and verified.
Startup binds playback and preview to the qualified indexed owner before
creating application ports. Older readers retain complete-file publication and
compatible schema1.2; ordinary hosts use1.3, and the indexed host uses schema1.4
with its separately owned indexed directory. The twelve-patch Next025 recipe
is `scripts/release/next-patches/v0.2.5/` and is now published, checksum-verified
and adopted by the four required managed pins.

A prior-candidate negative showed an overlay could downgrade indexed ownership.
The qualified reader rejects layout/folder replacement while permitting ordinary
folder overlays and indexed normalization overlays. Exact replay,33 affected
reader/config tests and strict lint pass. Linux and native Windows prove real
reader leases, unchanged index/day contents and mtimes, active replacement,
selected Dutch audio, normal/preview isolation, callback/SQL compensation,
finite controls and bounded shutdown. Windows additionally passes13 host checks
and35 publication/recovery cases. Independent decoding verifies the retained
post-edit outputs.

Current Linux/Windows full-host Native/Next playback and managed cold/cache
startup pass with published FFmpeg2.0.10/Next025. Native93 and Next Copy86
transport checks,20 headed browser cases,8483 composed workspace cases, strict
lint, static/dependency gates and canonical doctests pass within their recorded
source scopes. Four independently extracted application packages pass startup,
packaged readiness, bundled identity and served asset checks. ARM64/ARMv7 use
QEMU; native ARM and other approved unavailable hardware/macOS remain unverified.

Source-bound full CI recovery42460252936 and eligible PR CI42464500817 pass;
PR125 delivers the implementation into main as10ffff4e. Ordered window
acceptance and the accepted baseline advance are recorded in milestones
`ucu-13/14/15` and docs/automation/parity-baseline.md. Earlier failures and their
scoped recoveries remain retained. Milestone `ucu-15` owns exact component
identities and proof; historical `tmp/ucu14/` paths resolve beneath
`tmp/evidence/ucu13-15-v26.12.0/raw/`. Native ARM, macOS and approved hardware
debt remain unverified; no application release or installation is claimed.

## Context

UCU-15 row278 at ErsatzTV/legacy
`fbb3685b40cdc55a8c297de3364048e1b8765b15` adds dependent refresh and a daily
publication planner. The planner ignores items finished more than five minutes
ago, leaves unchanged days untouched, updates changed far-future days, and
changes the visible generation for active-horizon edits. Its complete-generation
boundary uses directory symlinks/junctions.

ADR 0095 deliberately uses a single broad-window JSON file with same-directory
atomic replacement on all platforms. The application holds publication/source
admission through replacement, cancellation and deadline checks. A scheduler
build additionally retains the old file until SQL commit succeeds, restoring it
on failure. These safety properties remain mandatory.

The current publisher serializes and flushes every complete manifest. Adding
whole-file byte equality would avoid identical refreshes, but editing a future
day would still rewrite all unchanged days. It cannot establish the existing
unchanged-day acceptance requirement. Conversely, independently replacing daily
files could expose mixed active generations and invalidate SQL compensation.

The fixed Next loader at
`92e93d72c0e3694417a22207374b9d12cbf38087` scans a folder for a filename whose
time bounds contain its selection clock, then reads that file. It has no atomic
index contract. A reader extension is therefore necessary for the design below;
it is not an existing upstream capability or an accepted runtime correction.

## Decision

### Retain one atomic visibility boundary

Amend only ADR 0095's playout-file representation. Keep SQLite authoritative,
the existing source/admission locks, bounded blocking filesystem work,
cancellation/deadlines, Windows sharing retries and reversible SQL/file
publication. Preserve Native as a supported selectable engine, all stored
engine/profile choices, international audio/subtitles, teletext and callback
isolation. No new service, database, scheduler or background coordinator is
introduced.

Replace the complete visible manifest with a small, versioned index in the
same channel-owned directory. The index names immutable, content-addressed
daily playout JSON files with explicit half-open time bounds. Publish changed
day files completely before replacing the index with the existing atomic-file
mechanism. An index references a complete accepted generation; incomplete or
unreferenced staged files are never discoverable as playouts.

### Preserve daily write economy and playback semantics

Partition by the configured schedule's local civil-day boundaries, including
DST and cross-midnight items. Retain each item's original timestamps, source
seek, selected tracks, graphics and stable occurrence identity across any
day-file duplication. Day membership must cover gaps and mirrors as well as
ordinary persisted items; file bounds never alter programme timing.

Use deterministic serialization and compare live day content with the fixed
upstream five-minute finished margin. An identical day retains its existing
file and metadata. A refresh with identical live content performs no playout
or index write. Trimming only finished items does not force publication.
Changed days receive new immutable files; publishing their small index leaves
unchanged day files untouched even for active-horizon changes. Empty schedules
explicitly withdraw the previous visible schedule through the index.

### Make the reader contract explicit

Extend only the separately qualified Next fork's playout loader to negotiate
and read the versioned index. Keep ordinary upstream folder loading available
outside this explicit mode. The application must require the index capability
before launching an indexed channel; a version string alone is insufficient.
An unsupported or malformed configured index fails visibly, without silently
scanning staged, obsolete or unrelated files.

The loader reads one complete index snapshot per selection and opens only the
matching referenced day. Validate version, unique non-overlapping bounds,
relative owned filenames, content identity and bounded sizes/counts. Reject
absolute paths, traversal and symlink escapes. Diagnostics omit credentials
and serialized media sources. Preserve gap/fallback, invalid-item and
troubleshooting end-of-playout behavior.

### Keep rollback and reclamation safe

For a private SQL build, stage immutable day files and the candidate index
while retaining publication/source admission. Replace the index reversibly;
accept it only after SQL commit, or restore the exact prior index on failure.
Never reclaim files reachable from either the accepted or rollback index.
Cancellation, expiry and failed replacement keep the prior visible generation.
Startup regenerates derived state from accepted SQL before starting Next.

Reclamation is part of this implementation, not a later waiver: bound owned
staging/obsolete storage and remove only unreachable task-owned chunks. Prove
the index-read/chunk-open race and bounded reader retry against concurrent
publication/reclamation; do not delete an old reader's required file and assume
the race is harmless. Preserve open-handle behavior on native Windows. If the
race cannot be solved within existing ownership and bounded retries, stop for
a revised design rather than shipping unbounded retention or weakened atomicity.

### Local implementation constraints

Use the existing `ProgrammeClock` for civil days. Bound the index to366 days
and128 KiB, each serialized day to32 MiB and total daily document bytes to256
MiB. Refuse excess input without truncating it or replacing the accepted index.
Bound directory preflight to4,096 entries and preserve unrelated filenames.
These are explicit error budgets, not a claim that oversized input is supported.

The publisher/source admission serializes writers and private SQL compensation.
Readers hold a shared `.index-readers.lock` lease through index/chunk reads;
reclamation takes its exclusive lease before deleting unreachable chunks.
Contention returns Busy to the existing bounded retry path. No reader lease is
held across staged SQL or index replacement. Before deleting any chunk, scan
for retained `.next-rollback-` evidence; its presence requires recovery from
accepted SQL and prevents cleanup/publication. Recoverable stale staging is
removed only inside the owned indexed directory. Qualification must prove the
same lease protocol in the actual Next reader and safe startup recovery.

Startup uses the coordinator's first committed read and holds its publisher/
source admission. It takes the exclusive reader lease through repair and index
replacement. Old index contents never select the recovered schedule. Correct
chunks are reused; missing/corrupt regular files are repaired atomically.
Retained rollback evidence is retired only after the accepted-SQL index is
visible. Private SQL preparation cannot bootstrap or repair this cache. A
reused owner's ordinary committed refresh can also recover lost/corrupt state;
invalid private input does not invalidate an otherwise healthy accepted cache.
Post-publication cleanup failure leaves referenced chunks intact and startup
unready until a committed-read retry succeeds. Bound recovery's owned disk
footprint to three256-MiB document generations plus four128-KiB index buffers,
including headroom for the index replacement. Excess input/unsafe objects fail
closed; source SQL and unrelated files remain intact.

The existing blocking publication worker registers exact gaps at visible commit,
before recovery cleanup and before releasing channel admission. Registration
therefore survives cancellation of its async awaiter. A fallback callback waits
for that same owner before checking the active instance and admitted interval;
the wait shares its existing30-second resolution budget and holds no context,
gap or source lock. Private SQL publication keeps admission through its existing
commit/compensation and subsequent registration. Finite troubleshooting previews
retain their independent manifests and admission. Callback input never creates
a publisher, and unrelated channels remain independent.

### Require behavioral acceptance

Use the existing publisher/coordinator/finalizer and actual playback fixtures.
Required local proofs include:

- Stable file identity/mtime and zero writes for unchanged live days; changed
  active and far-future days write only their new chunks and the small index.
- Finished-only trimming, empty withdrawal, local DST/day seams, mirrors,
  cross-midnight items, deterministic IDs and callback/source capability expiry.
- Concurrent readers see complete old or new accepted data; malformed/unknown
  indexes and escaping/oversized references are refused visibly.
- Cancelled/expired/permanently failed publication, transient Windows sharing
  failures, SQL commit failure and retry preserve exact accepted state, owned
  locks and bounded reclamation.
- Actual decoded Next playback through an active edit, selected international
  tracks, normal/preview isolation and owned shutdown. Reuse unaffected Native
  evidence; retain any required affected Native and final gates.
- Linux x64 and native Windows x64 proof, plus required ARM builds/packages/
  emulation. Existing unavailable native ARM/hardware and macOS debt remains
  explicitly unverified; no macOS or paid work is authorized.

This decision does not accept row278 or any UCU window by itself. Milestone
`ucu-15` retains every acceptance item. The Next extension needs its own source,
patch, regression, checksum and package identity under ADR 0130, plus separately
authorized public publication/managed adoption before final acceptance.

Record the extension's removal condition in the runtime patch manifest: retire
it only when an adopted unpatched upstream supplies equivalent atomic daily
publication, capability checks, rollback-compatible reading and unchanged
regressions. Preserve those tests. Do not remove it because patch context changes.

## Consequences

Unchanged days avoid serialization-to-disk and flushing while the index retains
one small atomic visibility/compensation boundary on every required platform.
No junction privileges or directory-link swap are needed. The cost is an
explicit Next reader extension and bounded immutable-file lifecycle, both of
which need runtime and concurrency proof before acceptance. Current packages
remain valid inputs for unrelated checks, not indexed-reader acceptance.

## Alternatives Considered

- Whole-file byte equality: useful for identical complete schedules, but does
  not satisfy unchanged-day write economy after one day changes.
- Per-day replacement without an index: cannot preserve complete-generation
  visibility and SQL rollback across multiple changed active files.
- Upstream symlink/junction generations: require changing the accepted portable
  atomic-file strategy and introducing a second platform-specific swap path;
  the fixed upstream Windows helper removes the existing junction before
  creating its replacement. This does not preserve the present visibility
  guarantee without additional platform work and proof.
- Filesystem reflink/block cloning: not a portable required-platform contract
  and does not establish unchanged day-file identity.
- Unbounded immutable generations: fails the bounded storage/ownership contract.

## References

- [ADR 0095](0095-native-pipeline-instead-of-ersatztv-next-engine.md)
- [ADR 0130](0130-qualify-fixed-ucu-runtime-successors.md)
- [UCU acceptance and evidence](../progress/milestones.toml)
- [Fixed source map](../../plans/upstream-catch-up.md)
- `$ERSATZTV_SRC/ErsatzTV.Application/Playouts/NextPlayoutFolderPlanner.cs`
- `$ERSATZTV_NEXT_SRC/crates/ersatztv-channel/src/playout_loader.rs`
