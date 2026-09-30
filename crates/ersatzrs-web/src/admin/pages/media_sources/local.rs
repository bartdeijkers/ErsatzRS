#![allow(clippy::too_many_lines, dead_code)]
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::admin::i18n::{media_kind_label, tr, TextKey};
use crate::admin::server_fns::media_sources::{
    delete_local_library, get_local_library_form, list_local_libraries,
    list_managed_addon_media_sources, LocalLibraryForm, LocalLibraryRow,
    ManagedAddonMediaSourceRow,
};
use crate::admin::ui::primitives::{
    dialog::DeleteDialog,
    resource_state::{admin_loading_resource_view, admin_result_view},
    table::{Column, PageParams, PagedTable, RowActionsRenderer},
    toast::{load_or_notify, use_toast},
};

stylance::import_crate_style!(style, "src/admin/pages/media_sources/shared.module.css");

// ─── List ─────────────────────────────────────────────────────────────────────

/// One row of the local sources table: either an operator-managed library or
/// the storage root a schema-v3 add-on owns.
#[derive(Clone)]
enum LocalSourceRow {
    Library(LocalLibraryRow),
    AddonStorage(ManagedAddonMediaSourceRow),
}

impl LocalSourceRow {
    fn name(&self) -> &str {
        match self {
            Self::Library(row) => &row.name,
            Self::AddonStorage(row) => &row.name,
        }
    }
}

/// Merge both loaders into one alphabetical list so add-on storage roots read
/// as ordinary rows instead of a separate section above the table.
fn merge_local_sources(
    libraries: Vec<LocalLibraryRow>,
    addon_sources: Vec<ManagedAddonMediaSourceRow>,
) -> Vec<LocalSourceRow> {
    let mut rows: Vec<LocalSourceRow> = libraries
        .into_iter()
        .map(LocalSourceRow::Library)
        .chain(addon_sources.into_iter().map(LocalSourceRow::AddonStorage))
        .collect();
    rows.sort_by_key(|row| row.name().to_lowercase());
    rows
}

/// Row actions for add-on owned rows: the primary action opens the owning
/// add-on instead of the library editor, and deleting stays disabled because
/// the add-on — not the operator — manages the registration.
fn addon_row_actions(href: String, label: &'static str) -> AnyView {
    view! {
        <div class=style::row_actions>
            <a href=href class=style::row_btn>{label}</a>
            <button class=style::row_btn_del disabled=true title=tr(TextKey::ManagedByAddon)>
                {tr(TextKey::Delete)}
            </button>
        </div>
    }
    .into_any()
}

#[component]
pub fn LocalLibrariesPage() -> impl IntoView {
    view! { <LocalLibrariesIsland/> }
}

#[island]
fn LocalLibrariesIsland() -> impl IntoView {
    let load_toast = use_toast();
    let libraries: LocalResource<Vec<LocalLibraryRow>> = LocalResource::new(move || async move {
        load_or_notify(list_local_libraries().await, load_toast)
    });
    let addon_sources = LocalResource::new(move || async move {
        load_or_notify(list_managed_addon_media_sources().await, load_toast)
    });

    let rows =
        Signal::derive(move || Some(merge_local_sources(libraries.get()?, addon_sources.get()?)));
    let total = Signal::derive(move || rows.get().map(|r| r.len() as u64));
    let params = RwSignal::new(PageParams::new(50));

    let delete_action = Action::new(|id: &i64| delete_local_library(*id));
    let delete_target: RwSignal<Option<LocalLibraryRow>> = RwSignal::new(None);
    let delete_open = RwSignal::new(false);

    Effect::new(move |_| {
        if matches!(delete_action.value().get(), Some(Ok(()))) {
            libraries.refetch();
            delete_open.set(false);
        }
    });

    let columns: Vec<Column<LocalSourceRow>> = vec![
        Column::new(tr(TextKey::Name), |r: &LocalSourceRow| match r {
            LocalSourceRow::Library(row) => {
                let name = row.name.clone();
                view! { <span>{name}</span> }.into_any()
            }
            LocalSourceRow::AddonStorage(row) => {
                let name = row.name.clone();
                let root_path = row.root_path.clone();
                view! {
                    <span data-testid="managed-addon-media-source" title=root_path>{name}</span>
                }
                .into_any()
            }
        }),
        Column::new(tr(TextKey::MediaKind), |r: &LocalSourceRow| {
            let label = match r {
                LocalSourceRow::Library(row) => media_kind_label(i64::from(row.media_kind)),
                LocalSourceRow::AddonStorage(_) => tr(TextKey::Addon).to_owned(),
            };
            view! { <span class=style::cell_muted>{label}</span> }.into_any()
        }),
        Column::new(tr(TextKey::Paths), |r: &LocalSourceRow| {
            let n = match r {
                LocalSourceRow::Library(row) => row.path_count,
                // The add-on owns exactly one storage root.
                LocalSourceRow::AddonStorage(_) => 1,
            };
            view! { <span class=style::cell_muted>{n}</span> }.into_any()
        }),
    ];

    let row_actions: RowActionsRenderer<LocalSourceRow> = Box::new(move |r| match r {
        LocalSourceRow::AddonStorage(row) => addon_row_actions(
            format!("/settings/addons/{}", row.addon_id),
            tr(TextKey::Edit),
        ),
        LocalSourceRow::Library(row) if row.managed_by_addon => addon_row_actions(
            "/media/addon-lists".to_owned(),
            tr(TextKey::AddonMediaLists),
        ),
        LocalSourceRow::Library(row) => {
            let id = row.id;
            let r_clone = row.clone();
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
        }
    });

    view! {
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
                dialog_id="local-source-delete-dialog"
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
    }
}

// ─── Editor ───────────────────────────────────────────────────────────────────

#[component]
pub fn LocalLibraryEditorPage() -> impl IntoView {
    let params = use_params_map();
    let lib_id = move || params.with(|p| p.get("id").and_then(|s| s.parse::<i64>().ok()));

    let initial = Resource::new(lib_id, |id| async move {
        match id {
            Some(id) => get_local_library_form(id).await,
            None => Ok(LocalLibraryForm::new_defaults()),
        }
    });

    view! {
        <Suspense fallback=admin_loading_resource_view>
            {move || initial.get().map(|result| admin_result_view(result, |initial_form| {
                view! { <LocalLibraryEditorForm initial_form=initial_form/> }.into_any()
            }))}
        </Suspense>
    }
}

#[component]
fn LocalLibraryEditorForm(
    initial_form: LocalLibraryForm,
    #[prop(optional)] error: Option<String>,
) -> impl IntoView {
    let lib_id = initial_form.id;
    #[cfg(target_os = "windows")]
    let path_example = r"C:\Media\Movies";
    #[cfg(not(target_os = "windows"))]
    let path_example = "/media/movies";
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
    // Leptos treats textarea children as raw text during SSR. Encode the
    // content explicitly so a rejected path cannot close the textarea tag.
    let paths_html = paths_text
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");

    view! {
        <div class=style::page>
            <form method="post" action="/admin-api/media-sources/local/save">
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
                    {error.map(|message| view! { <p class=style::error_msg role="alert">{message}</p> })}
                    <a
                        href=format!("/media/remote/streams/playlists/{lib_id}")
                        class=style::back_link
                        style:display=if lib_id != 0 && initial_form.media_kind == 7 { "" } else { "none" }
                    >
                        {tr(TextKey::RemoteStreamPlaylists)}
                    </a>
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
                            placeholder=path_example
                            inner_html=paths_html
                        ></textarea>
                        <p class=style::cell_muted>{tr(TextKey::EnterOnePathPerLine)}</p>
                    </div>
                </div>
            </form>
        </div>
    }
}

/// Render the same correction form for a failed HTML POST, retaining all input.
/// This response needs no JavaScript; Leptos escapes submitted paths and names.
#[cfg(feature = "ssr")]
pub(crate) fn local_library_error_document(
    initial_form: LocalLibraryForm,
    message: String,
    language: crate::admin::i18n::UiLanguage,
    is_dark_mode: bool,
) -> String {
    let owner = Owner::new();
    owner.with(|| {
        provide_context(language);
        view! {
            <!DOCTYPE html>
            <html lang=language.code()>
                <head>
                    <meta charset="utf-8"/>
                    <meta name="viewport" content="width=device-width, initial-scale=1"/>
                    <title>{tr(TextKey::LocalLibraries)}</title>
                    <link rel="stylesheet" href="/public/bundle.css"/>
                </head>
                <body data-theme=if is_dark_mode { "dark" } else { "light" }>
                    <main class=style::validation_page>
                        <LocalLibraryEditorForm initial_form error=message/>
                    </main>
                </body>
            </html>
        }
        .to_html()
    })
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{merge_local_sources, LocalSourceRow};
    use crate::admin::server_fns::media_sources::{LocalLibraryRow, ManagedAddonMediaSourceRow};

    fn library(name: &str) -> LocalLibraryRow {
        LocalLibraryRow {
            id: 1,
            name: name.to_owned(),
            media_kind: 1,
            path_count: 2,
            managed_by_addon: false,
        }
    }

    #[test]
    fn addon_storage_roots_sort_into_the_library_rows() {
        let rows = merge_local_sources(
            vec![library("Movies"), library("TV Shows")],
            vec![ManagedAddonMediaSourceRow {
                addon_id: "org.example.addon".to_owned(),
                name: "beeldengeluid".to_owned(),
                root_path: "/srv/media/beeldengeluid".to_owned(),
            }],
        );

        assert_eq!(
            rows.iter().map(LocalSourceRow::name).collect::<Vec<_>>(),
            vec!["beeldengeluid", "Movies", "TV Shows"]
        );
        assert!(matches!(rows[0], LocalSourceRow::AddonStorage(_)));
    }
}
