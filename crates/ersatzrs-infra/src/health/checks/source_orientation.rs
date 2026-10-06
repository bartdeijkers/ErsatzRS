use super::super::{
    async_trait, sqlx_error, Db, HealthCheck, HealthCheckResult, HealthCheckStatus, Pool, Result,
    Row,
};

/// `MediaStreamKind` = `Video`.
const MEDIA_STREAM_KIND_VIDEO: i32 = 1;
/// `FFmpegProfileVideoFormat` = `Copy`.
const VIDEO_FORMAT_COPY: i32 = 99;
/// Most an operator needs to see before the point is made.
const MAX_NAMED_ITEMS: usize = 5;

/// Report rotated items scheduled on channels that stream-copy video
/// (ADR 0003, milestone `p8c-02`).
///
/// A display matrix is container metadata, and MPEG-TS has no field for it.
/// Measured with `FFmpeg` 9 on a 90°-tagged stream:
///
/// | Output | Rotation after `-c copy` |
/// |---|---|
/// | MP4, Matroska, HLS with fMP4 segments | preserved |
/// | MPEG-TS, HLS with MPEG-TS segments | lost |
///
/// Every default serving path carries MPEG-TS, so a stream-copied portrait item
/// reaches the client with no rotation and is shown sideways. Transcoding is
/// unaffected: autorotation bakes the rotation into the pixels, which is why
/// this is scoped to copy profiles.
///
/// ErsatzRS reports rather than silently forcing a transcode. Copy is an
/// explicit operator choice with a large CPU cost attached, and overriding it
/// to fix geometry would trade one surprise for a bigger one.
pub(crate) struct SourceOrientationHealthCheck {
    pool: Pool<Db>,
}

impl SourceOrientationHealthCheck {
    pub(crate) fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HealthCheck for SourceOrientationHealthCheck {
    fn title(&self) -> &'static str {
        "Source Orientation"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        let rows = sqlx::query(
            // MediaVersion carries one nullable id per item type (ErsatzTV's
            // table-per-type layout), and exactly one is set, so coalescing
            // them yields the MediaItem id the playout references.
            //
            // Every ORDER BY column is also projected, which MySQL 8 requires
            // alongside DISTINCT.
            r#"SELECT DISTINCT c."Number", c."Name" AS "ChannelName", mf."Path", ms."Rotation"
               FROM "Channel" c
               INNER JOIN "FFmpegProfile" p ON p."Id" = c."FFmpegProfileId"
               INNER JOIN "Playout" po ON po."ChannelId" = c."Id"
               INNER JOIN "PlayoutItem" pi ON pi."PlayoutId" = po."Id"
               INNER JOIN "MediaVersion" mv
                       ON COALESCE(mv."MovieId", mv."EpisodeId", mv."MusicVideoId",
                                   mv."OtherVideoId", mv."SongId", mv."ImageId",
                                   mv."RemoteStreamId") = pi."MediaItemId"
               INNER JOIN "MediaStream" ms ON ms."MediaVersionId" = mv."Id"
               INNER JOIN "MediaFile" mf ON mf."MediaVersionId" = mv."Id"
               WHERE ms."MediaStreamKind" = ?
                 AND ms."Rotation" IS NOT NULL
                 AND ms."Rotation" <> 0
                 AND (p."TranscodeVideo" = 0 OR p."VideoFormat" = ?)
               ORDER BY c."Number", c."Name", mf."Path", ms."Rotation""#,
        )
        .bind(MEDIA_STREAM_KIND_VIDEO)
        .bind(VIDEO_FORMAT_COPY)
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_error)?;

        if rows.is_empty() {
            return Ok(HealthCheckResult::not_applicable(self.title()));
        }

        let mut affected = Vec::new();
        for row in &rows {
            let number = row.try_get::<String, _>("Number").unwrap_or_default();
            let path = row.try_get::<String, _>("Path").unwrap_or_default();
            let rotation = row.try_get::<i32, _>("Rotation").unwrap_or_default();
            affected.push(format!("{path} ({rotation}° on channel {number})"));
        }

        let named = affected
            .iter()
            .take(MAX_NAMED_ITEMS)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        let remainder = affected.len().saturating_sub(MAX_NAMED_ITEMS);
        let suffix = if remainder > 0 {
            format!(", and {remainder} more")
        } else {
            String::new()
        };

        Ok(HealthCheckResult::new(
            self.title(),
            HealthCheckStatus::Warning,
            format!(
                "{} rotated item{} scheduled on channels that copy video; MPEG-TS cannot carry a display matrix, so clients will show {} sideways: {named}{suffix}. Transcode the channel, or request the fMP4 HLS variant, which preserves rotation.",
                affected.len(),
                if affected.len() > 1 { "s are" } else { " is" },
                if affected.len() > 1 { "them" } else { "it" },
            ),
            "Stream-copied rotation is lost in MPEG-TS",
        ))
    }
}
