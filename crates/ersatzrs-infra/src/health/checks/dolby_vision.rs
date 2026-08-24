use super::super::{
    async_trait, sqlx_error, Db, HealthCheck, HealthCheckResult, HealthCheckStatus, Pool, Result,
    Row,
};
use ersatzrs_ffmpeg::{DolbyVisionHandling, FfmpegRuntimeReport, HardwareGraphOperation};

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
    codec: Option<&str>,
    profile: Option<&str>,
    pixel_format: Option<&str>,
    video_scan_kind: Option<i32>,
) -> Option<String> {
    let is_hevc_main10 = codec.is_some_and(|value| value.eq_ignore_ascii_case("hevc"))
        && profile.is_some_and(|value| {
            value
                .to_ascii_lowercase()
                .replace([' ', '-'], "")
                .contains("main10")
        })
        && pixel_format.is_some_and(|value| value.eq_ignore_ascii_case("yuv420p10le"));
    if matches!(
        configuration.handling(),
        DolbyVisionHandling::Hdr10BaseLayerDirectDecode
    ) && !is_hevc_main10
    {
        return Some("the source is not HEVC Main10 yuv420p10le".to_owned());
    }
    if matches!(
        configuration.handling(),
        DolbyVisionHandling::LibplaceboRequired
    ) {
        if !is_hevc_main10 || video_scan_kind == Some(2) {
            return Some("profile 5 requires progressive HEVC Main10 yuv420p10le input".to_owned());
        }
        if !runtime.capabilities.has_decoder("hevc_cuvid")
            || !runtime
                .capabilities
                .nvidia_decoders
                .iter()
                .any(|decoder| decoder.supports("hevc", 10))
        {
            return Some("the active runtime has no proven CUDA HEVC Main10 decode".to_owned());
        }
        let device = runtime.capabilities.hardware_graph_device();
        for operation in [
            HardwareGraphOperation::NvencDolbyVisionProfile5Sdr,
            HardwareGraphOperation::NvencDolbyVisionProfile5Hdr10,
        ] {
            if runtime
                .capabilities
                .hardware_graph_denied(operation, &device)
            {
                return Some(format!(
                    "{} is denied on the selected device",
                    operation.id()
                ));
            }
            if !runtime
                .capabilities
                .hardware_graph_proven(operation, &device)
            {
                return Some(format!(
                    "{} has not been positively proven on the selected device",
                    operation.id()
                ));
            }
        }
        return None;
    }
    (!configuration.is_playable()).then(|| configuration.evidence())
}

fn persisted_stream_rejection(
    row: &crate::db::DbRow,
    runtime: &FfmpegRuntimeReport,
) -> Option<String> {
    let name = row
        .try_get::<String, _>("Name")
        .unwrap_or_else(|_| "Unnamed media version".to_owned());
    let metadata_state = row
        .try_get::<Option<i32>, _>("DolbyVisionMetadataState")
        .ok()
        .flatten()
        .map(ersatzrs_domain::DolbyVisionMetadataState::from_i32)
        .unwrap_or_default();
    if metadata_state == ersatzrs_domain::DolbyVisionMetadataState::Incomplete {
        let hint = row
            .try_get::<Option<String>, _>("DolbyVisionMetadataHint")
            .ok()
            .flatten()
            .unwrap_or_else(|| "required provider fields are missing".to_owned());
        return Some(format!("{name}: incomplete metadata ({hint})"));
    }
    let Some(profile) = row
        .try_get::<Option<i32>, _>("DolbyVisionProfile")
        .ok()
        .flatten()
        .and_then(|profile| u8::try_from(profile).ok())
    else {
        return Some(format!("{name}: invalid or missing Dolby Vision profile"));
    };
    let compatibility_id = row
        .try_get::<Option<i32>, _>("DolbyVisionBlCompatibilityId")
        .ok()
        .flatten()
        .and_then(|value| u8::try_from(value).ok());
    let rpu_present = row
        .try_get::<Option<bool>, _>("DolbyVisionRpuPresent")
        .ok()
        .flatten();
    let el_present = row
        .try_get::<Option<bool>, _>("DolbyVisionElPresent")
        .ok()
        .flatten();
    let bl_present = row
        .try_get::<Option<bool>, _>("DolbyVisionBlPresent")
        .ok()
        .flatten();
    let (Some(compatibility_id), Some(rpu_present), Some(el_present), Some(bl_present)) =
        (compatibility_id, rpu_present, el_present, bl_present)
    else {
        return Some(format!(
            "{name}: profile {profile} (persisted Dolby Vision metadata is incomplete)"
        ));
    };
    let configuration = ersatzrs_ffmpeg::DolbyVisionConfiguration {
        profile,
        bl_signal_compatibility_id: compatibility_id,
        rpu_present,
        el_present,
        bl_present,
    };

    let codec = row.try_get::<Option<String>, _>("Codec").ok().flatten();
    let source_profile = row.try_get::<Option<String>, _>("Profile").ok().flatten();
    let pixel_format = row
        .try_get::<Option<String>, _>("PixelFormat")
        .ok()
        .flatten();
    let video_scan_kind = row.try_get::<i32, _>("VideoScanKind").ok();
    rejection_reason(
        configuration,
        runtime,
        codec.as_deref(),
        source_profile.as_deref(),
        pixel_format.as_deref(),
        video_scan_kind,
    )
    .map(|reason| format!("{name}: profile {profile} ({reason})"))
}

#[async_trait]
impl HealthCheck for DolbyVisionHealthCheck {
    fn title(&self) -> &'static str {
        "Dolby Vision"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        let rows = sqlx::query(
            r#"SELECT ms."Codec", ms."Profile", ms."PixelFormat",
                      ms."DolbyVisionProfile", ms."DolbyVisionBlCompatibilityId",
                      ms."DolbyVisionBlPresent",
                      ms."DolbyVisionElPresent", ms."DolbyVisionRpuPresent",
                      ms."DolbyVisionMetadataState", ms."DolbyVisionMetadataHint",
                      mv."Name", mv."VideoScanKind"
               FROM "MediaStream" ms
               JOIN "MediaVersion" mv ON mv."Id" = ms."MediaVersionId"
               WHERE ms."DolbyVisionProfile" IS NOT NULL
                  OR ms."DolbyVisionMetadataState" <> 0
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
            if let Some(reason) = persisted_stream_rejection(row, &self.runtime) {
                rejected.push(reason);
            } else {
                playable += 1;
            }
        }

        if !rejected.is_empty() {
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
                "{playable} Dolby Vision stream(s) have a supported playback path; Dolby Vision signalling is removed from transcoded output"
            ),
            "Dolby Vision playback paths are available",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::rejection_reason;
    use super::{DolbyVisionHealthCheck, HealthCheck};
    use ersatzrs_ffmpeg::HardwareGraphOperation;

    fn profile_7() -> ersatzrs_ffmpeg::DolbyVisionConfiguration {
        ersatzrs_ffmpeg::DolbyVisionConfiguration {
            profile: 7,
            bl_signal_compatibility_id: 6,
            rpu_present: true,
            el_present: true,
            bl_present: true,
        }
    }

    fn exact_input_reason(
        configuration: ersatzrs_ffmpeg::DolbyVisionConfiguration,
        runtime: &ersatzrs_ffmpeg::FfmpegRuntimeReport,
    ) -> Option<String> {
        rejection_reason(
            configuration,
            runtime,
            Some("hevc"),
            Some("Main 10"),
            Some("yuv420p10le"),
            Some(1),
        )
    }

    #[test]
    fn complete_profile_7_uses_direct_base_layer_decode_without_a_bitstream_filter() {
        let mut runtime = ersatzrs_ffmpeg::FfmpegRuntimeReport::for_tests();
        assert_eq!(exact_input_reason(profile_7(), &runtime), None);

        runtime.capabilities.bitstream_filters.clear();
        assert_eq!(exact_input_reason(profile_7(), &runtime), None);
    }

    #[test]
    fn incomplete_profile_7_remains_rejected() {
        let mut configuration = profile_7();
        configuration.el_present = false;

        let reason = exact_input_reason(
            configuration,
            &ersatzrs_ffmpeg::FfmpegRuntimeReport::for_tests(),
        )
        .expect("incomplete profile 7");

        assert!(reason.contains("uncharacterized"), "{reason}");
    }

    fn profile_5() -> ersatzrs_ffmpeg::DolbyVisionConfiguration {
        ersatzrs_ffmpeg::DolbyVisionConfiguration {
            profile: 5,
            bl_signal_compatibility_id: 0,
            rpu_present: true,
            el_present: false,
            bl_present: true,
        }
    }

    #[test]
    fn profile_5_health_requires_both_output_operations() {
        let mut runtime = ersatzrs_ffmpeg::FfmpegRuntimeReport::for_tests();
        runtime.capabilities = ersatzrs_ffmpeg::capability_report_for_tests();
        let device = runtime.capabilities.hardware_graph_device();
        runtime
            .capabilities
            .prove_hardware_graph(HardwareGraphOperation::NvencDolbyVisionProfile5Sdr, &device);

        let reason = exact_input_reason(profile_5(), &runtime).expect("HDR10 proof is missing");
        assert!(reason.contains("profile5-hdr10"), "{reason}");

        runtime.capabilities.prove_hardware_graph(
            HardwareGraphOperation::NvencDolbyVisionProfile5Hdr10,
            &device,
        );
        assert_eq!(exact_input_reason(profile_5(), &runtime), None);
    }

    #[tokio::test]
    async fn health_counts_each_rejected_stream_and_names_its_media_version() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("open Dolby Vision health database");
        sqlx::query(
            r#"CREATE TABLE "MediaVersion" (
                "Id" INTEGER PRIMARY KEY,
                "Name" TEXT NOT NULL,
                "VideoScanKind" INTEGER NOT NULL
            )"#,
        )
        .execute(&pool)
        .await
        .expect("create media version table");
        sqlx::query(
            r#"CREATE TABLE "MediaStream" (
                "Id" INTEGER PRIMARY KEY,
                "MediaVersionId" INTEGER NOT NULL,
                "Codec" TEXT, "Profile" TEXT, "PixelFormat" TEXT,
                "DolbyVisionProfile" INTEGER,
                "DolbyVisionBlCompatibilityId" INTEGER,
                "DolbyVisionBlPresent" INTEGER,
                "DolbyVisionElPresent" INTEGER,
                "DolbyVisionRpuPresent" INTEGER,
                "DolbyVisionMetadataState" INTEGER NOT NULL DEFAULT 0,
                "DolbyVisionMetadataHint" TEXT
            )"#,
        )
        .execute(&pool)
        .await
        .expect("create media stream table");
        sqlx::query(
            r#"INSERT INTO "MediaVersion" ("Id", "Name", "VideoScanKind")
               VALUES (1, 'Feature Cut', 1)"#,
        )
        .execute(&pool)
        .await
        .expect("seed media version");
        for id in [1_i64, 2] {
            sqlx::query(
                r#"INSERT INTO "MediaStream" (
                    "Id", "MediaVersionId", "DolbyVisionProfile",
                    "Codec", "Profile", "PixelFormat",
                    "DolbyVisionBlCompatibilityId", "DolbyVisionBlPresent",
                    "DolbyVisionElPresent", "DolbyVisionRpuPresent",
                    "DolbyVisionMetadataState"
                ) VALUES (?, 1, 9, 'hevc', 'Main 10', 'yuv420p10le', 1, 1, 0, 1, 1)"#,
            )
            .bind(id)
            .execute(&pool)
            .await
            .expect("seed rejected stream");
        }

        let result =
            DolbyVisionHealthCheck::new(pool, ersatzrs_ffmpeg::FfmpegRuntimeReport::for_tests())
                .check()
                .await
                .expect("run Dolby Vision health check");

        assert!(result.message.starts_with("2 Dolby Vision streams"));
        assert_eq!(result.message.matches("Feature Cut: profile 9").count(), 2);
    }
}
