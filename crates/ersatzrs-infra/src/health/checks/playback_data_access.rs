use ersatzrs_core::health::{
    HealthCheck, HealthCheckError, HealthCheckResult, HealthCheckStatus, Result,
};
use ersatzrs_core::repositories::RepositoryError;

use crate::db::Db;
use crate::ffmpeg_config::playback_language_preferences;
use crate::hls_worker::load_hls_work_ahead_limit;
use crate::transcode_capacity::load_active_transcode_limit;
use sqlx::Pool;

pub(crate) struct PlaybackDataAccessHealthCheck {
    pool: Pool<Db>,
}

impl PlaybackDataAccessHealthCheck {
    pub(crate) fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }

    fn invalid_value(setting: &str, error: &RepositoryError) -> HealthCheckResult {
        HealthCheckResult::new(
            "Playback Data Access",
            HealthCheckStatus::Fail,
            format!("The persisted {setting} is invalid: {error}"),
            format!("Invalid {setting}"),
        )
        .with_link("/system/ffmpeg")
    }

    fn operational_error(setting: &str, error: &RepositoryError) -> HealthCheckError {
        HealthCheckError::Database(format!("failed to load {setting}: {error}"))
    }
}

#[async_trait::async_trait]
impl HealthCheck for PlaybackDataAccessHealthCheck {
    fn title(&self) -> &'static str {
        "Playback Data Access"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        let active_transcode_limit = match load_active_transcode_limit(&self.pool).await {
            Ok(limit) => limit,
            Err(error) => {
                return match error {
                    error @ RepositoryError::Decode { .. } => {
                        Ok(Self::invalid_value("active transcode limit", &error))
                    }
                    error => Err(Self::operational_error("active transcode limit", &error)),
                }
            }
        };

        if let Err(error) = load_hls_work_ahead_limit(&self.pool).await {
            return match error {
                error @ RepositoryError::Decode { .. } => {
                    Ok(Self::invalid_value("HLS work-ahead limit", &error))
                }
                error => Err(Self::operational_error("HLS work-ahead limit", &error)),
            };
        }

        if let Err(error) = playback_language_preferences(&self.pool).await {
            return Err(Self::operational_error(
                "playback language preferences",
                &error,
            ));
        }

        if active_transcode_limit == Some(1) {
            // Only the configured default path is known here. Explicit request
            // mode overrides are resolved by the host; Next coerces legacy TS
            // requests to its hybrid wrapper. Copy profiles still need a native
            // legacy wrapper and a distinct programme FFmpeg process.
            let needs_legacy_wrapper: bool = sqlx::query_scalar(
                r#"SELECT EXISTS (
                     SELECT 1 FROM "Channel"
                     WHERE "IsEnabled" = 1 AND "StreamingEngine" = 0
                       AND "StreamingMode" = 1
                   )"#,
            )
            .fetch_one(&self.pool)
            .await
            .map_err(super::super::sqlx_error)?;
            if needs_legacy_wrapper {
                return Ok(HealthCheckResult::new(
                    self.title(),
                    HealthCheckStatus::Fail,
                    "An enabled channel uses the native legacy MPEG-TS mode. Its wrapper and programme encoder require two distinct active transcode slots. Increase the active transcode limit to at least 2, or select another streaming mode.",
                    "Legacy MPEG-TS requires at least two active transcode slots",
                )
                .with_link("/system/ffmpeg"));
            }
        }

        Ok(HealthCheckResult::pass(self.title()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn fixture() -> (tempfile::TempDir, Pool<Db>) {
        let folder = tempfile::tempdir().expect("create playback health database folder");
        let url = format!("sqlite:{}", folder.path().join("health.db").display());
        let pool = crate::db::init(&url).await.expect("initialize database");
        sqlx::query(
            r#"INSERT INTO "Resolution" ("Id", "Name", "IsCustom", "Width", "Height")
               VALUES (50, '1920x1080', 0, 1920, 1080)"#,
        )
        .execute(&pool)
        .await
        .expect("seed resolution");
        sqlx::query(
            r#"INSERT INTO "FFmpegProfile" (
                 "Id", "AudioBitrate", "AudioBufferSize", "AudioChannels", "AudioFormat",
                 "AudioSampleRate", "BitDepth", "HardwareAcceleration", "Name",
                 "NormalizeFramerate", "NormalizeLoudnessMode", "ResolutionId",
                 "ScalingBehavior", "ThreadCount", "VaapiDriver", "VideoBitrate",
                 "VideoBufferSize", "VideoFormat"
               ) VALUES (50, 192, 384, 2, 1, 48, 0, 0, 'Health', 0, 0, 50, 0, 0, 0, 2000, 4000, 1)"#,
        )
        .execute(&pool)
        .await
        .expect("seed profile");
        sqlx::query(
            r#"INSERT INTO "Channel" (
                 "Id", "UniqueId", "Number", "SortNumber", "Name", "FFmpegProfileId",
                 "IdleBehavior", "MusicVideoCreditsMode", "PlayoutMode", "PlayoutSource",
                 "SongVideoMode", "StreamingMode", "StreamSelectorMode", "SubtitleMode",
                 "TranscodeMode", "StreamingEngine", "IsEnabled"
               ) VALUES (
                 50, '00000000-0000-0000-0000-000000000050', '50', 50.0, 'Health', 50,
                 0, 0, 1, 0, 0, 1, 0, 0, 0, 0, 1
               )"#,
        )
        .execute(&pool)
        .await
        .expect("seed native legacy channel");
        (folder, pool)
    }

    async fn set_config(pool: &Pool<Db>, key: &str, value: &str) {
        super::super::super::upsert_config(pool, key, value)
            .await
            .expect("set playback configuration");
    }

    #[tokio::test]
    async fn legacy_wrapper_capacity_health_checks_only_enabled_native_legacy_defaults() {
        let (_folder, pool) = fixture().await;
        set_config(
            &pool,
            ersatzrs_domain::CONFIG_KEY_FFMPEG_ACTIVE_TRANSCODE_LIMIT,
            "1",
        )
        .await;
        let check = PlaybackDataAccessHealthCheck::new(pool.clone());
        for (enabled, engine, mode, expected) in [
            (true, 0, 1, HealthCheckStatus::Fail),
            (false, 0, 1, HealthCheckStatus::Pass),
            (true, 1, 1, HealthCheckStatus::Pass),
            (true, 1, 5, HealthCheckStatus::Pass),
            (true, 0, 2, HealthCheckStatus::Pass),
            (true, 0, 4, HealthCheckStatus::Pass),
            (true, 0, 5, HealthCheckStatus::Pass),
        ] {
            sqlx::query(
                r#"UPDATE "Channel" SET "IsEnabled" = ?, "StreamingEngine" = ?,
                   "StreamingMode" = ? WHERE "Id" = 50"#,
            )
            .bind(enabled)
            .bind(engine)
            .bind(mode)
            .execute(&pool)
            .await
            .expect("configure channel");
            assert_eq!(
                check.check().await.expect("check capacity").status,
                expected,
                "enabled={enabled}, engine={engine}, mode={mode}"
            );
        }
        pool.close().await;
    }

    #[tokio::test]
    async fn legacy_wrapper_capacity_health_includes_copy_profiles_and_accepts_two_slots() {
        let (_folder, pool) = fixture().await;
        sqlx::query(
            r#"UPDATE "FFmpegProfile" SET "TranscodeVideo" = 0, "TranscodeAudio" = 0
               WHERE "Id" = 50"#,
        )
        .execute(&pool)
        .await
        .expect("configure copy profile");
        let check = PlaybackDataAccessHealthCheck::new(pool.clone());
        let key = ersatzrs_domain::CONFIG_KEY_FFMPEG_ACTIVE_TRANSCODE_LIMIT;
        set_config(&pool, key, "1").await;
        let result = check.check().await.expect("check copy capacity");
        assert_eq!(result.status, HealthCheckStatus::Fail);
        assert_eq!(
            result.brief_message,
            "Legacy MPEG-TS requires at least two active transcode slots"
        );
        set_config(&pool, key, "2").await;
        assert_eq!(
            check.check().await.expect("check two slots").status,
            HealthCheckStatus::Pass
        );
        sqlx::query(r#"DELETE FROM "ConfigElement" WHERE "Key" = ?"#)
            .bind(key)
            .execute(&pool)
            .await
            .expect("remove configured limit");
        assert_eq!(
            check.check().await.expect("check default limit").status,
            HealthCheckStatus::Pass
        );
        pool.close().await;
    }

    #[tokio::test]
    async fn legacy_wrapper_capacity_health_preserves_configuration_and_database_errors() {
        let (_folder, pool) = fixture().await;
        let check = PlaybackDataAccessHealthCheck::new(pool.clone());
        let key = ersatzrs_domain::CONFIG_KEY_FFMPEG_ACTIVE_TRANSCODE_LIMIT;
        set_config(&pool, key, "invalid").await;
        let result = check.check().await.expect("check invalid limit");
        assert_eq!(result.status, HealthCheckStatus::Fail);
        assert_eq!(result.brief_message, "Invalid active transcode limit");
        set_config(&pool, key, "1").await;
        set_config(
            &pool,
            ersatzrs_domain::CONFIG_KEY_FFMPEG_WORK_AHEAD_SEGMENTERS,
            "invalid",
        )
        .await;
        let result = check.check().await.expect("check invalid work-ahead");
        assert_eq!(result.brief_message, "Invalid HLS work-ahead limit");
        pool.close().await;
        assert!(matches!(
            check.check().await,
            Err(HealthCheckError::Database(_))
        ));

        let (_folder, pool) = fixture().await;
        set_config(&pool, key, "1").await;
        sqlx::query(r#"ALTER TABLE "Channel" RENAME TO "UnavailableChannel""#)
            .execute(&pool)
            .await
            .expect("make channel query unavailable");
        let check = PlaybackDataAccessHealthCheck::new(pool.clone());
        assert!(matches!(
            check.check().await,
            Err(HealthCheckError::Database(_))
        ));
        pool.close().await;
    }
}
