# ADR 0094: Limit the add-on-list MVP to draft and publication

## Status

Accepted (2026-09-13)

## Implementation

Implemented — historical browsing and arbitrary Restore are removed from exposed
admin/API actions, verified in native tests and Chromium/Firefox with and without
JavaScript. Milestone `amm-o-01` owns the evidence and remaining folder-first
implementation; this scope reduction does not complete that milestone. Internal
publication and recovery machinery remains unchanged.

## Context

Folder sharing requires the current saved draft and independently published
content, not arbitrary historical revision browsing or rollback. The maintainer
has withdrawn that optional MVP feature and legacy-profile adoption: this beta
has one user who regularly starts with a fresh profile.

## Decision

- Withdraw historical revision browsing, historical comparison and arbitrary
  Restore from the MVP, including their native and hydrated admin/API actions.
  This supersedes that part of ADR 0085 and AMM-K-04, not their historical evidence.
- Preserve current draft versus published comparison, internal version counters,
  exact publication-job identities and immutable source snapshots needed by
  active publication, playback and recovery. Do not remove those internals merely
  because the optional history UI no longer exists.
- Preserve failed-publication recovery and its last-good-publication restore.
  This safety action is distinct from selecting an arbitrary historical revision.
- AMM-O-01 reconstructs current saved draft and published content from valid
  folders. It does not add a legacy-profile adopter, backfill or upgrader.
  Existing unadopted write paths remain until new-list file creation is wired;
  this decision is not permission to delete profile data or migrations.
- Do not extend the unfinished file-first historical Restore implementation.
  Keep removed uncommitted work recoverable outside the active implementation.

## Consequences

The MVP has a smaller operator workflow and no historical Restore integration
gate. Folder-first creation, remaining active writers, independent receiving
publication and Linux/Windows/browser acceptance still apply. Existing internal
retention is not redesigned or deleted in this change.

## Alternatives considered

- Finish history because it was already built: rejected; sunk effort does not
  make it necessary for the MVP.
- Delete every revision table/counter: rejected; current publication and crash
  recovery use them, and doing so would require an unrelated rewrite.

## References

- [ADR 0085](0085-broaden-addon-list-lifecycle-and-provider-options.md)
- [ADR 0093](0093-make-saved-addon-list-folders-authoritative.md)
- [Delivery plan](../../plans/addon-list-lifecycle-broadening.md)
- [Milestones](../progress/milestones.toml)
