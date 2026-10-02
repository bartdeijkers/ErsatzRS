# ADR 0060: Separate local television identities by folder provenance

## Status

Accepted (2026-08-02)

## Implementation

Partial (2026-10-02). The Windows case-alias correction is implemented locally
and scheduled for v1.1.1; native package deployment and legacy episode-duplicate
cleanup remain tracked in the [scanner parity record](../progress/parity/scanner.toml).
Full local-TV scans resolve shows within one
`LibraryPath` by exclusive top-level-folder provenance, normalized NFO provider
GUID, then exact title plus non-null year. A pre-scan transaction repairs
already-merged hierarchies, and the persisted health-check set reports any
remaining cross-folder show or season ownership.

## Context

ErsatzTV's local television repository matches a show by exact title/year
within a library path and falls back to an episode path under the show folder.
ErsatzRS previously matched only `ShowMetadata.Title`. Two folders with the
same title therefore shared one `Show`, and each folder's `tvshow.nfo` metadata
overwrote the other. A remake or reboot could consequently appear as extra
episodes in the original run's Season 1.

Title/year parity fixes the known collision, but it remains ambiguous when a
sidecar omits its year or corrects previously stored metadata. NFO provider
IDs and the already-persisted `LibraryFolder` hierarchy provide stronger
identity evidence without changing the schema.

## Decision

Scope local-show candidates to the same `LibraryPath` and apply this order:

1. Reuse a show owned exclusively by the same top-level folder. Folder
   provenance is authoritative for metadata corrections within an established
   folder.
2. Otherwise reuse an exact normalized NFO provider GUID. Provider names and
   values are trimmed and compared case-insensitively while persistence keeps
   the existing `source://value` shape.
3. Otherwise apply ErsatzTV's exact normalized title plus non-null year match.

Different non-null years, or different values for a provider present on both
identities, prevent a merge. Separate folders with neither a stable GUID nor a
year are kept separate instead of falling back to title alone. Plex, Jellyfin,
and Emby keep their provider-item-ID identity paths unchanged.

Before scanning episodes, detect a local show whose persisted episode folder
links span conflicting top-level identities. Retain its ID for the canonical
folder selected by episode count, then season count, then normalized path.
Create or reuse target shows for the remaining identities, preserve whole
season IDs when possible, split mixed seasons at episode granularity, reparent
folder-owned extras, and clear affected season-folder ETags. Each show split is
one transaction. Existing collection, playlist, Trakt, and scheduling
references remain on the retained canonical ID and are not cloned.

At scan end, delete empty local seasons and then empty local shows. Linked
ErsatzRS extras count as content. The `Local television identity` health check
warns when persisted local show or season ownership still crosses top-level
show folders and passes after reconciliation.

On Windows, a case-only path change can leave two `LibraryFolder` rows for
one physical directory. Before television reconciliation, merge those folder
aliases within the same library path, retaining the newest folder spelling,
reparenting child folders and media-file links, and invalidating ETags in one
transaction. Require matching canonical filesystem paths; keep missing,
unreadable or physically distinct directories separate. Folders with image
duration settings remain outside this television-only repair. Folder and
episode lookups may reuse verified Windows case aliases, while Unix keeps
case-sensitive path matching. Existing media IDs, versions, stream pins and
operator references remain intact; this does not delete legacy duplicate
episode IDs. Deployment and remaining cleanup are tracked by the scanner
parity record.

## Consequences

The next full local-library scan repairs existing remake/reboot collisions
without a migration or public API change. The largest hierarchy retains its
show, season, collection, and scheduling references; newly separated shows
must be added explicitly wherever an administrator wants them scheduled or
collected.

Folder provenance and conservative missing-year behavior deliberately improve
on ErsatzTV's ambiguous fallback. A renamed folder can recover the existing
show through a stable GUID, while an identity with no stable evidence may
remain separate and require metadata correction rather than risking another
silent merge.

## Alternatives considered

- Add a persisted local-show identity table. Rejected because the existing
  metadata GUIDs and folder/file links contain enough evidence for automatic
  repair, and a migration would not itself recover already-merged rows.
- Match title/year only. Rejected because it cannot preserve a show ID after a
  folder rename as reliably as a provider GUID and still merges two no-year
  folders with the same title.
- Clone collection, playlist, Trakt, and schedule references to every split
  show. Rejected because those memberships express administrator intent that
  cannot be inferred safely from folder ownership.

## References

- `$ERSATZTV_SRC/ErsatzTV.Infrastructure/Data/Repositories/TelevisionRepository.cs`
  (`GetShowByMetadata`, `DeleteEmptySeasons`, `DeleteEmptyShows`)
- `$ERSATZTV_SRC/ErsatzTV.Scanner/Core/Metadata/TelevisionFolderScanner.cs`
- `crates/ersatzrs-scanner/src/local_television.rs`
- `crates/ersatzrs-infra/src/health/checks/local_television_identity.rs`
