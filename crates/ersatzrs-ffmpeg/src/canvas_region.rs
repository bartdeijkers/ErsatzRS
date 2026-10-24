//! A bounded output rectangle for a watermark-only Next canvas (upstream row237).

use serde::{Deserialize, Serialize};

use ersatzrs_domain::{WatermarkLocation, WatermarkSize};

use crate::{FrameSize, InvalidCanvasRenderRequest, WatermarkOverlay};

/// Full-output coordinates, independent of the cropped canvas dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CanvasRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl CanvasRegion {
    /// Cover measured watermarks in full-output coordinates, with even edges.
    /// Unknown or invalid geometry and a full-frame union retain the ordinary
    /// full-frame canvas. Callers must measure every active image first.
    #[must_use]
    pub fn for_watermarks(
        frame: FrameSize,
        content: FrameSize,
        images: &[(WatermarkOverlay, FrameSize)],
    ) -> Option<Self> {
        if frame.width == 0
            || frame.height == 0
            || content.width == 0
            || content.height == 0
            || images.is_empty()
        {
            return None;
        }
        let mut bounds = (
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        );
        for (overlay, image) in images {
            let (left, top, right, bottom) = watermark_bounds(frame, content, overlay, *image)?;
            bounds = (
                bounds.0.min(left),
                bounds.1.min(top),
                bounds.2.max(right),
                bounds.3.max(bottom),
            );
        }
        let left = even_bound(bounds.0, frame.width, false)?;
        let top = even_bound(bounds.1, frame.height, false)?;
        let right = even_bound(bounds.2, frame.width, true)?;
        let bottom = even_bound(bounds.3, frame.height, true)?;
        if right <= left
            || bottom <= top
            || (left == 0 && top == 0 && right == frame.width && bottom == frame.height)
        {
            return None;
        }
        Some(Self {
            x: left,
            y: top,
            width: right - left,
            height: bottom - top,
        })
    }

    /// Refuse empty or overflowing rectangles before spawning.
    ///
    /// # Errors
    /// Returns an error for empty dimensions or overflowing coordinates. Signed
    /// regions remain valid when a later profile change makes them extend
    /// beyond the current frame; Next clips them at their original placement.
    pub fn validate(self) -> Result<(), InvalidCanvasRenderRequest> {
        if self.width == 0
            || self.height == 0
            || self.x.checked_add(self.width).is_none()
            || self.y.checked_add(self.height).is_none()
        {
            return Err(InvalidCanvasRenderRequest);
        }
        Ok(())
    }
}

fn even_bound(value: f64, limit: u32, upper: bool) -> Option<u32> {
    let value = value.clamp(0.0, f64::from(limit));
    let value = if upper { value.ceil() } else { value.floor() };
    let value: u64 = format!("{value:.0}").parse().ok()?;
    let value = (value + u64::from(upper)) & !1;
    u32::try_from(value.min(u64::from(limit))).ok()
}

fn watermark_bounds(
    frame: FrameSize,
    content: FrameSize,
    overlay: &WatermarkOverlay,
    image: FrameSize,
) -> Option<(f64, f64, f64, f64)> {
    if image.width == 0
        || image.height == 0
        || ![
            overlay.width_percent,
            overlay.horizontal_margin_percent,
            overlay.vertical_margin_percent,
        ]
        .iter()
        .all(|x| x.is_finite() && (0.0..=100.0).contains(x))
    {
        return None;
    }
    let (w, h) = if overlay.size == WatermarkSize::Scaled {
        let w = (overlay.width_percent / 100.0 * f64::from(frame.width)).round_ties_even();
        // FFmpeg's -1 height may round either side of the exact aspect ratio.
        // Keep both possible positions when a bottom/centre anchor moves by a pixel.
        (w, w * f64::from(image.height) / f64::from(image.width))
    } else {
        (f64::from(image.width), f64::from(image.height))
    };
    if w < 1.0 || h < 1.0 {
        return None;
    }
    let (mw, mh, px, py) = if overlay.place_within_source_content {
        (
            content.width,
            content.height,
            frame.width.saturating_sub(content.width),
            frame.height.saturating_sub(content.height),
        )
    } else {
        (frame.width, frame.height, 0, 0)
    };
    let mx = (overlay.horizontal_margin_percent / 100.0 * f64::from(mw) + f64::from(px) / 2.0)
        .round_ties_even();
    let my = (overlay.vertical_margin_percent / 100.0 * f64::from(mh) + f64::from(py) / 2.0)
        .round_ties_even();
    let x = match overlay.location {
        WatermarkLocation::TopLeft
        | WatermarkLocation::LeftMiddle
        | WatermarkLocation::BottomLeft => mx,
        WatermarkLocation::TopMiddle
        | WatermarkLocation::MiddleCenter
        | WatermarkLocation::BottomMiddle => (f64::from(frame.width) - w) / 2.0,
        _ => f64::from(frame.width) - w - mx,
    };
    let y = match overlay.location {
        WatermarkLocation::TopLeft | WatermarkLocation::TopMiddle | WatermarkLocation::TopRight => {
            my
        }
        WatermarkLocation::LeftMiddle
        | WatermarkLocation::MiddleCenter
        | WatermarkLocation::RightMiddle => (f64::from(frame.height) - h) / 2.0,
        _ => f64::from(frame.height) - h - my,
    };
    // Conservative integer bounds include the scaler's rounded height and the
    // compositor's integer anchor. At most one transparent edge pixel is added.
    Some((x.floor(), y.floor(), (x + w).ceil(), (y + h).ceil()))
}
