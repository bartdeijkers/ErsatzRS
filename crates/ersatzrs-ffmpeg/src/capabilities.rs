use ersatzrs_domain::{FFmpegProfileBitDepth, FFmpegProfileVideoFormat, HardwareAccelerationKind};

use crate::{
    DesiredVideoState, FfmpegCapabilityReport, FfmpegKnownDecoder, PixelFormat, SourceVideoCodec,
    SourceVideoStream, VaapiProfileEntrypoint,
};

/// Whether an encode/decode decision should use hardware or software.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FfmpegCapability {
    Software,
    Hardware,
}

impl FfmpegCapability {
    /// Returns true when the decision uses hardware.
    #[must_use]
    pub const fn is_hardware(self) -> bool {
        matches!(self, Self::Hardware)
    }
}

/// Typed hardware capability checks backed by the `FFmpeg` runtime report.
pub trait HardwareCapabilities {
    /// Return the hardware acceleration kind represented by this checker.
    fn acceleration(&self) -> HardwareAccelerationKind;

    /// Return whether source video can be decoded in hardware.
    fn can_decode(&self, source: &SourceVideoStream) -> FfmpegCapability;

    /// Return whether desired video can be encoded in hardware.
    fn can_encode(&self, desired: &DesiredVideoState) -> FfmpegCapability;
}

/// Factory for report-backed hardware capability checkers.
#[derive(Debug, Clone, Copy)]
pub struct HardwareCapabilitiesFactory<'a> {
    report: &'a FfmpegCapabilityReport,
}

impl<'a> HardwareCapabilitiesFactory<'a> {
    /// Create a factory backed by a Slice 10 runtime capability report.
    #[must_use]
    pub const fn new(report: &'a FfmpegCapabilityReport) -> Self {
        Self { report }
    }

    /// Build the checker for a configured hardware acceleration kind.
    #[must_use]
    pub fn for_acceleration(
        &self,
        acceleration: HardwareAccelerationKind,
    ) -> Box<dyn HardwareCapabilities + 'a> {
        if acceleration == HardwareAccelerationKind::None
            || !self.has_hardware_acceleration(acceleration)
        {
            return Box::new(NoHardwareCapabilities);
        }

        match acceleration {
            HardwareAccelerationKind::Qsv => Box::new(ReportHardwareCapabilities::new(
                acceleration,
                self.report,
                HardwareRules::Qsv,
            )),
            HardwareAccelerationKind::Nvenc => Box::new(ReportHardwareCapabilities::new(
                acceleration,
                self.report,
                HardwareRules::Nvidia,
            )),
            HardwareAccelerationKind::Vaapi => Box::new(ReportHardwareCapabilities::new(
                acceleration,
                self.report,
                HardwareRules::Vaapi,
            )),
            HardwareAccelerationKind::VideoToolbox => Box::new(ReportHardwareCapabilities::new(
                acceleration,
                self.report,
                HardwareRules::VideoToolbox,
            )),
            HardwareAccelerationKind::Amf => Box::new(ReportHardwareCapabilities::new(
                acceleration,
                self.report,
                HardwareRules::Amf,
            )),
            HardwareAccelerationKind::V4l2m2m => Box::new(ReportHardwareCapabilities::new(
                acceleration,
                self.report,
                HardwareRules::V4l2m2m,
            )),
            HardwareAccelerationKind::Rkmpp => Box::new(ReportHardwareCapabilities::new(
                acceleration,
                self.report,
                HardwareRules::Rkmpp,
            )),
            HardwareAccelerationKind::None => Box::new(NoHardwareCapabilities),
        }
    }

    /// Return whether the runtime report exposes a requested acceleration kind.
    #[must_use]
    pub fn has_hardware_acceleration(&self, acceleration: HardwareAccelerationKind) -> bool {
        match acceleration {
            HardwareAccelerationKind::None => true,
            HardwareAccelerationKind::Qsv => self.report.has_hwaccel("qsv"),
            HardwareAccelerationKind::Nvenc => self.report.has_hwaccel("cuda"),
            HardwareAccelerationKind::Vaapi => self.report.has_hwaccel("vaapi"),
            HardwareAccelerationKind::VideoToolbox => self.report.has_hwaccel("videotoolbox"),
            HardwareAccelerationKind::Rkmpp => self.report.has_hwaccel("rkmpp"),
            HardwareAccelerationKind::Amf => has_encoder_suffix(self.report, "_amf"),
            HardwareAccelerationKind::V4l2m2m => has_encoder_suffix(self.report, "_v4l2m2m"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NoHardwareCapabilities;

impl HardwareCapabilities for NoHardwareCapabilities {
    fn acceleration(&self) -> HardwareAccelerationKind {
        HardwareAccelerationKind::None
    }

    fn can_decode(&self, _source: &SourceVideoStream) -> FfmpegCapability {
        FfmpegCapability::Software
    }

    fn can_encode(&self, _desired: &DesiredVideoState) -> FfmpegCapability {
        FfmpegCapability::Software
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HardwareRules {
    Qsv,
    Nvidia,
    VideoToolbox,
    Vaapi,
    Amf,
    V4l2m2m,
    Rkmpp,
}

struct ReportHardwareCapabilities<'a> {
    acceleration: HardwareAccelerationKind,
    report: &'a FfmpegCapabilityReport,
    rules: HardwareRules,
}

impl<'a> ReportHardwareCapabilities<'a> {
    const fn new(
        acceleration: HardwareAccelerationKind,
        report: &'a FfmpegCapabilityReport,
        rules: HardwareRules,
    ) -> Self {
        Self {
            acceleration,
            report,
            rules,
        }
    }
}

impl HardwareCapabilities for ReportHardwareCapabilities<'_> {
    fn acceleration(&self) -> HardwareAccelerationKind {
        self.acceleration
    }

    fn can_decode(&self, source: &SourceVideoStream) -> FfmpegCapability {
        match self.rules {
            HardwareRules::Vaapi => vaapi_decode(self.report, source),
            HardwareRules::Qsv => qsv_decode(self.report, source),
            HardwareRules::Nvidia => nvidia_decode(self.report, source),
            HardwareRules::VideoToolbox => videotoolbox_decode(self.report, source),
            HardwareRules::Amf => FfmpegCapability::Software,
            HardwareRules::V4l2m2m => v4l2m2m_decode(self.report, source),
            HardwareRules::Rkmpp => FfmpegCapability::Hardware,
        }
    }

    fn can_encode(&self, desired: &DesiredVideoState) -> FfmpegCapability {
        match self.rules {
            HardwareRules::V4l2m2m => default_encode(self.report, self.acceleration, desired),
            HardwareRules::Qsv => qsv_encode(self.report, desired),
            HardwareRules::Vaapi => vaapi_encode(self.report, desired),
            HardwareRules::Nvidia => nvidia_encode(self.report, desired),
            HardwareRules::VideoToolbox => videotoolbox_encode(self.report, desired),
            HardwareRules::Amf | HardwareRules::Rkmpp => {
                if is_unsupported_ten_bit_hardware_encode(desired) {
                    FfmpegCapability::Software
                } else {
                    encoder_available(self.report, self.acceleration, desired)
                }
            }
        }
    }
}

fn default_decode(source: &SourceVideoStream) -> FfmpegCapability {
    if is_unsupported_hardware_decode(source) {
        FfmpegCapability::Software
    } else {
        FfmpegCapability::Hardware
    }
}

fn default_encode(
    report: &FfmpegCapabilityReport,
    acceleration: HardwareAccelerationKind,
    desired: &DesiredVideoState,
) -> FfmpegCapability {
    if is_unsupported_h264_ten_bit(desired.video_format, desired.bit_depth) {
        FfmpegCapability::Software
    } else {
        encoder_available(report, acceleration, desired)
    }
}

fn vaapi_decode(report: &FfmpegCapabilityReport, source: &SourceVideoStream) -> FfmpegCapability {
    if report.vaapi_profiles.is_empty() {
        return default_decode(source);
    }

    if vaapi_decode_profile_matches(&report.vaapi_profiles, source) {
        FfmpegCapability::Hardware
    } else {
        FfmpegCapability::Software
    }
}

fn vaapi_encode(report: &FfmpegCapabilityReport, desired: &DesiredVideoState) -> FfmpegCapability {
    if report.vaapi_profiles.is_empty() {
        return default_encode(report, HardwareAccelerationKind::Vaapi, desired);
    }

    let Some(encoder_name) =
        hardware_encoder_name(HardwareAccelerationKind::Vaapi, desired.video_format)
    else {
        return FfmpegCapability::Software;
    };

    if report.has_encoder(encoder_name)
        && vaapi_encode_profile_matches(&report.vaapi_profiles, desired)
    {
        FfmpegCapability::Hardware
    } else {
        FfmpegCapability::Software
    }
}

fn qsv_decode(report: &FfmpegCapabilityReport, source: &SourceVideoStream) -> FfmpegCapability {
    if is_unsupported_qsv_decode(source) {
        return FfmpegCapability::Software;
    }

    let Some(name) = qsv_decoder_name(source.codec) else {
        return FfmpegCapability::Software;
    };

    let device_profile_matches = report.qsv_profiles.is_empty()
        || vaapi_decode_profile_matches(&report.qsv_profiles, source);
    if report.has_decoder(name) && device_profile_matches {
        FfmpegCapability::Hardware
    } else {
        FfmpegCapability::Software
    }
}

fn is_unsupported_qsv_decode(source: &SourceVideoStream) -> bool {
    let is_h264_or_hevc = matches!(
        source.codec,
        SourceVideoCodec::H264 | SourceVideoCodec::Hevc
    );
    is_unsupported_hardware_decode(source)
        || (is_h264_or_hevc && bit_depth(source.pixel_format) == 10)
}

fn qsv_encode(report: &FfmpegCapabilityReport, desired: &DesiredVideoState) -> FfmpegCapability {
    if report.qsv_profiles.is_empty() {
        return default_encode(report, HardwareAccelerationKind::Qsv, desired);
    }

    let Some(encoder_name) =
        hardware_encoder_name(HardwareAccelerationKind::Qsv, desired.video_format)
    else {
        return FfmpegCapability::Software;
    };

    if report.has_encoder(encoder_name)
        && vaapi_encode_profile_matches(&report.qsv_profiles, desired)
    {
        FfmpegCapability::Hardware
    } else {
        FfmpegCapability::Software
    }
}

fn vaapi_decode_profile_matches(
    profiles: &[VaapiProfileEntrypoint],
    source: &SourceVideoStream,
) -> bool {
    if source.color_params.is_bt2020_ten
        || (source.codec == SourceVideoCodec::H264 && bit_depth(source.pixel_format) == 10)
    {
        return false;
    }

    vaapi_decode_profiles(source)
        .iter()
        .any(|profile| has_vaapi_entrypoint(profiles, profile, VaapiEntrypointKind::Decode))
}

fn vaapi_encode_profile_matches(
    profiles: &[VaapiProfileEntrypoint],
    desired: &DesiredVideoState,
) -> bool {
    if desired.video_format == FFmpegProfileVideoFormat::H264
        && desired.bit_depth == FFmpegProfileBitDepth::TenBit
    {
        return false;
    }

    vaapi_encode_profiles(desired)
        .iter()
        .any(|profile| has_vaapi_entrypoint(profiles, profile, VaapiEntrypointKind::Encode))
}

fn vaapi_decode_profiles(source: &SourceVideoStream) -> Vec<&'static str> {
    let profile = source
        .profile
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    match source.codec {
        SourceVideoCodec::H264 => match profile.as_str() {
            "baseline" | "66" => Vec::new(),
            "main" | "77" => vec!["VAProfileH264Main"],
            "high" | "100" | "high 10" | "110" => vec!["VAProfileH264High"],
            "baseline constrained" | "constrained baseline" | "578" => {
                vec!["VAProfileH264ConstrainedBaseline"]
            }
            _ => vec!["VAProfileH264Main", "VAProfileH264High"],
        },
        SourceVideoCodec::Hevc => match profile.as_str() {
            "main 10" | "2" => vec!["VAProfileHEVCMain10"],
            _ => vec!["VAProfileHEVCMain"],
        },
        SourceVideoCodec::Mpeg2Video => match profile.as_str() {
            "simple" | "5" => vec!["VAProfileMPEG2Simple"],
            _ => vec!["VAProfileMPEG2Main"],
        },
        SourceVideoCodec::Vc1 => match profile.as_str() {
            "simple" | "0" => vec!["VAProfileVC1Simple"],
            "advanced" | "3" => vec!["VAProfileVC1Advanced"],
            _ => vec!["VAProfileVC1Main"],
        },
        SourceVideoCodec::Vp9 => match profile.as_str() {
            "profile 1" | "1" => vec!["VAProfileVP9Profile1"],
            "profile 2" | "2" => vec!["VAProfileVP9Profile2"],
            "profile 3" | "3" => vec!["VAProfileVP9Profile3"],
            _ => vec!["VAProfileVP9Profile0"],
        },
        SourceVideoCodec::Av1 => vec!["VAProfileAV1Profile0"],
        SourceVideoCodec::Mpeg1Video
        | SourceVideoCodec::Mpeg4
        | SourceVideoCodec::MsMpeg4V2
        | SourceVideoCodec::MsMpeg4V3
        | SourceVideoCodec::RawVideo
        | SourceVideoCodec::Vp8
        | SourceVideoCodec::Unknown => Vec::new(),
    }
}

fn vaapi_encode_profiles(desired: &DesiredVideoState) -> Vec<&'static str> {
    match (desired.video_format, desired.bit_depth) {
        (FFmpegProfileVideoFormat::H264, FFmpegProfileBitDepth::EightBit) => {
            vec!["VAProfileH264Main"]
        }
        (FFmpegProfileVideoFormat::Hevc, FFmpegProfileBitDepth::TenBit) => {
            vec!["VAProfileHEVCMain10"]
        }
        (FFmpegProfileVideoFormat::Hevc, FFmpegProfileBitDepth::EightBit) => {
            vec!["VAProfileHEVCMain"]
        }
        (FFmpegProfileVideoFormat::Av1, _) => vec!["VAProfileAV1Profile0"],
        (FFmpegProfileVideoFormat::Mpeg2Video, _) => vec!["VAProfileMPEG2Main"],
        _ => Vec::new(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VaapiEntrypointKind {
    Decode,
    Encode,
}

fn has_vaapi_entrypoint(
    profiles: &[VaapiProfileEntrypoint],
    expected_profile: &str,
    kind: VaapiEntrypointKind,
) -> bool {
    profiles.iter().any(|entry| {
        entry.profile_matches(expected_profile)
            && match kind {
                VaapiEntrypointKind::Decode => entry.is_decode(),
                VaapiEntrypointKind::Encode => entry.is_encode(),
            }
    })
}

fn nvidia_decode(report: &FfmpegCapabilityReport, source: &SourceVideoStream) -> FfmpegCapability {
    // Hardware NVENC decode places frames on the GPU and runs them through the
    // CUDA filter graph (yadif_cuda/scale_cuda/overlay_cuda). If those filters
    // aren't compiled in (encoder-only build), that graph is unrunnable, so
    // decode in software — the pipeline then runs software filters and still
    // NVENC-encodes the result, preserving GPU-accelerated encode.
    if !nvenc_cuda_filters_available(report) {
        return FfmpegCapability::Software;
    }

    if is_unsupported_hardware_decode(source) {
        return FfmpegCapability::Software;
    }

    let codec = nvidia_decoder_codec(source.codec);
    let bit_depth = bit_depth(source.pixel_format);
    if report.hardware_decode_denied(HardwareAccelerationKind::Nvenc, codec, bit_depth) {
        return FfmpegCapability::Software;
    }

    // ErsatzTV's conservative architecture gate (ADR 0014 step 2): when GPU
    // model information is available, the upstream table is an additional AND
    // over the probe rows — a codec the table marks unsupported on the
    // detected architecture must not select hardware decode regardless of
    // what a probe row claims.
    if let Some(gpu) = nvidia_gpu_info(report) {
        if !nvidia_architecture_allows_decode(&gpu, source) {
            return FfmpegCapability::Software;
        }
    }

    let Some(decoder) = cuvid_decoder_for_codec(source.codec) else {
        return FfmpegCapability::Software;
    };

    if nvidia_decoder_detail_required(source.codec) && report.nvidia_decoders.is_empty() {
        return FfmpegCapability::Software;
    }

    if !report.nvidia_decoders.is_empty()
        && !nvidia_decoder_detail_matches(&report.nvidia_decoders, source)
    {
        return FfmpegCapability::Software;
    }

    if report.has_decoder(decoder.name()) {
        FfmpegCapability::Hardware
    } else {
        FfmpegCapability::Software
    }
}

fn nvidia_decoder_detail_required(codec: SourceVideoCodec) -> bool {
    codec == SourceVideoCodec::Av1
}

fn nvidia_decoder_detail_matches(
    details: &[crate::NvidiaDecoderCapability],
    source: &SourceVideoStream,
) -> bool {
    details.iter().any(|details| {
        details.supports(
            nvidia_decoder_codec(source.codec),
            bit_depth(source.pixel_format),
        )
    })
}

fn nvidia_encode(report: &FfmpegCapabilityReport, desired: &DesiredVideoState) -> FfmpegCapability {
    let Some(encoder_name) =
        hardware_encoder_name(HardwareAccelerationKind::Nvenc, desired.video_format)
    else {
        return FfmpegCapability::Software;
    };

    if !report.has_encoder(encoder_name) {
        return FfmpegCapability::Software;
    }

    let Some(details) = report
        .nvidia_encoders
        .iter()
        .find(|details| details.encoder_matches(encoder_name))
    else {
        return FfmpegCapability::Hardware;
    };

    if !nvidia_encoder_supports_bit_depth(details, desired.bit_depth) {
        return FfmpegCapability::Software;
    }

    let Some(profile) = nvidia_required_profile(desired) else {
        return FfmpegCapability::Hardware;
    };
    if details.profiles.is_empty() || details.has_profile(profile) {
        FfmpegCapability::Hardware
    } else {
        FfmpegCapability::Software
    }
}

fn nvidia_encoder_supports_bit_depth(
    details: &crate::NvidiaEncoderCapability,
    bit_depth: FFmpegProfileBitDepth,
) -> bool {
    match bit_depth {
        FFmpegProfileBitDepth::EightBit => {
            details.pixel_formats.is_empty()
                || ["yuv420p", "nv12", "yuv444p"]
                    .iter()
                    .any(|format| details.has_pixel_format(format))
        }
        FFmpegProfileBitDepth::TenBit => ["p010le", "p016le", "yuv420p10le", "yuv444p16le"]
            .iter()
            .any(|format| details.has_pixel_format(format)),
    }
}

fn nvidia_required_profile(desired: &DesiredVideoState) -> Option<&'static str> {
    match (desired.video_format, desired.bit_depth) {
        (FFmpegProfileVideoFormat::Hevc, FFmpegProfileBitDepth::EightBit)
        | (FFmpegProfileVideoFormat::Av1, _) => Some("main"),
        (FFmpegProfileVideoFormat::Hevc, FFmpegProfileBitDepth::TenBit) => Some("main10"),
        (FFmpegProfileVideoFormat::H264, FFmpegProfileBitDepth::TenBit) => Some("high444p"),
        (FFmpegProfileVideoFormat::H264, FFmpegProfileBitDepth::EightBit) => {
            let profile = desired
                .video_profile
                .as_deref()
                .unwrap_or("main")
                .to_ascii_lowercase();
            match profile.as_str() {
                "high" => Some("high"),
                "high10" | "high 10" => Some("high444p"),
                "baseline" => Some("baseline"),
                _ => Some("main"),
            }
        }
        _ => None,
    }
}

fn videotoolbox_decode(
    report: &FfmpegCapabilityReport,
    source: &SourceVideoStream,
) -> FfmpegCapability {
    if is_unsupported_hardware_decode(source) {
        return FfmpegCapability::Software;
    }

    let Some(fourcc) = videotoolbox_decoder_fourcc(source.codec) else {
        return FfmpegCapability::Software;
    };

    if report
        .videotoolbox_decoders
        .iter()
        .any(|decoder| decoder.eq_ignore_ascii_case(fourcc))
    {
        FfmpegCapability::Hardware
    } else {
        FfmpegCapability::Software
    }
}

fn videotoolbox_encode(
    report: &FfmpegCapabilityReport,
    desired: &DesiredVideoState,
) -> FfmpegCapability {
    if is_unsupported_h264_ten_bit(desired.video_format, desired.bit_depth) {
        return FfmpegCapability::Software;
    }

    let Some(encoder_name) =
        hardware_encoder_name(HardwareAccelerationKind::VideoToolbox, desired.video_format)
    else {
        return FfmpegCapability::Software;
    };

    if !report.has_encoder(encoder_name) {
        return FfmpegCapability::Software;
    }

    let expected = match desired.video_format {
        FFmpegProfileVideoFormat::H264 => "H.264 (HW)",
        FFmpegProfileVideoFormat::Hevc => "HEVC (HW)",
        _ => return FfmpegCapability::Software,
    };

    if report.videotoolbox_encoders.iter().any(|encoder| {
        encoder
            .to_ascii_lowercase()
            .contains(&expected.to_ascii_lowercase())
    }) {
        FfmpegCapability::Hardware
    } else {
        FfmpegCapability::Software
    }
}

fn v4l2m2m_decode(report: &FfmpegCapabilityReport, source: &SourceVideoStream) -> FfmpegCapability {
    if source.color_params.is_bt2020_ten {
        return FfmpegCapability::Software;
    }

    let Some(decoder) = v4l2m2m_decoder_for_codec(source.codec) else {
        return FfmpegCapability::Software;
    };

    if source.codec == SourceVideoCodec::H264 && bit_depth(source.pixel_format) == 10 {
        return FfmpegCapability::Software;
    }

    if report.has_decoder(decoder.name()) {
        FfmpegCapability::Hardware
    } else {
        FfmpegCapability::Software
    }
}

fn encoder_available(
    report: &FfmpegCapabilityReport,
    acceleration: HardwareAccelerationKind,
    desired: &DesiredVideoState,
) -> FfmpegCapability {
    let Some(name) = hardware_encoder_name(acceleration, desired.video_format) else {
        return FfmpegCapability::Software;
    };

    if report.has_encoder(name) {
        FfmpegCapability::Hardware
    } else {
        FfmpegCapability::Software
    }
}

/// Return the hardware encoder name for an acceleration/video-format pair.
#[must_use]
pub const fn hardware_encoder_name(
    acceleration: HardwareAccelerationKind,
    format: FFmpegProfileVideoFormat,
) -> Option<&'static str> {
    match (acceleration, format) {
        (HardwareAccelerationKind::Qsv, FFmpegProfileVideoFormat::H264) => Some("h264_qsv"),
        (HardwareAccelerationKind::Qsv, FFmpegProfileVideoFormat::Hevc) => Some("hevc_qsv"),
        (HardwareAccelerationKind::Qsv, FFmpegProfileVideoFormat::Mpeg2Video) => Some("mpeg2_qsv"),
        (HardwareAccelerationKind::Qsv, FFmpegProfileVideoFormat::Av1) => Some("av1_qsv"),
        (HardwareAccelerationKind::Nvenc, FFmpegProfileVideoFormat::H264) => Some("h264_nvenc"),
        (HardwareAccelerationKind::Nvenc, FFmpegProfileVideoFormat::Hevc) => Some("hevc_nvenc"),
        (HardwareAccelerationKind::Nvenc, FFmpegProfileVideoFormat::Av1) => Some("av1_nvenc"),
        (HardwareAccelerationKind::Amf, FFmpegProfileVideoFormat::H264) => Some("h264_amf"),
        (HardwareAccelerationKind::Amf, FFmpegProfileVideoFormat::Hevc) => Some("hevc_amf"),
        (HardwareAccelerationKind::Vaapi, FFmpegProfileVideoFormat::H264) => Some("h264_vaapi"),
        (HardwareAccelerationKind::Vaapi, FFmpegProfileVideoFormat::Hevc) => Some("hevc_vaapi"),
        (HardwareAccelerationKind::Vaapi, FFmpegProfileVideoFormat::Mpeg2Video) => {
            Some("mpeg2_vaapi")
        }
        (HardwareAccelerationKind::VideoToolbox, FFmpegProfileVideoFormat::H264) => {
            Some("h264_videotoolbox")
        }
        (HardwareAccelerationKind::VideoToolbox, FFmpegProfileVideoFormat::Hevc) => {
            Some("hevc_videotoolbox")
        }
        (HardwareAccelerationKind::V4l2m2m, FFmpegProfileVideoFormat::H264) => Some("h264_v4l2m2m"),
        (HardwareAccelerationKind::V4l2m2m, FFmpegProfileVideoFormat::Hevc) => Some("hevc_v4l2m2m"),
        (HardwareAccelerationKind::Rkmpp, FFmpegProfileVideoFormat::H264) => Some("h264_rkmpp"),
        (HardwareAccelerationKind::Rkmpp, FFmpegProfileVideoFormat::Hevc) => Some("hevc_rkmpp"),
        _ => None,
    }
}

fn cuvid_decoder_for_codec(codec: SourceVideoCodec) -> Option<FfmpegKnownDecoder> {
    match codec {
        SourceVideoCodec::Av1 => Some(FfmpegKnownDecoder::Av1Cuvid),
        SourceVideoCodec::H264 => Some(FfmpegKnownDecoder::H264Cuvid),
        SourceVideoCodec::Hevc => Some(FfmpegKnownDecoder::HevcCuvid),
        SourceVideoCodec::Mpeg2Video => Some(FfmpegKnownDecoder::Mpeg2Cuvid),
        SourceVideoCodec::Mpeg4 => Some(FfmpegKnownDecoder::Mpeg4Cuvid),
        SourceVideoCodec::Vc1 => Some(FfmpegKnownDecoder::Vc1Cuvid),
        SourceVideoCodec::Vp9 => Some(FfmpegKnownDecoder::Vp9Cuvid),
        _ => None,
    }
}

const fn nvidia_decoder_codec(codec: SourceVideoCodec) -> &'static str {
    match codec {
        SourceVideoCodec::Av1 => "av1",
        SourceVideoCodec::H264 => "h264",
        SourceVideoCodec::Hevc => "hevc",
        SourceVideoCodec::Mpeg2Video => "mpeg2video",
        SourceVideoCodec::Mpeg4 => "mpeg4",
        SourceVideoCodec::Vc1 => "vc1",
        SourceVideoCodec::Vp8 => "vp8",
        SourceVideoCodec::Vp9 => "vp9",
        SourceVideoCodec::Mpeg1Video
        | SourceVideoCodec::MsMpeg4V2
        | SourceVideoCodec::MsMpeg4V3
        | SourceVideoCodec::RawVideo
        | SourceVideoCodec::Unknown => "",
    }
}

const fn videotoolbox_decoder_fourcc(codec: SourceVideoCodec) -> Option<&'static str> {
    match codec {
        SourceVideoCodec::Av1 => Some("av01"),
        SourceVideoCodec::H264 => Some("avc1"),
        SourceVideoCodec::Hevc => Some("hvc1"),
        SourceVideoCodec::Mpeg2Video => Some("mp2v"),
        SourceVideoCodec::Mpeg4 => Some("mp4v"),
        SourceVideoCodec::Vp9 => Some("vp09"),
        _ => None,
    }
}

fn v4l2m2m_decoder_for_codec(codec: SourceVideoCodec) -> Option<FfmpegKnownDecoder> {
    match codec {
        SourceVideoCodec::H264 => Some(FfmpegKnownDecoder::H264V4l2m2m),
        SourceVideoCodec::Hevc => Some(FfmpegKnownDecoder::HevcV4l2m2m),
        SourceVideoCodec::Mpeg2Video => Some(FfmpegKnownDecoder::Mpeg2V4l2m2m),
        SourceVideoCodec::Mpeg4 => Some(FfmpegKnownDecoder::Mpeg4V4l2m2m),
        SourceVideoCodec::Vc1 => Some(FfmpegKnownDecoder::Vc1V4l2m2m),
        SourceVideoCodec::Vp8 => Some(FfmpegKnownDecoder::Vp8V4l2m2m),
        SourceVideoCodec::Vp9 => Some(FfmpegKnownDecoder::Vp9V4l2m2m),
        _ => None,
    }
}

const fn qsv_decoder_name(codec: SourceVideoCodec) -> Option<&'static str> {
    match codec {
        SourceVideoCodec::H264 => Some("h264_qsv"),
        SourceVideoCodec::Hevc => Some("hevc_qsv"),
        SourceVideoCodec::Mpeg2Video => Some("mpeg2_qsv"),
        SourceVideoCodec::Vc1 => Some("vc1_qsv"),
        SourceVideoCodec::Vp9 => Some("vp9_qsv"),
        SourceVideoCodec::Av1 => Some("av1_qsv"),
        _ => None,
    }
}

fn has_encoder_suffix(report: &FfmpegCapabilityReport, suffix: &str) -> bool {
    report.encoders.iter().any(|name| name.ends_with(suffix))
}

/// CUDA filters the NVENC *hardware-decode* path emits for scaling,
/// deinterlacing, and overlay composition on GPU frames. A full
/// `--enable-cuda-nvcc` `FFmpeg` build exposes all of them; encoder-only builds
/// (NVENC encoders + `cuda` hwaccel but no CUDA filters) do not, which would
/// make the on-GPU filter graph fail at runtime with `No such filter:
/// 'yadif_cuda'`.
///
/// These gate hardware *decode* only (see [`nvidia_decode`]): when the filters
/// are absent the source is decoded in software, and the transcode pipeline
/// then runs software filters and still NVENC-*encodes* the result (software
/// frames are accepted directly by the encoder). NVENC encode itself does not
/// require the CUDA filters. ErsatzTV degrades these per-filter to software
/// equivalents; ErsatzRS instead falls back the whole decode/filter path to
/// software when any is missing rather than porting the finer per-filter
/// fallback.
pub(crate) const NVENC_REQUIRED_CUDA_FILTERS: [&str; 3] =
    ["scale_cuda", "yadif_cuda", "overlay_cuda"];

pub(crate) fn nvenc_cuda_filters_available(report: &FfmpegCapabilityReport) -> bool {
    NVENC_REQUIRED_CUDA_FILTERS
        .iter()
        .all(|filter| report.has_filter(filter))
}

/// NVIDIA GPU model and SM architecture parsed from the capability report's
/// GPU list (native CUDA probe details or `FFmpeg`'s `h264_nvenc -gpu list`
/// fallback output).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NvidiaGpuInfo {
    /// SM compute architecture with the dot removed, such as `52` for SM 5.2
    /// (second-generation Maxwell) or `86` for SM 8.6 (Ampere). Mirrors
    /// ErsatzTV's `SM\s+(\d+\.\d+)` parse.
    pub architecture: i32,
    /// GPU model string, such as `NVIDIA GeForce GTX 960`, or `unknown` when
    /// the line carries no `< model >` segment.
    pub model: String,
}

fn nvidia_gpu_info(report: &FfmpegCapabilityReport) -> Option<NvidiaGpuInfo> {
    report
        .nvidia_gpu_list
        .as_deref()
        .and_then(parse_nvidia_gpu_info)
}

/// Parse the first GPU model/architecture row from NVIDIA GPU-list output.
///
/// Accepts both the native probe's `GPU #0 < model > has Compute SM 8.9`
/// lines and `FFmpeg`'s `-gpu list` variant. Like ErsatzTV, only the first
/// detected GPU drives the architecture gate.
#[must_use]
pub fn parse_nvidia_gpu_info(gpu_list: &str) -> Option<NvidiaGpuInfo> {
    for line in gpu_list.lines() {
        let Some(architecture) = parse_sm_architecture(line) else {
            continue;
        };
        let model = line
            .split_once('<')
            .and_then(|(_, rest)| rest.split_once('>'))
            .map(|(model, _)| model.trim().to_owned())
            .filter(|model| !model.is_empty())
            .unwrap_or_else(|| "unknown".to_owned());
        return Some(NvidiaGpuInfo {
            architecture,
            model,
        });
    }
    None
}

fn parse_sm_architecture(line: &str) -> Option<i32> {
    let (_, rest) = line.split_once("SM ")?;
    let version: String = rest
        .chars()
        .take_while(|ch| ch.is_ascii_digit() || *ch == '.')
        .collect();
    if !version.contains('.') {
        return None;
    }
    // ErsatzTV strips the dot and parses the remainder: SM 5.2 -> 52.
    version.replace('.', "").parse().ok()
}

/// Second-generation Maxwell GM206 models that can decode HEVC/VP9 even though
/// the rest of SM 5.2 cannot. From ErsatzTV's `NvidiaHardwareCapabilities`.
const MAXWELL_GM206_MODELS: [&str; 4] = ["GTX 750", "GTX 950", "GTX 960", "GTX 965M"];

/// ErsatzTV's conservative NVIDIA decode table, keyed on SM architecture and
/// GPU model. Ported from the pre-NvEncSharp `NvidiaHardwareCapabilities`
/// (the static table ADR 0014 requires as the parity-faithful floor).
fn nvidia_architecture_allows_decode(gpu: &NvidiaGpuInfo, source: &SourceVideoStream) -> bool {
    let architecture = gpu.architecture;
    let bit_depth = bit_depth(source.pixel_format);
    let is_hdr = source.color_params.is_hdr;
    let is_gm206 = MAXWELL_GM206_MODELS
        .iter()
        .any(|model| gpu.model.contains(model));
    match source.codec {
        // some second gen maxwell can decode hevc, otherwise pascal is required
        SourceVideoCodec::Hevc => (architecture == 52 && is_gm206) || architecture >= 60,
        // pascal is required to decode vp9 10-bit
        SourceVideoCodec::Vp9 if bit_depth == 10 => !is_hdr && architecture >= 60,
        // some second gen maxwell can decode vp9, otherwise pascal is required
        SourceVideoCodec::Vp9 => {
            !is_hdr && ((architecture == 52 && is_gm206) || architecture >= 60)
        }
        // no hardware decoding of 10-bit h264
        SourceVideoCodec::H264 => bit_depth < 10,
        SourceVideoCodec::Mpeg2Video | SourceVideoCodec::Vc1 => !is_hdr,
        // ampere is required for av1 decoding
        SourceVideoCodec::Av1 => architecture >= 86,
        // mpeg4: too many issues with odd content, so use software; everything
        // else is unknown to the upstream table and stays conservative.
        _ => false,
    }
}

fn is_unsupported_hardware_decode(source: &SourceVideoStream) -> bool {
    is_unsupported_h264_ten_bit(
        video_format_for_source_codec(source.codec),
        bit_depth_to_profile_depth(bit_depth(source.pixel_format)),
    ) || source.color_params.is_bt2020_ten
}

fn is_unsupported_ten_bit_hardware_encode(desired: &DesiredVideoState) -> bool {
    matches!(
        (desired.video_format, desired.bit_depth),
        (
            FFmpegProfileVideoFormat::H264 | FFmpegProfileVideoFormat::Hevc,
            FFmpegProfileBitDepth::TenBit
        )
    )
}

fn is_unsupported_h264_ten_bit(
    format: FFmpegProfileVideoFormat,
    bit_depth: FFmpegProfileBitDepth,
) -> bool {
    format == FFmpegProfileVideoFormat::H264 && bit_depth == FFmpegProfileBitDepth::TenBit
}

const fn bit_depth(pixel_format: Option<PixelFormat>) -> u8 {
    match pixel_format {
        Some(PixelFormat::Yuv420P10Le) => 10,
        Some(PixelFormat::Yuv420P | PixelFormat::Nv12 | PixelFormat::Vaapi | PixelFormat::Cuda)
        | None => 8,
    }
}

const fn bit_depth_to_profile_depth(bit_depth: u8) -> FFmpegProfileBitDepth {
    if bit_depth == 10 {
        FFmpegProfileBitDepth::TenBit
    } else {
        FFmpegProfileBitDepth::EightBit
    }
}

const fn video_format_for_source_codec(codec: SourceVideoCodec) -> FFmpegProfileVideoFormat {
    match codec {
        SourceVideoCodec::H264 => FFmpegProfileVideoFormat::H264,
        SourceVideoCodec::Hevc => FFmpegProfileVideoFormat::Hevc,
        SourceVideoCodec::Av1 => FFmpegProfileVideoFormat::Av1,
        SourceVideoCodec::Mpeg2Video => FFmpegProfileVideoFormat::Mpeg2Video,
        SourceVideoCodec::Mpeg1Video
        | SourceVideoCodec::Mpeg4
        | SourceVideoCodec::MsMpeg4V2
        | SourceVideoCodec::MsMpeg4V3
        | SourceVideoCodec::RawVideo
        | SourceVideoCodec::Vc1
        | SourceVideoCodec::Vp8
        | SourceVideoCodec::Vp9
        | SourceVideoCodec::Unknown => FFmpegProfileVideoFormat::None,
    }
}

/// Return a report containing the common software and hardware encoder/decoder names.
#[must_use]
pub fn capability_report_for_tests() -> FfmpegCapabilityReport {
    use std::collections::BTreeSet;

    FfmpegCapabilityReport {
        filters: BTreeSet::from([
            "scale_cuda".to_owned(),
            "yadif_cuda".to_owned(),
            "overlay_cuda".to_owned(),
        ]),
        encoders: BTreeSet::from([
            "aac".to_owned(),
            "ac3".to_owned(),
            "libsvtav1".to_owned(),
            "libx264".to_owned(),
            "libx265".to_owned(),
            "h264_qsv".to_owned(),
            "hevc_qsv".to_owned(),
            "mpeg2_qsv".to_owned(),
            "av1_qsv".to_owned(),
            "h264_nvenc".to_owned(),
            "hevc_nvenc".to_owned(),
            "av1_nvenc".to_owned(),
            "h264_amf".to_owned(),
            "hevc_amf".to_owned(),
            "h264_vaapi".to_owned(),
            "hevc_vaapi".to_owned(),
            "mpeg2_vaapi".to_owned(),
            "h264_videotoolbox".to_owned(),
            "hevc_videotoolbox".to_owned(),
            "h264_v4l2m2m".to_owned(),
            "hevc_v4l2m2m".to_owned(),
            "h264_rkmpp".to_owned(),
            "hevc_rkmpp".to_owned(),
        ]),
        decoders: FfmpegKnownDecoder::all_decoder_names()
            .iter()
            .map(|name| (*name).to_owned())
            .chain([
                "av1".to_owned(),
                "h264".to_owned(),
                "hevc".to_owned(),
                "mpeg1video".to_owned(),
                "mpeg2video".to_owned(),
                "mpeg4".to_owned(),
                "msmpeg4v2".to_owned(),
                "msmpeg4v3".to_owned(),
                "rawvideo".to_owned(),
                "vc1".to_owned(),
                "vp8".to_owned(),
                "vp9".to_owned(),
                "h264_qsv".to_owned(),
                "hevc_qsv".to_owned(),
                "mpeg2_qsv".to_owned(),
                "vc1_qsv".to_owned(),
                "vp9_qsv".to_owned(),
                "av1_qsv".to_owned(),
            ])
            .collect(),
        bitstream_filters: BTreeSet::from(["dovi_split".to_owned()]),
        hwaccels: BTreeSet::from([
            "cuda".to_owned(),
            "qsv".to_owned(),
            "rkmpp".to_owned(),
            "vaapi".to_owned(),
            "videotoolbox".to_owned(),
        ]),
        configuration_flags: BTreeSet::new(),
        vaapi_profiles: Vec::new(),
        qsv_profiles: Vec::new(),
        nvidia_gpu_list: None,
        nvidia_encoders: vec![
            crate::NvidiaEncoderCapability::new(
                "h264_nvenc",
                BTreeSet::from(["nv12".to_owned(), "p010le".to_owned()]),
                BTreeSet::from(["main".to_owned(), "high".to_owned(), "high444p".to_owned()]),
            ),
            crate::NvidiaEncoderCapability::new(
                "hevc_nvenc",
                BTreeSet::from(["nv12".to_owned(), "p010le".to_owned()]),
                BTreeSet::from(["main".to_owned(), "main10".to_owned()]),
            ),
            crate::NvidiaEncoderCapability::new(
                "av1_nvenc",
                BTreeSet::from(["nv12".to_owned(), "p010le".to_owned()]),
                BTreeSet::from(["main".to_owned()]),
            ),
        ],
        nvidia_decoders: nvidia_decoders_for_tests(),
        hardware_decode_denials: Vec::new(),
        hardware_graph_denials: Vec::new(),
        proven_hardware_graphs: Vec::new(),
        attempted_hardware_graphs: Vec::new(),
        videotoolbox_decoders: BTreeSet::from([
            "av01".to_owned(),
            "avc1".to_owned(),
            "hvc1".to_owned(),
            "mp2v".to_owned(),
            "mp4v".to_owned(),
            "vp09".to_owned(),
        ]),
        videotoolbox_encoders: BTreeSet::from(["H.264 (HW)".to_owned(), "HEVC (HW)".to_owned()]),
    }
}

fn nvidia_decoders_for_tests() -> Vec<crate::NvidiaDecoderCapability> {
    vec![
        crate::NvidiaDecoderCapability::new("h264", 8),
        crate::NvidiaDecoderCapability::new("hevc", 8),
        crate::NvidiaDecoderCapability::new("hevc", 10),
        crate::NvidiaDecoderCapability::new("mpeg2video", 8),
        crate::NvidiaDecoderCapability::new("mpeg4", 8),
        crate::NvidiaDecoderCapability::new("vc1", 8),
        crate::NvidiaDecoderCapability::new("vp9", 8),
        crate::NvidiaDecoderCapability::new("av1", 8),
        crate::NvidiaDecoderCapability::new("av1", 10),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_report_to_available_hardware_acceleration() {
        let report = capability_report_for_tests();
        let factory = HardwareCapabilitiesFactory::new(&report);

        assert!(factory.has_hardware_acceleration(HardwareAccelerationKind::Qsv));
        assert!(factory.has_hardware_acceleration(HardwareAccelerationKind::Nvenc));
        assert!(factory.has_hardware_acceleration(HardwareAccelerationKind::Amf));
        assert!(factory.has_hardware_acceleration(HardwareAccelerationKind::V4l2m2m));
    }

    #[test]
    fn nvenc_hardware_decode_requires_cuda_filters_but_encode_does_not() {
        // Encoder-only build: NVENC encoders + `cuda` hwaccel but no CUDA filters
        // (compiled without `--enable-cuda-nvcc`). NVENC *encode* must stay
        // available — the pipeline runs software filters and still NVENC-encodes
        // the software frames — but hardware *decode* must fall back to software,
        // because the on-GPU CUDA filter graph (yadif_cuda/scale_cuda/
        // overlay_cuda) is unrunnable without those filters.
        let source = SourceVideoStream::new(SourceVideoCodec::H264);

        // Baseline: a full build hardware-decodes this source.
        let full = capability_report_for_tests();
        assert_eq!(
            HardwareCapabilitiesFactory::new(&full)
                .for_acceleration(HardwareAccelerationKind::Nvenc)
                .can_decode(&source),
            FfmpegCapability::Hardware,
        );

        let mut report = capability_report_for_tests();
        report.filters.remove("scale_cuda");
        report.filters.remove("yadif_cuda");
        report.filters.remove("overlay_cuda");
        let factory = HardwareCapabilitiesFactory::new(&report);

        // NVENC is still an available acceleration and encode stays on hardware.
        assert!(factory.has_hardware_acceleration(HardwareAccelerationKind::Nvenc));
        let capabilities = factory.for_acceleration(HardwareAccelerationKind::Nvenc);
        assert_eq!(capabilities.acceleration(), HardwareAccelerationKind::Nvenc);
        assert_eq!(
            capabilities.can_encode(&DesiredVideoState::h264_1080p()),
            FfmpegCapability::Hardware,
        );
        // Hardware decode falls back to software so no CUDA filters are emitted.
        assert_eq!(capabilities.can_decode(&source), FfmpegCapability::Software);

        // A single missing CUDA filter is enough to force software decode.
        let mut partial = capability_report_for_tests();
        partial.filters.remove("yadif_cuda");
        assert_eq!(
            HardwareCapabilitiesFactory::new(&partial)
                .for_acceleration(HardwareAccelerationKind::Nvenc)
                .can_decode(&source),
            FfmpegCapability::Software,
        );
    }

    #[test]
    fn parses_nvidia_gpu_info_from_native_and_ffmpeg_gpu_lists() {
        let native = "GPU #0 < NVIDIA GeForce RTX 4070 > has Compute SM 8.9\n\n  Encoding:";
        assert_eq!(
            parse_nvidia_gpu_info(native),
            Some(NvidiaGpuInfo {
                architecture: 89,
                model: "NVIDIA GeForce RTX 4070".to_owned(),
            }),
        );

        let ffmpeg_list = "[h264_nvenc @ 0x55] [ GPU #0 - < GeForce GTX 960 > has Compute SM 5.2 ]";
        assert_eq!(
            parse_nvidia_gpu_info(ffmpeg_list),
            Some(NvidiaGpuInfo {
                architecture: 52,
                model: "GeForce GTX 960".to_owned(),
            }),
        );

        assert_eq!(parse_nvidia_gpu_info("no gpu lines here"), None);
    }

    #[test]
    fn nvidia_architecture_gate_is_additional_and_over_probe_rows() {
        // Probe rows report AV1 decode support, but the detected architecture
        // (Pascal, SM 6.1) predates Ampere: ErsatzTV's table wins and decode
        // must fall back to software.
        let mut report = capability_report_for_tests();
        report.nvidia_gpu_list =
            Some("GPU #0 < NVIDIA GeForce GTX 1060 > has Compute SM 6.1".to_owned());
        let capabilities = HardwareCapabilitiesFactory::new(&report)
            .for_acceleration(HardwareAccelerationKind::Nvenc);
        let av1 = SourceVideoStream::new(SourceVideoCodec::Av1);
        assert_eq!(capabilities.can_decode(&av1), FfmpegCapability::Software);

        // The same Pascal card decodes HEVC and H.264 in hardware.
        let hevc = SourceVideoStream::new(SourceVideoCodec::Hevc);
        assert_eq!(capabilities.can_decode(&hevc), FfmpegCapability::Hardware);
        let h264 = SourceVideoStream::new(SourceVideoCodec::H264);
        assert_eq!(capabilities.can_decode(&h264), FfmpegCapability::Hardware);

        // Mpeg4 is always software when the table is in play.
        let mpeg4 = SourceVideoStream::new(SourceVideoCodec::Mpeg4);
        assert_eq!(capabilities.can_decode(&mpeg4), FfmpegCapability::Software);
        drop(capabilities);

        // Ampere allows AV1.
        report.nvidia_gpu_list =
            Some("GPU #0 < NVIDIA GeForce RTX 3070 > has Compute SM 8.6".to_owned());
        let capabilities = HardwareCapabilitiesFactory::new(&report)
            .for_acceleration(HardwareAccelerationKind::Nvenc);
        assert_eq!(capabilities.can_decode(&av1), FfmpegCapability::Hardware);
        drop(capabilities);

        // Without GPU info the gate is skipped and probe rows decide alone.
        report.nvidia_gpu_list = None;
        let capabilities = HardwareCapabilitiesFactory::new(&report)
            .for_acceleration(HardwareAccelerationKind::Nvenc);
        assert_eq!(capabilities.can_decode(&av1), FfmpegCapability::Hardware);
    }

    #[test]
    fn nvidia_architecture_gate_allows_gm206_maxwell_hevc() {
        // Generic SM 5.2 Maxwell cannot decode HEVC...
        let mut report = capability_report_for_tests();
        report.nvidia_gpu_list =
            Some("GPU #0 < NVIDIA GeForce GTX 980 > has Compute SM 5.2".to_owned());
        let hevc = SourceVideoStream::new(SourceVideoCodec::Hevc);
        assert_eq!(
            HardwareCapabilitiesFactory::new(&report)
                .for_acceleration(HardwareAccelerationKind::Nvenc)
                .can_decode(&hevc),
            FfmpegCapability::Software,
        );

        // ...but the GM206 models can.
        report.nvidia_gpu_list =
            Some("GPU #0 < NVIDIA GeForce GTX 960 > has Compute SM 5.2".to_owned());
        assert_eq!(
            HardwareCapabilitiesFactory::new(&report)
                .for_acceleration(HardwareAccelerationKind::Nvenc)
                .can_decode(&hevc),
            FfmpegCapability::Hardware,
        );
    }

    #[test]
    fn missing_runtime_capability_returns_no_hardware_capabilities() {
        let report = FfmpegCapabilityReport::default();
        let factory = HardwareCapabilitiesFactory::new(&report);
        let capabilities = factory.for_acceleration(HardwareAccelerationKind::Qsv);
        let source = SourceVideoStream::new(SourceVideoCodec::H264);

        assert_eq!(capabilities.acceleration(), HardwareAccelerationKind::None);
        assert_eq!(capabilities.can_decode(&source), FfmpegCapability::Software);
    }

    #[test]
    fn v4l2m2m_decode_requires_matching_decoder() {
        let mut report = capability_report_for_tests();
        report.decoders.remove("vp9_v4l2m2m");
        let capabilities = HardwareCapabilitiesFactory::new(&report)
            .for_acceleration(HardwareAccelerationKind::V4l2m2m);
        let source = SourceVideoStream::new(SourceVideoCodec::Vp9);

        assert_eq!(capabilities.can_decode(&source), FfmpegCapability::Software);
    }

    #[test]
    fn hardware_encode_requires_matching_encoder() {
        let mut report = capability_report_for_tests();
        report.encoders.remove("h264_qsv");
        let capabilities = HardwareCapabilitiesFactory::new(&report)
            .for_acceleration(HardwareAccelerationKind::Qsv);
        let desired = DesiredVideoState::h264_1080p();

        assert_eq!(
            capabilities.can_encode(&desired),
            FfmpegCapability::Software
        );
    }

    #[test]
    fn vaapi_decode_uses_device_profile_entrypoints_when_present() {
        let mut report = capability_report_for_tests();
        report.vaapi_profiles = vec![VaapiProfileEntrypoint::new(
            "VAProfileH264Main",
            "VAEntrypointVLD",
        )];
        let capabilities = HardwareCapabilitiesFactory::new(&report)
            .for_acceleration(HardwareAccelerationKind::Vaapi);
        let mut high_profile = SourceVideoStream::new(SourceVideoCodec::H264);
        high_profile.profile = Some("high".to_owned());
        let mut main_profile = SourceVideoStream::new(SourceVideoCodec::H264);
        main_profile.profile = Some("main".to_owned());

        assert_eq!(
            capabilities.can_decode(&high_profile),
            FfmpegCapability::Software
        );
        assert_eq!(
            capabilities.can_decode(&main_profile),
            FfmpegCapability::Hardware
        );
    }

    #[test]
    fn qsv_encode_uses_device_profile_entrypoints_when_present() {
        let mut report = capability_report_for_tests();
        report.qsv_profiles = vec![VaapiProfileEntrypoint::new(
            "VAProfileHEVCMain",
            "VAEntrypointEncSlice",
        )];
        let capabilities = HardwareCapabilitiesFactory::new(&report)
            .for_acceleration(HardwareAccelerationKind::Qsv);
        let mut desired = DesiredVideoState::h264_1080p();
        desired.video_format = FFmpegProfileVideoFormat::Hevc;
        desired.bit_depth = FFmpegProfileBitDepth::TenBit;

        assert_eq!(
            capabilities.can_encode(&desired),
            FfmpegCapability::Software
        );

        desired.bit_depth = FFmpegProfileBitDepth::EightBit;
        assert_eq!(
            capabilities.can_encode(&desired),
            FfmpegCapability::Hardware
        );
    }

    #[test]
    fn qsv_decode_forces_ten_bit_h264_and_hevc_to_software() {
        let report = capability_report_for_tests();
        let capabilities = HardwareCapabilitiesFactory::new(&report)
            .for_acceleration(HardwareAccelerationKind::Qsv);
        let mut h264 = SourceVideoStream::new(SourceVideoCodec::H264);
        h264.pixel_format = Some(PixelFormat::Yuv420P10Le);
        let mut hevc = SourceVideoStream::new(SourceVideoCodec::Hevc);
        hevc.pixel_format = Some(PixelFormat::Yuv420P10Le);

        assert_eq!(capabilities.can_decode(&h264), FfmpegCapability::Software);
        assert_eq!(capabilities.can_decode(&hevc), FfmpegCapability::Software);
    }

    #[test]
    fn videotoolbox_uses_framework_decoder_and_encoder_lists() {
        let mut report = capability_report_for_tests();
        let capabilities = HardwareCapabilitiesFactory::new(&report)
            .for_acceleration(HardwareAccelerationKind::VideoToolbox);
        let source = SourceVideoStream::new(SourceVideoCodec::H264);
        let desired = DesiredVideoState::h264_1080p();

        assert_eq!(capabilities.can_decode(&source), FfmpegCapability::Hardware);
        assert_eq!(
            capabilities.can_encode(&desired),
            FfmpegCapability::Hardware
        );
        drop(capabilities);

        report.videotoolbox_decoders.clear();
        report.videotoolbox_encoders.clear();
        let capabilities = HardwareCapabilitiesFactory::new(&report)
            .for_acceleration(HardwareAccelerationKind::VideoToolbox);

        assert_eq!(capabilities.can_decode(&source), FfmpegCapability::Software);
        assert_eq!(
            capabilities.can_encode(&desired),
            FfmpegCapability::Software
        );
    }

    #[test]
    fn nvidia_encode_uses_encoder_detail_bit_depth_and_profiles_when_present() {
        let mut report = capability_report_for_tests();
        report.nvidia_encoders = vec![crate::NvidiaEncoderCapability::new(
            "hevc_nvenc",
            std::collections::BTreeSet::from(["nv12".to_owned()]),
            std::collections::BTreeSet::from(["main".to_owned(), "main10".to_owned()]),
        )];
        let capabilities = HardwareCapabilitiesFactory::new(&report)
            .for_acceleration(HardwareAccelerationKind::Nvenc);
        let mut desired = DesiredVideoState::h264_1080p();
        desired.video_format = FFmpegProfileVideoFormat::Hevc;
        desired.bit_depth = FFmpegProfileBitDepth::TenBit;
        desired.video_profile = None;

        assert_eq!(
            capabilities.can_encode(&desired),
            FfmpegCapability::Software
        );
        drop(capabilities);

        report.nvidia_encoders = vec![crate::NvidiaEncoderCapability::new(
            "hevc_nvenc",
            std::collections::BTreeSet::from(["nv12".to_owned(), "p010le".to_owned()]),
            std::collections::BTreeSet::from(["main".to_owned(), "main10".to_owned()]),
        )];
        let capabilities = HardwareCapabilitiesFactory::new(&report)
            .for_acceleration(HardwareAccelerationKind::Nvenc);

        assert_eq!(
            capabilities.can_encode(&desired),
            FfmpegCapability::Hardware
        );
    }

    #[test]
    fn nvidia_decode_uses_decoder_detail_bit_depth_when_present() {
        let mut report = capability_report_for_tests();
        report.nvidia_decoders = vec![crate::NvidiaDecoderCapability::new("hevc", 8)];
        let capabilities = HardwareCapabilitiesFactory::new(&report)
            .for_acceleration(HardwareAccelerationKind::Nvenc);
        let mut source = SourceVideoStream::new(SourceVideoCodec::Hevc);
        source.pixel_format = Some(PixelFormat::Yuv420P10Le);

        assert_eq!(capabilities.can_decode(&source), FfmpegCapability::Software);
        drop(capabilities);

        report
            .nvidia_decoders
            .push(crate::NvidiaDecoderCapability::new("hevc", 10));
        let capabilities = HardwareCapabilitiesFactory::new(&report)
            .for_acceleration(HardwareAccelerationKind::Nvenc);

        assert_eq!(capabilities.can_decode(&source), FfmpegCapability::Hardware);
    }

    #[test]
    fn nvidia_av1_decode_requires_decoder_detail() {
        let mut report = capability_report_for_tests();
        report.nvidia_decoders.clear();
        let capabilities = HardwareCapabilitiesFactory::new(&report)
            .for_acceleration(HardwareAccelerationKind::Nvenc);
        let source = SourceVideoStream::new(SourceVideoCodec::Av1);
        let desired = DesiredVideoState::h264_1080p();

        assert_eq!(capabilities.can_decode(&source), FfmpegCapability::Software);
        assert_eq!(
            capabilities.can_encode(&desired),
            FfmpegCapability::Hardware
        );
    }

    #[test]
    fn nvidia_runtime_denial_overrides_positive_decoder_detail() {
        let mut report = capability_report_for_tests();
        report.deny_hardware_decode(crate::HardwareDecodeDenial::new(
            HardwareAccelerationKind::Nvenc,
            "av1",
            8,
        ));
        let capabilities = HardwareCapabilitiesFactory::new(&report)
            .for_acceleration(HardwareAccelerationKind::Nvenc);
        let source = SourceVideoStream::new(SourceVideoCodec::Av1);
        let desired = DesiredVideoState::h264_1080p();

        assert_eq!(capabilities.can_decode(&source), FfmpegCapability::Software);
        assert_eq!(
            capabilities.can_encode(&desired),
            FfmpegCapability::Hardware
        );
    }
}
