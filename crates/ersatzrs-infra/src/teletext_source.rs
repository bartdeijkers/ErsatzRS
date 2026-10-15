//! Transactional report-only source teletext metadata (ADR 0121).
//!
//! These helpers do not acquire media, certify source bytes, or expose selectable
//! pages. The caller supplies a genuine acquisition binding and retains the
//! existing `SQLite` writer transaction. Errors require the caller to roll back;
//! accepted writes must be committed with the ordinary media projection.

use ersatzrs_core::repositories::{ManagedLibraryOperationClaim, RepositoryError, Result};
use ersatzrs_domain::{EntityId, SourceTeletextObservation};
use sqlx::{Row as _, Transaction};

use crate::db::{Db, DbRow};
use crate::repositories::managed_library_operation::claim_is_current;

mod codec;
mod header_codec;
mod parents;

/// Per-observation resource policy, not a DVB inventory ceiling.
pub const MAX_SOURCE_TELETEXT_DESCRIPTORS: usize =
    crate::ffprobe::teletext::MAX_PROJECTED_TELETEXT_ENTRIES;
/// Per-observation resource policy, matching auxiliary program acquisition.
pub const MAX_SOURCE_TELETEXT_PROGRAMS: usize =
    crate::ffprobe::teletext_probe::MAX_TELETEXT_PROBE_PROGRAMS;

/// Exact durable rows selected by the acquisition adapter, including one file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceTeletextParents {
    pub media_version_id: EntityId,
    pub media_stream_id: EntityId,
    pub media_file_id: EntityId,
}

/// Replace one exact stream/file report under the current managed scan lease.
///
/// `false` means the lease is stale and no metadata changed. Parent checks
/// include the lease's library even for unbound report-only metadata. A bound
/// revision remains an adapter association, not immutable-byte proof.
/// Ordinary streams, subtitles, audio, and source identity remain untouched.
/// An accepted write is not perpetual commit authorization: callers doing more
/// work must retain/recheck the existing lease before their later commit.
/// # Errors
/// Returns static validation/storage failures. On error the caller must roll
/// back its transaction; this helper neither commits nor rolls back other work.
pub async fn replace(
    tx: &mut Transaction<'_, Db>,
    claim: &ManagedLibraryOperationClaim,
    parents: SourceTeletextParents,
    observation: &SourceTeletextObservation,
) -> Result<bool> {
    if !claim_is_current(tx, claim).await.map_err(|_| storage())? {
        return Ok(false);
    }
    codec::validate(observation)?;
    let context = parents::validate(tx, parents, observation).await?;
    parents::validate_claim(tx, claim, &context).await?;
    // Reads above cannot change metadata. Recheck expiry before the first write;
    // the caller's writer transaction excludes concurrent lease replacement.
    if !claim_is_current(tx, claim).await.map_err(|_| storage())? {
        return Ok(false);
    }
    let encoded = codec::encode(observation);
    let id: i64 = sqlx::query_scalar(
        r#"INSERT INTO "SourceTeletextMetadata" (
            "MediaVersionId", "MediaStreamId", "MediaFileId", "StreamIndex",
            "BindingStatus", "MediaSourceId", "Provider", "SourceRevision",
            "Provenance", "PidStatus", "Pid", "ProgramStatus", "DescriptorStatus",
            "TrailingUnpairedPairs", "ObservedPagesStatus"
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0)
        ON CONFLICT("MediaStreamId", "MediaFileId") DO UPDATE SET
            "MediaVersionId"=excluded."MediaVersionId", "StreamIndex"=excluded."StreamIndex",
            "BindingStatus"=excluded."BindingStatus", "MediaSourceId"=excluded."MediaSourceId",
            "Provider"=excluded."Provider", "SourceRevision"=excluded."SourceRevision",
            "Provenance"=excluded."Provenance", "PidStatus"=excluded."PidStatus",
            "Pid"=excluded."Pid", "ProgramStatus"=excluded."ProgramStatus",
            "DescriptorStatus"=excluded."DescriptorStatus",
            "TrailingUnpairedPairs"=excluded."TrailingUnpairedPairs",
            "ObservedPagesStatus"=excluded."ObservedPagesStatus"
        RETURNING "Id""#,
    )
    .bind(parents.media_version_id)
    .bind(parents.media_stream_id)
    .bind(parents.media_file_id)
    .bind(observation.stream_index)
    .bind(encoded.binding_status)
    .bind(encoded.source_id)
    .bind(encoded.provider)
    .bind(encoded.revision)
    .bind(encoded.provenance)
    .bind(encoded.pid_status)
    .bind(encoded.pid)
    .bind(encoded.program_status)
    .bind(encoded.descriptor_status)
    .bind(encoded.trailing_unpaired_pairs)
    .fetch_one(&mut **tx)
    .await
    .map_err(|_| storage())?;
    codec::replace_children(tx, id, observation).await?;
    if !claim_is_current(tx, claim).await.map_err(|_| storage())? {
        // Metadata already changed inside this transaction. Returning false
        // would violate its no-mutation contract; require caller rollback.
        return Err(invalid("lease expired during metadata replacement"));
    }
    Ok(true)
}

/// Read one exact current stream/file report in a consistent caller transaction.
///
/// `None` means no acquisition record is present, never proven absence of pages.
/// Bound records are checked against current database ownership, without
/// verifying their opaque revisions against external source bytes.
/// # Errors
/// Returns static failures for corrupt, excessive or inconsistent metadata,
/// changed parent/source association, or storage failure.
pub async fn load(
    tx: &mut Transaction<'_, Db>,
    media_stream_id: EntityId,
    media_file_id: EntityId,
) -> Result<Option<SourceTeletextObservation>> {
    if media_stream_id <= 0 || media_file_id <= 0 {
        return Err(invalid("parent identity"));
    }
    let row = sqlx::query(
        r#"SELECT "Id", "MediaVersionId", "MediaStreamId", "MediaFileId", "StreamIndex",
            "BindingStatus", "MediaSourceId", "Provenance", "PidStatus", "Pid",
            "ProgramStatus", "DescriptorStatus", "TrailingUnpairedPairs", "ObservedPagesStatus",
            CASE WHEN TYPEOF("Provider")='text' AND LENGTH(CAST("Provider" AS BLOB)) <= 8
                THEN "Provider" ELSE NULL END AS "Provider",
            CASE WHEN TYPEOF("SourceRevision")='text'
                AND LENGTH(CAST("SourceRevision" AS BLOB)) BETWEEN 1 AND 1024
                THEN "SourceRevision" ELSE NULL END AS "SourceRevision",
            CASE WHEN
            ("Provider" IS NULL OR (TYPEOF("Provider")='text' AND LENGTH(CAST("Provider" AS BLOB)) <= 8))
            AND ("SourceRevision" IS NULL OR (TYPEOF("SourceRevision")='text'
                AND LENGTH(CAST("SourceRevision" AS BLOB)) BETWEEN 1 AND 1024))
            THEN 1 ELSE 0 END AS "TextBounded"
        FROM "SourceTeletextMetadata" WHERE "MediaStreamId"=? AND "MediaFileId"=?"#,
    ).bind(media_stream_id).bind(media_file_id).fetch_optional(&mut **tx).await.map_err(|_| storage())?;
    let Some(row) = row else { return Ok(None) };
    if column::<i64>(&row, "TextBounded")? != 1 {
        return Err(invalid("stored text bound"));
    }
    let parents = SourceTeletextParents {
        media_version_id: column(&row, "MediaVersionId")?,
        media_stream_id: column(&row, "MediaStreamId")?,
        media_file_id: column(&row, "MediaFileId")?,
    };
    let observation = codec::decode(tx, &row, parents).await?;
    parents::validate(tx, parents, &observation).await?;
    Ok(Some(observation))
}

pub(super) fn column<T>(row: &DbRow, name: &str) -> Result<T>
where
    T: for<'a> sqlx::Decode<'a, Db> + sqlx::Type<Db>,
{
    row.try_get(name).map_err(|_| invalid("stored value"))
}

pub(super) fn invalid(message: &'static str) -> RepositoryError {
    RepositoryError::Validation {
        field: "source teletext metadata",
        message: message.to_owned(),
    }
}

pub(super) fn storage() -> RepositoryError {
    RepositoryError::unavailable("source teletext metadata", "storage unavailable")
}

#[cfg(test)]
mod tests;
