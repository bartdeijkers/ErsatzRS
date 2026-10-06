//! Canvas publication separates stable occurrence identity from process credentials.

use chrono::{DateTime, TimeDelta, Utc};
use ersatzrs_core::repositories::{RepositoryError, Result};
use ersatzrs_domain::{Channel, ChannelWatermarkMode, Resolution};
use serde::Serialize;

use crate::{
    db::DbPool,
    playback::{self, ResolvedPlaybackGraphicsElement, ResolvedPlaybackWatermark},
};

use super::{canvas::CanvasBinding, media::MediaRelay, protocol, schedule::ScheduledMedia};

/// Pure publication metadata participates in validation; credentials and the
/// current process UUID are installed only after that validation succeeds.
#[derive(Clone, Serialize)]
pub(crate) struct CanvasProjection {
    binding: CanvasBinding,
    hint: protocol::ProbeHint,
}

impl CanvasProjection {
    pub(super) fn publish(
        &self,
        relay: &MediaRelay,
        session: Option<uuid::Uuid>,
        start: DateTime<Utc>,
    ) -> Result<protocol::Watermark> {
        let mut binding = self.binding.clone();
        if start < binding.start || start >= binding.finish {
            return Err(unavailable());
        }
        binding.playout_start = (start != binding.start).then_some(start);
        binding.session = session;
        if binding.item == 0 && session.is_none() {
            return Err(unavailable());
        }
        Ok(protocol::Watermark {
            source: relay.canvas_source(&binding, self.hint.clone())?,
            kind: Some(protocol::GraphicsLayerKind::Canvas),
            location: protocol::WatermarkLocation::TopLeft,
            stream_index: Some(0),
            timing: None,
            within_source_content: None,
            width_percent: None,
            horizontal_margin_percent: None,
            vertical_margin_percent: None,
            opacity_percent: None,
        })
    }
}

/// Upstream row146 composites all selected watermarks and graphics in one
/// output-sized canvas. Mutable templates and assets are read at callback time.
pub(super) async fn project(
    pool: &DbPool,
    channel: &Channel,
    media: &ScheduledMedia,
    resolution: &Resolution,
    projected: &mut protocol::PlayoutItem,
    watermarks: &[ResolvedPlaybackWatermark],
    graphics: &[ResolvedPlaybackGraphicsElement],
) -> Result<Option<Box<CanvasProjection>>> {
    if graphics.is_empty()
        && !watermarks
            .iter()
            .any(|selected| selected.watermark.mode != ChannelWatermarkMode::None)
    {
        return Ok(None);
    }
    if graphics.is_empty()
        && watermarks.iter().all(|selected| {
            matches!(
                selected.watermark.mode,
                ChannelWatermarkMode::None
                    | ChannelWatermarkMode::Permanent
                    | ChannelWatermarkMode::Intermittent
            )
        })
    {
        projected.graphics = super::watermark::project(watermarks)?;
        for layer in &mut projected.graphics {
            layer.kind = Some(protocol::GraphicsLayerKind::Media);
        }
        return Ok(None);
    }
    let width = u32::try_from(resolution.width)
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(unavailable)?;
    let height = u32::try_from(resolution.height)
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(unavailable)?;
    let index = projected
        .tracks
        .as_ref()
        .and_then(|tracks| tracks.video.as_ref())
        .and_then(|video| video.stream_index)
        .map(i32::try_from)
        .transpose()
        .map_err(|_| unavailable())?;
    let video =
        playback::load_source_video_stream_by_index(pool, media.media_version_id, index).await?;
    let expires = media
        .item
        .finish
        .checked_add_signed(TimeDelta::hours(2))
        .ok_or_else(unavailable)?;
    Ok(Some(Box::new(CanvasProjection {
        binding: CanvasBinding {
            video_stream_index: index,
            ..CanvasBinding::new(channel.id, media, expires)
        },
        hint: protocol::ProbeHint {
            format_name: Some("nut".into()),
            duration_ms: None,
            audio: Vec::new(),
            subtitle: Vec::new(),
            video: vec![protocol::VideoProbeHint {
                codec: "ffv1".into(),
                width,
                height,
                pix_fmt: "bgra".into(),
                stream_index: 0,
                frame_rate: video
                    .and_then(|video| video.frame_rate)
                    .map(ersatzrs_ffmpeg::FrameRate::r_frame_rate),
                profile: None,
                field_order: None,
                sample_aspect_ratio: Some("1:1".into()),
                display_aspect_ratio: None,
                color_range: None,
                color_space: None,
                color_transfer: None,
                color_primaries: None,
                dv_profile: None,
                has_hdr10_metadata: None,
                rotation: None,
            }],
        },
    })))
}

fn unavailable() -> RepositoryError {
    RepositoryError::unavailable("Next graphics projection", "canvas occurrence is invalid")
}

#[cfg(test)]
#[path = "graphics_tests.rs"]
mod tests;
