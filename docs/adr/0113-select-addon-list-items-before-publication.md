# ADR 0113: Select add-on list items before publication

## Status

Accepted (2026-10-02)

## Implementation

Implemented — local acceptance is recorded in milestone `v111-findings-01`.
Packaged/platform deployment and publication are outside this acceptance.

## Context

Finding 01 requires small, relevant creator and podcast selections rather than
publishing an entire feed. The existing overview-first import, source validator,
adaptive refresh, exact-revision publication and artifact ownership already
provide durable work. ErsatzTV has no corresponding repository-installed add-on
draft workflow (ADR 0094); its list matching remains the baseline for unaffected
collections. Selection is an intentional extension, not a second import system.

## Decision

Save optional provider-neutral filters per list: rolling age in UTC days,
newest N, inclusive minimum/maximum duration and case-insensitive title terms.
Shared selection value objects live in `ersatzrs-domain`, so native and WASM
forms use the same serialized contract; `ersatzrs-core` owns their evaluation.
Any include term matches; any exclude term rejects. Dates and duration remain
unknown when the provider supplies no facts. Explain those rejections instead
of inventing values. Newest selection orders known publication dates descending,
then provider rank and stable item identity. Newest-N requires a known date. Manual pins bypass filters without
consuming the newest-N allowance; manual exclusions always win.

Preview decisions against the committed overview without downloading, publishing
or changing settings. Saving authorizes automatic application through ADR 0094.
An optional first-import selection review must be established before the import
can publish; ordinary lists retain their existing automatic behavior. This is
one initial selection decision, not a recurring publication wizard.

Apply selection inside the existing draft/import revision fences, before
acquisition. Reevaluate rolling selections even when the source has not changed.
Use the existing bounded detail worker for facts needed to classify plausible
candidates; rejected items do not trigger automatic acquisition. Keep manual
detail requests and their provider cooldowns intact.

Excluded rolling items remain archived, with stable identities, overrides,
retained artifacts and historical playback references. A failed candidate must
preserve the last complete published revision. Selection does not delete media
files or clear scheduling state. Successful publication and its verified scan
retain the existing scan-to-playout dependency invalidation.

Pause automatic polling independently of the saved cadence, validator and retry
cooldown. Resume through the same durable coordinator. Display saved filters,
effective cutoff, last/next poll, active/archive counts and localized reasons in
the existing hydrated and native-form workspace.

Bound a selection assessment to 10,000 committed items and 32 terms per include
or exclude list, each at most 256 UTF-8 bytes. Excessive input fails explicitly
before any candidate is applied; it does not truncate the published selection.

## Consequences

Upgrade defaults preserve existing settings and explicit inclusion decisions.
Only opted-in lists acquire selection behavior. Unknown facts can delay or
exclude a candidate; the UI must expose the reason and available enrichment.
Selection shares the existing revision, retry, publication and artifact gates.

## Alternatives Considered

- Another external updater: rejected because it duplicates publication and
  loses native revision, scan and recovery guarantees.
- Delete rolling rejections: rejected because archived and scheduled media must
  remain available.
- Replace adaptive polling: rejected because the existing coordinator already
  persists validators, checkpoints and bounded retries.

## References

- [Overview-first import](0079-make-addon-list-imports-resumable.md)
- [Automatic publication](0094-automatically-publish-addon-list-updates.md)
- Milestone `v111-findings-01` in [structured progress](../progress/README.md).
