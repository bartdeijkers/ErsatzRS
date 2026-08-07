use super::super::{
    async_trait, sqlx_error, Db, FfmpegRuntimeReport, HealthCheck, HealthCheckResult,
    HealthCheckStatus, Path, Pool, Result, Row, HW_ACCEL_VAAPI,
};

pub(crate) struct VaapiDeviceHealthCheck {
    pool: Pool<Db>,
    runtime: FfmpegRuntimeReport,
}

impl VaapiDeviceHealthCheck {
    pub(crate) fn new(pool: Pool<Db>, runtime: FfmpegRuntimeReport) -> Self {
        Self { pool, runtime }
    }
}

#[async_trait]
impl HealthCheck for VaapiDeviceHealthCheck {
    fn title(&self) -> &'static str {
        "VAAPI Device"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        if !self.runtime.capabilities.has_hwaccel("vaapi") {
            return Ok(HealthCheckResult::not_applicable(self.title()));
        }

        let rows = sqlx::query(
            // See the note in `output_dynamic_range.rs`: MySQL 8 rejects an
            // ORDER BY column that is absent from a DISTINCT projection.
            r#"SELECT DISTINCT p."Name", p."VaapiDevice"
               FROM "Channel" c
               INNER JOIN "FFmpegProfile" p ON p."Id" = c."FFmpegProfileId"
               WHERE p."HardwareAcceleration" = ?
                 AND p."VaapiDevice" IS NOT NULL
                 AND p."VaapiDevice" != ''
               ORDER BY p."Name", p."VaapiDevice""#,
        )
        .bind(HW_ACCEL_VAAPI)
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_error)?;

        if rows.is_empty() {
            return Ok(HealthCheckResult::not_applicable(self.title()));
        }

        let missing: Vec<String> = rows
            .iter()
            .filter_map(|row| {
                let device = row
                    .try_get::<Option<String>, _>("VaapiDevice")
                    .ok()
                    .flatten()?;
                if Path::new(&device).exists() {
                    None
                } else {
                    let name = row
                        .try_get::<Option<String>, _>("Name")
                        .ok()
                        .flatten()
                        .unwrap_or_else(|| "Unnamed".to_owned());
                    Some(format!("{name} ({device})"))
                }
            })
            .collect();

        if missing.is_empty() {
            return Ok(HealthCheckResult::pass(self.title()));
        }

        Ok(HealthCheckResult::new(
            self.title(),
            HealthCheckStatus::Fail,
            format!(
                "FFmpeg Profile{} configured for VAAPI {} a device that does not exist on disk: {}",
                if missing.len() > 1 { "s" } else { "" },
                if missing.len() > 1 {
                    "reference"
                } else {
                    "references"
                },
                missing.join(", ")
            ),
            "Configured VAAPI device does not exist",
        ))
    }
}
