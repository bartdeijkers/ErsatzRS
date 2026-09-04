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
    AddonMediaListRecoveryAction,
};
use serde::Deserialize;
use serde_json::json;

use crate::{
    admin::server_fns::addon_media_lists::{
        add_addon_media_list_with_state, delete_addon_media_list_with_state,
        get_addon_media_list_publication_readiness_with_state, match_addon_media_list_with_state,
        publish_addon_media_list_with_state, recover_addon_media_list_publication_with_state,
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
    #[serde(default)]
    auto_refresh: bool,
    #[serde(default)]
    generate_playlist: bool,
}

const fn default_true() -> bool {
    true
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
pub struct AddonMediaListItemReviewForm {
    list_id: i64,
    item_id: i64,
    etag: String,
    title: String,
    #[serde(default)]
    plot: String,
    #[serde(default)]
    season: String,
    #[serde(default)]
    episode: String,
    #[serde(default)]
    genres: String,
    #[serde(default)]
    tags: String,
    #[serde(default)]
    chapter_input: String,
}

#[derive(Debug, Deserialize)]
pub struct AddonMediaListSaveForm {
    id: i64,
    #[serde(default)]
    saved_revision: i32,
    #[serde(default)]
    auto_refresh: bool,
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
    )
    .await;
    match result {
        Ok(id) => {
            let update = state
                .application
                .update_addon_media_list(ersatzrs_app::commands::UpdateAddonMediaList {
                    id,
                    auto_refresh: input.auto_refresh,
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
        addon_id: current.addon_id,
        source_url: current.source_url,
        folder_name: current.folder_name,
        auto_refresh: input.auto_refresh,
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
    if current.auto_refresh != input.auto_refresh
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
    let mut metadata = detail.nfo.metadata;
    metadata.title = input.title;
    metadata.plot = non_empty_form_value(&input.plot);
    metadata.season = input.season.trim().parse().ok();
    metadata.episode = input.episode.trim().parse().ok();
    metadata.genres = comma_form_values(&input.genres);
    metadata.tags = line_form_values(&input.tags);
    let save = state
        .application
        .save_addon_media_list_item_nfo(ersatzrs_app::commands::SaveAddonMediaListNfo {
            id: input.item_id,
            list_id: Some(input.list_id),
            metadata,
            etag: input.etag,
        })
        .await;
    let result = match save {
        Ok(()) => {
            state
                .application
                .update_addon_media_list_item_chapters(
                    ersatzrs_app::commands::UpdateAddonMediaListItemChapters {
                        id: input.item_id,
                        chapter_input: input.chapter_input,
                    },
                )
                .await
        }
        Err(error) => Err(error),
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
    fn save_form_defaults_missing_checkboxes_to_false() {
        let body = Bytes::from_static(b"id=42&presentation_kind=television_show");
        let parsed: AddonMediaListSaveForm = serde_urlencoded::from_bytes(&body).unwrap();
        assert_eq!(parsed.id, 42);
        assert!(!parsed.auto_refresh);
        assert!(!parsed.generate_playlist);
        assert!(!parsed.load_item_details);
        assert_eq!(
            parsed.presentation_kind,
            AddonMediaListPresentationKind::TelevisionShow
        );
    }

    #[test]
    fn save_form_rebuilds_visible_metadata_without_dropping_unedited_fields() {
        let body = Bytes::from_static(
            b"id=42&presentation_kind=other_videos&metadata_etag=revision&metadata_title=Programme&metadata_original_title=Original&metadata_plot=Summary&metadata_year=2026&metadata_genres=Archive%2C+News&metadata_tags=featured%0Alocal&metadata_people=Presenter+%7C+Host&metadata_poster=https%3A%2F%2Fexample.invalid%2Fposter.jpg",
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
    fn item_review_form_preserves_multiline_values() {
        let body = Bytes::from_static(
            b"list_id=42&item_id=7&etag=revision&title=Episode&tags=archive%0Afeatured&chapter_input=0%3A00+Intro%0A1%3A35+First+topic",
        );
        let parsed: AddonMediaListItemReviewForm = serde_urlencoded::from_bytes(&body).unwrap();
        assert_eq!(parsed.tags, "archive\nfeatured");
        assert_eq!(parsed.chapter_input, "0:00 Intro\n1:35 First topic");
    }

    #[test]
    fn import_form_carries_every_visible_source_setting() {
        let body = Bytes::from_static(
            b"addon_id=org.example.list&source_url=https%3A%2F%2Fexample.invalid%2Flist&folder_name=Watchlist&presentation_kind=movies&auto_refresh=true&generate_playlist=true&load_item_details=true",
        );
        let parsed: AddonMediaListAddForm = serde_urlencoded::from_bytes(&body).unwrap();
        assert_eq!(parsed.addon_id, "org.example.list");
        assert_eq!(parsed.folder_name, "Watchlist");
        assert!(parsed.auto_refresh);
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
}
