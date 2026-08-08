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

use ersatzrs_domain::HardwareAccelerationKind;

use crate::filter::simple::PixelFormat;

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

/// Source colour tags used to fill in a `zscale` conversion's input side.
///
/// Every field falls back to BT.709 when the source did not carry it, because
/// `zscale` refuses to convert from an unknown colourspace and refusing to play
/// an untagged file would be worse than assuming the near-universal default.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SourceColorTags {
    pub color_range: Option<String>,
    pub color_space: Option<String>,
    pub color_transfer: Option<String>,
    pub color_primaries: Option<String>,
}

impl SourceColorTags {
    /// Render the input-side `setparams` that pins the conversion source.
    #[must_use]
    pub fn to_setparams(&self) -> String {
        format!(
            "setparams=range={}:colorspace={}:color_trc={}:color_primaries={}",
            tag_or(self.color_range.as_deref(), "tv"),
            tag_or(self.color_space.as_deref(), "bt709"),
            tag_or(self.color_transfer.as_deref(), "bt709"),
            tag_or(self.color_primaries.as_deref(), "bt709"),
        )
    }
}

fn tag_or(value: Option<&str>, fallback: &str) -> String {
    match value.map(str::trim) {
        Some(value) if !value.is_empty() && !value.eq_ignore_ascii_case("unknown") => {
            value.to_owned()
        }
        _ => fallback.to_owned(),
    }
}

/// ST 2086 mastering-display colour volume and CTA-861.3 content light level.
///
/// Both live in `ersatzrs-domain` because they are per-profile configuration an
/// operator can edit (ADR 0064), not pipeline-internal detail. Re-exported here
/// so the filter and pipeline code reads as one HDR vocabulary.
pub use ersatzrs_domain::ffmpeg::{ContentLightLevel, MasteringDisplayMetadata};

/// Drop Dolby Vision side data while keeping HDR10 static metadata.
///
/// ADR 0064: HDR10 output "must not retain Dolby Vision signalling that the
/// muxed stream no longer satisfies". A transcoded HDR10 stream is HDR10, not
/// Dolby Vision — its base layer has been re-encoded and no RPU accompanies it
/// — so carrying the record forward would describe a stream that does not
/// exist, and a client could select a Dolby Vision decode path for it.
///
/// Distinct from the SDR strip in [`super::tonemap`], which also removes
/// mastering display and content light: those stay valid here and are the whole
/// point of an HDR10 output.
#[must_use]
pub fn strip_dolby_vision_metadata_filter() -> &'static str {
    "sidedata=mode=delete:type=DOVI_RPU_BUFFER,sidedata=mode=delete:type=DOVI_METADATA"
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
/// `hwdownload_format` is the *source surface's* software format, or `None`
/// when the frames are already in software. It must not be derived from the
/// desired output: `hwdownload` can only produce the format the surface
/// actually holds, and an HDR10 profile wants 10-bit output from what may be an
/// 8-bit source. Measured against `FFmpeg` 9: `format=p010le` on an 8-bit CUDA
/// surface fails with `Invalid output format p010le for hwframe download`.
/// `source_tags` fills in the input colour tags. `zscale` cannot convert from
/// an unknown colourspace — an untagged source fails with
/// `code 3074: no path between colorspaces` — and untagged files are common, so
/// the input side is asserted explicitly. Missing tags fall back to BT.709, the
/// same simplification ErsatzTV's colorspace filter makes for unknown colours.
#[must_use]
pub fn sdr_to_hdr10_filter(
    hwdownload_format: Option<&str>,
    source_tags: &SourceColorTags,
) -> String {
    let hwdownload = hwdownload_prefix(hwdownload_format);
    let input_tags = source_tags.to_setparams();
    format!(
        "{hwdownload}{input_tags},zscale=t=linear:npl={nits},zscale=p={HDR10_PRIMARIES},\
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
/// HLG is a 10-bit format, so a hardware HLG surface is always p010; callers
/// pass `Some("p010le")` when the frames are in hardware.
#[must_use]
pub fn hlg_to_hdr10_filter(hwdownload_format: Option<&str>) -> String {
    let hwdownload = hwdownload_prefix(hwdownload_format);
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

/// Whether a hardware acceleration has a proven HDR10 output graph.
///
/// ADR 0064: hardware acceleration may change performance, not colour
/// semantics. A backend without a proven HDR10 graph must therefore not be used
/// for an HDR10 profile at all — producing SDR from it would be exactly the
/// silent downgrade the ADR forbids — so the pipeline demotes to software
/// encoding and health reports why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hdr10OutputSupport {
    /// Characterized end to end against real hardware.
    Proven,
    /// No fixture evidence yet; demote to software rather than guess.
    Unproven,
}

/// Classify a hardware acceleration for HDR10 output.
///
/// Only the software encoder and NVENC are proven. Both were verified against
/// `FFmpeg` 9 with a real HDR10, HLG, and SDR source: correct `bt2020nc` /
/// `smpte2084` / `bt2020` signalling plus mastering-display and
/// MaxCLL/MaxFALL side data in the muxed output. No Intel, AMD, Apple, or
/// Rockchip hardware was available to characterize the others.
#[must_use]
pub const fn hdr10_output_support(acceleration: HardwareAccelerationKind) -> Hdr10OutputSupport {
    match acceleration {
        HardwareAccelerationKind::None | HardwareAccelerationKind::Nvenc => {
            Hdr10OutputSupport::Proven
        }
        HardwareAccelerationKind::Qsv
        | HardwareAccelerationKind::Vaapi
        | HardwareAccelerationKind::VideoToolbox
        | HardwareAccelerationKind::Amf
        | HardwareAccelerationKind::V4l2m2m
        | HardwareAccelerationKind::Rkmpp => Hdr10OutputSupport::Unproven,
    }
}

/// Upload a software HDR10 frame back to CUDA so NVENC keeps encoding it.
///
/// Verified against `FFmpeg` 9: an SDR or HLG source converted in software, then
/// `format=p010le,hwupload_cuda`, then `scale_cuda`, encodes on `hevc_nvenc`
/// with full HDR10 signalling and static metadata.
#[must_use]
pub fn hdr10_cuda_upload_suffix() -> String {
    ",format=p010le,hwupload_cuda".to_owned()
}

/// The NVENC form of [`sdr_to_hdr10_filter`].
#[must_use]
pub fn sdr_to_hdr10_cuda_filter(
    hwdownload_format: Option<&str>,
    source_tags: &SourceColorTags,
) -> String {
    format!(
        "{}{}",
        sdr_to_hdr10_filter(hwdownload_format, source_tags),
        hdr10_cuda_upload_suffix()
    )
}

/// The NVENC form of [`hlg_to_hdr10_filter`].
#[must_use]
pub fn hlg_to_hdr10_cuda_filter(hwdownload_format: Option<&str>) -> String {
    format!(
        "{}{}",
        hlg_to_hdr10_filter(hwdownload_format),
        hdr10_cuda_upload_suffix()
    )
}

fn hwdownload_prefix(hwdownload_format: Option<&str>) -> String {
    hwdownload_format.map_or_else(String::new, |format| format!("hwdownload,format={format},"))
}

/// The software format a hardware surface holding this source can be
/// downloaded as.
///
/// Derived from the *source* pixel format, because that is what the decoder put
/// in the surface. 10-bit sources give p010, everything else nv12.
#[must_use]
pub const fn hardware_surface_download_format(
    source_pixel_format: Option<PixelFormat>,
) -> &'static str {
    match source_pixel_format {
        Some(PixelFormat::Yuv420P10Le) => "p010le",
        _ => "nv12",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let filter = sdr_to_hdr10_filter(None, &SourceColorTags::default());

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
    fn hardware_frames_are_downloaded_in_the_source_surface_format() {
        // An 8-bit SDR source in a CUDA surface must download as nv12: asking
        // for p010le fails with `Invalid output format p010le for hwframe
        // download`, measured against FFmpeg 9.
        assert_eq!(
            hardware_surface_download_format(Some(PixelFormat::Yuv420P)),
            "nv12"
        );
        assert_eq!(
            hardware_surface_download_format(Some(PixelFormat::Nv12)),
            "nv12"
        );
        assert_eq!(hardware_surface_download_format(None), "nv12");
        // A 10-bit source holds p010.
        assert_eq!(
            hardware_surface_download_format(Some(PixelFormat::Yuv420P10Le)),
            "p010le"
        );

        assert!(
            sdr_to_hdr10_filter(Some("nv12"), &SourceColorTags::default())
                .starts_with("hwdownload,format=nv12,")
        );
        assert!(sdr_to_hdr10_filter(None, &SourceColorTags::default()).starts_with("setparams="));
        assert!(hlg_to_hdr10_filter(Some("p010le")).starts_with("hwdownload,format=p010le,"));
    }

    #[test]
    fn untagged_sources_are_pinned_to_bt709_before_conversion() {
        // zscale refuses to convert from an unknown colourspace
        // (`code 3074: no path between colorspaces`), and untagged files are
        // common, so the input side is always asserted.
        let filter = sdr_to_hdr10_filter(None, &SourceColorTags::default());
        assert!(
            filter.starts_with(
                "setparams=range=tv:colorspace=bt709:color_trc=bt709:color_primaries=bt709,zscale="
            ),
            "{filter}"
        );

        // Real source tags win over the fallback.
        let tagged = SourceColorTags {
            color_range: Some("pc".to_owned()),
            color_space: Some("smpte170m".to_owned()),
            color_transfer: Some("smpte170m".to_owned()),
            color_primaries: Some("smpte170m".to_owned()),
        };
        assert!(
            sdr_to_hdr10_filter(None, &tagged).starts_with(
                "setparams=range=pc:colorspace=smpte170m:color_trc=smpte170m:color_primaries=smpte170m,"
            ),
            "{}",
            sdr_to_hdr10_filter(None, &tagged)
        );

        // ffprobe writes the literal string "unknown" for missing tags.
        let unknown = SourceColorTags {
            color_space: Some("unknown".to_owned()),
            ..SourceColorTags::default()
        };
        assert!(
            sdr_to_hdr10_filter(None, &unknown).contains("colorspace=bt709"),
            "{}",
            sdr_to_hdr10_filter(None, &unknown)
        );
    }

    #[test]
    fn the_cuda_variants_re_upload_for_nvenc() {
        assert!(sdr_to_hdr10_cuda_filter(None, &SourceColorTags::default())
            .ends_with(",format=p010le,hwupload_cuda"));
        assert!(hlg_to_hdr10_cuda_filter(Some("p010le")).ends_with(",format=p010le,hwupload_cuda"));
    }

    #[test]
    fn only_software_and_nvenc_are_proven_for_hdr10() {
        use ersatzrs_domain::HardwareAccelerationKind as Hw;

        for proven in [Hw::None, Hw::Nvenc] {
            assert_eq!(hdr10_output_support(proven), Hdr10OutputSupport::Proven);
        }
        for unproven in [
            Hw::Qsv,
            Hw::Vaapi,
            Hw::VideoToolbox,
            Hw::Amf,
            Hw::V4l2m2m,
            Hw::Rkmpp,
        ] {
            assert_eq!(hdr10_output_support(unproven), Hdr10OutputSupport::Unproven);
        }
    }

    #[test]
    fn preserving_pq_only_pins_the_output_tags() {
        assert_eq!(
            preserve_hdr10_filter(),
            "setparams=range=tv:colorspace=bt2020nc:color_trc=smpte2084:color_primaries=bt2020"
        );
    }
}
