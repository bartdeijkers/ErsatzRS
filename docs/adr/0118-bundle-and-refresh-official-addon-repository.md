# ADR 0118: Bundle and refresh the official add-on repository

## Status

Accepted (2026-10-09). The maintainer accepted planning this behavior after
Phase 9 teletext and Phase 10 video jukebox. Visual design refinement may run
before those phases finish; production implementation remains dependency-gated.

The maintainer explicitly locked Sources design v0.1 on 2026-10-09. Its
[lock record](../reference/ui/design-artifacts/addon-sources/LOCK.md) pins the
accepted reference; design approval does not start the implementation milestones.

## Implementation

Not implemented. Milestones `oad-01` through `oad-04` and the
`official-addon-distribution` parity record own delivery and acceptance.

## Context

[ADR 0081](0081-use-local-addon-repositories.md) replaced public catalogs with
operator-acquired Git checkouts and ZIP bundles when the official repository
became private. The maintainer has made ErsatzRS-addons public again and finds
repeated ZIP uploads burdensome. The Sources page also exposes an unexplained
checkout path and gives unknown-source setup priority over official discovery.

The maintainer requires file installation to remain supported, alongside a
bundled official repository enabled on first run and a per-repository automatic
update toggle that defaults to On for the official repository on first run.

## Decision

### Distribution and first run

Ship a pinned, tested, host-compatible snapshot of the official repository in
ErsatzRS release distributions, including native archives and Docker images.
Keep the external add-ons project independently versioned; use its complete
repository bundle rather than executing a mutable Git checkout. Carry package
identity, origin, integrity metadata and required license notices.

Initialize the official source from that bundled snapshot in the writable
profile. Its catalog is available without a first-run network request. Record
source enablement and automatic repository updates as On only when initializing
a new profile. Repeated startup and application upgrades preserve saved choices,
newer repository contents, installed packages, settings and data.

Enabling the repository means making its offers available. This decision does
not authorize automatically installing or enabling yt-dlp or every contained
add-on. Such executable defaults require a separate explicit product decision.

### Automatic repository updates

Expose a persisted, per-source **Automatically update this repository** setting
for the official online source, independently from source enablement. Download
and validate eligible published official releases in bounded background work;
startup and playback must not depend on a successful online check. Keep a manual
**Check for updates** action when automatic updates are Off.

Repository refresh updates available offers and package contents. It does not
replace running installed add-ons or change their enablement. Preserve the
existing explicit install/update and permission-review boundaries, including
native tool maintenance as a separate policy.

Validate before atomic promotion and retain the last usable snapshot on network,
validation or cancellation failure. Persist enough state to avoid duplicate
work after restart and show version, last successful check, progress and
recoverable errors. Select concrete cadence and transport mechanics in the
implementation plan without creating a general-purpose update framework.

### Trust, offline sources and migration

The bundled official source works with Unknown sources Off. Official provenance
comes from the application-owned distribution and update origin, never a name or
repository ID supplied by an uploaded ZIP. Do not temporarily enable Unknown
sources to bootstrap or refresh the official source.

Retain unknown-source installation from both individual add-on ZIPs and complete
repository ZIPs, as well as manual Git checkouts and the existing bounded safe
extraction, manifest, compatibility, fingerprint and immutable-install guards.
Uploaded repositories remain offline snapshots; local checkouts remain
operator-pulled. Neither acquires an automatic network origin merely by sharing
the official repository's ID.

Existing profiles require an explicit migration/adoption path that preserves
installed data and source ownership. Do not overwrite a local checkout or
bundle, silently reclassify it as trusted official content, or resurrect a
source deliberately disabled or removed. Upgrades must not treat missing new
settings as proof of a brand-new profile.

### Administration and scheduling

Lead Sources with the official repository and its update policy. Keep **Install
from file** visible, with contextual unknown-source enablement and both ZIP
formats. Put manual Git instructions and the labelled, read-only server checkout
folder under Advanced. Expose distinct online, bundled/offline and local source
states; preserve installed add-ons when a removable source is removed.

Implementation begins only after `p9b-02` and `p10b-02` are Complete. Design
artifacts may be refined now and remain proposals until explicitly locked.
Do not renumber Phase 11 or expand teletext/jukebox acceptance for this work.

This decision replaces ADR 0081's local-only restriction for the official source
when delivered. Its manual Git/ZIP acquisition and immutable execution contracts
remain. Current user documentation continues to describe the implemented local
behavior until the owning milestones provide replacement evidence.

## Consequences

- First-run discovery can work offline, and routine official repository updates
  no longer require downloading and uploading ZIPs by hand.
- Packaging, profile initialization, source provenance, persisted update state,
  migration, health and Sources UI need coordinated changes in the host and
  separate add-ons repository.
- Bundling a repository does not establish availability of external tools used
  by its packages, executable readiness, or support on an untested platform.
- Native and Docker delivery must identify the exact compatible bundle. A newer
  public release alone is not evidence of host or platform compatibility.

## Alternatives Considered

- Continue ZIP-only official distribution: retains recurring acquisition friction.
- Fetch the first catalog only from the network: prevents offline first discovery.
- Clone or pull Git inside ErsatzRS: expands process and credential ownership.
- Automatically update executables during repository refresh: exceeds this scope.
- Remove or hide file installation: conflicts with the maintainer's requirement.

## References

- [Deferred implementation plan](../../plans/official-addon-distribution.md)
- [Design artifact and review guide](../reference/ui/design-artifacts/addon-sources/README.md)
- [Milestone acceptance](../progress/milestones.toml)
- [Feature debt](../progress/parity/official-addon-distribution.toml)
- [Current repository contract](../reference/addon-repositories.md)
- [Current package contract](../reference/addon-packages.md)
- [Official repository](https://github.com/bartdeijkers/ErsatzRS-addons)
