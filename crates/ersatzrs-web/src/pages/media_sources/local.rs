#![allow(clippy::too_many_lines, dead_code)]
use leptos::prelude::*;
use leptos::web_sys;
use leptos_router::hooks::{use_navigate, use_params_map};
use leptos_router::NavigateOptions;

use crate::server::media_sources::{
    delete_local_library, get_local_library_form, list_local_libraries, save_local_library,
    LocalLibraryForm, LocalLibraryPath, LocalLibraryRow,
};
use crate::ui::primitives::{
    dialog::DeleteDialog,
    table::{Column, PageParams, PagedTable, RowActionsRenderer},
};

stylance::import_crate_style!(style, "src/pages/media_sources/shared.module.css");

// ─── List ─────────────────────────────────────────────────────────────────────

#[component]
pub fn LocalLibrariesPage() -> impl IntoView {
    let rows: Resource<Vec<LocalLibraryRow>> = Resource::new(
        || (),
        |()| async { list_local_libraries().await.unwrap_or_default() },
    );

    let total = Signal::derive(move || rows.get().map_or(0, |r| r.len() as u64));
    let params = RwSignal::new(PageParams::new(50));

    let delete_action = Action::new(|id: &i64| delete_local_library(*id));
    let delete_target: RwSignal<Option<LocalLibraryRow>> = RwSignal::new(None);
    let delete_open = RwSignal::new(false);

    Effect::new(move |_| {
        if matches!(delete_action.value().get(), Some(Ok(()))) {
            rows.refetch();
            delete_open.set(false);
        }
    });

    let columns: Vec<Column<LocalLibraryRow>> = vec![
        Column::new("Name", |r: &LocalLibraryRow| {
            let n = r.name.clone();
            view! { <span>{n}</span> }.into_any()
        }),
        Column::new("Media Kind", |r: &LocalLibraryRow| {
            let label = crate::server::media_sources::media_kind_label(r.media_kind);
            view! { <span class=style::cell_muted>{label}</span> }.into_any()
        }),
        Column::new("Paths", |r: &LocalLibraryRow| {
            let n = r.path_count;
            view! { <span class=style::cell_muted>{n}</span> }.into_any()
        }),
    ];

    let row_actions: RowActionsRenderer<LocalLibraryRow> = Box::new(move |r| {
        let id = r.id;
        let r_clone = r.clone();
        view! {
            <div class=style::row_actions>
                <a href=format!("/media/sources/local/{id}/edit") class=style::row_btn>"Edit"</a>
                <button
                    class=style::row_btn_del
                    on:click=move |_| {
                        delete_target.set(Some(r_clone.clone()));
                        delete_open.set(true);
                    }
                >
                    "Delete"
                </button>
            </div>
        }
        .into_any()
    });

    view! {
        <div class=style::page>
            <div class=style::toolbar>
                <h1 class=style::page_title>"Local Libraries"</h1>
                <div class=style::toolbar_actions>
                    <a href="/media/sources/local/add" class=style::btn_primary>"+ Add Library"</a>
                </div>
            </div>

            <PagedTable
                rows=rows
                total=total
                params=params
                columns=columns
                row_actions=Some(row_actions)
            />

            {move || {
                delete_target.get().map(|r| {
                    let name = r.name.clone();
                    view! {
                        <DeleteDialog
                            open=delete_open
                            entity_name=name
                            on_confirm=Callback::new(move |confirmed: bool| {
                                if confirmed { delete_action.dispatch(r.id); }
                                else { delete_open.set(false); }
                            })
                        />
                    }
                })
            }}
        </div>
    }
}

// ─── Editor ───────────────────────────────────────────────────────────────────

#[component]
pub fn LocalLibraryEditorPage() -> impl IntoView {
    let params = use_params_map();
    let lib_id = move || params.with(|p| p.get("id").and_then(|s| s.parse::<i64>().ok()));

    let initial: Resource<LocalLibraryForm> = Resource::new(lib_id, |id| async move {
        match id {
            Some(id) => get_local_library_form(id).await.unwrap_or_default(),
            None => LocalLibraryForm::new_defaults(),
        }
    });

    let name = RwSignal::new(String::new());
    let media_kind = RwSignal::new(1i32);
    let paths: RwSignal<Vec<LocalLibraryPath>> = RwSignal::new(vec![]);
    let new_path = RwSignal::new(String::new());
    let name_err: RwSignal<Option<String>> = RwSignal::new(None);

    Effect::new(move |_| {
        if let Some(f) = initial.get() {
            name.set(f.name);
            media_kind.set(f.media_kind);
            paths.set(f.paths);
        }
    });

    let navigate = use_navigate();
    let save_action = Action::new(move |form: &LocalLibraryForm| {
        let form = form.clone();
        async move { save_local_library(form).await }
    });

    Effect::new(move |_| match save_action.value().get() {
        Some(Ok(_)) => {
            navigate("/media/sources/local", NavigateOptions::default());
        }
        Some(Err(e)) => {
            name_err.set(Some(format!("Save failed: {e}")));
        }
        None => {}
    });

    let on_add_path = move |_: leptos::ev::MouseEvent| {
        let p = new_path.get_untracked().trim().to_string();
        if p.is_empty() {
            return;
        }
        paths.update(|list| {
            if !list.iter().any(|x| x.path == p) {
                list.push(LocalLibraryPath { id: 0, path: p });
            }
        });
        new_path.set(String::new());
    };

    let on_save = move |_| {
        let n = name.get_untracked();
        if n.trim().is_empty() {
            name_err.set(Some("Name is required.".into()));
            return;
        }
        name_err.set(None);
        let form = LocalLibraryForm {
            id: lib_id().unwrap_or(0),
            name: n,
            media_kind: media_kind.get_untracked(),
            paths: paths.get_untracked(),
        };
        save_action.dispatch(form);
    };

    let media_kind_opts: Vec<(i32, String)> = vec![
        (1, "Movies".into()),
        (2, "Shows".into()),
        (3, "Music Videos".into()),
        (4, "Other Videos".into()),
        (5, "Songs".into()),
        (6, "Images".into()),
        (7, "Remote Streams".into()),
    ];

    view! {
        <div class=style::page>
            <div class=style::toolbar>
                <div>
                    <a href="/media/sources/local" class=style::back_link>"← Local Libraries"</a>
                    <h1 class=style::page_title>
                        {move || if lib_id().is_some() { "Edit Library" } else { "Add Library" }}
                    </h1>
                </div>
                <div class=style::toolbar_actions>
                    <button class=style::btn_primary on:click=on_save>
                        {move || if lib_id().is_some() { "Save" } else { "Add" }}
                    </button>
                </div>
            </div>

            <div class=style::editor_form>
                <div class=style::form_row>
                    <label class=style::form_label>"Name"</label>
                    <input
                        class=style::form_input
                        type="text"
                        prop:value=move || name.get()
                        on:input=move |e| name.set(event_target_value(&e))
                    />
                    {move || name_err.get().map(|e| view! {
                        <span class=style::form_error>{e}</span>
                    })}
                </div>

                <div class=style::form_row>
                    <label class=style::form_label>"Media Kind"</label>
                    <select
                        class=style::form_select
                        disabled=move || lib_id().is_some()
                        prop:value=move || media_kind.get().to_string()
                        on:change=move |e| {
                            if let Ok(v) = event_target_value(&e).parse::<i32>() {
                                media_kind.set(v);
                            }
                        }
                    >
                        {media_kind_opts.iter().map(|(val, label)| {
                            let v = *val;
                            let l = label.clone();
                            view! {
                                <option value=v.to_string()
                                    selected=move || media_kind.get() == v>
                                    {l}
                                </option>
                            }
                        }).collect::<Vec<_>>()}
                    </select>
                </div>

                <div class=style::form_section>
                    <label class=style::form_label>"Library Paths"</label>
                    <div class=style::path_add_row>
                        <input
                            class=style::form_input
                            type="text"
                            placeholder="/mnt/media/movies"
                            prop:value=move || new_path.get()
                            on:input=move |e| new_path.set(event_target_value(&e))
                            on:keydown=move |e: web_sys::KeyboardEvent| {
                                if e.key() == "Enter" {
                                    let p = new_path.get_untracked().trim().to_string();
                                    if !p.is_empty() {
                                        paths.update(|list| {
                                            if !list.iter().any(|x| x.path == p) {
                                                list.push(LocalLibraryPath { id: 0, path: p });
                                            }
                                        });
                                        new_path.set(String::new());
                                    }
                                }
                            }
                        />
                        <button class=style::btn_secondary on:click=on_add_path>"Add Path"</button>
                    </div>

                    {move || {
                        let list = paths.get();
                        if list.is_empty() {
                            view! {
                                <p class=style::cell_muted>"No paths added yet."</p>
                            }.into_any()
                        } else {
                            view! {
                                <table style="width:100%;border-collapse:collapse">
                                    <tbody>
                                        {list.iter().enumerate().map(|(i, p)| {
                                            let path_str = p.path.clone();
                                            view! {
                                                <tr>
                                                    <td style="padding:0.35rem 0;font-size:0.875rem">{path_str}</td>
                                                    <td style="text-align:right">
                                                        <button
                                                            class=style::row_btn_del
                                                            on:click=move |_| {
                                                                paths.update(|list| { list.remove(i); });
                                                            }
                                                        >"Remove"</button>
                                                    </td>
                                                </tr>
                                            }
                                        }).collect::<Vec<_>>()}
                                    </tbody>
                                </table>
                            }.into_any()
                        }
                    }}
                </div>
            </div>
        </div>
    }
}
