//! ADR 0137: stable provider identities and separately qualified physical routing.
use super::{JellyfinPlaybackMetadata, MediaStream, MediaStreamKind, RepositoryError, Result};
use crate::{db::Db, repositories::SqlxResultExt as _};
use sqlx::Transaction;

// Shared by scan admission and Health. The caller binds `v` to a MediaVersion.
// This checks stored structure only; native file/source qualification is separate.
// Extra map entries can describe provider attachments omitted from MediaStream.
pub(crate) const INVALID_SOURCE_INDEX_MAP: &str = r#"
    v."ProviderStreamIndexVersion" <> 1
    OR NOT EXISTS (SELECT 1 FROM "MediaStream" s
        WHERE s."MediaVersionId"=v."Id" AND s."MediaStreamKind"=1)
    OR EXISTS (SELECT 1 FROM "MediaStream" s
        LEFT JOIN "ProviderStreamIndex" m ON m."MediaVersionId"=s."MediaVersionId"
            AND m."StreamIndex"=s."Index"
        WHERE s."MediaVersionId"=v."Id" AND s."MediaStreamKind"<>5
            AND m."StreamIndex" IS NULL)
    OR EXISTS (SELECT 1 FROM "MediaStream" s
        WHERE s."MediaVersionId"=v."Id" AND s."MediaStreamKind"<>5
        GROUP BY s."Index" HAVING COUNT(*)>1)
    OR EXISTS (SELECT 1 FROM "ProviderStreamIndex" m
        WHERE m."MediaVersionId"=v."Id"
            AND (m."StreamIndex">2147483647 OR m."SourceStreamIndex">m."StreamIndex"))
"#;

pub(super) fn physical_streams(playback: &JellyfinPlaybackMetadata) -> Result<Vec<MediaStream>> {
    let map = playback.source_stream_indexes.as_ref().ok_or_else(|| {
        RepositoryError::Conflict(
            "Jellyfin source stream indexes are incomplete or ambiguous".into(),
        )
    })?;
    playback
        .streams
        .iter()
        .map(|stream| {
            let mut physical = stream.clone();
            if stream.kind != MediaStreamKind::ExternalSubtitle {
                physical.index = *map.get(&stream.index).ok_or_else(|| {
                    RepositoryError::Conflict("Jellyfin source stream has no physical index".into())
                })?;
            }
            Ok(physical)
        })
        .collect()
}

pub(super) async fn persist(
    tx: &mut Transaction<'_, Db>,
    version: i64,
    playback: &JellyfinPlaybackMetadata,
) -> Result<()> {
    let map = playback.source_stream_indexes.as_ref().ok_or_else(|| {
        RepositoryError::Conflict("Jellyfin source stream indexes were not qualified".into())
    })?;
    sqlx::query(r#"DELETE FROM "ProviderStreamIndex" WHERE "MediaVersionId"=?"#)
        .bind(version)
        .execute(&mut **tx)
        .await
        .repository()?;
    for (identity, physical) in map {
        sqlx::query(r#"INSERT INTO "ProviderStreamIndex" ("MediaVersionId","StreamIndex","SourceStreamIndex") VALUES (?,?,?)"#)
            .bind(version).bind(identity).bind(physical).execute(&mut **tx).await.repository()?;
    }
    sqlx::query(r#"UPDATE "MediaVersion" SET "ProviderStreamIndexVersion"=1 WHERE "Id"=?"#)
        .bind(version)
        .execute(&mut **tx)
        .await
        .repository()?;
    Ok(())
}
