# ADR 0119: Acknowledge unavailable streams by manual exclusion

## Status

Accepted 2026-10-09

## Implementation

Implemented (Unavailable health check and focused SQLite regressions, 2026-10-09).

## Context

The Unavailable health check reports provider-unavailable Remote Streams even
after an operator excludes their add-on-list items from publication. The
canonical stream remains in the managed library, so retaining it for future
refreshes also retains a warning that the operator has already addressed.

ErsatzTV's `ErsatzTV.Infrastructure/Health/Checks/UnavailableHealthCheck.cs`
reports unavailable media without this add-on-list acknowledgement contract.
The maintainer accepted using the existing manual publication exclusion for
this purpose rather than adding an independent acknowledgement state.

## Decision

Suppress a Remote Stream's Unavailable finding when it has add-on-list owners
and every owner has `Included = 0` and `SelectionOverride = 0`. The override
distinguishes a manual exclusion from automatic filtering. Any owner without
this explicit decision keeps the stream actionable, including an item still
included in another list. Standalone Remote Streams remain actionable.

Apply the predicate before both distinct stream counting and path sampling.
Shared-version and versionless projected episodes remain classified as remote
media and cannot reappear as false missing-disk warnings after suppression.

Preserve media availability and publication state. Acknowledgement takes effect
on the next health evaluation, independently of publishing the saved draft.
The existing explicit Health refresh can bypass the cached result. Including
an item again restores its warning if its provider still reports unavailable.
Missing local files and other health checks retain their existing behavior.

## Consequences

Operators can acknowledge a known unavailable item through the existing
publication control without losing provider status or retained source data.
Automatic selection cannot conceal an availability problem. The change uses
existing columns and indexed ownership lookups; no schema, UI, translation,
or dependency change is required.

## Alternatives Considered

- A separate acknowledgement flag: duplicates the existing operator decision
  and introduces persistence and UI work without a demonstrated need.
- Suppress every `Included = 0` item: would also conceal automatic exclusions.
- Suppress when any owner is excluded: would hide streams still included in
  another list.

## References

- [Unavailable health check](../../crates/ersatzrs-infra/src/health/checks/unavailable.rs)
- [Health parity record](../progress/parity/ersatzrs-healthcheck.toml)
- [Add-on selection migration](../../migrations/202610051730_addon_list_selection.sql)
