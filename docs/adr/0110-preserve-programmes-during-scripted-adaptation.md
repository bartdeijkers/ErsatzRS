# ADR 0110: Preserve programmes during Scripted adaptation

## Status

Accepted (2026-10-02) — implements the maintainer-approved finding 06 solution.

## Implementation

Partial — finding 06 has local feature acceptance in `v111-findings-06`.
The linked Classic deferred-save and midnight-overlap debt remains owned by
findings 11/12; this ADR cannot claim complete implementation until that work
is accepted. Stored-kind admission, transactional refresh-intent publication
fencing, versioned Scripted boundary checkpoints, runtime source capture and
bounded successful-scan comparison/admission are implemented locally. Candidate
source revalidation fences unprocessed receipts and concurrent managed-library
work; transient publication races retain intent with durable retry.
Refresh retains the active programme group and restores its source rotation
before replacing the future. Missing or incompatible checkpoints are refused
and reported by Health. The playout list now projects durable source revision,
last successful build, pending comparison/Refresh and failures. Explicit Scripted
Reset can retain the current group and start new future rotation, with a persisted
history cutoff preventing later Refresh from reviving pre-Reset history. Classic
also captures programme checkpoints and selects the complete boundary for explicit
preservation, retaining current rows/settings/history during Refresh or Reset.
Its day passes no longer rewind into programmes crossing midnight. Explicit YAML
Reset also retains programmes and excludes archived history from later source
restoration. YAML now captures source and instruction checkpoints, including
remaining Count/All iterations and absolute timed-instruction targets. Their
atomic publication and restoration pass production SQLite restart, legacy
database upgrade and unchanged-future comparisons. Localized Health guidance
reports missing or incompatible state. Block now uses history checkpoints for
explicit Refresh/Reset preservation, retaining complete guide intervals and
starting fresh Reset history at the retained boundary. Randomized and ordered
sources resume correctly after restart; localized Health rejects incompatible
boundary state. Next snapshot publication acknowledgements now gate build success
and durable intent completion, sharing the committed phase of the build lock.
Connected builds now retain complete programmes beyond the producer prefetch
horizon, including configured leading mirrors, and publish before that horizon
can reach the replacement boundary. Focused scheduler/database checks pass;
decoded native and Next HLS producer handover now passes with Classic and
Scripted synthetic programme openings and old/new audio. Internal schedulers now
have localized explicit mode controls with preservation enabled by default;
matched Chromium/Firefox checks cover actions, cancellation and responsive
English/Dutch rendering. Explicit Classic Continue refuses incomplete
continuation state instead of silently promoting the operator's choice to Refresh.
Cancelled Next preparation now revokes replacement permission; the blocking
publisher checks it again after serialization and flush while retaining its
admission locks until cleanup. The preserving-build finalizer described below
compensates completed replacements before SQL commit; ordinary publication
does not undo a completed replacement or establish cross-resource atomicity.
SQL publication now exposes an owned preparation transaction. Its candidate
reader includes assigned item IDs, audio/subtitle selection and graphics
attachments while ordinary readers retain the committed timeline. The normal
repository apply path prepares and immediately commits. Host-owned scheduling
now stages active Next projections between those steps; standalone repository
callers retain the immediate-commit behavior.
The channel publisher can now stage a file from that private candidate while
holding publisher admission and other source read locks. Focused tests cover
source/mirror clocks, abandonment and preparation timeout with a simple wire
projector. Candidate reads now capture item overlay variables and active template
selection through their own transaction. Local and Remote Stream projections use
the shared playback selectors with those prospective inputs, preserving precedence
without loading unpublished item IDs through the ordinary pool. Ordinary host
refresh and candidate preparation now share the full projector. A prepared host
owner retains the staged file and registers fallback gaps only after publication.
Focused integration proof covers selected audio, embedded bitmap subtitles and
canvas publication from uncommitted IDs. Active preparation groups main/preview
owners by their shared channel publisher, includes active mirrors, rejects
conflicting path replacements and drops earlier staged files when a later owner
cannot prepare before the deadline. Web composition passes its existing host
through scheduler options into manual, periodic and startup-repair builds.
Preparation errors/cancellation roll back the candidate. Programme-preserving
builds now replace files reversibly before committing SQL, then notify remaining
readers. Explicit replacement and append-only builds without a retained-prefix
deadline keep SQL-first ordering because their new IDs may be selected immediately. Already
published Next channels do not receive redundant refresh requests, including
their preview owners sharing the same manifest.
The worker retry test proves unchanged timeline/continuation state on preparation
failure and fresh source/mirror publication on retry. A leading mirror's unbounded
read is clamped to the sortable four-digit SQL date range. Final replacement
retries transient I/O and Windows sharing errors with the same prepared bytes
and retained publisher admission: at most eight attempts, 1.775 seconds of
backoff, and no extension of the original deadline or cancellation ownership.
Portable injected-failure checks pass; the real Windows sharing-handle recovery
test still requires execution on Windows. An owned blocking finalizer retains
the source build hold and publisher/read guards across caller cancellation.
Each preserving file replacement retains its exact previous bytes. File failure,
cancelled commit admission or SQL commit failure restores the already replaced
group and leaves the SQL candidate uncommitted. Successful SQL commit disarms
all file compensation before registering fallback gaps.
The safe boundary includes five seconds per configured Next publisher plus two
seconds for the SQL decision. Finalization subtracts this recovery reserve from
the candidate deadline, keeping bounded file restoration ahead of producer
admission. Native-only builds retain their existing boundary. Storage failure
which also prevents restoration is reported, retains rollback bytes when
possible and requires accepted-SQL republication; it is not acknowledged as a
successful build. Startup reprojects accepted SQL before launching Next.
Cancelled automatic Refresh keeps its durable intent pending with bounded
retry backoff; cancellation must not turn recoverable work into terminal failure.
The real Next Scripted producer now passes post-replacement cancellation,
exact SQL/file restoration and fresh retry without restarting playback, with a
complete programme opening and old/new audio decoded from the full capture.
No database/filesystem atomicity or uninterrupted storage-failure guarantee is claimed.
Decoded Block/YAML handover also passes on both producers, including reversible
Next cancellation/retry. Separate native/Next HTTP smokes, the TSDuck gate,
workspace closeout, strict lint, dependency audit and final matched Leptos bundle
verification pass. Producer tests alone are not HTTP delivery or Windows proof;
packaged Windows and other-platform release acceptance remain separate.
External JSON reconciles an accepted baseline with the supplied absolute schedule.
Versioned SQLite storage, shared playback/guide readers and owned worker
publication have focused migration, restart, deadline, reader and action proof.
External operator controls and Health readiness have focused Rust and matched
Chromium/Firefox proof. Native external handover has decoded opening/audio proof
without changing authored programme clocks or materializing timeline rows.
The existing external reader now rejects non-positive programme durations.

## Context

The existing Scripted Refresh replaces the current programme. Automatic
Continue has a separate append-only contract under ADR 0109. Library changes
must reach future programmes without resetting series rotation or interrupting
the programme already being watched.

The existing admin reset action also trusts a caller-supplied scheduler kind.
Scan refresh intents already persist generations and coalesce requests, but
acknowledging a generation after publication does not prevent an older build
from publishing after a newer source change.

## Decision

- Resolve scheduler identity from persisted playout state. Expose explicit,
  validated rebuild modes and a preserve-current-programme option; an implicit
  kind-to-mode mapping is retained only for the compatible legacy action.
  Coalesce the entire explicit operator choice, including preservation. The
  latest explicit queued choice wins; automatic work cannot replace it. Keep
  the existing strongest-mode rule when coalescing automatic requests alone.
  Explicit Continue preserves the existing timeline and cannot silently promote
  to Refresh when saved Classic continuation state is incomplete. Refuse it with
  guidance to choose a preserving Refresh or Reset; automatic legacy recovery
  retains its existing promotion policy. The internal-scheduler dialog defaults
  to preserving Refresh and explains the effect of each mode before admission.
- External JSON remains externally timed: never manufacture a programme seam by
  trimming media or shifting external timestamps. Continue must append an exact
  adjoining suffix without changing accepted items; pruning old items from the
  source file does not erase the accepted past. Preserving Refresh/Reset retain
  complete programmes through a shared absolute boundary beyond the protected
  producer horizon. Both adopt the supplied future; Reset cannot reset rotation
  owned by the external author. Refuse absent baselines, incompatible boundaries,
  or guide visibility changes that would alter retained programmes. An empty
  candidate may withdraw the unprotected future. Reject non-positive durations
  because they cannot form a progressing timeline. These rules deliberately
  strengthen upstream's no-op external builder; they do not authorize edits to
  the source file or insertion of external programmes as library items.
- Store an explicitly accepted external document in the additive
  `PlayoutExternalJsonSnapshot` table, keyed to its playout with cascade deletion.
  Playback, mirrors, guide and programme graphics share this accepted reader.
  The migration creates no snapshots: existing direct readers retain file-driven
  updates until an explicit action establishes controlled publication. Once a
  baseline is accepted, source edits or missing files cannot bypass it. A corrupt
  or unsupported accepted version fails closed instead of reading an unrelated
  live document. Replacing a revision requires the captured revision, stored
  External JSON scheduler kind and selected source path still to match inside
  the SQLite writer transaction. Check the safe publication deadline before
  preparation, after writer admission and before commit. Bound serialized
  documents to 32 MiB while encoding, as well as in the schema. Persist the
  original absolute start and durations without materializing library items.
- Route explicit external actions through the existing owned maintenance worker,
  before the no-op core scheduler. Core admission accepts the explicit policies,
  but the no-op engine refuses to claim their execution. Unadopted automatic
  builds retain legacy direct-file semantics. After adoption, ordinary Continue
  appends only; scheduled Reset also preserves the accepted current programmes.
  Missing/null sources are read failures, not an instruction to erase the future;
  an explicit empty document expresses withdrawal. No-change checks retain the
  revision. Preserving publication observes the same prefetch and leading-mirror
  deadline as internal schedules and never restarts native producers. Explicit
  replacement notifies native owners, including on retry after a notification
  failure. Do not claim successful external publication for a Next channel or
  mirror: its existing schedule projection does not consume external documents.
  The operator list distinguishes direct-file reading from controlled updates
  and displays the accepted revision. Before adoption, the dialog explains the
  one-time non-preserving replacement; Continue and preserving submission are
  unavailable without a baseline. Health reports direct mode as information,
  source-path changes or an exhausted horizon as warnings, and incompatible
  accepted state or Next consumers as failures. Diagnostics omit source paths
  and provide localized recovery guidance. An explicit non-preserving Refresh
  or Reset may replace an unreadable document using its existing revision fence;
  preserving and automatic actions continue to refuse it.
- An explicit Scripted Reset with programme preservation keeps the published
  group and its history, then starts a new rotation at that group's end. Store
  fresh state at the retained boundary, including a history cutoff inherited
  by later checkpoints. Refresh and Continue must not resurrect pre-Reset
  history, including for sources first registered later in the script. Legacy
  checkpoints without this optional cutoff retain their existing semantics.
- Classic captures full source and scheduler state at programme boundaries,
  including resumable Multiple and Flood iterations. Explicit preserving
  Refresh validates and restores the exact current-programme boundary; Reset
  starts fresh state there, including ordered/shuffled fill-group progress,
  while retaining the current group. Legacy Refresh
  continues selecting the earliest checkpoint on its rebuild day. Keep the
  complete checkpoint and dated-source validation for both paths, and make
  preserve-current readiness part of Classic Health. The daily build limit is
  not a cut: advance the next pass beyond any programme emitted across midnight.
- Explicit YAML Reset starts fresh rotation at the retained programme boundary.
  Keep a serialized history cutoff through Continue, including when a source or
  marathon first becomes active after Reset. Legacy anchors without the cutoff
  retain their existing semantics. Start a new guide group even when the reset
  instructions lock it without advancing, and evaluate history cleanup against
  the request clock rather than the end of a potentially long retained group.
  Refuse rewind instructions that overlap retained rows before publication.
- Persist YAML programme checkpoints in the additive `PlayoutYamlCheckpoint`
  table in the same transaction as items, history and the tail anchor. Include
  instruction position, pending Count/All iterations or timed target, sequence
  order, ambient list state, pending shuffles, source identities/cycles and Smart
  Shuffle state. Reuse Classic source reconciliation after membership changes.
  Capture only complete programme units, including their pre/mid/post rolls;
  preserving Refresh never replaces a primary programme while its pre-roll plays.
  Validate ownership, version, source shape, instruction cursor and published
  boundary before accepting state. Changed instructions require an explicit
  Reset rather than silently interpreting an old cursor against a different plan.
  Continue prefers a complete tail checkpoint and retains legacy anchor/history
  fallback for older timelines. Health reports missing or incompatible programme
  checkpoints with localized recovery guidance.
- Make future YAML sequence shuffles reproducible from the persisted playout
  seed and a serialized shuffle counter. A fresh entropy draw after restoration
  would change already-planned future rotations even with unchanged sources.
  Reset still selects a fresh playout seed; older contexts default the new counter
  to zero without discarding their saved current sequence order.
- Block primary-source history carries an additive versioned checkpoint of the
  next source position, playout/cycle seeds, random draw counter and Smart Shuffle state. Keep
  the original history detail fields for existing readers. Reuse Classic seeded
  shuffles, group separation and membership reconciliation so unconsumed surviving
  items stay ahead of additions and consumed items. Random draws are reproducible
  after restart rather than drawing fresh entropy on each rebuild. Legacy ordered
  history remains readable; legacy randomized history is not evidence of a complete
  checkpoint for an explicitly preserving rebuild. Reject malformed or incompatible
  checkpoint state instead of publishing a guessed replacement.
  A new playout seed starts fresh randomized state even when an archived source
  only returns later; it must not restore the pre-Reset cycle from history.
  Explicit Block rebuilds use the complete guide interval, including rolls and
  filler, as the preservation boundary. Reused template guide-group IDs must
  also match their guide start; an occurrence cannot retain all later uses of
  that template. Reset records a persisted history floor at the boundary, while
  Refresh validates retained primary history before replacing the future.
  Health applies the same checkpoint/version requirements, excluding retained
  pre-Reset programmes from the new rotation's readiness check.
- Preserve the published programme through its boundary, including its row
  identities, track choices and attachments. Restore scheduling state at that
  boundary before generating the replacement future. Reuse existing collection
  and playlist reconciliation so membership changes do not replay consumed
  shuffled content. Missing or incompatible restore state refuses replacement
  observably; it never silently becomes Reset.
  Capture a monotonic deadline from the requested clock and retained future
  boundary before loading explicit-build inputs. Refuse an expired candidate
  before publication, after acquiring the database writer and before commit.
  Roll back expired publication work and keep durable Refresh intent eligible
  for bounded retry at a newly selected programme boundary. This construction-
  and transaction-time guard does not establish producer handover: already
  prefetched content and Next snapshot publication require their own acceptance.
- A Next snapshot refresh failure must not stop the current producer. Reuse its
  existing session refresh loop and serialized atomic publisher: bound each
  attempt to thirty seconds and retry after one second, doubling to a maximum
  thirty-second cooldown. Notifications coalesce without bypassing that cooldown;
  notifications received during an attempt still require a subsequent read.
  Heartbeat, idle expiry and Stop remain independently supervised. A removed
  channel or switch away from Next is a typed terminal outcome, not retry work.
  Keep retry intent in the running session: a new session already publishes a
  fresh database snapshot before starting. A timed-out blocking writer retains
  the publisher gate until it exits, preventing later writes from overtaking it.
  Pass the earliest coalesced scheduler deadline into the projection/publisher.
  Check it before temporary-file creation and after serialization and flush,
  immediately before replacement, independently of async timeout polling.
  Expired requests retain the running producer and require a fresh request;
  retrying that same generation must not extend its safe handover window.
  As with cancellation, a replacement already started cannot be undone.
  This protects the active process on refresh failure; it does not acknowledge
  a rebuild's effective boundary or prove that prefetched future content matches.
- A build with active snapshot owners must obtain publication acknowledgement
  before recording success or completing its durable Refresh generation. Give
  each session request a monotonic local generation: an older in-flight read
  cannot acknowledge a request arriving after that read began. Notify source,
  mirror, direct and independently registered preview owners concurrently, using
  one bounded deadline. Failure leaves the previous successful-build timestamp
  and durable retry eligibility intact; it does not roll back a committed SQL
  timeline or claim the old snapshot has adopted the new future.
  Reuse the singleton playout locker with an explicit committed-read phase.
  After all timeline writes, the owning builder may admit publication readers
  while continuing to exclude later builders through status/intent completion.
  Readers hold their own guards through the atomic file replacement, including
  abandoned blocking writes. Releasing the builder cannot release those readers.
  No timeline writes are allowed after entering this read phase. These guards
  serialize committed-source publication; they do not solve producer prefetch or
  make the SQLite/file boundary one atomic transaction.
- Prepare the complete SQL write set through the existing repository transaction
  before exposing it to snapshot owners. Reuse the committed item decoder for
  candidate reads, including actual database IDs, track overrides, watermarks
  and graphics variables. Keep items, history, checkpoints, dependency baselines,
  anchors and templates in that same transaction. Dropping the preparation owner
  rolls it back; recheck the captured deadline before commit. Retain the existing
  playout lock and bound preparation because SQLite's writer is reserved.
  This enables Next file preparation before SQL visibility; it does not by itself
  coordinate the two final replacements or make them atomic. The existing direct
  repository method retains its prepare-and-commit behavior until that integration
  is implemented and verified.
  A staged Next file owns its temporary file and serialized channel admission;
  no discoverable publication changes during serialization or flush. Use the
  pending build only for its own source playout, retaining ordinary read guards
  for other source playouts and applying the requested mirror's signed offset.
  Bound projection/admission by the earlier of the caller and SQL deadlines,
  and recheck deadline/attempt ownership before final replacement. Do not prepare
  the same channel publisher twice concurrently: main/preview owners sharing it
  are coalesced by the worker integration.
- Finalize programme-preserving candidates before exposing their SQL write set.
  One owned blocking finalizer retains the originating build lease, publisher
  admission and source read guards even if its async caller is dropped. Retain
  previous file bytes, replace each source/mirror file, then commit SQL. Failure
  before commit restores already replaced files; SQL commit success disarms all
  compensation before gap registration. Bound replacement retries and reserve
  five seconds per configured Next publisher plus two seconds for the SQL
  decision ahead of the existing safe publication deadline. Include inactive
  configured mirrors in that planning reserve. Permanent storage failures which
  also prevent compensation remain visible failures; retain rollback bytes when
  possible and reproject accepted SQL before restarting a producer. This is
  coordinated compensation, not database/filesystem atomicity. Explicit immediate
  replacements and append-only builds without a protected-prefix deadline retain
  SQL-first ordering so an immediately selected item ID is already readable.
- For builds connected to playback registries, select a whole programme boundary
  beyond a conservative prefetch reserve: sixty seconds of producer buffering,
  one second for native whole-second admission, sixty seconds for an unknown
  fallback, and the largest configured leading-mirror offset. Reserve at least
  thirty additional seconds for construction/publication. Include idle configured
  mirrors and registries because a viewer can join during construction. If this
  floor falls in a published gap or unfilled guide interval, retain that whole
  reservation too. Round the floor upward to persisted 100 ns precision.
  Apply this boundary consistently to Classic, Block, YAML and Scripted; keep
  the actual request clock for history and cleanup. The publication deadline is
  the retained boundary minus the prefetch reserve, not the boundary itself.
  Offline builds without a playback registry retain their immediate-boundary
  behavior. This conservative policy can retain multiple short programmes and
  must be represented accurately in the eventual operator controls. Database
  proofs do not establish decoded continuity or recovery after a snapshot owner
  fails to adopt the committed future before its deadline.
- Capture Scripted content dependencies through actual source resolution,
  including dynamically constructed library/tag queries. Relevant successful
  changed scans enqueue a coalesced durable Refresh through the existing scan
  refresh coordinator. No-change and unrelated scans do not rebuild timelines.
  Persist empty selections as well as populated ones, with semantic signatures
  per library so another library's changes cannot be attributed to this scan.
  Publish dependencies in the timeline transaction. Continue preserves the
  retained future's existing dependency baselines and adds new sources; Refresh
  and Reset replace the set. A candidate or failed build cannot acknowledge a
  changed source for an unchanged published future.
- Successful local, Plex, Jellyfin and Emby scans persist a library revision and
  coalesced comparison work. The existing refresh sweep compares at most sixteen
  library/playout pairs within thirty seconds, with durable backoff on failure.
  Changed observations and Refresh intent publication share a transaction;
  restart and concurrent sweeps cannot acknowledge a change without its intent.
  Health reports comparison failures while retaining the published timeline.
- Check the captured source generation inside the publication transaction,
  before any timeline or history mutation. A newer generation keeps its pending
  work; an older result cannot overwrite it. Preserve bounded retry and restart
  recovery rather than adding a second background scheduler.
- Re-resolve candidate source selections before publication and compare their
  semantic signatures. Capture library receipts and managed-library attempt
  counters before these reads; validate both inside the timeline transaction.
  A completed scan awaiting dependency comparison cannot publish a stale
  candidate. A scan that starts and fails during validation is also detected.
  Reuse the durable managed-library operation boundary for active scans,
  publication, recovery and retries; do not create another scan lease. Because
  empty and dynamic selections can gain members from any library, this guard
  conservatively defers Scripted publication while managed-library work remains
  active. It preserves the published timeline and does not enqueue unrelated
  rebuilds. Deferred Refresh intents retain generation-aware 30-to-300-second
  backoff through restart; newer observations do not shorten that cooldown.
- Show source revision, last successful build, pending refresh and failure in
  localized administration and keep Health aligned with restore readiness.
  Preserving database rows must also preserve the running producer until its
  programme boundary.
- Count observed semantic selection changes per Scripted playout, in the same
  transaction as comparison acknowledgement and Refresh intent. This revision
  survives intent completion; no-change scans do not advance it. Start at zero
  for the upgrade baseline rather than inventing earlier history. Retain the
  last successful build timestamp independently from the latest attempt; an
  upgrade can recover it only when the previously stored attempt succeeded.

## Consequences

Refresh can update future selections without restarting the current programme.
This deliberately improves the Scripted rebuild contract beyond ErsatzTV's
reset-oriented UI. It requires versioned persisted restore state and rejection
of unsafe legacy state, plus SQLite restart and real playback evidence.

## Alternatives Considered

- Reset on every scan: rejected because it restarts programmes and rotation.
- Keep the whole future horizon: rejected because new selections could wait
  days before entering the schedule.
- Delete future rows without restoring state: rejected because a future-tail
  anchor or raw collection index cannot reproduce the selected boundary.
- Rely only on generation-aware acknowledgement: rejected because it leaves a
  stale publication window before acknowledgement.

## References

- [Rebuild semantics](0020-playout-rebuild-on-impacting-changes.md)
- [Scripted continuation](0109-scripted-continuation-publication.md)
- [Rebuild parity](../progress/parity/playout-rebuild-on-impacting-changes.toml)
- `$ERSATZTV_SRC/ErsatzTV/Pages/Playouts.razor`
- `$ERSATZTV_SRC/ErsatzTV.Core/Scheduling/Engine/SchedulingEngine.cs`
