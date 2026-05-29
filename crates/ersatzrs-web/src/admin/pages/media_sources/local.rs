#![allow(clippy::too_many_lines, dead_code)]
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::admin::i18n::{media_kind_label, tr, TextKey};
use crate::admin::server_fns::media_sources::{
    delete_local_library, get_local_library_form, list_local_libraries, LocalLibraryForm,
    LocalLibraryRow,
};
use crate::admin::ui::primitives::{
    dialog::DeleteDialog,
    table::{Column, PageParams, PagedTable, RowActionsRenderer},
};

stylance::import_crate_style!(style, "src/admin/pages/media_sources/shared.module.css");

// ─── List ─────────────────────────────────────────────────────────────────────

#[component]
pub fn LocalLibrariesPage() -> impl IntoView {
    view! { <LocalLibrariesIsland/> }
}

#[island]
fn LocalLibrariesIsland() -> impl IntoView {
    let rows: Resource<Vec<LocalLibraryRow>> = Resource::new(
        || (),
        |()| async { list_local_libraries().await.unwrap_or_default() },
    );

    let total = Signal::derive(move || rows.get().map(|r| r.len() as u64));
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
        Column::new(tr(TextKey::Name), |r: &LocalLibraryRow| {
            let n = r.name.clone();
            view! { <span>{n}</span> }.into_any()
        }),
        Column::new(tr(TextKey::MediaKind), |r: &LocalLibraryRow| {
            let label = media_kind_label(i64::from(r.media_kind));
            view! { <span class=style::cell_muted>{label}</span> }.into_any()
        }),
        Column::new(tr(TextKey::Paths), |r: &LocalLibraryRow| {
            let n = r.path_count;
            view! { <span class=style::cell_muted>{n}</span> }.into_any()
        }),
    ];

    let row_actions: RowActionsRenderer<LocalLibraryRow> = Box::new(move |r| {
        let id = r.id;
        let r_clone = r.clone();
        view! {
            <div class=style::row_actions>
                <a href=format!("/media/sources/local/{id}/edit") class=style::row_btn>{tr(TextKey::Edit)}</a>
                <button
                    class=style::row_btn_del
                    on:click=move |_| {
                        delete_target.set(Some(r_clone.clone()));
                        delete_open.set(true);
                    }
                >
                    {tr(TextKey::Delete)}
                </button>
            </div>
        }
        .into_any()
    });

    view! {
        <Suspense fallback=|| ()>
        <div class=style::page>
            <div class=style::toolbar>
                <h1 class=style::page_title>{tr(TextKey::LocalLibraries)}</h1>
                <div class=style::toolbar_actions>
                    <a href="/media/sources/local/add" class=style::btn_primary>{format!("+ {}", tr(TextKey::AddLibrary))}</a>
                </div>
            </div>

            <PagedTable
                rows=rows
                total=total
                params=params
                columns=columns
                row_actions=Some(row_actions)
            />

            // DeleteDialog is always rendered; visibility is controlled via `open`.
            <DeleteDialog
                open=delete_open
                entity_name=Signal::derive(move || {
                    delete_target.get().map(|r| r.name.clone()).unwrap_or_default()
                })
                on_confirm=Callback::new(move |confirmed: bool| {
                    if let Some(r) = delete_target.get_untracked() {
                        if confirmed { delete_action.dispatch(r.id); }
                        else { delete_open.set(false); }
                    } else {
                        delete_open.set(false);
                    }
                })
            />
        </div>
        </Suspense>
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

    view! {
        <Suspense fallback=move || view! { <p class=style::cell_muted>{tr(TextKey::Loading)}</p> }>
            {move || initial.get().map(|initial_form| view! {
                <LocalLibraryEditorForm initial_form=initial_form/>
            })}
        </Suspense>
    }
}

#[component]
fn LocalLibraryEditorForm(initial_form: LocalLibraryForm) -> impl IntoView {
    let lib_id = initial_form.id;
    let media_kind_opts: Vec<(i32, String)> = vec![
        (1, tr(TextKey::Movies).into()),
        (2, tr(TextKey::TvShows).into()),
        (3, tr(TextKey::MusicVideos).into()),
        (4, tr(TextKey::OtherVideos).into()),
        (5, tr(TextKey::Songs).into()),
        (6, tr(TextKey::Images).into()),
        (7, tr(TextKey::RemoteStreams).into()),
    ];
    let paths_text = initial_form
        .paths
        .iter()
        .map(|p| p.path.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    view! {
        <div class=style::page>
            <form method="post" action="/api/media-sources/local/save">
                <input type="hidden" name="id" value=lib_id.to_string()/>

                <div class=style::toolbar>
                    <div>
                        <a href="/media/sources/local" class=style::back_link>{format!("← {}", tr(TextKey::LocalLibraries))}</a>
                        <h1 class=style::page_title>
                            {if lib_id != 0 { tr(TextKey::EditLibrary) } else { tr(TextKey::AddLibrary) }}
                        </h1>
                    </div>
                    <div class=style::toolbar_actions>
                        <button class=style::btn_primary type="submit">
                            {if lib_id != 0 { tr(TextKey::Save) } else { tr(TextKey::Add) }}
                        </button>
                    </div>
                </div>

                <div class=style::editor_form>
                    <div class=style::form_row>
                        <label class=style::form_label for="local-library-name">{tr(TextKey::Name)}</label>
                        <input
                            id="local-library-name"
                            class=style::form_input
                            type="text"
                            name="name"
                            value=initial_form.name
                        />
                    </div>

                    <div class=style::form_row>
                        <label class=style::form_label for="local-library-kind">{tr(TextKey::MediaKind)}</label>
                        <select
                            id="local-library-kind"
                            class=style::form_select
                            name="media_kind"
                            disabled=lib_id != 0
                        >
                            {media_kind_opts.iter().map(|(val, label)| {
                                let v = *val;
                                let l = label.clone();
                                let selected = v == initial_form.media_kind;
                                view! {
                                    <option value=v.to_string() selected=selected>{l}</option>
                                }
                            }).collect::<Vec<_>>()}
                        </select>
                        <input
                            type="hidden"
                            name="media_kind"
                            value=initial_form.media_kind.to_string()
                            disabled=lib_id == 0
                        />
                    </div>

                    <div class=style::form_section>
                        <label class=style::form_label for="local-library-paths">{tr(TextKey::LibraryPaths)}</label>
                        <textarea
                            id="local-library-paths"
                            class=style::form_input
                            name="paths_text"
                            rows="5"
                            placeholder="/mnt/media/movies"
                        >{paths_text}</textarea>
                        <p class=style::cell_muted>{tr(TextKey::EnterOnePathPerLine)}</p>
                    </div>
                </div>
            </form>
        </div>
    }
}
