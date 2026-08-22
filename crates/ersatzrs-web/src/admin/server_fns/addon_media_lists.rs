use ersatzrs_domain::{
    AddonMediaListContentKind, AddonMediaListItemAvailability, AddonMediaListItemRole,
    AddonMediaListPresentationKind, AddonMediaListReviewState,
};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddonMediaListRow {
    pub id: i64,
    pub addon_id: String,
    pub provider_id: String,
    pub source_url: String,
    pub name: String,
    pub item_count: i32,
    pub match_count: i32,
    pub auto_refresh: bool,
    pub generate_playlist: bool,
    pub presentation_kind: AddonMediaListPresentationKind,
    pub requested_presentation_kind: AddonMediaListPresentationKind,
    pub presentation_error: Option<String>,
    pub review_state: AddonMediaListReviewState,
    pub review_error: Option<String>,
    pub published_revision: i32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddonMediaListForm {
    pub id: i64,
    pub addon_id: String,
    pub source_url: String,
    pub folder_name: String,
    pub auto_refresh: bool,
    pub generate_playlist: bool,
    pub content_kind_override: AddonMediaListContentKind,
    pub presentation_kind: AddonMediaListPresentationKind,
    pub requested_presentation_kind: AddonMediaListPresentationKind,
    pub presentation_error: Option<String>,
    pub review_state: AddonMediaListReviewState,
    pub review_error: Option<String>,
    pub published_revision: i32,
    pub legacy_chapter_input: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddonMediaListProviderRow {
    pub addon_id: String,
    pub name: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddonMediaListItemRow {
    pub id: i64,
    pub rank: i32,
    pub display_title: String,
    pub availability: AddonMediaListItemAvailability,
    pub availability_reason: Option<String>,
    pub replacement_media_item_id: Option<i64>,
    pub replacement_media_version_id: Option<i64>,
    pub replacement_title: Option<String>,
    pub replacement_path: Option<String>,
    pub replacement_valid: bool,
    pub title: String,
    pub year: Option<i32>,
    pub season: Option<i32>,
    pub episode: Option<i32>,
    pub source_url: Option<String>,
    pub duration_seconds: Option<u64>,
    pub parent_provider_id: Option<String>,
    pub chapter_input: String,
    pub provider_chapter_input: String,
    pub projected_episode_id: Option<i64>,
    pub content_kind: AddonMediaListContentKind,
    pub collection_role: AddonMediaListItemRole,
    pub thumbnail_url: Option<String>,
    pub tags: Vec<String>,
    pub people: Vec<String>,
    pub included: bool,
    pub published: bool,
    pub metadata_conflict_count: u32,
}

/// Per-availability entry totals for one list.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddonMediaListItemCountsRow {
    pub available: u64,
    pub unavailable: u64,
    pub unknown: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddonMediaListNfoPerson {
    pub name: String,
    pub role: Option<String>,
    pub order: Option<i32>,
    pub thumb: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListArtworkRole {
    Poster,
    Fanart,
    Thumb,
    #[default]
    Other,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddonMediaListArtwork {
    pub url: String,
    pub role: AddonMediaListArtworkRole,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub language: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct AddonMediaListNfoMetadata {
    pub title: String,
    pub original_title: Option<String>,
    pub sort_title: Option<String>,
    pub outline: Option<String>,
    pub plot: Option<String>,
    pub tagline: Option<String>,
    pub show_title: Option<String>,
    pub season: Option<i32>,
    pub episode: Option<i32>,
    pub year: Option<i32>,
    pub release_date: Option<String>,
    pub content_ratings: Vec<String>,
    pub genres: Vec<String>,
    pub tags: Vec<String>,
    pub studios: Vec<String>,
    pub music_labels: Vec<String>,
    pub languages: Vec<String>,
    pub people: Vec<AddonMediaListNfoPerson>,
    pub writers: Vec<String>,
    pub directors: Vec<String>,
    pub artists: Vec<String>,
    pub producers: Vec<String>,
    pub original_broadcasters: Vec<String>,
    pub broadcasters: Vec<String>,
    pub collection: Option<String>,
    pub artwork: Vec<AddonMediaListArtwork>,
    pub artwork_urls: Vec<String>,
    pub guids: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListNfoOwnership {
    #[default]
    Generated,
    User,
    Invalid,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddonMediaListNfoForm {
    pub metadata: AddonMediaListNfoMetadata,
    pub etag: String,
    pub ownership: AddonMediaListNfoOwnership,
    pub error: Option<String>,
    pub path_available: bool,
    pub conflicts: BTreeMap<String, AddonMediaListMetadataConflict>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddonMediaListMetadataConflict {
    pub provider_before: serde_json::Value,
    pub provider_after: serde_json::Value,
    pub operator_value: serde_json::Value,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddonMediaListItemDetail {
    pub item: AddonMediaListItemRow,
    pub nfo: AddonMediaListNfoForm,
    pub chapters: Vec<AddonMediaListItemRow>,
}

#[cfg(not(target_arch = "wasm32"))]
impl From<ersatzrs_core::repositories::AddonMediaListItemAdminRow> for AddonMediaListItemRow {
    fn from(row: ersatzrs_core::repositories::AddonMediaListItemAdminRow) -> Self {
        Self {
            id: row.id,
            rank: row.rank,
            display_title: row.display_title,
            availability: row.availability,
            availability_reason: row.availability_reason,
            replacement_media_item_id: row.replacement_media_item_id,
            replacement_media_version_id: row.replacement_media_version_id,
            replacement_title: row.replacement_title,
            replacement_path: row.replacement_path,
            replacement_valid: row.replacement_valid,
            title: row.title,
            year: row.year,
            season: row.season,
            episode: row.episode,
            source_url: row.source_url,
            duration_seconds: row.duration_seconds,
            parent_provider_id: row.parent_provider_id,
            chapter_input: row.chapter_input,
            provider_chapter_input: row.provider_chapter_input,
            projected_episode_id: row.projected_episode_id,
            content_kind: row.content_kind,
            collection_role: row.collection_role,
            thumbnail_url: row.thumbnail_url,
            tags: row.tags,
            people: row.people,
            included: row.included,
            published: row.published,
            metadata_conflict_count: row.metadata_conflict_count,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl From<ersatzrs_core::repositories::AddonMediaListNfoMetadata> for AddonMediaListNfoMetadata {
    fn from(metadata: ersatzrs_core::repositories::AddonMediaListNfoMetadata) -> Self {
        Self {
            title: metadata.title,
            original_title: metadata.original_title,
            sort_title: metadata.sort_title,
            outline: metadata.outline,
            plot: metadata.plot,
            tagline: metadata.tagline,
            show_title: metadata.show_title,
            season: metadata.season,
            episode: metadata.episode,
            year: metadata.year,
            release_date: metadata.release_date,
            content_ratings: metadata.content_ratings,
            genres: metadata.genres,
            tags: metadata.tags,
            studios: metadata.studios,
            music_labels: metadata.music_labels,
            languages: metadata.languages,
            people: metadata
                .people
                .into_iter()
                .map(|person| AddonMediaListNfoPerson {
                    name: person.name,
                    role: person.role,
                    order: person.order,
                    thumb: person.thumb,
                })
                .collect(),
            writers: metadata.writers,
            directors: metadata.directors,
            artists: metadata.artists,
            producers: metadata.producers,
            original_broadcasters: metadata.original_broadcasters,
            broadcasters: metadata.broadcasters,
            collection: metadata.collection,
            artwork: metadata
                .artwork
                .into_iter()
                .map(|artwork| AddonMediaListArtwork {
                    url: artwork.url,
                    role: match artwork.role {
                        ersatzrs_core::repositories::AddonMediaListArtworkRole::Poster => {
                            AddonMediaListArtworkRole::Poster
                        }
                        ersatzrs_core::repositories::AddonMediaListArtworkRole::Fanart => {
                            AddonMediaListArtworkRole::Fanart
                        }
                        ersatzrs_core::repositories::AddonMediaListArtworkRole::Thumb => {
                            AddonMediaListArtworkRole::Thumb
                        }
                        ersatzrs_core::repositories::AddonMediaListArtworkRole::Other => {
                            AddonMediaListArtworkRole::Other
                        }
                    },
                    width: artwork.width,
                    height: artwork.height,
                    language: artwork.language,
                })
                .collect(),
            artwork_urls: metadata.artwork_urls,
            guids: metadata.guids,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl From<AddonMediaListNfoMetadata> for ersatzrs_core::repositories::AddonMediaListNfoMetadata {
    fn from(metadata: AddonMediaListNfoMetadata) -> Self {
        Self {
            title: metadata.title,
            original_title: metadata.original_title,
            sort_title: metadata.sort_title,
            outline: metadata.outline,
            plot: metadata.plot,
            tagline: metadata.tagline,
            show_title: metadata.show_title,
            season: metadata.season,
            episode: metadata.episode,
            year: metadata.year,
            release_date: metadata.release_date,
            content_ratings: metadata.content_ratings,
            genres: metadata.genres,
            tags: metadata.tags,
            studios: metadata.studios,
            music_labels: metadata.music_labels,
            languages: metadata.languages,
            people: metadata
                .people
                .into_iter()
                .map(
                    |person| ersatzrs_core::repositories::AddonMediaListNfoPerson {
                        name: person.name,
                        role: person.role,
                        order: person.order,
                        thumb: person.thumb,
                    },
                )
                .collect(),
            writers: metadata.writers,
            directors: metadata.directors,
            artists: metadata.artists,
            producers: metadata.producers,
            original_broadcasters: metadata.original_broadcasters,
            broadcasters: metadata.broadcasters,
            collection: metadata.collection,
            artwork: metadata
                .artwork
                .into_iter()
                .map(
                    |artwork| ersatzrs_core::repositories::AddonMediaListArtwork {
                        url: artwork.url,
                        role: match artwork.role {
                            AddonMediaListArtworkRole::Poster => {
                                ersatzrs_core::repositories::AddonMediaListArtworkRole::Poster
                            }
                            AddonMediaListArtworkRole::Fanart => {
                                ersatzrs_core::repositories::AddonMediaListArtworkRole::Fanart
                            }
                            AddonMediaListArtworkRole::Thumb => {
                                ersatzrs_core::repositories::AddonMediaListArtworkRole::Thumb
                            }
                            AddonMediaListArtworkRole::Other => {
                                ersatzrs_core::repositories::AddonMediaListArtworkRole::Other
                            }
                        },
                        width: artwork.width,
                        height: artwork.height,
                        language: artwork.language,
                    },
                )
                .collect(),
            artwork_urls: metadata.artwork_urls,
            guids: metadata.guids,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl From<ersatzrs_core::repositories::AddonMediaListNfoForm> for AddonMediaListNfoForm {
    fn from(form: ersatzrs_core::repositories::AddonMediaListNfoForm) -> Self {
        use ersatzrs_core::repositories::AddonMediaListNfoOwnership as CoreOwnership;
        Self {
            metadata: form.metadata.into(),
            etag: form.etag,
            ownership: match form.ownership {
                CoreOwnership::Generated => AddonMediaListNfoOwnership::Generated,
                CoreOwnership::User => AddonMediaListNfoOwnership::User,
                CoreOwnership::Invalid => AddonMediaListNfoOwnership::Invalid,
            },
            error: form.error,
            path_available: form.path_available,
            conflicts: form
                .conflicts
                .into_iter()
                .map(|(field, conflict)| {
                    (
                        field,
                        AddonMediaListMetadataConflict {
                            provider_before: conflict.provider_before,
                            provider_after: conflict.provider_after,
                            operator_value: conflict.operator_value,
                        },
                    )
                })
                .collect(),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl From<ersatzrs_core::repositories::AddonMediaListItemDetail> for AddonMediaListItemDetail {
    fn from(detail: ersatzrs_core::repositories::AddonMediaListItemDetail) -> Self {
        Self {
            item: detail.item.into(),
            nfo: detail.nfo.into(),
            chapters: detail.chapters.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddonMediaListReplacementRow {
    pub media_item_id: i64,
    pub media_version_id: i64,
    pub title: String,
    pub path: String,
}

#[server]
pub async fn list_addon_media_list_providers(
) -> Result<Vec<AddonMediaListProviderRow>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let overview = state
        .application
        .addon_overview()
        .await
        .map_err(ServerFnError::new)?;
    let mut providers = overview
        .installed
        .into_iter()
        .filter(|item| item.enabled)
        .filter(|item| {
            item.manifest.capabilities.iter().any(|capability| {
                capability.id == ersatzrs_addon_contract::CAPABILITY_MEDIA_LIST_LIST_V1
                    || capability.id == ersatzrs_addon_contract::CAPABILITY_MEDIA_LIST_LIST_V2
                    || capability.id == ersatzrs_addon_contract::CAPABILITY_MEDIA_LIST_LIST_V3
                    || capability.id == ersatzrs_addon_contract::CAPABILITY_MEDIA_LIST_LIST_V4
            })
        })
        .map(|item| AddonMediaListProviderRow {
            addon_id: item.addon_id,
            name: item
                .manifest
                .name
                .get("en-US")
                .or_else(|| item.manifest.name.values().next())
                .cloned()
                .unwrap_or_default(),
        })
        .collect::<Vec<_>>();
    providers.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(providers)
}

/// The add-on that would process one operator source URL, if exactly one does.
///
/// The dialog calls this while the operator edits the URL so the resolved
/// choice is visible before saving, and remains overridable.
#[server]
pub async fn resolve_addon_media_list_provider(
    source_url: String,
) -> Result<Option<AddonMediaListProviderRow>, ServerFnError> {
    use crate::host::state::AppState;

    let source_url = source_url.trim().to_owned();
    if source_url.is_empty() {
        return Ok(None);
    }
    let state = expect_context::<AppState>();
    let routing = state
        .application
        .resolve_source_addon(
            source_url,
            MEDIA_LIST_CAPABILITIES
                .iter()
                .map(|capability| (*capability).to_owned())
                .collect(),
        )
        .await
        .map_err(ServerFnError::new)?;
    let Some(addon_id) = routing.addon_id().map(str::to_owned) else {
        return Ok(None);
    };
    Ok(list_addon_media_list_providers()
        .await?
        .into_iter()
        .find(|provider| provider.addon_id == addon_id))
}

#[server]
pub async fn list_addon_media_lists(
    page: u32,
    page_size: u32,
) -> Result<(Vec<AddonMediaListRow>, u64), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    list_addon_media_lists_with_state(&state, page, page_size)
        .await
        .map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
pub async fn list_addon_media_lists_with_state(
    state: &crate::host::state::AppState,
    page: u32,
    page_size: u32,
) -> ersatzrs_app::Result<(Vec<AddonMediaListRow>, u64)> {
    let (rows, total) = state
        .application
        .list_addon_media_lists(ersatzrs_app::queries::ListAddonMediaLists { page, page_size })
        .await?;
    Ok((
        rows.into_iter()
            .map(|row| AddonMediaListRow {
                id: row.id,
                addon_id: row.addon_id,
                provider_id: row.provider_id,
                source_url: row.source_url,
                name: row.name,
                item_count: row.item_count,
                match_count: row.match_count,
                auto_refresh: row.auto_refresh,
                generate_playlist: row.generate_playlist,
                presentation_kind: row.presentation_kind,
                requested_presentation_kind: row.requested_presentation_kind,
                presentation_error: row.presentation_error,
                review_state: row.review_state,
                review_error: row.review_error,
                published_revision: row.published_revision,
            })
            .collect(),
        total,
    ))
}

#[server]
pub async fn get_addon_media_lists_page_size() -> Result<u32, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .get_addon_media_lists_page_size()
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn get_addon_media_list_form(
    id: i64,
) -> Result<Option<AddonMediaListForm>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .get_addon_media_list(id)
        .await
        .map_err(ServerFnError::new)
        .map(|form| {
            form.map(|form| AddonMediaListForm {
                id: form.id,
                addon_id: form.addon_id,
                source_url: form.source_url,
                folder_name: form.folder_name,
                auto_refresh: form.auto_refresh,
                generate_playlist: form.generate_playlist,
                content_kind_override: form.content_kind_override,
                presentation_kind: form.presentation_kind,
                requested_presentation_kind: form.requested_presentation_kind,
                presentation_error: form.presentation_error,
                review_state: form.review_state,
                review_error: form.review_error,
                published_revision: form.published_revision,
                legacy_chapter_input: form.legacy_chapter_input,
            })
        })
}

#[server]
pub async fn list_addon_media_list_items(
    id: i64,
) -> Result<Vec<AddonMediaListItemRow>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .list_addon_media_list_items(id)
        .await
        .map_err(ServerFnError::new)
        .map(|rows| {
            rows.into_iter()
                .map(|row| AddonMediaListItemRow {
                    id: row.id,
                    rank: row.rank,
                    display_title: row.display_title,
                    availability: row.availability,
                    availability_reason: row.availability_reason,
                    replacement_media_item_id: row.replacement_media_item_id,
                    replacement_media_version_id: row.replacement_media_version_id,
                    replacement_title: row.replacement_title,
                    replacement_path: row.replacement_path,
                    replacement_valid: row.replacement_valid,
                    title: row.title,
                    year: row.year,
                    season: row.season,
                    episode: row.episode,
                    source_url: row.source_url,
                    duration_seconds: row.duration_seconds,
                    parent_provider_id: row.parent_provider_id,
                    chapter_input: row.chapter_input,
                    provider_chapter_input: row.provider_chapter_input,
                    projected_episode_id: row.projected_episode_id,
                    content_kind: row.content_kind,
                    collection_role: row.collection_role,
                    thumbnail_url: row.thumbnail_url,
                    tags: row.tags,
                    people: row.people,
                    included: row.included,
                    published: row.published,
                    metadata_conflict_count: row.metadata_conflict_count,
                })
                .collect()
        })
}

/// Load one page of entries in one availability group.
///
/// A `page_size` of zero means "every entry in this group", which the editor
/// offers explicitly.
#[server]
pub async fn list_addon_media_list_items_page(
    id: i64,
    availability: AddonMediaListItemAvailability,
    page: u32,
    page_size: u32,
) -> Result<Vec<AddonMediaListItemRow>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .list_addon_media_list_items_page(
            id,
            availability,
            page,
            (page_size > 0).then_some(page_size),
        )
        .await
        .map_err(ServerFnError::new)
        .map(|rows| rows.into_iter().map(Into::into).collect())
}

/// Count the entries of one list per availability group.
#[server]
pub async fn get_addon_media_list_item_counts(
    id: i64,
) -> Result<AddonMediaListItemCountsRow, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .addon_media_list_item_counts(id)
        .await
        .map_err(ServerFnError::new)
        .map(|counts| AddonMediaListItemCountsRow {
            available: counts.available,
            unavailable: counts.unavailable,
            unknown: counts.unknown,
        })
}

/// Set the descriptive classification of one entry.
#[server]
pub async fn set_addon_media_list_item_content_kind(
    item_id: i64,
    content_kind: AddonMediaListContentKind,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    expect_context::<AppState>()
        .application
        .set_addon_media_list_item_content_kind(
            ersatzrs_app::commands::SetAddonMediaListItemContentKind {
                item_id,
                content_kind,
            },
        )
        .await
        .map_err(ServerFnError::new)
}

/// Include or exclude one entry from the next publication.
#[server]
pub async fn set_addon_media_list_item_included(
    list_id: i64,
    item_id: i64,
    included: bool,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    expect_context::<AppState>()
        .application
        .set_addon_media_list_item_included(ersatzrs_app::commands::SetAddonMediaListItemIncluded {
            list_id,
            item_id,
            included,
        })
        .await
        .map_err(ServerFnError::new)
}

/// Set the collection-aware role of one source for the next publication.
#[server]
pub async fn set_addon_media_list_item_role(
    list_id: i64,
    item_id: i64,
    role: AddonMediaListItemRole,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    expect_context::<AppState>()
        .application
        .set_addon_media_list_item_role(ersatzrs_app::commands::SetAddonMediaListItemRole {
            list_id,
            item_id,
            role,
        })
        .await
        .map_err(ServerFnError::new)
}

/// Apply one classification to every entry of a list as a bulk default.
#[server]
pub async fn apply_addon_media_list_content_kind(
    id: i64,
    content_kind: AddonMediaListContentKind,
) -> Result<u64, ServerFnError> {
    use crate::host::state::AppState;

    expect_context::<AppState>()
        .application
        .apply_addon_media_list_content_kind(
            ersatzrs_app::commands::ApplyAddonMediaListContentKind { id, content_kind },
        )
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn search_addon_media_list_replacements(
    query: String,
) -> Result<Vec<AddonMediaListReplacementRow>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .search_addon_media_list_replacements(&query)
        .await
        .map_err(ServerFnError::new)
        .map(|rows| {
            rows.into_iter()
                .map(|row| AddonMediaListReplacementRow {
                    media_item_id: row.media_item_id,
                    media_version_id: row.media_version_id,
                    title: row.title,
                    path: row.path,
                })
                .collect()
        })
}

#[server]
pub async fn set_addon_media_list_item_replacement(
    item_id: i64,
    media_item_id: Option<i64>,
    media_version_id: Option<i64>,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .set_addon_media_list_item_replacement(
            ersatzrs_app::commands::SetAddonMediaListItemReplacement {
                item_id,
                media_item_id,
                media_version_id,
            },
        )
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn get_addon_media_list_item(
    list_id: i64,
    item_id: i64,
) -> Result<Option<AddonMediaListItemDetail>, ServerFnError> {
    use crate::host::state::AppState;

    expect_context::<AppState>()
        .application
        .get_addon_media_list_item(list_id, item_id)
        .await
        .map(|detail| detail.map(Into::into))
        .map_err(ServerFnError::new)
}

#[server]
pub async fn get_addon_media_list_show_nfo(
    id: i64,
) -> Result<AddonMediaListNfoForm, ServerFnError> {
    use crate::host::state::AppState;

    expect_context::<AppState>()
        .application
        .get_addon_media_list_show_nfo(id)
        .await
        .map(Into::into)
        .map_err(ServerFnError::new)
}

#[server]
pub async fn request_addon_media_list_presentation(
    id: i64,
    presentation_kind: AddonMediaListPresentationKind,
    show_title: Option<String>,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    expect_context::<AppState>()
        .application
        .request_addon_media_list_presentation(
            ersatzrs_app::commands::RequestAddonMediaListPresentation {
                id,
                presentation_kind,
                show_title,
            },
        )
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn save_addon_media_list_show_nfo(
    id: i64,
    metadata: AddonMediaListNfoMetadata,
    etag: String,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    expect_context::<AppState>()
        .application
        .save_addon_media_list_show_nfo(ersatzrs_app::commands::SaveAddonMediaListNfo {
            id,
            list_id: None,
            metadata: metadata.into(),
            etag,
        })
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn save_addon_media_list_item_nfo(
    list_id: i64,
    id: i64,
    metadata: AddonMediaListNfoMetadata,
    etag: String,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    expect_context::<AppState>()
        .application
        .save_addon_media_list_item_nfo(ersatzrs_app::commands::SaveAddonMediaListNfo {
            id,
            list_id: Some(list_id),
            metadata: metadata.into(),
            etag,
        })
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn reset_addon_media_list_nfo(
    list_id: i64,
    item_id: Option<i64>,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    expect_context::<AppState>()
        .application
        .reset_addon_media_list_nfo(list_id, item_id)
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn update_addon_media_list_item_chapters(
    id: i64,
    chapter_input: String,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    expect_context::<AppState>()
        .application
        .update_addon_media_list_item_chapters(
            ersatzrs_app::commands::UpdateAddonMediaListItemChapters { id, chapter_input },
        )
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn add_addon_media_list(
    addon_id: String,
    source_url: String,
    folder_name: String,
    content_kind_override: AddonMediaListContentKind,
    chapter_input: String,
) -> Result<i64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    add_addon_media_list_with_state(
        &state,
        addon_id,
        source_url,
        folder_name,
        content_kind_override,
        chapter_input,
    )
    .await
    .map_err(ServerFnError::new)
}

#[server]
pub async fn match_addon_media_list(id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    match_addon_media_list_with_state(&state, id)
        .await
        .map_err(ServerFnError::new)
}

/// Publish the reviewed metadata snapshot, then scan its managed library.
#[server]
pub async fn publish_addon_media_list(id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .publish_addon_media_list(ersatzrs_app::commands::PublishAddonMediaList { id })
        .await
        .map_err(ServerFnError::new)?;
    if let Some(library_id) = state
        .application
        .addon_media_list_library_id(id)
        .await
        .map_err(ServerFnError::new)?
    {
        let _ = state
            .application
            .request_library_scan(ersatzrs_app::commands::RequestLibraryScan::new(
                library_id, false,
            ))
            .await;
    }
    Ok(())
}

#[server]
pub async fn update_addon_media_list(form: AddonMediaListForm) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    update_addon_media_list_with_state(&state, form)
        .await
        .map_err(ServerFnError::new)
}

/// Move one saved list to a different add-on and re-import it through it.
#[server]
pub async fn change_addon_media_list_addon(id: i64, addon_id: String) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let logged_addon_id = addon_id.clone();
    state
        .application
        .change_addon_media_list_addon(ersatzrs_app::commands::ChangeAddonMediaListAddon {
            id,
            addon_id,
        })
        .await
        .inspect_err(|error| {
            log_addon_media_list_failure("change-addon", Some(&logged_addon_id), Some(id), error);
        })
        .map_err(ServerFnError::new)
}

#[server]
pub async fn delete_addon_media_list(id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    delete_addon_media_list_with_state(&state, id)
        .await
        .map_err(ServerFnError::new)
}

/// Record a failed add-on media list operation with its provider diagnostic.
///
/// The admin UI surfaces the same message to the operator, and add-on
/// diagnostics reaching this layer are already bounded and redacted by the
/// contract boundary, so no URL, secret, or local path is emitted here beyond
/// what the operator already sees.
#[cfg(feature = "ssr")]
fn log_addon_media_list_failure(
    operation: &'static str,
    addon_id: Option<&str>,
    list_id: Option<i64>,
    error: &ersatzrs_app::ApplicationError,
) {
    tracing::warn!(
        operation,
        addon_id = addon_id.unwrap_or("n/a"),
        list_id = list_id.unwrap_or_default(),
        failure_kind = "addon-media-list",
        operation_message = %error,
        "add-on media list operation failed"
    );
}

/// Every contract version that can enumerate an add-on media list.
#[cfg(feature = "ssr")]
const MEDIA_LIST_CAPABILITIES: [&str; 4] = [
    ersatzrs_addon_contract::CAPABILITY_MEDIA_LIST_LIST_V1,
    ersatzrs_addon_contract::CAPABILITY_MEDIA_LIST_LIST_V2,
    ersatzrs_addon_contract::CAPABILITY_MEDIA_LIST_LIST_V3,
    ersatzrs_addon_contract::CAPABILITY_MEDIA_LIST_LIST_V4,
];

/// Select the add-on for one operator source URL when none was chosen.
///
/// Both the interactive dialog and the plain HTML form reach this seam, so the
/// stored add-on is identical whichever path the operator used.
#[cfg(feature = "ssr")]
async fn resolve_media_list_addon(
    state: &crate::host::state::AppState,
    source_url: &str,
) -> ersatzrs_app::Result<String> {
    use ersatzrs_app::SourceAddonRouting;

    let routing = state
        .application
        .resolve_source_addon(
            source_url.to_owned(),
            MEDIA_LIST_CAPABILITIES
                .iter()
                .map(|capability| (*capability).to_owned())
                .collect(),
        )
        .await
        .map_err(|error| ersatzrs_app::ApplicationError::Validation {
            field: "addon_id",
            message: error.to_string(),
        })?;
    match routing {
        SourceAddonRouting::Declared(addon_id) | SourceAddonRouting::Default(addon_id) => {
            Ok(addon_id)
        }
        SourceAddonRouting::None => Err(ersatzrs_app::ApplicationError::Validation {
            field: "addon_id",
            message: "No installed add-on handles this URL".to_owned(),
        }),
        SourceAddonRouting::Ambiguous(addon_ids) => {
            Err(ersatzrs_app::ApplicationError::Validation {
                field: "addon_id",
                message: format!(
                    "Several add-ons handle this URL; choose one: {}",
                    addon_ids.join(", ")
                ),
            })
        }
    }
}

#[cfg(feature = "ssr")]
pub async fn add_addon_media_list_with_state(
    state: &crate::host::state::AppState,
    addon_id: String,
    source_url: String,
    folder_name: String,
    content_kind_override: AddonMediaListContentKind,
    chapter_input: String,
) -> ersatzrs_app::Result<i64> {
    // An omitted choice is resolved from the URL rather than rejected, so the
    // operator only has to name an add-on when nothing claims the host.
    let addon_id = if addon_id.trim().is_empty() {
        resolve_media_list_addon(state, source_url.trim()).await?
    } else {
        addon_id
    };
    let logged_addon_id = addon_id.clone();
    let list_id = state
        .application
        .add_addon_media_list(ersatzrs_app::commands::AddAddonMediaList {
            addon_id,
            source_url,
            folder_name,
            content_kind_override,
            chapter_input,
        })
        .await
        .inspect_err(|error| {
            log_addon_media_list_failure("add", Some(&logged_addon_id), None, error);
        })?;
    let published = state
        .application
        .get_addon_media_list(list_id)
        .await?
        .is_some_and(|form| form.review_state == AddonMediaListReviewState::Published);
    if published {
        if let Some(library_id) = state
            .application
            .addon_media_list_library_id(list_id)
            .await?
        {
            let outcome = state
                .application
                .request_library_scan(ersatzrs_app::commands::RequestLibraryScan::new(
                    library_id, false,
                ))
                .await;
            tracing::info!(
                list_id,
                library_id,
                ?outcome,
                "queued managed add-on list library scan"
            );
        }
    }
    Ok(list_id)
}

#[cfg(feature = "ssr")]
pub async fn match_addon_media_list_with_state(
    state: &crate::host::state::AppState,
    id: i64,
) -> ersatzrs_app::Result<()> {
    state
        .application
        .match_addon_media_list(ersatzrs_app::commands::MatchAddonMediaList { id })
        .await
        .inspect_err(|error| log_addon_media_list_failure("match", None, Some(id), error))
}

#[cfg(feature = "ssr")]
pub async fn update_addon_media_list_with_state(
    state: &crate::host::state::AppState,
    form: AddonMediaListForm,
) -> ersatzrs_app::Result<()> {
    let id = form.id;
    state
        .application
        .update_addon_media_list(ersatzrs_app::commands::UpdateAddonMediaList {
            id: form.id,
            auto_refresh: form.auto_refresh,
            generate_playlist: form.generate_playlist,
            content_kind_override: form.content_kind_override,
            chapter_input: form.legacy_chapter_input,
        })
        .await
        .inspect_err(|error| log_addon_media_list_failure("update", None, Some(id), error))?;
    // Save is the review boundary. Publication stages the durable imported
    // snapshot and performs the one required library scan, so settings edits
    // must not queue an eager scan (or indirectly enumerate the add-on) here.
    Ok(())
}

#[cfg(feature = "ssr")]
pub async fn delete_addon_media_list_with_state(
    state: &crate::host::state::AppState,
    id: i64,
) -> ersatzrs_app::Result<()> {
    state
        .application
        .delete_addon_media_list(ersatzrs_app::commands::DeleteAddonMediaList { id })
        .await
        .inspect_err(|error| log_addon_media_list_failure("delete", None, Some(id), error))
}
