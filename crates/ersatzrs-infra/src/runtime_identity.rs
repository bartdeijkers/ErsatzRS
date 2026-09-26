use ersatzrs_ffmpeg::{FfmpegCapabilityReport, FfmpegRuntimeIdentity, NvidiaDeviceIdentity};
use serde::{Deserialize, Serialize};

/// Persistable portion of a validated runtime identity.
///
/// Publication generations are deliberately excluded: they scope in-process
/// feedback, while this value decides whether evidence may survive a restart.
/// Container driver exposure is included because identical binaries and GPUs
/// can have different usable libraries after an image/environment change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PersistedRuntimeIdentity {
    ffmpeg_path: String,
    ffmpeg_sha256: String,
    ffprobe_path: String,
    ffprobe_sha256: String,
    selected_nvidia_device: Option<PersistedNvidiaDeviceIdentity>,
    /// Changes which host driver libraries the NVIDIA container runtime exposes.
    #[serde(default)]
    nvidia_driver_capabilities: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct PersistedNvidiaDeviceIdentity {
    index: u32,
    model: String,
    uuid: Option<String>,
    driver_version: Option<String>,
}

impl PersistedRuntimeIdentity {
    pub(crate) fn from_capabilities(report: &FfmpegCapabilityReport) -> Option<Self> {
        let identity = report.runtime_identity.as_ref()?;
        if report.has_hwaccel("cuda") && identity.selected_nvidia_device.is_none() {
            return None;
        }
        Self::from_runtime(identity)
    }

    fn from_runtime(identity: &FfmpegRuntimeIdentity) -> Option<Self> {
        Some(Self {
            ffmpeg_path: identity.ffmpeg_path.clone(),
            ffmpeg_sha256: identity.ffmpeg_sha256.clone()?,
            ffprobe_path: identity.ffprobe_path.clone(),
            ffprobe_sha256: identity.ffprobe_sha256.clone()?,
            selected_nvidia_device: identity
                .selected_nvidia_device
                .as_ref()
                .map(PersistedNvidiaDeviceIdentity::from),
            nvidia_driver_capabilities: crate::startup_config::environment_variable(
                "NVIDIA_DRIVER_CAPABILITIES",
            )
            .and_then(|value| value.into_string().ok()),
        })
    }

    pub(crate) fn matches(&self, report: &FfmpegCapabilityReport) -> bool {
        Self::from_capabilities(report).as_ref() == Some(self)
    }
}

impl From<&NvidiaDeviceIdentity> for PersistedNvidiaDeviceIdentity {
    fn from(value: &NvidiaDeviceIdentity) -> Self {
        Self {
            index: value.index,
            model: value.model.clone(),
            uuid: value.uuid.clone(),
            driver_version: value.driver_version.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use ersatzrs_ffmpeg::{FfmpegRuntimeIdentity, FfmpegRuntimeReport};

    use super::PersistedRuntimeIdentity;

    #[test]
    fn incomplete_test_runtime_identity_is_not_persistable() {
        let report = FfmpegRuntimeReport::for_tests();
        assert!(PersistedRuntimeIdentity::from_capabilities(&report.capabilities).is_none());
    }

    #[test]
    fn cuda_runtime_without_a_selected_device_identity_is_not_persistable() {
        let mut report = FfmpegRuntimeReport::for_tests();
        report.capabilities.hwaccels.insert("cuda".to_owned());
        report.capabilities.runtime_identity = Some(FfmpegRuntimeIdentity {
            generation: 1,
            ffmpeg_path: "/opt/ffmpeg".to_owned(),
            ffmpeg_sha256: Some("ffmpeg-digest".to_owned()),
            ffprobe_path: "/opt/ffprobe".to_owned(),
            ffprobe_sha256: Some("ffprobe-digest".to_owned()),
            selected_nvidia_device: None,
        });

        assert!(PersistedRuntimeIdentity::from_capabilities(&report.capabilities).is_none());
    }
}
