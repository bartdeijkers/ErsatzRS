use axum::{
    extract::{Form, State},
    response::Redirect,
};
use serde::Deserialize;

use crate::admin::server_fns::ffmpeg_profiles::{
    ffmpeg_profile_form_into_admin, h264_profile_for_bit_depth, video_format_validation_error,
    video_profile_options, FfmpegProfileForm, HDR10_BIT_DEPTH_ERROR, HDR10_CODEC_ERROR,
};
use crate::host::state::AppState;

#[derive(Debug, Deserialize)]
pub struct FfmpegProfileSaveForm {
    id: i64,
    name: String,
    hardware_acceleration: Option<i32>,
    video_format: Option<i32>,
    video_profile: Option<String>,
    bit_depth: Option<i32>,
}

pub async fn save_ffmpeg_profile_form(
    State(state): State<AppState>,
    Form(input): Form<FfmpegProfileSaveForm>,
) -> Redirect {
    let name = input.name.trim();
    if input.id < 0 {
        return error_redirect(input.id, "validation");
    }
    if name.is_empty() {
        return error_redirect(input.id, "name-required");
    }
    if name.chars().count() > 50 {
        return error_redirect(input.id, "name-too-long");
    }

    let mut form = if input.id == 0 {
        ffmpeg_profile_form_into_admin(FfmpegProfileForm::new_defaults())
    } else {
        match state.application.get_ffmpeg_profile(input.id).await {
            Ok(Some(form)) => form,
            Ok(None) => return error_redirect(input.id, "save"),
            Err(error) => {
                tracing::error!(%error, profile_id = input.id, "failed to load FFmpeg profile for HTML form");
                return error_redirect(input.id, "save");
            }
        }
    };
    name.clone_into(&mut form.name);

    if let (Some(hw), Some(vf), Some(bit_depth)) = (
        input.hardware_acceleration,
        input.video_format,
        input.bit_depth,
    ) {
        if video_format_validation_error(hw, vf).is_some() {
            return error_redirect(input.id, &format!("format-{hw}"));
        }
        if vf == 3 && bit_depth == 1 {
            return error_redirect(input.id, "mpeg2-ten-bit");
        }

        let requested_profile = input.video_profile.unwrap_or_default();
        form.hardware_acceleration = hw;
        form.video_format = vf;
        form.video_profile =
            fallback_video_profile(hw, vf, bit_depth, &requested_profile).unwrap_or_default();
        form.bit_depth = if vf == 3 { 0 } else { bit_depth };
    }

    match state
        .application
        .save_ffmpeg_profile(ersatzrs_app::commands::SaveFfmpegProfile { form })
        .await
    {
        // The list is interactive. Complete HTML saves on the populated editor
        // so a no-JS user can inspect the exact persisted result.
        Ok(id) => Redirect::to(&format!("/ffmpeg/{id}")),
        Err(
            ersatzrs_app::ApplicationError::Validation { field: "name", .. }
            | ersatzrs_app::ApplicationError::Conflict(_),
        ) => error_redirect(input.id, "duplicate"),
        Err(ersatzrs_app::ApplicationError::Validation { message, .. }) => {
            let code = match message.as_str() {
                "VideoProfile cannot be high10 with 8-bit h264" => "profile-eight-bit",
                "VideoProfile must be high10 with 10-bit h264" => "profile-ten-bit",
                "VideoProfile must be high444p with 10-bit h264" => "profile-nvenc-ten-bit",
                HDR10_CODEC_ERROR => "hdr-codec",
                HDR10_BIT_DEPTH_ERROR => "hdr-depth",
                _ => "validation",
            };
            error_redirect(input.id, code)
        }
        Err(error) => {
            tracing::error!(%error, profile_id = input.id, "failed to save FFmpeg profile from HTML form");
            error_redirect(input.id, "save")
        }
    }
}

fn error_redirect(id: i64, code: &str) -> Redirect {
    let path = if id <= 0 {
        "/ffmpeg/add".to_owned()
    } else {
        format!("/ffmpeg/{id}")
    };
    Redirect::to(&format!("{path}?error={code}"))
}

fn fallback_video_profile(hw: i32, vf: i32, bit_depth: i32, requested: &str) -> Option<String> {
    if vf != 1 {
        return None;
    }

    let requested = h264_profile_for_bit_depth(hw, bit_depth, requested);
    if bit_depth == 1 {
        return Some(requested.to_owned());
    }
    video_profile_options(hw, vf)
        .contains(&requested)
        .then(|| requested.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin::server_fns::ffmpeg_profiles::{
        VIDEO_PROFILE_HIGH, VIDEO_PROFILE_HIGH10, VIDEO_PROFILE_HIGH444P, VIDEO_PROFILE_MAIN,
    };

    #[test]
    fn fallback_video_profile_persists_h264_select_values() {
        assert_eq!(
            fallback_video_profile(2, 1, 0, VIDEO_PROFILE_HIGH),
            Some(VIDEO_PROFILE_HIGH.to_owned())
        );
        assert_eq!(
            fallback_video_profile(0, 1, 0, VIDEO_PROFILE_MAIN),
            Some(VIDEO_PROFILE_MAIN.to_owned())
        );
        assert_eq!(fallback_video_profile(0, 2, 0, VIDEO_PROFILE_MAIN), None);
    }

    #[test]
    fn fallback_video_profile_matches_ersatztv_ten_bit_getter() {
        assert_eq!(
            fallback_video_profile(2, 1, 1, VIDEO_PROFILE_HIGH),
            Some(VIDEO_PROFILE_HIGH444P.to_owned())
        );
        assert_eq!(
            fallback_video_profile(0, 1, 1, VIDEO_PROFILE_HIGH),
            Some(VIDEO_PROFILE_HIGH10.to_owned())
        );
    }

    #[test]
    fn fallback_depth_transition_preserves_valid_eight_bit_choices() {
        for hw in 0..=7 {
            assert_eq!(
                fallback_video_profile(hw, 1, 0, VIDEO_PROFILE_HIGH10),
                video_profile_options(hw, 1)
                    .contains(&VIDEO_PROFILE_HIGH)
                    .then(|| VIDEO_PROFILE_HIGH.to_owned())
            );
        }
        for choice in [
            VIDEO_PROFILE_MAIN,
            VIDEO_PROFILE_HIGH,
            VIDEO_PROFILE_HIGH444P,
        ] {
            assert_eq!(
                fallback_video_profile(2, 1, 0, choice),
                Some(choice.to_owned())
            );
        }
    }
}
