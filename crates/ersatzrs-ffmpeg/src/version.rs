use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::process::ExitStatus;

use ersatzrs_domain::HardwareAccelerationKind;
pub use ersatzrs_hwprobe::NvidiaDeviceIdentity;
use sha2::{Digest, Sha256};
use tokio::io::AsyncReadExt;

use crate::runtime::FfmpegRuntimeAcquisition;

/// Supported `FFmpeg` and `ffprobe` major version.
pub const REQUIRED_MAJOR: u32 = 9;
/// Minimum required `FFmpeg` and `ffprobe` minor version.
pub const REQUIRED_MINOR: u32 = 0;

pub(crate) const DEFAULT_FFMPEG_PATH: &str = "ffmpeg";
pub(crate) const DEFAULT_FFPROBE_PATH: &str = "ffprobe";
pub(crate) const FFMPEG_PATH_ENV_VARS: &[&str] = &[
    "ErsatzRS_FFMPEG__PATH",
    "ERSATZRS_FFMPEG__PATH",
    "FFMPEG_PATH",
];
pub(crate) const FFPROBE_PATH_ENV_VARS: &[&str] = &[
    "ErsatzRS_FFPROBE__PATH",
    "ERSATZRS_FFPROBE__PATH",
    "FFPROBE_PATH",
    "ERSATZRS_FFPROBE",
];

const REQUIRED_FILTERS: &[&str] = &[
    // ErsatzRS intentionally adds drawvg for the in-process graphics engine
    // replacement covered by ADR 0003.
    "drawvg",
    "drawtext",
    "apad",
    "aresample",
    "color",
    "colorchannelmixer",
    "colorspace",
    "crop",
    "fade",
    "format",
    "fps",
    "framerate",
    "loop",
    "loudnorm",
    "overlay",
    "pad",
    "realtime",
    "reverse",
    "setpts",
    "settb",
    "scale",
    "subtitles",
    "tonemap",
    "tpad",
    "trim",
    "yadif",
    "zscale",
];
const REQUIRED_ENCODERS: &[&str] = &[
    "aac",
    "ac3",
    "ffv1",
    "libsvtav1",
    "libx264",
    "libx265",
    "png",
];
const REQUIRED_DECODERS: &[&str] = &["av1", "ffv1", "h264", "hevc"];
const REQUIRED_MUXERS: &[&str] = &["nut"];
const REQUIRED_DEMUXERS: &[&str] = &["nut"];
const REQUIRED_CONFIGURATION_FLAGS: &[&str] = &[
    "--enable-cairo",
    "--enable-libfontconfig",
    "--enable-libass",
    "--enable-libfreetype",
    "--enable-libharfbuzz",
];

/// Errors returned while checking the local `FFmpeg` toolchain.
#[derive(thiserror::Error, Debug)]
pub enum FfmpegCheckError {
    /// A required binary could not be spawned.
    #[error("{tool} not found at {path}: {error}. Please install {tool} major version {required_major} (at least {required_major}.{required_minor}).")]
    ToolNotFound {
        /// Binary name, such as `ffmpeg` or `ffprobe`.
        tool: &'static str,
        /// Path that was attempted.
        path: String,
        /// Minimum required major version.
        required_major: u32,
        /// Minimum required minor version.
        required_minor: u32,
        /// Spawn error text.
        error: String,
    },
    /// A required binary exited unsuccessfully.
    #[error("{tool} at {path} exited with status {status}: {stderr}")]
    ToolFailed {
        /// Binary name, such as `ffmpeg` or `ffprobe`.
        tool: &'static str,
        /// Path that was attempted.
        path: String,
        /// Process exit status.
        status: String,
        /// Captured stderr.
        stderr: String,
    },
    /// The version banner did not contain a parseable version.
    #[error("could not parse {tool} version from output: {output:?}")]
    ParseError {
        /// Binary name, such as `ffmpeg` or `ffprobe`.
        tool: &'static str,
        /// Output that could not be parsed.
        output: String,
    },
    /// A binary is older than the required runtime floor.
    #[error("{tool} {found} is too old. ErsatzRS requires {tool} major version {required_major} (at least {required_major}.{required_minor}).")]
    TooOld {
        /// Binary name, such as `ffmpeg` or `ffprobe`.
        tool: &'static str,
        /// Version reported by the binary.
        found: FfmpegVersion,
        /// Minimum required major version.
        required_major: u32,
        /// Minimum required minor version.
        required_minor: u32,
    },
    /// A binary uses a newer, not-yet-verified major version.
    #[error("{tool} {found} is unsupported. ErsatzRS requires {tool} major version {required_major} (at least {required_major}.{required_minor}).")]
    UnsupportedMajor {
        /// Binary name, such as `ffmpeg` or `ffprobe`.
        tool: &'static str,
        /// Version reported by the binary.
        found: FfmpegVersion,
        /// Supported major version.
        required_major: u32,
        /// Minimum supported minor version.
        required_minor: u32,
    },
    /// One or more required `FFmpeg` capabilities were absent.
    #[error("FFmpeg runtime is missing required capabilities: {0:?}")]
    RuntimeIssues(Vec<FfmpegRuntimeIssue>),
}

/// Parsed semantic version from an FFmpeg-family version banner.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct FfmpegVersion {
    /// Major version.
    pub major: u32,
    /// Minor version.
    pub minor: u32,
    /// Patch version, or zero when the banner omits it.
    pub patch: u32,
}

impl FfmpegVersion {
    /// Returns whether this version satisfies a major/minor runtime floor.
    #[must_use]
    pub const fn satisfies(&self, req_major: u32, req_minor: u32) -> bool {
        self.major > req_major || self.major == req_major && self.minor >= req_minor
    }

    /// Returns whether this version is within the verified major-version line.
    #[must_use]
    pub const fn is_supported(&self, req_major: u32, req_minor: u32) -> bool {
        self.major == req_major && self.minor >= req_minor
    }
}

impl std::fmt::Display for FfmpegVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Runtime check inputs, including binary paths and required capabilities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FfmpegRuntimeCheckConfig {
    /// Path to the `ffmpeg` binary.
    pub ffmpeg_path: String,
    /// Path to the `ffprobe` binary.
    pub ffprobe_path: String,
    /// Required major version.
    pub required_major: u32,
    /// Required minor version.
    pub required_minor: u32,
    /// Filters that must be present in `ffmpeg -filters`.
    pub required_filters: Vec<String>,
    /// Encoders that must be present in `ffmpeg -encoders`.
    pub required_encoders: Vec<String>,
    /// Decoders that must be present in `ffmpeg -decoders`.
    pub required_decoders: Vec<String>,
    /// Output containers that must be supported by `ffmpeg -formats`.
    pub required_muxers: Vec<String>,
    /// Input containers that must be supported by `ffmpeg -formats`.
    pub required_demuxers: Vec<String>,
    /// Configuration flags that must appear in `ffmpeg -version`.
    pub required_configuration_flags: Vec<String>,
    /// Hardware accelerators that must be present.
    pub required_hwaccels: Vec<String>,
    /// Hardware accelerators to report as warnings when absent.
    pub optional_hwaccels: Vec<String>,
    /// Optional hardware devices to probe for per-device VAAPI/QSV capability
    /// details.
    pub hardware_probe_devices: Vec<FfmpegHardwareProbeDevice>,
}

impl Default for FfmpegRuntimeCheckConfig {
    fn default() -> Self {
        Self {
            ffmpeg_path: ffmpeg_path_from_env(),
            ffprobe_path: ffprobe_path_from_env(),
            required_major: REQUIRED_MAJOR,
            required_minor: REQUIRED_MINOR,
            required_filters: to_owned_vec(REQUIRED_FILTERS),
            required_encoders: to_owned_vec(REQUIRED_ENCODERS),
            required_decoders: to_owned_vec(REQUIRED_DECODERS),
            required_muxers: to_owned_vec(REQUIRED_MUXERS),
            required_demuxers: to_owned_vec(REQUIRED_DEMUXERS),
            required_configuration_flags: to_owned_vec(REQUIRED_CONFIGURATION_FLAGS),
            required_hwaccels: Vec::new(),
            optional_hwaccels: Vec::new(),
            hardware_probe_devices: Vec::new(),
        }
    }
}

impl FfmpegRuntimeCheckConfig {
    /// Create the default check config from process environment variables.
    #[must_use]
    pub fn from_env() -> Self {
        Self::default()
    }

    /// Create a check config with explicit, already-resolved tool paths.
    ///
    /// Used by startup after the [`crate::locator`] has resolved the `ffmpeg`
    /// and `ffprobe` paths from DB config / next-to-executable / `PATH`, so the
    /// runtime report reflects the same paths that spawn sites will use.
    #[must_use]
    pub fn with_paths(ffmpeg_path: impl Into<String>, ffprobe_path: impl Into<String>) -> Self {
        Self {
            ffmpeg_path: ffmpeg_path.into(),
            ffprobe_path: ffprobe_path.into(),
            ..Self::default()
        }
    }

    /// Attach optional hardware device probes to this runtime check.
    #[must_use]
    pub fn with_hardware_probe_devices(mut self, devices: Vec<FfmpegHardwareProbeDevice>) -> Self {
        self.hardware_probe_devices = devices;
        self
    }
}

/// Profile-specific hardware probe inputs used to enrich the runtime report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FfmpegHardwareProbeDevice {
    /// VAAPI display name, such as `drm`.
    pub vaapi_display: Option<String>,
    /// VAAPI/QSV device path, such as `/dev/dri/renderD128`.
    pub vaapi_device: Option<String>,
    /// Driver name for `LIBVA_DRIVER_NAME`.
    pub vaapi_driver: Option<String>,
}

impl FfmpegHardwareProbeDevice {
    /// Create a hardware probe config from optional profile values.
    #[must_use]
    pub fn new(
        vaapi_display: Option<String>,
        vaapi_device: Option<String>,
        vaapi_driver: Option<String>,
    ) -> Self {
        Self {
            vaapi_display,
            vaapi_device,
            vaapi_driver,
        }
    }
}

/// Version details for one FFmpeg-family binary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FfmpegToolVersion {
    /// Binary name, such as `ffmpeg` or `ffprobe`.
    pub tool: &'static str,
    /// Path that was executed.
    pub path: String,
    /// Parsed version.
    pub version: FfmpegVersion,
    /// Configure flags parsed from the version banner.
    pub configuration_flags: BTreeSet<String>,
    /// SHA-256 digest of the executed binary. This is absent only when the
    /// executable was resolved indirectly by the OS and could not be opened.
    pub sha256: Option<String>,
}

/// Complete identity of one validated `FFmpeg` runtime publication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FfmpegRuntimeIdentity {
    /// Monotonic publication generation.
    pub generation: u64,
    /// Executed `FFmpeg` path and digest.
    pub ffmpeg_path: String,
    pub ffmpeg_sha256: Option<String>,
    /// Executed ffprobe path and digest.
    pub ffprobe_path: String,
    pub ffprobe_sha256: Option<String>,
    /// Device-zero identity used for NVIDIA capability and graph probes.
    pub selected_nvidia_device: Option<NvidiaDeviceIdentity>,
}

/// Capability names reported by the local `ffmpeg` binary.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FfmpegCapabilityReport {
    /// Runtime publication that produced these capabilities. Runtime-learned
    /// denials and generated commands must remain scoped to this identity.
    pub runtime_identity: Option<FfmpegRuntimeIdentity>,
    /// Filter names from `ffmpeg -filters`.
    pub filters: BTreeSet<String>,
    /// Encoder names from `ffmpeg -encoders`.
    pub encoders: BTreeSet<String>,
    /// Decoder names from `ffmpeg -decoders`.
    pub decoders: BTreeSet<String>,
    /// Output container names from `ffmpeg -formats`.
    pub muxers: BTreeSet<String>,
    /// Input container names from `ffmpeg -formats`.
    pub demuxers: BTreeSet<String>,
    /// Bitstream filter names from `ffmpeg -bsfs`.
    pub bitstream_filters: BTreeSet<String>,
    /// Hardware accelerator names from `ffmpeg -hwaccels`.
    pub hwaccels: BTreeSet<String>,
    /// Configure flags parsed from `ffmpeg -version`.
    pub configuration_flags: BTreeSet<String>,
    /// VAAPI device profile/entrypoint rows, usually parsed from `vainfo`.
    pub vaapi_profiles: Vec<VaapiProfileEntrypoint>,
    /// QSV device profile/entrypoint rows, usually parsed from `ffmpeg` QSV
    /// probe output plus `vainfo` on Linux.
    pub qsv_profiles: Vec<VaapiProfileEntrypoint>,
    /// NVIDIA native device details, or raw GPU-list probe output from
    /// `FFmpeg`'s `h264_nvenc -gpu list` fallback path.
    pub nvidia_gpu_list: Option<String>,
    /// The exact NVIDIA adapter whose device-zero capabilities were probed.
    pub selected_nvidia_device: Option<NvidiaDeviceIdentity>,
    /// NVIDIA encoder details parsed from `ffmpeg -h encoder=<name>` output.
    pub nvidia_encoders: Vec<NvidiaEncoderCapability>,
    /// NVIDIA decoder details reported by the native CUDA/NVDEC device probe.
    pub nvidia_decoders: Vec<NvidiaDecoderCapability>,
    /// Runtime-learned negative hardware decode rows.
    pub hardware_decode_denials: Vec<HardwareDecodeDenial>,
    /// Runtime-learned negative hardware *graph* rows (ADR 0014, milestone
    /// `p8b-01`). Distinct from decode denials: these record whole
    /// operation-scoped graphs whose components each initialize but do not
    /// interoperate.
    pub hardware_graph_denials: Vec<crate::graph_probe::HardwareGraphDenial>,
    /// Graph operations a functional probe positively proved, with the device
    /// they were proved on. Empty on a runtime that has not probed yet.
    pub proven_hardware_graphs: Vec<(crate::graph_probe::HardwareGraphOperation, String)>,
    /// Graph operations whose functional probe was attempted, including probes
    /// that failed without a classifiable denial. This is deliberately
    /// separate from proofs and denials: an attempted-but-unproven graph must
    /// not fall back to the legacy "never probed" selection behavior.
    pub attempted_hardware_graphs: Vec<(crate::graph_probe::HardwareGraphOperation, String)>,
    /// `VideoToolbox` hardware decoder `FourCC` values reported by the macOS
    /// framework probe, such as `avc1` or `hvc1`.
    pub videotoolbox_decoders: BTreeSet<String>,
    /// `VideoToolbox` encoder names reported by the macOS framework probe, such
    /// as `H.264 (HW)` or `HEVC (HW)`.
    pub videotoolbox_encoders: BTreeSet<String>,
}

impl FfmpegCapabilityReport {
    /// Returns whether a named filter is present.
    #[must_use]
    pub fn has_filter(&self, name: &str) -> bool {
        self.filters.contains(name)
    }

    /// Returns whether a named encoder is present.
    #[must_use]
    pub fn has_encoder(&self, name: &str) -> bool {
        self.encoders.contains(name)
    }

    /// Returns whether a named decoder is present.
    #[must_use]
    pub fn has_decoder(&self, name: &str) -> bool {
        self.decoders.contains(name)
    }

    /// Returns whether a named bitstream filter is present.
    #[must_use]
    pub fn has_bitstream_filter(&self, name: &str) -> bool {
        self.bitstream_filters.contains(name)
    }

    /// Returns whether a named hardware accelerator is present.
    #[must_use]
    pub fn has_hwaccel(&self, name: &str) -> bool {
        self.hwaccels.contains(name)
    }

    /// Returns whether a configure flag is present.
    #[must_use]
    pub fn has_configuration_flag(&self, flag: &str) -> bool {
        self.configuration_flags.contains(flag)
    }

    /// Return whether runtime evidence has denied a hardware decode path.
    #[must_use]
    pub fn hardware_decode_denied(
        &self,
        acceleration: HardwareAccelerationKind,
        codec: &str,
        bit_depth: u8,
    ) -> bool {
        self.hardware_decode_denials
            .iter()
            .any(|denial| denial.matches(acceleration, codec, bit_depth))
    }

    /// Add a runtime-learned denial row if it is not already present.
    pub fn deny_hardware_decode(&mut self, denial: HardwareDecodeDenial) {
        if !self
            .hardware_decode_denials
            .iter()
            .any(|existing| existing.matches(denial.acceleration, &denial.codec, denial.bit_depth))
        {
            self.hardware_decode_denials.push(denial);
        }
    }

    /// The device identity used to key hardware-graph denials.
    ///
    /// `nvidia_gpu_list` holds the *raw* probe output, which is the entire
    /// `ffmpeg` banner plus the encoder trace — unusable as a cache key and
    /// unreadable in a health message. This extracts the model name, such as
    /// `NVIDIA GeForce RTX 3080 Ti`, and returns an empty string when the
    /// runtime reported no usable device line.
    #[must_use]
    pub fn hardware_graph_device(&self) -> String {
        if let Some(device) = &self.selected_nvidia_device {
            return device.uuid.as_ref().map_or_else(
                || format!("cuda:{}:{}", device.index, device.model),
                |uuid| format!("cuda:{}:{uuid}", device.index),
            );
        }
        self.nvidia_gpu_list
            .as_deref()
            .and_then(crate::capabilities::parse_nvidia_gpu_info)
            .map(|info| info.model)
            .filter(|model| model != "unknown")
            .unwrap_or_default()
    }

    /// Return whether runtime evidence has denied a hardware graph operation.
    ///
    /// `device` is the device identity the caller is about to run on; pass an
    /// empty string when it is unknown, which makes any denial for the
    /// operation apply.
    #[must_use]
    pub fn hardware_graph_denied(
        &self,
        operation: crate::graph_probe::HardwareGraphOperation,
        device: &str,
    ) -> bool {
        self.hardware_graph_denials
            .iter()
            .any(|denial| denial.matches(operation, device))
    }

    /// Record that a functional graph probe was attempted on this device.
    pub fn record_hardware_graph_probe_attempt(
        &mut self,
        operation: crate::graph_probe::HardwareGraphOperation,
        device: &str,
    ) {
        if !self
            .attempted_hardware_graphs
            .iter()
            .any(|(attempted, attempted_device)| {
                *attempted == operation && attempted_device == device
            })
        {
            self.attempted_hardware_graphs
                .push((operation, device.to_owned()));
        }
    }

    /// Whether this graph's functional probe was attempted on this device.
    #[must_use]
    pub fn hardware_graph_probe_attempted(
        &self,
        operation: crate::graph_probe::HardwareGraphOperation,
        device: &str,
    ) -> bool {
        self.attempted_hardware_graphs
            .iter()
            .any(|(attempted, attempted_device)| {
                *attempted == operation
                    && (attempted_device.is_empty()
                        || device.is_empty()
                        || attempted_device.eq_ignore_ascii_case(device))
            })
    }

    /// Record that a graph operation demonstrably ran on this device.
    ///
    /// Positive evidence is tracked explicitly rather than inferred from the
    /// absence of a denial, so an unclassified probe failure cannot leave a
    /// broken graph looking selectable.
    pub fn prove_hardware_graph(
        &mut self,
        operation: crate::graph_probe::HardwareGraphOperation,
        device: &str,
    ) {
        self.record_hardware_graph_probe_attempt(operation, device);
        if !self
            .proven_hardware_graphs
            .iter()
            .any(|(proven, proven_device)| *proven == operation && proven_device == device)
        {
            self.proven_hardware_graphs
                .push((operation, device.to_owned()));
        }
    }

    /// Whether runtime evidence positively proved this graph on this device.
    #[must_use]
    pub fn hardware_graph_proven(
        &self,
        operation: crate::graph_probe::HardwareGraphOperation,
        device: &str,
    ) -> bool {
        self.proven_hardware_graphs
            .iter()
            .any(|(proven, proven_device)| {
                *proven == operation
                    && (proven_device.is_empty()
                        || device.is_empty()
                        || proven_device.eq_ignore_ascii_case(device))
            })
    }

    /// Whether this runtime has run the functional graph probes at all.
    ///
    /// Distinguishes "probed and found nothing wrong" from "never probed", so a
    /// database written before graph probing existed is not treated as evidence
    /// that every graph failed.
    #[must_use]
    pub fn has_probed_hardware_graphs(&self) -> bool {
        !self.attempted_hardware_graphs.is_empty()
            || !self.proven_hardware_graphs.is_empty()
            || !self.hardware_graph_denials.is_empty()
    }

    /// Add a runtime-learned graph denial row if it is not already present.
    ///
    /// Also drops any earlier proof of the same operation on the same device:
    /// a graph cannot be simultaneously proven and denied, and the denial is
    /// the newer evidence — a startup probe can pass while the graph still
    /// fails under real media.
    pub fn deny_hardware_graph(&mut self, denial: crate::graph_probe::HardwareGraphDenial) {
        self.record_hardware_graph_probe_attempt(denial.operation, &denial.device);
        self.proven_hardware_graphs.retain(|(operation, device)| {
            !(*operation == denial.operation
                && (denial.device.is_empty()
                    || device.is_empty()
                    || denial.device.eq_ignore_ascii_case(device)))
        });
        if !self
            .hardware_graph_denials
            .iter()
            .any(|existing| existing.matches(denial.operation, &denial.device))
        {
            self.hardware_graph_denials.push(denial);
        }
    }
}

#[cfg(test)]
mod hardware_graph_device_tests {
    use super::FfmpegCapabilityReport;

    #[test]
    fn device_identity_is_the_gpu_model_not_the_raw_probe_blob() {
        // `nvidia_gpu_list` holds the whole ffmpeg banner plus the encoder
        // trace. Using it verbatim made the health message unreadable and the
        // denial cache key useless.
        let report = FfmpegCapabilityReport {
            nvidia_gpu_list: Some(
                "ffmpeg version n9.0-20260805\n  configuration: --enable-gpl\n\
                 [h264_nvenc @ 0x1] [ GPU #0 - < NVIDIA GeForce RTX 3080 Ti > has Compute SM 8.6 ]\n\
                 frame=    0 fps=0.0"
                    .to_owned(),
            ),
            ..FfmpegCapabilityReport::default()
        };

        assert_eq!(report.hardware_graph_device(), "NVIDIA GeForce RTX 3080 Ti");
    }

    #[test]
    fn an_absent_or_unparsable_device_is_empty_not_garbage() {
        assert_eq!(
            FfmpegCapabilityReport::default().hardware_graph_device(),
            ""
        );

        for gpu_list in [
            "no gpu line here",
            // A device line with no `< model >` segment parses as "unknown",
            // which is not a usable identity either.
            "[ GPU #0 - has Compute SM 8.6 ]",
        ] {
            let report = FfmpegCapabilityReport {
                nvidia_gpu_list: Some(gpu_list.to_owned()),
                ..FfmpegCapabilityReport::default()
            };
            assert_eq!(report.hardware_graph_device(), "", "{gpu_list}");
        }
    }
}

/// Select the ADR 0017 seed-time hardware acceleration preference from a
/// runtime capability report.
#[must_use]
pub fn preferred_default_profile_acceleration(
    report: &FfmpegCapabilityReport,
) -> HardwareAccelerationKind {
    if report.has_hwaccel("cuda") && has_encoder_suffix(&report.encoders, "_nvenc") {
        HardwareAccelerationKind::Nvenc
    } else if report.has_hwaccel("qsv") && has_encoder_suffix(&report.encoders, "_qsv") {
        HardwareAccelerationKind::Qsv
    } else if report.has_hwaccel("vaapi") && has_encoder_suffix(&report.encoders, "_vaapi") {
        HardwareAccelerationKind::Vaapi
    } else if report.has_hwaccel("videotoolbox")
        && (has_encoder_suffix(&report.encoders, "_videotoolbox")
            || report
                .videotoolbox_encoders
                .iter()
                .any(|encoder| encoder.contains("(HW)")))
    {
        HardwareAccelerationKind::VideoToolbox
    } else if has_encoder_suffix(&report.encoders, "_amf") {
        HardwareAccelerationKind::Amf
    } else {
        HardwareAccelerationKind::None
    }
}

/// One VAAPI-style profile/entrypoint row reported by a hardware device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaapiProfileEntrypoint {
    /// Profile name, such as `VAProfileH264High` or `VAProfileHEVCMain10`.
    pub profile: String,
    /// Entrypoint name, such as `VAEntrypointVLD`, `VAEntrypointEncSlice`, or
    /// `VAEntrypointEncSliceLP`.
    pub entrypoint: String,
    /// Rate-control modes reported for this profile/entrypoint row.
    pub rate_control_modes: BTreeSet<String>,
    /// Whether the row reports `VA_ENC_PACKED_HEADER_MISC`.
    pub packed_header_misc: bool,
}

/// `FFmpeg`-reported details for one NVIDIA encoder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NvidiaEncoderCapability {
    /// Encoder name, such as `h264_nvenc`.
    pub encoder: String,
    /// Pixel formats accepted by the encoder.
    pub pixel_formats: BTreeSet<String>,
    /// Profile option names accepted by the encoder.
    pub profiles: BTreeSet<String>,
    /// Preset names accepted by the encoder.
    pub presets: BTreeSet<String>,
    /// Tune names accepted by the encoder.
    pub tunes: BTreeSet<String>,
    /// Whether the hardware encoder accepted B-frame reference mode in a safe
    /// encode probe.
    pub supports_b_frames: bool,
}

impl NvidiaEncoderCapability {
    /// Create an NVIDIA encoder detail row.
    #[must_use]
    pub fn new(
        encoder: impl Into<String>,
        pixel_formats: BTreeSet<String>,
        profiles: BTreeSet<String>,
    ) -> Self {
        Self {
            encoder: encoder.into(),
            pixel_formats,
            profiles,
            presets: BTreeSet::new(),
            tunes: BTreeSet::new(),
            supports_b_frames: false,
        }
    }

    #[must_use]
    pub fn with_b_frames(mut self) -> Self {
        self.supports_b_frames = true;
        self
    }

    #[must_use]
    pub fn with_options(mut self, presets: BTreeSet<String>, tunes: BTreeSet<String>) -> Self {
        self.presets = presets;
        self.tunes = tunes;
        self
    }

    #[must_use]
    pub fn encoder_matches(&self, expected: &str) -> bool {
        self.encoder.eq_ignore_ascii_case(expected)
    }

    #[must_use]
    pub fn has_pixel_format(&self, expected: &str) -> bool {
        self.pixel_formats
            .iter()
            .any(|value| value.eq_ignore_ascii_case(expected))
    }

    #[must_use]
    pub fn has_profile(&self, expected: &str) -> bool {
        self.profiles
            .iter()
            .any(|value| value.eq_ignore_ascii_case(expected))
    }

    /// Returns whether the encoder advertises a preset.
    #[must_use]
    pub fn has_preset(&self, expected: &str) -> bool {
        self.presets
            .iter()
            .any(|value| value.eq_ignore_ascii_case(expected))
    }

    /// Returns whether the encoder advertises a tune.
    #[must_use]
    pub fn has_tune(&self, expected: &str) -> bool {
        self.tunes
            .iter()
            .any(|value| value.eq_ignore_ascii_case(expected))
    }
}

/// Native CUDA/NVDEC support for one source codec and bit depth.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NvidiaDecoderCapability {
    /// Source codec name, such as `h264`, `hevc`, or `av1`.
    pub codec: String,
    /// Supported decoded bit depth.
    pub bit_depth: u8,
}

/// Runtime-learned negative decode support for one accelerator/codec/depth.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HardwareDecodeDenial {
    /// Hardware acceleration kind that failed at runtime.
    pub acceleration: HardwareAccelerationKind,
    /// Source codec name, such as `av1`.
    pub codec: String,
    /// Source bit depth.
    pub bit_depth: u8,
}

impl HardwareDecodeDenial {
    /// Create a hardware decode denial row.
    #[must_use]
    pub fn new(
        acceleration: HardwareAccelerationKind,
        codec: impl Into<String>,
        bit_depth: u8,
    ) -> Self {
        Self {
            acceleration,
            codec: codec.into(),
            bit_depth,
        }
    }

    /// Return whether this denial applies to a decode request.
    #[must_use]
    pub fn matches(
        &self,
        acceleration: HardwareAccelerationKind,
        codec: &str,
        bit_depth: u8,
    ) -> bool {
        self.acceleration == acceleration
            && self.bit_depth == bit_depth
            && self.codec.eq_ignore_ascii_case(codec)
    }
}

impl NvidiaDecoderCapability {
    /// Create an NVIDIA decoder detail row.
    #[must_use]
    pub fn new(codec: impl Into<String>, bit_depth: u8) -> Self {
        Self {
            codec: codec.into(),
            bit_depth,
        }
    }

    #[must_use]
    pub fn supports(&self, codec: &str, bit_depth: u8) -> bool {
        self.bit_depth == bit_depth && self.codec.eq_ignore_ascii_case(codec)
    }
}

impl VaapiProfileEntrypoint {
    /// Create a profile/entrypoint row from raw probe text.
    #[must_use]
    pub fn new(profile: impl Into<String>, entrypoint: impl Into<String>) -> Self {
        Self {
            profile: profile.into(),
            entrypoint: entrypoint.into(),
            rate_control_modes: BTreeSet::new(),
            packed_header_misc: false,
        }
    }

    /// Return a copy with one VAAPI rate-control mode attached.
    #[must_use]
    pub fn with_rate_control_mode(mut self, mode: impl Into<String>) -> Self {
        self.rate_control_modes.insert(mode.into());
        self
    }

    /// Return a copy with packed-header misc support attached.
    #[must_use]
    pub fn with_packed_header_misc(mut self) -> Self {
        self.packed_header_misc = true;
        self
    }

    #[must_use]
    pub fn is_decode(&self) -> bool {
        self.entrypoint.eq_ignore_ascii_case("VAEntrypointVLD")
            || self.entrypoint.eq_ignore_ascii_case("Decode")
    }

    #[must_use]
    pub fn is_encode(&self) -> bool {
        matches!(
            self.entrypoint.to_ascii_lowercase().as_str(),
            "vaentrypointencslice" | "vaentrypointencslicelp" | "encode" | "encodelowpower"
        )
    }

    #[must_use]
    pub fn profile_matches(&self, expected: &str) -> bool {
        self.profile.eq_ignore_ascii_case(expected)
    }

    #[must_use]
    pub fn has_rate_control_mode(&self, expected: &str) -> bool {
        self.rate_control_modes
            .iter()
            .any(|mode| mode.eq_ignore_ascii_case(expected))
    }
}

/// Non-fatal or fatal capability issue discovered after both tools are present.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FfmpegRuntimeIssue {
    /// Required filter is missing.
    RequiredFilter { name: String },
    /// Required encoder is missing.
    RequiredEncoder { name: String },
    /// Required decoder is missing.
    RequiredDecoder { name: String },
    /// Required output container is missing.
    RequiredMuxer { name: String },
    /// Required input container is missing.
    RequiredDemuxer { name: String },
    /// Required configure flag is missing.
    RequiredConfigurationFlag { flag: String },
    /// Required hardware accelerator is missing.
    RequiredHwaccel { name: String },
    /// Optional hardware accelerator is missing.
    OptionalHwaccel { name: String },
    /// A required encoder-specific option value is missing.
    RequiredEncoderOption { encoder: String, option: String },
}

impl FfmpegRuntimeIssue {
    /// Returns true when the issue should fail startup.
    #[must_use]
    pub const fn is_error(&self) -> bool {
        !matches!(self, Self::OptionalHwaccel { .. })
    }
}

/// Full `FFmpeg` runtime report used by startup, health, and the admin UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FfmpegRuntimeReport {
    /// Monotonic publication generation. A settings refresh increments this
    /// value only when the complete candidate runtime is ready to publish.
    pub generation: u64,
    /// `ffmpeg` version details.
    pub ffmpeg: FfmpegToolVersion,
    /// `ffprobe` version details.
    pub ffprobe: FfmpegToolVersion,
    /// Capabilities reported by `ffmpeg`.
    pub capabilities: FfmpegCapabilityReport,
    /// Capability issues found during the runtime check.
    pub issues: Vec<FfmpegRuntimeIssue>,
    /// How the accepted FFmpeg/ffprobe runtime was resolved or acquired.
    pub acquisition: FfmpegRuntimeAcquisition,
}

impl FfmpegRuntimeReport {
    /// Assign the generation used for an atomic runtime publication.
    pub fn set_generation(&mut self, generation: u64) {
        self.generation = generation;
        self.capabilities.runtime_identity = Some(FfmpegRuntimeIdentity {
            generation,
            ffmpeg_path: self.ffmpeg.path.clone(),
            ffmpeg_sha256: self.ffmpeg.sha256.clone(),
            ffprobe_path: self.ffprobe.path.clone(),
            ffprobe_sha256: self.ffprobe.sha256.clone(),
            selected_nvidia_device: self.capabilities.selected_nvidia_device.clone(),
        });
    }

    /// Returns true when the report contains any fatal issue.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.issues.iter().any(FfmpegRuntimeIssue::is_error)
    }

    /// Count fatal and warning issues.
    #[must_use]
    pub fn issue_counts(&self) -> (u32, u32) {
        let errors = self
            .issues
            .iter()
            .filter(|issue| issue.is_error())
            .count()
            .try_into()
            .unwrap_or(u32::MAX);
        let warnings = self
            .issues
            .iter()
            .filter(|issue| !issue.is_error())
            .count()
            .try_into()
            .unwrap_or(u32::MAX);
        (errors, warnings)
    }

    /// Minimal report used by tests that do not need to spawn `FFmpeg`.
    #[must_use]
    pub fn for_tests() -> Self {
        let version = FfmpegVersion {
            major: REQUIRED_MAJOR,
            minor: REQUIRED_MINOR,
            patch: 0,
        };
        let flags = BTreeSet::from_iter(to_owned_vec(REQUIRED_CONFIGURATION_FLAGS));
        let capabilities = FfmpegCapabilityReport {
            runtime_identity: None,
            filters: BTreeSet::from_iter(to_owned_vec(REQUIRED_FILTERS)),
            encoders: BTreeSet::from_iter(to_owned_vec(REQUIRED_ENCODERS)),
            decoders: BTreeSet::from_iter(to_owned_vec(REQUIRED_DECODERS)),
            muxers: BTreeSet::from_iter(to_owned_vec(REQUIRED_MUXERS)),
            demuxers: BTreeSet::from_iter(to_owned_vec(REQUIRED_DEMUXERS)),
            bitstream_filters: BTreeSet::from(["dovi_split".to_owned()]),
            hwaccels: BTreeSet::new(),
            configuration_flags: flags.clone(),
            vaapi_profiles: Vec::new(),
            qsv_profiles: Vec::new(),
            nvidia_gpu_list: None,
            selected_nvidia_device: None,
            nvidia_encoders: Vec::new(),
            nvidia_decoders: Vec::new(),
            hardware_decode_denials: Vec::new(),
            hardware_graph_denials: Vec::new(),
            proven_hardware_graphs: Vec::new(),
            attempted_hardware_graphs: Vec::new(),
            videotoolbox_decoders: BTreeSet::new(),
            videotoolbox_encoders: BTreeSet::new(),
        };
        Self {
            generation: 0,
            ffmpeg: FfmpegToolVersion {
                tool: "ffmpeg",
                path: DEFAULT_FFMPEG_PATH.to_owned(),
                version: version.clone(),
                configuration_flags: flags.clone(),
                sha256: None,
            },
            ffprobe: FfmpegToolVersion {
                tool: "ffprobe",
                path: DEFAULT_FFPROBE_PATH.to_owned(),
                version,
                configuration_flags: flags,
                sha256: None,
            },
            capabilities,
            issues: Vec::new(),
            acquisition: FfmpegRuntimeAcquisition::default(),
        }
    }
}

/// Resolve the `ffmpeg` path from supported environment variables.
#[must_use]
pub fn ffmpeg_path_from_env() -> String {
    path_from_env(FFMPEG_PATH_ENV_VARS, DEFAULT_FFMPEG_PATH)
}

/// Resolve the `ffprobe` path from supported environment variables.
#[must_use]
pub fn ffprobe_path_from_env() -> String {
    path_from_env(FFPROBE_PATH_ENV_VARS, DEFAULT_FFPROBE_PATH)
}

fn path_from_env(keys: &[&str], default: &str) -> String {
    crate::locator::first_non_empty_env(keys).unwrap_or_else(|| default.to_owned())
}

/// Parse the version triple from the first line of `ffmpeg -version` or
/// `ffprobe -version` output.
///
/// # Errors
///
/// Returns [`FfmpegCheckError::ParseError`] when the line does not contain a
/// parseable major/minor version.
pub fn parse_version(first_line: &str) -> Result<FfmpegVersion, FfmpegCheckError> {
    parse_version_for_tool("ffmpeg", first_line)
}

fn parse_version_for_tool(
    tool: &'static str,
    first_line: &str,
) -> Result<FfmpegVersion, FfmpegCheckError> {
    let token =
        first_line
            .split_whitespace()
            .nth(2)
            .ok_or_else(|| FfmpegCheckError::ParseError {
                tool,
                output: first_line.to_owned(),
            })?;
    let token = token.strip_prefix('n').unwrap_or(token);
    let numeric = token.split('-').next().unwrap_or(token);

    let mut parts = numeric.splitn(3, '.');
    let major = parse_component(tool, parts.next(), first_line)?;
    let minor = parse_component(tool, parts.next(), first_line)?;
    let patch = parts
        .next()
        .map(|part| {
            part.parse::<u32>()
                .map_err(|_| FfmpegCheckError::ParseError {
                    tool,
                    output: first_line.to_owned(),
                })
        })
        .transpose()?
        .unwrap_or(0);

    Ok(FfmpegVersion {
        major,
        minor,
        patch,
    })
}

fn parse_component(
    tool: &'static str,
    part: Option<&str>,
    line: &str,
) -> Result<u32, FfmpegCheckError> {
    part.and_then(|value| value.parse::<u32>().ok())
        .ok_or_else(|| FfmpegCheckError::ParseError {
            tool,
            output: line.to_owned(),
        })
}

/// Parse configure flags from an FFmpeg-family version banner.
#[must_use]
pub fn parse_configuration_flags(output: &str) -> BTreeSet<String> {
    let mut flags: BTreeSet<String> = output
        .lines()
        .find_map(|line| line.trim().strip_prefix("configuration:"))
        .map(|line| {
            line.split_whitespace()
                .filter(|token| token.starts_with("--"))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();

    if flags.contains("--enable-fontconfig") {
        flags.insert("--enable-libfontconfig".to_owned());
    }
    if flags.contains("--enable-libfontconfig") {
        flags.insert("--enable-fontconfig".to_owned());
    }

    flags
}

/// Parse names from `ffmpeg -filters`, `ffmpeg -encoders`, or
/// `ffmpeg -decoders` output.
#[must_use]
pub fn parse_capability_names(output: &str) -> BTreeSet<String> {
    output
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let flags = parts.next()?;
            let name = parts.next()?;
            let is_capability_line = flags.len() >= 2
                && flags
                    .chars()
                    .all(|ch| ch == '.' || ch.is_ascii_alphabetic());
            (is_capability_line && name != "=").then(|| name.to_owned())
        })
        .collect()
}

/// Parse input and output support separately; a demuxer alone cannot produce a canvas.
fn parse_container_formats(output: &str) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut muxers = BTreeSet::new();
    let mut demuxers = BTreeSet::new();
    for line in output.lines() {
        let mut parts = line.split_whitespace();
        let (Some(flags), Some(names)) = (parts.next(), parts.next()) else {
            continue;
        };
        if names == "="
            || !flags
                .bytes()
                .all(|byte| matches!(byte, b'D' | b'E' | b'd' | b'.'))
        {
            continue;
        }
        for name in names.split(',').filter(|name| !name.is_empty()) {
            if flags.contains('E') {
                muxers.insert(name.to_owned());
            }
            if flags.contains('D') {
                demuxers.insert(name.to_owned());
            }
        }
    }
    (muxers, demuxers)
}

/// Parse names from `ffmpeg -hwaccels` output.
#[must_use]
pub fn parse_hwaccels(output: &str) -> BTreeSet<String> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("Hardware acceleration methods"))
        .map(str::to_owned)
        .collect()
}

/// Parse names from `ffmpeg -bsfs` output.
#[must_use]
pub fn parse_bitstream_filters(output: &str) -> BTreeSet<String> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| {
            !line.is_empty()
                && !line.starts_with("Bitstream filters:")
                && !line.chars().any(char::is_whitespace)
        })
        .map(str::to_owned)
        .collect()
}

/// Spawn `ffmpeg -version`, parse the version, and enforce the supported line.
///
/// # Errors
///
/// Returns an error if `ffmpeg` cannot be spawned, prints unparsable output, or
/// reports a version outside the supported `FFmpeg` 9 major.
pub async fn check_ffmpeg_version() -> Result<FfmpegVersion, FfmpegCheckError> {
    let config = FfmpegRuntimeCheckConfig::from_env();
    Ok(check_tool_version(
        "ffmpeg",
        &config.ffmpeg_path,
        config.required_major,
        config.required_minor,
    )
    .await?
    .version)
}

/// Check the local `FFmpeg` runtime using environment-derived paths.
///
/// # Errors
///
/// Returns an error when `ffmpeg` or `ffprobe` is missing, too old, exits
/// unsuccessfully, prints unparsable version output, or lacks required
/// capabilities.
pub async fn check_ffmpeg_runtime() -> Result<FfmpegRuntimeReport, FfmpegCheckError> {
    check_ffmpeg_runtime_with_config(&FfmpegRuntimeCheckConfig::from_env()).await
}

/// Check the local `FFmpeg` runtime using an explicit config.
///
/// # Errors
///
/// Returns an error when `ffmpeg` or `ffprobe` is missing, too old, exits
/// unsuccessfully, prints unparsable version output, or lacks required
/// capabilities.
pub async fn check_ffmpeg_runtime_with_config(
    config: &FfmpegRuntimeCheckConfig,
) -> Result<FfmpegRuntimeReport, FfmpegCheckError> {
    let ffmpeg = check_tool_version(
        "ffmpeg",
        &config.ffmpeg_path,
        config.required_major,
        config.required_minor,
    )
    .await?;
    let ffprobe = check_tool_version(
        "ffprobe",
        &config.ffprobe_path,
        config.required_major,
        config.required_minor,
    )
    .await?;
    check_ffprobe_json(&config.ffprobe_path).await?;

    let mut capabilities = collect_ffmpeg_capabilities(config).await?;
    capabilities
        .configuration_flags
        .clone_from(&ffmpeg.configuration_flags);
    capabilities.runtime_identity = Some(FfmpegRuntimeIdentity {
        generation: 0,
        ffmpeg_path: ffmpeg.path.clone(),
        ffmpeg_sha256: ffmpeg.sha256.clone(),
        ffprobe_path: ffprobe.path.clone(),
        ffprobe_sha256: ffprobe.sha256.clone(),
        selected_nvidia_device: capabilities.selected_nvidia_device.clone(),
    });

    let issues = collect_runtime_issues(config, &capabilities);
    let report = FfmpegRuntimeReport {
        generation: 0,
        ffmpeg,
        ffprobe,
        capabilities,
        issues,
        acquisition: FfmpegRuntimeAcquisition::default(),
    };

    if report.has_errors() {
        return Err(FfmpegCheckError::RuntimeIssues(
            report
                .issues
                .iter()
                .filter(|issue| issue.is_error())
                .cloned()
                .collect(),
        ));
    }

    run_required_smoke_transcodes(config).await?;

    Ok(report)
}

async fn run_required_smoke_transcodes(
    config: &FfmpegRuntimeCheckConfig,
) -> Result<(), FfmpegCheckError> {
    run_tool(
        "ffmpeg",
        &config.ffmpeg_path,
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=32x32:d=0.1",
            "-frames:v",
            "1",
            "-fps_mode:v",
            "cfr",
            "-f",
            "null",
            "-",
        ],
    )
    .await?;

    if config
        .required_filters
        .iter()
        .any(|filter| filter == "drawvg")
    {
        run_tool(
            "ffmpeg",
            &config.ffmpeg_path,
            &[
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=c=black:s=32x32:d=0.1",
                "-vf",
                "drawvg=script='rect 4 4 16 16 fill',format=yuv420p",
                "-frames:v",
                "1",
                "-f",
                "null",
                "-",
            ],
        )
        .await?;
    }

    if config
        .required_filters
        .iter()
        .any(|filter| filter == "zscale")
    {
        run_tool(
            "ffmpeg",
            &config.ffmpeg_path,
            &[
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=c=black:s=64x64:d=0.1,format=yuv420p10le,setparams=color_primaries=bt2020:color_trc=smpte2084:colorspace=bt2020nc",
                "-vf",
                "zscale=t=linear:npl=100,tonemap=hable,zscale=p=bt709:t=bt709:m=bt709,format=yuv420p",
                "-frames:v",
                "1",
                "-f",
                "null",
                "-",
            ],
        )
        .await?;
    }

    Ok(())
}

async fn check_tool_version(
    tool: &'static str,
    path: &str,
    required_major: u32,
    required_minor: u32,
) -> Result<FfmpegToolVersion, FfmpegCheckError> {
    let output = run_tool(tool, path, &["-version"]).await?;
    let stdout = String::from_utf8(output).map_err(|error| FfmpegCheckError::ParseError {
        tool,
        output: error.to_string(),
    })?;
    let first_line = stdout
        .lines()
        .next()
        .ok_or_else(|| FfmpegCheckError::ParseError {
            tool,
            output: String::new(),
        })?;

    let version = parse_version_for_tool(tool, first_line)?;
    if version.major < required_major
        || version.major == required_major && version.minor < required_minor
    {
        return Err(FfmpegCheckError::TooOld {
            tool,
            found: version,
            required_major,
            required_minor,
        });
    }
    if version.major > required_major {
        return Err(FfmpegCheckError::UnsupportedMajor {
            tool,
            found: version,
            required_major,
            required_minor,
        });
    }

    Ok(FfmpegToolVersion {
        tool,
        path: path.to_owned(),
        version,
        configuration_flags: parse_configuration_flags(&stdout),
        sha256: sha256_file(path).await,
    })
}

async fn sha256_file(path: &str) -> Option<String> {
    let mut file = tokio::fs::File::open(path).await.ok()?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1_024];
    loop {
        let count = file.read(&mut buffer).await.ok()?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    let mut digest = String::with_capacity(64);
    for byte in hasher.finalize() {
        write!(digest, "{byte:02x}").ok()?;
    }
    Some(digest)
}

async fn collect_ffmpeg_capabilities(
    config: &FfmpegRuntimeCheckConfig,
) -> Result<FfmpegCapabilityReport, FfmpegCheckError> {
    let filters = String::from_utf8(
        run_tool("ffmpeg", &config.ffmpeg_path, &["-hide_banner", "-filters"]).await?,
    )
    .map_err(|error| FfmpegCheckError::ParseError {
        tool: "ffmpeg",
        output: error.to_string(),
    })?;
    let encoders = String::from_utf8(
        run_tool(
            "ffmpeg",
            &config.ffmpeg_path,
            &["-hide_banner", "-encoders"],
        )
        .await?,
    )
    .map_err(|error| FfmpegCheckError::ParseError {
        tool: "ffmpeg",
        output: error.to_string(),
    })?;
    let decoders = String::from_utf8(
        run_tool(
            "ffmpeg",
            &config.ffmpeg_path,
            &["-hide_banner", "-decoders"],
        )
        .await?,
    )
    .map_err(|error| FfmpegCheckError::ParseError {
        tool: "ffmpeg",
        output: error.to_string(),
    })?;
    let bitstream_filters = String::from_utf8(
        run_tool("ffmpeg", &config.ffmpeg_path, &["-hide_banner", "-bsfs"]).await?,
    )
    .map_err(|error| FfmpegCheckError::ParseError {
        tool: "ffmpeg",
        output: error.to_string(),
    })?;
    let hwaccels = String::from_utf8(
        run_tool(
            "ffmpeg",
            &config.ffmpeg_path,
            &["-hide_banner", "-hwaccels"],
        )
        .await?,
    )
    .map_err(|error| FfmpegCheckError::ParseError {
        tool: "ffmpeg",
        output: error.to_string(),
    })?;

    let formats = String::from_utf8(
        run_tool("ffmpeg", &config.ffmpeg_path, &["-hide_banner", "-formats"]).await?,
    )
    .map_err(|error| FfmpegCheckError::ParseError {
        tool: "ffmpeg",
        output: error.to_string(),
    })?;
    let (muxers, demuxers) = parse_container_formats(&formats);

    let mut report = FfmpegCapabilityReport {
        runtime_identity: None,
        filters: parse_capability_names(&filters),
        encoders: parse_capability_names(&encoders),
        decoders: parse_capability_names(&decoders),
        muxers,
        demuxers,
        bitstream_filters: parse_bitstream_filters(&bitstream_filters),
        hwaccels: parse_hwaccels(&hwaccels),
        configuration_flags: BTreeSet::new(),
        vaapi_profiles: Vec::new(),
        qsv_profiles: Vec::new(),
        nvidia_gpu_list: None,
        selected_nvidia_device: None,
        nvidia_encoders: Vec::new(),
        nvidia_decoders: Vec::new(),
        hardware_decode_denials: Vec::new(),
        hardware_graph_denials: Vec::new(),
        proven_hardware_graphs: Vec::new(),
        attempted_hardware_graphs: Vec::new(),
        videotoolbox_decoders: BTreeSet::new(),
        videotoolbox_encoders: BTreeSet::new(),
    };

    enrich_device_profiles(config, &mut report).await;

    Ok(report)
}

async fn enrich_device_profiles(
    config: &FfmpegRuntimeCheckConfig,
    report: &mut FfmpegCapabilityReport,
) {
    if report.has_hwaccel("vaapi") || has_encoder_suffix(&report.encoders, "_vaapi") {
        report.vaapi_profiles = probe_all_vainfo_profiles(&config.hardware_probe_devices).await;
    }

    if report.has_hwaccel("qsv") || has_encoder_suffix(&report.encoders, "_qsv") {
        report.qsv_profiles = probe_all_qsv_profiles(config).await;
    }

    if report.has_hwaccel("cuda") || has_encoder_suffix(&report.encoders, "_nvenc") {
        let nvidia = ersatzrs_hwprobe::probe_nvidia();
        let nvidia_attempted = nvidia.attempted;
        report
            .selected_nvidia_device
            .clone_from(&nvidia.selected_device);
        if nvidia_attempted {
            report.nvidia_gpu_list = match nvidia.device_details {
                Some(details) => Some(details),
                None => probe_nvidia_gpu_list(&config.ffmpeg_path).await,
            };
        } else {
            report.nvidia_gpu_list = probe_nvidia_gpu_list(&config.ffmpeg_path).await;
        }
        if nvidia.encoders_attempted {
            report.nvidia_encoders = nvidia
                .encoders
                .into_iter()
                .map(|encoder| {
                    let capability = NvidiaEncoderCapability::new(
                        encoder.encoder,
                        encoder.pixel_formats,
                        encoder.profiles,
                    );
                    if encoder.supports_b_frames {
                        capability.with_b_frames()
                    } else {
                        capability
                    }
                })
                .collect();
            let cli_capabilities = probe_nvidia_encoder_capabilities(config, report).await;
            for capability in &mut report.nvidia_encoders {
                if let Some(cli) = cli_capabilities
                    .iter()
                    .find(|cli| cli.encoder_matches(&capability.encoder))
                {
                    capability.presets.clone_from(&cli.presets);
                    capability.tunes.clone_from(&cli.tunes);
                }
            }
        } else {
            report.nvidia_encoders = probe_nvidia_encoder_capabilities(config, report).await;
        }
        if nvidia_attempted {
            report.nvidia_decoders = nvidia
                .decoders
                .into_iter()
                .map(|decoder| NvidiaDecoderCapability::new(decoder.codec, decoder.bit_depth))
                .collect();
        } else {
            report.nvidia_decoders = probe_nvidia_decoder_capabilities(config, report).await;
        }
        if !nvidia_attempted && report.nvidia_decoders.is_empty() {
            report.nvidia_decoders = report
                .nvidia_gpu_list
                .as_deref()
                .map(parse_nvidia_decoder_capabilities)
                .unwrap_or_default();
        }
    }

    if report.has_hwaccel("videotoolbox") || has_encoder_suffix(&report.encoders, "_videotoolbox") {
        let videotoolbox =
            probe_videotoolbox_framework_capabilities(&config.ffmpeg_path, report).await;
        report.videotoolbox_decoders = videotoolbox.decoders;
        report.videotoolbox_encoders = videotoolbox.encoders;
    }
}

fn has_encoder_suffix(encoders: &BTreeSet<String>, suffix: &str) -> bool {
    encoders.iter().any(|name| name.ends_with(suffix))
}

async fn probe_all_qsv_profiles(config: &FfmpegRuntimeCheckConfig) -> Vec<VaapiProfileEntrypoint> {
    if config.hardware_probe_devices.is_empty() {
        return probe_qsv_profiles(&config.ffmpeg_path, None).await;
    }

    let mut profiles = Vec::new();
    for device in &config.hardware_probe_devices {
        profiles.extend(probe_qsv_profiles(&config.ffmpeg_path, Some(device)).await);
    }
    dedupe_vaapi_profiles(profiles)
}

async fn probe_nvidia_gpu_list(ffmpeg_path: &str) -> Option<String> {
    if cfg!(target_os = "macos") {
        return None;
    }

    let output = tokio::process::Command::new(ffmpeg_path)
        .args([
            "-f",
            "lavfi",
            "-i",
            "nullsrc",
            "-c:v",
            "h264_nvenc",
            "-gpu",
            "list",
            "-f",
            "null",
            "-",
        ])
        .output()
        .await
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let text = if stdout.trim().is_empty() {
        stderr.trim()
    } else {
        stdout.trim()
    };
    (!text.is_empty()).then(|| text.to_owned())
}

async fn probe_nvidia_encoder_capabilities(
    config: &FfmpegRuntimeCheckConfig,
    report: &FfmpegCapabilityReport,
) -> Vec<NvidiaEncoderCapability> {
    let mut capabilities = Vec::new();
    for encoder in ["h264_nvenc", "hevc_nvenc", "av1_nvenc"] {
        if !report.has_encoder(encoder) {
            continue;
        }

        let encoder_arg = format!("encoder={encoder}");
        let Ok(output) = run_tool(
            "ffmpeg",
            &config.ffmpeg_path,
            &["-hide_banner", "-h", &encoder_arg],
        )
        .await
        else {
            continue;
        };
        let text = String::from_utf8_lossy(&output);
        let mut capability = parse_nvidia_encoder_capability(encoder, &text);
        if encoder == "hevc_nvenc" && probe_hevc_nvenc_b_frame_ref_mode(&config.ffmpeg_path).await {
            capability = capability.with_b_frames();
        }
        capabilities.push(capability);
    }
    capabilities
}

async fn probe_hevc_nvenc_b_frame_ref_mode(ffmpeg_path: &str) -> bool {
    if cfg!(target_os = "macos") {
        return false;
    }

    tokio::process::Command::new(ffmpeg_path)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=64x64:rate=1:duration=1",
            "-frames:v",
            "1",
            "-pix_fmt",
            "yuv420p",
            "-c:v",
            "hevc_nvenc",
            "-bf",
            "1",
            "-b_ref_mode",
            "1",
            "-f",
            "null",
            "-",
        ])
        .output()
        .await
        .is_ok_and(|output| output.status.success())
}

#[derive(Debug, Clone, Copy)]
struct NvidiaDecoderProbeSpec {
    codec: &'static str,
    bit_depth: u8,
    decoder: &'static str,
    encoder: &'static str,
    pixel_format: &'static str,
}

async fn probe_nvidia_decoder_capabilities(
    config: &FfmpegRuntimeCheckConfig,
    report: &FfmpegCapabilityReport,
) -> Vec<NvidiaDecoderCapability> {
    let mut capabilities = Vec::new();
    for spec in nvidia_decoder_probe_specs(report) {
        if probe_nvidia_decoder_capability(&config.ffmpeg_path, spec).await {
            capabilities.push(NvidiaDecoderCapability::new(spec.codec, spec.bit_depth));
        }
    }
    capabilities
}

fn nvidia_decoder_probe_specs(report: &FfmpegCapabilityReport) -> Vec<NvidiaDecoderProbeSpec> {
    const SPECS: &[NvidiaDecoderProbeSpec] = &[
        NvidiaDecoderProbeSpec {
            codec: "h264",
            bit_depth: 8,
            decoder: "h264_cuvid",
            encoder: "libx264",
            pixel_format: "yuv420p",
        },
        NvidiaDecoderProbeSpec {
            codec: "h264",
            bit_depth: 10,
            decoder: "h264_cuvid",
            encoder: "libx264",
            pixel_format: "yuv420p10le",
        },
        NvidiaDecoderProbeSpec {
            codec: "hevc",
            bit_depth: 8,
            decoder: "hevc_cuvid",
            encoder: "libx265",
            pixel_format: "yuv420p",
        },
        NvidiaDecoderProbeSpec {
            codec: "hevc",
            bit_depth: 10,
            decoder: "hevc_cuvid",
            encoder: "libx265",
            pixel_format: "yuv420p10le",
        },
        NvidiaDecoderProbeSpec {
            codec: "mpeg2video",
            bit_depth: 8,
            decoder: "mpeg2_cuvid",
            encoder: "mpeg2video",
            pixel_format: "yuv420p",
        },
        NvidiaDecoderProbeSpec {
            codec: "mpeg4",
            bit_depth: 8,
            decoder: "mpeg4_cuvid",
            encoder: "mpeg4",
            pixel_format: "yuv420p",
        },
        NvidiaDecoderProbeSpec {
            codec: "vp8",
            bit_depth: 8,
            decoder: "vp8_cuvid",
            encoder: "libvpx",
            pixel_format: "yuv420p",
        },
        NvidiaDecoderProbeSpec {
            codec: "vp9",
            bit_depth: 8,
            decoder: "vp9_cuvid",
            encoder: "libvpx-vp9",
            pixel_format: "yuv420p",
        },
        NvidiaDecoderProbeSpec {
            codec: "vp9",
            bit_depth: 10,
            decoder: "vp9_cuvid",
            encoder: "libvpx-vp9",
            pixel_format: "yuv420p10le",
        },
        NvidiaDecoderProbeSpec {
            codec: "av1",
            bit_depth: 8,
            decoder: "av1_cuvid",
            encoder: "libaom-av1",
            pixel_format: "yuv420p",
        },
        NvidiaDecoderProbeSpec {
            codec: "av1",
            bit_depth: 10,
            decoder: "av1_cuvid",
            encoder: "libaom-av1",
            pixel_format: "yuv420p10le",
        },
    ];

    SPECS
        .iter()
        .copied()
        .filter(|spec| report.has_decoder(spec.decoder) && report.has_encoder(spec.encoder))
        .collect()
}

async fn probe_nvidia_decoder_capability(ffmpeg_path: &str, spec: NvidiaDecoderProbeSpec) -> bool {
    let sample_path = nvidia_decoder_probe_sample_path(spec);
    let sample = sample_path.to_string_lossy().into_owned();
    let mut encode_args = vec![
        "-hide_banner",
        "-loglevel",
        "error",
        "-f",
        "lavfi",
        "-i",
        "testsrc2=size=64x64:rate=1:duration=1",
        "-frames:v",
        "1",
        "-pix_fmt",
        spec.pixel_format,
        "-c:v",
        spec.encoder,
        "-f",
        "matroska",
    ];
    if spec.encoder == "libx265" {
        encode_args.extend(["-x265-params", "log-level=error"]);
    }
    encode_args.extend(["-y", &sample]);

    let encoded = tokio::process::Command::new(ffmpeg_path)
        .args(&encode_args)
        .output()
        .await
        .is_ok_and(|output| output.status.success());
    if !encoded {
        let _ = std::fs::remove_file(&sample_path);
        return false;
    }

    let decoded = tokio::process::Command::new(ffmpeg_path)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-hwaccel",
            "cuda",
            "-hwaccel_output_format",
            "cuda",
            "-c:v",
            spec.decoder,
            "-i",
            &sample,
            "-frames:v",
            "1",
            "-f",
            "null",
            "-",
        ])
        .output()
        .await
        .is_ok_and(|output| output.status.success());
    let _ = std::fs::remove_file(sample_path);
    decoded
}

fn nvidia_decoder_probe_sample_path(spec: NvidiaDecoderProbeSpec) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    std::env::temp_dir().join(format!(
        "ersatzrs-nvdec-probe-{}-{}-{}-{nanos}.mkv",
        std::process::id(),
        spec.codec,
        spec.bit_depth
    ))
}

/// Extract the useful NVIDIA device lines from `FFmpeg`'s `-gpu list` output.
#[must_use]
pub fn nvidia_gpu_list_summary(output: &str) -> Vec<String> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| {
            let lower = line.to_ascii_lowercase();
            lower.starts_with("gpu #")
                || lower.contains("nvidia")
                || lower.contains("geforce")
                || lower.contains("quadro")
                || lower.contains("tesla")
                || lower.contains("rtx")
                || lower.contains("gtx")
        })
        .map(str::to_owned)
        .collect()
}

/// Parse NVIDIA encoder details from `ffmpeg -h encoder=<name>` output.
#[must_use]
pub fn parse_nvidia_encoder_capability(encoder: &str, output: &str) -> NvidiaEncoderCapability {
    let mut pixel_formats = BTreeSet::new();
    let mut profiles = BTreeSet::new();
    let mut presets = BTreeSet::new();
    let mut tunes = BTreeSet::new();
    let mut option_values: Option<&str> = None;

    for line in output.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("Supported pixel formats:") {
            pixel_formats.extend(rest.split_whitespace().map(str::to_owned));
            option_values = None;
            continue;
        }

        if trimmed.starts_with("-profile") {
            option_values = Some("profile");
            continue;
        }
        if trimmed.starts_with("-preset") {
            option_values = Some("preset");
            continue;
        }
        if trimmed.starts_with("-tune") {
            option_values = Some("tune");
            continue;
        }

        if let Some(option) = option_values {
            if trimmed.starts_with('-') {
                option_values = None;
                continue;
            }

            let mut parts = trimmed.split_whitespace();
            let Some(candidate) = parts.next() else {
                continue;
            };
            if candidate
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
            {
                match option {
                    "profile" => profiles.insert(candidate.to_owned()),
                    "preset" => presets.insert(candidate.to_owned()),
                    "tune" => tunes.insert(candidate.to_owned()),
                    _ => false,
                };
            }
        }
    }

    NvidiaEncoderCapability::new(encoder, pixel_formats, profiles).with_options(presets, tunes)
}

/// Parse native NVIDIA decoder details from CUDA device-detail text.
///
/// ErsatzTV's CUDA helper reports decoder support as
/// `- Supports <codec> <bit-depth>-bit` under a "Decoding" heading. The
/// current `FFmpeg` CLI fallback does not emit those rows, so an empty result
/// means no native decoder detail was available.
#[must_use]
pub fn parse_nvidia_decoder_capabilities(output: &str) -> Vec<NvidiaDecoderCapability> {
    let mut capabilities = Vec::new();
    let mut in_decoding = false;

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.eq_ignore_ascii_case("decoding:") {
            in_decoding = true;
            continue;
        }
        if trimmed.eq_ignore_ascii_case("encoding:") {
            in_decoding = false;
            continue;
        }
        if !in_decoding {
            continue;
        }

        let Some(rest) = trimmed.strip_prefix("- Supports ") else {
            continue;
        };
        let mut parts = rest.split_whitespace();
        let Some(codec) = parts.next() else {
            continue;
        };
        let Some(bit_depth) = parts
            .next()
            .and_then(|value| value.strip_suffix("-bit"))
            .and_then(|value| value.parse::<u8>().ok())
        else {
            continue;
        };

        capabilities.push(NvidiaDecoderCapability::new(
            normalize_nvidia_decoder_codec(codec),
            bit_depth,
        ));
    }

    capabilities
}

fn normalize_nvidia_decoder_codec(codec: &str) -> String {
    match codec.to_ascii_lowercase().as_str() {
        "h264" => "h264".to_owned(),
        "hevc" => "hevc".to_owned(),
        "mpeg2video" => "mpeg2video".to_owned(),
        "mpeg4" => "mpeg4".to_owned(),
        "vc1" => "vc1".to_owned(),
        "vp8" => "vp8".to_owned(),
        "vp9" => "vp9".to_owned(),
        "av1" => "av1".to_owned(),
        other => other.to_owned(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct VideoToolboxFrameworkCapabilities {
    decoders: BTreeSet<String>,
    encoders: BTreeSet<String>,
}

async fn probe_videotoolbox_framework_capabilities(
    ffmpeg_path: &str,
    report: &FfmpegCapabilityReport,
) -> VideoToolboxFrameworkCapabilities {
    let native_probe = ersatzrs_hwprobe::probe_videotoolbox();
    if native_probe.attempted {
        return VideoToolboxFrameworkCapabilities {
            decoders: native_probe.decoders,
            encoders: native_probe.encoders,
        };
    }

    if cfg!(target_os = "macos") {
        probe_videotoolbox_ffmpeg_capabilities(ffmpeg_path, report).await
    } else {
        VideoToolboxFrameworkCapabilities::default()
    }
}

#[derive(Debug, Clone, Copy)]
struct VideoToolboxDecoderProbeSpec {
    fourcc: &'static str,
    encoder: &'static str,
    pixel_format: &'static str,
}

async fn probe_videotoolbox_ffmpeg_capabilities(
    ffmpeg_path: &str,
    report: &FfmpegCapabilityReport,
) -> VideoToolboxFrameworkCapabilities {
    let mut capabilities = VideoToolboxFrameworkCapabilities::default();
    for spec in videotoolbox_decoder_probe_specs(report) {
        if probe_videotoolbox_decoder_capability(ffmpeg_path, spec).await {
            capabilities.decoders.insert(spec.fourcc.to_owned());
        }
    }
    if report.has_encoder("h264_videotoolbox")
        && probe_videotoolbox_encoder_capability(ffmpeg_path, "h264_videotoolbox").await
    {
        capabilities.encoders.insert("H.264 (HW)".to_owned());
    }
    if report.has_encoder("hevc_videotoolbox")
        && probe_videotoolbox_encoder_capability(ffmpeg_path, "hevc_videotoolbox").await
    {
        capabilities.encoders.insert("HEVC (HW)".to_owned());
    }
    capabilities
}

fn videotoolbox_decoder_probe_specs(
    report: &FfmpegCapabilityReport,
) -> Vec<VideoToolboxDecoderProbeSpec> {
    const SPECS: &[VideoToolboxDecoderProbeSpec] = &[
        VideoToolboxDecoderProbeSpec {
            fourcc: "av01",
            encoder: "libaom-av1",
            pixel_format: "yuv420p",
        },
        VideoToolboxDecoderProbeSpec {
            fourcc: "avc1",
            encoder: "libx264",
            pixel_format: "yuv420p",
        },
        VideoToolboxDecoderProbeSpec {
            fourcc: "hvc1",
            encoder: "libx265",
            pixel_format: "yuv420p",
        },
        VideoToolboxDecoderProbeSpec {
            fourcc: "mp2v",
            encoder: "mpeg2video",
            pixel_format: "yuv420p",
        },
        VideoToolboxDecoderProbeSpec {
            fourcc: "mp4v",
            encoder: "mpeg4",
            pixel_format: "yuv420p",
        },
        VideoToolboxDecoderProbeSpec {
            fourcc: "vp09",
            encoder: "libvpx-vp9",
            pixel_format: "yuv420p",
        },
    ];
    SPECS
        .iter()
        .copied()
        .filter(|spec| report.has_encoder(spec.encoder))
        .collect()
}

async fn probe_videotoolbox_decoder_capability(
    ffmpeg_path: &str,
    spec: VideoToolboxDecoderProbeSpec,
) -> bool {
    let sample_path = videotoolbox_probe_sample_path(spec.fourcc);
    let sample = sample_path.to_string_lossy().into_owned();
    let mut encode_args = vec![
        "-hide_banner",
        "-loglevel",
        "error",
        "-f",
        "lavfi",
        "-i",
        "testsrc2=size=64x64:rate=1:duration=1",
        "-frames:v",
        "1",
        "-pix_fmt",
        spec.pixel_format,
        "-c:v",
        spec.encoder,
        "-f",
        "matroska",
    ];
    if spec.encoder == "libx265" {
        encode_args.extend(["-x265-params", "log-level=error"]);
    }
    encode_args.extend(["-y", &sample]);

    let encoded = tokio::process::Command::new(ffmpeg_path)
        .args(&encode_args)
        .output()
        .await
        .is_ok_and(|output| output.status.success());
    if !encoded {
        let _ = std::fs::remove_file(&sample_path);
        return false;
    }

    let decoded = tokio::process::Command::new(ffmpeg_path)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-hwaccel",
            "videotoolbox",
            "-i",
            &sample,
            "-frames:v",
            "1",
            "-f",
            "null",
            "-",
        ])
        .output()
        .await
        .is_ok_and(|output| output.status.success());
    let _ = std::fs::remove_file(sample_path);
    decoded
}

async fn probe_videotoolbox_encoder_capability(ffmpeg_path: &str, encoder: &str) -> bool {
    tokio::process::Command::new(ffmpeg_path)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=64x64:rate=1:duration=1",
            "-frames:v",
            "1",
            "-pix_fmt",
            "yuv420p",
            "-c:v",
            encoder,
            "-f",
            "null",
            "-",
        ])
        .output()
        .await
        .is_ok_and(|output| output.status.success())
}

fn videotoolbox_probe_sample_path(fourcc: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    std::env::temp_dir().join(format!(
        "ersatzrs-videotoolbox-probe-{}-{fourcc}-{nanos}.mkv",
        std::process::id()
    ))
}

async fn probe_qsv_profiles(
    ffmpeg_path: &str,
    device: Option<&FfmpegHardwareProbeDevice>,
) -> Vec<VaapiProfileEntrypoint> {
    let mut args = Vec::from([
        "-f".to_owned(),
        "lavfi".to_owned(),
        "-i".to_owned(),
        "nullsrc".to_owned(),
        "-t".to_owned(),
        "00:00:01".to_owned(),
    ]);
    if cfg!(target_os = "linux") {
        if let Some(qsv_device) = device.and_then(|device| device.vaapi_device.as_deref()) {
            args.extend(["-qsv_device".to_owned(), qsv_device.to_owned()]);
        }
    }
    args.extend([
        "-c:v".to_owned(),
        "h264_qsv".to_owned(),
        "-f".to_owned(),
        "null".to_owned(),
        "-".to_owned(),
    ]);

    let Ok(output) = tokio::process::Command::new(ffmpeg_path)
        .args(args)
        .output()
        .await
    else {
        return Vec::new();
    };

    if !output.status.success() {
        return Vec::new();
    }

    if cfg!(target_os = "linux") {
        probe_vainfo_profiles(device).await
    } else {
        Vec::new()
    }
}

async fn probe_all_vainfo_profiles(
    devices: &[FfmpegHardwareProbeDevice],
) -> Vec<VaapiProfileEntrypoint> {
    if devices.is_empty() {
        return probe_vainfo_profiles(None).await;
    }

    let mut profiles = Vec::new();
    for device in devices {
        profiles.extend(probe_vainfo_profiles(Some(device)).await);
    }
    dedupe_vaapi_profiles(profiles)
}

async fn probe_vainfo_profiles(
    device: Option<&FfmpegHardwareProbeDevice>,
) -> Vec<VaapiProfileEntrypoint> {
    let mut args = Vec::new();
    let display = device
        .and_then(|device| device.vaapi_display.as_deref())
        .unwrap_or("drm");
    args.extend(["--display".to_owned(), display.to_owned()]);
    if let Some(device_path) = device.and_then(|device| device.vaapi_device.as_deref()) {
        args.extend(["--device".to_owned(), device_path.to_owned()]);
    }
    args.push("-a".to_owned());

    let mut command = tokio::process::Command::new("vainfo");
    command.args(args);
    if let Some(driver) = device.and_then(|device| device.vaapi_driver.as_deref()) {
        command.env("LIBVA_DRIVER_NAME", driver);
    }

    let Ok(output) = command.output().await else {
        return Vec::new();
    };

    let mut combined = String::new();
    combined.push_str(&String::from_utf8_lossy(&output.stdout));
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    parse_vaapi_profile_entrypoints(&combined)
}

fn dedupe_vaapi_profiles(profiles: Vec<VaapiProfileEntrypoint>) -> Vec<VaapiProfileEntrypoint> {
    let mut deduped = Vec::new();
    for profile in profiles {
        if !deduped.contains(&profile) {
            deduped.push(profile);
        }
    }
    deduped
}

/// Parse VAAPI profile/entrypoint rows from `vainfo -a` output.
#[must_use]
pub fn parse_vaapi_profile_entrypoints(output: &str) -> Vec<VaapiProfileEntrypoint> {
    let mut profiles = Vec::new();
    for line in output.lines() {
        if let Some(entry) = (|| {
            let mut profile = None;
            let mut entrypoint = None;
            for token in line.split(|ch: char| ch.is_whitespace() || ch == ':' || ch == ',') {
                if token.starts_with("VAProfile") {
                    profile = Some(token);
                } else if token.starts_with("VAEntrypoint") {
                    entrypoint = Some(token);
                }
            }
            Some(VaapiProfileEntrypoint::new(profile?, entrypoint?))
        })() {
            profiles.push(entry);
            continue;
        }

        if let Some(last) = profiles.last_mut() {
            for token in line.split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_') {
                if let Some(mode) = token.strip_prefix("VA_RC_") {
                    last.rate_control_modes.insert(mode.to_owned());
                } else if token == "VA_ENC_PACKED_HEADER_MISC" {
                    last.packed_header_misc = true;
                }
            }
        }
    }
    profiles
}

async fn check_ffprobe_json(ffprobe_path: &str) -> Result<(), FfmpegCheckError> {
    let output = run_tool(
        "ffprobe",
        ffprobe_path,
        &["-v", "error", "-of", "json", "-show_program_version"],
    )
    .await?;
    let stdout = String::from_utf8(output).map_err(|error| FfmpegCheckError::ParseError {
        tool: "ffprobe",
        output: error.to_string(),
    })?;

    if !stdout.contains("\"program_version\"") {
        return Err(FfmpegCheckError::ParseError {
            tool: "ffprobe",
            output: stdout,
        });
    }

    Ok(())
}

async fn run_tool(
    tool: &'static str,
    path: &str,
    args: &[&str],
) -> Result<Vec<u8>, FfmpegCheckError> {
    let output = tokio::process::Command::new(path)
        .args(args)
        .output()
        .await
        .map_err(|error| FfmpegCheckError::ToolNotFound {
            tool,
            path: path.to_owned(),
            required_major: REQUIRED_MAJOR,
            required_minor: REQUIRED_MINOR,
            error: error.to_string(),
        })?;

    if !output.status.success() {
        return Err(FfmpegCheckError::ToolFailed {
            tool,
            path: path.to_owned(),
            status: status_string(output.status),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }

    Ok(output.stdout)
}

fn status_string(status: ExitStatus) -> String {
    status
        .code()
        .map_or_else(|| status.to_string(), |code| code.to_string())
}

fn collect_runtime_issues(
    config: &FfmpegRuntimeCheckConfig,
    capabilities: &FfmpegCapabilityReport,
) -> Vec<FfmpegRuntimeIssue> {
    let mut issues = Vec::new();

    for name in &config.required_filters {
        if !capabilities.has_filter(name) {
            issues.push(FfmpegRuntimeIssue::RequiredFilter { name: name.clone() });
        }
    }
    for name in &config.required_encoders {
        if !capabilities.has_encoder(name) {
            issues.push(FfmpegRuntimeIssue::RequiredEncoder { name: name.clone() });
        }
    }
    for name in &config.required_decoders {
        if !capabilities.has_decoder(name) {
            issues.push(FfmpegRuntimeIssue::RequiredDecoder { name: name.clone() });
        }
    }
    for name in &config.required_muxers {
        if !capabilities.muxers.contains(name) {
            issues.push(FfmpegRuntimeIssue::RequiredMuxer { name: name.clone() });
        }
    }
    for name in &config.required_demuxers {
        if !capabilities.demuxers.contains(name) {
            issues.push(FfmpegRuntimeIssue::RequiredDemuxer { name: name.clone() });
        }
    }
    for flag in &config.required_configuration_flags {
        if !capabilities.has_configuration_flag(flag) {
            issues.push(FfmpegRuntimeIssue::RequiredConfigurationFlag { flag: flag.clone() });
        }
    }
    for name in &config.required_hwaccels {
        if !capabilities.has_hwaccel(name) {
            issues.push(FfmpegRuntimeIssue::RequiredHwaccel { name: name.clone() });
        }
    }
    for name in &config.optional_hwaccels {
        if !capabilities.has_hwaccel(name) {
            issues.push(FfmpegRuntimeIssue::OptionalHwaccel { name: name.clone() });
        }
    }

    for encoder in &capabilities.nvidia_encoders {
        for preset in ["p1", "p2", "p3", "p4", "p5", "p6", "p7"] {
            if !encoder.has_preset(preset) {
                issues.push(FfmpegRuntimeIssue::RequiredEncoderOption {
                    encoder: encoder.encoder.clone(),
                    option: format!("preset={preset}"),
                });
            }
        }
        for tune in ["hq", "ll", "ull", "lossless"] {
            if !encoder.has_tune(tune) {
                issues.push(FfmpegRuntimeIssue::RequiredEncoderOption {
                    encoder: encoder.encoder.clone(),
                    option: format!("tune={tune}"),
                });
            }
        }
    }

    issues
}

fn to_owned_vec(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

#[cfg(test)]
mod tests {
    use super::{
        collect_runtime_issues, dedupe_vaapi_profiles, nvidia_decoder_probe_specs,
        nvidia_gpu_list_summary, parse_bitstream_filters, parse_capability_names,
        parse_configuration_flags, parse_hwaccels, parse_nvidia_decoder_capabilities,
        parse_nvidia_encoder_capability, parse_vaapi_profile_entrypoints, parse_version,
        preferred_default_profile_acceleration, videotoolbox_decoder_probe_specs,
        FfmpegCapabilityReport, FfmpegHardwareProbeDevice, FfmpegRuntimeCheckConfig,
        FfmpegRuntimeIssue, FfmpegVersion, HardwareAccelerationKind, NvidiaDecoderCapability,
        VaapiProfileEntrypoint, REQUIRED_MAJOR, REQUIRED_MINOR,
    };
    use std::collections::BTreeSet;

    #[test]
    fn parses_release_version_without_patch() {
        assert_eq!(
            parse_version("ffmpeg version 8.1 Copyright").unwrap(),
            FfmpegVersion {
                major: 8,
                minor: 1,
                patch: 0
            }
        );
    }

    #[test]
    fn parses_n_prefixed_git_version_with_patch_and_suffix() {
        assert_eq!(
            parse_version("ffprobe version n8.1.2-12-gabcdef Copyright").unwrap(),
            FfmpegVersion {
                major: 8,
                minor: 1,
                patch: 2
            }
        );
    }

    #[test]
    fn parses_distro_suffixed_version() {
        assert_eq!(
            parse_version("ffmpeg version 8.1.1-0ubuntu1 Copyright").unwrap(),
            FfmpegVersion {
                major: 8,
                minor: 1,
                patch: 1
            }
        );
    }

    #[test]
    fn accepts_only_the_verified_ffmpeg_major_line() {
        for minor in [0, 2] {
            assert!(FfmpegVersion {
                major: 9,
                minor,
                patch: 0,
            }
            .is_supported(REQUIRED_MAJOR, REQUIRED_MINOR));
        }
        for major in [8, 10] {
            assert!(!FfmpegVersion {
                major,
                minor: 0,
                patch: 0,
            }
            .is_supported(REQUIRED_MAJOR, REQUIRED_MINOR));
        }
    }

    #[test]
    fn parses_configuration_flags() {
        let flags = parse_configuration_flags(
            "ffmpeg version 8.1\nconfiguration: --enable-gpl --enable-cairo --enable-libx264\n",
        );

        assert!(flags.contains("--enable-cairo"));
        assert!(flags.contains("--enable-libx264"));
        assert!(!flags.contains("configuration:"));
    }

    #[test]
    fn parses_filter_encoder_and_decoder_names() {
        let output = "
Filters:
  TSC drawvg            V->V       Draw vector graphics.
  ... overlay           VV->V      Overlay video.
Encoders:
 V....D libx264         H.264 encoder
 A..... aac             AAC encoder
";
        let names = parse_capability_names(output);

        assert!(names.contains("drawvg"));
        assert!(names.contains("overlay"));
        assert!(names.contains("libx264"));
        assert!(names.contains("aac"));
        assert!(!names.contains("Filters:"));
        assert!(!names.contains("="));
    }

    #[test]
    fn parses_hwaccel_names() {
        let output = "Hardware acceleration methods:\nvaapi\ncuda\nvulkan\n";
        let names = parse_hwaccels(output);

        assert_eq!(
            names,
            BTreeSet::from(["cuda".to_owned(), "vaapi".to_owned(), "vulkan".to_owned()])
        );
    }

    #[test]
    fn parses_bitstream_filter_names() {
        let output = "Bitstream filters:\naac_adtstoasc\ndovi_rpu\ndovi_split\n";

        assert_eq!(
            parse_bitstream_filters(output),
            BTreeSet::from([
                "aac_adtstoasc".to_owned(),
                "dovi_rpu".to_owned(),
                "dovi_split".to_owned(),
            ])
        );
    }

    #[test]
    fn parses_vainfo_profile_entrypoints() {
        let output = "\
vainfo: VA-API version: 1.22
      VAProfileH264Main               : VAEntrypointVLD
      VAProfileH264Main               : VAEntrypointEncSlice
          VAConfigAttribRateControl              : VA_RC_CQP
          VAConfigAttribEncPackedHeaders         : VA_ENC_PACKED_HEADER_MISC
      VAProfileHEVCMain10             : VAEntrypointEncSliceLP
          VAConfigAttribRateControl              : VA_RC_CBR
";

        let entries = parse_vaapi_profile_entrypoints(output);

        assert_eq!(entries.len(), 3);
        assert!(entries[0].profile_matches("VAProfileH264Main"));
        assert!(entries[0].is_decode());
        assert!(entries[1].is_encode());
        assert!(entries[1].has_rate_control_mode("CQP"));
        assert!(entries[1].packed_header_misc);
        assert!(entries[2].profile_matches("VAProfileHEVCMain10"));
        assert!(entries[2].is_encode());
        assert!(entries[2].has_rate_control_mode("CBR"));
    }

    #[test]
    fn runtime_config_accepts_profile_hardware_probe_devices() {
        let config = FfmpegRuntimeCheckConfig::with_paths("ffmpeg-custom", "ffprobe-custom")
            .with_hardware_probe_devices(vec![FfmpegHardwareProbeDevice::new(
                Some("drm".to_owned()),
                Some("/dev/dri/renderD128".to_owned()),
                Some("iHD".to_owned()),
            )]);

        assert_eq!(config.ffmpeg_path, "ffmpeg-custom");
        assert_eq!(config.ffprobe_path, "ffprobe-custom");
        assert_eq!(
            config.hardware_probe_devices,
            vec![FfmpegHardwareProbeDevice::new(
                Some("drm".to_owned()),
                Some("/dev/dri/renderD128".to_owned()),
                Some("iHD".to_owned())
            )]
        );
    }

    #[test]
    fn dedupes_vaapi_profile_entrypoint_rows() {
        let entries = dedupe_vaapi_profiles(vec![
            VaapiProfileEntrypoint::new("VAProfileH264Main", "VAEntrypointVLD"),
            VaapiProfileEntrypoint::new("VAProfileH264Main", "VAEntrypointVLD"),
            VaapiProfileEntrypoint::new("VAProfileH264Main", "VAEntrypointEncSlice"),
        ]);

        assert_eq!(
            entries,
            vec![
                VaapiProfileEntrypoint::new("VAProfileH264Main", "VAEntrypointVLD"),
                VaapiProfileEntrypoint::new("VAProfileH264Main", "VAEntrypointEncSlice"),
            ]
        );
    }

    #[test]
    fn summarizes_nvidia_gpu_list_output() {
        let output = "\
[h264_nvenc @ 0x123] Available devices for encoding:
  GPU #0: NVIDIA GeForce RTX 4070
  GPU #1: Tesla T4
  unrelated line
";

        assert_eq!(
            nvidia_gpu_list_summary(output),
            vec![
                "GPU #0: NVIDIA GeForce RTX 4070".to_owned(),
                "GPU #1: Tesla T4".to_owned(),
            ]
        );
    }

    #[test]
    fn parses_nvidia_encoder_capability_help() {
        let output = "\
Encoder h264_nvenc [NVIDIA NVENC H.264 encoder]:
    Supported hardware devices: cuda cuda
    Supported pixel formats: yuv420p nv12 p010le yuv444p
h264_nvenc AVOptions:
  -profile           <int>        E..V....... Set the encoding profile
     baseline        0            E..V.......
     main            1            E..V.......
     high            2            E..V.......
     high444p        3            E..V.......
  -preset            <int>        E..V....... Set the encoding preset
     p1              12            E..V....... fastest
     p7              18            E..V....... best quality
  -tune              <int>        E..V....... Set the encoding tuning info
     hq               1            E..V....... high quality
     ll               2            E..V....... low latency
     ull              3            E..V....... ultra low latency
     lossless         4            E..V....... lossless
";

        let capability = parse_nvidia_encoder_capability("h264_nvenc", output);

        assert_eq!(capability.encoder, "h264_nvenc");
        assert!(capability.has_pixel_format("p010le"));
        assert!(capability.has_profile("high444p"));
        assert!(capability.has_preset("p1"));
        assert!(capability.has_preset("p7"));
        assert!(capability.has_tune("ll"));
        assert!(capability.has_tune("lossless"));
        assert!(!capability.has_profile("preset"));
    }

    #[test]
    fn parses_nvidia_decoder_device_details() {
        let output = "\
  Encoding:
    - Supports hevc 8-bit (with B-frames)

  Decoding:
    - Supports h264 8-bit
    - Supports hevc 8-bit
    - Supports hevc 10-bit
    - Supports av1 10-bit
";

        let capabilities = parse_nvidia_decoder_capabilities(output);

        assert_eq!(capabilities.len(), 4);
        assert!(capabilities
            .iter()
            .any(|capability| capability.supports("hevc", 10)));
        assert!(capabilities
            .iter()
            .any(|capability| capability.supports("av1", 10)));
    }

    #[test]
    fn native_nvidia_decoder_probe_specs_require_decoder_and_sample_encoder() {
        let report = FfmpegCapabilityReport {
            encoders: BTreeSet::from(["libx264".to_owned(), "libx265".to_owned()]),
            decoders: BTreeSet::from([
                "h264_cuvid".to_owned(),
                "hevc_cuvid".to_owned(),
                "av1_cuvid".to_owned(),
            ]),
            ..FfmpegCapabilityReport::default()
        };

        let specs = nvidia_decoder_probe_specs(&report);

        assert_eq!(
            specs
                .iter()
                .map(|spec| NvidiaDecoderCapability::new(spec.codec, spec.bit_depth))
                .collect::<Vec<_>>(),
            vec![
                NvidiaDecoderCapability::new("h264", 8),
                NvidiaDecoderCapability::new("h264", 10),
                NvidiaDecoderCapability::new("hevc", 8),
                NvidiaDecoderCapability::new("hevc", 10),
            ]
        );
    }

    #[test]
    fn videotoolbox_decoder_probe_specs_require_sample_encoder() {
        let report = FfmpegCapabilityReport {
            encoders: BTreeSet::from([
                "libx264".to_owned(),
                "libx265".to_owned(),
                "libaom-av1".to_owned(),
            ]),
            ..FfmpegCapabilityReport::default()
        };

        let specs = videotoolbox_decoder_probe_specs(&report);

        assert_eq!(
            specs.iter().map(|spec| spec.fourcc).collect::<Vec<_>>(),
            vec!["av01", "avc1", "hvc1"]
        );
    }

    #[test]
    fn default_runtime_check_requires_ersatztv_filters_plus_graphics_filters() {
        let config = FfmpegRuntimeCheckConfig::default();

        assert_eq!(
            config.required_filters,
            [
                "drawvg",
                "drawtext",
                "apad",
                "aresample",
                "color",
                "colorchannelmixer",
                "colorspace",
                "crop",
                "fade",
                "format",
                "fps",
                "framerate",
                "loop",
                "loudnorm",
                "overlay",
                "pad",
                "realtime",
                "reverse",
                "setpts",
                "settb",
                "scale",
                "subtitles",
                "tonemap",
                "tpad",
                "trim",
                "yadif",
                "zscale",
            ]
        );
    }

    #[test]
    fn reports_missing_required_and_optional_capabilities() {
        let config = FfmpegRuntimeCheckConfig {
            required_filters: vec!["drawvg".to_owned()],
            required_encoders: vec!["libx264".to_owned()],
            required_decoders: vec!["h264".to_owned()],
            required_configuration_flags: vec!["--enable-cairo".to_owned()],
            required_hwaccels: vec!["vaapi".to_owned()],
            optional_hwaccels: vec!["cuda".to_owned()],
            ..FfmpegRuntimeCheckConfig::default()
        };
        let capabilities = FfmpegCapabilityReport {
            runtime_identity: None,
            filters: BTreeSet::new(),
            encoders: BTreeSet::from(["libx264".to_owned()]),
            decoders: BTreeSet::from(["h264".to_owned()]),
            muxers: BTreeSet::from(["nut".to_owned()]),
            demuxers: BTreeSet::from(["nut".to_owned()]),
            bitstream_filters: BTreeSet::new(),
            hwaccels: BTreeSet::new(),
            configuration_flags: BTreeSet::new(),
            vaapi_profiles: Vec::new(),
            qsv_profiles: Vec::new(),
            nvidia_gpu_list: None,
            selected_nvidia_device: None,
            nvidia_encoders: Vec::new(),
            nvidia_decoders: Vec::new(),
            hardware_decode_denials: Vec::new(),
            hardware_graph_denials: Vec::new(),
            proven_hardware_graphs: Vec::new(),
            attempted_hardware_graphs: Vec::new(),
            videotoolbox_decoders: BTreeSet::new(),
            videotoolbox_encoders: BTreeSet::new(),
        };

        let issues = collect_runtime_issues(&config, &capabilities);

        assert_eq!(
            issues,
            vec![
                FfmpegRuntimeIssue::RequiredFilter {
                    name: "drawvg".to_owned()
                },
                FfmpegRuntimeIssue::RequiredConfigurationFlag {
                    flag: "--enable-cairo".to_owned()
                },
                FfmpegRuntimeIssue::RequiredHwaccel {
                    name: "vaapi".to_owned()
                },
                FfmpegRuntimeIssue::OptionalHwaccel {
                    name: "cuda".to_owned()
                }
            ]
        );
        assert!(issues[0].is_error());
        assert!(!issues[3].is_error());
    }

    #[test]
    fn default_profile_acceleration_prefers_nvenc_then_qsv_then_vaapi() {
        let report = FfmpegCapabilityReport {
            encoders: BTreeSet::from([
                "h264_vaapi".to_owned(),
                "hevc_qsv".to_owned(),
                "h264_nvenc".to_owned(),
            ]),
            hwaccels: BTreeSet::from(["vaapi".to_owned(), "qsv".to_owned(), "cuda".to_owned()]),
            ..FfmpegCapabilityReport::default()
        };
        assert_eq!(
            preferred_default_profile_acceleration(&report),
            HardwareAccelerationKind::Nvenc
        );

        let report = FfmpegCapabilityReport {
            encoders: BTreeSet::from(["h264_vaapi".to_owned(), "hevc_qsv".to_owned()]),
            hwaccels: BTreeSet::from(["vaapi".to_owned(), "qsv".to_owned()]),
            ..FfmpegCapabilityReport::default()
        };
        assert_eq!(
            preferred_default_profile_acceleration(&report),
            HardwareAccelerationKind::Qsv
        );

        let report = FfmpegCapabilityReport {
            encoders: BTreeSet::from(["h264_vaapi".to_owned()]),
            hwaccels: BTreeSet::from(["vaapi".to_owned()]),
            ..FfmpegCapabilityReport::default()
        };
        assert_eq!(
            preferred_default_profile_acceleration(&report),
            HardwareAccelerationKind::Vaapi
        );
    }

    #[test]
    fn default_profile_acceleration_handles_platform_specific_encoders() {
        let report = FfmpegCapabilityReport {
            hwaccels: BTreeSet::from(["videotoolbox".to_owned()]),
            videotoolbox_encoders: BTreeSet::from(["H.264 (HW)".to_owned()]),
            ..FfmpegCapabilityReport::default()
        };
        assert_eq!(
            preferred_default_profile_acceleration(&report),
            HardwareAccelerationKind::VideoToolbox
        );

        let report = FfmpegCapabilityReport {
            encoders: BTreeSet::from(["h264_amf".to_owned()]),
            ..FfmpegCapabilityReport::default()
        };
        assert_eq!(
            preferred_default_profile_acceleration(&report),
            HardwareAccelerationKind::Amf
        );

        assert_eq!(
            preferred_default_profile_acceleration(&FfmpegCapabilityReport::default()),
            HardwareAccelerationKind::None
        );
    }
}

#[cfg(all(test, unix))]
mod unix_tests {
    use super::{
        check_ffmpeg_runtime_with_config, collect_ffmpeg_capabilities,
        probe_videotoolbox_ffmpeg_capabilities, FfmpegCapabilityReport, FfmpegCheckError,
        FfmpegRuntimeCheckConfig, FfmpegRuntimeIssue, FfmpegVersion,
    };
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Serializes tests that spawn tools while another test may be writing a
    /// fake-tool script. A concurrent fork can briefly inherit the writable
    /// script descriptor, causing its next exec to fail with `ETXTBSY`.
    static TOOL_EXEC_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    #[tokio::test(flavor = "current_thread")]
    async fn runtime_check_reports_missing_ffmpeg() {
        let _exec_guard = TOOL_EXEC_LOCK.lock().await;
        let config = test_config("/definitely/missing/ffmpeg", "/definitely/missing/ffprobe");

        let error = check_ffmpeg_runtime_with_config(&config)
            .await
            .expect_err("missing ffmpeg should fail startup check");

        assert!(matches!(
            error,
            FfmpegCheckError::ToolNotFound { tool: "ffmpeg", .. }
        ));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn runtime_check_reports_missing_ffprobe() {
        let _exec_guard = TOOL_EXEC_LOCK.lock().await;
        let dir = temp_test_dir("missing-ffprobe");
        let ffmpeg = write_ffmpeg_script(&dir, "ffmpeg", "9.0", true, &[]);
        let config = test_config(ffmpeg.to_str().expect("utf-8 path"), "/missing/ffprobe");

        let error = check_ffmpeg_runtime_with_config(&config)
            .await
            .expect_err("missing ffprobe should fail startup check");

        assert!(matches!(
            error,
            FfmpegCheckError::ToolNotFound {
                tool: "ffprobe",
                ..
            }
        ));
        fs::remove_dir_all(dir).expect("remove temp dir");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn runtime_check_rejects_too_old_ffmpeg() {
        let _exec_guard = TOOL_EXEC_LOCK.lock().await;
        let dir = temp_test_dir("old-ffmpeg");
        let ffmpeg = write_ffmpeg_script(&dir, "ffmpeg", "8.0", true, &[]);
        let ffprobe = write_ffprobe_script(&dir, "ffprobe", "9.0");
        let config = test_config(
            ffmpeg.to_str().expect("utf-8 path"),
            ffprobe.to_str().expect("utf-8 path"),
        );

        let error = check_ffmpeg_runtime_with_config(&config)
            .await
            .expect_err("old ffmpeg should fail startup check");

        assert!(matches!(
            error,
            FfmpegCheckError::TooOld {
                tool: "ffmpeg",
                found: FfmpegVersion {
                    major: 8,
                    minor: 0,
                    ..
                },
                ..
            }
        ));
        fs::remove_dir_all(dir).expect("remove temp dir");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn runtime_check_rejects_too_old_ffprobe() {
        let _exec_guard = TOOL_EXEC_LOCK.lock().await;
        let dir = temp_test_dir("old-ffprobe");
        let ffmpeg = write_ffmpeg_script(&dir, "ffmpeg", "9.0", true, &[]);
        let ffprobe = write_ffprobe_script(&dir, "ffprobe", "8.0");
        let config = test_config(
            ffmpeg.to_str().expect("utf-8 path"),
            ffprobe.to_str().expect("utf-8 path"),
        );

        let error = check_ffmpeg_runtime_with_config(&config)
            .await
            .expect_err("old ffprobe should fail startup check");

        assert!(matches!(
            error,
            FfmpegCheckError::TooOld {
                tool: "ffprobe",
                found: FfmpegVersion {
                    major: 8,
                    minor: 0,
                    ..
                },
                ..
            }
        ));
        fs::remove_dir_all(dir).expect("remove temp dir");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn runtime_check_rejects_missing_required_drawvg() {
        let _exec_guard = TOOL_EXEC_LOCK.lock().await;
        let dir = temp_test_dir("missing-drawvg");
        let ffmpeg = write_ffmpeg_script(&dir, "ffmpeg", "9.0", false, &[]);
        let ffprobe = write_ffprobe_script(&dir, "ffprobe", "9.0");
        let config = test_config(
            ffmpeg.to_str().expect("utf-8 path"),
            ffprobe.to_str().expect("utf-8 path"),
        );

        let error = check_ffmpeg_runtime_with_config(&config)
            .await
            .expect_err("missing drawvg should fail startup check");

        assert!(
            matches!(error, FfmpegCheckError::RuntimeIssues(issues) if issues == vec![
                FfmpegRuntimeIssue::RequiredFilter {
                    name: "drawvg".to_owned()
                }
            ])
        );
        fs::remove_dir_all(dir).expect("remove temp dir");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn runtime_check_reports_optional_gpu_missing_without_failing() {
        let _exec_guard = TOOL_EXEC_LOCK.lock().await;
        let dir = temp_test_dir("optional-gpu");
        let ffmpeg = write_ffmpeg_script(&dir, "ffmpeg", "9.0", true, &["cuda"]);
        let ffprobe = write_ffprobe_script(&dir, "ffprobe", "9.0");
        let mut config = test_config(
            ffmpeg.to_str().expect("utf-8 path"),
            ffprobe.to_str().expect("utf-8 path"),
        );
        config.optional_hwaccels = vec!["cuda".to_owned(), "vaapi".to_owned()];

        let report = check_ffmpeg_runtime_with_config(&config)
            .await
            .expect("optional GPU absence should not fail startup check");

        assert_eq!(
            report.issues,
            vec![FfmpegRuntimeIssue::OptionalHwaccel {
                name: "vaapi".to_owned()
            }]
        );
        assert!(!report.has_errors());
        assert!(report.capabilities.muxers.contains("nut"));
        assert!(report.capabilities.demuxers.contains("nut"));
        fs::remove_dir_all(dir).expect("remove temp dir");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn native_nvidia_decoder_probe_populates_decoder_rows() {
        let _exec_guard = TOOL_EXEC_LOCK.lock().await;
        let dir = temp_test_dir("nvidia-decoder-probe");
        let ffmpeg = write_nvidia_decoder_probe_ffmpeg_script(&dir);
        let config = FfmpegRuntimeCheckConfig::with_paths(
            ffmpeg.to_str().expect("utf-8 path"),
            "/unused/ffprobe",
        );

        let report = collect_ffmpeg_capabilities(&config)
            .await
            .expect("fake ffmpeg should produce capabilities");

        assert!(report.nvidia_gpu_list.is_none());
        assert_eq!(report.nvidia_decoders.len(), 4);
        assert!(report
            .nvidia_decoders
            .iter()
            .any(|decoder| decoder.supports("h264", 10)));
        assert!(report
            .nvidia_decoders
            .iter()
            .any(|decoder| decoder.supports("hevc", 10)));
        fs::remove_dir_all(dir).expect("remove temp dir");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn videotoolbox_probe_populates_framework_result_fields() {
        let _exec_guard = TOOL_EXEC_LOCK.lock().await;
        let dir = temp_test_dir("videotoolbox-probe");
        let ffmpeg = write_videotoolbox_probe_ffmpeg_script(&dir);
        let report = FfmpegCapabilityReport {
            encoders: [
                "libx264",
                "libx265",
                "h264_videotoolbox",
                "hevc_videotoolbox",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
            ..FfmpegCapabilityReport::default()
        };

        let capabilities =
            probe_videotoolbox_ffmpeg_capabilities(ffmpeg.to_str().expect("utf-8 path"), &report)
                .await;

        assert_eq!(
            capabilities.decoders,
            ["avc1".to_owned(), "hvc1".to_owned()].into_iter().collect()
        );
        assert_eq!(
            capabilities.encoders,
            ["H.264 (HW)".to_owned(), "HEVC (HW)".to_owned()]
                .into_iter()
                .collect()
        );
        fs::remove_dir_all(dir).expect("remove temp dir");
    }

    fn test_config(ffmpeg_path: &str, ffprobe_path: &str) -> FfmpegRuntimeCheckConfig {
        FfmpegRuntimeCheckConfig {
            ffmpeg_path: ffmpeg_path.to_owned(),
            ffprobe_path: ffprobe_path.to_owned(),
            required_filters: vec!["drawvg".to_owned()],
            required_encoders: Vec::new(),
            required_decoders: Vec::new(),
            required_configuration_flags: Vec::new(),
            required_hwaccels: Vec::new(),
            optional_hwaccels: Vec::new(),
            ..FfmpegRuntimeCheckConfig::default()
        }
    }

    fn temp_test_dir(name: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time after unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "ersatzrs-ffmpeg-{name}-{}-{suffix}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create temp dir");
        path
    }

    fn write_ffmpeg_script(
        dir: &Path,
        name: &str,
        version: &str,
        include_drawvg: bool,
        hwaccels: &[&str],
    ) -> PathBuf {
        let drawvg_line = if include_drawvg {
            "  TSC drawvg            V->V       Draw vector graphics.\n"
        } else {
            ""
        };
        let hwaccel_lines = hwaccels.join("\\n");
        let body = format!(
            r#"#!/bin/sh
case "$*" in
  "-version")
    cat <<'EOF'
ffmpeg version {version} Copyright
configuration: --enable-cairo
EOF
    ;;
  "-hide_banner -filters")
    cat <<'EOF'
Filters:
  T.. = Timeline support
{drawvg_line}  ... overlay           VV->V      Overlay video.
EOF
    ;;
  "-hide_banner -encoders")
    cat <<'EOF'
Encoders:
 V....D libx264         H.264 encoder
EOF
    ;;
  "-hide_banner -decoders")
    cat <<'EOF'
Decoders:
 V....D h264            H.264 decoder
EOF
    ;;
  "-hide_banner -formats")
    printf 'Formats:\n DE nut NUT\n'
    ;;
  "-hide_banner -bsfs")
    printf 'Bitstream filters:\ndovi_split\n'
    ;;
  "-hide_banner -hwaccels")
    printf 'Hardware acceleration methods:\n{hwaccel_lines}\n'
    ;;
  "-hide_banner -loglevel error -f lavfi -i color=c=black:s=32x32:d=0.1 -vf drawvg=script='rect 4 4 16 16 fill',format=yuv420p -frames:v 1 -f null -")
    exit 0
    ;;
  "-hide_banner -loglevel error -f lavfi -i color=c=black:s=32x32:d=0.1 -frames:v 1 -fps_mode:v cfr -f null -")
    exit 0
    ;;
  *)
    exit 2
    ;;
esac
"#
        );
        write_script(dir, name, &body)
    }

    fn write_nvidia_decoder_probe_ffmpeg_script(dir: &Path) -> PathBuf {
        let body = r#"#!/bin/sh
case "$*" in
  "-hide_banner -filters")
    cat <<'EOF'
Filters:
  TSC drawvg            V->V       Draw vector graphics.
EOF
    ;;
  "-hide_banner -encoders")
    cat <<'EOF'
Encoders:
 V....D libx264         H.264 encoder
 V....D libx265         HEVC encoder
EOF
    ;;
  "-hide_banner -decoders")
    cat <<'EOF'
Decoders:
 V....D h264_cuvid      Nvidia CUVID H264 decoder
 V....D hevc_cuvid      Nvidia CUVID HEVC decoder
EOF
    ;;
  "-hide_banner -formats")
    printf 'Formats:\n DE nut NUT\n'
    ;;
  "-hide_banner -bsfs")
    printf 'Bitstream filters:\ndovi_split\n'
    ;;
  "-hide_banner -hwaccels")
    printf 'Hardware acceleration methods:\ncuda\n'
    ;;
  "-hide_banner -loglevel error -f lavfi -i testsrc2=size=64x64:rate=1:duration=1 -frames:v 1 -pix_fmt yuv420p -c:v libx264 -f matroska -y "*)
    exit 0
    ;;
  "-hide_banner -loglevel error -f lavfi -i testsrc2=size=64x64:rate=1:duration=1 -frames:v 1 -pix_fmt yuv420p10le -c:v libx264 -f matroska -y "*)
    exit 0
    ;;
  "-hide_banner -loglevel error -f lavfi -i testsrc2=size=64x64:rate=1:duration=1 -frames:v 1 -pix_fmt yuv420p -c:v libx265 -f matroska -x265-params log-level=error -y "*)
    exit 0
    ;;
  "-hide_banner -loglevel error -f lavfi -i testsrc2=size=64x64:rate=1:duration=1 -frames:v 1 -pix_fmt yuv420p10le -c:v libx265 -f matroska -x265-params log-level=error -y "*)
    exit 0
    ;;
  "-hide_banner -loglevel error -hwaccel cuda -hwaccel_output_format cuda -c:v h264_cuvid -i "*)
    exit 0
    ;;
  "-hide_banner -loglevel error -hwaccel cuda -hwaccel_output_format cuda -c:v hevc_cuvid -i "*)
    exit 0
    ;;
  *)
    exit 2
    ;;
esac
"#;
        write_script(dir, "ffmpeg", body)
    }

    fn write_videotoolbox_probe_ffmpeg_script(dir: &Path) -> PathBuf {
        let body = r#"#!/bin/sh
case "$*" in
  "-hide_banner -loglevel error -f lavfi -i testsrc2=size=64x64:rate=1:duration=1 -frames:v 1 -pix_fmt yuv420p -c:v libx264 -f matroska -y "*)
    exit 0
    ;;
  "-hide_banner -loglevel error -f lavfi -i testsrc2=size=64x64:rate=1:duration=1 -frames:v 1 -pix_fmt yuv420p -c:v libx265 -f matroska -x265-params log-level=error -y "*)
    exit 0
    ;;
  "-hide_banner -loglevel error -hwaccel videotoolbox -i "*)
    exit 0
    ;;
  "-hide_banner -loglevel error -f lavfi -i testsrc2=size=64x64:rate=1:duration=1 -frames:v 1 -pix_fmt yuv420p -c:v h264_videotoolbox -f null -")
    exit 0
    ;;
  "-hide_banner -loglevel error -f lavfi -i testsrc2=size=64x64:rate=1:duration=1 -frames:v 1 -pix_fmt yuv420p -c:v hevc_videotoolbox -f null -")
    exit 0
    ;;
  *)
    exit 2
    ;;
esac
"#;
        write_script(dir, "ffmpeg", body)
    }

    fn write_ffprobe_script(dir: &Path, name: &str, version: &str) -> PathBuf {
        let body = format!(
            r#"#!/bin/sh
case "$*" in
  "-version")
    cat <<'EOF'
ffprobe version {version} Copyright
configuration: --enable-cairo
EOF
    ;;
  "-v error -of json -show_program_version")
    printf '{{"program_version":{{"version":"{version}"}}}}\n'
    ;;
  *)
    exit 2
    ;;
esac
"#
        );
        write_script(dir, name, &body)
    }

    fn write_script(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, body).expect("write fake tool script");
        let mut permissions = fs::metadata(&path)
            .expect("read fake tool metadata")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions).expect("make fake tool executable");
        path
    }
}

#[cfg(test)]
#[path = "canvas_capability_tests.rs"]
mod canvas_capability_tests;
