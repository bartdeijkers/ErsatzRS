use ersatzrs_domain::HardwareAccelerationKind;

use crate::filter::simple::{FrameDataLocation, FrameState, PixelFormat};

#[must_use]
pub fn hardware_upload_filter(
    acceleration: HardwareAccelerationKind,
    qsv_extra_hardware_frames: u32,
) -> String {
    match acceleration {
        HardwareAccelerationKind::None => String::new(),
        HardwareAccelerationKind::Nvenc => "hwupload_cuda".to_owned(),
        HardwareAccelerationKind::Qsv => {
            format!("hwupload=extra_hw_frames={qsv_extra_hardware_frames}")
        }
        HardwareAccelerationKind::Vaapi => "format=nv12|p010le|vaapi,hwupload".to_owned(),
        HardwareAccelerationKind::Amf
        | HardwareAccelerationKind::VideoToolbox
        | HardwareAccelerationKind::V4l2m2m
        | HardwareAccelerationKind::Rkmpp => "hwupload".to_owned(),
    }
}

#[must_use]
pub fn hardware_download_filter(current: &FrameState) -> String {
    if current.frame_data_location != FrameDataLocation::Hardware {
        return String::new();
    }

    match current.pixel_format {
        PixelFormat::Vaapi => "hwdownload,format=vaapi|p010le".to_owned(),
        PixelFormat::Cuda => "hwdownload,format=p010le".to_owned(),
        PixelFormat::Nv12 => "hwdownload,format=nv12".to_owned(),
        PixelFormat::Yuv420P10Le => "hwdownload,format=yuv420p10le".to_owned(),
        PixelFormat::Yuv420P => "hwdownload,format=yuv420p".to_owned(),
    }
}
