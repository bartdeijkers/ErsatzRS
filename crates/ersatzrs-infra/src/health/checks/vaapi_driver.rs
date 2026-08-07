use super::super::{
    async_trait, sqlx_error, Db, FfmpegRuntimeReport, HealthCheck, HealthCheckResult,
    HealthCheckStatus, Pool, Result, Row, HW_ACCEL_VAAPI, VAAPI_DRIVER_AUTO,
};

pub(crate) struct VaapiDriverHealthCheck {
    pool: Pool<Db>,
    runtime: FfmpegRuntimeReport,
}

impl VaapiDriverHealthCheck {
    pub(crate) fn new(pool: Pool<Db>, runtime: FfmpegRuntimeReport) -> Self {
        Self { pool, runtime }
    }
}

#[async_trait]
impl HealthCheck for VaapiDriverHealthCheck {
    fn title(&self) -> &'static str {
        "VAAPI Driver"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        let rows = sqlx::query(
            // See the note in `output_dynamic_range.rs`: MySQL 8 rejects an
            // ORDER BY column that is absent from a DISTINCT projection.
            r#"SELECT DISTINCT p."Name", p."VaapiDevice", p."VaapiDriver"
               FROM "Channel" c
               INNER JOIN "FFmpegProfile" p ON p."Id" = c."FFmpegProfileId"
               WHERE p."HardwareAcceleration" = ?
               ORDER BY p."Name", p."VaapiDevice", p."VaapiDriver""#,
        )
        .bind(HW_ACCEL_VAAPI)
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_error)?;

        if rows.is_empty() {
            return Ok(HealthCheckResult::not_applicable(self.title()));
        }

        if !self.runtime.capabilities.has_hwaccel("vaapi") {
            return Ok(HealthCheckResult::new(
                self.title(),
                HealthCheckStatus::Fail,
                "FFmpeg Profile is configured for VAAPI, but ffmpeg reports no VAAPI hardware acceleration capability. Hardware Acceleration WILL NOT WORK as configured.",
                "Hardware acceleration WILL NOT WORK...",
            ));
        }

        let default_driver_count = rows
            .iter()
            .filter(|row| {
                row.try_get::<i32, _>("VaapiDriver")
                    .is_ok_and(|driver| driver == VAAPI_DRIVER_AUTO)
            })
            .count();

        if default_driver_count > 0 {
            return Ok(HealthCheckResult::new(
                self.title(),
                HealthCheckStatus::Info,
                format!(
                    "{} FFmpeg Profile{} set to use Default VAAPI Driver; selecting iHD (Gen 8+) or i965 (up to Gen 9) may offer better performance with Intel iGPU",
                    default_driver_count,
                    if default_driver_count > 1 { "s are" } else { " is" }
                ),
                "Default VAAPI driver is not optimal",
            ));
        }

        Ok(HealthCheckResult::pass(self.title()))
    }
}
