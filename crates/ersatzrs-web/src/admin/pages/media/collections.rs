//! Manual collection list and editor — ports `Pages/ManualCollections.razor`
//! and `CollectionEditor.razor`. The editor page offers autocomplete-backed
//! member management (maps to `ErsatzTV`'s `MudAutocomplete`).
use crate::admin::ui::primitives::pagination::page_size_limit;
use leptos::prelude::*;
use leptos_router::hooks::{use_params_map, use_query_map};

use crate::admin::i18n::{delete_failed, media_browser_error_message, save_failed, tr, TextKey};
use crate::admin::server_fns::media_browser::{
    add_item_to_collection, delete_collection, get_collection, list_collection_members,
    list_collections, remove_item_from_collection, save_collection, CollectionForm,
    CollectionListRow, CollectionMember,
};
use crate::admin::ui::primitives::page_header::{PageAction, PageHeader};
use crate::admin::ui::primitives::{
    dialog::DeleteDialog,
    form::TextField,
    icon::{icon_style, IconName},
    pagination::PaginationControls,
    resource_state::{admin_loading_resource_view, admin_result_view},
    toast::{load_failed_message, load_or_notify, use_toast, ToastSeverity},
};

// collections.module.css is shared with multi_collections.rs and
// smart_collections.rs; classes unused in this file's per-invocation
// generated constants are used there.
use crate::admin::ui::primitives::button::CancelLink;
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

// ---------------------------------------------------------------------------
// List page
// ---------------------------------------------------------------------------

#[component]
pub fn CollectionsPage() -> impl IntoView {
    let initial = Resource::new_blocking(|| (), |()| list_collections(String::new(), 0, 10));

    view! {
        <Suspense fallback=admin_loading_resource_view>
            {move || {
                initial.get().map(|result| admin_result_view(result, |(initial_rows, initial_total)| {
                    view! {
                        <CollectionsIsland
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
fn CollectionsIsland(initial_rows: Vec<CollectionListRow>, initial_total: u64) -> impl IntoView {
    let search = RwSignal::new(String::new());
    let page = RwSignal::new(0u32);
    let page_size = RwSignal::new(10u32);
    let toast = use_toast();

    let current_data = RwSignal::new((initial_rows, initial_total));
    let data: LocalResource<(Vec<CollectionListRow>, u64)> = LocalResource::new(move || {
        let q = search.get();
        let p = page.get();
        let size = page_size.get();
        async move { load_or_notify(list_collections(q, p, size).await, toast) }
    });
    Effect::new(move |_| {
        if let Some(next_data) = data.get() {
            current_data.set(next_data);
        }
    });

    let delete_target: RwSignal<Option<CollectionListRow>> = RwSignal::new(None);
    let delete_open = RwSignal::new(false);
    let delete_action = Action::new(|id: &i64| delete_collection(*id));

    Effect::new(move |_| {
        if let Some(res) = delete_action.value().get() {
            match res {
                Ok(()) => {
                    toast.push(tr(TextKey::CollectionDeleted), ToastSeverity::Success);
                    data.refetch();
                }
                Err(e) => toast.push(
                    delete_failed(media_browser_error_message(&e)),
                    ToastSeverity::Error,
                ),
            }
            delete_open.set(false);
        }
    });

    view! {
        <div class=style::page>
            <section class=style::content>
                <PageHeader title=tr(TextKey::ManualCollections)>
                    <PageAction href="/media/collections/add" label=tr(TextKey::AddCollection)/>
                </PageHeader>

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
                                        placeholder=tr(TextKey::SearchForManualCollections)
                                        prop:value=move || search.get()
                                        on:input=move |e| {
                                            search.set(event_target_value(&e));
                                            page.set(0);
                                        }
                                    />
                                </label>
                            </div>
                            <table>
                                <tbody>
                                    {rows.into_iter().map(|row| {
                                        let id = row.id;
                                        let name = row.name.clone();
                                        let row_for_delete = row.clone();
                                        view! {
                                            <tr>
                                                <td>
                                                    <a href=format!("/media/collections/{id}")>
                                                        {name}
                                                    </a>
                                                </td>
                                                <td>
                                                    <div class=style::row_actions>
                                                        <a
                                                            class=style::icon_button
                                                            href=format!("/media/collections/{id}")
                                                            title=tr(TextKey::EditCollection)
                                                            aria-label=tr(TextKey::EditCollection)
                                                        >
                                                            <span style=icon_style(IconName::Edit, "1.25rem") aria-hidden="true"></span>
                                                        </a>
                                                        <button
                                                            type="button"
                                                            class=style::icon_button
                                                            title=tr(TextKey::DeleteCollection)
                                                            aria-label=tr(TextKey::DeleteCollection)
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
                                        <td colspan="2" class=style::empty_cell>{tr(TextKey::NoCollections)}</td>
                                    </tr>
                                </tbody>
                            </table>
                            <PaginationControls
                                page=Signal::derive(move || page.get())
                                total=Signal::derive(move || Some(total))
                                page_size=Signal::derive(move || page_size.get())
                                page_size_options=Some(vec![10, 25, 50])
                                on_page_size_change=Callback::new(move |value: u32| {
                                    page_size.set(value);
                                    page.set(0);
                                })
                                on_previous=Callback::new(move |()| {
                                    page.update(|value| *value = value.saturating_sub(1));
                                })
                                on_next=Callback::new(move |()| {
                                    let size = page_size_limit(page_size.get());
                                    let max_page = u32::try_from(total.div_ceil(size))
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
                dialog_id="collection-delete-dialog"
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
pub fn CollectionEditorPage() -> impl IntoView {
    let params = use_params_map();
    let collection_id = params.with_untracked(|p| {
        p.get("id")
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or_default()
    });
    let query = use_query_map();
    let save_error =
        query.with_untracked(|params| params.get("error").unwrap_or_default()) == "save";

    view! { <CollectionEditorIsland collection_id save_error/> }
}

#[island]
fn CollectionEditorIsland(collection_id: i64, save_error: bool) -> impl IntoView {
    let default_name = tr(TextKey::NewCollectionTitle).to_string();
    let toast = use_toast();
    // `/media/collections/add` has no `:id` segment, so id 0 means "new" and
    // must not be queried — the application layer rejects it as invalid.
    //
    // The failure travels in the resource value, not through the toast channel:
    // this loader runs during SSR where `ToastStore::push` is inert by
    // construction (hydration-lint rule-3). The island keeps ownership of the
    // async boundary so the `<form>` stays server-rendered for the no-JS
    // fallback; `load_error` below is read reactively, so a failed load is
    // already visible and non-submittable in the SSR HTML.
    let initial: Resource<Result<CollectionForm, String>> = Resource::new_blocking(
        move || collection_id,
        move |id| {
            let default_name = default_name.clone();
            async move {
                match (id != 0).then_some(id) {
                    Some(id) => get_collection(id)
                        .await
                        .map(Option::unwrap_or_default)
                        .map_err(load_failed_message),
                    None => Ok(CollectionForm {
                        name: default_name,
                        ..CollectionForm::default()
                    }),
                }
            }
        },
    );
    let load_error = Signal::derive(move || initial.get().and_then(Result::err));

    let name = RwSignal::new(String::new());
    let id_signal: RwSignal<i64> = RwSignal::new(0);
    let hydrated = RwSignal::new(false);
    Effect::new(move |_| {
        if let Some(Ok(f)) = initial.get() {
            id_signal.set(f.id);
            name.set(f.name);
        }
        hydrated.set(true);
    });

    let name_err: RwSignal<Option<String>> = RwSignal::new(None);
    if save_error {
        name_err.set(Some(save_failed(media_browser_error_message("save"))));
    }
    let name_err_signal: Signal<Option<String>> = name_err.into();

    let save_action = Action::new(|form: &CollectionForm| {
        let form = form.clone();
        async move { save_collection(form).await }
    });

    Effect::new(move |_| {
        if let Some(res) = save_action.value().get() {
            match res {
                Ok(new_id) => {
                    toast.push(tr(TextKey::CollectionSaved), ToastSeverity::Success);
                    if id_signal.get_untracked() == 0 {
                        set_browser_location("/media/collections");
                    } else {
                        set_browser_location(&format!("/media/collections/{new_id}"));
                    }
                    id_signal.set(new_id);
                }
                Err(e) => toast.push(
                    save_failed(media_browser_error_message(&e)),
                    ToastSeverity::Error,
                ),
            }
        }
    });

    let on_save = move |()| {
        let trimmed = name.get().trim().to_string();
        if trimmed.is_empty() {
            name_err.set(Some(tr(TextKey::CollectionNameRequired).into()));
            return;
        }
        if trimmed.chars().count() > 50 {
            name_err.set(Some(media_browser_error_message(
                "Collection name must be 50 characters or less",
            )));
            return;
        }
        name_err.set(None);
        save_action.dispatch(CollectionForm {
            id: id_signal.get_untracked(),
            name: trimmed,
            query: String::new(),
        });
    };

    view! {
        <form
            method="post"
            action="/admin-api/collections/save"
            class=style::page
            data-collection-editor-hydrated=move || if hydrated.get() { "true" } else { "false" }
            on:submit=move |event| {
                event.prevent_default();
                on_save(());
            }
        >
            <input type="hidden" name="id" value=move || id_signal.get().to_string()/>
            <div class=style::editor_action_bar>
                <button
                    type="submit"
                    class=style::btn_add
                    disabled=move || save_action.pending().get() || load_error.get().is_some()
                >
                    <span>{move || if id_signal.get() == 0 { "+" } else { "✓" }}</span>
                    {move || if id_signal.get() == 0 {
                        tr(TextKey::AddCollection).to_string()
                    } else {
                        tr(TextKey::SaveCollection).to_string()
                    }}
                </button>
                <CancelLink href="/media/collections"/>
            </div>

            <div class=style::editor>
                <h1 class=style::page_title>{tr(TextKey::Collection)}</h1>
                <p
                    class=style::load_error
                    style:display=move || if load_error.get().is_some() { "" } else { "none" }
                >
                    {move || load_error.get().unwrap_or_default()}
                </p>
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

// ---------------------------------------------------------------------------
// Autocomplete + member list (island)
// ---------------------------------------------------------------------------

#[island]
fn MembersSection(collection_id: i64) -> impl IntoView {
    use crate::admin::server_fns::media_browser::{suggest_media_items, MediaItemSuggestion};

    // The suggestion fetch below runs only after hydration, so it can keep
    // using the toast channel.
    let load_toast = use_toast();
    // This loader runs during SSR, where the toast channel is inert by
    // construction (hydration-lint rule-3), so the failure is carried in the
    // resource value and rendered by `admin_result_view` instead.
    let members: Resource<Result<Vec<CollectionMember>, String>> = Resource::new(
        move || collection_id,
        move |id| async move {
            list_collection_members(id)
                .await
                .map_err(load_failed_message)
        },
    );

    let query = RwSignal::new(String::new());
    let suggestions: RwSignal<Vec<MediaItemSuggestion>> = RwSignal::new(Vec::new());
    let show_suggest = RwSignal::new(false);

    // Debounced suggestion fetch: re-run whenever `query` changes.
    Effect::new(move |_| {
        let q = query.get();
        let trimmed = q.trim().to_string();
        if trimmed.is_empty() {
            suggestions.set(Vec::new());
            show_suggest.set(false);
            return;
        }
        show_suggest.set(true);
        leptos::task::spawn_local(async move {
            match suggest_media_items(trimmed).await {
                Ok(results) => suggestions.set(results),
                Err(error) => {
                    load_toast.push(load_failed_message(error), ToastSeverity::Error);
                }
            }
        });
    });

    let add_action = Action::new(|(cid, mid): &(i64, i64)| {
        let cid = *cid;
        let mid = *mid;
        async move { add_item_to_collection(cid, mid).await }
    });
    let remove_action = Action::new(|(cid, mid): &(i64, i64)| {
        let cid = *cid;
        let mid = *mid;
        async move { remove_item_from_collection(cid, mid).await }
    });

    Effect::new(move |_| {
        if add_action.value().get().is_some() {
            members.refetch();
            query.set(String::new());
            show_suggest.set(false);
        }
    });
    Effect::new(move |_| {
        if remove_action.value().get().is_some() {
            members.refetch();
        }
    });

    view! {
        <div>
            <h2 class=style::section_title>{tr(TextKey::Members)}</h2>

            <div class=style::autocomplete>
                <input
                    class=style::autocomplete_input
                    type="text"
                    placeholder=tr(TextKey::SearchMoviesOrShows)
                    prop:value=move || query.get()
                    on:input=move |e| query.set(event_target_value(&e))
                    on:focus=move |_| {
                        if !query.get().trim().is_empty() {
                            show_suggest.set(true);
                        }
                    }
                />
                <ul
                    class=style::autocomplete_list
                    style:display=move || {
                        if show_suggest.get() && !suggestions.get().is_empty() { "" } else { "none" }
                    }
                >
                    {move || suggestions.get().into_iter().map(|s| {
                        let title = s.title.clone();
                        let kind = s.kind.clone();
                        let item_id = s.id;
                        view! {
                            <li
                                class=style::autocomplete_item
                                on:mousedown=move |e| {
                                    e.prevent_default();
                                    add_action.dispatch((collection_id, item_id));
                                }
                            >
                                <span class=style::kind_badge>{kind}</span>
                                <span>{title}</span>
                            </li>
                        }
                    }).collect_view()}
                </ul>
            </div>

            <Suspense fallback=admin_loading_resource_view>
                {move || {
                    members.get().map(|result| admin_result_view(result, |rows| {
                        if rows.is_empty() {
                            return view! {
                                <p class=style::empty>{tr(TextKey::NoMembers)}</p>
                            }
                            .into_any();
                        }
                        view! {
                            <ul class=style::members_list>
                                {rows.into_iter().map(|m| {
                                    let mid = m.media_item_id;
                                    let kind = m.kind.clone();
                                    let title = m.title.clone();
                                    view! {
                                        <li class=style::member>
                                            <span class=style::kind_badge>{kind}</span>
                                            <span class=style::member_title>{title}</span>
                                            <button
                                                class=style::btn_remove
                                                on:click=move |_| {
                                                    remove_action.dispatch((collection_id, mid));
                                                }
                                            >
                                                {tr(TextKey::Remove)}
                                            </button>
                                        </li>
                                    }
                                }).collect_view()}
                            </ul>
                        }
                        .into_any()
                    }))
                }}
            </Suspense>
        </div>
    }
}
