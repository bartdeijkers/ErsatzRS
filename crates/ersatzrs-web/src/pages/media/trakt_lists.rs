//! Trakt list list and editor — ports `Pages/TraktLists.razor`,
//! `Pages/TraktListEditor.razor`, and `Shared/AddTraktListDialog.razor`.
#![allow(clippy::too_many_lines, dead_code)]

use leptos::prelude::*;
use leptos_router::hooks::{use_navigate, use_params_map};
use leptos_router::NavigateOptions;

use crate::server::trakt::{
    add_trakt_list, delete_trakt_list, get_trakt_list_form, list_trakt_lists, match_trakt_list,
    update_trakt_list, TraktListForm, TraktListRow,
};
use crate::ui::primitives::dialog::DeleteDialog;
use crate::ui::primitives::toast::{use_toast, ToastSeverity};

stylance::import_crate_style!(style, "src/pages/media/collections.module.css");

const PAGE_SIZE: u32 = 25;

#[component]
pub fn TraktListsPage() -> impl IntoView {
    let page = RwSignal::new(0u32);
    let add_value = RwSignal::new(String::new());

    let data: Resource<(Vec<TraktListRow>, u64)> = Resource::new(
        move || page.get(),
        move |p| async move { list_trakt_lists(p, PAGE_SIZE).await.unwrap_or_default() },
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
                    toast.push("Trakt list added.", ToastSeverity::Success);
                    add_value.set(String::new());
                    data.refetch();
                }
                Err(error) => toast.push(format!("Add failed: {error}"), ToastSeverity::Error),
            }
        }
    });

    Effect::new(move |_| {
        if let Some(result) = match_action.value().get() {
            match result {
                Ok(()) => {
                    toast.push("Trakt list matched.", ToastSeverity::Success);
                    data.refetch();
                }
                Err(error) => toast.push(format!("Match failed: {error}"), ToastSeverity::Error),
            }
        }
    });

    Effect::new(move |_| {
        if let Some(result) = delete_action.value().get() {
            match result {
                Ok(()) => {
                    toast.push("Trakt list deleted.", ToastSeverity::Success);
                    data.refetch();
                }
                Err(error) => toast.push(format!("Delete failed: {error}"), ToastSeverity::Error),
            }
            delete_open.set(false);
            delete_target.set(None);
        }
    });

    view! {
        <div class=style::page>
            <div class=style::toolbar>
                <h1 class=style::page_title>"Trakt Lists"</h1>
                <input
                    class=style::search
                    type="text"
                    placeholder="https://trakt.tv/users/username/lists/list-name"
                    prop:value=move || add_value.get()
                    on:input=move |event| add_value.set(event_target_value(&event))
                />
                <button
                    class=style::btn_add
                    disabled=move || add_value.with(|value| value.trim().is_empty())
                    on:click=move |_| {
                        add_action.dispatch(add_value.get_untracked());
                    }
                >
                    "Add Trakt List"
                </button>
            </div>

            <Suspense fallback=move || view! { <p class=style::empty>"Loading…"</p> }>
                {move || {
                    data.get().map(|(rows, _)| {
                        if rows.is_empty() {
                            return view! {
                                <p class=style::empty>"No Trakt lists yet."</p>
                            }
                            .into_any();
                        }

                        view! {
                            <div class=style::table_wrap>
                                <table>
                                    <thead>
                                        <tr>
                                            <th>"Id"</th>
                                            <th>"Name"</th>
                                            <th>"Match Status"</th>
                                            <th/>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {rows.into_iter().map(|row| {
                                            let id = row.id;
                                            let slug = row.slug.clone();
                                            let name = row.name.clone();
                                            let match_status = format!("{} of {}", row.match_count, row.item_count);
                                            let row_for_delete = row.clone();
                                            view! {
                                                <tr>
                                                    <td>{slug}</td>
                                                    <td>{name}</td>
                                                    <td>{match_status}</td>
                                                    <td>
                                                        <div class=style::row_actions>
                                                            <a
                                                                class=style::btn_edit
                                                                href=format!("/media/trakt/lists/{id}")
                                                            >
                                                                "Edit"
                                                            </a>
                                                            <a
                                                                class=style::btn_edit
                                                                href=format!("/search?query=trakt_list%3A{}", row.trakt_id)
                                                            >
                                                                "Search"
                                                            </a>
                                                            <button
                                                                class=style::btn_edit
                                                                on:click=move |_| {
                                                                    match_action.dispatch(id);
                                                                }
                                                            >
                                                                "Match"
                                                            </button>
                                                            <button
                                                                class=style::btn_delete
                                                                on:click=move |_| {
                                                                    delete_target.set(Some(row_for_delete.clone()));
                                                                    delete_open.set(true);
                                                                }
                                                            >
                                                                "Delete"
                                                            </button>
                                                        </div>
                                                    </td>
                                                </tr>
                                            }
                                        }).collect_view()}
                                    </tbody>
                                </table>
                            </div>
                        }
                        .into_any()
                    })
                }}
            </Suspense>

            {move || {
                delete_target.get().map(|row| {
                    view! {
                        <DeleteDialog
                            open=delete_open
                            entity_name=row.name
                            on_confirm=Callback::new(move |confirmed: bool| {
                                if confirmed {
                                    delete_action.dispatch(row.id);
                                } else {
                                    delete_open.set(false);
                                }
                            })
                        />
                    }
                })
            }}
        </div>
    }
}

#[component]
pub fn TraktListEditorPage() -> impl IntoView {
    let params = use_params_map();
    let list_id = move || params.with(|p| p.get("id").and_then(|value| value.parse::<i64>().ok()));
    let initial: Resource<Option<TraktListForm>> = Resource::new(list_id, |id| async move {
        match id {
            Some(id) => get_trakt_list_form(id).await.unwrap_or_default(),
            None => None,
        }
    });

    let form_id = RwSignal::new(0i64);
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
    let navigate = use_navigate();
    Effect::new(move |_| {
        if let Some(result) = save_action.value().get() {
            match result {
                Ok(()) => {
                    toast.push("Trakt list saved.", ToastSeverity::Success);
                    navigate("/media/trakt/lists", NavigateOptions::default());
                }
                Err(error) => toast.push(format!("Save failed: {error}"), ToastSeverity::Error),
            }
        }
    });

    view! {
        <div class=style::page>
            <div class=style::toolbar>
                <h1 class=style::page_title>"Trakt List"</h1>
                <button
                    class=style::btn_add
                    on:click=move |_| {
                        save_action.dispatch(TraktListForm {
                            id: form_id.get_untracked(),
                            slug: slug.get_untracked(),
                            auto_refresh: auto_refresh.get_untracked(),
                            generate_playlist: generate_playlist.get_untracked(),
                        });
                    }
                >
                    "Save Trakt List"
                </button>
            </div>

            <Suspense fallback=move || view! { <p class=style::empty>"Loading…"</p> }>
                {move || match initial.get() {
                    Some(Some(_)) => view! {
                        <div class=style::editor>
                            <section>
                                <h2 class=style::section_title>"Id"</h2>
                                <input class=style::search type="text" prop:value=move || slug.get() disabled/>
                            </section>
                            <section>
                                <label class=style::member>
                                    <input
                                        type="checkbox"
                                        prop:checked=move || auto_refresh.get()
                                        on:change=move |event| {
                                            auto_refresh.set(event_target_checked(&event));
                                        }
                                    />
                                    <span class=style::member_title>"Automatic Refresh"</span>
                                </label>
                            </section>
                            <section>
                                <label class=style::member>
                                    <input
                                        type="checkbox"
                                        prop:checked=move || generate_playlist.get()
                                        on:change=move |event| {
                                            generate_playlist.set(event_target_checked(&event));
                                        }
                                    />
                                    <span class=style::member_title>"Generate Playlist"</span>
                                </label>
                            </section>
                        </div>
                    }.into_any(),
                    Some(None) => view! { <p class=style::empty>"Trakt list not found."</p> }.into_any(),
                    None => view! { <p class=style::empty>"Loading…"</p> }.into_any(),
                }}
            </Suspense>
        </div>
    }
}
