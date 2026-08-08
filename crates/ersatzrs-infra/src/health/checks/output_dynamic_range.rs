use super::super::{
    async_trait, sqlx_error, Db, HealthCheck, HealthCheckResult, HealthCheckStatus, Pool, Result,
    Row,
};
use ersatzrs_domain::ffmpeg::{ContentLightLevel, MasteringDisplayMetadata};

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
            // Every ORDER BY column must also appear in the DISTINCT
            // projection. MySQL 8 rejects otherwise (error 3065, "Expression
            // ... is not in SELECT list ... incompatible with DISTINCT"), and
            // ordering by a column that was projected away is meaningless in
            // any case. Ordering by the projected tuple is a total order over
            // the distinct rows, so the message stays deterministic.
            r#"SELECT DISTINCT p."Name", p."VideoFormat", p."BitDepth", p."TranscodeVideo",
                      p."HardwareAcceleration", p."HdrMasteringDisplay", p."HdrMaxCll",
                      p."HdrMaxFall"
               FROM "Channel" c
               INNER JOIN "FFmpegProfile" p ON p."Id" = c."FFmpegProfileId"
               WHERE p."OutputDynamicRange" = ?
               ORDER BY p."Name", p."VideoFormat", p."BitDepth", p."TranscodeVideo",
                        p."HardwareAcceleration", p."HdrMasteringDisplay", p."HdrMaxCll",
                        p."HdrMaxFall""#,
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
        let mut unusable_metadata = Vec::new();
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
                demoted.push(name.clone());
            }

            // ADR 0064 requires the signalled values to be documented. A stored
            // override that no longer parses is silently replaced by the
            // documented default at load time, which keeps channels on the air
            // but means the operator's edit is not what gets signalled.
            if !hdr_metadata_override_is_usable(row) {
                unusable_metadata.push(name);
            }
        }

        Ok(self.report(&invalid, &inert, &unusable_metadata, &demoted))
    }
}

impl OutputDynamicRangeHealthCheck {
    /// Report the most serious finding, in the order an operator should act on
    /// them: wrong output first, then inert selections, then metadata that is
    /// silently ignored, then a performance demotion.
    fn report(
        &self,
        invalid: &[String],
        inert: &[String],
        unusable_metadata: &[String],
        demoted: &[String],
    ) -> HealthCheckResult {
        if !invalid.is_empty() {
            return HealthCheckResult::new(
                self.title(),
                HealthCheckStatus::Fail,
                format!(
                    "{} FFmpeg Profile{} selected for HDR10 output cannot produce it: {}. Edit the profile to use hevc or av1 with 10-bit, or select SDR output.",
                    invalid.len(),
                    if invalid.len() > 1 { "s" } else { "" },
                    invalid.join(", ")
                ),
                "HDR10 output is configured but cannot be produced",
            );
        }

        if !inert.is_empty() {
            return HealthCheckResult::new(
                self.title(),
                HealthCheckStatus::Info,
                format!(
                    "{} FFmpeg Profile{} selected for HDR10 output but copy video; the output keeps the source dynamic range: {}",
                    inert.len(),
                    if inert.len() > 1 { "s are" } else { " is" },
                    inert.join(", ")
                ),
                "HDR10 selection does not apply to copied video",
            );
        }

        if !unusable_metadata.is_empty() {
            return HealthCheckResult::new(
                self.title(),
                HealthCheckStatus::Warning,
                format!(
                    "{} FFmpeg Profile{} an HDR10 mastering display or content light level that cannot be used, so the documented defaults are signalled instead: {}. Re-enter the values in the profile editor.",
                    unusable_metadata.len(),
                    if unusable_metadata.len() > 1 { "s store" } else { " stores" },
                    unusable_metadata.join(", ")
                ),
                "Stored HDR10 metadata is not usable",
            );
        }

        if !demoted.is_empty() {
            return HealthCheckResult::new(
                self.title(),
                HealthCheckStatus::Warning,
                format!(
                    "{} FFmpeg Profile{} HDR10 output on hardware without a proven HDR10 graph and will encode in software instead: {}. Only software and NVENC are characterized for HDR10.",
                    demoted.len(),
                    if demoted.len() > 1 { "s request" } else { " requests" },
                    demoted.join(", ")
                ),
                "HDR10 output falls back to software encoding",
            );
        }

        HealthCheckResult::pass(self.title())
    }
}

/// True when the profile's stored HDR10 metadata override is either absent
/// (use the documented default) or fully usable.
///
/// Mirrors the repository's decode fallback, so the check reports exactly the
/// rows whose stored values are being discarded.
fn hdr_metadata_override_is_usable(row: &crate::db::DbRow) -> bool {
    let mastering_display = row
        .try_get::<Option<String>, _>("HdrMasteringDisplay")
        .unwrap_or_default();
    if let Some(value) = mastering_display.as_deref().map(str::trim) {
        if !value.is_empty() && MasteringDisplayMetadata::parse(value).is_none() {
            return false;
        }
    }

    let max_cll = row
        .try_get::<Option<i32>, _>("HdrMaxCll")
        .unwrap_or_default();
    let max_fall = row
        .try_get::<Option<i32>, _>("HdrMaxFall")
        .unwrap_or_default();
    match (max_cll, max_fall) {
        (None, None) => true,
        (Some(max_cll), Some(max_fall)) => u32::try_from(max_cll)
            .ok()
            .zip(u32::try_from(max_fall).ok())
            .is_some_and(|(max_cll, max_fall)| ContentLightLevel::new(max_cll, max_fall).is_some()),
        // A half-configured pair is not a valid CTA-861.3 override.
        _ => false,
    }
}
