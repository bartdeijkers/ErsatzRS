//! Single permanent or intermittent watermark projection from the shared selector.

use ersatzrs_core::repositories::{RepositoryError, Result};
use ersatzrs_domain::{ChannelWatermarkMode, WatermarkLocation, WatermarkSize};

use crate::playback::ResolvedPlaybackWatermark;

use super::protocol;

/// Preserve selection precedence; unsupported active combinations fail closed.
/// Disabled selections do not contribute an overlay. The shared image resolver
/// supplies local cached files, so no provider credentials or source seeks are
/// introduced here. HTTP remains a wire capability, not a new image resolver.
pub(super) fn project(
    selected: &[ResolvedPlaybackWatermark],
) -> Result<Option<protocol::Watermark>> {
    let mut active = selected
        .iter()
        .filter(|selected| selected.watermark.mode != ChannelWatermarkMode::None);
    let Some(selected) = active.next() else {
        return Ok(None);
    };
    if active.next().is_some()
        || !matches!(
            selected.watermark.mode,
            ChannelWatermarkMode::Permanent | ChannelWatermarkMode::Intermittent
        )
    {
        return Err(unavailable());
    }
    let watermark = &selected.watermark;
    let path = selected
        .image_path
        .to_str()
        .filter(|path| !path.trim().is_empty())
        .ok_or_else(unavailable)?;
    let width_percent = match watermark.size {
        WatermarkSize::Scaled => Some(percent(watermark.width_percent)?),
        WatermarkSize::ActualSize => None,
    };
    Ok(Some(protocol::Watermark {
        timing: timing(watermark)?,
        source: protocol::Source::Local {
            path: path.to_owned(),
            in_point_ms: None,
            out_point_ms: None,
        },
        stream_index: Some(0),
        location: match watermark.location {
            WatermarkLocation::TopLeft => protocol::WatermarkLocation::TopLeft,
            WatermarkLocation::TopMiddle => protocol::WatermarkLocation::TopCenter,
            WatermarkLocation::TopRight => protocol::WatermarkLocation::TopRight,
            WatermarkLocation::LeftMiddle => protocol::WatermarkLocation::CenterLeft,
            WatermarkLocation::MiddleCenter => protocol::WatermarkLocation::Center,
            WatermarkLocation::RightMiddle => protocol::WatermarkLocation::CenterRight,
            WatermarkLocation::BottomLeft => protocol::WatermarkLocation::BottomLeft,
            WatermarkLocation::BottomMiddle => protocol::WatermarkLocation::BottomCenter,
            WatermarkLocation::BottomRight => protocol::WatermarkLocation::BottomRight,
        },
        width_percent,
        horizontal_margin_percent: Some(percent(watermark.horizontal_margin_percent)?),
        vertical_margin_percent: Some(percent(watermark.vertical_margin_percent)?),
        opacity_percent: Some(percent(f64::from(watermark.opacity))?),
    }))
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

fn percent(value: f64) -> Result<f64> {
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
