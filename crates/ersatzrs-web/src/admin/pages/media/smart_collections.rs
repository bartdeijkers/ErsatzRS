//! Smart collection list and editor — ports `Pages/SmartCollections.razor`
//! and `SmartCollectionEditor.razor`. Smart collections store a search query
//! (Lucene in `ErsatzTV`, parity-pending in `ErsatzRS`).
#![allow(clippy::too_many_lines, dead_code)]
use leptos::prelude::*;
use leptos_router::hooks::{use_params_map, use_query_map};

use crate::admin::i18n::{
    delete_failed, media_browser_error_message, page_of, range_of, save_failed, tr, TextKey,
};
use crate::admin::server_fns::media_browser::{
    delete_smart_collection, get_smart_collection, list_smart_collections, save_smart_collection,
    CollectionForm, CollectionListRow,
};
use crate::admin::ui::primitives::{
    dialog::DeleteDialog,
    form::TextField,
    icon::{icon_style, IconName},
    toast::{use_toast, ToastSeverity},
};

stylance::import_crate_style!(style, "src/admin/pages/media/collections.module.css");

#[cfg(target_arch = "wasm32")]
fn set_browser_location(path: &str) {
    if let Some(window) = web_sys::window() {
        let _ = window.location().set_href(path);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn set_browser_location(_path: &str) {}

// ---------------------------------------------------------------------------
// List page
// ---------------------------------------------------------------------------

#[component]
pub fn SmartCollectionsPage() -> impl IntoView {
    let initial: Resource<(Vec<CollectionListRow>, u64)> = Resource::new_blocking(
        || (),
        |()| async {
            list_smart_collections(String::new(), 0, 10)
                .await
                .unwrap_or_default()
        },
    );

    view! {
        <Suspense fallback=move || view! { <p class=style::empty_cell>{tr(TextKey::Loading)}</p> }>
            {move || {
                initial.get().map(|(initial_rows, initial_total)| {
                    view! {
                        <SmartCollectionsIsland
                            initial_rows=initial_rows
                            initial_total=initial_total
                        />
                    }
                })
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

    let current_data = RwSignal::new((initial_rows, initial_total));
    let data: LocalResource<(Vec<CollectionListRow>, u64)> = LocalResource::new(move || {
        let q = search.get();
        let p = page.get();
        let size = page_size.get();
        async move { list_smart_collections(q, p, size).await.unwrap_or_default() }
    });
    Effect::new(move |_| {
        if let Some(next_data) = data.get() {
            current_data.set(next_data);
        }
    });

    let delete_target: RwSignal<Option<CollectionListRow>> = RwSignal::new(None);
    let delete_open = RwSignal::new(false);
    let delete_action = Action::new(|id: &i64| delete_smart_collection(*id));

    let toast = use_toast();
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
                <h1 class=style::page_title>{tr(TextKey::SmartCollections)}</h1>
                <div class=style::divider/>

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
                            <div class=style::pager>
                                <span>{tr(TextKey::RowsPerPage)}</span>
                                <select
                                    class=style::page_size
                                    prop:value=move || page_size.get().to_string()
                                    on:change=move |event| {
                                        if let Ok(value) = event_target_value(&event).parse::<u32>() {
                                            page_size.set(value.max(1));
                                            page.set(0);
                                        }
                                    }
                                >
                                    <option value="10" selected=move || page_size.get() == 10>"10"</option>
                                    <option value="25" selected=move || page_size.get() == 25>"25"</option>
                                    <option value="50" selected=move || page_size.get() == 50>"50"</option>
                                </select>
                                <span class=style::pager_status>
                                    {range_of(
                                        u32::try_from(range_start).unwrap_or(u32::MAX),
                                        u32::try_from(range_end).unwrap_or(u32::MAX),
                                        total,
                                    )}
                                </span>
                                <span class=style::pager_status>{page_of(current_page + 1, total_pages)}</span>
                                <div class=style::pager_buttons>
                                    <button
                                        type="button"
                                        class=style::pager_btn
                                        disabled={current_page == 0}
                                        on:click=move |_| page.update(|p| *p = p.saturating_sub(1))
                                    >
                                        "‹"
                                    </button>
                                    <button
                                        type="button"
                                        class=style::pager_btn
                                        disabled={current_page + 1 >= total_pages}
                                        on:click=move |_| page.update(|p| *p = (*p + 1).min(total_pages.saturating_sub(1)))
                                    >
                                        "›"
                                    </button>
                                </div>
                            </div>
                        </div>
                    }
                    .into_any()
                }}
            </section>

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
    let collection_id = params.with_untracked(|p| {
        p.get("id")
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or_default()
    });
    let query = use_query_map();
    let save_error =
        query.with_untracked(|params| params.get("error").unwrap_or_default()) == "save";

    view! { <SmartCollectionEditorIsland collection_id save_error/> }
}

#[island]
fn SmartCollectionEditorIsland(collection_id: i64, save_error: bool) -> impl IntoView {
    let initial: Resource<CollectionForm> = Resource::new_blocking(
        move || collection_id,
        |id| async move {
            get_smart_collection(id)
                .await
                .unwrap_or_default()
                .unwrap_or_default()
        },
    );

    let name = RwSignal::new(String::new());
    let query = RwSignal::new(String::new());
    let id_signal: RwSignal<i64> = RwSignal::new(0);
    let hydrated = RwSignal::new(false);
    Effect::new(move |_| {
        if let Some(f) = initial.get() {
            id_signal.set(f.id);
            name.set(f.name);
            query.set(f.query);
            hydrated.set(true);
        }
    });

    let name_err: RwSignal<Option<String>> = RwSignal::new(None);
    if save_error {
        name_err.set(Some(save_failed(media_browser_error_message("save"))));
    }
    let name_err_signal: Signal<Option<String>> = name_err.into();

    let save_action = Action::new(|form: &CollectionForm| {
        let form = form.clone();
        async move { save_smart_collection(form).await }
    });

    let toast = use_toast();

    Effect::new(move |_| {
        if let Some(res) = save_action.value().get() {
            match res {
                Ok(_) => {
                    toast.push(tr(TextKey::SmartCollectionSaved), ToastSeverity::Success);
                    set_browser_location("/media/smart-collections");
                }
                Err(e) => toast.push(
                    save_failed(media_browser_error_message(e)),
                    ToastSeverity::Error,
                ),
            }
        }
    });

    let on_save = move |()| {
        let trimmed_name = name.get().trim().to_string();
        let trimmed_query = query.get().trim().to_string();
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
        save_action.dispatch(CollectionForm {
            id: id_signal.get_untracked(),
            name: trimmed_name,
            query: trimmed_query,
        });
    };

    view! {
        <form
            method="post"
            action="/api/smart-collections/save"
            class=style::page
            data-smart-collection-editor-hydrated=move || if hydrated.get() { "true" } else { "false" }
            on:submit=move |event| {
                event.prevent_default();
                on_save(());
            }
        >
            <input type="hidden" name="id" value=move || id_signal.get().to_string()/>
            <input type="hidden" name="query" value=move || query.get()/>
            <div class=style::editor_action_bar>
                <button
                    type="submit"
                    class=style::btn_add
                    disabled=move || save_action.pending().get()
                >
                    <span style=icon_style(IconName::Save, "1.125rem") aria-hidden="true"></span>
                    {tr(TextKey::SaveSmartCollection)}
                </button>
            </div>

            <div class=style::editor>
                <h1 class=style::page_title>{tr(TextKey::SmartCollection)}</h1>
                <TextField
                    label=tr(TextKey::Name)
                    value=name
                    name=Some("name")
                    required=true
                    max_length=Some(50)
                    error=Some(name_err_signal)
                />
            </div>
        </form>
    }
}
