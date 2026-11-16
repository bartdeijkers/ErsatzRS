//! ADR 0137: stable provider identities and separately qualified physical routing.
use super::{JellyfinPlaybackMetadata, MediaStream, MediaStreamKind, RepositoryError, Result};
use crate::{db::Db, repositories::SqlxResultExt as _};
use sqlx::Transaction;

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
