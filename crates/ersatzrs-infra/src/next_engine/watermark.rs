//! Ordered permanent and intermittent overlays from the shared watermark selector.

use ersatzrs_core::repositories::{RepositoryError, Result};
use ersatzrs_domain::{ChannelWatermarkMode, WatermarkLocation, WatermarkSize};

use crate::playback::{PlaybackWatermarkImage, ResolvedPlaybackWatermark};

use super::protocol;

/// Preserve selection precedence; unsupported active combinations fail closed.
/// Disabled selections do not contribute an overlay. The shared image resolver
/// preserves local cached files and operator-supplied channel-logo URLs.
pub(super) fn project(selected: &[ResolvedPlaybackWatermark]) -> Result<Vec<protocol::Watermark>> {
    let mut active: Vec<_> = selected
        .iter()
        .filter(|selected| selected.watermark.mode != ChannelWatermarkMode::None)
        .collect();
    // Match PlayoutItemConverter.OrderBy(ZIndex), retaining selection order on ties.
    active.sort_by_key(|selected| selected.watermark.z_index);
    active.into_iter().map(project_layer).collect()
}

pub(super) fn project_layer(selected: &ResolvedPlaybackWatermark) -> Result<protocol::Watermark> {
    if !matches!(
        selected.watermark.mode,
        ChannelWatermarkMode::Permanent | ChannelWatermarkMode::Intermittent
    ) {
        return Err(unavailable());
    }
    let watermark = &selected.watermark;
    let width_percent = match watermark.size {
        WatermarkSize::Scaled => Some(percent(watermark.width_percent)?),
        WatermarkSize::ActualSize => None,
    };
    Ok(protocol::Watermark {
        kind: None,
        timing: timing(watermark)?,
        source: image_source(&selected.image)?,
        stream_index: Some(0),
        within_source_content: watermark.place_within_source_content.then_some(true),
        location: location(watermark.location),
        width_percent,
        horizontal_margin_percent: Some(percent(watermark.horizontal_margin_percent)?),
        vertical_margin_percent: Some(percent(watermark.vertical_margin_percent)?),
        opacity_percent: Some(percent(f64::from(watermark.opacity))?),
    })
}

pub(super) fn location(value: WatermarkLocation) -> protocol::WatermarkLocation {
    match value {
        WatermarkLocation::TopLeft => protocol::WatermarkLocation::TopLeft,
        WatermarkLocation::TopMiddle => protocol::WatermarkLocation::TopCenter,
        WatermarkLocation::TopRight => protocol::WatermarkLocation::TopRight,
        WatermarkLocation::LeftMiddle => protocol::WatermarkLocation::CenterLeft,
        WatermarkLocation::MiddleCenter => protocol::WatermarkLocation::Center,
        WatermarkLocation::RightMiddle => protocol::WatermarkLocation::CenterRight,
        WatermarkLocation::BottomLeft => protocol::WatermarkLocation::BottomLeft,
        WatermarkLocation::BottomMiddle => protocol::WatermarkLocation::BottomCenter,
        WatermarkLocation::BottomRight => protocol::WatermarkLocation::BottomRight,
    }
}

pub(super) fn image_source(image: &PlaybackWatermarkImage) -> Result<protocol::Source> {
    match image {
        PlaybackWatermarkImage::Local(path) => Ok(protocol::Source::Local {
            path: path
                .to_str()
                .filter(|path| !path.trim().is_empty())
                .ok_or_else(unavailable)?
                .to_owned(),
            in_point_ms: None,
            out_point_ms: None,
            probe_hint: None,
        }),
        PlaybackWatermarkImage::Http(uri) => {
            let parsed = url::Url::parse(uri).map_err(|_| unavailable())?;
            if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
                return Err(unavailable());
            }
            Ok(protocol::Source::Http {
                uri: uri.clone(),
                headers: Vec::new(),
                user_agent: None,
                keep_alive: Some(false),
                reconnect: None,
                reconnect_delay_max: None,
                timeout_us: None,
                in_point_ms: None,
                out_point_ms: None,
                probe_hint: None,
                is_live: None,
            })
        }
    }
}

fn timing(
    watermark: &ersatzrs_domain::ChannelWatermark,
) -> Result<Option<protocol::WatermarkTiming>> {
    if watermark.mode != ChannelWatermarkMode::Intermittent {
        return Ok(None);
    }
    // Widen before multiplication: persisted i32 minutes/seconds can exceed
    // i32 milliseconds. Next's omitted fade duration defaults to one second.
    let frequency_ms =
        u64::try_from(watermark.frequency_minutes).map_err(|_| unavailable())? * 60_000;
    let hold_ms = u64::try_from(watermark.duration_seconds).map_err(|_| unavailable())? * 1_000;
    if hold_ms < 1_000 || hold_ms + 2_000 > frequency_ms {
        // Next emits no fade filters for invalid timing, making it permanent.
        return Err(unavailable());
    }
    Ok(Some(protocol::WatermarkTiming::Periodic {
        clock: protocol::PeriodicClock::Wall,
        frequency_ms,
        hold_ms,
    }))
}

pub(super) fn percent(value: f64) -> Result<f64> {
    if value.is_finite() && (0.0..=100.0).contains(&value) {
        Ok(value)
    } else {
        Err(unavailable())
    }
}

fn unavailable() -> RepositoryError {
    RepositoryError::unavailable(
        "Next watermark projection",
        "selection is unsupported or invalid",
    )
}

#[cfg(test)]
#[path = "watermark_tests.rs"]
mod tests;
