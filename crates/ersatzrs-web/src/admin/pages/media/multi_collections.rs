//! Multi-collection list and editor. Ports `Pages/MultiCollections.razor`
//! and `Pages/MultiCollectionEditor.razor`.
#![allow(clippy::too_many_lines, dead_code)]

use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::admin::server_fns::media_browser::{
    delete_multi_collection, get_multi_collection, get_multi_collection_editor_options,
    get_multi_collections_page_size, list_multi_collections, save_multi_collection,
    MultiCollectionEditorOptions, MultiCollectionForm, MultiCollectionItemForm,
    MultiCollectionListRow, MultiCollectionSelectOption,
};
use crate::admin::ui::primitives::{
    button::{Button, ButtonVariant, LinkButton},
    dialog::DeleteDialog,
    form::TextField,
    toast::{use_toast, ToastSeverity},
};

stylance::import_crate_style!(style, "src/admin/pages/media/collections.module.css");

const COLLECTION_TYPE_SMART_COLLECTION: i32 = 5;
const PLAYBACK_ORDER_CHRONOLOGICAL: i32 = 1;

#[cfg(target_arch = "wasm32")]
fn set_browser_location(path: &str) {
    if let Some(window) = web_sys::window() {
        let _ = window.location().set_href(path);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn set_browser_location(_path: &str) {}

#[component]
pub fn MultiCollectionsPage() -> impl IntoView {
    view! { <MultiCollectionsIsland/> }
}

#[island]
fn MultiCollectionsIsland() -> impl IntoView {
    let search = RwSignal::new(String::new());
    let page = RwSignal::new(0u32);
    let page_size = RwSignal::new(10u32);

    let saved_page_size = Resource::new(
        || (),
        |()| async { get_multi_collections_page_size().await.unwrap_or(10) },
    );
    Effect::new(move |_| {
        if let Some(saved) = saved_page_size.get() {
            page_size.set(saved);
        }
    });

    let data: Resource<(Vec<MultiCollectionListRow>, u64)> = Resource::new(
        move || (search.get(), page.get(), page_size.get()),
        move |(q, p, size)| async move { list_multi_collections(q, p, size).await.unwrap_or_default() },
    );

    let delete_target: RwSignal<Option<MultiCollectionListRow>> = RwSignal::new(None);
    let delete_open = RwSignal::new(false);
    let delete_action = Action::new(|id: &i64| delete_multi_collection(*id));

    let toast = use_toast();
    Effect::new(move |_| {
        if let Some(res) = delete_action.value().get() {
            match res {
                Ok(()) => {
                    toast.push("Multi collection deleted.", ToastSeverity::Success);
                    data.refetch();
                }
                Err(error) => toast.push(format!("Delete failed: {error}"), ToastSeverity::Error),
            }
            delete_open.set(false);
        }
    });

    view! {
        <div class=style::page>
            <div class=style::toolbar>
                <h1 class=style::page_title>"Multi Collections"</h1>
                <input
                    class=style::search
                    type="search"
                    placeholder="Search for multi collections"
                    prop:value=move || search.get()
                    on:input=move |event| {
                        search.set(event_target_value(&event));
                        page.set(0);
                    }
                />
                <a href="/media/multi-collections/add" class=style::btn_add>
                    "+ Add Multi Collection"
                </a>
            </div>

            <Suspense fallback=move || view! { <p class=style::empty>"Loading…"</p> }>
                {move || {
                    data.get().map(|(rows, total)| {
                        let size = u64::from(page_size.get().max(1));
                        let total_pages = u32::try_from(total.div_ceil(size))
                            .unwrap_or(u32::MAX)
                            .max(1);

                        let rows_view = if rows.is_empty() {
                            view! { <p class=style::empty>"No multi collections yet."</p> }
                                .into_any()
                        } else {
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
                                                        <td>{name}</td>
                                                        <td>
                                                            <div class=style::row_actions>
                                                                <a
                                                                    class=style::btn_edit
                                                                    href=format!("/media/multi-collections/{id}/edit")
                                                                >
                                                                    "Edit"
                                                                </a>
                                                                <button
                                                                    type="button"
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
                        };

                        view! {
                            {rows_view}
                            <div class=style::pager>
                                <button
                                    type="button"
                                    class=style::pager_btn
                                    disabled=move || page.get() == 0
                                    on:click=move |_| {
                                        page.update(|page| *page = page.saturating_sub(1));
                                    }
                                >
                                    "Previous"
                                </button>
                                <span class=style::pager_status>
                                    {move || format!("Page {} of {}", page.get() + 1, total_pages)}
                                </span>
                                <button
                                    type="button"
                                    class=style::pager_btn
                                    disabled=move || { page.get() + 1 >= total_pages }
                                    on:click=move |_| {
                                        let max_page = total_pages.saturating_sub(1);
                                        page.update(|page| *page = (*page + 1).min(max_page));
                                    }
                                >
                                    "Next"
                                </button>
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
                                    <option value="10">"10"</option>
                                    <option value="25">"25"</option>
                                    <option value="50">"50"</option>
                                    <option value="100">"100"</option>
                                </select>
                            </div>
                        }
                        .into_any()
                    })
                }}
            </Suspense>

            <DeleteDialog
                open=delete_open
                entity_name=Signal::derive(move || {
                    delete_target.get().map(|row| row.name).unwrap_or_default()
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

#[component]
pub fn MultiCollectionEditorPage() -> impl IntoView {
    let params = use_params_map();
    let id = params.with(|p| p.get("id").and_then(|s| s.parse::<i64>().ok()).unwrap_or(0));
    view! { <MultiCollectionEditorIsland id/> }
}

#[island]
fn MultiCollectionEditorIsland(id: i64) -> impl IntoView {
    let route_id = move || (id != 0).then_some(id);

    let initial: Resource<MultiCollectionForm> = Resource::new(route_id, |id| async move {
        match id {
            Some(id) => get_multi_collection(id)
                .await
                .unwrap_or_default()
                .unwrap_or_default(),
            None => MultiCollectionForm {
                name: "New Multi Collection".to_string(),
                ..MultiCollectionForm::default()
            },
        }
    });
    let options: Resource<MultiCollectionEditorOptions> = Resource::new(
        || (),
        |()| async {
            get_multi_collection_editor_options()
                .await
                .unwrap_or_default()
        },
    );

    let id_signal = RwSignal::new(0_i64);
    let name = RwSignal::new(String::new());
    let items = RwSignal::new(Vec::<MultiCollectionItemForm>::new());
    Effect::new(move |_| {
        if let Some(form) = initial.get() {
            id_signal.set(form.id);
            name.set(form.name);
            items.set(form.items);
        }
    });

    let selected_collection = RwSignal::new(0_i64);
    let selected_smart_collection = RwSignal::new(0_i64);
    let name_err: RwSignal<Option<String>> = RwSignal::new(None);
    let name_err_signal: Signal<Option<String>> = name_err.into();

    let save_action = Action::new(|form: &MultiCollectionForm| {
        let form = form.clone();
        async move { save_multi_collection(form).await }
    });

    let toast = use_toast();
    Effect::new(move |_| {
        if let Some(result) = save_action.value().get() {
            match result {
                Ok(_) => {
                    toast.push("Multi collection saved.", ToastSeverity::Success);
                    set_browser_location("/media/multi-collections");
                }
                Err(error) => toast.push(format!("Save failed: {error}"), ToastSeverity::Error),
            }
        }
    });

    let add_collection = move |option: MultiCollectionSelectOption| {
        items.update(|items| {
            let exists = items
                .iter()
                .any(|item| item.collection_id == Some(option.id));
            if !exists {
                items.push(MultiCollectionItemForm {
                    collection_id: Some(option.id),
                    smart_collection_id: None,
                    name: option.name,
                    collection_type: option.collection_type,
                    use_custom_playback_order: option.use_custom_playback_order,
                    schedule_as_group: false,
                    playback_order: PLAYBACK_ORDER_CHRONOLOGICAL,
                });
            }
        });
        selected_collection.set(0);
    };

    let add_smart_collection = move |option: MultiCollectionSelectOption| {
        items.update(|items| {
            let exists = items
                .iter()
                .any(|item| item.smart_collection_id == Some(option.id));
            if !exists {
                items.push(MultiCollectionItemForm {
                    collection_id: None,
                    smart_collection_id: Some(option.id),
                    name: option.name,
                    collection_type: COLLECTION_TYPE_SMART_COLLECTION,
                    use_custom_playback_order: false,
                    schedule_as_group: false,
                    playback_order: PLAYBACK_ORDER_CHRONOLOGICAL,
                });
            }
        });
        selected_smart_collection.set(0);
    };

    let on_save = Callback::new(move |()| {
        let trimmed = name.get().trim().to_string();
        if trimmed.is_empty() {
            name_err.set(Some("Multi-collection name is required!".to_string()));
            return;
        }
        if trimmed.chars().count() > 50 {
            name_err.set(Some(
                "Multi-collection name cannot be longer than 50 characters.".to_string(),
            ));
            return;
        }
        name_err.set(None);
        save_action.dispatch(MultiCollectionForm {
            id: id_signal.get_untracked(),
            name: trimmed,
            items: items.get_untracked(),
        });
    });

    view! {
        <div class=style::page>
            <div class=style::toolbar>
                <h1 class=style::page_title>"Multi Collection"</h1>
                <Button
                    variant=ButtonVariant::Primary
                    on_click=Some(on_save.clone())
                >
                    {move || if id_signal.get() == 0 {
                        "Add Multi Collection"
                    } else {
                        "Save Multi Collection"
                    }}
                </Button>
            </div>

            <div class=style::editor>
                <TextField label="Name" value=name error=Some(name_err_signal)/>

                <Suspense fallback=move || view! { <p class=style::empty>"Loading…"</p> }>
                    {move || {
                        options.get().map(|options| {
                            view! {
                                <div class=style::field_group>
                                    <label class=style::field_label>"Collections"</label>
                                    <select
                                        class=style::select
                                        prop:value=move || selected_collection.get().to_string()
                                        on:change=move |event| {
                                            selected_collection.set(
                                                event_target_value(&event).parse::<i64>().unwrap_or_default(),
                                            );
                                        }
                                    >
                                        <option value="0">"Select collection"</option>
                                        {options.collections.iter().map(|option| {
                                            let id = option.id;
                                            let disabled = items.with(|items| {
                                                items.iter().any(|item| item.collection_id == Some(id))
                                            });
                                            view! {
                                                <option value=id.to_string() disabled=disabled>
                                                    {option.name.clone()}
                                                </option>
                                            }
                                        }).collect_view()}
                                    </select>
                                    <p class=style::help_text>
                                        "Disabled collections are already present in this multi collection"
                                    </p>
                                    <button
                                        type="button"
                                        class=style::btn_add
                                        disabled=move || selected_collection.get() == 0
                                        on:click={
                                            let options = options.collections.clone();
                                            move |_| {
                                                let id = selected_collection.get_untracked();
                                                if let Some(option) = options.iter().find(|option| option.id == id) {
                                                    add_collection(option.clone());
                                                }
                                            }
                                        }
                                    >
                                        "Add Collection"
                                    </button>
                                </div>

                                <div class=style::field_group>
                                    <label class=style::field_label>"Smart Collections"</label>
                                    <select
                                        class=style::select
                                        prop:value=move || selected_smart_collection.get().to_string()
                                        on:change=move |event| {
                                            selected_smart_collection.set(
                                                event_target_value(&event).parse::<i64>().unwrap_or_default(),
                                            );
                                        }
                                    >
                                        <option value="0">"Select smart collection"</option>
                                        {options.smart_collections.iter().map(|option| {
                                            let id = option.id;
                                            let disabled = items.with(|items| {
                                                items.iter().any(|item| item.smart_collection_id == Some(id))
                                            });
                                            view! {
                                                <option value=id.to_string() disabled=disabled>
                                                    {option.name.clone()}
                                                </option>
                                            }
                                        }).collect_view()}
                                    </select>
                                    <p class=style::help_text>
                                        "Disabled collections are already present in this multi collection"
                                    </p>
                                    <button
                                        type="button"
                                        class=style::btn_add
                                        disabled=move || selected_smart_collection.get() == 0
                                        on:click={
                                            let options = options.smart_collections.clone();
                                            move |_| {
                                                let id = selected_smart_collection.get_untracked();
                                                if let Some(option) = options.iter().find(|option| option.id == id) {
                                                    add_smart_collection(option.clone());
                                                }
                                            }
                                        }
                                    >
                                        "Add Smart Collection"
                                    </button>
                                </div>
                            }
                        })
                    }}
                </Suspense>

                <section>
                    <h2 class=style::section_title>
                        {move || format!("{} Items", name.get())}
                    </h2>
                    <MultiCollectionItemsTable items=items/>
                </section>

                <div class=style::form_actions>
                    <Button
                        variant=ButtonVariant::Primary
                        on_click=Some(on_save)
                    >
                        {move || if id_signal.get() == 0 {
                            "Add Multi Collection"
                        } else {
                            "Save Multi Collection"
                        }}
                    </Button>
                    <LinkButton
                        href="/media/multi-collections".to_string()
                        variant=ButtonVariant::Text
                    >
                        "Back"
                    </LinkButton>
                </div>
            </div>
        </div>
    }
}

#[component]
fn MultiCollectionItemsTable(items: RwSignal<Vec<MultiCollectionItemForm>>) -> impl IntoView {
    view! {
        <div class=style::table_wrap>
            <table>
                <thead>
                    <tr>
                        <th>"Collection"</th>
                        <th>"Schedule As Group"</th>
                        <th>"Playback Order"</th>
                        <th/>
                    </tr>
                </thead>
                <tbody>
                    {move || {
                        let mut rows: Vec<_> = items.get().into_iter().enumerate().collect();
                        rows.sort_by(|(_, a), (_, b)| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                        if rows.is_empty() {
                            return view! {
                                <tr>
                                    <td colspan="4">"No items yet."</td>
                                </tr>
                            }
                            .into_any();
                        }
                        rows.into_iter().map(|(index, item)| {
                            let checked = item.schedule_as_group;
                            let label = if item.schedule_as_group {
                                if item.use_custom_playback_order {
                                    "Custom"
                                } else {
                                    "Chronological"
                                }
                            } else {
                                ""
                            };
                            view! {
                                <tr>
                                    <td>{item.name}</td>
                                    <td>
                                        <input
                                            type="checkbox"
                                            checked=checked
                                            prop:checked=move || {
                                                items.with(|items| {
                                                    items.get(index).map(|item| item.schedule_as_group).unwrap_or(false)
                                                })
                                            }
                                            on:change=move |event| {
                                                let checked = event_target_checked(&event);
                                                items.update(|items| {
                                                    if let Some(item) = items.get_mut(index) {
                                                        item.schedule_as_group = checked;
                                                    }
                                                });
                                            }
                                        />
                                    </td>
                                    <td>{label}</td>
                                    <td>
                                        <div class=style::row_actions>
                                            <button
                                                type="button"
                                                class=style::btn_delete
                                                on:click=move |_| {
                                                    items.update(|items| {
                                                        if index < items.len() {
                                                            items.remove(index);
                                                        }
                                                    });
                                                }
                                            >
                                                "Delete"
                                            </button>
                                        </div>
                                    </td>
                                </tr>
                            }
                        }).collect_view().into_any()
                    }}
                </tbody>
            </table>
        </div>
    }
}
