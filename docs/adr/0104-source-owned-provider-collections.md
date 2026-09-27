# ADR 0104: Source-owned provider collections

## Status

Accepted

## Implementation

Implemented (UCU-08 row 129, 2026-09-27).

Maintainer accepted this decision on 2026-09-27. Source ownership, conservative
backfills, atomic complete-root reconciliation and durable refresh recovery are
implemented and verified. Integrated restart continuity covers three providers,
three randomized playback orders, source collisions and descendant invalidation.
The row source map records focused acceptance; UCU-08 milestone-wide gates remain
open and are separate from this implementation claim.

## Context

UCU-08 row 129 ports upstream collection tag maintenance. Native Plex,
Jellyfin and Emby collection upserts currently match only Key or ItemId.
Those identifiers belong to one server. Two configured servers can return the
same identifier, causing the second scan to rename the first collection and
replace its membership. Source-scoped member lookup alone cannot prevent this.

The inspected ErsatzTV collection models and scanners also use globally matched
external identifiers. Preserving that behavior would violate the row's source
isolation requirement. Existing native collection IDs can be referenced by
schedules, fillers and persisted enumerators; replacing or deleting those IDs
can cascade into user configuration and scheduling state.

Provider collection lists are enumerated per library or root. Absence from one
root is not evidence of absence from another. Failed or truncated pagination is
not evidence of deletion.

## Decision

Scope provider collection identity to provider, MediaSourceId and external ID.
Add nullable source ownership to the three provider collection tables, with
unique indexes for claimed identities. Retain the existing Collection ID.

Keep the shared collection display-name uniqueness constraint. If a provider
name is already taken, suffix the new display name with provider and source ID;
include the collection ID and a numeric suffix if further collisions exist.
Keep that display name stable while the provider name is unchanged. Retain the
original provider name for collection tags. Never rename another collection to
make room for a newly discovered one.

Backfill a legacy collection only when its existing members identify exactly
one source of the matching provider, with no conflicting collection for that
identity. Leave empty, mixed-source or otherwise ambiguous collections unclaimed
and unchanged. Never select an arbitrary owner by scan order. A subsequent
scan creates a separate claimed collection when no unambiguous claimed identity
exists; it must not overwrite an unclaimed collection or its references.

Retain existing membership row IDs and playback overrides when a member remains
in the collection; update its provider order without rebuilding the row.

Record the roots that have successfully discovered each claimed collection.
Reconcile absence only within the successfully and completely enumerated root.
If another root still discovers a collection, retain it. On failure, cancellation
or incomplete pagination, preserve previous discovery and membership state for
the affected incomplete operation. Gather and validate provider pages before
opening the publication transaction; commit identities, membership, tags and
root discovery together. Count raw response rows before projecting supported
member kinds, require consistent totals and reject malformed identities.

When the last known root confirms removal, remove only the provider-managed
membership and its typed tags. Retain the Collection row, provider identity and
user scheduling references so rediscovery reuses the same ID. Do not delete
media, playback metadata, audio/subtitle rows or enumerator checkpoints. Notify
existing membership invalidation/refresh paths after a successful transaction.

Capture old Classic media dependencies before publication, including current
and alternate schedules, nested content and fillers. Compare semantic membership,
order, display names and typed tags inside the publication transaction; unchanged
scans produce no refresh work. Expand changed shows and seasons to their playable
episodes. Persist changed media IDs and old affected playout IDs atomically with
publication, because the pool-backed resolver cannot observe uncommitted new
search matches. The existing startup/periodic refresh sweep resolves new matches
and atomically transfers their union with the old matches into existing
ScanPlayoutRefreshIntent rows. Transfer failure or cancellation retains the
publication record for retry. Each sweep transfers at most 32 records; stale
records use the existing Scan Refresh State health threshold. Native collection
changes have a distinct bounded reason, coalescing with remote-stream changes
without losing their mixed provenance. Plex network tag publication reuses the
same handoff with a distinct reason and expands affected shows to episodes.
This adds no worker process or second playout-build queue.

Tag ownership remains metadata owner, provider type and external collection ID;
source-scoped media membership confines those writes to the owning server.
Unclaimed legacy collections are excluded from automatic removal. Explicit admin
deletion removes confidently owned tags and records affected refresh work in the
same transaction as the requested deletion. It preserves ambiguous unclaimed
tags instead of guessing ownership.

Backfill an untyped legacy tag only when its single metadata owner resolves to
one provider source. Classify collection tags before Plex labels, matching a
claimed collection on both source and external ID. Only leftovers on a Plex
source can become Plex labels; leave them untyped if an unclaimed Plex
collection has the same external ID. Preserve existing types, ordinary tags,
empty external IDs, orphan tags and ambiguous metadata/source ownership. This
adapts upstream's global collection-first/Plex-label fallback to source-owned
identities without guessing the owner of ambiguous legacy rows.

## Consequences

Same-ID collections from different servers become independent. Existing
unambiguous identities and schedule references survive upgrades and scans.
Confirmed provider removal clears searchable membership without cascading into
schedule configuration. Rediscovery can restore membership on the same ID.

Ambiguous legacy collections may remain visible alongside a newly claimed
collection. This preserves user choices rather than guessing which server they
represented. They remain available for explicit user management.

Acceptance requires real SQLite upgrade, rollback and restart tests; two-server
ID collisions; repeated scans and renames; multiple discovery roots; empty and
failed/truncated responses; removal and rediscovery; preservation of unrelated
tags, media tracks, user references and scheduling continuity. This ADR does not
waive any UCU-08 row or milestone gate.

## Alternatives Considered

- Keep globally matched identifiers: reproduces cross-server overwrites.
- Infer ownership from the current response: scan order can take ownership of
  another server's collection, especially when empty.
- Delete Collection rows when absent: can cascade into user scheduling data.
- Sweep a whole source after one library scan: incorrectly removes collections
  belonging to other libraries or roots.
- Rebuild every collection with new IDs: breaks existing references needlessly.

## References

- [UCU source audit and acceptance](../../plans/upstream-catch-up.md)
- [Scanner parity and debt](../progress/parity/scanner.toml)
- [Migration contract](../how-to/add-migration.md)
- ErsatzTV `ErsatzTV.Core/Domain/Collection/{Plex,Jellyfin,Emby}Collection.cs`
  and `ErsatzTV.Scanner/Core/Plex/PlexCollectionScanner.cs` at row 128,
  plus the row 129 `9067cde8` tag-maintenance diff.
