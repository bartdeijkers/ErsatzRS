# ADR 0127: Preserve exact provider-selected source identity

## Status

Proposed

## Implementation

Partial — selected-source persistence, version-owned file keys, private cache
identity and request binding are implemented as one F5 prerequisite. P9A-04 owns
verification. Complete-response acquisition, configurable shared storage
admission and retained native/session input are implemented. Their named Linux
candidate proof is separate from accepted-runtime adoption and the remaining F5
matrix. Neither a provider declaration nor a URL fingerprint proves source bytes.

## Context

ADR 0121 requires captions and AV to belong to the same selected source.
Jellyfin/Emby declare a selected `MediaSources[].Id`, but the old projection
discards it and forwards only the item ID. The provider can consequently choose
a different default version. A globally unique hash of the provider path can
also transfer a file between versions or servers which legitimately report the
same path. Refusing every such collision instead prevents legitimate scans.

Reading the latest selector at media-open time alone is insufficient: after
caption preparation, a changed selector could pair old captions with new AV.
Credentials must still be resolved at opening time and remain private.

## Decision

Retain the qualified selected provider string in nullable
`MediaFile.ProviderMediaSourceId`, separate from the internal server ID.
Qualify the declaration using the exact version owner, file path and complete
embedded stream indexes in the existing scan transaction. Missing or
unqualified declarations store `NULL`. Changing this field invalidates only
that file's teletext observations; no-op assignments do not.

Preserve existing file IDs and opaque path hashes. New Jellyfin/Emby files use
domain-separated version-and-path hashes under the existing unique index.
Reuse an unambiguous owned legacy or current row; never transfer a foreign
row. Deep refresh may restore the original path of its own legacy row. A genuine
path move updates the unique existing version file, preserving its identity so
Next cannot select an older file and selector. If no exact match exists and
several files could be moved, refuse instead of guessing. Ambiguous ownership
or a corrupt namespaced collision is refused atomically.

Forward the selected ID as encoded `MediaSourceId` query data on the authenticated
static video request. Include it in the existing private caption/cache identity
and promotion checks. Existing `NULL` rows keep ordinary legacy playback and do
not acquire exact-source caption readiness.

For a selected remote source, publish a private request header containing a
domain-separated SHA-256 fingerprint of the resolved upstream URI. Immediately
before opening, compare this with the actual resolved URI, after normal
capability authentication and exact item/version/file checks. Missing, malformed
or changed bindings fail before provider HTTP work. Provider credentials remain
fresh headers; they and the raw source selector never enter published relay URLs.
The fingerprint is an association guard, not an authentication capability or a
content digest. Retained finite source ownership and byte qualification remain
separate requirements.

Retained whole provider sources use a dedicated configurable storage budget,
separate from add-on artifact authority. The maintainer-approved defaults are
20 GiB total, 8 GiB per acquired source and at least 2 GiB free space. Every limit
is configurable. The total accounts for all simultaneously retained source
files and copies, including owners still preparing, decoding, playing or held
after uncertain cleanup. Reservations and actual I/O must remain bounded after
caller loss. Exhaustion explicitly refuses caption preparation and preserves
ordinary AV; it grants no partial-source, caption-readiness or acceptance waiver.
Use the existing configuration repository, with byte-valued keys
`teletext.source_storage.total_bytes`,
`teletext.source_storage.maximum_source_bytes`, and
`teletext.source_storage.minimum_free_bytes`. Changed limits govern subsequent
admissions; admitted immutable sources retain their original charge. Invalid
settings refuse optional caption preparation and warn through the existing
localized teletext-work health check.

Reserve the complete peak, including copies, before acquiring provider bytes.
One shared state directory, OS admission locks and opaque durable receipts
coordinate participating processes. Charge actual owners through writes, probes,
decoders and readers, including abandoned callers and uncertain cleanup. Delete
owned files before releasing their receipt. A stale or unlocked receipt never
proves that former native consumers stopped; retain its charge until independent
cleanup proof permits recovery. No age-based reclamation is authorized here.

Acquire one complete authenticated response, then native-qualify those exact
retained bytes. Compare the entire embedded index/kind/codec layout, alternate
audio selection metadata and attached-picture status with the selected database
file. Create ephemeral provider-bound descriptor intents with the actual native
filesystem revision; never manufacture SQL readiness or substitute a content
hash for that revision. Bind the private caption cache separately to the full
content hash and length, provider/file/track intent, descriptor provenance,
selected video and existing complete-source clock policy. The complete PSI/PES
and EOF validation still runs; an already retained input need not be copied.

Use bounded occurrence entries under the existing Next session owner. A local
path alone cannot report per-programme native reader/reopen lifetime. Therefore
publish a capability-protected, occurrence-bound internal HTTP file grant with
HEAD and byte ranges. All AV opens use the same retained file as captions, not
another provider download. Expiry denies new opens; existing actual readers
retain their file and quota until completion. Revalidate the original published
selection before caption promotion. Each new AV open revalidates exact source
identity independently from later presentation or caption-selection changes;
presentation-only channel edits keep the same immutable file grant readable.
Refused optional preparation preserves ordinary AV; a changed source cannot fall
back to a stale grant. Session shutdown fences callbacks and joins preparation, actual
readers and storage cleanup under the existing single cleanup budget. Unknown
cleanup retains the complete session ownership and capacity as before.

## Consequences

Different providers and versions can safely share a reported path. Scans retain
old file identities and observations on no-op selector writes. Source changes
cannot reuse prepared captions or silently retarget a previously issued media
request. Existing schema health readiness follows the additive migration.

An in-flight URI can still return changed bytes without changing its selector.
The retained-input component provides bounded owned acquisition, complete EOF,
source qualification and joined session cleanup; P9A-04 identifies its exact
proof scope. Remaining providers, formats, delivery policies, native platforms,
restart recovery and accepted-runtime adoption still require their applicable
acceptance evidence. No client or complete F5 acceptance follows from this slice.

## Alternatives Considered

- Forward only the item/default source: cannot establish selected correspondence.
- Reject all globally repeated paths: breaks valid cross-server scans.
- Re-key existing rows or remove the unique index: unnecessary identity churn.
- Put credentials or raw selectors in private relay URLs: unnecessary exposure.
- Trust a cache key without binding media requests: leaves a post-promotion race.

## References

- [ADR 0121](0121-teletext-pages-transport-and-addon-contract.md).
- [P9A-04 acceptance and evidence](../progress/milestones.toml).
- `$ERSATZTV_SRC/ErsatzTV.Infrastructure/Scheduling/PlayoutItemConverter.cs`.
- `$ERSATZTV_SRC/ErsatzTV/Controllers/InternalController.cs`.
- [Jellyfin video stream request](https://typescript-sdk.jellyfin.org/interfaces/generated-client.VideoApiGetVideoStreamRequest.html).
- [Emby video streaming](https://dev.emby.media/doc/restapi/Video-Streaming.html).
