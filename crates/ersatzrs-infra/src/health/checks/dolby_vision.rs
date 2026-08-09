use super::super::{
    async_trait, sqlx_error, Db, HealthCheck, HealthCheckResult, HealthCheckStatus, Pool, Result,
    Row,
};
use ersatzrs_ffmpeg::{DolbyVisionHandling, FfmpegRuntimeReport};

/// Report Dolby Vision items ErsatzRS will not play (ADR 0064, milestone
/// `p8a-02`).
///
/// ADR 0064 requires unproven Dolby Vision paths to be rejected visibly rather
/// than played with plausible but incorrect colour. A rejection the operator
/// cannot see is indistinguishable from a bug, so every affected item is named
/// here with the reason.
pub(crate) struct DolbyVisionHealthCheck {
    pool: Pool<Db>,
    runtime: FfmpegRuntimeReport,
}

impl DolbyVisionHealthCheck {
    pub(crate) fn new(pool: Pool<Db>, runtime: FfmpegRuntimeReport) -> Self {
        Self { pool, runtime }
    }
}

fn rejection_reason(
    configuration: ersatzrs_ffmpeg::DolbyVisionConfiguration,
    runtime: &FfmpegRuntimeReport,
) -> Option<String> {
    if matches!(
        configuration.handling(),
        DolbyVisionHandling::Hdr10BaseLayerAfterSeparation
    ) && !runtime.capabilities.has_bitstream_filter("dovi_split")
    {
        return Some("the active FFmpeg runtime has no dovi_split bitstream filter".to_owned());
    }
    (!configuration.is_playable()).then(|| configuration.evidence())
}

#[async_trait]
impl HealthCheck for DolbyVisionHealthCheck {
    fn title(&self) -> &'static str {
        "Dolby Vision"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        let rows = sqlx::query(
            r#"SELECT ms."DolbyVisionProfile", ms."DolbyVisionBlCompatibilityId",
                      ms."DolbyVisionBlPresent",
                      ms."DolbyVisionElPresent", ms."DolbyVisionRpuPresent",
                      mv."Name"
               FROM "MediaStream" ms
               JOIN "MediaVersion" mv ON mv."Id" = ms."MediaVersionId"
               WHERE ms."DolbyVisionProfile" IS NOT NULL
               ORDER BY ms."DolbyVisionProfile", ms."Id""#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_error)?;

        if rows.is_empty() {
            return Ok(HealthCheckResult::not_applicable(self.title()));
        }

        let mut rejected = Vec::new();
        let mut playable = 0_usize;
        for row in &rows {
            let Some(profile) = row
                .try_get::<Option<i32>, _>("DolbyVisionProfile")
                .ok()
                .flatten()
                .and_then(|profile| u8::try_from(profile).ok())
            else {
                continue;
            };
            let configuration = ersatzrs_ffmpeg::DolbyVisionConfiguration {
                profile,
                bl_signal_compatibility_id: row
                    .try_get::<Option<i32>, _>("DolbyVisionBlCompatibilityId")
                    .ok()
                    .flatten()
                    .and_then(|value| u8::try_from(value).ok())
                    .unwrap_or_default(),
                rpu_present: row
                    .try_get::<Option<bool>, _>("DolbyVisionRpuPresent")
                    .ok()
                    .flatten()
                    .unwrap_or_default(),
                el_present: row
                    .try_get::<Option<bool>, _>("DolbyVisionElPresent")
                    .ok()
                    .flatten()
                    .unwrap_or_default(),
                bl_present: row
                    .try_get::<Option<bool>, _>("DolbyVisionBlPresent")
                    .ok()
                    .flatten()
                    .unwrap_or_default(),
            };

            if let Some(reason) = rejection_reason(configuration, &self.runtime) {
                rejected.push(format!("profile {profile} ({reason})"));
            } else {
                playable += 1;
            }
        }

        if !rejected.is_empty() {
            rejected.sort();
            rejected.dedup();
            return Ok(HealthCheckResult::new(
                self.title(),
                HealthCheckStatus::Warning,
                format!(
                    "{} Dolby Vision stream{} cannot be played correctly and will be rejected rather than played with wrong colour: {}",
                    rejected.len(),
                    if rejected.len() > 1 { "s" } else { "" },
                    rejected.join("; ")
                ),
                "Some Dolby Vision sources are not supported",
            ));
        }

        Ok(HealthCheckResult::new(
            self.title(),
            HealthCheckStatus::Info,
            format!(
                "{playable} Dolby Vision stream(s) play from a conventional base layer; Dolby Vision signalling is removed from the output"
            ),
            "Dolby Vision plays from its base layer",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::rejection_reason;

    fn profile_7() -> ersatzrs_ffmpeg::DolbyVisionConfiguration {
        ersatzrs_ffmpeg::DolbyVisionConfiguration {
            profile: 7,
            bl_signal_compatibility_id: 6,
            rpu_present: true,
            el_present: true,
            bl_present: true,
        }
    }

    #[test]
    fn complete_profile_7_requires_the_runtime_bitstream_filter() {
        let mut runtime = ersatzrs_ffmpeg::FfmpegRuntimeReport::for_tests();
        assert_eq!(rejection_reason(profile_7(), &runtime), None);

        runtime.capabilities.bitstream_filters.clear();
        let reason = rejection_reason(profile_7(), &runtime).expect("missing capability");
        assert!(reason.contains("no dovi_split"), "{reason}");
    }

    #[test]
    fn incomplete_profile_7_remains_rejected_with_the_filter_available() {
        let mut configuration = profile_7();
        configuration.el_present = false;

        let reason = rejection_reason(
            configuration,
            &ersatzrs_ffmpeg::FfmpegRuntimeReport::for_tests(),
        )
        .expect("incomplete profile 7");

        assert!(reason.contains("incomplete"), "{reason}");
    }
}
