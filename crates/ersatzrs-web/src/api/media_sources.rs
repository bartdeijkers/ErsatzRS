use axum::{
    extract::{Form, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
};
use serde::Deserialize;

use crate::host::state::AppState;

#[derive(Debug, Deserialize)]
pub struct LocalLibrarySaveForm {
    id: i64,
    name: String,
    media_kind: i32,
    #[serde(default)]
    paths_text: String,
    #[serde(default)]
    intent: String,
}

pub async fn save_local_library_form(
    State(state): State<AppState>,
    Form(input): Form<LocalLibrarySaveForm>,
) -> Response {
    let paths = input
        .paths_text
        .lines()
        .filter(|path| !path.trim().is_empty())
        .map(|path| ersatzrs_core::repositories::LocalLibraryPathAdmin {
            id: 0,
            path: path.to_string(),
        })
        .collect();

    let form = ersatzrs_core::repositories::LocalLibraryAdmin {
        id: input.id,
        name: input.name,
        media_kind: input.media_kind,
        paths,
    };

    if input.intent == "preview" {
        return preview_local_library(&state, form).await;
    }

    match state.application.save_local_library(form.clone()).await {
        Ok(_) => Redirect::to("/media/sources/local").into_response(),
        Err(error) => {
            // Paths belong in the operator response, never in service logs.
            tracing::debug!(library_id = input.id, "local library save rejected");
            let (language, is_dark_mode) = local_library_page_preferences(&state).await;
            let message =
                crate::admin::server_fns::media_sources::local_library_error_text(language, &error);
            let draft = crate::admin::server_fns::media_sources::LocalLibraryForm {
                id: form.id,
                name: form.name,
                media_kind: form.media_kind,
                paths: form
                    .paths
                    .into_iter()
                    .map(
                        |path| crate::admin::server_fns::media_sources::LocalLibraryPath {
                            id: path.id,
                            path: path.path,
                        },
                    )
                    .collect(),
            };
            (
                StatusCode::BAD_REQUEST,
                Html(
                    crate::admin::pages::media_sources::local::local_library_error_document(
                        draft,
                        message,
                        language,
                        is_dark_mode,
                    ),
                ),
            )
                .into_response()
        }
    }
}

async fn preview_local_library(
    state: &AppState,
    form: ersatzrs_core::repositories::LocalLibraryAdmin,
) -> Response {
    use crate::admin::i18n::TextKey;
    let (language, is_dark_mode) = local_library_page_preferences(state).await;
    let outcome = state.application.preview_television_library(&form).await;
    let draft = crate::admin::server_fns::media_sources::LocalLibraryForm {
        id: form.id,
        name: form.name,
        media_kind: form.media_kind,
        paths: form
            .paths
            .into_iter()
            .map(
                |path| crate::admin::server_fns::media_sources::LocalLibraryPath {
                    id: path.id,
                    path: path.path,
                },
            )
            .collect(),
    };
    match outcome {
        Ok(reports) => (
            StatusCode::OK,
            Html(
                crate::admin::pages::media_sources::local::local_library_preview_document(
                    draft,
                    reports,
                    language,
                    is_dark_mode,
                ),
            ),
        )
            .into_response(),
        Err(_) => (
            StatusCode::BAD_REQUEST,
            Html(
                crate::admin::pages::media_sources::local::local_library_error_document(
                    draft,
                    crate::admin::i18n::text(language, TextKey::FolderPreviewUnavailable).into(),
                    language,
                    is_dark_mode,
                ),
            ),
        )
            .into_response(),
    }
}

async fn local_library_page_preferences(
    state: &AppState,
) -> (crate::admin::i18n::UiLanguage, bool) {
    let language = if let Ok(language) =
        crate::admin::server_fns::settings::load_ui_language_for_api(
            state.application.ports().admin_settings(),
        )
        .await
    {
        language
    } else {
        tracing::warn!("failed to load UI language for local library correction form");
        crate::admin::i18n::UiLanguage::English
    };
    let is_dark_mode = if let Ok(is_dark_mode) =
        crate::admin::server_fns::settings::load_bool_for_api(
            state.application.ports().admin_settings(),
            ersatzrs_domain::CONFIG_KEY_PAGES_IS_DARK_MODE,
            true,
        )
        .await
    {
        is_dark_mode
    } else {
        tracing::warn!("failed to load UI theme for local library correction form");
        true
    };
    (language, is_dark_mode)
}
