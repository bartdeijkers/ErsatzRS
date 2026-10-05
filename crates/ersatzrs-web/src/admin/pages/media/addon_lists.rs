//! Provider-neutral add-on media-list manager. ADR 0067 owns the deliberate
//! boundary that keeps provider-specific discovery and parsing in add-ons.

use std::collections::{BTreeMap, BTreeSet};

use ersatzrs_domain::{
    AddonMediaListAutoPublicationHoldReason, AddonMediaListContentKind, AddonMediaListDeliveryMode,
    AddonMediaListDetailPolicy, AddonMediaListDetailState, AddonMediaListImportMode,
    AddonMediaListImportPurpose, AddonMediaListImportState, AddonMediaListIncrementalStrategy,
    AddonMediaListItemAvailability, AddonMediaListItemKind, AddonMediaListItemRole,
    AddonMediaListPresentationKind, AddonMediaListPublicationEnqueueTrigger,
    AddonMediaListPublicationErrorCategory, AddonMediaListPublicationEventKind,
    AddonMediaListPublicationFilesystemTransition, AddonMediaListPublicationOutcome,
    AddonMediaListPublicationPhase, AddonMediaListPublicationRetryClass,
    AddonMediaListPublicationState, AddonMediaListRecoveryAction, AddonMediaListRefreshCadence,
    AddonMediaListRefreshPublicationPolicy, AddonMediaListReviewState, RemoteStreamLiveness,
};
pub mod workspace;

use leptos::prelude::*;
use leptos_router::hooks::{use_params_map, use_query_map};

use crate::admin::i18n::{
    addon_media_list_error_message, delete_failed, match_status, save_failed, tr, TextKey,
};
use crate::admin::server_fns::addon_media_lists::get_addon_media_list_show_nfo;
use crate::admin::server_fns::addon_media_lists::{
    add_addon_media_list, apply_addon_media_list_content_kind,
    apply_addon_media_list_item_review_edits, change_addon_media_list_addon,
    compare_addon_media_list_revisions, confirm_addon_media_list_removal,
    control_addon_media_list_import, delete_addon_media_list, get_addon_media_list_form,
    get_addon_media_list_item, get_addon_media_list_publication_history,
    get_addon_media_list_publication_readiness, get_addon_media_list_reset_impact,
    get_addon_media_list_revision_history, get_addon_media_list_saved_draft_diff,
    get_addon_media_list_source_probe, get_addon_media_lists_page_size,
    list_addon_media_list_items, list_addon_media_list_providers, list_addon_media_lists,
    match_addon_media_list, publish_addon_media_list, reconcile_addon_media_list,
    recover_addon_media_list_publication, redacted_publication_diagnostics_json,
    request_addon_media_list_item_details, request_addon_media_list_presentation,
    reset_addon_media_list, resolve_addon_media_list_provider, restore_addon_media_list_revision,
    save_addon_media_list_item_nfo, save_addon_media_list_show_nfo,
    search_addon_media_list_replacements, set_addon_media_list_item_replacement,
    set_addon_media_list_item_role, test_addon_media_list_source, update_addon_media_list,
    update_addon_media_list_item_chapters, validate_addon_media_list_item_chapters,
    AddonMediaListAdminSortDirection, AddonMediaListAdminSortField, AddonMediaListArtwork,
    AddonMediaListArtworkRole, AddonMediaListAutoPublicationHoldRow, AddonMediaListDraftDiffRow,
    AddonMediaListForm, AddonMediaListImportAction, AddonMediaListImportProgressRow,
    AddonMediaListItemDetail, AddonMediaListItemReviewEdit, AddonMediaListItemRow,
    AddonMediaListMetadataConflict, AddonMediaListNfoForm, AddonMediaListNfoMetadata,
    AddonMediaListNfoPerson, AddonMediaListProviderRow, AddonMediaListPublicationBlocker,
    AddonMediaListPublicationBlockerKind, AddonMediaListPublicationEventRow,
    AddonMediaListPublicationHistoryJobRow, AddonMediaListPublicationHistoryRow,
    AddonMediaListPublicationReadiness, AddonMediaListPublicationStatus,
    AddonMediaListPublishResult, AddonMediaListReconcileResult, AddonMediaListReplacementRow,
    AddonMediaListResetImpactRow, AddonMediaListResetResult, AddonMediaListRestoreResult,
    AddonMediaListRevisionDiffRow, AddonMediaListRevisionSummaryRow, AddonMediaListRow,
    AddonMediaListSourceProbeOutcome, AddonMediaListSourceProbeRow, AddonMediaListSourceProbeState,
};
use crate::admin::ui::primitives::{
    button::{Button, ButtonVariant},
    dialog::{DeleteDialog, Modal},
    icon::{icon_style, IconName},
    pagination::PaginationControls,
    resource_state::{admin_loading_resource_view, admin_result_view},
    toast::{load_or_notify, use_toast, ToastSeverity},
};

stylance::import_crate_style!(style, "src/admin/pages/media/addon_lists.module.css");

#[cfg(target_arch = "wasm32")]
fn set_browser_location(path: &str) {
    if let Some(window) = web_sys::window() {
        let _ = window.location().set_href(path);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn set_browser_location(_path: &str) {}

#[cfg(target_arch = "wasm32")]
fn focus_browser_element_after_render(id: &'static str) {
    use wasm_bindgen::JsCast;

    leptos::task::spawn_local(async move {
        gloo_timers::future::TimeoutFuture::new(0).await;
        if let Some(element) = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.get_element_by_id(id))
            .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
        {
            let _ = element.focus();
        }
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn focus_browser_element_after_render(_id: &'static str) {}

/// Return only a URL's host for destructive-action context. This intentionally
/// rejects malformed authorities rather than risking disclosure of user-info,
/// query, fragment, or path data.
fn redacted_source_host(source_url: &str) -> Option<String> {
    let (_, remainder) = source_url.trim().split_once("://")?;
    let authority = remainder
        .split(['/', '\\', '?', '#'])
        .next()?
        .rsplit('@')
        .next()?;
    let host = if let Some(ipv6) = authority.strip_prefix('[') {
        ipv6.split_once(']')?.0
    } else {
        authority.split(':').next()?
    };
    (!host.is_empty() && !host.chars().any(char::is_whitespace)).then(|| host.to_ascii_lowercase())
}

fn reset_confirmation_matches(confirmed_name: &str, list_name: &str) -> bool {
    confirmed_name.trim() == list_name
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DraftReviewCategory {
    Added,
    MetadataChanged,
    Excluded,
    Unavailable,
    Unknown,
    Conflicted,
    PendingRemoval,
    Reappeared,
    RemovalCandidate,
}

fn draft_review_category(value: &str) -> Option<DraftReviewCategory> {
    match value {
        "added" => Some(DraftReviewCategory::Added),
        "metadata_changed" => Some(DraftReviewCategory::MetadataChanged),
        "excluded" => Some(DraftReviewCategory::Excluded),
        "unavailable" => Some(DraftReviewCategory::Unavailable),
        "unknown" => Some(DraftReviewCategory::Unknown),
        "conflicted" => Some(DraftReviewCategory::Conflicted),
        "pending_removal" => Some(DraftReviewCategory::PendingRemoval),
        "reappeared" => Some(DraftReviewCategory::Reappeared),
        "removal_candidate" => Some(DraftReviewCategory::RemovalCandidate),
        _ => None,
    }
}

const fn draft_review_category_value(category: DraftReviewCategory) -> &'static str {
    match category {
        DraftReviewCategory::Added => "added",
        DraftReviewCategory::MetadataChanged => "metadata_changed",
        DraftReviewCategory::Excluded => "excluded",
        DraftReviewCategory::Unavailable => "unavailable",
        DraftReviewCategory::Unknown => "unknown",
        DraftReviewCategory::Conflicted => "conflicted",
        DraftReviewCategory::PendingRemoval => "pending_removal",
        DraftReviewCategory::Reappeared => "reappeared",
        DraftReviewCategory::RemovalCandidate => "removal_candidate",
    }
}

const fn draft_review_category_text_key(category: DraftReviewCategory) -> TextKey {
    match category {
        DraftReviewCategory::Added => TextKey::Added,
        DraftReviewCategory::MetadataChanged => TextKey::MetadataChanges,
        DraftReviewCategory::Excluded => TextKey::ExcludedItems,
        DraftReviewCategory::Unavailable => TextKey::Unavailable,
        DraftReviewCategory::Unknown => TextKey::UnknownAvailability,
        DraftReviewCategory::Conflicted => TextKey::Conflicts,
        DraftReviewCategory::PendingRemoval => TextKey::PendingRemovals,
        DraftReviewCategory::Reappeared => TextKey::Reappearances,
        DraftReviewCategory::RemovalCandidate => TextKey::RemovalCandidates,
    }
}

fn draft_review_item_ids(
    diff: &AddonMediaListDraftDiffRow,
    category: DraftReviewCategory,
) -> &[i64] {
    match category {
        DraftReviewCategory::Added => &diff.added_item_ids,
        DraftReviewCategory::MetadataChanged => &diff.metadata_changed_item_ids,
        DraftReviewCategory::Excluded => &diff.excluded_item_ids,
        DraftReviewCategory::Unavailable => &diff.unavailable_item_ids,
        DraftReviewCategory::Unknown => &diff.unknown_item_ids,
        DraftReviewCategory::Conflicted => &diff.conflicted_item_ids,
        DraftReviewCategory::PendingRemoval => &diff.pending_removal_item_ids,
        DraftReviewCategory::Reappeared => &diff.reappeared_item_ids,
        DraftReviewCategory::RemovalCandidate => &diff.removal_candidate_item_ids,
    }
}

fn item_matches_draft_category(
    item_id: i64,
    category: Option<DraftReviewCategory>,
    diff: &AddonMediaListDraftDiffRow,
) -> bool {
    category.is_none_or(|category| draft_review_item_ids(diff, category).contains(&item_id))
}

const fn revision_can_restore(restorable: bool, revision: i32, current_revision: i32) -> bool {
    restorable && revision < current_revision
}

fn addon_list_sort_field(value: &str) -> AddonMediaListAdminSortField {
    match value {
        "id" => AddonMediaListAdminSortField::Id,
        _ => AddonMediaListAdminSortField::Name,
    }
}

fn addon_list_sort_direction(value: &str) -> AddonMediaListAdminSortDirection {
    match value {
        "descending" => AddonMediaListAdminSortDirection::Descending,
        _ => AddonMediaListAdminSortDirection::Ascending,
    }
}

const fn addon_list_sort_direction_value(
    direction: AddonMediaListAdminSortDirection,
) -> &'static str {
    match direction {
        AddonMediaListAdminSortDirection::Ascending => "ascending",
        AddonMediaListAdminSortDirection::Descending => "descending",
    }
}

const fn addon_list_sort_aria(
    current_field: AddonMediaListAdminSortField,
    current_direction: AddonMediaListAdminSortDirection,
    column: AddonMediaListAdminSortField,
) -> &'static str {
    if !matches!(
        (current_field, column),
        (
            AddonMediaListAdminSortField::Name,
            AddonMediaListAdminSortField::Name
        ) | (
            AddonMediaListAdminSortField::Id,
            AddonMediaListAdminSortField::Id
        )
    ) {
        return "none";
    }
    addon_list_sort_direction_value(current_direction)
}

const fn next_addon_list_sort(
    current_field: AddonMediaListAdminSortField,
    current_direction: AddonMediaListAdminSortDirection,
    column: AddonMediaListAdminSortField,
) -> (
    AddonMediaListAdminSortField,
    AddonMediaListAdminSortDirection,
) {
    let same_column = matches!(
        (current_field, column),
        (
            AddonMediaListAdminSortField::Name,
            AddonMediaListAdminSortField::Name
        ) | (
            AddonMediaListAdminSortField::Id,
            AddonMediaListAdminSortField::Id
        )
    );
    let direction = if same_column {
        match current_direction {
            AddonMediaListAdminSortDirection::Ascending => {
                AddonMediaListAdminSortDirection::Descending
            }
            AddonMediaListAdminSortDirection::Descending => {
                AddonMediaListAdminSortDirection::Ascending
            }
        }
    } else {
        AddonMediaListAdminSortDirection::Ascending
    };
    (column, direction)
}

const fn addon_list_sort_indicator(
    current_field: AddonMediaListAdminSortField,
    current_direction: AddonMediaListAdminSortDirection,
    column: AddonMediaListAdminSortField,
) -> &'static str {
    let same_column = matches!(
        (current_field, column),
        (
            AddonMediaListAdminSortField::Name,
            AddonMediaListAdminSortField::Name
        ) | (
            AddonMediaListAdminSortField::Id,
            AddonMediaListAdminSortField::Id
        )
    );
    if !same_column {
        return "";
    }
    match current_direction {
        AddonMediaListAdminSortDirection::Ascending => "▲",
        AddonMediaListAdminSortDirection::Descending => "▼",
    }
}

#[component]
fn AddonMediaListResetConsequences(
    list_name: String,
    source_host: String,
    impact: RwSignal<AddonMediaListResetImpactRow>,
) -> impl IntoView {
    view! {
        <dl class=style::reset_identity>
            <div>
                <dt>{tr(TextKey::Name)}</dt>
                <dd>{list_name}</dd>
            </div>
            <div>
                <dt>{tr(TextKey::SourceHost)}</dt>
                <dd>{source_host}</dd>
            </div>
        </dl>
        <dl class=style::operator_metrics>
            <div><dt>{tr(TextKey::PublishedItems)}</dt><dd>{move || impact.with(|value| value.published_items)}</dd></div>
            <div><dt>{tr(TextKey::GeneratedPlaylists)}</dt><dd>{move || impact.with(|value| value.generated_playlists)}</dd></div>
            <div><dt>{tr(TextKey::Channels)}</dt><dd>{move || impact.with(|value| value.channels)}</dd></div>
            <div><dt>{tr(TextKey::Schedules)}</dt><dd>{move || impact.with(|value| value.schedules)}</dd></div>
            <div><dt>{tr(TextKey::ActiveOrFuturePlayouts)}</dt><dd>{move || impact.with(|value| value.active_or_future_playouts)}</dd></div>
        </dl>
        <div class=style::reset_consequences>
            <div>
                <strong>{tr(TextKey::ResetRemoves)}</strong>
                <ul>
                    <li>{tr(TextKey::ResetRemovesItems)}</li>
                    <li>{tr(TextKey::ResetRemovesArtifacts)}</li>
                    <li>{tr(TextKey::ResetRemovesRuntime)}</li>
                </ul>
            </div>
            <div>
                <strong>{tr(TextKey::ResetRetains)}</strong>
                <p>{tr(TextKey::ResetRetainsSettings)}</p>
            </div>
        </div>
        <p class=style::reset_warning role="note">
            <span style=icon_style(IconName::Warning, "1rem") aria-hidden="true"></span>
            <span>{tr(TextKey::ResetReloadFailureWarning)}</span>
        </p>
    }
}

#[cfg(target_arch = "wasm32")]
async fn upload_metadata_artwork(
    input: web_sys::HtmlInputElement,
    list_id: i64,
    item_id: Option<i64>,
    role: AddonMediaListArtworkRole,
    etag: &str,
) -> Result<(String, String), String> {
    use gloo_net::http::Request;
    use web_sys::FormData;

    let file = input
        .files()
        .and_then(|files| files.get(0))
        .ok_or_else(|| "no file was selected".to_owned())?;
    let form_data = FormData::new().map_err(|error| format!("{error:?}"))?;
    form_data
        .append_with_str("list_id", &list_id.to_string())
        .map_err(|error| format!("{error:?}"))?;
    if let Some(item_id) = item_id {
        form_data
            .append_with_str("item_id", &item_id.to_string())
            .map_err(|error| format!("{error:?}"))?;
    }
    form_data
        .append_with_str(
            "role",
            match role {
                AddonMediaListArtworkRole::Poster => "poster",
                AddonMediaListArtworkRole::Fanart => "fanart",
                AddonMediaListArtworkRole::Thumb => "thumb",
                AddonMediaListArtworkRole::Other => "other",
            },
        )
        .map_err(|error| format!("{error:?}"))?;
    form_data
        .append_with_str("etag", etag)
        .map_err(|error| format!("{error:?}"))?;
    let blob: &web_sys::Blob = file.as_ref();
    form_data
        .append_with_blob_and_filename("file", blob, &file.name())
        .map_err(|error| format!("{error:?}"))?;
    let response = Request::post("/admin-api/addon-media-lists/artwork")
        .body(form_data)
        .map_err(|error| error.to_string())?
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if !response.ok() {
        return Err(format!("HTTP {}", response.status()));
    }
    let value: serde_json::Value = response.json().await.map_err(|error| error.to_string())?;
    let url = value["url"]
        .as_str()
        .ok_or_else(|| "upload response has no artwork URL".to_owned())?
        .to_owned();
    let etag = value["etag"]
        .as_str()
        .ok_or_else(|| "upload response has no metadata ETag".to_owned())?
        .to_owned();
    Ok((url, etag))
}

#[component]
fn ArtworkRoleInput(
    list_id: i64,
    item_id: Option<i64>,
    role: AddonMediaListArtworkRole,
    label: TextKey,
    url: RwSignal<String>,
    etag: RwSignal<String>,
    candidates: Vec<String>,
) -> impl IntoView {
    let uploading = RwSignal::new(false);
    let toast = use_toast();
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (etag, toast);
    let role_name = match role {
        AddonMediaListArtworkRole::Poster => "poster",
        AddonMediaListArtworkRole::Fanart => "fanart",
        AddonMediaListArtworkRole::Thumb => "thumb",
        AddonMediaListArtworkRole::Other => "other",
    };
    let list_name = format!(
        "addon-artwork-{}-{}-{}",
        list_id,
        item_id.unwrap_or(0),
        role_name,
    );
    let native_field_name = match role {
        AddonMediaListArtworkRole::Poster => Some("metadata_poster"),
        AddonMediaListArtworkRole::Fanart => Some("metadata_fanart"),
        AddonMediaListArtworkRole::Thumb => Some("metadata_thumbnail"),
        AddonMediaListArtworkRole::Other => None,
    };
    let initial_url = url.get_untracked();
    view! {
        <div class=style::artwork_role_field>
            <span>{tr(label)}</span>
            <input
                class=style::boxed_input
                type="text"
                name=native_field_name
                inputmode="url"
                aria-label=tr(label)
                data-artwork-role=role_name
                list=list_name.clone()
                value=initial_url
                prop:value=move || url.get()
                on:input=move |event| url.set(event_target_value(&event))
            />
            <datalist id=list_name>
                {candidates.into_iter().map(|candidate| view! { <option value=candidate/> }).collect_view()}
            </datalist>
            <span class=style::artwork_upload_control>
                <span style=icon_style(IconName::Upload, "0.95rem") aria-hidden="true"></span>
                <span>{tr(TextKey::UploadImage)}</span>
                <input
                    type="file"
                    accept="image/jpeg,image/png,image/webp"
                    aria-label=format!("{}: {}", tr(TextKey::UploadImage), tr(label))
                    data-artwork-role=role_name
                    disabled=move || uploading.get()
                    on:change=move |event| {
                        #[cfg(target_arch = "wasm32")]
                        {
                            use leptos::task::spawn_local;
                            use wasm_bindgen::JsCast as _;
                            let Some(target) = event.target() else { return; };
                            let Ok(input) = target.dyn_into::<web_sys::HtmlInputElement>() else { return; };
                            uploading.set(true);
                            spawn_local(async move {
                                match upload_metadata_artwork(
                                    input,
                                    list_id,
                                    item_id,
                                    role,
                                    &etag.get_untracked(),
                                )
                                .await
                                {
                                    Ok((uploaded_url, uploaded_etag)) => {
                                        url.set(uploaded_url);
                                        etag.set(uploaded_etag);
                                        toast.push(tr(TextKey::Saved).to_string(), ToastSeverity::Success);
                                    }
                                    Err(error) => toast.push(
                                        format!("{}: {error}", tr(TextKey::UploadFailed)),
                                        ToastSeverity::Error,
                                    ),
                                }
                                uploading.set(false);
                            });
                        }
                        let _ = event;
                    }
                />
            </span>
        </div>
    }
}

fn content_kind_from_value(value: &str) -> AddonMediaListContentKind {
    match value {
        "television_episode" => AddonMediaListContentKind::TelevisionEpisode,
        "movie" => AddonMediaListContentKind::Movie,
        "music_video" => AddonMediaListContentKind::MusicVideo,
        "other_video" => AddonMediaListContentKind::OtherVideo,
        "song" => AddonMediaListContentKind::Song,
        "image" => AddonMediaListContentKind::Image,
        _ => AddonMediaListContentKind::Auto,
    }
}

const fn content_kind_value(value: AddonMediaListContentKind) -> &'static str {
    match value {
        AddonMediaListContentKind::Auto => "auto",
        AddonMediaListContentKind::TelevisionEpisode => "television_episode",
        AddonMediaListContentKind::Movie => "movie",
        AddonMediaListContentKind::MusicVideo => "music_video",
        AddonMediaListContentKind::OtherVideo => "other_video",
        AddonMediaListContentKind::Song => "song",
        AddonMediaListContentKind::Image => "image",
    }
}

fn presentation_kind_from_value(value: &str) -> AddonMediaListPresentationKind {
    match value {
        "remote_streams" => AddonMediaListPresentationKind::RemoteStreams,
        "television_show" => AddonMediaListPresentationKind::TelevisionShow,
        "movies" => AddonMediaListPresentationKind::Movies,
        "music_videos" => AddonMediaListPresentationKind::MusicVideos,
        "other_videos" => AddonMediaListPresentationKind::OtherVideos,
        "songs" => AddonMediaListPresentationKind::Songs,
        "images" => AddonMediaListPresentationKind::Images,
        _ => AddonMediaListPresentationKind::Auto,
    }
}

const fn presentation_kind_value(value: AddonMediaListPresentationKind) -> &'static str {
    match value {
        AddonMediaListPresentationKind::Auto => "auto",
        AddonMediaListPresentationKind::RemoteStreams => "remote_streams",
        AddonMediaListPresentationKind::TelevisionShow => "television_show",
        AddonMediaListPresentationKind::Movies => "movies",
        AddonMediaListPresentationKind::MusicVideos => "music_videos",
        AddonMediaListPresentationKind::OtherVideos => "other_videos",
        AddonMediaListPresentationKind::Songs => "songs",
        AddonMediaListPresentationKind::Images => "images",
    }
}

/// Rows-per-page choices for the provider-ordered item overview; zero renders
/// every currently filtered row.
const ITEM_PAGE_SIZES: [u32; 5] = [10, 25, 50, 100, 0];
const DEFAULT_ITEM_PAGE_SIZE: u32 = 25;

/// Setting or clearing the local replacement of one entry.
type ReplacementAction = Action<(i64, Option<(i64, i64)>), Result<(), ServerFnError>>;
type ItemRoleAction = Action<(i64, i64, AddonMediaListItemRole), Result<(), ServerFnError>>;
type DetailRequestAction = Action<(i64, i64), Result<(), ServerFnError>>;
type PublicationRecoveryAction = Action<
    (i64, i64, i32, AddonMediaListRecoveryAction),
    Result<AddonMediaListPublicationStatus, ServerFnError>,
>;
/// Searching local media that could replace one entry.
type ReplacementSearchAction =
    Action<String, Result<Vec<AddonMediaListReplacementRow>, ServerFnError>>;

#[derive(Clone, Debug)]
struct ReviewUndoEntry {
    label: String,
    snapshots: Vec<AddonMediaListItemReviewEdit>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowSaveState {
    Saving,
    Saved,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EpisodeNumberPair {
    Unnumbered,
    Numbered(i32, i32),
    Invalid,
}

fn episode_number_pair(season: Option<i32>, episode: Option<i32>) -> EpisodeNumberPair {
    match (season, episode) {
        (None, None) | (Some(0), Some(0)) => EpisodeNumberPair::Unnumbered,
        (Some(season), Some(episode)) if season >= 0 && episode >= 1 => {
            EpisodeNumberPair::Numbered(season, episode)
        }
        _ => EpisodeNumberPair::Invalid,
    }
}

fn parse_episode_number_pair(season: &str, episode: &str) -> EpisodeNumberPair {
    let season = season.trim();
    let episode = episode.trim();
    if season.is_empty() && episode.is_empty() {
        return EpisodeNumberPair::Unnumbered;
    }
    match (season.parse::<i32>(), episode.parse::<i32>()) {
        (Ok(0), Ok(0)) => EpisodeNumberPair::Unnumbered,
        (Ok(season), Ok(episode)) if season >= 0 && episode >= 1 => {
            EpisodeNumberPair::Numbered(season, episode)
        }
        _ => EpisodeNumberPair::Invalid,
    }
}

type ReviewEditAction = Action<
    (i64, Vec<AddonMediaListItemReviewEdit>, ReviewUndoEntry),
    Result<ReviewUndoEntry, ServerFnError>,
>;

fn review_snapshot(item: &AddonMediaListItemRow) -> AddonMediaListItemReviewEdit {
    AddonMediaListItemReviewEdit {
        item_id: item.id,
        content_kind: item.content_kind,
        included: item.included,
        liveness_override: item.liveness_override,
        season: item.season,
        episode: item.episode,
    }
}

fn dispatch_review_edit(
    action: ReviewEditAction,
    row_save_states: RwSignal<BTreeMap<i64, RowSaveState>>,
    readiness_refresh_pending: RwSignal<bool>,
    list_id: i64,
    previous: AddonMediaListItemReviewEdit,
    desired: AddonMediaListItemReviewEdit,
    label: TextKey,
) {
    if action.pending().get_untracked() {
        return;
    }
    row_save_states.update(|states| {
        states.insert(desired.item_id, RowSaveState::Saving);
    });
    readiness_refresh_pending.set(true);
    action.dispatch((
        list_id,
        vec![desired],
        ReviewUndoEntry {
            label: tr(label).to_string(),
            snapshots: vec![previous],
        },
    ));
}

const fn liveness_text_key(liveness: RemoteStreamLiveness) -> TextKey {
    match liveness {
        RemoteStreamLiveness::Unknown => TextKey::UnknownAvailability,
        RemoteStreamLiveness::Finite => TextKey::FiniteMedia,
        RemoteStreamLiveness::Live => TextKey::LiveSource,
    }
}

const fn detail_state_text_key(state: AddonMediaListDetailState) -> TextKey {
    match state {
        AddonMediaListDetailState::NotRequested => TextKey::FetchDetails,
        AddonMediaListDetailState::Queued => TextKey::DetailsQueued,
        AddonMediaListDetailState::WaitingForSlot => TextKey::WaitingForProvider,
        AddonMediaListDetailState::Running => TextKey::DetailsLoading,
        AddonMediaListDetailState::WaitingToRetry | AddonMediaListDetailState::Failed => {
            TextKey::RetryDetails
        }
        AddonMediaListDetailState::Complete => TextKey::DetailsComplete,
        AddonMediaListDetailState::Skipped => TextKey::DetailsSkipped,
    }
}

const fn import_state_text_key(state: AddonMediaListImportState) -> TextKey {
    match state {
        AddonMediaListImportState::Queued | AddonMediaListImportState::Discovering => {
            TextKey::FindingProgrammes
        }
        AddonMediaListImportState::Verifying => TextKey::CheckingOverview,
        AddonMediaListImportState::Ready => TextKey::ReadyToReview,
        AddonMediaListImportState::Paused => TextKey::PauseImport,
        AddonMediaListImportState::NeedsAttention => TextKey::NeedReview,
        AddonMediaListImportState::Cancelled => TextKey::CancelImport,
        AddonMediaListImportState::Superseded => TextKey::Import,
    }
}

fn import_progress_phase(
    progress: &AddonMediaListImportProgressRow,
    published: bool,
    policy: AddonMediaListDetailPolicy,
) -> TextKey {
    if progress.state == AddonMediaListImportState::Ready {
        if progress.discovered_count == 0
            && progress.purpose != AddonMediaListImportPurpose::Reconcile
            && (progress.mode == AddonMediaListImportMode::Full
                || progress.retained_item_count == 0)
        {
            return TextKey::EmptyImportNeedsRetry;
        }
        if progress.detail_pending > 0 {
            return if progress.current_title.is_some() {
                TextKey::LoadingItemDetails
            } else {
                TextKey::WaitingForProvider
            };
        }
        if published && policy == AddonMediaListDetailPolicy::ManualOnly {
            return TextKey::OverviewOnly;
        }
    }
    import_state_text_key(progress.state)
}

fn import_can_retry(progress: &AddonMediaListImportProgressRow) -> bool {
    matches!(
        progress.state,
        AddonMediaListImportState::NeedsAttention | AddonMediaListImportState::Cancelled
    ) || (progress.state == AddonMediaListImportState::Ready
        && progress.discovered_count == 0
        && progress.purpose != AddonMediaListImportPurpose::Reconcile
        && (progress.mode == AddonMediaListImportMode::Full || progress.retained_item_count == 0))
}

fn import_discovery_summary(progress: &AddonMediaListImportProgressRow) -> String {
    if progress.strategy == AddonMediaListIncrementalStrategy::None {
        return format!("{} {}", progress.discovered_count, tr(TextKey::Discovered));
    }
    format!(
        "{} {} · {} {} · {} {}",
        progress.examined_count,
        tr(TextKey::Examined),
        progress.archived_skipped_count,
        tr(TextKey::ArchivedSkipped),
        progress.emitted_count,
        tr(TextKey::Emitted),
    )
}

fn format_clock_duration(seconds: u64) -> String {
    let hours = seconds / 3_600;
    let minutes = seconds % 3_600 / 60;
    let seconds = seconds % 60;
    if hours > 0 {
        format!("{hours:02}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

fn elapsed_between(started_at: &str, ended_at: Option<&str>) -> Option<String> {
    let started = chrono::DateTime::parse_from_rfc3339(started_at).ok()?;
    let ended = ended_at.map_or_else(
        || Some(chrono::Utc::now()),
        |value| {
            chrono::DateTime::parse_from_rfc3339(value)
                .ok()
                .map(|value| value.with_timezone(&chrono::Utc))
        },
    )?;
    let seconds = u64::try_from(
        (ended - started.with_timezone(&chrono::Utc))
            .num_seconds()
            .max(0),
    )
    .ok()?;
    Some(format_clock_duration(seconds))
}

#[cfg(test)]
fn elapsed_from(started_at: &str) -> Option<String> {
    elapsed_between(started_at, None)
}

fn format_progress_timestamp(value: &str) -> Option<String> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|timestamp| timestamp.format("%Y-%m-%d %H:%M:%S UTC").to_string())
}

fn progress_sample(progress: &AddonMediaListImportProgressRow) -> (bool, u64, Option<u64>) {
    let detail_done = progress
        .detail_complete
        .saturating_add(progress.detail_skipped)
        .saturating_add(progress.detail_failed);
    if progress.detail_pending > 0 || detail_done > 0 {
        (true, detail_done, Some(progress.detail_total))
    } else if progress.strategy != AddonMediaListIncrementalStrategy::None {
        (false, progress.examined_count, progress.total_hint)
    } else {
        (false, progress.discovered_count, progress.total_hint)
    }
}

#[cfg(target_arch = "wasm32")]
fn browser_now_millis() -> f64 {
    js_sys::Date::now()
}

#[cfg(not(target_arch = "wasm32"))]
fn browser_now_millis() -> f64 {
    0.0
}

const fn liveness_override_value(liveness: Option<RemoteStreamLiveness>) -> &'static str {
    match liveness {
        None | Some(RemoteStreamLiveness::Unknown) => "provider",
        Some(RemoteStreamLiveness::Finite) => "finite",
        Some(RemoteStreamLiveness::Live) => "live",
    }
}

fn liveness_override_from_value(value: &str) -> Option<RemoteStreamLiveness> {
    match value {
        "finite" => Some(RemoteStreamLiveness::Finite),
        "live" => Some(RemoteStreamLiveness::Live),
        _ => None,
    }
}

/// One item row, identical in all three sections.
///
/// Local replacement is offered everywhere: a playable entry can still be worth
/// replacing with a local copy.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn item_row_view(
    list_id: i64,
    item: AddonMediaListItemRow,
    replacement_target: RwSignal<Option<i64>>,
    replacement_query: RwSignal<String>,
    search_action: ReplacementSearchAction,
    replacement_action: ReplacementAction,
    review_edit_action: ReviewEditAction,
    row_save_states: RwSignal<BTreeMap<i64, RowSaveState>>,
    readiness_refresh_pending: RwSignal<bool>,
    role_action: ItemRoleAction,
    episode_preview: Memo<BTreeMap<i64, (i32, i32)>>,
    episode_numbering_open: RwSignal<bool>,
    episode_numbering_start_item_id: RwSignal<Option<i64>>,
    presentation: RwSignal<AddonMediaListPresentationKind>,
    request_detail_action: DetailRequestAction,
    detail_request_item: RwSignal<Option<i64>>,
) -> AnyView {
    let item_id = item.id;
    let title = item.display_title.clone();
    let search_title = title.clone();
    let has_replacement = item.replacement_media_version_id.is_some();
    let replacement_state = if !has_replacement {
        "none"
    } else if item.replacement_valid {
        "valid"
    } else {
        "invalid"
    };
    let replacement = if replacement_state == "invalid" {
        tr(TextKey::InvalidReplacement).to_string()
    } else {
        item.replacement_title
            .clone()
            .unwrap_or_else(|| tr(TextKey::NoReplacement).to_string())
    };
    let content_kind = item.content_kind;
    let item_is_episode = content_kind == AddonMediaListContentKind::TelevisionEpisode;
    let review_state = review_snapshot(&item);
    let collection_role = item.collection_role;
    let included = item.included;
    let stored_episode_number = episode_number_pair(item.season, item.episode);
    let current_episode_number = match stored_episode_number {
        EpisodeNumberPair::Numbered(season, episode) => {
            format!("S{season:02}E{episode:02}")
        }
        EpisodeNumberPair::Unnumbered | EpisodeNumberPair::Invalid => String::new(),
    };
    let first_aired = item
        .first_aired
        .clone()
        .unwrap_or_else(|| tr(TextKey::UnknownAvailability).to_string());
    let row_number = item.rank.saturating_add(1).to_string();
    let row_aria_label = format!("{} {row_number}", tr(TextKey::Number));
    let is_remote_stream = item.kind == AddonMediaListItemKind::RemoteStream;
    let provider_liveness = item.provider_liveness;
    let liveness_override = item.liveness_override;
    let effective_liveness = item.effective_liveness;
    let liveness_conflict = item.liveness_conflict;
    let thumbnail_url = item.thumbnail_url.clone().unwrap_or_default();
    let has_thumbnail = !thumbnail_url.is_empty();
    let tags_summary = item
        .tags
        .iter()
        .take(3)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    let people_summary = item
        .people
        .iter()
        .take(2)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    let has_metadata_summary = !tags_summary.is_empty() || !people_summary.is_empty();
    let detail_state = item.detail_state;
    let detail_diagnostic = item.detail_diagnostic.clone().unwrap_or_default();
    let detail_pending = matches!(
        detail_state,
        AddonMediaListDetailState::Queued
            | AddonMediaListDetailState::WaitingForSlot
            | AddonMediaListDetailState::Running
    );
    let detail_terminal = matches!(
        detail_state,
        AddonMediaListDetailState::Complete | AddonMediaListDetailState::Skipped
    );
    let detail_action_label = format!("{}: {title}", tr(detail_state_text_key(detail_state)));
    let can_request_details =
        item.parent_provider_id.is_none() && !detail_pending && !detail_terminal;
    let base_needs_review = item.availability != AddonMediaListItemAvailability::Available
        || (item.replacement_media_version_id.is_some() && !item.replacement_valid)
        || item.title.trim().is_empty()
        || (is_remote_stream
            && (effective_liveness == RemoteStreamLiveness::Unknown || liveness_conflict));
    let is_episode_context = item_is_episode
        || presentation.get_untracked() == AddonMediaListPresentationKind::TelevisionShow;
    let season_episode_unnumbered =
        is_episode_context && stored_episode_number == EpisodeNumberPair::Unnumbered;
    let season_episode_invalid =
        is_episode_context && stored_episode_number == EpisodeNumberPair::Invalid;
    let season_episode_needs_attention = season_episode_unnumbered || season_episode_invalid;
    let season_episode_warning_id = format!("addon-list-item-numbering-warning-{item_id}");
    let availability_reason = item.availability_reason.unwrap_or_default();
    let has_reason = !availability_reason.is_empty();
    let season_input = RwSignal::new(
        item.season
            .map_or_else(String::new, |value| value.to_string()),
    );
    let episode_input = RwSignal::new(
        item.episode
            .map_or_else(String::new, |value| value.to_string()),
    );
    let episode_input_pair =
        Memo::new(move |_| parse_episode_number_pair(&season_input.get(), &episode_input.get()));
    let review_href = format!("/media/addon-lists/{list_id}/items/{item_id}");
    let title_href = review_href.clone();
    view! {
        <li
            class=move || if episode_preview.with(|preview| preview.contains_key(&item_id)) {
                format!("{} {}", style::metadata_item, style::numbering_preview_row)
            } else {
                style::metadata_item.to_owned()
            }
            data-item-id=item_id
        >
            <article aria-labelledby=format!("addon-list-item-title-{item_id}")>
                <header class=style::item_header>
                    <span class=style::item_order aria-label=row_aria_label>{row_number}</span>
                    <span class=style::item_thumbnail>
                        <img src=thumbnail_url alt="" style:display=if has_thumbnail { "block" } else { "none" }/>
                        <span style:display=if has_thumbnail { "none" } else { "inline-flex" }>
                            <span style=icon_style(IconName::Image, "1.15rem") aria-hidden="true"></span>
                        </span>
                    </span>
                    <div class=style::item_identity>
                        <a
                            id=format!("addon-list-item-title-{item_id}")
                            class=style::row_title
                            href=title_href
                        >{title}</a>
                        <small class=style::row_note style:display=if has_reason { "block" } else { "none" }>{availability_reason}</small>
                        <small class=style::row_note style:display=if has_metadata_summary { "block" } else { "none" }>
                            {if !people_summary.is_empty() && !tags_summary.is_empty() {
                                format!("{people_summary} · {tags_summary}")
                            } else if !people_summary.is_empty() {
                                people_summary
                            } else {
                                tags_summary
                            }}
                        </small>
                        <span
                            class=style::row_save_state
                            role="status"
                            aria-live="polite"
                            data-state=move || row_save_states.with(|states| match states.get(&item_id) {
                                Some(RowSaveState::Saving) => "saving",
                                Some(RowSaveState::Saved) => "saved",
                                Some(RowSaveState::Error) => "error",
                                None => "",
                            })
                            style:display=move || if row_save_states.with(|states| states.contains_key(&item_id)) { "inline-flex" } else { "none" }
                        >{move || row_save_states.with(|states| match states.get(&item_id) {
                            Some(RowSaveState::Saving) => tr(TextKey::Saving),
                            Some(RowSaveState::Saved) => tr(TextKey::Saved),
                            Some(RowSaveState::Error) => tr(TextKey::Error),
                            None => "",
                        })}</span>
                    </div>
                    <label class=style::item_include>
                        <input
                            type="checkbox"
                            checked=included
                            disabled=move || review_edit_action.pending().get() || role_action.pending().get()
                            on:change=move |event| {
                                let mut desired = review_state;
                                desired.included = event_target_checked(&event);
                                dispatch_review_edit(
                                    review_edit_action,
                                    row_save_states,
                                    readiness_refresh_pending,
                                    list_id,
                                    review_state,
                                    desired,
                                    TextKey::IncludeInPublication,
                                );
                            }
                        />
                        <span>{tr(TextKey::IncludeInPublication)}</span>
                    </label>
                </header>

                <dl class=style::item_fields>
                    <div class=style::item_field>
                        <dt>{tr(TextKey::FirstAired)}</dt>
                        <dd>{first_aired}</dd>
                    </div>
                    <div class=style::item_field>
                        <dt>{tr(TextKey::SeasonEpisode)}</dt>
                        <dd>
                            {move || if item_is_episode
                                || presentation.get() == AddonMediaListPresentationKind::TelevisionShow
                            {
                                let preview_current_episode_number = current_episode_number.clone();
                                view! {
                                    <div class=style::episode_quick_edit>
                                        <input
                                            type="number"
                                            min="0"
                                            step="1"
                                            inputmode="numeric"
                                            aria-label=tr(TextKey::Season)
                                            aria-invalid=move || if episode_input_pair.get() == EpisodeNumberPair::Invalid { "true" } else { "false" }
                                            aria-describedby=season_episode_warning_id.clone()
                                            placeholder="S"
                                            prop:value=move || season_input.get()
                                            on:input=move |event| season_input.set(event_target_value(&event))
                                        />
                                        <input
                                            type="number"
                                            min="1"
                                            step="1"
                                            inputmode="numeric"
                                            aria-label=tr(TextKey::Episode)
                                            aria-invalid=move || if episode_input_pair.get() == EpisodeNumberPair::Invalid { "true" } else { "false" }
                                            aria-describedby=season_episode_warning_id.clone()
                                            placeholder="E"
                                            prop:value=move || episode_input.get()
                                            on:input=move |event| episode_input.set(event_target_value(&event))
                                        />
                                        <button
                                            type="button"
                                            class=style::episode_apply
                                            aria-label=tr(TextKey::SaveSeasonEpisode)
                                            title=tr(TextKey::SaveSeasonEpisode)
                                            disabled=move || {
                                                review_edit_action.pending().get()
                                                    || role_action.pending().get()
                                                    || episode_input_pair.get() == EpisodeNumberPair::Invalid
                                            }
                                            on:click=move |_| {
                                                let pair = parse_episode_number_pair(
                                                    &season_input.get_untracked(),
                                                    &episode_input.get_untracked(),
                                                );
                                                let (season, episode) = match pair {
                                                    EpisodeNumberPair::Unnumbered => (None, None),
                                                    EpisodeNumberPair::Numbered(season, episode) => {
                                                        (Some(season), Some(episode))
                                                    }
                                                    EpisodeNumberPair::Invalid => return,
                                                };
                                                let mut desired = review_state;
                                                desired.season = season;
                                                desired.episode = episode;
                                                dispatch_review_edit(
                                                    review_edit_action,
                                                    row_save_states,
                                                    readiness_refresh_pending,
                                                    list_id,
                                                    review_state,
                                                    desired,
                                                    TextKey::SeasonEpisode,
                                                );
                                            }
                                        >"✓"</button>
                                    </div>
                                    <small
                                        id=season_episode_warning_id.clone()
                                        class=move || if episode_input_pair.get() == EpisodeNumberPair::Invalid {
                                            style::item_field_warning
                                        } else {
                                            style::item_field_recommendation
                                        }
                                        style:display=move || if matches!(episode_input_pair.get(), EpisodeNumberPair::Numbered(_, _)) { "none" } else { "block" }
                                    >{move || match episode_input_pair.get() {
                                        EpisodeNumberPair::Invalid => tr(TextKey::BlockerSeasonEpisodeMissing),
                                        EpisodeNumberPair::Unnumbered => tr(TextKey::Unnumbered),
                                        EpisodeNumberPair::Numbered(_, _) => "",
                                    }}</small>
                                    <small class=style::episode_preview>
                                        {move || episode_preview.with(|preview| {
                                            preview.get(&item_id).map_or_else(String::new, |(season, episode)| {
                                                let next = format!("S{season:02}E{episode:02}");
                                                if preview_current_episode_number.is_empty() || preview_current_episode_number == next {
                                                    next
                                                } else {
                                                    format!("{preview_current_episode_number} → {next}")
                                                }
                                            })
                                        })}
                                    </small>
                                    <button
                                        type="button"
                                        class=style::start_here
                                        style:display=move || if episode_numbering_open.get() { "inline-flex" } else { "none" }
                                        aria-pressed=move || if episode_numbering_start_item_id.get() == Some(item_id) { "true" } else { "false" }
                                        on:click=move |_| episode_numbering_start_item_id.set(Some(item_id))
                                    >{tr(TextKey::StartHere)}</button>
                                }.into_any()
                            } else {
                                let display_episode_number = current_episode_number.clone();
                                view! { <span>{if display_episode_number.is_empty() { "—".to_owned() } else { display_episode_number }}</span> }.into_any()
                            }}
                        </dd>
                    </div>
                    <div class=style::item_field>
                        <dt>{tr(TextKey::MediaKind)}</dt>
                        <dd>
                            {move || if presentation.get() == AddonMediaListPresentationKind::Movies {
                                view! {
                                    <select
                                        class=style::row_select
                                        aria-label=tr(TextKey::PrimaryFeature)
                                        disabled=move || role_action.pending().get() || review_edit_action.pending().get()
                                        prop:value=match collection_role {
                                            AddonMediaListItemRole::Auto => "auto",
                                            AddonMediaListItemRole::Primary => "primary",
                                            AddonMediaListItemRole::Extra => "extra",
                                        }
                                        on:change=move |event| {
                                            if role_action.pending().get_untracked() || review_edit_action.pending().get_untracked() {
                                                return;
                                            }
                                            let role = match event_target_value(&event).as_str() {
                                                "primary" => AddonMediaListItemRole::Primary,
                                                "extra" => AddonMediaListItemRole::Extra,
                                                _ => AddonMediaListItemRole::Auto,
                                            };
                                            row_save_states.update(|states| {
                                                states.insert(item_id, RowSaveState::Saving);
                                            });
                                            role_action.dispatch((list_id, item_id, role));
                                        }
                                    >
                                        <option value="auto">{tr(TextKey::ReviewRecommended)}</option>
                                        <option value="primary">{tr(TextKey::PrimaryFeature)}</option>
                                        <option value="extra">{tr(TextKey::MovieExtra)}</option>
                                    </select>
                                }.into_any()
                            } else {
                                view! {
                                    <select
                                        class=style::row_select
                                        aria-label=tr(TextKey::MediaKind)
                                        disabled=move || review_edit_action.pending().get() || role_action.pending().get()
                                        prop:value=content_kind_value(content_kind)
                                        on:change=move |event| {
                                            let mut desired = review_state;
                                            desired.content_kind = content_kind_from_value(&event_target_value(&event));
                                            dispatch_review_edit(
                                                review_edit_action,
                                                row_save_states,
                                                readiness_refresh_pending,
                                                list_id,
                                                review_state,
                                                desired,
                                                TextKey::MediaKind,
                                            );
                                        }
                                    >
                                        <option value="auto">{tr(TextKey::LeaveEmptyToAutoDetect)}</option>
                                        <option value="television_episode">{tr(TextKey::TelevisionEpisode)}</option>
                                        <option value="movie">{tr(TextKey::Movie)}</option>
                                        <option value="music_video">{tr(TextKey::MusicVideo)}</option>
                                        <option value="other_video">{tr(TextKey::OtherVideo)}</option>
                                        <option value="song">{tr(TextKey::Song)}</option>
                                        <option value="image">{tr(TextKey::Image)}</option>
                                    </select>
                                }.into_any()
                            }}
                        </dd>
                    </div>
                    <div class=style::item_field>
                        <dt>{tr(TextKey::PlaybackBehavior)}</dt>
                        <dd>
                            <select
                                class=style::row_select
                                aria-label=tr(TextKey::PlaybackBehavior)
                                disabled=move || review_edit_action.pending().get() || role_action.pending().get()
                                style:display=if is_remote_stream { "block" } else { "none" }
                                prop:value=liveness_override_value(liveness_override)
                                on:change=move |event| {
                                    let mut desired = review_state;
                                    desired.liveness_override = liveness_override_from_value(&event_target_value(&event));
                                    dispatch_review_edit(
                                        review_edit_action,
                                        row_save_states,
                                        readiness_refresh_pending,
                                        list_id,
                                        review_state,
                                        desired,
                                        TextKey::PlaybackBehavior,
                                    );
                                }
                            >
                                <option value="provider">
                                    {format!("{} · {}", tr(TextKey::UseProviderValue), tr(liveness_text_key(provider_liveness)))}
                                </option>
                                <option value="finite">{tr(TextKey::FiniteMedia)}</option>
                                <option value="live">{tr(TextKey::LiveSource)}</option>
                            </select>
                            <small class=style::liveness_conflict style:display=if is_remote_stream && liveness_conflict { "block" } else { "none" }>{tr(TextKey::ProviderValueChanged)}</small>
                            <span style:display=if is_remote_stream { "none" } else { "inline" }>"—"</span>
                        </dd>
                    </div>
                    <div class=style::item_field data-replacement-state=replacement_state title=item.replacement_path.unwrap_or_default()>
                        <dt>{tr(TextKey::Status)}</dt>
                        <dd>
                            <span class=move || if ((item_is_episode
                                || presentation.get() == AddonMediaListPresentationKind::TelevisionShow)
                                && season_episode_needs_attention)
                                || base_needs_review
                                || (presentation.get() == AddonMediaListPresentationKind::Movies
                                    && collection_role == AddonMediaListItemRole::Auto)
                            {
                                style::review_status
                            } else {
                                style::ready_status
                            }>
                                <span style=move || icon_style(
                                    if ((item_is_episode
                                        || presentation.get() == AddonMediaListPresentationKind::TelevisionShow)
                                        && season_episode_needs_attention)
                                        || base_needs_review
                                        || (presentation.get() == AddonMediaListPresentationKind::Movies
                                            && collection_role == AddonMediaListItemRole::Auto)
                                    {
                                        IconName::Warning
                                    } else {
                                        IconName::Check
                                    },
                                    "0.95rem",
                                ) aria-hidden="true"></span>
                                {move || if (item_is_episode
                                    || presentation.get() == AddonMediaListPresentationKind::TelevisionShow)
                                    && season_episode_invalid
                                {
                                    tr(TextKey::BlockerSeasonEpisodeMissing)
                                } else if season_episode_unnumbered
                                    || base_needs_review
                                    || (presentation.get() == AddonMediaListPresentationKind::Movies
                                        && collection_role == AddonMediaListItemRole::Auto)
                                {
                                    tr(TextKey::ReviewRecommended)
                                } else {
                                    tr(TextKey::Available)
                                }}
                            </span>
                            <small class=style::row_note>{replacement}</small>
                        </dd>
                    </div>
                </dl>

                <footer class=style::item_actions>
                    <button
                        type="button"
                        class=style::detail_action
                        title=move || {
                            if detail_diagnostic.is_empty() {
                                tr(detail_state_text_key(detail_state)).to_string()
                            } else {
                                format!("{}: {detail_diagnostic}", tr(detail_state_text_key(detail_state)))
                            }
                        }
                        aria-label=detail_action_label
                        aria-busy=move || if detail_pending || (request_detail_action.pending().get() && detail_request_item.get() == Some(item_id)) { "true" } else { "false" }
                        style:display=if item.parent_provider_id.is_none() { "inline-flex" } else { "none" }
                        disabled=move || !can_request_details || request_detail_action.pending().get()
                        on:click=move |_| {
                            detail_request_item.set(Some(item_id));
                            request_detail_action.dispatch((list_id, item_id));
                        }
                    >
                        <span
                            style=icon_style(
                                match detail_state {
                                    AddonMediaListDetailState::Complete => IconName::Check,
                                    AddonMediaListDetailState::Queued
                                    | AddonMediaListDetailState::WaitingForSlot
                                    | AddonMediaListDetailState::Running => IconName::Clock,
                                    AddonMediaListDetailState::WaitingToRetry
                                    | AddonMediaListDetailState::Failed => IconName::Refresh,
                                    _ => IconName::Download,
                                },
                                "1rem",
                            )
                            aria-hidden="true"
                        ></span>
                        <span>{tr(detail_state_text_key(detail_state))}</span>
                    </button>
                    <button
                        type="button"
                        class=style::icon_button
                        title=tr(TextKey::Replace)
                        aria-label=tr(TextKey::Replace)
                        on:click=move |_| {
                            replacement_target.set(Some(item_id));
                            replacement_query.set(search_title.clone());
                            search_action.dispatch(search_title.clone());
                        }
                    >
                        <span style=icon_style(IconName::FindReplace, "1.25rem") aria-hidden="true"></span>
                    </button>
                    <a
                        class=style::icon_button
                        href=review_href
                        title=tr(TextKey::ReviewItem)
                        aria-label=tr(TextKey::ReviewItem)
                    >
                        <span style=icon_style(IconName::Edit, "1.2rem") aria-hidden="true"></span>
                        <span>{tr(TextKey::ReviewItem)}</span>
                    </a>
                    <button
                        type="button"
                        class=style::icon_button
                        title=tr(TextKey::Clear)
                        aria-label=tr(TextKey::Clear)
                        disabled=!has_replacement
                        on:click=move |_| {
                            replacement_action.dispatch((item_id, None));
                        }
                    >
                        <span style=icon_style(IconName::Delete, "1.25rem") aria-hidden="true"></span>
                    </button>
                </footer>
            </article>
        </li>
    }
    .into_any()
}

fn metadata_value_text(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "—".to_owned(),
        serde_json::Value::String(value) if value.is_empty() => "—".to_owned(),
        serde_json::Value::String(value) => value.clone(),
        value => serde_json::to_string(value).unwrap_or_else(|_| "—".to_owned()),
    }
}

#[component]
fn MetadataConflictSummary(
    conflicts: BTreeMap<String, AddonMediaListMetadataConflict>,
) -> impl IntoView {
    let has_conflicts = !conflicts.is_empty();

    view! {
        <section
            class=style::metadata_conflicts
            style:display=if has_conflicts { "grid" } else { "none" }
            aria-label=tr(TextKey::MetadataConflicts)
        >
            <header>
                <span style=icon_style(IconName::Warning, "1.1rem") aria-hidden="true"></span>
                <div>
                    <h3>{tr(TextKey::MetadataConflicts)}</h3>
                    <p>{tr(TextKey::MetadataConflictsHelp)}</p>
                </div>
            </header>
            <div class=style::metadata_conflict_table>
                <table>
                    <thead>
                        <tr>
                            <th scope="col">{tr(TextKey::Name)}</th>
                            <th scope="col">{tr(TextKey::PreviousProviderValue)}</th>
                            <th scope="col">{tr(TextKey::NewProviderValue)}</th>
                            <th scope="col">{tr(TextKey::OperatorValue)}</th>
                        </tr>
                    </thead>
                    <tbody>
                        {conflicts.into_iter().map(|(field, conflict)| view! {
                            <tr>
                                <th scope="row">{field}</th>
                                <td>{metadata_value_text(&conflict.provider_before)}</td>
                                <td>{metadata_value_text(&conflict.provider_after)}</td>
                                <td>{metadata_value_text(&conflict.operator_value)}</td>
                            </tr>
                        }).collect_view()}
                    </tbody>
                </table>
            </div>
        </section>
    }
}

#[component]
pub fn AddonMediaListsPage() -> impl IntoView {
    let query = use_query_map();
    let error = query.with_untracked(|params| params.get("error").unwrap_or_default());
    let initial_sort_field = query
        .with_untracked(|params| addon_list_sort_field(&params.get("sort").unwrap_or_default()));
    let initial_sort_direction = query.with_untracked(|params| {
        addon_list_sort_direction(&params.get("direction").unwrap_or_default())
    });
    let initial_page = query.with_untracked(|q| {
        q.get("page")
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(0)
    });
    let requested_size = query.with_untracked(|q| {
        q.get("page_size")
            .and_then(|v| v.parse::<u32>().ok())
            .filter(|n| [0, 10, 25, 50, 100].contains(n))
    });
    let initial = Resource::new_blocking(
        || (),
        move |()| async move {
            let page_size = requested_size.unwrap_or(get_addon_media_lists_page_size().await?);
            let rows = list_addon_media_lists(
                initial_page,
                page_size,
                initial_sort_field,
                initial_sort_direction,
            )
            .await?;
            let providers = list_addon_media_list_providers().await?;
            Ok::<_, ServerFnError>((page_size, rows, providers))
        },
    );

    view! {
        <Suspense fallback=admin_loading_resource_view>
            {move || {
                let error = error.clone();
                initial.get().map(move |result| admin_result_view(result, |(initial_page_size, (initial_rows, initial_total), providers)| {
                    view! {
                        <AddonMediaListsIsland
                            initial_error=error.clone()
                            initial_page_size=initial_page_size
                            initial_page=initial_page
                            initial_rows=initial_rows
                            initial_total=initial_total
                            initial_sort_field=initial_sort_field
                            initial_sort_direction=initial_sort_direction
                            providers=providers
                        />
                    }
                    .into_any()
                }))
            }}
        </Suspense>
        <AddonListDiskWarningSettings list_id=None/>
    }
}

/// Native Health settings stay outside media draft hydration and publication.
#[component]
fn AddonListDiskWarningSettings(list_id: Option<i64>) -> impl IntoView {
    #[cfg(feature = "ssr")]
    {
        let state = expect_context::<crate::host::state::AppState>();
        let result = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(state.application.addon_media_list_disk_warning_gb(list_id))
        });
        let query = use_query_map();
        let status = query.with_untracked(|params| params.get("disk_warning").unwrap_or_default());
        let failed = result.is_err();
        let saved = status == "saved";
        let value = result.map(|value| value.to_string()).unwrap_or_default();
        let label = tr(if list_id.is_some() {
            TextKey::AddonListDiskWarningGb
        } else {
            TextKey::AddonListsCombinedDiskWarningGb
        });
        view! {
            <section id="addon-list-disk-warning-settings" class=style::disk_warning_settings data-testid="addon-list-disk-warning-settings">
                <div class=style::content>
                    <h2>{tr(TextKey::AddonListDiskWarnings)}</h2>
                    <form method="post" action="/admin-api/addon-lists/disk-warning" class=style::disk_warning_form>
                        <input type="hidden" name="list_id" value=list_id.map(|id| id.to_string()).unwrap_or_default()/>
                        <label class=style::source_field>
                            <span class=style::field_title>{label}</span>
                            <input class=style::boxed_input type="number" name="warning_gb"
                                min="0" max="4294967295" step="1" required
                                value=value disabled=failed aria-describedby="addon-disk-warning-help"/>
                        </label>
                        <p id="addon-disk-warning-help">{tr(TextKey::AddonListDiskWarningHelp)}</p>
                        <p role="alert" hidden=!(failed || status == "error")>{tr(TextKey::AddonListDiskWarningFailed)}</p>
                        <p role="status" data-testid="addon-disk-warning-saved" hidden=!saved>{tr(TextKey::Saved)}</p>
                        <button type="submit" class=style::btn_add disabled=failed>{tr(TextKey::Save)}</button>
                    </form>
                </div>
            </section>
        }.into_any()
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = list_id;
        ().into_any()
    }
}

#[island]
fn AddonMediaListsIsland(
    initial_error: String,
    initial_page_size: u32,
    initial_page: u32,
    initial_rows: Vec<AddonMediaListRow>,
    initial_total: u64,
    initial_sort_field: AddonMediaListAdminSortField,
    initial_sort_direction: AddonMediaListAdminSortDirection,
    providers: Vec<AddonMediaListProviderRow>,
) -> impl IntoView {
    let page = RwSignal::new(if initial_page_size == 0 {
        0
    } else {
        initial_page.min(
            u32::try_from(initial_total.div_ceil(u64::from(initial_page_size)))
                .unwrap_or(u32::MAX)
                .saturating_sub(1),
        )
    });
    let page_size = RwSignal::new(initial_page_size);
    let sort_field = RwSignal::new(initial_sort_field);
    let sort_direction = RwSignal::new(initial_sort_direction);
    let current_data = RwSignal::new((initial_rows, initial_total));
    // The add-on is resolved from the URL, so the dialog opens with no
    // provider preselected instead of whichever one happens to sort first.
    let add_addon_id = RwSignal::new(String::new());
    let add_addon_chosen = RwSignal::new(false);
    let add_source_url = RwSignal::new(String::new());
    let add_folder_name = RwSignal::new(String::new());
    let add_content_kind = RwSignal::new(AddonMediaListContentKind::Auto);
    let add_refresh_cadence = RwSignal::new(AddonMediaListRefreshCadence::Daily);
    let add_load_item_details = RwSignal::new(true);
    let add_selection_review = RwSignal::new(false);
    let add_open = RwSignal::new(false);
    let has_initial_error = !initial_error.is_empty();
    // The plain HTML form path distinguishes an unroutable URL from a generic
    // add failure so the operator learns what to do about it.
    let initial_error_message = if initial_error == "add-unresolved" {
        tr(TextKey::AddonMediaListNoAddonForUrl)
    } else {
        tr(TextKey::AddonMediaListActionFailed)
    };
    let toast = use_toast();

    let data: LocalResource<(Vec<AddonMediaListRow>, u64)> = LocalResource::new(move || {
        let p = page.get();
        let size = page_size.get();
        let field = sort_field.get();
        let direction = sort_direction.get();
        async move {
            load_or_notify(
                list_addon_media_lists(p, size, field, direction).await,
                toast,
            )
        }
    });
    Effect::new(move |_| {
        if let Some(next_data) = data.get() {
            current_data.set(next_data);
        }
    });

    let add_action = Action::new(
        |value: &(
            String,
            String,
            String,
            AddonMediaListContentKind,
            AddonMediaListRefreshCadence,
            bool,
            bool,
        )| {
            let (
                addon_id,
                source_url,
                folder_name,
                content_kind_override,
                refresh_cadence,
                load_item_details,
                initial_selection_review,
            ) = value.clone();
            async move {
                add_addon_media_list(
                    addon_id,
                    source_url,
                    folder_name,
                    content_kind_override,
                    String::new(),
                    load_item_details,
                    refresh_cadence,
                    AddonMediaListDeliveryMode::Stream,
                    Some(initial_selection_review),
                )
                .await
            }
        },
    );
    let match_action = Action::new(|id: &i64| match_addon_media_list(*id));
    let delete_action = Action::new(|id: &i64| delete_addon_media_list(*id));

    let delete_target: RwSignal<Option<AddonMediaListRow>> = RwSignal::new(None);
    let delete_open = RwSignal::new(false);

    Effect::new(move |_| {
        if let Some(result) = add_action.value().get() {
            match result {
                Ok(_) => {
                    toast.push(
                        tr(TextKey::AddonMediaListAdded).to_string(),
                        ToastSeverity::Success,
                    );
                    add_source_url.set(String::new());
                    add_folder_name.set(String::new());
                    add_addon_id.set(String::new());
                    add_addon_chosen.set(false);
                    add_load_item_details.set(true);
                    add_selection_review.set(false);
                    add_refresh_cadence.set(AddonMediaListRefreshCadence::Daily);
                    add_open.set(false);
                    data.refetch();
                }
                Err(error) => toast.push(
                    format!(
                        "{}: {}",
                        tr(TextKey::AddFailed),
                        addon_media_list_error_message(error)
                    ),
                    ToastSeverity::Error,
                ),
            }
        }
    });

    Effect::new(move |_| {
        if let Some(result) = match_action.value().get() {
            match result {
                Ok(()) => {
                    toast.push(
                        tr(TextKey::AddonMediaListMatched).to_string(),
                        ToastSeverity::Success,
                    );
                    data.refetch();
                }
                Err(error) => toast.push(
                    format!(
                        "{}: {}",
                        tr(TextKey::MatchFailed),
                        addon_media_list_error_message(error)
                    ),
                    ToastSeverity::Error,
                ),
            }
        }
    });

    Effect::new(move |_| {
        if let Some(result) = delete_action.value().get() {
            match result {
                Ok(()) => {
                    toast.push(
                        tr(TextKey::AddonMediaListDeleted).to_string(),
                        ToastSeverity::Success,
                    );
                    data.refetch();
                }
                Err(error) => toast.push(
                    delete_failed(addon_media_list_error_message(error)),
                    ToastSeverity::Error,
                ),
            }
            delete_open.set(false);
            delete_target.set(None);
        }
    });

    view! {
        <div class=style::page>
            <section class=style::content>
                <header class=style::list_heading>
                    <h1 class=style::page_title>{tr(TextKey::AddonMediaLists)}</h1>
                    <a
                        class=style::btn_add
                        href="/media/addon-lists/add"
                        style:display=move || if current_data.with(|(rows, _)| rows.is_empty()) { "none" } else { "inline-flex" }
                    >
                        <span style=icon_style(IconName::Add, "1.125rem") aria-hidden="true"></span>
                        <span>{tr(TextKey::AddAddonMediaList)}</span>
                    </a>
                </header>
                <div class=style::divider/>
                <p class=style::empty style:display=move || if has_initial_error { "" } else { "none" }>
                    {initial_error_message}
                </p>
                <div class=style::empty_state style:display=move || if current_data.with(|(rows, _)| rows.is_empty()) { "grid" } else { "none" }>
                    <p>{tr(TextKey::NoAddonMediaListsYet)}</p>
                    <a class=style::btn_add href="/media/addon-lists/add">
                        <span style=icon_style(IconName::Add, "1.125rem") aria-hidden="true"></span>
                        <span>{tr(TextKey::AddAddonMediaList)}</span>
                    </a>
                </div>

                {move || {
                    let (rows, total) = current_data.get();
                    let has_rows = !rows.is_empty();
                    let current_sort_field = sort_field.get();
                    let current_sort_direction = sort_direction.get();
                    let (_, next_name_direction) = next_addon_list_sort(
                        current_sort_field,
                        current_sort_direction,
                        AddonMediaListAdminSortField::Name,
                    );
                    let (_, next_id_direction) = next_addon_list_sort(
                        current_sort_field,
                        current_sort_direction,
                        AddonMediaListAdminSortField::Id,
                    );

                    view! {
                                <div class=style::table_wrap style:display=if has_rows { "block" } else { "none" }>
                                    <table>
                                        <colgroup>
                                            <col/>
                                            <col/>
                                            <col/>
                                            <col class=style::actions_col/>
                                        </colgroup>
                                        <thead>
                                            <tr>
                                                <th
                                                    scope="col"
                                                    aria-sort=addon_list_sort_aria(
                                                        current_sort_field,
                                                        current_sort_direction,
                                                        AddonMediaListAdminSortField::Name,
                                                    )
                                                >
                                                    <form method="get" action="/media/addon-lists">
                                                        <input type="hidden" name="sort" value="name"/>
                                                        <input
                                                            type="hidden"
                                                            name="direction"
                                                            value=addon_list_sort_direction_value(next_name_direction)
                                                        />
                                                        <button
                                                            type="submit"
                                                            class=style::sort_button
                                                            aria-label=tr(TextKey::SortByName)
                                                            on:click=move |event| {
                                                                event.prevent_default();
                                                                let (field, direction) = next_addon_list_sort(
                                                                    sort_field.get_untracked(),
                                                                    sort_direction.get_untracked(),
                                                                    AddonMediaListAdminSortField::Name,
                                                                );
                                                                sort_field.set(field);
                                                                sort_direction.set(direction);
                                                                page.set(0);
                                                            }
                                                        >
                                                            {tr(TextKey::Name)}
                                                            <span aria-hidden="true">
                                                                {addon_list_sort_indicator(
                                                                    current_sort_field,
                                                                    current_sort_direction,
                                                                    AddonMediaListAdminSortField::Name,
                                                                )}
                                                            </span>
                                                        </button>
                                                    </form>
                                                </th>
                                                <th
                                                    scope="col"
                                                    aria-sort=addon_list_sort_aria(
                                                        current_sort_field,
                                                        current_sort_direction,
                                                        AddonMediaListAdminSortField::Id,
                                                    )
                                                >
                                                    <form method="get" action="/media/addon-lists">
                                                        <input type="hidden" name="sort" value="id"/>
                                                        <input
                                                            type="hidden"
                                                            name="direction"
                                                            value=addon_list_sort_direction_value(next_id_direction)
                                                        />
                                                        <button
                                                            type="submit"
                                                            class=style::sort_button
                                                            aria-label=tr(TextKey::SortById)
                                                            on:click=move |event| {
                                                                event.prevent_default();
                                                                let (field, direction) = next_addon_list_sort(
                                                                    sort_field.get_untracked(),
                                                                    sort_direction.get_untracked(),
                                                                    AddonMediaListAdminSortField::Id,
                                                                );
                                                                sort_field.set(field);
                                                                sort_direction.set(direction);
                                                                page.set(0);
                                                            }
                                                        >
                                                            {tr(TextKey::Id)}
                                                            <span aria-hidden="true">
                                                                {addon_list_sort_indicator(
                                                                    current_sort_field,
                                                                    current_sort_direction,
                                                                    AddonMediaListAdminSortField::Id,
                                                                )}
                                                            </span>
                                                        </button>
                                                    </form>
                                                </th>
                                                <th scope="col">{tr(TextKey::MatchStatus)}</th>
                                                <th/>
                                            </tr>
                                        </thead>
                                        <tbody>
                                            {rows.into_iter().map(|row| {
                                                    let id = row.id;
                                                    let provider_id = row.provider_id.clone();
                                                    let addon_id = row.addon_id.clone();
                                                    let name = row.name.clone();
                                                    let match_status = match_status(
                                                        u64::try_from(row.match_count).unwrap_or_default(),
                                                        u64::try_from(row.item_count).unwrap_or_default(),
                                                    );
                                                    let import_summary = row.import_progress.as_ref().map_or_else(|| {
                                                        if row.detail_import_policy == AddonMediaListDetailPolicy::ManualOnly {
                                                            tr(TextKey::OverviewOnly).to_string()
                                                        } else {
                                                            String::new()
                                                        }
                                                    }, |progress| {
                                                        format!(
                                                            "{} · {} · {} / {} {}",
                                                            tr(import_state_text_key(progress.state)),
                                                            import_discovery_summary(progress),
                                                            progress.detail_complete,
                                                            progress.detail_total,
                                                            tr(TextKey::Details),
                                                        )
                                                    });
                                                    let has_import_summary = !import_summary.is_empty();
                                                    let row_for_delete = row.clone();
                                                    view! {
                                                        <tr>
                                                            <td>
                                                                <a
                                                                    class=style::row_title
                                                                    href=format!("/media/addon-lists/{id}")
                                                                >{name}</a>
                                                                <small class=style::row_note style:display=if has_import_summary { "block" } else { "none" }>{import_summary}</small>
                                                            </td>
                                                            <td>{format!("{addon_id} / {provider_id}")}</td>
                                                            <td>{match_status}</td>
                                                            <td>
                                                                <div class=style::row_actions>
                                                                    <a
                                                                        class=style::icon_button
                                                                        href=format!("/media/addon-lists/{id}")
                                                                        title=tr(TextKey::EditAddonMediaListProperties)
                                                                        aria-label=tr(TextKey::EditAddonMediaListProperties)
                                                                    >
                                                                        <span style=icon_style(IconName::Edit, "1.25rem") aria-hidden="true"></span>
                                                                    </a>
                                                                    <a
                                                                        class=style::icon_button
                                                                        href=format!("/search?query=addon_list%3a{id}")
                                                                        title=tr(TextKey::SearchAddonMediaList)
                                                                        aria-label=tr(TextKey::SearchAddonMediaList)
                                                                    >
                                                                        <span style=icon_style(IconName::Search, "1.25rem") aria-hidden="true"></span>
                                                                    </a>
                                                                    <form method="post" action="/admin-api/addon-lists/match">
                                                                        <input type="hidden" name="id" value=id.to_string()/>
                                                                        <button
                                                                            type="submit"
                                                                            class=style::icon_button
                                                                            title=tr(TextKey::MatchAddonMediaListItems)
                                                                            aria-label=tr(TextKey::MatchAddonMediaListItems)
                                                                            on:click=move |event| {
                                                                                event.prevent_default();
                                                                                match_action.dispatch(id);
                                                                            }
                                                                        >
                                                                            <span style=icon_style(IconName::Refresh, "1.25rem") aria-hidden="true"></span>
                                                                        </button>
                                                                    </form>
                                                                    <form method="post" action="/admin-api/addon-lists/delete">
                                                                        <input type="hidden" name="id" value=id.to_string()/>
                                                                        <button
                                                                            type="submit"
                                                                            class=style::icon_button
                                                                            title=tr(TextKey::DeleteAddonMediaList)
                                                                            aria-label=tr(TextKey::DeleteAddonMediaList)
                                                                            on:click=move |event| {
                                                                                event.prevent_default();
                                                                                delete_target.set(Some(row_for_delete.clone()));
                                                                                delete_open.set(true);
                                                                            }
                                                                        >
                                                                            <span style=icon_style(IconName::Delete, "1.25rem") aria-hidden="true"></span>
                                                                        </button>
                                                                    </form>
                                                                </div>
                                                            </td>
                                                        </tr>
                                                    }
                                                }).collect_view()}
                                        </tbody>
                                    </table>
                                </div>
                                <noscript><crate::admin::ui::primitives::pagination::PaginationLinks
                                    page=initial_page total=initial_total page_size=initial_page_size
                                    previous_href=format!("?page={}&page_size={}&sort={}&direction={}",initial_page.saturating_sub(1),initial_page_size,match initial_sort_field {AddonMediaListAdminSortField::Name=>"name",AddonMediaListAdminSortField::Id=>"id"},addon_list_sort_direction_value(initial_sort_direction))
                                    next_href=format!("?page={}&page_size={}&sort={}&direction={}",initial_page.saturating_add(1),initial_page_size,match initial_sort_field {AddonMediaListAdminSortField::Name=>"name",AddonMediaListAdminSortField::Id=>"id"},addon_list_sort_direction_value(initial_sort_direction))
                                    page_size_fields=vec![crate::admin::ui::primitives::pagination::PaginationFormField{name:"sort".into(),value:match initial_sort_field {AddonMediaListAdminSortField::Name=>"name",AddonMediaListAdminSortField::Id=>"id"}.into()},crate::admin::ui::primitives::pagination::PaginationFormField{name:"direction".into(),value:addon_list_sort_direction_value(initial_sort_direction).into()}]
                                    page_size_action="/media/addon-lists".into()
                                    page_size_options=Some(vec![10,25,50,100,0])
                                /></noscript>
                                <div class=style::js_only><PaginationControls
                                    page=Signal::derive(move || page.get())
                                    total=Signal::derive(move || Some(total))
                                    page_size=Signal::derive(move || page_size.get())
                                    page_size_options=Some(vec![10, 25, 50, 100, 0])
                                    on_page_size_change=Callback::new(move |value: u32| {
                                        page_size.set(value);
                                        page.set(0);
                                    })
                                    on_previous=Callback::new(move |()| {
                                        page.update(|value| *value = value.saturating_sub(1));
                                    })
                                    on_next=Callback::new(move |()| {
                                        let size = page_size.get().max(1);
                                        let max_page = u32::try_from(total.div_ceil(u64::from(size)))
                                            .unwrap_or(u32::MAX)
                                            .saturating_sub(1);
                                        page.update(|value| *value = value.saturating_add(1).min(max_page));
                                    })
                                /></div>
                            }.into_any()
                }}
            </section>

            <AddAddonMediaListDialog
                open=add_open
                addon_id=add_addon_id
                addon_chosen=add_addon_chosen
                source_url=add_source_url
                folder_name=add_folder_name
                content_kind=add_content_kind
                refresh_cadence=add_refresh_cadence
                load_item_details=add_load_item_details
                initial_selection_review=add_selection_review
                providers=providers
                on_submit=Callback::new(move |value: (String, String, String, AddonMediaListContentKind, AddonMediaListRefreshCadence, bool, bool)| {
                    add_action.dispatch(value);
                })
            />
            <DeleteDialog
                dialog_id="addon-media-list-delete-dialog"
                open=delete_open
                entity_name=Signal::derive(move || {
                    delete_target.get().map(|row| row.name).unwrap_or_default()
                })
                on_confirm=Callback::new(move |confirmed: bool| {
                    if confirmed {
                        if let Some(row) = delete_target.get_untracked() {
                            delete_action.dispatch(row.id);
                        } else {
                            delete_open.set(false);
                        }
                    } else {
                        delete_open.set(false);
                    }
                })
            />
        </div>
    }
}

#[component]
fn AddAddonMediaListDialog(
    open: RwSignal<bool>,
    addon_id: RwSignal<String>,
    addon_chosen: RwSignal<bool>,
    source_url: RwSignal<String>,
    folder_name: RwSignal<String>,
    content_kind: RwSignal<AddonMediaListContentKind>,
    refresh_cadence: RwSignal<AddonMediaListRefreshCadence>,
    load_item_details: RwSignal<bool>,
    initial_selection_review: RwSignal<bool>,
    providers: Vec<AddonMediaListProviderRow>,
    on_submit: Callback<(
        String,
        String,
        String,
        AddonMediaListContentKind,
        AddonMediaListRefreshCadence,
        bool,
        bool,
    )>,
) -> impl IntoView {
    let adaptive_provider_ids = StoredValue::new(
        providers
            .iter()
            .filter(|provider| provider.supports_adaptive_refresh)
            .map(|provider| provider.addon_id.clone())
            .collect::<BTreeSet<_>>(),
    );
    let supports_adaptive_refresh =
        Memo::new(move |_| adaptive_provider_ids.with_value(|ids| ids.contains(&addon_id.get())));
    Effect::new(move |_| {
        if !supports_adaptive_refresh.get()
            && refresh_cadence.get_untracked() == AddonMediaListRefreshCadence::Adaptive
        {
            refresh_cadence.set(AddonMediaListRefreshCadence::Daily);
        }
    });
    // Resolving on change rather than on every keystroke keeps one request per
    // pasted or completed URL instead of one per character.
    let resolve_action = Action::new(|url: &String| {
        let url = url.clone();
        async move { resolve_addon_media_list_provider(url).await }
    });
    let resolved_name = RwSignal::new(String::new());
    Effect::new(move |_| {
        let Some(result) = resolve_action.value().get() else {
            return;
        };
        // An operator choice always outranks the resolved one.
        if addon_chosen.get_untracked() {
            return;
        }
        // A failed lookup and "nothing claims it" mean the same thing here:
        // the operator picks.
        if let Ok(Some(provider)) = result {
            resolved_name.set(provider.name);
            addon_id.set(provider.addon_id);
        } else {
            resolved_name.set(String::new());
            addon_id.set(String::new());
        }
    });

    view! {
        <div
            class=style::backdrop
            style:display=move || if open.get() { "" } else { "none" }
        />
        <div
            class=style::overlay
            style:display=move || if open.get() { "" } else { "none" }
            on:click=move |_| open.set(false)
        >
            <div class=style::panel on:click=|event| event.stop_propagation()>
                <h2 class=style::dialog_title>{tr(TextKey::AddAddonMediaList)}</h2>
                <form
                    method="post"
                    action="/admin-api/addon-lists/add"
                    on:submit=move |event| {
                        event.prevent_default();
                        let addon_id = addon_id.get_untracked();
                        let source_url = source_url.get_untracked();
                        let folder_name = folder_name.get_untracked();
                        let content_kind = content_kind.get_untracked();
                        let refresh_cadence = refresh_cadence.get_untracked();
                        let load_item_details = load_item_details.get_untracked();
                        // The add-on may be empty: the server resolves it
                        // from the URL, so the same rule applies to the plain
                        // HTML form post.
                        if !source_url.trim().is_empty() {
                            on_submit.run((addon_id, source_url, folder_name, content_kind, refresh_cadence, load_item_details, initial_selection_review.get_untracked()));
                        }
                    }
                >
                    <div class=style::dialog_body>
                        <p>{tr(TextKey::EnterAddonMediaListUrl)}</p>
                        <label class=style::dialog_field>
                            <span>{tr(TextKey::AddonMediaListUrl)}</span>
                            <input
                                type="text"
                                name="source_url"
                                placeholder="https://example.invalid/lists/list-name"
                                prop:value=move || source_url.get()
                                on:input=move |event| source_url.set(event_target_value(&event))
                                on:change=move |event| {
                                    let value = event_target_value(&event);
                                    source_url.set(value.clone());
                                    if !value.trim().is_empty() {
                                        resolve_action.dispatch(value);
                                    }
                                }
                            />
                        </label>
                        <label class=style::dialog_field>
                            <span>{tr(TextKey::Addon)}</span>
                            <select
                                name="addon_id"
                                prop:value=move || addon_id.get()
                                on:change=move |event| {
                                    let value = event_target_value(&event);
                                    // Returning to the empty option hands
                                    // resolution back to the URL, so ask again
                                    // rather than leaving the field blank.
                                    let chosen = !value.trim().is_empty();
                                    addon_chosen.set(chosen);
                                    addon_id.set(value);
                                    if !chosen {
                                        let url = source_url.get_untracked();
                                        if !url.trim().is_empty() {
                                            resolve_action.dispatch(url);
                                        }
                                    }
                                }
                            >
                                <option value="">{tr(TextKey::AddonAutoDetect)}</option>
                                {providers.into_iter().map(|provider| view! {
                                    <option value=provider.addon_id>{provider.name}</option>
                                }).collect_view()}
                            </select>
                            <p
                                class=style::dialog_hint
                                data-testid="addon-auto-detected"
                                style:display=move || {
                                    if resolved_name.with(String::is_empty) || addon_chosen.get() {
                                        "none"
                                    } else {
                                        ""
                                    }
                                }
                            >
                                {tr(TextKey::AddonAutoDetected)}
                            </p>
                        </label>
                        <label class=style::dialog_field>
                            <span>{tr(TextKey::MediaKind)}</span>
                            <select
                                name="content_kind_override"
                                prop:value=move || content_kind_value(content_kind.get())
                                on:change=move |event| {
                                    content_kind.set(content_kind_from_value(&event_target_value(&event)));
                                }
                            >
                                <option value="auto">{tr(TextKey::LeaveEmptyToAutoDetect)}</option>
                                <option value="television_episode">{tr(TextKey::TelevisionEpisode)}</option>
                                <option value="movie">{tr(TextKey::Movie)}</option>
                                <option value="music_video">{tr(TextKey::MusicVideo)}</option>
                                <option value="other_video">{tr(TextKey::OtherVideo)}</option>
                            </select>
                        </label>
                        <label class=style::dialog_field>
                            <span>{tr(TextKey::AutomaticRefresh)}</span>
                            <RefreshCadenceSelect
                                cadence=refresh_cadence
                                supports_adaptive=Signal::derive(move || supports_adaptive_refresh.get())
                                preserve_unavailable_adaptive=false
                                class_name=""
                            />
                        </label>
                        <label class=style::dialog_field>
                            <span>{tr(TextKey::AddonMediaListFolder)}</span>
                            <input
                                type="text"
                                name="folder_name"
                                placeholder=tr(TextKey::AddonMediaListFolderHelp)
                                prop:value=move || folder_name.get()
                                on:input=move |event| folder_name.set(event_target_value(&event))
                            />
                        </label>
                        <label class=style::playlist_option>
                        <input type="hidden" name="load_item_details_present" value="true"/>
                        <input type="checkbox" name="initial_selection_review" value="true" prop:checked=move || initial_selection_review.get() on:change=move |event| initial_selection_review.set(event_target_checked(&event))/>
                        <span><b>{tr(TextKey::AddonSelectionInitialReview)}</b><small>{tr(TextKey::AddonSelectionInitialReviewHelp)}</small></span>
                    </label>
                        <label class=style::playlist_option>
                            <input
                                type="checkbox"
                                name="load_item_details"
                                value="true"
                                prop:checked=move || load_item_details.get()
                                on:change=move |event| load_item_details.set(event_target_checked(&event))
                            />
                            <span>
                                <b>{tr(TextKey::LoadItemDetailsAfterPublishing)}</b>
                                <small>{tr(TextKey::LoadItemDetailsHelp)}</small>
                            </span>
                        </label>
                    </div>
                    <div class=style::dialog_footer>
                        <button type="button" class=style::btn_cancel on:click=move |_| open.set(false)>
                            {tr(TextKey::Cancel)}
                        </button>
                        <button
                            type="submit"
                            class=style::btn_add
                            disabled=move || source_url.with(|value| value.trim().is_empty())
                        >
                            {tr(TextKey::Add)}
                        </button>
                    </div>
                </form>
            </div>
        </div>
    }
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

const fn presentation_icon(presentation: AddonMediaListPresentationKind) -> IconName {
    match presentation {
        AddonMediaListPresentationKind::TelevisionShow => IconName::Tv,
        AddonMediaListPresentationKind::Movies | AddonMediaListPresentationKind::OtherVideos => {
            IconName::Film
        }
        AddonMediaListPresentationKind::MusicVideos | AddonMediaListPresentationKind::Songs => {
            IconName::MusicNote
        }
        AddonMediaListPresentationKind::Images => IconName::Image,
        AddonMediaListPresentationKind::RemoteStreams => IconName::Radio,
        AddonMediaListPresentationKind::Auto => IconName::Tune,
    }
}

const fn refresh_cadence_value(cadence: AddonMediaListRefreshCadence) -> &'static str {
    match cadence {
        AddonMediaListRefreshCadence::Adaptive => "adaptive",
        AddonMediaListRefreshCadence::Daily => "daily",
        AddonMediaListRefreshCadence::Weekly => "weekly",
        AddonMediaListRefreshCadence::Monthly => "monthly",
        AddonMediaListRefreshCadence::Yearly => "yearly",
        AddonMediaListRefreshCadence::Never => "never",
    }
}

const fn delivery_mode_value(mode: AddonMediaListDeliveryMode) -> &'static str {
    match mode {
        AddonMediaListDeliveryMode::Stream => "stream",
        AddonMediaListDeliveryMode::DownloadAhead => "download_ahead",
        AddonMediaListDeliveryMode::DownloadAheadNewVideos => "download_ahead_new_videos",
    }
}

#[component]
fn DeliveryModeSelect(
    mode: RwSignal<AddonMediaListDeliveryMode>,
    saved_mode: AddonMediaListDeliveryMode,
    cutoff_at: Option<String>,
    supports_download: Signal<bool>,
) -> impl IntoView {
    let cutoff = cutoff_at.unwrap_or_default();
    let cutoff_label = chrono::DateTime::parse_from_rfc3339(&cutoff)
        .ok()
        .and_then(|timestamp| {
            format_progress_timestamp(&timestamp.with_timezone(&chrono::Utc).to_rfc3339())
        })
        .unwrap_or_else(|| cutoff.clone());
    let has_cutoff =
        saved_mode == AddonMediaListDeliveryMode::DownloadAheadNewVideos && !cutoff.is_empty();
    view! {
        <div class=style::source_field>
            <label>
                <span class=style::field_title>{tr(TextKey::AddonDeliveryMode)}</span>
                <select
                    name="delivery_mode"
                    class=style::boxed_input
                    aria-label=tr(TextKey::AddonDeliveryMode)
                    aria-describedby="addon-delivery-help addon-delivery-availability"
                    prop:value=move || delivery_mode_value(mode.get())
                    on:change=move |event| mode.set(match event_target_value(&event).as_str() {
                        "download_ahead" => AddonMediaListDeliveryMode::DownloadAhead,
                        "download_ahead_new_videos" => AddonMediaListDeliveryMode::DownloadAheadNewVideos,
                        _ => AddonMediaListDeliveryMode::Stream,
                    })
                >
                    <option value="stream" selected=saved_mode == AddonMediaListDeliveryMode::Stream>{tr(TextKey::AddonDeliveryStream)}</option>
                    <option value="download_ahead" selected=saved_mode == AddonMediaListDeliveryMode::DownloadAhead disabled=move || !supports_download.get() && saved_mode != AddonMediaListDeliveryMode::DownloadAhead>{tr(TextKey::AddonDeliveryDownloadAhead)}</option>
                    <option value="download_ahead_new_videos" selected=saved_mode == AddonMediaListDeliveryMode::DownloadAheadNewVideos disabled=move || !supports_download.get() && saved_mode != AddonMediaListDeliveryMode::DownloadAheadNewVideos>{tr(TextKey::AddonDeliveryNewVideos)}</option>
                </select>
            </label>
            <small id="addon-delivery-help">{tr(TextKey::AddonDeliveryHelp)}</small>
            <small id="addon-delivery-availability" style:display=move || if supports_download.get() { "none" } else { "block" }>{tr(TextKey::AddonDeliveryUnavailable)}</small>
            <p data-testid="addon-delivery-cutoff" style:display=if has_cutoff { "block" } else { "none" }>
                <span>{tr(TextKey::AddonDeliveryCutoff)}": "</span>
                <time datetime=cutoff>{cutoff_label}</time>
            </p>
            <small style:display=move || if mode.get() == AddonMediaListDeliveryMode::DownloadAheadNewVideos && saved_mode != AddonMediaListDeliveryMode::DownloadAheadNewVideos { "block" } else { "none" }>{tr(TextKey::AddonDeliveryFreshCutoff)}</small>
        </div>
    }
}

fn refresh_cadence_from_value(value: &str) -> AddonMediaListRefreshCadence {
    match value {
        "adaptive" => AddonMediaListRefreshCadence::Adaptive,
        "weekly" => AddonMediaListRefreshCadence::Weekly,
        "monthly" => AddonMediaListRefreshCadence::Monthly,
        "yearly" => AddonMediaListRefreshCadence::Yearly,
        "never" => AddonMediaListRefreshCadence::Never,
        _ => AddonMediaListRefreshCadence::Daily,
    }
}

const fn refresh_cadence_icon(cadence: AddonMediaListRefreshCadence) -> IconName {
    if matches!(cadence, AddonMediaListRefreshCadence::Never) {
        IconName::RefreshOff
    } else {
        IconName::Clock
    }
}

#[component]
fn RefreshCadenceSelect(
    cadence: RwSignal<AddonMediaListRefreshCadence>,
    supports_adaptive: Signal<bool>,
    preserve_unavailable_adaptive: bool,
    class_name: &'static str,
) -> impl IntoView {
    view! {
        <select
            class=class_name
            name="refresh_cadence"
            aria-label=tr(TextKey::AutomaticRefresh)
            prop:value=move || refresh_cadence_value(cadence.get())
            on:change=move |event| {
                cadence.set(refresh_cadence_from_value(&event_target_value(&event)));
            }
        >
            <option
                value="adaptive"
                disabled=move || !supports_adaptive.get() && !preserve_unavailable_adaptive
                selected=move || cadence.get() == AddonMediaListRefreshCadence::Adaptive
            >{tr(TextKey::RefreshCadenceAdaptive)}</option>
            <option value="daily" selected=move || cadence.get() == AddonMediaListRefreshCadence::Daily>{tr(TextKey::RefreshCadenceDaily)}</option>
            <option value="weekly" selected=move || cadence.get() == AddonMediaListRefreshCadence::Weekly>{tr(TextKey::RefreshCadenceWeekly)}</option>
            <option value="monthly" selected=move || cadence.get() == AddonMediaListRefreshCadence::Monthly>{tr(TextKey::RefreshCadenceMonthly)}</option>
            <option value="yearly" selected=move || cadence.get() == AddonMediaListRefreshCadence::Yearly>{tr(TextKey::RefreshCadenceYearly)}</option>
            <option value="never" selected=move || cadence.get() == AddonMediaListRefreshCadence::Never>{tr(TextKey::RefreshCadenceNever)}</option>
        </select>
    }
}

const fn presentation_text_key(presentation: AddonMediaListPresentationKind) -> TextKey {
    match presentation {
        AddonMediaListPresentationKind::TelevisionShow => TextKey::TvShows,
        AddonMediaListPresentationKind::Movies => TextKey::Movies,
        AddonMediaListPresentationKind::MusicVideos => TextKey::MusicVideo,
        AddonMediaListPresentationKind::OtherVideos => TextKey::OtherVideos,
        AddonMediaListPresentationKind::Songs => TextKey::Songs,
        AddonMediaListPresentationKind::Images => TextKey::Images,
        AddonMediaListPresentationKind::RemoteStreams => TextKey::RemoteStreams,
        AddonMediaListPresentationKind::Auto => TextKey::CollectionTypeAuto,
    }
}

/// Source-first add page based on the approved Option D artifact.
///
/// Loading the source creates a draft and moves into the same shared editor as
/// an existing list. Review/save and conditional preparation precede explicit
/// publication, the only step that materializes the list for scheduling.
#[component]
pub fn AddAddonMediaListPage() -> impl IntoView {
    let query = use_query_map();
    let initial_addon_id =
        query.with_untracked(|params| params.get("addon_id").unwrap_or_default());
    admin_result_view(
        load_addon_media_list_providers_initial(),
        move |providers| {
            view! {
                <AddAddonMediaListPageIsland providers=providers initial_addon_id=initial_addon_id/>
            }
            .into_any()
        },
    )
}

#[cfg(feature = "ssr")]
fn load_addon_media_list_providers_initial() -> Result<Vec<AddonMediaListProviderRow>, String> {
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current()
            .block_on(list_addon_media_list_providers())
            .map_err(|error| error.to_string())
    })
}

#[cfg(not(feature = "ssr"))]
fn load_addon_media_list_providers_initial() -> Result<Vec<AddonMediaListProviderRow>, String> {
    Err("add-on list providers are loaded by server-side rendering".to_owned())
}

#[island]
fn AddAddonMediaListPageIsland(
    providers: Vec<AddonMediaListProviderRow>,
    initial_addon_id: String,
) -> impl IntoView {
    let any_download_provider = providers
        .iter()
        .any(|provider| provider.supports_download_ahead);
    let download_provider_ids = StoredValue::new(
        providers
            .iter()
            .filter(|provider| provider.supports_download_ahead)
            .map(|provider| provider.addon_id.clone())
            .collect::<BTreeSet<_>>(),
    );
    let delivery_mode = RwSignal::new(AddonMediaListDeliveryMode::Stream);
    let any_adaptive_provider = providers
        .iter()
        .any(|provider| provider.supports_adaptive_refresh);
    let adaptive_provider_ids = StoredValue::new(
        providers
            .iter()
            .filter(|provider| provider.supports_adaptive_refresh)
            .map(|provider| provider.addon_id.clone())
            .collect::<BTreeSet<_>>(),
    );
    let context_addon_name = providers
        .iter()
        .find(|provider| provider.addon_id == initial_addon_id)
        .map(|provider| provider.name.clone());
    let has_addon_context = context_addon_name.is_some();
    let addon_id = RwSignal::new(initial_addon_id.clone());
    let addon_chosen = RwSignal::new(!initial_addon_id.is_empty());
    let source_url = RwSignal::new(String::new());
    let folder_name = RwSignal::new(String::new());
    let presentation = RwSignal::new(AddonMediaListPresentationKind::TelevisionShow);
    let refresh_cadence = RwSignal::new(AddonMediaListRefreshCadence::Daily);
    let generate_playlist = RwSignal::new(false);
    let load_item_details = RwSignal::new(true);
    let initial_selection_review = RwSignal::new(false);
    let resolved_name = RwSignal::new(String::new());
    let hydrated = RwSignal::new(false);
    let supports_adaptive_refresh =
        Memo::new(move |_| adaptive_provider_ids.with_value(|ids| ids.contains(&addon_id.get())));
    Effect::new(move |_| hydrated.set(true));
    Effect::new(move |_| {
        if !supports_adaptive_refresh.get()
            && refresh_cadence.get() == AddonMediaListRefreshCadence::Adaptive
        {
            refresh_cadence.set(AddonMediaListRefreshCadence::Daily);
        }
    });

    let resolve_action = Action::new(|url: &String| {
        let url = url.clone();
        async move { resolve_addon_media_list_provider(url).await }
    });
    Effect::new(move |_| {
        let Some(result) = resolve_action.value().get() else {
            return;
        };
        if addon_chosen.get_untracked() {
            return;
        }
        if let Ok(Some(provider)) = result {
            resolved_name.set(provider.name);
            addon_id.set(provider.addon_id);
        } else {
            resolved_name.set(String::new());
            addon_id.set(String::new());
        }
    });

    let import_action = Action::new(
        |value: &(
            String,
            String,
            String,
            AddonMediaListPresentationKind,
            AddonMediaListRefreshCadence,
            AddonMediaListDeliveryMode,
            bool,
            bool,
            bool,
        )| {
            let (
                addon_id,
                source_url,
                folder_name,
                presentation,
                refresh_cadence,
                delivery_mode,
                playlist,
                load_item_details,
                initial_selection_review,
            ) = value.clone();
            async move {
                let id = add_addon_media_list(
                    addon_id,
                    source_url,
                    folder_name,
                    presentation_content_kind(presentation),
                    String::new(),
                    load_item_details,
                    refresh_cadence,
                    delivery_mode,
                    Some(initial_selection_review),
                )
                .await?;
                // The list now exists. A later settings failure must open that
                // list with a notice, as the no-JS form does, rather than
                // report the whole add as failed.
                let settings = async {
                    let mut form = get_addon_media_list_form(id)
                        .await?
                        .ok_or_else(|| ServerFnError::new("imported add-on list was not found"))?;
                    form.refresh_cadence = refresh_cadence;
                    form.generate_playlist = playlist;
                    update_addon_media_list(form).await?;
                    request_addon_media_list_presentation(id, presentation, None).await?;
                    Ok::<(), ServerFnError>(())
                }
                .await;
                Ok::<(i64, bool), ServerFnError>((id, settings.is_ok()))
            }
        },
    );
    Effect::new(move |_| {
        if let Some(result) = import_action.value().get() {
            match result {
                Ok((id, true)) => {
                    set_browser_location(&format!("/media/addon-lists/{id}?import=1"));
                }
                Ok((id, false)) => {
                    set_browser_location(&format!("/media/addon-lists/{id}?error=save"));
                }
                // The form keeps the actionable error beside its submit.
                Err(_) => {}
            }
        }
    });

    view! {
        <div class=style::page>
            <div class=style::action_bar>
                <a class=style::icon_button href="/media/addon-lists" title=tr(TextKey::Back) aria-label=tr(TextKey::Back)>
                    <span style=icon_style(IconName::ChevronLeft, "1.25rem") aria-hidden="true"></span>
                </a>
            </div>
            <section class=style::import_shell>
                <header class=style::import_header>
                    <div>
                        <h1 class=style::page_title>{tr(TextKey::AddAddonMediaList)}</h1>
                        <p>{tr(TextKey::AddonWorkspaceImportSteps)}</p>
                    </div>
                </header>

                <form
                    class=style::source_card
                    method="post"
                    action="/admin-api/addon-lists/add"
                    on:submit=move |event| {
                    event.prevent_default();
                    if source_url.with(|value| !value.trim().is_empty()) {
                        import_action.dispatch((
                            addon_id.get_untracked(),
                            source_url.get_untracked(),
                            folder_name.get_untracked(),
                            presentation.get_untracked(),
                            refresh_cadence.get_untracked(),
                            delivery_mode.get_untracked(),
                            generate_playlist.get_untracked(),
                            load_item_details.get_untracked(),
                            initial_selection_review.get_untracked(),
                        ));
                    }
                    }
                >
                    <p class=style::context_notice style:display=if has_addon_context { "flex" } else { "none" } role="status">
                        <span style=icon_style(IconName::Check, "1rem") aria-hidden="true"></span>
                        <span>{format!("{}: {}", tr(TextKey::AddonContextSelected), context_addon_name.unwrap_or_default())}</span>
                    </p>
                    <label class=style::source_url_field>
                        <span class=style::field_label>{tr(TextKey::AddonMediaListUrl)}</span>
                        <input
                            class=style::boxed_input
                            type="url"
                            name="source_url"
                            required
                            placeholder="https://example.invalid/list"
                            prop:value=move || source_url.get()
                            on:input=move |event| source_url.set(event_target_value(&event))
                            on:change=move |event| {
                                let value = event_target_value(&event);
                                source_url.set(value.clone());
                                if !value.trim().is_empty() {
                                    resolve_action.dispatch(value);
                                }
                            }
                        />
                        <span
                            class=style::source_detected
                            style:display=move || if resolved_name.with(String::is_empty) { "none" } else { "inline-flex" }
                        >
                            <span style=icon_style(IconName::Check, "1rem") aria-hidden="true"></span>
                            {move || format!("{} {}", tr(TextKey::AddonAutoDetected), resolved_name.get())}
                        </span>
                        <span role="status" hidden=move || addon_chosen.get() || source_url.with(|url| url.trim().is_empty()) || (!resolve_action.pending().get() && !resolved_name.with(String::is_empty))>
                            {move || tr(if resolve_action.pending().get() {TextKey::WaitingForProvider} else {TextKey::AddonWorkspaceChooseProvider})}
                        </span>
                    </label>

                    <label class=style::source_field>
                        <span class=style::field_title>
                            <span style=icon_style(IconName::Settings, "1.05rem") aria-hidden="true"></span>
                            <span>{tr(TextKey::Addon)}</span>
                        </span>
                        <select
                            class=style::boxed_input
                            name="addon_id"
                            aria-label=tr(TextKey::Addon)
                            prop:value=move || addon_id.get()
                            on:change=move |event| {
                            let value = event_target_value(&event);
                            addon_chosen.set(!value.is_empty());
                            addon_id.set(value);
                        }>
                            <option value="">{tr(TextKey::AddonAutoDetect)}</option>
                            {providers.into_iter().map(|provider| {
                                let selected = provider.addon_id == initial_addon_id;
                                view! {
                                    <option value=provider.addon_id selected=selected>{provider.name}</option>
                                }
                            }).collect_view()}
                        </select>
                    </label>

                    <div class=style::source_pair>
                        <label class=style::source_field>
                            <span class=style::field_title>
                                <span style=move || icon_style(presentation_icon(presentation.get()), "1.05rem") aria-hidden="true"></span>
                                <span>{tr(TextKey::CollectionType)}</span>
                            </span>
                            <select
                                class=style::boxed_input
                                name="presentation_kind"
                                aria-label=tr(TextKey::CollectionType)
                                prop:value=move || presentation_kind_value(presentation.get())
                                on:change=move |event| {
                                presentation.set(presentation_kind_from_value(&event_target_value(&event)));
                            }>
                                <option value="movies">{tr(TextKey::Movies)}</option>
                                <option value="television_show">{tr(TextKey::TvShows)}</option>
                                <option value="music_videos">{tr(TextKey::MusicVideo)}</option>
                                <option value="other_videos">{tr(TextKey::OtherVideos)}</option>
                                <option value="songs">{tr(TextKey::Songs)}</option>
                                <option value="images">{tr(TextKey::Images)}</option>
                                <option value="remote_streams">{tr(TextKey::RemoteStreams)}</option>
                            </select>
                        </label>
                        <label class=style::source_field>
                            <span class=style::field_title>
                                <span style=move || icon_style(refresh_cadence_icon(refresh_cadence.get()), "1.05rem") aria-hidden="true"></span>
                                <span>{tr(TextKey::AutomaticRefresh)}</span>
                            </span>
                            <RefreshCadenceSelect
                                cadence=refresh_cadence
                                supports_adaptive=Signal::derive(move || {
                                    if hydrated.get() {
                                        supports_adaptive_refresh.get()
                                    } else {
                                        any_adaptive_provider
                                    }
                                })
                                preserve_unavailable_adaptive=false
                                class_name=style::boxed_input
                            />
                        </label>
                    </div>

                    <label class=style::source_field>
                        <span class=style::field_title>
                            <span style=icon_style(IconName::Folder, "1.05rem") aria-hidden="true"></span>
                            <span>{tr(TextKey::AddonMediaListFolder)}</span>
                        </span>
                        <input class=style::boxed_input type="text" name="folder_name" prop:value=move || folder_name.get() on:input=move |event| folder_name.set(event_target_value(&event)) placeholder=tr(TextKey::AddonMediaListFolderHelp)/>
                    </label>

                    <DeliveryModeSelect
                        mode=delivery_mode
                        saved_mode=AddonMediaListDeliveryMode::Stream
                        cutoff_at=None
                        supports_download=Signal::derive(move || if hydrated.get() {
                            download_provider_ids.with_value(|ids| ids.contains(&addon_id.get()))
                        } else { any_download_provider })
                    />

                    <label class=style::playlist_option>
                        <input type="checkbox" name="generate_playlist" value="true" prop:checked=move || generate_playlist.get() on:change=move |event| generate_playlist.set(event_target_checked(&event))/>
                        <span>
                            <b>{tr(TextKey::GeneratePlaylist)}</b>
                            <small>{tr(TextKey::GeneratePlaylistFromSortedAddonMediaListItems)}</small>
                        </span>
                    </label>

                    <label class=style::playlist_option>
                        <input type="checkbox" name="load_item_details" value="true" prop:checked=move || load_item_details.get() on:change=move |event| load_item_details.set(event_target_checked(&event))/>
                        <span>
                            <b>{tr(TextKey::LoadItemDetailsAfterPublishing)}</b>
                            <small>{move || tr(if load_item_details.get() {
                                TextKey::LoadItemDetailsHelp
                            } else {
                                TextKey::OverviewOnlyManualDetailsHelp
                            })}</small>
                        </span>
                    </label>

                    <label class=style::playlist_option>
                        <input type="hidden" name="load_item_details_present" value="true"/>
                        <input type="checkbox" name="initial_selection_review" value="true" prop:checked=move || initial_selection_review.get() on:change=move |event| initial_selection_review.set(event_target_checked(&event))/>
                        <span><b>{tr(TextKey::AddonSelectionInitialReview)}</b><small>{tr(TextKey::AddonSelectionInitialReviewHelp)}</small></span>
                    </label>

                    <footer class=style::import_footer>
                        <p role="alert" hidden=move || !matches!(import_action.value().get(),Some(Err(_)))>{move || import_action.value().get().and_then(Result::err).map(addon_media_list_error_message)}</p>
                        <button class=style::btn_add type="submit" disabled=move || import_action.pending().get()>
                            <span style=icon_style(IconName::Download, "1.1rem") aria-hidden="true"></span>
                            <span>{tr(TextKey::ImportOverview)}</span>
                        </button>
                    </footer>
                </form>
            </section>
        </div>
    }
}

#[component]
pub fn AddonMediaListEditorPage() -> impl IntoView {
    let params = use_params_map();
    let query = use_query_map();
    let id = params.with(|p| {
        p.get("id")
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(0)
    });
    let error = query.with_untracked(|params| params.get("error").unwrap_or_default());
    let status = query.with_untracked(|params| params.get("status").unwrap_or_default());
    let compare_left = query.with_untracked(|params| {
        params
            .get("compare_left")
            .and_then(|value| value.parse::<i32>().ok())
    });
    let compare_right = query.with_untracked(|params| {
        params
            .get("compare_right")
            .and_then(|value| value.parse::<i32>().ok())
    });
    let review_category =
        query.with_untracked(|params| params.get("review_category").unwrap_or_default());
    admin_result_view(
        load_addon_media_list_editor_initial(id, compare_left.zip(compare_right)),
        move |(
            initial_form,
            initial_items,
            initial_readiness,
            initial_history,
            initial_show_nfo,
            providers,
            initial_source_probe,
            initial_draft_diff,
            initial_reset_impact,
            initial_revisions,
            initial_revision_diff,
            _load_status,
        )| {
            view! {
                <AddonMediaListEditorIsland
                    id=id
                    options_available=super::addon_list_options::options_available(id)
                    initial_error=error
                    initial_status=status
                    initial_form=initial_form
                    initial_items=initial_items
                    initial_readiness=initial_readiness
                    initial_history=initial_history
                    initial_show_nfo=initial_show_nfo
                    providers=providers
                    initial_source_probe=initial_source_probe
                    initial_draft_diff=initial_draft_diff
                    initial_reset_impact=initial_reset_impact
                    initial_revisions=initial_revisions
                    initial_revision_diff=initial_revision_diff
                    initial_compare_revisions=compare_left.zip(compare_right)
                    initial_review_category=review_category
                />
                <AddonListDiskWarningSettings list_id=Some(id)/>
                <super::addon_list_options::AddonListOptions list_id=id item_ids=Vec::new()/>
            }
            .into_any()
        },
    )
}

#[derive(Clone, Debug, Default)]
struct EditorLoadStatus {
    incomplete: bool,
    disabled_tasks: std::collections::BTreeSet<&'static str>,
}

#[cfg(feature = "ssr")]
impl EditorLoadStatus {
    fn retain<T: Default, E>(&mut self, result: Result<T, E>, tasks: &[&'static str]) -> T {
        if let Ok(value) = result {
            value
        } else {
            self.incomplete = true;
            self.disabled_tasks.extend(tasks.iter().copied());
            T::default()
        }
    }
}

type AddonMediaListEditorInitial = (
    Option<AddonMediaListForm>,
    Vec<AddonMediaListItemRow>,
    AddonMediaListPublicationReadiness,
    AddonMediaListPublicationHistoryRow,
    AddonMediaListNfoForm,
    Vec<AddonMediaListProviderRow>,
    Option<AddonMediaListSourceProbeRow>,
    AddonMediaListDraftDiffRow,
    AddonMediaListResetImpactRow,
    Vec<AddonMediaListRevisionSummaryRow>,
    Option<AddonMediaListRevisionDiffRow>,
    EditorLoadStatus,
);

#[cfg(feature = "ssr")]
fn load_addon_media_list_editor_initial(
    id: i64,
    compare_revisions: Option<(i32, i32)>,
) -> Result<AddonMediaListEditorInitial, String> {
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async move {
            consistent_editor_read(|| async {
                let form = get_addon_media_list_form(id)
                    .await
                    .map_err(|error| error.to_string())?;
                let revision = form.as_ref().map(|form| form.draft_revision);
                let mut status = EditorLoadStatus::default();
                let items =
                    status.retain(list_addon_media_list_items(id).await, &["items", "item"]);
                let readiness_result = get_addon_media_list_publication_readiness(id).await;
                let readiness_failed = readiness_result.is_err();
                let mut readiness = status.retain(readiness_result, &[]);
                let history = status.retain(
                    get_addon_media_list_publication_history(id).await,
                    &["activity"],
                );
                let show_nfo = status.retain(
                    get_addon_media_list_show_nfo(id).await,
                    &["settings", "artwork"],
                );
                let providers = status.retain(
                    list_addon_media_list_providers().await,
                    &["settings", "playback"],
                );
                let source_probe =
                    status.retain(get_addon_media_list_source_probe(id).await, &["activity"]);
                let draft_diff =
                    status.retain(get_addon_media_list_saved_draft_diff(id).await, &["items"]);
                let reset_impact =
                    status.retain(get_addon_media_list_reset_impact(id).await, &["activity"]);
                let revisions = status.retain(
                    get_addon_media_list_revision_history(id).await,
                    &["activity"],
                );
                let revision_diff = if let Some((left, right)) = compare_revisions {
                    status.retain(
                        compare_addon_media_list_revisions(id, left, right)
                            .await
                            .map(Some),
                        &["activity"],
                    )
                } else {
                    None
                };
                let final_form = get_addon_media_list_form(id).await;
                let stable = final_form
                    .as_ref()
                    .is_ok_and(|form| form.as_ref().map(|form| form.draft_revision) == revision);
                if readiness_failed || Some(readiness.saved_revision) != revision {
                    // An invalid revision has no admissible publication action. The
                    // existing readiness poll can replace it with authoritative data.
                    readiness.saved_revision = -1;
                    status.incomplete = true;
                }
                Ok((
                    (
                        form,
                        items,
                        readiness,
                        history,
                        show_nfo,
                        providers,
                        source_probe,
                        draft_diff,
                        reset_impact,
                        revisions,
                        revision_diff,
                        status,
                    ),
                    stable,
                ))
            })
            .await
        })
    })
}

#[cfg(feature = "ssr")]
async fn consistent_editor_read<F, Fut>(mut read: F) -> Result<AddonMediaListEditorInitial, String>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<(AddonMediaListEditorInitial, bool), String>>,
{
    let (first, stable) = read().await?;
    if stable {
        return Ok(first);
    }
    // Only one retry: ongoing background progress must not hide the form.
    let (mut retained, stable) = if let Ok(retry) = read().await {
        retry
    } else {
        // Keep the first form and explicitly mark this failed retry below.
        (first, false)
    };
    if !stable {
        retained.2.saved_revision = -1;
        retained.11.incomplete = true;
        retained.11.disabled_tasks.extend([
            "items", "item", "settings", "playback", "artwork", "activity",
        ]);
    }
    Ok(retained)
}

#[cfg(not(feature = "ssr"))]
fn load_addon_media_list_editor_initial(
    _id: i64,
    _compare_revisions: Option<(i32, i32)>,
) -> Result<AddonMediaListEditorInitial, String> {
    Err("the add-on list editor is loaded by server-side rendering".to_owned())
}

#[component]
pub fn AddonMediaListItemPage() -> impl IntoView {
    let params = use_params_map();
    let query = use_query_map();
    let list_id = params.with(|params| {
        params
            .get("id")
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or_default()
    });
    let item_id = params.with(|params| {
        params
            .get("item_id")
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or_default()
    });
    let status = query.with_untracked(|params| params.get("status").unwrap_or_default());
    let error = query.with_untracked(|params| params.get("error").unwrap_or_default());
    admin_result_view(
        load_addon_media_list_item_initial(list_id, item_id),
        move |detail| {
            detail.map_or_else(
                || {
                    view! { <p class=style::empty>{tr(TextKey::AddonMediaListNotFound)}</p> }
                        .into_any()
                },
                |detail| {
                    view! {
                        <AddonMediaListItemIsland
                            list_id=list_id
                            detail=detail
                            initial_status=status
                            initial_error=error
                        />
                        <super::addon_list_options::AddonListOptions list_id=list_id item_ids=vec![item_id]/>
                    }
                    .into_any()
                },
            )
        },
    )
}

#[cfg(feature = "ssr")]
fn load_addon_media_list_item_initial(
    list_id: i64,
    item_id: i64,
) -> Result<Option<AddonMediaListItemDetail>, String> {
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current()
            .block_on(get_addon_media_list_item(list_id, item_id))
            .map_err(|error| error.to_string())
    })
}

#[cfg(not(feature = "ssr"))]
fn load_addon_media_list_item_initial(
    _list_id: i64,
    _item_id: i64,
) -> Result<Option<AddonMediaListItemDetail>, String> {
    Err("the add-on list item editor is loaded by server-side rendering".to_owned())
}

#[island]
fn AddonMediaListItemIsland(
    list_id: i64,
    detail: AddonMediaListItemDetail,
    initial_status: String,
    initial_error: String,
) -> impl IntoView {
    let item_id = detail.item.id;
    let original = detail.nfo.metadata.clone();
    let metadata_conflicts = detail.nfo.conflicts.clone();
    let saved_metadata = RwSignal::new(original.clone());
    let etag = RwSignal::new(detail.nfo.etag.clone());
    let title = RwSignal::new(original.title.clone());
    let original_title = RwSignal::new(original.original_title.clone().unwrap_or_default());
    let plot = RwSignal::new(original.plot.clone().unwrap_or_default());
    let year = RwSignal::new(
        original
            .year
            .map_or_else(String::new, |value| value.to_string()),
    );
    let release_date = RwSignal::new(original.release_date.clone().unwrap_or_default());
    let season = RwSignal::new(
        original
            .season
            .map_or_else(String::new, |value| value.to_string()),
    );
    let episode = RwSignal::new(
        original
            .episode
            .map_or_else(String::new, |value| value.to_string()),
    );
    let genres = RwSignal::new(original.genres.join(", "));
    let tags = RwSignal::new(original.tags.join("\n"));
    let artists = RwSignal::new(original.artists.join(", "));
    let studios = RwSignal::new(original.studios.join(", "));
    let directors = RwSignal::new(original.directors.join(", "));
    let writers = RwSignal::new(original.writers.join(", "));
    let languages = RwSignal::new(original.languages.join(", "));
    let people = RwSignal::new(people_lines(&original.people));
    let poster_url = RwSignal::new(artwork_value(
        &original.artwork,
        AddonMediaListArtworkRole::Poster,
    ));
    let fanart_url = RwSignal::new(artwork_value(
        &original.artwork,
        AddonMediaListArtworkRole::Fanart,
    ));
    let thumbnail_url = RwSignal::new(artwork_value(
        &original.artwork,
        AddonMediaListArtworkRole::Thumb,
    ));
    let poster_candidates =
        artwork_candidates(&original.artwork, AddonMediaListArtworkRole::Poster);
    let fanart_candidates =
        artwork_candidates(&original.artwork, AddonMediaListArtworkRole::Fanart);
    let thumbnail_candidates =
        artwork_candidates(&original.artwork, AddonMediaListArtworkRole::Thumb);
    let provider_chapters = detail.item.provider_chapter_input.clone();
    let chapters_editable = detail.item.parent_provider_id.is_none();
    let saved_operator_chapters = RwSignal::new(detail.item.chapter_input.clone());
    let chapters_dirty = RwSignal::new(false);
    let submitted_chapters = RwSignal::new(detail.item.chapter_input.clone());
    let chapters = RwSignal::new(if detail.item.chapter_input.trim().is_empty() {
        provider_chapters.clone()
    } else {
        detail.item.chapter_input.clone()
    });
    let toast = use_toast();
    let save_action = Action::new(
        move |value: &(AddonMediaListNfoMetadata, String, Option<String>)| {
            let (metadata, etag, chapters) = value.clone();
            async move {
                let save_result = async {
                    if let Some(chapters) = chapters.as_deref() {
                        validate_addon_media_list_item_chapters(chapters.to_owned()).await?;
                    }
                    save_addon_media_list_item_nfo(list_id, item_id, metadata, etag).await?;
                    if let Some(chapters) = chapters {
                        update_addon_media_list_item_chapters(item_id, chapters).await?;
                    }
                    Ok::<(), ServerFnError>(())
                }
                .await;
                let refreshed = get_addon_media_list_item(list_id, item_id).await;
                (save_result, refreshed)
            }
        },
    );
    let restore_action = Action::new(move |(): &()| async move {
        update_addon_media_list_item_chapters(item_id, String::new()).await
    });
    Effect::new(move |_| {
        if let Some((save_result, refreshed)) = save_action.value().get() {
            match (save_result, refreshed) {
                (Ok(()), Ok(Some(refreshed))) => {
                    saved_metadata.set(refreshed.nfo.metadata);
                    etag.set(refreshed.nfo.etag);
                    saved_operator_chapters.set(refreshed.item.chapter_input);
                    if chapters.get_untracked() == submitted_chapters.get_untracked() {
                        chapters_dirty.set(false);
                    }
                    toast.push(tr(TextKey::Saved).to_string(), ToastSeverity::Success);
                }
                (Err(error), Ok(Some(refreshed))) => {
                    saved_metadata.set(refreshed.nfo.metadata);
                    etag.set(refreshed.nfo.etag);
                    saved_operator_chapters.set(refreshed.item.chapter_input);
                    toast.push(
                        save_failed(addon_media_list_error_message(error)),
                        ToastSeverity::Error,
                    );
                }
                (_, Ok(None)) => toast.push(
                    tr(TextKey::AddonMediaListNotFound).to_string(),
                    ToastSeverity::Error,
                ),
                (save_result, Err(refresh_error)) => toast.push(
                    save_failed(addon_media_list_error_message(
                        save_result.err().unwrap_or(refresh_error),
                    )),
                    ToastSeverity::Error,
                ),
            }
        }
    });
    Effect::new(move |_| {
        if let Some(result) = restore_action.value().get() {
            match result {
                Ok(()) => {
                    saved_operator_chapters.set(String::new());
                    chapters_dirty.set(false);
                    toast.push(tr(TextKey::Saved).to_string(), ToastSeverity::Success);
                }
                Err(error) => toast.push(
                    save_failed(addon_media_list_error_message(error)),
                    ToastSeverity::Error,
                ),
            }
        }
    });
    let build_metadata = move || {
        let mut metadata = saved_metadata.get_untracked();
        metadata.title = title.get_untracked();
        metadata.original_title = non_empty_value(&original_title.get_untracked());
        metadata.plot = non_empty_value(&plot.get_untracked());
        metadata.year = year.get_untracked().trim().parse().ok();
        metadata.release_date = non_empty_value(&release_date.get_untracked());
        metadata.season = season.get_untracked().trim().parse().ok();
        metadata.episode = episode.get_untracked().trim().parse().ok();
        metadata.genres = comma_values(&genres.get_untracked());
        metadata.tags = line_values(&tags.get_untracked());
        metadata.artists = comma_values(&artists.get_untracked());
        metadata.studios = comma_values(&studios.get_untracked());
        metadata.directors = comma_values(&directors.get_untracked());
        metadata.writers = comma_values(&writers.get_untracked());
        metadata.languages = comma_values(&languages.get_untracked());
        metadata.people = people_values(&people.get_untracked(), &metadata.people);
        metadata.artwork = artwork_values(
            &metadata.artwork,
            [
                (
                    &poster_url.get_untracked(),
                    AddonMediaListArtworkRole::Poster,
                ),
                (
                    &fanart_url.get_untracked(),
                    AddonMediaListArtworkRole::Fanart,
                ),
                (
                    &thumbnail_url.get_untracked(),
                    AddonMediaListArtworkRole::Thumb,
                ),
            ],
        );
        metadata
    };
    let native_etag = etag.get_untracked();
    view! {
        <div class=style::page>
            <div class=style::action_bar>
                <a class=style::icon_button href=format!("/media/addon-lists/{list_id}")>{tr(TextKey::Back)}</a>
                <button form="addon-list-item-review-form" type="submit" class=style::btn_add disabled=move || save_action.pending().get() on:click=move |event| {
                    event.prevent_default();
                    let chapter_update = (chapters_editable && chapters_dirty.get_untracked())
                        .then(|| chapters.get_untracked());
                    submitted_chapters.set(chapter_update.clone().unwrap_or_else(|| saved_operator_chapters.get_untracked()));
                    save_action.dispatch((build_metadata(), etag.get_untracked(), chapter_update));
                }>{tr(TextKey::SaveChanges)}</button>
            </div>
            <form
                id="addon-list-item-review-form"
                class=style::editor
                method="post"
                action="/admin-api/addon-lists/items/review"
            >
                <h1 class=style::page_title>{detail.item.display_title}</h1>
                <p class=style::draft_note role="status" style:display=if initial_status == "saved" { "flex" } else { "none" }>{tr(TextKey::Saved)}</p>
                <p class=style::review_error role="alert" style:display=if initial_error.is_empty() { "none" } else { "flex" }>{tr(TextKey::AddonMediaListSaveFailed)}</p>
                <input type="hidden" name="list_id" value=list_id.to_string()/>
                <input type="hidden" name="item_id" value=item_id.to_string()/>
                <input type="hidden" name="etag" value=native_etag prop:value=move || etag.get()/>
                <MetadataConflictSummary conflicts=metadata_conflicts/>
                <section class=style::general_metadata_panel data-testid="addon-item-metadata">
                    <label class=style::field_row><span class=style::field_label>{tr(TextKey::Title)}</span><input class=style::text_input name="title" value=title.get_untracked() prop:value=move || title.get() on:input=move |event| title.set(event_target_value(&event))/></label>
                    <label class=style::field_row><span class=style::field_label>{tr(TextKey::OriginalTitle)}</span><input class=style::text_input name="original_title" value=original_title.get_untracked() prop:value=move || original_title.get() on:input=move |event| original_title.set(event_target_value(&event))/></label>
                    <label class=style::field_row><span class=style::field_label>{tr(TextKey::Year)}</span><input class=style::text_input name="year" inputmode="numeric" value=year.get_untracked() prop:value=move || year.get() on:input=move |event| year.set(event_target_value(&event))/></label>
                    <label class=style::field_row><span class=style::field_label>{tr(TextKey::ReleaseDate)}</span><input class=style::text_input name="release_date" type="date" value=release_date.get_untracked() prop:value=move || release_date.get() on:input=move |event| release_date.set(event_target_value(&event))/></label>
                    <label class=style::field_row><span class=style::field_label>{tr(TextKey::Season)}</span><input class=style::text_input name="season" inputmode="numeric" value=season.get_untracked() prop:value=move || season.get() on:input=move |event| season.set(event_target_value(&event))/></label>
                    <label class=style::field_row><span class=style::field_label>{tr(TextKey::Episode)}</span><input class=style::text_input name="episode" inputmode="numeric" value=episode.get_untracked() prop:value=move || episode.get() on:input=move |event| episode.set(event_target_value(&event))/></label>
                    <label class=format!("{} {}", style::field_row, style::metadata_wide_row)><span class=style::field_label>{tr(TextKey::Summary)}</span><textarea class=style::text_input name="plot" rows="8" prop:value=move || plot.get() on:input=move |event| plot.set(event_target_value(&event))>{plot.get_untracked()}</textarea></label>
                    <label class=style::field_row><span class=style::field_label>{tr(TextKey::Genres)}</span><input class=style::text_input name="genres" value=genres.get_untracked() prop:value=move || genres.get() on:input=move |event| genres.set(event_target_value(&event))/></label>
                    <label class=format!("{} {}", style::field_row, style::general_tag_row)><span class=style::field_label>{tr(TextKey::Tags)}</span><textarea class=style::tag_input name="tags" rows="4" spellcheck="false" prop:value=move || tags.get() on:input=move |event| tags.set(event_target_value(&event))>{tags.get_untracked()}</textarea></label>
                    <label class=style::field_row><span class=style::field_label>{tr(TextKey::Artists)}</span><input class=style::text_input name="artists" value=artists.get_untracked() prop:value=move || artists.get() on:input=move |event| artists.set(event_target_value(&event))/></label>
                    <label class=style::field_row><span class=style::field_label>{tr(TextKey::Studios)}</span><input class=style::text_input name="studios" value=studios.get_untracked() prop:value=move || studios.get() on:input=move |event| studios.set(event_target_value(&event))/></label>
                    <label class=style::field_row><span class=style::field_label>{tr(TextKey::Directors)}</span><input class=style::text_input name="directors" value=directors.get_untracked() prop:value=move || directors.get() on:input=move |event| directors.set(event_target_value(&event))/></label>
                    <label class=style::field_row><span class=style::field_label>{tr(TextKey::Writers)}</span><input class=style::text_input name="writers" value=writers.get_untracked() prop:value=move || writers.get() on:input=move |event| writers.set(event_target_value(&event))/></label>
                    <label class=style::field_row><span class=style::field_label>{tr(TextKey::Languages)}</span><input class=style::text_input name="languages" value=languages.get_untracked() prop:value=move || languages.get() on:input=move |event| languages.set(event_target_value(&event))/></label>
                    <label class=format!("{} {}", style::field_row, style::metadata_wide_row)><span class=style::field_label>{tr(TextKey::CastAndCredits)}</span><span><textarea class=style::text_input name="people" rows="3" spellcheck="false" prop:value=move || people.get() on:input=move |event| people.set(event_target_value(&event))>{people.get_untracked()}</textarea><small class=style::field_hint>{tr(TextKey::OneCreditPerLine)}</small></span></label>
                    <div class=format!("{} {}", style::field_row, style::general_artwork_row)><div class=style::field_label>{tr(TextKey::Artwork)}</div><div class=style::general_artwork_fields>
                        <ArtworkRoleInput list_id=list_id item_id=Some(item_id) role=AddonMediaListArtworkRole::Poster label=TextKey::Poster url=poster_url etag=etag candidates=poster_candidates/>
                        <ArtworkRoleInput list_id=list_id item_id=Some(item_id) role=AddonMediaListArtworkRole::Fanart label=TextKey::FanArt url=fanart_url etag=etag candidates=fanart_candidates/>
                        <ArtworkRoleInput list_id=list_id item_id=Some(item_id) role=AddonMediaListArtworkRole::Thumb label=TextKey::Thumbnail url=thumbnail_url etag=etag candidates=thumbnail_candidates/>
                    </div></div>
                    <label class=format!("{} {}", style::field_row, style::metadata_wide_row)><span class=style::field_label>{tr(TextKey::ChapterInput)}</span><textarea class=style::text_input name="chapter_input" rows="8" disabled=!chapters_editable prop:value=move || chapters.get() on:input=move |event| {
                        chapters.set(event_target_value(&event));
                        chapters_dirty.set(true);
                    }>{chapters.get_untracked()}</textarea></label>
                    <button type="button" class=style::btn_cancel style:display=if provider_chapters.is_empty() || !chapters_editable { "none" } else { "inline-flex" } on:click=move |_| {
                        chapters.set(provider_chapters.clone());
                        restore_action.dispatch(());
                    }>{tr(TextKey::RestoreProviderChapters)}</button>
                </section>
            </form>
        </div>
    }
}

fn non_empty_value(value: &str) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.trim().to_owned())
}

fn comma_values(value: &str) -> Vec<String> {
    value.split(',').filter_map(non_empty_value).collect()
}

fn line_values(value: &str) -> Vec<String> {
    value.lines().filter_map(non_empty_value).collect()
}

fn people_lines(people: &[AddonMediaListNfoPerson]) -> String {
    people
        .iter()
        .map(|person| {
            person.role.as_deref().map_or_else(
                || person.name.clone(),
                |role| format!("{} | {role}", person.name),
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn people_values(
    value: &str,
    original: &[AddonMediaListNfoPerson],
) -> Vec<AddonMediaListNfoPerson> {
    let parsed = value
        .lines()
        .filter_map(|line| {
            let (name, role) = line
                .split_once('|')
                .map_or((line, None), |(name, role)| (name, non_empty_value(role)));
            non_empty_value(name).map(|name| AddonMediaListNfoPerson {
                name,
                role,
                ..AddonMediaListNfoPerson::default()
            })
        })
        .collect::<Vec<_>>();
    // Name/role are the only editable fields. An untouched input must not
    // discard provider order/thumbnails or falsely mark the draft as edited.
    if parsed
        .iter()
        .map(|person| (&person.name, &person.role))
        .eq(original.iter().map(|person| (&person.name, &person.role)))
    {
        original.to_vec()
    } else {
        parsed
    }
}

fn artwork_value(artwork: &[AddonMediaListArtwork], role: AddonMediaListArtworkRole) -> String {
    artwork
        .iter()
        .find(|candidate| candidate.role == role)
        .map(|candidate| candidate.url.clone())
        .unwrap_or_default()
}

fn artwork_candidates(
    artwork: &[AddonMediaListArtwork],
    role: AddonMediaListArtworkRole,
) -> Vec<String> {
    artwork
        .iter()
        .filter(|candidate| candidate.role == role)
        .map(|candidate| candidate.url.clone())
        .collect()
}

fn artwork_values(
    original: &[AddonMediaListArtwork],
    values: [(&str, AddonMediaListArtworkRole); 3],
) -> Vec<AddonMediaListArtwork> {
    let mut result = original
        .iter()
        .filter(|candidate| candidate.role == AddonMediaListArtworkRole::Other)
        .cloned()
        .collect::<Vec<_>>();
    for (url, role) in values {
        let Some(url) = non_empty_value(url) else {
            continue;
        };
        let mut candidate = original
            .iter()
            .find(|candidate| candidate.role == role)
            .cloned()
            .unwrap_or_default();
        candidate.url = url;
        candidate.role = role;
        result.push(candidate);
    }
    result
}

fn item_needs_review(
    item: &AddonMediaListItemRow,
    presentation: AddonMediaListPresentationKind,
) -> bool {
    item.availability != AddonMediaListItemAvailability::Available
        || item.metadata_conflict_count > 0
        || (item.replacement_media_version_id.is_some() && !item.replacement_valid)
        || item.title.trim().is_empty()
        || (item.kind == AddonMediaListItemKind::RemoteStream
            && (item.effective_liveness == RemoteStreamLiveness::Unknown || item.liveness_conflict))
        || ((item.content_kind == AddonMediaListContentKind::TelevisionEpisode
            || presentation == AddonMediaListPresentationKind::TelevisionShow)
            && !matches!(
                episode_number_pair(item.season, item.episode),
                EpisodeNumberPair::Numbered(_, _)
            ))
        || (presentation == AddonMediaListPresentationKind::Movies
            && item.collection_role == AddonMediaListItemRole::Auto)
}

fn bulk_scope_items(
    items: &[AddonMediaListItemRow],
    scope: &str,
    presentation: AddonMediaListPresentationKind,
    current_filter_needs_review: bool,
    page_start: usize,
    page_end: usize,
) -> Vec<AddonMediaListItemRow> {
    match scope {
        "current_page" => items
            .iter()
            .filter(|item| !current_filter_needs_review || item_needs_review(item, presentation))
            .skip(page_start)
            .take(page_end.saturating_sub(page_start))
            .cloned()
            .collect(),
        "needs_review" => items
            .iter()
            .filter(|item| item_needs_review(item, presentation))
            .cloned()
            .collect(),
        _ => items.to_vec(),
    }
}

#[cfg(test)]
fn episode_numbering_preview_values(
    items: &[AddonMediaListItemRow],
    start_item_id: Option<i64>,
    season: i32,
    starting_episode: i32,
    count: usize,
    replace_existing: bool,
) -> Vec<i32> {
    let assignments = episode_numbering_preview_assignments(
        items,
        start_item_id,
        season,
        starting_episode,
        count,
        replace_existing,
    );
    items
        .iter()
        .filter_map(|item| assignments.get(&item.id).map(|(_, episode)| *episode))
        .collect()
}

fn numbering_candidates(
    items: &[AddonMediaListItemRow],
    start_item_id: Option<i64>,
) -> Vec<&AddonMediaListItemRow> {
    let candidates = items
        .iter()
        .filter(|item| item.included && item.parent_provider_id.is_none())
        .collect::<Vec<_>>();
    let start = start_item_id
        .and_then(|item_id| candidates.iter().position(|item| item.id == item_id))
        .unwrap_or(0);
    candidates.into_iter().skip(start).collect()
}

fn episode_numbering_preview_assignments(
    items: &[AddonMediaListItemRow],
    start_item_id: Option<i64>,
    season: i32,
    starting_episode: i32,
    count: usize,
    replace_existing: bool,
) -> BTreeMap<i64, (i32, i32)> {
    let targets = numbering_candidates(items, start_item_id)
        .into_iter()
        .filter(|item| {
            replace_existing
                || item.season.is_none_or(|value| value < 0)
                || item.episode.is_none_or(|value| value < 1)
        })
        .take(count)
        .collect::<Vec<_>>();
    let target_ids = targets.iter().map(|item| item.id).collect::<BTreeSet<_>>();
    let mut occupied = BTreeSet::new();
    occupied.extend(
        items
            .iter()
            .filter(|item| item.included && item.parent_provider_id.is_none())
            .filter(|item| !target_ids.contains(&item.id))
            .filter(|item| item.season == Some(season))
            .filter_map(|item| item.episode.filter(|value| *value >= 1)),
    );
    let mut assignments = BTreeMap::new();
    let mut episode = starting_episode;
    for item in targets {
        while occupied.contains(&episode) {
            let Some(next) = episode.checked_add(1) else {
                return assignments;
            };
            episode = next;
        }
        assignments.insert(item.id, (season, episode));
        occupied.insert(episode);
        episode = episode.saturating_add(1);
    }
    assignments
}

fn parse_first_aired(value: Option<&str>) -> Option<chrono::NaiveDate> {
    let value = value?.trim();
    let date = value.get(..10).unwrap_or(value);
    chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()
}

/// Order numbering candidates oldest first. Providers such as channel upload
/// feeds list newest first, so provider rank alone would number the newest
/// item as episode 1. An undated item keeps its provider neighbour by taking
/// the date of the preceding dated item (or the first dated item when none
/// precedes it); ties keep provider order.
fn order_by_first_aired<'a>(
    items: impl IntoIterator<Item = &'a AddonMediaListItemRow>,
) -> Vec<&'a AddonMediaListItemRow> {
    let items = items.into_iter().collect::<Vec<_>>();
    let first_known = items
        .iter()
        .find_map(|item| parse_first_aired(item.first_aired.as_deref()));
    let mut known = first_known;
    let mut dated = items
        .into_iter()
        .map(|item| {
            if let Some(date) = parse_first_aired(item.first_aired.as_deref()) {
                known = Some(date);
            }
            (known, item)
        })
        .collect::<Vec<_>>();
    dated.sort_by_key(|(date, _)| *date);
    dated.into_iter().map(|(_, item)| item).collect()
}

fn auto_detect_episode_assignments(
    items: &[AddonMediaListItemRow],
    start_item_id: Option<i64>,
    starting_season: i32,
    minimum_gap_days: i64,
    count: usize,
) -> BTreeMap<i64, (i32, i32)> {
    let mut assignments = BTreeMap::new();
    let mut season = starting_season;
    let mut episode = 0_i32;
    let mut previous_known_date = None;
    for item in order_by_first_aired(
        numbering_candidates(items, start_item_id)
            .into_iter()
            .take(count),
    ) {
        let first_aired = parse_first_aired(item.first_aired.as_deref());
        if let (Some(previous), Some(current)) = (previous_known_date, first_aired) {
            if current.signed_duration_since(previous).num_days() >= minimum_gap_days {
                season = season.saturating_add(1);
                episode = 0;
            }
        }
        if first_aired.is_some() {
            previous_known_date = first_aired;
        }
        episode = episode.saturating_add(1);
        assignments.insert(item.id, (season, episode));
    }
    assignments
}

const fn item_section_heading(presentation: AddonMediaListPresentationKind) -> TextKey {
    match presentation {
        AddonMediaListPresentationKind::Movies => TextKey::MovieFilesAndExtras,
        AddonMediaListPresentationKind::TelevisionShow => TextKey::Episodes,
        AddonMediaListPresentationKind::MusicVideos => TextKey::MusicVideos,
        AddonMediaListPresentationKind::OtherVideos => TextKey::OtherVideos,
        AddonMediaListPresentationKind::Songs => TextKey::Songs,
        AddonMediaListPresentationKind::Images => TextKey::Images,
        AddonMediaListPresentationKind::RemoteStreams => TextKey::RemoteStreams,
        AddonMediaListPresentationKind::Auto => TextKey::Media,
    }
}

const fn item_section_help(presentation: AddonMediaListPresentationKind) -> TextKey {
    match presentation {
        AddonMediaListPresentationKind::Movies => TextKey::MovieFilesAndExtrasHelp,
        AddonMediaListPresentationKind::TelevisionShow => TextKey::EpisodesMetadataHelp,
        AddonMediaListPresentationKind::MusicVideos => TextKey::MusicVideosMetadataHelp,
        AddonMediaListPresentationKind::OtherVideos => TextKey::OtherVideosMetadataHelp,
        AddonMediaListPresentationKind::Songs => TextKey::SongsMetadataHelp,
        AddonMediaListPresentationKind::Images => TextKey::ImagesMetadataHelp,
        AddonMediaListPresentationKind::RemoteStreams => TextKey::RemoteStreamsMetadataHelp,
        AddonMediaListPresentationKind::Auto => TextKey::MetadataDraftNotice,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PublicationPrimaryAction {
    None,
    Save,
    Prepare,
    Publish,
    Queued,
    Publishing,
    Restoring,
    PublishBlocked,
}

fn publication_primary_action(
    settings_dirty: bool,
    readiness: &AddonMediaListPublicationReadiness,
) -> PublicationPrimaryAction {
    if readiness.saved_revision < 0 {
        PublicationPrimaryAction::None
    } else if settings_dirty {
        PublicationPrimaryAction::Save
    } else if let Some(publication) = readiness.publication.as_ref() {
        match publication.state {
            AddonMediaListPublicationState::Queued => PublicationPrimaryAction::Queued,
            AddonMediaListPublicationState::Publishing => PublicationPrimaryAction::Publishing,
            AddonMediaListPublicationState::Restoring => PublicationPrimaryAction::Restoring,
            AddonMediaListPublicationState::Waiting
            | AddonMediaListPublicationState::Expired
            | AddonMediaListPublicationState::Failed => PublicationPrimaryAction::PublishBlocked,
            AddonMediaListPublicationState::Complete => {
                publication_primary_action_without_job(readiness)
            }
        }
    } else if readiness.review_state == AddonMediaListReviewState::Publishing {
        PublicationPrimaryAction::PublishBlocked
    } else {
        publication_primary_action_without_job(readiness)
    }
}

fn publication_primary_action_without_job(
    readiness: &AddonMediaListPublicationReadiness,
) -> PublicationPrimaryAction {
    if !readiness.blockers.is_empty() {
        PublicationPrimaryAction::None
    } else if readiness.required_playback_detail_count > 0
        || readiness.required_prepared_media_count > 0
    {
        if publication_preparation_active(readiness) {
            PublicationPrimaryAction::None
        } else {
            PublicationPrimaryAction::Prepare
        }
    } else if readiness.review_state == AddonMediaListReviewState::Published {
        PublicationPrimaryAction::None
    } else {
        PublicationPrimaryAction::Publish
    }
}

fn publication_preparation_active(readiness: &AddonMediaListPublicationReadiness) -> bool {
    readiness.required_playback_detail_pending > 0 || readiness.required_prepared_media_pending > 0
}

fn prepared_media_status(readiness: RwSignal<AddonMediaListPublicationReadiness>) -> impl IntoView {
    view! {
        <p
            class=style::preparation_state
            data-prepared-media-status
            style:display=move || if readiness.with(|value| value.required_prepared_media_count > 0) { "flex" } else { "none" }
            role="status"
        >
            <span style=icon_style(IconName::Clock, "1rem") aria-hidden="true"></span>
            <span>{move || readiness.with(|value| format!("{}: {}", tr(TextKey::ProcessedMediaRequired), value.required_prepared_media_count))}</span>
        </p>
    }
}

const fn publication_phase_text_key(phase: AddonMediaListPublicationPhase) -> TextKey {
    match phase {
        AddonMediaListPublicationPhase::AwaitingLease => TextKey::PhaseAwaitingLease,
        AddonMediaListPublicationPhase::Staging => TextKey::PhaseStaging,
        AddonMediaListPublicationPhase::Scanning => TextKey::PhaseScanning,
        AddonMediaListPublicationPhase::Committing => TextKey::PhaseCommitting,
        AddonMediaListPublicationPhase::RestoringFiles => TextKey::PhaseRestoringFiles,
        AddonMediaListPublicationPhase::RestoringProjection => TextKey::PhaseRestoringProjection,
        AddonMediaListPublicationPhase::Reconciling => TextKey::PhaseReconciling,
        AddonMediaListPublicationPhase::Verifying => TextKey::PhaseVerifying,
        AddonMediaListPublicationPhase::CleaningUp => TextKey::PhaseCleaningUp,
        AddonMediaListPublicationPhase::Complete => TextKey::PhaseComplete,
    }
}

fn publication_readiness_marker(
    readiness: &AddonMediaListPublicationReadiness,
) -> Option<(i64, AddonMediaListPublicationState, String)> {
    readiness.publication.as_ref().map(|publication| {
        (
            publication.job_id,
            publication.state,
            publication.last_progress_at.clone(),
        )
    })
}

const fn publication_state_text_key(state: AddonMediaListPublicationState) -> TextKey {
    match state {
        AddonMediaListPublicationState::Queued => TextKey::PublicationQueued,
        AddonMediaListPublicationState::Waiting => TextKey::PhaseAwaitingLease,
        AddonMediaListPublicationState::Publishing => TextKey::Publishing,
        AddonMediaListPublicationState::Restoring => TextKey::RestoringPublication,
        AddonMediaListPublicationState::Expired => TextKey::PublicationExpired,
        AddonMediaListPublicationState::Failed => TextKey::Failed,
        AddonMediaListPublicationState::Complete => TextKey::Completed,
    }
}

const fn publication_outcome_text_key(outcome: AddonMediaListPublicationOutcome) -> TextKey {
    match outcome {
        AddonMediaListPublicationOutcome::Published => TextKey::OutcomePublished,
        AddonMediaListPublicationOutcome::RestoredAfterFailure => {
            TextKey::OutcomeRestoredAfterFailure
        }
    }
}

const fn publication_event_text_key(kind: AddonMediaListPublicationEventKind) -> TextKey {
    match kind {
        AddonMediaListPublicationEventKind::ExistingJobAdopted => TextKey::EventExistingJobAdopted,
        AddonMediaListPublicationEventKind::Enqueued => TextKey::EventEnqueued,
        AddonMediaListPublicationEventKind::LeaseWaitStarted => TextKey::EventLeaseWaitStarted,
        AddonMediaListPublicationEventKind::LeaseAcquired => TextKey::EventLeaseAcquired,
        AddonMediaListPublicationEventKind::LeaseReclaimed => TextKey::EventLeaseReclaimed,
        AddonMediaListPublicationEventKind::PhaseAdvanced => TextKey::EventPhaseAdvanced,
        AddonMediaListPublicationEventKind::ChildScanStarted => TextKey::EventChildScanStarted,
        AddonMediaListPublicationEventKind::ChildScanCompleted => TextKey::EventChildScanCompleted,
        AddonMediaListPublicationEventKind::ChildScanFailed => TextKey::EventChildScanFailed,
        AddonMediaListPublicationEventKind::OriginalFailure => TextKey::EventOriginalFailure,
        AddonMediaListPublicationEventKind::RecoveryFailure => TextKey::EventRecoveryFailure,
        AddonMediaListPublicationEventKind::RetryScheduled => TextKey::EventRetryScheduled,
        AddonMediaListPublicationEventKind::RecoveryRequested => TextKey::EventRecoveryRequested,
        AddonMediaListPublicationEventKind::Completed => TextKey::EventCompleted,
        AddonMediaListPublicationEventKind::HistoryTruncated => TextKey::EventHistoryTruncated,
    }
}

const fn publication_trigger_text_key(trigger: AddonMediaListPublicationEnqueueTrigger) -> TextKey {
    match trigger {
        AddonMediaListPublicationEnqueueTrigger::Operator => TextKey::TriggerOperator,
        AddonMediaListPublicationEnqueueTrigger::RestoreRevision => TextKey::TriggerRestoreRevision,
        AddonMediaListPublicationEnqueueTrigger::AutomaticRefresh => {
            TextKey::TriggerAutomaticRefresh
        }
    }
}

const fn publication_filesystem_transition_text_key(
    transition: AddonMediaListPublicationFilesystemTransition,
) -> TextKey {
    match transition {
        AddonMediaListPublicationFilesystemTransition::PreviousBackedUp => {
            TextKey::FilesystemPreviousBackedUp
        }
        AddonMediaListPublicationFilesystemTransition::CandidateCommitted => {
            TextKey::FilesystemCandidateCommitted
        }
        AddonMediaListPublicationFilesystemTransition::PreviousRestored => {
            TextKey::FilesystemPreviousRestored
        }
        AddonMediaListPublicationFilesystemTransition::ProjectionRestored => {
            TextKey::FilesystemProjectionRestored
        }
        AddonMediaListPublicationFilesystemTransition::RevisionVerified => {
            TextKey::FilesystemRevisionVerified
        }
        AddonMediaListPublicationFilesystemTransition::CleanupCompleted => {
            TextKey::FilesystemCleanupCompleted
        }
    }
}

const fn publication_retry_class_text_key(
    retry_class: AddonMediaListPublicationRetryClass,
) -> TextKey {
    match retry_class {
        AddonMediaListPublicationRetryClass::TransientRecovery => TextKey::RetryTransientRecovery,
    }
}

const fn publication_recovery_action_text_key(action: AddonMediaListRecoveryAction) -> TextKey {
    match action {
        AddonMediaListRecoveryAction::Retry => TextKey::RetryRecovery,
        AddonMediaListRecoveryAction::RestorePreviousRevision => TextKey::RestorePreviousRevision,
    }
}

const fn publication_error_category_text_key(
    category: AddonMediaListPublicationErrorCategory,
) -> TextKey {
    match category {
        AddonMediaListPublicationErrorCategory::FilesystemIo => TextKey::ErrorFilesystemIo,
        AddonMediaListPublicationErrorCategory::PermissionDenied => TextKey::ErrorPermissionDenied,
        AddonMediaListPublicationErrorCategory::SharingViolation => TextKey::ErrorSharingViolation,
        AddonMediaListPublicationErrorCategory::UnsafeFilesystemState => {
            TextKey::ErrorUnsafeFilesystemState
        }
        AddonMediaListPublicationErrorCategory::InvalidPublicationMarker => {
            TextKey::ErrorInvalidPublicationMarker
        }
        AddonMediaListPublicationErrorCategory::PublicationIntegrity => {
            TextKey::ErrorPublicationIntegrity
        }
    }
}

#[allow(clippy::unused_async, unused_variables)] // Clipboard I/O awaits only in the WASM build.
async fn copy_redacted_publication_diagnostics(
    history: AddonMediaListPublicationHistoryRow,
) -> Result<(), String> {
    let diagnostics =
        redacted_publication_diagnostics_json(&history).map_err(|error| error.to_string())?;
    #[cfg(target_arch = "wasm32")]
    {
        let window = web_sys::window().ok_or_else(|| "browser window is unavailable".to_owned())?;
        wasm_bindgen_futures::JsFuture::from(
            window.navigator().clipboard().write_text(&diagnostics),
        )
        .await
        .map_err(|_| "clipboard write was rejected".to_owned())?;
    }
    Ok(())
}

const fn auto_publication_hold_text_key(
    reason: AddonMediaListAutoPublicationHoldReason,
) -> TextKey {
    match reason {
        AddonMediaListAutoPublicationHoldReason::BaselineUnavailable => {
            TextKey::AutoPublicationBaselineUnavailable
        }
        AddonMediaListAutoPublicationHoldReason::ExistingChanges => {
            TextKey::AutoPublicationExistingChanges
        }
        AddonMediaListAutoPublicationHoldReason::Conflicts => TextKey::AutoPublicationConflicts,
        AddonMediaListAutoPublicationHoldReason::Removals => TextKey::AutoPublicationRemovals,
        AddonMediaListAutoPublicationHoldReason::Restricted => TextKey::AutoPublicationRestricted,
        AddonMediaListAutoPublicationHoldReason::Unavailable => TextKey::AutoPublicationUnavailable,
        AddonMediaListAutoPublicationHoldReason::OptionsChanged => {
            TextKey::AutoPublicationOptionsChanged
        }
        AddonMediaListAutoPublicationHoldReason::PreparationFailed => {
            TextKey::AutoPublicationPreparationFailed
        }
        AddonMediaListAutoPublicationHoldReason::ConcurrentChange => {
            TextKey::AutoPublicationConcurrentChange
        }
    }
}

#[component]
fn PublicationActivity(
    list_id: i64,
    history: RwSignal<AddonMediaListPublicationHistoryRow>,
    hold: RwSignal<Option<AddonMediaListAutoPublicationHoldRow>>,
) -> impl IntoView {
    let toast = use_toast();
    let copy_action = Action::new_local(|history: &AddonMediaListPublicationHistoryRow| {
        copy_redacted_publication_diagnostics(history.clone())
    });
    Effect::new(move |_| {
        if let Some(result) = copy_action.value().get() {
            match result {
                Ok(()) => toast.push(
                    tr(TextKey::RedactedDiagnosticsCopied).to_owned(),
                    ToastSeverity::Success,
                ),
                Err(_) => toast.push(tr(TextKey::CopyFailed).to_owned(), ToastSeverity::Error),
            }
        }
    });
    view! {
        <section
            id="addon-list-publication-activity"
            class=style::publication_activity
            aria-labelledby="addon-list-publication-activity-heading"
        >
            <span class=style::sr_only role="status" aria-live="polite" aria-atomic="true">
                {move || history.with(|history| {
                    let event_count = history.jobs.iter().map(|job| job.events.len()).sum::<usize>();
                    format!(
                        "{}: {} {}, {} {}",
                        tr(TextKey::PublicationActivity),
                        history.jobs.len(),
                        tr(TextKey::PublicationJobs),
                        event_count,
                        tr(TextKey::PublicationEvents),
                    )
                })}
            </span>
            <header class=style::publication_activity_header>
                <h2 id="addon-list-publication-activity-heading">{tr(TextKey::PublicationActivity)}</h2>
                <div class=style::publication_activity_actions>
                    <button
                        type="button"
                        class=style::btn_cancel
                        data-publication-diagnostics-copy
                        disabled=move || copy_action.pending().get()
                        on:click=move |_| {
                            if !copy_action.pending().get_untracked() {
                                copy_action.dispatch(history.get_untracked());
                            }
                        }
                    >
                        <span style=icon_style(IconName::Copy, "1rem") aria-hidden="true"></span>
                        <span>{tr(TextKey::CopyRedactedDiagnostics)}</span>
                    </button>
                    <noscript>
                        <style>"[data-publication-diagnostics-copy] { display: none !important; }"</style>
                        <a
                            class=style::btn_cancel
                            href=format!("/admin-api/addon-lists/{list_id}/publication-diagnostics")
                        >
                            <span style=icon_style(IconName::Download, "1rem") aria-hidden="true"></span>
                            <span>{tr(TextKey::DownloadRedactedDiagnostics)}</span>
                        </a>
                    </noscript>
                </div>
            </header>
            <p
                class=style::review_error
                role="status"
                data-testid="automatic-publication-hold"
                style:display=move || if hold.with(Option::is_some) { "block" } else { "none" }
            >
                {move || hold.with(|hold| hold.as_ref().map(|hold| format!(
                    "{}: {} ({})", tr(TextKey::AutoPublicationHeld),
                    tr(auto_publication_hold_text_key(hold.reason)), hold.recorded_at,
                )).unwrap_or_default())}
            </p>
            <p
                class=style::empty
                style:display=move || if history.with(|history| history.jobs.is_empty()) { "block" } else { "none" }
            >{tr(TextKey::NoPublicationActivity)}</p>
            <div
                class=style::publication_activity_jobs
                style:display=move || if history.with(|history| history.jobs.is_empty()) { "none" } else { "grid" }
            >
                <For
                    each=move || history.with(|history| {
                        history.jobs.iter().cloned().enumerate().collect::<Vec<_>>()
                    })
                    key=|(_, job)| job.job_id
                    children=move |(index, job)| view! {
                        <PublicationActivityJob job=job newest=index == 0 />
                    }
                />
            </div>
        </section>
    }
}

#[component]
fn PublicationActivityJob(
    job: AddonMediaListPublicationHistoryJobRow,
    newest: bool,
) -> impl IntoView {
    let state = publication_state_text_key(job.state);
    let outcome = job.outcome.map(publication_outcome_text_key);
    let summary_status = outcome.unwrap_or(state);
    let has_outcome = outcome.is_some();
    let job_events = job.events;
    view! {
        <details class=style::publication_activity_job open=newest>
            <summary class=style::publication_activity_summary>
                <strong>{format!("{} {}", tr(TextKey::PublicationGeneration), job.generation)}</strong>
                <span>{format!("{} {}", tr(TextKey::CorrelationId), job.operation_id)}</span>
                <span>{tr(summary_status)}</span>
            </summary>
            <dl class=style::publication_activity_job_meta>
                <div>
                    <dt>{tr(TextKey::SavedRevision)}</dt>
                    <dd>{job.saved_revision}</dd>
                </div>
                <div>
                    <dt>{tr(TextKey::TargetPublishedRevision)}</dt>
                    <dd>{job.target_published_revision}</dd>
                </div>
                <div>
                    <dt>{tr(TextKey::PublicationPhase)}</dt>
                    <dd>{tr(publication_phase_text_key(job.phase))}</dd>
                </div>
                <div>
                    <dt>{tr(TextKey::Status)}</dt>
                    <dd>{tr(state)}</dd>
                </div>
                <div style:display=if has_outcome { "block" } else { "none" }>
                    <dt>{tr(TextKey::PublicationOutcome)}</dt>
                    <dd>{outcome.map(tr).unwrap_or_default()}</dd>
                </div>
            </dl>
            <ol class=style::publication_activity_events>
                <For
                    each=move || job_events.clone()
                    key=|event| event.sequence
                    children=move |event| view! { <PublicationActivityEvent event=event /> }
                />
            </ol>
        </details>
    }
}

#[component]
fn PublicationActivityEvent(event: AddonMediaListPublicationEventRow) -> impl IntoView {
    let occurred_at = event.occurred_at.clone();
    let display_time =
        format_progress_timestamp(&occurred_at).unwrap_or_else(|| occurred_at.clone());
    let phase = event
        .phase
        .map(|phase| tr(publication_phase_text_key(phase)))
        .unwrap_or_default();
    let trigger = event
        .enqueue_trigger
        .map(|trigger| tr(publication_trigger_text_key(trigger)))
        .unwrap_or_default();
    let filesystem_transition = event
        .filesystem_transition
        .map(|transition| tr(publication_filesystem_transition_text_key(transition)))
        .unwrap_or_default();
    let retry_class = event
        .retry_class
        .map(|retry_class| tr(publication_retry_class_text_key(retry_class)))
        .unwrap_or_default();
    let recovery_action = event
        .recovery_action
        .map(|action| tr(publication_recovery_action_text_key(action)))
        .unwrap_or_default();
    let error_category = event
        .error_category
        .map(|category| tr(publication_error_category_text_key(category)))
        .unwrap_or_default();
    let outcome = event
        .outcome
        .map(|outcome| tr(publication_outcome_text_key(outcome)))
        .unwrap_or_default();
    let has_phase = event.phase.is_some();
    let has_trigger = event.enqueue_trigger.is_some();
    let has_attempt = event.attempt.is_some();
    let has_related_operation = event.related_operation_id.is_some();
    let has_filesystem_transition = event.filesystem_transition.is_some();
    let has_retry_class = event.retry_class.is_some();
    let has_recovery_action = event.recovery_action.is_some();
    let has_error_category = event.error_category.is_some();
    let has_os_error_code = event.os_error_code.is_some();
    let has_published_count = event.published_item_count.is_some();
    let has_playlist_count = event.playlist_item_count.is_some();
    let has_artwork_count = event.artwork_count.is_some();
    let has_outcome = event.outcome.is_some();
    view! {
        <li class=style::publication_activity_event>
            <time datetime=occurred_at>{display_time}</time>
            <div>
                <strong>{tr(publication_event_text_key(event.kind))}</strong>
                <span class=style::publication_activity_event_details>
                    <span style:display=if has_trigger { "inline" } else { "none" }>{trigger}</span>
                    <span style:display=if has_phase { "inline" } else { "none" }>{format!("{}: {phase}", tr(TextKey::PublicationPhase))}</span>
                    <span style:display=if has_attempt { "inline" } else { "none" }>{format!("{}: {}", tr(TextKey::PublicationAttempts), event.attempt.unwrap_or_default())}</span>
                    <span style:display=if has_related_operation { "inline" } else { "none" }>{format!("{}: {}", tr(TextKey::RelatedOperation), event.related_operation_id.unwrap_or_default())}</span>
                    <span style:display=if has_filesystem_transition { "inline" } else { "none" }>{format!("{}: {filesystem_transition}", tr(TextKey::FilesystemTransition))}</span>
                    <span style:display=if has_retry_class { "inline" } else { "none" }>{format!("{}: {retry_class}", tr(TextKey::RetryClass))}</span>
                    <span style:display=if has_recovery_action { "inline" } else { "none" }>{format!("{}: {recovery_action}", tr(TextKey::RecoveryAction))}</span>
                    <span style:display=if has_error_category { "inline" } else { "none" }>{format!("{}: {error_category}", tr(TextKey::FailureReason))}</span>
                    <span style:display=if has_os_error_code { "inline" } else { "none" }>{format!("{}: {}", tr(TextKey::OperatingSystemErrorCode), event.os_error_code.unwrap_or_default())}</span>
                    <span style:display=if has_published_count { "inline" } else { "none" }>{format!("{}: {}", tr(TextKey::Items), event.published_item_count.unwrap_or_default())}</span>
                    <span style:display=if has_playlist_count { "inline" } else { "none" }>{format!("{}: {}", tr(TextKey::PlaylistItems), event.playlist_item_count.unwrap_or_default())}</span>
                    <span style:display=if has_artwork_count { "inline" } else { "none" }>{format!("{}: {}", tr(TextKey::Artwork), event.artwork_count.unwrap_or_default())}</span>
                    <span style:display=if has_outcome { "inline" } else { "none" }>{format!("{}: {outcome}", tr(TextKey::PublicationOutcome))}</span>
                </span>
            </div>
        </li>
    }
}

fn publication_recovery_view(
    list_id: i64,
    publication: AddonMediaListPublicationStatus,
    recovery_action: PublicationRecoveryAction,
) -> AnyView {
    let job_id = publication.job_id;
    let generation = publication.generation;
    let can_retry = publication.can_retry;
    let can_restore = publication.can_restore;
    let phase = tr(publication_phase_text_key(publication.phase));
    let diagnostic = publication
        .diagnostic_code
        .unwrap_or_else(|| tr(TextKey::BlockerRecoveryRequired).to_owned());
    view! {
        <aside class=style::recovery_alert role="alert" data-testid="addon-publication-recovery">
            <strong>{tr(TextKey::PublishBlocked)}</strong>
            <dl>
                <div><dt>{tr(TextKey::PublicationPhase)}</dt><dd>{phase}</dd></div>
                <div><dt>{tr(TextKey::PublicationAttempts)}</dt><dd>{publication.attempt_count}</dd></div>
                <div><dt>{tr(TextKey::LastProgress)}</dt><dd>{publication.last_progress_at}</dd></div>
                <div><dt>{tr(TextKey::FailureReason)}</dt><dd>{diagnostic}</dd></div>
            </dl>
            <div class=style::recovery_actions>
                <form method="post" action="/admin-api/addon-lists/recover" style:display=if can_retry { "block" } else { "none" }>
                    <input type="hidden" name="list_id" value=list_id/>
                    <input type="hidden" name="job_id" value=job_id/>
                    <input type="hidden" name="generation" value=generation/>
                    <input type="hidden" name="action" value="retry"/>
                    <button
                        type="submit"
                        class=style::btn_add
                        disabled=move || recovery_action.pending().get()
                        on:click=move |event| {
                            event.prevent_default();
                            if !recovery_action.pending().get_untracked() {
                                recovery_action.dispatch((
                                    list_id,
                                    job_id,
                                    generation,
                                    AddonMediaListRecoveryAction::Retry,
                                ));
                            }
                        }
                    >{tr(TextKey::RetryRecovery)}</button>
                </form>
                <form method="post" action="/admin-api/addon-lists/recover" style:display=if can_restore { "block" } else { "none" }>
                    <input type="hidden" name="list_id" value=list_id/>
                    <input type="hidden" name="job_id" value=job_id/>
                    <input type="hidden" name="generation" value=generation/>
                    <input type="hidden" name="action" value="restore"/>
                    <button
                        type="submit"
                        class=style::btn_cancel
                        disabled=move || recovery_action.pending().get()
                        on:click=move |event| {
                            event.prevent_default();
                            if !recovery_action.pending().get_untracked() {
                                recovery_action.dispatch((
                                    list_id,
                                    job_id,
                                    generation,
                                    AddonMediaListRecoveryAction::RestorePreviousRevision,
                                ));
                            }
                        }
                    >{tr(TextKey::RestorePreviousRevision)}</button>
                </form>
            </div>
        </aside>
    }
    .into_any()
}

const fn publication_blocker_text_key(kind: AddonMediaListPublicationBlockerKind) -> TextKey {
    match kind {
        AddonMediaListPublicationBlockerKind::SelectionReviewRequired => {
            TextKey::AddonSelectionReviewRequired
        }
        AddonMediaListPublicationBlockerKind::OverviewIncomplete => {
            TextKey::BlockerOverviewIncomplete
        }
        AddonMediaListPublicationBlockerKind::PublicationInProgress => {
            TextKey::BlockerPublicationInProgress
        }
        AddonMediaListPublicationBlockerKind::RecoveryRequired => TextKey::BlockerRecoveryRequired,
        AddonMediaListPublicationBlockerKind::CollectionTitleMissing => {
            TextKey::BlockerCollectionTitleMissing
        }
        AddonMediaListPublicationBlockerKind::InvalidArtwork => TextKey::BlockerInvalidArtwork,
        AddonMediaListPublicationBlockerKind::NoIncludedItems => TextKey::BlockerNoIncludedItems,
        AddonMediaListPublicationBlockerKind::MoviePrimaryCount => {
            TextKey::BlockerMoviePrimaryCount
        }
        AddonMediaListPublicationBlockerKind::PlaybackDecisionRequired => {
            TextKey::BlockerPlaybackDecisionRequired
        }
        AddonMediaListPublicationBlockerKind::PlaybackDetailsUnavailable => {
            TextKey::BlockerPlaybackDetailsUnavailable
        }
        AddonMediaListPublicationBlockerKind::PlaybackDetailsFailed => {
            TextKey::BlockerPlaybackDetailsFailed
        }
        AddonMediaListPublicationBlockerKind::UnavailableWithoutReplacement => {
            TextKey::BlockerUnavailableWithoutReplacement
        }
        AddonMediaListPublicationBlockerKind::ItemTitleMissing => TextKey::BlockerItemTitleMissing,
        AddonMediaListPublicationBlockerKind::SeasonEpisodeMissing => {
            TextKey::BlockerSeasonEpisodeMissing
        }
        AddonMediaListPublicationBlockerKind::ArtistMissing => TextKey::BlockerArtistMissing,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PublicationBlockerGroup {
    kind: AddonMediaListPublicationBlockerKind,
    item_count: u64,
    first_item_title: Option<String>,
    item_scoped: bool,
}

fn publication_blocker_groups(
    blockers: &[AddonMediaListPublicationBlocker],
) -> Vec<PublicationBlockerGroup> {
    let mut groups = Vec::<PublicationBlockerGroup>::new();
    for blocker in blockers {
        let item_scoped = blocker.item_rank.is_some();
        if let Some(group) = groups
            .iter_mut()
            .find(|group| group.kind == blocker.kind && group.item_scoped == item_scoped)
        {
            group.item_count = group.item_count.saturating_add(u64::from(item_scoped));
        } else {
            groups.push(PublicationBlockerGroup {
                kind: blocker.kind,
                item_count: u64::from(item_scoped),
                first_item_title: item_scoped.then(|| blocker.item_title.clone()).flatten(),
                item_scoped,
            });
        }
    }
    groups
}

fn publication_blocker_group_label(group: &PublicationBlockerGroup) -> String {
    let message = tr(publication_blocker_text_key(group.kind));
    if group.item_count > 1 {
        format!("{} {} · {message}", group.item_count, tr(TextKey::Items))
    } else if let Some(title) = group.first_item_title.as_deref() {
        format!("{title}: {message}")
    } else {
        message.to_owned()
    }
}

const fn publication_blocker_href(kind: AddonMediaListPublicationBlockerKind) -> &'static str {
    match kind {
        AddonMediaListPublicationBlockerKind::CollectionTitleMissing
        | AddonMediaListPublicationBlockerKind::InvalidArtwork => "#addon-list-general-metadata",
        AddonMediaListPublicationBlockerKind::SelectionReviewRequired
        | AddonMediaListPublicationBlockerKind::OverviewIncomplete
        | AddonMediaListPublicationBlockerKind::PublicationInProgress
        | AddonMediaListPublicationBlockerKind::RecoveryRequired => "#addon-list-import-progress",
        AddonMediaListPublicationBlockerKind::NoIncludedItems
        | AddonMediaListPublicationBlockerKind::MoviePrimaryCount
        | AddonMediaListPublicationBlockerKind::PlaybackDecisionRequired
        | AddonMediaListPublicationBlockerKind::PlaybackDetailsUnavailable
        | AddonMediaListPublicationBlockerKind::PlaybackDetailsFailed
        | AddonMediaListPublicationBlockerKind::UnavailableWithoutReplacement
        | AddonMediaListPublicationBlockerKind::ItemTitleMissing
        | AddonMediaListPublicationBlockerKind::SeasonEpisodeMissing
        | AddonMediaListPublicationBlockerKind::ArtistMissing => "#addon-list-items-heading",
    }
}

#[island]
fn AddonMediaListEditorIsland(
    id: i64,
    options_available: bool,
    initial_error: String,
    initial_status: String,
    initial_form: Option<AddonMediaListForm>,
    initial_items: Vec<AddonMediaListItemRow>,
    initial_readiness: AddonMediaListPublicationReadiness,
    initial_history: AddonMediaListPublicationHistoryRow,
    initial_show_nfo: AddonMediaListNfoForm,
    providers: Vec<AddonMediaListProviderRow>,
    initial_source_probe: Option<AddonMediaListSourceProbeRow>,
    initial_draft_diff: AddonMediaListDraftDiffRow,
    initial_reset_impact: AddonMediaListResetImpactRow,
    initial_revisions: Vec<AddonMediaListRevisionSummaryRow>,
    initial_revision_diff: Option<AddonMediaListRevisionDiffRow>,
    initial_compare_revisions: Option<(i32, i32)>,
    initial_review_category: String,
) -> impl IntoView {
    let initial_error_key = match initial_error.as_str() {
        "unsaved" => Some(TextKey::PublishUnsavedChanges),
        "blocked" => Some(TextKey::PublicationBlocked),
        "recovery" => Some(TextKey::RecoveryFailed),
        "reset" => Some(TextKey::ResetAddonMediaListFailed),
        "reconcile" => Some(TextKey::ReconcileFailed),
        "removal-confirmation" => Some(TextKey::RemovalConfirmationFailed),
        "source-test" => Some(TextKey::SourceTestFailed),
        "revision-restore" => Some(TextKey::RestoreRevisionFailed),
        "" => None,
        _ => Some(TextKey::AddonMediaListSaveFailed),
    };
    let initial_status_key = match initial_status.as_str() {
        "saved" => Some(TextKey::DraftSaved),
        "published" => Some(TextKey::Published),
        "publishing" => Some(TextKey::Publishing),
        "preparing" => Some(TextKey::PreparingPublication),
        "preparing-media" => Some(TextKey::PreparingProcessedMedia),
        "recovery-queued" => Some(TextKey::RecoveryQueued),
        "reset-queued" => Some(TextKey::ResetQueued),
        "reconcile-queued" => Some(TextKey::ReconcileQueued),
        "removal-confirmed" => Some(TextKey::RemovalConfirmed),
        "source-test-queued" => Some(TextKey::SourceTestQueued),
        "revision-restore-queued" => Some(TextKey::RestoreRevisionQueued),
        _ => None,
    };
    let form = initial_form.unwrap_or_default();
    let form_found = form.id != 0 || id == 0;
    let form_id = RwSignal::new(form.id);
    let list_name = StoredValue::new(form.name.clone());
    let addon_id = form.addon_id.clone();
    let source_url = form.source_url.clone();
    let source_host = StoredValue::new(
        redacted_source_host(&source_url).unwrap_or_else(|| tr(TextKey::Unavailable).to_owned()),
    );
    let submit_addon_id = addon_id.clone();
    let submit_source_url = source_url.clone();
    let refresh_cadence = RwSignal::new(form.refresh_cadence);
    let delivery_mode = RwSignal::new(form.delivery_mode);
    let saved_delivery_mode = form.delivery_mode;
    let delivery_cutoff_at = form.delivery_cutoff_at.clone();
    let refresh_publication_policy =
        RwSignal::new(AddonMediaListRefreshPublicationPolicy::Automatic);
    let saved_refresh_publication_policy = form.refresh_publication_policy;
    let generate_playlist = RwSignal::new(form.generate_playlist);
    let detail_import_policy = RwSignal::new(form.detail_import_policy);
    let publication_readiness = RwSignal::new(initial_readiness);
    let publication_history = RwSignal::new(initial_history);
    let auto_publication_hold = RwSignal::new(form.auto_publication_hold.clone());
    let source_probe = RwSignal::new(initial_source_probe);
    let draft_diff = RwSignal::new(initial_draft_diff);
    let active_draft_category = draft_review_category(&initial_review_category);
    let default_compare_revisions = initial_compare_revisions.or_else(|| {
        initial_revisions
            .first()
            .zip(initial_revisions.get(1))
            .map(|(left, right)| (left.revision, right.revision))
    });
    let revision_history = RwSignal::new(initial_revisions);
    let revision_diff = RwSignal::new(initial_revision_diff);
    let compare_left = RwSignal::new(default_compare_revisions.map(|value| value.0));
    let compare_right = RwSignal::new(default_compare_revisions.map(|value| value.1));
    let reset_impact = RwSignal::new(initial_reset_impact);
    let import_progress = RwSignal::new(form.import_progress.clone());
    let published_revision = RwSignal::new(form.published_revision);
    let poll_epoch = RwSignal::new(0_u64);
    let poll_running = RwSignal::new(false);
    let progress_rate_sample = RwSignal::new(None::<(bool, u64, f64)>);
    let progress_live_summary = RwSignal::new(
        form.import_progress
            .as_ref()
            .map(|progress| {
                format!(
                    "{}: {}",
                    tr(import_progress_phase(
                        progress,
                        form.published_revision > 0,
                        form.detail_import_policy,
                    )),
                    progress.discovered_count
                )
            })
            .unwrap_or_default(),
    );
    let content_kind_override = RwSignal::new(form.content_kind_override);
    let legacy_chapter_input = form.legacy_chapter_input.clone();
    let presentation_kind = RwSignal::new(form.requested_presentation_kind);
    let published_presentation_kind = form.presentation_kind;
    let has_published_projection = form.published_revision > 0;
    let description_expanded = RwSignal::new(false);
    let is_draft = form.review_state != AddonMediaListReviewState::Published;
    let show_metadata = initial_show_nfo.metadata.clone();
    let saved_show_metadata = RwSignal::new(show_metadata.clone());
    let show_metadata_conflicts = initial_show_nfo.conflicts.clone();
    let show_metadata_conflict_count = show_metadata_conflicts.len();
    let show_etag = RwSignal::new(initial_show_nfo.etag.clone());
    let show_title = RwSignal::new(show_metadata.title.clone());
    let show_original_title =
        RwSignal::new(show_metadata.original_title.clone().unwrap_or_default());
    let show_sort_title = RwSignal::new(show_metadata.sort_title.clone().unwrap_or_default());
    let show_plot = RwSignal::new(show_metadata.plot.clone().unwrap_or_default());
    let show_year = RwSignal::new(
        show_metadata
            .year
            .map_or_else(String::new, |value| value.to_string()),
    );
    let show_genres = RwSignal::new(show_metadata.genres.join(", "));
    let show_tags = RwSignal::new(show_metadata.tags.join("\n"));
    let show_studios = RwSignal::new(show_metadata.studios.join(", "));
    let show_directors = RwSignal::new(show_metadata.directors.join(", "));
    let show_writers = RwSignal::new(show_metadata.writers.join(", "));
    let show_artists = RwSignal::new(show_metadata.artists.join(", "));
    let show_languages = RwSignal::new(show_metadata.languages.join(", "));
    let show_people = RwSignal::new(people_lines(&show_metadata.people));
    let show_poster = RwSignal::new(artwork_value(
        &show_metadata.artwork,
        AddonMediaListArtworkRole::Poster,
    ));
    let show_fanart = RwSignal::new(artwork_value(
        &show_metadata.artwork,
        AddonMediaListArtworkRole::Fanart,
    ));
    let show_thumbnail = RwSignal::new(artwork_value(
        &show_metadata.artwork,
        AddonMediaListArtworkRole::Thumb,
    ));
    let show_poster_candidates =
        artwork_candidates(&show_metadata.artwork, AddonMediaListArtworkRole::Poster);
    let show_fanart_candidates =
        artwork_candidates(&show_metadata.artwork, AddonMediaListArtworkRole::Fanart);
    let show_thumbnail_candidates =
        artwork_candidates(&show_metadata.artwork, AddonMediaListArtworkRole::Thumb);
    let presentation_error = form
        .presentation_error
        .as_deref()
        .map(addon_media_list_error_message)
        .unwrap_or_default();
    let has_presentation_error = !presentation_error.is_empty();
    let review_error = form.review_error.clone().unwrap_or_default();
    let has_review_error = !review_error.is_empty();
    let display_id = form.source_url.clone();
    let display_folder = form.folder_name.clone();
    let form_addon_id = form.addon_id.clone();
    let supports_download_ahead = providers
        .iter()
        .any(|provider| provider.addon_id == form.addon_id && provider.supports_download_ahead);
    let supports_adaptive_refresh = providers
        .iter()
        .any(|provider| provider.addon_id == form.addon_id && provider.supports_adaptive_refresh);
    let last_refresh_succeeded_at = form
        .last_refresh_succeeded_at
        .as_deref()
        .and_then(format_progress_timestamp)
        .unwrap_or_else(|| tr(TextKey::NeverRefreshed).to_owned());
    let next_refresh_at = form
        .next_refresh_at
        .as_deref()
        .and_then(format_progress_timestamp)
        .unwrap_or_else(|| tr(TextKey::NotAvailable).to_owned());
    let refresh_retry_at = form
        .refresh_retry_at
        .as_deref()
        .and_then(format_progress_timestamp)
        .unwrap_or_else(|| tr(TextKey::NotAvailable).to_owned());
    let detected_provider_name = providers
        .iter()
        .find(|provider| provider.addon_id == form.addon_id)
        .map_or_else(|| form.addon_id.clone(), |provider| provider.name.clone());
    // Changing the add-on re-imports the list through the new provider, so the
    // operator confirms before item identity is re-derived.
    let selected_addon_id = RwSignal::new(form.addon_id.clone());
    let change_addon_open = RwSignal::new(false);
    let apply_content_kind_open = RwSignal::new(false);
    let reset_open = RwSignal::new(false);
    let reset_status_visible = RwSignal::new(initial_status == "reset-queued");
    let reconcile_status_visible = RwSignal::new(initial_status == "reconcile-queued");
    let restore_open = RwSignal::new(false);
    let restore_revision = RwSignal::new(None::<i32>);
    let reset_confirmed_name = RwSignal::new(String::new());
    let reset_confirmation_valid = Memo::new(move |_| {
        reset_confirmed_name
            .with(|confirmed| reset_confirmation_matches(confirmed, &list_name.get_value()))
    });
    let change_addon_action =
        Action::new(move |addon_id: &String| change_addon_media_list_addon(id, addon_id.clone()));
    // The list value is a bulk default rather than the per-item truth, so it is
    // only pushed onto entries when the operator asks for it.
    let apply_content_kind_action = Action::new(|value: &(i64, AddonMediaListContentKind)| {
        let (id, content_kind) = *value;
        async move { apply_addon_media_list_content_kind(id, content_kind).await }
    });
    let reset_action = Action::new(|value: &(i64, String)| {
        let (id, confirmed_name) = value.clone();
        async move { reset_addon_media_list(id, confirmed_name).await }
    });
    let reconcile_action = Action::new(|id: &i64| reconcile_addon_media_list(*id));
    let confirm_removal_action = Action::new(|value: &(i64, i64, i32)| {
        let (list_id, item_id, expected_generation) = *value;
        async move { confirm_addon_media_list_removal(list_id, item_id, expected_generation).await }
    });
    let source_test_action = Action::new(|value: &(i64, i32)| {
        let (id, saved_revision) = *value;
        async move { test_addon_media_list_source(id, saved_revision).await }
    });
    let restore_revision_action = Action::new(|value: &(i64, i32, i32, i32)| {
        let (id, source_revision, draft_revision, published_revision) = *value;
        async move {
            restore_addon_media_list_revision(
                id,
                source_revision,
                draft_revision,
                published_revision,
            )
            .await
        }
    });
    let compare_revision_action = Action::new(|value: &(i64, i32, i32)| {
        let (id, left_revision, right_revision) = *value;
        async move { compare_addon_media_list_revisions(id, left_revision, right_revision).await }
    });
    let episode_numbering_open = RwSignal::new(false);
    let episode_numbering_season = RwSignal::new("1".to_owned());
    let episode_numbering_start = RwSignal::new("1".to_owned());
    let episode_numbering_count = RwSignal::new(String::new());
    let replace_existing_episode_numbers = RwSignal::new(false);
    let episode_numbering_start_item_id = RwSignal::new(None::<i64>);
    let episode_numbering_auto_detect = RwSignal::new(false);
    let episode_numbering_minimum_gap_days = RwSignal::new("90".to_owned());
    let checked_generate_playlist = form.generate_playlist;
    let replacement_target = RwSignal::new(None::<i64>);
    let replacement_query = RwSignal::new(String::new());
    let replacement_results = RwSignal::new(Vec::<AddonMediaListReplacementRow>::new());
    let saved_refresh_cadence = form.refresh_cadence;
    let saved_generate_playlist = form.generate_playlist;
    let saved_detail_import_policy = form.detail_import_policy;
    let saved_content_kind_override = form.content_kind_override;
    let saved_presentation_kind = form.requested_presentation_kind;
    let metadata_dirty = Memo::new(move |_| {
        saved_show_metadata.with(|saved| {
            show_title.get() != saved.title
                || non_empty_value(&show_original_title.get()) != saved.original_title
                || non_empty_value(&show_sort_title.get()) != saved.sort_title
                || non_empty_value(&show_plot.get()) != saved.plot
                || show_year.get().trim().parse::<i32>().ok() != saved.year
                || comma_values(&show_genres.get()) != saved.genres
                || line_values(&show_tags.get()) != saved.tags
                || comma_values(&show_studios.get()) != saved.studios
                || comma_values(&show_directors.get()) != saved.directors
                || comma_values(&show_writers.get()) != saved.writers
                || comma_values(&show_artists.get()) != saved.artists
                || comma_values(&show_languages.get()) != saved.languages
                || people_values(&show_people.get(), &saved.people) != saved.people
                || artwork_values(
                    &saved.artwork,
                    [
                        (&show_poster.get(), AddonMediaListArtworkRole::Poster),
                        (&show_fanart.get(), AddonMediaListArtworkRole::Fanart),
                        (&show_thumbnail.get(), AddonMediaListArtworkRole::Thumb),
                    ],
                ) != saved.artwork
        })
    });
    let settings_dirty = Memo::new(move |_| {
        refresh_cadence.get() != saved_refresh_cadence
            || delivery_mode.get() != saved_delivery_mode
            || refresh_publication_policy.get() != saved_refresh_publication_policy
            || generate_playlist.get() != saved_generate_playlist
            || detail_import_policy.get() != saved_detail_import_policy
            || content_kind_override.get() != saved_content_kind_override
            || presentation_kind.get() != saved_presentation_kind
            || metadata_dirty.get()
    });
    let primary_action = Memo::new(move |_| {
        publication_readiness
            .with(|readiness| publication_primary_action(settings_dirty.get(), readiness))
    });
    let import_step_active = Memo::new(move |_| {
        import_progress.with(|progress| {
            progress.as_ref().is_some_and(|progress| {
                progress.state != AddonMediaListImportState::Ready && published_revision.get() == 0
            })
        })
    });
    let publish_step_active = Memo::new(move |_| {
        publication_readiness.with(|readiness| {
            readiness.review_state == AddonMediaListReviewState::Publishing
                || publication_preparation_active(readiness)
        })
    });
    let published_step_complete = Memo::new(move |_| {
        publication_readiness
            .with(|readiness| readiness.review_state == AddonMediaListReviewState::Published)
    });

    let save_action = Action::new(
        |value: &(
            AddonMediaListForm,
            AddonMediaListPresentationKind,
            AddonMediaListNfoMetadata,
            String,
        )| {
            let (form, presentation_kind, show_metadata, show_etag) = value.clone();
            async move {
                let id = form.id;
                update_addon_media_list(form).await?;
                save_addon_media_list_show_nfo(id, show_metadata, show_etag).await?;
                request_addon_media_list_presentation(id, presentation_kind, None).await
            }
        },
    );
    let search_action = Action::new(|query: &String| {
        let query = query.clone();
        async move { search_addon_media_list_replacements(query).await }
    });
    let replacement_action = Action::new(|value: &(i64, Option<(i64, i64)>)| {
        let (item_id, replacement) = *value;
        async move {
            let (media_item_id, media_version_id) =
                replacement.map_or((None, None), |(item, version)| (Some(item), Some(version)));
            set_addon_media_list_item_replacement(item_id, media_item_id, media_version_id).await
        }
    });

    let toast = use_toast();
    // Keep one provider-ordered overview for every collection type. Availability
    // is a review status, not a separate navigation hierarchy.
    let items = RwSignal::new(initial_items);
    let episode_numbering_available = Memo::new(move |_| {
        presentation_kind.get() == AddonMediaListPresentationKind::TelevisionShow
            || items.with(|items| {
                items
                    .iter()
                    .any(|item| item.content_kind == AddonMediaListContentKind::TelevisionEpisode)
            })
    });
    let row_save_states = RwSignal::new(BTreeMap::<i64, RowSaveState>::new());
    let bulk_scope_value = RwSignal::new("all".to_owned());
    let bulk_content_kind_value = RwSignal::new("unchanged".to_owned());
    let bulk_liveness_value = RwSignal::new("unchanged".to_owned());
    let last_review_undo = RwSignal::new(None::<ReviewUndoEntry>);
    let need_review_filter = RwSignal::new(false);
    let item_page = RwSignal::new(0_u32);
    let item_page_size = RwSignal::new(DEFAULT_ITEM_PAGE_SIZE);
    let need_review_count = Memo::new(move |_| {
        items.with(|items| {
            items
                .iter()
                .filter(|item| item_needs_review(item, presentation_kind.get()))
                .count()
        })
    });
    let filtered_item_count = Memo::new(move |_| {
        items.with(|items| {
            draft_diff.with(|diff| {
                items
                    .iter()
                    .filter(|item| {
                        item_matches_draft_category(item.id, active_draft_category, diff)
                            && (!need_review_filter.get()
                                || item_needs_review(item, presentation_kind.get()))
                    })
                    .count()
            })
        })
    });
    let episode_numbering_candidate_count = Memo::new(move |_| {
        items.with(|items| {
            numbering_candidates(items, episode_numbering_start_item_id.get())
                .into_iter()
                .filter(|item| {
                    episode_numbering_auto_detect.get()
                        || replace_existing_episode_numbers.get()
                        || item.season.is_none_or(|value| value < 0)
                        || item.episode.is_none_or(|value| value < 1)
                })
                .count()
        })
    });
    let episode_numbering_assignment_count = Memo::new(move |_| {
        let available = episode_numbering_candidate_count.get();
        episode_numbering_count.with(|value| {
            let value = value.trim();
            if value.is_empty() {
                available
            } else {
                value
                    .parse::<usize>()
                    .ok()
                    .map_or(0, |count| count.min(available))
            }
        })
    });
    let episode_numbering_row_preview = Memo::new(move |_| {
        if !episode_numbering_open.get() || !episode_numbering_available.get() {
            return BTreeMap::new();
        }
        let Some(season) = episode_numbering_season
            .get()
            .trim()
            .parse::<i32>()
            .ok()
            .filter(|value| *value >= 0)
        else {
            return BTreeMap::new();
        };
        let starting_episode = if episode_numbering_auto_detect.get() {
            1
        } else {
            let Some(starting_episode) = episode_numbering_start
                .get()
                .trim()
                .parse::<i32>()
                .ok()
                .filter(|value| *value >= 1)
            else {
                return BTreeMap::new();
            };
            starting_episode
        };
        let count = episode_numbering_assignment_count.get();
        items.with(|items| {
            if episode_numbering_auto_detect.get() {
                let Some(minimum_gap_days) = episode_numbering_minimum_gap_days
                    .get()
                    .trim()
                    .parse::<i64>()
                    .ok()
                    .filter(|value| *value >= 1)
                else {
                    return BTreeMap::new();
                };
                auto_detect_episode_assignments(
                    items,
                    episode_numbering_start_item_id.get(),
                    season,
                    minimum_gap_days,
                    count,
                )
            } else {
                episode_numbering_preview_assignments(
                    items,
                    episode_numbering_start_item_id.get(),
                    season,
                    starting_episode,
                    count,
                    replace_existing_episode_numbers.get(),
                )
            }
        })
    });
    let episode_numbering_preview = Memo::new(move |_| {
        let assignments = episode_numbering_row_preview.get();
        if assignments.is_empty() {
            return None;
        }
        items.with(|items| {
            let ordered = items
                .iter()
                .filter_map(|item| assignments.get(&item.id).copied())
                .collect::<Vec<_>>();
            ordered
                .first()
                .copied()
                .zip(ordered.last().copied())
                .map(|(first, last)| (ordered.len(), first, last))
        })
    });
    let item_page_range = Memo::new(move |_| {
        let total = filtered_item_count.get();
        let size = item_page_size.get();
        let total_pages = if size == 0 {
            1
        } else {
            u32::try_from(total.div_ceil(size as usize))
                .unwrap_or(u32::MAX)
                .max(1)
        };
        let page = item_page.get().min(total_pages.saturating_sub(1));
        let start = if size == 0 {
            0
        } else {
            usize::try_from(page).unwrap_or(usize::MAX) * size as usize
        }
        .min(total);
        let end = if size == 0 {
            total
        } else {
            start.saturating_add(size as usize).min(total)
        };
        (start, end, total, page, total_pages)
    });
    let bulk_scope_count = Memo::new(move |_| {
        let (start, end, _, _, _) = item_page_range.get();
        items.with(|items| {
            draft_diff.with(|diff| {
                let category_items = items
                    .iter()
                    .filter(|item| {
                        item_matches_draft_category(item.id, active_draft_category, diff)
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                bulk_scope_items(
                    &category_items,
                    &bulk_scope_value.get(),
                    presentation_kind.get(),
                    need_review_filter.get(),
                    start,
                    end,
                )
                .len()
            })
        })
    });
    let option_scopes = Memo::new(move |_| {
        let (start, end, _, _, _) = item_page_range.get();
        items.with(|items| draft_diff.with(|diff| {
            let category_items = items.iter().filter(|item| item_matches_draft_category(item.id, active_draft_category, diff)).cloned().collect::<Vec<_>>();
            let page_ids = bulk_scope_items(&category_items, "current_page", presentation_kind.get(), need_review_filter.get(), start, end).iter().map(|item| item.id).collect::<BTreeSet<_>>();
            [("all", TextKey::AllItems), ("current_page", TextKey::CurrentPage), ("current_filter", TextKey::AddonOptionsCurrentFilter)].into_iter().map(|(scope, label)| {
                let scoped = if scope == "current_filter" {
                    category_items.iter().filter(|item| !need_review_filter.get() || item_needs_review(item, presentation_kind.get())).cloned().collect::<Vec<_>>()
                } else { bulk_scope_items(&category_items, scope, presentation_kind.get(), need_review_filter.get(), start, end) };
                let off_page = scoped.iter().filter(|item| !page_ids.contains(&item.id)).count();
                let value = serde_json::json!({"items": scoped.iter().map(|item| item.id.to_string()).collect::<Vec<_>>().join(","), "off_page_count": off_page}).to_string();
                (label, value)
            }).collect::<Vec<_>>()
        }))
    });
    let episode_numbering_off_page_count = Memo::new(move |_| {
        let preview = episode_numbering_row_preview.get();
        if preview.is_empty() {
            return 0;
        }
        let (start, end, _, _, _) = item_page_range.get();
        let visible_ids = items.with(|items| {
            draft_diff.with(|diff| {
                items
                    .iter()
                    .filter(|item| {
                        item_matches_draft_category(item.id, active_draft_category, diff)
                            && (!need_review_filter.get()
                                || item_needs_review(item, presentation_kind.get()))
                    })
                    .skip(start)
                    .take(end.saturating_sub(start))
                    .map(|item| item.id)
                    .collect::<BTreeSet<_>>()
            })
        });
        preview
            .keys()
            .filter(|item_id| !visible_ids.contains(item_id))
            .count()
    });
    let replacement_item_title = Memo::new(move |_| {
        let target = replacement_target.get()?;
        items.with(|items| {
            items
                .iter()
                .find(|item| item.id == target)
                .map(|item| item.display_title.clone())
        })
    });
    let reload_items_action = Action::new(|id: &i64| list_addon_media_list_items(*id));
    let reload_readiness_action =
        Action::new(|id: &i64| get_addon_media_list_publication_readiness(*id));
    let reload_history_action =
        Action::new(|id: &i64| get_addon_media_list_publication_history(*id));
    let reload_draft_diff_action =
        Action::new(|id: &i64| get_addon_media_list_saved_draft_diff(*id));
    let reload_lifecycle_action = Action::new(|id: &i64| {
        let id = *id;
        async move {
            let revisions = get_addon_media_list_revision_history(id).await?;
            let impact = get_addon_media_list_reset_impact(id).await?;
            Ok::<_, ServerFnError>((revisions, impact))
        }
    });
    let readiness_refresh_pending = RwSignal::new(false);
    let reload_show_nfo_action = Action::new(|id: &i64| get_addon_media_list_show_nfo(*id));
    let review_edit_action = Action::new(
        |value: &(i64, Vec<AddonMediaListItemReviewEdit>, ReviewUndoEntry)| {
            let (id, edits, undo) = value.clone();
            async move {
                apply_addon_media_list_item_review_edits(id, edits).await?;
                Ok(undo)
            }
        },
    );
    let undo_review_edit_action =
        Action::new(|value: &(i64, Vec<AddonMediaListItemReviewEdit>)| {
            let (id, snapshots) = value.clone();
            async move { apply_addon_media_list_item_review_edits(id, snapshots).await }
        });
    let item_role_action = Action::new(|value: &(i64, i64, AddonMediaListItemRole)| {
        let (list_id, item_id, role) = *value;
        async move { set_addon_media_list_item_role(list_id, item_id, role).await }
    });
    let request_detail_action = Action::new(|value: &(i64, i64)| {
        let (list_id, item_id) = *value;
        async move { request_addon_media_list_item_details(list_id, item_id).await }
    });
    let detail_request_item = RwSignal::new(None::<i64>);
    let control_import_action = Action::new(|value: &(i64, AddonMediaListImportAction)| {
        let (list_id, action) = *value;
        async move { control_addon_media_list_import(list_id, action).await }
    });
    let publish_action = Action::new(|value: &(i64, i32)| {
        let (id, saved_revision) = *value;
        async move { publish_addon_media_list(id, saved_revision).await }
    });
    let recovery_action = Action::new(|value: &(i64, i64, i32, AddonMediaListRecoveryAction)| {
        let (list_id, job_id, generation, action) = *value;
        async move { recover_addon_media_list_publication(list_id, job_id, generation, action).await }
    });
    let row_mutation_pending = Memo::new(move |_| {
        review_edit_action.pending().get()
            || undo_review_edit_action.pending().get()
            || item_role_action.pending().get()
            || replacement_action.pending().get()
            || request_detail_action.pending().get()
            || reload_items_action.pending().get()
            || reload_readiness_action.pending().get()
            || recovery_action.pending().get()
            || readiness_refresh_pending.get()
    });
    let reload_items = move || {
        readiness_refresh_pending.set(true);
        reload_items_action.dispatch(id);
    };
    Effect::new(move |_| {
        if let Some(result) = reload_items_action.value().get() {
            match result {
                Ok(next) => {
                    items.set(next);
                    reload_readiness_action.dispatch(id);
                }
                Err(error) => {
                    readiness_refresh_pending.set(false);
                    toast.push(addon_media_list_error_message(error), ToastSeverity::Error);
                }
            }
        }
    });
    Effect::new(move |_| {
        if let Some(result) = reload_readiness_action.value().get() {
            match result {
                Ok(next) => {
                    let marker_changed = publication_readiness.with_untracked(|current| {
                        publication_readiness_marker(current) != publication_readiness_marker(&next)
                    });
                    publication_readiness.set(next);
                    reload_draft_diff_action.dispatch(id);
                    if marker_changed {
                        reload_history_action.dispatch(id);
                        reload_lifecycle_action.dispatch(id);
                    }
                }
                Err(error) => {
                    toast.push(addon_media_list_error_message(error), ToastSeverity::Error);
                }
            }
            readiness_refresh_pending.set(false);
        }
    });
    Effect::new(move |_| {
        if let Some(result) = reload_history_action.value().get() {
            match result {
                Ok(next) => publication_history.set(next),
                Err(error) => {
                    toast.push(addon_media_list_error_message(error), ToastSeverity::Error);
                }
            }
        }
    });
    Effect::new(move |_| {
        if let Some(Ok(next)) = reload_draft_diff_action.value().get() {
            draft_diff.set(next);
        }
    });
    Effect::new(move |_| {
        if let Some(Ok((revisions, impact))) = reload_lifecycle_action.value().get() {
            revision_history.set(revisions);
            reset_impact.set(impact);
        }
    });
    Effect::new(move |_| {
        if let Some(result) = reload_show_nfo_action.value().get() {
            match result {
                Ok(next) if !metadata_dirty.get_untracked() => {
                    let metadata = next.metadata;
                    let poster =
                        artwork_value(&metadata.artwork, AddonMediaListArtworkRole::Poster);
                    let fanart =
                        artwork_value(&metadata.artwork, AddonMediaListArtworkRole::Fanart);
                    let thumbnail =
                        artwork_value(&metadata.artwork, AddonMediaListArtworkRole::Thumb);
                    show_etag.set(next.etag);
                    show_title.set(metadata.title.clone());
                    show_original_title.set(metadata.original_title.clone().unwrap_or_default());
                    show_sort_title.set(metadata.sort_title.clone().unwrap_or_default());
                    show_plot.set(metadata.plot.clone().unwrap_or_default());
                    show_year.set(
                        metadata
                            .year
                            .map_or_else(String::new, |value| value.to_string()),
                    );
                    show_genres.set(metadata.genres.join(", "));
                    show_tags.set(metadata.tags.join("\n"));
                    show_studios.set(metadata.studios.join(", "));
                    show_directors.set(metadata.directors.join(", "));
                    show_writers.set(metadata.writers.join(", "));
                    show_artists.set(metadata.artists.join(", "));
                    show_languages.set(metadata.languages.join(", "));
                    show_people.set(people_lines(&metadata.people));
                    show_poster.set(poster);
                    show_fanart.set(fanart);
                    show_thumbnail.set(thumbnail);
                    saved_show_metadata.set(metadata);
                }
                Ok(_) => {
                    // Preserve in-progress operator edits when a background
                    // refresh returns newer server metadata.
                }
                Err(error) => {
                    toast.push(addon_media_list_error_message(error), ToastSeverity::Error);
                }
            }
        }
    });
    Effect::new(move |_| {
        if let Some(result) = request_detail_action.value().get() {
            detail_request_item.set(None);
            match result {
                Ok(()) => {
                    reload_items();
                    if !poll_running.get_untracked() {
                        poll_epoch.update(|epoch| *epoch = epoch.wrapping_add(1));
                    }
                }
                Err(error) => toast.push(
                    save_failed(addon_media_list_error_message(error)),
                    ToastSeverity::Error,
                ),
            }
        }
        if let Some(result) = control_import_action.value().get() {
            match result {
                Ok(()) => {
                    toast.push(tr(TextKey::Saved).to_string(), ToastSeverity::Success);
                    reload_items();
                    if !poll_running.get_untracked() {
                        poll_epoch.update(|epoch| *epoch = epoch.wrapping_add(1));
                    }
                }
                Err(error) => toast.push(
                    save_failed(addon_media_list_error_message(error)),
                    ToastSeverity::Error,
                ),
            }
        }
    });
    Effect::new(move |_| {
        let _epoch = poll_epoch.get();
        #[cfg(target_arch = "wasm32")]
        {
            if poll_running.get_untracked() {
                return;
            }
            poll_running.set(true);
            leptos::task::spawn_local(async move {
                let mut previous_item_marker = import_progress.get_untracked().map(|progress| {
                    (
                        progress.state,
                        progress.discovered_count,
                        progress.detail_complete,
                        progress.detail_skipped,
                        progress.detail_failed,
                        progress.detail_pending,
                        progress.current_title.clone(),
                    )
                });
                let mut error_delay_ms = 1_000_u32;
                let mut last_announcement = None::<(TextKey, u64)>;
                let mut readiness_error_announced = false;
                let mut source_probe_error_announced = false;
                loop {
                    let visibility_delay_ms = web_sys::window()
                        .and_then(|window| window.document())
                        .map_or(
                            5_000,
                            |document| if document.hidden() { 5_000 } else { 1_000 },
                        );
                    let delay_ms = visibility_delay_ms.max(error_delay_ms);
                    gloo_timers::future::TimeoutFuture::new(delay_ms).await;
                    match get_addon_media_list_form(id).await {
                        Ok(Some(next_form)) => {
                            auto_publication_hold.set(next_form.auto_publication_hold.clone());
                            error_delay_ms = 0;
                            let next_marker = next_form.import_progress.as_ref().map(|progress| {
                                (
                                    progress.state,
                                    progress.discovered_count,
                                    progress.detail_complete,
                                    progress.detail_skipped,
                                    progress.detail_failed,
                                    progress.detail_pending,
                                    progress.current_title.clone(),
                                )
                            });
                            published_revision.set(next_form.published_revision);
                            detail_import_policy.set(next_form.detail_import_policy);
                            if let Some(progress) = next_form.import_progress.as_ref() {
                                let (details, count, _) = progress_sample(progress);
                                let now = browser_now_millis();
                                progress_rate_sample.update(|sample| match *sample {
                                    Some((sample_details, sample_count, _))
                                        if sample_details == details && count >= sample_count => {}
                                    _ => *sample = Some((details, count, now)),
                                });
                                let phase = import_progress_phase(
                                    progress,
                                    next_form.published_revision > 0,
                                    next_form.detail_import_policy,
                                );
                                let coarse_count = count / 10;
                                if last_announcement != Some((phase, coarse_count)) {
                                    progress_live_summary.set(format!("{}: {count}", tr(phase)));
                                    last_announcement = Some((phase, coarse_count));
                                }
                            }
                            import_progress.set(next_form.import_progress);
                            match get_addon_media_list_publication_readiness(id).await {
                                Ok(next_readiness) => {
                                    let marker_changed =
                                        publication_readiness.with_untracked(|current| {
                                            publication_readiness_marker(current)
                                                != publication_readiness_marker(&next_readiness)
                                        });
                                    publication_readiness.set(next_readiness);
                                    if marker_changed {
                                        reload_history_action.dispatch(id);
                                        reload_lifecycle_action.dispatch(id);
                                        reload_draft_diff_action.dispatch(id);
                                    }
                                    readiness_error_announced = false;
                                }
                                Err(error) => {
                                    error_delay_ms = 2_000;
                                    if !readiness_error_announced {
                                        toast.push(
                                            addon_media_list_error_message(error),
                                            ToastSeverity::Error,
                                        );
                                        readiness_error_announced = true;
                                    }
                                    continue;
                                }
                            }
                            match get_addon_media_list_source_probe(id).await {
                                Ok(next_probe) => {
                                    source_probe.set(next_probe);
                                    source_probe_error_announced = false;
                                }
                                Err(error) => {
                                    source_probe.set(None);
                                    if !source_probe_error_announced {
                                        toast.push(
                                            addon_media_list_error_message(error),
                                            ToastSeverity::Error,
                                        );
                                        source_probe_error_announced = true;
                                    }
                                }
                            }
                            if next_marker != previous_item_marker {
                                let became_ready =
                                    next_marker.as_ref().is_some_and(|(state, ..)| {
                                        *state == AddonMediaListImportState::Ready
                                    }) && previous_item_marker.as_ref().is_none_or(
                                        |(state, ..)| *state != AddonMediaListImportState::Ready,
                                    );
                                previous_item_marker = next_marker;
                                reload_items_action.dispatch(id);
                                if became_ready {
                                    reload_show_nfo_action.dispatch(id);
                                }
                            }
                            let publication_active =
                                publication_readiness.with_untracked(|readiness| {
                                    publication_preparation_active(readiness)
                                        || readiness.publication.as_ref().is_some_and(
                                            |publication| {
                                                matches!(
                                                    publication.state,
                                                    AddonMediaListPublicationState::Queued
                                                        | AddonMediaListPublicationState::Publishing
                                                        | AddonMediaListPublicationState::Restoring
                                                )
                                            },
                                        )
                                });
                            let source_probe_active = source_probe.with_untracked(|probe| {
                                probe.as_ref().is_some_and(|probe| {
                                    matches!(
                                        probe.state,
                                        AddonMediaListSourceProbeState::Queued
                                            | AddonMediaListSourceProbeState::Running
                                    )
                                })
                            });
                            let should_stop = !publication_active
                                && !source_probe_active
                                && import_progress.with_untracked(|progress| {
                                    progress.as_ref().is_none_or(|progress| {
                                        matches!(
                                            progress.state,
                                            AddonMediaListImportState::Paused
                                                | AddonMediaListImportState::NeedsAttention
                                                | AddonMediaListImportState::Cancelled
                                                | AddonMediaListImportState::Superseded
                                        ) || (progress.state == AddonMediaListImportState::Ready
                                            && progress.detail_pending == 0)
                                    })
                                });
                            if should_stop {
                                break;
                            }
                        }
                        Ok(None) => break,
                        Err(_) => {
                            error_delay_ms = if error_delay_ms == 0 {
                                2_000
                            } else {
                                error_delay_ms.saturating_mul(2).min(30_000)
                            };
                        }
                    }
                }
                poll_running.set(false);
            });
        }
    });
    Effect::new(move |_| {
        if let Some(result) = item_role_action.value().get() {
            match result {
                Ok(()) => {
                    row_save_states.update(|states| {
                        for state in states.values_mut() {
                            if *state == RowSaveState::Saving {
                                *state = RowSaveState::Saved;
                            }
                        }
                    });
                    reload_items();
                }
                Err(error) => {
                    row_save_states.update(|states| {
                        for state in states.values_mut() {
                            if *state == RowSaveState::Saving {
                                *state = RowSaveState::Error;
                            }
                        }
                    });
                    reload_items();
                    toast.push(
                        save_failed(addon_media_list_error_message(error)),
                        ToastSeverity::Error,
                    );
                }
            }
        }
    });
    Effect::new(move |_| {
        if let Some(result) = review_edit_action.value().get() {
            match result {
                Ok(undo) => {
                    row_save_states.update(|states| {
                        for state in states.values_mut() {
                            if *state == RowSaveState::Saving {
                                *state = RowSaveState::Saved;
                            }
                        }
                    });
                    last_review_undo.set(Some(undo));
                    toast.push(tr(TextKey::Saved).to_string(), ToastSeverity::Success);
                    reload_items();
                }
                Err(error) => {
                    row_save_states.update(|states| {
                        for state in states.values_mut() {
                            if *state == RowSaveState::Saving {
                                *state = RowSaveState::Error;
                            }
                        }
                    });
                    reload_items();
                    toast.push(
                        save_failed(addon_media_list_error_message(error)),
                        ToastSeverity::Error,
                    );
                }
            }
        }
    });
    Effect::new(move |_| {
        if let Some(result) = undo_review_edit_action.value().get() {
            match result {
                Ok(_) => {
                    last_review_undo.set(None);
                    toast.push(
                        tr(TextKey::ChangeUndone).to_string(),
                        ToastSeverity::Success,
                    );
                    reload_items();
                }
                Err(error) => toast.push(
                    save_failed(addon_media_list_error_message(error)),
                    ToastSeverity::Error,
                ),
            }
        }
    });
    Effect::new(move |_| {
        if let Some(result) = publish_action.value().get() {
            match result {
                Ok(AddonMediaListPublishResult::Queued { .. }) => {
                    toast.push(tr(TextKey::Publishing).to_string(), ToastSeverity::Info);
                    set_browser_location(&format!("/media/addon-lists/{id}"));
                }
                Ok(AddonMediaListPublishResult::PreparingMedia { item_count }) => {
                    toast.push(
                        format!("{}: {item_count}", tr(TextKey::ProcessedMediaQueued)),
                        ToastSeverity::Info,
                    );
                    reload_readiness_action.dispatch(id);
                    if !poll_running.get_untracked() {
                        poll_epoch.update(|epoch| *epoch = epoch.wrapping_add(1));
                    }
                }
                Ok(AddonMediaListPublishResult::PreparingPlaybackDetails { item_count }) => {
                    toast.push(
                        format!("{}: {item_count}", tr(TextKey::PlaybackDetailsQueued)),
                        ToastSeverity::Info,
                    );
                    reload_items();
                    reload_readiness_action.dispatch(id);
                    if !poll_running.get_untracked() {
                        poll_epoch.update(|epoch| *epoch = epoch.wrapping_add(1));
                    }
                }
                Err(error) => toast.push(
                    save_failed(addon_media_list_error_message(error)),
                    ToastSeverity::Error,
                ),
            }
        }
    });
    Effect::new(move |_| {
        if let Some(result) = recovery_action.value().get() {
            match result {
                Ok(_) => {
                    toast.push(tr(TextKey::RecoveryQueued).to_string(), ToastSeverity::Info);
                    reload_readiness_action.dispatch(id);
                    reload_history_action.dispatch(id);
                    if !poll_running.get_untracked() {
                        poll_epoch.update(|epoch| *epoch = epoch.wrapping_add(1));
                    }
                }
                Err(error) => toast.push(
                    save_failed(addon_media_list_error_message(error)),
                    ToastSeverity::Error,
                ),
            }
        }
    });
    Effect::new(move |_| {
        if let Some(result) = apply_content_kind_action.value().get() {
            match result {
                Ok(_) => {
                    toast.push(tr(TextKey::Saved).to_string(), ToastSeverity::Success);
                    reload_items();
                }
                Err(error) => toast.push(
                    save_failed(addon_media_list_error_message(error)),
                    ToastSeverity::Error,
                ),
            }
        }
    });
    Effect::new(move |_| {
        if let Some(result) = save_action.value().get() {
            match result {
                Ok(()) => {
                    toast.push(
                        tr(TextKey::AddonMediaListSaved).to_string(),
                        ToastSeverity::Success,
                    );
                    set_browser_location(&format!("/media/addon-lists/{id}"));
                }
                Err(error) => toast.push(
                    save_failed(addon_media_list_error_message(error)),
                    ToastSeverity::Error,
                ),
            }
        }
    });
    Effect::new(move |_| {
        if let Some(result) = change_addon_action.value().get() {
            change_addon_open.set(false);
            match result {
                Ok(()) => {
                    toast.push(
                        tr(TextKey::AddonMediaListSaved).to_string(),
                        ToastSeverity::Success,
                    );
                    reload_items();
                }
                Err(error) => toast.push(
                    save_failed(addon_media_list_error_message(error)),
                    ToastSeverity::Error,
                ),
            }
        }
    });
    Effect::new(move |_| {
        if let Some(result) = reset_action.value().get() {
            match result {
                Ok(AddonMediaListResetResult::Queued { .. }) => {
                    toast.push(tr(TextKey::ResetQueued).to_owned(), ToastSeverity::Info);
                    reset_status_visible.set(true);
                    focus_browser_element_after_render("addon-list-reset-status");
                    if !poll_running.get_untracked() {
                        poll_epoch.update(|epoch| *epoch = epoch.wrapping_add(1));
                    }
                }
                Err(_) => toast.push(
                    tr(TextKey::ResetAddonMediaListFailed).to_owned(),
                    ToastSeverity::Error,
                ),
            }
        }
    });
    Effect::new(move |_| {
        if let Some(result) = reconcile_action.value().get() {
            match result {
                Ok(AddonMediaListReconcileResult::Queued { .. }) => {
                    toast.push(tr(TextKey::ReconcileQueued).to_owned(), ToastSeverity::Info);
                    reconcile_status_visible.set(true);
                    focus_browser_element_after_render("addon-list-reconcile-status");
                    if !poll_running.get_untracked() {
                        poll_epoch.update(|epoch| *epoch = epoch.wrapping_add(1));
                    }
                }
                Err(_) => toast.push(
                    tr(TextKey::ReconcileFailed).to_owned(),
                    ToastSeverity::Error,
                ),
            }
        }
    });
    Effect::new(move |_| {
        if let Some(result) = confirm_removal_action.value().get() {
            match result {
                Ok(()) => {
                    toast.push(
                        tr(TextKey::RemovalConfirmed).to_owned(),
                        ToastSeverity::Success,
                    );
                    reload_items();
                }
                Err(_) => toast.push(
                    tr(TextKey::RemovalConfirmationFailed).to_owned(),
                    ToastSeverity::Error,
                ),
            }
        }
    });
    Effect::new(move |_| {
        if let Some(result) = source_test_action.value().get() {
            match result {
                Ok(_) => {
                    toast.push(
                        tr(TextKey::SourceTestQueued).to_owned(),
                        ToastSeverity::Info,
                    );
                    source_probe.update(|probe| {
                        *probe = Some(AddonMediaListSourceProbeRow {
                            state: AddonMediaListSourceProbeState::Queued,
                            saved_draft_revision: publication_readiness
                                .with_untracked(|readiness| readiness.saved_revision),
                            source_revision: None,
                            capabilities: Vec::new(),
                            outcome: None,
                            provider_id: None,
                            list_title: None,
                            count_hint: None,
                            returned_item_count: None,
                            page_complete: None,
                            duration_milliseconds: None,
                            diagnostic_code: None,
                            diagnostic: None,
                            attempt_count: 0,
                            updated_at: String::new(),
                            completed_at: None,
                        });
                    });
                    if !poll_running.get_untracked() {
                        poll_epoch.update(|epoch| *epoch = epoch.wrapping_add(1));
                    }
                }
                Err(_) => toast.push(
                    tr(TextKey::SourceTestFailed).to_owned(),
                    ToastSeverity::Error,
                ),
            }
        }
    });
    Effect::new(move |_| {
        if let Some(result) = compare_revision_action.value().get() {
            match result {
                Ok(diff) => revision_diff.set(Some(diff)),
                Err(error) => {
                    toast.push(addon_media_list_error_message(error), ToastSeverity::Error);
                }
            }
        }
    });
    Effect::new(move |_| {
        if let Some(result) = restore_revision_action.value().get() {
            restore_open.set(false);
            match result {
                Ok(AddonMediaListRestoreResult { .. }) => {
                    toast.push(
                        tr(TextKey::RestoreRevisionQueued).to_owned(),
                        ToastSeverity::Info,
                    );
                    set_browser_location(&format!(
                        "/media/addon-lists/{id}?status=revision-restore-queued"
                    ));
                }
                Err(_) => toast.push(
                    tr(TextKey::RestoreRevisionFailed).to_owned(),
                    ToastSeverity::Error,
                ),
            }
        }
    });
    Effect::new(move |_| {
        if let Some(result) = search_action.value().get() {
            match result {
                Ok(rows) => replacement_results.set(rows),
                Err(error) => {
                    toast.push(addon_media_list_error_message(error), ToastSeverity::Error);
                }
            }
        }
    });
    Effect::new(move |_| {
        if let Some(result) = replacement_action.value().get() {
            match result {
                Ok(()) => {
                    replacement_target.set(None);
                    reload_items();
                }
                Err(error) => toast.push(
                    save_failed(addon_media_list_error_message(error)),
                    ToastSeverity::Error,
                ),
            }
        }
    });

    view! {
        <div class=style::page>
            <header
                class=style::metadata_hero
                style:display=move || if form_found { "grid" } else { "none" }
            >
                <span class=style::metadata_hero_icon style=move || icon_style(presentation_icon(presentation_kind.get()), "1.6rem") aria-hidden="true"></span>
                <div>
                    <h1 class=style::metadata_title_heading>
                        <label class=style::metadata_title_editor for="addon-list-title">
                            <span>{tr(TextKey::Title)}</span>
                            <input
                                id="addon-list-title"
                                class=style::metadata_title_input
                                form="addon_media_lists-list-editor-form"
                                name="metadata_title"
                                required
                                value=show_title.get_untracked()
                                prop:value=move || show_title.get()
                                on:input=move |event| show_title.set(event_target_value(&event))
                            />
                        </label>
                    </h1>
                    <p class=style::metadata_type>{move || tr(presentation_text_key(presentation_kind.get()))}</p>
                    <p
                        id="addon-list-description"
                        class=move || if description_expanded.get() {
                            format!("{} {}", style::metadata_description, style::description_expanded)
                        } else {
                            style::metadata_description.to_owned()
                        }
                    >{move || show_plot.get()}</p>
                    <button
                        type="button"
                        class=format!("{} {}", style::description_toggle, style::js_only)
                        hidden=move || show_plot.with(|plot| plot.chars().count() <= 160)
                        aria-expanded=move || if description_expanded.get() { "true" } else { "false" }
                        aria-controls="addon-list-description"
                        on:click=move |_| description_expanded.update(|expanded| *expanded = !*expanded)
                    >
                        {move || tr(if description_expanded.get() { TextKey::ShowLess } else { TextKey::ShowFullDescription })}
                    </button>
                    <p
                        class=style::metadata_hero_conflict
                        data-testid="addon-metadata-conflict-summary"
                        style:display=if show_metadata_conflict_count == 0 { "none" } else { "inline-flex" }
                    >
                        <span style=icon_style(IconName::Warning, "1rem") aria-hidden="true"></span>
                        <span>{format!("{} {}", show_metadata_conflict_count, tr(TextKey::MetadataConflicts))}</span>
                    </p>
                </div>
            </header>
            <div
                class=style::review_workspace
                style:display=move || if form_found { "grid" } else { "none" }
            >
            <section class=style::workflow_shell aria-labelledby="addon-list-readiness-heading">
                <nav aria-label=tr(TextKey::PublicationReadiness)>
                    <ol class=style::workflow_steps>
                        <li
                            class=move || if import_step_active.get() { style::workflow_current } else { style::workflow_complete }
                            aria-current=move || import_step_active.get().then_some("step")
                        >
                            <b>"1"</b><span>{tr(TextKey::Import)}</span>
                        </li>
                        <li
                            class=move || if published_step_complete.get() || publish_step_active.get() {
                                style::workflow_complete
                            } else if !import_step_active.get() {
                                style::workflow_current
                            } else {
                                ""
                            }
                            aria-current=move || (!import_step_active.get() && !publish_step_active.get() && !published_step_complete.get()).then_some("step")
                        >
                            <b>"2"</b><span>{tr(TextKey::ReadyToReview)}</span>
                        </li>
                        <li
                            class=move || if published_step_complete.get() {
                                style::workflow_complete
                            } else if publish_step_active.get() {
                                style::workflow_current
                            } else {
                                ""
                            }
                            aria-current=move || (publish_step_active.get() && !published_step_complete.get()).then_some("step")
                        >
                            <b>"3"</b><span>{tr(TextKey::PublishList)}</span>
                        </li>
                    </ol>
                </nav>
                <section class=style::readiness_summary>
                    <div>
                        <h2 id="addon-list-readiness-heading">{tr(TextKey::PublicationReadiness)}</h2>
                        <p class=style::revision_state role="status" aria-live="polite">
                            <strong>{move || match primary_action.get() {
                                PublicationPrimaryAction::Save => tr(TextKey::UnsavedChanges),
                                PublicationPrimaryAction::Prepare => tr(TextKey::PreparePublication),
                                PublicationPrimaryAction::Queued => tr(TextKey::PublicationQueued),
                                PublicationPrimaryAction::Publishing => tr(TextKey::Publishing),
                                PublicationPrimaryAction::Restoring => tr(TextKey::RestoringPublication),
                                PublicationPrimaryAction::PublishBlocked => tr(TextKey::PublishBlocked),
                                PublicationPrimaryAction::None
                                    if publication_readiness.with(|readiness| readiness.required_prepared_media_pending > 0) => tr(TextKey::PreparingProcessedMedia),
                                PublicationPrimaryAction::None
                                    if publication_readiness.with(|readiness| readiness.required_playback_detail_pending > 0) => tr(TextKey::PreparingPublication),
                                PublicationPrimaryAction::None
                                    if publication_readiness.with(|readiness| readiness.review_state == AddonMediaListReviewState::Published) => tr(TextKey::Published),
                                PublicationPrimaryAction::None | PublicationPrimaryAction::Publish => tr(TextKey::DraftSaved),
                            }}</strong>
                            <span>{move || format!("{} {}", tr(TextKey::SavedRevision), publication_readiness.with(|readiness| readiness.saved_revision))}</span>
                        </p>
                        <p>{tr(TextKey::PublishSavedRevisionHelp)}</p>
                        <p>{tr(TextKey::ChangesSavedImmediately)}</p>
                    </div>
                    <dl class=style::readiness_metrics>
                        <div>
                            <dt>{tr(TextKey::IncludeInPublication)}</dt>
                            <dd><a href="#addon-list-items-heading">{move || publication_readiness.with(|readiness| readiness.included_count)}</a></dd>
                        </div>
                        <div>
                            <dt>{tr(TextKey::PublicationBlockers)}</dt>
                            <dd><a href="#addon-list-publication-blockers">{move || publication_readiness.with(|readiness| readiness.blockers.len())}</a></dd>
                        </div>
                        <div>
                            <dt>{tr(TextKey::ReviewRecommendations)}</dt>
                            <dd>
                                <a
                                    href="#addon-list-items-heading"
                                    on:click=move |_| {
                                        need_review_filter.set(true);
                                        item_page.set(0);
                                    }
                                >{move || publication_readiness.with(|readiness| readiness.recommendation_count)}</a>
                            </dd>
                        </div>
                        <div>
                            <dt>{tr(TextKey::OptionalDetails)}</dt>
                            <dd><a href="#addon-list-items-heading">{move || publication_readiness.with(|readiness| format!("{} / {}", readiness.optional_detail_complete, readiness.optional_detail_total))}</a></dd>
                        </div>
                        <div style:display=move || if publication_readiness.with(|readiness| readiness.restricted_excluded_count > 0) { "block" } else { "none" }>
                            <dt>{tr(TextKey::RestrictedItemsExcluded)}</dt>
                            <dd>{move || publication_readiness.with(|readiness| readiness.restricted_excluded_count)}</dd>
                        </div>
                    </dl>
                    <p
                        class=style::readiness_ok
                        style:display=move || if publication_readiness.with(|readiness| readiness.blockers.is_empty()) { "flex" } else { "none" }
                    >{tr(TextKey::NoPublicationBlockers)}</p>
                    <p
                        class=style::preparation_state
                        style:display=move || if publication_readiness.with(|readiness| readiness.required_playback_detail_count > 0) { "flex" } else { "none" }
                    >
                        <span style=icon_style(IconName::Clock, "1rem") aria-hidden="true"></span>
                        <span>{move || publication_readiness.with(|readiness| format!("{}: {}", tr(TextKey::PreparePublication), readiness.required_playback_detail_count))}</span>
                    </p>
                    {prepared_media_status(publication_readiness)}
                    <p class=style::workflow_message style:display=if initial_error_key.is_some() { "flex" } else { "none" } role="alert">
                        {initial_error_key.map(tr).unwrap_or_default()}
                    </p>
                    <p class=style::workflow_message style:display=if initial_status_key.is_some() { "flex" } else { "none" } role="status">
                        {initial_status_key.map(tr).unwrap_or_default()}
                    </p>
                    {move || publication_readiness.with(|readiness| {
                        readiness.publication.clone().and_then(|publication| {
                            matches!(
                                publication.state,
                                AddonMediaListPublicationState::Waiting
                                    | AddonMediaListPublicationState::Expired
                                    | AddonMediaListPublicationState::Failed
                            )
                            .then(|| publication_recovery_view(id, publication, recovery_action))
                        })
                    })}
                </section>
                <div class=format!("{} {}", style::workflow_action, style::js_only)>
                <button
                    form="addon_media_lists-list-editor-form"
                    type="submit"
                    class=style::btn_add
                    disabled=move || row_mutation_pending.get() || save_action.pending().get()
                    style:display=move || if primary_action.get() == PublicationPrimaryAction::Save { "inline-flex" } else { "none" }
                    on:click=move |event| {
                        event.prevent_default();
                        if row_mutation_pending.get_untracked() || save_action.pending().get_untracked() {
                            return;
                        }
                        let mut next_show_metadata = show_metadata.clone();
                        next_show_metadata.title = show_title.get_untracked();
                        next_show_metadata.original_title = non_empty_value(&show_original_title.get_untracked());
                        next_show_metadata.sort_title = non_empty_value(&show_sort_title.get_untracked());
                        next_show_metadata.plot = non_empty_value(&show_plot.get_untracked());
                        next_show_metadata.year = show_year.get_untracked().trim().parse().ok();
                        next_show_metadata.genres = comma_values(&show_genres.get_untracked());
                        next_show_metadata.tags = line_values(&show_tags.get_untracked());
                        next_show_metadata.studios = comma_values(&show_studios.get_untracked());
                        next_show_metadata.directors = comma_values(&show_directors.get_untracked());
                        next_show_metadata.writers = comma_values(&show_writers.get_untracked());
                        next_show_metadata.artists = comma_values(&show_artists.get_untracked());
                        next_show_metadata.languages = comma_values(&show_languages.get_untracked());
                        next_show_metadata.people = people_values(&show_people.get_untracked(), &next_show_metadata.people);
                        next_show_metadata.artwork = artwork_values(
                            &show_metadata.artwork,
                            [
                                (&show_poster.get_untracked(), AddonMediaListArtworkRole::Poster),
                                (&show_fanart.get_untracked(), AddonMediaListArtworkRole::Fanart),
                                (&show_thumbnail.get_untracked(), AddonMediaListArtworkRole::Thumb),
                            ],
                        );
                        save_action.dispatch((AddonMediaListForm {
                            selection: form.selection.clone(),
                            delivery_mode: delivery_mode.get_untracked(),
                            delivery_cutoff_at: form.delivery_cutoff_at.clone(),
                            auto_publication_hold: auto_publication_hold.get_untracked(),
                            id: form_id.get_untracked(),
                            name: list_name.get_value(),
                            addon_id: submit_addon_id.clone(),
                            source_url: submit_source_url.clone(),
                            folder_name: form.folder_name.clone(),
                            refresh_cadence: refresh_cadence.get_untracked(),
                            refresh_publication_policy: refresh_publication_policy.get_untracked(),
                            last_refresh_succeeded_at: form.last_refresh_succeeded_at.clone(),
                            next_refresh_at: form.next_refresh_at.clone(),
                            refresh_retry_at: form.refresh_retry_at.clone(),
                            generate_playlist: generate_playlist.get_untracked(),
                            content_kind_override: content_kind_override.get_untracked(),
                            legacy_chapter_input: legacy_chapter_input.clone(),
                            presentation_kind: form.presentation_kind,
                            requested_presentation_kind: form.requested_presentation_kind,
                            presentation_error: form.presentation_error.clone(),
                            review_state: form.review_state,
                            review_error: form.review_error.clone(),
                            published_revision: form.published_revision,
                            draft_revision: form.draft_revision,
                            detail_import_policy: detail_import_policy.get_untracked(),
                            import_progress: form.import_progress.clone(),
                        }, presentation_kind.get_untracked(), next_show_metadata, show_etag.get_untracked()));
                    }
                >
                    <span style=icon_style(IconName::Save, "1.125rem") aria-hidden="true"></span>
                    <span>{tr(TextKey::SaveChanges)}</span>
                </button>
                <button
                    form="addon_media_lists-list-editor-form"
                    type="submit"
                    formaction="/admin-api/addon-lists/publish"
                    formmethod="post"
                    class=style::btn_add
                    disabled=move || row_mutation_pending.get() || publish_action.pending().get()
                    style:display=move || if matches!(primary_action.get(), PublicationPrimaryAction::Prepare | PublicationPrimaryAction::Publish) { "inline-flex" } else { "none" }
                    on:click=move |event| {
                        event.prevent_default();
                        if row_mutation_pending.get_untracked() || publish_action.pending().get_untracked() {
                            return;
                        }
                        publish_action.dispatch((
                            id,
                            publication_readiness.with_untracked(|readiness| readiness.saved_revision),
                        ));
                    }
                >
                    <span style=icon_style(IconName::Upload, "1.125rem") aria-hidden="true"></span>
                    <span>{move || if primary_action.get() == PublicationPrimaryAction::Prepare {
                        tr(TextKey::PreparePublication)
                    } else {
                        tr(TextKey::PublishList)
                    }}</span>
                </button>
                <button
                    type="button"
                    class=style::btn_add
                    disabled=true
                    style:display=move || if primary_action.get() == PublicationPrimaryAction::Publishing { "inline-flex" } else { "none" }
                >
                    <span style=icon_style(IconName::Clock, "1.125rem") aria-hidden="true"></span>
                    <span>{tr(TextKey::Publishing)}</span>
                </button>
                <button
                    type="button"
                    class=style::btn_add
                    disabled=true
                    style:display=move || if primary_action.get() == PublicationPrimaryAction::Queued { "inline-flex" } else { "none" }
                >
                    <span style=icon_style(IconName::Clock, "1.125rem") aria-hidden="true"></span>
                    <span>{tr(TextKey::PublicationQueued)}</span>
                </button>
                <button
                    type="button"
                    class=style::btn_add
                    disabled=true
                    style:display=move || if primary_action.get() == PublicationPrimaryAction::Restoring { "inline-flex" } else { "none" }
                >
                    <span style=icon_style(IconName::Clock, "1.125rem") aria-hidden="true"></span>
                    <span>{tr(TextKey::RestoringPublication)}</span>
                </button>
                <button
                    type="button"
                    class=style::btn_add
                    disabled=true
                    style:display=move || if primary_action.get() == PublicationPrimaryAction::PublishBlocked { "inline-flex" } else { "none" }
                >
                    <span style=icon_style(IconName::Warning, "1.125rem") aria-hidden="true"></span>
                    <span>{tr(TextKey::PublishBlocked)}</span>
                </button>
                </div>
                <noscript>
                    <div class=style::nojs_workflow_actions>
                        <button
                            form="addon_media_lists-list-editor-form"
                            type="submit"
                            class=style::btn_add
                        >
                            <span style=icon_style(IconName::Save, "1.125rem") aria-hidden="true"></span>
                            <span>{tr(TextKey::SaveChanges)}</span>
                        </button>
                        <button
                            form="addon_media_lists-list-editor-form"
                            type="submit"
                            formaction="/admin-api/addon-lists/publish"
                            formmethod="post"
                            class=style::btn_add
                            disabled=move || !matches!(primary_action.get(), PublicationPrimaryAction::Prepare | PublicationPrimaryAction::Publish)
                        >
                            <span style=icon_style(IconName::Upload, "1.125rem") aria-hidden="true"></span>
                            <span>{move || match primary_action.get() {
                                PublicationPrimaryAction::Prepare => tr(TextKey::PreparePublication),
                                PublicationPrimaryAction::Queued => tr(TextKey::PublicationQueued),
                                PublicationPrimaryAction::Publishing => tr(TextKey::Publishing),
                                PublicationPrimaryAction::Restoring => tr(TextKey::RestoringPublication),
                                PublicationPrimaryAction::PublishBlocked => tr(TextKey::PublishBlocked),
                                _ => tr(TextKey::PublishList),
                            }}</span>
                        </button>
                    </div>
                </noscript>
            <section
                id="addon-list-publication-blockers"
                class=style::readiness_details
                aria-labelledby="addon-list-publication-blockers-heading"
                style:display=move || if publication_readiness.with(|readiness| readiness.blockers.is_empty()) { "none" } else { "grid" }
            >
                <h2 id="addon-list-publication-blockers-heading">{tr(TextKey::PublicationBlockers)}</h2>
                <ul class=style::blocker_list aria-label=tr(TextKey::PublicationBlockers)>
                    {move || publication_readiness.with(|readiness| {
                        publication_blocker_groups(&readiness.blockers).into_iter().map(|group| {
                            let kind = group.kind;
                            view! {
                                <li>
                                    <span style=icon_style(IconName::Warning, "1rem") aria-hidden="true"></span>
                                    <a
                                        href=publication_blocker_href(kind)
                                        on:click=move |_| {
                                            if kind == AddonMediaListPublicationBlockerKind::SeasonEpisodeMissing {
                                                episode_numbering_open.set(true);
                                                need_review_filter.set(false);
                                                item_page.set(0);
                                            } else if publication_blocker_href(kind) == "#addon-list-items-heading" {
                                                need_review_filter.set(true);
                                                item_page.set(0);
                                            }
                                        }
                                    >{publication_blocker_group_label(&group)}</a>
                                </li>
                            }
                        }).collect_view()
                    })}
                </ul>
            </section>
            </section>

            <div class=style::review_main>

            {move || import_progress.get().map(|progress| {
                let state = progress.state;
                let phase = import_progress_phase(
                    &progress,
                    published_revision.get() > 0,
                    detail_import_policy.get(),
                );
                let (is_detail_progress, current, total) = progress_sample(&progress);
                let percent = total
                    .map_or(0, |total| {
                        current.saturating_mul(100).checked_div(total).unwrap_or(100)
                    })
                    .min(100);
                let current_title = progress.current_title.clone().unwrap_or_default();
                let diagnostic = progress.diagnostic.clone().unwrap_or_default();
                let has_current_title = !current_title.is_empty();
                let has_diagnostic = !diagnostic.is_empty();
                let has_empty_import = state == AddonMediaListImportState::Ready
                    && progress.discovered_count == 0
                    && (progress.mode == AddonMediaListImportMode::Full
                        || progress.retained_item_count == 0);
                let has_archive_counts =
                    progress.strategy != AddonMediaListIncrementalStrategy::None;
                let overview_progress_label = if has_archive_counts {
                    TextKey::Examined
                } else {
                    TextKey::Discovered
                };
                let has_detail_failed = progress.detail_failed > 0;
                let completed = state == AddonMediaListImportState::Ready;
                let elapsed = elapsed_between(
                    &progress.started_at,
                    completed.then_some(progress.last_progress_at.as_deref()).flatten(),
                )
                .unwrap_or_default();
                let has_elapsed = !elapsed.is_empty();
                let elapsed_label = if completed {
                    TextKey::CompletedIn
                } else {
                    TextKey::ElapsedTime
                };
                let last_progress = progress.last_progress_at.as_deref()
                    .and_then(format_progress_timestamp)
                    .unwrap_or_default();
                let has_last_progress = !last_progress.is_empty();
                let estimate = progress_rate_sample.get().and_then(
                    |(sample_is_detail, sample_count, sample_at)| {
                        let now = browser_now_millis();
                        let elapsed_seconds = (now - sample_at) / 1_000.0;
                        let total = total?;
                        let completed_since_sample = current.saturating_sub(sample_count);
                        if sample_is_detail != is_detail_progress
                            || completed_since_sample < 3
                            || elapsed_seconds < 3.0
                            || current >= total
                        {
                            return None;
                        }
                        let completed_since_sample =
                            u32::try_from(completed_since_sample).ok()?;
                        let remaining = u32::try_from(total - current).ok()?;
                        let rate = f64::from(completed_since_sample) / elapsed_seconds;
                        let eta_seconds = f64::from(remaining) / rate;
                        std::time::Duration::try_from_secs_f64(eta_seconds)
                            .ok()
                            .map(|duration| format_clock_duration(duration.as_secs()))
                    },
                ).unwrap_or_default();
                let has_estimate = !estimate.is_empty();
                let progress_element = if let Some(total) = total {
                    view! {
                        <progress
                            role="progressbar"
                            max=total.max(1).to_string()
                            value=current.min(total).to_string()
                            aria-valuemin="0"
                            aria-valuemax=total.to_string()
                            aria-valuenow=current.min(total).to_string()
                        >{format!("{percent}%")}</progress>
                    }.into_any()
                } else {
                    view! { <progress role="progressbar"></progress> }.into_any()
                };
                let contextual_action = if import_can_retry(&progress) { view! {
                    <button type="button" class=style::btn_cancel on:click=move |_| {
                        control_import_action.dispatch((id, AddonMediaListImportAction::Retry));
                    }>{tr(TextKey::RetryImportNow)}</button>
                }.into_any() } else { match state {
                    AddonMediaListImportState::Queued => view! {
                        <button type="button" class=style::btn_cancel on:click=move |_| {
                            control_import_action.dispatch((id, AddonMediaListImportAction::Cancel));
                        }>{tr(TextKey::CancelImport)}</button>
                    }.into_any(),
                    AddonMediaListImportState::Discovering | AddonMediaListImportState::Verifying => view! {
                        <button type="button" class=style::btn_cancel on:click=move |_| {
                            control_import_action.dispatch((id, AddonMediaListImportAction::Pause));
                        }>{tr(TextKey::PauseImport)}</button>
                    }.into_any(),
                    AddonMediaListImportState::Paused => view! {
                        <button type="button" class=style::btn_cancel on:click=move |_| {
                            control_import_action.dispatch((id, AddonMediaListImportAction::Resume));
                        }>{tr(TextKey::ResumeImport)}</button>
                    }.into_any(),
                    AddonMediaListImportState::NeedsAttention
                    | AddonMediaListImportState::Cancelled
                    | AddonMediaListImportState::Ready
                    | AddonMediaListImportState::Superseded => ().into_any(),
                }};
                view! {
                    <section id="addon-list-import-progress" class=style::import_progress_card data-import-state=format!("{state:?}")>
                        <span class=style::sr_only aria-live="polite" aria-atomic="true">
                            {move || progress_live_summary.get()}
                        </span>
                        <div class=style::import_progress_heading>
                            <div>
                                <strong>{tr(phase)}</strong>
                                <span>{if let Some(total) = total {
                                    format!("{current} / {total} {}", if is_detail_progress { tr(TextKey::Details) } else { tr(overview_progress_label) })
                                } else {
                                    format!("{current} {}", tr(overview_progress_label))
                                }}</span>
                            </div>
                            <span class=style::import_progress_percent>{total.map_or_else(|| "…".to_owned(), |_| format!("{percent}%"))}</span>
                        </div>
                        {progress_element}
                        <p style:display=if has_current_title { "block" } else { "none" }>{current_title}</p>
                        <p class=style::import_progress_details>
                            <span style:display=if has_elapsed { "inline" } else { "none" }>{format!("{}: {elapsed}", tr(elapsed_label))}</span>
                            <span style:display=if has_last_progress { "inline" } else { "none" }>{format!("{}: {last_progress}", tr(TextKey::LastSuccessfulProgress))}</span>
                            <span style:display=if has_estimate { "inline" } else { "none" }>{format!("{}: {estimate}", tr(TextKey::EstimatedRemainingTime))}</span>
                        </p>
                        <p class=style::import_progress_details>
                            <span style:display=if has_archive_counts { "inline" } else { "none" }>{import_discovery_summary(&progress)}</span>
                            <span>{format!("{} / {} {}", progress.detail_complete, progress.detail_total, tr(TextKey::DetailsComplete))}</span>
                            <span>{format!("{} {}", progress.detail_skipped, tr(TextKey::DetailsSkipped))}</span>
                            <span style:display=if has_detail_failed { "inline" } else { "none" }>{format!("{} {}", progress.detail_failed, tr(TextKey::Failed))}</span>
                        </p>
                        <p class=style::review_error style:display=if has_diagnostic { "flex" } else { "none" }>{diagnostic}</p>
                        <p class=style::review_error style:display=if has_empty_import { "flex" } else { "none" }>
                            {tr(TextKey::EmptyImportRetryHelp)}
                        </p>
                        <div class=style::import_progress_actions>
                            {contextual_action}
                        </div>
                    </section>
                }
            })}

            <PublicationActivity list_id=id history=publication_history hold=auto_publication_hold />

            <section class=style::operator_panel aria-labelledby="addon-list-source-test-heading">
                <header class=style::operator_panel_header>
                    <div>
                        <h2 id="addon-list-source-test-heading">{tr(TextKey::TestSource)}</h2>
                        <p>{tr(TextKey::TestSourceHelp)}</p>
                    </div>
                    <form method="post" action="/admin-api/addon-lists/test-source">
                        <input type="hidden" name="id" value=id />
                        <input
                            type="hidden"
                            name="saved_revision"
                            value=move || publication_readiness.with(|value| value.saved_revision)
                        />
                        <button
                            type="submit"
                            class=style::btn_cancel
                            disabled=move || source_test_action.pending().get()
                            on:click=move |event| {
                                event.prevent_default();
                                if !source_test_action.pending().get_untracked() {
                                    source_test_action.dispatch((
                                        id,
                                        publication_readiness
                                            .with_untracked(|value| value.saved_revision),
                                    ));
                                }
                            }
                        >{tr(TextKey::TestSource)}</button>
                    </form>
                </header>
                {move || source_probe.get().map(|probe| {
                    let state_key = match probe.state {
                        AddonMediaListSourceProbeState::Queued => TextKey::SourceTestQueued,
                        AddonMediaListSourceProbeState::Running => TextKey::SourceTestRunning,
                        AddonMediaListSourceProbeState::Complete => match probe.outcome {
                            Some(AddonMediaListSourceProbeOutcome::Available) => TextKey::SourceAvailable,
                            Some(AddonMediaListSourceProbeOutcome::Unsupported) => TextKey::SourceUnsupported,
                            Some(AddonMediaListSourceProbeOutcome::Failed) | None => TextKey::SourceTestFailed,
                        },
                        AddonMediaListSourceProbeState::Stale => TextKey::SourceTestStale,
                    };
                    let diagnostic = probe.diagnostic.or(probe.diagnostic_code).unwrap_or_default();
                    let source_revision = probe.source_revision.unwrap_or_default();
                    let capabilities = probe.capabilities.join(", ");
                    let provider_id = probe.provider_id.unwrap_or_default();
                    let list_title = probe.list_title.unwrap_or_default();
                    let page_complete = probe.page_complete.map(|complete| {
                        tr(if complete { TextKey::Yes } else { TextKey::No }).to_owned()
                    }).unwrap_or_default();
                    view! {
                        <div class=style::source_probe role="status" aria-live="polite">
                            <strong>{tr(state_key)}</strong>
                            <dl class=style::operator_metrics>
                                <div><dt>{tr(TextKey::SavedRevision)}</dt><dd>{probe.saved_draft_revision}</dd></div>
                                <div style:display=if source_revision.is_empty() { "none" } else { "block" }><dt>{tr(TextKey::SourceRevision)}</dt><dd>{source_revision.clone()}</dd></div>
                                <div style:display=if probe.returned_item_count.is_none() { "none" } else { "block" }><dt>{tr(TextKey::ReturnedFirstPageItems)}</dt><dd>{probe.returned_item_count.unwrap_or_default()}</dd></div>
                                <div style:display=if probe.count_hint.is_none() { "none" } else { "block" }><dt>{tr(TextKey::ProviderTotalItemCount)}</dt><dd>{probe.count_hint.unwrap_or_default()}</dd></div>
                                <div style:display=if probe.duration_milliseconds.is_none() { "none" } else { "block" }><dt>{tr(TextKey::Duration)}</dt><dd>{format!("{} ms", probe.duration_milliseconds.unwrap_or_default())}</dd></div>
                                <div style:display=if capabilities.is_empty() { "none" } else { "block" }><dt>{tr(TextKey::Capabilities)}</dt><dd>{capabilities.clone()}</dd></div>
                                <div style:display=if provider_id.is_empty() { "none" } else { "block" }><dt>{tr(TextKey::ProviderIdentity)}</dt><dd>{provider_id.clone()}</dd></div>
                                <div style:display=if list_title.is_empty() { "none" } else { "block" }><dt>{tr(TextKey::Title)}</dt><dd>{list_title.clone()}</dd></div>
                                <div style:display=if page_complete.is_empty() { "none" } else { "block" }><dt>{tr(TextKey::FirstPageComplete)}</dt><dd>{page_complete.clone()}</dd></div>
                            </dl>
                            <p class=style::review_error style:display=if diagnostic.is_empty() { "none" } else { "flex" }>{diagnostic.clone()}</p>
                        </div>
                    }
                })}
            </section>

            <section class=style::operator_panel aria-labelledby="addon-list-draft-diff-heading">
                <header class=style::operator_panel_header>
                    <div>
                        <h2 id="addon-list-draft-diff-heading">{tr(TextKey::DraftChanges)}</h2>
                        <p>{tr(TextKey::ExactDraftDiffHelp)}</p>
                    </div>
                    <div class=style::js_only>
                        <Button
                            variant=ButtonVariant::Secondary
                            disabled=Signal::derive(move || reconcile_action.pending().get())
                            on_click=Some(Callback::new(move |()| {
                                reconcile_status_visible.set(false);
                                reconcile_action.dispatch(id);
                            }))
                        >
                            <span style=icon_style(IconName::Refresh, "1rem") aria-hidden="true"></span>
                            <span>{tr(TextKey::ReconcileEntireSource)}</span>
                        </Button>
                    </div>
                </header>
                <p>{tr(TextKey::ReconcileEntireSourceHelp)}</p>
                <p
                    id="addon-list-reconcile-status"
                    class=style::workflow_message
                    role="status"
                    aria-live="polite"
                    tabindex="-1"
                    style:display=move || if reconcile_status_visible.get() { "flex" } else { "none" }
                >{tr(TextKey::ReconcileQueued)}</p>
                <noscript>
                    <form method="post" action="/admin-api/addon-lists/reconcile">
                        <input type="hidden" name="id" value=id />
                        <button type="submit" class=style::btn_add>
                            <span style=icon_style(IconName::Refresh, "1rem") aria-hidden="true"></span>
                            <span>{tr(TextKey::ReconcileEntireSource)}</span>
                        </button>
                    </form>
                </noscript>
                <p class=style::empty style:display=move || if draft_diff.with(|value| value.exact) { "none" } else { "block" }>{tr(TextKey::DraftDiffUnavailable)}</p>
                <dl class=style::operator_metrics style:display=move || if draft_diff.with(|value| value.exact) { "grid" } else { "none" }>
                    <div><dt>{format!("{} · {}", tr(TextKey::Items), tr(TextKey::Added))}</dt><dd><a data-testid="draft-diff-added" href=format!("?review_category={}#addon-list-items-heading", draft_review_category_value(DraftReviewCategory::Added))>{move || draft_diff.with(|value| value.added)}</a></dd></div>
                    <div><dt>{tr(TextKey::MetadataChanges)}</dt><dd><a data-testid="draft-diff-metadata-changed" href=format!("?review_category={}#addon-list-items-heading", draft_review_category_value(DraftReviewCategory::MetadataChanged))>{move || draft_diff.with(|value| value.metadata_changed)}</a></dd></div>
                    <div><dt>{tr(TextKey::ExcludedItems)}</dt><dd><a data-testid="draft-diff-excluded" href=format!("?review_category={}#addon-list-items-heading", draft_review_category_value(DraftReviewCategory::Excluded))>{move || draft_diff.with(|value| value.excluded)}</a></dd></div>
                    <div><dt>{tr(TextKey::Unavailable)}</dt><dd><a data-testid="draft-diff-unavailable" href=format!("?review_category={}#addon-list-items-heading", draft_review_category_value(DraftReviewCategory::Unavailable))>{move || draft_diff.with(|value| value.unavailable)}</a></dd></div>
                    <div><dt>{tr(TextKey::UnknownAvailability)}</dt><dd><a data-testid="draft-diff-unknown" href=format!("?review_category={}#addon-list-items-heading", draft_review_category_value(DraftReviewCategory::Unknown))>{move || draft_diff.with(|value| value.unknown)}</a></dd></div>
                    <div><dt>{tr(TextKey::Conflicts)}</dt><dd><a data-testid="draft-diff-conflicted" href=format!("?review_category={}#addon-list-items-heading", draft_review_category_value(DraftReviewCategory::Conflicted))>{move || draft_diff.with(|value| value.conflicted)}</a></dd></div>
                    <div><dt>{tr(TextKey::PendingRemovals)}</dt><dd><a data-testid="draft-diff-pending-removals" href=format!("?review_category={}#addon-list-items-heading", draft_review_category_value(DraftReviewCategory::PendingRemoval))>{move || draft_diff.with(|value| value.pending_removals)}</a></dd></div>
                    <div><dt>{tr(TextKey::Reappearances)}</dt><dd><a data-testid="draft-diff-reappeared" href=format!("?review_category={}#addon-list-items-heading", draft_review_category_value(DraftReviewCategory::Reappeared))>{move || draft_diff.with(|value| value.reappeared)}</a></dd></div>
                    <div><dt>{tr(TextKey::RemovalCandidates)}</dt><dd><a data-testid="draft-diff-removal-candidates" href=format!("?review_category={}#addon-list-items-heading", draft_review_category_value(DraftReviewCategory::RemovalCandidate))>{move || draft_diff.with(|value| value.removal_candidates)}</a></dd></div>
                </dl>
                <div
                    class=style::revision_rows
                    style:display=move || if draft_diff.with(|value| value.pending_removals > 0) { "grid" } else { "none" }
                >
                    <For
                        each=move || {
                            let pending = draft_diff.with(|value| {
                                value.pending_removal_item_ids.iter().copied().collect::<BTreeSet<_>>()
                            });
                            items.with(|items| {
                                items
                                    .iter()
                                    .filter(|item| pending.contains(&item.id))
                                    .map(|item| (item.id, item.display_title.clone()))
                                    .collect::<Vec<_>>()
                            })
                        }
                        key=|(item_id, _)| *item_id
                        children=move |(item_id, title)| view! {
                            <div class=style::revision_row>
                                <div>
                                    <strong>{title}</strong>
                                    <span>{tr(TextKey::PendingRemovalHelp)}</span>
                                </div>
                                <span>{tr(TextKey::PendingRemovals)}</span>
                                <div class=style::js_only>
                                    <Button
                                        variant=ButtonVariant::Secondary
                                        disabled=Signal::derive(move || confirm_removal_action.pending().get())
                                        on_click=Some(Callback::new(move |()| {
                                            if let Some(generation) = draft_diff.with_untracked(|value| {
                                                value.pending_removal_generations.get(&item_id).copied()
                                            }) {
                                                confirm_removal_action.dispatch((id, item_id, generation));
                                            }
                                        }))
                                    >{tr(TextKey::ConfirmRemoval)}</Button>
                                </div>
                                <noscript>
                                    <form method="post" action="/admin-api/addon-lists/confirm-removal">
                                        <input type="hidden" name="list_id" value=id />
                                        <input type="hidden" name="item_id" value=item_id />
                                        <input
                                            type="hidden"
                                            name="expected_generation"
                                            value=move || draft_diff.with(|value| {
                                                value.pending_removal_generations
                                                    .get(&item_id)
                                                    .copied()
                                                    .unwrap_or_default()
                                            })
                                        />
                                        <button type="submit" class=style::btn_add>{tr(TextKey::ConfirmRemoval)}</button>
                                    </form>
                                </noscript>
                            </div>
                        }
                    />
                </div>
            </section>

            <section class=style::operator_panel aria-labelledby="addon-list-revision-history-heading">
                <header class=style::operator_panel_header>
                    <div>
                        <h2 id="addon-list-revision-history-heading">{tr(TextKey::RevisionHistory)}</h2>
                        <p>{tr(TextKey::RevisionHistoryHelp)}</p>
                    </div>
                </header>
                <p class=style::empty style:display=move || if revision_history.with(Vec::is_empty) { "block" } else { "none" }>{tr(TextKey::NoRevisionHistory)}</p>
                <form method="get" action=format!("/media/addon-lists/{id}") class=style::revision_compare>
                    <label>
                        <span>{tr(TextKey::Revision)}</span>
                        <select
                            name="compare_left"
                            required
                            on:change=move |event| compare_left.set(event_target_value(&event).parse().ok())
                        >
                            <For each=move || revision_history.get() key=|revision| revision.revision children=move |revision| view! {
                                <option value=revision.revision selected=move || compare_left.get() == Some(revision.revision)>{revision.revision}</option>
                            } />
                        </select>
                    </label>
                    <label>
                        <span>{tr(TextKey::Revision)}</span>
                        <select
                            name="compare_right"
                            required
                            on:change=move |event| compare_right.set(event_target_value(&event).parse().ok())
                        >
                            <For each=move || revision_history.get() key=|revision| revision.revision children=move |revision| view! {
                                <option value=revision.revision selected=move || compare_right.get() == Some(revision.revision)>{revision.revision}</option>
                            } />
                        </select>
                    </label>
                    <button
                        type="submit"
                        class=style::btn_cancel
                        disabled=move || compare_left.get().is_none() || compare_right.get().is_none() || compare_left.get() == compare_right.get() || compare_revision_action.pending().get()
                        on:click=move |event| {
                            if let Some((left, right)) = compare_left.get_untracked().zip(compare_right.get_untracked()) {
                                event.prevent_default();
                                compare_revision_action.dispatch((id, left, right));
                            }
                        }
                    >{tr(TextKey::CompareRevisions)}</button>
                </form>
                {move || revision_diff.get().map(|diff| view! {
                    <dl class=style::operator_metrics role="status" aria-live="polite" data-testid="revision-diff">
                        <div><dt>{format!("{} · {}", tr(TextKey::Items), tr(TextKey::Added))}</dt><dd>{diff.items_added}</dd></div>
                        <div><dt>{format!("{} · {}", tr(TextKey::Items), tr(TextKey::Removed))}</dt><dd>{diff.items_removed}</dd></div>
                        <div><dt>{format!("{} · {}", tr(TextKey::Items), tr(TextKey::Changed))}</dt><dd>{diff.items_changed}</dd></div>
                        <div><dt>{tr(TextKey::MetadataChanges)}</dt><dd>{u8::from(diff.list_metadata_changed)}</dd></div>
                        <div><dt>{tr(TextKey::EffectiveOptions)}</dt><dd>{u8::from(diff.effective_options_changed)}</dd></div>
                        <div><dt>{tr(TextKey::Files)}</dt><dd>{format!("+{} / -{} / ~{}", diff.files_added, diff.files_removed, diff.files_changed)}</dd></div>
                        <div><dt>{tr(TextKey::Artifacts)}</dt><dd>{format!("+{} / -{}", diff.artifacts_added, diff.artifacts_removed)}</dd></div>
                    </dl>
                })}
                <div class=style::revision_rows>
                    <For each=move || revision_history.get() key=|revision| revision.revision children=move |revision| {
                        let revision_number = revision.revision;
                        let restorable = revision.restorable;
                        let native_restore_disabled = !revision_can_restore(
                            restorable,
                            revision_number,
                            published_revision.get_untracked(),
                        );
                        view! {
                            <article class=style::revision_row>
                                <div>
                                    <strong>{format!("{} {}", tr(TextKey::Revision), revision_number)}</strong>
                                    <span>{revision.completed_at.unwrap_or_else(|| tr(TextKey::Unavailable).to_owned())}</span>
                                </div>
                                <dl class=style::revision_counts>
                                    <div><dt>{tr(TextKey::Items)}</dt><dd>{revision.published_item_count}</dd></div>
                                    <div><dt>{tr(TextKey::Files)}</dt><dd>{revision.file_count}</dd></div>
                                    <div><dt>{tr(TextKey::Artifacts)}</dt><dd>{revision.artifact_count}</dd></div>
                                </dl>
                                <form method="post" action="/admin-api/addon-lists/restore-revision">
                                    <input type="hidden" name="id" value=id />
                                    <input type="hidden" name="source_published_revision" value=revision_number />
                                    <input type="hidden" name="expected_draft_revision" value=move || publication_readiness.with(|value| value.saved_revision) />
                                    <input type="hidden" name="expected_published_revision" value=move || published_revision.get() />
                                    <noscript>
                                        <label class=style::restore_confirmation>
                                            <input type="checkbox" required />
                                            <span>{tr(TextKey::RevisionHistoryHelp)}</span>
                                        </label>
                                        <button type="submit" class=style::btn_cancel disabled=native_restore_disabled>{tr(TextKey::RestoreRevision)}</button>
                                    </noscript>
                                    <button
                                        type="button"
                                        class=format!("{} {}", style::btn_cancel, style::js_only)
                                        disabled=move || {
                                            !revision_can_restore(
                                                restorable,
                                                revision_number,
                                                published_revision.get(),
                                            )
                                        }
                                        on:click=move |_| {
                                            restore_revision.set(Some(revision_number));
                                            restore_open.set(true);
                                        }
                                    >{tr(TextKey::RestoreRevision)}</button>
                                </form>
                            </article>
                        }
                    } />
                </div>
            </section>

            <form
                id="addon_media_lists-list-editor-form"
                method="post"
                action="/admin-api/addon-lists/save"
                class=style::editor
                style:display=move || if form_found { "" } else { "none" }
            >
                            <p class=style::draft_note style:display=if is_draft { "" } else { "none" }>
                                <span style=icon_style(IconName::Info, "1rem") aria-hidden="true"></span>
                                <span>{tr(TextKey::MetadataDraftNotice)}</span>
                            </p>
                            <p class=style::empty style:display=move || if has_presentation_error { "" } else { "none" }>
                                {presentation_error}
                            </p>
                            <p class=style::review_error role="alert" style:display=move || if has_review_error { "" } else { "none" }>
                                <span style=icon_style(IconName::Warning, "1rem") aria-hidden="true"></span>
                                <span>{review_error}</span>
                            </p>
                            <input type="hidden" name="id" value=move || form_id.get().to_string()/>
                            <input type="hidden" name="saved_revision" value=move || publication_readiness.with(|readiness| readiness.saved_revision.to_string())/>
                            <input type="hidden" name="metadata_etag" value=move || show_etag.get()/>
                            <div class=style::field_row>
                                <div class=style::field_label>{tr(TextKey::AddonMediaListUrl)}</div>
                                <div class=style::url_summary>
                                    <input
                                        class=style::text_input
                                        type="text"
                                        value=display_id
                                        disabled
                                    />
                                    <small>
                                        <span style=icon_style(IconName::Check, "0.9rem") aria-hidden="true"></span>
                                        {format!("{} {detected_provider_name}", tr(TextKey::AddonAutoDetected))}
                                    </small>
                                </div>
                            </div>
                            <div class=style::field_row data-testid="addon-media-list-addon">
                                <div class=style::field_label>{tr(TextKey::Addon)}</div>
                                <div class=style::addon_row>
                                    <select
                                        class=style::text_input
                                        aria-label=tr(TextKey::Addon)
                                        prop:value=move || selected_addon_id.get()
                                        on:change=move |event| {
                                            selected_addon_id.set(event_target_value(&event));
                                        }
                                    >
                                        {providers.into_iter().map(|provider| view! {
                                            <option value=provider.addon_id>{provider.name}</option>
                                        }).collect_view()}
                                    </select>
                                    <button
                                        type="button"
                                        class=style::btn_cancel
                                        disabled=move || {
                                            selected_addon_id.get() == form_addon_id
                                                || change_addon_action.pending().get()
                                        }
                                        on:click=move |_| change_addon_open.set(true)
                                    >{tr(TextKey::ChangeAddon)}</button>
                                    <small>{tr(TextKey::ChangeAddonHelp)}</small>
                                </div>
                            </div>
                            <a href="#addon-list-disk-warning-settings">{tr(TextKey::AddonListDiskWarnings)}</a>
                            <div class=style::editor_source_pair>
                                <DeliveryModeSelect
                                    mode=delivery_mode
                                    saved_mode=saved_delivery_mode
                                    cutoff_at=delivery_cutoff_at
                                    supports_download=Signal::derive(move || supports_download_ahead)
                                />
                                <label class=style::source_field>
                                    <span class=style::field_title>
                                        <span style=move || icon_style(presentation_icon(presentation_kind.get()), "1rem") aria-hidden="true"></span>
                                        <span>{tr(TextKey::CollectionType)}</span>
                                    </span>
                                    <select
                                        class=style::boxed_input
                                        name="presentation_kind"
                                        aria-label=tr(TextKey::CollectionType)
                                        prop:value=move || presentation_kind_value(presentation_kind.get())
                                        on:change=move |event| {
                                            let next = presentation_kind_from_value(&event_target_value(&event));
                                            presentation_kind.set(next);
                                            content_kind_override.set(presentation_content_kind(next));
                                        }
                                    >
                                        <option value="auto">{tr(TextKey::CollectionTypeAuto)}</option>
                                        <option value="television_show">{tr(TextKey::TvShows)}</option>
                                        <option value="movies">{tr(TextKey::Movies)}</option>
                                        <option value="music_videos">{tr(TextKey::MusicVideo)}</option>
                                        <option value="other_videos">{tr(TextKey::OtherVideos)}</option>
                                        <option value="songs">{tr(TextKey::Songs)}</option>
                                        <option value="images">{tr(TextKey::Images)}</option>
                                        <option value="remote_streams">{tr(TextKey::RemoteStreams)}</option>
                                    </select>
                                </label>
                                <div class=style::source_field>
                                    <span class=style::field_title>
                                        <span style=move || icon_style(refresh_cadence_icon(refresh_cadence.get()), "1rem") aria-hidden="true"></span>
                                        <span>{tr(TextKey::AutomaticRefresh)}</span>
                                    </span>
                                    <RefreshCadenceSelect
                                        cadence=refresh_cadence
                                        supports_adaptive=Signal::derive(move || supports_adaptive_refresh)
                                        preserve_unavailable_adaptive=saved_refresh_cadence == AddonMediaListRefreshCadence::Adaptive
                                        class_name=style::boxed_input
                                    />
                                    <p
                                        class=style::review_error
                                        role="alert"
                                        data-testid="adaptive-refresh-unavailable"
                                        style:display=move || if !supports_adaptive_refresh && refresh_cadence.get() == AddonMediaListRefreshCadence::Adaptive { "flex" } else { "none" }
                                    >
                                        <span style=icon_style(IconName::Warning, "1rem") aria-hidden="true"></span>
                                        <span>{tr(TextKey::AdaptiveRefreshUnavailable)}</span>
                                    </p>
                                    <dl class=style::operator_metrics data-testid="addon-list-refresh-status">
                                        <div><dt>{tr(TextKey::LastSuccessfulRefresh)}</dt><dd>{last_refresh_succeeded_at}</dd></div>
                                        <div><dt>{tr(TextKey::NextScheduledRefresh)}</dt><dd>{next_refresh_at}</dd></div>
                                        <div><dt>{tr(TextKey::RefreshRetryAt)}</dt><dd>{refresh_retry_at}</dd></div>
                                    </dl>
                                    <input type="hidden" name="refresh_publication_policy" value="automatic"/>
                                    <p id="refresh-publication-policy-help">{tr(TextKey::RefreshPublicationPolicyHelp)}</p>
                                </div>
                                <p class=style::type_change_note data-testid="addon-media-type-change-help">
                                    <span style=icon_style(IconName::Edit, "1rem") aria-hidden="true"></span>
                                    <span class=style::type_change_copy>
                                        <span
                                            class=style::type_change_route
                                            aria-live="polite"
                                            style:display=if has_published_projection { "inline" } else { "none" }
                                        >
                                            <strong>{format!("{}: ", tr(TextKey::CurrentLibraryType))}</strong>
                                            <span>{tr(presentation_text_key(published_presentation_kind))}</span>
                                            <span
                                                data-testid="addon-media-type-change-route"
                                                style:display=move || if matches!(presentation_kind.get(), AddonMediaListPresentationKind::Auto) || presentation_kind.get() == published_presentation_kind { "none" } else { "inline" }
                                            >
                                                <span aria-hidden="true">" → "</span>
                                                <span>{move || tr(presentation_text_key(presentation_kind.get()))}</span>
                                            </span>
                                        </span>
                                        <span>{tr(TextKey::CollectionTypeChangeHelp)}</span>
                                    </span>
                                </p>
                            </div>
                            <div class=style::field_row>
                                <div class=style::field_label>
                                    <span class=style::field_title>
                                        <span style=icon_style(IconName::Folder, "1rem") aria-hidden="true"></span>
                                        <span>{tr(TextKey::AddonMediaListFolder)}</span>
                                    </span>
                                </div>
                                <input class=style::text_input value=display_folder disabled/>
                            </div>
                            <div
                                id="addon-list-general-metadata"
                                class=style::general_metadata_panel
                                data-testid="addon-general-metadata"
                            >
                            <MetadataConflictSummary conflicts=show_metadata_conflicts.clone()/>
                            <div class=style::field_row>
                                <div class=style::field_label>{tr(TextKey::OriginalTitle)}</div>
                                <input class=style::text_input name="metadata_original_title" aria-label=tr(TextKey::OriginalTitle) value=show_original_title.get_untracked() prop:value=move || show_original_title.get() on:input=move |event| show_original_title.set(event_target_value(&event))/>
                            </div>
                            <div class=style::field_row>
                                <div class=style::field_label>{tr(TextKey::SortTitle)}</div>
                                <input class=style::text_input name="metadata_sort_title" aria-label=tr(TextKey::SortTitle) value=show_sort_title.get_untracked() prop:value=move || show_sort_title.get() on:input=move |event| show_sort_title.set(event_target_value(&event))/>
                            </div>
                            <div class=style::field_row>
                                <div class=style::field_label>{tr(TextKey::Year)}</div>
                                <input class=style::text_input name="metadata_year" aria-label=tr(TextKey::Year) inputmode="numeric" value=show_year.get_untracked() prop:value=move || show_year.get() on:input=move |event| show_year.set(event_target_value(&event))/>
                            </div>
                            <div class=format!("{} {}", style::field_row, style::metadata_wide_row)>
                                <div class=style::field_label>{tr(TextKey::Summary)}</div>
                                <textarea class=style::boxed_textarea name="metadata_plot" aria-label=tr(TextKey::Summary) rows="4" prop:value=move || show_plot.get() on:input=move |event| show_plot.set(event_target_value(&event))>{show_plot.get_untracked()}</textarea>
                            </div>
                            <div class=style::field_row>
                                <div class=style::field_label>{tr(TextKey::Genres)}</div>
                                <input class=style::text_input name="metadata_genres" aria-label=tr(TextKey::Genres) value=show_genres.get_untracked() prop:value=move || show_genres.get() on:input=move |event| show_genres.set(event_target_value(&event))/>
                            </div>
                            <div class=format!("{} {}", style::field_row, style::general_tag_row)>
                                <div class=style::field_label>{tr(TextKey::Tags)}</div>
                                <div class=style::tag_field>
                                    <textarea class=style::tag_input name="metadata_tags" aria-label=tr(TextKey::Tags) rows="4" spellcheck="false" prop:value=move || show_tags.get() on:input=move |event| show_tags.set(event_target_value(&event))>{show_tags.get_untracked()}</textarea>
                                    <small>{move || format!("{} · {}", line_values(&show_tags.get()).len(), tr(TextKey::OneTagPerLine))}</small>
                                </div>
                            </div>
                            <div class=style::field_row>
                                <div class=style::field_label>{tr(TextKey::Studios)}</div>
                                <input class=style::text_input name="metadata_studios" aria-label=tr(TextKey::Studios) value=show_studios.get_untracked() prop:value=move || show_studios.get() on:input=move |event| show_studios.set(event_target_value(&event))/>
                            </div>
                            <div class=style::field_row>
                                <div class=style::field_label>{tr(TextKey::Directors)}</div>
                                <input class=style::text_input name="metadata_directors" aria-label=tr(TextKey::Directors) value=show_directors.get_untracked() prop:value=move || show_directors.get() on:input=move |event| show_directors.set(event_target_value(&event))/>
                            </div>
                            <div class=style::field_row>
                                <div class=style::field_label>{tr(TextKey::Writers)}</div>
                                <input class=style::text_input name="metadata_writers" aria-label=tr(TextKey::Writers) value=show_writers.get_untracked() prop:value=move || show_writers.get() on:input=move |event| show_writers.set(event_target_value(&event))/>
                            </div>
                            <div class=style::field_row>
                                <div class=style::field_label>{tr(TextKey::Artists)}</div>
                                <input class=style::text_input name="metadata_artists" aria-label=tr(TextKey::Artists) value=show_artists.get_untracked() prop:value=move || show_artists.get() on:input=move |event| show_artists.set(event_target_value(&event))/>
                            </div>
                            <div class=style::field_row>
                                <div class=style::field_label>{tr(TextKey::Languages)}</div>
                                <input class=style::text_input name="metadata_languages" aria-label=tr(TextKey::Languages) value=show_languages.get_untracked() prop:value=move || show_languages.get() on:input=move |event| show_languages.set(event_target_value(&event))/>
                            </div>
                            <div class=format!("{} {}", style::field_row, style::general_tag_row)>
                                <div class=style::field_label>{tr(TextKey::CastAndCredits)}</div>
                                <div>
                                    <textarea class=style::boxed_textarea name="metadata_people" aria-label=tr(TextKey::CastAndCredits) rows="3" spellcheck="false" prop:value=move || show_people.get() on:input=move |event| show_people.set(event_target_value(&event))>{show_people.get_untracked()}</textarea>
                                    <small class=style::field_hint>{tr(TextKey::OneCreditPerLine)}</small>
                                </div>
                            </div>
                            <div class=format!("{} {}", style::field_row, style::general_artwork_row)>
                                <div class=style::field_label>{tr(TextKey::Artwork)}</div>
                                <div class=style::general_artwork_fields>
                                    <ArtworkRoleInput list_id=id item_id=None role=AddonMediaListArtworkRole::Poster label=TextKey::Poster url=show_poster etag=show_etag candidates=show_poster_candidates.clone()/>
                                    <ArtworkRoleInput list_id=id item_id=None role=AddonMediaListArtworkRole::Fanart label=TextKey::FanArt url=show_fanart etag=show_etag candidates=show_fanart_candidates.clone()/>
                                    <ArtworkRoleInput list_id=id item_id=None role=AddonMediaListArtworkRole::Thumb label=TextKey::Thumbnail url=show_thumbnail etag=show_etag candidates=show_thumbnail_candidates.clone()/>
                                </div>
                            </div>
                            </div>
                            <div class=style::field_row>
                                <div class=style::field_label>{tr(TextKey::GeneratePlaylist)}</div>
                                <label class=style::checkbox_row>
                                    <input
                                        type="checkbox"
                                        name="generate_playlist"
                                        value="true"
                                        checked=checked_generate_playlist
                                        prop:checked=move || generate_playlist.get()
                                        on:change=move |event| {
                                            generate_playlist.set(event_target_checked(&event));
                                        }
                                    />
                                    <small>{tr(TextKey::GeneratePlaylistFromSortedAddonMediaListItems)}</small>
                                </label>
                            </div>
                            <div class=style::field_row>
                                <div class=style::field_label>{tr(TextKey::Details)}</div>
                                <label class=style::checkbox_row>
                                    <input
                                        type="checkbox"
                                        name="load_item_details"
                                        value="true"
                                        prop:checked=move || detail_import_policy.get() == AddonMediaListDetailPolicy::Automatic
                                        on:change=move |event| {
                                            detail_import_policy.set(if event_target_checked(&event) {
                                                AddonMediaListDetailPolicy::Automatic
                                            } else {
                                                AddonMediaListDetailPolicy::ManualOnly
                                            });
                                        }
                                    />
                                    <span>
                                        <b>{tr(TextKey::LoadItemDetailsAfterPublishing)}</b>
                                        <small>{tr(TextKey::LoadItemDetailsHelp)}</small>
                                    </span>
                                </label>
                            </div>
            </form>
            <section class=style::content style:display=move || if form_found { "" } else { "none" }>
                <section class=style::item_overview aria-labelledby="addon-list-items-heading">
                    <header class=style::item_overview_heading>
                        <div>
                            <h2 id="addon-list-items-heading">
                                {move || tr(item_section_heading(presentation_kind.get()))}
                            </h2>
                            <p>{move || tr(item_section_help(presentation_kind.get()))}</p>
                        </div>
                        <button
                            type="button"
                            class=style::numbering_toggle
                            style:display=move || if episode_numbering_available.get() { "inline-flex" } else { "none" }
                            aria-expanded=move || if episode_numbering_open.get() { "true" } else { "false" }
                            aria-controls="episode-numbering-panel"
                            on:click=move |_| episode_numbering_open.update(|open| *open = !*open)
                        >
                            <span style=icon_style(IconName::Edit, "1rem") aria-hidden="true"></span>
                            <span>{tr(TextKey::NumberEpisodes)}</span>
                        </button>
                    </header>
                    {active_draft_category.map(|category| view! {
                        <aside class=style::draft_filter role="status" data-testid="draft-review-filter">
                            <strong>{tr(draft_review_category_text_key(category))}</strong>
                            <a href=format!("/media/addon-lists/{id}#addon-list-items-heading")>{tr(TextKey::ClearFilters)}</a>
                        </aside>
                    })}
                    <section
                        id="episode-numbering-panel"
                        class=style::numbering_panel
                        aria-label=tr(TextKey::NumberEpisodes)
                        style:display=move || if episode_numbering_available.get() && episode_numbering_open.get() { "grid" } else { "none" }
                    >
                        <p>{tr(TextKey::NumberEpisodesHelp)}</p>
                        <div class=style::numbering_mode>
                            <label class=style::checkbox_row>
                                <input
                                    type="checkbox"
                                    prop:checked=move || episode_numbering_auto_detect.get()
                                    on:change=move |event| {
                                        episode_numbering_auto_detect.set(event_target_checked(&event));
                                    }
                                />
                                <small>{tr(TextKey::AutoDetectSeasons)}</small>
                            </label>
                            <small>{tr(TextKey::AutoDetectSeasonsHelp)}</small>
                        </div>
                        <div class=style::numbering_cursor>
                            <span>{tr(TextKey::NumberingStartsAt)}</span>
                            <strong>{move || episode_numbering_start_item_id.get().and_then(|item_id| {
                                items.with(|items| items.iter().find(|item| item.id == item_id).map(|item| {
                                    format!("#{} · {}", item.rank.saturating_add(1), item.display_title)
                                }))
                            }).unwrap_or_else(|| tr(TextKey::FirstIncludedItem).to_string())}</strong>
                            <button
                                type="button"
                                class=style::start_here
                                disabled=move || episode_numbering_start_item_id.get().is_none()
                                on:click=move |_| episode_numbering_start_item_id.set(None)
                            >{tr(TextKey::StartWithFirstItem)}</button>
                        </div>
                        <div class=style::numbering_fields>
                            <label class=style::metadata_field>
                                <span>{tr(TextKey::Season)}</span>
                                <input
                                    class=style::boxed_input
                                    type="number"
                                    min="0"
                                    step="1"
                                    inputmode="numeric"
                                    aria-label=tr(TextKey::Season)
                                    prop:value=move || episode_numbering_season.get()
                                    on:input=move |event| episode_numbering_season.set(event_target_value(&event))
                                />
                            </label>
                            <label class=style::metadata_field>
                                <span>{tr(TextKey::FirstEpisode)}</span>
                                <input
                                    class=style::boxed_input
                                    type="number"
                                    min="1"
                                    step="1"
                                    inputmode="numeric"
                                    aria-label=tr(TextKey::FirstEpisode)
                                    disabled=move || episode_numbering_auto_detect.get()
                                    prop:value=move || episode_numbering_start.get()
                                    on:input=move |event| episode_numbering_start.set(event_target_value(&event))
                                />
                            </label>
                            <label class=style::metadata_field>
                                <span>{tr(TextKey::NumberToAssign)}</span>
                                <input
                                    class=style::boxed_input
                                    type="number"
                                    min="1"
                                    step="1"
                                    inputmode="numeric"
                                    aria-label=tr(TextKey::NumberToAssign)
                                    placeholder=tr(TextKey::AllRemaining)
                                    prop:value=move || episode_numbering_count.get()
                                    on:input=move |event| episode_numbering_count.set(event_target_value(&event))
                                />
                            </label>
                            <label
                                class=style::metadata_field
                                style:display=move || if episode_numbering_auto_detect.get() { "grid" } else { "none" }
                            >
                                <span>{tr(TextKey::MinimumSeasonGapDays)}</span>
                                <input
                                    class=style::boxed_input
                                    type="number"
                                    min="1"
                                    step="1"
                                    inputmode="numeric"
                                    aria-label=tr(TextKey::MinimumSeasonGapDays)
                                    prop:value=move || episode_numbering_minimum_gap_days.get()
                                    on:input=move |event| episode_numbering_minimum_gap_days.set(event_target_value(&event))
                                />
                            </label>
                        </div>
                        <div class=style::numbering_footer>
                            <div>
                                <label class=style::checkbox_row>
                                    <input
                                        type="checkbox"
                                        disabled=move || episode_numbering_auto_detect.get()
                                        prop:checked=move || replace_existing_episode_numbers.get()
                                        on:change=move |event| replace_existing_episode_numbers.set(event_target_checked(&event))
                                    />
                                    <small>{tr(TextKey::ReplaceExistingEpisodeNumbers)}</small>
                                </label>
                                <small class=style::numbering_preview aria-live="polite">
                                    {move || {
                                        match episode_numbering_preview.get() {
                                            Some((count, (first_season, first_episode), (last_season, last_episode))) => {
                                                format!("{count} {} · S{first_season:02}E{first_episode:02}–S{last_season:02}E{last_episode:02}", tr(TextKey::Episodes))
                                            }
                                            _ => format!("0 {}", tr(TextKey::Episodes)),
                                        }
                                    }}
                                </small>
                                <small
                                    class=style::numbering_off_page
                                    aria-live="polite"
                                    style:display=move || { if episode_numbering_off_page_count.get() > 0 { "block" } else { "none" } }
                                >
                                    {move || format!("{} {}", episode_numbering_off_page_count.get(), tr(TextKey::OtherPageItemsAffected))}
                                </small>
                            </div>
                            <button
                                type="button"
                                class=style::btn_add
                                disabled=move || {
                                    review_edit_action.pending().get()
                                        || episode_numbering_assignment_count.get() == 0
                                        || !matches!(episode_numbering_season.get().trim().parse::<i32>(), Ok(value) if value >= 0)
                                        || (!episode_numbering_auto_detect.get()
                                            && !matches!(episode_numbering_start.get().trim().parse::<i32>(), Ok(value) if value >= 1))
                                        || (episode_numbering_auto_detect.get()
                                            && !matches!(episode_numbering_minimum_gap_days.get().trim().parse::<i64>(), Ok(value) if value >= 1))
                                }
                                on:click=move |_| {
                                    let assignments = episode_numbering_row_preview.get_untracked();
                                    let previous = items.with_untracked(|items| {
                                        items
                                            .iter()
                                            .filter(|item| assignments.contains_key(&item.id))
                                            .map(review_snapshot)
                                            .collect::<Vec<_>>()
                                    });
                                    let desired = previous
                                        .iter()
                                        .map(|snapshot| {
                                            let mut desired = *snapshot;
                                            if let Some((season, episode)) = assignments.get(&snapshot.item_id) {
                                                desired.season = Some(*season);
                                                desired.episode = Some(*episode);
                                            }
                                            desired
                                        })
                                        .collect::<Vec<_>>();
                                    if !desired.is_empty() {
                                        row_save_states.update(|states| {
                                            for edit in &desired {
                                                states.insert(edit.item_id, RowSaveState::Saving);
                                            }
                                        });
                                        review_edit_action.dispatch((
                                            id,
                                            desired,
                                            ReviewUndoEntry {
                                                label: tr(TextKey::NumberEpisodes).to_string(),
                                                snapshots: previous,
                                            },
                                        ));
                                        episode_numbering_open.set(false);
                                    }
                                }
                            >
                                <span>{tr(TextKey::Apply)}</span>
                            </button>
                        </div>
                    </section>
                    <div class=style::discovery_summary aria-live="polite">
                        <strong>{move || format!("{} {}", items.with(Vec::len), tr(TextKey::Discovered))}</strong>
                        <span aria-hidden="true">"·"</span>
                        <button
                            type="button"
                            class=style::review_filter
                            disabled=move || need_review_count.get() == 0
                            aria-pressed=move || if need_review_filter.get() { "true" } else { "false" }
                            on:click=move |_| {
                                item_page.set(0);
                                need_review_filter.update(|value| *value = !*value);
                            }
                        >
                            <span style=icon_style(IconName::Warning, "1rem") aria-hidden="true"></span>
                            <span>{move || format!("{} {}", need_review_count.get(), tr(TextKey::ReviewRecommended))}</span>
                        </button>
                    </div>
                    <section class=style::bulk_edit_toolbar aria-label=tr(TextKey::BulkEdit)>
                        <form method="post" action="/admin-api/addon-lists/options/edit" hidden=!options_available>
                            <input type="hidden" name="list_id" value=id.to_string()/>
                            <label>
                                <span>{tr(TextKey::AddonOptions)}</span>
                                <select name="scope" class=style::page_size aria-label=format!("{} · {}", tr(TextKey::AddonOptions), tr(TextKey::Scope))>
                                    {move || option_scopes.get().into_iter().map(|(label, value)| view! { <option value=value>{tr(label)}</option> }).collect_view()}
                                </select>
                            </label>
                            <button type="submit" class=style::undo_button disabled=move || items.with(Vec::is_empty)>{tr(TextKey::AddonOptions)}</button>
                        </form>
                        <label>
                            <span>{tr(TextKey::Scope)}</span>
                            <select
                                class=style::page_size
                                aria-label=tr(TextKey::Scope)
                                prop:value=move || bulk_scope_value.get()
                                on:change=move |event| bulk_scope_value.set(event_target_value(&event))
                            >
                                <option value="all">{tr(TextKey::AllItems)}</option>
                                <option value="current_page">{tr(TextKey::CurrentPage)}</option>
                                <option value="needs_review">{tr(TextKey::ItemsNeedingReview)}</option>
                            </select>
                        </label>
                        <label>
                            <span>{tr(TextKey::MediaKind)}</span>
                            <select
                                class=style::page_size
                                aria-label=format!("{} · {}", tr(TextKey::BulkEdit), tr(TextKey::MediaKind))
                                prop:value=move || bulk_content_kind_value.get()
                                on:change=move |event| bulk_content_kind_value.set(event_target_value(&event))
                            >
                                <option value="unchanged">{tr(TextKey::KeepCurrentValue)}</option>
                                <option value="auto">{tr(TextKey::LeaveEmptyToAutoDetect)}</option>
                                <option value="television_episode">{tr(TextKey::TelevisionEpisode)}</option>
                                <option value="movie">{tr(TextKey::Movie)}</option>
                                <option value="music_video">{tr(TextKey::MusicVideo)}</option>
                                <option value="other_video">{tr(TextKey::OtherVideo)}</option>
                                <option value="song">{tr(TextKey::Song)}</option>
                                <option value="image">{tr(TextKey::Image)}</option>
                            </select>
                        </label>
                        <label>
                            <span>{tr(TextKey::PlaybackBehavior)}</span>
                            <select
                                class=style::page_size
                                aria-label=format!("{} · {}", tr(TextKey::BulkEdit), tr(TextKey::PlaybackBehavior))
                                prop:value=move || bulk_liveness_value.get()
                                on:change=move |event| bulk_liveness_value.set(event_target_value(&event))
                            >
                                <option value="unchanged">{tr(TextKey::KeepCurrentValue)}</option>
                                <option value="provider">{tr(TextKey::UseProviderValue)}</option>
                                <option value="finite">{tr(TextKey::FiniteMedia)}</option>
                                <option value="live">{tr(TextKey::LiveSource)}</option>
                            </select>
                        </label>
                        <button
                            type="button"
                            class=style::undo_button
                            disabled=move || last_review_undo.with(Option::is_none) || undo_review_edit_action.pending().get()
                            on:click=move |_| {
                                if let Some(undo) = last_review_undo.get_untracked() {
                                    undo_review_edit_action.dispatch((id, undo.snapshots));
                                }
                            }
                        >
                            {move || last_review_undo.with(|undo| undo.as_ref().map_or_else(
                                || tr(TextKey::UndoLastChange).to_string(),
                                |undo| format!("{}: {}", tr(TextKey::Undo), undo.label),
                            ))}
                        </button>
                        <button
                            type="button"
                            class=style::btn_add
                            disabled=move || {
                                review_edit_action.pending().get()
                                    || bulk_scope_count.get() == 0
                                    || (bulk_content_kind_value.get() == "unchanged"
                                        && bulk_liveness_value.get() == "unchanged")
                            }
                            on:click=move |_| {
                                let (start, end, _, _, _) = item_page_range.get_untracked();
                                let scoped = items.with_untracked(|items| {
                                    draft_diff.with_untracked(|diff| {
                                        let category_items = items
                                            .iter()
                                            .filter(|item| item_matches_draft_category(
                                                item.id,
                                                active_draft_category,
                                                diff,
                                            ))
                                            .cloned()
                                            .collect::<Vec<_>>();
                                        bulk_scope_items(
                                            &category_items,
                                            &bulk_scope_value.get_untracked(),
                                            presentation_kind.get_untracked(),
                                            need_review_filter.get_untracked(),
                                            start,
                                            end,
                                        )
                                    })
                                });
                                let content_kind = bulk_content_kind_value.get_untracked();
                                let liveness = bulk_liveness_value.get_untracked();
                                let mut previous = Vec::new();
                                let mut desired = Vec::new();
                                for item in scoped {
                                    let before = review_snapshot(&item);
                                    let mut after = before;
                                    if content_kind != "unchanged" {
                                        after.content_kind = content_kind_from_value(&content_kind);
                                    }
                                    if liveness != "unchanged"
                                        && item.kind == AddonMediaListItemKind::RemoteStream
                                    {
                                        after.liveness_override = liveness_override_from_value(&liveness);
                                    }
                                    if before != after {
                                        previous.push(before);
                                        desired.push(after);
                                    }
                                }
                                if !desired.is_empty() {
                                    row_save_states.update(|states| {
                                        for edit in &desired {
                                            states.insert(edit.item_id, RowSaveState::Saving);
                                        }
                                    });
                                    review_edit_action.dispatch((
                                        id,
                                        desired,
                                        ReviewUndoEntry {
                                            label: tr(TextKey::BulkEdit).to_string(),
                                            snapshots: previous,
                                        },
                                    ));
                                }
                            }
                        >{move || format!("{} ({})", tr(TextKey::Apply), bulk_scope_count.get())}</button>
                    </section>
                    <div class=style::review_collection>
                        <p class=style::empty style:display=move || if filtered_item_count.get() == 0 { "block" } else { "none" }>
                            {tr(TextKey::NoItemsToDisplay)}
                        </p>
                        <div class=style::item_column_header data-testid="addon-item-column-header" aria-hidden="true">
                            <span>{tr(TextKey::Title)}</span>
                            <span>{tr(TextKey::Details)}</span>
                            <span>{tr(TextKey::Actions)}</span>
                        </div>
                        <ol class=style::metadata_items data-testid="addon-metadata-items">
                                <For
                                    each=move || {
                                        let (start, end, _, _, _) = item_page_range.get();
                                        items
                                            .get()
                                            .into_iter()
                                            .filter(|item| {
                                                draft_diff.with(|diff| {
                                                    item_matches_draft_category(
                                                        item.id,
                                                        active_draft_category,
                                                        diff,
                                                    )
                                                }) && (!need_review_filter.get()
                                                    || item_needs_review(item, presentation_kind.get()))
                                            })
                                            .skip(start)
                                            .take(end.saturating_sub(start))
                                            .collect::<Vec<_>>()
                                    }
                                    key=move |item| format!(
                                        "{}:{:?}:{}:{:?}:{:?}:{:?}:{:?}:{}:{:?}:{:?}",
                                        item.id,
                                        item.content_kind,
                                        item.included,
                                        item.liveness_override,
                                        item.season,
                                        item.episode,
                                        item.collection_role,
                                        item.replacement_media_version_id.unwrap_or_default(),
                                        item.detail_state,
                                        row_save_states.with(|states| states.get(&item.id).copied()),
                                    )
                                    children=move |item| item_row_view(
                                        id,
                                        item,
                                        replacement_target,
                                        replacement_query,
                                        search_action,
                                        replacement_action,
                                        review_edit_action,
                                        row_save_states,
                                        readiness_refresh_pending,
                                        item_role_action,
                                        episode_numbering_row_preview,
                                        episode_numbering_open,
                                        episode_numbering_start_item_id,
                                        presentation_kind,
                                        request_detail_action,
                                        detail_request_item,
                                    )
                                />
                        </ol>
                    </div>
                    <PaginationControls
                        page=Signal::derive(move || item_page_range.get().3)
                        total=Signal::derive(move || {
                            Some(u64::try_from(filtered_item_count.get()).unwrap_or(u64::MAX))
                        })
                        page_size=Signal::derive(move || item_page_size.get())
                        page_size_options=Some(ITEM_PAGE_SIZES.to_vec())
                        on_page_size_change=Callback::new(move |value: u32| {
                            item_page.set(0);
                            item_page_size.set(value);
                        })
                        on_previous=Callback::new(move |()| {
                            let current_page = item_page_range.get().3;
                            item_page.set(current_page.saturating_sub(1));
                        })
                        on_next=Callback::new(move |()| {
                            let (_, _, _, current_page, total_pages) = item_page_range.get();
                            item_page.set(
                                current_page.saturating_add(1).min(total_pages.saturating_sub(1)),
                            );
                        })
                    />
                </section>
                <section
                    class=style::destructive_actions
                    aria-labelledby="addon-list-destructive-actions-heading"
                    data-testid="addon-list-destructive-actions"
                >
                    <header>
                        <div>
                            <h2 id="addon-list-destructive-actions-heading">
                                {tr(TextKey::DestructiveActions)}
                            </h2>
                            <p>{tr(TextKey::ResetAddonMediaListHelp)}</p>
                        </div>
                        <div class=style::js_only>
                            <Button
                                variant=ButtonVariant::Danger
                                disabled=Signal::derive(move || reset_action.pending().get())
                                on_click=Some(Callback::new(move |()| {
                                    reset_confirmed_name.set(String::new());
                                    reset_open.set(true);
                                }))
                            >
                                <span style=icon_style(IconName::Refresh, "1rem") aria-hidden="true"></span>
                                <span>{tr(TextKey::ResetAndReload)}</span>
                            </Button>
                        </div>
                    </header>
                    <p
                        id="addon-list-reset-status"
                        class=style::workflow_message
                        role="status"
                        aria-live="polite"
                        tabindex="-1"
                        style:display=move || if reset_status_visible.get() { "flex" } else { "none" }
                    >{tr(TextKey::ResetQueued)}</p>
                    <AddonMediaListResetConsequences
                        list_name=list_name.get_value()
                        source_host=source_host.get_value()
                        impact=reset_impact
                    />
                    <noscript>
                        <form
                            class=style::reset_fallback_form
                            method="post"
                            action="/admin-api/addon-lists/reset"
                        >
                            <input type="hidden" name="id" value=id />
                            <label for="addon-list-reset-confirmation-nojs">
                                <span>{tr(TextKey::TypeListNameToConfirm)}</span>
                                <input
                                    id="addon-list-reset-confirmation-nojs"
                                    type="text"
                                    name="confirmed_name"
                                    required
                                    autocomplete="off"
                                    spellcheck="false"
                                />
                            </label>
                            <button type="submit" class=style::reset_button>
                                <span style=icon_style(IconName::Refresh, "1rem") aria-hidden="true"></span>
                                <span>{tr(TextKey::ResetAndReload)}</span>
                            </button>
                        </form>
                    </noscript>
                </section>
                <Modal
                    dialog_id="addon-media-list-apply-kind-dialog"
                    open=apply_content_kind_open
                    title=tr(TextKey::ApplyToAllItems).to_owned()
                    on_close=Callback::new(move |confirmed: bool| {
                        if confirmed {
                            apply_content_kind_action
                                .dispatch((id, content_kind_override.get_untracked()));
                        }
                    })
                >
                    <p>{tr(TextKey::ApplyToAllItemsPrompt)}</p>
                </Modal>
                <Modal
                    dialog_id="addon-media-list-change-addon-dialog"
                    open=change_addon_open
                    title=tr(TextKey::ChangeAddon).to_owned()
                    on_close=Callback::new(move |confirmed: bool| {
                        if confirmed {
                            change_addon_action.dispatch(selected_addon_id.get_untracked());
                        }
                    })
                >
                    <p>{tr(TextKey::ChangeAddonPrompt)}</p>
                </Modal>
                <Modal
                    dialog_id="addon-media-list-reset-dialog"
                    open=reset_open
                    title=tr(TextKey::ResetAndReload).to_owned()
                    on_close=Callback::new(move |confirmed: bool| {
                        let confirmed_name = reset_confirmed_name.get_untracked();
                        if confirmed
                            && reset_confirmation_matches(&confirmed_name, &list_name.get_value())
                        {
                            reset_action.dispatch((id, confirmed_name.trim().to_owned()));
                        }
                        reset_confirmed_name.set(String::new());
                    })
                    confirm_label=Some(tr(TextKey::ResetAndReload))
                    confirm_disabled=Signal::derive(move || {
                        !reset_confirmation_valid.get() || reset_action.pending().get()
                    })
                    destructive=true
                    wide=true
                >
                    <div class=style::reset_dialog_body>
                        <p>{tr(TextKey::ResetAddonMediaListHelp)}</p>
                        <AddonMediaListResetConsequences
                            list_name=list_name.get_value()
                            source_host=source_host.get_value()
                            impact=reset_impact
                        />
                        <label class=style::reset_confirmation for="addon-list-reset-confirmation">
                            <span>{tr(TextKey::TypeListNameToConfirm)}</span>
                            <input
                                id="addon-list-reset-confirmation"
                                type="text"
                                autocomplete="off"
                                spellcheck="false"
                                prop:value=move || reset_confirmed_name.get()
                                on:input=move |event| {
                                    reset_confirmed_name.set(event_target_value(&event));
                                }
                            />
                        </label>
                    </div>
                </Modal>
                <Modal
                    dialog_id="addon-media-list-restore-revision-dialog"
                    open=restore_open
                    title=tr(TextKey::RestoreRevision).to_owned()
                    on_close=Callback::new(move |confirmed: bool| {
                        if confirmed {
                            if let Some(source_revision) = restore_revision.get_untracked() {
                                restore_revision_action.dispatch((
                                    id,
                                    source_revision,
                                    publication_readiness
                                        .with_untracked(|value| value.saved_revision),
                                    published_revision.get_untracked(),
                                ));
                            }
                        }
                    })
                    confirm_label=Some(tr(TextKey::RestoreRevision))
                    confirm_disabled=Signal::derive(move || restore_revision_action.pending().get())
                    destructive=true
                    wide=true
                >
                    <p>{move || format!(
                        "{} {} → {} {}",
                        tr(TextKey::RestoreRevisionWarning),
                        restore_revision.get().unwrap_or_default(),
                        tr(TextKey::TargetPublishedRevision),
                        published_revision.get().saturating_add(1),
                    )}</p>
                </Modal>
                <div
                    class=style::backdrop
                    style:display=move || if replacement_target.get().is_some() { "" } else { "none" }
                />
                <div
                    class=style::overlay
                    style:display=move || if replacement_target.get().is_some() { "" } else { "none" }
                    on:click=move |_| replacement_target.set(None)
                >
                    <div class=style::picker_panel on:click=|event| event.stop_propagation()>
                        // Naming the target item here is what makes the dialog
                        // usable: the list below it can hold many similarly
                        // titled episodes.
                        <h2 class=style::dialog_title>
                            {move || replacement_item_title
                                .get()
                                .unwrap_or_else(|| tr(TextKey::LocalReplacement).to_string())}
                        </h2>
                        <div class=style::picker_body>
                            <div class=style::picker_search>
                                <input
                                    class=format!("{} {}", style::text_input, style::picker_search_input)
                                    type="search"
                                    aria-label=tr(TextKey::Search)
                                    prop:value=move || replacement_query.get()
                                    on:input=move |event| replacement_query.set(event_target_value(&event))
                                />
                                <button
                                    type="button"
                                    class=style::btn_add
                                    disabled=move || search_action.pending().get()
                                    on:click=move |_| {
                                        search_action.dispatch(replacement_query.get_untracked());
                                    }
                                >
                                    <span style=icon_style(IconName::Search, "1.125rem") aria-hidden="true"></span>
                                    <span>{tr(TextKey::Search)}</span>
                                </button>
                            </div>
                            <div class=style::picker_results>
                                <table>
                                    <thead>
                                        <tr>
                                            <th class=style::picker_name>{tr(TextKey::Name)}</th>
                                            <th class=style::picker_path>{tr(TextKey::Location)}</th>
                                            <th class=style::picker_action/>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {move || {
                                            let candidates = replacement_results.get();
                                            if candidates.is_empty() {
                                                let message = if search_action.pending().get() {
                                                    tr(TextKey::Loading)
                                                } else {
                                                    tr(TextKey::NoItemsToDisplay)
                                                };
                                                return view! {
                                                    <tr>
                                                        <td class=style::empty_cell colspan="3">{message}</td>
                                                    </tr>
                                                }
                                                .into_any();
                                            }
                                            candidates
                                                .into_iter()
                                                .map(|candidate| {
                                                    let target = replacement_target.get_untracked();
                                                    view! {
                                                        <tr>
                                                            <td class=style::picker_name>{candidate.title}</td>
                                                            <td class=style::picker_path>{candidate.path}</td>
                                                            <td class=style::picker_action>
                                                                <div class=style::row_actions>
                                                                    <button
                                                                        type="button"
                                                                        class=style::btn_add
                                                                        on:click=move |_| {
                                                                            if let Some(item_id) = target {
                                                                                replacement_action.dispatch((
                                                                                    item_id,
                                                                                    Some((candidate.media_item_id, candidate.media_version_id)),
                                                                                ));
                                                                            }
                                                                        }
                                                                    >
                                                                        {tr(TextKey::Replace)}
                                                                    </button>
                                                                </div>
                                                            </td>
                                                        </tr>
                                                    }
                                                })
                                                .collect_view()
                                                .into_any()
                                        }}
                                    </tbody>
                                </table>
                            </div>
                        </div>
                        <div class=style::dialog_footer>
                            <button
                                type="button"
                                class=style::btn_cancel
                                on:click=move |_| replacement_target.set(None)
                            >
                                {tr(TextKey::Cancel)}
                            </button>
                        </div>
                    </div>
                </div>
            </section>
            </div>
            </div>
            <p
                class=style::empty
                style:display=move || if form_found { "none" } else { "" }
            >
                {tr(TextKey::AddonMediaListNotFound)}
            </p>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unchanged_people_input_preserves_provider_fields_and_preparation_action() {
        let original = vec![AddonMediaListNfoPerson {
            name: "Fixture presenter".into(),
            role: Some("Channel".into()),
            order: Some(0),
            thumb: Some("https://example.invalid/presenter.jpg".into()),
        }];
        let parsed = people_values(&people_lines(&original), &original);
        assert_eq!(parsed, original);
        let readiness = AddonMediaListPublicationReadiness {
            required_prepared_media_count: 1,
            ..AddonMediaListPublicationReadiness::default()
        };
        assert_eq!(
            publication_primary_action(parsed != original, &readiness),
            PublicationPrimaryAction::Prepare
        );
        assert_ne!(
            people_values("Changed presenter | Channel", &original),
            original
        );
        assert!(people_values("", &original).is_empty());
    }

    #[test]
    fn reset_source_context_never_includes_credentials_or_url_details() {
        assert_eq!(
            redacted_source_host(
                "https://operator:secret@Media.Example:8443/private/list?token=hidden#part"
            ),
            Some("media.example".to_owned())
        );
        assert_eq!(
            redacted_source_host("https://[2001:db8::1]:8443/private"),
            Some("2001:db8::1".to_owned())
        );
        assert_eq!(
            redacted_source_host("not a URL with /private?token=hidden"),
            None
        );
        assert_eq!(
            redacted_source_host("https://media.example\\private?token=hidden"),
            Some("media.example".to_owned())
        );
    }

    #[test]
    fn reset_confirmation_is_trimmed_and_case_sensitive() {
        assert!(reset_confirmation_matches(
            "  Current List  ",
            "Current List"
        ));
        assert!(!reset_confirmation_matches("current list", "Current List"));
        assert!(!reset_confirmation_matches(
            "Current List",
            " Current List "
        ));
        assert!(!reset_confirmation_matches(
            "Current List copy",
            "Current List"
        ));
    }

    #[test]
    fn revision_restore_is_only_offered_for_an_older_restorable_revision() {
        assert!(revision_can_restore(true, 2, 3));
        assert!(!revision_can_restore(false, 2, 3));
        assert!(!revision_can_restore(true, 3, 3));
        assert!(!revision_can_restore(true, 4, 3));
    }

    #[test]
    fn draft_review_category_uses_exact_item_ids_and_rejects_unknown_values() {
        let diff = AddonMediaListDraftDiffRow {
            added_item_ids: vec![11],
            metadata_changed_item_ids: vec![12],
            excluded_item_ids: vec![13],
            unavailable_item_ids: vec![14],
            unknown_item_ids: vec![19],
            conflicted_item_ids: vec![15],
            pending_removal_item_ids: vec![16],
            reappeared_item_ids: vec![17],
            removal_candidate_item_ids: vec![18],
            ..AddonMediaListDraftDiffRow::default()
        };
        for (value, item_id) in [
            ("added", 11),
            ("metadata_changed", 12),
            ("excluded", 13),
            ("unavailable", 14),
            ("conflicted", 15),
            ("pending_removal", 16),
            ("reappeared", 17),
            ("removal_candidate", 18),
            ("unknown", 19),
        ] {
            let category = draft_review_category(value).unwrap();
            assert!(item_matches_draft_category(item_id, Some(category), &diff));
            assert!(!item_matches_draft_category(99, Some(category), &diff));
        }
        assert_eq!(draft_review_category("not_a_category"), None);
        assert!(item_matches_draft_category(99, None, &diff));
    }

    /// A value string that does not survive the round trip silently resets the
    /// operator's choice: the select renders nothing selected, and the next
    /// save stores `Auto`. Both enums grew, so every variant is checked.
    #[test]
    fn content_kind_values_round_trip_for_every_variant() {
        for kind in [
            AddonMediaListContentKind::Auto,
            AddonMediaListContentKind::TelevisionEpisode,
            AddonMediaListContentKind::Movie,
            AddonMediaListContentKind::MusicVideo,
            AddonMediaListContentKind::OtherVideo,
            AddonMediaListContentKind::Song,
            AddonMediaListContentKind::Image,
        ] {
            assert_eq!(
                content_kind_from_value(content_kind_value(kind)),
                kind,
                "{kind:?} does not survive the select round trip"
            );
        }
        assert_eq!(
            content_kind_from_value("not-a-kind"),
            AddonMediaListContentKind::Auto
        );
    }

    #[test]
    fn presentation_kind_values_round_trip_for_every_variant() {
        for kind in [
            AddonMediaListPresentationKind::Auto,
            AddonMediaListPresentationKind::RemoteStreams,
            AddonMediaListPresentationKind::TelevisionShow,
            AddonMediaListPresentationKind::Movies,
            AddonMediaListPresentationKind::MusicVideos,
            AddonMediaListPresentationKind::OtherVideos,
            AddonMediaListPresentationKind::Songs,
            AddonMediaListPresentationKind::Images,
        ] {
            assert_eq!(
                presentation_kind_from_value(presentation_kind_value(kind)),
                kind,
                "{kind:?} does not survive the select round trip"
            );
        }
        assert_eq!(
            presentation_kind_from_value("not-a-kind"),
            AddonMediaListPresentationKind::Auto
        );
    }

    /// Zero is the "all rows" choice for the client-side review overview;
    /// losing it would quietly make long source lists harder to audit.
    #[test]
    fn item_page_sizes_offer_an_unbounded_choice_and_the_default() {
        assert!(ITEM_PAGE_SIZES.contains(&0));
        assert!(ITEM_PAGE_SIZES.contains(&DEFAULT_ITEM_PAGE_SIZE));
    }

    #[test]
    fn detected_episode_needs_numbering_when_collection_presentation_is_auto() {
        let item = AddonMediaListItemRow {
            title: "Detected episode".to_owned(),
            availability: AddonMediaListItemAvailability::Available,
            content_kind: AddonMediaListContentKind::TelevisionEpisode,
            season: None,
            episode: None,
            ..AddonMediaListItemRow::default()
        };

        assert!(item_needs_review(
            &item,
            AddonMediaListPresentationKind::Auto
        ));
    }

    #[test]
    fn episode_numbering_accepts_blank_or_complete_pairs_only() {
        assert_eq!(
            episode_number_pair(None, None),
            EpisodeNumberPair::Unnumbered
        );
        assert_eq!(
            episode_number_pair(Some(0), Some(1)),
            EpisodeNumberPair::Numbered(0, 1)
        );
        assert_eq!(
            episode_number_pair(Some(0), Some(0)),
            EpisodeNumberPair::Unnumbered
        );
        assert_eq!(
            episode_number_pair(Some(1), None),
            EpisodeNumberPair::Invalid
        );
        assert_eq!(
            episode_number_pair(Some(1), Some(0)),
            EpisodeNumberPair::Invalid
        );

        assert_eq!(
            parse_episode_number_pair("", ""),
            EpisodeNumberPair::Unnumbered
        );
        assert_eq!(
            parse_episode_number_pair("2", "7"),
            EpisodeNumberPair::Numbered(2, 7)
        );
        assert_eq!(
            parse_episode_number_pair("0", "0"),
            EpisodeNumberPair::Unnumbered
        );
        assert_eq!(
            parse_episode_number_pair("2", ""),
            EpisodeNumberPair::Invalid
        );
    }

    #[test]
    fn numbering_preview_skips_numbers_already_used_in_the_selected_season() {
        let items = vec![
            AddonMediaListItemRow {
                id: 1,
                season: Some(2),
                episode: Some(3),
                included: true,
                ..AddonMediaListItemRow::default()
            },
            AddonMediaListItemRow {
                id: 2,
                included: true,
                ..AddonMediaListItemRow::default()
            },
            AddonMediaListItemRow {
                id: 4,
                included: true,
                ..AddonMediaListItemRow::default()
            },
            AddonMediaListItemRow {
                id: 3,
                season: Some(2),
                episode: Some(4),
                included: true,
                ..AddonMediaListItemRow::default()
            },
        ];

        assert_eq!(
            episode_numbering_preview_values(&items, None, 2, 3, 2, false),
            [5, 6]
        );
        assert_eq!(
            episode_numbering_preview_values(&items, None, 2, 3, 2, true),
            [3, 5]
        );
    }

    #[test]
    fn numbering_preview_highlights_only_assignable_top_level_items() {
        let items = vec![
            AddonMediaListItemRow {
                id: 1,
                included: true,
                ..AddonMediaListItemRow::default()
            },
            AddonMediaListItemRow {
                id: 2,
                included: false,
                ..AddonMediaListItemRow::default()
            },
            AddonMediaListItemRow {
                id: 3,
                included: true,
                parent_provider_id: Some("parent".to_owned()),
                ..AddonMediaListItemRow::default()
            },
            AddonMediaListItemRow {
                id: 4,
                included: true,
                season: Some(1),
                episode: Some(7),
                ..AddonMediaListItemRow::default()
            },
            AddonMediaListItemRow {
                id: 5,
                included: true,
                ..AddonMediaListItemRow::default()
            },
        ];

        assert_eq!(
            episode_numbering_preview_assignments(&items, None, 1, 1, 5, false),
            BTreeMap::from([(1, (1, 1)), (5, (1, 2))])
        );
        assert_eq!(
            episode_numbering_preview_assignments(&items, None, 1, 1, 2, true),
            BTreeMap::from([(1, (1, 1)), (4, (1, 2))])
        );
    }

    #[test]
    fn numbering_can_start_at_a_selected_row_and_detect_date_gaps() {
        let items = [
            (1, Some("2020-01-01")),
            (2, None),
            (3, Some("2020-02-01")),
            (4, Some("2020-06-15")),
        ]
        .into_iter()
        .map(|(id, first_aired)| AddonMediaListItemRow {
            id,
            included: true,
            first_aired: first_aired.map(str::to_owned),
            ..AddonMediaListItemRow::default()
        })
        .collect::<Vec<_>>();

        assert_eq!(
            auto_detect_episode_assignments(&items, None, 1, 90, 4),
            BTreeMap::from([(1, (1, 1)), (2, (1, 2)), (3, (1, 3)), (4, (2, 1)),])
        );
        let newest_first = items.iter().rev().cloned().collect::<Vec<_>>();
        assert_eq!(
            // The undated item follows its provider predecessor, item 3.
            auto_detect_episode_assignments(&newest_first, None, 1, 90, 4),
            BTreeMap::from([(1, (1, 1)), (2, (1, 3)), (3, (1, 2)), (4, (2, 1)),])
        );
        assert_eq!(
            episode_numbering_preview_assignments(&items, Some(3), 4, 7, 2, true),
            BTreeMap::from([(3, (4, 7)), (4, (4, 8))])
        );
    }

    #[test]
    #[allow(clippy::too_many_lines)] // Follow the complete overview-to-published progress transition.
    fn progress_phase_switches_from_overview_to_published_details() {
        let overview = AddonMediaListImportProgressRow {
            state: AddonMediaListImportState::Discovering,
            discovered_count: 12,
            total_hint: Some(40),
            ..AddonMediaListImportProgressRow::default()
        };
        assert_eq!(
            import_progress_phase(&overview, false, AddonMediaListDetailPolicy::Automatic),
            TextKey::FindingProgrammes
        );
        assert_eq!(progress_sample(&overview), (false, 12, Some(40)));

        let details = AddonMediaListImportProgressRow {
            state: AddonMediaListImportState::Ready,
            discovered_count: 8,
            detail_total: 8,
            detail_complete: 2,
            detail_skipped: 1,
            detail_failed: 1,
            detail_pending: 4,
            current_title: Some("Episode four".to_owned()),
            ..AddonMediaListImportProgressRow::default()
        };
        assert_eq!(
            import_progress_phase(&details, true, AddonMediaListDetailPolicy::Automatic),
            TextKey::LoadingItemDetails
        );
        assert_eq!(progress_sample(&details), (true, 4, Some(8)));

        let complete_manual = AddonMediaListImportProgressRow {
            state: AddonMediaListImportState::Ready,
            discovered_count: 1,
            ..AddonMediaListImportProgressRow::default()
        };
        assert_eq!(
            import_progress_phase(
                &complete_manual,
                true,
                AddonMediaListDetailPolicy::ManualOnly,
            ),
            TextKey::OverviewOnly
        );

        let empty = AddonMediaListImportProgressRow {
            state: AddonMediaListImportState::Ready,
            discovered_count: 0,
            ..AddonMediaListImportProgressRow::default()
        };
        assert_eq!(
            import_progress_phase(&empty, false, AddonMediaListDetailPolicy::ManualOnly),
            TextKey::EmptyImportNeedsRetry
        );
        assert!(import_can_retry(&empty));
        let empty_reconcile = AddonMediaListImportProgressRow {
            purpose: AddonMediaListImportPurpose::Reconcile,
            ..empty.clone()
        };
        assert_eq!(
            import_progress_phase(
                &empty_reconcile,
                false,
                AddonMediaListDetailPolicy::ManualOnly,
            ),
            TextKey::ReadyToReview
        );
        assert!(!import_can_retry(&empty_reconcile));
        let unchanged_incremental = AddonMediaListImportProgressRow {
            state: AddonMediaListImportState::Ready,
            mode: AddonMediaListImportMode::Incremental,
            strategy: AddonMediaListIncrementalStrategy::SkipKnown,
            discovered_count: 0,
            examined_count: 12,
            archived_skipped_count: 12,
            emitted_count: 0,
            total_hint: Some(12),
            retained_item_count: 12,
            ..AddonMediaListImportProgressRow::default()
        };
        assert_eq!(
            import_progress_phase(
                &unchanged_incremental,
                false,
                AddonMediaListDetailPolicy::ManualOnly,
            ),
            TextKey::ReadyToReview
        );
        assert!(!import_can_retry(&unchanged_incremental));
        assert_eq!(
            progress_sample(&unchanged_incremental),
            (false, 12, Some(12))
        );
        let empty_incremental = AddonMediaListImportProgressRow {
            retained_item_count: 0,
            ..unchanged_incremental.clone()
        };
        assert_eq!(
            import_progress_phase(
                &empty_incremental,
                false,
                AddonMediaListDetailPolicy::ManualOnly,
            ),
            TextKey::EmptyImportNeedsRetry
        );
        assert!(import_can_retry(&empty_incremental));
        assert!(!import_can_retry(&AddonMediaListImportProgressRow {
            state: AddonMediaListImportState::Ready,
            discovered_count: 1,
            ..AddonMediaListImportProgressRow::default()
        }));
    }

    #[test]
    fn overview_sort_parsing_and_toggle_are_stable() {
        assert_eq!(
            addon_list_sort_field("unexpected"),
            AddonMediaListAdminSortField::Name
        );
        assert_eq!(
            addon_list_sort_direction("unexpected"),
            AddonMediaListAdminSortDirection::Ascending
        );
        assert_eq!(
            next_addon_list_sort(
                AddonMediaListAdminSortField::Name,
                AddonMediaListAdminSortDirection::Ascending,
                AddonMediaListAdminSortField::Name,
            ),
            (
                AddonMediaListAdminSortField::Name,
                AddonMediaListAdminSortDirection::Descending,
            )
        );
        assert_eq!(
            next_addon_list_sort(
                AddonMediaListAdminSortField::Name,
                AddonMediaListAdminSortDirection::Descending,
                AddonMediaListAdminSortField::Id,
            ),
            (
                AddonMediaListAdminSortField::Id,
                AddonMediaListAdminSortDirection::Ascending,
            )
        );
        assert_eq!(
            addon_list_sort_aria(
                AddonMediaListAdminSortField::Id,
                AddonMediaListAdminSortDirection::Descending,
                AddonMediaListAdminSortField::Name,
            ),
            "none"
        );
    }

    #[test]
    fn progress_durations_are_stable_and_bounded() {
        assert_eq!(format_clock_duration(0), "00:00");
        assert_eq!(format_clock_duration(65), "01:05");
        assert_eq!(format_clock_duration(3_661), "01:01:01");
        assert_eq!(
            elapsed_between("2026-08-26T10:00:00Z", Some("2026-08-26T10:01:05Z")),
            Some("01:05".to_owned())
        );
        assert!(elapsed_from("not-a-timestamp").is_none());
    }

    #[test]
    fn publication_blockers_group_repeated_item_failures_without_losing_global_reasons() {
        let mut blockers = (0..48)
            .map(|rank| AddonMediaListPublicationBlocker {
                item_id: None,
                kind: AddonMediaListPublicationBlockerKind::SeasonEpisodeMissing,
                item_rank: Some(rank + 1),
                item_title: Some(format!("Episode {}", rank + 1)),
                count: 1,
            })
            .collect::<Vec<_>>();
        blockers.push(AddonMediaListPublicationBlocker {
            item_id: None,
            kind: AddonMediaListPublicationBlockerKind::CollectionTitleMissing,
            item_rank: None,
            item_title: None,
            count: 1,
        });

        let groups = publication_blocker_groups(&blockers);

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].item_count, 48);
        assert_eq!(
            publication_blocker_group_label(&groups[0]),
            "48 Items · Enter a valid season and episode number."
        );
        assert_eq!(groups[1].item_count, 0);
        assert_eq!(
            publication_blocker_group_label(&groups[1]),
            "The collection needs a title."
        );
    }

    #[test]
    fn prepared_media_status_ssr_keeps_a_stable_localized_status_node() {
        let owner = Owner::new();
        owner.with(|| {
            let readiness = RwSignal::new(AddonMediaListPublicationReadiness {
                required_prepared_media_count: 3,
                ..AddonMediaListPublicationReadiness::default()
            });
            let html = prepared_media_status(readiness).to_html();
            assert!(html.contains("data-prepared-media-status"));
            assert!(html.contains("role=\"status\""));
            assert!(html.contains("Preparation tasks: 3"));

            readiness.update(|value| value.required_prepared_media_count = 0);
            let html = prepared_media_status(readiness).to_html();
            assert!(html.contains("data-prepared-media-status"));
            assert!(html.replace(' ', "").contains("display:none"));
        });
    }

    #[test]
    fn prepared_media_keeps_polling_and_failed_work_offers_explicit_retry() {
        let mut readiness = AddonMediaListPublicationReadiness {
            included_count: 1,
            review_state: AddonMediaListReviewState::Ready,
            required_prepared_media_count: 1,
            ..AddonMediaListPublicationReadiness::default()
        };
        assert_eq!(
            publication_primary_action(false, &readiness),
            PublicationPrimaryAction::Prepare
        );
        assert!(!publication_preparation_active(&readiness));

        readiness.required_prepared_media_pending = 1;
        assert!(publication_preparation_active(&readiness));
        assert_eq!(
            publication_primary_action(false, &readiness),
            PublicationPrimaryAction::None
        );

        // A failed preparation remains required but is not automatically retried.
        readiness.required_prepared_media_pending = 0;
        assert!(!publication_preparation_active(&readiness));
        assert_eq!(
            publication_primary_action(false, &readiness),
            PublicationPrimaryAction::Prepare
        );
        assert_eq!(
            publication_primary_action(true, &readiness),
            PublicationPrimaryAction::Save
        );

        readiness.required_prepared_media_count = 0;
        assert_eq!(
            publication_primary_action(false, &readiness),
            PublicationPrimaryAction::Publish
        );
    }

    #[test]
    fn publication_primary_action_tracks_the_exact_saved_state() {
        let mut readiness = AddonMediaListPublicationReadiness {
            included_count: 48,
            recommendation_count: 48,
            review_state: AddonMediaListReviewState::Ready,
            ..AddonMediaListPublicationReadiness::default()
        };
        assert_eq!(
            publication_primary_action(false, &readiness),
            PublicationPrimaryAction::Publish
        );
        assert_eq!(
            publication_primary_action(true, &readiness),
            PublicationPrimaryAction::Save
        );

        readiness.required_playback_detail_count = 2;
        assert_eq!(
            publication_primary_action(false, &readiness),
            PublicationPrimaryAction::Prepare
        );
        readiness.required_playback_detail_pending = 2;
        assert_eq!(
            publication_primary_action(false, &readiness),
            PublicationPrimaryAction::None
        );

        readiness.required_playback_detail_count = 0;
        readiness.required_playback_detail_pending = 0;
        readiness.blockers.push(AddonMediaListPublicationBlocker {
            item_id: None,
            kind: AddonMediaListPublicationBlockerKind::NoIncludedItems,
            item_rank: None,
            item_title: None,
            count: 0,
        });
        assert_eq!(
            publication_primary_action(false, &readiness),
            PublicationPrimaryAction::None
        );

        readiness.blockers.clear();
        readiness.review_state = AddonMediaListReviewState::Publishing;
        assert_eq!(
            publication_primary_action(false, &readiness),
            PublicationPrimaryAction::PublishBlocked
        );
        let publication_status = |state| AddonMediaListPublicationStatus {
            outcome: None,
            job_id: 1,
            operation_id: 2,
            generation: 3,
            saved_revision: 4,
            target_published_revision: 5,
            phase: AddonMediaListPublicationPhase::Staging,
            state,
            attempt_count: 1,
            last_progress_at: "2026-09-03T00:00:00Z".to_owned(),
            diagnostic_code: None,
            recovery_diagnostic_present: false,
            can_retry: false,
            can_restore: false,
        };
        for (state, expected) in [
            (
                AddonMediaListPublicationState::Queued,
                PublicationPrimaryAction::Queued,
            ),
            (
                AddonMediaListPublicationState::Publishing,
                PublicationPrimaryAction::Publishing,
            ),
            (
                AddonMediaListPublicationState::Restoring,
                PublicationPrimaryAction::Restoring,
            ),
            (
                AddonMediaListPublicationState::Expired,
                PublicationPrimaryAction::PublishBlocked,
            ),
        ] {
            readiness.publication = Some(publication_status(state));
            assert_eq!(publication_primary_action(false, &readiness), expected);
        }
        readiness.publication = None;
        readiness.review_state = AddonMediaListReviewState::Published;
        assert_eq!(
            publication_primary_action(false, &readiness),
            PublicationPrimaryAction::None
        );
    }

    #[test]
    fn activity_refresh_marker_ignores_scalar_readiness_changes() {
        let mut readiness = AddonMediaListPublicationReadiness {
            included_count: 10,
            publication: Some(AddonMediaListPublicationStatus {
                outcome: None,
                job_id: 7,
                operation_id: 8,
                generation: 2,
                saved_revision: 3,
                target_published_revision: 4,
                phase: AddonMediaListPublicationPhase::Staging,
                state: AddonMediaListPublicationState::Publishing,
                attempt_count: 1,
                last_progress_at: "2026-09-04T12:00:00Z".to_owned(),
                diagnostic_code: None,
                recovery_diagnostic_present: false,
                can_retry: false,
                can_restore: false,
            }),
            ..AddonMediaListPublicationReadiness::default()
        };
        let marker = publication_readiness_marker(&readiness);

        readiness.included_count = 11;
        readiness.recommendation_count = 5;
        assert_eq!(publication_readiness_marker(&readiness), marker);

        readiness.publication.as_mut().unwrap().last_progress_at =
            "2026-09-04T12:00:01Z".to_owned();
        assert_ne!(publication_readiness_marker(&readiness), marker);
    }

    #[test]
    fn activity_keeps_original_and_recovery_failures_distinct() {
        assert_eq!(
            publication_event_text_key(AddonMediaListPublicationEventKind::OriginalFailure),
            TextKey::EventOriginalFailure
        );
        assert_eq!(
            publication_event_text_key(AddonMediaListPublicationEventKind::RecoveryFailure),
            TextKey::EventRecoveryFailure
        );
        assert_ne!(
            publication_event_text_key(AddonMediaListPublicationEventKind::OriginalFailure),
            publication_event_text_key(AddonMediaListPublicationEventKind::RecoveryFailure)
        );
        assert_eq!(
            publication_event_text_key(AddonMediaListPublicationEventKind::LeaseReclaimed),
            TextKey::EventLeaseReclaimed
        );
        assert_eq!(
            publication_error_category_text_key(
                AddonMediaListPublicationErrorCategory::SharingViolation,
            ),
            TextKey::ErrorSharingViolation
        );
    }
}

#[cfg(all(test, feature = "ssr"))]
mod editor_load_tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicI32, Ordering},
        Arc,
    };

    #[tokio::test]
    async fn editor_load_retries_concurrent_updates_once_and_retains_the_form() {
        for updates in [1, 2] {
            let revision = Arc::new(AtomicI32::new(1));
            let mut calls = 0;
            let result = consistent_editor_read(|| {
                calls += 1;
                let call = calls;
                let revision = Arc::clone(&revision);
                async move {
                    let before = revision.load(Ordering::SeqCst);
                    let mut snapshot = AddonMediaListEditorInitial::default();
                    snapshot.0 = Some(AddonMediaListForm {
                        draft_revision: before,
                        name: "Retained fixture".into(),
                        ..Default::default()
                    });
                    snapshot.2.saved_revision = before;
                    if call <= updates {
                        let writer = Arc::clone(&revision);
                        tokio::spawn(async move {
                            writer.fetch_add(1, Ordering::SeqCst);
                        })
                        .await
                        .unwrap();
                    }
                    Ok((snapshot, before == revision.load(Ordering::SeqCst)))
                }
            })
            .await
            .unwrap();
            assert_eq!(calls, 2);
            assert_eq!(result.0.as_ref().unwrap().name, "Retained fixture");
            assert_eq!(result.0.as_ref().unwrap().draft_revision, 2);
            assert_eq!(result.11.incomplete, updates == 2);
            assert_eq!(result.11.disabled_tasks.contains("playback"), updates == 2);
            assert_eq!(result.2.saved_revision, if updates == 2 { -1 } else { 2 });
        }
    }

    #[test]
    fn ancillary_failure_disables_only_dependent_tasks() {
        let mut status = EditorLoadStatus::default();
        let history: Vec<()> =
            status.retain(Err::<Vec<()>, _>("storage unavailable"), &["activity"]);
        assert!(history.is_empty());
        assert!(status.incomplete);
        assert!(status.disabled_tasks.contains("activity"));
        assert!(!status.disabled_tasks.contains("playback"));
        let values = status.retain(Ok::<_, ()>(vec!["saved input"]), &["playback"]);
        assert_eq!(values, ["saved input"]);
    }
}
