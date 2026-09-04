use axum::{
    body::Body,
    extract::{Form, Multipart, Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Redirect, Response},
    Json,
};
use ersatzrs_core::repositories::{
    AddonMediaListArtwork, AddonMediaListArtworkRole, AddonMediaListNfoMetadata,
    AddonMediaListNfoPerson, RepositoryError,
};
use ersatzrs_domain::{
    AddonMediaListContentKind, AddonMediaListDetailPolicy, AddonMediaListPresentationKind,
    AddonMediaListRecoveryAction, AddonMediaListRefreshCadence,
};
use serde::Deserialize;
use serde_json::json;

use crate::{
    admin::server_fns::addon_media_lists::{
        add_addon_media_list_with_state, delete_addon_media_list_with_state,
        get_addon_media_list_publication_history_with_state,
        get_addon_media_list_publication_readiness_with_state, match_addon_media_list_with_state,
        publish_addon_media_list_with_state, recover_addon_media_list_publication_with_state,
        redacted_publication_diagnostics_json, reset_addon_media_list_with_state,
        update_addon_media_list_with_state, AddonMediaListForm, AddonMediaListPublishResult,
    },
    host::state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct AddonMediaListAddForm {
    #[serde(default)]
    addon_id: String,
    source_url: String,
    #[serde(default)]
    folder_name: String,
    #[serde(default)]
    content_kind_override: AddonMediaListContentKind,
    #[serde(default)]
    chapter_input: String,
    #[serde(default = "default_true")]
    load_item_details: bool,
    #[serde(default)]
    presentation_kind: AddonMediaListPresentationKind,
    #[serde(default = "default_refresh_cadence")]
    refresh_cadence: AddonMediaListRefreshCadence,
    #[serde(default)]
    generate_playlist: bool,
}

const fn default_true() -> bool {
    true
}

const fn default_refresh_cadence() -> AddonMediaListRefreshCadence {
    AddonMediaListRefreshCadence::Daily
}

#[derive(Debug, Deserialize)]
pub struct AddonMediaListIdForm {
    id: i64,
}

#[derive(Debug, Deserialize)]
pub struct AddonMediaListRecoveryForm {
    list_id: i64,
    job_id: i64,
    generation: i32,
    action: String,
}

#[derive(Debug, Deserialize)]
pub struct AddonMediaListResetForm {
    id: i64,
    confirmed_name: String,
}

#[derive(Debug, Deserialize)]
pub struct AddonMediaListSourceTestForm {
    id: i64,
    saved_revision: i32,
}

#[derive(Debug, Deserialize)]
pub struct AddonMediaListRestoreRevisionForm {
    id: i64,
    source_published_revision: i32,
    expected_draft_revision: i32,
    expected_published_revision: i32,
}

#[derive(Debug, Deserialize)]
pub struct AddonMediaListItemReviewForm {
    list_id: i64,
    item_id: i64,
    etag: String,
    title: String,
    #[serde(default)]
    original_title: String,
    #[serde(default)]
    plot: String,
    #[serde(default)]
    year: String,
    #[serde(default)]
    release_date: String,
    #[serde(default)]
    season: String,
    #[serde(default)]
    episode: String,
    #[serde(default)]
    genres: String,
    #[serde(default)]
    tags: String,
    #[serde(default)]
    artists: String,
    #[serde(default)]
    studios: String,
    #[serde(default)]
    directors: String,
    #[serde(default)]
    writers: String,
    #[serde(default)]
    languages: String,
    #[serde(default)]
    people: String,
    #[serde(default)]
    metadata_poster: String,
    #[serde(default)]
    metadata_fanart: String,
    #[serde(default)]
    metadata_thumbnail: String,
    #[serde(default)]
    chapter_input: String,
}

#[derive(Debug, Deserialize)]
pub struct AddonMediaListSaveForm {
    id: i64,
    #[serde(default)]
    saved_revision: i32,
    refresh_cadence: AddonMediaListRefreshCadence,
    #[serde(default)]
    generate_playlist: bool,
    #[serde(default)]
    load_item_details: bool,
    #[serde(default)]
    presentation_kind: AddonMediaListPresentationKind,
    #[serde(default)]
    metadata_etag: String,
    #[serde(default)]
    metadata_title: String,
    #[serde(default)]
    metadata_original_title: String,
    #[serde(default)]
    metadata_sort_title: String,
    #[serde(default)]
    metadata_plot: String,
    #[serde(default)]
    metadata_year: String,
    #[serde(default)]
    metadata_genres: String,
    #[serde(default)]
    metadata_tags: String,
    #[serde(default)]
    metadata_studios: String,
    #[serde(default)]
    metadata_directors: String,
    #[serde(default)]
    metadata_writers: String,
    #[serde(default)]
    metadata_artists: String,
    #[serde(default)]
    metadata_languages: String,
    #[serde(default)]
    metadata_people: String,
    #[serde(default)]
    metadata_poster: String,
    #[serde(default)]
    metadata_fanart: String,
    #[serde(default)]
    metadata_thumbnail: String,
}

fn people_form_values(value: &str) -> Vec<AddonMediaListNfoPerson> {
    value
        .lines()
        .filter_map(|line| {
            let (name, role) = line.split_once('|').map_or((line, None), |(name, role)| {
                (name, non_empty_form_value(role))
            });
            non_empty_form_value(name).map(|name| AddonMediaListNfoPerson {
                name,
                role,
                order: None,
                thumb: None,
            })
        })
        .collect()
}

fn metadata_from_save_form(
    input: &AddonMediaListSaveForm,
    mut metadata: AddonMediaListNfoMetadata,
) -> AddonMediaListNfoMetadata {
    input.metadata_title.trim().clone_into(&mut metadata.title);
    metadata.original_title = non_empty_form_value(&input.metadata_original_title);
    metadata.sort_title = non_empty_form_value(&input.metadata_sort_title);
    metadata.plot = non_empty_form_value(&input.metadata_plot);
    metadata.year = input.metadata_year.trim().parse().ok();
    metadata.genres = comma_form_values(&input.metadata_genres);
    metadata.tags = line_form_values(&input.metadata_tags);
    metadata.studios = comma_form_values(&input.metadata_studios);
    metadata.directors = comma_form_values(&input.metadata_directors);
    metadata.writers = comma_form_values(&input.metadata_writers);
    metadata.artists = comma_form_values(&input.metadata_artists);
    metadata.languages = comma_form_values(&input.metadata_languages);
    metadata.people = people_form_values(&input.metadata_people);
    metadata.artwork.retain(|artwork| {
        !matches!(
            artwork.role,
            AddonMediaListArtworkRole::Poster
                | AddonMediaListArtworkRole::Fanart
                | AddonMediaListArtworkRole::Thumb
        )
    });
    for (value, role) in [
        (&input.metadata_poster, AddonMediaListArtworkRole::Poster),
        (&input.metadata_fanart, AddonMediaListArtworkRole::Fanart),
        (&input.metadata_thumbnail, AddonMediaListArtworkRole::Thumb),
    ] {
        if let Some(url) = non_empty_form_value(value) {
            metadata.artwork.push(AddonMediaListArtwork {
                url,
                role,
                width: None,
                height: None,
                language: None,
            });
        }
    }
    metadata
}

fn item_metadata_from_review_form(
    input: &AddonMediaListItemReviewForm,
    mut metadata: AddonMediaListNfoMetadata,
) -> AddonMediaListNfoMetadata {
    input.title.trim().clone_into(&mut metadata.title);
    metadata.original_title = non_empty_form_value(&input.original_title);
    metadata.plot = non_empty_form_value(&input.plot);
    metadata.year = input.year.trim().parse().ok();
    metadata.release_date = non_empty_form_value(&input.release_date);
    metadata.season = input.season.trim().parse().ok();
    metadata.episode = input.episode.trim().parse().ok();
    metadata.genres = comma_form_values(&input.genres);
    metadata.tags = line_form_values(&input.tags);
    metadata.artists = comma_form_values(&input.artists);
    metadata.studios = comma_form_values(&input.studios);
    metadata.directors = comma_form_values(&input.directors);
    metadata.writers = comma_form_values(&input.writers);
    metadata.languages = comma_form_values(&input.languages);
    metadata.people = people_form_values(&input.people);
    metadata.artwork.retain(|artwork| {
        !matches!(
            artwork.role,
            AddonMediaListArtworkRole::Poster
                | AddonMediaListArtworkRole::Fanart
                | AddonMediaListArtworkRole::Thumb
        )
    });
    for (value, role) in [
        (&input.metadata_poster, AddonMediaListArtworkRole::Poster),
        (&input.metadata_fanart, AddonMediaListArtworkRole::Fanart),
        (&input.metadata_thumbnail, AddonMediaListArtworkRole::Thumb),
    ] {
        if let Some(url) = non_empty_form_value(value) {
            metadata.artwork.push(AddonMediaListArtwork {
                url,
                role,
                width: None,
                height: None,
                language: None,
            });
        }
    }
    metadata
}

fn is_unresolved_addon(error: &ersatzrs_app::ApplicationError) -> bool {
    matches!(
        error,
        ersatzrs_app::ApplicationError::Validation {
            field: "addon_id",
            ..
        }
    )
}

pub async fn add_addon_media_list_form(
    State(state): State<AppState>,
    Form(input): Form<AddonMediaListAddForm>,
) -> Redirect {
    let result = add_addon_media_list_with_state(
        &state,
        input.addon_id,
        input.source_url,
        input.folder_name,
        input.content_kind_override,
        input.chapter_input.clone(),
        if input.load_item_details {
            AddonMediaListDetailPolicy::Automatic
        } else {
            AddonMediaListDetailPolicy::ManualOnly
        },
        input.refresh_cadence,
    )
    .await;
    match result {
        Ok(id) => {
            let update = state
                .application
                .update_addon_media_list(ersatzrs_app::commands::UpdateAddonMediaList {
                    id,
                    refresh_cadence: input.refresh_cadence,
                    generate_playlist: input.generate_playlist,
                    content_kind_override: presentation_content_kind(input.presentation_kind),
                    chapter_input: input.chapter_input,
                    detail_import_policy: if input.load_item_details {
                        AddonMediaListDetailPolicy::Automatic
                    } else {
                        AddonMediaListDetailPolicy::ManualOnly
                    },
                })
                .await;
            let presentation = state
                .application
                .request_addon_media_list_presentation(
                    ersatzrs_app::commands::RequestAddonMediaListPresentation {
                        id,
                        presentation_kind: input.presentation_kind,
                        show_title: None,
                    },
                )
                .await;
            match update.and(presentation) {
                Ok(()) => Redirect::to(&format!("/media/addon-lists/{id}?import=1")),
                Err(error) => {
                    tracing::error!(%error, addon_media_list_id = id, "failed to finish add-on media-list HTML import settings");
                    Redirect::to(&format!("/media/addon-lists/{id}?error=save"))
                }
            }
        }
        Err(error) => {
            tracing::error!(%error, "failed to add add-on media list from HTML form");
            if is_unresolved_addon(&error) {
                Redirect::to("/media/addon-lists?error=add-unresolved")
            } else {
                Redirect::to("/media/addon-lists?error=add")
            }
        }
    }
}

pub async fn match_addon_media_list_form(
    State(state): State<AppState>,
    Form(input): Form<AddonMediaListIdForm>,
) -> Redirect {
    match match_addon_media_list_with_state(&state, input.id).await {
        Ok(()) => Redirect::to("/media/addon-lists"),
        Err(error) => {
            tracing::error!(%error, addon_media_list_id = input.id, "failed to match add-on media list from HTML form");
            Redirect::to("/media/addon-lists?error=match")
        }
    }
}

pub async fn save_addon_media_list_form(
    State(state): State<AppState>,
    Form(input): Form<AddonMediaListSaveForm>,
) -> Redirect {
    let id = input.id;
    let current = match state.application.get_addon_media_list(id).await {
        Ok(Some(current)) => current,
        Ok(None) => return Redirect::to("/media/addon-lists?error=not-found"),
        Err(error) => {
            tracing::error!(%error, addon_media_list_id = id, "failed to load add-on media list for HTML save");
            return Redirect::to(&format!("/media/addon-lists/{id}?error=save"));
        }
    };
    let form = AddonMediaListForm {
        id,
        name: current.name,
        addon_id: current.addon_id,
        source_url: current.source_url,
        folder_name: current.folder_name,
        refresh_cadence: input.refresh_cadence,
        last_refresh_succeeded_at: current.last_refresh_succeeded_at,
        next_refresh_at: current.next_refresh_at,
        refresh_retry_at: current.refresh_retry_at,
        generate_playlist: input.generate_playlist,
        content_kind_override: presentation_content_kind(input.presentation_kind),
        legacy_chapter_input: current.legacy_chapter_input,
        presentation_kind: current.presentation_kind,
        requested_presentation_kind: current.requested_presentation_kind,
        presentation_error: current.presentation_error,
        review_state: current.review_state,
        review_error: current.review_error,
        published_revision: current.published_revision,
        draft_revision: current.draft_revision,
        detail_import_policy: if input.load_item_details {
            AddonMediaListDetailPolicy::Automatic
        } else {
            AddonMediaListDetailPolicy::ManualOnly
        },
        import_progress: current.import_progress.map(Into::into),
    };
    match update_addon_media_list_with_state(&state, form).await {
        Ok(()) => {
            let current_nfo = match state.application.get_addon_media_list_show_nfo(id).await {
                Ok(current_nfo) => current_nfo,
                Err(error) => {
                    tracing::error!(%error, addon_media_list_id = id, "failed to load add-on media-list metadata for HTML save");
                    return Redirect::to(&format!("/media/addon-lists/{id}?error=save"));
                }
            };
            let metadata = metadata_from_save_form(&input, current_nfo.metadata);
            let etag = if input.metadata_etag.is_empty() {
                current_nfo.etag
            } else {
                input.metadata_etag.clone()
            };
            if let Err(error) = state
                .application
                .save_addon_media_list_show_nfo(ersatzrs_app::commands::SaveAddonMediaListNfo {
                    id,
                    list_id: None,
                    metadata,
                    etag,
                })
                .await
            {
                tracing::error!(%error, addon_media_list_id = id, "failed to save add-on media-list metadata from HTML form");
                return Redirect::to(&format!("/media/addon-lists/{id}?error=save"));
            }
            match state
                .application
                .request_addon_media_list_presentation(
                    ersatzrs_app::commands::RequestAddonMediaListPresentation {
                        id,
                        presentation_kind: input.presentation_kind,
                        show_title: None,
                    },
                )
                .await
            {
                Ok(()) => Redirect::to(&format!("/media/addon-lists/{id}?status=saved")),
                Err(error) => {
                    tracing::error!(%error, addon_media_list_id = id, "failed to request add-on media-list presentation from HTML save");
                    Redirect::to(&format!("/media/addon-lists/{id}?error=save"))
                }
            }
        }
        Err(error) => {
            tracing::error!(%error, addon_media_list_id = id, "failed to save add-on media list from HTML form");
            Redirect::to(&format!("/media/addon-lists/{id}?error=save"))
        }
    }
}

pub async fn publish_addon_media_list_form(
    State(state): State<AppState>,
    Form(input): Form<AddonMediaListSaveForm>,
) -> Redirect {
    let id = input.id;
    let current = match state.application.get_addon_media_list(id).await {
        Ok(Some(current)) => current,
        Ok(None) => return Redirect::to("/media/addon-lists?error=not-found"),
        Err(error) => {
            tracing::error!(%error, addon_media_list_id = id, "failed to load add-on media list for HTML publish");
            return Redirect::to(&format!("/media/addon-lists/{id}?error=publish"));
        }
    };
    let submitted_policy = if input.load_item_details {
        AddonMediaListDetailPolicy::Automatic
    } else {
        AddonMediaListDetailPolicy::ManualOnly
    };
    if current.refresh_cadence != input.refresh_cadence
        || current.generate_playlist != input.generate_playlist
        || current.requested_presentation_kind != input.presentation_kind
        || current.detail_import_policy != submitted_policy
    {
        return Redirect::to(&format!("/media/addon-lists/{id}?error=unsaved"));
    }
    let current_nfo = match state.application.get_addon_media_list_show_nfo(id).await {
        Ok(current_nfo) => current_nfo,
        Err(error) => {
            tracing::error!(%error, addon_media_list_id = id, "failed to load add-on media-list metadata for HTML publish");
            return Redirect::to(&format!("/media/addon-lists/{id}?error=publish"));
        }
    };
    if current_nfo.etag != input.metadata_etag
        || current_nfo.metadata != metadata_from_save_form(&input, current_nfo.metadata.clone())
    {
        return Redirect::to(&format!("/media/addon-lists/{id}?error=unsaved"));
    }
    let readiness = match get_addon_media_list_publication_readiness_with_state(&state, id).await {
        Ok(readiness) => readiness,
        Err(error) => {
            tracing::error!(%error, addon_media_list_id = id, "failed to classify add-on media-list HTML publication");
            return Redirect::to(&format!("/media/addon-lists/{id}?error=publish"));
        }
    };
    if !readiness.blockers.is_empty() {
        return Redirect::to(&format!("/media/addon-lists/{id}?error=blocked"));
    }
    if input.saved_revision != readiness.saved_revision {
        return Redirect::to(&format!("/media/addon-lists/{id}?error=unsaved"));
    }
    match publish_addon_media_list_with_state(&state, id, input.saved_revision).await {
        Ok(AddonMediaListPublishResult::Queued { .. }) => {
            Redirect::to(&format!("/media/addon-lists/{id}?status=publishing"))
        }
        Ok(AddonMediaListPublishResult::PreparingPlaybackDetails { .. }) => {
            Redirect::to(&format!("/media/addon-lists/{id}?status=preparing"))
        }
        Err(error) => {
            tracing::error!(%error, addon_media_list_id = id, "failed to publish add-on media list from HTML form");
            Redirect::to(&format!("/media/addon-lists/{id}?error=publish"))
        }
    }
}

pub async fn recover_addon_media_list_publication_form(
    State(state): State<AppState>,
    Form(input): Form<AddonMediaListRecoveryForm>,
) -> Redirect {
    let action = match input.action.as_str() {
        "retry" => AddonMediaListRecoveryAction::Retry,
        "restore" => AddonMediaListRecoveryAction::RestorePreviousRevision,
        _ => {
            return Redirect::to(&format!(
                "/media/addon-lists/{}?error=recovery",
                input.list_id
            ));
        }
    };
    match recover_addon_media_list_publication_with_state(
        &state,
        input.list_id,
        input.job_id,
        input.generation,
        action,
    )
    .await
    {
        Ok(_) => Redirect::to(&format!(
            "/media/addon-lists/{}?status=recovery-queued",
            input.list_id
        )),
        Err(error) => {
            tracing::error!(%error, addon_media_list_id = input.list_id, publication_job_id = input.job_id, "failed to recover add-on media-list publication from HTML form");
            Redirect::to(&format!(
                "/media/addon-lists/{}?error=recovery",
                input.list_id
            ))
        }
    }
}

pub async fn reset_addon_media_list_form(
    State(state): State<AppState>,
    Form(input): Form<AddonMediaListResetForm>,
) -> Redirect {
    match reset_addon_media_list_with_state(&state, input.id, input.confirmed_name).await {
        Ok(_) => Redirect::to(&format!(
            "/media/addon-lists/{}?status=reset-queued",
            input.id
        )),
        Err(error) => {
            tracing::warn!(
                addon_media_list_id = input.id,
                error = %error,
                "failed to reset and reload add-on media list from HTML form"
            );
            Redirect::to(&format!("/media/addon-lists/{}?error=reset", input.id))
        }
    }
}

pub async fn test_addon_media_list_source_form(
    State(state): State<AppState>,
    Form(input): Form<AddonMediaListSourceTestForm>,
) -> Redirect {
    match state
        .application
        .test_addon_media_list_source(ersatzrs_app::commands::TestAddonMediaListSource {
            id: input.id,
            saved_revision: input.saved_revision,
        })
        .await
    {
        Ok(_) => Redirect::to(&format!(
            "/media/addon-lists/{}?status=source-test-queued",
            input.id
        )),
        Err(error) => {
            tracing::warn!(addon_media_list_id = input.id, error = %error, "failed to queue add-on media-list source test from HTML form");
            Redirect::to(&format!(
                "/media/addon-lists/{}?error=source-test",
                input.id
            ))
        }
    }
}

pub async fn restore_addon_media_list_revision_form(
    State(state): State<AppState>,
    Form(input): Form<AddonMediaListRestoreRevisionForm>,
) -> Redirect {
    match state
        .application
        .restore_addon_media_list_revision(ersatzrs_app::commands::RestoreAddonMediaListRevision {
            id: input.id,
            source_published_revision: input.source_published_revision,
            expected_draft_revision: input.expected_draft_revision,
            expected_published_revision: input.expected_published_revision,
        })
        .await
    {
        Ok(_) => Redirect::to(&format!(
            "/media/addon-lists/{}?status=revision-restore-queued",
            input.id
        )),
        Err(error) => {
            tracing::warn!(addon_media_list_id = input.id, source_published_revision = input.source_published_revision, error = %error, "failed to restore add-on media-list revision from HTML form");
            Redirect::to(&format!(
                "/media/addon-lists/{}?error=revision-restore",
                input.id
            ))
        }
    }
}

/// Download the same typed, redacted publication history used by the hydrated
/// clipboard action. The fixed filename prevents list data from reaching the
/// response headers.
pub async fn download_addon_media_list_publication_diagnostics(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Response {
    let history = match get_addon_media_list_publication_history_with_state(&state, id).await {
        Ok(history) => history,
        Err(error) => {
            tracing::error!(%error, addon_media_list_id = id, "failed to load redacted add-on publication diagnostics");
            return StatusCode::UNPROCESSABLE_ENTITY.into_response();
        }
    };
    match redacted_publication_diagnostics_json(&history) {
        Ok(body) => publication_diagnostics_download_response(body),
        Err(error) => {
            tracing::error!(%error, addon_media_list_id = id, "failed to serialize redacted add-on publication diagnostics");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

fn publication_diagnostics_download_response(body: String) -> Response {
    (
        [
            (header::CONTENT_TYPE, "application/json; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"ersatzrs-addon-publication-diagnostics.json\"",
            ),
            (header::CACHE_CONTROL, "private, no-store"),
        ],
        Body::from(body),
    )
        .into_response()
}

pub async fn save_addon_media_list_item_review_form(
    State(state): State<AppState>,
    Form(input): Form<AddonMediaListItemReviewForm>,
) -> Redirect {
    let path = format!(
        "/media/addon-lists/{}/items/{}",
        input.list_id, input.item_id
    );
    let detail = match state
        .application
        .get_addon_media_list_item(input.list_id, input.item_id)
        .await
    {
        Ok(Some(detail)) => detail,
        Ok(None) => return Redirect::to(&format!("{path}?error=not-found")),
        Err(error) => {
            tracing::error!(%error, list_id = input.list_id, item_id = input.item_id, "failed to load add-on item for HTML review save");
            return Redirect::to(&format!("{path}?error=save"));
        }
    };
    let chapter_update = changed_item_chapter_input(
        detail.item.parent_provider_id.is_some(),
        &detail.item.chapter_input,
        &detail.item.provider_chapter_input,
        &input.chapter_input,
    );
    if let Some(chapter_input) = chapter_update.as_deref() {
        if let Err(error) = ersatzrs_core::media_chapters::parse_chapter_input(chapter_input, None)
        {
            tracing::warn!(%error, list_id = input.list_id, item_id = input.item_id, "rejected invalid chapter input from HTML review form");
            return Redirect::to(&format!("{path}?error=save"));
        }
    }
    let metadata = item_metadata_from_review_form(&input, detail.nfo.metadata);
    let save = state
        .application
        .save_addon_media_list_item_nfo(ersatzrs_app::commands::SaveAddonMediaListNfo {
            id: input.item_id,
            list_id: Some(input.list_id),
            metadata,
            etag: input.etag,
        })
        .await;
    let result = match (save, chapter_update) {
        (Ok(()), Some(chapter_input)) => {
            state
                .application
                .update_addon_media_list_item_chapters(
                    ersatzrs_app::commands::UpdateAddonMediaListItemChapters {
                        id: input.item_id,
                        chapter_input,
                    },
                )
                .await
        }
        (Ok(()), None) => Ok(()),
        (Err(error), _) => Err(error),
    };
    match result {
        Ok(()) => Redirect::to(&format!("{path}?status=saved")),
        Err(error) => {
            tracing::error!(%error, list_id = input.list_id, item_id = input.item_id, "failed to save add-on item from HTML review form");
            Redirect::to(&format!("{path}?error=save"))
        }
    }
}

fn non_empty_form_value(value: &str) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.trim().to_owned())
}

fn comma_form_values(value: &str) -> Vec<String> {
    value.split(',').filter_map(non_empty_form_value).collect()
}

fn line_form_values(value: &str) -> Vec<String> {
    value.lines().filter_map(non_empty_form_value).collect()
}

fn changed_item_chapter_input(
    is_fragment: bool,
    saved: &str,
    provider: &str,
    submitted: &str,
) -> Option<String> {
    let saved = saved.trim();
    let provider = provider.trim();
    let submitted_trimmed = submitted.trim();
    (!is_fragment
        && submitted_trimmed != saved
        && !(saved.is_empty() && submitted_trimmed == provider))
        .then(|| submitted.to_owned())
}

const fn presentation_content_kind(
    presentation: AddonMediaListPresentationKind,
) -> AddonMediaListContentKind {
    match presentation {
        AddonMediaListPresentationKind::TelevisionShow => {
            AddonMediaListContentKind::TelevisionEpisode
        }
        AddonMediaListPresentationKind::Movies => AddonMediaListContentKind::Movie,
        AddonMediaListPresentationKind::MusicVideos => AddonMediaListContentKind::MusicVideo,
        AddonMediaListPresentationKind::OtherVideos => AddonMediaListContentKind::OtherVideo,
        AddonMediaListPresentationKind::Songs => AddonMediaListContentKind::Song,
        AddonMediaListPresentationKind::Images => AddonMediaListContentKind::Image,
        AddonMediaListPresentationKind::Auto | AddonMediaListPresentationKind::RemoteStreams => {
            AddonMediaListContentKind::Auto
        }
    }
}

pub async fn delete_addon_media_list_form(
    State(state): State<AppState>,
    Form(input): Form<AddonMediaListIdForm>,
) -> Redirect {
    match delete_addon_media_list_with_state(&state, input.id).await {
        Ok(()) => Redirect::to("/media/addon-lists"),
        Err(error) => {
            tracing::error!(%error, addon_media_list_id = input.id, "failed to delete add-on media list from HTML form");
            Redirect::to("/media/addon-lists?error=delete")
        }
    }
}

/// Multipart overhead allowed in addition to the validated image payload.
pub const MAX_ARTWORK_REQUEST_BYTES: usize =
    ersatzrs_infra::addon_artwork::MAX_ARTWORK_BYTES + 64 * 1024;

/// Store one role-aware artwork upload inside the metadata draft.
pub async fn upload_artwork(State(state): State<AppState>, mut multipart: Multipart) -> Response {
    let mut list_id = None;
    let mut item_id = None;
    let mut role = None;
    let mut etag = None;
    let mut image = None;
    loop {
        let field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(error) => return api_error(StatusCode::BAD_REQUEST, error.to_string()),
        };
        let Some(name) = field.name().map(str::to_owned) else {
            continue;
        };
        if name == "file" {
            let content_type = field
                .content_type()
                .unwrap_or("application/octet-stream")
                .to_owned();
            match field.bytes().await {
                Ok(bytes) => image = Some((content_type, bytes)),
                Err(error) => return api_error(StatusCode::BAD_REQUEST, error.to_string()),
            }
            continue;
        }
        let value = match field.text().await {
            Ok(value) => value,
            Err(error) => return api_error(StatusCode::BAD_REQUEST, error.to_string()),
        };
        match name.as_str() {
            "list_id" => list_id = value.parse::<i64>().ok().filter(|id| *id > 0),
            "item_id" => item_id = value.parse::<i64>().ok().filter(|id| *id > 0),
            "role" => role = parse_role(&value),
            "etag" => etag = (!value.trim().is_empty()).then_some(value),
            _ => {}
        }
    }
    let Some(list_id) = list_id else {
        return api_error(StatusCode::BAD_REQUEST, "list_id is required");
    };
    let Some(role) = role else {
        return api_error(
            StatusCode::BAD_REQUEST,
            "a supported artwork role is required",
        );
    };
    let Some(etag) = etag else {
        return api_error(StatusCode::BAD_REQUEST, "etag is required");
    };
    let Some((content_type, bytes)) = image else {
        return api_error(StatusCode::BAD_REQUEST, "an artwork file is required");
    };
    match state
        .application
        .ports()
        .addon_media_lists()
        .upload_artwork(list_id, item_id, role, &content_type, &bytes, &etag)
        .await
    {
        Ok(form) => {
            let selected = form
                .metadata
                .artwork
                .iter()
                .find(|candidate| candidate.role == role)
                .map(|candidate| candidate.url.clone())
                .unwrap_or_default();
            Json(json!({"etag": form.etag, "url": selected})).into_response()
        }
        Err(RepositoryError::NotFound { .. }) => {
            api_error(StatusCode::NOT_FOUND, "metadata owner was not found")
        }
        Err(RepositoryError::Conflict(message)) => api_error(StatusCode::CONFLICT, message),
        Err(error) => api_error(StatusCode::UNPROCESSABLE_ENTITY, error.to_string()),
    }
}

/// Serve a database-backed draft image for the editor preview. The opaque
/// upload id is not a filesystem path and cannot escape managed storage.
pub async fn uploaded_artwork(
    State(state): State<AppState>,
    Path(upload_id): Path<i64>,
) -> Response {
    let upload = match state
        .application
        .get_addon_media_list_artwork_upload(upload_id)
        .await
    {
        Ok(Some(upload)) => upload,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => {
            tracing::error!(%error, upload_id, "failed to load add-on metadata artwork");
            return StatusCode::UNPROCESSABLE_ENTITY.into_response();
        }
    };
    (
        [
            (header::CONTENT_TYPE, upload.content_type),
            (header::CACHE_CONTROL, "private, no-store".to_owned()),
        ],
        Body::from(upload.bytes),
    )
        .into_response()
}

fn parse_role(value: &str) -> Option<AddonMediaListArtworkRole> {
    match value.trim().to_ascii_lowercase().as_str() {
        "poster" => Some(AddonMediaListArtworkRole::Poster),
        "fanart" => Some(AddonMediaListArtworkRole::Fanart),
        "thumb" | "thumbnail" => Some(AddonMediaListArtworkRole::Thumb),
        _ => None,
    }
}

fn api_error(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(json!({"error": message.into()}))).into_response()
}

#[cfg(test)]
mod tests {
    use axum::body::Bytes;

    use super::*;

    #[test]
    fn save_form_requires_cadence_and_defaults_missing_checkboxes_to_false() {
        let body = Bytes::from_static(b"id=42&presentation_kind=television_show");
        assert!(serde_urlencoded::from_bytes::<AddonMediaListSaveForm>(&body).is_err());
        let body =
            Bytes::from_static(b"id=42&presentation_kind=television_show&refresh_cadence=never");
        let parsed: AddonMediaListSaveForm = serde_urlencoded::from_bytes(&body).unwrap();
        assert_eq!(parsed.id, 42);
        assert_eq!(parsed.refresh_cadence, AddonMediaListRefreshCadence::Never);
        assert!(!parsed.generate_playlist);
        assert!(!parsed.load_item_details);
        assert_eq!(
            parsed.presentation_kind,
            AddonMediaListPresentationKind::TelevisionShow
        );
    }

    #[test]
    fn lifecycle_forms_preserve_exact_revision_preconditions() {
        let source: AddonMediaListSourceTestForm =
            serde_urlencoded::from_bytes(b"id=42&saved_revision=7").unwrap();
        assert_eq!((source.id, source.saved_revision), (42, 7));

        let restore: AddonMediaListRestoreRevisionForm = serde_urlencoded::from_bytes(
            b"id=42&source_published_revision=3&expected_draft_revision=7&expected_published_revision=5",
        )
        .unwrap();
        assert_eq!(
            (
                restore.id,
                restore.source_published_revision,
                restore.expected_draft_revision,
                restore.expected_published_revision,
            ),
            (42, 3, 7, 5)
        );
    }

    #[test]
    fn save_form_rebuilds_visible_metadata_without_dropping_unedited_fields() {
        let body = Bytes::from_static(
            b"id=42&presentation_kind=other_videos&refresh_cadence=daily&metadata_etag=revision&metadata_title=Programme&metadata_original_title=Original&metadata_plot=Summary&metadata_year=2026&metadata_genres=Archive%2C+News&metadata_tags=featured%0Alocal&metadata_people=Presenter+%7C+Host&metadata_poster=https%3A%2F%2Fexample.invalid%2Fposter.jpg",
        );
        let parsed: AddonMediaListSaveForm = serde_urlencoded::from_bytes(&body).unwrap();
        let original = AddonMediaListNfoMetadata {
            tagline: Some("preserved".to_owned()),
            artwork: vec![AddonMediaListArtwork {
                url: "https://example.invalid/other.jpg".to_owned(),
                role: AddonMediaListArtworkRole::Other,
                ..AddonMediaListArtwork::default()
            }],
            ..AddonMediaListNfoMetadata::default()
        };

        let metadata = metadata_from_save_form(&parsed, original);

        assert_eq!(metadata.title, "Programme");
        assert_eq!(metadata.year, Some(2026));
        assert_eq!(metadata.tags, ["featured", "local"]);
        assert_eq!(metadata.people[0].name, "Presenter");
        assert_eq!(metadata.people[0].role.as_deref(), Some("Host"));
        assert_eq!(metadata.tagline.as_deref(), Some("preserved"));
        assert!(metadata
            .artwork
            .iter()
            .any(|artwork| artwork.role == AddonMediaListArtworkRole::Other));
        assert!(metadata.artwork.iter().any(|artwork| {
            artwork.role == AddonMediaListArtworkRole::Poster
                && artwork.url == "https://example.invalid/poster.jpg"
        }));
    }

    #[test]
    fn item_review_form_rebuilds_every_visible_metadata_field() {
        let body = Bytes::from_static(
            b"list_id=42&item_id=7&etag=revision&title=Episode&original_title=Original&plot=Summary&year=2026&release_date=2026-09-04&season=2&episode=3&genres=Archive%2C+News&tags=archive%0Afeatured&artists=Artist&studios=Studio&directors=Director&writers=Writer&languages=nl%2C+en&people=Presenter+%7C+Host&metadata_poster=https%3A%2F%2Fexample.invalid%2Fposter.jpg&metadata_fanart=https%3A%2F%2Fexample.invalid%2Ffanart.jpg&metadata_thumbnail=https%3A%2F%2Fexample.invalid%2Fthumb.jpg&chapter_input=0%3A00+Intro%0A1%3A35+First+topic",
        );
        let parsed: AddonMediaListItemReviewForm = serde_urlencoded::from_bytes(&body).unwrap();
        let original = AddonMediaListNfoMetadata {
            tagline: Some("preserved".to_owned()),
            artwork: vec![AddonMediaListArtwork {
                url: "https://example.invalid/other.jpg".to_owned(),
                role: AddonMediaListArtworkRole::Other,
                ..AddonMediaListArtwork::default()
            }],
            ..AddonMediaListNfoMetadata::default()
        };

        let metadata = item_metadata_from_review_form(&parsed, original);

        assert_eq!(metadata.title, "Episode");
        assert_eq!(metadata.original_title.as_deref(), Some("Original"));
        assert_eq!(metadata.plot.as_deref(), Some("Summary"));
        assert_eq!(metadata.year, Some(2026));
        assert_eq!(metadata.release_date.as_deref(), Some("2026-09-04"));
        assert_eq!((metadata.season, metadata.episode), (Some(2), Some(3)));
        assert_eq!(metadata.genres, ["Archive", "News"]);
        assert_eq!(metadata.tags, ["archive", "featured"]);
        assert_eq!(metadata.artists, ["Artist"]);
        assert_eq!(metadata.studios, ["Studio"]);
        assert_eq!(metadata.directors, ["Director"]);
        assert_eq!(metadata.writers, ["Writer"]);
        assert_eq!(metadata.languages, ["nl", "en"]);
        assert_eq!(metadata.people[0].name, "Presenter");
        assert_eq!(metadata.people[0].role.as_deref(), Some("Host"));
        assert_eq!(metadata.tagline.as_deref(), Some("preserved"));
        assert!(metadata.artwork.iter().any(|artwork| {
            artwork.role == AddonMediaListArtworkRole::Poster
                && artwork.url == "https://example.invalid/poster.jpg"
        }));
        assert!(metadata.artwork.iter().any(|artwork| {
            artwork.role == AddonMediaListArtworkRole::Fanart
                && artwork.url == "https://example.invalid/fanart.jpg"
        }));
        assert!(metadata.artwork.iter().any(|artwork| {
            artwork.role == AddonMediaListArtworkRole::Thumb
                && artwork.url == "https://example.invalid/thumb.jpg"
        }));
        assert!(metadata
            .artwork
            .iter()
            .any(|artwork| artwork.role == AddonMediaListArtworkRole::Other));
        assert_eq!(parsed.chapter_input, "0:00 Intro\n1:35 First topic");
    }

    #[test]
    fn item_review_chapters_change_only_for_an_explicit_full_item_edit() {
        let provider = "0:00 Intro\n1:35 First topic";
        assert_eq!(
            changed_item_chapter_input(false, "", provider, provider),
            None
        );
        assert_eq!(
            changed_item_chapter_input(false, "0:00 Saved", provider, "0:00 Saved"),
            None
        );
        assert_eq!(
            changed_item_chapter_input(false, "", provider, "0:00 Replacement"),
            Some("0:00 Replacement".to_owned())
        );
        assert_eq!(
            changed_item_chapter_input(true, "", provider, "0:00 Replacement"),
            None
        );
    }

    #[test]
    fn import_form_carries_every_visible_source_setting() {
        let body = Bytes::from_static(
            b"addon_id=org.example.list&source_url=https%3A%2F%2Fexample.invalid%2Flist&folder_name=Watchlist&presentation_kind=movies&refresh_cadence=monthly&generate_playlist=true&load_item_details=true",
        );
        let parsed: AddonMediaListAddForm = serde_urlencoded::from_bytes(&body).unwrap();
        assert_eq!(parsed.addon_id, "org.example.list");
        assert_eq!(parsed.folder_name, "Watchlist");
        assert_eq!(
            parsed.refresh_cadence,
            AddonMediaListRefreshCadence::Monthly
        );
        assert!(parsed.generate_playlist);
        assert!(parsed.load_item_details);
        assert_eq!(
            parsed.presentation_kind,
            AddonMediaListPresentationKind::Movies
        );
    }

    #[test]
    fn only_publishable_artwork_roles_are_accepted() {
        assert_eq!(
            parse_role("poster"),
            Some(AddonMediaListArtworkRole::Poster)
        );
        assert_eq!(
            parse_role("fanart"),
            Some(AddonMediaListArtworkRole::Fanart)
        );
        assert_eq!(
            parse_role("thumbnail"),
            Some(AddonMediaListArtworkRole::Thumb)
        );
        assert_eq!(parse_role("other"), None);
    }

    #[test]
    fn diagnostics_download_is_private_json_with_a_fixed_filename() {
        let response = publication_diagnostics_download_response("{}".to_owned());
        assert_eq!(
            response.headers()[header::CONTENT_TYPE],
            "application/json; charset=utf-8"
        );
        assert_eq!(
            response.headers()[header::CONTENT_DISPOSITION],
            "attachment; filename=\"ersatzrs-addon-publication-diagnostics.json\""
        );
        assert_eq!(
            response.headers()[header::CACHE_CONTROL],
            "private, no-store"
        );
    }
}
