# ADR 0127: Preserve exact provider-selected source identity

## Status

Proposed

## Implementation

Partial — selected-source persistence, version-owned file keys, private cache
identity and request binding are implemented as one F5 prerequisite. P9A-04 owns
verification. Retained finite HTTP source acquisition remains open; neither a
provider declaration nor a URL fingerprint proves immutable source bytes.

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

## Consequences

Different providers and versions can safely share a reported path. Scans retain
old file identities and observations on no-op selector writes. Source changes
cannot reuse prepared captions or silently retarget a previously issued media
request. Existing schema health readiness follows the additive migration.

An in-flight URI can still return changed bytes without changing its selector.
F5 therefore still requires bounded owned acquisition, complete EOF and source
qualification, cancellation, cleanup and ordinary playback evidence. No client,
platform or runtime-adoption acceptance follows from this prerequisite.

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
