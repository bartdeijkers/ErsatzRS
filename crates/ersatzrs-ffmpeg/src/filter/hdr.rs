//! HDR10 output signalling and static metadata (ADR 0064).
//!
//! ErsatzTV has no equivalent: it always tone-maps HDR sources to BT.709. This
//! module owns the beyond-parity `OutputDynamicRange::Hdr10Bt2020Pq` half of
//! the contract, so the SDR half stays byte-for-byte the ported ErsatzTV
//! behavior in [`super::tonemap`].
//!
//! The filter shapes here were characterized against the shipped `FFmpeg` 9
//! runtime rather than derived from documentation; see
//! `docs/how-to/hdr-output-characterization.md` for the probes and their
//! decoded results.

use crate::filter::simple::{FrameDataLocation, FrameState, PixelFormat};

/// BT.2020 primaries tag used for every HDR10 output.
pub const HDR10_PRIMARIES: &str = "bt2020";
/// SMPTE ST 2084 (PQ) transfer tag used for every HDR10 output.
pub const HDR10_TRANSFER: &str = "smpte2084";
/// Non-constant-luminance BT.2020 matrix used for every HDR10 output.
pub const HDR10_MATRIX: &str = "bt2020nc";
/// HDR10 output is always limited/TV range.
pub const HDR10_RANGE: &str = "tv";

/// Fixed SDR reference white for SDR-to-HDR10 mapping, in cd/m².
///
/// ITU-R BT.2408 defines HDR reference (diffuse) white as 203 cd/m². Mapping
/// SDR 100% white to that level is the documented policy ADR 0064 requires:
/// it keeps SDR programme material at a natural brightness next to HDR items
/// instead of blasting it to the display's peak, and it invents no highlight
/// detail above diffuse white.
///
/// Verified end to end: a 100% white BT.709 source through
/// [`sdr_to_hdr10_filter`] decodes to 10-bit Y = 573, which is the PQ code
/// value for 203 cd/m² in limited range.
pub const SDR_REFERENCE_WHITE_NITS: u32 = 203;

/// How a source stream's transfer characteristics map onto the output contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SourceTransferKind {
    /// Anything that is not PQ or HLG, including untagged streams.
    #[default]
    Sdr,
    /// SMPTE ST 2084 perceptual quantizer.
    Pq,
    /// ARIB STD-B67 hybrid log-gamma.
    Hlg,
}

impl SourceTransferKind {
    /// Classify from an ffprobe `color_transfer` tag.
    #[must_use]
    pub fn from_transfer_tag(transfer: Option<&str>) -> Self {
        let transfer = transfer.unwrap_or_default().trim().to_ascii_lowercase();
        if transfer.contains("smpte2084") {
            Self::Pq
        } else if transfer.contains("arib-std-b67") {
            Self::Hlg
        } else {
            Self::Sdr
        }
    }

    /// True when the source already carries a high-dynamic-range transfer.
    #[must_use]
    pub const fn is_hdr(self) -> bool {
        matches!(self, Self::Pq | Self::Hlg)
    }
}

/// SMPTE ST 2086 mastering-display colour volume.
///
/// Chromaticity values are in `FFmpeg`'s `G(x,y)B(x,y)R(x,y)WP(x,y)` units of
/// 1/50000, and luminance in `L(max,min)` units of 1/10000 cd/m², matching the
/// `-mastering_display` option and x265's `master-display` parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MasteringDisplayMetadata {
    pub green_x: u32,
    pub green_y: u32,
    pub blue_x: u32,
    pub blue_y: u32,
    pub red_x: u32,
    pub red_y: u32,
    pub white_point_x: u32,
    pub white_point_y: u32,
    pub max_luminance: u32,
    pub min_luminance: u32,
}

impl MasteringDisplayMetadata {
    /// Documented seeded default: BT.2020 container primaries with a D65 white
    /// point, 1000 cd/m² peak and 0.0001 cd/m² black.
    ///
    /// ADR 0064 forbids hidden guessed values. This is the value ErsatzRS emits
    /// when a transform changed the meaning of the source metadata or the
    /// source carried none, and it is stated here rather than buried in a
    /// format string so it can be reviewed and later made editable.
    pub const BT2020_1000_NITS: Self = Self {
        green_x: 8_500,
        green_y: 39_850,
        blue_x: 6_550,
        blue_y: 2_300,
        red_x: 35_400,
        red_y: 14_600,
        white_point_x: 15_635,
        white_point_y: 16_450,
        max_luminance: 10_000_000,
        min_luminance: 1,
    };

    /// Render the `-mastering_display` / x265 `master-display` value.
    #[must_use]
    pub fn to_ffmpeg_value(self) -> String {
        format!(
            "G({},{})B({},{})R({},{})WP({},{})L({},{})",
            self.green_x,
            self.green_y,
            self.blue_x,
            self.blue_y,
            self.red_x,
            self.red_y,
            self.white_point_x,
            self.white_point_y,
            self.max_luminance,
            self.min_luminance,
        )
    }
}

/// CTA-861.3 content light level: `MaxCLL` and `MaxFALL` in cd/m².
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContentLightLevel {
    pub max_cll: u32,
    pub max_fall: u32,
}

impl ContentLightLevel {
    /// Documented seeded default paired with
    /// [`MasteringDisplayMetadata::BT2020_1000_NITS`].
    pub const DEFAULT: Self = Self {
        max_cll: 1_000,
        max_fall: 400,
    };

    /// Render the `-content_light` value.
    #[must_use]
    pub fn to_ffmpeg_value(self) -> String {
        format!("{},{}", self.max_cll, self.max_fall)
    }
}

/// Pin the output frame tags to the HDR10 contract.
///
/// The pipeline appends this after every transform that establishes HDR10
/// output. Encoder `-color_*` output options alone proved unreliable: a
/// libx265 HDR10 encode whose frames still carried the source tags emitted
/// `color_transfer=unknown`, while `setparams` produced correct
/// `bt2020nc/smpte2084/bt2020/tv` signalling. The codebase already uses the
/// same `setparams` tail on the SDR tonemap path.
#[must_use]
pub fn hdr10_setparams() -> String {
    format!(
        "setparams=range={HDR10_RANGE}:colorspace={HDR10_MATRIX}:color_trc={HDR10_TRANSFER}:color_primaries={HDR10_PRIMARIES}"
    )
}

/// Map an SDR source into the HDR10 signal at [`SDR_REFERENCE_WHITE_NITS`].
///
/// No highlight detail is invented: the transform is a pure re-encoding of the
/// same picture into PQ, with SDR diffuse white landing at 203 cd/m².
#[must_use]
pub fn sdr_to_hdr10_filter(current: &FrameState) -> String {
    let hwdownload = hwdownload_prefix(current);
    format!(
        "{hwdownload}zscale=t=linear:npl={nits},zscale=p={HDR10_PRIMARIES},\
zscale=t={HDR10_TRANSFER}:m={HDR10_MATRIX}:r={HDR10_RANGE}:npl={nits},\
format={pixel_format},{setparams}",
        nits = SDR_REFERENCE_WHITE_NITS,
        pixel_format = PixelFormat::Yuv420P10Le.ffmpeg_name(),
        setparams = hdr10_setparams(),
    )
}

/// Normalize an HLG source into the PQ HDR10 output contract.
///
/// HLG and PQ are both HDR, but a single linear stream must not change its
/// transfer function between items, so HLG is converted rather than passed
/// through. `npl=1000` is the nominal peak the HLG OOTF is resolved against and
/// matches the seeded mastering-display peak.
#[must_use]
pub fn hlg_to_hdr10_filter(current: &FrameState) -> String {
    let hwdownload = hwdownload_prefix(current);
    format!(
        "{hwdownload}zscale=t=linear:npl=1000,zscale=p={HDR10_PRIMARIES},\
zscale=t={HDR10_TRANSFER}:m={HDR10_MATRIX}:r={HDR10_RANGE}:npl=1000,\
format={pixel_format},{setparams}",
        pixel_format = PixelFormat::Yuv420P10Le.ffmpeg_name(),
        setparams = hdr10_setparams(),
    )
}

/// Keep a PQ source's picture untouched and only pin the output tags.
///
/// A PQ BT.2020 source already satisfies the output contract, so the cheapest
/// correct action is to assert the tags and let the encoder carry the source's
/// own mastering-display and content-light side data through. Verified for both
/// libx265 and `hevc_nvenc`: scaling a tagged HDR10 source with no metadata
/// options preserved mastering display and MaxCLL/MaxFALL in the output.
#[must_use]
pub fn preserve_hdr10_filter() -> String {
    hdr10_setparams()
}

fn hwdownload_prefix(current: &FrameState) -> String {
    if current.frame_data_location != FrameDataLocation::Hardware {
        return String::new();
    }

    let download_format = match current.pixel_format {
        PixelFormat::Cuda | PixelFormat::Yuv420P10Le | PixelFormat::Vaapi => "p010le",
        PixelFormat::Nv12 => "nv12",
        PixelFormat::Yuv420P => "yuv420p",
    };
    format!("hwdownload,format={download_format},")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter::simple::FrameSize;

    fn software_state() -> FrameState {
        FrameState::new(FrameSize::new(1920, 1080))
    }

    #[test]
    fn transfer_tags_classify_pq_hlg_and_sdr() {
        assert_eq!(
            SourceTransferKind::from_transfer_tag(Some("smpte2084")),
            SourceTransferKind::Pq
        );
        assert_eq!(
            SourceTransferKind::from_transfer_tag(Some("arib-std-b67")),
            SourceTransferKind::Hlg
        );
        assert_eq!(
            SourceTransferKind::from_transfer_tag(Some("bt709")),
            SourceTransferKind::Sdr
        );
        assert_eq!(
            SourceTransferKind::from_transfer_tag(None),
            SourceTransferKind::Sdr
        );
        assert!(SourceTransferKind::Pq.is_hdr());
        assert!(SourceTransferKind::Hlg.is_hdr());
        assert!(!SourceTransferKind::Sdr.is_hdr());
    }

    #[test]
    fn mastering_display_renders_the_ffmpeg_value_shape() {
        assert_eq!(
            MasteringDisplayMetadata::BT2020_1000_NITS.to_ffmpeg_value(),
            "G(8500,39850)B(6550,2300)R(35400,14600)WP(15635,16450)L(10000000,1)"
        );
        assert_eq!(ContentLightLevel::DEFAULT.to_ffmpeg_value(), "1000,400");
    }

    #[test]
    fn sdr_to_hdr10_uses_the_documented_reference_white() {
        let filter = sdr_to_hdr10_filter(&software_state());

        assert!(filter.contains("npl=203"), "{filter}");
        assert!(filter.contains("zscale=t=smpte2084"), "{filter}");
        assert!(
            filter.ends_with(
                "setparams=range=tv:colorspace=bt2020nc:color_trc=smpte2084:color_primaries=bt2020"
            ),
            "{filter}"
        );
        assert!(!filter.starts_with("hwdownload"), "{filter}");
    }

    #[test]
    fn hardware_frames_are_downloaded_before_software_hdr_conversion() {
        let mut state = software_state();
        state.frame_data_location = FrameDataLocation::Hardware;
        state.pixel_format = PixelFormat::Cuda;

        assert!(
            sdr_to_hdr10_filter(&state).starts_with("hwdownload,format=p010le,"),
            "{}",
            sdr_to_hdr10_filter(&state)
        );
    }

    #[test]
    fn preserving_pq_only_pins_the_output_tags() {
        assert_eq!(
            preserve_hdr10_filter(),
            "setparams=range=tv:colorspace=bt2020nc:color_trc=smpte2084:color_primaries=bt2020"
        );
    }
}
