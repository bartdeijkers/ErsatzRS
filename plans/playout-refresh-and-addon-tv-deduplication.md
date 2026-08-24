# Classic Refresh Continuity and Add-on Television Deduplication

## Status

Corrective plan registered on 2026-08-23 from redacted Windows dogfood. It owns
milestones `crf-01`, `crf-02`, and `amm-h-01`. No runtime code or production
database mutation is part of this registration.

## Outcome

Close two independently caused correctness failures before the next release:

1. A membership-changing managed Remote Stream scan must refresh only dependent
   playouts, and a relevant Classic Refresh must continue the uninterrupted
   sequence without replaying already-aired media.
2. Publishing or refreshing an add-on television list must persist each logical
   artwork candidate once and return each projected episode once, even while a
   legacy database still contains exact duplicate artwork rows.

The tracks share one plan because both were exposed by the same Windows
acceptance session and both require safe repair of persisted state. Their code
changes and tests remain separable.

## Confirmed Failures

### Classic Refresh can rewind shuffled media

- Two membership-changing managed Remote Stream scans enqueued `Refresh` for
  every playout that had a `PlayoutProgramScheduleAnchor`, including an
  unrelated library-scoped Classic playout.
- The audited database contained 26 program-schedule anchors and all were
  undated continuation anchors. The production Classic build path writes that
  state, while `retained_refresh_anchors` correctly accepts only a dated
  checkpoint aligned with the rebuild day.
- The existing checkpoint regression manually injects the dated row. It does
  not prove that a fresh or multi-day production build ever creates one.
- Refresh therefore had no restorable state after clearing the generated
  window. A deterministic shuffle restarted and media aired the previous day
  appeared again in the next playout window.
- Periodic scans whose membership was unchanged did not enqueue Refresh. The
  trigger defect is the global impact query after a real delta, not the timer.

Relevant implementation surfaces:

- `crates/ersatzrs-core/src/scheduling/classic/mod.rs`
- `crates/ersatzrs-infra/src/maintenance.rs`
- `crates/ersatzrs-scanner/src/lib.rs`
- `docs/adr/0020-playout-rebuild-on-impacting-changes.md`

### Add-on television rows are multiplied by artwork

- The audited projection contained 27 unique Remote Stream media identities and
  27 unique Episode records. The import did not create duplicate episodes.
- Every corresponding `EpisodeMetadata` record had two identical thumbnail
  `Artwork` rows. The television query consequently expanded 27 logical
  episodes to 54 result rows; two seasons expanded from 13 and 14 logical rows
  to 26 and 28 displayed rows.
- `addon_media_list.rs` first converts `metadata.artwork` and then pushes
  `artwork_path` or the first `artwork_url` as another thumbnail without an
  equivalence check. `metadata::replace_artwork` persists every candidate.
- `television.rs` joins all thumbnail rows without selecting one, so one bad
  metadata invariant becomes a visible duplicate item. A second projection
  showed the same pattern, making this systemic rather than list-specific.

Relevant implementation surfaces:

- `crates/ersatzrs-infra/src/repositories/addon_media_list.rs`
- `crates/ersatzrs-infra/src/metadata.rs`
- `crates/ersatzrs-infra/src/repositories/television.rs`
- `plans/addon-list-artwork-and-metadata-projection.md`

## Repair Invariants

- Refresh is continuity-preserving. It may re-stamp the active window but must
  not silently behave as Reset or replay consumed shuffled media.
- The destructive clear happens only after all required checkpoint state has
  been validated or safely bootstrapped.
- A scanner reports concrete membership deltas. Impact resolution follows
  schedule and collection dependencies; it never treats anchor ownership as a
  dependency proxy.
- One logical episode is one repository result regardless of artwork
  cardinality. Producer idempotency and consumer cardinality are both enforced.
- Artwork repair removes exact duplicates only. Different kinds, paths, URLs,
  language variants, and operator replacements remain intact.
- Existing databases are tested on a copy and backed up before any corrective
  migration or maintenance action is offered to operators.

## Track A — Real Classic Refresh Checkpoints (`crf-01`)

1. Add a failing production-shaped regression that performs a fresh multi-day
   Classic build, persists it through the real repository, restarts, and issues
   Refresh without manually seeding any anchor. Compare the post-boundary media
   identities and scheduling contexts with an uninterrupted baseline.
2. Define the checkpoint moment from the pinned ErsatzTV builder: persist dated
   per-source enumerator/shuffle state at each rebuildable day seam, while
   retaining the undated end-of-horizon anchor solely for Continue.
3. Persist checkpoints transactionally with generated items and the main
   playout anchor. Multiple schedule items that resolve the same content source
   must share or distinguish state exactly as the scheduling key requires.
4. Make Refresh preflight the full checkpoint set before deleting rows. Reject
   stale, partial, wrong-day, duplicate, or incompatible context versions with
   one actionable diagnostic.
5. Handle legacy databases deliberately. Bootstrap only when the earliest
   surviving item/context and collection revision prove a deterministic state
   at the rewrite boundary. Otherwise leave the current window untouched,
   surface a failing Classic Playout State health result, and require an
   explicitly confirmed fresh Reset; never infer Reset from missing data.
6. Reconcile membership changes against stable scheduling identity. An added or
   removed candidate must not make a raw saved index point at a different
   already-consumed item. If upstream parity cannot represent this safely, amend
   ADR 0020 before changing the behavior contract.
7. Cover Shuffle, Random, RandomRotation, duplicate references, day boundaries,
   restart, settings re-stamping, and the admin Classic reset mapping on SQLite
   and MySQL.

## Track B — Dependency-scoped Scan Invalidation (`crf-02`)

1. Change scan reconciliation to return whether membership changed plus the
   affected library and stable media identities. Preserve the no-op path for an
   unchanged scan.
2. Replace the global `PlayoutProgramScheduleAnchor` query with dependency
   resolution through program-schedule items and their collection,
   multi-collection, smart-collection, rerun, playlist, and library sources.
   Reuse the same dependency semantics as schedule resolution instead of adding
   a scanner-only approximation.
3. Enqueue one coalesced request per impacted playout after the scan transaction
   commits. Record a bounded reason such as library membership added, removed,
   replaced, or re-identified; never enqueue per item.
4. Add an integration matrix with two unrelated managed libraries, a directly
   scoped collection, an indirect smart/multi-collection dependency, and a
   playout with no dependency. Only the provably affected set may change.
5. Combine the matrix with Track A's restart regression so a relevant refresh
   proves uninterrupted order and an unrelated scan proves byte-for-byte stable
   playout rows and anchors.
6. Extend health and diagnostics so a failed dependency resolution or refused
   continuity preflight cannot look like a successful refresh.

## Track C — Idempotent Artwork and Cardinality-safe TV Reads (`amm-h-01`)

1. Capture the producer regression with one provider artwork candidate whose
   local materialized path is also exposed through `artwork_path`. Assert one
   normalized thumbnail after repeated Publish and refresh cycles.
2. Centralize artwork identity as `(metadata owner, kind, normalized source)`.
   Normalize local paths and URLs consistently on all supported platforms, add
   the fallback thumbnail only when no equivalent candidate exists, and retain
   distinct valid variants.
3. Deduplicate again inside `replace_artwork` so every current and future caller
   receives an idempotent replace operation. Add a database uniqueness guard
   only if its SQLite/MySQL and path-normalization semantics match the contract;
   do not rely on a constraint as the sole defense.
4. Make television list queries choose one deterministic preferred thumbnail
   per metadata record, for example through a ranked/correlated selection. Do
   not hide the issue with a broad `DISTINCT` that can merge genuinely different
   episodes or produce arbitrary artwork.
5. Add a transactional repair for legacy data. Identify duplicates by metadata
   owner, artwork kind, and canonical source; keep the lowest stable row, delete
   only exact extras, preserve every non-equivalent variant, and log aggregate
   counts without titles or private paths. Provide dry-run/audit evidence before
   applying it to an operator database.
6. Extend health with duplicate-artwork and projected-row-cardinality checks so
   the same invariant cannot regress silently.
7. Prove logical list and season counts, pagination, item links, thumbnails, and
   localized empty/loading/error states in repository and browser tests. The
   27-item fixture must remain 27 rows with season counts 13 and 14 before and
   after repeated publication, repair, and restart.

## Verification and Release Gate

- Focused scheduler, maintenance, scanner, metadata, television repository, and
  migration tests pass on SQLite and MySQL.
- Health regressions detect missing/invalid checkpoints, refused Refresh,
  unresolved scan impact, and exact duplicate artwork.
- Browser coverage proves television list counts and links in Chromium and
  Firefox without hydration or console errors.
- A redacted copy of the Windows dogfood database is audited first, backed up,
  repaired, restarted, and audited again. No live database is mutated during
  development or documentation work.
- The final implementation updates `[Unreleased]`, the four parity files, ADR
  0020 implementation evidence, milestone evidence, and the session log.
- Before release, run both Clippy configurations, test closeout, Leptos build,
  hydration lint, web parity, progress validation, migration checks, and runtime
  smoke. Playback-sensitive checkpoint changes also run the TSDuck manual gate.

## Completion Order

1. `crf-01`: make Refresh intrinsically continuity-safe.
2. `crf-02`: narrow scan invalidation and prove the complete trigger path.
3. `amm-h-01`: producer deduplication, defensive television reads, and safe
   existing-data repair. This track may be implemented independently but must
   close before the next release.
