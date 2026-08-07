use super::super::{
    async_trait, sqlx_error, Db, HealthCheck, HealthCheckResult, HealthCheckStatus, Pool, Result,
    Row,
};

/// `OutputDynamicRange` = `Hdr10Bt2020Pq`.
const OUTPUT_DYNAMIC_RANGE_HDR10: i32 = 1;
/// `FFmpegProfileBitDepth` = `TenBit`.
const BIT_DEPTH_TEN: i32 = 1;
/// `FFmpegProfileVideoFormat` values that can carry HDR10 static metadata.
const HDR10_CAPABLE_VIDEO_FORMATS: [i32; 2] = [2 /* Hevc */, 4 /* Av1 */];
/// `FFmpegProfileVideoFormat` = `Copy`.
const VIDEO_FORMAT_COPY: i32 = 99;
/// `HardwareAccelerationKind` values with a proven HDR10 output graph:
/// 0 = None (software) and 2 = Nvenc. Keep in step with
/// `ersatzrs_ffmpeg::hdr10_output_support`.
const HDR10_PROVEN_ACCELERATIONS: [i32; 2] = [0, 2];

/// Report `FFmpeg` profiles whose HDR10 output contract cannot be honoured
/// (ADR 0064).
///
/// Application validation rejects these combinations at save time, so a row
/// that reaches this check came from a database edited outside ErsatzRS, from a
/// restored backup, or from a profile whose codec was changed by a migration.
/// ADR 0064 forbids silently downgrading HDR10 to SDR, so the operator has to
/// be told rather than served wrongly-coloured video.
pub(crate) struct OutputDynamicRangeHealthCheck {
    pool: Pool<Db>,
}

impl OutputDynamicRangeHealthCheck {
    pub(crate) fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HealthCheck for OutputDynamicRangeHealthCheck {
    fn title(&self) -> &'static str {
        "Output Dynamic Range"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        let rows = sqlx::query(
            r#"SELECT DISTINCT p."Name", p."VideoFormat", p."BitDepth", p."TranscodeVideo",
                      p."HardwareAcceleration"
               FROM "Channel" c
               INNER JOIN "FFmpegProfile" p ON p."Id" = c."FFmpegProfileId"
               WHERE p."OutputDynamicRange" = ?
               ORDER BY p."Name", p."Id""#,
        )
        .bind(OUTPUT_DYNAMIC_RANGE_HDR10)
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_error)?;

        if rows.is_empty() {
            return Ok(HealthCheckResult::not_applicable(self.title()));
        }

        let mut invalid = Vec::new();
        let mut inert = Vec::new();
        let mut demoted = Vec::new();
        for row in &rows {
            let name = row.try_get::<String, _>("Name").unwrap_or_default();
            let video_format = row.try_get::<i32, _>("VideoFormat").unwrap_or_default();
            let bit_depth = row.try_get::<i32, _>("BitDepth").unwrap_or_default();
            let transcode_video = row.try_get::<bool, _>("TranscodeVideo").unwrap_or(true);

            // Video copy preserves the source elementary stream, so an HDR10
            // selection is inert rather than wrong. Worth saying, not failing.
            if !transcode_video || video_format == VIDEO_FORMAT_COPY {
                inert.push(name);
                continue;
            }

            if !HDR10_CAPABLE_VIDEO_FORMATS.contains(&video_format) {
                invalid.push(format!("{name} (codec cannot carry HDR10 static metadata)"));
                continue;
            }
            if bit_depth != BIT_DEPTH_TEN {
                invalid.push(format!("{name} (HDR10 requires 10-bit)"));
                continue;
            }

            // ADR 0064: a backend without a proven HDR10 graph is demoted to
            // software encoding rather than allowed to emit SDR. Correct, but
            // markedly slower, so the operator has to be told.
            let acceleration = row
                .try_get::<i32, _>("HardwareAcceleration")
                .unwrap_or_default();
            if !HDR10_PROVEN_ACCELERATIONS.contains(&acceleration) {
                demoted.push(name);
            }
        }

        if !invalid.is_empty() {
            return Ok(HealthCheckResult::new(
                self.title(),
                HealthCheckStatus::Fail,
                format!(
                    "{} FFmpeg Profile{} selected for HDR10 output cannot produce it: {}. Edit the profile to use hevc or av1 with 10-bit, or select SDR output.",
                    invalid.len(),
                    if invalid.len() > 1 { "s" } else { "" },
                    invalid.join(", ")
                ),
                "HDR10 output is configured but cannot be produced",
            ));
        }

        if !inert.is_empty() {
            return Ok(HealthCheckResult::new(
                self.title(),
                HealthCheckStatus::Info,
                format!(
                    "{} FFmpeg Profile{} selected for HDR10 output but copy video; the output keeps the source dynamic range: {}",
                    inert.len(),
                    if inert.len() > 1 { "s are" } else { " is" },
                    inert.join(", ")
                ),
                "HDR10 selection does not apply to copied video",
            ));
        }

        if !demoted.is_empty() {
            return Ok(HealthCheckResult::new(
                self.title(),
                HealthCheckStatus::Warning,
                format!(
                    "{} FFmpeg Profile{} HDR10 output on hardware without a proven HDR10 graph and will encode in software instead: {}. Only software and NVENC are characterized for HDR10.",
                    demoted.len(),
                    if demoted.len() > 1 { "s request" } else { " requests" },
                    demoted.join(", ")
                ),
                "HDR10 output falls back to software encoding",
            ));
        }

        Ok(HealthCheckResult::pass(self.title()))
    }
}
