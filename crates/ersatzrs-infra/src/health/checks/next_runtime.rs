use super::super::{
    async_trait, sqlx_error, Db, HealthCheck, HealthCheckResult, HealthCheckStatus, Pool, Result,
};
use std::{
    io,
    path::{Path, PathBuf},
};

pub(crate) struct NextRuntimeHealthCheck {
    pool: Pool<Db>,
    resolve: fn() -> io::Result<PathBuf>,
}

impl NextRuntimeHealthCheck {
    pub(crate) fn new(pool: Pool<Db>) -> Self {
        Self {
            pool,
            resolve: crate::next_engine::host::channel_binary,
        }
    }
}

#[async_trait]
impl HealthCheck for NextRuntimeHealthCheck {
    fn title(&self) -> &'static str {
        "Next Runtime Availability"
    }
    async fn check(&self) -> Result<HealthCheckResult> {
        let channels: Vec<i64> = sqlx::query_scalar(r#"SELECT "Id" FROM "Channel" WHERE "StreamingEngine"=1 AND "IsEnabled"=1 ORDER BY "Id""#)
            .fetch_all(&self.pool).await.map_err(sqlx_error)?;
        if channels.is_empty() {
            return Ok(HealthCheckResult::new(
                self.title(),
                HealthCheckStatus::NotApplicable,
                "No enabled channel selects the Next engine",
                "Next is not selected",
            ));
        }
        if (self.resolve)().is_err() {
            return Ok(HealthCheckResult::new(self.title(), HealthCheckStatus::Fail,
                "Enabled Next channels require the bundled ersatztv-channel executable or a valid ETV_NEXT_FOLDER override",
                "Next executable is unavailable").with_link("/channels"));
        }
        let mut invalid = 0;
        for channel in channels {
            let profile = crate::playback::load_channel_playback_profile(&self.pool, channel).await;
            let supported = matches!(profile, Ok(Some(ref profile)) if crate::next_engine::config::channel_config(profile,
                Path::new("next-playouts"), Path::new("ffmpeg"), Path::new("ffprobe")).is_ok());
            if !supported {
                invalid += 1;
            }
        }
        if invalid > 0 {
            return Ok(HealthCheckResult::new(self.title(), HealthCheckStatus::Fail,
                format!("{invalid} enabled Next channel profile(s) are unavailable or unsupported; review codecs, HDR, resolution and acceleration"),
                "Next channel profile is unavailable or unsupported").with_link("/channels"));
        }
        Ok(HealthCheckResult::pass(self.title()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn next_runtime_is_required_only_when_selected_and_profiles_are_checked() {
        let (pool, _, _, _) = crate::next_engine::schedule::tests::fixture().await;
        let mut check = NextRuntimeHealthCheck {
            pool: pool.clone(),
            resolve: || Err(io::Error::from(io::ErrorKind::NotFound)),
        };
        assert_eq!(
            check.check().await.unwrap().status,
            HealthCheckStatus::NotApplicable
        );
        sqlx::query(r#"UPDATE "Channel" SET "StreamingEngine"=1, "IsEnabled"=1"#)
            .execute(&pool)
            .await
            .unwrap();
        let missing = check.check().await.unwrap();
        assert_eq!(missing.status, HealthCheckStatus::Fail);
        assert!(missing.brief_message.contains("executable"));
        check.resolve = || Ok(PathBuf::from("synthetic-engine"));
        assert_eq!(check.check().await.unwrap().status, HealthCheckStatus::Fail);
        sqlx::query(r#"UPDATE "FFmpegProfile" SET "VideoFormat"=1, "AudioFormat"=1"#)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(check.check().await.unwrap().status, HealthCheckStatus::Pass);
    }
}
