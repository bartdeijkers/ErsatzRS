use super::super::{
    async_trait, load_config, runtime_issue_name, Db, FfmpegRuntimeReport, HealthCheck,
    HealthCheckResult, HealthCheckStatus, Pool, Result,
};

pub(crate) struct FFmpegCapabilitiesHealthCheck {
    runtime: FfmpegRuntimeReport,
    pool: Pool<Db>,
}

impl FFmpegCapabilitiesHealthCheck {
    pub(crate) fn new(runtime: FfmpegRuntimeReport, pool: Pool<Db>) -> Self {
        Self { runtime, pool }
    }
}

#[async_trait]
impl HealthCheck for FFmpegCapabilitiesHealthCheck {
    fn title(&self) -> &'static str {
        "FFmpeg Capabilities"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        // ADR 0106: AV1 needs the native fMP4 HLS route, not MPEG-TS wrappers.
        // Persisted streaming modes: legacy TS=1, HLS Direct=2, HLS=4, Hybrid=5.
        let incompatible: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM "Channel" c
               JOIN "FFmpegProfile" p ON p."Id" = c."FFmpegProfileId"
               WHERE c."IsEnabled" = 1 AND p."TranscodeVideo" = 1
                 AND p."VideoFormat" = 4
                 AND (c."StreamingEngine" = 1 OR c."StreamingMode" IN (1, 5))"#,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(super::super::sqlx_error)?;
        if incompatible > 0 {
            return Ok(HealthCheckResult::new(
                self.title(), HealthCheckStatus::Fail,
                "AV1 output requires native HLS. Change the channel engine/mode or select H.264/HEVC in its FFmpeg profile.",
                "AV1 output requires native HLS",
            ).with_link("/channels"));
        }

        let mut missing: Vec<String> = self
            .runtime
            .issues
            .iter()
            .filter(|issue| issue.is_error())
            .map(runtime_issue_name)
            .collect();

        let probe_enabled = load_config(
            &self.pool,
            ersatzrs_domain::CONFIG_KEY_FFMPEG_PROBE_FOR_INTERLACED_FRAMES,
        )
        .await?
        .is_some_and(|value| value.eq_ignore_ascii_case("true") || value == "1");
        if probe_enabled && !self.runtime.capabilities.filters.contains("idet") {
            missing.push("filter idet".to_owned());
        }

        if missing.is_empty() {
            if self.runtime.capabilities.hardware_decode_denials.is_empty() {
                return Ok(HealthCheckResult::not_applicable(self.title()));
            }

            let denials = self
                .runtime
                .capabilities
                .hardware_decode_denials
                .iter()
                .map(|denial| {
                    format!(
                        "{:?} {} {}-bit",
                        denial.acceleration, denial.codec, denial.bit_depth
                    )
                })
                .collect::<Vec<_>>();
            return Ok(HealthCheckResult::new(
                self.title(),
                HealthCheckStatus::Warning,
                format!(
                    "FFmpeg hardware decode fallback is active for: [{}]",
                    denials.join(", ")
                ),
                "FFmpeg hardware decode fallback is active",
            ));
        }

        Ok(HealthCheckResult::new(
            self.title(),
            HealthCheckStatus::Fail,
            format!(
                "FFmpeg is missing required capabilities and will NOT work correctly: [{}]",
                missing.join(", ")
            ),
            "FFmpeg is missing required capabilities and will NOT work correctly",
        ))
    }
}
