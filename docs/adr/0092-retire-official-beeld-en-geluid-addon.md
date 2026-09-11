# ADR 0092: Retire the official Beeld & Geluid add-on

## Status

Accepted (2026-09-11)

## Implementation

Implemented (working tree, 2026-09-11)

## Context

The maintainer withdrew Beeld & Geluid because its use is not legally
available to this project. This records the maintainer's product decision,
not an independent legal assessment. Current host and official-package source
trees already contain no Beeld & Geluid implementation, but documentation and
the Windows release checklist still advertise or require it.

## Decision

Drop official support for `org.ersatzrs.addon.beeldengeluid`. Remove its active
usage instructions, package availability claims and live acceptance obligations.
Do not access the service for development or acceptance. Its parser-hardening
work is withdrawn, not deferred, and withdrawn checks are not reported passed.
This supersedes the Beeld & Geluid provider obligations in ADR 0070, not its
provider-neutral chapter, metadata or publication contracts.

Keep yt-dlp and the provider-neutral add-on host, list lifecycle, metadata,
chapter and playback features. Retain historical decisions, release evidence
and offline generic regression fixtures; they do not imply current support.
Do not uninstall operator-managed packages, delete saved lists or media,
migrate databases, alter historical releases, or rewrite Git history.
A future official integration requires a new explicit maintainer decision.

## Consequences

- The official repository offers yt-dlp only.
- AMM-N-03 and the next Windows beta no longer require Beeld & Geluid evidence.
- Anamorphic playback, yt-dlp, native recovery and other accepted release
  checks remain required.
- Already installed snapshots are not remotely revoked or automatically
  removed; operators retain control of their configuration and data.

## Alternatives Considered

- Keep the provider as an untested option: rejected by the maintainer's
  withdrawal, which removes support rather than merely deferring testing.
- Remove provider-neutral host features or historical evidence: unnecessary
  and outside the approved scope.

## References

- [ADR 0070](0070-schatkamer-url-sources-and-chapter-fragments.md)
- [Add-on package reference](../reference/addon-packages.md)
- [Windows beta release hardening](../../plans/windows-beta-release-hardening.md)
