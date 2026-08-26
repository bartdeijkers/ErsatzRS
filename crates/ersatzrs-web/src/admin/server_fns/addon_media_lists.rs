use ersatzrs_domain::{
    AddonMediaListContentKind, AddonMediaListDetailPolicy, AddonMediaListDetailState,
    AddonMediaListImportState, AddonMediaListItemAvailability, AddonMediaListItemKind,
    AddonMediaListItemRole, AddonMediaListPresentationKind, AddonMediaListReviewState,
    RemoteStreamLiveness,
};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum AddonMediaListPublishResult {
    Published,
    PreparingPlaybackDetails { item_count: u64 },
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListPublicationBlockerKind {
    OverviewIncomplete,
    PublicationInProgress,
    RecoveryRequired,
    CollectionTitleMissing,
    InvalidArtwork,
    NoIncludedItems,
    MoviePrimaryCount,
    PlaybackDecisionRequired,
    PlaybackDetailsUnavailable,
    PlaybackDetailsFailed,
    UnavailableWithoutReplacement,
    ItemTitleMissing,
    SeasonEpisodeMissing,
    ArtistMissing,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddonMediaListPublicationBlocker {
    pub kind: AddonMediaListPublicationBlockerKind,
    pub item_rank: Option<i32>,
    pub item_title: Option<String>,
    pub count: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddonMediaListPublicationReadiness {
    pub included_count: u64,
    pub blockers: Vec<AddonMediaListPublicationBlocker>,
    pub recommendation_count: u64,
    pub required_playback_detail_count: u64,
    pub required_playback_detail_pending: u64,
    pub optional_detail_total: u64,
    pub optional_detail_complete: u64,
    pub optional_detail_pending: u64,
    pub saved_revision: i32,
    pub review_state: AddonMediaListReviewState,
}

#[cfg(not(target_arch = "wasm32"))]
impl From<ersatzrs_core::repositories::AddonMediaListPublicationReadiness>
    for AddonMediaListPublicationReadiness
{
    fn from(value: ersatzrs_core::repositories::AddonMediaListPublicationReadiness) -> Self {
        Self {
            included_count: value.included_count,
            blockers: value.blockers.into_iter().map(Into::into).collect(),
            recommendation_count: value.recommendation_count,
            required_playback_detail_count: value.required_playback_detail_count,
            required_playback_detail_pending: value.required_playback_detail_pending,
            optional_detail_total: value.optional_detail_total,
            optional_detail_complete: value.optional_detail_complete,
            optional_detail_pending: value.optional_detail_pending,
            saved_revision: value.saved_revision,
            review_state: value.review_state,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl From<ersatzrs_core::repositories::AddonMediaListPublicationBlocker>
    for AddonMediaListPublicationBlocker
{
    fn from(value: ersatzrs_core::repositories::AddonMediaListPublicationBlocker) -> Self {
        Self {
            kind: match value.kind {
                ersatzrs_core::repositories::AddonMediaListPublicationBlockerKind::OverviewIncomplete => AddonMediaListPublicationBlockerKind::OverviewIncomplete,
                ersatzrs_core::repositories::AddonMediaListPublicationBlockerKind::PublicationInProgress => AddonMediaListPublicationBlockerKind::PublicationInProgress,
                ersatzrs_core::repositories::AddonMediaListPublicationBlockerKind::RecoveryRequired => AddonMediaListPublicationBlockerKind::RecoveryRequired,
                ersatzrs_core::repositories::AddonMediaListPublicationBlockerKind::CollectionTitleMissing => AddonMediaListPublicationBlockerKind::CollectionTitleMissing,
                ersatzrs_core::repositories::AddonMediaListPublicationBlockerKind::InvalidArtwork => AddonMediaListPublicationBlockerKind::InvalidArtwork,
                ersatzrs_core::repositories::AddonMediaListPublicationBlockerKind::NoIncludedItems => AddonMediaListPublicationBlockerKind::NoIncludedItems,
                ersatzrs_core::repositories::AddonMediaListPublicationBlockerKind::MoviePrimaryCount => AddonMediaListPublicationBlockerKind::MoviePrimaryCount,
                ersatzrs_core::repositories::AddonMediaListPublicationBlockerKind::PlaybackDecisionRequired => AddonMediaListPublicationBlockerKind::PlaybackDecisionRequired,
                ersatzrs_core::repositories::AddonMediaListPublicationBlockerKind::PlaybackDetailsUnavailable => AddonMediaListPublicationBlockerKind::PlaybackDetailsUnavailable,
                ersatzrs_core::repositories::AddonMediaListPublicationBlockerKind::PlaybackDetailsFailed => AddonMediaListPublicationBlockerKind::PlaybackDetailsFailed,
                ersatzrs_core::repositories::AddonMediaListPublicationBlockerKind::UnavailableWithoutReplacement => AddonMediaListPublicationBlockerKind::UnavailableWithoutReplacement,
                ersatzrs_core::repositories::AddonMediaListPublicationBlockerKind::ItemTitleMissing => AddonMediaListPublicationBlockerKind::ItemTitleMissing,
                ersatzrs_core::repositories::AddonMediaListPublicationBlockerKind::SeasonEpisodeMissing => AddonMediaListPublicationBlockerKind::SeasonEpisodeMissing,
                ersatzrs_core::repositories::AddonMediaListPublicationBlockerKind::ArtistMissing => AddonMediaListPublicationBlockerKind::ArtistMissing,
            },
            item_rank: value.item_rank,
            item_title: value.item_title,
            count: value.count,
        }
    }
}

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
    pub detail_import_policy: AddonMediaListDetailPolicy,
    pub import_progress: Option<AddonMediaListImportProgressRow>,
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
    pub detail_import_policy: AddonMediaListDetailPolicy,
    pub import_progress: Option<AddonMediaListImportProgressRow>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddonMediaListImportProgressRow {
    pub state: AddonMediaListImportState,
    pub started_at: String,
    pub discovered_count: u64,
    pub total_hint: Option<u64>,
    pub detail_total: u64,
    pub detail_complete: u64,
    pub detail_skipped: u64,
    pub detail_failed: u64,
    pub detail_pending: u64,
    pub current_title: Option<String>,
    pub last_progress_at: Option<String>,
    pub diagnostic: Option<String>,
}

/// Browser-safe wire representation of an operator action for an overview import.
///
/// The matching repository type is intentionally native-only; keeping this small
/// enum in the web contract prevents the hydrated bundle from depending on
/// `ersatzrs-core`.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListImportAction {
    Pause,
    Resume,
    Retry,
    Cancel,
}

#[cfg(not(target_arch = "wasm32"))]
impl From<AddonMediaListImportAction> for ersatzrs_core::repositories::AddonMediaListImportAction {
    fn from(action: AddonMediaListImportAction) -> Self {
        match action {
            AddonMediaListImportAction::Pause => Self::Pause,
            AddonMediaListImportAction::Resume => Self::Resume,
            AddonMediaListImportAction::Retry => Self::Retry,
            AddonMediaListImportAction::Cancel => Self::Cancel,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl From<ersatzrs_core::repositories::AddonMediaListImportProgress>
    for AddonMediaListImportProgressRow
{
    fn from(progress: ersatzrs_core::repositories::AddonMediaListImportProgress) -> Self {
        Self {
            state: progress.state,
            started_at: progress.started_at,
            discovered_count: progress.discovered_count,
            total_hint: progress.total_hint,
            detail_total: progress.detail_total,
            detail_complete: progress.detail_complete,
            detail_skipped: progress.detail_skipped,
            detail_failed: progress.detail_failed,
            detail_pending: progress.detail_pending,
            current_title: progress.current_title,
            last_progress_at: progress.last_progress_at,
            diagnostic: progress.diagnostic,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddonMediaListProviderRow {
    pub addon_id: String,
    pub name: String,
}

// These independent transport projections mirror the admin read model; they
// are not alternate states of one boolean state machine.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddonMediaListItemRow {
    pub id: i64,
    pub rank: i32,
    pub kind: AddonMediaListItemKind,
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
    pub first_aired: Option<String>,
    pub source_url: Option<String>,
    pub duration_seconds: Option<u64>,
    pub provider_liveness: RemoteStreamLiveness,
    pub liveness_override: Option<RemoteStreamLiveness>,
    pub effective_liveness: RemoteStreamLiveness,
    pub liveness_conflict: bool,
    pub published_liveness: RemoteStreamLiveness,
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
    pub detail_state: AddonMediaListDetailState,
    pub detail_diagnostic: Option<String>,
}

/// Exact operator-owned values for one quick table edit or undo snapshot.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddonMediaListItemReviewEdit {
    pub item_id: i64,
    pub content_kind: AddonMediaListContentKind,
    pub included: bool,
    pub liveness_override: Option<RemoteStreamLiveness>,
    pub season: Option<i32>,
    pub episode: Option<i32>,
}

#[cfg(not(target_arch = "wasm32"))]
impl From<AddonMediaListItemReviewEdit>
    for ersatzrs_core::repositories::AddonMediaListItemReviewEdit
{
    fn from(edit: AddonMediaListItemReviewEdit) -> Self {
        Self {
            item_id: edit.item_id,
            content_kind: edit.content_kind,
            included: edit.included,
            liveness_override: edit.liveness_override,
            season: edit.season,
            episode: edit.episode,
        }
    }
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
            kind: row.kind,
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
            first_aired: row.first_aired,
            source_url: row.source_url,
            duration_seconds: row.duration_seconds,
            provider_liveness: row.provider_liveness,
            liveness_override: row.liveness_override,
            effective_liveness: row.effective_liveness,
            liveness_conflict: row.liveness_conflict,
            published_liveness: row.published_liveness,
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
            detail_state: row.detail_state,
            detail_diagnostic: row.detail_diagnostic,
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
                    || capability.id == ersatzrs_addon_contract::CAPABILITY_MEDIA_LIST_LIST_V5
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
                detail_import_policy: row.detail_import_policy,
                import_progress: row.import_progress.map(Into::into),
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
                detail_import_policy: form.detail_import_policy,
                import_progress: form.import_progress.map(Into::into),
            })
        })
}

#[server]
pub async fn get_addon_media_list_publication_readiness(
    id: i64,
) -> Result<AddonMediaListPublicationReadiness, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    get_addon_media_list_publication_readiness_with_state(&state, id)
        .await
        .map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
pub async fn get_addon_media_list_publication_readiness_with_state(
    state: &crate::host::state::AppState,
    id: i64,
) -> ersatzrs_app::Result<AddonMediaListPublicationReadiness> {
    state
        .application
        .addon_media_list_publication_readiness(id)
        .await
        .map(Into::into)
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
                    kind: row.kind,
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
                    first_aired: row.first_aired,
                    source_url: row.source_url,
                    duration_seconds: row.duration_seconds,
                    provider_liveness: row.provider_liveness,
                    liveness_override: row.liveness_override,
                    effective_liveness: row.effective_liveness,
                    liveness_conflict: row.liveness_conflict,
                    published_liveness: row.published_liveness,
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
                    detail_state: row.detail_state,
                    detail_diagnostic: row.detail_diagnostic,
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

/// Set or restore the playback behavior of one Remote Stream entry.
#[server]
pub async fn set_addon_media_list_item_liveness(
    list_id: i64,
    item_id: i64,
    liveness: Option<RemoteStreamLiveness>,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    expect_context::<AppState>()
        .application
        .set_addon_media_list_item_liveness(ersatzrs_app::commands::SetAddonMediaListItemLiveness {
            list_id,
            item_id,
            liveness,
        })
        .await
        .map_err(ServerFnError::new)
}

/// Apply or restore playback behavior for selected or all Remote Stream entries.
#[server]
pub async fn apply_addon_media_list_liveness(
    id: i64,
    liveness: Option<RemoteStreamLiveness>,
    item_ids: Option<Vec<i64>>,
) -> Result<u64, ServerFnError> {
    use crate::host::state::AppState;

    expect_context::<AppState>()
        .application
        .apply_addon_media_list_liveness(ersatzrs_app::commands::ApplyAddonMediaListLiveness {
            id,
            liveness,
            item_ids: item_ids.unwrap_or_default(),
        })
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

/// Atomically apply quick-review values or a prior undo snapshot.
#[server]
pub async fn apply_addon_media_list_item_review_edits(
    id: i64,
    edits: Vec<AddonMediaListItemReviewEdit>,
) -> Result<u64, ServerFnError> {
    use crate::host::state::AppState;

    expect_context::<AppState>()
        .application
        .apply_addon_media_list_item_review_edits(
            ersatzrs_app::commands::ApplyAddonMediaListItemReviewEdits {
                id,
                edits: edits.into_iter().map(Into::into).collect(),
            },
        )
        .await
        .map_err(ServerFnError::new)
}

/// Assign one season and consecutive episode numbers in original provider order.
#[server]
pub async fn assign_addon_media_list_episode_numbers(
    id: i64,
    season: i32,
    starting_episode: i32,
    item_count: Option<u32>,
    missing_only: bool,
) -> Result<u64, ServerFnError> {
    use crate::host::state::AppState;

    expect_context::<AppState>()
        .application
        .assign_addon_media_list_episode_numbers(
            ersatzrs_app::commands::AssignAddonMediaListEpisodeNumbers {
                id,
                season,
                starting_episode,
                item_count,
                missing_only,
            },
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
    load_item_details: bool,
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
        if load_item_details {
            AddonMediaListDetailPolicy::Automatic
        } else {
            AddonMediaListDetailPolicy::ManualOnly
        },
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
pub async fn publish_addon_media_list(
    id: i64,
) -> Result<AddonMediaListPublishResult, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    publish_addon_media_list_with_state(&state, id)
        .await
        .map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
pub async fn publish_addon_media_list_with_state(
    state: &crate::host::state::AppState,
    id: i64,
) -> ersatzrs_app::Result<AddonMediaListPublishResult> {
    let pending_details = state
        .application
        .publish_addon_media_list(ersatzrs_app::commands::PublishAddonMediaList { id })
        .await?;
    if let Some(item_count) = pending_details {
        return Ok(AddonMediaListPublishResult::PreparingPlaybackDetails { item_count });
    }
    if let Some(library_id) = state.application.addon_media_list_library_id(id).await? {
        let _ = state
            .application
            .request_library_scan(ersatzrs_app::commands::RequestLibraryScan::new(
                library_id, false,
            ))
            .await;
    }
    Ok(AddonMediaListPublishResult::Published)
}

#[server]
pub async fn update_addon_media_list(form: AddonMediaListForm) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    update_addon_media_list_with_state(&state, form)
        .await
        .map_err(ServerFnError::new)
}

/// Queue one top-level item for detail enrichment with manual priority.
#[server]
pub async fn request_addon_media_list_item_details(
    list_id: i64,
    item_id: i64,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    expect_context::<AppState>()
        .application
        .request_addon_media_list_item_details(
            ersatzrs_app::commands::RequestAddonMediaListItemDetails { list_id, item_id },
        )
        .await
        .map_err(ServerFnError::new)
}

/// Pause, resume, retry, or cancel the current overview generation.
#[server]
pub async fn control_addon_media_list_import(
    id: i64,
    action: AddonMediaListImportAction,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    expect_context::<AppState>()
        .application
        .control_addon_media_list_import(ersatzrs_app::commands::ControlAddonMediaListImport {
            id,
            action: action.into(),
        })
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
const MEDIA_LIST_CAPABILITIES: [&str; 5] = [
    ersatzrs_addon_contract::CAPABILITY_MEDIA_LIST_LIST_V1,
    ersatzrs_addon_contract::CAPABILITY_MEDIA_LIST_LIST_V2,
    ersatzrs_addon_contract::CAPABILITY_MEDIA_LIST_LIST_V3,
    ersatzrs_addon_contract::CAPABILITY_MEDIA_LIST_LIST_V4,
    ersatzrs_addon_contract::CAPABILITY_MEDIA_LIST_LIST_V5,
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
    detail_import_policy: AddonMediaListDetailPolicy,
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
            detail_import_policy,
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
            detail_import_policy: form.detail_import_policy,
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
