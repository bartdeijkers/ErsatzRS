//! Row26's single permanent watermark projection from the shared selector.

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
    if active.next().is_some() || selected.watermark.mode != ChannelWatermarkMode::Permanent {
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
