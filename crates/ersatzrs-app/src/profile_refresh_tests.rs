//! Verify saved-profile changes invalidate the derived Next graphics inputs.
use super::tests::{FailingFfmpegRepository, OptionalFfmpegProfile};
use crate::{ChannelRuntime, ChannelRuntimeError};
use ersatzrs_domain::{FFmpegProfile, FFmpegProfileVideoFormat, ScalingBehavior};
use std::sync::Mutex;

#[derive(Default)]
struct RecordingRuntime(Mutex<Vec<i64>>);

#[async_trait::async_trait]
impl ChannelRuntime for RecordingRuntime {
    async fn active_channel_numbers(&self) -> Result<Vec<String>, ChannelRuntimeError> {
        panic!("profile refresh must not enumerate active sessions")
    }

    async fn session_models(
        &self,
    ) -> Result<Vec<ersatzrs_core::hls::HlsSessionModel>, ChannelRuntimeError> {
        panic!("profile refresh must not inspect or reset sessions")
    }

    async fn stop_channel_session(&self, _: &str) -> Result<bool, ChannelRuntimeError> {
        panic!("profile refresh must not stop playback")
    }

    fn request_subtitle_extraction(&self, _: Vec<i64>) {
        panic!("profile refresh must not enqueue extraction")
    }

    async fn refresh_profile_playouts(&self, id: i64) -> Result<(), ChannelRuntimeError> {
        self.0.lock().unwrap().push(id);
        Ok(())
    }
}

#[tokio::test]
async fn profile_graphics_changes_refresh_scaling_copy_and_resolution_only() {
    for change in [
        "unchanged",
        "name",
        "bitrate",
        "scaling",
        "copy",
        "resolution",
    ] {
        let before = FFmpegProfile::default();
        let mut after = before.clone();
        match change {
            "name" => after.name = "Renamed".into(),
            "bitrate" => after.video_bitrate += 1,
            "scaling" => after.scaling_behavior = ScalingBehavior::Stretch,
            "copy" => after.video_format = FFmpegProfileVideoFormat::Copy,
            "resolution" => after.resolution_id += 1,
            _ => {}
        }
        let runtime = RecordingRuntime::default();
        let saved = crate::commands::save_profile_and_refresh(
            &after,
            Some(&before),
            &FailingFfmpegRepository {
                optional: OptionalFfmpegProfile::Missing,
            },
            &runtime,
        )
        .await
        .unwrap();
        assert_eq!(saved, 1);
        let expected = if matches!(change, "scaling" | "copy" | "resolution") {
            vec![saved]
        } else {
            Vec::new()
        };
        assert_eq!(*runtime.0.lock().unwrap(), expected, "{change}");
    }
}

#[test]
fn raw_profile_requests_keep_strict_bit_depth_validation() {
    for hw in 0..=7 {
        let required = if hw == 2 { "high444p" } else { "high10" };
        assert!(super::validate_ffmpeg_profile_options(hw, 1, 1, required).is_ok());
        for invalid in ["", "main", "high"] {
            assert!(matches!(
                super::validate_ffmpeg_profile_options(hw, 1, 1, invalid),
                Err(crate::ApplicationError::Validation {
                    field: "video_profile",
                    ..
                })
            ));
        }
        assert!(matches!(
            super::validate_ffmpeg_profile_options(hw, 1, 0, "high10"),
            Err(crate::ApplicationError::Validation {
                field: "video_profile",
                ..
            })
        ));
        assert!(super::validate_ffmpeg_profile_options(hw, 1, 0, "main").is_ok());
        assert!(super::validate_ffmpeg_profile_options(hw, 1, 0, "high").is_ok());
    }
}
