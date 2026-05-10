use crate::filter::simple::{FrameDataLocation, FrameRate, FrameState};

/// Emits `ErsatzTV`'s `framerate=fps=N/D:flags=-scd` filter, prefixed by an
/// `hwdownload` step when the current frame data lives on hardware.
#[must_use]
pub fn frame_rate_filter(current: &FrameState, frame_rate: FrameRate) -> String {
    let frame_rate_filter = format!("framerate=fps={}:flags=-scd", frame_rate.r_frame_rate());

    if current.frame_data_location == FrameDataLocation::Hardware {
        let pixel_format = current.pixel_format.ffmpeg_name();
        return format!("hwdownload,format={pixel_format},{frame_rate_filter}");
    }

    frame_rate_filter
}
