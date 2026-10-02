# ADR 0110: Preserve programmes during Scripted adaptation

## Status

Accepted (2026-10-02) — implements the maintainer-approved finding 06 solution.

## Implementation

Partial — stored-kind admission, transactional refresh-intent publication
fencing, versioned Scripted boundary checkpoints, runtime source capture and
bounded successful-scan comparison/admission are implemented locally. Candidate
source revalidation fences unprocessed receipts and concurrent managed-library
work; transient publication races retain intent with durable retry.
Refresh retains the active programme group and restores its source rotation
before replacing the future. Missing or incompatible checkpoints are refused
and reported by Health. Explicit UI modes, revision feedback and running-producer handover acceptance remain open under
v111-findings-06.

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
- Preserve the published programme through its boundary, including its row
  identities, track choices and attachments. Restore scheduling state at that
  boundary before generating the replacement future. Reuse existing collection
  and playlist reconciliation so membership changes do not replay consumed
  shuffled content. Missing or incompatible restore state refuses replacement
  observably; it never silently becomes Reset.
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
