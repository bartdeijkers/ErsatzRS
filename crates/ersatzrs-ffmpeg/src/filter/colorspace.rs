use crate::filter::simple::{FrameDataLocation, FrameState, PixelFormat};

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ColorParams {
    pub color_range: Option<String>,
    pub color_space: Option<String>,
    pub color_transfer: Option<String>,
    pub color_primaries: Option<String>,
    pub is_unknown: bool,
    pub is_mixed: bool,
    pub is_bt470_bg: bool,
    pub is_bt2020_ten: bool,
}

#[must_use]
pub fn colorspace_filter(
    current: &FrameState,
    color_params: &ColorParams,
    desired_pixel_format: PixelFormat,
    force_input_overrides: bool,
    is_qsv: bool,
) -> String {
    let hwdownload = build_hwdownload_prefix(current);

    if color_params.is_bt470_bg {
        return match desired_pixel_format {
            PixelFormat::Yuv420P10Le => {
                format!("{hwdownload}colormatrix=src=bt470bg:dst=bt709,format=yuv420p10")
            }
            PixelFormat::Yuv420P => {
                format!("{hwdownload}colormatrix=src=bt470bg:dst=bt709,format=yuv420p")
            }
            _ => String::new(),
        };
    }

    let input_overrides =
        if !color_params.is_bt2020_ten && (color_params.is_mixed || force_input_overrides) {
            Some(build_input_overrides(color_params))
        } else {
            None
        };
    let input_overrides = input_overrides.as_deref().unwrap_or_default();

    if color_params.is_unknown {
        if is_qsv {
            return format!(
                "{hwdownload}setparams=range=tv:colorspace=bt709:color_trc=bt709:color_primaries=bt709"
            );
        }
        return "setparams=range=tv:colorspace=bt709:color_trc=bt709:color_primaries=bt709"
            .to_owned();
    }

    match desired_pixel_format {
        PixelFormat::Yuv420P10Le => {
            format!("{hwdownload}colorspace={input_overrides}all=bt709:format=yuv420p10:range=tv")
        }
        PixelFormat::Yuv420P => {
            format!("{hwdownload}colorspace={input_overrides}all=bt709:format=yuv420p:range=tv")
        }
        _ => String::new(),
    }
}

fn build_hwdownload_prefix(current: &FrameState) -> String {
    if current.frame_data_location != FrameDataLocation::Hardware {
        return String::new();
    }

    match current.pixel_format {
        PixelFormat::Vaapi | PixelFormat::Cuda => "hwdownload,format=p010le,".to_owned(),
        PixelFormat::Nv12 => "hwdownload,format=nv12,".to_owned(),
        PixelFormat::Yuv420P10Le => "hwdownload,format=yuv420p10le,".to_owned(),
        PixelFormat::Yuv420P => "hwdownload,format=yuv420p,".to_owned(),
    }
}

fn build_input_overrides(cp: &ColorParams) -> String {
    let range = cp
        .color_range
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(|value| format!("irange={value}:"))
        .unwrap_or_default();

    let transfer = normalized_value(cp.color_transfer.as_deref(), "bt709");
    let primaries = normalized_value(cp.color_primaries.as_deref(), "bt709");
    let space = normalized_value(cp.color_space.as_deref(), "bt709");

    format!("{range}ispace={space}:itrc={transfer}:iprimaries={primaries}:")
}

fn normalized_value(value: Option<&str>, fallback: &str) -> String {
    match value {
        Some(value) if !value.trim().is_empty() && !value.eq_ignore_ascii_case("reserved") => {
            value.to_owned()
        }
        _ => fallback.to_owned(),
    }
}
