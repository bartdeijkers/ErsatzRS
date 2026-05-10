use ersatzrs_domain::{FFmpegProfileBitDepth, FFmpegProfileVideoFormat, HardwareAccelerationKind};

use crate::{
    DesiredVideoState, FfmpegCapabilityReport, FfmpegKnownDecoder, PixelFormat, SourceVideoCodec,
    SourceVideoStream,
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
                HardwareRules::Default,
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
    Default,
    Qsv,
    Nvidia,
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
            HardwareRules::Default | HardwareRules::Vaapi => default_decode(source),
            HardwareRules::Qsv => qsv_decode(self.report, source),
            HardwareRules::Nvidia => nvidia_decode(self.report, source),
            HardwareRules::Amf => FfmpegCapability::Software,
            HardwareRules::V4l2m2m => v4l2m2m_decode(self.report, source),
            HardwareRules::Rkmpp => FfmpegCapability::Hardware,
        }
    }

    fn can_encode(&self, desired: &DesiredVideoState) -> FfmpegCapability {
        match self.rules {
            HardwareRules::Default
            | HardwareRules::Qsv
            | HardwareRules::Vaapi
            | HardwareRules::V4l2m2m => default_encode(self.report, self.acceleration, desired),
            HardwareRules::Nvidia => encoder_available(self.report, self.acceleration, desired),
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

fn qsv_decode(report: &FfmpegCapabilityReport, source: &SourceVideoStream) -> FfmpegCapability {
    if is_unsupported_hardware_decode(source) {
        return FfmpegCapability::Software;
    }

    let Some(name) = qsv_decoder_name(source.codec) else {
        return FfmpegCapability::Software;
    };

    if report.has_decoder(name) {
        FfmpegCapability::Hardware
    } else {
        FfmpegCapability::Software
    }
}

fn nvidia_decode(report: &FfmpegCapabilityReport, source: &SourceVideoStream) -> FfmpegCapability {
    if is_unsupported_hardware_decode(source) {
        return FfmpegCapability::Software;
    }

    let Some(decoder) = cuvid_decoder_for_codec(source.codec) else {
        return FfmpegCapability::Software;
    };

    if report.has_decoder(decoder.name()) {
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
        filters: BTreeSet::new(),
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
        hwaccels: BTreeSet::from([
            "cuda".to_owned(),
            "qsv".to_owned(),
            "rkmpp".to_owned(),
            "vaapi".to_owned(),
            "videotoolbox".to_owned(),
        ]),
        configuration_flags: BTreeSet::new(),
    }
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
}
