//! Operation-scoped functional hardware-graph capability (ADR 0014, milestone
//! `p8b-01`).
//!
//! ErsatzRS previously selected the NVENC HDR Vulkan/libplacebo graph after
//! checking only that `ffmpeg -hwaccels` listed `vulkan`. That is not evidence
//! that the graph works: the advertised accelerator, the Vulkan *device* that
//! gets selected, and Vulkan-to-CUDA interoperability are three different
//! things.
//!
//! Measured failure on a WSL2 host with an RTX 3080 Ti and driver 610.47:
//! `ffmpeg -hwaccels` lists `vulkan` and `-init_hw_device vulkan` succeeds, but
//! the only Vulkan device present is `llvmpipe` (a software rasterizer), so
//! `hwmap=derive_device=cuda` fails with `Could not derive CUDA device` and
//! every HDR NVENC transcode dies. The proven CUDA-only graph succeeds on the
//! same machine. See `docs/explanation/hdr-output-characterization.md` and the
//! parity record for the captured transcripts.
//!
//! This module owns the pure half: which operations exist, what probe command
//! proves each one, how to classify a failure, and which graph to fall back to.
//! Running the probe and persisting the result belongs to `ersatzrs-infra`,
//! matching the existing `HardwareDecodeDenial` split.

use ersatzrs_domain::HardwareAccelerationKind;

/// A hardware graph whose viability must be proven functionally, not inferred
/// from an advertised capability.
///
/// Scoped to an *operation* rather than to a device or a filter, because the
/// failure modes observed in practice are interoperability failures between
/// components that each initialize successfully on their own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HardwareGraphOperation {
    /// CUDA decode/readback, Vulkan/libplacebo Dolby Vision profile 5 to SDR,
    /// RGBA readback, software YUV conversion, and H.264 NVENC.
    NvencDolbyVisionProfile5Sdr,
    /// CUDA decode/readback, Vulkan/libplacebo Dolby Vision profile 5 to
    /// HDR10, RGBA64 readback, software ten-bit YUV conversion, and HEVC
    /// Main10 NVENC.
    NvencDolbyVisionProfile5Hdr10,
    /// Vulkan decode/upload, libplacebo tone-mapping, `hwmap` into CUDA, a CUDA
    /// scale, and an NVENC encode — the HDR path ErsatzRS prefers on NVIDIA.
    NvencHdrVulkanLibplacebo,
    /// CUDA upload, CUDA scale, and an NVENC encode. The fallback the primary
    /// graph retries through; also the graph used for non-HDR NVENC work.
    NvencCudaScale,
}

impl HardwareGraphOperation {
    /// Stable identifier used in persisted rows and health messages.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::NvencDolbyVisionProfile5Sdr => "nvenc-dolby-vision-profile5-sdr",
            Self::NvencDolbyVisionProfile5Hdr10 => "nvenc-dolby-vision-profile5-hdr10",
            Self::NvencHdrVulkanLibplacebo => "nvenc-hdr-vulkan-libplacebo",
            Self::NvencCudaScale => "nvenc-cuda-scale",
        }
    }

    /// Parse a persisted identifier.
    #[must_use]
    pub fn from_id(value: &str) -> Option<Self> {
        match value {
            "nvenc-dolby-vision-profile5-sdr" => Some(Self::NvencDolbyVisionProfile5Sdr),
            "nvenc-dolby-vision-profile5-hdr10" => Some(Self::NvencDolbyVisionProfile5Hdr10),
            "nvenc-hdr-vulkan-libplacebo" => Some(Self::NvencHdrVulkanLibplacebo),
            "nvenc-cuda-scale" => Some(Self::NvencCudaScale),
            _ => None,
        }
    }

    /// The hardware acceleration this operation encodes with.
    #[must_use]
    pub const fn acceleration(self) -> HardwareAccelerationKind {
        HardwareAccelerationKind::Nvenc
    }

    /// The next graph to try when this one is denied.
    ///
    /// ADR 0014 allows exactly one retry, and the retry must preserve hardware
    /// encoding where possible: the NVENC HDR graph falls back to CUDA-only
    /// tone-mapping, which still encodes on the GPU. `NvencCudaScale` has no
    /// further hardware fallback — its denial demotes the whole profile to
    /// software, which the existing acceleration demotion already handles.
    #[must_use]
    pub const fn fallback(self) -> Option<Self> {
        match self {
            Self::NvencHdrVulkanLibplacebo => Some(Self::NvencCudaScale),
            Self::NvencDolbyVisionProfile5Sdr
            | Self::NvencDolbyVisionProfile5Hdr10
            | Self::NvencCudaScale => None,
        }
    }

    /// Build the bounded functional probe command arguments.
    ///
    /// The probe is deliberately tiny and self-contained: a `lavfi` source, one
    /// frame, and a null muxer, so it touches no user media and cannot hang on
    /// I/O. It runs the *real* graph shape, because the whole point is that
    /// every component initializing individually does not prove they
    /// interoperate.
    ///
    /// The frame size is not arbitrary. NVENC rejects dimensions below its
    /// minimum with `InitializeEncoder failed: invalid param (8): Frame
    /// Dimension less than the minimum supported value`, which a naive 32x32
    /// probe would misread as a graph denial. 320x240 in, 256x144 out clears
    /// the minimum on every NVENC generation ErsatzRS supports.
    #[must_use]
    pub fn probe_arguments(self) -> Vec<String> {
        let mut args: Vec<String> = ["-hide_banner", "-nostdin", "-loglevel", "error"]
            .iter()
            .map(|value| (*value).to_owned())
            .collect();

        match self {
            Self::NvencDolbyVisionProfile5Sdr => {
                args.extend(NVENC_PROFILE5_DEVICE_PREFIX.iter().map(|v| (*v).to_owned()));
                let filter = format!(
                    "format=yuv420p10le,hwupload_cuda=device=0,{}",
                    crate::dolby_vision_profile_5_sdr_filter(crate::FrameSize::new(256, 144))
                );
                args.extend(
                    ["-f", "lavfi", "-i", PROBE_SOURCE, "-vf"]
                        .iter()
                        .map(|value| (*value).to_owned()),
                );
                args.push(filter);
                args.extend(
                    ["-c:v", "h264_nvenc"]
                        .iter()
                        .map(|value| (*value).to_owned()),
                );
            }
            Self::NvencDolbyVisionProfile5Hdr10 => {
                args.extend(NVENC_PROFILE5_DEVICE_PREFIX.iter().map(|v| (*v).to_owned()));
                let filter = format!(
                    "format=yuv420p10le,hwupload_cuda=device=0,{}",
                    crate::dolby_vision_profile_5_hdr10_filter(crate::FrameSize::new(256, 144))
                );
                args.extend(
                    ["-f", "lavfi", "-i", PROBE_SOURCE, "-vf"]
                        .iter()
                        .map(|value| (*value).to_owned()),
                );
                args.push(filter);
                args.extend(
                    ["-c:v", "hevc_nvenc", "-profile:v", "main10"]
                        .iter()
                        .map(|value| (*value).to_owned()),
                );
            }
            Self::NvencHdrVulkanLibplacebo => {
                args.extend(NVENC_HDR_DEVICE_PREFIX.iter().map(|v| (*v).to_owned()));
                args.extend(
                    [
                        "-f",
                        "lavfi",
                        "-i",
                        PROBE_SOURCE,
                        "-vf",
                        "format=yuv420p10le,hwupload,libplacebo=format=nv12:colorspace=bt709:color_primaries=bt709:color_trc=bt709,hwmap=derive_device=cuda,scale_cuda=256:144",
                        "-c:v",
                        "h264_nvenc",
                    ]
                    .iter()
                    .map(|value| (*value).to_owned()),
                );
            }
            Self::NvencCudaScale => {
                args.extend(NVENC_CUDA_DEVICE_PREFIX.iter().map(|v| (*v).to_owned()));
                args.extend(
                    [
                        "-f",
                        "lavfi",
                        "-i",
                        PROBE_SOURCE,
                        "-vf",
                        "format=nv12,hwupload_cuda,scale_cuda=256:144",
                        "-c:v",
                        "h264_nvenc",
                    ]
                    .iter()
                    .map(|value| (*value).to_owned()),
                );
            }
        }

        args.extend(
            ["-frames:v", "1", "-f", "null", "-"]
                .iter()
                .map(|value| (*value).to_owned()),
        );
        args
    }
}

/// `lavfi` source shared by every probe. See `probe_arguments` for why the
/// size matters.
const PROBE_SOURCE: &str = "testsrc2=size=320x240:duration=0.2:rate=5";

/// Device-initialization prefix the NVENC HDR graph uses, shared by the probe
/// and the playback pipeline.
///
/// The Vulkan device is *derived from the selected CUDA device* (`vk@nv`)
/// rather than opened by default. That distinction is the whole point: on a
/// host with an NVIDIA GPU and a software Vulkan rasterizer, a bare
/// `-init_hw_device vulkan` opens llvmpipe, so a probe using it would approve
/// or deny a device playback never touches.
pub const NVENC_HDR_DEVICE_PREFIX: [&str; 6] = [
    "-init_hw_device",
    "cuda=nv:0",
    "-init_hw_device",
    "vulkan=vk@nv",
    "-filter_hw_device",
    "vk",
];

/// Device-initialization prefix the CUDA-only NVENC graph uses.
pub const NVENC_CUDA_DEVICE_PREFIX: [&str; 4] =
    ["-init_hw_device", "cuda=nv:0", "-filter_hw_device", "nv"];

/// Device initialization shared by both characterized profile 5 operations.
pub const NVENC_PROFILE5_DEVICE_PREFIX: [&str; 6] = NVENC_HDR_DEVICE_PREFIX;

/// Why a functional graph probe failed.
///
/// Every variant maps to a stderr signature captured from a real `FFmpeg` 9 run;
/// `GraphFailureKind::classify` is the only place those strings live.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphFailureKind {
    /// No usable Vulkan device, or the requested index does not exist.
    VulkanDevice,
    /// The libplacebo filter is missing or refused to initialize.
    Libplacebo,
    /// `hwmap` could not bridge two hardware device contexts.
    HardwareMap,
    /// A CUDA context could not be derived or made current — the WSL2/llvmpipe
    /// case that motivated this milestone.
    CudaInterop,
    /// The hardware encoder itself refused to open.
    EncoderInit,
    /// A required filter is not present in this build.
    MissingFilter,
}

impl GraphFailureKind {
    /// Short, stable reason text for health and troubleshooting rows.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::VulkanDevice => "no usable Vulkan device",
            Self::Libplacebo => "libplacebo failed to initialize",
            Self::HardwareMap => "hardware frame mapping failed",
            Self::CudaInterop => "Vulkan and CUDA are not interoperable on this device",
            Self::EncoderInit => "the hardware encoder failed to initialize",
            Self::MissingFilter => "a required filter is missing from this FFmpeg build",
        }
    }

    /// Classify an `FFmpeg` stderr tail from a failed graph probe.
    ///
    /// Order matters: CUDA-interop is checked before the generic hardware-map
    /// class because the observed WSL2 failure emits both, and the interop
    /// reason is the actionable one.
    #[must_use]
    pub fn classify(stderr: &str) -> Option<Self> {
        let lower = stderr.to_ascii_lowercase();

        if lower.contains("could not derive cuda device")
            || lower.contains("cuctxpushcurrent")
            || lower.contains("cuda_error_invalid_value")
        {
            return Some(Self::CudaInterop);
        }
        if lower.contains("unable to find device with index")
            || lower.contains("device creation failed")
            || lower.contains("failed initializing vulkan device")
            || lower.contains("failed creating vulkan device")
            || lower.contains("found no suitable device")
            || (lower.contains("vulkan") && lower.contains("no such device"))
        {
            return Some(Self::VulkanDevice);
        }
        if lower.contains("no such filter") {
            return Some(Self::MissingFilter);
        }
        if lower.contains("libplacebo") {
            return Some(Self::Libplacebo);
        }
        if lower.contains("failed to configure output pad")
            || lower.contains("error reinitializing filters")
        {
            return Some(Self::HardwareMap);
        }
        if lower.contains("initializeencoder failed")
            || lower.contains("error while opening encoder")
            || lower.contains("could not open encoder")
        {
            return Some(Self::EncoderInit);
        }

        None
    }

    /// Classify a live playback failure as belonging to the selected hardware
    /// graph.
    ///
    /// Playback stderr contains failures from inputs, subtitles, overlays,
    /// muxers, and encoders as well as the hardware graph. The broader probe
    /// classifier is safe only for the synthetic one-graph probe. Runtime
    /// learning therefore requires operation-specific filter or device
    /// evidence and deliberately excludes generic filter-reinitialization and
    /// encoder-open messages.
    #[must_use]
    pub fn classify_runtime(operation: HardwareGraphOperation, stderr: &str) -> Option<Self> {
        let lower = stderr.to_ascii_lowercase();
        let lines = lower.lines().collect::<Vec<_>>();
        let is_cuda_filter_line = |line: &str| {
            line.contains("scale_cuda")
                || line.contains("hwupload_cuda")
                || line.contains("parsed_scale_cuda")
                || line.contains("parsed_hwupload_cuda")
        };
        let is_vulkan_graph_line = |line: &str| {
            line.contains("libplacebo")
                || line.contains("hwmap")
                || line.contains("derive_device=cuda")
                || line.contains("vulkan=vk@nv")
        };
        let line_is_cuda_context_failure = |line: &str| {
            line.contains("cuctxpushcurrent") || line.contains("cuda_error_invalid_value")
        };
        let line_is_filter_failure = |line: &str| {
            line.contains("failed to configure output pad") || line_is_cuda_context_failure(line)
        };

        match operation {
            HardwareGraphOperation::NvencDolbyVisionProfile5Sdr
            | HardwareGraphOperation::NvencDolbyVisionProfile5Hdr10 => {
                return Self::classify_profile_5_runtime(&lower);
            }
            HardwareGraphOperation::NvencHdrVulkanLibplacebo => {
                if lower.contains("could not derive cuda device")
                    || lines.iter().any(|line| {
                        is_vulkan_graph_line(line) && line_is_cuda_context_failure(line)
                    })
                {
                    return Some(Self::CudaInterop);
                }
                if lower.contains("failed initializing vulkan device")
                    || lower.contains("failed creating vulkan device")
                    || lower.contains("found no suitable device")
                    || (lower.contains("vulkan")
                        && (lower.contains("unable to find device with index")
                            || lower.contains("device creation failed")
                            || lower.contains("no such device")))
                {
                    return Some(Self::VulkanDevice);
                }
                if lines.iter().any(|line| {
                    line.contains("no such filter")
                        && (is_vulkan_graph_line(line) || is_cuda_filter_line(line))
                }) {
                    return Some(Self::MissingFilter);
                }
                if lines.iter().any(|line| {
                    line.contains("libplacebo")
                        && (line.contains("failed to initialize")
                            || line.contains("error initializing"))
                }) {
                    return Some(Self::Libplacebo);
                }
                if lines.iter().any(|line| {
                    (is_vulkan_graph_line(line) || is_cuda_filter_line(line))
                        && line_is_filter_failure(line)
                }) {
                    return Some(Self::HardwareMap);
                }
            }
            HardwareGraphOperation::NvencCudaScale => {
                if lines
                    .iter()
                    .any(|line| line.contains("no such filter") && is_cuda_filter_line(line))
                {
                    return Some(Self::MissingFilter);
                }
                if lines
                    .iter()
                    .any(|line| is_cuda_filter_line(line) && line_is_filter_failure(line))
                {
                    return Some(Self::HardwareMap);
                }
            }
        }

        None
    }

    fn classify_profile_5_runtime(lower: &str) -> Option<Self> {
        let lines = lower.lines().collect::<Vec<_>>();
        let is_graph_line = |line: &str| {
            line.contains("libplacebo")
                || line.contains("hwmap")
                || line.contains("derive_device=cuda")
                || line.contains("vulkan=vk@nv")
                || line.contains("apply_dolbyvision")
                || line.contains("rgba64")
                || line.contains("hwdownload")
                || line.contains("hwupload")
                || line.contains("zscale")
        };
        if lower.contains("failed initializing vulkan device")
            || lower.contains("failed creating vulkan device")
            || lower.contains("found no suitable device")
            || (lower.contains("vulkan")
                && (lower.contains("unable to find device with index")
                    || lower.contains("device creation failed")
                    || lower.contains("no such device")))
        {
            return Some(Self::VulkanDevice);
        }
        if lines
            .iter()
            .any(|line| line.contains("no such filter") && is_graph_line(line))
        {
            return Some(Self::MissingFilter);
        }
        if lines.iter().any(|line| {
            line.contains("libplacebo")
                && (line.contains("failed to initialize") || line.contains("error initializing"))
        }) {
            return Some(Self::Libplacebo);
        }
        if lines.iter().any(|line| {
            is_graph_line(line)
                && (line.contains("failed to configure output pad")
                    || line.contains("cuctxpushcurrent")
                    || line.contains("cuda_error_invalid_value"))
        }) {
            return Some(Self::HardwareMap);
        }
        None
    }
}

/// A runtime-learned denial of one hardware graph operation.
///
/// Cached by `FFmpeg` binary identity and device identity as well as operation,
/// so replacing the runtime or moving the database to a different machine does
/// not carry a stale denial along. The binary identity is held by the existing
/// ADR 0014 config row; this record carries the device identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HardwareGraphDenial {
    /// Operation that failed its functional probe.
    pub operation: HardwareGraphOperation,
    /// Device identity the probe ran against, such as an NVIDIA GPU name.
    /// Empty when the runtime could not report one.
    pub device: String,
    /// Classified failure reason.
    pub kind: GraphFailureKind,
}

impl HardwareGraphDenial {
    /// Create a graph denial row.
    #[must_use]
    pub fn new(
        operation: HardwareGraphOperation,
        device: impl Into<String>,
        kind: GraphFailureKind,
    ) -> Self {
        Self {
            operation,
            device: device.into(),
            kind,
        }
    }

    /// Return whether this denial applies to an operation on a device.
    ///
    /// A denial learned against an unknown device (empty string) applies
    /// broadly, because the alternative is retrying a known-broken graph on
    /// every playback.
    #[must_use]
    pub fn matches(&self, operation: HardwareGraphOperation, device: &str) -> bool {
        self.operation == operation
            && (self.device.is_empty()
                || device.is_empty()
                || self.device.eq_ignore_ascii_case(device))
    }

    /// Operator-facing evidence line for health and troubleshooting.
    #[must_use]
    pub fn evidence(&self) -> String {
        let device = if self.device.is_empty() {
            "unknown device".to_owned()
        } else {
            self.device.clone()
        };
        format!(
            "{} is unavailable on {device}: {}",
            self.operation.id(),
            self.kind.reason()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_ids_round_trip() {
        for operation in [
            HardwareGraphOperation::NvencDolbyVisionProfile5Sdr,
            HardwareGraphOperation::NvencDolbyVisionProfile5Hdr10,
            HardwareGraphOperation::NvencHdrVulkanLibplacebo,
            HardwareGraphOperation::NvencCudaScale,
        ] {
            assert_eq!(
                HardwareGraphOperation::from_id(operation.id()),
                Some(operation)
            );
        }
        assert_eq!(HardwareGraphOperation::from_id("nonsense"), None);
    }

    #[test]
    fn the_hdr_graph_falls_back_to_cuda_and_keeps_hardware_encoding() {
        assert_eq!(
            HardwareGraphOperation::NvencHdrVulkanLibplacebo.fallback(),
            Some(HardwareGraphOperation::NvencCudaScale)
        );
        // Exactly one retry: the fallback has no further hardware graph.
        assert_eq!(HardwareGraphOperation::NvencCudaScale.fallback(), None);
    }

    #[test]
    fn probe_commands_are_bounded_and_touch_no_user_media() {
        for operation in [
            HardwareGraphOperation::NvencDolbyVisionProfile5Sdr,
            HardwareGraphOperation::NvencDolbyVisionProfile5Hdr10,
            HardwareGraphOperation::NvencHdrVulkanLibplacebo,
            HardwareGraphOperation::NvencCudaScale,
        ] {
            let args = operation.probe_arguments();
            let joined = args.join(" ");

            assert!(joined.contains("-f lavfi"), "{joined}");
            assert!(joined.contains("-frames:v 1"), "{joined}");
            assert!(joined.ends_with("-f null -"), "{joined}");
            assert!(joined.contains("-nostdin"), "{joined}");
            // NVENC's minimum frame dimension, learned the hard way.
            assert!(
                joined.contains("256:144") || joined.contains("w=256:h=144"),
                "{joined}"
            );
        }
    }

    #[test]
    fn profile_5_probes_match_the_copy_bearing_playback_graphs() {
        let sdr = HardwareGraphOperation::NvencDolbyVisionProfile5Sdr
            .probe_arguments()
            .join(" ");
        let hdr10 = HardwareGraphOperation::NvencDolbyVisionProfile5Hdr10
            .probe_arguments()
            .join(" ");

        for graph in [&sdr, &hdr10] {
            assert!(graph.contains("-init_hw_device cuda=nv:0"), "{graph}");
            assert!(graph.contains("-init_hw_device vulkan=vk@nv"), "{graph}");
            assert!(graph.contains("-filter_hw_device vk"), "{graph}");
            assert!(graph.contains("hwupload_cuda=device=0"), "{graph}");
            assert!(
                graph.contains("hwdownload,format=p010le,hwupload"),
                "{graph}"
            );
            assert!(graph.contains("apply_dolbyvision=true"), "{graph}");
            assert!(!graph.contains("hwmap=derive_device=cuda"), "{graph}");
            assert!(!graph.contains("scale_cuda"), "{graph}");
        }
        assert!(sdr.contains("format=rgba,format=yuv420p"), "{sdr}");
        assert!(sdr.contains("h264_nvenc"), "{sdr}");
        assert!(hdr10.contains("format=rgba64le"), "{hdr10}");
        assert!(hdr10.contains("format=yuv420p10le"), "{hdr10}");
        assert!(hdr10.contains("hevc_nvenc -profile:v main10"), "{hdr10}");
    }

    #[test]
    fn the_hdr_probe_runs_the_real_graph_shape() {
        let joined = HardwareGraphOperation::NvencHdrVulkanLibplacebo
            .probe_arguments()
            .join(" ");

        // Every component that has to interoperate must appear, or the probe
        // proves nothing that the advertised hwaccel list did not already.
        // The Vulkan device is derived from the selected CUDA device, exactly
        // as playback does it — not a default Vulkan device.
        assert!(joined.contains("-init_hw_device cuda=nv"), "{joined}");
        assert!(joined.contains("-init_hw_device vulkan=vk@nv"), "{joined}");
        assert!(joined.contains("-filter_hw_device vk"), "{joined}");
        assert!(joined.contains("libplacebo"), "{joined}");
        assert!(joined.contains("hwmap=derive_device=cuda"), "{joined}");
        assert!(joined.contains("scale_cuda"), "{joined}");
        assert!(joined.contains("h264_nvenc"), "{joined}");
    }

    #[test]
    fn classifies_the_measured_wsl2_vulkan_cuda_interop_failure() {
        // Captured verbatim from FFmpeg 9.0 on WSL2 with an RTX 3080 Ti, where
        // the only Vulkan device is llvmpipe.
        let stderr = "[CUDA @ 0x72734c030580] Could not derive CUDA device.\n\
             [CUDA @ 0x72734c030580] cu->cuCtxPushCurrent(hwctx->cuda_ctx) failed -> CUDA_ERROR_INVALID_VALUE: invalid argument\n\
             [Parsed_scale_cuda_4 @ 0x72734c005700] Failed to configure output pad on Parsed_scale_cuda_4\n\
             [vf#0:0 @ 0x5a3660d30540] Error reinitializing filters!";

        assert_eq!(
            GraphFailureKind::classify(stderr),
            Some(GraphFailureKind::CudaInterop),
            "the interop reason must win over the generic hardware-map class"
        );
    }

    #[test]
    fn classifies_a_missing_vulkan_device() {
        let stderr = "[Vulkan @ 0x5cb621757c00] Unable to find device with index 99!\n\
             Device creation failed: -19.\n\
             Failed to set value 'vulkan=vk:99' for option 'init_hw_device': No such device";

        assert_eq!(
            GraphFailureKind::classify(stderr),
            Some(GraphFailureKind::VulkanDevice)
        );
    }

    #[test]
    fn classifies_libplacebos_missing_vulkan_device_as_a_device_problem() {
        // Captured from FFmpeg 9.0: the *non*-Vulkan NVENC HDR chain still
        // runs libplacebo, which needs a Vulkan device anyway. The actionable
        // diagnosis is the missing device, not "libplacebo".
        let stderr = "[libplacebo @ 0x621bcb770640] Found no suitable device, giving up.\n\
             [libplacebo @ 0x621bcb770640] Failed initializing vulkan device\n\
             [Parsed_libplacebo_1 @ 0x621bcb770540] Failed creating Vulkan device!\n\
             [AVFilterGraph @ 0x621bcb76fb00] Error initializing filters";

        assert_eq!(
            GraphFailureKind::classify(stderr),
            Some(GraphFailureKind::VulkanDevice)
        );
    }

    #[test]
    fn classifies_a_missing_filter_and_an_encoder_failure() {
        assert_eq!(
            GraphFailureKind::classify(
                "[AVFilterGraph @ 0x640689af9840] No such filter: 'libplacebo'"
            ),
            Some(GraphFailureKind::MissingFilter)
        );
        assert_eq!(
            GraphFailureKind::classify(
                "[h264_nvenc @ 0x1] InitializeEncoder failed: invalid param (8): Frame Dimension less than the minimum supported value."
            ),
            Some(GraphFailureKind::EncoderInit)
        );
    }

    #[test]
    fn unrelated_failures_are_not_classified_as_graph_denials() {
        // ADR 0014: never learn a denial from an arbitrary FFmpeg failure.
        for stderr in [
            "",
            "No space left on device",
            "Invalid data found when processing input",
            "Connection timed out",
        ] {
            assert_eq!(GraphFailureKind::classify(stderr), None, "{stderr}");
        }
    }

    #[test]
    fn runtime_classification_requires_operation_specific_evidence() {
        for operation in [
            HardwareGraphOperation::NvencHdrVulkanLibplacebo,
            HardwareGraphOperation::NvencCudaScale,
        ] {
            for stderr in [
                "Error reinitializing filters!",
                "Failed to configure output pad on Parsed_overlay_3",
                "Error while opening encoder for output stream #0:0",
                "InitializeEncoder failed: invalid param",
                "No such filter: 'subtitles'",
                "[Parsed_libplacebo_1] using vulkan device\n[Parsed_overlay_3] Failed to configure output pad",
            ] {
                assert_eq!(
                    GraphFailureKind::classify_runtime(operation, stderr),
                    None,
                    "{operation:?}: {stderr}"
                );
            }
        }
    }

    #[test]
    fn runtime_classification_accepts_selected_graph_evidence() {
        assert_eq!(
            GraphFailureKind::classify_runtime(
                HardwareGraphOperation::NvencHdrVulkanLibplacebo,
                "[CUDA @ 0x1] Could not derive CUDA device."
            ),
            Some(GraphFailureKind::CudaInterop)
        );
        assert_eq!(
            GraphFailureKind::classify_runtime(
                HardwareGraphOperation::NvencCudaScale,
                "[Parsed_scale_cuda_4] Failed to configure output pad"
            ),
            Some(GraphFailureKind::HardwareMap)
        );
        assert_eq!(
            GraphFailureKind::classify_runtime(
                HardwareGraphOperation::NvencCudaScale,
                "No such filter: 'hwupload_cuda'"
            ),
            Some(GraphFailureKind::MissingFilter)
        );
        assert_eq!(
            GraphFailureKind::classify_runtime(
                HardwareGraphOperation::NvencHdrVulkanLibplacebo,
                "[Parsed_hwmap_3] Failed to configure output pad"
            ),
            Some(GraphFailureKind::HardwareMap)
        );
    }

    #[test]
    fn denials_match_by_operation_and_device() {
        let denial = HardwareGraphDenial::new(
            HardwareGraphOperation::NvencHdrVulkanLibplacebo,
            "NVIDIA GeForce RTX 3080 Ti",
            GraphFailureKind::CudaInterop,
        );

        assert!(denial.matches(
            HardwareGraphOperation::NvencHdrVulkanLibplacebo,
            "NVIDIA GeForce RTX 3080 Ti"
        ));
        // A different operation on the same device is unaffected.
        assert!(!denial.matches(
            HardwareGraphOperation::NvencCudaScale,
            "NVIDIA GeForce RTX 3080 Ti"
        ));
        // A different GPU must re-probe rather than inherit the denial.
        assert!(!denial.matches(
            HardwareGraphOperation::NvencHdrVulkanLibplacebo,
            "NVIDIA GeForce RTX 4090"
        ));
    }

    #[test]
    fn evidence_names_the_operation_device_and_reason() {
        let denial = HardwareGraphDenial::new(
            HardwareGraphOperation::NvencHdrVulkanLibplacebo,
            "NVIDIA GeForce RTX 3080 Ti",
            GraphFailureKind::CudaInterop,
        );

        assert_eq!(
            denial.evidence(),
            "nvenc-hdr-vulkan-libplacebo is unavailable on NVIDIA GeForce RTX 3080 Ti: Vulkan and CUDA are not interoperable on this device"
        );
    }
}
