//! Smart collection list and editor — ports `Pages/SmartCollections.razor`
//! and `SmartCollectionEditor.razor`. Smart collections store a search query
//! (Lucene in `ErsatzTV`, parity-pending in `ErsatzRS`).
#![allow(clippy::too_many_lines, dead_code)]
use leptos::prelude::*;
use leptos_router::hooks::{use_navigate, use_params_map};
use leptos_router::NavigateOptions;

use crate::admin::server_fns::media_browser::{
    delete_smart_collection, get_smart_collection, list_smart_collections, save_smart_collection,
    CollectionForm, CollectionListRow,
};
use crate::admin::ui::primitives::{
    button::{Button, ButtonVariant, LinkButton},
    dialog::DeleteDialog,
    form::{TextArea, TextField},
    toast::{use_toast, ToastSeverity},
};

stylance::import_crate_style!(style, "src/admin/pages/media/collections.module.css");

const PAGE_SIZE: u32 = 25;

// ---------------------------------------------------------------------------
// List page
// ---------------------------------------------------------------------------

#[component]
pub fn SmartCollectionsPage() -> impl IntoView {
    let search = RwSignal::new(String::new());
    let page = RwSignal::new(0u32);

    let data: Resource<(Vec<CollectionListRow>, u64)> = Resource::new(
        move || (search.get(), page.get()),
        move |(q, p)| async move {
            list_smart_collections(q, p, PAGE_SIZE)
                .await
                .unwrap_or_default()
        },
    );

    let delete_target: RwSignal<Option<CollectionListRow>> = RwSignal::new(None);
    let delete_open = RwSignal::new(false);
    let delete_action = Action::new(|id: &i64| delete_smart_collection(*id));

    let toast = use_toast();
    Effect::new(move |_| {
        if let Some(res) = delete_action.value().get() {
            match res {
                Ok(()) => {
                    toast.push("Smart collection deleted.", ToastSeverity::Success);
                    data.refetch();
                }
                Err(e) => toast.push(format!("Delete failed: {e}"), ToastSeverity::Error),
            }
            delete_open.set(false);
        }
    });

    view! {
        <div class=style::page>
            <div class=style::toolbar>
                <h1 class=style::page_title>"Smart Collections"</h1>
                <input
                    class=style::search
                    type="search"
                    placeholder="Search…"
                    prop:value=move || search.get()
                    on:input=move |e| search.set(event_target_value(&e))
                />
                <a href="/media/smart-collections/add" class=style::btn_add>"+ Add Smart Collection"</a>
            </div>

            <Suspense fallback=move || view! { <p class=style::empty>"Loading…"</p> }>
                {move || {
                    data.get().map(|(rows, _)| {
                        if rows.is_empty() {
                            return view! {
                                <p class=style::empty>"No smart collections yet."</p>
                            }
                            .into_any();
                        }
                        view! {
                            <div class=style::table_wrap>
                                <table>
                                    <thead>
                                        <tr>
                                            <th>"Name"</th>
                                            <th>"Query"</th>
                                            <th/>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {rows.into_iter().map(|row| {
                                            let id = row.id;
                                            let name = row.name.clone();
                                            let query = row.query.clone().unwrap_or_default();
                                            let row_for_delete = row.clone();
                                            view! {
                                                <tr>
                                                    <td>
                                                        <a href=format!("/media/smart-collections/{id}")>
                                                            {name}
                                                        </a>
                                                    </td>
                                                    <td>{query}</td>
                                                    <td>
                                                        <div class=style::row_actions>
                                                            <a
                                                                class=style::btn_edit
                                                                href=format!("/media/smart-collections/{id}")
                                                            >
                                                                "Edit"
                                                            </a>
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

            <DeleteDialog
                open=delete_open
                entity_name=Signal::derive(move || {
                    delete_target.get().map(|row| row.name.clone()).unwrap_or_default()
                })
                on_confirm=Callback::new(move |confirmed: bool| {
                    if let Some(row) = delete_target.get_untracked() {
                        if confirmed {
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

// ---------------------------------------------------------------------------
// Editor page
// ---------------------------------------------------------------------------

#[component]
pub fn SmartCollectionEditorPage() -> impl IntoView {
    let params = use_params_map();
    let collection_id = move || params.with(|p| p.get("id").and_then(|s| s.parse::<i64>().ok()));

    let initial: Resource<CollectionForm> = Resource::new(collection_id, |id| async move {
        match id {
            Some(id) => get_smart_collection(id)
                .await
                .unwrap_or_default()
                .unwrap_or_default(),
            None => CollectionForm::default(),
        }
    });

    let name = RwSignal::new(String::new());
    let query = RwSignal::new(String::new());
    let id_signal: RwSignal<i64> = RwSignal::new(0);
    Effect::new(move |_| {
        if let Some(f) = initial.get() {
            id_signal.set(f.id);
            name.set(f.name);
            query.set(f.query);
        }
    });

    let name_err: RwSignal<Option<String>> = RwSignal::new(None);
    let name_err_signal: Signal<Option<String>> = name_err.into();
    let query_err: RwSignal<Option<String>> = RwSignal::new(None);
    let query_err_signal: Signal<Option<String>> = query_err.into();

    let save_action = Action::new(|form: &CollectionForm| {
        let form = form.clone();
        async move { save_smart_collection(form).await }
    });

    let toast = use_toast();
    let navigate = use_navigate();

    Effect::new(move |_| {
        if let Some(res) = save_action.value().get() {
            match res {
                Ok(new_id) => {
                    toast.push("Smart collection saved.", ToastSeverity::Success);
                    if id_signal.get_untracked() == 0 {
                        let target = format!("/media/smart-collections/{new_id}");
                        navigate(&target, NavigateOptions::default());
                    }
                    id_signal.set(new_id);
                }
                Err(e) => toast.push(format!("Save failed: {e}"), ToastSeverity::Error),
            }
        }
    });

    let on_save = move |()| {
        let trimmed_name = name.get().trim().to_string();
        let trimmed_query = query.get().trim().to_string();
        let mut ok = true;
        if trimmed_name.is_empty() {
            name_err.set(Some("Name is required".into()));
            ok = false;
        } else {
            name_err.set(None);
        }
        if trimmed_query.is_empty() {
            query_err.set(Some("Query is required".into()));
            ok = false;
        } else {
            query_err.set(None);
        }
        if !ok {
            return;
        }
        save_action.dispatch(CollectionForm {
            id: id_signal.get_untracked(),
            name: trimmed_name,
            query: trimmed_query,
        });
    };

    view! {
        <div class=style::page>
            <h1 class=style::page_title>
                {move || if id_signal.get() == 0 {
                    "New Smart Collection".to_string()
                } else {
                    "Edit Smart Collection".to_string()
                }}
            </h1>
            <div class=style::editor>
                <TextField label="Name" value=name error=Some(name_err_signal)/>
                <TextArea
                    label="Query"
                    value=query
                    placeholder="e.g. genre:action OR genre:comedy"
                    error=Some(query_err_signal)
                />

                <div class=style::form_actions>
                    <Button
                        variant=ButtonVariant::Primary
                        on_click=Some(Callback::new(on_save))
                    >
                        "Save"
                    </Button>
                    <LinkButton
                        href="/media/smart-collections".to_string()
                        variant=ButtonVariant::Text
                    >
                        "Back"
                    </LinkButton>
                </div>
            </div>
        </div>
    }
}
