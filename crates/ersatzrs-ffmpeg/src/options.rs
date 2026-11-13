use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EnvironmentVariable {
    pub name: String,
    pub value: String,
}

impl EnvironmentVariable {
    #[must_use]
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamKind {
    Video,
    Audio,
    Subtitle,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PipelineStep {
    Generic {
        name: &'static str,
        global_options: Vec<String>,
        filter_options: Vec<String>,
        output_options: Vec<String>,
        environment_variables: Vec<EnvironmentVariable>,
    },
    Encoder {
        name: String,
        kind: StreamKind,
        output_options: Vec<String>,
    },
    ComplexFilter {
        options: Vec<String>,
    },
    StreamSeek {
        position: Duration,
    },
}

impl PipelineStep {
    #[must_use]
    pub fn generic(
        name: &'static str,
        output_options: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self::Generic {
            name,
            global_options: Vec::new(),
            filter_options: Vec::new(),
            output_options: output_options.into_iter().map(Into::into).collect(),
            environment_variables: Vec::new(),
        }
    }

    #[must_use]
    pub fn global(
        name: &'static str,
        options: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self::Generic {
            name,
            global_options: options.into_iter().map(Into::into).collect(),
            filter_options: Vec::new(),
            output_options: Vec::new(),
            environment_variables: Vec::new(),
        }
    }

    #[must_use]
    pub fn filter(
        name: &'static str,
        options: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self::Generic {
            name,
            global_options: Vec::new(),
            filter_options: options.into_iter().map(Into::into).collect(),
            output_options: Vec::new(),
            environment_variables: Vec::new(),
        }
    }

    #[must_use]
    pub fn env(name: &'static str, variable: EnvironmentVariable) -> Self {
        Self::Generic {
            name,
            global_options: Vec::new(),
            filter_options: Vec::new(),
            output_options: Vec::new(),
            environment_variables: vec![variable],
        }
    }

    #[must_use]
    pub fn encoder(
        name: impl Into<String>,
        kind: StreamKind,
        extra: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        let name = name.into();
        let mut output_options = vec![format!("-codec:{}", stream_suffix(kind)), name.clone()];
        output_options.extend(extra.into_iter().map(Into::into));
        Self::Encoder {
            name,
            kind,
            output_options,
        }
    }

    #[must_use]
    pub fn complex_filter(options: Vec<String>) -> Self {
        Self::ComplexFilter { options }
    }

    #[must_use]
    pub fn stream_seek(position: Duration) -> Self {
        Self::StreamSeek { position }
    }

    #[must_use]
    pub fn global_options(&self) -> &[String] {
        match self {
            Self::Generic { global_options, .. } => global_options,
            Self::StreamSeek { .. } | Self::Encoder { .. } | Self::ComplexFilter { .. } => &[],
        }
    }

    #[must_use]
    pub fn filter_options(&self) -> &[String] {
        match self {
            Self::Generic { filter_options, .. } => filter_options,
            Self::ComplexFilter { options } => options,
            Self::StreamSeek { .. } | Self::Encoder { .. } => &[],
        }
    }

    #[must_use]
    pub fn output_options(&self) -> &[String] {
        match self {
            Self::Generic { output_options, .. } | Self::Encoder { output_options, .. } => {
                output_options
            }
            Self::StreamSeek { .. } | Self::ComplexFilter { .. } => &[],
        }
    }

    #[must_use]
    pub fn environment_variables(&self) -> &[EnvironmentVariable] {
        match self {
            Self::Generic {
                environment_variables,
                ..
            } => environment_variables,
            Self::StreamSeek { .. } | Self::Encoder { .. } | Self::ComplexFilter { .. } => &[],
        }
    }

    #[must_use]
    pub fn is_video_encoder(&self) -> bool {
        matches!(
            self,
            Self::Encoder {
                kind: StreamKind::Video,
                ..
            }
        )
    }

    #[must_use]
    pub fn is_reordered_out_of_output_pass(&self) -> bool {
        matches!(self, Self::StreamSeek { .. } | Self::ComplexFilter { .. })
    }
}

const fn stream_suffix(kind: StreamKind) -> &'static str {
    match kind {
        StreamKind::Video => "v",
        StreamKind::Audio => "a",
        StreamKind::Subtitle => "s",
        StreamKind::Other => "d",
    }
}

#[must_use]
pub fn no_standard_input() -> PipelineStep {
    PipelineStep::global("nostdin", ["-nostdin"])
}

#[must_use]
pub fn hide_banner() -> PipelineStep {
    PipelineStep::global("hide_banner", ["-hide_banner"])
}

#[must_use]
pub fn no_stats() -> PipelineStep {
    PipelineStep::global("nostats", ["-nostats"])
}

#[must_use]
pub fn progress_to_stdout() -> PipelineStep {
    PipelineStep::global("progress", ["-progress", "pipe:1"])
}

/// Keep machine-readable completion reports separate from encoded stdout.
#[must_use]
pub fn progress_to_stderr() -> PipelineStep {
    PipelineStep::global("progress_stderr", ["-progress", "pipe:2"])
}

#[must_use]
pub fn loglevel_error() -> PipelineStep {
    PipelineStep::global("loglevel_error", ["-loglevel", "error"])
}

#[must_use]
pub fn standard_format_flags() -> PipelineStep {
    PipelineStep::global(
        "standard_format_flags",
        ["-fflags", "+genpts+discardcorrupt+igndts"],
    )
}

#[must_use]
pub fn map_all_streams() -> PipelineStep {
    PipelineStep::generic("map_all_streams", ["-map", "0"])
}

#[must_use]
pub fn copy_all_streams() -> PipelineStep {
    PipelineStep::generic("copy_all_streams", ["-c", "copy"])
}

#[must_use]
pub fn closed_gop() -> PipelineStep {
    PipelineStep::generic("closed_gop", ["-flags", "cgop"])
}

#[must_use]
pub fn no_b_frames() -> PipelineStep {
    PipelineStep::generic("no_b_frames", ["-bf", "0"])
}

#[must_use]
pub fn fast_start() -> PipelineStep {
    PipelineStep::generic("fast_start", ["-movflags", "+faststart"])
}

#[must_use]
pub fn no_demux_decode_delay() -> PipelineStep {
    PipelineStep::generic(
        "no_demux_decode_delay",
        ["-muxdelay", "0", "-muxpreload", "0"],
    )
}

#[must_use]
pub fn no_scene_detect(value: u32) -> PipelineStep {
    PipelineStep::generic(
        "no_scene_detect",
        ["-sc_threshold".to_owned(), value.to_string()],
    )
}

const MPEGTS_FALLBACK_MUXRATE_KBPS: i64 = 10_000;
const MPEGTS_MUXRATE_OVERHEAD_NUMERATOR: i64 = 12;
const MPEGTS_MUXRATE_OVERHEAD_DENOMINATOR: i64 = 10;
const MPEGTS_MAX_MUXRATE_BPS: i64 = 1_500_000_000;

/// PCR emission period used for MPEG-TS outputs with profile-derived CBR muxing.
pub const MPEGTS_PCR_PERIOD_MS: u32 = 20;

/// MPEG-TS muxer timing options shared by direct streams, HLS segments, and wrappers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MpegTsMuxingOptions {
    /// Constant mux rate in bits per second.
    pub muxrate_bps: i64,
    /// PCR period in milliseconds.
    pub pcr_period_ms: u32,
}

impl MpegTsMuxingOptions {
    /// Derive a CBR mux rate from profile bitrates with enough overhead for TS packets.
    #[must_use]
    pub fn from_bitrates(video_kbps: Option<u32>, audio_kbps: Option<u32>) -> Self {
        let total_kbps = video_kbps
            .map(i64::from)
            .unwrap_or_default()
            .saturating_add(audio_kbps.map(i64::from).unwrap_or_default());
        let total_kbps = if total_kbps > 0 {
            total_kbps
        } else {
            MPEGTS_FALLBACK_MUXRATE_KBPS
        };
        let muxrate_bps = total_kbps
            .saturating_mul(1_000)
            .saturating_mul(MPEGTS_MUXRATE_OVERHEAD_NUMERATOR)
            / MPEGTS_MUXRATE_OVERHEAD_DENOMINATOR;

        Self {
            muxrate_bps: muxrate_bps.min(MPEGTS_MAX_MUXRATE_BPS),
            pcr_period_ms: MPEGTS_PCR_PERIOD_MS,
        }
    }

    /// Render `FFmpeg` HLS `-hls_segment_options` for MPEG-TS segment muxing.
    #[must_use]
    pub fn hls_segment_options(self, existing: Option<&str>) -> String {
        let mut options = Vec::new();
        if let Some(existing) = existing.filter(|value| !value.trim().is_empty()) {
            options.push(existing.to_owned());
        }
        options.push(format!("muxrate={}", self.muxrate_bps));
        options.push(format!("pcr_period={}", self.pcr_period_ms));
        options.join(":")
    }
}

/// Render the `FFmpeg` `-muxrate` output option.
#[must_use]
pub fn mpegts_muxrate(options: MpegTsMuxingOptions) -> PipelineStep {
    PipelineStep::generic(
        "mpegts_muxrate",
        ["-muxrate".to_owned(), options.muxrate_bps.to_string()],
    )
}

/// Render the `FFmpeg` `-pcr_period` output option.
#[must_use]
pub fn mpegts_pcr_period(options: MpegTsMuxingOptions) -> PipelineStep {
    PipelineStep::generic(
        "mpegts_pcr_period",
        ["-pcr_period".to_owned(), options.pcr_period_ms.to_string()],
    )
}

#[must_use]
pub fn time_limit(duration: Duration) -> PipelineStep {
    PipelineStep::generic("time_limit", ["-t".to_owned(), format_time_limit(duration)])
}

/// Sample one selected video stream for native interlace detection.
#[must_use]
pub fn interlace_probe_arguments(path: &str, index: usize, seek: Duration) -> Vec<String> {
    vec![
        "-hide_banner".into(),
        "-nostdin".into(),
        "-ss".into(),
        format_time_limit(seek),
        "-i".into(),
        path.into(),
        "-map".into(),
        format!("0:{index}"),
        "-vf".into(),
        "idet".into(),
        "-frames:v".into(),
        "200".into(),
        "-an".into(),
        "-sn".into(),
        "-dn".into(),
        "-f".into(),
        "null".into(),
        "-".into(),
    ]
}

#[must_use]
pub fn format_time_limit(duration: Duration) -> String {
    let total_seconds = duration.as_secs();
    let hours = total_seconds / 3_600;
    let minutes = (total_seconds % 3_600) / 60;
    let seconds = total_seconds % 60;
    let ticks = duration.subsec_nanos() / 100;
    format!("{hours:02}:{minutes:02}:{seconds:02}.{ticks:07}")
}

/// Microseconds parsed from the current generated `HH:MM:SS.fffffff` syntax.
///
/// Native `FFmpeg` 9 `av_parse_time` consumes at most four hour digits and six
/// fractional digits. Larger hour values emitted by [`format_time_limit`]
/// therefore have no valid native time, even when their microseconds fit i64.
/// This does not validate arbitrary custom time strings or source timing.
#[must_use]
pub fn parsed_time_limit_microseconds(duration: Duration) -> Option<i64> {
    if duration.as_secs() / 3_600 > 9_999 {
        return None;
    }
    i64::try_from(duration.as_micros()).ok()
}

#[must_use]
pub fn no_autoscale() -> PipelineStep {
    PipelineStep::generic("no_autoscale", ["-noautoscale"])
}

#[must_use]
pub fn mpegts_output(initial_discontinuity: bool) -> PipelineStep {
    if initial_discontinuity {
        PipelineStep::generic(
            "mpegts",
            ["-f", "mpegts", "-mpegts_flags", "+initial_discontinuity"],
        )
    } else {
        PipelineStep::generic("mpegts", ["-f", "mpegts"])
    }
}

#[must_use]
pub fn pipe_output() -> PipelineStep {
    PipelineStep::generic("pipe", ["pipe:1"])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multi_day_interlace_probe_seeks_and_maps_only_selected_video() {
        insta::assert_snapshot!(interlace_probe_arguments("fixture.mkv", 3, Duration::from_hours(48)).join(" "), @"-hide_banner -nostdin -ss 48:00:00.0000000 -i fixture.mkv -map 0:3 -vf idet -frames:v 200 -an -sn -dn -f null -");
    }

    #[test]
    fn multi_day_time_limits_keep_total_hours_and_fractional_precision() {
        for (duration, expected) in [
            (Duration::ZERO, "00:00:00.0000000"),
            (Duration::new(86_399, 999_999_900), "23:59:59.9999999"),
            (Duration::from_hours(24), "24:00:00.0000000"),
            (Duration::new(90_061, 123_456_700), "25:01:01.1234567"),
            (Duration::from_hours(100), "100:00:00.0000000"),
        ] {
            assert_eq!(format_time_limit(duration), expected);
        }
    }

    #[test]
    fn parsed_time_limits_require_four_hour_digits_and_floor_fractional_microseconds() {
        assert_eq!(parsed_time_limit_microseconds(Duration::ZERO), Some(0));
        assert_eq!(
            parsed_time_limit_microseconds(Duration::new(90_061, 123_456_789)),
            Some(90_061_123_456)
        );
        let last = Duration::new(35_999_999, 999_999_999);
        assert_eq!(format_time_limit(last), "9999:59:59.9999999");
        assert_eq!(
            parsed_time_limit_microseconds(last),
            Some(35_999_999_999_999)
        );
        for invalid in [Duration::from_hours(10_000), Duration::MAX] {
            assert_eq!(parsed_time_limit_microseconds(invalid), None);
        }
    }

    #[test]
    fn mpegts_muxing_uses_profile_bitrates_with_overhead() {
        let options = MpegTsMuxingOptions::from_bitrates(Some(8_000), Some(192));

        assert_eq!(options.muxrate_bps, 9_830_400);
        assert_eq!(options.pcr_period_ms, 20);
        assert_eq!(
            options.hls_segment_options(Some("mpegts_flags=+initial_discontinuity")),
            "mpegts_flags=+initial_discontinuity:muxrate=9830400:pcr_period=20"
        );
    }

    #[test]
    fn mpegts_muxing_uses_fallback_when_bitrates_are_absent() {
        let options = MpegTsMuxingOptions::from_bitrates(None, None);

        assert_eq!(options.muxrate_bps, 12_000_000);
    }
}
