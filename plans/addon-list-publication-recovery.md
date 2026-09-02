# Add-on list publication recovery

## Status

Corrective plan registered on 2026-08-31 from redacted Windows dogfood and
broadened on 2026-09-02. It owns milestones `amm-j-01` through `amm-j-04`.
The live list was recovered separately; this plan does not contain production
data, paths, URLs, titles, credentials, or runtime implementation.

## Outcome

Make add-on-list publication a durable, restart-safe operation instead of one
long admin request. A scan, process interruption, Windows sharing violation,
or host restart must leave either the last complete revision or a resumable
recovery job. No valid journal may become a permanent publication blocker.
Future incidents must retain enough typed causal history to distinguish the
original failure from a later rollback or recovery failure without leaking
private media or provider data.

Provider authentication and age restrictions remain item-scoped. When an
official add-on cannot access one item without credentials, it reports a
provider-neutral terminal availability outcome. ErsatzRS retains the item for
review, excludes it from the attempted revision unless a valid replacement is
available, and publishes the remaining usable items. Raw cookies never cross
the add-on boundary or enter persisted diagnostics.

## Confirmed incident

- A 165-item Windows publication persisted `Publishing`, an intact journal,
  and an intact previous-revision directory backup, but no failed-publication
  intermediate directory. The rollback therefore stopped at its first managed
  directory rename.
- `ReviewError` remained empty because the filesystem restore precedes the
  database update that records the original publication error.
- A later startup reported one failed interrupted Local scan-impact replay for
  the affected managed library. Health and the editor detected the stuck
  publication, but neither startup nor the admin UI could claim its journal.
- The publication overlapped a scheduled library scan. The scan completed, but
  the enclosing admin request never completed its commit or rollback. This
  beta did not persist enough phase diagnostics to prove whether request
  cancellation or a Windows sharing violation during rollback was the
  immediate trigger. Both paths can produce the observed stranded journal and
  both are required fault-injection cases for the repair.
- Five initially visible and one later detail failure were provider
  authentication/age restrictions. They remained optional details and were
  not publication blockers.
- With the host stopped, the journal-owned directory swap succeeded
  immediately. Restoring the journal snapshots, scanning through the admin
  UI, and matching the library produced 165 matched items, 165 playlist
  members, and 166 artwork materializations. The next explicit publication
  completed as revision 3 with zero blockers and a healthy database.

## Root cause

`publish_addon_media_list` stages the revision, performs a synchronous Local
scan, commits the projection, and attempts rollback inside the same admin
request. `restore_publication_backup` performs one directory rename with no
shared library lease, handle-drain barrier, retry, or persisted recovery phase.
The request lifetime can end after scan success, and a transient Windows
sharing violation can fail the first rollback rename. Current evidence cannot
distinguish those immediate triggers; either can stop before draft state and
the bounded diagnostic are restored.

The journal makes the interruption visible but is not executable durable work:
there is no startup claimant, expired lease, retry schedule, idempotent phase
transition, or operator recovery command. Health can report the condition but
cannot progress it. The atomic-publication contract is consequently complete
for injected in-process failures but incomplete for a real Windows filesystem
failure or process interruption.

## Repair invariants

- The last complete files, projection, playlist membership, and published
  liveness remain usable until one new revision commits completely.
- Publication, recovery, reset, refresh, and every manual, scheduled, startup,
  or child scan have one durable per-library owner. A publication child scan
  reuses its parent owner token; every unrelated scan defers and cannot overlap
  the same managed library.
- Every filesystem transition is idempotent and classified from durable phase
  plus observable directory state. Recovery never guesses which directory is
  authoritative from timestamps alone.
- Windows sharing violations receive bounded retry and jitter after scanner,
  ffprobe, and child-process handles are known to be closed. Permanent
  ownership, path, or integrity failures stop safely with an actionable,
  redacted diagnostic.
- Startup may resume or roll back an expired publication lease without an
  administrator editing SQLite or managed files.
- Provider detail failures that are not needed for playback remain optional.
  Authentication- or restriction-required playback failures skip only that
  item; they never strand the whole list in Preparing or Publishing.
- Raw cookies, authorization headers, URLs, media titles, and private paths do
  not enter the job table, health output, logs, fixtures, or plan evidence.
- Each operation has one correlation ID and a bounded ordered event history;
  original and recovery failures remain distinct, and retention never removes
  an active or recovery-required job.

## AMM-J-01 — Durable publication coordinator

1. Replace the request-owned publication sequence with an application-owned
   durable job. Publish validates the exact saved revision, persists intent,
   and returns a typed queued/active state.
2. Persist generation, saved revision, phase, attempt count, lease owner and
   expiry, heartbeat, next eligibility, bounded diagnostic, and terminal
   outcome. Phases cover staging, scanning, committing, restoring files,
   restoring projection, reconciling, and complete.
3. Claim one job transactionally per list and managed library. The same
   durable lease admission is mandatory for scheduled scans, manual scans,
   startup replay, refresh, reset, and recovery. Duplicate clicks return the
   existing job; a stale claimant cannot commit or remove a newer generation.
4. Route the Local scan through the normal durable scan coordinator with the
   publication owner token and wait for its persisted result. If another scan
   already owns the library, wait before staging. Once staging starts, every
   unrelated scan waits until commit or rollback reaches a safe terminal
   phase. Do not hold an HTTP request or database write transaction while
   waiting or scanning.
5. Persist the original failure before filesystem recovery begins. Every later
   recovery failure appends only a bounded redacted phase diagnostic while
   retaining the original cause.
6. On startup, reclaim expired leases and continue the exact persisted phase.
   A crash between filesystem success and database persistence must be
   recognized from journal markers and converge without duplicating revision
   or playlist rows.

## AMM-J-02 — Windows-safe atomic rollback

1. Give every managed, backup, and failed directory a journal-owned revision
   marker. Validate same-parent ownership and marker identity before rename or
   cleanup.
2. Add an explicit scanner/process completion barrier before directory swaps.
   On Windows, retry only sharing/access violations with bounded exponential
   backoff and jitter; POSIX retains the same state machine and acceptance.
3. Keep the previous revision backup immutable. Do not use hard links for
   mutable publication files unless all writers are proven replace-only;
   otherwise copy bytes or use a snapshot strategy whose mutation isolation is
   tested on NTFS and POSIX filesystems.
4. Make partial swaps resumable: managed present, backup present, failed
   present, or any safe two-directory combination maps to one deterministic
   next action. Ambiguous or foreign directories stop without deletion.
5. Restore published item identities, liveness, artwork materializations,
   projection, matching, playlist metadata, and refresh intents before clearing
   the journal. Clear it only in the final database transition.
6. Keep failed staging data until the restored or committed revision passes
   integrity and scan verification. Cleanup is a separate idempotent terminal
   step and reports aggregate counts only.

## AMM-J-03 — Recovery UX and restricted-item continuation

1. Show durable publication phase, attempts, last progress, and redacted
   diagnostic in the shared editor and health. Distinguish active work from an
   expired lease that needs recovery.
2. Add localized Retry recovery and Restore previous revision actions backed by
   the same application commands for hydrated and no-JavaScript paths. Neither
   action bypasses generation or ownership checks.
3. Keep list settings and item mutations locked only while a live lease owns
   the publication. Render **Publishing** only while that lease is executing.
   A failed, expired, or recovery-required job renders a disabled **Publish
   blocked** action followed immediately by an alert that names the failed
   phase, original reason, and the exact Retry recovery or Restore previous
   revision action that clears it.
4. Extend the add-on result contract with a provider-neutral terminal
   availability reason. Official yt-dlp mapping owns recognition of provider
   authentication and age restrictions; the host does not parse yt-dlp text.
5. Never persist or echo cookie content. A provider-neutral restricted outcome
   marks the affected item unavailable, retains its review row and diagnostic
   category, and excludes it from the attempted revision so the remaining
   items can publish. ADR 0085 and AMM-M separately own the typed per-item
   browser-cookie selector; the browser enum is not cookie material.
6. If a previously published item becomes restricted, preserve the last
   published revision until the next explicit Publish. The draft shows the
   pending exclusion and allows a Local replacement; refresh alone never
   silently removes current playout content.

## AMM-J-04 — Forensic operation history

1. Persist at most 256 immutable causal events per job, reserving the last
   entry for `history_truncated`. Record operation/correlation ID, sequence,
   enqueue trigger, phase, attempt, lease wait/owner reference, child scan,
   filesystem transition, retry class, sanitized operating-system error
   category and numeric code, recovery action, aggregate counts, and outcome.
2. Keep heartbeat and current progress as scalar job fields rather than event
   spam. Retain terminal history for at most 30 days and 50 jobs per list while
   preserving every active or recovery-required job.
3. Persist each event in the same transaction as the phase transition it
   describes. Resume and duplicate claims cannot duplicate, reorder, or
   overwrite events; later rollback failures append without replacing the
   original failure.
4. Render a localized semantic Activity timeline in the editor and matching
   aggregate state in Health/runtime audit. **Copy redacted diagnostics** and
   its no-JavaScript **Download redacted diagnostics** equivalent emit only the
   typed visible fields.
5. Reject URLs, paths, titles, cookies, headers, provider stderr, and arbitrary
   exception text from persisted events, UI diagnostics, fixtures, and copied
   output.

## Verification and release gate

- Repository and application tests inject failure before and after every
  durable phase, including process death after each filesystem rename and
  after external scan success but before result persistence.
- A deterministic overlap test starts a scheduled scan, submits Publish,
  disconnects the browser after scan completion, and proves that request
  cancellation cannot cancel or strand the application-owned job. A separate
  held-handle test injects the Windows sharing-violation path; both recover
  through the same durable journal state machine while retaining distinct
  original event classifications.
- Event tests prove transactional order, resume deduplication, the 256-event
  cap and truncation marker, 30-day/50-job retention, active-job protection,
  and the absence of private fields in copied diagnostics.
- Paused-time tests cover lease expiry, retry extension, duplicate claims,
  cancellation, and bounded Windows sharing-violation backoff without real
  sleeps.
- NTFS and POSIX integration tests hold a file handle during rollback, release
  it later, and prove convergence to one complete previous or new revision.
- SQLite tests prove revision, item, artwork, playlist, and refresh cardinality
  through resume, rollback, and repeated recovery.
- Contract fixtures prove structured restricted/authentication outcomes in the
  official add-on without cookies in stdout, stderr, snapshots, or diagnostics.
- Chromium and Firefox cover active publication, expired recovery, Retry,
  Restore, no-JavaScript submission, restart, and a list that publishes usable
  items while one restricted item is excluded.
- Windows dogfood repeats the exact failure shape with redacted evidence: stop
  or fault publication after scan, restart, recover in the UI, publish, and
  verify the archive's packaged binary, health, item count, playlist count,
  revision, and absence of a recovery journal.
- Final closeout runs affected Rust checks, hydration lint, web parity,
  progress validation, runtime smoke, and the playback gates required by any
  changed scan/refresh behavior.

## Completion order

1. `amm-j-01`: make publication and recovery durable and restart-safe.
2. `amm-j-02`: prove atomic filesystem rollback and immutable backups on
   Windows and POSIX.
3. `amm-j-03`: expose safe recovery actions and keep restricted items from
   blocking the remaining publication.
4. `amm-j-04`: preserve and expose a bounded causal history that makes future
   failure triggers distinguishable.

AMM-J is release-critical for the next Windows beta. It does not reopen the
provider-specific parsing boundary or permit direct production-database repair
as a normal operator workflow.
