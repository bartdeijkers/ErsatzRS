//! Provider-neutral add-on media-list manager derived from ErsatzTV's
//! `Pages/TraktLists.razor`, `Pages/TraktListEditor.razor`, and
//! `Shared/AddTraktListDialog.razor`; ADR 0067 owns the deliberate boundary change.

use ersatzrs_domain::{
    AddonMediaListContentKind, AddonMediaListItemAvailability, AddonMediaListItemRole,
    AddonMediaListPresentationKind, AddonMediaListReviewState,
};
use leptos::prelude::*;
use leptos_router::hooks::{use_params_map, use_query_map};

use crate::admin::i18n::{
    addon_media_list_error_message, delete_failed, match_status, range_of, save_failed, tr, TextKey,
};
use crate::admin::server_fns::addon_media_lists::{
    add_addon_media_list, apply_addon_media_list_content_kind, change_addon_media_list_addon,
    delete_addon_media_list, get_addon_media_list_form, get_addon_media_list_item,
    get_addon_media_list_show_nfo, get_addon_media_lists_page_size, list_addon_media_list_items,
    list_addon_media_list_providers, list_addon_media_lists, match_addon_media_list,
    publish_addon_media_list, request_addon_media_list_presentation,
    resolve_addon_media_list_provider, save_addon_media_list_item_nfo,
    save_addon_media_list_show_nfo, search_addon_media_list_replacements,
    set_addon_media_list_item_content_kind, set_addon_media_list_item_included,
    set_addon_media_list_item_replacement, set_addon_media_list_item_role, update_addon_media_list,
    update_addon_media_list_item_chapters, AddonMediaListArtwork, AddonMediaListArtworkRole,
    AddonMediaListForm, AddonMediaListItemDetail, AddonMediaListItemRow, AddonMediaListNfoForm,
    AddonMediaListNfoMetadata, AddonMediaListNfoPerson, AddonMediaListProviderRow,
    AddonMediaListReplacementRow, AddonMediaListRow,
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
type ItemDetailAction = Action<i64, Result<Option<AddonMediaListItemDetail>, ServerFnError>>;
/// Searching local media that could replace one entry.
type ReplacementSearchAction =
    Action<String, Result<Vec<AddonMediaListReplacementRow>, ServerFnError>>;

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
    presentation: RwSignal<AddonMediaListPresentationKind>,
    open_item: RwSignal<Option<i64>>,
    loaded_detail: RwSignal<Option<AddonMediaListItemDetail>>,
    detail_action: ItemDetailAction,
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
    let base_needs_review = item.availability != AddonMediaListItemAvailability::Available
        || (item.replacement_media_version_id.is_some() && !item.replacement_valid)
        || item.title.trim().is_empty()
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
            class=style::metadata_item_row
            aria-expanded=move || if open_item.get() == Some(item_id) { "true" } else { "false" }
            on:click=move |_| toggle_editor()
        >
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
            <td colspan="6">
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

#[component]
fn InlineItemMetadataForm(
    list_id: i64,
    detail: AddonMediaListItemDetail,
    on_saved: Callback<i64>,
) -> impl IntoView {
    let item_id = detail.item.id;
    let original = detail.nfo.metadata.clone();
    let etag = detail.nfo.etag.clone();
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
        [thumbnail_url.get(), poster_url.get(), fanart_url.get()]
            .into_iter()
            .find(|value| !value.trim().is_empty())
            .unwrap_or_else(|| legacy_artwork.clone())
    });
    let toast = use_toast();
    let save_action = Action::new(move |metadata: &AddonMediaListNfoMetadata| {
        let metadata = metadata.clone();
        let etag = etag.clone();
        async move { save_addon_media_list_item_nfo(list_id, item_id, metadata, etag).await }
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
                    <label><span>{tr(TextKey::Poster)}</span><input class=style::boxed_input type="url" prop:value=move || poster_url.get() on:input=move |event| poster_url.set(event_target_value(&event))/></label>
                    <label><span>{tr(TextKey::FanArt)}</span><input class=style::boxed_input type="url" prop:value=move || fanart_url.get() on:input=move |event| fanart_url.set(event_target_value(&event))/></label>
                    <label><span>{tr(TextKey::Thumbnail)}</span><input class=style::boxed_input type="url" prop:value=move || thumbnail_url.get() on:input=move |event| thumbnail_url.set(event_target_value(&event))/></label>
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
        |value: &(String, String, String, AddonMediaListContentKind)| {
            let (addon_id, source_url, folder_name, content_kind_override) = value.clone();
            async move {
                add_addon_media_list(
                    addon_id,
                    source_url,
                    folder_name,
                    content_kind_override,
                    String::new(),
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
                                                    let row_for_delete = row.clone();
                                                    view! {
                                                        <tr>
                                                            <td>{format!("{addon_id} / {provider_id}")}</td>
                                                            <td>{name}</td>
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
                providers=providers
                on_submit=Callback::new(move |value: (String, String, String, AddonMediaListContentKind)| {
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
    providers: Vec<AddonMediaListProviderRow>,
    on_submit: Callback<(String, String, String, AddonMediaListContentKind)>,
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
                        // The add-on may be empty: the server resolves it
                        // from the URL, so the same rule applies to the plain
                        // HTML form post.
                        if !source_url.trim().is_empty() {
                            on_submit.run((addon_id, source_url, folder_name, content_kind));
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
        )| {
            let (addon_id, source_url, folder_name, presentation, auto_refresh, playlist) =
                value.clone();
            async move {
                let id = add_addon_media_list(
                    addon_id,
                    source_url,
                    folder_name,
                    presentation_content_kind(presentation),
                    String::new(),
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
                        <select class=style::boxed_input prop:value=move || addon_id.get() on:change=move |event| {
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
                            <select class=style::boxed_input prop:value=move || presentation_kind_value(presentation.get()) on:change=move |event| {
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
        || (item.replacement_media_version_id.is_some() && !item.replacement_valid)
        || item.title.trim().is_empty()
        || (presentation == AddonMediaListPresentationKind::TelevisionShow
            && (item.season.is_none() || item.episode.is_none()))
        || (presentation == AddonMediaListPresentationKind::Movies
            && item.collection_role == AddonMediaListItemRole::Auto)
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
    let content_kind_override = RwSignal::new(form.content_kind_override);
    let legacy_chapter_input = form.legacy_chapter_input.clone();
    let presentation_kind = RwSignal::new(form.requested_presentation_kind);
    let general_metadata_open = RwSignal::new(false);
    let is_draft = form.review_state != AddonMediaListReviewState::Published;
    let show_metadata = initial_show_nfo.metadata.clone();
    let show_etag = initial_show_nfo.etag.clone();
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
            items
                .iter()
                .filter(|item| {
                    !need_review_filter.get() || item_needs_review(item, presentation_kind.get())
                })
                .count()
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
    let open_item = RwSignal::new(None::<i64>);
    let loaded_detail = RwSignal::new(None::<AddonMediaListItemDetail>);
    let item_detail_action = Action::new(move |item_id: &i64| {
        let item_id = *item_id;
        async move { get_addon_media_list_item(id, item_id).await }
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
                        }, presentation_kind.get_untracked(), next_show_metadata, show_etag.clone()));
                    }
                >
                    <span style=icon_style(IconName::Save, "1.125rem") aria-hidden="true"></span>
                    <span>{tr(TextKey::SaveAddonMediaList)}</span>
                </button>
            </div>

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
                                style:display=move || if general_metadata_open.get() { "grid" } else { "none" }
                            >
                            <div class=style::field_row>
                                <div class=style::field_label>{tr(TextKey::Title)}</div>
                                <input class=style::text_input prop:value=move || show_title.get() on:input=move |event| show_title.set(event_target_value(&event))/>
                            </div>
                            <div class=style::field_row>
                                <div class=style::field_label>{tr(TextKey::OriginalTitle)}</div>
                                <input class=style::text_input prop:value=move || show_original_title.get() on:input=move |event| show_original_title.set(event_target_value(&event))/>
                            </div>
                            <div class=style::field_row>
                                <div class=style::field_label>{tr(TextKey::SortTitle)}</div>
                                <input class=style::text_input prop:value=move || show_sort_title.get() on:input=move |event| show_sort_title.set(event_target_value(&event))/>
                            </div>
                            <div class=style::field_row>
                                <div class=style::field_label>{tr(TextKey::Year)}</div>
                                <input class=style::text_input inputmode="numeric" prop:value=move || show_year.get() on:input=move |event| show_year.set(event_target_value(&event))/>
                            </div>
                            <div class=style::field_row>
                                <div class=style::field_label>{tr(TextKey::Summary)}</div>
                                <textarea class=style::boxed_textarea rows="4" prop:value=move || show_plot.get() on:input=move |event| show_plot.set(event_target_value(&event))/>
                            </div>
                            <div class=style::field_row>
                                <div class=style::field_label>{tr(TextKey::Genres)}</div>
                                <input class=style::text_input prop:value=move || show_genres.get() on:input=move |event| show_genres.set(event_target_value(&event))/>
                            </div>
                            <div class=format!("{} {}", style::field_row, style::general_tag_row)>
                                <div class=style::field_label>{tr(TextKey::Tags)}</div>
                                <div class=style::tag_field>
                                    <textarea class=style::tag_input rows="4" spellcheck="false" prop:value=move || show_tags.get() on:input=move |event| show_tags.set(event_target_value(&event))/>
                                    <small>{move || format!("{} · {}", line_values(&show_tags.get()).len(), tr(TextKey::OneTagPerLine))}</small>
                                </div>
                            </div>
                            <div class=style::field_row>
                                <div class=style::field_label>{tr(TextKey::Studios)}</div>
                                <input class=style::text_input prop:value=move || show_studios.get() on:input=move |event| show_studios.set(event_target_value(&event))/>
                            </div>
                            <div class=style::field_row>
                                <div class=style::field_label>{tr(TextKey::Directors)}</div>
                                <input class=style::text_input prop:value=move || show_directors.get() on:input=move |event| show_directors.set(event_target_value(&event))/>
                            </div>
                            <div class=style::field_row>
                                <div class=style::field_label>{tr(TextKey::Writers)}</div>
                                <input class=style::text_input prop:value=move || show_writers.get() on:input=move |event| show_writers.set(event_target_value(&event))/>
                            </div>
                            <div class=style::field_row>
                                <div class=style::field_label>{tr(TextKey::Artists)}</div>
                                <input class=style::text_input prop:value=move || show_artists.get() on:input=move |event| show_artists.set(event_target_value(&event))/>
                            </div>
                            <div class=style::field_row>
                                <div class=style::field_label>{tr(TextKey::Languages)}</div>
                                <input class=style::text_input prop:value=move || show_languages.get() on:input=move |event| show_languages.set(event_target_value(&event))/>
                            </div>
                            <div class=format!("{} {}", style::field_row, style::general_tag_row)>
                                <div class=style::field_label>{tr(TextKey::CastAndCredits)}</div>
                                <div>
                                    <textarea class=style::boxed_textarea rows="3" spellcheck="false" prop:value=move || show_people.get() on:input=move |event| show_people.set(event_target_value(&event))/>
                                    <small class=style::field_hint>{tr(TextKey::OneCreditPerLine)}</small>
                                </div>
                            </div>
                            <div class=format!("{} {}", style::field_row, style::general_artwork_row)>
                                <div class=style::field_label>{tr(TextKey::Artwork)}</div>
                                <div class=style::general_artwork_fields>
                                    <label><span>{tr(TextKey::Poster)}</span><input class=style::boxed_input type="url" prop:value=move || show_poster.get() on:input=move |event| show_poster.set(event_target_value(&event))/></label>
                                    <label><span>{tr(TextKey::FanArt)}</span><input class=style::boxed_input type="url" prop:value=move || show_fanart.get() on:input=move |event| show_fanart.set(event_target_value(&event))/></label>
                                    <label><span>{tr(TextKey::Thumbnail)}</span><input class=style::boxed_input type="url" prop:value=move || show_thumbnail.get() on:input=move |event| show_thumbnail.set(event_target_value(&event))/></label>
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
                    </header>
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
                                    <th>{tr(TextKey::IncludeItem)}</th>
                                    <th class=style::item_artwork_cell/>
                                    <th>{tr(TextKey::Name)}</th>
                                    <th>{tr(TextKey::MediaKind)}</th>
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
                                            <tr><td class=style::empty_cell colspan="6">{tr(TextKey::NoItemsToDisplay)}</td></tr>
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
                                            presentation_kind,
                                            open_item,
                                            loaded_detail,
                                            item_detail_action,
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
                        disabled=move || publish_action.pending().get()
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
}
