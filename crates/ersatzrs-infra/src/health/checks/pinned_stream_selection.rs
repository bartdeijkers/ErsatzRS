use super::super::{
    async_trait, sqlx_error, Db, HealthCheck, HealthCheckResult, HealthCheckStatus, Pool, Result,
};

const PINNED_SELECTION_QUERY: &str = r#"SELECT COUNT(*)
FROM (
    SELECT "MediaItemId", "MediaVersionId", "VideoStreamIndex",
           "AudioStreamIndex", "SubtitleStreamIndex"
    FROM "PlaylistItem"
    WHERE "MediaVersionId" IS NOT NULL
    UNION ALL
    SELECT "MediaItemId", "MediaVersionId", "VideoStreamIndex",
           "AudioStreamIndex", "SubtitleStreamIndex"
    FROM "ProgramScheduleItem"
    WHERE "MediaVersionId" IS NOT NULL
    UNION ALL
    SELECT "MediaItemId", "MediaVersionId", "VideoStreamIndex",
           "AudioStreamIndex", "SubtitleStreamIndex"
    FROM "PlayoutItem"
    WHERE "MediaVersionId" IS NOT NULL
) pinned
WHERE pinned."MediaItemId" IS NULL
   OR pinned."VideoStreamIndex" IS NULL
   OR NOT EXISTS (
       SELECT 1
       FROM "MediaVersion" mv
       WHERE mv."Id" = pinned."MediaVersionId"
         AND (
                mv."MovieId" = pinned."MediaItemId"
             OR mv."EpisodeId" = pinned."MediaItemId"
             OR mv."MusicVideoId" = pinned."MediaItemId"
             OR mv."OtherVideoId" = pinned."MediaItemId"
             OR mv."RemoteStreamId" = pinned."MediaItemId"
             OR mv."SongId" = pinned."MediaItemId"
             OR mv."ImageId" = pinned."MediaItemId"
         )
   )
   OR (
       NOT EXISTS (
           SELECT 1
           FROM "MediaStream" stream
           WHERE stream."MediaVersionId" = pinned."MediaVersionId"
             AND stream."MediaStreamKind" = 1
             AND stream."AttachedPic" = 0
             AND stream."Index" = pinned."VideoStreamIndex"
       )
       AND NOT (
           pinned."VideoStreamIndex" = 0
           AND EXISTS (
               SELECT 1 FROM "MediaStream" stream
               WHERE stream."MediaVersionId" = pinned."MediaVersionId"
                 AND stream."MediaStreamKind" = 2
           )
           AND NOT EXISTS (
               SELECT 1 FROM "MediaStream" stream
               WHERE stream."MediaVersionId" = pinned."MediaVersionId"
                 AND stream."MediaStreamKind" = 1
                 AND stream."AttachedPic" = 0
           )
       )
   )
   OR (
       pinned."AudioStreamIndex" IS NOT NULL
       AND NOT EXISTS (
           SELECT 1
           FROM "MediaStream" stream
           WHERE stream."MediaVersionId" = pinned."MediaVersionId"
             AND stream."MediaStreamKind" = 2
             AND stream."Index" = pinned."AudioStreamIndex"
       )
   )
   OR (
       pinned."SubtitleStreamIndex" IS NOT NULL
       AND NOT EXISTS (
           SELECT 1
           FROM "MediaStream" stream
           WHERE stream."MediaVersionId" = pinned."MediaVersionId"
             AND stream."MediaStreamKind" = 3
             AND stream."Index" = pinned."SubtitleStreamIndex"
       )
       AND NOT EXISTS (
           SELECT 1
           FROM "Subtitle" subtitle
           LEFT JOIN "MovieMetadata" movie
             ON movie."Id" = subtitle."MovieMetadataId"
           LEFT JOIN "EpisodeMetadata" episode
             ON episode."Id" = subtitle."EpisodeMetadataId"
           LEFT JOIN "MusicVideoMetadata" music_video
             ON music_video."Id" = subtitle."MusicVideoMetadataId"
           LEFT JOIN "OtherVideoMetadata" other_video
             ON other_video."Id" = subtitle."OtherVideoMetadataId"
           LEFT JOIN "RemoteStreamMetadata" remote_stream
             ON remote_stream."Id" = subtitle."RemoteStreamMetadataId"
           LEFT JOIN "SongMetadata" song
             ON song."Id" = subtitle."SongMetadataId"
           WHERE subtitle."StreamIndex" = pinned."SubtitleStreamIndex"
             AND (
                    movie."MovieId" = pinned."MediaItemId"
                 OR episode."EpisodeId" = pinned."MediaItemId"
                 OR music_video."MusicVideoId" = pinned."MediaItemId"
                 OR other_video."OtherVideoId" = pinned."MediaItemId"
                 OR remote_stream."RemoteStreamId" = pinned."MediaItemId"
                 OR song."SongId" = pinned."MediaItemId"
             )
       )
   )"#;

pub(crate) struct PinnedStreamSelectionHealthCheck {
    pool: Pool<Db>,
}

impl PinnedStreamSelectionHealthCheck {
    pub(crate) fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HealthCheck for PinnedStreamSelectionHealthCheck {
    fn title(&self) -> &'static str {
        "Pinned Stream Selection"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        let invalid = sqlx::query_scalar::<_, i64>(PINNED_SELECTION_QUERY)
            .fetch_one(&self.pool)
            .await
            .map_err(sqlx_error)?;

        if invalid == 0 {
            return Ok(HealthCheckResult::pass(self.title()));
        }

        Ok(HealthCheckResult::new(
            self.title(),
            HealthCheckStatus::Fail,
            format!(
                "{invalid} scheduled or playlist item(s) reference a media version or stream that no longer exists. Playback is blocked to prevent selecting different content."
            ),
            format!("{invalid} stale pinned stream selection(s)"),
        )
        .with_link("/media/movies"))
    }
}

#[cfg(test)]
#[path = "pinned_stream_selection_tests.rs"]
mod tests;
