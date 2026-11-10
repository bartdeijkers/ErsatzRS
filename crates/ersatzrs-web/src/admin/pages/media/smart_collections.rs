//! Smart collection list and editor — ports `Pages/SmartCollections.razor`
//! and `SmartCollectionEditor.razor`. Smart collections and reusable saved
//! searches share the core-owned canonical query contract.
use leptos::prelude::*;
use leptos_router::hooks::{use_params_map, use_query_map};
use serde::{Deserialize, Serialize};

use crate::admin::i18n::{
    delete_failed, media_browser_error_message, save_failed, search_query_error_message, tr,
    TextKey,
};
use crate::admin::search_query::{
    QueryComposerGroup, QueryComposerModel, QueryComposerNode, QueryComposerRule, QueryGroupKind,
};
use crate::admin::server_fns::media_browser::{
    delete_smart_collection, get_smart_collection, list_smart_collections, save_smart_collection,
    CollectionForm, CollectionListRow,
};
use crate::admin::server_fns::search::{
    compose_search_query, list_saved_searches, project_search_query, save_saved_search,
    search_results, SavedSearchForm, SearchResults,
};
use crate::admin::ui::primitives::page_header::PageHeader;
use crate::admin::ui::primitives::{
    dialog::{DeleteDialog, Modal},
    form::TextField,
    icon::{icon_style, IconName},
    pagination::PaginationControls,
    resource_state::{admin_loading_resource_view, admin_result_view},
    toast::{load_failed_message, load_or_notify, use_toast, ToastSeverity},
};

// collections.module.css is shared with collections.rs and
// multi_collections.rs; classes unused in this file's per-invocation
// generated constants are used there.
stylance::import_crate_style!(
    #[allow(dead_code)]
    style,
    "src/admin/pages/media/collections.module.css"
);

#[cfg(target_arch = "wasm32")]
fn set_browser_location(path: &str) {
    if let Some(window) = web_sys::window() {
        let _ = window.location().set_href(path);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn set_browser_location(_path: &str) {}

#[derive(Clone)]
struct SmartCollectionSaveIntent {
    form: CollectionForm,
    model: Option<QueryComposerModel>,
}

#[derive(Clone)]
struct SavedSearchSaveIntent {
    form: SavedSearchForm,
    model: Option<QueryComposerModel>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct SmartCollectionEditorRequest {
    collection_id: i64,
    carried_query: String,
    carried_name: String,
    save_error: bool,
    return_to: String,
    saved_search_id: i64,
    saved_search_name: String,
    saved_search_error: String,
    saved_search_saved: bool,
    saved_search_loaded: bool,
}

// ---------------------------------------------------------------------------
// List page
// ---------------------------------------------------------------------------

#[component]
pub fn SmartCollectionsPage() -> impl IntoView {
    let initial = Resource::new_blocking(|| (), |()| list_smart_collections(String::new(), 0, 10));

    view! {
        <Suspense fallback=admin_loading_resource_view>
            {move || {
                initial.get().map(|result| admin_result_view(result, |(initial_rows, initial_total)| {
                    view! {
                        <SmartCollectionsIsland
                            initial_rows=initial_rows
                            initial_total=initial_total
                        />
                    }
                    .into_any()
                }))
            }}
        </Suspense>
    }
}

#[island]
fn SmartCollectionsIsland(
    initial_rows: Vec<CollectionListRow>,
    initial_total: u64,
) -> impl IntoView {
    let search = RwSignal::new(String::new());
    let page = RwSignal::new(0u32);
    let page_size = RwSignal::new(10u32);
    let toast = use_toast();

    let current_data = RwSignal::new((initial_rows, initial_total));
    let data: LocalResource<(Vec<CollectionListRow>, u64)> = LocalResource::new(move || {
        let q = search.get();
        let p = page.get();
        let size = page_size.get();
        async move { load_or_notify(list_smart_collections(q, p, size).await, toast) }
    });
    Effect::new(move |_| {
        if let Some(next_data) = data.get() {
            current_data.set(next_data);
        }
    });

    let delete_target: RwSignal<Option<CollectionListRow>> = RwSignal::new(None);
    let delete_open = RwSignal::new(false);
    let delete_action = Action::new(|id: &i64| delete_smart_collection(*id));

    Effect::new(move |_| {
        if let Some(res) = delete_action.value().get() {
            match res {
                Ok(()) => {
                    toast.push(tr(TextKey::SmartCollectionDeleted), ToastSeverity::Success);
                    data.refetch();
                }
                Err(e) => toast.push(
                    delete_failed(media_browser_error_message(e)),
                    ToastSeverity::Error,
                ),
            }
            delete_open.set(false);
        }
    });

    view! {
        <div class=style::page>
            <section class=style::content>
                <PageHeader title=tr(TextKey::SmartCollections)/>

                {move || {
                    let (rows, total) = current_data.get();
                    view! {
                        <div class=style::table_wrap>
                            <div class=style::table_toolbar>
                                <label class=style::search_field>
                                    <span style=icon_style(IconName::FilterList, "1.125rem") aria-hidden="true"></span>
                                    <input
                                        class=style::search
                                        type="search"
                                        placeholder=tr(TextKey::SearchForSmartCollections)
                                        prop:value=move || search.get()
                                        on:input=move |e| {
                                            search.set(event_target_value(&e));
                                            page.set(0);
                                        }
                                    />
                                </label>
                            </div>
                            <table class=style::smart_table>
                                <tbody>
                                    {rows.into_iter().map(|row| {
                                        let id = row.id;
                                        let name = row.name.clone();
                                        let query = row.query.clone().unwrap_or_default();
                                        let search_href = format!("/search?query={}", urlencoding::encode(&query));
                                        let row_for_delete = row.clone();
                                        view! {
                                            <tr>
                                                <td>{name}</td>
                                                <td>
                                                    <div class=style::row_actions>
                                                        <a
                                                            class=style::icon_button
                                                            href=format!("/media/smart-collections/{id}/edit")
                                                            title=tr(TextKey::EditSmartCollection)
                                                            aria-label=tr(TextKey::EditSmartCollection)
                                                        >
                                                            <span style=icon_style(IconName::Edit, "1.25rem") aria-hidden="true"></span>
                                                        </a>
                                                        <a
                                                            class=style::icon_button
                                                            href=search_href
                                                            title=tr(TextKey::SearchSmartCollection)
                                                            aria-label=tr(TextKey::SearchSmartCollection)
                                                        >
                                                            <span style=icon_style(IconName::Search, "1.25rem") aria-hidden="true"></span>
                                                        </a>
                                                        <button
                                                            type="button"
                                                            class=style::icon_button
                                                            title=tr(TextKey::DeleteSmartCollection)
                                                            aria-label=tr(TextKey::DeleteSmartCollection)
                                                            on:click=move |_| {
                                                                delete_target.set(Some(row_for_delete.clone()));
                                                                delete_open.set(true);
                                                            }
                                                        >
                                                            <span style=icon_style(IconName::Delete, "1.25rem") aria-hidden="true"></span>
                                                        </button>
                                                    </div>
                                                </td>
                                            </tr>
                                        }
                                    }).collect_view()}
                                    <tr style:display=move || if total == 0 { "" } else { "none" }>
                                        <td colspan="2" class=style::empty_cell>{tr(TextKey::NoItems)}</td>
                                    </tr>
                                </tbody>
                            </table>
                            <PaginationControls
                                page=Signal::derive(move || page.get())
                                total=Signal::derive(move || Some(total))
                                page_size=Signal::derive(move || page_size.get())
                                page_size_options=Some(vec![10, 25, 50])
                                on_page_size_change=Callback::new(move |value: u32| {
                                    page_size.set(value.max(1));
                                    page.set(0);
                                })
                                on_previous=Callback::new(move |()| {
                                    page.update(|value| *value = value.saturating_sub(1));
                                })
                                on_next=Callback::new(move |()| {
                                    let size = page_size.get().max(1);
                                    let max_page = u32::try_from(total.div_ceil(u64::from(size)))
                                        .unwrap_or(u32::MAX)
                                        .saturating_sub(1);
                                    page.update(|value| *value = value.saturating_add(1).min(max_page));
                                })
                            />
                        </div>
                    }
                    .into_any()
                }}
            </section>

            <DeleteDialog
                dialog_id="smart-collection-delete-dialog"
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
    let collection_id = params.with_untracked(|p| {
        p.get("id")
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or_default()
    });
    let query_params = use_query_map();
    let save_error =
        query_params.with_untracked(|params| params.get("error").unwrap_or_default()) == "save";
    let carried_query =
        query_params.with_untracked(|params| params.get("query").unwrap_or_default());
    let carried_name =
        query_params.with_untracked(|params| params.get("collection_name").unwrap_or_default());
    let saved_search_id = query_params.with_untracked(|params| {
        params
            .get("saved_search_id")
            .and_then(|value| value.parse::<i64>().ok())
            .filter(|id| *id > 0)
            .unwrap_or_default()
    });
    let saved_search_name =
        query_params.with_untracked(|params| params.get("saved_search_name").unwrap_or_default());
    let saved_search_error =
        query_params.with_untracked(|params| params.get("saved_error").unwrap_or_default());
    let saved_search_saved =
        query_params.with_untracked(|params| params.get("saved").unwrap_or_default()) == "search";
    let saved_search_loaded =
        query_params.with_untracked(|params| params.get("loaded").unwrap_or_default()) == "search";
    let return_to = safe_search_return_path(
        &query_params.with_untracked(|params| params.get("return_to").unwrap_or_default()),
    );

    let request = SmartCollectionEditorRequest {
        collection_id,
        carried_query,
        carried_name,
        save_error,
        return_to,
        saved_search_id,
        saved_search_name,
        saved_search_error,
        saved_search_saved,
        saved_search_loaded,
    };
    view! { <SmartCollectionEditorIsland request/> }
}

#[island]
fn SmartCollectionEditorIsland(request: SmartCollectionEditorRequest) -> impl IntoView {
    view! {
        <SmartCollectionComposer
            collection_id=request.collection_id
            initial_query=request.carried_query
            initial_name=request.carried_name
            save_error=request.save_error
            return_to=request.return_to
            initial_saved_search_id=request.saved_search_id
            initial_saved_search_name=request.saved_search_name
            saved_search_error=request.saved_search_error
            saved_search_saved=request.saved_search_saved
            saved_search_loaded=request.saved_search_loaded
        />
    }
}

#[component]
fn SmartCollectionComposer(
    collection_id: i64,
    initial_query: String,
    initial_name: String,
    save_error: bool,
    return_to: String,
    initial_saved_search_id: i64,
    initial_saved_search_name: String,
    saved_search_error: String,
    saved_search_saved: bool,
    saved_search_loaded: bool,
) -> impl IntoView {
    let toast = use_toast();
    let carried_query = StoredValue::new(initial_query.clone());
    let carried_name = StoredValue::new(initial_name.clone());
    let name = RwSignal::new(initial_name);
    let canonical_query = RwSignal::new(initial_query.clone());
    let id_signal = RwSignal::new(collection_id);
    let model = RwSignal::new(QueryComposerModel::default());
    let model_revision = RwSignal::new(0u64);
    let structured_dirty = RwSignal::new(false);
    let composition_pending = RwSignal::new(false);
    let projection_pending = RwSignal::new(false);
    let projection_revision = RwSignal::new(0u64);
    let expanded_group = RwSignal::new(QueryGroupKind::All);
    let focused_group = RwSignal::new(None::<GroupLocation>);
    let hydrated = RwSignal::new(false);
    let initial_loaded = RwSignal::new(false);

    let name_err: RwSignal<Option<String>> = RwSignal::new(None);
    let query_error = RwSignal::new(
        (collection_id == 0 && initial_query.trim().is_empty())
            .then(|| tr(TextKey::SearchQueryRequired).to_owned()),
    );
    if save_error {
        name_err.set(Some(save_failed(media_browser_error_message("save"))));
    }
    let name_err_signal: Signal<Option<String>> = name_err.into();

    let save_action = Action::new(|intent: &SmartCollectionSaveIntent| {
        let intent = intent.clone();
        async move {
            let mut form = intent.form;
            if let Some(model) = intent.model {
                form.query = compose_search_query(model).await?;
            }
            save_smart_collection(form).await
        }
    });

    let composed_query: LocalResource<Option<(u64, Result<String, ServerFnError>)>> =
        LocalResource::new(move || {
            let revision = model_revision.get();
            let current_model = model.get();
            async move {
                if revision == 0 {
                    None
                } else {
                    Some((revision, compose_search_query(current_model).await))
                }
            }
        });
    Effect::new(move |_| {
        let Some(Some((revision, result))) = composed_query.get() else {
            return;
        };
        if revision != model_revision.get_untracked() || !structured_dirty.get_untracked() {
            return;
        }
        composition_pending.set(false);
        match result {
            Ok(query) => {
                canonical_query.set(query);
                query_error.set(None);
            }
            Err(error) => query_error.set(Some(search_query_error_message(error))),
        }
    });

    let preview: LocalResource<Result<SearchResults, ServerFnError>> =
        LocalResource::new(move || {
            let query = canonical_query.get();
            async move { search_results(query).await }
        });

    let projection_action = Action::new(|input: &(u64, String)| {
        let (revision, query) = input.clone();
        async move {
            let result = project_search_query(query.clone()).await;
            (revision, query, result)
        }
    });
    let start_projection = Callback::new(move |query: String| {
        structured_dirty.set(false);
        composition_pending.set(false);
        projection_pending.set(true);
        projection_revision.update(|revision| *revision = revision.wrapping_add(1));
        projection_action.dispatch((projection_revision.get_untracked(), query));
    });
    let initial_load_action = Action::new(|id: &i64| {
        let id = *id;
        async move { get_smart_collection(id).await }
    });
    Effect::new(move |_| {
        let Some((revision, projected_query, result)) = projection_action.value().get() else {
            return;
        };
        if revision != projection_revision.get_untracked() {
            return;
        }
        projection_pending.set(false);
        match result {
            Ok(projected) => {
                #[cfg(target_arch = "wasm32")]
                leptos::leptos_dom::helpers::request_animation_frame(move || {
                    model.set(projected);
                    if projected_query.trim().is_empty() {
                        query_error.set(Some(tr(TextKey::SearchQueryRequired).to_owned()));
                    } else {
                        query_error.set(None);
                    }
                    initial_loaded.set(true);
                });
                #[cfg(not(target_arch = "wasm32"))]
                {
                    model.set(projected);
                    if projected_query.trim().is_empty() {
                        query_error.set(Some(tr(TextKey::SearchQueryRequired).to_owned()));
                    } else {
                        query_error.set(None);
                    }
                    initial_loaded.set(true);
                }
            }
            Err(error) => {
                query_error.set(Some(search_query_error_message(error)));
                initial_loaded.set(true);
            }
        }
    });
    Effect::new(move |_| {
        let Some(result) = initial_load_action.value().get() else {
            return;
        };
        match result {
            Ok(Some(mut form)) => {
                let carried_query = carried_query.get_value();
                let carried_name = carried_name.get_value();
                if !carried_query.is_empty() {
                    form.query = carried_query;
                }
                if !carried_name.is_empty() {
                    form.name = carried_name;
                }
                id_signal.set(form.id);
                name.set(form.name);
                canonical_query.set(form.query.clone());
                if form.query.trim().is_empty() {
                    query_error.set(Some(tr(TextKey::SearchQueryRequired).to_owned()));
                    initial_loaded.set(true);
                } else {
                    start_projection.run(form.query);
                }
            }
            Ok(None) => {
                query_error.set(Some(load_failed_message("Smart collection not found")));
                initial_loaded.set(true);
            }
            Err(error) => {
                query_error.set(Some(load_failed_message(error)));
                initial_loaded.set(true);
            }
        }
    });
    #[cfg(target_arch = "wasm32")]
    {
        let initial_started = RwSignal::new(false);
        Effect::new(move |_| {
            if initial_started.get_untracked() {
                return;
            }
            initial_started.set(true);
            leptos::leptos_dom::helpers::request_animation_frame(move || {
                hydrated.set(true);
                if collection_id == 0 {
                    let query = canonical_query.get_untracked();
                    if query.trim().is_empty() {
                        initial_loaded.set(true);
                    } else {
                        start_projection.run(query);
                    }
                } else {
                    initial_load_action.dispatch(collection_id);
                }
            });
        });
    }

    Effect::new(move |_| {
        if let Some(res) = save_action.value().get() {
            match res {
                Ok(_) => {
                    toast.push(tr(TextKey::SmartCollectionSaved), ToastSeverity::Success);
                    set_browser_location("/media/smart-collections");
                }
                Err(e) => toast.push(
                    save_failed(search_query_error_message(e)),
                    ToastSeverity::Error,
                ),
            }
        }
    });

    let on_save = move |()| {
        if !initial_loaded.get_untracked() || composition_pending.get_untracked() {
            return;
        }
        let trimmed_name = name.get().trim().to_string();
        let trimmed_query = canonical_query.get().trim().to_string();
        if trimmed_name.is_empty() {
            name_err.set(Some(tr(TextKey::NameRequired).into()));
            return;
        }
        if trimmed_name.chars().count() > 50 {
            name_err.set(Some(media_browser_error_message(
                "Smart collection name must be 50 characters or less",
            )));
            return;
        }
        name_err.set(None);
        if trimmed_query.is_empty() {
            query_error.set(Some(tr(TextKey::SearchQueryRequired).to_owned()));
            return;
        }
        save_action.dispatch(SmartCollectionSaveIntent {
            form: CollectionForm {
                id: id_signal.get_untracked(),
                name: trimmed_name,
                query: trimmed_query,
            },
            model: structured_dirty
                .get_untracked()
                .then(|| model.get_untracked()),
        });
    };

    let initial_saved_rows = if initial_saved_search_id > 0 {
        vec![SavedSearchForm {
            id: initial_saved_search_id,
            name: initial_saved_search_name.clone(),
            query: canonical_query.get_untracked(),
        }]
    } else {
        Vec::new()
    };
    let selected_saved_search_id = RwSignal::new(initial_saved_search_id);
    let saved_search_name = RwSignal::new(initial_saved_search_name);
    let saved_searches = RwSignal::new(initial_saved_rows);
    let saved_search_error_message_signal = RwSignal::new(
        (!saved_search_error.is_empty())
            .then(|| saved_search_fallback_error_message(&saved_search_error)),
    );
    let saved_search_notice = RwSignal::new(if saved_search_saved {
        Some(tr(TextKey::SavedSearchSaved).to_owned())
    } else if saved_search_loaded {
        Some(tr(TextKey::SavedSearchLoaded).to_owned())
    } else {
        None
    });
    let list_saved_searches_action = Action::new(|(): &()| list_saved_searches());
    Effect::new(move |_| {
        let Some(result) = list_saved_searches_action.value().get() else {
            return;
        };
        match result {
            Ok(rows) => saved_searches.set(rows),
            Err(error) => {
                saved_search_error_message_signal.set(Some(search_query_error_message(error)));
            }
        }
    });
    #[cfg(target_arch = "wasm32")]
    {
        let saved_searches_started = RwSignal::new(false);
        Effect::new(move |_| {
            if saved_searches_started.get_untracked() {
                return;
            }
            saved_searches_started.set(true);
            leptos::leptos_dom::helpers::request_animation_frame(move || {
                list_saved_searches_action.dispatch(());
            });
        });
    }
    let save_saved_search_action = Action::new(|intent: &SavedSearchSaveIntent| {
        let intent = intent.clone();
        async move {
            let mut form = intent.form;
            if let Some(model) = intent.model {
                form.query = compose_search_query(model).await?;
            }
            save_saved_search(form).await
        }
    });
    Effect::new(move |_| {
        let Some(result) = save_saved_search_action.value().get() else {
            return;
        };
        match result {
            Ok(id) => {
                selected_saved_search_id.set(id);
                let updated = SavedSearchForm {
                    id,
                    name: saved_search_name.get_untracked().trim().to_owned(),
                    query: canonical_query.get_untracked().trim().to_owned(),
                };
                saved_searches.update(|rows| {
                    if let Some(row) = rows.iter_mut().find(|row| row.id == id) {
                        *row = updated.clone();
                    } else {
                        rows.push(updated);
                    }
                });
                saved_search_error_message_signal.set(None);
                saved_search_notice.set(Some(tr(TextKey::SavedSearchSaved).to_owned()));
                toast.push(tr(TextKey::SavedSearchSaved), ToastSeverity::Success);
            }
            Err(error) => {
                let message = search_query_error_message(error);
                saved_search_error_message_signal.set(Some(message.clone()));
                toast.push(save_failed(message), ToastSeverity::Error);
            }
        }
    });
    let on_save_saved_search = Callback::new(move |()| {
        if !initial_loaded.get_untracked() || composition_pending.get_untracked() {
            return;
        }
        let saved_name = saved_search_name.get_untracked().trim().to_owned();
        if saved_name.is_empty() {
            saved_search_error_message_signal
                .set(Some(tr(TextKey::SavedSearchNameRequired).to_owned()));
            return;
        }
        if saved_name.chars().count() > 80 {
            saved_search_error_message_signal
                .set(Some(tr(TextKey::SavedSearchNameTooLong).to_owned()));
            return;
        }
        let query = canonical_query.get_untracked().trim().to_owned();
        if query.is_empty() {
            query_error.set(Some(tr(TextKey::SearchQueryRequired).to_owned()));
            return;
        }
        saved_search_error_message_signal.set(None);
        saved_search_notice.set(None);
        save_saved_search_action.dispatch(SavedSearchSaveIntent {
            form: SavedSearchForm {
                id: selected_saved_search_id.get_untracked(),
                name: saved_name,
                query,
            },
            model: structured_dirty
                .get_untracked()
                .then(|| model.get_untracked()),
        });
    });
    let on_load_saved_search = Callback::new(move |saved: SavedSearchForm| {
        selected_saved_search_id.set(saved.id);
        saved_search_name.set(saved.name);
        canonical_query.set(saved.query.clone());
        saved_search_error_message_signal.set(None);
        saved_search_notice.set(Some(tr(TextKey::SavedSearchLoaded).to_owned()));
        focused_group.set(None);
        start_projection.run(saved.query);
        toast.push(tr(TextKey::SavedSearchLoaded), ToastSeverity::Success);
    });
    let on_new_saved_search = Callback::new(move |()| {
        selected_saved_search_id.set(0);
        saved_search_name.set(String::new());
        saved_search_error_message_signal.set(None);
        saved_search_notice.set(None);
    });

    let rule_editor_open = RwSignal::new(false);
    let rule_editor_target = RwSignal::new(None::<RuleEditorTarget>);
    let rule_editor_value = RwSignal::new(QueryComposerRule::default());
    let open_rule_editor = Callback::new(
        move |(target, rule): (RuleEditorTarget, QueryComposerRule)| {
            rule_editor_target.set(Some(target));
            rule_editor_value.set(rule);
            rule_editor_open.set(true);
        },
    );
    let mark_model_changed = Callback::new(move |()| {
        structured_dirty.set(true);
        composition_pending.set(true);
        model_revision.update(|revision| *revision = revision.wrapping_add(1));
    });
    let remove_node = Callback::new(move |location: NodeLocation| {
        model.update(|current| {
            if let Some(nodes) = current.nodes_at_path_mut(location.kind, &location.parent_path) {
                if location.index < nodes.len() {
                    nodes.remove(location.index);
                }
            }
        });
        mark_model_changed.run(());
    });
    let open_focus = Callback::new(move |location: GroupLocation| {
        focused_group.set(Some(location));
    });

    let canonical_search_href = move || {
        format!(
            "/search?query={}",
            urlencoding::encode(&canonical_query.get())
        )
    };

    view! {
        <form
            method="post"
            action="/admin-api/smart-collections/save"
            class=format!("{} {}", style::page, style::composer_page)
            data-smart-collection-editor-hydrated=move || if hydrated.get() { "true" } else { "false" }
            data-composer-all=move || model.get().all.len().to_string()
            data-composer-any=move || model.get().any.len().to_string()
            data-composer-none=move || model.get().none.len().to_string()
            data-projection-revision=move || projection_revision.get().to_string()
            on:submit=move |event| {
                event.prevent_default();
                on_save(());
            }
        >
            <input type="hidden" name="id" value=move || id_signal.get().to_string()/>
            <input type="hidden" name="return_to" value=return_to.clone()/>
            <div class=style::editor_action_bar>
                <a class=style::composer_back href=return_to.clone()>{tr(TextKey::BackToSearch)}</a>
                <button
                    type="submit"
                    class=format!("{} {}", style::btn_add, style::composer_save)
                    disabled=move || save_action.pending().get()
                        || (hydrated.get()
                            && (!initial_loaded.get()
                                || composition_pending.get()))
                        || query_error.get().is_some()
                >
                    <span style=icon_style(IconName::Save, "1.125rem") aria-hidden="true"></span>
                    {tr(TextKey::SaveSmartCollection)}
                </button>
            </div>

            <fieldset
                class=style::composer_workspace
                disabled=move || hydrated.get() && !initial_loaded.get()
                aria-busy=move || (hydrated.get() && (!initial_loaded.get() || projection_pending.get())).to_string()
            >
            <main class=style::composer>
                <header class=style::composer_header>
                    <div>
                        <p class=style::composer_eyebrow>{tr(TextKey::CollectionComposer)}</p>
                        <h1 class=style::page_title>{tr(TextKey::SmartCollection)}</h1>
                    </div>
                    <span class=style::query_status data-valid=move || query_error.get().is_none().to_string()>
                        {move || if query_error.get().is_none() { tr(TextKey::QueryValid).to_owned() } else { tr(TextKey::QueryInvalid).to_owned() }}
                    </span>
                </header>

                <div class=style::composer_layout>
                    <aside class=style::composer_details aria-label=tr(TextKey::CollectionDetails)>
                        <h2>{tr(TextKey::CollectionDetails)}</h2>
                        <TextField
                            label=tr(TextKey::Name)
                            value=name
                            name=Some("name")
                            required=true
                            max_length=Some(50)
                            error=Some(name_err_signal)
                        />
                        <details class=style::canonical_details>
                            <summary>{tr(TextKey::CanonicalQuery)}</summary>
                            <p>{tr(TextKey::CanonicalQueryHelp)}</p>
                            <textarea
                                name="query"
                                rows="5"
                                prop:value=move || canonical_query.get()
                                on:input=move |event| {
                                    canonical_query.set(event_target_value(&event));
                                    saved_search_notice.set(None);
                                }
                                on:change=move |event| {
                                    let query = event_target_value(&event);
                                    canonical_query.set(query.clone());
                                    start_projection.run(query);
                                }
                            >{canonical_query.get_untracked()}</textarea>
                        </details>
                        <details class=style::saved_searches>
                            <summary>{tr(TextKey::SavedSearches)}</summary>
                            <p>{tr(TextKey::SavedSearchHelp)}</p>
                            <SavedSearchControls
                                rows=Signal::from(saved_searches)
                                selected_id=selected_saved_search_id
                                saved_name=saved_search_name
                                error=saved_search_error_message_signal
                                notice=saved_search_notice
                                pending=Signal::derive(move || {
                                    save_saved_search_action.pending().get()
                                        || (hydrated.get()
                                            && (!initial_loaded.get()
                                                || composition_pending.get()))
                                })
                                on_load=on_load_saved_search
                                on_save=on_save_saved_search
                                on_new=on_new_saved_search
                            />
                        </details>
                    </aside>

                    <section
                        class=style::composer_rules
                        style:display=move || if focused_group.get().is_none() { "" } else { "none" }
                        aria-label=tr(TextKey::Rules)
                    >
                        <div class=style::section_heading>
                            <div><h2>{tr(TextKey::Rules)}</h2><p>{tr(TextKey::NestedGroupHelp)}</p></div>
                        </div>
                        {move || {
                            // Own the model subscription at the island root. This
                            // remounts the card views after asynchronous initial
                            // projection instead of leaving their SSR-empty
                            // dynamic markers stale during hydration.
                            let _ = model.get();
                            view! {
                                <RuleGroupCard
                                    kind=QueryGroupKind::All
                                    model
                                    expanded_group
                                    open_rule_editor
                                    remove_node
                                    open_focus
                                    on_model_changed=mark_model_changed
                                />
                                <RuleGroupCard
                                    kind=QueryGroupKind::Any
                                    model
                                    expanded_group
                                    open_rule_editor
                                    remove_node
                                    open_focus
                                    on_model_changed=mark_model_changed
                                />
                                <RuleGroupCard
                                    kind=QueryGroupKind::None
                                    model
                                    expanded_group
                                    open_rule_editor
                                    remove_node
                                    open_focus
                                    on_model_changed=mark_model_changed
                                />
                            }
                        }}
                    </section>

                    <NestedFocusView
                        model
                        focused_group
                        open_rule_editor
                        remove_node
                        on_model_changed=mark_model_changed
                    />

                    <aside class=style::composer_preview aria-label=tr(TextKey::LivePreview)>
                        <div class=style::preview_heading>
                            <h2>{tr(TextKey::LivePreview)}</h2>
                            <a href=canonical_search_href>{tr(TextKey::SearchSmartCollection)}</a>
                        </div>
                        <strong class=style::preview_count>
                            {move || preview.get().and_then(Result::ok).map(|results| search_result_total(&results)).unwrap_or_default()}
                        </strong>
                        <span>{tr(TextKey::MatchingItems)}</span>
                        <p
                            class=style::query_error
                            style:display=move || if query_error.get().is_some() { "" } else { "none" }
                        >{move || query_error.get().unwrap_or_default()}</p>
                    </aside>
                </div>
            </main>
            </fieldset>

            <Modal
                dialog_id="smart-collection-rule-dialog"
                open=rule_editor_open
                title=Signal::derive(move || {
                    if rule_editor_target.get().and_then(|target| target.index).is_some() {
                        tr(TextKey::EditRule).to_owned()
                    } else {
                        tr(TextKey::AddRule).to_owned()
                    }
                })
                confirm_label=Some(tr(TextKey::Apply))
                confirm_disabled=Signal::derive(move || rule_editor_value.get().is_empty())
                on_close=Callback::new(move |confirmed: bool| {
                    if !confirmed {
                        return;
                    }
                    let Some(target) = rule_editor_target.get_untracked() else {
                        return;
                    };
                    let rule = rule_editor_value.get_untracked();
                    model.update(|current| {
                        let Some(nodes) = current.nodes_at_path_mut(target.kind, &target.parent_path) else {
                            return;
                        };
                        if let Some(index) = target.index {
                            if let Some(node) = nodes.get_mut(index) {
                                *node = QueryComposerNode::Rule(rule);
                            }
                        } else {
                            nodes.push(QueryComposerNode::Rule(rule));
                        }
                    });
                    mark_model_changed.run(());
                })
            >
                <RuleEditorFields rule=rule_editor_value/>
            </Modal>
        </form>
    }
}

#[component]
fn SavedSearchControls(
    rows: Signal<Vec<SavedSearchForm>>,
    selected_id: RwSignal<i64>,
    saved_name: RwSignal<String>,
    error: RwSignal<Option<String>>,
    notice: RwSignal<Option<String>>,
    pending: Signal<bool>,
    on_load: Callback<SavedSearchForm>,
    on_save: Callback<()>,
    on_new: Callback<()>,
) -> impl IntoView {
    view! {
        <div class=style::saved_search_controls>
            <label>
                <span>{tr(TextKey::ChooseSavedSearch)}</span>
                <select
                    name="saved_search_id"
                    prop:value=move || selected_id.get().to_string()
                    on:change=move |event| {
                        let id = event_target_value(&event).parse::<i64>().unwrap_or_default();
                        selected_id.set(id);
                        if let Some(saved) = rows.get().into_iter().find(|saved| saved.id == id) {
                            saved_name.set(saved.name.clone());
                        } else {
                            saved_name.set(String::new());
                        }
                        error.set(None);
                        notice.set(None);
                    }
                >
                    <option value="0">{tr(TextKey::ChooseSavedSearch)}</option>
                    <For
                        each=move || rows.get()
                        key=|saved| saved.id
                        children=move |saved| {
                            let id = saved.id;
                        view! {
                            <option value=id.to_string() selected=selected_id.get_untracked() == id>
                                {saved.name}
                            </option>
                        }
                        }
                    />
                </select>
            </label>
            <p
                class=style::saved_search_status
                style:display=move || if rows.get().is_empty() { "" } else { "none" }
            >{tr(TextKey::NoSavedSearches)}</p>
            <div class=style::saved_search_actions>
                <button
                    type="submit"
                    formmethod="post"
                    formaction="/admin-api/saved-searches/load"
                    formnovalidate
                    class=style::secondary_button
                    disabled=move || pending.get()
                    on:click=move |event| {
                        event.prevent_default();
                        if let Some(saved) = rows
                            .get()
                            .into_iter()
                            .find(|saved| saved.id == selected_id.get_untracked())
                        {
                            on_load.run(saved);
                        }
                    }
                >{tr(TextKey::LoadSavedSearch)}</button>
                <button
                    type="button"
                    class=style::text_button
                    disabled=move || pending.get()
                    on:click=move |_| on_new.run(())
                >{tr(TextKey::NewSavedSearch)}</button>
            </div>
            <label>
                <span>{tr(TextKey::SavedSearchName)}</span>
                <input
                    type="text"
                    name="saved_search_name"
                    maxlength="80"
                    value=saved_name.get_untracked()
                    prop:value=move || saved_name.get()
                    on:input=move |event| {
                        saved_name.set(event_target_value(&event));
                        error.set(None);
                        notice.set(None);
                    }
                />
            </label>
            <button
                type="submit"
                formmethod="post"
                formaction="/admin-api/saved-searches/save"
                formnovalidate
                class=style::secondary_button
                disabled=move || pending.get()
                on:click=move |event| {
                    event.prevent_default();
                    on_save.run(());
                }
            >
                {move || if selected_id.get() == 0 {
                    tr(TextKey::SaveSavedSearch)
                } else {
                    tr(TextKey::UpdateSavedSearch)
                }}
            </button>
            <p
                class=style::saved_search_error
                role="alert"
                style:display=move || if error.get().is_some() { "" } else { "none" }
            >{move || error.get().unwrap_or_default()}</p>
            <p
                class=style::saved_search_notice
                role="status"
                style:display=move || if notice.get().is_some() { "" } else { "none" }
            >{move || notice.get().unwrap_or_default()}</p>
        </div>
    }
}

fn safe_search_return_path(candidate: &str) -> String {
    if candidate == "/search" {
        return candidate.to_owned();
    }
    if let Some(query) = candidate.strip_prefix("/search?query=") {
        let decoded = urlencoding::decode(query).unwrap_or_else(|_| query.into());
        return format!("/search?query={}", urlencoding::encode(&decoded));
    }
    "/search".to_owned()
}

fn saved_search_fallback_error_message(code: &str) -> String {
    match code {
        "name_required" => tr(TextKey::SavedSearchNameRequired).to_owned(),
        "name_too_long" => tr(TextKey::SavedSearchNameTooLong).to_owned(),
        "name_unique" => tr(TextKey::SavedSearchNameUnique).to_owned(),
        "query_required" => tr(TextKey::SearchQueryRequired).to_owned(),
        "query_rule_required" => tr(TextKey::SearchRuleValueRequired).to_owned(),
        "query_group_required" => tr(TextKey::SearchGroupRequired).to_owned(),
        "query_wildcard_required" => tr(TextKey::SearchWildcardRequired).to_owned(),
        _ => save_failed(media_browser_error_message("save")),
    }
}

#[derive(Clone, Debug)]
struct GroupLocation {
    kind: QueryGroupKind,
    path: Vec<usize>,
}

#[derive(Clone, Debug)]
struct NodeLocation {
    kind: QueryGroupKind,
    parent_path: Vec<usize>,
    index: usize,
}

#[derive(Clone, Debug)]
struct RuleEditorTarget {
    kind: QueryGroupKind,
    parent_path: Vec<usize>,
    index: Option<usize>,
}

#[component]
fn RuleGroupCard(
    kind: QueryGroupKind,
    model: RwSignal<QueryComposerModel>,
    expanded_group: RwSignal<QueryGroupKind>,
    open_rule_editor: Callback<(RuleEditorTarget, QueryComposerRule)>,
    remove_node: Callback<NodeLocation>,
    open_focus: Callback<GroupLocation>,
    on_model_changed: Callback<()>,
) -> impl IntoView {
    let is_expanded = move || expanded_group.get() == kind;
    let accent_class = match kind {
        QueryGroupKind::All => style::group_all,
        QueryGroupKind::Any => style::group_any,
        QueryGroupKind::None => style::group_none,
    };

    view! {
        <article class=format!("{} {}", style::group_card, accent_class)>
            <button
                type="button"
                class=style::group_header
                aria-expanded=move || is_expanded().to_string()
                on:click=move |_| expanded_group.set(kind)
            >
                <span><strong>{group_label(kind)}</strong><small>{group_help(kind)}</small></span>
                <span class=style::group_count>{move || model.get().nodes(kind).len()}</span>
            </button>
            <div class=style::group_body style:display=move || if is_expanded() { "" } else { "none" }>
                <div class=style::rule_list>
                    <p
                        class=style::no_rules
                        style:display=move || if model.get().nodes(kind).is_empty() { "" } else { "none" }
                    >{tr(TextKey::NoRules)}</p>
                    <For
                        each=move || model.get().nodes(kind).to_vec().into_iter().enumerate()
                        key=|(index, node)| format!("{index}:{node:?}")
                        children=move |(index, node)| {
                            match node {
                                QueryComposerNode::Rule(rule) => {
                                    let edit_rule = rule.clone();
                                    view! {
                                        <div class=style::rule_row>
                                            <span class=style::rule_summary>{rule_summary(&rule)}</span>
                                            <div class=style::rule_actions>
                                                <button type="button" on:click=move |_| {
                                                    open_rule_editor.run((RuleEditorTarget {
                                                        kind,
                                                        parent_path: Vec::new(),
                                                        index: Some(index),
                                                    }, edit_rule.clone()));
                                                }>{tr(TextKey::EditRule)}</button>
                                                <button type="button" on:click=move |_| remove_node.run(NodeLocation {
                                                    kind,
                                                    parent_path: Vec::new(),
                                                    index,
                                                }) aria-label=tr(TextKey::RemoveRule)>
                                                    <span style=icon_style(IconName::Delete, "1rem") aria-hidden="true"></span>
                                                </button>
                                            </div>
                                        </div>
                                    }.into_any()
                                }
                                QueryComposerNode::Group(group) => {
                                    let child_count = group.children.len();
                                    view! {
                                        <div class=format!("{} {}", style::rule_row, style::nested_row)>
                                            <span class=style::rule_summary>
                                                <strong>{tr(TextKey::NestedGroup)}</strong>
                                                <small>{format!("{} · {child_count}", group_label(group.kind))}</small>
                                            </span>
                                            <div class=style::rule_actions>
                                                <button type="button" on:click=move |_| open_focus.run(GroupLocation {
                                                    kind,
                                                    path: vec![index],
                                                })>{tr(TextKey::EditNestedGroup)}</button>
                                                <button type="button" on:click=move |_| remove_node.run(NodeLocation {
                                                    kind,
                                                    parent_path: Vec::new(),
                                                    index,
                                                }) aria-label=tr(TextKey::RemoveRule)>
                                                    <span style=icon_style(IconName::Delete, "1rem") aria-hidden="true"></span>
                                                </button>
                                            </div>
                                        </div>
                                    }.into_any()
                                }
                            }
                        }
                    />
                </div>
                <div class=style::group_actions>
                    <button type="button" class=style::secondary_button on:click=move |_| {
                        open_rule_editor.run((RuleEditorTarget {
                            kind,
                            parent_path: Vec::new(),
                            index: None,
                        }, QueryComposerRule::default()));
                    }>{"+ "}{tr(TextKey::AddRule)}</button>
                    <button type="button" class=style::text_button on:click=move |_| {
                        let mut path = Vec::new();
                        model.update(|current| {
                            let nodes = current.nodes_mut(kind);
                            path.push(nodes.len());
                            nodes.push(QueryComposerNode::Group(QueryComposerGroup {
                                kind: QueryGroupKind::Any,
                                children: Vec::new(),
                            }));
                        });
                        on_model_changed.run(());
                        open_focus.run(GroupLocation { kind, path: path.clone() });
                    }>{tr(TextKey::AddNestedGroup)}</button>
                </div>
            </div>
        </article>
    }
}

#[component]
fn NestedFocusView(
    model: RwSignal<QueryComposerModel>,
    focused_group: RwSignal<Option<GroupLocation>>,
    open_rule_editor: Callback<(RuleEditorTarget, QueryComposerRule)>,
    remove_node: Callback<NodeLocation>,
    on_model_changed: Callback<()>,
) -> impl IntoView {
    let back = move |_| {
        focused_group.update(|current| {
            if let Some(location) = current {
                if location.path.len() > 1 {
                    location.path.pop();
                } else {
                    *current = None;
                }
            }
        });
    };

    view! {
        <section
            class=format!("{} {}", style::composer_rules, style::focus_view)
            style:display=move || if focused_group.get().is_some() { "" } else { "none" }
            aria-label=tr(TextKey::NestedGroup)
        >
            <div class=style::focus_header>
                <button type="button" class=style::text_button on:click=back>{"← "}{tr(TextKey::BackToRules)}</button>
                <div>
                    <p class=style::composer_eyebrow>{tr(TextKey::NestedGroup)}</p>
                    <h2>{move || focused_group.get().and_then(|location| {
                        model.get().group_at_path(location.kind, &location.path).map(|group| group_label(group.kind).to_owned())
                    }).unwrap_or_else(|| tr(TextKey::NestedGroup).to_owned())}</h2>
                </div>
                <label class=style::operator_select>
                    <span>{tr(TextKey::MatchType)}</span>
                    <select
                        prop:value=move || focused_group.get().and_then(|location| {
                            model.get().group_at_path(location.kind, &location.path).map(|group| group_kind_value(group.kind).to_owned())
                        }).unwrap_or_else(|| "any".to_owned())
                        on:change=move |event| {
                            let next_kind = parse_group_kind(&event_target_value(&event));
                            if let Some(location) = focused_group.get_untracked() {
                                model.update(|current| {
                                    if let Some(group) = current.group_at_path_mut(location.kind, &location.path) {
                                        group.kind = next_kind;
                                    }
                                });
                                on_model_changed.run(());
                            }
                        }
                    >
                        <option value="all">{tr(TextKey::MatchAll)}</option>
                        <option value="any">{tr(TextKey::MatchAny)}</option>
                        <option value="none">{tr(TextKey::ExcludeAny)}</option>
                    </select>
                </label>
            </div>

            <div class=style::rule_list>
                {move || {
                    let Some(location) = focused_group.get() else {
                        return ().into_any();
                    };
                    let nodes = model.get().nodes_at_path(location.kind, &location.path).unwrap_or_default().to_vec();
                    if nodes.is_empty() {
                        return view! { <p class=style::no_rules>{tr(TextKey::NoRules)}</p> }.into_any();
                    }
                    nodes.into_iter().enumerate().map(|(index, node)| {
                        let parent_path = location.path.clone();
                        match node {
                            QueryComposerNode::Rule(rule) => {
                                let edit_rule = rule.clone();
                                let edit_path = parent_path.clone();
                                let remove_path = parent_path.clone();
                                view! {
                                    <div class=style::rule_row>
                                        <span class=style::rule_summary>{rule_summary(&rule)}</span>
                                        <div class=style::rule_actions>
                                            <button type="button" on:click=move |_| open_rule_editor.run((RuleEditorTarget {
                                                kind: location.kind,
                                                parent_path: edit_path.clone(),
                                                index: Some(index),
                                            }, edit_rule.clone()))>{tr(TextKey::EditRule)}</button>
                                            <button type="button" on:click=move |_| remove_node.run(NodeLocation {
                                                kind: location.kind,
                                                parent_path: remove_path.clone(),
                                                index,
                                            }) aria-label=tr(TextKey::RemoveRule)>
                                                <span style=icon_style(IconName::Delete, "1rem") aria-hidden="true"></span>
                                            </button>
                                        </div>
                                    </div>
                                }.into_any()
                            }
                            QueryComposerNode::Group(group) => {
                                let mut child_path = parent_path.clone();
                                child_path.push(index);
                                let remove_path = parent_path.clone();
                                let child_count = group.children.len();
                                view! {
                                    <div class=format!("{} {}", style::rule_row, style::nested_row)>
                                        <span class=style::rule_summary><strong>{tr(TextKey::NestedGroup)}</strong><small>{format!("{} · {child_count}", group_label(group.kind))}</small></span>
                                        <div class=style::rule_actions>
                                            <button type="button" on:click=move |_| focused_group.set(Some(GroupLocation {
                                                kind: location.kind,
                                                path: child_path.clone(),
                                            }))>{tr(TextKey::EditNestedGroup)}</button>
                                            <button type="button" on:click=move |_| remove_node.run(NodeLocation {
                                                kind: location.kind,
                                                parent_path: remove_path.clone(),
                                                index,
                                            }) aria-label=tr(TextKey::RemoveRule)>
                                                <span style=icon_style(IconName::Delete, "1rem") aria-hidden="true"></span>
                                            </button>
                                        </div>
                                    </div>
                                }.into_any()
                            }
                        }
                    }).collect_view().into_any()
                }}
            </div>
            <div class=style::group_actions>
                <button type="button" class=style::secondary_button on:click=move |_| {
                    if let Some(location) = focused_group.get_untracked() {
                        open_rule_editor.run((RuleEditorTarget {
                            kind: location.kind,
                            parent_path: location.path,
                            index: None,
                        }, QueryComposerRule::default()));
                    }
                }>{"+ "}{tr(TextKey::AddRule)}</button>
                <button type="button" class=style::text_button on:click=move |_| {
                    let Some(mut location) = focused_group.get_untracked() else {
                        return;
                    };
                    model.update(|current| {
                        if let Some(nodes) = current.nodes_at_path_mut(location.kind, &location.path) {
                            location.path.push(nodes.len());
                            nodes.push(QueryComposerNode::Group(QueryComposerGroup {
                                kind: QueryGroupKind::Any,
                                children: Vec::new(),
                            }));
                        }
                    });
                    on_model_changed.run(());
                    focused_group.set(Some(location));
                }>{tr(TextKey::AddNestedGroup)}</button>
            </div>
        </section>
    }
}

#[component]
fn RuleEditorFields(rule: RwSignal<QueryComposerRule>) -> impl IntoView {
    const FIELDS: [(&str, TextKey); 21] = [
        ("", TextKey::AllFields),
        ("title", TextKey::Title),
        ("type", TextKey::Type),
        ("year", TextKey::Year),
        ("genre", TextKey::Genre),
        ("tag", TextKey::Tags),
        ("collection", TextKey::Collection),
        ("studio", TextKey::Studio),
        ("music_label", TextKey::MusicLabels),
        ("network", TextKey::Network),
        ("actor", TextKey::Actor),
        ("director", TextKey::Director),
        ("writer", TextKey::Writer),
        ("language", TextKey::Language),
        ("subtitle_language", TextKey::SubtitleLanguage),
        ("content_rating", TextKey::ContentRatings),
        ("show_title", TextKey::ShowTitle),
        ("library_name", TextKey::Library),
        ("content_kind", TextKey::ContentKindLabel),
        ("sub_kind", TextKey::SubKindLabel),
        ("has_commentary", TextKey::HasCommentary),
    ];

    view! {
        <div class=style::rule_editor>
            <label>
                <span>{tr(TextKey::RuleField)}</span>
                <select prop:value=move || rule.get().field on:change=move |event| {
                    rule.update(|current| current.field = event_target_value(&event));
                }>
                    {FIELDS.into_iter().map(|(value, label)| view! { <option value=value>{tr(label)}</option> }).collect_view()}
                    {move || {
                        let current = rule.get().field;
                        (!FIELDS.iter().any(|(value, _)| *value == current))
                            .then(|| view! { <option value=current.clone()>{current.clone()}</option> })
                    }}
                </select>
            </label>
            <label>
                <span>{tr(TextKey::MatchType)}</span>
                <select prop:value=move || rule.get().value_kind() on:change=move |event| {
                    rule.update(|current| current.set_value_kind(&event_target_value(&event)));
                }>
                    <option value="text">{tr(TextKey::TextContains)}</option>
                    <option value="phrase">{tr(TextKey::ExactPhrase)}</option>
                    <option value="wildcard">{tr(TextKey::Wildcard)}</option>
                    <option value="fuzzy">{tr(TextKey::Fuzzy)}</option>
                    <option value="range">{tr(TextKey::Range)}</option>
                </select>
            </label>
            <label>
                <span>{move || if rule.get().value_kind() == "range" { tr(TextKey::RangeStart) } else { tr(TextKey::MatchValue) }}</span>
                <input type="text" prop:value=move || rule.get().primary_value() on:input=move |event| {
                    rule.update(|current| current.set_primary_value(event_target_value(&event)));
                }/>
            </label>
            <label style:display=move || if matches!(rule.get().value_kind(), "range" | "fuzzy" | "phrase") { "" } else { "none" }>
                <span>{move || if rule.get().value_kind() == "range" { tr(TextKey::RangeEnd) } else { tr(TextKey::MatchValue) }}</span>
                <input type="text" prop:value=move || rule.get().secondary_value() on:input=move |event| {
                    rule.update(|current| current.set_secondary_value(event_target_value(&event)));
                }/>
            </label>
        </div>
    }
}

fn rule_summary(rule: &QueryComposerRule) -> String {
    let value = match rule.value_kind() {
        "range" => format!("{} – {}", rule.primary_value(), rule.secondary_value()),
        "fuzzy" => format!("{} ~{}", rule.primary_value(), rule.secondary_value()),
        _ => rule.primary_value(),
    };
    format!("{} · {value}", field_label(&rule.field))
}

fn field_label(field: &str) -> String {
    let key = match field {
        "" => TextKey::AllFields,
        "title" => TextKey::Title,
        "type" | "media_kind" => TextKey::Type,
        "year" => TextKey::Year,
        "genre" => TextKey::Genre,
        "tag" => TextKey::Tags,
        "collection" => TextKey::Collection,
        "studio" => TextKey::Studio,
        "music_label" => TextKey::MusicLabels,
        "network" => TextKey::Network,
        "actor" => TextKey::Actor,
        "director" => TextKey::Director,
        "writer" => TextKey::Writer,
        "language" => TextKey::Language,
        "subtitle_language" => TextKey::SubtitleLanguage,
        "content_rating" => TextKey::ContentRatings,
        "show_title" => TextKey::ShowTitle,
        "library_name" => TextKey::Library,
        "content_kind" => TextKey::ContentKindLabel,
        "sub_kind" => TextKey::SubKindLabel,
        "has_commentary" => TextKey::HasCommentary,
        _ => return field.to_owned(),
    };
    tr(key).to_owned()
}

const fn group_kind_value(kind: QueryGroupKind) -> &'static str {
    match kind {
        QueryGroupKind::All => "all",
        QueryGroupKind::Any => "any",
        QueryGroupKind::None => "none",
    }
}

fn parse_group_kind(value: &str) -> QueryGroupKind {
    match value {
        "all" => QueryGroupKind::All,
        "none" => QueryGroupKind::None,
        _ => QueryGroupKind::Any,
    }
}

fn group_label(kind: QueryGroupKind) -> &'static str {
    match kind {
        QueryGroupKind::All => tr(TextKey::MatchAll),
        QueryGroupKind::Any => tr(TextKey::MatchAny),
        QueryGroupKind::None => tr(TextKey::ExcludeAny),
    }
}

fn group_help(kind: QueryGroupKind) -> &'static str {
    match kind {
        QueryGroupKind::All => tr(TextKey::MatchAllHelp),
        QueryGroupKind::Any => tr(TextKey::MatchAnyHelp),
        QueryGroupKind::None => tr(TextKey::ExcludeAnyHelp),
    }
}

fn search_result_total(results: &SearchResults) -> u32 {
    results.movies.total_count
        + results.shows.total_count
        + results.seasons.total_count
        + results.episodes.total_count
        + results.artists.total_count
        + results.music_videos.total_count
        + results.other_videos.total_count
        + results.songs.total_count
        + results.images.total_count
        + results.remote_streams.total_count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn return_path_is_limited_to_global_search() {
        assert_eq!(
            safe_search_return_path("/search?query=genre%3Adrama"),
            "/search?query=genre%3Adrama"
        );
        assert_eq!(safe_search_return_path("//example.test"), "/search");
        assert_eq!(safe_search_return_path("/searching"), "/search");
    }

    #[test]
    fn preview_total_includes_every_search_section() {
        let mut results = SearchResults::default();
        results.movies.total_count = 2;
        results.songs.total_count = 3;
        results.remote_streams.total_count = 5;

        assert_eq!(search_result_total(&results), 10);
    }
}
