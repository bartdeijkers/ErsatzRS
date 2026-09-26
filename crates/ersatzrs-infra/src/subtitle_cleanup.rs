//! Database-only subtitle cleanup matching upstream `DeleteOrphanedSubtitlesHandler`.

use sqlx::Pool;

use crate::db::{assert_sql_safe, Db};

pub(crate) const INVALID_SUBTITLE: &str = r#"
    ("ArtistMetadataId" IS NULL AND "EpisodeMetadataId" IS NULL
    AND "ImageMetadataId" IS NULL AND "MovieMetadataId" IS NULL
    AND "MusicVideoMetadataId" IS NULL AND "OtherVideoMetadataId" IS NULL
    AND "RemoteStreamMetadataId" IS NULL AND "SeasonMetadataId" IS NULL
    AND "ShowMetadataId" IS NULL AND "SongMetadataId" IS NULL)
    OR ("SubtitleKind" = 1 AND ("Path" IS NULL OR "Path" = '') AND (
        EXISTS (SELECT 1 FROM "MovieMetadata" M
            INNER JOIN "MediaItem" I ON I."Id" = M."MovieId"
            INNER JOIN "LibraryPath" P ON P."Id" = I."LibraryPathId"
            INNER JOIN "LocalLibrary" L ON L."Id" = P."LibraryId"
            WHERE M."Id" = "Subtitle"."MovieMetadataId")
        OR
        EXISTS (SELECT 1 FROM "EpisodeMetadata" M
            INNER JOIN "MediaItem" I ON I."Id" = M."EpisodeId"
            INNER JOIN "LibraryPath" P ON P."Id" = I."LibraryPathId"
            INNER JOIN "LocalLibrary" L ON L."Id" = P."LibraryId"
            WHERE M."Id" = "Subtitle"."EpisodeMetadataId")
        OR
        EXISTS (SELECT 1 FROM "MusicVideoMetadata" M
            INNER JOIN "MediaItem" I ON I."Id" = M."MusicVideoId"
            INNER JOIN "LibraryPath" P ON P."Id" = I."LibraryPathId"
            INNER JOIN "LocalLibrary" L ON L."Id" = P."LibraryId"
            WHERE M."Id" = "Subtitle"."MusicVideoMetadataId")
        OR
        EXISTS (SELECT 1 FROM "OtherVideoMetadata" M
            INNER JOIN "MediaItem" I ON I."Id" = M."OtherVideoId"
            INNER JOIN "LibraryPath" P ON P."Id" = I."LibraryPathId"
            INNER JOIN "LocalLibrary" L ON L."Id" = P."LibraryId"
            WHERE M."Id" = "Subtitle"."OtherVideoMetadataId")
    ))"#;

/// Remove ownerless subtitles and local video sidecars without a path.
///
/// Each batch of at most 5,000 rows is an atomic SQL statement. At most `limit`
/// rows are removed per pass; subsequent scheduled passes drain any remainder.
/// Cached files are preserved. Provider sidecars, embedded and generated
/// subtitles may legitimately have no path.
/// A failed batch rolls back; earlier batches remain committed.
///
/// # Errors
///
/// Returns an error if database deletion fails.
pub async fn clean_orphaned_subtitles(pool: &Pool<Db>, limit: u32) -> Result<u64, sqlx::Error> {
    let mut deleted = 0;
    while deleted < u64::from(limit) {
        let batch = (u64::from(limit) - deleted).min(5_000);
        let count = sqlx::query(assert_sql_safe(format!(
            r#"DELETE FROM "Subtitle" WHERE "Id" IN (
                SELECT "Id" FROM "Subtitle" WHERE {INVALID_SUBTITLE}
                ORDER BY "Id" LIMIT ?
            )"#,
        )))
        .bind(i64::try_from(batch).unwrap_or(5_000))
        .execute(pool)
        .await?
        .rows_affected();
        deleted += count;
        if count == 0 {
            break;
        }
    }
    Ok(deleted)
}
