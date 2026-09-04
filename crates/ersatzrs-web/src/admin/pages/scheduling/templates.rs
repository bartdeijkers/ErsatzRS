//! Templates list and editor — ports `Pages/Templates.razor` and `Pages/TemplateEditor.razor`.
#![allow(clippy::too_many_lines)]

use std::collections::BTreeMap;

use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::admin::i18n::{delete_failed, scheduling_content_error_message, tr, TextKey};
use crate::admin::server_fns::templates::{
    copy_template, create_template, create_template_group, delete_template, delete_template_group,
    format_short_time, get_template, get_template_items, list_template_block_groups,
    list_template_groups, list_templates, TemplateBlockGroupOption, TemplateBlockOption,
    TemplateForm, TemplateGroupRow, TemplateItemForm, TemplateRow,
};
use crate::admin::ui::primitives::{
    dialog::DeleteDialog,
    resource_state::{admin_loading_resource_view, admin_result_view},
    toast::{load_or_notify, use_toast, ToastSeverity},
};

mod style {
    #![allow(dead_code)]
    stylance::import_crate_style!(s, "src/admin/pages/scheduling/templates.module.css");
    pub use s::*;
}

#[component]
pub fn TemplatesPage() -> impl IntoView {
    let initial = Resource::new_blocking(
        || (),
        |()| async move { tokio::try_join!(list_template_groups(), list_templates(String::new())) },
    );

    view! {
        <Suspense fallback=admin_loading_resource_view>
            {move || {
                initial.get().map(|result| admin_result_view(result, |(initial_groups, initial_templates)| {
                    view! {
                        <TemplatesIsland
                            initial_groups=initial_groups
                            initial_templates=initial_templates
                        />
                    }
                    .into_any()
                }))
            }}
        </Suspense>
    }
}

#[island]
fn TemplatesIsland(
    initial_groups: Vec<TemplateGroupRow>,
    initial_templates: Vec<TemplateRow>,
) -> impl IntoView {
    let toast = use_toast();
    let current_groups = RwSignal::new(initial_groups);
    let groups: LocalResource<Vec<TemplateGroupRow>> =
        LocalResource::new(
            move || async move { load_or_notify(list_template_groups().await, toast) },
        );
    Effect::new(move |_| {
        if let Some(rows) = groups.get() {
            current_groups.set(rows);
        }
    });
    let search = RwSignal::new(String::new());
    let current_templates = RwSignal::new(initial_templates);
    let templates: LocalResource<Vec<TemplateRow>> = LocalResource::new(move || {
        let query = search.get();
        async move { load_or_notify(list_templates(query).await, toast) }
    });
    Effect::new(move |_| {
        if let Some(rows) = templates.get() {
            current_templates.set(rows);
        }
    });

    let new_group_name = RwSignal::new(String::new());
    let new_template_group_id = RwSignal::new(0i64);
    let new_template_name = RwSignal::new(String::new());
    let create_group_action = Action::new(|name: &String| create_template_group(name.clone()));
    let create_template_action =
        Action::new(|(group_id, name): &(i64, String)| create_template(*group_id, name.clone()));

    let copy_open = RwSignal::new(false);
    let copy_target = RwSignal::new(None::<TemplateRow>);
    let copy_group_id = RwSignal::new(0i64);
    let copy_name = RwSignal::new(String::new());
    let copy_action = Action::new(|(id, group_id, name): &(i64, i64, String)| {
        copy_template(*id, *group_id, name.clone())
    });

    let delete_group_open = RwSignal::new(false);
    let delete_group_target = RwSignal::new(None::<(i64, String)>);
    let delete_template_open = RwSignal::new(false);
    let delete_template_target = RwSignal::new(None::<(i64, String)>);
    let delete_group_action = Action::new(|id: &i64| delete_template_group(*id));
    let delete_template_action = Action::new(|id: &i64| delete_template(*id));

    Effect::new(move |_| {
        if let Some(result) = create_group_action.value().get() {
            match result {
                Ok(id) => {
                    new_group_name.set(String::new());
                    new_template_group_id.set(id);
                    groups.refetch();
                    templates.refetch();
                    toast.push(tr(TextKey::TemplateGroupCreated), ToastSeverity::Success);
                }
                Err(error) => toast.push(
                    format!(
                        "{}: {}",
                        tr(TextKey::AddFailed),
                        scheduling_content_error_message(error)
                    ),
                    ToastSeverity::Error,
                ),
            }
        }
    });

    Effect::new(move |_| {
        if let Some(result) = create_template_action.value().get() {
            match result {
                Ok(_) => {
                    new_template_name.set(String::new());
                    templates.refetch();
                    groups.refetch();
                    toast.push(tr(TextKey::TemplateCreated), ToastSeverity::Success);
                }
                Err(error) => toast.push(
                    format!(
                        "{}: {}",
                        tr(TextKey::AddFailed),
                        scheduling_content_error_message(error)
                    ),
                    ToastSeverity::Error,
                ),
            }
        }
    });

    Effect::new(move |_| {
        if let Some(result) = copy_action.value().get() {
            match result {
                Ok(id) => {
                    copy_open.set(false);
                    copy_target.set(None);
                    templates.refetch();
                    groups.refetch();
                    toast.push(tr(TextKey::TemplateCopied), ToastSeverity::Success);
                    #[cfg(target_arch = "wasm32")]
                    {
                        let _ = window().location().set_href(&format!("/templates/{id}"));
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        let _ = id;
                    }
                }
                Err(error) => toast.push(
                    format!(
                        "{}: {}",
                        tr(TextKey::CopyFailed),
                        scheduling_content_error_message(error)
                    ),
                    ToastSeverity::Error,
                ),
            }
        }
    });

    Effect::new(move |_| {
        if let Some(result) = delete_group_action.value().get() {
            match result {
                Ok(()) => {
                    groups.refetch();
                    templates.refetch();
                    toast.push(tr(TextKey::TemplateGroupDeleted), ToastSeverity::Success);
                }
                Err(error) => toast.push(
                    delete_failed(scheduling_content_error_message(error)),
                    ToastSeverity::Error,
                ),
            }
        }
    });

    Effect::new(move |_| {
        if let Some(result) = delete_template_action.value().get() {
            match result {
                Ok(()) => {
                    groups.refetch();
                    templates.refetch();
                    toast.push(tr(TextKey::TemplateDeleted), ToastSeverity::Success);
                }
                Err(error) => toast.push(
                    delete_failed(scheduling_content_error_message(error)),
                    ToastSeverity::Error,
                ),
            }
        }
    });

    view! {
        <div class=style::page>
            <div class=style::content>
                <section class=style::section>
                    <h1 class=style::page_title>{tr(TextKey::TemplateGroups)}</h1>
                    <div class=style::divider></div>
                    <form method="post" action="/api/templates/groups/create" class=style::form_grid>
                        <label class=style::label for="template-group-name">{tr(TextKey::TemplateGroupName)}</label>
                        <input
                            id="template-group-name"
                            class=style::input
                            name="name"
                            required=true
                            prop:value=move || new_group_name.get()
                            on:input=move |event| new_group_name.set(event_target_value(&event))
                        />
                        <div></div>
                        <button
                            type="submit"
                            class=style::btn_primary
                            on:click=move |event| {
                                event.prevent_default();
                                let name = new_group_name.get_untracked().trim().to_string();
                                if !name.is_empty() {
                                    create_group_action.dispatch(name);
                                }
                            }
                        >
                            {tr(TextKey::AddTemplateGroup)}
                        </button>
                    </form>
                </section>

                <section class=style::section>
                    <h2 class=style::section_title>{tr(TextKey::Templates)}</h2>
                    <div class=style::divider></div>
                    <form method="post" action="/api/templates/create" class=style::form_grid>
                        <label class=style::label for="template-group">{tr(TextKey::TemplateGroup)}</label>
                        <select
                            id="template-group"
                            class=style::select
                            name="group_id"
                            prop:value=move || new_template_group_id.get().to_string()
                            on:change=move |event| {
                                if let Ok(id) = event_target_value(&event).parse::<i64>() {
                                    new_template_group_id.set(id);
                                }
                            }
                        >
                            <option value="0"></option>
                            {move || {
                                current_groups
                                    .get()
                                    .into_iter()
                                    .map(|group| {
                                        view! { <option value=group.id.to_string()>{group.name}</option> }
                                    })
                                    .collect_view()
                            }}
                        </select>

                        <label class=style::label for="template-name">{tr(TextKey::TemplateName)}</label>
                        <input
                            id="template-name"
                            class=style::input
                            name="name"
                            required=true
                            prop:value=move || new_template_name.get()
                            on:input=move |event| new_template_name.set(event_target_value(&event))
                        />
                        <div></div>
                        <button
                            type="submit"
                            class=style::btn_primary
                            on:click=move |event| {
                                event.prevent_default();
                                let group_id = new_template_group_id.get_untracked();
                                let name = new_template_name.get_untracked().trim().to_string();
                                if group_id > 0 && !name.is_empty() {
                                    create_template_action.dispatch((group_id, name));
                                }
                            }
                        >
                            {tr(TextKey::AddTemplate)}
                        </button>
                    </form>
                </section>

                <div class=style::toolbar>
                    <span class=style::filter_icon aria-hidden="true"></span>
                    <input
                        type="search"
                        placeholder=tr(TextKey::SearchForTemplates)
                        prop:value=move || search.get()
                        on:input=move |event| search.set(event_target_value(&event))
                    />
                </div>

                {move || {
                    let rows = current_templates.get();
                    view! {
                        <TemplateTable
                            rows=rows
                            copy_open=copy_open
                            copy_target=copy_target
                            copy_group_id=copy_group_id
                            copy_name=copy_name
                            delete_group_open=delete_group_open
                            delete_group_target=delete_group_target
                            delete_template_open=delete_template_open
                            delete_template_target=delete_template_target
                        />
                    }
                }}
            </div>

            <div
                class=style::modal_backdrop
                style:display=move || if copy_open.get() { "" } else { "none" }
                on:click=move |_| copy_open.set(false)
            />
            <div
                class=style::modal_panel
                style:display=move || if copy_open.get() { "" } else { "none" }
            >
                <h2 class=style::modal_title>{tr(TextKey::CopyTemplate)}</h2>
                <p class=style::label>{tr(TextKey::SelectGroupForNewTemplate)}</p>
                <form method="post" action="/api/templates/copy" class=style::form_grid>
                    <input
                        type="hidden"
                        name="id"
                        value=move || copy_target.get().map(|row| row.id.to_string()).unwrap_or_default()
                    />
                    <label class=style::label for="copy-template-group">{tr(TextKey::NewTemplateGroup)}</label>
                    <select
                        id="copy-template-group"
                        name="group_id"
                        class=style::select
                        prop:value=move || copy_group_id.get().to_string()
                        on:change=move |event| {
                            if let Ok(id) = event_target_value(&event).parse::<i64>() {
                                copy_group_id.set(id);
                            }
                        }
                    >
                        <option value="0"></option>
                        {move || {
                            current_groups
                                .get()
                                .into_iter()
                                .map(|group| {
                                    view! { <option value=group.id.to_string()>{group.name}</option> }
                                })
                                .collect_view()
                        }}
                    </select>
                    <label class=style::label for="copy-template-name">{tr(TextKey::NewTemplateName)}</label>
                    <input
                        id="copy-template-name"
                        class=style::input
                        name="name"
                        prop:value=move || copy_name.get()
                        on:input=move |event| copy_name.set(event_target_value(&event))
                    />
                    <div></div>
                    <div class=style::modal_actions>
                        <button type="button" class=style::btn_default on:click=move |_| copy_open.set(false)>
                            {tr(TextKey::Cancel)}
                        </button>
                        <button
                            type="submit"
                            class=style::btn_primary
                            disabled=move || {
                                copy_target.get().is_none()
                                    || copy_group_id.get() == 0
                                    || copy_name.get().trim().is_empty()
                            }
                            on:click=move |event| {
                                event.prevent_default();
                                if let Some(target) = copy_target.get_untracked() {
                                    copy_action.dispatch((
                                        target.id,
                                        copy_group_id.get_untracked(),
                                        copy_name.get_untracked().trim().to_string(),
                                    ));
                                }
                            }
                        >
                            {tr(TextKey::CopyTemplate)}
                        </button>
                    </div>
                </form>
            </div>

            <DeleteDialog
                dialog_id="template-group-delete-dialog"
                open=delete_group_open
                entity_name=Signal::derive(move || {
                    delete_group_target
                        .get()
                        .map(|(_, name)| name)
                        .unwrap_or_default()
                })
                on_confirm=Callback::new(move |confirmed: bool| {
                    if let Some((id, _)) = delete_group_target.get_untracked() {
                        if confirmed {
                            delete_group_action.dispatch(id);
                        } else {
                            delete_group_open.set(false);
                        }
                    } else {
                        delete_group_open.set(false);
                    }
                })
            />
            <DeleteDialog
                dialog_id="template-delete-dialog"
                open=delete_template_open
                entity_name=Signal::derive(move || {
                    delete_template_target
                        .get()
                        .map(|(_, name)| name)
                        .unwrap_or_default()
                })
                on_confirm=Callback::new(move |confirmed: bool| {
                    if let Some((id, _)) = delete_template_target.get_untracked() {
                        if confirmed {
                            delete_template_action.dispatch(id);
                        } else {
                            delete_template_open.set(false);
                        }
                    } else {
                        delete_template_open.set(false);
                    }
                })
            />
        </div>
    }
}

#[component]
fn TemplateTable(
    rows: Vec<TemplateRow>,
    copy_open: RwSignal<bool>,
    copy_target: RwSignal<Option<TemplateRow>>,
    copy_group_id: RwSignal<i64>,
    copy_name: RwSignal<String>,
    delete_group_open: RwSignal<bool>,
    delete_group_target: RwSignal<Option<(i64, String)>>,
    delete_template_open: RwSignal<bool>,
    delete_template_target: RwSignal<Option<(i64, String)>>,
) -> impl IntoView {
    if rows.is_empty() {
        return view! { <p class=style::empty>{tr(TextKey::NoTemplatesFound)}</p> }.into_any();
    }

    let mut grouped: BTreeMap<(String, i64), Vec<TemplateRow>> = BTreeMap::new();
    for row in rows {
        grouped
            .entry((row.group_name.clone(), row.group_id))
            .or_default()
            .push(row);
    }

    view! {
        <div class=style::table_wrap>
            <table>
                <thead>
                    <tr>
                        <th>{tr(TextKey::Name)}</th>
                        <th></th>
                    </tr>
                </thead>
                <tbody>
                    {grouped.into_iter().map(|((group_name, group_id), rows)| {
                        let delete_group_name = group_name.clone();
                        view! {
                            <tr>
                                <td class=style::group_header>{group_name.clone()}</td>
                                <td class=style::group_header>
                                    <div class=style::header_actions>
                                        <form method="post" action="/api/templates/groups/delete">
                                            <input type="hidden" name="id" value=group_id.to_string()/>
                                            <button
                                                type="submit"
                                                title=tr(TextKey::DeleteTemplateGroup)
                                                class=format!("{} {}", style::icon_button, style::danger)
                                                on:click=move |event| {
                                                    event.prevent_default();
                                                    delete_group_target
                                                        .set(Some((group_id, delete_group_name.clone())));
                                                    delete_group_open.set(true);
                                                }
                                            >
                                                <span class=style::delete_icon aria-hidden="true"></span>
                                            </button>
                                        </form>
                                    </div>
                                </td>
                            </tr>
                            {rows.into_iter().map(|row| {
                                let id = row.id;
                                let name = row.name.clone();
                                let copy_row = row.clone();
                                let copy_name_value = name.clone();
                                let delete_name = name.clone();
                                view! {
                                    <tr>
                                        <td>
                                            {if id >= 0 {
                                                let href = format!("/templates/{id}");
                                                view! {
                                                    <a
                                                        href=href.clone()
                                                        on:click=move |event| navigate_by_browser(event, &href)
                                                    >
                                                        {name}
                                                    </a>
                                                }.into_any()
                                            } else {
                                                view! { <span>{name}</span> }.into_any()
                                            }}
                                        </td>
                                        <td>
                                            <div class=style::actions>
                                                {if id >= 0 {
                                                    let href = format!("/templates/{id}");
                                                    view! {
                                                        <a
                                                            class=style::icon_button
                                                            title=tr(TextKey::EditTemplate)
                                                            href=href.clone()
                                                            on:click=move |event| navigate_by_browser(event, &href)
                                                        >
                                                            <span class=style::edit_icon aria-hidden="true"></span>
                                                        </a>
                                                        <button
                                                            type="button"
                                                            class=style::icon_button
                                                            title=tr(TextKey::CopyTemplate)
                                                            on:click=move |_| {
                                                                copy_group_id.set(copy_row.group_id);
                                                                copy_name.set(format!("{} {}", copy_name_value, tr(TextKey::CopyNameSuffix)));
                                                                copy_target.set(Some(copy_row.clone()));
                                                                copy_open.set(true);
                                                            }
                                                        >
                                                            <span class=style::copy_icon aria-hidden="true"></span>
                                                        </button>
                                                        <form method="post" action="/api/templates/delete">
                                                            <input type="hidden" name="id" value=id.to_string()/>
                                                            <button
                                                                type="submit"
                                                                class=format!("{} {}", style::icon_button, style::danger)
                                                                title=tr(TextKey::DeleteTemplate)
                                                                on:click=move |event| {
                                                                    event.prevent_default();
                                                                    delete_template_target
                                                                        .set(Some((id, delete_name.clone())));
                                                                    delete_template_open.set(true);
                                                                }
                                                            >
                                                                <span class=style::delete_icon aria-hidden="true"></span>
                                                            </button>
                                                        </form>
                                                    }.into_any()
                                                } else {
                                                    view! { <span class=style::icon_button></span> }.into_any()
                                                }}
                                            </div>
                                        </td>
                                    </tr>
                                }
                            }).collect_view()}
                        }
                    }).collect_view()}
                </tbody>
            </table>
        </div>
    }.into_any()
}

#[component]
pub fn TemplateEditorPage() -> impl IntoView {
    let params = use_params_map();
    let template_id = move || {
        params.with(|p| {
            p.get("id")
                .and_then(|value| value.parse::<i64>().ok())
                .unwrap_or(0)
        })
    };
    let data = Resource::new_blocking(template_id, |id| async move {
        tokio::try_join!(
            get_template(id),
            get_template_items(id),
            list_template_block_groups(),
        )
    });

    view! {
        <Suspense fallback=admin_loading_resource_view>
            {move || data.get().map(|result| admin_result_view(result, |(template, items, groups)| {
                match template {
                    Some(template) => view! {
                        <TemplateEditorIsland template=template initial_items=items block_groups=groups/>
                    }.into_any(),
                    None => view! {
                        <TemplateNotFoundRedirect/>
                    }.into_any(),
                }
            }))}
        </Suspense>
    }
}

#[island]
fn TemplateEditorIsland(
    template: TemplateForm,
    initial_items: Vec<TemplateItemForm>,
    block_groups: Vec<TemplateBlockGroupOption>,
) -> impl IntoView {
    view! {
        <TemplateEditorForm
            template=template
            initial_items=initial_items
            block_groups=block_groups
        />
    }
}

#[component]
fn TemplateNotFoundRedirect() -> impl IntoView {
    Effect::new(move |_| {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = window().location().set_href("/templates");
        }
    });

    view! {
        <p class=style::empty>
            {tr(TextKey::TemplateNotFound)}
            " "
            <a href="/templates">{tr(TextKey::BackToTemplates)}</a>
        </p>
    }
}

#[component]
fn TemplateEditorForm(
    template: TemplateForm,
    initial_items: Vec<TemplateItemForm>,
    block_groups: Vec<TemplateBlockGroupOption>,
) -> impl IntoView {
    let template_id = template.id;
    let group_id = template.group_id;
    let group_name = template.group_name.clone();
    let name = RwSignal::new(template.name.clone());
    let items = RwSignal::new(initial_items);
    let selected_block_group = RwSignal::new(0i64);
    let blocks = RwSignal::new(Vec::<TemplateBlockOption>::new());
    let toast = use_toast();
    Effect::new(move |_| {
        let group_id = selected_block_group.get();
        #[cfg(target_arch = "wasm32")]
        leptos::task::spawn_local(async move {
            let rows = if group_id <= 0 {
                Vec::new()
            } else {
                load_or_notify(
                    crate::admin::server_fns::templates::list_template_blocks_for_group(group_id)
                        .await,
                    toast,
                )
            };
            blocks.set(rows);
        });
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = group_id;
        }
    });
    let selected_block = RwSignal::new(None::<TemplateBlockOption>);
    let selected_start = RwSignal::new(0i32);
    let block_to_remove = RwSignal::new(None::<usize>);
    let drag_index = RwSignal::new(None::<usize>);

    let add_block = move || {
        if let Some(block) = selected_block.get_untracked() {
            let mut start = selected_start.get_untracked();
            let existing = items.get_untracked();
            while start < 24 * 60 {
                let end = start + block.minutes;
                if end <= 24 * 60 && !intersects_any(&existing, start, end, None) {
                    items.update(|rows| {
                        rows.push(TemplateItemForm {
                            block_id: block.id,
                            block_name: block.name.clone(),
                            start_minutes: start,
                            duration_minutes: block.minutes,
                        });
                        rows.sort_by_key(|item| item.start_minutes);
                    });
                    return;
                }
                start += 5;
            }
            toast.push(tr(TextKey::BlockWillNotFit), ToastSeverity::Error);
        }
    };

    view! {
        <div class=style::page>
            <div class=style::action_bar>
                <div class=style::action_row>
                    <button
                        type="submit"
                        form="template-editor-form"
                        name="action"
                        value="save"
                        class=style::btn_primary
                    >
                        {tr(TextKey::SaveTemplate)}
                    </button>
                    <span class=style::action_title>
                        {group_name.clone()} " / " {move || name.get()}
                    </span>
                    <a
                        class=style::btn_default
                        href="/templates"
                        on:click=move |event| navigate_by_browser(event, "/templates")
                    >
                        {tr(TextKey::Back)}
                    </a>
                </div>
            </div>
            <div class=style::content>
                <form id="template-editor-form" method="post" action="/api/templates/items/save">
                    <input type="hidden" name="template_id" value=template_id.to_string()/>
                    <input type="hidden" name="group_id" value=group_id.to_string()/>
                    <h1 class=style::section_title>{tr(TextKey::General)}</h1>
                    <div class=style::divider></div>
                    <div class=style::form_grid>
                        <label class=style::label>{tr(TextKey::TemplateGroupName)}</label>
                        <input class=style::input disabled=true prop:value=group_name.clone()/>
                        <label class=style::label for="template-editor-name">{tr(TextKey::TemplateName)}</label>
                        <input
                            id="template-editor-name"
                            class=style::input
                            name="name"
                            prop:value=move || name.get()
                            on:input=move |event| name.set(event_target_value(&event))
                        />
                    </div>

                    <h2 class=style::section_title>{tr(TextKey::AddContent)}</h2>
                    <div class=style::divider></div>
                    <div class=style::form_grid>
                        <label class=style::label for="template-block-group">{tr(TextKey::BlockGroup)}</label>
                        <select
                            id="template-block-group"
                            class=style::select
                            prop:value=move || selected_block_group.get().to_string()
                            on:change=move |event| {
                                if let Ok(group_id) = event_target_value(&event).parse::<i64>() {
                                    selected_block_group.set(group_id);
                                    selected_block.set(None);
                                }
                            }
                        >
                            <option value="0"></option>
                            {block_groups.into_iter().map(|group| {
                                view! { <option value=group.id.to_string()>{group.name}</option> }
                            }).collect_view()}
                        </select>

                        <label class=style::label for="template-block">{tr(TextKey::Block)}</label>
                        <select
                            id="template-block"
                            class=style::select
                            prop:value=move || {
                                selected_block
                                    .get()
                                    .map_or_else(|| "0".to_string(), |block| block.id.to_string())
                            }
                            on:change=move |event| {
                                let selected_id = event_target_value(&event).parse::<i64>().unwrap_or_default();
                                selected_block.set(
                                    blocks
                                        .get_untracked()
                                        .into_iter()
                                        .find(|block| block.id == selected_id),
                                );
                            }
                        >
                            <option value="0"></option>
                            {move || blocks.get().into_iter().map(|block| {
                                view! { <option value=block.id.to_string()>{block.name}</option> }
                            }).collect_view()}
                        </select>

                        <label class=style::label for="template-start">{tr(TextKey::StartTimeOnOrAfter)}</label>
                        <select
                            id="template-start"
                            class=style::select
                            prop:value=move || selected_start.get().to_string()
                            on:change=move |event| {
                                if let Ok(value) = event_target_value(&event).parse::<i32>() {
                                    selected_start.set(value);
                                }
                            }
                        >
                            {(0..(24 * 60)).step_by(5).map(|minutes| {
                                view! { <option value=minutes.to_string()>{format_short_time(minutes)}</option> }
                            }).collect_view()}
                        </select>
                        <div></div>
                        <button
                            type="button"
                            class=style::btn_primary
                            disabled=move || selected_block.get().is_none()
                            on:click=move |_| add_block()
                        >
                            {tr(TextKey::AddBlockToTemplate)}
                        </button>
                    </div>

                    <h2 class=style::section_title>{tr(TextKey::RemoveContent)}</h2>
                    <div class=style::divider></div>
                    <div class=style::form_grid>
                        <label class=style::label for="template-remove">{tr(TextKey::BlockToRemove)}</label>
                        <select
                            id="template-remove"
                            class=style::select
                            on:change=move |event| {
                                let value = event_target_value(&event);
                                block_to_remove.set(value.parse::<usize>().ok());
                            }
                        >
                            <option value="">{tr(TextKey::NoneOption)}</option>
                            {move || items.get().into_iter().enumerate().map(|(index, item)| {
                                let label = format!("{} - {}", format_short_time(item.start_minutes), item.block_name);
                                view! { <option value=index.to_string()>{label}</option> }
                            }).collect_view()}
                        </select>
                        <div></div>
                        <button
                            type="button"
                            class=style::btn_primary
                            disabled=move || block_to_remove.get().is_none()
                            on:click=move |_| {
                                if let Some(index) = block_to_remove.get_untracked() {
                                    items.update(|rows| {
                                        if index < rows.len() {
                                            rows.remove(index);
                                        }
                                    });
                                    block_to_remove.set(None);
                                }
                            }
                        >
                            {tr(TextKey::RemoveBlockFromTemplate)}
                        </button>
                    </div>

                    <h2 class=style::section_title>{tr(TextKey::Content)}</h2>
                    <div class=style::divider></div>
                    {move || hidden_item_inputs(items.get())}
                    <div class=style::timeline_wrap>
                        <div class=style::timeline>
                            <div class=style::time_labels>
                                {(0..24).map(|hour| view! {
                                    <div class=style::time_label>{format!("{hour:02}:00")}</div>
                                }).collect_view()}
                            </div>
                            <div
                                class=style::time_grid
                                on:dragover=move |event| event.prevent_default()
                                on:drop=move |event| {
                                    event.prevent_default();
                                    let Some(index) = drag_index.get_untracked() else {
                                        return;
                                    };
                                    drag_index.set(None);

                                    #[cfg(target_arch = "wasm32")]
                                    {
                                        use wasm_bindgen::JsCast;

                                        let Some(target) = event.current_target() else {
                                            return;
                                        };
                                        let Ok(element) = target.dyn_into::<web_sys::Element>() else {
                                            return;
                                        };
                                        let rect = element.get_bounding_client_rect();
                                        let Some(new_start) = snap_drop_minutes(
                                            f64::from(event.client_y()) - rect.top(),
                                            rect.height(),
                                        ) else {
                                            return;
                                        };

                                        let current = items.get_untracked();
                                        let Some(item) = current.get(index).cloned() else {
                                            return;
                                        };
                                        let new_end = new_start + item.duration_minutes;
                                        if new_end > 24 * 60
                                            || intersects_any(&current, new_start, new_end, Some(index))
                                        {
                                            toast.push(
                                                tr(TextKey::BlockCannotBeMoved),
                                                ToastSeverity::Error,
                                            );
                                            return;
                                        }

                                        items.update(|rows| {
                                            if let Some(row) = rows.get_mut(index) {
                                                row.start_minutes = new_start;
                                            }
                                            rows.sort_by_key(|row| row.start_minutes);
                                        });
                                    }

                                    #[cfg(not(target_arch = "wasm32"))]
                                    {
                                        let _ = index;
                                    }
                                }
                            >
                                {move || items.get().into_iter().enumerate().map(|(index, item)| {
                                    let top = f64::from(item.start_minutes) / 60.0 * 3.0;
                                    let height = (f64::from(item.duration_minutes.max(5)) / 60.0 * 3.0).max(1.5);
                                    let end = item.start_minutes + item.duration_minutes;
                                    view! {
                                        <div
                                            class=style::timeline_item
                                            draggable="true"
                                            style=format!("top: {top:.3}rem; height: {height:.3}rem;")
                                            on:dragstart=move |_| drag_index.set(Some(index))
                                            on:dragend=move |_| drag_index.set(None)
                                        >
                                            <strong>{item.block_name}</strong>
                                            <span class=style::timeline_time>
                                                {format!("{} - {}", format_short_time(item.start_minutes), format_short_time(end))}
                                            </span>
                                        </div>
                                    }
                                }).collect_view()}
                            </div>
                        </div>
                    </div>
                    <button type="submit" name="action" value="save" class=style::hidden>{tr(TextKey::SaveTemplate)}</button>
                </form>
            </div>
        </div>
    }
}

fn hidden_item_inputs(items: Vec<TemplateItemForm>) -> impl IntoView {
    view! {
        <>
            {items.into_iter().map(|item| view! {
                <>
                    <input type="hidden" name="item_block_id" value=item.block_id.to_string()/>
                    <input type="hidden" name="item_block_name" value=item.block_name/>
                    <input type="hidden" name="item_start_minutes" value=item.start_minutes.to_string()/>
                    <input type="hidden" name="item_duration_minutes" value=item.duration_minutes.to_string()/>
                </>
            }).collect_view()}
        </>
    }
}

fn intersects_any(
    items: &[TemplateItemForm],
    start: i32,
    end: i32,
    skip_index: Option<usize>,
) -> bool {
    items.iter().enumerate().any(|(index, item)| {
        if Some(index) == skip_index {
            return false;
        }
        let other_start = item.start_minutes;
        let other_end = item.start_minutes + item.duration_minutes;
        start < other_end && other_start < end
    })
}

#[cfg_attr(not(any(test, target_arch = "wasm32")), allow(dead_code))]
#[allow(clippy::cast_possible_truncation)]
fn snap_drop_minutes(offset_y: f64, grid_height: f64) -> Option<i32> {
    if !offset_y.is_finite() || !grid_height.is_finite() || grid_height <= 0.0 {
        return None;
    }
    let clamped = offset_y.clamp(0.0, grid_height);
    let raw_minutes = clamped / grid_height * f64::from(24 * 60);
    let snapped = (raw_minutes / 5.0).round() as i32 * 5;
    Some(snapped.clamp(0, 24 * 60))
}

fn navigate_by_browser(event: leptos::ev::MouseEvent, href: &str) {
    #[cfg(target_arch = "wasm32")]
    {
        event.prevent_default();
        let _ = window().location().set_href(href);
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (event, href);
    }
}

#[cfg(test)]
mod tests {
    use super::snap_drop_minutes;

    #[test]
    fn drop_offsets_snap_to_five_minute_day_grid() {
        assert_eq!(snap_drop_minutes(0.0, 1440.0), Some(0));
        assert_eq!(snap_drop_minutes(480.0, 1440.0), Some(480));
        assert_eq!(snap_drop_minutes(1440.0, 1440.0), Some(1440));
        assert_eq!(snap_drop_minutes(-10.0, 1440.0), Some(0));
    }
}
