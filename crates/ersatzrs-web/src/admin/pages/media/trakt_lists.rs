//! Trakt list list and editor — ports `Pages/TraktLists.razor`,
//! `Pages/TraktListEditor.razor`, and `Shared/AddTraktListDialog.razor`.
#![allow(clippy::too_many_lines, dead_code)]

use leptos::prelude::*;
use leptos_router::hooks::{use_params_map, use_query_map};

use crate::admin::i18n::{delete_failed, match_status, range_of, save_failed, tr, TextKey};
use crate::admin::server_fns::trakt::{
    add_trakt_list, delete_trakt_list, get_trakt_list_form, get_trakt_lists_page_size,
    list_trakt_lists, match_trakt_list, update_trakt_list, TraktListForm, TraktListRow,
};
use crate::admin::ui::primitives::{
    dialog::DeleteDialog,
    icon::{icon_style, IconName},
    toast::{use_toast, ToastSeverity},
};

stylance::import_crate_style!(style, "src/admin/pages/media/trakt_lists.module.css");

#[cfg(target_arch = "wasm32")]
fn set_browser_location(path: &str) {
    if let Some(window) = web_sys::window() {
        let _ = window.location().set_href(path);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn set_browser_location(_path: &str) {}

#[component]
pub fn TraktListsPage() -> impl IntoView {
    let query = use_query_map();
    let error = query.with_untracked(|params| params.get("error").unwrap_or_default());
    view! { <TraktListsIsland initial_error=error/> }
}

#[island]
fn TraktListsIsland(initial_error: String) -> impl IntoView {
    let page = RwSignal::new(0u32);
    let page_size = RwSignal::new(10u32);
    let add_value = RwSignal::new(String::new());
    let add_open = RwSignal::new(false);
    let has_initial_error = !initial_error.is_empty();

    let saved_page_size = Resource::new_blocking(
        || (),
        |()| async { get_trakt_lists_page_size().await.unwrap_or(10) },
    );
    Effect::new(move |_| {
        if let Some(saved) = saved_page_size.get() {
            page_size.set(saved);
        }
    });

    let data: Resource<(Vec<TraktListRow>, u64)> = Resource::new_blocking(
        move || (page.get(), page_size.get()),
        move |(p, size)| async move { list_trakt_lists(p, size).await.unwrap_or_default() },
    );

    let add_action = Action::new(|value: &String| {
        let value = value.clone();
        async move { add_trakt_list(value).await }
    });
    let match_action = Action::new(|id: &i64| match_trakt_list(*id));
    let delete_action = Action::new(|id: &i64| delete_trakt_list(*id));

    let delete_target: RwSignal<Option<TraktListRow>> = RwSignal::new(None);
    let delete_open = RwSignal::new(false);
    let toast = use_toast();

    Effect::new(move |_| {
        if let Some(result) = add_action.value().get() {
            match result {
                Ok(_) => {
                    toast.push(
                        tr(TextKey::TraktListAdded).to_string(),
                        ToastSeverity::Success,
                    );
                    add_value.set(String::new());
                    add_open.set(false);
                    data.refetch();
                }
                Err(error) => toast.push(
                    format!("{}: {error}", tr(TextKey::AddFailed)),
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
                        tr(TextKey::TraktListMatched).to_string(),
                        ToastSeverity::Success,
                    );
                    data.refetch();
                }
                Err(error) => toast.push(
                    format!("{}: {error}", tr(TextKey::MatchFailed)),
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
                        tr(TextKey::TraktListDeleted).to_string(),
                        ToastSeverity::Success,
                    );
                    data.refetch();
                }
                Err(error) => toast.push(delete_failed(error), ToastSeverity::Error),
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
                    <span>{tr(TextKey::AddTraktList)}</span>
                </button>
            </div>
            <section class=style::content>
                <h1 class=style::page_title>{tr(TextKey::TraktLists)}</h1>
                <div class=style::divider/>
                <p class=style::empty style:display=move || if has_initial_error { "" } else { "none" }>
                    {tr(TextKey::TraktListActionFailed)}
                </p>

                <Suspense fallback=move || view! { <p class=style::empty>{tr(TextKey::Loading)}</p> }>
                    {move || {
                        data.get().map(|(rows, total)| {
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
                                                        <td class=style::empty_cell colspan="4">{tr(TextKey::NoTraktListsYet)}</td>
                                                    </tr>
                                                }.into_any()
                                            } else {
                                                rows.into_iter().map(|row| {
                                                    let id = row.id;
                                                    let slug = row.slug.clone();
                                                    let name = row.name.clone();
                                                    let trakt_id = row.trakt_id;
                                                    let match_status = match_status(
                                                        u64::try_from(row.match_count).unwrap_or_default(),
                                                        u64::try_from(row.item_count).unwrap_or_default(),
                                                    );
                                                    let row_for_delete = row.clone();
                                                    view! {
                                                        <tr>
                                                            <td>{slug}</td>
                                                            <td>{name}</td>
                                                            <td>{match_status}</td>
                                                            <td>
                                                                <div class=style::row_actions>
                                                                    <a
                                                                        class=style::icon_button
                                                                        href=format!("/media/trakt/lists/{id}")
                                                                        title=tr(TextKey::EditTraktListProperties)
                                                                        aria-label=tr(TextKey::EditTraktListProperties)
                                                                    >
                                                                        <span style=icon_style(IconName::Edit, "1.25rem") aria-hidden="true"></span>
                                                                    </a>
                                                                    <a
                                                                        class=style::icon_button
                                                                        href=format!("/search?query=trakt_list%3a{trakt_id}")
                                                                        title=tr(TextKey::SearchTraktList)
                                                                        aria-label=tr(TextKey::SearchTraktList)
                                                                    >
                                                                        <span style=icon_style(IconName::Search, "1.25rem") aria-hidden="true"></span>
                                                                    </a>
                                                                    <form method="post" action="/api/trakt-lists/match">
                                                                        <input type="hidden" name="id" value=id.to_string()/>
                                                                        <button
                                                                            type="submit"
                                                                            class=style::icon_button
                                                                            title=tr(TextKey::MatchTraktListItems)
                                                                            aria-label=tr(TextKey::MatchTraktListItems)
                                                                            on:click=move |event| {
                                                                                event.prevent_default();
                                                                                match_action.dispatch(id);
                                                                            }
                                                                        >
                                                                            <span style=icon_style(IconName::Refresh, "1.25rem") aria-hidden="true"></span>
                                                                        </button>
                                                                    </form>
                                                                    <form method="post" action="/api/trakt-lists/delete">
                                                                        <input type="hidden" name="id" value=id.to_string()/>
                                                                        <button
                                                                            type="submit"
                                                                            class=style::icon_button
                                                                            title=tr(TextKey::DeleteTraktList)
                                                                            aria-label=tr(TextKey::DeleteTraktList)
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
                        })
                    }}
                </Suspense>
            </section>

            <AddTraktListDialog
                open=add_open
                value=add_value
                on_submit=Callback::new(move |value: String| {
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
fn AddTraktListDialog(
    open: RwSignal<bool>,
    value: RwSignal<String>,
    on_submit: Callback<String>,
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
                <h2 class=style::dialog_title>{tr(TextKey::AddTraktList)}</h2>
                <form
                    method="post"
                    action="/api/trakt-lists/add"
                    on:submit=move |event| {
                        event.prevent_default();
                        let input = value.get_untracked();
                        if !input.trim().is_empty() {
                            on_submit.run(input);
                        }
                    }
                >
                    <div class=style::dialog_body>
                        <p>{tr(TextKey::EnterTraktListUrl)}</p>
                        <label class=style::dialog_field>
                            <span>{tr(TextKey::TraktListUrl)}</span>
                            <input
                                type="text"
                                name="url"
                                placeholder="https://trakt.tv/users/username/lists/list-name"
                                prop:value=move || value.get()
                                on:input=move |event| value.set(event_target_value(&event))
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
                            disabled=move || value.with(|value| value.trim().is_empty())
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
pub fn TraktListEditorPage() -> impl IntoView {
    let params = use_params_map();
    let query = use_query_map();
    let id = params.with(|p| {
        p.get("id")
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(0)
    });
    let error = query.with_untracked(|params| params.get("error").unwrap_or_default());
    view! { <TraktListEditorIsland id initial_error=error/> }
}

#[island]
fn TraktListEditorIsland(id: i64, initial_error: String) -> impl IntoView {
    let has_initial_error = !initial_error.is_empty();
    let initial: Resource<Option<TraktListForm>> = Resource::new_blocking(
        move || id,
        |id| async move { get_trakt_list_form(id).await.unwrap_or_default() },
    );

    let form_id = RwSignal::new(id);
    let slug = RwSignal::new(String::new());
    let auto_refresh = RwSignal::new(false);
    let generate_playlist = RwSignal::new(false);

    Effect::new(move |_| {
        if let Some(Some(form)) = initial.get() {
            form_id.set(form.id);
            slug.set(form.slug);
            auto_refresh.set(form.auto_refresh);
            generate_playlist.set(form.generate_playlist);
        }
    });

    let save_action = Action::new(|form: &TraktListForm| {
        let form = form.clone();
        async move { update_trakt_list(form).await }
    });

    let toast = use_toast();
    Effect::new(move |_| {
        if let Some(result) = save_action.value().get() {
            match result {
                Ok(()) => {
                    toast.push(
                        tr(TextKey::TraktListSaved).to_string(),
                        ToastSeverity::Success,
                    );
                    set_browser_location("/media/trakt/lists");
                }
                Err(error) => toast.push(save_failed(error), ToastSeverity::Error),
            }
        }
    });

    view! {
        <div class=style::page>
            <div class=style::action_bar>
                <button
                    form="trakt-list-editor-form"
                    type="submit"
                    class=style::btn_add
                    on:click=move |event| {
                        event.prevent_default();
                        save_action.dispatch(TraktListForm {
                            id: form_id.get_untracked(),
                            slug: slug.get_untracked(),
                            auto_refresh: auto_refresh.get_untracked(),
                            generate_playlist: generate_playlist.get_untracked(),
                        });
                    }
                >
                    <span style=icon_style(IconName::Save, "1.125rem") aria-hidden="true"></span>
                    <span>{tr(TextKey::SaveTraktList)}</span>
                </button>
            </div>

            <Suspense fallback=move || view! { <p class=style::empty>{tr(TextKey::Loading)}</p> }>
                {move || match initial.get() {
                    Some(Some(form)) => view! {
                        <form
                            id="trakt-list-editor-form"
                            method="post"
                            action="/api/trakt-lists/save"
                            class=style::editor
                        >
                            <h1 class=style::page_title>{tr(TextKey::TraktList)}</h1>
                            <p class=style::empty style:display=move || if has_initial_error { "" } else { "none" }>
                                {tr(TextKey::TraktListSaveFailed)}
                            </p>
                            <input type="hidden" name="id" value=form.id.to_string()/>
                            <input type="hidden" name="slug" value=form.slug.clone()/>
                            <div class=style::field_row>
                                <div class=style::field_label>{tr(TextKey::Id)}</div>
                                <input
                                    class=style::text_input
                                    type="text"
                                    value=form.slug.clone()
                                    prop:value=move || slug.get()
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
                                        checked=form.auto_refresh
                                        prop:checked=move || auto_refresh.get()
                                        on:change=move |event| {
                                            auto_refresh.set(event_target_checked(&event));
                                        }
                                    />
                                    <small>{tr(TextKey::UpdateListFromTraktDaily)}</small>
                                </label>
                            </div>
                            <div class=style::field_row>
                                <div class=style::field_label>{tr(TextKey::GeneratePlaylist)}</div>
                                <label class=style::checkbox_row>
                                    <input
                                        type="checkbox"
                                        name="generate_playlist"
                                        value="true"
                                        checked=form.generate_playlist
                                        prop:checked=move || generate_playlist.get()
                                        on:change=move |event| {
                                            generate_playlist.set(event_target_checked(&event));
                                        }
                                    />
                                    <small>{tr(TextKey::GeneratePlaylistFromSortedTraktItems)}</small>
                                </label>
                            </div>
                        </form>
                    }.into_any(),
                    Some(None) => view! { <p class=style::empty>{tr(TextKey::TraktListNotFound)}</p> }.into_any(),
                    None => view! { <p class=style::empty>{tr(TextKey::Loading)}</p> }.into_any(),
                }}
            </Suspense>
        </div>
    }
}
