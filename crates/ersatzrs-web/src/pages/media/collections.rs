//! Manual collection list and editor — ports `Pages/ManualCollections.razor`
//! and `CollectionEditor.razor`. The editor page offers autocomplete-backed
//! member management (maps to `ErsatzTV`'s `MudAutocomplete`).
#![allow(clippy::too_many_lines, dead_code)]
use leptos::prelude::*;
use leptos_router::hooks::{use_navigate, use_params_map};
use leptos_router::NavigateOptions;

use crate::server::media_browser::{
    add_item_to_collection, delete_collection, get_collection, list_collection_members,
    list_collections, remove_item_from_collection, save_collection, CollectionForm,
    CollectionListRow, CollectionMember,
};
use crate::ui::primitives::{
    button::{Button, ButtonVariant, LinkButton},
    dialog::DeleteDialog,
    form::TextField,
    toast::{use_toast, ToastSeverity},
};

stylance::import_crate_style!(style, "src/pages/media/collections.module.css");

const PAGE_SIZE: u32 = 25;

// ---------------------------------------------------------------------------
// List page
// ---------------------------------------------------------------------------

#[component]
pub fn CollectionsPage() -> impl IntoView {
    let search = RwSignal::new(String::new());
    let page = RwSignal::new(0u32);

    let data: Resource<(Vec<CollectionListRow>, u64)> = Resource::new(
        move || (search.get(), page.get()),
        move |(q, p)| async move { list_collections(q, p, PAGE_SIZE).await.unwrap_or_default() },
    );

    let delete_target: RwSignal<Option<CollectionListRow>> = RwSignal::new(None);
    let delete_open = RwSignal::new(false);
    let delete_action = Action::new(|id: &i64| delete_collection(*id));

    let toast = use_toast();
    Effect::new(move |_| {
        if let Some(res) = delete_action.value().get() {
            match res {
                Ok(()) => {
                    toast.push("Collection deleted.", ToastSeverity::Success);
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
                <h1 class=style::page_title>"Collections"</h1>
                <input
                    class=style::search
                    type="search"
                    placeholder="Search…"
                    prop:value=move || search.get()
                    on:input=move |e| search.set(event_target_value(&e))
                />
                <a href="/media/collections/add" class=style::btn_add>"+ Add Collection"</a>
            </div>

            <Suspense fallback=move || view! { <p class=style::empty>"Loading…"</p> }>
                {move || {
                    data.get().map(|(rows, _)| {
                        if rows.is_empty() {
                            return view! {
                                <p class=style::empty>"No collections yet."</p>
                            }
                            .into_any();
                        }
                        view! {
                            <div class=style::table_wrap>
                                <table>
                                    <thead>
                                        <tr>
                                            <th>"Name"</th>
                                            <th/>
                                        </tr>
                                    </thead>
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
                                                                class=style::btn_edit
                                                                href=format!("/media/collections/{id}")
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

            {move || {
                delete_target.get().map(|row| {
                    let id = row.id;
                    let name = row.name.clone();
                    view! {
                        <DeleteDialog
                            open=delete_open
                            entity_name=name
                            on_confirm=Callback::new(move |confirmed: bool| {
                                if confirmed {
                                    delete_action.dispatch(id);
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

// ---------------------------------------------------------------------------
// Editor page
// ---------------------------------------------------------------------------

#[component]
pub fn CollectionEditorPage() -> impl IntoView {
    let params = use_params_map();
    let collection_id = move || params.with(|p| p.get("id").and_then(|s| s.parse::<i64>().ok()));

    let initial: Resource<CollectionForm> = Resource::new(collection_id, |id| async move {
        match id {
            Some(id) => get_collection(id)
                .await
                .unwrap_or_default()
                .unwrap_or_default(),
            None => CollectionForm::default(),
        }
    });

    let name = RwSignal::new(String::new());
    let id_signal: RwSignal<i64> = RwSignal::new(0);
    Effect::new(move |_| {
        if let Some(f) = initial.get() {
            id_signal.set(f.id);
            name.set(f.name);
        }
    });

    let name_err: RwSignal<Option<String>> = RwSignal::new(None);
    let name_err_signal: Signal<Option<String>> = name_err.into();

    let save_action = Action::new(|form: &CollectionForm| {
        let form = form.clone();
        async move { save_collection(form).await }
    });

    let toast = use_toast();
    let navigate = use_navigate();

    Effect::new(move |_| {
        if let Some(res) = save_action.value().get() {
            match res {
                Ok(new_id) => {
                    toast.push("Collection saved.", ToastSeverity::Success);
                    if id_signal.get_untracked() == 0 {
                        let target = format!("/media/collections/{new_id}");
                        navigate(&target, NavigateOptions::default());
                    }
                    id_signal.set(new_id);
                }
                Err(e) => toast.push(format!("Save failed: {e}"), ToastSeverity::Error),
            }
        }
    });

    let on_save = move |()| {
        let trimmed = name.get().trim().to_string();
        if trimmed.is_empty() {
            name_err.set(Some("Name is required".into()));
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
        <div class=style::page>
            <h1 class=style::page_title>
                {move || if id_signal.get() == 0 { "New Collection".to_string() } else { "Edit Collection".to_string() }}
            </h1>
            <div class=style::editor>
                <TextField label="Name" value=name error=Some(name_err_signal)/>

                <div class=style::form_actions>
                    <Button
                        variant=ButtonVariant::Primary
                        on_click=Some(Callback::new(on_save))
                    >
                        "Save"
                    </Button>
                    <LinkButton href="/media/collections".to_string() variant=ButtonVariant::Text>
                        "Back"
                    </LinkButton>
                </div>

                {move || {
                    let cid = id_signal.get();
                    (cid != 0).then(|| view! { <MembersSection collection_id=cid/> })
                }}
            </div>
        </div>
    }
}

// ---------------------------------------------------------------------------
// Autocomplete + member list (island)
// ---------------------------------------------------------------------------

#[island]
fn MembersSection(collection_id: i64) -> impl IntoView {
    use crate::server::media_browser::{suggest_media_items, MediaItemSuggestion};

    let members: Resource<Vec<CollectionMember>> = Resource::new(
        move || collection_id,
        move |id| async move { list_collection_members(id).await.unwrap_or_default() },
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
            let results = suggest_media_items(trimmed).await.unwrap_or_default();
            suggestions.set(results);
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
            <h2 class=style::section_title>"Members"</h2>

            <div class=style::autocomplete>
                <input
                    class=style::autocomplete_input
                    type="text"
                    placeholder="Search movies or shows…"
                    prop:value=move || query.get()
                    on:input=move |e| query.set(event_target_value(&e))
                    on:focus=move |_| {
                        if !query.get().trim().is_empty() {
                            show_suggest.set(true);
                        }
                    }
                />
                <Show when=move || show_suggest.get() && !suggestions.get().is_empty()>
                    <ul class=style::autocomplete_list>
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
                </Show>
            </div>

            <Suspense fallback=move || view! { <p class=style::empty>"Loading members…"</p> }>
                {move || {
                    members.get().map(|rows| {
                        if rows.is_empty() {
                            return view! {
                                <p class=style::empty>"No members yet."</p>
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
                                                "Remove"
                                            </button>
                                        </li>
                                    }
                                }).collect_view()}
                            </ul>
                        }
                        .into_any()
                    })
                }}
            </Suspense>
        </div>
    }
}
