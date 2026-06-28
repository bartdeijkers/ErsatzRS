use super::super::{
    acceleration_name, async_trait, sqlx_error, supported_acceleration_kinds, Db,
    FfmpegRuntimeReport, HealthCheck, HealthCheckResult, HealthCheckStatus, Pool, Result, Row,
    STREAMING_MODE_HLS_DIRECT,
};

pub(crate) struct HardwareAccelerationHealthCheck {
    pool: Pool<Db>,
    runtime: FfmpegRuntimeReport,
}

impl HardwareAccelerationHealthCheck {
    pub(crate) fn new(pool: Pool<Db>, runtime: FfmpegRuntimeReport) -> Self {
        Self { pool, runtime }
    }
}

#[async_trait]
impl HealthCheck for HardwareAccelerationHealthCheck {
    fn title(&self) -> &'static str {
        "Hardware Acceleration"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        let supported = supported_acceleration_kinds(&self.runtime);
        if supported.is_empty() {
            return Ok(HealthCheckResult::new(
                self.title(),
                HealthCheckStatus::Info,
                "No compatible hardware acceleration kinds are supported by ffmpeg",
                "FFmpeg does not support hardware acceleration",
            ));
        }

        let rows = sqlx::query(
            r#"SELECT c."Number", c."Name", p."HardwareAcceleration"
               FROM "Channel" c
               INNER JOIN "FFmpegProfile" p ON p."Id" = c."FFmpegProfileId"
               WHERE c."StreamingMode" != ?
               ORDER BY c."SortNumber", c."Number", c."Id""#,
        )
        .bind(STREAMING_MODE_HLS_DIRECT)
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_error)?;

        let bad_channels: Vec<String> = rows
            .iter()
            .filter_map(|row| {
                let acceleration = row.try_get::<i32, _>("HardwareAcceleration").ok()?;
                (!supported.contains(&acceleration)).then(|| {
                    let number = row.try_get::<Option<String>, _>("Number").ok().flatten();
                    let name = row.try_get::<Option<String>, _>("Name").ok().flatten();
                    format!(
                        "{} - {}",
                        number.unwrap_or_else(|| "?".to_owned()),
                        name.unwrap_or_else(|| "Unnamed".to_owned())
                    )
                })
            })
            .collect();

        if bad_channels.is_empty() {
            return Ok(HealthCheckResult::pass(self.title()));
        }

        Ok(HealthCheckResult::new(
            self.title(),
            HealthCheckStatus::Warning,
            format!(
                "The following channels use ffmpeg profiles that are not configured for hardware acceleration ({}): {}",
                supported.iter().map(|kind| acceleration_name(*kind)).collect::<Vec<_>>().join(", "),
                bad_channels.join(", ")
            ),
            format!(
                "{} channels are not configured for hardware acceleration",
                bad_channels.len()
            ),
        ))
    }
}
