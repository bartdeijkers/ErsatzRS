use ersatzrs_domain::HardwareAccelerationKind;

use crate::filter::simple::{FrameDataLocation, FrameState, PixelFormat};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TonemapAlgorithm {
    #[default]
    Hable,
    Mobius,
    Reinhard,
    Linear,
    Gamma,
    Clip,
    Bt2390,
}

impl TonemapAlgorithm {
    #[must_use]
    pub const fn ffmpeg_name(self) -> &'static str {
        match self {
            Self::Hable => "hable",
            Self::Mobius => "mobius",
            Self::Reinhard => "reinhard",
            Self::Linear => "linear",
            Self::Gamma => "gamma",
            Self::Clip => "clip",
            Self::Bt2390 => "bt2390",
        }
    }
}

#[must_use]
pub fn tonemap_filter(
    current: &FrameState,
    desired_pixel_format: PixelFormat,
    algorithm: TonemapAlgorithm,
) -> String {
    let tonemap = format!(
        "zscale=t=linear,tonemap={},zscale=p=bt709:t=bt709:m=bt709:r=tv,format={},setparams=range=tv:colorspace=bt709:color_trc=bt709:color_primaries=bt709",
        algorithm.ffmpeg_name(),
        desired_pixel_format.ffmpeg_name()
    );

    if current.frame_data_location == FrameDataLocation::Hardware {
        let download_format = software_format(current.pixel_format);
        return format!("hwdownload,format={download_format},{tonemap}");
    }

    tonemap
}

#[must_use]
pub fn tonemap_cuda_filter(
    desired_pixel_format: PixelFormat,
    algorithm: TonemapAlgorithm,
) -> String {
    let (vulkan_output_format, cuda_format) = match desired_pixel_format {
        PixelFormat::Yuv420P10Le => ("p016", ",scale_cuda=format=p010"),
        _ => (desired_pixel_format.ffmpeg_name(), ""),
    };

    format!(
        "libplacebo=tonemapping={}:colorspace=bt709:color_primaries=bt709:color_trc=bt709:format={vulkan_output_format},hwupload_cuda{cuda_format}",
        algorithm.ffmpeg_name()
    )
}

#[must_use]
pub fn tonemap_qsv_filter() -> &'static str {
    "vpp_qsv=tonemap=1"
}

#[must_use]
pub fn tonemap_vaapi_filter(algorithm: TonemapAlgorithm) -> String {
    format!(
        "hwupload=derive_device=vaapi,hwmap=derive_device=opencl,tonemap_opencl=tonemap={},hwmap=derive_device=vaapi:reverse=1,scale_vaapi=format=p010le",
        algorithm.ffmpeg_name()
    )
}

#[must_use]
pub fn tonemap_filter_for(
    acceleration: HardwareAccelerationKind,
    current: &FrameState,
    desired_pixel_format: PixelFormat,
    algorithm: TonemapAlgorithm,
) -> String {
    match acceleration {
        HardwareAccelerationKind::Nvenc => tonemap_cuda_filter(desired_pixel_format, algorithm),
        HardwareAccelerationKind::Qsv => tonemap_qsv_filter().to_owned(),
        HardwareAccelerationKind::Vaapi => tonemap_vaapi_filter(algorithm),
        HardwareAccelerationKind::None
        | HardwareAccelerationKind::Amf
        | HardwareAccelerationKind::VideoToolbox
        | HardwareAccelerationKind::V4l2m2m
        | HardwareAccelerationKind::Rkmpp => {
            tonemap_filter(current, desired_pixel_format, algorithm)
        }
    }
}

const fn software_format(pixel_format: PixelFormat) -> &'static str {
    match pixel_format {
        PixelFormat::Cuda | PixelFormat::Yuv420P10Le => "p010le",
        PixelFormat::Vaapi | PixelFormat::Nv12 => "nv12",
        PixelFormat::Yuv420P => "yuv420p",
    }
}
