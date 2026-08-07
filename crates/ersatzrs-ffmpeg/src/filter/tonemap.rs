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

/// Drop every HDR and Dolby Vision static-metadata side data block.
///
/// ADR 0064: "For SDR output, strip HDR10 and Dolby Vision metadata after
/// tonemapping. A pipeline must never copy source HDR metadata onto transformed
/// SDR frames." Without this the metadata survives the tone-map and is written
/// onto BT.709 frames, which measurably happened for every HDR source and every
/// encoder tested (libx264, libx265, `h264_nvenc`) — a display would be told to
/// expect a 1000-nit BT.2020 grade for pictures that are neither.
///
/// The `sidedata` filter is used rather than encoder-specific options such as
/// x264's `mastering-display=none`, because the metadata has to be gone for
/// every encoder, including hardware ones with no equivalent option.
#[must_use]
pub fn strip_hdr_metadata_filter() -> &'static str {
    "sidedata=mode=delete:type=MASTERING_DISPLAY_METADATA,sidedata=mode=delete:type=CONTENT_LIGHT_LEVEL,sidedata=mode=delete:type=DOVI_RPU_BUFFER,sidedata=mode=delete:type=DOVI_METADATA"
}

#[must_use]
pub fn tonemap_filter(
    current: &FrameState,
    desired_pixel_format: PixelFormat,
    algorithm: TonemapAlgorithm,
) -> String {
    let tonemap = format!(
        "zscale=t=linear,tonemap={},zscale=p=bt709:t=bt709:m=bt709:r=tv,format={},setparams=range=tv:colorspace=bt709:color_trc=bt709:color_primaries=bt709,{}",
        algorithm.ffmpeg_name(),
        desired_pixel_format.ffmpeg_name(),
        strip_hdr_metadata_filter()
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
        "libplacebo=tonemapping={}:colorspace=bt709:color_primaries=bt709:color_trc=bt709:format={vulkan_output_format},{strip},hwupload_cuda{cuda_format}",
        algorithm.ffmpeg_name(),
        strip = strip_hdr_metadata_filter()
    )
}

/// NVENC HDR tone-map that does not touch Vulkan at all.
///
/// [`tonemap_cuda_filter`] runs libplacebo, which *always* needs a Vulkan
/// device even when the `vulkan` hwaccel is not requested — measured against
/// `FFmpeg` 9: the non-Vulkan NVENC HDR chain still failed with
/// `Found no suitable device, giving up. / Failed initializing vulkan device`.
/// So libplacebo is not a fallback for a broken Vulkan stack; this is.
///
/// Tone-maps in software, then uploads to CUDA so the NVENC encode is
/// preserved, which is what ADR 0014 means by "retry through the next proven
/// graph while preserving hardware encode where possible".
#[must_use]
pub fn tonemap_cuda_software_fallback_filter(
    current: &FrameState,
    desired_pixel_format: PixelFormat,
    algorithm: TonemapAlgorithm,
) -> String {
    // `hwdownload` can only produce the *surface's* software format, not an
    // arbitrary one. A tone-map only ever runs on an HDR source, and HDR
    // sources decode to 10-bit CUDA surfaces, so the download format is p010le
    // regardless of the profile's desired output bit depth. Measured against
    // FFmpeg 9: `hwdownload,format=nv12` on a 10-bit HDR CUDA decode fails with
    // `Invalid output format nv12 for hwframe download`, while `format=p010le`
    // succeeds. The generic `tonemap_filter` prefix derives its format from the
    // tracked pixel format, which the CUDA hardware configuration sets from the
    // desired bit depth rather than from the source.
    let tonemap = if current.frame_data_location == FrameDataLocation::Hardware {
        let software_only = FrameState {
            frame_data_location: FrameDataLocation::Software,
            ..current.clone()
        };
        format!(
            "hwdownload,format=p010le,{}",
            tonemap_filter(&software_only, desired_pixel_format, algorithm)
        )
    } else {
        tonemap_filter(current, desired_pixel_format, algorithm)
    };

    let upload_format = match desired_pixel_format {
        PixelFormat::Yuv420P10Le => "p010le",
        _ => "nv12",
    };
    format!("{tonemap},format={upload_format},hwupload_cuda")
}

#[must_use]
pub fn tonemap_qsv_filter() -> String {
    format!("{},vpp_qsv=tonemap=1", strip_hdr_metadata_filter())
}

#[must_use]
pub fn tonemap_vaapi_filter(algorithm: TonemapAlgorithm) -> String {
    format!(
        "{strip},hwupload=derive_device=vaapi,hwmap=derive_device=opencl,tonemap_opencl=tonemap={},hwmap=derive_device=vaapi:reverse=1,scale_vaapi=format=p010le",
        algorithm.ffmpeg_name(),
        strip = strip_hdr_metadata_filter()
    )
}

/// Select the tone-map filter for an acceleration kind.
///
/// `vulkan_libplacebo_proven` gates the NVIDIA libplacebo path. It must be
/// false whenever the operation-scoped functional probe denied the
/// Vulkan/libplacebo graph, because libplacebo cannot run without a working
/// Vulkan device (ADR 0014, milestone `p8b-01`).
#[must_use]
pub fn tonemap_filter_for(
    acceleration: HardwareAccelerationKind,
    current: &FrameState,
    desired_pixel_format: PixelFormat,
    algorithm: TonemapAlgorithm,
    vulkan_libplacebo_proven: bool,
) -> String {
    match acceleration {
        HardwareAccelerationKind::Nvenc if vulkan_libplacebo_proven => {
            tonemap_cuda_filter(desired_pixel_format, algorithm)
        }
        HardwareAccelerationKind::Nvenc => {
            tonemap_cuda_software_fallback_filter(current, desired_pixel_format, algorithm)
        }
        HardwareAccelerationKind::Qsv => tonemap_qsv_filter(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter::simple::FrameSize;

    fn cuda_hdr_state() -> FrameState {
        let mut state = FrameState::new(FrameSize::new(1920, 1080));
        state.frame_data_location = FrameDataLocation::Hardware;
        // The CUDA hardware configuration tracks the *desired* output format,
        // which is nv12 for an 8-bit profile even when the HDR source surface
        // is 10-bit. This is exactly the state that produced the runtime
        // failure the fallback has to avoid.
        state.pixel_format = PixelFormat::Nv12;
        state
    }

    #[test]
    fn the_cuda_fallback_downloads_hdr_surfaces_as_p010le() {
        let filter = tonemap_cuda_software_fallback_filter(
            &cuda_hdr_state(),
            PixelFormat::Yuv420P,
            TonemapAlgorithm::Hable,
        );

        assert!(
            filter.starts_with("hwdownload,format=p010le,"),
            "nv12 download fails on a 10-bit HDR CUDA surface, got: {filter}"
        );
        assert!(!filter.contains("format=nv12,zscale"), "{filter}");
    }

    #[test]
    fn the_cuda_fallback_uses_no_vulkan_and_re_uploads_for_nvenc() {
        let filter = tonemap_cuda_software_fallback_filter(
            &cuda_hdr_state(),
            PixelFormat::Yuv420P,
            TonemapAlgorithm::Hable,
        );

        assert!(!filter.contains("libplacebo"), "{filter}");
        assert!(!filter.contains("hwupload,"), "{filter}");
        assert!(filter.ends_with("format=nv12,hwupload_cuda"), "{filter}");
        assert!(filter.contains("tonemap=hable"), "{filter}");
    }

    #[test]
    fn software_frames_skip_the_download_entirely() {
        let filter = tonemap_cuda_software_fallback_filter(
            &FrameState::new(FrameSize::new(1920, 1080)),
            PixelFormat::Yuv420P,
            TonemapAlgorithm::Hable,
        );

        assert!(!filter.contains("hwdownload"), "{filter}");
        assert!(filter.ends_with("format=nv12,hwupload_cuda"), "{filter}");
    }

    #[test]
    fn a_ten_bit_profile_uploads_p010le() {
        let filter = tonemap_cuda_software_fallback_filter(
            &cuda_hdr_state(),
            PixelFormat::Yuv420P10Le,
            TonemapAlgorithm::Hable,
        );

        assert!(filter.ends_with("format=p010le,hwupload_cuda"), "{filter}");
    }
}
