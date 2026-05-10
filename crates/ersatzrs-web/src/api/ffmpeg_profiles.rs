use axum::{
    extract::{Form, State},
    response::Redirect,
};
use serde::Deserialize;

use crate::host::state::AppState;

#[derive(Debug, Deserialize)]
pub struct FfmpegProfileSaveForm {
    id: i64,
    name: String,
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

    match sqlx::query(r#"UPDATE "FFmpegProfile" SET "Name" = ? WHERE "Id" = ?"#)
        .bind(name)
        .bind(input.id)
        .execute(&state.pool)
        .await
    {
        Ok(_) => Redirect::to("/ffmpeg"),
        Err(error) => {
            tracing::error!(%error, profile_id = input.id, "failed to save FFmpeg profile from HTML form");
            Redirect::to(&format!("/ffmpeg/{}?error=save", input.id))
        }
    }
}
