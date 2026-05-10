use ersatzrs_domain::HardwareAccelerationKind;

use crate::filter::simple::{FrameDataLocation, FrameState, PixelFormat};

#[must_use]
pub fn yadif_filter(current: &FrameState) -> String {
    if current.frame_data_location == FrameDataLocation::Hardware {
        if current.pixel_format == PixelFormat::Nv12 {
            return "hwdownload,format=nv12,yadif=1".to_owned();
        }
        return format!(
            "hwdownload,format={},yadif=1",
            current.pixel_format.ffmpeg_name()
        );
    }
    "yadif=1".to_owned()
}

#[must_use]
pub fn yadif_cuda_filter(current: &FrameState) -> &'static str {
    if current.frame_data_location == FrameDataLocation::Hardware {
        "yadif_cuda"
    } else {
        "hwupload_cuda,yadif_cuda"
    }
}

/// `deinterlace_qsv` only supports `nv12` — uploaded frames are forced via
/// `format=nv12` to mirror `ErsatzTV`'s behavior.
#[must_use]
pub fn deinterlace_qsv_filter(current: &FrameState, extra_hardware_frames: u32) -> String {
    if current.frame_data_location == FrameDataLocation::Software {
        format!("format=nv12,hwupload=extra_hw_frames={extra_hardware_frames},deinterlace_qsv")
    } else {
        "deinterlace_qsv".to_owned()
    }
}

#[must_use]
pub fn deinterlace_vaapi_filter(current: &FrameState) -> &'static str {
    if current.frame_data_location == FrameDataLocation::Hardware {
        "deinterlace_vaapi"
    } else {
        "format=nv12|p010le|vaapi,hwupload,deinterlace_vaapi"
    }
}

/// Selects the appropriate deinterlace filter for the active hardware
/// acceleration. `extra_hardware_frames` is only consulted for QSV.
#[must_use]
pub fn deinterlace_filter(
    acceleration: HardwareAccelerationKind,
    current: &FrameState,
    extra_hardware_frames: u32,
) -> String {
    match acceleration {
        HardwareAccelerationKind::Nvenc => yadif_cuda_filter(current).to_owned(),
        HardwareAccelerationKind::Qsv => deinterlace_qsv_filter(current, extra_hardware_frames),
        HardwareAccelerationKind::Vaapi => deinterlace_vaapi_filter(current).to_owned(),
        HardwareAccelerationKind::None
        | HardwareAccelerationKind::Amf
        | HardwareAccelerationKind::VideoToolbox
        | HardwareAccelerationKind::V4l2m2m
        | HardwareAccelerationKind::Rkmpp => yadif_filter(current),
    }
}
