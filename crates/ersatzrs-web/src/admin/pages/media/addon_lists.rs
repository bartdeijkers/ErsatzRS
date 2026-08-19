//! Provider-neutral add-on media-list manager derived from ErsatzTV's
//! `Pages/TraktLists.razor`, `Pages/TraktListEditor.razor`, and
//! `Shared/AddTraktListDialog.razor`; ADR 0067 owns the deliberate boundary change.

use ersatzrs_domain::{
    AddonMediaListContentKind, AddonMediaListItemAvailability, AddonMediaListPresentationKind,
};
use leptos::prelude::*;
use leptos_router::hooks::{use_params_map, use_query_map};

use crate::admin::i18n::{
    addon_media_list_error_message, delete_failed, match_status, range_of, save_failed, tr, TextKey,
};
use crate::admin::server_fns::addon_media_lists::{
    add_addon_media_list, apply_addon_media_list_content_kind, change_addon_media_list_addon,
    delete_addon_media_list, get_addon_media_list_form, get_addon_media_list_item,
    get_addon_media_list_item_counts, get_addon_media_list_show_nfo,
    get_addon_media_lists_page_size, list_addon_media_list_items, list_addon_media_list_items_page,
    list_addon_media_list_providers, list_addon_media_lists, match_addon_media_list,
    request_addon_media_list_presentation, resolve_addon_media_list_provider,
    save_addon_media_list_item_nfo, save_addon_media_list_show_nfo,
    search_addon_media_list_replacements, set_addon_media_list_item_content_kind,
    set_addon_media_list_item_replacement, update_addon_media_list,
    update_addon_media_list_item_chapters, AddonMediaListForm, AddonMediaListItemCountsRow,
    AddonMediaListItemDetail, AddonMediaListItemRow, AddonMediaListNfoForm,
    AddonMediaListNfoMetadata, AddonMediaListProviderRow, AddonMediaListReplacementRow,
    AddonMediaListRow,
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

/// Paging and collapse state for one availability section of the item list.
///
/// Every field is a signal so one `Copy` value can be handed to the row
/// renderer without threading four arguments through it.
#[derive(Clone, Copy)]
struct ItemSectionState {
    page: RwSignal<u32>,
    /// Rows per page; zero means every row in the group.
    page_size: RwSignal<u32>,
    open: RwSignal<bool>,
    /// Rows currently rendered, seeded server-side so the section is not empty
    /// before hydration.
    rows: RwSignal<Vec<AddonMediaListItemRow>>,
}

impl ItemSectionState {
    fn new(open: bool, rows: Vec<AddonMediaListItemRow>) -> Self {
        Self {
            page: RwSignal::new(0),
            page_size: RwSignal::new(DEFAULT_ITEM_PAGE_SIZE),
            open: RwSignal::new(open),
            rows: RwSignal::new(rows),
        }
    }
}

/// Rows-per-page choices offered per section; zero renders as "all".
const ITEM_PAGE_SIZES: [u32; 5] = [10, 25, 50, 100, 0];
const DEFAULT_ITEM_PAGE_SIZE: u32 = 25;

/// Load one section's rows, refetching whenever its page or size changes.
///
/// The rows land in `state.rows` through an effect rather than being read from
/// the resource in the view: a local resource has no server-side value, and
/// reading one directly while the island renders disposes its reactive owner.
fn item_section_resource(
    id: i64,
    availability: AddonMediaListItemAvailability,
    state: ItemSectionState,
    toast: crate::admin::ui::primitives::toast::ToastStore,
) -> LocalResource<Vec<AddonMediaListItemRow>> {
    let resource = LocalResource::new(move || {
        let page = state.page.get();
        let page_size = state.page_size.get();
        async move {
            load_or_notify(
                list_addon_media_list_items_page(id, availability, page, page_size).await,
                toast,
            )
        }
    });
    Effect::new(move |_| {
        if let Some(rows) = resource.get() {
            state.rows.set(rows);
        }
    });
    resource
}

/// Stable test hook naming the availability group a section renders.
const fn availability_test_id(availability: AddonMediaListItemAvailability) -> &'static str {
    match availability {
        AddonMediaListItemAvailability::Unavailable => "non-playable-items",
        AddonMediaListItemAvailability::Unknown => "unknown-items",
        AddonMediaListItemAvailability::Available => "playable-items",
    }
}

/// Setting or clearing the local replacement of one entry.
type ReplacementAction = Action<(i64, Option<(i64, i64)>), Result<(), ServerFnError>>;
/// Setting the descriptive classification of one entry.
type ContentKindAction = Action<(i64, AddonMediaListContentKind), Result<(), ServerFnError>>;
/// Searching local media that could replace one entry.
type ReplacementSearchAction =
    Action<String, Result<Vec<AddonMediaListReplacementRow>, ServerFnError>>;

/// One item row, identical in all three sections.
///
/// Local replacement is offered everywhere: a playable entry can still be worth
/// replacing with a local copy.
#[allow(clippy::too_many_arguments)]
fn item_row_view(
    list_id: i64,
    item: AddonMediaListItemRow,
    replacement_target: RwSignal<Option<i64>>,
    replacement_query: RwSignal<String>,
    search_action: ReplacementSearchAction,
    replacement_action: ReplacementAction,
    content_kind_action: ContentKindAction,
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
    let availability_reason = item.availability_reason.unwrap_or_default();
    let has_reason = !availability_reason.is_empty();
    view! {
        <tr>
            <td>
                <a href=format!("/media/addon-lists/{list_id}/items/{item_id}")>{title}</a>
                <small
                    class=style::row_note
                    style:display=if has_reason { "" } else { "none" }
                >{availability_reason}</small>
            </td>
            <td>
                <select
                    class=style::text_input
                    aria-label=tr(TextKey::MediaKind)
                    prop:value=content_kind_value(content_kind)
                    on:change=move |event| {
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
            </td>
            <td
                data-replacement-state=replacement_state
                title=item.replacement_path.unwrap_or_default()
            >{replacement}</td>
            <td>
                <div class=style::row_actions>
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
                </div>
            </td>
        </tr>
    }
    .into_any()
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
                <button type="button" class=style::btn_add on:click=move |_| add_open.set(true)>
                    <span style=icon_style(IconName::Add, "1.125rem") aria-hidden="true"></span>
                    <span>{tr(TextKey::AddAddonMediaList)}</span>
                </button>
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
            let counts = get_addon_media_list_item_counts(id).await?;
            Ok::<_, ServerFnError>((form, items, show_nfo, providers, counts))
        },
    );

    view! {
        <Suspense fallback=admin_loading_resource_view>
            {move || {
                let error = error.clone();
                initial.get().map(move |result| admin_result_view(result, |(initial_form, initial_items, initial_show_nfo, providers, initial_counts)| {
                    view! {
                        <AddonMediaListEditorIsland
                            id=id
                            initial_error=error.clone()
                            initial_form=initial_form
                            initial_items=initial_items
                            initial_show_nfo=initial_show_nfo
                            providers=providers
                            initial_counts=initial_counts
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
    let tags = RwSignal::new(original.tags.join(", "));
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
        metadata.tags = comma_values(&tags.get_untracked());
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
                <label class=style::field_row><span class=style::field_label>{tr(TextKey::Tags)}</span><input class=style::text_input prop:value=move || tags.get() on:input=move |event| tags.set(event_target_value(&event))/></label>
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

#[island]
fn AddonMediaListEditorIsland(
    id: i64,
    initial_error: String,
    initial_form: Option<AddonMediaListForm>,
    initial_items: Vec<AddonMediaListItemRow>,
    initial_show_nfo: AddonMediaListNfoForm,
    providers: Vec<AddonMediaListProviderRow>,
    initial_counts: AddonMediaListItemCountsRow,
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
    let show_metadata = initial_show_nfo.metadata.clone();
    let show_etag = initial_show_nfo.etag.clone();
    let show_title = RwSignal::new(show_metadata.title.clone());
    let show_plot = RwSignal::new(show_metadata.plot.clone().unwrap_or_default());
    let presentation_error = form
        .presentation_error
        .as_deref()
        .map(addon_media_list_error_message)
        .unwrap_or_default();
    let has_presentation_error = !presentation_error.is_empty();
    let display_id = form.source_url.clone();
    let form_addon_id = form.addon_id.clone();
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
                if presentation_kind == AddonMediaListPresentationKind::TelevisionShow
                    && !show_metadata.title.trim().is_empty()
                {
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
    // One section per availability group. Each keeps its own page and page size
    // so paging the non-playable entries never moves the playable ones.
    let counts = RwSignal::new(initial_counts);
    let section_rows = |availability: AddonMediaListItemAvailability| {
        initial_items
            .iter()
            .filter(|item| item.availability == availability)
            .take(DEFAULT_ITEM_PAGE_SIZE as usize)
            .cloned()
            .collect::<Vec<_>>()
    };
    let unavailable = ItemSectionState::new(
        true,
        section_rows(AddonMediaListItemAvailability::Unavailable),
    );
    let unknown =
        ItemSectionState::new(false, section_rows(AddonMediaListItemAvailability::Unknown));
    let available = ItemSectionState::new(
        false,
        section_rows(AddonMediaListItemAvailability::Available),
    );
    let unavailable_rows = item_section_resource(
        id,
        AddonMediaListItemAvailability::Unavailable,
        unavailable,
        toast,
    );
    let unknown_rows =
        item_section_resource(id, AddonMediaListItemAvailability::Unknown, unknown, toast);
    let available_rows = item_section_resource(
        id,
        AddonMediaListItemAvailability::Available,
        available,
        toast,
    );
    // The picker names its target, which can sit in any of the three sections.
    let replacement_item_title = Memo::new(move |_| {
        let target = replacement_target.get()?;
        [unavailable, unknown, available]
            .into_iter()
            .find_map(|section| {
                section
                    .rows
                    .get()
                    .into_iter()
                    .find(|item| item.id == target)
                    .map(|item| item.display_title)
            })
    });
    let counts_action = Action::new(|id: &i64| get_addon_media_list_item_counts(*id));
    let item_content_kind_action = Action::new(|value: &(i64, AddonMediaListContentKind)| {
        let (item_id, content_kind) = *value;
        async move { set_addon_media_list_item_content_kind(item_id, content_kind).await }
    });
    let reload_sections = move || {
        unavailable_rows.refetch();
        unknown_rows.refetch();
        available_rows.refetch();
        counts_action.dispatch(id);
    };
    Effect::new(move |_| {
        if let Some(Ok(next)) = counts_action.value().get() {
            counts.set(next);
        }
    });
    Effect::new(move |_| {
        if let Some(result) = apply_content_kind_action.value().get() {
            match result {
                Ok(_) => {
                    toast.push(tr(TextKey::Saved).to_string(), ToastSeverity::Success);
                    reload_sections();
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
                    set_browser_location("/media/addon-lists");
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
                    reload_sections();
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
                    reload_sections();
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
                            next_show_metadata.plot = non_empty_value(&show_plot.get_untracked());
                            save_action.dispatch((AddonMediaListForm {
                                id: form_id.get_untracked(),
                                addon_id: submit_addon_id.clone(),
                                source_url: submit_source_url.clone(),
                                auto_refresh: auto_refresh.get_untracked(),
                                generate_playlist: generate_playlist.get_untracked(),
                                content_kind_override: content_kind_override.get_untracked(),
                                legacy_chapter_input: legacy_chapter_input.clone(),
                                presentation_kind: form.presentation_kind,
                                requested_presentation_kind: form.requested_presentation_kind,
                                presentation_error: form.presentation_error.clone(),
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
                                <h1 class=style::page_title>{tr(TextKey::AddonMediaList)}</h1>
                                <p class=style::empty style:display=move || if has_initial_error { "" } else { "none" }>
                                    {tr(TextKey::AddonMediaListSaveFailed)}
                                </p>
                                <p class=style::empty style:display=move || if has_presentation_error { "" } else { "none" }>
                                    {presentation_error}
                                </p>
                                <input type="hidden" name="id" value=move || form_id.get().to_string()/>
                                <input type="hidden" name="addon_id" value=addon_id/>
                                <input type="hidden" name="source_url" value=source_url/>
                                <div class=style::field_row>
                                    <div class=style::field_label>{tr(TextKey::AddonMediaListUrl)}</div>
                                    <input
                                        class=style::text_input
                                        type="text"
                                        value=display_id
                                        disabled
                                    />
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
                                <div class=style::field_row>
                                    <div class=style::field_label>{tr(TextKey::CollectionType)}</div>
                                    <select
                                        prop:value=move || presentation_kind_value(presentation_kind.get())
                                        on:change=move |event| presentation_kind.set(
                                            presentation_kind_from_value(&event_target_value(&event)),
                                        )
                                    >
                                        <option value="auto">{tr(TextKey::CollectionTypeAuto)}</option>
                                        <option value="television_show">{tr(TextKey::TvShows)}</option>
                                        <option value="movies">{tr(TextKey::Movies)}</option>
                                        <option value="music_videos">{tr(TextKey::Music)}</option>
                                        <option value="other_videos">{tr(TextKey::OtherVideos)}</option>
                                        <option value="songs">{tr(TextKey::Songs)}</option>
                                        <option value="images">{tr(TextKey::Images)}</option>
                                        <option value="remote_streams">{tr(TextKey::RemoteStreams)}</option>
                                    </select>
                                </div>
                                <div class=style::field_row style:display=move || if presentation_kind.get() == AddonMediaListPresentationKind::TelevisionShow { "" } else { "none" }>
                                    <div class=style::field_label>{tr(TextKey::Title)}</div>
                                    <input class=style::text_input prop:value=move || show_title.get() on:input=move |event| show_title.set(event_target_value(&event))/>
                                </div>
                                <div class=style::field_row style:display=move || if presentation_kind.get() == AddonMediaListPresentationKind::TelevisionShow { "" } else { "none" }>
                                    <div class=style::field_label>{tr(TextKey::Summary)}</div>
                                    <textarea class=style::text_input rows="6" prop:value=move || show_plot.get() on:input=move |event| show_plot.set(event_target_value(&event))/>
                                </div>
                                <div class=style::field_row>
                                    <div class=style::field_label>{tr(TextKey::MediaKind)}</div>
                                    <div class=style::addon_row>
                                    <select
                                        name="content_kind_override"
                                        prop:value=move || content_kind_value(content_kind_override.get())
                                        on:change=move |event| {
                                            content_kind_override.set(content_kind_from_value(&event_target_value(&event)));
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
                                        <button
                                            type="button"
                                            class=style::btn_cancel
                                            disabled=move || apply_content_kind_action.pending().get()
                                            on:click=move |_| apply_content_kind_open.set(true)
                                        >{tr(TextKey::ApplyToAllItems)}</button>
                                    </div>
                                </div>
                                <div class=style::field_row>
                                    <div class=style::field_label>{tr(TextKey::AutomaticRefresh)}</div>
                                    <label class=style::checkbox_row>
                                        <input
                                            type="checkbox"
                                            name="auto_refresh"
                                            value="true"
                                            checked=checked_auto_refresh
                                            prop:checked=move || auto_refresh.get()
                                            on:change=move |event| {
                                                auto_refresh.set(event_target_checked(&event));
                                            }
                                        />
                                        <small>{tr(TextKey::UpdateAddonMediaListDaily)}</small>
                                    </label>
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
    {[
                        (AddonMediaListItemAvailability::Unavailable, TextKey::NonPlayableItems, unavailable),
                        (AddonMediaListItemAvailability::Unknown, TextKey::UnknownItems, unknown),
                        (AddonMediaListItemAvailability::Available, TextKey::PlayableItems, available),
                    ].into_iter().map(|(availability, label, state)| {
                        let total = move || match availability {
                            AddonMediaListItemAvailability::Unavailable => counts.get().unavailable,
                            AddonMediaListItemAvailability::Unknown => counts.get().unknown,
                            AddonMediaListItemAvailability::Available => counts.get().available,
                        };
                        view! {
                            <section class=style::item_section data-testid=availability_test_id(availability)>
                                <button
                                    type="button"
                                    class=style::section_toggle
                                    aria-expanded=move || if state.open.get() { "true" } else { "false" }
                                    on:click=move |_| state.open.update(|open| *open = !*open)
                                >
                                    <span
                                        style=move || icon_style(
                                            if state.open.get() { IconName::ChevronDown } else { IconName::ChevronRight },
                                            "1.25rem",
                                        )
                                        aria-hidden="true"
                                    ></span>
                                    <h2>{move || format!("{} ({})", tr(label), total())}</h2>
                                </button>
                                <div style:display=move || if state.open.get() { "" } else { "none" }>
                                    <div
                                        class=style::table_wrap
                                        style:display=move || if total() == 0 { "none" } else { "" }
                                    >
                                        <table>
                                            <thead>
                                                <tr>
                                                    <th>{tr(TextKey::Name)}</th>
                                                    <th>{tr(TextKey::MediaKind)}</th>
                                                    <th>{tr(TextKey::LocalReplacement)}</th>
                                                    <th/>
                                                </tr>
                                            </thead>
                                            <tbody>
                                                {move || state.rows.get().into_iter().map(|item| {
                                                    item_row_view(
                                                        id,
                                                        item,
                                                        replacement_target,
                                                        replacement_query,
                                                        search_action,
                                                        replacement_action,
                                                        item_content_kind_action,
                                                    )
                                                }).collect_view()}
                                            </tbody>
                                        </table>
                                    </div>
                                    <p
                                        class=style::empty
                                        style:display=move || if total() == 0 { "" } else { "none" }
                                    >{tr(TextKey::NoItemsToDisplay)}</p>
                                    <div
                                        class=style::pager
                                        style:display=move || if total() == 0 { "none" } else { "" }
                                    >
                                        <span class=style::pager_status>{tr(TextKey::RowsPerPage)}</span>
                                        <select
                                            class=style::page_size
                                            aria-label=tr(TextKey::RowsPerPage)
                                            prop:value=move || state.page_size.get().to_string()
                                            on:change=move |event| {
                                                state.page.set(0);
                                                state.page_size.set(
                                                    event_target_value(&event).parse().unwrap_or(DEFAULT_ITEM_PAGE_SIZE),
                                                );
                                            }
                                        >
                                            {ITEM_PAGE_SIZES.into_iter().map(|size| view! {
                                                <option value=size.to_string()>
                                                    {if size == 0 { tr(TextKey::All).to_string() } else { size.to_string() }}
                                                </option>
                                            }).collect_view()}
                                        </select>
                                        <div class=style::pager_buttons>
                                            <button
                                                type="button"
                                                class=style::icon_button
                                                aria-label=tr(TextKey::PreviousPage)
                                                disabled=move || state.page.get() == 0
                                                on:click=move |_| state.page.update(|page| *page = page.saturating_sub(1))
                                            >
                                                <span style=icon_style(IconName::ChevronLeft, "1.25rem") aria-hidden="true"></span>
                                            </button>
                                            <button
                                                type="button"
                                                class=style::icon_button
                                                aria-label=tr(TextKey::NextPage)
                                                disabled=move || {
                                                    let size = state.page_size.get();
                                                    size == 0
                                                        || u64::from(state.page.get() + 1) * u64::from(size) >= total()
                                                }
                                                on:click=move |_| state.page.update(|page| *page += 1)
                                            >
                                                <span style=icon_style(IconName::ChevronRight, "1.25rem") aria-hidden="true"></span>
                                            </button>
                                        </div>
                                    </div>
                                </div>
                            </section>
                        }
                    }).collect_view()}
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

    /// The three sections are told apart only by this hook, in the browser
    /// specs and in the DOM. Two groups sharing one id would make a spec assert
    /// against the wrong section without failing.
    #[test]
    fn every_availability_group_has_its_own_test_hook() {
        let ids = [
            availability_test_id(AddonMediaListItemAvailability::Unavailable),
            availability_test_id(AddonMediaListItemAvailability::Unknown),
            availability_test_id(AddonMediaListItemAvailability::Available),
        ];
        let unique = ids.iter().collect::<std::collections::BTreeSet<_>>();
        assert_eq!(unique.len(), ids.len());
    }

    /// Zero is the "all rows" sentinel the repository turns into an unbounded
    /// limit; losing it from the offered sizes would quietly cap long lists.
    #[test]
    fn item_page_sizes_offer_an_unbounded_choice_and_the_default() {
        assert!(ITEM_PAGE_SIZES.contains(&0));
        assert!(ITEM_PAGE_SIZES.contains(&DEFAULT_ITEM_PAGE_SIZE));
    }
}
