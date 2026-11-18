# ADR 0136: Serve published caption snapshots with scoped grants

## Status

Proposed 2026-11-16. Local implementation and isolated qualification belong to
the active UCU-16 continuation. This grants no publication or delivery authority.

## Implementation

Partial — local row292 functionality is verified for ordinary local/provider/prepared sidecars,
ordinary sidecars beside optional source-caption intent, and automatic transient
previews. The current combined117-test trial includes62 actual Next cases,
retains Native provider controls, strict source/publication/session/pin authority,
Unicode and HTTP failures, and cache reuse. Exact managed FFmpeg accepts matching
empty SRT, ASS and WebVTT through extensionless HTTP. Original controlled
negatives and the bounded row292 closeout are retained in the owning milestone.
Local playback closeout and shared UCU-16 window acceptance remain distinct.

## Context

Row292 adds upstream empty-subtitle delivery when a scan removes subtitle IDs or
paths after serialization. The original Rust implementation published ordinary
local sidecar paths directly. A successful scan notification cannot repair the first input
while the producer is still waiting for readiness. Automatic pre-publication
availability filtering does not close this later race. Ordinary provider text
instead used the entire mutable subtitle selection in a deferred item digest;
removing only an automatic subtitle row prevents real-worker readiness for20s,
despite a passing present-file control. Prepared add-on media also fingerprints
the mutable caption catalog with its immutable AV publication. Clearing only the
automatic sidecar Path replaces a decoded blue programme with black fallback;
present and missing-file controls preserve AV. The continuation retains these
distinct negatives.

## Decision

Extend the existing internal media relay with bounded text-caption serving.
Persisted standalone caption sources use an opaque HMAC-SHA256 grant,
using an independent process-local random signing key and the already locked
RustCrypto hmac0.13 dependency. Use the library's constant-time verification. Grants contain
only binding identities/digests, format, expiry and admitted owner IDs; never
source paths or provider credentials. There is no new runtime service or table.

Recheck the active owner, output channel/source-mirror relationship, persisted
item, media version/file and exact subtitle metadata/path binding before and
after bounded file reading. Owner shutdown, changed media or foreign bindings
cannot acquire captions. Reject invalid/expired/tampered grants before storage.
Preserve source seeks and standalone stream0, Unicode/charset handling, byte/time
bounds and no-store delivery. Return an empty document matching the advertised
format only for an unavailable automatic caption in an otherwise valid binding.
Malformed text, storage failures and exact pinned selections remain observable
failures.

Ordinary provider text retains deferred AV admission through a signed grant.
Its AV digest excludes mutable caption selection/extraction metadata while
retaining the original persisted occurrence, source/version/file revision,
selected video/audio, channel policy, graphics and provider intent. It also binds
the provider's remote server/item identity when the AV file itself is local.
Only that independently signed digest can admit the old caption selection after
its row disappears; an arbitrary changed item digest, local-caption token or a
replacement subtitle ID cannot authorize the callback. Keep exact pins and
actual qualified source captions on their existing strict preparation paths.
When an ordinary sidecar coexists with optional finite-source intent, still run
the existing complete-source qualifier with the actual runtime support. Preserve
its ordinary AV fallback; a caption-only change during preparation requires fresh
signed AV/intent validation. Do not turn an unrelated source error into grace.
Read provider text through the existing bounded authenticated fetch and owned
Burn cache, revalidating caption and AV identity before/after preparation and
reading. Preserve the existing32MiB text bound and bounded preparation budget;
retain its existing source/text cache key independently of occurrence authority.
HTTP/authentication/storage/decoding failures stay errors. Do not contact a new
provider source using a grant captured for an old one.

Automatic text sidecars for prepared add-on media use the same independent
caption grant with a distinct prepared-artifact purpose. Its AV authority retains
the frozen list revision/digest, artifact readiness/path/length, media version/file,
stream layout, selected AV, package/settings, channel/profile, graphics and timing;
only the mutable caption catalog and projected subtitle track are excluded.
Opening the callback or reading captions still re-resolves the frozen publication
and confined prepared AV artifact through the existing resolver. Captions retain
their original metadata/path binding and ID; another ID is never substituted.
Read text with the existing bounded charset decoder and five-second deadline,
rechecking source and session before and after reading. Exact pins retain strict
selection, and this route admits no producer or retained-source substitution.

Automatic transient previews use a distinct signed preview scope and exactly one
session owner. Zero channel/item IDs refer only to that session's frozen context;
they never authorize a generic database lookup. Recheck the original media
version/file, occurrence/path binding and original caption ID before/after reads.
Prepared previews retain the same frozen publication/artifact checks as persisted
prepared media. Mint private preview grants before publication without creating
fake SQL playout rows or admitting a session early. Explicit/custom choices and
generated credits keep their existing paths. Already prepared provider/embedded
captions keep their owned artifact; do not copy or fetch them again.

Embedded and observed source-qualified captions retain their original media or
owned preparation contracts. They do not use the upstream positive-ID sidecar
endpoint; no new variant matrix is required absent a concrete affected regression.

Keep all required source/window/platform, international playback, browser and
Health proof in the existing continuation records. Completed local row292
components do not imply integrated window or runtime-package acceptance.

## Consequences

Caption disappearance can no longer destroy unrelated programme AV on the
qualified path. Grants expire with the occurrence and stop admitting reads when
their process owners end. File reads and provider preparation stay bounded; no caption grant
opens an upstream media stream or relaxes AV publication identity.

## Alternatives Considered

- Scan notification alone: the retained startup race fails before that worker
  can process its queued publication update.
- Accept every changed dynamic-item digest: rejected; this would weaken media,
  selected-track and publication authority.
- Copy every caption during schedule publication: rejected; it moves potentially
  large text reads into schedule publication and needs artifact retention work.

## References

- [ADR0095](0095-native-pipeline-instead-of-ersatztv-next-engine.md)
- [ADR0132](0132-normalize-burned-text-subtitles-before-playback.md)
- [Catch-up plan](../../plans/upstream-catch-up.md)
- [Continuation milestone](../progress/milestones.toml)
- [RustCrypto hmac0.13 API](https://docs.rs/hmac/0.13.0/hmac/)
- [Exact upstream source](https://github.com/ErsatzTV/legacy/commit/6bbb38d86bb5065ced69bfb0a034b35d0d34f74d)
