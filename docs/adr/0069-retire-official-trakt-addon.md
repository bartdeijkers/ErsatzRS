# ADR 0069: Retire the official Trakt add-on

## Status

Accepted (2026-08-16)

## Implementation

Implemented (working tree, 2026-08-16)

## Context

ADR 0067 moved Trakt list access out of the host and into a native Rust add-on
in the public official add-ons repository. That add-on consumed
`ersatzrs-addon-contract` from the private ErsatzRS repository. Local builds
succeeded through the maintainer's Git credentials, but the public add-ons
workflow could not reproduce the build without being granted a separate secret
that could read the private repository.

The maintainer requires every published artifact to remain inside repositories
owned by the project and does not want to publish the contract crate to
crates.io. After reviewing the remaining GitHub-only credential and repository
split options, the maintainer chose to remove the Trakt add-on.

## Decision

Remove `org.ersatzrs.addon.trakt`, its native Rust crate, its package metadata,
and its cross-platform build jobs from the official add-ons repository. The
signed catalog continues to publish Beeld & Geluid and yt-dlp through GitHub
Pages and no longer requires access to the private ErsatzRS repository.

Keep the provider-neutral Add-on Lists host feature and the
`media-list.list.v1` and `media-list.list.v2` contracts. Beeld & Geluid still
uses that surface for Schatkamer series, shared lists, and searches. Keep the
independently versioned `ersatzrs-addon-contract` crate private and guarded
with `publish = false`; its runnable reference example and conformance tests
remain the executable contract proof.

Trakt list parity is deliberately dropped. ErsatzRS does not restore the old
built-in Trakt client, tables, routes, or secret handling, and the migration
from ADR 0067 remains irreversible.

## Consequences

- The official catalog and its CI need no credential for the private ErsatzRS
  repository and build no native Trakt binaries.
- New installations cannot install an official Trakt provider package.
- Removing the package from the catalog does not silently restore or migrate
  the legacy built-in Trakt feature.
- Provider-neutral Add-on Lists remain available to current and future
  packages, including Beeld & Geluid.
- A future Trakt integration requires a new explicit decision and must satisfy
  the repository-only publication boundary without crates.io.

## Alternatives Considered

- Give the public add-ons workflow a read-only token for the private ErsatzRS
  repository: rejected because it adds a cross-repository secret solely to
  build one optional provider package.
- Publish `ersatzrs-addon-contract` to crates.io: rejected because the
  maintainer explicitly prohibits publication outside owned GitHub
  repositories.
- Create another public repository for the contract crate: rejected in favor
  of removing the Trakt integration and its distribution obligation.
- Restore Trakt inside the host: rejected because it would reverse the
  provider-neutral boundary and reintroduce provider-specific persistence,
  routes, and secret handling.

## References

- [ADR 0066: Repository-backed add-ons](0066-repository-backed-addons.md)
- [ADR 0067: Move provider lists into repository-installed add-ons](0067-move-provider-lists-into-addons.md)
- [Add-on media-list contract reference](../reference/addon-media-lists.md)
- [Trakt list parity record](../progress/parity/trakt-lists.toml)
