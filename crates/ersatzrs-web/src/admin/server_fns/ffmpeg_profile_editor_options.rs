//! Row282: derive editable H264 depth choices and preserve actionable save errors.
use leptos::prelude::ServerFnError;

use super::{
    video_format_validation_error, HDR10_BIT_DEPTH_ERROR, HDR10_CODEC_ERROR, VIDEO_PROFILE_HIGH,
    VIDEO_PROFILE_HIGH10, VIDEO_PROFILE_HIGH444P,
};
use crate::admin::i18n::{ffmpeg_profile_error_message, save_failed};

pub(super) const PROFILE_SAVE_FAILED: &str =
    "The FFmpeg profile could not be saved. Check the application log.";
const PROFILE_SETTINGS_INVALID: &str =
    "The FFmpeg profile settings are invalid. Review the selected options.";
const PROFILE_NAME_DUPLICATE: &str = "An FFmpeg profile with this name already exists.";

pub(crate) fn h264_profile_for_bit_depth(hw: i32, depth: i32, requested: &str) -> &str {
    if depth == 1 {
        if hw == 2 {
            VIDEO_PROFILE_HIGH444P
        } else {
            VIDEO_PROFILE_HIGH10
        }
    } else if requested.trim() == VIDEO_PROFILE_HIGH10 {
        VIDEO_PROFILE_HIGH
    } else {
        requested.trim()
    }
}

/// URL codes are a closed vocabulary; arbitrary query text is never echoed.
pub(crate) fn profile_form_error_message(code: &str) -> Option<&'static str> {
    match code {
        "name-required" => Some("Profile name is required."),
        "name-too-long" => Some("Profile name must be 50 characters or fewer."),
        "duplicate" => Some(PROFILE_NAME_DUPLICATE),
        "mpeg2-ten-bit" => Some("Mpeg2Video does not support 10-bit content"),
        "profile-eight-bit" => Some("VideoProfile cannot be high10 with 8-bit h264"),
        "profile-ten-bit" => Some("VideoProfile must be high10 with 10-bit h264"),
        "profile-nvenc-ten-bit" => Some("VideoProfile must be high444p with NVIDIA 10-bit h264"),
        "hdr-codec" => Some(HDR10_CODEC_ERROR),
        "hdr-depth" => Some(HDR10_BIT_DEPTH_ERROR),
        "validation" => Some(PROFILE_SETTINGS_INVALID),
        "save" => Some(PROFILE_SAVE_FAILED),
        _ => code
            .strip_prefix("format-")
            .and_then(|hw| hw.parse::<i32>().ok())
            .filter(|hw| matches!(hw, 1..=7))
            .and_then(|hw| video_format_validation_error(hw, 99)),
    }
}

pub(crate) fn profile_save_feedback(error: ServerFnError) -> String {
    let message = match error {
        ServerFnError::ServerError(message) => message,
        _ => PROFILE_SAVE_FAILED.to_owned(),
    };
    save_failed(ffmpeg_profile_error_message(message))
}

#[cfg(feature = "ssr")]
pub(crate) fn profile_save_server_error(error: ersatzrs_app::ApplicationError) -> ServerFnError {
    use ersatzrs_app::ApplicationError;
    let message = match error {
        ApplicationError::Validation { message, .. } | ApplicationError::Conflict(message) => {
            if message == "VideoProfile must be high444p with 10-bit h264" {
                "VideoProfile must be high444p with NVIDIA 10-bit h264".to_owned()
            } else {
                message
            }
        }
        error => {
            tracing::error!(%error, "Failed to save FFmpeg profile");
            PROFILE_SAVE_FAILED.to_owned()
        }
    };
    ServerFnError::new(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin::i18n::{ffmpeg_profile_error_text, UiLanguage};
    use leptos::prelude::{provide_context, Owner};

    #[test]
    fn h264_depth_changes_preserve_valid_native_choices() {
        for hw in 0..=7 {
            for stored in ["", "main", "high", "high10", "high444p", " "] {
                assert_eq!(
                    h264_profile_for_bit_depth(hw, 1, stored),
                    if hw == 2 { "high444p" } else { "high10" }
                );
                assert_eq!(
                    h264_profile_for_bit_depth(hw, 0, stored),
                    if stored == "high10" {
                        "high"
                    } else {
                        stored.trim()
                    }
                );
            }
        }
    }

    #[test]
    fn html_save_errors_are_closed_and_localized_in_every_culture() {
        for code in [
            "",
            "format-0",
            "format-99",
            "format--1",
            "<script>synthetic</script>",
        ] {
            assert!(profile_form_error_message(code).is_none());
        }
        for registration in crate::admin::languages::LANGUAGES {
            let language = UiLanguage::from_config(registration.code);
            for code in [
                "name-required",
                "name-too-long",
                "duplicate",
                "validation",
                "save",
                "mpeg2-ten-bit",
                "profile-eight-bit",
                "profile-ten-bit",
                "profile-nvenc-ten-bit",
                "hdr-codec",
                "hdr-depth",
                "format-1",
                "format-2",
                "format-3",
                "format-4",
                "format-5",
                "format-6",
                "format-7",
            ] {
                let message = profile_form_error_message(code).unwrap();
                let localized = ffmpeg_profile_error_text(language, message);
                assert!(!localized.is_empty());
                if registration.code != "en-us"
                    && matches!(code, "duplicate" | "validation" | "save")
                {
                    assert_ne!(localized, message, "{code} in {}", registration.code);
                }
            }
        }
    }

    #[test]
    fn save_feedback_unwraps_transport_and_application_error_wrappers() {
        for registration in crate::admin::languages::LANGUAGES {
            let language = UiLanguage::from_config(registration.code);
            let owner = Owner::new();
            owner.with(|| {
                provide_context(language);
                for message in [
                    "VideoProfile must be high10 with 10-bit h264",
                    PROFILE_NAME_DUPLICATE,
                ] {
                    let expected = save_failed(ffmpeg_profile_error_text(language, message));
                    assert_eq!(profile_save_feedback(ServerFnError::new(message)), expected);
                }
            });
        }
    }

    #[cfg(feature = "ssr")]
    #[test]
    fn typed_validation_keeps_its_cause_and_operational_details_stay_in_logs() {
        let error = profile_save_server_error(ersatzrs_app::ApplicationError::Validation {
            field: "video_profile",
            message: "VideoProfile must be high444p with 10-bit h264".into(),
        });
        assert!(
            matches!(error, ServerFnError::ServerError(message) if message == "VideoProfile must be high444p with NVIDIA 10-bit h264")
        );
        let error = profile_save_server_error(ersatzrs_app::ApplicationError::Unavailable {
            operation: "synthetic storage",
            message: "synthetic operational detail".into(),
        });
        assert!(
            matches!(error, ServerFnError::ServerError(message) if message == PROFILE_SAVE_FAILED)
        );
    }
}
