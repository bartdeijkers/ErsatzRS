use serde::{Deserialize, Serialize};

use crate::{EntityId, FilterMode, ScalingBehavior};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum HardwareAccelerationKind {
    #[default]
    None,
    Qsv,
    Nvenc,
    Vaapi,
    VideoToolbox,
    Amf,
    V4l2m2m,
    Rkmpp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FFmpegProfileVideoFormat {
    #[default]
    None,
    H264,
    Hevc,
    Mpeg2Video,
    Av1,
    Copy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FFmpegProfileAudioFormat {
    #[default]
    None,
    Aac,
    Ac3,
    AacLatm,
    Copy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FFmpegProfileBitDepth {
    #[default]
    EightBit,
    TenBit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FFmpegProfileTonemapAlgorithm {
    #[default]
    Linear,
    Clip,
    Gamma,
    Reinhard,
    Mobius,
    Hable,
}

/// Output dynamic-range contract of an [`FFmpegProfile`] (ADR 0064).
///
/// The profile describes the *output* stream, not a per-item preference, so an
/// `Hdr10Bt2020Pq` profile stays HDR10 when an SDR item follows an HDR item.
/// Bit depth stays a separate field: 10-bit SDR is valid and must not be
/// mistaken for HDR.
///
/// Only meaningful when video is transcoded.
/// [`FFmpegProfileVideoFormat::Copy`] preserves the source elementary stream
/// and ignores this field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum OutputDynamicRange {
    /// Produce BT.709 SDR. HDR10, HLG, and supported Dolby Vision inputs are
    /// tone-mapped to SDR and all HDR/Dolby Vision signaling is removed.
    #[default]
    SdrBt709,
    /// Produce a stable BT.2020/PQ HDR10 output. HDR input is preserved or
    /// normalized; SDR input is mapped into the HDR10 signal at a fixed
    /// reference white without inventing highlight detail.
    Hdr10Bt2020Pq,
}

impl OutputDynamicRange {
    /// True when the profile must emit BT.2020/PQ HDR10 signaling.
    #[must_use]
    pub const fn is_hdr10(self) -> bool {
        matches!(self, Self::Hdr10Bt2020Pq)
    }

    /// Codecs able to carry HDR10 static metadata in the containers ErsatzRS
    /// serves (MPEG-TS and fMP4/HLS).
    ///
    /// H.264 is excluded deliberately: while HEVC, AV1, and MPEG-2 signalling
    /// differ, only HEVC and AV1 have a mastering-display/content-light SEI or
    /// OBU metadata path that `FFmpeg` emits and that clients read. Rejecting the
    /// rest is ADR 0064's "never silently downgrade" rule.
    #[must_use]
    pub const fn codec_supports_hdr10(video_format: FFmpegProfileVideoFormat) -> bool {
        matches!(
            video_format,
            FFmpegProfileVideoFormat::Hevc | FFmpegProfileVideoFormat::Av1
        )
    }
}

/// SMPTE ST 2086 mastering-display metadata, in the fixed-point units `FFmpeg`
/// and x265 use: chromaticities in 1/50000 increments and luminance in
/// 1/10000 cd/m².
///
/// ADR 0064 requires the values ErsatzRS emits to be documented and editable
/// rather than hidden inside a format string, so this is a real value type with
/// a parser, bounds, and a per-profile override rather than a constant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MasteringDisplayMetadata {
    pub green_x: u32,
    pub green_y: u32,
    pub blue_x: u32,
    pub blue_y: u32,
    pub red_x: u32,
    pub red_y: u32,
    pub white_point_x: u32,
    pub white_point_y: u32,
    pub max_luminance: u32,
    pub min_luminance: u32,
}

impl Default for MasteringDisplayMetadata {
    fn default() -> Self {
        Self::BT2020_1000_NITS
    }
}

impl MasteringDisplayMetadata {
    /// Documented seeded default: BT.2020 container primaries with a D65 white
    /// point, 1000 cd/m² peak and 0.0001 cd/m² black.
    ///
    /// ADR 0064 forbids hidden guessed values. This is what ErsatzRS emits when
    /// a transform changed the meaning of the source metadata or the source
    /// carried none, and when the profile does not override it.
    pub const BT2020_1000_NITS: Self = Self {
        green_x: 8_500,
        green_y: 39_850,
        blue_x: 6_550,
        blue_y: 2_300,
        red_x: 35_400,
        red_y: 14_600,
        white_point_x: 15_635,
        white_point_y: 16_450,
        max_luminance: 10_000_000,
        min_luminance: 1,
    };

    /// Largest chromaticity coordinate ST 2086 can express (1.0 in 1/50000
    /// units).
    pub const MAX_CHROMATICITY: u32 = 50_000;
    /// 100000 cd/m² in 1/10000 units — far above any real mastering display,
    /// but the point is to reject typos, not to predict display technology.
    pub const MAX_LUMINANCE: u32 = 1_000_000_000;

    /// Render the `-mastering_display` / x265 `master-display` value.
    #[must_use]
    pub fn to_ffmpeg_value(self) -> String {
        format!(
            "G({},{})B({},{})R({},{})WP({},{})L({},{})",
            self.green_x,
            self.green_y,
            self.blue_x,
            self.blue_y,
            self.red_x,
            self.red_y,
            self.white_point_x,
            self.white_point_y,
            self.max_luminance,
            self.min_luminance,
        )
    }

    /// Parse the `G(x,y)B(x,y)R(x,y)WP(x,y)L(max,min)` form that
    /// [`Self::to_ffmpeg_value`] produces.
    ///
    /// Returns `None` for anything malformed or out of range, so an operator
    /// cannot persist a value that would make every HDR10 encode fail at spawn
    /// time. Whitespace between groups is tolerated because operators paste
    /// these from mediainfo output.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        let mut rest = value.trim();
        let mut pair = |label: &str| -> Option<(u32, u32)> {
            rest = rest.trim_start().strip_prefix(label)?.trim_start();
            rest = rest.strip_prefix('(')?;
            let (inside, tail) = rest.split_once(')')?;
            rest = tail;
            let (first, second) = inside.split_once(',')?;
            Some((first.trim().parse().ok()?, second.trim().parse().ok()?))
        };

        // Fixed order, matching what `to_ffmpeg_value` writes and what x265
        // documents. Accepting groups in any order would let a typo silently
        // swap the red and green primaries.
        let (green_x, green_y) = pair("G")?;
        let (blue_x, blue_y) = pair("B")?;
        let (red_x, red_y) = pair("R")?;
        let (white_point_x, white_point_y) = pair("WP")?;
        let (max_luminance, min_luminance) = pair("L")?;

        if !rest.trim().is_empty() {
            return None;
        }

        let parsed = Self {
            green_x,
            green_y,
            blue_x,
            blue_y,
            red_x,
            red_y,
            white_point_x,
            white_point_y,
            max_luminance,
            min_luminance,
        };
        parsed.is_valid().then_some(parsed)
    }

    /// True when every coordinate is inside ST 2086's range and the luminance
    /// window is non-degenerate.
    #[must_use]
    pub const fn is_valid(self) -> bool {
        let chromaticities = [
            self.green_x,
            self.green_y,
            self.blue_x,
            self.blue_y,
            self.red_x,
            self.red_y,
            self.white_point_x,
            self.white_point_y,
        ];
        let mut index = 0;
        while index < chromaticities.len() {
            if chromaticities[index] > Self::MAX_CHROMATICITY {
                return false;
            }
            index += 1;
        }
        self.max_luminance <= Self::MAX_LUMINANCE && self.max_luminance > self.min_luminance
    }
}

/// CTA-861.3 content light level: `MaxCLL` and `MaxFALL`, both in cd/m².
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentLightLevel {
    pub max_cll: u32,
    pub max_fall: u32,
}

impl Default for ContentLightLevel {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl ContentLightLevel {
    /// Documented seeded default paired with
    /// [`MasteringDisplayMetadata::BT2020_1000_NITS`].
    pub const DEFAULT: Self = Self {
        max_cll: 1_000,
        max_fall: 400,
    };

    /// CTA-861.3 stores both as unsigned 16-bit values.
    pub const MAX_LEVEL: u32 = 65_535;

    /// Build a validated pair.
    ///
    /// `MaxFALL` is a frame-average, so it can never exceed `MaxCLL`; a pair
    /// that violates that describes no real content and some clients reject it
    /// outright.
    #[must_use]
    pub const fn new(max_cll: u32, max_fall: u32) -> Option<Self> {
        let candidate = Self { max_cll, max_fall };
        if candidate.is_valid() {
            Some(candidate)
        } else {
            None
        }
    }

    /// True when both levels fit CTA-861.3 and `MaxFALL <= MaxCLL`.
    #[must_use]
    pub const fn is_valid(self) -> bool {
        self.max_cll > 0
            && self.max_cll <= Self::MAX_LEVEL
            && self.max_fall <= Self::MAX_LEVEL
            && self.max_fall <= self.max_cll
    }

    /// Render the `-content_light` value.
    #[must_use]
    pub fn to_ffmpeg_value(self) -> String {
        format!("{},{}", self.max_cll, self.max_fall)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum NormalizeLoudnessMode {
    #[default]
    Off,
    LoudNorm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum VaapiDriver {
    #[default]
    Auto,
    IHD,
    I965,
    RadeonSI,
    Nouveau,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Resolution {
    pub id: EntityId,
    pub name: String,
    pub is_custom: bool,
    pub width: i32,
    pub height: i32,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct FFmpegProfile {
    pub id: EntityId,
    pub name: String,
    pub thread_count: i32,
    pub transcode_audio: bool,
    pub transcode_video: bool,
    pub hardware_acceleration: HardwareAccelerationKind,
    pub vaapi_display: Option<String>,
    pub vaapi_driver: VaapiDriver,
    pub vaapi_device: Option<String>,
    pub qsv_extra_hardware_frames: Option<i32>,
    pub resolution_id: EntityId,
    pub scaling_behavior: ScalingBehavior,
    pub pad_mode: FilterMode,
    pub video_format: FFmpegProfileVideoFormat,
    pub video_profile: Option<String>,
    pub video_preset: Option<String>,
    pub video_tune: Option<String>,
    pub allow_b_frames: bool,
    pub bit_depth: FFmpegProfileBitDepth,
    pub video_bitrate: i32,
    pub video_buffer_size: i32,
    pub tonemap_algorithm: FFmpegProfileTonemapAlgorithm,
    /// ADR 0064 output dynamic-range contract. Ignored when
    /// `video_format == FFmpegProfileVideoFormat::Copy`.
    pub output_dynamic_range: OutputDynamicRange,
    /// Mastering display to signal when HDR10 output must synthesize metadata,
    /// or `None` to use [`MasteringDisplayMetadata::BT2020_1000_NITS`].
    ///
    /// Never overrides metadata the source itself carried: ADR 0064 prefers a
    /// real grade over a configured one. This is the seeded default made
    /// editable, for operators whose displays are graded differently.
    pub hdr_mastering_display: Option<MasteringDisplayMetadata>,
    /// Content light levels paired with `hdr_mastering_display`, or `None` to
    /// use [`ContentLightLevel::DEFAULT`].
    pub hdr_content_light: Option<ContentLightLevel>,
    pub audio_format: FFmpegProfileAudioFormat,
    pub audio_bitrate: i32,
    pub audio_buffer_size: i32,
    pub normalize_loudness_mode: NormalizeLoudnessMode,
    pub target_loudness: Option<f64>,
    pub audio_channels: i32,
    pub audio_sample_rate: i32,
    pub normalize_framerate: bool,
    pub normalize_colors: bool,
    pub deinterlace_video: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::{ContentLightLevel, MasteringDisplayMetadata};

    #[test]
    fn the_documented_default_round_trips_through_the_parser() {
        let rendered = MasteringDisplayMetadata::BT2020_1000_NITS.to_ffmpeg_value();

        assert_eq!(
            rendered,
            "G(8500,39850)B(6550,2300)R(35400,14600)WP(15635,16450)L(10000000,1)"
        );
        assert_eq!(
            MasteringDisplayMetadata::parse(&rendered),
            Some(MasteringDisplayMetadata::BT2020_1000_NITS),
            "an operator must be able to paste back what the editor showed them"
        );
    }

    #[test]
    fn parsing_tolerates_the_spacing_operators_paste() {
        let spaced =
            " G(8500, 39850) B(6550, 2300) R(35400, 14600) WP(15635, 16450) L(10000000, 1) ";

        assert_eq!(
            MasteringDisplayMetadata::parse(spaced),
            Some(MasteringDisplayMetadata::BT2020_1000_NITS)
        );
    }

    #[test]
    fn parsing_rejects_values_ffmpeg_would_choke_on() {
        // Missing group, wrong order, trailing junk, out-of-range chromaticity,
        // and a degenerate luminance window.
        for rejected in [
            "G(8500,39850)B(6550,2300)R(35400,14600)L(10000000,1)",
            "B(6550,2300)G(8500,39850)R(35400,14600)WP(15635,16450)L(10000000,1)",
            "G(8500,39850)B(6550,2300)R(35400,14600)WP(15635,16450)L(10000000,1)junk",
            "G(50001,39850)B(6550,2300)R(35400,14600)WP(15635,16450)L(10000000,1)",
            "G(8500,39850)B(6550,2300)R(35400,14600)WP(15635,16450)L(1,10000000)",
            "",
            "not metadata at all",
        ] {
            assert_eq!(
                MasteringDisplayMetadata::parse(rejected),
                None,
                "{rejected} must not be accepted"
            );
        }
    }

    #[test]
    fn content_light_enforces_the_cta_861_3_relationship() {
        assert_eq!(
            ContentLightLevel::new(1_000, 400),
            Some(ContentLightLevel::DEFAULT)
        );
        assert!(ContentLightLevel::DEFAULT.is_valid());

        // MaxFALL is a frame average, so it can never exceed MaxCLL.
        assert_eq!(ContentLightLevel::new(400, 1_000), None);
        // Both are unsigned 16-bit in CTA-861.3.
        assert_eq!(ContentLightLevel::new(65_536, 400), None);
        assert_eq!(ContentLightLevel::new(1_000, 65_536), None);
        // Zero MaxCLL describes no content and would be a hidden default.
        assert_eq!(ContentLightLevel::new(0, 0), None);
    }
}
