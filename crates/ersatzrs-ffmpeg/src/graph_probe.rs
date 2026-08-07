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
//! same machine. See `docs/how-to/hdr-output-characterization.md` and the
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
            Self::NvencHdrVulkanLibplacebo => "nvenc-hdr-vulkan-libplacebo",
            Self::NvencCudaScale => "nvenc-cuda-scale",
        }
    }

    /// Parse a persisted identifier.
    #[must_use]
    pub fn from_id(value: &str) -> Option<Self> {
        match value {
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
            Self::NvencCudaScale => None,
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
            Self::NvencHdrVulkanLibplacebo => {
                args.extend(
                    [
                        "-init_hw_device",
                        "vulkan=vk",
                        "-filter_hw_device",
                        "vk",
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
                args.extend(
                    [
                        "-init_hw_device",
                        "cuda=cu",
                        "-filter_hw_device",
                        "cu",
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
            assert!(joined.contains("256:144"), "{joined}");
        }
    }

    #[test]
    fn the_hdr_probe_runs_the_real_graph_shape() {
        let joined = HardwareGraphOperation::NvencHdrVulkanLibplacebo
            .probe_arguments()
            .join(" ");

        // Every component that has to interoperate must appear, or the probe
        // proves nothing that the advertised hwaccel list did not already.
        assert!(joined.contains("-init_hw_device vulkan=vk"), "{joined}");
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
