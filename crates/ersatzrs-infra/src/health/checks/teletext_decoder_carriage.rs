//! Optional requirements follow explicit output-channel choices, never video Copy.

use ersatzrs_domain::SourceTeletextCaptionDelivery;
use ersatzrs_ffmpeg::{FfmpegCapabilityReport, TeletextDecoderCapability, TeletextDecoderStatus};
use sqlx::Row as _;

use super::super::super::{sqlx_error, HealthCheckStatus, Result};
use crate::{db::DbPool, source_teletext_delivery::load_channel_source_teletext_delivery};

const READY: &str = "DVB subtitle delivery requirements are available. Per-source caption preparation and client display still require validation.";
const MISSING: &str = "DVB subtitle delivery is configured but its required FFmpeg support is unavailable. Select a compatible runtime; ordinary playback remains available.";
const INVALID: &str = "A channel has invalid source teletext delivery settings. Review its subtitle delivery choice; ordinary playback remains available.";
const OUTPUT: &str = "A channel uses DVB subtitles with an unsupported engine or output. Select native MPEG-TS or change subtitle delivery; ordinary playback remains available.";

pub(super) struct CarriageContext {
    pool: DbPool,
    requirements_available: bool,
    hybrid_available: bool,
}

impl CarriageContext {
    pub(super) fn new(pool: DbPool, capabilities: &FfmpegCapabilityReport) -> Self {
        Self {
            pool,
            requirements_available: capabilities.has_encoder("dvbsub")
                && capabilities.has_decoder("dvbsub")
                && capabilities.bitstream_filters.contains("setts")
                && capabilities.muxers.contains("matroska")
                && capabilities.muxers.contains("mpegts")
                && capabilities.demuxers.contains("matroska")
                && capabilities.demuxers.contains("concat"),
            hybrid_available: capabilities.muxers.contains("segment"),
        }
    }

    pub(super) async fn check(
        &self,
        decoder: &TeletextDecoderCapability,
    ) -> Result<Option<(HealthCheckStatus, &'static str)>> {
        // The existing loader owns UUID resolution and ConfigElement decoding.
        // Defaults, disabled channels and orphaned keys add no runtime demands.
        let channels = sqlx::query(
            r#"SELECT c."Id", c."StreamingEngine", c."StreamingMode"
               FROM "Channel" c JOIN "ConfigElement" cfg
                 ON cfg."Key" = 'channel.' || c."UniqueId" || '.source_caption_delivery'
               WHERE c."IsEnabled" = 1 ORDER BY c."Id""#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_error)?;
        let mut selected = false;
        let mut hybrid_selected = false;
        for channel in channels {
            let id = channel.try_get("Id").map_err(sqlx_error)?;
            let Ok(delivery) = load_channel_source_teletext_delivery(&self.pool, id).await else {
                return Ok(Some((HealthCheckStatus::Warning, INVALID)));
            };
            if delivery == SourceTeletextCaptionDelivery::Burn {
                continue;
            }
            let engine: i32 = channel.try_get("StreamingEngine").map_err(sqlx_error)?;
            let mode: i32 = channel.try_get("StreamingMode").map_err(sqlx_error)?;
            // Persisted channel values: native=0, MPEG-TS=1, Hybrid=5.
            if engine != 0 || !matches!(mode, 1 | 5) {
                return Ok(Some((HealthCheckStatus::Warning, OUTPUT)));
            }
            selected = true;
            hybrid_selected |= mode == 5;
        }
        Ok(selected.then_some(
            if self.requirements_available
                && (!hybrid_selected || self.hybrid_available)
                && decoder.status == TeletextDecoderStatus::Ready
            {
                (HealthCheckStatus::Pass, READY)
            } else {
                (HealthCheckStatus::Warning, MISSING)
            },
        ))
    }
}

#[cfg(test)]
#[path = "teletext_decoder_carriage_tests.rs"]
mod tests;
