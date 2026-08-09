use crate::{strip_dolby_vision_metadata_filter, strip_hdr_metadata_filter, FrameSize};

/// Convert a CUDA-decoded Dolby Vision profile 5 frame into software BT.709.
///
/// The RGBA readback is intentional. Direct YUV readback from libplacebo was
/// corrupt on the characterized runtimes; the functional graph probe exercises
/// this same intermediate before the operation may be selected.
#[must_use]
pub fn dolby_vision_profile_5_sdr_filter(size: FrameSize) -> String {
    format!(
        "hwdownload,format=p010le,hwupload,libplacebo=apply_dolbyvision=true:w={}:h={}:colorspace=bt709:color_primaries=bt709:color_trc=bt709:format=rgba,hwdownload,format=rgba,format=yuv420p,setparams=range=tv:colorspace=bt709:color_trc=bt709:color_primaries=bt709,{}",
        size.width,
        size.height,
        strip_hdr_metadata_filter()
    )
}

/// Convert a CUDA-decoded Dolby Vision profile 5 frame into software HDR10.
///
/// Libplacebo consumes the RPU and establishes BT.2020/PQ. RGBA64 readback and
/// explicit software YUV conversion preserve ten-bit precision before NVENC.
#[must_use]
pub fn dolby_vision_profile_5_hdr10_filter(size: FrameSize) -> String {
    format!(
        "hwdownload,format=p010le,hwupload,libplacebo=apply_dolbyvision=true:w={}:h={}:colorspace=bt2020nc:color_primaries=bt2020:color_trc=smpte2084:format=rgba64le,hwdownload,format=rgba64le,zscale=min=gbr:tin=smpte2084:pin=bt2020:rin=full:m=bt2020nc:t=smpte2084:p=bt2020:r=tv,format=yuv420p10le,setparams=range=tv:colorspace=bt2020nc:color_trc=smpte2084:color_primaries=bt2020,{}",
        size.width,
        size.height,
        strip_dolby_vision_metadata_filter()
    )
}
