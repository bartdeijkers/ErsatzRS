//! Read-only preflight of local sidecars in the next day's native playouts.
use super::super::{
    async_trait, sqlx_error, Db, HealthCheck, HealthCheckResult, HealthCheckStatus, Pool, Result,
    Row,
};
use crate::{playback::PlaybackSubtitle, subtitle_text};

const MAX_SOURCES: usize = 256;
const QUERY: &str = r#"
SELECT DISTINCT s."Id", s."Path", s."Codec", mf."Path" AS "MediaPath"
FROM "Subtitle" s
LEFT JOIN "MovieMetadata" mm ON mm."Id" = s."MovieMetadataId"
LEFT JOIN "EpisodeMetadata" em ON em."Id" = s."EpisodeMetadataId"
LEFT JOIN "MusicVideoMetadata" vm ON vm."Id" = s."MusicVideoMetadataId"
LEFT JOIN "OtherVideoMetadata" om ON om."Id" = s."OtherVideoMetadataId"
LEFT JOIN "SongMetadata" sm ON sm."Id" = s."SongMetadataId"
JOIN "PlayoutItem" i ON i."MediaItemId" = COALESCE(mm."MovieId", em."EpisodeId", vm."MusicVideoId", om."OtherVideoId", sm."SongId")
JOIN "Playout" p ON p."Id" = i."PlayoutId"
JOIN "Channel" c ON c."Id" = p."ChannelId"
JOIN "MediaVersion" mv ON (mv."MovieId" = mm."MovieId" OR mv."EpisodeId" = em."EpisodeId" OR mv."MusicVideoId" = vm."MusicVideoId" OR mv."OtherVideoId" = om."OtherVideoId" OR mv."SongId" = sm."SongId")
JOIN "MediaFile" mf ON mf."MediaVersionId" = mv."Id"
JOIN "LibraryFolder" lf ON lf."Id" = mf."LibraryFolderId"
JOIN "LibraryPath" lp ON lp."Id" = lf."LibraryPathId"
JOIN "LocalLibrary" ll ON ll."Id" = lp."LibraryId"
WHERE s."SubtitleKind" = 1 AND s."Path" IS NOT NULL
AND c."IsEnabled" = 1 AND c."StreamingEngine" = 0
AND datetime(i."Finish") > datetime(?) AND datetime(i."Start") < datetime(?)
AND (i."MediaVersionId" IS NULL OR i."MediaVersionId" = mv."Id")
AND lower(COALESCE(s."Codec", '')) IN ('subrip', 'srt', 'ass', 'ssa', 'webvtt')
ORDER BY s."Id", mf."Path" LIMIT 257
"#;

pub(crate) struct SubtitleEncodingHealthCheck {
    pool: Pool<Db>,
}

impl SubtitleEncodingHealthCheck {
    pub(crate) fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HealthCheck for SubtitleEncodingHealthCheck {
    fn title(&self) -> &'static str {
        "Subtitle Encoding"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        let now = chrono::Utc::now();
        let rows = sqlx::query(QUERY)
            .bind(now.to_rfc3339())
            .bind((now + chrono::Duration::days(1)).to_rfc3339())
            .fetch_all(&self.pool)
            .await
            .map_err(sqlx_error)?;
        let mut invalid = 0usize;
        let mut unchecked = rows.len().saturating_sub(MAX_SOURCES);
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        for (index, row) in rows.iter().take(MAX_SOURCES).enumerate() {
            let subtitle = PlaybackSubtitle {
                id: row.try_get("Id").map_err(sqlx_error)?,
                subtitle_kind: 1,
                path: row.try_get("Path").map_err(sqlx_error)?,
                codec: row.try_get("Codec").map_err(sqlx_error)?,
                title: None,
                stream_index: 0,
                default: false,
                forced: false,
                sdh: false,
                language: None,
                is_extracted: false,
            };
            let media_path: String = row.try_get("MediaPath").map_err(sqlx_error)?;
            let Some(path) = crate::playback::subtitle_text_path(&subtitle, &media_path) else {
                invalid += 1;
                continue;
            };
            // Health never makes provider/network requests. Remote sources are
            // decoded at playback admission and in troubleshooting instead.
            if crate::playback::subtitle_is_http(&path) {
                continue;
            }
            match tokio::time::timeout_at(deadline, subtitle_text::read_source(&path)).await {
                Ok(Ok(_)) => {}
                Ok(Err(_)) => invalid += 1,
                Err(_) => {
                    unchecked += rows.len().min(MAX_SOURCES) - index;
                    break;
                }
            }
        }
        if invalid == 0 && unchecked == 0 {
            return Ok(HealthCheckResult::pass(self.title()));
        }
        Ok(HealthCheckResult::new(
            self.title(), HealthCheckStatus::Warning,
            format!("Upcoming local subtitles need attention: {invalid} unreadable or uncertain source(s); {unchecked} source(s) not checked within the preflight limit. Use playback troubleshooting to identify affected media. Convert the subtitle to UTF-8 or add a .charset companion with its correct encoding, then run Health again. Preflight covers local sidecars scheduled in the next 24 hours; remote sources are checked when playback starts."),
            format!("{invalid} subtitle encoding problem(s); {unchecked} unchecked source(s)"),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn subtitle_encoding_health_warns_before_playback_and_recovers_without_mutation() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
        let check = SubtitleEncodingHealthCheck::new(pool.clone());
        assert_eq!(check.check().await.unwrap().status, HealthCheckStatus::Pass);
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("synthetic-private.srt");
        tokio::fs::write(&path, b"1\n00:00:00,000 --> 00:00:01,000\ncaf\xe9\n")
            .await
            .unwrap();
        // Minimal real repository relationships; a disabled/old row cannot
        // turn current local preflight into an unrelated whole-library scan.
        sqlx::raw_sql(r#"
        INSERT INTO "MediaSource" ("Id") VALUES (1);
        INSERT INTO "LocalMediaSource" ("Id") VALUES (1);
        INSERT INTO "Library" ("Id","MediaKind","MediaSourceId") VALUES (1,0,1);
        INSERT INTO "LocalLibrary" ("Id") VALUES (1);
        INSERT INTO "LibraryPath" ("Id","LibraryId") VALUES (1,1);
        INSERT INTO "LibraryFolder" ("Id","LibraryPathId") VALUES (1,1);
        INSERT INTO "MediaItem" ("Id","LibraryPathId","State") VALUES (1,1,0);
        INSERT INTO "Movie" ("Id") VALUES (1);
        INSERT INTO "MovieMetadata" ("Id","MovieId","MetadataKind","DateAdded","DateUpdated") VALUES (1,1,0,'2026-01-01','2026-01-01');
        INSERT INTO "MediaVersion" ("Id","MovieId","DateAdded","DateUpdated","Duration","Height","Width","VideoScanKind") VALUES (1,1,'2026-01-01','2026-01-01','00:01:00',180,320,0);
        INSERT INTO "MediaFile" ("Id","LibraryFolderId","MediaVersionId","Path","PathHash") VALUES (1,1,1,'synthetic/video.mkv','synthetic');

        "#).execute(&pool).await.unwrap();
        sqlx::query(r#"INSERT INTO "Resolution" ("Id", "Name", "IsCustom", "Width", "Height") VALUES (999, 'synthetic',0,320,180)"#).execute(&pool).await.unwrap();
        sqlx::query(r#"INSERT INTO "FFmpegProfile" ("Id", "AudioBitrate", "AudioBufferSize", "AudioChannels", "AudioFormat", "AudioSampleRate", "BitDepth", "HardwareAcceleration", "Name", "NormalizeFramerate", "NormalizeLoudnessMode", "ResolutionId", "ScalingBehavior", "ThreadCount", "VaapiDriver", "VideoBitrate", "VideoBufferSize", "VideoFormat") VALUES (1,192,384,2,1,48,0,0,'synthetic',0,0,999,0,0,0,2000,4000,1)"#).execute(&pool).await.unwrap();
        sqlx::query(r#"INSERT INTO "Channel" ("Id","Number","FFmpegProfileId","IdleBehavior","MusicVideoCreditsMode","PlayoutMode","PlayoutSource","SongVideoMode","SortNumber","StreamSelectorMode","StreamingMode","SubtitleMode","TranscodeMode","UniqueId") VALUES (1,'1',1,0,0,0,0,0,1,0,0,3,0,'synthetic')"#).execute(&pool).await.unwrap();
        sqlx::query(
            r#"INSERT INTO "Playout" ("Id","ChannelId","ScheduleKind","Seed") VALUES (1,1,0,0)"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        let now = chrono::Utc::now();
        sqlx::query(r#"INSERT INTO "PlayoutItem" ("Id","PlayoutId","MediaItemId","DisableWatermarks","FillerKind","Finish","GuideGroup","InPoint","OutPoint","Start") VALUES (1,1,1,0,0,?,0,'00:00:00','00:01:00',?)"#).bind((now+chrono::Duration::minutes(1)).to_rfc3339()).bind(now.to_rfc3339()).execute(&pool).await.unwrap();
        sqlx::query(r#"INSERT INTO "Subtitle" ("MovieMetadataId","SubtitleKind","StreamIndex","Codec","Path","Default","Forced","DateAdded","DateUpdated") VALUES (1,1,0,'subrip',?,0,0,'2026-01-01','2026-01-01')"#).bind(path.to_str().unwrap()).execute(&pool).await.unwrap();
        let result = check.check().await.unwrap();
        assert_eq!(result.status, HealthCheckStatus::Warning);
        assert_eq!(
            result.brief_message,
            "1 subtitle encoding problem(s); 0 unchecked source(s)"
        );
        assert!(!serde_json::to_string(&result)
            .unwrap()
            .contains("synthetic-private"));
        tokio::fs::write(
            root.path().join("synthetic-private.srt.charset"),
            "windows-1252",
        )
        .await
        .unwrap();
        assert_eq!(check.check().await.unwrap().status, HealthCheckStatus::Pass);
        assert_eq!(
            sqlx::query_scalar::<_, i64>(r#"SELECT COUNT(*) FROM "Subtitle""#)
                .fetch_one(&pool)
                .await
                .unwrap(),
            1
        );
    }
}
