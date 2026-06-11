use ersatzrs_domain::HardwareAccelerationKind;

use crate::FfmpegCapabilityReport;

const ALL_DECODER_NAMES: &[&str] = &[
    "av1_cuvid",
    "h264_cuvid",
    "hevc_cuvid",
    "mpeg2_cuvid",
    "mpeg4_cuvid",
    "vc1_cuvid",
    "vp9_cuvid",
    "libdav1d",
    "libaom-av1",
    "h264_v4l2m2m",
    "hevc_v4l2m2m",
    "mpeg2_v4l2m2m",
    "mpeg4_v4l2m2m",
    "vc1_v4l2m2m",
    "vp8_v4l2m2m",
    "vp9_v4l2m2m",
];

const V4L2M2M_DECODER_NAMES: &[&str] = &[
    "h264_v4l2m2m",
    "hevc_v4l2m2m",
    "mpeg2_v4l2m2m",
    "mpeg4_v4l2m2m",
    "vc1_v4l2m2m",
    "vp8_v4l2m2m",
    "vp9_v4l2m2m",
];

/// Source video codecs understood by the `FFmpeg` pipeline decoder dispatcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceVideoCodec {
    H264,
    Hevc,
    Av1,
    Mpeg1Video,
    Mpeg2Video,
    Mpeg4,
    MsMpeg4V2,
    MsMpeg4V3,
    RawVideo,
    Vc1,
    Vp8,
    Vp9,
    Unknown,
}

impl SourceVideoCodec {
    /// Parse an FFmpeg/ffprobe codec name into the dispatcher codec enum.
    #[must_use]
    pub fn from_ffmpeg_name(name: &str) -> Self {
        match name.trim().to_ascii_lowercase().as_str() {
            "h264" => Self::H264,
            "hevc" | "h265" => Self::Hevc,
            "av1" => Self::Av1,
            "mpeg1video" => Self::Mpeg1Video,
            "mpeg2video" => Self::Mpeg2Video,
            "mpeg4" => Self::Mpeg4,
            "msmpeg4v2" => Self::MsMpeg4V2,
            "msmpeg4v3" => Self::MsMpeg4V3,
            "rawvideo" => Self::RawVideo,
            "vc1" => Self::Vc1,
            "vp8" => Self::Vp8,
            "vp9" => Self::Vp9,
            _ => Self::Unknown,
        }
    }
}

/// Known decoder names mirrored from `ErsatzTV`'s `FFmpegKnownDecoder`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FfmpegKnownDecoder {
    Av1Cuvid,
    H264Cuvid,
    HevcCuvid,
    Mpeg2Cuvid,
    Mpeg4Cuvid,
    Vc1Cuvid,
    Vp9Cuvid,
    Libdav1d,
    LibaomAv1,
    H264V4l2m2m,
    HevcV4l2m2m,
    Mpeg2V4l2m2m,
    Mpeg4V4l2m2m,
    Vc1V4l2m2m,
    Vp8V4l2m2m,
    Vp9V4l2m2m,
}

impl FfmpegKnownDecoder {
    /// Return the `FFmpeg` decoder name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Av1Cuvid => "av1_cuvid",
            Self::H264Cuvid => "h264_cuvid",
            Self::HevcCuvid => "hevc_cuvid",
            Self::Mpeg2Cuvid => "mpeg2_cuvid",
            Self::Mpeg4Cuvid => "mpeg4_cuvid",
            Self::Vc1Cuvid => "vc1_cuvid",
            Self::Vp9Cuvid => "vp9_cuvid",
            Self::Libdav1d => "libdav1d",
            Self::LibaomAv1 => "libaom-av1",
            Self::H264V4l2m2m => "h264_v4l2m2m",
            Self::HevcV4l2m2m => "hevc_v4l2m2m",
            Self::Mpeg2V4l2m2m => "mpeg2_v4l2m2m",
            Self::Mpeg4V4l2m2m => "mpeg4_v4l2m2m",
            Self::Vc1V4l2m2m => "vc1_v4l2m2m",
            Self::Vp8V4l2m2m => "vp8_v4l2m2m",
            Self::Vp9V4l2m2m => "vp9_v4l2m2m",
        }
    }

    /// Return all decoder names `ErsatzTV` treats as known optional decoders.
    #[must_use]
    pub fn all_decoder_names() -> &'static [&'static str] {
        ALL_DECODER_NAMES
    }

    /// Return the subset of known decoders that belongs to V4L2 M2M.
    #[must_use]
    pub fn v4l2m2m_decoder_names() -> &'static [&'static str] {
        V4L2M2M_DECODER_NAMES
    }
}

/// `FFmpeg` input-option decoder selected for a source video stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoDecoder {
    Explicit(&'static str),
    ExplicitSoftware(&'static str),
    ImplicitCuda,
    ImplicitVaapi,
    ImplicitVideoToolbox,
    ImplicitRkmpp,
}

impl VideoDecoder {
    /// Build `FFmpeg` input options for the selected decoder.
    #[must_use]
    pub fn input_options(self) -> Vec<String> {
        match self {
            Self::Explicit(name) => vec!["-c:v".to_owned(), name.to_owned()],
            Self::ExplicitSoftware(name) => vec![
                "-hwaccel".to_owned(),
                "none".to_owned(),
                "-c:v".to_owned(),
                name.to_owned(),
            ],
            Self::ImplicitCuda
            | Self::ImplicitVaapi
            | Self::ImplicitVideoToolbox
            | Self::ImplicitRkmpp => Vec::new(),
        }
    }
}

/// Select the ErsatzTV-compatible video decoder for a source codec and hardware mode.
#[must_use]
pub fn select_video_decoder(
    acceleration: HardwareAccelerationKind,
    codec: SourceVideoCodec,
) -> Option<VideoDecoder> {
    match acceleration {
        HardwareAccelerationKind::Qsv => qsv_decoder(codec),
        HardwareAccelerationKind::Nvenc => Some(VideoDecoder::ImplicitCuda),
        HardwareAccelerationKind::Vaapi => Some(VideoDecoder::ImplicitVaapi),
        HardwareAccelerationKind::VideoToolbox => Some(VideoDecoder::ImplicitVideoToolbox),
        HardwareAccelerationKind::V4l2m2m => v4l2m2m_decoder(codec),
        HardwareAccelerationKind::Rkmpp => Some(VideoDecoder::ImplicitRkmpp),
        HardwareAccelerationKind::None | HardwareAccelerationKind::Amf => {
            software_decoder(codec, None)
        }
    }
}

/// Select the ErsatzTV-compatible software video decoder.
#[must_use]
pub fn select_software_video_decoder(
    codec: SourceVideoCodec,
    report: Option<&FfmpegCapabilityReport>,
) -> Option<VideoDecoder> {
    software_decoder(codec, report)
}

/// Select a software decoder while also disabling `FFmpeg`'s automatic hardware
/// acceleration for the input.
#[must_use]
pub fn select_forced_software_video_decoder(
    codec: SourceVideoCodec,
    report: Option<&FfmpegCapabilityReport>,
) -> Option<VideoDecoder> {
    software_decoder(codec, report).map(|decoder| match decoder {
        VideoDecoder::Explicit(name) => VideoDecoder::ExplicitSoftware(name),
        other => other,
    })
}

fn qsv_decoder(codec: SourceVideoCodec) -> Option<VideoDecoder> {
    match codec {
        SourceVideoCodec::H264 => Some(VideoDecoder::Explicit("h264_qsv")),
        SourceVideoCodec::Hevc => Some(VideoDecoder::Explicit("hevc_qsv")),
        SourceVideoCodec::Mpeg2Video => Some(VideoDecoder::Explicit("mpeg2_qsv")),
        SourceVideoCodec::Vc1 => Some(VideoDecoder::Explicit("vc1_qsv")),
        SourceVideoCodec::Vp9 => Some(VideoDecoder::Explicit("vp9_qsv")),
        SourceVideoCodec::Av1 => Some(VideoDecoder::Explicit("av1_qsv")),
        _ => software_decoder(codec, None),
    }
}

fn v4l2m2m_decoder(codec: SourceVideoCodec) -> Option<VideoDecoder> {
    match codec {
        SourceVideoCodec::H264 => Some(VideoDecoder::Explicit(
            FfmpegKnownDecoder::H264V4l2m2m.name(),
        )),
        SourceVideoCodec::Hevc => Some(VideoDecoder::Explicit(
            FfmpegKnownDecoder::HevcV4l2m2m.name(),
        )),
        SourceVideoCodec::Mpeg2Video => Some(VideoDecoder::Explicit(
            FfmpegKnownDecoder::Mpeg2V4l2m2m.name(),
        )),
        SourceVideoCodec::Mpeg4 => Some(VideoDecoder::Explicit(
            FfmpegKnownDecoder::Mpeg4V4l2m2m.name(),
        )),
        SourceVideoCodec::Vc1 => Some(VideoDecoder::Explicit(
            FfmpegKnownDecoder::Vc1V4l2m2m.name(),
        )),
        SourceVideoCodec::Vp8 => Some(VideoDecoder::Explicit(
            FfmpegKnownDecoder::Vp8V4l2m2m.name(),
        )),
        SourceVideoCodec::Vp9 => Some(VideoDecoder::Explicit(
            FfmpegKnownDecoder::Vp9V4l2m2m.name(),
        )),
        _ => software_decoder(codec, None),
    }
}

fn software_decoder(
    codec: SourceVideoCodec,
    report: Option<&FfmpegCapabilityReport>,
) -> Option<VideoDecoder> {
    match codec {
        SourceVideoCodec::H264 => Some(VideoDecoder::Explicit("h264")),
        SourceVideoCodec::Hevc => Some(VideoDecoder::Explicit("hevc")),
        SourceVideoCodec::Av1 => Some(VideoDecoder::Explicit(av1_software_decoder(report))),
        SourceVideoCodec::Mpeg1Video => Some(VideoDecoder::Explicit("mpeg1video")),
        SourceVideoCodec::Mpeg2Video => Some(VideoDecoder::Explicit("mpeg2video")),
        SourceVideoCodec::Mpeg4 => Some(VideoDecoder::Explicit("mpeg4")),
        SourceVideoCodec::MsMpeg4V2 => Some(VideoDecoder::Explicit("msmpeg4v2")),
        SourceVideoCodec::MsMpeg4V3 => Some(VideoDecoder::Explicit("msmpeg4v3")),
        SourceVideoCodec::RawVideo => Some(VideoDecoder::Explicit("rawvideo")),
        SourceVideoCodec::Vc1 => Some(VideoDecoder::Explicit("vc1")),
        SourceVideoCodec::Vp8 => Some(VideoDecoder::Explicit("vp8")),
        SourceVideoCodec::Vp9 => Some(VideoDecoder::Explicit("vp9")),
        SourceVideoCodec::Unknown => None,
    }
}

fn av1_software_decoder(report: Option<&FfmpegCapabilityReport>) -> &'static str {
    if report.is_some_and(|report| report.has_decoder("libdav1d")) {
        "libdav1d"
    } else if report.is_some_and(|report| report.has_decoder("libaom-av1")) {
        "libaom-av1"
    } else {
        "av1"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ffmpeg_codec_names() {
        assert_eq!(
            SourceVideoCodec::from_ffmpeg_name("h264"),
            SourceVideoCodec::H264
        );
        assert_eq!(
            SourceVideoCodec::from_ffmpeg_name("H265"),
            SourceVideoCodec::Hevc
        );
        assert_eq!(
            SourceVideoCodec::from_ffmpeg_name("vp9"),
            SourceVideoCodec::Vp9
        );
        assert_eq!(
            SourceVideoCodec::from_ffmpeg_name("not-real"),
            SourceVideoCodec::Unknown
        );
    }

    #[test]
    fn exposes_known_decoder_tables() {
        assert!(FfmpegKnownDecoder::all_decoder_names().contains(&"h264_cuvid"));
        assert!(FfmpegKnownDecoder::all_decoder_names().contains(&"vp9_v4l2m2m"));
        assert_eq!(
            FfmpegKnownDecoder::v4l2m2m_decoder_names(),
            &[
                "h264_v4l2m2m",
                "hevc_v4l2m2m",
                "mpeg2_v4l2m2m",
                "mpeg4_v4l2m2m",
                "vc1_v4l2m2m",
                "vp8_v4l2m2m",
                "vp9_v4l2m2m",
            ]
        );
    }

    #[test]
    fn selects_hardware_decoders_when_available_for_acceleration() {
        assert_eq!(
            select_video_decoder(HardwareAccelerationKind::Qsv, SourceVideoCodec::Hevc),
            Some(VideoDecoder::Explicit("hevc_qsv"))
        );
        assert_eq!(
            select_video_decoder(HardwareAccelerationKind::V4l2m2m, SourceVideoCodec::Vp9),
            Some(VideoDecoder::Explicit("vp9_v4l2m2m"))
        );
        assert_eq!(
            select_video_decoder(HardwareAccelerationKind::Nvenc, SourceVideoCodec::H264),
            Some(VideoDecoder::ImplicitCuda)
        );
    }

    #[test]
    fn falls_back_to_software_decoder_for_unsupported_hardware_pair() {
        assert_eq!(
            select_video_decoder(HardwareAccelerationKind::Qsv, SourceVideoCodec::Mpeg4),
            Some(VideoDecoder::Explicit("mpeg4"))
        );
        assert_eq!(
            select_video_decoder(HardwareAccelerationKind::None, SourceVideoCodec::Unknown),
            None
        );
    }
}
