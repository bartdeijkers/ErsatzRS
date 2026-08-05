//! Multi-collection list and editor. Ports `Pages/MultiCollections.razor`
//! and `Pages/MultiCollectionEditor.razor`.

use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::admin::i18n::{
    delete_failed, media_browser_error_message, page_of, save_failed, tr, TextKey,
};
use crate::admin::server_fns::media_browser::{
    delete_multi_collection, get_multi_collection, get_multi_collection_editor_options,
    get_multi_collections_page_size, list_multi_collections, save_multi_collection,
    MultiCollectionEditorOptions, MultiCollectionForm, MultiCollectionItemForm,
    MultiCollectionListRow, MultiCollectionSelectOption,
};
use crate::admin::ui::primitives::{
    dialog::DeleteDialog,
    form::TextField,
    icon::{icon_style, IconName},
    resource_state::{admin_loading_resource_view, admin_result_view},
    toast::{load_or_notify, use_toast, ToastSeverity},
};

// collections.module.css is shared with collections.rs and
// smart_collections.rs; classes unused in this file's per-invocation
// generated constants are used there.
stylance::import_crate_style!(
    #[allow(dead_code)]
    style,
    "src/admin/pages/media/collections.module.css"
);

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
    let initial = Resource::new_blocking(
        || (),
        |()| async {
            let page_size = get_multi_collections_page_size().await?;
            let rows = list_multi_collections(String::new(), 0, page_size).await?;
            Ok::<_, ServerFnError>((page_size, rows))
        },
    );

    view! {
        <Suspense fallback=admin_loading_resource_view>
            {move || {
                initial.get().map(|result| admin_result_view(result, |(initial_page_size, (initial_rows, initial_total))| {
                    view! {
                        <MultiCollectionsIsland
                            initial_page_size=initial_page_size
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
fn MultiCollectionsIsland(
    initial_page_size: u32,
    initial_rows: Vec<MultiCollectionListRow>,
    initial_total: u64,
) -> impl IntoView {
    let search = RwSignal::new(String::new());
    let page = RwSignal::new(0u32);
    let page_size = RwSignal::new(initial_page_size);
    let current_data = RwSignal::new((initial_rows, initial_total));
    let toast = use_toast();

    let data: LocalResource<(Vec<MultiCollectionListRow>, u64)> = LocalResource::new(move || {
        let q = search.get();
        let p = page.get();
        let size = page_size.get();
        async move { load_or_notify(list_multi_collections(q, p, size).await, toast) }
    });
    Effect::new(move |_| {
        if let Some(next_data) = data.get() {
            current_data.set(next_data);
        }
    });

    let delete_target: RwSignal<Option<MultiCollectionListRow>> = RwSignal::new(None);
    let delete_open = RwSignal::new(false);
    let delete_action = Action::new(|id: &i64| delete_multi_collection(*id));

    Effect::new(move |_| {
        if let Some(res) = delete_action.value().get() {
            match res {
                Ok(()) => {
                    toast.push(tr(TextKey::MultiCollectionDeleted), ToastSeverity::Success);
                    data.refetch();
                }
                Err(error) => toast.push(
                    delete_failed(media_browser_error_message(error)),
                    ToastSeverity::Error,
                ),
            }
            delete_open.set(false);
        }
    });

    view! {
        <div class=style::page>
            <div class=style::toolbar>
                <h1 class=style::page_title>{tr(TextKey::MultiCollections)}</h1>
                <input
                    class=style::search
                    type="search"
                    placeholder=tr(TextKey::SearchForMultiCollections)
                    prop:value=move || search.get()
                    on:input=move |event| {
                        search.set(event_target_value(&event));
                        page.set(0);
                    }
                />
                <a href="/media/multi-collections/add" class=style::btn_add>
                    {tr(TextKey::AddMultiCollection)}
                </a>
            </div>

            {move || {
                let (rows, total) = current_data.get();
                let size = u64::from(page_size.get().max(1));
                let total_pages = u32::try_from(total.div_ceil(size))
                    .unwrap_or(u32::MAX)
                    .max(1);

                let rows_view = if rows.is_empty() {
                    view! { <p class=style::empty>{tr(TextKey::NoMultiCollections)}</p> }
                        .into_any()
                } else {
                    view! {
                        <div class=style::table_wrap>
                            <table>
                                <thead>
                                    <tr>
                                        <th>{tr(TextKey::Name)}</th>
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
                                                            {tr(TextKey::Edit)}
                                                        </a>
                                                        <form method="post" action="/api/multi-collections/delete">
                                                            <input type="hidden" name="id" value=id.to_string()/>
                                                            <button
                                                                type="submit"
                                                                class=style::btn_delete
                                                                on:click=move |event| {
                                                                    event.prevent_default();
                                                                    delete_target.set(Some(row_for_delete.clone()));
                                                                    delete_open.set(true);
                                                                }
                                                            >
                                                                {tr(TextKey::Delete)}
                                                            </button>
                                                        </form>
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
                            {tr(TextKey::PreviousPage)}
                        </button>
                        <span class=style::pager_status>
                            {move || page_of(page.get() + 1, total_pages)}
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
                            {tr(TextKey::NextPage)}
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
            }}

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
    let default_name = tr(TextKey::NewMultiCollection).to_string();
    let fallback_form = load_multi_collection_no_script_form(id, &default_name);
    let initial = Resource::new_blocking(
        move || (id, default_name.clone()),
        move |(id, default_name)| async move {
            let form = if id == 0 {
                MultiCollectionForm {
                    name: default_name,
                    ..MultiCollectionForm::default()
                }
            } else {
                get_multi_collection(id).await?.unwrap_or_default()
            };
            let options = get_multi_collection_editor_options().await?;
            Ok::<_, ServerFnError>((form, options))
        },
    );

    view! {
        <style>".multi-collection-noscript { display: none; }"</style>
        <noscript>
            <style>".multi-collection-noscript { display: block; } .multi-collection-js { display: none; }"</style>
            <div
                class="multi-collection-noscript"
                inner_html=multi_collection_editor_fallback_html(fallback_form.as_ref())
            ></div>
        </noscript>
        <div class="multi-collection-js">
            <Suspense fallback=admin_loading_resource_view>
                {move || {
                    initial.get().map(|result| admin_result_view(result, |(initial_form, editor_options)| {
                        view! {
                            <MultiCollectionEditorIsland
                                initial_form=initial_form
                                editor_options=editor_options
                            />
                        }
                        .into_any()
                    }))
                }}
            </Suspense>
        </div>
    }
}

fn load_multi_collection_no_script_form(
    id: i64,
    default_name: &str,
) -> Result<MultiCollectionForm, String> {
    if id == 0 {
        return Ok(MultiCollectionForm {
            name: default_name.to_string(),
            ..MultiCollectionForm::default()
        });
    }

    #[cfg(feature = "ssr")]
    {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(async move { get_multi_collection(id).await })
                .map(Option::unwrap_or_default)
                .map_err(|error| error.to_string())
        })
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = id;
        Ok(MultiCollectionForm::default())
    }
}

fn multi_collection_editor_fallback_html(form: Result<&MultiCollectionForm, &String>) -> String {
    let form = match form {
        Ok(form) => form,
        Err(error) => {
            return format!(
                r#"<p class="{}" role="alert">{}: {}</p>"#,
                style::empty,
                tr(TextKey::ErrorOccurred),
                escape_html(error),
            );
        }
    };
    let (button, icon) = if form.id == 0 {
        (tr(TextKey::AddMultiCollection), IconName::Add)
    } else {
        (tr(TextKey::SaveMultiCollection), IconName::Save)
    };
    let icon_style = escape_html(&icon_style(icon, "1.125rem"));
    format!(
        r#"<form method="post" action="/api/multi-collections/save" class="{page}"><input type="hidden" name="id" value="{id}"><div class="{editor_action_bar}"><button type="submit" class="{btn_add}"><span style="{icon_style}" aria-hidden="true"></span>{button}</button></div><div class="{editor}"><h1 class="{page_title}">{heading}</h1><div class="{editor_full_row} {divider}"></div><label class="{field_label}" for="multi-collection-name">{name_label}</label><input id="multi-collection-name" name="name" value="{name}" required maxlength="50"></div></form>"#,
        page = style::page,
        editor_action_bar = style::editor_action_bar,
        page_title = style::page_title,
        heading = tr(TextKey::MultiCollection),
        btn_add = style::btn_add,
        editor_full_row = style::editor_full_row,
        divider = style::divider,
        id = form.id,
        button = button,
        icon_style = icon_style,
        editor = style::editor,
        field_label = style::field_label,
        name_label = tr(TextKey::Name),
        name = escape_html(&form.name),
    )
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[island]
fn MultiCollectionEditorIsland(
    initial_form: MultiCollectionForm,
    editor_options: MultiCollectionEditorOptions,
) -> impl IntoView {
    let id_signal = RwSignal::new(initial_form.id);
    let name = RwSignal::new(initial_form.name);
    let items = RwSignal::new(initial_form.items);

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
                    toast.push(tr(TextKey::MultiCollectionSaved), ToastSeverity::Success);
                    set_browser_location("/media/multi-collections");
                }
                Err(error) => toast.push(
                    save_failed(media_browser_error_message(error)),
                    ToastSeverity::Error,
                ),
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
            name_err.set(Some(tr(TextKey::MultiCollectionNameRequired).to_string()));
            return;
        }
        if trimmed.chars().count() > 50 {
            name_err.set(Some(tr(TextKey::MultiCollectionNameTooLong).to_string()));
            return;
        }
        name_err.set(None);
        save_action.dispatch(MultiCollectionForm {
            id: id_signal.get_untracked(),
            name: trimmed,
            items: items.get_untracked(),
        });
    });

    let save_icon = if id_signal.get_untracked() == 0 {
        IconName::Add
    } else {
        IconName::Save
    };
    let save_label = if id_signal.get_untracked() == 0 {
        tr(TextKey::AddMultiCollection)
    } else {
        tr(TextKey::SaveMultiCollection)
    };

    view! {
        <form
            method="post"
            action="/api/multi-collections/save"
            class=style::page
            on:submit=move |event| {
                event.prevent_default();
                on_save.run(());
            }
        >
            <input type="hidden" name="id" value=move || id_signal.get().to_string()/>
            <div class=style::editor_action_bar>
                <button type="submit" class=style::btn_add>
                    <span style=icon_style(save_icon, "1.125rem") aria-hidden="true"></span>
                    {save_label}
                </button>
            </div>

            <div class=style::editor>
                <h1 class=style::page_title>{tr(TextKey::MultiCollection)}</h1>
                <div class=format!("{} {}", style::editor_full_row, style::divider)></div>

                <TextField
                    label=tr(TextKey::Name)
                    value=name
                    name=Some("name")
                    required=true
                    max_length=Some(50)
                    error=Some(name_err_signal)
                />

                {
                    let options = editor_options.clone();
                    view! {
                                    <label class=style::field_label>{tr(TextKey::Collections)}</label>
                                    <div class=style::selector_control>
                                    <select
                                        class=style::select
                                        prop:value=move || selected_collection.get().to_string()
                                        on:change=move |event| {
                                            selected_collection.set(
                                                event_target_value(&event).parse::<i64>().unwrap_or_default(),
                                            );
                                        }
                                    >
                                        <option value="0">{tr(TextKey::SelectCollection)}</option>
                                        {options.collections.iter().map(|option| {
                                            let id = option.id;
                                            view! {
                                                <option
                                                    value=id.to_string()
                                                    disabled=move || {
                                                        items.with(|items| {
                                                            items.iter().any(|item| item.collection_id == Some(id))
                                                        })
                                                    }
                                                >
                                                    {option.name.clone()}
                                                </option>
                                            }
                                        }).collect_view()}
                                    </select>
                                    <p class=style::help_text>
                                        {tr(TextKey::DisabledCollectionsAlreadyPresent)}
                                    </p>
                                    </div>
                                    <div class=style::field_spacer aria-hidden="true"></div>
                                    <button
                                        type="button"
                                        class=format!("{} {}", style::btn_add, style::selector_action)
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
                                        <span style=icon_style(IconName::Add, "1.125rem") aria-hidden="true"></span>
                                        {tr(TextKey::AddCollection)}
                                    </button>

                                    <label class=style::field_label>{tr(TextKey::SmartCollections)}</label>
                                    <div class=style::selector_control>
                                    <select
                                        class=style::select
                                        prop:value=move || selected_smart_collection.get().to_string()
                                        on:change=move |event| {
                                            selected_smart_collection.set(
                                                event_target_value(&event).parse::<i64>().unwrap_or_default(),
                                            );
                                        }
                                    >
                                        <option value="0">{tr(TextKey::SelectSmartCollection)}</option>
                                        {options.smart_collections.iter().map(|option| {
                                            let id = option.id;
                                            view! {
                                                <option
                                                    value=id.to_string()
                                                    disabled=move || {
                                                        items.with(|items| {
                                                            items.iter().any(|item| item.smart_collection_id == Some(id))
                                                        })
                                                    }
                                                >
                                                    {option.name.clone()}
                                                </option>
                                            }
                                        }).collect_view()}
                                    </select>
                                    <p class=style::help_text>
                                        {tr(TextKey::DisabledCollectionsAlreadyPresent)}
                                    </p>
                                    </div>
                                    <div class=style::field_spacer aria-hidden="true"></div>
                                    <button
                                        type="button"
                                        class=format!("{} {}", style::btn_add, style::selector_action)
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
                                        <span style=icon_style(IconName::Add, "1.125rem") aria-hidden="true"></span>
                                        {tr(TextKey::AddSmartCollection)}
                                    </button>
                    }
                }

                <section class=format!("{} {}", style::editor_full_row, style::items_section)>
                    <h2 class=style::section_title>
                        {move || {
                            let current = name.get();
                            if current.trim().is_empty() {
                                tr(TextKey::MultiCollectionItems).to_string()
                            } else {
                                format!("{current} {}", tr(TextKey::Items))
                            }
                        }}
                    </h2>
                    <div class=style::divider></div>
                    {move || multi_collection_item_hidden_fields(items.get())}
                    <MultiCollectionItemsTable items=items/>
                </section>
            </div>
        </form>
    }
}

fn multi_collection_item_hidden_fields(items: Vec<MultiCollectionItemForm>) -> impl IntoView {
    items
        .into_iter()
        .enumerate()
        .map(|(index, item)| {
            let collection_id = item.collection_id.unwrap_or_default().to_string();
            let smart_collection_id = item.smart_collection_id.unwrap_or_default().to_string();
            let collection_type = item.collection_type.to_string();
            let use_custom_playback_order = item.use_custom_playback_order.to_string();
            let playback_order = item.playback_order.to_string();
            view! {
                <input type="hidden" name="item_collection_id" value=collection_id/>
                <input type="hidden" name="item_smart_collection_id" value=smart_collection_id/>
                <input type="hidden" name="item_name" value=item.name/>
                <input type="hidden" name="item_collection_type" value=collection_type/>
                <input type="hidden" name="item_use_custom_playback_order" value=use_custom_playback_order/>
                <input type="hidden" name="item_playback_order" value=playback_order/>
                {if item.schedule_as_group {
                    view! {
                        <input
                            type="hidden"
                            name="item_schedule_as_group_index"
                            value=index.to_string()
                        />
                    }
                    .into_any()
                } else {
                    ().into_any()
                }}
            }
        })
        .collect_view()
}

#[component]
fn MultiCollectionItemsTable(items: RwSignal<Vec<MultiCollectionItemForm>>) -> impl IntoView {
    view! {
        <div class=style::table_wrap>
            <table>
                <colgroup>
                    <col/>
                    <col style="width: 20%"/>
                    <col style="width: 30%"/>
                    <col style="width: 60px"/>
                </colgroup>
                <thead>
                    <tr>
                        <th>{tr(TextKey::Collection)}</th>
                        <th>{tr(TextKey::ScheduleAsGroup)}</th>
                        <th>{tr(TextKey::PlaybackOrder)}</th>
                        <th/>
                    </tr>
                </thead>
                <tbody>
                    {move || {
                        let mut rows: Vec<_> = items.get().into_iter().enumerate().collect();
                        rows.sort_by_key(|(_, item)| item.name.to_lowercase());
                        if rows.is_empty() {
                            return view! {
                                <tr>
                                    <td colspan="4">{tr(TextKey::NoItems)}</td>
                                </tr>
                            }
                            .into_any();
                        }
                        rows.into_iter().map(|(index, item)| {
                            let checked = item.schedule_as_group;
                            let label = if item.schedule_as_group {
                                if item.use_custom_playback_order {
                                    tr(TextKey::Custom)
                                } else {
                                    tr(TextKey::Chronological)
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
                                                    items.get(index).is_some_and(|item| item.schedule_as_group)
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
                                                {tr(TextKey::Delete)}
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
