//! Provider-neutral add-on media-list manager derived from ErsatzTV's
//! `Pages/TraktLists.razor`, `Pages/TraktListEditor.razor`, and
//! `Shared/AddTraktListDialog.razor`; ADR 0067 owns the deliberate boundary change.

use std::collections::{BTreeMap, BTreeSet};

use ersatzrs_domain::{
    AddonMediaListContentKind, AddonMediaListDetailPolicy, AddonMediaListDetailState,
    AddonMediaListImportState, AddonMediaListItemAvailability, AddonMediaListItemKind,
    AddonMediaListItemRole, AddonMediaListPresentationKind, AddonMediaListReviewState,
    RemoteStreamLiveness,
};
use leptos::prelude::*;
use leptos_router::hooks::{use_params_map, use_query_map};

use crate::admin::i18n::{
    addon_media_list_error_message, delete_failed, match_status, range_of, save_failed, tr, TextKey,
};
use crate::admin::server_fns::addon_media_lists::{
    add_addon_media_list, apply_addon_media_list_content_kind, apply_addon_media_list_liveness,
    assign_addon_media_list_episode_numbers, change_addon_media_list_addon,
    control_addon_media_list_import, delete_addon_media_list, get_addon_media_list_form,
    get_addon_media_list_item, get_addon_media_list_show_nfo, get_addon_media_lists_page_size,
    list_addon_media_list_items, list_addon_media_list_providers, list_addon_media_lists,
    match_addon_media_list, publish_addon_media_list, request_addon_media_list_item_details,
    request_addon_media_list_presentation, resolve_addon_media_list_provider,
    save_addon_media_list_item_nfo, save_addon_media_list_show_nfo,
    search_addon_media_list_replacements, set_addon_media_list_item_content_kind,
    set_addon_media_list_item_included, set_addon_media_list_item_liveness,
    set_addon_media_list_item_replacement, set_addon_media_list_item_role, update_addon_media_list,
    update_addon_media_list_item_chapters, AddonMediaListArtwork, AddonMediaListArtworkRole,
    AddonMediaListForm, AddonMediaListImportAction, AddonMediaListImportProgressRow,
    AddonMediaListItemDetail, AddonMediaListItemRow, AddonMediaListMetadataConflict,
    AddonMediaListNfoForm, AddonMediaListNfoMetadata, AddonMediaListNfoPerson,
    AddonMediaListProviderRow, AddonMediaListReplacementRow, AddonMediaListRow,
};
use crate::admin::ui::primitives::{
    dialog::{DeleteDialog, Modal},
    icon::{icon_style, IconName},
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
    let response = Request::post("/api/addon-media-lists/artwork")
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
    view! {
        <div class=style::artwork_role_field>
            <span>{tr(label)}</span>
            <input
                class=style::boxed_input
                type="text"
                inputmode="url"
                aria-label=tr(label)
                data-artwork-role=role_name
                list=list_name.clone()
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
/// Setting the descriptive classification of one entry.
type ContentKindAction = Action<(i64, AddonMediaListContentKind), Result<(), ServerFnError>>;
type InclusionAction = Action<(i64, i64, bool), Result<(), ServerFnError>>;
type ItemRoleAction = Action<(i64, i64, AddonMediaListItemRole), Result<(), ServerFnError>>;
type LivenessAction = Action<(i64, i64, Option<RemoteStreamLiveness>), Result<(), ServerFnError>>;
type ItemDetailAction = Action<i64, Result<Option<AddonMediaListItemDetail>, ServerFnError>>;
type DetailRequestAction = Action<(i64, i64), Result<(), ServerFnError>>;
/// Searching local media that could replace one entry.
type ReplacementSearchAction =
    Action<String, Result<Vec<AddonMediaListReplacementRow>, ServerFnError>>;

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

fn elapsed_from(started_at: &str) -> Option<String> {
    let started = chrono::DateTime::parse_from_rfc3339(started_at).ok()?;
    let seconds = u64::try_from(
        (chrono::Utc::now() - started.with_timezone(&chrono::Utc))
            .num_seconds()
            .max(0),
    )
    .ok()?;
    Some(format_clock_duration(seconds))
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
    content_kind_action: ContentKindAction,
    inclusion_action: InclusionAction,
    role_action: ItemRoleAction,
    liveness_action: LivenessAction,
    selected_items: RwSignal<BTreeSet<i64>>,
    episode_preview: Memo<BTreeMap<i64, (i32, i32)>>,
    presentation: RwSignal<AddonMediaListPresentationKind>,
    open_item: RwSignal<Option<i64>>,
    loaded_detail: RwSignal<Option<AddonMediaListItemDetail>>,
    detail_action: ItemDetailAction,
    request_detail_action: DetailRequestAction,
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
    let collection_role = item.collection_role;
    let included = item.included;
    let current_episode_number = item
        .season
        .zip(item.episode)
        .map(|(season, episode)| format!("S{season:02}E{episode:02}"));
    let current_episode_number = current_episode_number.unwrap_or_default();
    let first_aired = item
        .first_aired
        .clone()
        .unwrap_or_else(|| tr(TextKey::UnknownAvailability).to_string());
    let row_number = item.rank.saturating_add(1).to_string();
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
            && (effective_liveness == RemoteStreamLiveness::Unknown || liveness_conflict))
        || (presentation.get_untracked() == AddonMediaListPresentationKind::TelevisionShow
            && (item.season.is_none() || item.episode.is_none()));
    let availability_reason = item.availability_reason.unwrap_or_default();
    let has_reason = !availability_reason.is_empty();
    let toggle_editor = move || {
        if open_item.get_untracked() == Some(item_id) {
            open_item.set(None);
            loaded_detail.set(None);
        } else {
            open_item.set(Some(item_id));
            loaded_detail.set(None);
            detail_action.dispatch(item_id);
        }
    };
    view! {
        <>
        <tr
            class=move || if episode_preview.with(|preview| preview.contains_key(&item_id)) {
                format!("{} {}", style::metadata_item_row, style::numbering_preview_row)
            } else {
                style::metadata_item_row.to_owned()
            }
            aria-expanded=move || if open_item.get() == Some(item_id) { "true" } else { "false" }
            on:click=move |_| toggle_editor()
        >
            <td class=style::selection_cell>
                <input
                    type="checkbox"
                    aria-label=tr(TextKey::SelectItem)
                    style:visibility=if is_remote_stream { "visible" } else { "hidden" }
                    prop:checked=move || selected_items.with(|selected| selected.contains(&item_id))
                    on:change=move |event| {
                        event.stop_propagation();
                        let checked = event_target_checked(&event);
                        selected_items.update(|selected| {
                            if checked {
                                selected.insert(item_id);
                            } else {
                                selected.remove(&item_id);
                            }
                        });
                    }
                    on:click=|event| event.stop_propagation()
                />
            </td>
            <td class=style::number_cell>{row_number}</td>
            <td>
                <input
                    type="checkbox"
                    checked=included
                    aria-label=tr(TextKey::IncludeItem)
                    on:change=move |event| {
                        event.stop_propagation();
                        inclusion_action.dispatch((list_id, item_id, event_target_checked(&event)));
                    }
                    on:click=|event| event.stop_propagation()
                />
            </td>
            <td class=style::item_artwork_cell>
                <span class=style::item_thumbnail>
                    <img src=thumbnail_url alt="" style:display=if has_thumbnail { "block" } else { "none" }/>
                    <span style:display=if has_thumbnail { "none" } else { "inline-flex" }>
                        <span style=icon_style(IconName::Image, "1.15rem") aria-hidden="true"></span>
                    </span>
                </span>
            </td>
            <td>
                <button type="button" class=style::row_title on:click=move |event| {
                    event.stop_propagation();
                    toggle_editor();
                }>{title}</button>
                <small
                    class=style::row_note
                    style:display=if has_reason { "" } else { "none" }
                >{availability_reason}</small>
                <small class=style::row_note style:display=if has_metadata_summary { "" } else { "none" }>
                    {if !people_summary.is_empty() && !tags_summary.is_empty() {
                        format!("{people_summary} · {tags_summary}")
                    } else if !people_summary.is_empty() {
                        people_summary
                    } else {
                        tags_summary
                    }}
                </small>
            </td>
            <td class=style::compact_cell>{first_aired}</td>
            <td class=style::episode_cell>
                {move || {
                    if let Some((season, episode)) =
                        episode_preview.with(|preview| preview.get(&item_id).copied())
                    {
                        let next = format!("S{season:02}E{episode:02}");
                        if current_episode_number.is_empty() || current_episode_number == next {
                            next
                        } else {
                            format!("{current_episode_number} → {next}")
                        }
                    } else if current_episode_number.is_empty() {
                        "—".to_owned()
                    } else {
                        current_episode_number.clone()
                    }
                }}
            </td>
            <td>
                {move || if presentation.get() == AddonMediaListPresentationKind::Movies {
                    view! {
                        <select
                            class=style::row_select
                            aria-label=tr(TextKey::PrimaryFeature)
                            prop:value=match collection_role {
                                AddonMediaListItemRole::Auto => "auto",
                                AddonMediaListItemRole::Primary => "primary",
                                AddonMediaListItemRole::Extra => "extra",
                            }
                            on:click=|event| event.stop_propagation()
                            on:change=move |event| {
                                event.stop_propagation();
                                let role = match event_target_value(&event).as_str() {
                                    "primary" => AddonMediaListItemRole::Primary,
                                    "extra" => AddonMediaListItemRole::Extra,
                                    _ => AddonMediaListItemRole::Auto,
                                };
                                role_action.dispatch((list_id, item_id, role));
                            }
                        >
                            <option value="auto">{tr(TextKey::NeedReview)}</option>
                            <option value="primary">{tr(TextKey::PrimaryFeature)}</option>
                            <option value="extra">{tr(TextKey::MovieExtra)}</option>
                        </select>
                    }.into_any()
                } else {
                    view! {
                        <select
                            class=style::row_select
                            aria-label=tr(TextKey::MediaKind)
                            prop:value=content_kind_value(content_kind)
                            on:click=|event| event.stop_propagation()
                            on:change=move |event| {
                                event.stop_propagation();
                                content_kind_action.dispatch((
                                    item_id,
                                    content_kind_from_value(&event_target_value(&event)),
                                ));
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
            </td>
            <td class=style::liveness_cell>
                <select
                    class=style::row_select
                    aria-label=tr(TextKey::PlaybackBehavior)
                    style:display=if is_remote_stream { "block" } else { "none" }
                    prop:value=liveness_override_value(liveness_override)
                    on:click=|event| event.stop_propagation()
                    on:change=move |event| {
                        event.stop_propagation();
                        liveness_action.dispatch((
                            list_id,
                            item_id,
                            liveness_override_from_value(&event_target_value(&event)),
                        ));
                    }
                >
                    <option value="provider">
                        {format!("{} · {}", tr(TextKey::UseProviderValue), tr(liveness_text_key(provider_liveness)))}
                    </option>
                    <option value="finite">{tr(TextKey::FiniteMedia)}</option>
                    <option value="live">{tr(TextKey::LiveSource)}</option>
                </select>
                <small
                    class=style::liveness_conflict
                    style:display=if is_remote_stream && liveness_conflict { "block" } else { "none" }
                >{tr(TextKey::ProviderValueChanged)}</small>
                <span style:display=if is_remote_stream { "none" } else { "inline" }>"—"</span>
            </td>
            <td data-replacement-state=replacement_state title=item.replacement_path.unwrap_or_default()>
                <span class=move || if base_needs_review
                    || (presentation.get() == AddonMediaListPresentationKind::Movies
                        && collection_role == AddonMediaListItemRole::Auto)
                {
                    style::review_status
                } else {
                    style::ready_status
                }>
                    <span style=move || icon_style(
                        if base_needs_review
                            || (presentation.get() == AddonMediaListPresentationKind::Movies
                                && collection_role == AddonMediaListItemRole::Auto)
                        {
                            IconName::Warning
                        } else {
                            IconName::Check
                        },
                        "0.95rem",
                    ) aria-hidden="true"></span>
                    {move || if base_needs_review
                        || (presentation.get() == AddonMediaListPresentationKind::Movies
                            && collection_role == AddonMediaListItemRole::Auto)
                    {
                        tr(TextKey::NeedReview)
                    } else {
                        tr(TextKey::Available)
                    }}
                </span>
                <small class=style::row_note>{replacement}</small>
            </td>
            <td>
                <div class=style::row_actions>
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
                        aria-busy=if detail_pending { "true" } else { "false" }
                        style:display=if item.parent_provider_id.is_none() { "inline-flex" } else { "none" }
                        disabled=move || !can_request_details || request_detail_action.pending().get()
                        on:click=move |event| {
                            event.stop_propagation();
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
                        on:click=move |event| {
                            event.stop_propagation();
                            replacement_target.set(Some(item_id));
                            replacement_query.set(search_title.clone());
                            search_action.dispatch(search_title.clone());
                        }
                    >
                        <span style=icon_style(IconName::FindReplace, "1.25rem") aria-hidden="true"></span>
                    </button>
                    <button
                        type="button"
                        class=style::icon_button
                        title=tr(TextKey::EditMetadata)
                        aria-label=tr(TextKey::EditMetadata)
                        on:click=move |event| {
                            event.stop_propagation();
                            toggle_editor();
                        }
                    >
                        <span style=icon_style(IconName::Edit, "1.2rem") aria-hidden="true"></span>
                    </button>
                    <button
                        type="button"
                        class=style::icon_button
                        title=tr(TextKey::Clear)
                        aria-label=tr(TextKey::Clear)
                        disabled=!has_replacement
                        on:click=move |event| {
                            event.stop_propagation();
                            replacement_action.dispatch((item_id, None));
                        }
                    >
                        <span style=icon_style(IconName::Delete, "1.25rem") aria-hidden="true"></span>
                    </button>
                </div>
            </td>
        </tr>
        <tr
            class=style::metadata_accordion_row
            style:display=move || if open_item.get() == Some(item_id) { "table-row" } else { "none" }
        >
            <td colspan="11">
                {move || loaded_detail.get().map_or_else(
                    || view! { <p class=style::inline_loading>{tr(TextKey::Loading)}</p> }.into_any(),
                    |detail| view! {
                        <InlineItemMetadataForm
                            list_id=list_id
                            detail=detail
                            on_saved=Callback::new(move |saved_item_id| {
                                detail_action.dispatch(saved_item_id);
                            })
                        />
                    }.into_any(),
                )}
            </td>
        </tr>
        </>
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
fn InlineItemMetadataForm(
    list_id: i64,
    detail: AddonMediaListItemDetail,
    on_saved: Callback<i64>,
) -> impl IntoView {
    let item_id = detail.item.id;
    let original = detail.nfo.metadata.clone();
    let metadata_conflicts = detail.nfo.conflicts.clone();
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
    let legacy_artwork = original.artwork_urls.first().cloned().unwrap_or_default();
    let artwork_open = RwSignal::new(false);
    let preview_artwork = Memo::new(move |_| {
        let value = [thumbnail_url.get(), poster_url.get(), fanart_url.get()]
            .into_iter()
            .find(|value| !value.trim().is_empty())
            .unwrap_or_else(|| legacy_artwork.clone());
        artwork_preview_url(&value)
    });
    let toast = use_toast();
    let save_action = Action::new(move |metadata: &AddonMediaListNfoMetadata| {
        let metadata = metadata.clone();
        let expected_etag = etag.get_untracked();
        async move { save_addon_media_list_item_nfo(list_id, item_id, metadata, expected_etag).await }
    });
    Effect::new(move |_| {
        if let Some(result) = save_action.value().get() {
            match result {
                Ok(()) => {
                    toast.push(tr(TextKey::Saved).to_string(), ToastSeverity::Success);
                    on_saved.run(item_id);
                }
                Err(error) => toast.push(
                    save_failed(addon_media_list_error_message(error)),
                    ToastSeverity::Error,
                ),
            }
        }
    });
    let is_episode = detail.item.content_kind == AddonMediaListContentKind::TelevisionEpisode;
    let is_music = matches!(
        detail.item.content_kind,
        AddonMediaListContentKind::MusicVideo | AddonMediaListContentKind::Song
    );
    view! {
        <section class=style::inline_editor>
            <MetadataConflictSummary conflicts=metadata_conflicts/>
            <div class=style::inline_edit_grid>
                <label class=style::metadata_field>
                    <span>{tr(TextKey::Title)}</span>
                    <input class=style::boxed_input prop:value=move || title.get() on:input=move |event| title.set(event_target_value(&event))/>
                </label>
                <label class=style::metadata_field>
                    <span>{tr(TextKey::OriginalTitle)}</span>
                    <input class=style::boxed_input prop:value=move || original_title.get() on:input=move |event| original_title.set(event_target_value(&event))/>
                </label>
                <label class=style::metadata_field>
                    <span>{tr(TextKey::Year)}</span>
                    <input class=style::boxed_input inputmode="numeric" prop:value=move || year.get() on:input=move |event| year.set(event_target_value(&event))/>
                </label>
                <label class=style::metadata_field style:display=if is_episode { "grid" } else { "none" }>
                    <span>{tr(TextKey::ReleaseDate)}</span>
                    <input class=style::boxed_input type="date" prop:value=move || release_date.get() on:input=move |event| release_date.set(event_target_value(&event))/>
                </label>
                <label class=style::metadata_field style:display=if is_episode { "grid" } else { "none" }>
                    <span>{tr(TextKey::Season)}</span>
                    <input class=style::boxed_input inputmode="numeric" prop:value=move || season.get() on:input=move |event| season.set(event_target_value(&event))/>
                </label>
                <label class=style::metadata_field style:display=if is_episode { "grid" } else { "none" }>
                    <span>{tr(TextKey::Episode)}</span>
                    <input class=style::boxed_input inputmode="numeric" prop:value=move || episode.get() on:input=move |event| episode.set(event_target_value(&event))/>
                </label>
                <label class=format!("{} {}", style::metadata_field, style::metadata_wide)>
                    <span>{tr(TextKey::Summary)}</span>
                    <textarea class=style::boxed_textarea rows="3" prop:value=move || plot.get() on:input=move |event| plot.set(event_target_value(&event))/>
                </label>
                <label class=format!("{} {}", style::metadata_field, style::tag_field)>
                    <span>{tr(TextKey::Tags)}</span>
                    <textarea class=style::tag_input rows="4" spellcheck="false" prop:value=move || tags.get() on:input=move |event| tags.set(event_target_value(&event))/>
                    <small>{move || format!("{} · {}", line_values(&tags.get()).len(), tr(TextKey::OneTagPerLine))}</small>
                </label>
                <label class=style::metadata_field style:display=if is_episode { "none" } else { "grid" }>
                    <span>{tr(TextKey::ReleaseDate)}</span>
                    <input class=style::boxed_input type="date" prop:value=move || release_date.get() on:input=move |event| release_date.set(event_target_value(&event))/>
                </label>
                <label class=style::metadata_field>
                    <span>{tr(TextKey::Genres)}</span>
                    <input class=style::boxed_input prop:value=move || genres.get() on:input=move |event| genres.set(event_target_value(&event))/>
                </label>
                <label class=style::metadata_field style:display=if is_music { "grid" } else { "none" }>
                    <span>{tr(TextKey::Artist)}</span>
                    <input class=style::boxed_input prop:value=move || artists.get() on:input=move |event| artists.set(event_target_value(&event))/>
                </label>
                <label class=style::metadata_field>
                    <span>{tr(TextKey::Studios)}</span>
                    <input class=style::boxed_input prop:value=move || studios.get() on:input=move |event| studios.set(event_target_value(&event))/>
                </label>
                <label class=style::metadata_field>
                    <span>{tr(TextKey::Directors)}</span>
                    <input class=style::boxed_input prop:value=move || directors.get() on:input=move |event| directors.set(event_target_value(&event))/>
                </label>
                <label class=style::metadata_field>
                    <span>{tr(TextKey::Writers)}</span>
                    <input class=style::boxed_input prop:value=move || writers.get() on:input=move |event| writers.set(event_target_value(&event))/>
                </label>
                <label class=style::metadata_field>
                    <span>{tr(TextKey::Languages)}</span>
                    <input class=style::boxed_input prop:value=move || languages.get() on:input=move |event| languages.set(event_target_value(&event))/>
                </label>
                <label class=format!("{} {}", style::metadata_field, style::metadata_wide)>
                    <span>{tr(TextKey::CastAndCredits)}</span>
                    <textarea class=style::boxed_textarea rows="2" spellcheck="false" prop:value=move || people.get() on:input=move |event| people.set(event_target_value(&event))/>
                    <small>{tr(TextKey::OneCreditPerLine)}</small>
                </label>
            </div>
            <aside class=style::inline_artwork>
                <div class=style::artwork_preview>
                    <img src=move || preview_artwork.get() alt="" style:display=move || if preview_artwork.with(String::is_empty) { "none" } else { "block" }/>
                    <span style:display=move || if preview_artwork.with(String::is_empty) { "inline-flex" } else { "none" }>
                        <span style=icon_style(IconName::Image, "1.75rem") aria-hidden="true"></span>
                    </span>
                    <button type="button" class=style::artwork_edit title=tr(TextKey::EditMetadata) aria-label=tr(TextKey::EditMetadata) on:click=move |_| artwork_open.update(|open| *open = !*open)>
                        <span style=icon_style(IconName::Edit, "1rem") aria-hidden="true"></span>
                    </button>
                </div>
                <div class=style::artwork_fields style:display=move || if artwork_open.get() { "grid" } else { "none" }>
                    <ArtworkRoleInput list_id item_id=Some(item_id) role=AddonMediaListArtworkRole::Poster label=TextKey::Poster url=poster_url etag candidates=artwork_candidates(&original.artwork, AddonMediaListArtworkRole::Poster)/>
                    <ArtworkRoleInput list_id item_id=Some(item_id) role=AddonMediaListArtworkRole::Fanart label=TextKey::FanArt url=fanart_url etag candidates=artwork_candidates(&original.artwork, AddonMediaListArtworkRole::Fanart)/>
                    <ArtworkRoleInput list_id item_id=Some(item_id) role=AddonMediaListArtworkRole::Thumb label=TextKey::Thumbnail url=thumbnail_url etag candidates=artwork_candidates(&original.artwork, AddonMediaListArtworkRole::Thumb)/>
                </div>
            </aside>
            <footer class=style::inline_actions>
                <button type="button" class=style::btn_add disabled=move || save_action.pending().get() on:click=move |_| {
                    let mut metadata = original.clone();
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
                    metadata.people = people_values(&people.get_untracked());
                    metadata.artwork = artwork_values(
                        &original.artwork,
                        [
                            (&poster_url.get_untracked(), AddonMediaListArtworkRole::Poster),
                            (&fanart_url.get_untracked(), AddonMediaListArtworkRole::Fanart),
                            (&thumbnail_url.get_untracked(), AddonMediaListArtworkRole::Thumb),
                        ],
                    );
                    save_action.dispatch(metadata);
                }>
                    <span style=icon_style(IconName::Save, "1rem") aria-hidden="true"></span>
                    <span>{tr(TextKey::Save)}</span>
                </button>
            </footer>
        </section>
    }
}

#[component]
pub fn AddonMediaListsPage() -> impl IntoView {
    let query = use_query_map();
    let error = query.with_untracked(|params| params.get("error").unwrap_or_default());
    let initial = Resource::new_blocking(
        || (),
        |()| async {
            let page_size = get_addon_media_lists_page_size().await?;
            let rows = list_addon_media_lists(0, page_size).await?;
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
                            initial_rows=initial_rows
                            initial_total=initial_total
                            providers=providers
                        />
                    }
                    .into_any()
                }))
            }}
        </Suspense>
    }
}

#[island]
fn AddonMediaListsIsland(
    initial_error: String,
    initial_page_size: u32,
    initial_rows: Vec<AddonMediaListRow>,
    initial_total: u64,
    providers: Vec<AddonMediaListProviderRow>,
) -> impl IntoView {
    let page = RwSignal::new(0u32);
    let page_size = RwSignal::new(initial_page_size);
    let current_data = RwSignal::new((initial_rows, initial_total));
    // The add-on is resolved from the URL, so the dialog opens with no
    // provider preselected instead of whichever one happens to sort first.
    let add_addon_id = RwSignal::new(String::new());
    let add_addon_chosen = RwSignal::new(false);
    let add_source_url = RwSignal::new(String::new());
    let add_folder_name = RwSignal::new(String::new());
    let add_content_kind = RwSignal::new(AddonMediaListContentKind::Auto);
    let add_load_item_details = RwSignal::new(true);
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
        async move { load_or_notify(list_addon_media_lists(p, size).await, toast) }
    });
    Effect::new(move |_| {
        if let Some(next_data) = data.get() {
            current_data.set(next_data);
        }
    });

    let add_action = Action::new(
        |value: &(String, String, String, AddonMediaListContentKind, bool)| {
            let (addon_id, source_url, folder_name, content_kind_override, load_item_details) =
                value.clone();
            async move {
                add_addon_media_list(
                    addon_id,
                    source_url,
                    folder_name,
                    content_kind_override,
                    String::new(),
                    load_item_details,
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
            <div class=style::action_bar>
                <a class=style::btn_add href="/media/addon-lists/add">
                    <span style=icon_style(IconName::Add, "1.125rem") aria-hidden="true"></span>
                    <span>{tr(TextKey::AddAddonMediaList)}</span>
                </a>
            </div>
            <section class=style::content>
                <h1 class=style::page_title>{tr(TextKey::AddonMediaLists)}</h1>
                <div class=style::divider/>
                <p class=style::empty style:display=move || if has_initial_error { "" } else { "none" }>
                    {initial_error_message}
                </p>

                {move || {
                    let (rows, total) = current_data.get();
                    let size = page_size.get().max(1);
                    let total_pages = u32::try_from(total.div_ceil(u64::from(size)))
                        .unwrap_or(u32::MAX)
                        .max(1);
                    let current_page = page.get().min(total_pages.saturating_sub(1));
                    let range_start = if total == 0 {
                        0
                    } else {
                        u64::from(current_page) * u64::from(size) + 1
                    };
                    let range_end = ((u64::from(current_page) + 1) * u64::from(size)).min(total);

                    view! {
                                <div class=style::table_wrap>
                                    <table>
                                        <colgroup>
                                            <col/>
                                            <col/>
                                            <col/>
                                            <col class=style::actions_col/>
                                        </colgroup>
                                        <thead>
                                            <tr>
                                                <th>{tr(TextKey::Id)}</th>
                                                <th>{tr(TextKey::Name)}</th>
                                                <th>{tr(TextKey::MatchStatus)}</th>
                                                <th/>
                                            </tr>
                                        </thead>
                                        <tbody>
                                            {if rows.is_empty() {
                                                view! {
                                                    <tr>
                                                        <td class=style::empty_cell colspan="4">{tr(TextKey::NoAddonMediaListsYet)}</td>
                                                    </tr>
                                                }.into_any()
                                            } else {
                                                rows.into_iter().map(|row| {
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
                                                            "{} · {} {} · {} / {} {}",
                                                            tr(import_state_text_key(progress.state)),
                                                            progress.discovered_count,
                                                            tr(TextKey::Discovered),
                                                            progress.detail_complete,
                                                            progress.detail_total,
                                                            tr(TextKey::Details),
                                                        )
                                                    });
                                                    let has_import_summary = !import_summary.is_empty();
                                                    let row_for_delete = row.clone();
                                                    view! {
                                                        <tr>
                                                            <td>{format!("{addon_id} / {provider_id}")}</td>
                                                            <td>
                                                                <span>{name}</span>
                                                                <small class=style::row_note style:display=if has_import_summary { "block" } else { "none" }>{import_summary}</small>
                                                            </td>
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
                                                                    <form method="post" action="/api/addon-lists/match">
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
                                                                    <form method="post" action="/api/addon-lists/delete">
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
                                                }).collect_view().into_any()
                                            }}
                                        </tbody>
                                    </table>
                                </div>
                                <div class=style::pager>
                                    <span>{format!("{}:", tr(TextKey::RowsPerPage))}</span>
                                    <select
                                        class=style::page_size
                                        aria-label=tr(TextKey::RowsPerPage)
                                        prop:value=move || page_size.get().to_string()
                                        on:change=move |event| {
                                            if let Ok(value) = event_target_value(&event).parse::<u32>() {
                                                page_size.set(value.max(1));
                                                page.set(0);
                                            }
                                        }
                                    >
                                        <option value="10">"10"</option>
                                        <option value="25">"25"</option>
                                        <option value="50">"50"</option>
                                        <option value="100">"100"</option>
                                    </select>
                                    <span class=style::pager_status>
                                        {range_of(
                                            u32::try_from(range_start).unwrap_or(u32::MAX),
                                            u32::try_from(range_end).unwrap_or(u32::MAX),
                                            total,
                                        )}
                                    </span>
                                    <div class=style::pager_buttons>
                                        <button
                                            type="button"
                                            class=style::icon_button
                                            disabled=move || page.get() == 0
                                            title=tr(TextKey::PreviousPage)
                                            aria-label=tr(TextKey::PreviousPage)
                                            on:click=move |_| page.update(|page| *page = page.saturating_sub(1))
                                        >
                                            <span style=icon_style(IconName::ChevronLeft, "1.25rem") aria-hidden="true"></span>
                                        </button>
                                        <button
                                            type="button"
                                            class=style::icon_button
                                            disabled=move || { page.get() + 1 >= total_pages }
                                            title=tr(TextKey::NextPage)
                                            aria-label=tr(TextKey::NextPage)
                                            on:click=move |_| {
                                                page.update(|page| {
                                                    *page = (*page + 1).min(total_pages.saturating_sub(1));
                                                });
                                            }
                                        >
                                            <span style=icon_style(IconName::ChevronRight, "1.25rem") aria-hidden="true"></span>
                                        </button>
                                    </div>
                                </div>
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
                load_item_details=add_load_item_details
                providers=providers
                on_submit=Callback::new(move |value: (String, String, String, AddonMediaListContentKind, bool)| {
                    add_action.dispatch(value);
                })
            />
            <DeleteDialog
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
    load_item_details: RwSignal<bool>,
    providers: Vec<AddonMediaListProviderRow>,
    on_submit: Callback<(String, String, String, AddonMediaListContentKind, bool)>,
) -> impl IntoView {
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
                    action="/api/addon-lists/add"
                    on:submit=move |event| {
                        event.prevent_default();
                        let addon_id = addon_id.get_untracked();
                        let source_url = source_url.get_untracked();
                        let folder_name = folder_name.get_untracked();
                        let content_kind = content_kind.get_untracked();
                        let load_item_details = load_item_details.get_untracked();
                        // The add-on may be empty: the server resolves it
                        // from the URL, so the same rule applies to the plain
                        // HTML form post.
                        if !source_url.trim().is_empty() {
                            on_submit.run((addon_id, source_url, folder_name, content_kind, load_item_details));
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
/// an existing list. Publication remains the second and only materializing
/// step.
#[component]
pub fn AddAddonMediaListPage() -> impl IntoView {
    let providers = Resource::new_blocking(|| (), |()| list_addon_media_list_providers());
    view! {
        <Suspense fallback=admin_loading_resource_view>
            {move || providers.get().map(|result| admin_result_view(result, |providers| view! {
                <AddAddonMediaListPageIsland providers=providers/>
            }.into_any()))}
        </Suspense>
    }
}

#[island]
fn AddAddonMediaListPageIsland(providers: Vec<AddonMediaListProviderRow>) -> impl IntoView {
    let addon_id = RwSignal::new(String::new());
    let addon_chosen = RwSignal::new(false);
    let source_url = RwSignal::new(String::new());
    let folder_name = RwSignal::new(String::new());
    let presentation = RwSignal::new(AddonMediaListPresentationKind::TelevisionShow);
    let auto_refresh = RwSignal::new(true);
    let generate_playlist = RwSignal::new(false);
    let load_item_details = RwSignal::new(true);
    let resolved_name = RwSignal::new(String::new());
    let toast = use_toast();

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
            bool,
            bool,
            bool,
        )| {
            let (
                addon_id,
                source_url,
                folder_name,
                presentation,
                auto_refresh,
                playlist,
                load_item_details,
            ) = value.clone();
            async move {
                let id = add_addon_media_list(
                    addon_id,
                    source_url,
                    folder_name,
                    presentation_content_kind(presentation),
                    String::new(),
                    load_item_details,
                )
                .await?;
                let mut form = get_addon_media_list_form(id)
                    .await?
                    .ok_or_else(|| ServerFnError::new("imported add-on list was not found"))?;
                form.auto_refresh = auto_refresh;
                form.generate_playlist = playlist;
                update_addon_media_list(form).await?;
                request_addon_media_list_presentation(id, presentation, None).await?;
                Ok::<i64, ServerFnError>(id)
            }
        },
    );
    Effect::new(move |_| {
        if let Some(result) = import_action.value().get() {
            match result {
                Ok(id) => set_browser_location(&format!("/media/addon-lists/{id}?import=1")),
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

    view! {
        <div class=style::page>
            <div class=style::action_bar>
                <a class=style::icon_button href="/media/addon-lists" title=tr(TextKey::Back) aria-label=tr(TextKey::Back)>
                    <span style=icon_style(IconName::ChevronLeft, "1.25rem") aria-hidden="true"></span>
                </a>
            </div>
            <main class=style::import_shell>
                <header class=style::import_header>
                    <div>
                        <h1 class=style::page_title>{tr(TextKey::AddAddonMediaList)}</h1>
                        <p>{tr(TextKey::MetadataDraftNotice)}</p>
                    </div>
                    <ol class=style::stepper aria-label=tr(TextKey::AddAddonMediaList)>
                        <li class=style::step_active><b>"1"</b><span>{tr(TextKey::Import)}</span></li>
                        <li><b>"2"</b><span>{tr(TextKey::PublishMetadata)}</span></li>
                    </ol>
                </header>

                <form class=style::source_card on:submit=move |event| {
                    event.prevent_default();
                    if source_url.with(|value| !value.trim().is_empty()) {
                        import_action.dispatch((
                            addon_id.get_untracked(),
                            source_url.get_untracked(),
                            folder_name.get_untracked(),
                            presentation.get_untracked(),
                            auto_refresh.get_untracked(),
                            generate_playlist.get_untracked(),
                            load_item_details.get_untracked(),
                        ));
                    }
                }>
                    <label class=style::source_url_field>
                        <span class=style::field_label>{tr(TextKey::AddonMediaListUrl)}</span>
                        <input
                            class=style::boxed_input
                            type="url"
                            name="source_url"
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
                            {providers.into_iter().map(|provider| view! {
                                <option value=provider.addon_id>{provider.name}</option>
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
                        <div class=style::source_field>
                            <span class=style::field_title>
                                <span style=move || icon_style(if auto_refresh.get() { IconName::Clock } else { IconName::RefreshOff }, "1.05rem") aria-hidden="true"></span>
                                <span>{tr(TextKey::AutomaticRefresh)}</span>
                            </span>
                            <label class=style::compact_toggle>
                                <input type="checkbox" prop:checked=move || auto_refresh.get() on:change=move |event| auto_refresh.set(event_target_checked(&event))/>
                                <span>{tr(TextKey::UpdateAddonMediaListDaily)}</span>
                            </label>
                        </div>
                    </div>

                    <label class=style::source_field>
                        <span class=style::field_title>
                            <span style=icon_style(IconName::Folder, "1.05rem") aria-hidden="true"></span>
                            <span>{tr(TextKey::AddonMediaListFolder)}</span>
                        </span>
                        <input class=style::boxed_input type="text" prop:value=move || folder_name.get() on:input=move |event| folder_name.set(event_target_value(&event)) placeholder=tr(TextKey::AddonMediaListFolderHelp)/>
                    </label>

                    <label class=style::playlist_option>
                        <input type="checkbox" prop:checked=move || generate_playlist.get() on:change=move |event| generate_playlist.set(event_target_checked(&event))/>
                        <span>
                            <b>{tr(TextKey::GeneratePlaylist)}</b>
                            <small>{tr(TextKey::GeneratePlaylistFromSortedAddonMediaListItems)}</small>
                        </span>
                    </label>

                    <label class=style::playlist_option>
                        <input type="checkbox" prop:checked=move || load_item_details.get() on:change=move |event| load_item_details.set(event_target_checked(&event))/>
                        <span>
                            <b>{tr(TextKey::LoadItemDetailsAfterPublishing)}</b>
                            <small>{move || tr(if load_item_details.get() {
                                TextKey::LoadItemDetailsHelp
                            } else {
                                TextKey::OverviewOnlyManualDetailsHelp
                            })}</small>
                        </span>
                    </label>

                    <footer class=style::import_footer>
                        <p>{tr(TextKey::MetadataDraftNotice)}</p>
                        <button class=style::btn_add type="submit" disabled=move || source_url.with(|value| value.trim().is_empty()) || import_action.pending().get()>
                            <span style=icon_style(IconName::Download, "1.1rem") aria-hidden="true"></span>
                            <span>{tr(TextKey::Import)}</span>
                        </button>
                    </footer>
                </form>
            </main>
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
    let initial = Resource::new_blocking(
        move || id,
        |id| async move {
            let form = get_addon_media_list_form(id).await?;
            let items = list_addon_media_list_items(id).await?;
            let show_nfo = get_addon_media_list_show_nfo(id).await?;
            let providers = list_addon_media_list_providers().await?;
            Ok::<_, ServerFnError>((form, items, show_nfo, providers))
        },
    );

    view! {
        <Suspense fallback=admin_loading_resource_view>
            {move || {
                let error = error.clone();
                initial.get().map(move |result| admin_result_view(result, |(initial_form, initial_items, initial_show_nfo, providers)| {
                    view! {
                        <AddonMediaListEditorIsland
                            id=id
                            initial_error=error.clone()
                            initial_form=initial_form
                            initial_items=initial_items
                            initial_show_nfo=initial_show_nfo
                            providers=providers
                        />
                    }
                    .into_any()
                }))
            }}
        </Suspense>
    }
}

#[component]
pub fn AddonMediaListItemPage() -> impl IntoView {
    let params = use_params_map();
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
    let initial = Resource::new_blocking(
        move || (list_id, item_id),
        |(list_id, item_id)| get_addon_media_list_item(list_id, item_id),
    );
    view! {
        <Suspense fallback=admin_loading_resource_view>
            {move || initial.get().map(|result| admin_result_view(result, move |detail| {
                detail.map_or_else(
                    || view! { <p class=style::empty>{tr(TextKey::AddonMediaListNotFound)}</p> }.into_any(),
                    |detail| view! { <AddonMediaListItemIsland list_id=list_id detail=detail/> }.into_any(),
                )
            }))}
        </Suspense>
    }
}

#[island]
fn AddonMediaListItemIsland(list_id: i64, detail: AddonMediaListItemDetail) -> impl IntoView {
    let item_id = detail.item.id;
    let original = detail.nfo.metadata.clone();
    let etag = detail.nfo.etag.clone();
    let title = RwSignal::new(original.title.clone());
    let plot = RwSignal::new(original.plot.clone().unwrap_or_default());
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
    let provider_chapters = detail.item.provider_chapter_input.clone();
    let saved_operator_chapters = RwSignal::new(detail.item.chapter_input.clone());
    let chapters_dirty = RwSignal::new(false);
    let submitted_chapters = RwSignal::new(detail.item.chapter_input.clone());
    let chapters = RwSignal::new(if detail.item.chapter_input.trim().is_empty() {
        provider_chapters.clone()
    } else {
        detail.item.chapter_input.clone()
    });
    let toast = use_toast();
    let save_action = Action::new(move |value: &(AddonMediaListNfoMetadata, String, String)| {
        let (metadata, etag, chapters) = value.clone();
        async move {
            save_addon_media_list_item_nfo(list_id, item_id, metadata, etag).await?;
            update_addon_media_list_item_chapters(item_id, chapters).await
        }
    });
    let restore_action = Action::new(move |(): &()| async move {
        update_addon_media_list_item_chapters(item_id, String::new()).await
    });
    Effect::new(move |_| {
        if let Some(result) = save_action.value().get() {
            match result {
                Ok(()) => {
                    saved_operator_chapters.set(submitted_chapters.get_untracked());
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
    Effect::new(move |_| {
        if let Some(result) = restore_action.value().get() {
            match result {
                Ok(()) => toast.push(tr(TextKey::Saved).to_string(), ToastSeverity::Success),
                Err(error) => toast.push(
                    save_failed(addon_media_list_error_message(error)),
                    ToastSeverity::Error,
                ),
            }
        }
    });
    let build_metadata = move || {
        let mut metadata = original.clone();
        metadata.title = title.get_untracked();
        metadata.plot = non_empty_value(&plot.get_untracked());
        metadata.season = season.get_untracked().trim().parse().ok();
        metadata.episode = episode.get_untracked().trim().parse().ok();
        metadata.genres = comma_values(&genres.get_untracked());
        metadata.tags = line_values(&tags.get_untracked());
        metadata
    };
    view! {
        <div class=style::page>
            <div class=style::action_bar>
                <a class=style::icon_button href=format!("/media/addon-lists/{list_id}")>{tr(TextKey::Back)}</a>
                <button type="button" class=style::btn_add on:click=move |_| {
                    let chapter_input = if chapters_dirty.get_untracked() {
                        chapters.get_untracked()
                    } else {
                        saved_operator_chapters.get_untracked()
                    };
                    submitted_chapters.set(chapter_input.clone());
                    save_action.dispatch((build_metadata(), etag.clone(), chapter_input));
                }>{tr(TextKey::Save)}</button>
            </div>
            <section class=style::editor>
                <h1 class=style::page_title>{detail.item.display_title}</h1>
                <label class=style::field_row><span class=style::field_label>{tr(TextKey::Title)}</span><input class=style::text_input prop:value=move || title.get() on:input=move |event| title.set(event_target_value(&event))/></label>
                <label class=style::field_row><span class=style::field_label>{tr(TextKey::Summary)}</span><textarea class=style::text_input rows="8" prop:value=move || plot.get() on:input=move |event| plot.set(event_target_value(&event))/></label>
                <label class=style::field_row><span class=style::field_label>{tr(TextKey::Season)}</span><input class=style::text_input inputmode="numeric" prop:value=move || season.get() on:input=move |event| season.set(event_target_value(&event))/></label>
                <label class=style::field_row><span class=style::field_label>{tr(TextKey::Episode)}</span><input class=style::text_input inputmode="numeric" prop:value=move || episode.get() on:input=move |event| episode.set(event_target_value(&event))/></label>
                <label class=style::field_row><span class=style::field_label>{tr(TextKey::Genres)}</span><input class=style::text_input prop:value=move || genres.get() on:input=move |event| genres.set(event_target_value(&event))/></label>
                <label class=style::field_row><span class=style::field_label>{tr(TextKey::Tags)}</span><textarea class=style::tag_input rows="4" spellcheck="false" prop:value=move || tags.get() on:input=move |event| tags.set(event_target_value(&event))/></label>
                <label class=style::field_row><span class=style::field_label>{tr(TextKey::ChapterInput)}</span><textarea class=style::text_input rows="8" prop:value=move || chapters.get() on:input=move |event| {
                    chapters.set(event_target_value(&event));
                    chapters_dirty.set(true);
                }/></label>
                {(!provider_chapters.is_empty()).then(|| view! {
                    <button type="button" class=style::btn_cancel on:click=move |_| {
                        chapters.set(provider_chapters.clone());
                        saved_operator_chapters.set(String::new());
                        chapters_dirty.set(false);
                        restore_action.dispatch(());
                    }>{tr(TextKey::RestoreProviderChapters)}</button>
                })}
            </section>
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

fn people_values(value: &str) -> Vec<AddonMediaListNfoPerson> {
    value
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
        .collect()
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

fn artwork_preview_url(value: &str) -> String {
    value.strip_prefix("ersatzrs-upload://").map_or_else(
        || value.to_owned(),
        |id| format!("/api/addon-media-lists/artwork/{id}"),
    )
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
        || (presentation == AddonMediaListPresentationKind::TelevisionShow
            && (item.season.is_none() || item.episode.is_none()))
        || (presentation == AddonMediaListPresentationKind::Movies
            && item.collection_role == AddonMediaListItemRole::Auto)
}

fn episode_numbering_preview_values(
    items: &[AddonMediaListItemRow],
    season: i32,
    starting_episode: i32,
    count: usize,
    replace_existing: bool,
) -> Vec<i32> {
    let candidates = items
        .iter()
        .filter(|item| item.included && item.parent_provider_id.is_none())
        .collect::<Vec<_>>();
    let mut occupied = BTreeSet::new();
    if replace_existing {
        occupied.extend(
            candidates
                .iter()
                .skip(count)
                .filter(|item| item.season == Some(season))
                .filter_map(|item| item.episode.filter(|value| *value >= 1)),
        );
    } else {
        occupied.extend(
            candidates
                .iter()
                .filter(|item| item.season == Some(season))
                .filter_map(|item| item.episode.filter(|value| *value >= 1)),
        );
    }
    let mut values = Vec::with_capacity(count);
    let mut episode = starting_episode;
    while values.len() < count {
        while occupied.contains(&episode) {
            let Some(next) = episode.checked_add(1) else {
                return values;
            };
            episode = next;
        }
        values.push(episode);
        occupied.insert(episode);
        episode = episode.saturating_add(1);
    }
    values
}

fn episode_numbering_preview_assignments(
    items: &[AddonMediaListItemRow],
    season: i32,
    starting_episode: i32,
    count: usize,
    replace_existing: bool,
) -> BTreeMap<i64, (i32, i32)> {
    let item_ids = items
        .iter()
        .filter(|item| item.included && item.parent_provider_id.is_none())
        .filter(|item| {
            replace_existing
                || item.season.is_none_or(|value| value < 0)
                || item.episode.is_none_or(|value| value < 1)
        })
        .take(count)
        .map(|item| item.id);
    let values =
        episode_numbering_preview_values(items, season, starting_episode, count, replace_existing);
    item_ids
        .zip(values)
        .map(|(item_id, episode)| (item_id, (season, episode)))
        .collect()
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

#[island]
fn AddonMediaListEditorIsland(
    id: i64,
    initial_error: String,
    initial_form: Option<AddonMediaListForm>,
    initial_items: Vec<AddonMediaListItemRow>,
    initial_show_nfo: AddonMediaListNfoForm,
    providers: Vec<AddonMediaListProviderRow>,
) -> impl IntoView {
    let has_initial_error = !initial_error.is_empty();
    let form = initial_form.unwrap_or_default();
    let form_found = form.id != 0 || id == 0;
    let form_id = RwSignal::new(form.id);
    let addon_id = form.addon_id.clone();
    let source_url = form.source_url.clone();
    let submit_addon_id = addon_id.clone();
    let submit_source_url = source_url.clone();
    let auto_refresh = RwSignal::new(form.auto_refresh);
    let generate_playlist = RwSignal::new(form.generate_playlist);
    let detail_import_policy = RwSignal::new(form.detail_import_policy);
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
    let general_metadata_open = RwSignal::new(false);
    let is_draft = form.review_state != AddonMediaListReviewState::Published;
    let show_metadata = initial_show_nfo.metadata.clone();
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
    let detected_provider_name = providers
        .iter()
        .find(|provider| provider.addon_id == form.addon_id)
        .map_or_else(|| form.addon_id.clone(), |provider| provider.name.clone());
    // Changing the add-on re-imports the list through the new provider, so the
    // operator confirms before item identity is re-derived.
    let selected_addon_id = RwSignal::new(form.addon_id.clone());
    let change_addon_open = RwSignal::new(false);
    let apply_content_kind_open = RwSignal::new(false);
    let change_addon_action =
        Action::new(move |addon_id: &String| change_addon_media_list_addon(id, addon_id.clone()));
    // The list value is a bulk default rather than the per-item truth, so it is
    // only pushed onto entries when the operator asks for it.
    let apply_content_kind_action = Action::new(|value: &(i64, AddonMediaListContentKind)| {
        let (id, content_kind) = *value;
        async move { apply_addon_media_list_content_kind(id, content_kind).await }
    });
    let episode_numbering_open = RwSignal::new(false);
    let episode_numbering_season = RwSignal::new("1".to_owned());
    let episode_numbering_start = RwSignal::new("1".to_owned());
    let episode_numbering_count = RwSignal::new(String::new());
    let replace_existing_episode_numbers = RwSignal::new(false);
    let assign_episode_numbers_action =
        Action::new(|value: &(i64, i32, i32, Option<u32>, bool)| {
            let (id, season, starting_episode, item_count, missing_only) = *value;
            async move {
                assign_addon_media_list_episode_numbers(
                    id,
                    season,
                    starting_episode,
                    item_count,
                    missing_only,
                )
                .await
            }
        });
    let checked_auto_refresh = form.auto_refresh;
    let checked_generate_playlist = form.generate_playlist;
    let replacement_target = RwSignal::new(None::<i64>);
    let replacement_query = RwSignal::new(String::new());
    let replacement_results = RwSignal::new(Vec::<AddonMediaListReplacementRow>::new());

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
                if !show_metadata.title.trim().is_empty() {
                    save_addon_media_list_show_nfo(id, show_metadata, show_etag).await?;
                }
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
    let selected_items = RwSignal::new(BTreeSet::<i64>::new());
    let bulk_liveness_value = RwSignal::new("provider".to_owned());
    let need_review_filter = RwSignal::new(false);
    let item_page = RwSignal::new(0_u32);
    let item_page_size = RwSignal::new(DEFAULT_ITEM_PAGE_SIZE);
    let remote_item_ids = Memo::new(move |_| {
        items.with(|items| {
            items
                .iter()
                .filter(|item| item.kind == AddonMediaListItemKind::RemoteStream)
                .map(|item| item.id)
                .collect::<Vec<_>>()
        })
    });
    let selected_remote_count = Memo::new(move |_| {
        let remote_ids = remote_item_ids.get();
        selected_items.with(|selected| {
            remote_ids
                .iter()
                .filter(|item_id| selected.contains(item_id))
                .count()
        })
    });
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
            items
                .iter()
                .filter(|item| {
                    !need_review_filter.get() || item_needs_review(item, presentation_kind.get())
                })
                .count()
        })
    });
    let episode_numbering_candidate_count = Memo::new(move |_| {
        items.with(|items| {
            items
                .iter()
                .filter(|item| item.included && item.parent_provider_id.is_none())
                .filter(|item| {
                    replace_existing_episode_numbers.get()
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
    let episode_numbering_preview = Memo::new(move |_| {
        let season = episode_numbering_season
            .get()
            .trim()
            .parse::<i32>()
            .ok()
            .filter(|value| *value >= 0)?;
        let starting_episode = episode_numbering_start
            .get()
            .trim()
            .parse::<i32>()
            .ok()
            .filter(|value| *value >= 1)?;
        let count = episode_numbering_assignment_count.get();
        let values = items.with(|items| {
            episode_numbering_preview_values(
                items,
                season,
                starting_episode,
                count,
                replace_existing_episode_numbers.get(),
            )
        });
        Some((season, values))
    });
    let episode_numbering_row_preview = Memo::new(move |_| {
        if !episode_numbering_open.get()
            || presentation_kind.get() != AddonMediaListPresentationKind::TelevisionShow
        {
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
        let Some(starting_episode) = episode_numbering_start
            .get()
            .trim()
            .parse::<i32>()
            .ok()
            .filter(|value| *value >= 1)
        else {
            return BTreeMap::new();
        };
        let count = episode_numbering_assignment_count.get();
        items.with(|items| {
            episode_numbering_preview_assignments(
                items,
                season,
                starting_episode,
                count,
                replace_existing_episode_numbers.get(),
            )
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
    let episode_numbering_off_page_count = Memo::new(move |_| {
        let preview = episode_numbering_row_preview.get();
        if preview.is_empty() {
            return 0;
        }
        let (start, end, _, _, _) = item_page_range.get();
        let visible_ids = items.with(|items| {
            items
                .iter()
                .filter(|item| {
                    !need_review_filter.get() || item_needs_review(item, presentation_kind.get())
                })
                .skip(start)
                .take(end.saturating_sub(start))
                .map(|item| item.id)
                .collect::<BTreeSet<_>>()
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
    let item_content_kind_action = Action::new(|value: &(i64, AddonMediaListContentKind)| {
        let (item_id, content_kind) = *value;
        async move { set_addon_media_list_item_content_kind(item_id, content_kind).await }
    });
    let item_inclusion_action = Action::new(|value: &(i64, i64, bool)| {
        let (list_id, item_id, included) = *value;
        async move { set_addon_media_list_item_included(list_id, item_id, included).await }
    });
    let item_role_action = Action::new(|value: &(i64, i64, AddonMediaListItemRole)| {
        let (list_id, item_id, role) = *value;
        async move { set_addon_media_list_item_role(list_id, item_id, role).await }
    });
    let item_liveness_action = Action::new(|value: &(i64, i64, Option<RemoteStreamLiveness>)| {
        let (list_id, item_id, liveness) = *value;
        async move { set_addon_media_list_item_liveness(list_id, item_id, liveness).await }
    });
    let bulk_liveness_action =
        Action::new(|value: &(i64, Option<RemoteStreamLiveness>, Vec<i64>)| {
            let (id, liveness, item_ids) = value.clone();
            async move {
                let item_ids = (!item_ids.is_empty()).then_some(item_ids);
                apply_addon_media_list_liveness(id, liveness, item_ids).await
            }
        });
    let open_item = RwSignal::new(None::<i64>);
    let loaded_detail = RwSignal::new(None::<AddonMediaListItemDetail>);
    let item_detail_action = Action::new(move |item_id: &i64| {
        let item_id = *item_id;
        async move { get_addon_media_list_item(id, item_id).await }
    });
    let request_detail_action = Action::new(|value: &(i64, i64)| {
        let (list_id, item_id) = *value;
        async move { request_addon_media_list_item_details(list_id, item_id).await }
    });
    let control_import_action = Action::new(|value: &(i64, AddonMediaListImportAction)| {
        let (list_id, action) = *value;
        async move { control_addon_media_list_import(list_id, action).await }
    });
    let publish_action = Action::new(|id: &i64| publish_addon_media_list(*id));
    let reload_items = move || {
        reload_items_action.dispatch(id);
    };
    Effect::new(move |_| {
        if let Some(result) = reload_items_action.value().get() {
            match result {
                Ok(next) => items.set(next),
                Err(error) => {
                    toast.push(addon_media_list_error_message(error), ToastSeverity::Error);
                }
            }
        }
    });
    Effect::new(move |_| {
        if let Some(result) = item_detail_action.value().get() {
            match result {
                Ok(detail) => {
                    if let Some(detail) = &detail {
                        items.update(|items| {
                            if let Some(row) = items.iter_mut().find(|row| row.id == detail.item.id)
                            {
                                *row = detail.item.clone();
                            }
                        });
                    }
                    loaded_detail.set(detail);
                }
                Err(error) => {
                    toast.push(addon_media_list_error_message(error), ToastSeverity::Error);
                }
            }
        }
    });
    Effect::new(move |_| {
        if let Some(result) = request_detail_action.value().get() {
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
                            error_delay_ms = 0;
                            let next_marker = next_form.import_progress.as_ref().map(|progress| {
                                (
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
                            if next_marker != previous_item_marker {
                                previous_item_marker = next_marker;
                                reload_items_action.dispatch(id);
                            }
                            let should_stop = import_progress.with_untracked(|progress| {
                                progress.as_ref().is_some_and(|progress| {
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
        if let Some(result) = item_inclusion_action.value().get() {
            match result {
                Ok(()) => reload_items(),
                Err(error) => toast.push(
                    save_failed(addon_media_list_error_message(error)),
                    ToastSeverity::Error,
                ),
            }
        }
        if let Some(result) = item_role_action.value().get() {
            match result {
                Ok(()) => reload_items(),
                Err(error) => toast.push(
                    save_failed(addon_media_list_error_message(error)),
                    ToastSeverity::Error,
                ),
            }
        }
        if let Some(result) = item_liveness_action.value().get() {
            match result {
                Ok(()) => reload_items(),
                Err(error) => toast.push(
                    save_failed(addon_media_list_error_message(error)),
                    ToastSeverity::Error,
                ),
            }
        }
        if let Some(result) = bulk_liveness_action.value().get() {
            match result {
                Ok(_) => {
                    selected_items.set(BTreeSet::new());
                    toast.push(tr(TextKey::Saved).to_string(), ToastSeverity::Success);
                    reload_items();
                }
                Err(error) => toast.push(
                    save_failed(addon_media_list_error_message(error)),
                    ToastSeverity::Error,
                ),
            }
        }
        if let Some(result) = publish_action.value().get() {
            match result {
                Ok(()) => {
                    toast.push(tr(TextKey::Saved).to_string(), ToastSeverity::Success);
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
        if let Some(result) = assign_episode_numbers_action.value().get() {
            match result {
                Ok(_) => {
                    episode_numbering_open.set(false);
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
            <div class=style::action_bar>
                <button
                    form="addon_media_lists-list-editor-form"
                    type="submit"
                    class=style::btn_add
                    on:click=move |event| {
                        event.prevent_default();
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
                        next_show_metadata.people = people_values(&show_people.get_untracked());
                        next_show_metadata.artwork = artwork_values(
                            &show_metadata.artwork,
                            [
                                (&show_poster.get_untracked(), AddonMediaListArtworkRole::Poster),
                                (&show_fanart.get_untracked(), AddonMediaListArtworkRole::Fanart),
                                (&show_thumbnail.get_untracked(), AddonMediaListArtworkRole::Thumb),
                            ],
                        );
                        save_action.dispatch((AddonMediaListForm {
                            id: form_id.get_untracked(),
                            addon_id: submit_addon_id.clone(),
                            source_url: submit_source_url.clone(),
                            folder_name: form.folder_name.clone(),
                            auto_refresh: auto_refresh.get_untracked(),
                            generate_playlist: generate_playlist.get_untracked(),
                            content_kind_override: content_kind_override.get_untracked(),
                            legacy_chapter_input: legacy_chapter_input.clone(),
                            presentation_kind: form.presentation_kind,
                            requested_presentation_kind: form.requested_presentation_kind,
                            presentation_error: form.presentation_error.clone(),
                            review_state: form.review_state,
                            review_error: form.review_error.clone(),
                            published_revision: form.published_revision,
                            detail_import_policy: detail_import_policy.get_untracked(),
                            import_progress: form.import_progress.clone(),
                        }, presentation_kind.get_untracked(), next_show_metadata, show_etag.get_untracked()));
                    }
                >
                    <span style=icon_style(IconName::Save, "1.125rem") aria-hidden="true"></span>
                    <span>{tr(TextKey::SaveAddonMediaList)}</span>
                </button>
            </div>

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
                let has_detail_failed = progress.detail_failed > 0;
                let elapsed = elapsed_from(&progress.started_at).unwrap_or_default();
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
                let contextual_action = match state {
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
                    AddonMediaListImportState::NeedsAttention | AddonMediaListImportState::Cancelled => view! {
                        <button type="button" class=style::btn_cancel on:click=move |_| {
                            control_import_action.dispatch((id, AddonMediaListImportAction::Retry));
                        }>{tr(TextKey::RetryImportNow)}</button>
                    }.into_any(),
                    AddonMediaListImportState::Ready | AddonMediaListImportState::Superseded => ().into_any(),
                };
                view! {
                    <section class=style::import_progress_card data-import-state=format!("{state:?}")>
                        <span class=style::sr_only aria-live="polite" aria-atomic="true">
                            {move || progress_live_summary.get()}
                        </span>
                        <div class=style::import_progress_heading>
                            <div>
                                <strong>{tr(phase)}</strong>
                                <span>{if let Some(total) = total {
                                    format!("{current} / {total} {}", if is_detail_progress { tr(TextKey::Details) } else { tr(TextKey::Discovered) })
                                } else {
                                    format!("{current} {}", tr(TextKey::Discovered))
                                }}</span>
                            </div>
                            <span class=style::import_progress_percent>{total.map_or_else(|| "…".to_owned(), |_| format!("{percent}%"))}</span>
                        </div>
                        {progress_element}
                        <p style:display=if has_current_title { "block" } else { "none" }>{current_title}</p>
                        <p class=style::import_progress_details>
                            <span>{format!("{}: {elapsed}", tr(TextKey::ElapsedTime))}</span>
                            <span style:display=if has_last_progress { "inline" } else { "none" }>{format!("{}: {last_progress}", tr(TextKey::LastSuccessfulProgress))}</span>
                            <span style:display=if has_estimate { "inline" } else { "none" }>{format!("{}: {estimate}", tr(TextKey::EstimatedRemainingTime))}</span>
                        </p>
                        <p class=style::import_progress_details>
                            <span>{format!("{} / {} {}", progress.detail_complete, progress.detail_total, tr(TextKey::DetailsComplete))}</span>
                            <span>{format!("{} {}", progress.detail_skipped, tr(TextKey::DetailsSkipped))}</span>
                            <span style:display=if has_detail_failed { "inline" } else { "none" }>{format!("{} {}", progress.detail_failed, tr(TextKey::Failed))}</span>
                        </p>
                        <p class=style::review_error style:display=if has_diagnostic { "flex" } else { "none" }>{diagnostic}</p>
                        <div class=style::import_progress_actions>
                            {contextual_action}
                        </div>
                    </section>
                }
            })}

            <form
                id="addon_media_lists-list-editor-form"
                method="post"
                action="/api/addon-lists/save"
                class=style::editor
                style:display=move || if form_found { "" } else { "none" }
            >
                            <header
                                class=style::metadata_hero
                                aria-expanded=move || if general_metadata_open.get() { "true" } else { "false" }
                                on:click=move |_| general_metadata_open.update(|open| *open = !*open)
                            >
                                <span class=style::metadata_hero_icon style=move || icon_style(presentation_icon(presentation_kind.get()), "1.6rem") aria-hidden="true"></span>
                                <div>
                                    <h1>{move || show_title.get()}</h1>
                                    <p class=style::metadata_type>{move || tr(presentation_text_key(presentation_kind.get()))}</p>
                                    <p class=style::metadata_description>{move || show_plot.get()}</p>
                                    <p
                                        class=style::metadata_hero_conflict
                                        style:display=if show_metadata_conflict_count == 0 { "none" } else { "inline-flex" }
                                    >
                                        <span style=icon_style(IconName::Warning, "1rem") aria-hidden="true"></span>
                                        <span>{format!("{} {}", show_metadata_conflict_count, tr(TextKey::MetadataConflicts))}</span>
                                    </p>
                                </div>
                                <button
                                    type="button"
                                    class=style::metadata_hero_edit
                                    title=tr(TextKey::EditMetadata)
                                    aria-label=tr(TextKey::EditMetadata)
                                    aria-expanded=move || if general_metadata_open.get() { "true" } else { "false" }
                                    on:click=move |event| {
                                        event.stop_propagation();
                                        general_metadata_open.update(|open| *open = !*open);
                                    }
                                >
                                    <span style=icon_style(IconName::Edit, "1.2rem") aria-hidden="true"></span>
                                </button>
                            </header>
                            <p class=style::draft_note style:display=if is_draft { "" } else { "none" }>
                                <span style=icon_style(IconName::Info, "1rem") aria-hidden="true"></span>
                                <span>{tr(TextKey::MetadataDraftNotice)}</span>
                            </p>
                            <p class=style::empty style:display=move || if has_initial_error { "" } else { "none" }>
                                {tr(TextKey::AddonMediaListSaveFailed)}
                            </p>
                            <p class=style::empty style:display=move || if has_presentation_error { "" } else { "none" }>
                                {presentation_error}
                            </p>
                            <p class=style::review_error role="alert" style:display=move || if has_review_error { "" } else { "none" }>
                                <span style=icon_style(IconName::Warning, "1rem") aria-hidden="true"></span>
                                <span>{review_error}</span>
                            </p>
                            <input type="hidden" name="id" value=move || form_id.get().to_string()/>
                            <input type="hidden" name="addon_id" value=addon_id/>
                            <input type="hidden" name="source_url" value=source_url/>
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
                            <div class=style::editor_source_pair>
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
                                        <span style=move || icon_style(if auto_refresh.get() { IconName::Clock } else { IconName::RefreshOff }, "1rem") aria-hidden="true"></span>
                                        <span>{tr(TextKey::AutomaticRefresh)}</span>
                                    </span>
                                    <label class=style::compact_toggle>
                                        <input
                                            type="checkbox"
                                            name="auto_refresh"
                                            value="true"
                                            checked=checked_auto_refresh
                                            prop:checked=move || auto_refresh.get()
                                            on:change=move |event| auto_refresh.set(event_target_checked(&event))
                                        />
                                        <span>{tr(TextKey::UpdateAddonMediaListDaily)}</span>
                                    </label>
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
                                class=style::general_metadata_panel
                                data-testid="addon-general-metadata"
                                style:display=move || if general_metadata_open.get() { "grid" } else { "none" }
                            >
                            <MetadataConflictSummary conflicts=show_metadata_conflicts.clone()/>
                            <div class=style::field_row>
                                <div class=style::field_label>{tr(TextKey::Title)}</div>
                                <input class=style::text_input aria-label=tr(TextKey::Title) prop:value=move || show_title.get() on:input=move |event| show_title.set(event_target_value(&event))/>
                            </div>
                            <div class=style::field_row>
                                <div class=style::field_label>{tr(TextKey::OriginalTitle)}</div>
                                <input class=style::text_input aria-label=tr(TextKey::OriginalTitle) prop:value=move || show_original_title.get() on:input=move |event| show_original_title.set(event_target_value(&event))/>
                            </div>
                            <div class=style::field_row>
                                <div class=style::field_label>{tr(TextKey::SortTitle)}</div>
                                <input class=style::text_input aria-label=tr(TextKey::SortTitle) prop:value=move || show_sort_title.get() on:input=move |event| show_sort_title.set(event_target_value(&event))/>
                            </div>
                            <div class=style::field_row>
                                <div class=style::field_label>{tr(TextKey::Year)}</div>
                                <input class=style::text_input aria-label=tr(TextKey::Year) inputmode="numeric" prop:value=move || show_year.get() on:input=move |event| show_year.set(event_target_value(&event))/>
                            </div>
                            <div class=style::field_row>
                                <div class=style::field_label>{tr(TextKey::Summary)}</div>
                                <textarea class=style::boxed_textarea aria-label=tr(TextKey::Summary) rows="4" prop:value=move || show_plot.get() on:input=move |event| show_plot.set(event_target_value(&event))/>
                            </div>
                            <div class=style::field_row>
                                <div class=style::field_label>{tr(TextKey::Genres)}</div>
                                <input class=style::text_input aria-label=tr(TextKey::Genres) prop:value=move || show_genres.get() on:input=move |event| show_genres.set(event_target_value(&event))/>
                            </div>
                            <div class=format!("{} {}", style::field_row, style::general_tag_row)>
                                <div class=style::field_label>{tr(TextKey::Tags)}</div>
                                <div class=style::tag_field>
                                    <textarea class=style::tag_input aria-label=tr(TextKey::Tags) rows="4" spellcheck="false" prop:value=move || show_tags.get() on:input=move |event| show_tags.set(event_target_value(&event))/>
                                    <small>{move || format!("{} · {}", line_values(&show_tags.get()).len(), tr(TextKey::OneTagPerLine))}</small>
                                </div>
                            </div>
                            <div class=style::field_row>
                                <div class=style::field_label>{tr(TextKey::Studios)}</div>
                                <input class=style::text_input aria-label=tr(TextKey::Studios) prop:value=move || show_studios.get() on:input=move |event| show_studios.set(event_target_value(&event))/>
                            </div>
                            <div class=style::field_row>
                                <div class=style::field_label>{tr(TextKey::Directors)}</div>
                                <input class=style::text_input aria-label=tr(TextKey::Directors) prop:value=move || show_directors.get() on:input=move |event| show_directors.set(event_target_value(&event))/>
                            </div>
                            <div class=style::field_row>
                                <div class=style::field_label>{tr(TextKey::Writers)}</div>
                                <input class=style::text_input aria-label=tr(TextKey::Writers) prop:value=move || show_writers.get() on:input=move |event| show_writers.set(event_target_value(&event))/>
                            </div>
                            <div class=style::field_row>
                                <div class=style::field_label>{tr(TextKey::Artists)}</div>
                                <input class=style::text_input aria-label=tr(TextKey::Artists) prop:value=move || show_artists.get() on:input=move |event| show_artists.set(event_target_value(&event))/>
                            </div>
                            <div class=style::field_row>
                                <div class=style::field_label>{tr(TextKey::Languages)}</div>
                                <input class=style::text_input aria-label=tr(TextKey::Languages) prop:value=move || show_languages.get() on:input=move |event| show_languages.set(event_target_value(&event))/>
                            </div>
                            <div class=format!("{} {}", style::field_row, style::general_tag_row)>
                                <div class=style::field_label>{tr(TextKey::CastAndCredits)}</div>
                                <div>
                                    <textarea class=style::boxed_textarea aria-label=tr(TextKey::CastAndCredits) rows="3" spellcheck="false" prop:value=move || show_people.get() on:input=move |event| show_people.set(event_target_value(&event))/>
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
                            style:display=move || if presentation_kind.get() == AddonMediaListPresentationKind::TelevisionShow { "inline-flex" } else { "none" }
                            aria-expanded=move || if episode_numbering_open.get() { "true" } else { "false" }
                            aria-controls="episode-numbering-panel"
                            on:click=move |_| episode_numbering_open.update(|open| *open = !*open)
                        >
                            <span style=icon_style(IconName::Edit, "1rem") aria-hidden="true"></span>
                            <span>{tr(TextKey::NumberEpisodes)}</span>
                        </button>
                    </header>
                    <section
                        id="episode-numbering-panel"
                        class=style::numbering_panel
                        aria-label=tr(TextKey::NumberEpisodes)
                        style:display=move || if presentation_kind.get() == AddonMediaListPresentationKind::TelevisionShow && episode_numbering_open.get() { "grid" } else { "none" }
                    >
                        <p>{tr(TextKey::NumberEpisodesHelp)}</p>
                        <div class=style::numbering_fields>
                            <label class=style::metadata_field>
                                <span>{tr(TextKey::Season)}</span>
                                <input
                                    class=style::boxed_input
                                    type="number"
                                    min="0"
                                    step="1"
                                    inputmode="numeric"
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
                                    placeholder=tr(TextKey::AllRemaining)
                                    prop:value=move || episode_numbering_count.get()
                                    on:input=move |event| episode_numbering_count.set(event_target_value(&event))
                                />
                            </label>
                        </div>
                        <div class=style::numbering_footer>
                            <div>
                                <label class=style::checkbox_row>
                                    <input
                                        type="checkbox"
                                        prop:checked=move || replace_existing_episode_numbers.get()
                                        on:change=move |event| replace_existing_episode_numbers.set(event_target_checked(&event))
                                    />
                                    <small>{tr(TextKey::ReplaceExistingEpisodeNumbers)}</small>
                                </label>
                                <small class=style::numbering_preview aria-live="polite">
                                    {move || {
                                        match episode_numbering_preview.get() {
                                            Some((season, values)) if !values.is_empty() => {
                                                let first = values[0];
                                                let last = values[values.len() - 1];
                                                format!("{} {} · S{season:02}E{first:02}–S{season:02}E{last:02}", values.len(), tr(TextKey::Episodes))
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
                                    assign_episode_numbers_action.pending().get()
                                        || episode_numbering_assignment_count.get() == 0
                                        || !matches!(episode_numbering_season.get().trim().parse::<i32>(), Ok(value) if value >= 0)
                                        || !matches!(episode_numbering_start.get().trim().parse::<i32>(), Ok(value) if value >= 1)
                                }
                                on:click=move |_| {
                                    let season = episode_numbering_season.get_untracked().trim().parse::<i32>();
                                    let starting_episode = episode_numbering_start.get_untracked().trim().parse::<i32>();
                                    let item_count = episode_numbering_count.with_untracked(|value| {
                                        let value = value.trim();
                                        if value.is_empty() { Some(None) } else { value.parse::<u32>().ok().map(Some) }
                                    });
                                    if let (Ok(season), Ok(starting_episode), Some(item_count)) = (season, starting_episode, item_count) {
                                        assign_episode_numbers_action.dispatch((
                                            id,
                                            season,
                                            starting_episode,
                                            item_count,
                                            !replace_existing_episode_numbers.get_untracked(),
                                        ));
                                    }
                                }
                            >
                                <span>{tr(TextKey::Apply)}</span>
                            </button>
                        </div>
                    </section>
                    <section
                        class=style::liveness_bulk_toolbar
                        aria-label=tr(TextKey::PlaybackBehavior)
                        style:display=move || if remote_item_ids.with(Vec::is_empty) { "none" } else { "flex" }
                    >
                        <label>
                            <span>{tr(TextKey::PlaybackBehavior)}</span>
                            <select
                                class=style::page_size
                                prop:value=move || bulk_liveness_value.get()
                                on:change=move |event| bulk_liveness_value.set(event_target_value(&event))
                            >
                                <option value="provider">{tr(TextKey::UseProviderValue)}</option>
                                <option value="finite">{tr(TextKey::FiniteMedia)}</option>
                                <option value="live">{tr(TextKey::LiveSource)}</option>
                            </select>
                        </label>
                        <button
                            type="button"
                            class=style::numbering_toggle
                            disabled=move || selected_remote_count.get() == 0 || bulk_liveness_action.pending().get()
                            on:click=move |_| {
                                let remote_ids = remote_item_ids.get();
                                let item_ids = selected_items.with_untracked(|selected| {
                                    remote_ids
                                        .into_iter()
                                        .filter(|item_id| selected.contains(item_id))
                                        .collect::<Vec<_>>()
                                });
                                bulk_liveness_action.dispatch((
                                    id,
                                    liveness_override_from_value(&bulk_liveness_value.get_untracked()),
                                    item_ids,
                                ));
                            }
                        >
                            {move || format!("{} ({})", tr(TextKey::ApplyToSelected), selected_remote_count.get())}
                        </button>
                        <button
                            type="button"
                            class=style::numbering_toggle
                            disabled=move || remote_item_ids.with(Vec::is_empty) || bulk_liveness_action.pending().get()
                            on:click=move |_| {
                                bulk_liveness_action.dispatch((
                                    id,
                                    liveness_override_from_value(&bulk_liveness_value.get_untracked()),
                                    Vec::new(),
                                ));
                            }
                        >{tr(TextKey::ApplyToAllRemoteStreams)}</button>
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
                            <span>{move || format!("{} {}", need_review_count.get(), tr(TextKey::NeedReview))}</span>
                        </button>
                    </div>
                    <div class=style::table_wrap>
                        <table data-testid="addon-metadata-items">
                            <thead>
                                <tr>
                                    <th class=style::selection_cell>
                                        <input
                                            type="checkbox"
                                            aria-label=tr(TextKey::SelectItem)
                                            style:visibility=move || if remote_item_ids.with(Vec::is_empty) { "hidden" } else { "visible" }
                                            prop:checked=move || {
                                                let total = remote_item_ids.with(Vec::len);
                                                total > 0 && selected_remote_count.get() == total
                                            }
                                            on:change=move |event| {
                                                if event_target_checked(&event) {
                                                    selected_items.set(remote_item_ids.get().into_iter().collect());
                                                } else {
                                                    selected_items.set(BTreeSet::new());
                                                }
                                            }
                                        />
                                    </th>
                                    <th class=style::number_cell aria-label=tr(TextKey::Number)>
                                        <span aria-hidden="true">"#"</span>
                                    </th>
                                    <th>{tr(TextKey::IncludeItem)}</th>
                                    <th class=style::item_artwork_cell/>
                                    <th>{tr(TextKey::Name)}</th>
                                    <th>{tr(TextKey::FirstAired)}</th>
                                    <th>{tr(TextKey::SeasonEpisode)}</th>
                                    <th>{tr(TextKey::MediaKind)}</th>
                                    <th>{tr(TextKey::PlaybackBehavior)}</th>
                                    <th>{tr(TextKey::Status)}</th>
                                    <th/>
                                </tr>
                            </thead>
                            <tbody>
                                {move || {
                                    let (start, end, _, _, _) = item_page_range.get();
                                    let rows = items
                                        .get()
                                        .into_iter()
                                        .filter(|item| {
                                            !need_review_filter.get()
                                                || item_needs_review(item, presentation_kind.get())
                                        })
                                        .skip(start)
                                        .take(end.saturating_sub(start))
                                        .collect::<Vec<_>>();
                                    if rows.is_empty() {
                                        return view! {
                                            <tr><td class=style::empty_cell colspan="11">{tr(TextKey::NoItemsToDisplay)}</td></tr>
                                        }.into_any();
                                    }
                                    rows.into_iter().map(|item| {
                                        item_row_view(
                                            id,
                                            item,
                                            replacement_target,
                                            replacement_query,
                                            search_action,
                                            replacement_action,
                                            item_content_kind_action,
                                            item_inclusion_action,
                                            item_role_action,
                                            item_liveness_action,
                                            selected_items,
                                            episode_numbering_row_preview,
                                            presentation_kind,
                                            open_item,
                                            loaded_detail,
                                            item_detail_action,
                                            request_detail_action,
                                        )
                                    }).collect_view().into_any()
                                }}
                            </tbody>
                        </table>
                    </div>
                    <nav class=style::pager aria-label=tr(TextKey::RowsPerPage)>
                        <span>{format!("{}:", tr(TextKey::RowsPerPage))}</span>
                        <select
                            class=style::page_size
                            aria-label=tr(TextKey::RowsPerPage)
                            prop:value=move || item_page_size.get().to_string()
                            on:change=move |event| {
                                item_page.set(0);
                                item_page_size.set(event_target_value(&event).parse().unwrap_or(DEFAULT_ITEM_PAGE_SIZE));
                            }
                        >
                            {ITEM_PAGE_SIZES.into_iter().map(|size| view! {
                                <option value=size.to_string()>
                                    {if size == 0 { tr(TextKey::All).to_string() } else { size.to_string() }}
                                </option>
                            }).collect_view()}
                        </select>
                        <span class=style::pager_status role="status" aria-live="polite">
                            {move || {
                                let (start, end, total, _, _) = item_page_range.get();
                                range_of(
                                    if total == 0 { 0 } else { u32::try_from(start + 1).unwrap_or(u32::MAX) },
                                    u32::try_from(end).unwrap_or(u32::MAX),
                                    u64::try_from(total).unwrap_or(u64::MAX),
                                )
                            }}
                        </span>
                        <div class=style::pager_buttons>
                            <button
                                type="button"
                                class=style::icon_button
                                aria-label=tr(TextKey::PreviousPage)
                                disabled=move || item_page_range.get().3 == 0
                                on:click=move |_| item_page.update(|page| *page = page.saturating_sub(1))
                            >
                                <span style=icon_style(IconName::ChevronLeft, "1.25rem") aria-hidden="true"></span>
                            </button>
                            <button
                                type="button"
                                class=style::icon_button
                                aria-label=tr(TextKey::NextPage)
                                disabled=move || {
                                    let (_, _, _, page, total_pages) = item_page_range.get();
                                    page + 1 >= total_pages
                                }
                                on:click=move |_| item_page.update(|page| *page = page.saturating_add(1))
                            >
                                <span style=icon_style(IconName::ChevronRight, "1.25rem") aria-hidden="true"></span>
                            </button>
                        </div>
                    </nav>
                </section>
                <footer class=style::publication_footer>
                    <div>
                        <h3>{tr(TextKey::PublishMetadata)}</h3>
                        <p>{tr(TextKey::MetadataDraftNotice)}</p>
                    </div>
                    <button
                        type="button"
                        class=style::btn_add
                        disabled=move || {
                            publish_action.pending().get()
                                || import_progress.with(|progress| {
                                    progress.as_ref().is_some_and(|progress| {
                                        progress.state != AddonMediaListImportState::Ready
                                    })
                                })
                        }
                        on:click=move |_| {
                            publish_action.dispatch(id);
                        }
                    >
                        <span style=icon_style(IconName::Upload, "1.125rem") aria-hidden="true"></span>
                        <span>{tr(TextKey::PublishMetadata)}</span>
                    </button>
                </footer>
                <Modal
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
                season: Some(3),
                episode: Some(4),
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
            episode_numbering_preview_values(&items, 2, 3, 2, false),
            [5, 6]
        );
        assert_eq!(
            episode_numbering_preview_values(&items, 2, 3, 2, true),
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
            episode_numbering_preview_assignments(&items, 1, 1, 5, false),
            BTreeMap::from([(1, (1, 1)), (5, (1, 2))])
        );
        assert_eq!(
            episode_numbering_preview_assignments(&items, 1, 1, 2, true),
            BTreeMap::from([(1, (1, 1)), (4, (1, 2))])
        );
    }

    #[test]
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
    }

    #[test]
    fn progress_durations_are_stable_and_bounded() {
        assert_eq!(format_clock_duration(0), "00:00");
        assert_eq!(format_clock_duration(65), "01:05");
        assert_eq!(format_clock_duration(3_661), "01:01:01");
        assert!(elapsed_from("not-a-timestamp").is_none());
    }
}
