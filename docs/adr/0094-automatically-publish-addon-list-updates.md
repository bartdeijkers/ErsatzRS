# ADR 0094: Automatically publish add-on list updates

## Status

Accepted 2026-09-16

## Implementation

Partial

Tracked by AMM-P-01. Linux runtime acceptance is verified. Exact-SHA release
CI and packaging remain open; native Windows acceptance belongs to the
maintainer after the Windows-only beta package is built.

## Context

Overview-first publication can expose incomplete metadata indefinitely: later
successful enrichment changes availability or duration and requires manual
review. The maintainer requires automatic publication, including existing saved
changes, while preserving overrides, retained media, and playback continuity.
ErsatzTV has no equivalent repository-installed add-on draft workflow; its NFO
episode reader preserves `mpaa` as ContentRating and `aired` as the release date.

## Decision

This supersedes the mandatory metadata review in ADR 0075 and additions-only
scheduled publication in ADR 0085. Every saved, valid change is automatically
published through the existing durable, exact-revision managed-library worker.
Imports, refreshes, enrichment and operator saves all use the same route.
Unsubmitted edits are never included. Overrides, including explicit empty
values, win; conflicts are informative rather than publication blockers.

Batch enrichment after 30 quiet seconds, with a 120-second maximum admission
wait. Preserve revision fences, leases, bounded provider work, retry cooldowns,
atomic file publication, recovery and the last complete playable publication.
Optional missing metadata does not block; permanent technical errors remain
observable. Installation permission review and destructive-action confirmations
are outside this decision.

Source disappearance does not remove items or locally stored media. Source
availability is distinct from effective playability; a valid local artifact or
replacement remains usable. Preserve artifact references and user inclusion.

The yt-dlp add-on maps channel/uploader to both studio and broadcaster. Explicit
nonnegative integer age limits use `age:<number>`, not a guessed national rating.
Show genres, tags, languages and studios fall back to a stable bounded union of
included episode metadata. Show source age falls back to the highest known
source age. Explicit header data and operator overrides take precedence.
Provider headers remain distinct from derived values. Episode projection stores
the first content rating, consistently with existing scalar metadata tables;
NFOs and provider documents retain all ratings.

Migrate existing review policies and saved pending work to automatic publication.
Resume journals before admitting new work. Upgrade repair uses durable jobs,
not direct edits to a live installation. Existing detail results are reusable.

## Consequences

No review or separate publication click is required. Saving a change authorizes
its automatic application. Health and Activity report actual waiting, retries
and terminal errors. Local media remains usable after source removal.

## Alternatives considered

- Keep review for operational changes: rejected because it creates unwanted
  recurring manual work.
- Remove missing source items automatically: rejected because retained local
  files or replacements can remain playable.
- Bypass publication and update metadata tables directly: rejected because it
  makes NFOs, revisions, scanner state and playback inconsistent.

## References

- [Previous metadata decision](0075-review-addon-metadata-before-publication.md)
- [Lifecycle and provider options](0085-broaden-addon-list-lifecycle-and-provider-options.md)
- [Structured progress](../progress/README.md)
