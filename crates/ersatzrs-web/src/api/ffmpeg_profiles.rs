use axum::{
    extract::{Form, State},
    response::Redirect,
};
use serde::Deserialize;

use crate::admin::server_fns::ffmpeg_profiles::{
    video_format_validation_error, video_profile_options, VIDEO_PROFILE_HIGH10,
    VIDEO_PROFILE_HIGH444P,
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
    if input.id <= 0 || name.is_empty() || name.len() > 50 {
        return Redirect::to(&format!("/ffmpeg/{}?error=save", input.id.max(0)));
    }

    let duplicate = sqlx::query_scalar::<_, i64>(
        r#"SELECT COUNT(*) FROM "FFmpegProfile" WHERE "Name" = ? AND "Id" != ?"#,
    )
    .bind(name)
    .bind(input.id)
    .fetch_one(&state.pool)
    .await;

    match duplicate {
        Ok(0) => {}
        Ok(_) => return Redirect::to(&format!("/ffmpeg/{}?error=duplicate", input.id)),
        Err(error) => {
            tracing::error!(%error, "failed to check duplicate FFmpeg profile name");
            return Redirect::to(&format!("/ffmpeg/{}?error=save", input.id));
        }
    }

    let result = if let (Some(hw), Some(vf), Some(bit_depth)) = (
        input.hardware_acceleration,
        input.video_format,
        input.bit_depth,
    ) {
        if video_format_validation_error(hw, vf).is_some() || vf == 3 && bit_depth == 1 {
            return Redirect::to(&format!("/ffmpeg/{}?error=validation", input.id));
        }

        let requested_profile = input.video_profile.unwrap_or_default();
        let video_profile = fallback_video_profile(hw, vf, bit_depth, &requested_profile);
        let bit_depth = if vf == 3 { 0 } else { bit_depth };

        sqlx::query(
            r#"UPDATE "FFmpegProfile"
               SET "Name" = ?, "HardwareAcceleration" = ?, "VideoFormat" = ?,
                   "VideoProfile" = ?, "BitDepth" = ?
               WHERE "Id" = ?"#,
        )
        .bind(name)
        .bind(hw)
        .bind(vf)
        .bind(video_profile)
        .bind(bit_depth)
        .bind(input.id)
        .execute(&state.pool)
        .await
    } else {
        sqlx::query(r#"UPDATE "FFmpegProfile" SET "Name" = ? WHERE "Id" = ?"#)
            .bind(name)
            .bind(input.id)
            .execute(&state.pool)
            .await
    };

    match result {
        Ok(_) => Redirect::to("/ffmpeg"),
        Err(error) => {
            tracing::error!(%error, profile_id = input.id, "failed to save FFmpeg profile from HTML form");
            Redirect::to(&format!("/ffmpeg/{}?error=save", input.id))
        }
    }
}

fn fallback_video_profile(hw: i32, vf: i32, bit_depth: i32, requested: &str) -> Option<String> {
    if vf != 1 {
        return None;
    }

    if bit_depth == 1 {
        return Some(
            if hw == 2 {
                VIDEO_PROFILE_HIGH444P
            } else {
                VIDEO_PROFILE_HIGH10
            }
            .to_owned(),
        );
    }

    let requested = requested.trim();
    video_profile_options(hw, vf)
        .contains(&requested)
        .then(|| requested.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin::server_fns::ffmpeg_profiles::{VIDEO_PROFILE_HIGH, VIDEO_PROFILE_MAIN};

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
}
