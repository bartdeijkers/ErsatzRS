//! Bounded artifact-coordinate projections, without source or consumer admission.

use super::{plain_input_offset, NativeSourceConsumerOrigin, NativeSourceConsumerTiming};

const MAX_GROUPS: usize = 4096;

/// Stable group selection and millisecond projection for every native-origin bound.
///
/// Residuals retain the difference from the exact desired delay. A stable rounded
/// result is not proof of zero error at the original caption's packet time base,
/// source association, complete display context or actual generated playback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeSourceBitmapAlignment {
    instep_bounds: (i64, i64),
    active_group: Option<usize>,
    following_group: Option<usize>,
    following_delay: Option<i64>,
    delay_residual_bounds: Option<(i64, i64)>,
}

impl NativeSourceBitmapAlignment {
    /// Closed artifact-coordinate bounds, never one chosen exact instep.
    #[must_use]
    pub const fn artifact_instep_bounds_microseconds(self) -> (i64, i64) {
        self.instep_bounds
    }

    /// Last group at or before every possible instep; absent before the first.
    /// Complete bootstrap content remains the retained preparer's responsibility.
    #[must_use]
    pub const fn active_group_index(self) -> Option<usize> {
        self.active_group
    }

    /// First group strictly after every possible instep; absent after the last.
    #[must_use]
    pub const fn following_group_index(self) -> Option<usize> {
        self.following_group
    }

    /// Positive following delay rounded to Matroska's millisecond time base.
    #[must_use]
    pub const fn following_delay_microseconds(self) -> Option<i64> {
        self.following_delay
    }

    /// Closed bounds of rounded delay minus exact desired delay, in microseconds.
    /// A nonzero residual is exposed, not accepted as a playback tolerance.
    #[must_use]
    pub const fn following_delay_residual_bounds_microseconds(self) -> Option<(i64, i64)> {
        self.delay_residual_bounds
    }
}

/// Project actual selected AV timing into an exact retained caption artifact.
///
/// For every correlated n9.0 format-origin bound F, calculate native AV zero
/// `Kv=max(F+effective_seek,Eav)`, then artifact instep `Kv-C`. C must be the exact
/// singleton caption-only native zero retained with the artifact. Group indices
/// and the positive nearest-millisecond following delay must agree for every F;
/// signed half ties round away from zero, matching `av_rescale_q`.
///
/// Group coordinates must be the retained artifact's strictly ordered, bounded
/// millisecond observations. The caller owns their association, complete display
/// context, native version/input-policy provenance and the sealed command's
/// effective seek. AV contains only streams actually enabled on that source
/// owner, never an unselected audio track or another physical input's clock.
///
/// This pure projection opens nothing and grants no source, clock, precision or
/// consumer authority. Unknown metadata, arithmetic overflow, unstable selection
/// or rounding returns `None`; no arbitrary origin or time base is substituted.
#[must_use]
pub fn project_bitmap_consumer_origin(
    av: &NativeSourceConsumerOrigin,
    caption: &NativeSourceConsumerOrigin,
    caption_origin_microseconds: i64,
    observed_group_microseconds: &[i64],
    effective_seek_us: i64,
) -> Option<NativeSourceBitmapAlignment> {
    if observed_group_microseconds.is_empty()
        || observed_group_microseconds.len() > MAX_GROUPS
        || observed_group_microseconds
            .iter()
            .any(|point| *point < 0 || point % 1000 != 0)
        || observed_group_microseconds
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
    {
        return None;
    }
    let av_timing = checked_timings(av, caption, caption_origin_microseconds, effective_seek_us)?;
    let (lower, upper) = av_timing.format_start_bounds_microseconds();
    if upper.checked_sub(lower)? > 2 {
        return None;
    }
    let mut result: Option<NativeSourceBitmapAlignment> = None;
    for format in lower..=upper {
        // Preserve native intermediate arithmetic, including discarded offsets.
        let zero = plain_input_offset(
            format,
            av_timing.enabled_start_microseconds(),
            effective_seek_us,
            0,
        )?
        .checked_neg()?;
        if plain_input_offset(format, caption_origin_microseconds, 0, 0)?.checked_neg()?
            != caption_origin_microseconds
        {
            return None;
        }
        let instep = zero.checked_sub(caption_origin_microseconds)?;
        let after = observed_group_microseconds.partition_point(|point| *point <= instep);
        let active_group = after.checked_sub(1);
        let following = observed_group_microseconds.get(after);
        let (following_delay, residual) = if let Some(next) = following {
            let desired = next.checked_sub(instep)?;
            let rounded = nearest_millisecond(desired).filter(|delay| *delay > 0)?;
            (Some(rounded), Some(rounded.checked_sub(desired)?))
        } else {
            (None, None)
        };
        let point = NativeSourceBitmapAlignment {
            instep_bounds: (instep, instep),
            active_group,
            following_group: following.map(|_| after),
            following_delay,
            delay_residual_bounds: residual.map(|value| (value, value)),
        };
        if let Some(result) = &mut result {
            if !result.include(point) {
                return None;
            }
        } else {
            result = Some(point);
        }
    }
    result
}

fn checked_timings(
    av: &NativeSourceConsumerOrigin,
    caption: &NativeSourceConsumerOrigin,
    origin: i64,
    seek: i64,
) -> Option<NativeSourceConsumerTiming> {
    let [stream] = caption.streams() else {
        return None;
    };
    stream.start_pts()?;
    stream.time_base()?;
    if av.format_name() != Some("mpegts")
        || av.format_name() != caption.format_name()
        || av.format_start() != caption.format_start()
        || av.streams().is_empty()
        || av.streams().iter().any(|av| {
            !matches!(av.codec_type(), "video" | "audio")
                || av.index() == stream.index()
                || av.pid() == stream.pid()
        })
        || stream.codec_type() != "subtitle"
        || caption.timing(0)?.presentation_zero_bounds_microseconds() != (origin, origin)
    {
        return None;
    }
    av.timing(seek)
}

impl NativeSourceBitmapAlignment {
    fn include(&mut self, point: Self) -> bool {
        if self.active_group != point.active_group
            || self.following_group != point.following_group
            || self.following_delay != point.following_delay
        {
            return false;
        }
        self.instep_bounds.0 = self.instep_bounds.0.min(point.instep_bounds.0);
        self.instep_bounds.1 = self.instep_bounds.1.max(point.instep_bounds.1);
        if let (Some(bounds), Some(value)) =
            (&mut self.delay_residual_bounds, point.delay_residual_bounds)
        {
            bounds.0 = bounds.0.min(value.0);
            bounds.1 = bounds.1.max(value.1);
        }
        true
    }
}

fn nearest_millisecond(microseconds: i64) -> Option<i64> {
    let value = i128::from(microseconds);
    let magnitude = value.checked_abs()?.checked_add(500)?.checked_div(1000)?;
    let signed = if value < 0 { -magnitude } else { magnitude };
    i64::try_from(signed.checked_mul(1000)?).ok()
}

#[cfg(test)]
#[path = "teletext_consumer_bitmap_alignment_tests.rs"]
mod tests;
