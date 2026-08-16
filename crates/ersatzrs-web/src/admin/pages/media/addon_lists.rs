//! Provider-neutral add-on media-list manager derived from ErsatzTV's
//! `Pages/TraktLists.razor`, `Pages/TraktListEditor.razor`, and
//! `Shared/AddTraktListDialog.razor`; ADR 0067 owns the deliberate boundary change.

use ersatzrs_domain::AddonMediaListContentKind;
use leptos::prelude::*;
use leptos_router::hooks::{use_params_map, use_query_map};

use crate::admin::i18n::{
    addon_media_list_error_message, delete_failed, match_status, range_of, save_failed, tr, TextKey,
};
use crate::admin::server_fns::addon_media_lists::{
    add_addon_media_list, delete_addon_media_list, get_addon_media_list_form,
    get_addon_media_lists_page_size, list_addon_media_list_items, list_addon_media_list_providers,
    list_addon_media_lists, match_addon_media_list, search_addon_media_list_replacements,
    set_addon_media_list_item_replacement, update_addon_media_list, AddonMediaListForm,
    AddonMediaListItemRow, AddonMediaListProviderRow, AddonMediaListReplacementRow,
    AddonMediaListRow,
};
use crate::admin::ui::primitives::{
    dialog::DeleteDialog,
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
    let add_addon_id = RwSignal::new(
        providers
            .first()
            .map(|provider| provider.addon_id.clone())
            .unwrap_or_default(),
    );
    let add_source_url = RwSignal::new(String::new());
    let add_folder_name = RwSignal::new(String::new());
    let add_chapter_input = RwSignal::new(String::new());
    let add_content_kind = RwSignal::new(AddonMediaListContentKind::Auto);
    let add_open = RwSignal::new(false);
    let has_initial_error = !initial_error.is_empty();
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
        |value: &(String, String, String, AddonMediaListContentKind, String)| {
            let (addon_id, source_url, folder_name, content_kind_override, chapter_input) =
                value.clone();
            async move {
                add_addon_media_list(
                    addon_id,
                    source_url,
                    folder_name,
                    content_kind_override,
                    chapter_input,
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
                    add_chapter_input.set(String::new());
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
                    {tr(TextKey::AddonMediaListActionFailed)}
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
                source_url=add_source_url
                folder_name=add_folder_name
                chapter_input=add_chapter_input
                content_kind=add_content_kind
                providers=providers
                on_submit=Callback::new(move |value: (String, String, String, AddonMediaListContentKind, String)| {
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
    source_url: RwSignal<String>,
    folder_name: RwSignal<String>,
    chapter_input: RwSignal<String>,
    content_kind: RwSignal<AddonMediaListContentKind>,
    providers: Vec<AddonMediaListProviderRow>,
    on_submit: Callback<(String, String, String, AddonMediaListContentKind, String)>,
) -> impl IntoView {
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
                        let chapter_input = chapter_input.get_untracked();
                        if !addon_id.trim().is_empty() && !source_url.trim().is_empty() {
                            on_submit.run((addon_id, source_url, folder_name, content_kind, chapter_input));
                        }
                    }
                >
                    <div class=style::dialog_body>
                        <p>{tr(TextKey::EnterAddonMediaListUrl)}</p>
                        <label class=style::dialog_field>
                            <span>{tr(TextKey::Addon)}</span>
                            <select
                                name="addon_id"
                                prop:value=move || addon_id.get()
                                on:change=move |event| addon_id.set(event_target_value(&event))
                            >
                                {providers.into_iter().map(|provider| view! {
                                    <option value=provider.addon_id>{provider.name}</option>
                                }).collect_view()}
                            </select>
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
                                <option value="television_episode">{tr(TextKey::TelevisionShow)}</option>
                                <option value="movie">{tr(TextKey::Movie)}</option>
                                <option value="music_video">{tr(TextKey::MusicVideo)}</option>
                                <option value="other_video">{tr(TextKey::OtherVideo)}</option>
                            </select>
                        </label>
                        <label class=style::dialog_field>
                            <span>{tr(TextKey::AddonMediaListUrl)}</span>
                            <input
                                type="text"
                                name="source_url"
                                placeholder="https://example.invalid/lists/list-name"
                                prop:value=move || source_url.get()
                                on:input=move |event| source_url.set(event_target_value(&event))
                            />
                        </label>
                        <label class=style::dialog_field>
                            <span>{tr(TextKey::ChapterInput)}</span>
                            <textarea
                                name="chapter_input"
                                rows="6"
                                placeholder=tr(TextKey::ChapterInputHelp)
                                prop:value=move || chapter_input.get()
                                on:input=move |event| chapter_input.set(event_target_value(&event))
                            />
                            <small>{tr(TextKey::ChapterInputHelp)}</small>
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
                            disabled=move || {
                                addon_id.with(|value| value.trim().is_empty())
                                    || source_url.with(|value| value.trim().is_empty())
                            }
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
            Ok::<_, ServerFnError>((form, items))
        },
    );

    view! {
        <Suspense fallback=admin_loading_resource_view>
            {move || {
                let error = error.clone();
                initial.get().map(move |result| admin_result_view(result, |(initial_form, initial_items)| {
                    view! {
                        <AddonMediaListEditorIsland
                            id=id
                            initial_error=error.clone()
                            initial_form=initial_form
                            initial_items=initial_items
                        />
                    }
                    .into_any()
                }))
            }}
        </Suspense>
    }
}

#[island]
fn AddonMediaListEditorIsland(
    id: i64,
    initial_error: String,
    initial_form: Option<AddonMediaListForm>,
    initial_items: Vec<AddonMediaListItemRow>,
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
    let chapter_input = RwSignal::new(form.chapter_input.clone());
    let display_id = format!("{} / {}", form.addon_id, form.source_url);
    let checked_auto_refresh = form.auto_refresh;
    let checked_generate_playlist = form.generate_playlist;
    let items = RwSignal::new(initial_items);
    let replacement_target = RwSignal::new(None::<i64>);
    let replacement_query = RwSignal::new(String::new());
    let replacement_results = RwSignal::new(Vec::<AddonMediaListReplacementRow>::new());

    let save_action = Action::new(|form: &AddonMediaListForm| {
        let form = form.clone();
        async move { update_addon_media_list(form).await }
    });
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
    let refresh_items_action = Action::new(move |(): &()| list_addon_media_list_items(id));

    let toast = use_toast();
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
                    refresh_items_action.dispatch(());
                }
                Err(error) => toast.push(
                    save_failed(addon_media_list_error_message(error)),
                    ToastSeverity::Error,
                ),
            }
        }
    });
    Effect::new(move |_| {
        if let Some(result) = refresh_items_action.value().get() {
            match result {
                Ok(rows) => items.set(rows),
                Err(error) => {
                    toast.push(addon_media_list_error_message(error), ToastSeverity::Error);
                }
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
                        save_action.dispatch(AddonMediaListForm {
                            id: form_id.get_untracked(),
                            addon_id: submit_addon_id.clone(),
                            source_url: submit_source_url.clone(),
                            auto_refresh: auto_refresh.get_untracked(),
                            generate_playlist: generate_playlist.get_untracked(),
                            content_kind_override: content_kind_override.get_untracked(),
                            chapter_input: chapter_input.get_untracked(),
                        });
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
                            <input type="hidden" name="id" value=move || form_id.get().to_string()/>
                            <input type="hidden" name="addon_id" value=addon_id/>
                            <input type="hidden" name="source_url" value=source_url/>
                            <div class=style::field_row>
                                <div class=style::field_label>{tr(TextKey::MediaKind)}</div>
                                <select
                                    name="content_kind_override"
                                    prop:value=move || content_kind_value(content_kind_override.get())
                                    on:change=move |event| {
                                        content_kind_override.set(content_kind_from_value(&event_target_value(&event)));
                                    }
                                >
                                    <option value="auto">{tr(TextKey::LeaveEmptyToAutoDetect)}</option>
                                    <option value="television_episode">{tr(TextKey::TelevisionShow)}</option>
                                    <option value="movie">{tr(TextKey::Movie)}</option>
                                    <option value="music_video">{tr(TextKey::MusicVideo)}</option>
                                    <option value="other_video">{tr(TextKey::OtherVideo)}</option>
                                </select>
                            </div>
                            <div class=style::field_row>
                                <div class=style::field_label>{tr(TextKey::ChapterInput)}</div>
                                <div>
                                    <textarea
                                        class=style::text_input
                                        name="chapter_input"
                                        rows="8"
                                        placeholder=tr(TextKey::ChapterInputHelp)
                                        prop:value=move || chapter_input.get()
                                        on:input=move |event| chapter_input.set(event_target_value(&event))
                                    />
                                    <small>{tr(TextKey::ChapterInputHelp)}</small>
                                </div>
                            </div>
                            <div class=style::field_row>
                                <div class=style::field_label>{tr(TextKey::Id)}</div>
                                <input
                                    class=style::text_input
                                    type="text"
                                    value=display_id
                                    disabled
                                />
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
                <h2>{tr(TextKey::LocalReplacement)}</h2>
                <div class=style::table_wrap>
                    <table>
                        <thead>
                            <tr>
                                <th>{tr(TextKey::Name)}</th>
                                <th>{tr(TextKey::Status)}</th>
                                <th>{tr(TextKey::LocalReplacement)}</th>
                                <th/>
                            </tr>
                        </thead>
                        <tbody>
                            {move || items.get().into_iter().map(|item| {
                                let item_id = item.id;
                                let title = item.display_title.clone();
                                let status = match item.availability {
                                    ersatzrs_domain::AddonMediaListItemAvailability::Available => tr(TextKey::Available).to_string(),
                                    ersatzrs_domain::AddonMediaListItemAvailability::Unavailable => tr(TextKey::Unavailable).to_string(),
                                    ersatzrs_domain::AddonMediaListItemAvailability::Unknown => tr(TextKey::UnknownAvailability).to_string(),
                                };
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
                                    item.replacement_title.clone().unwrap_or_else(|| tr(TextKey::NoReplacement).to_string())
                                };
                                view! {
                                    <tr>
                                        <td>{title.clone()}</td>
                                        <td title=item.availability_reason.unwrap_or_default()>{status}</td>
                                        <td
                                            data-replacement-state=replacement_state
                                            title=item.replacement_path.unwrap_or_default()
                                        >{replacement}</td>
                                        <td>
                                            <div class=style::row_actions>
                                                <button type="button" class=style::icon_button on:click=move |_| {
                                                    replacement_target.set(Some(item_id));
                                                    replacement_query.set(title.clone());
                                                    search_action.dispatch(title.clone());
                                                }>{tr(TextKey::Replace)}</button>
                                                <button type="button" class=style::icon_button disabled=!has_replacement on:click=move |_| {
                                                    replacement_action.dispatch((item_id, None));
                                                }>{tr(TextKey::Clear)}</button>
                                            </div>
                                        </td>
                                    </tr>
                                }
                            }).collect_view()}
                        </tbody>
                    </table>
                </div>
                <div class=style::editor style:display=move || if replacement_target.get().is_some() { "" } else { "none" }>
                    <div class=style::field_row>
                        <input class=style::text_input type="search" prop:value=move || replacement_query.get() on:input=move |event| replacement_query.set(event_target_value(&event))/>
                        <button type="button" class=style::btn_add on:click=move |_| {
                            search_action.dispatch(replacement_query.get_untracked());
                        }>{tr(TextKey::Search)}</button>
                    </div>
                    <div class=style::table_wrap>
                        <table>
                            <tbody>
                                {move || replacement_results.get().into_iter().map(|candidate| {
                                    let target = replacement_target.get_untracked();
                                    view! {
                                        <tr>
                                            <td>{candidate.title}</td>
                                            <td>{candidate.path}</td>
                                            <td><button type="button" class=style::btn_add on:click=move |_| {
                                                if let Some(item_id) = target {
                                                    replacement_action.dispatch((item_id, Some((candidate.media_item_id, candidate.media_version_id))));
                                                }
                                            }>{tr(TextKey::Replace)}</button></td>
                                        </tr>
                                    }
                                }).collect_view()}
                            </tbody>
                        </table>
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
