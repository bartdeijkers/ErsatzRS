//! Checked alignment under the audited plain native n9.0 MPEG-TS input policy.

use super::{
    teletext_consumer_origin::{NativeSourceConsumerOrigin, NativeSourceConsumerTiming},
    teletext_origin::NativeSourceTimeBase,
};

#[path = "teletext_consumer_bitmap_alignment.rs"]
mod bitmap;
pub use bitmap::{project_bitmap_consumer_origin, NativeSourceBitmapAlignment};

/// A constant caption-input offset valid for every correlated format-origin bound.
///
/// The caller owns same-source, native-version, input-flags and enabled-stream
/// provenance. This calculation neither admits a source nor qualifies a seek
/// landing, subtitle decoder state, discontinuity or eventual output timestamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeSourceConsumerAlignment {
    offset_us: i64,
    caption_index: i32,
    caption_base: NativeSourceTimeBase,
    caption_timing: NativeSourceConsumerTiming,
    av_timing: NativeSourceConsumerTiming,
}

impl NativeSourceConsumerAlignment {
    /// Native `-itsoffset` for the independently opened, unseeked caption input.
    #[must_use]
    pub const fn caption_input_offset_microseconds(self) -> i64 {
        self.offset_us
    }

    #[must_use]
    pub const fn caption_stream_index(self) -> i32 {
        self.caption_index
    }

    #[must_use]
    pub const fn caption_time_base(self) -> NativeSourceTimeBase {
        self.caption_base
    }

    #[must_use]
    pub const fn caption_timing(self) -> NativeSourceConsumerTiming {
        self.caption_timing
    }

    #[must_use]
    pub const fn av_timing(self) -> NativeSourceConsumerTiming {
        self.av_timing
    }
}

/// Align a caption-only input with the actual selected AV owner's native clock.
///
/// Both projections must describe the same reported MPEG-TS format origin. For
/// every possible integer origin F in its checked enclosure, native correction
/// gives Kc=max(F,Ecaption) and Kv=max(F+seek,Eav). Select an integer offset D
/// only when rescaling D-Kc and -Kv to the actual caption packet time base gives
/// exactly equal ticks for every F. The uncertainty is correlated, not replaced
/// by a chosen exact origin or an assumed 90 kHz time base.
///
/// Unknown metadata, arithmetic overflow or incompatible rounding returns `None`.
/// The policy excludes copyts, timestamp scaling/synchronization, looping and
/// preexisting input offsets; callers must establish those policy facts.
#[must_use]
pub fn align_independent_caption_origin(
    av: &NativeSourceConsumerOrigin,
    caption: &NativeSourceConsumerOrigin,
    effective_seek_us: i64,
) -> Option<NativeSourceConsumerAlignment> {
    if av.format_name() != caption.format_name()
        || av.format_start() != caption.format_start()
        || av
            .streams()
            .iter()
            .any(|stream| !matches!(stream.codec_type(), "video" | "audio"))
    {
        return None;
    }
    let [caption_stream] = caption.streams() else {
        return None;
    };
    if caption_stream.codec_type() != "subtitle" {
        return None;
    }
    let caption_base = caption_stream.time_base()?;
    let av_timing = av.timing(effective_seek_us)?;
    let caption_timing = caption.timing(0)?;
    let (lower, upper) = av_timing.format_start_bounds_microseconds();

    // The audited format projection yields at most three integer origins.
    // Each acceptable D is in the intersection of monotone rounding intervals.
    // If that intersection exists, it contains an integer between the smallest
    // and largest exact Kc-Kv, since each interval contains its own exact value.
    let zeros = (lower..=upper)
        .map(|format| {
            Some((
                format,
                format.max(caption_timing.enabled_start_microseconds()),
                format
                    .checked_add(effective_seek_us)?
                    .max(av_timing.enabled_start_microseconds()),
            ))
        })
        .collect::<Option<Vec<_>>>()?;
    let differences = zeros
        .iter()
        .map(|(_, caption_zero, av_zero)| caption_zero.checked_sub(*av_zero))
        .collect::<Option<Vec<_>>>()?;
    let first = *differences.iter().min()?;
    let last = *differences.iter().max()?;
    let offset_us = (first..=last).find(|offset| {
        zeros.iter().all(|(format, _, _)| {
            let Some(caption_offset) = plain_input_offset(
                *format,
                caption_timing.enabled_start_microseconds(),
                0,
                *offset,
            ) else {
                return false;
            };
            let Some(av_offset) = plain_input_offset(
                *format,
                av_timing.enabled_start_microseconds(),
                effective_seek_us,
                0,
            ) else {
                return false;
            };
            let Some(caption_ticks) = offset_ticks(caption_offset, caption_base) else {
                return false;
            };
            offset_ticks(av_offset, caption_base) == Some(caption_ticks)
        })
    })?;
    Some(NativeSourceConsumerAlignment {
        offset_us,
        caption_index: caption_stream.index(),
        caption_base,
        caption_timing,
        av_timing,
    })
}

// Preserve native arithmetic order, not only the algebraically simplified
// final D-max(F+S,E). ffmpeg_demux.c first constructs F+S and D-(F+S).
// ffmpeg_opt.c then always computes E-F; only a nonzero difference replaces
// the offset with -max(F+S,E), followed by addition of the input offset D.
// An overflowing discarded intermediate must not qualify an otherwise safe
// mathematical final value.
fn plain_input_offset(format: i64, enabled: i64, seek: i64, offset: i64) -> Option<i64> {
    let timestamp = format.checked_add(seek)?;
    let initial_offset = offset.checked_sub(timestamp)?;
    let difference = enabled.checked_sub(format)?;
    if difference == 0 {
        Some(initial_offset)
    } else {
        // Native computes this addition again inside the correction branch.
        let absolute_seek = format.checked_add(seek)?;
        absolute_seek
            .max(enabled)
            .checked_neg()?
            .checked_add(offset)
    }
}

// ffmpeg_demux.c applies av_rescale_q(ifile->ts_offset, AV_TIME_BASE_Q,
// pkt->time_base) independently to each known PTS/DTS. av_rescale_q uses nearest
// rounding with signed half ties away from zero. No floating point is involved.
fn offset_ticks(offset_us: i64, base: NativeSourceTimeBase) -> Option<i64> {
    let numerator = i128::from(offset_us).checked_mul(i128::from(base.denominator()))?;
    let denominator = 1_000_000_i128.checked_mul(i128::from(base.numerator()))?;
    let magnitude = numerator
        .checked_abs()?
        .checked_add(denominator / 2)?
        .checked_div(denominator)?;
    let signed = if numerator < 0 {
        magnitude.checked_neg()?
    } else {
        magnitude
    };
    i64::try_from(signed)
        .ok()
        .filter(|value| *value != i64::MIN)
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::*;
    use crate::ffprobe::teletext_consumer_origin::parse_consumer_origin;

    fn fixture(format: &str, video_start: i64, caption_start: i64, base: &str) -> Value {
        json!({
            "format":{"format_name":"mpegts","start_time":format},
            "streams":[
                {"index":0,"id":"0x100","codec_type":"video","start_pts":video_start,"time_base":base},
                {"index":1,"id":"0x101","codec_type":"subtitle","start_pts":caption_start,"time_base":base}
            ]
        })
    }

    fn parse(metadata: &Value, enabled: &[i32]) -> NativeSourceConsumerOrigin {
        parse_consumer_origin(&serde_json::to_vec(metadata).unwrap(), enabled).unwrap()
    }

    fn alignment(metadata: &Value, seek: i64) -> Option<NativeSourceConsumerAlignment> {
        align_independent_caption_origin(&parse(metadata, &[0]), &parse(metadata, &[1]), seek)
    }

    #[test]
    fn staggered_source_preserves_exact_caption_packet_offset_despite_format_uncertainty() {
        // Eav=1.4s represents the actual enabled AV minimum, not the late video.
        let metadata = fixture("0.860944", 126_000, 77_485, "1/90000");
        let result = alignment(&metadata, 500_000).unwrap();
        assert_eq!(result.caption_input_offset_microseconds(), -539_056);
        assert_eq!(result.caption_stream_index(), 1);
        assert_eq!(result.caption_time_base().denominator(), 90_000);
        assert_eq!(
            result
                .caption_timing()
                .presentation_zero_bounds_microseconds(),
            (860_944, 860_945)
        );
        assert_eq!(
            result.av_timing().presentation_zero_bounds_microseconds(),
            (1_400_000, 1_400_000)
        );
    }

    #[test]
    fn correlated_format_origin_cancels_exactly_at_nonstandard_time_base() {
        let metadata = fixture("1.000000", 0, 0, "7/48000");
        let result = alignment(&metadata, 123_457).unwrap();
        assert_eq!(result.caption_input_offset_microseconds(), -123_457);
        assert_eq!(result.caption_time_base().numerator(), 7);
        for format in 999_999..=1_000_001 {
            assert_eq!(
                offset_ticks(-123_457 - format, result.caption_time_base()),
                offset_ticks(-(format + 123_457), result.caption_time_base())
            );
        }
    }

    #[test]
    fn unresolved_origin_at_one_microsecond_packet_resolution_is_rejected() {
        let metadata = fixture("1.000000", 2_000_000, 0, "1/1000000");
        assert!(alignment(&metadata, 0).is_none());
    }

    #[test]
    fn neighboring_integer_offset_is_used_when_lower_bound_crosses_a_packet_tick() {
        let mut metadata = fixture("0.000010", 15, 0, "1/90000");
        metadata["streams"][0]["time_base"] = json!("1/1000000");
        let result = alignment(&metadata, 0).unwrap();
        // The lower-bound difference -6 produces -2 ticks at F=11, whereas
        // the required -15us AV offset rounds to -1 tick. D=-5 works for all F.
        assert_eq!(result.caption_input_offset_microseconds(), -5);
    }

    #[test]
    fn signed_native_rounding_uses_actual_base_and_ties_away_from_zero() {
        let base = NativeSourceTimeBase::parse("1/200000").unwrap();
        for (microseconds, expected) in [(2, 0), (3, 1), (-2, 0), (-3, -1)] {
            assert_eq!(offset_ticks(microseconds, base), Some(expected));
        }
        let base = NativeSourceTimeBase::parse("1/500000").unwrap();
        assert_eq!(offset_ticks(1, base), Some(1));
        assert_eq!(offset_ticks(-1, base), Some(-1));
    }

    #[test]
    fn missing_starts_overflow_and_different_native_origins_remain_unknown() {
        let metadata = fixture("1.000000", 90_000, 45_000, "1/90000");
        let av = parse(&metadata, &[0]);
        let mut different = metadata.clone();
        different["format"]["start_time"] = json!("1.000001");
        assert!(align_independent_caption_origin(&av, &parse(&different, &[1]), 0).is_none());
        for (field, value) in [
            ("start_pts", Value::Null),
            ("time_base", Value::Null),
            ("time_base", json!("2147483647/1")),
        ] {
            let mut unknown = metadata.clone();
            unknown["streams"][1][field] = value;
            if field == "time_base" && unknown["streams"][1][field].is_string() {
                unknown["streams"][1]["start_pts"] = json!(i64::MAX);
            }
            assert!(alignment(&unknown, 0).is_none());
        }
        assert!(alignment(&metadata, i64::MAX).is_none());
        assert!(alignment(&metadata, -1).is_none());
        let base = NativeSourceTimeBase::parse("1/2147483647").unwrap();
        assert!(offset_ticks(i64::MAX, base).is_none());
    }

    #[test]
    fn overflowing_native_intermediates_cannot_be_hidden_by_safe_final_offsets() {
        let metadata = fixture("-1.000000", 1, i64::MAX, "1/1000000");
        // The simplified offset D=MAX-1 gives D-Kc=-1, exactly matching
        // -Kv=-1. Native first evaluates D-F and Ecaption-F, which overflow.
        assert!(alignment(&metadata, 0).is_none());
        assert_eq!((i64::MAX - 1).checked_sub(i64::MAX), Some(-1));

        // Check the individual native operations, including branches whose
        // mathematically simplified result would otherwise be representable.
        assert!(plain_input_offset(-1, 0, 0, i64::MAX).is_none());
        assert!(plain_input_offset(-1, i64::MAX, 0, 0).is_none());
        assert!(plain_input_offset(i64::MAX, i64::MAX, 1, 0).is_none());
        assert!(plain_input_offset(0, 10, 0, i64::MIN + 5).is_none());
        assert_eq!(
            plain_input_offset(-1, 0, 0, i64::MAX - 1),
            Some(i64::MAX - 1)
        );
        assert_eq!(plain_input_offset(5, 5, 2, 3), Some(-4));
    }

    #[test]
    fn unrelated_format_or_enabled_roles_cannot_supply_alignment() {
        let metadata = fixture("1.000000", 90_000, 45_000, "1/90000");
        let av = parse(&metadata, &[0]);
        assert!(align_independent_caption_origin(&av, &av, 0).is_none());
        let caption = parse(&metadata, &[1]);
        assert!(align_independent_caption_origin(&caption, &caption, 0).is_none());
        assert!(align_independent_caption_origin(&av, &parse(&metadata, &[0, 1]), 0).is_none());
        let mut other = metadata;
        other["format"]["format_name"] = json!("matroska,webm");
        assert!(alignment(&other, 0).is_none());
    }
}
