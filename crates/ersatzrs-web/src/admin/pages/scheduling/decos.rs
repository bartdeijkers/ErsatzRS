//! Decos and deco templates list/editor pages.
#![allow(clippy::too_many_lines)]

use std::collections::BTreeMap;

use leptos::ev;
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::admin::i18n::{
    delete_failed, save_failed, scheduling_content_error_message, tr, TextKey,
};
use crate::admin::server_fns::decos::{
    create_deco, create_deco_group, create_deco_template, create_deco_template_group, delete_deco,
    delete_deco_group, delete_deco_template, delete_deco_template_group, format_short_time,
    get_deco, get_deco_template, get_deco_template_items, list_deco_groups,
    list_deco_template_groups, list_deco_templates, list_decos, load_deco_editor_options,
    save_deco, save_deco_template_with_items, DecoBreakContentForm, DecoEditorOptions, DecoForm,
    DecoGroupRow, DecoOption, DecoPlaylistOption, DecoRow, DecoTemplateForm, DecoTemplateGroupRow,
    DecoTemplateItemForm, DecoTemplateRow, DecoTemplateWithItemsForm,
};
use crate::admin::ui::primitives::{
    dialog::DeleteDialog,
    toast::{use_toast, ToastSeverity},
};
#[cfg(feature = "ssr")]
use crate::{
    admin::server_fns::decos::{
        get_deco_template_items_with_pool, get_deco_template_with_pool, get_deco_with_pool,
        list_deco_groups_with_pool, list_deco_template_groups_with_pool,
        list_deco_templates_with_pool, list_decos_with_pool, load_deco_editor_options_with_pool,
    },
    host::state::AppState,
};

mod style {
    #![allow(dead_code)]
    stylance::import_crate_style!(s, "src/admin/pages/scheduling/templates.module.css");
    pub use s::*;
}

#[component]
pub fn DecosPage() -> impl IntoView {
    view! {
        <style>".decos-noscript-list { display: none; }"</style>
        <noscript>
            <style>".decos-noscript-list { display: block; }"</style>
            <div class="decos-noscript-list">
                <DecosNoScriptRows/>
            </div>
        </noscript>
        <DecosIsland/>
    }
}

#[component]
fn DecosNoScriptRows() -> impl IntoView {
    let (groups, rows) = load_decos_no_script_rows().unwrap_or_default();
    view! {
        <div class=style::page>
            <div class=style::content>
                <section class=style::section>
                    <h2 class=style::section_title>{tr(TextKey::SavedDecos)}</h2>
                    <div class=style::divider></div>
                    <NoScriptDecoTree groups=groups rows=rows/>
                </section>
            </div>
        </div>
    }
}

#[island]
fn DecosIsland() -> impl IntoView {
    let groups: Resource<Vec<DecoGroupRow>> = Resource::new_blocking(
        || (),
        |()| async { list_deco_groups().await.unwrap_or_default() },
    );
    let search = RwSignal::new(String::new());
    let decos: Resource<Vec<DecoRow>> = Resource::new_blocking(
        move || search.get(),
        |query| async move { list_decos(query).await.unwrap_or_default() },
    );

    let new_group_name = RwSignal::new(String::new());
    let new_deco_group_id = RwSignal::new(0i64);
    let new_deco_name = RwSignal::new(String::new());
    let create_group_action = Action::new(|name: &String| create_deco_group(name.clone()));
    let create_deco_action =
        Action::new(|(group_id, name): &(i64, String)| create_deco(*group_id, name.clone()));

    let delete_group_open = RwSignal::new(false);
    let delete_group_target = RwSignal::new(None::<(i64, String)>);
    let delete_deco_open = RwSignal::new(false);
    let delete_deco_target = RwSignal::new(None::<(i64, String)>);
    let delete_group_action = Action::new(|id: &i64| delete_deco_group(*id));
    let delete_deco_action = Action::new(|id: &i64| delete_deco(*id));
    let toast = use_toast();

    Effect::new(move |_| {
        if let Some(result) = create_group_action.value().get() {
            match result {
                Ok(id) => {
                    new_group_name.set(String::new());
                    new_deco_group_id.set(id);
                    groups.refetch();
                    decos.refetch();
                    toast.push(tr(TextKey::DecoGroupCreated), ToastSeverity::Success);
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
        if let Some(result) = create_deco_action.value().get() {
            match result {
                Ok(_) => {
                    new_deco_name.set(String::new());
                    groups.refetch();
                    decos.refetch();
                    toast.push(tr(TextKey::DecoCreated), ToastSeverity::Success);
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
        if let Some(result) = delete_group_action.value().get() {
            match result {
                Ok(()) => {
                    groups.refetch();
                    decos.refetch();
                    toast.push(tr(TextKey::DecoGroupDeleted), ToastSeverity::Success);
                }
                Err(error) => toast.push(
                    delete_failed(scheduling_content_error_message(error)),
                    ToastSeverity::Error,
                ),
            }
        }
    });

    Effect::new(move |_| {
        if let Some(result) = delete_deco_action.value().get() {
            match result {
                Ok(()) => {
                    groups.refetch();
                    decos.refetch();
                    toast.push(tr(TextKey::DecoDeleted), ToastSeverity::Success);
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
                    <h1 class=style::page_title>{tr(TextKey::DecoGroups)}</h1>
                    <div class=style::divider></div>
                    <form method="post" action="/api/decos/groups/create" class=style::form_grid>
                        <label class=style::label for="deco-group-name">{tr(TextKey::DecoGroupName)}</label>
                        <input
                            id="deco-group-name"
                            class=style::input
                            name="name"
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
                            {tr(TextKey::AddDecoGroup)}
                        </button>
                    </form>
                </section>

                <section class=style::section>
                    <h2 class=style::section_title>{tr(TextKey::Decos)}</h2>
                    <div class=style::divider></div>
                    <form method="post" action="/api/decos/create" class=style::form_grid>
                        <label class=style::label for="deco-group">{tr(TextKey::DecoGroup)}</label>
                        <Suspense fallback=move || view! { <select class=style::select disabled=true></select> }>
                            {move || groups.get().map(|rows| view! {
                                <select
                                    id="deco-group"
                                    class=style::select
                                    name="group_id"
                                    prop:value=move || new_deco_group_id.get().to_string()
                                    on:change=move |event| {
                                        if let Ok(id) = event_target_value(&event).parse::<i64>() {
                                            new_deco_group_id.set(id);
                                        }
                                    }
                                >
                                    <option value="0"></option>
                                    {rows.into_iter().map(|group| {
                                        view! { <option value=group.id.to_string()>{group.name}</option> }
                                    }).collect_view()}
                                </select>
                            })}
                        </Suspense>

                        <label class=style::label for="deco-name">{tr(TextKey::DecoName)}</label>
                        <input
                            id="deco-name"
                            class=style::input
                            name="name"
                            prop:value=move || new_deco_name.get()
                            on:input=move |event| new_deco_name.set(event_target_value(&event))
                        />
                        <div></div>
                        <button
                            type="submit"
                            class=style::btn_primary
                            on:click=move |event| {
                                event.prevent_default();
                                let group_id = new_deco_group_id.get_untracked();
                                let name = new_deco_name.get_untracked().trim().to_string();
                                if group_id > 0 && !name.is_empty() {
                                    create_deco_action.dispatch((group_id, name));
                                }
                            }
                        >
                            {tr(TextKey::AddDeco)}
                        </button>
                    </form>
                </section>

                <div class=style::toolbar>
                    <span class=style::filter_icon aria-hidden="true"></span>
                    <input
                        type="search"
                        placeholder=tr(TextKey::SearchForDecos)
                        prop:value=move || search.get()
                        on:input=move |event| search.set(event_target_value(&event))
                    />
                </div>

                <Suspense fallback=move || view! { <p class=style::empty>{tr(TextKey::Loading)}</p> }>
                    {move || decos.get().map(|rows| view! {
                        <DecoTable
                            rows=rows
                            delete_group_open=delete_group_open
                            delete_group_target=delete_group_target
                            delete_deco_open=delete_deco_open
                            delete_deco_target=delete_deco_target
                        />
                    })}
                </Suspense>
            </div>

            <DeleteDialog
                open=delete_group_open
                entity_name=Signal::derive(move || {
                    delete_group_target.get().map(|(_, name)| name).unwrap_or_default()
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
                open=delete_deco_open
                entity_name=Signal::derive(move || {
                    delete_deco_target.get().map(|(_, name)| name).unwrap_or_default()
                })
                on_confirm=Callback::new(move |confirmed: bool| {
                    if let Some((id, _)) = delete_deco_target.get_untracked() {
                        if confirmed {
                            delete_deco_action.dispatch(id);
                        } else {
                            delete_deco_open.set(false);
                        }
                    } else {
                        delete_deco_open.set(false);
                    }
                })
            />
        </div>
    }
}

#[component]
fn DecoTable(
    rows: Vec<DecoRow>,
    delete_group_open: RwSignal<bool>,
    delete_group_target: RwSignal<Option<(i64, String)>>,
    delete_deco_open: RwSignal<bool>,
    delete_deco_target: RwSignal<Option<(i64, String)>>,
) -> impl IntoView {
    if rows.is_empty() {
        return view! { <p class=style::empty>{tr(TextKey::NoDecosFound)}</p> }.into_any();
    }

    let mut grouped: BTreeMap<(String, i64), Vec<DecoRow>> = BTreeMap::new();
    for row in rows {
        grouped
            .entry((row.group_name.clone(), row.group_id))
            .or_default()
            .push(row);
    }

    view! {
        <div class=style::tree_wrap>
            {grouped.into_iter().map(|((group_name, group_id), rows)| {
                        let delete_group_name = group_name.clone();
                        view! {
                            <div class=format!("{} {}", style::tree_row, style::group)>
                                <div class=style::tree_label>
                                    <span class=style::tree_toggle>"▾"</span>
                                    <span>{group_name.clone()}</span>
                                </div>
                                <div class=style::header_actions>
                                    <form method="post" action="/api/decos/groups/delete">
                                        <input type="hidden" name="id" value=group_id.to_string()/>
                                        <button
                                            type="submit"
                                            title=tr(TextKey::DeleteDecoGroup)
                                            class=format!("{} {}", style::icon_button, style::danger)
                                            on:click=move |event| {
                                                event.prevent_default();
                                                delete_group_target.set(Some((group_id, delete_group_name.clone())));
                                                delete_group_open.set(true);
                                            }
                                        >
                                            <span class=style::delete_icon aria-hidden="true"></span>
                                        </button>
                                    </form>
                                </div>
                            </div>
                            <div class=style::tree_children>
                                {rows.into_iter().map(|row| {
                                    let id = row.id;
                                    let name = row.name.clone();
                                    let delete_name = name.clone();
                                    view! {
                                        <div class=style::tree_row>
                                            <div class=style::tree_label>
                                                <span class=style::tree_toggle></span>
                                                {if id >= 0 {
                                                    let href = format!("/decos/{id}");
                                                    view! { <a href=href>{name}</a> }.into_any()
                                                } else {
                                                    view! { <span>{name}</span> }.into_any()
                                                }}
                                            </div>
                                            <div class=style::actions>
                                                {if id >= 0 {
                                                    let href = format!("/decos/{id}");
                                                    view! {
                                                        <a class=style::icon_button title=tr(TextKey::EditDeco) href=href>
                                                            <span class=style::edit_icon aria-hidden="true"></span>
                                                        </a>
                                                        <form method="post" action="/api/decos/delete">
                                                            <input type="hidden" name="id" value=id.to_string()/>
                                                            <button
                                                                type="submit"
                                                                class=format!("{} {}", style::icon_button, style::danger)
                                                                title=tr(TextKey::DeleteDeco)
                                                                on:click=move |event| {
                                                                    event.prevent_default();
                                                                    delete_deco_target.set(Some((id, delete_name.clone())));
                                                                    delete_deco_open.set(true);
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
                                        </div>
                                    }
                                }).collect_view()}
                            </div>
                        }
                    }).collect_view()}
        </div>
    }.into_any()
}

#[component]
fn NoScriptDecoTree(groups: Vec<DecoGroupRow>, rows: Vec<DecoRow>) -> impl IntoView {
    let mut grouped: BTreeMap<(String, i64), Vec<DecoRow>> = BTreeMap::new();
    for row in rows {
        grouped
            .entry((row.group_name.clone(), row.group_id))
            .or_default()
            .push(row);
    }
    for group in groups.into_iter().filter(|group| group.deco_count == 0) {
        grouped.entry((group.name, group.id)).or_default();
    }

    view! {
        <div class=style::tree_wrap>
            {grouped.into_iter().map(|((group_name, _), rows)| view! {
                <div class=format!("{} {}", style::tree_row, style::group)>
                    <div class=style::tree_label>
                        <span class=style::tree_toggle>"▾"</span>
                        <span>{group_name}</span>
                    </div>
                    <div></div>
                </div>
                <div class=style::tree_children>
                    {rows.into_iter().map(|row| {
                        let href = format!("/decos/{}", row.id);
                        view! {
                            <div class=style::tree_row>
                                <div class=style::tree_label>
                                    <span class=style::tree_toggle></span>
                                    <a href=href>{row.name}</a>
                                </div>
                                <div></div>
                            </div>
                        }
                    }).collect_view()}
                </div>
            }).collect_view()}
        </div>
    }
}
#[component]
pub fn DecoEditorPage() -> impl IntoView {
    let params = use_params_map();
    let deco_id = move || {
        params.with(|p| {
            p.get("id")
                .and_then(|value| value.parse::<i64>().ok())
                .unwrap_or(0)
        })
    };

    view! {
        <style>".deco-noscript-editor, main > form#deco-editor-form { display: none; }"</style>
        <noscript>
            <style>".deco-noscript-editor, main > form#deco-editor-form { display: block; } .deco-js-editor { display: none; }"</style>
            <div class="deco-noscript-editor">
                <DecoEditorNoScript deco_id=deco_id()/>
            </div>
        </noscript>
        <div class="deco-js-editor">
            <DecoEditorIsland deco_id=deco_id()/>
        </div>
    }
}

#[island]
fn DecoEditorIsland(deco_id: i64) -> impl IntoView {
    let data: Resource<(Option<DecoForm>, DecoEditorOptions)> = Resource::new_blocking(
        move || deco_id,
        |id| async move {
            let deco = get_deco(id).await.unwrap_or(None);
            let options = load_deco_editor_options().await.unwrap_or_default();
            (deco, options)
        },
    );

    view! {
        <Suspense fallback=move || view! { <p class=style::empty>{tr(TextKey::Loading)}</p> }>
            {move || data.get().map(|(deco, options)| match deco {
                Some(deco) => view! { <DecoEditorForm initial=deco options=options/> }.into_any(),
                None => view! {
                    <p class=style::empty>
                        {tr(TextKey::DecoNotFound)} " "
                        <a href="/decos">{tr(TextKey::BackToDecos)}</a>
                    </p>
                }.into_any(),
            })}
        </Suspense>
    }
}

#[component]
fn DecoEditorNoScript(deco_id: i64) -> impl IntoView {
    match load_deco_editor_no_script_data(deco_id) {
        Some((deco, options)) => {
            view! { <DecoEditorForm initial=deco options=options/> }.into_any()
        }
        None => view! {
            <p class=style::empty>
                {tr(TextKey::DecoNotFound)} " "
                <a href="/decos">{tr(TextKey::BackToDecos)}</a>
            </p>
        }
        .into_any(),
    }
}

#[component]
fn DecoEditorForm(initial: DecoForm, options: DecoEditorOptions) -> impl IntoView {
    let form = RwSignal::new(initial);
    let selected_break_index = RwSignal::new(None::<usize>);
    let selected_playlist_group = RwSignal::new(0i64);
    let save_action = Action::new(|form: &DecoForm| save_deco(form.clone()));
    let toast = use_toast();

    let add_break_content = move || {
        form.update(|form| {
            form.break_content.push(DecoBreakContentForm {
                id: 0,
                collection_type: 6,
                placement: 0,
                ..DecoBreakContentForm::default()
            });
            selected_break_index.set(Some(form.break_content.len().saturating_sub(1)));
        });
    };

    Effect::new(move |_| {
        if let Some(result) = save_action.value().get() {
            match result {
                Ok(()) => {
                    toast.push(tr(TextKey::DecoSaved), ToastSeverity::Success);
                    #[cfg(target_arch = "wasm32")]
                    {
                        let _ = window().location().set_href("/decos");
                    }
                }
                Err(error) => toast.push(
                    save_failed(scheduling_content_error_message(error)),
                    ToastSeverity::Error,
                ),
            }
        }
    });

    let DecoEditorOptions {
        watermarks,
        graphics_elements,
        collections,
        shows,
        seasons,
        artists,
        multi_collections,
        smart_collections,
        playlist_groups,
        playlists,
    } = options;

    view! {
        <div class=style::page>
            <div class=style::content>
                <div class=style::action_bar>
                    <div class=style::action_row>
                        <button
                            type="submit"
                            form="deco-editor-form"
                            name="action"
                            value="save"
                            class=style::btn_primary
                            on:click=move |event| {
                                event.prevent_default();
                                save_action.dispatch(form.get_untracked());
                            }
                        >
                            {tr(TextKey::Save)}
                        </button>
                        <span class=style::action_title>{move || format!("{}: {}", tr(TextKey::Deco), form.get().name)}</span>
                        <a class=style::btn_default href="/decos">{tr(TextKey::Back)}</a>
                    </div>
                </div>

                <form id="deco-editor-form" method="post" action="/api/decos/save">
                    {move || hidden_deco_inputs(form.get())}

                    <section class=style::section>
                        <h1 class=style::page_title>{tr(TextKey::Deco)}</h1>
                        <div class=style::divider></div>
                        <div class=style::form_grid>
                            <label class=style::label>{tr(TextKey::DecoGroupName)}</label>
                            <input
                                class=style::input
                                disabled=true
                                value=move || form.get().group_name
                                prop:value=move || form.get().group_name
                            />
                            <label class=style::label for="deco-editor-name">{tr(TextKey::DecoName)}</label>
                            <input
                                id="deco-editor-name"
                                class=style::input
                                name="name"
                                value=move || form.get().name
                                prop:value=move || form.get().name
                                on:input=move |event| {
                                    let value = event_target_value(&event);
                                    form.update(|form| form.name = value);
                                }
                            />
                        </div>
                    </section>

                    <DecoModeAndMultiSelectSection
                        title_key=TextKey::Watermark
                        mode_label_key=TextKey::WatermarkMode
                        item_label_key=TextKey::Watermark
                        use_during_filler_label_key=TextKey::UseWatermarkDuringFiller
                        options=watermarks.clone()
                        form=form
                        target="watermark"
                    />

                    <DecoModeAndMultiSelectSection
                        title_key=TextKey::GraphicsElements
                        mode_label_key=TextKey::GraphicsElementsMode
                        item_label_key=TextKey::GraphicsElements
                        use_during_filler_label_key=TextKey::UseGraphicsElementsDuringFiller
                        options=graphics_elements.clone()
                        form=form
                        target="graphics"
                    />

                    <section class=style::section>
                        <h2 class=style::section_title>{tr(TextKey::BreakContent)}</h2>
                        <div class=style::divider></div>
                        <p class=style::label>{tr(TextKey::BreakContentBody)}</p>
                        <div class=style::form_grid>
                            <label class=style::label for="deco-break-mode">{tr(TextKey::BreakContentMode)}</label>
                            <select
                                id="deco-break-mode"
                                class=style::select
                                name="break_content_mode"
                                prop:value=move || form.get().break_content_mode.to_string()
                                on:change=move |event| {
                                    if let Ok(value) = event_target_value(&event).parse::<i32>() {
                                        form.update(|form| form.break_content_mode = value);
                                    }
                                }
                            >
                                {mode_options(form.get().break_content_mode, false)}
                            </select>
                            <div></div>
                            <button
                                type="button"
                                class=style::btn_primary
                                disabled=move || form.get().break_content_mode != 2
                                on:click=move |_| add_break_content()
                            >
                                {tr(TextKey::AddBreakContent)}
                            </button>
                        </div>

                        <DecoBreakContentTable
                            form=form
                            selected_break_index=selected_break_index
                            collections=collections.clone()
                            shows=shows.clone()
                            seasons=seasons.clone()
                            artists=artists.clone()
                            multi_collections=multi_collections.clone()
                            smart_collections=smart_collections.clone()
                            playlists=playlists.clone()
                        />

                        <DecoBreakContentEditor
                            form=form
                            selected_break_index=selected_break_index
                            collections=collections.clone()
                            shows=shows.clone()
                            seasons=seasons.clone()
                            artists=artists.clone()
                            multi_collections=multi_collections.clone()
                            smart_collections=smart_collections.clone()
                            playlist_groups=playlist_groups.clone()
                            playlists=playlists.clone()
                            selected_playlist_group=selected_playlist_group
                        />
                    </section>

                    <DecoCollectionSection
                        title_key=TextKey::DefaultFiller
                        body_key=TextKey::DefaultFillerBody
                        target="default"
                        form=form
                        collections=collections.clone()
                        shows=shows.clone()
                        seasons=seasons.clone()
                        artists=artists.clone()
                        multi_collections=multi_collections.clone()
                        smart_collections=smart_collections.clone()
                    />

                    <DecoCollectionSection
                        title_key=TextKey::DeadAirFallback
                        body_key=TextKey::DeadAirFallbackBody
                        target="dead-air"
                        form=form
                        collections=collections.clone()
                        shows=shows.clone()
                        seasons=seasons.clone()
                        artists=artists.clone()
                        multi_collections=multi_collections.clone()
                        smart_collections=smart_collections.clone()
                    />

                    <button type="submit" name="action" value="save" class=style::hidden>
                        {tr(TextKey::Save)}
                    </button>
                </form>
            </div>
        </div>
    }
}

#[component]
pub fn DecoTemplatesPage() -> impl IntoView {
    view! {
        <style>".deco-templates-noscript-list { display: none; }"</style>
        <noscript>
            <style>".deco-templates-noscript-list { display: block; }"</style>
            <div class="deco-templates-noscript-list">
                <DecoTemplatesNoScriptRows/>
            </div>
        </noscript>
        <DecoTemplatesIsland/>
    }
}

#[component]
fn DecoTemplatesNoScriptRows() -> impl IntoView {
    let (groups, rows) = load_deco_templates_no_script_rows().unwrap_or_default();
    view! {
        <div class=style::page>
            <div class=style::content>
                <section class=style::section>
                    <h2 class=style::section_title>{tr(TextKey::SavedDecoTemplates)}</h2>
                    <div class=style::divider></div>
                    <NoScriptDecoTemplateTree groups=groups rows=rows/>
                </section>
            </div>
        </div>
    }
}

#[island]
fn DecoTemplatesIsland() -> impl IntoView {
    let groups: Resource<Vec<DecoTemplateGroupRow>> = Resource::new_blocking(
        || (),
        |()| async { list_deco_template_groups().await.unwrap_or_default() },
    );
    let search = RwSignal::new(String::new());
    let templates: Resource<Vec<DecoTemplateRow>> = Resource::new_blocking(
        move || search.get(),
        |query| async move { list_deco_templates(query).await.unwrap_or_default() },
    );

    let new_group_name = RwSignal::new(String::new());
    let new_template_group_id = RwSignal::new(0i64);
    let new_template_name = RwSignal::new(String::new());
    let create_group_action = Action::new(|name: &String| create_deco_template_group(name.clone()));
    let create_template_action = Action::new(|(group_id, name): &(i64, String)| {
        create_deco_template(*group_id, name.clone())
    });
    let delete_group_open = RwSignal::new(false);
    let delete_group_target = RwSignal::new(None::<(i64, String)>);
    let delete_template_open = RwSignal::new(false);
    let delete_template_target = RwSignal::new(None::<(i64, String)>);
    let delete_group_action = Action::new(|id: &i64| delete_deco_template_group(*id));
    let delete_template_action = Action::new(|id: &i64| delete_deco_template(*id));
    let toast = use_toast();

    Effect::new(move |_| {
        if let Some(result) = create_group_action.value().get() {
            match result {
                Ok(id) => {
                    new_group_name.set(String::new());
                    new_template_group_id.set(id);
                    groups.refetch();
                    templates.refetch();
                    toast.push(
                        tr(TextKey::DecoTemplateGroupCreated),
                        ToastSeverity::Success,
                    );
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
                    groups.refetch();
                    templates.refetch();
                    toast.push(tr(TextKey::DecoTemplateCreated), ToastSeverity::Success);
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
        if let Some(result) = delete_group_action.value().get() {
            match result {
                Ok(()) => {
                    groups.refetch();
                    templates.refetch();
                    toast.push(
                        tr(TextKey::DecoTemplateGroupDeleted),
                        ToastSeverity::Success,
                    );
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
                    toast.push(tr(TextKey::DecoTemplateDeleted), ToastSeverity::Success);
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
                    <h1 class=style::page_title>{tr(TextKey::DecoTemplateGroups)}</h1>
                    <div class=style::divider></div>
                    <form method="post" action="/api/deco-templates/groups/create" class=style::form_grid>
                        <label class=style::label for="deco-template-group-name">{tr(TextKey::DecoTemplateGroupName)}</label>
                        <input
                            id="deco-template-group-name"
                            class=style::input
                            name="name"
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
                            {tr(TextKey::AddDecoTemplateGroup)}
                        </button>
                    </form>
                </section>

                <section class=style::section>
                    <h2 class=style::section_title>{tr(TextKey::DecoTemplates)}</h2>
                    <div class=style::divider></div>
                    <form method="post" action="/api/deco-templates/create" class=style::form_grid>
                        <label class=style::label for="deco-template-group">{tr(TextKey::DecoTemplateGroup)}</label>
                        <Suspense fallback=move || view! { <select class=style::select disabled=true></select> }>
                            {move || groups.get().map(|rows| view! {
                                <select
                                    id="deco-template-group"
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
                                    {rows.into_iter().map(|group| {
                                        view! { <option value=group.id.to_string()>{group.name}</option> }
                                    }).collect_view()}
                                </select>
                            })}
                        </Suspense>

                        <label class=style::label for="deco-template-name">{tr(TextKey::DecoTemplateName)}</label>
                        <input
                            id="deco-template-name"
                            class=style::input
                            name="name"
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
                            {tr(TextKey::AddDecoToTemplate)}
                        </button>
                    </form>
                </section>

                <div class=style::toolbar>
                    <span class=style::filter_icon aria-hidden="true"></span>
                    <input
                        type="search"
                        placeholder=tr(TextKey::SearchForDecoTemplates)
                        prop:value=move || search.get()
                        on:input=move |event| search.set(event_target_value(&event))
                    />
                </div>

                <Suspense fallback=move || view! { <p class=style::empty>{tr(TextKey::Loading)}</p> }>
                    {move || templates.get().map(|rows| view! {
                        <DecoTemplateTable
                            rows=rows
                            delete_group_open=delete_group_open
                            delete_group_target=delete_group_target
                            delete_template_open=delete_template_open
                            delete_template_target=delete_template_target
                        />
                    })}
                </Suspense>
            </div>

            <DeleteDialog
                open=delete_group_open
                entity_name=Signal::derive(move || {
                    delete_group_target.get().map(|(_, name)| name).unwrap_or_default()
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
                open=delete_template_open
                entity_name=Signal::derive(move || {
                    delete_template_target.get().map(|(_, name)| name).unwrap_or_default()
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
fn DecoTemplateTable(
    rows: Vec<DecoTemplateRow>,
    delete_group_open: RwSignal<bool>,
    delete_group_target: RwSignal<Option<(i64, String)>>,
    delete_template_open: RwSignal<bool>,
    delete_template_target: RwSignal<Option<(i64, String)>>,
) -> impl IntoView {
    if rows.is_empty() {
        return view! { <p class=style::empty>{tr(TextKey::NoDecoTemplatesFound)}</p> }.into_any();
    }

    let mut grouped: BTreeMap<(String, i64), Vec<DecoTemplateRow>> = BTreeMap::new();
    for row in rows {
        grouped
            .entry((row.group_name.clone(), row.group_id))
            .or_default()
            .push(row);
    }

    view! {
        <div class=style::tree_wrap>
            {grouped.into_iter().map(|((group_name, group_id), rows)| {
                        let delete_group_name = group_name.clone();
                        view! {
                            <div class=format!("{} {}", style::tree_row, style::group)>
                                <div class=style::tree_label>
                                    <span class=style::tree_toggle>"▾"</span>
                                    <span>{group_name.clone()}</span>
                                </div>
                                <div class=style::header_actions>
                                    <form method="post" action="/api/deco-templates/groups/delete">
                                        <input type="hidden" name="id" value=group_id.to_string()/>
                                        <button
                                            type="submit"
                                            title=tr(TextKey::DeleteDecoTemplateGroup)
                                            class=format!("{} {}", style::icon_button, style::danger)
                                            on:click=move |event| {
                                                event.prevent_default();
                                                delete_group_target.set(Some((group_id, delete_group_name.clone())));
                                                delete_group_open.set(true);
                                            }
                                        >
                                            <span class=style::delete_icon aria-hidden="true"></span>
                                        </button>
                                    </form>
                                </div>
                            </div>
                            <div class=style::tree_children>
                                {rows.into_iter().map(|row| {
                                    let id = row.id;
                                    let name = row.name.clone();
                                    let delete_name = name.clone();
                                    view! {
                                        <div class=style::tree_row>
                                            <div class=style::tree_label>
                                                <span class=style::tree_toggle></span>
                                                {if id >= 0 {
                                                    let href = format!("/deco-templates/{id}");
                                                    view! { <a href=href>{name}</a> }.into_any()
                                                } else {
                                                    view! { <span>{name}</span> }.into_any()
                                                }}
                                            </div>
                                            <div class=style::actions>
                                                {if id >= 0 {
                                                    let href = format!("/deco-templates/{id}");
                                                    view! {
                                                        <a class=style::icon_button title=tr(TextKey::EditDecoTemplate) href=href>
                                                            <span class=style::edit_icon aria-hidden="true"></span>
                                                        </a>
                                                        <form method="post" action="/api/deco-templates/delete">
                                                            <input type="hidden" name="id" value=id.to_string()/>
                                                            <button
                                                                type="submit"
                                                                class=format!("{} {}", style::icon_button, style::danger)
                                                                title=tr(TextKey::DeleteDecoTemplate)
                                                                on:click=move |event| {
                                                                    event.prevent_default();
                                                                    delete_template_target.set(Some((id, delete_name.clone())));
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
                                        </div>
                                    }
                                }).collect_view()}
                            </div>
                        }
                    }).collect_view()}
        </div>
    }.into_any()
}

#[component]
fn NoScriptDecoTemplateTree(
    groups: Vec<DecoTemplateGroupRow>,
    rows: Vec<DecoTemplateRow>,
) -> impl IntoView {
    let mut grouped: BTreeMap<(String, i64), Vec<DecoTemplateRow>> = BTreeMap::new();
    for row in rows {
        grouped
            .entry((row.group_name.clone(), row.group_id))
            .or_default()
            .push(row);
    }
    for group in groups.into_iter().filter(|group| group.template_count == 0) {
        grouped.entry((group.name, group.id)).or_default();
    }

    view! {
        <div class=style::tree_wrap>
            {grouped.into_iter().map(|((group_name, _), rows)| view! {
                <div class=format!("{} {}", style::tree_row, style::group)>
                    <div class=style::tree_label>
                        <span class=style::tree_toggle>"▾"</span>
                        <span>{group_name}</span>
                    </div>
                    <div></div>
                </div>
                <div class=style::tree_children>
                    {rows.into_iter().map(|row| {
                        let href = format!("/deco-templates/{}", row.id);
                        view! {
                            <div class=style::tree_row>
                                <div class=style::tree_label>
                                    <span class=style::tree_toggle></span>
                                    <a href=href>{row.name}</a>
                                </div>
                                <div></div>
                            </div>
                        }
                    }).collect_view()}
                </div>
            }).collect_view()}
        </div>
    }
}
#[component]
pub fn DecoTemplateEditorPage() -> impl IntoView {
    let params = use_params_map();
    let template_id = move || {
        params.with(|p| {
            p.get("id")
                .and_then(|value| value.parse::<i64>().ok())
                .unwrap_or(0)
        })
    };

    view! {
        <style>".deco-template-noscript-editor, main > form#deco-template-editor-form { display: none; }"</style>
        <noscript>
            <style>".deco-template-noscript-editor, main > form#deco-template-editor-form { display: block; } .deco-template-js-editor { display: none; }"</style>
            <div class="deco-template-noscript-editor">
                <DecoTemplateEditorNoScript template_id=template_id()/>
            </div>
        </noscript>
        <div class="deco-template-js-editor">
            <DecoTemplateEditorIsland template_id=template_id()/>
        </div>
    }
}

#[island]
fn DecoTemplateEditorIsland(template_id: i64) -> impl IntoView {
    let data: Resource<(
        Option<DecoTemplateForm>,
        Vec<DecoTemplateItemForm>,
        Vec<DecoGroupRow>,
    )> = Resource::new_blocking(
        move || template_id,
        |id| async move {
            let template = get_deco_template(id).await.unwrap_or(None);
            let items = get_deco_template_items(id).await.unwrap_or_default();
            let groups = list_deco_groups().await.unwrap_or_default();
            (template, items, groups)
        },
    );

    view! {
        <Suspense fallback=move || view! { <p class=style::empty>{tr(TextKey::Loading)}</p> }>
            {move || data.get().map(|(template, items, groups)| {
                match template {
                    Some(template) => view! {
                        <DecoTemplateEditorForm template=template initial_items=items deco_groups=groups/>
                    }.into_any(),
                    None => view! {
                        <p class=style::empty>
                            {tr(TextKey::DecoTemplateNotFound)} " "
                            <a href="/deco-templates">{tr(TextKey::BackToDecoTemplates)}</a>
                        </p>
                    }.into_any(),
                }
            })}
        </Suspense>
    }
}

#[component]
fn DecoTemplateEditorNoScript(template_id: i64) -> impl IntoView {
    match load_deco_template_editor_no_script_data(template_id) {
        Some((template, items, groups)) => view! {
            <DecoTemplateEditorForm template=template initial_items=items deco_groups=groups/>
        }
        .into_any(),
        None => view! {
            <p class=style::empty>
                {tr(TextKey::DecoTemplateNotFound)} " "
                <a href="/deco-templates">{tr(TextKey::BackToDecoTemplates)}</a>
            </p>
        }
        .into_any(),
    }
}

#[component]
fn DecoTemplateEditorForm(
    template: DecoTemplateForm,
    initial_items: Vec<DecoTemplateItemForm>,
    deco_groups: Vec<DecoGroupRow>,
) -> impl IntoView {
    let template_id = template.id;
    let group_id = template.group_id;
    let group_name = template.group_name.clone();
    let name = RwSignal::new(template.name.clone());
    let items = RwSignal::new(initial_items);
    let selected_deco_group = RwSignal::new(0i64);
    let decos: Resource<Vec<DecoRow>> = Resource::new(
        move || selected_deco_group.get(),
        |group_id| async move {
            if group_id <= 0 {
                Vec::new()
            } else {
                list_decos(String::new())
                    .await
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|row| row.group_id == group_id && row.id >= 0)
                    .collect()
            }
        },
    );
    let selected_deco = RwSignal::new(None::<DecoRow>);
    let selected_start = RwSignal::new(0i32);
    let duration_hours = RwSignal::new(0i32);
    let duration_minutes = RwSignal::new(15i32);
    let deco_to_remove = RwSignal::new(None::<usize>);
    let drag_index = RwSignal::new(None::<usize>);
    let toast = use_toast();
    let save_action =
        Action::new(|form: &DecoTemplateWithItemsForm| save_deco_template_with_items(form.clone()));

    let add_deco = move || {
        if let Some(deco) = selected_deco.get_untracked() {
            let duration = (duration_hours.get_untracked().max(0) * 60
                + duration_minutes.get_untracked().max(0))
            .max(5);
            let mut start = selected_start.get_untracked().clamp(0, 24 * 60);
            while start + duration <= 24 * 60 {
                let end = start + duration;
                if !intersects_any(&items.get_untracked(), start, end, None) {
                    items.update(|rows| {
                        rows.push(DecoTemplateItemForm {
                            deco_id: deco.id,
                            deco_name: deco.name.clone(),
                            start_minutes: start,
                            end_minutes: end,
                        });
                        rows.sort_by_key(|row| (row.start_minutes, row.end_minutes, row.deco_id));
                    });
                    return;
                }
                start += 5;
            }

            toast.push(tr(TextKey::DecoCannotBeAdded), ToastSeverity::Error);
        }
    };

    let move_template_item = move |index: usize, new_start: i32| {
        let current = items.get_untracked();
        let Some(item) = current.get(index).cloned() else {
            return;
        };
        let duration = item.end_minutes - item.start_minutes;
        let new_end = new_start + duration;
        if new_start < 0
            || new_end > 24 * 60
            || intersects_any(&current, new_start, new_end, Some(index))
        {
            toast.push(tr(TextKey::DecoCannotBeMoved), ToastSeverity::Error);
            return;
        }

        items.update(|rows| {
            if let Some(row) = rows.get_mut(index) {
                row.start_minutes = new_start;
                row.end_minutes = new_end;
            }
            rows.sort_by_key(|row| (row.start_minutes, row.end_minutes, row.deco_id));
        });
    };
    #[cfg(not(target_arch = "wasm32"))]
    let _ = &move_template_item;

    let on_timeline_drop = move |event: ev::DragEvent| {
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
            let Some(new_start) =
                snap_drop_minutes(f64::from(event.client_y()) - rect.top(), rect.height())
            else {
                return;
            };
            move_template_item(index, new_start);
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = index;
        }
    };

    let remove_deco = move || {
        if let Some(index) = deco_to_remove.get_untracked() {
            items.update(|rows| {
                if index < rows.len() {
                    rows.remove(index);
                }
            });
            deco_to_remove.set(None);
        }
    };

    Effect::new(move |_| {
        if let Some(result) = save_action.value().get() {
            match result {
                Ok(()) => {
                    toast.push(tr(TextKey::DecoTemplateSaved), ToastSeverity::Success);
                    #[cfg(target_arch = "wasm32")]
                    {
                        let _ = window().location().set_href("/deco-templates");
                    }
                }
                Err(error) => toast.push(
                    save_failed(scheduling_content_error_message(error)),
                    ToastSeverity::Error,
                ),
            }
        }
    });

    view! {
        <div class=style::page>
            <div class=style::content>
                <div class=style::action_bar>
                    <div class=style::action_row>
                        <a class=style::btn_default href="/deco-templates">{tr(TextKey::Back)}</a>
                        <span class=style::action_title>{move || format!("{}: {}", tr(TextKey::DecoTemplate), name.get())}</span>
                        <button
                            type="submit"
                            form="deco-template-editor-form"
                            name="action"
                            value="save"
                            class=style::btn_primary
                            on:click=move |event| {
                                event.prevent_default();
                                save_action.dispatch(DecoTemplateWithItemsForm {
                                    id: template_id,
                                    group_id,
                                    name: name.get_untracked().trim().to_string(),
                                    items: items.get_untracked(),
                                });
                            }
                        >
                            {tr(TextKey::Save)}
                        </button>
                    </div>
                </div>

                <noscript>
                    <form method="post" action="/api/deco-templates/items/save">
                        <input type="hidden" name="template_id" value=template_id.to_string()/>
                        <input type="hidden" name="group_id" value=group_id.to_string()/>
                        <button type="submit" name="action" value="save">{tr(TextKey::SaveTemplate)}</button>
                    </form>
                </noscript>
                <form
                    id="deco-template-editor-form"
                    method="post"
                    action="/api/deco-templates/items/save"
                >
                    <input type="hidden" name="template_id" value=template_id.to_string()/>
                    <input type="hidden" name="group_id" value=group_id.to_string()/>
                    <section class=style::section>
                    <h1 class=style::page_title>{tr(TextKey::DecoTemplate)}</h1>
                    <div class=style::divider></div>
                    <div class=style::form_grid>
                        <label class=style::label>{tr(TextKey::DecoTemplateGroupName)}</label>
                        <input class=style::input value=group_name disabled=true/>
                        <label class=style::label for="deco-template-editor-name">{tr(TextKey::DecoTemplateName)}</label>
                        <input
                            id="deco-template-editor-name"
                            class=style::input
                            name="name"
                            value=move || name.get()
                            prop:value=move || name.get()
                            on:input=move |event| name.set(event_target_value(&event))
                        />
                    </div>
                </section>

                <section class=style::section>
                    <h2 class=style::section_title>{tr(TextKey::Decos)}</h2>
                    <div class=style::divider></div>
                    <div class=style::form_grid>
                        <label class=style::label for="template-deco-group">{tr(TextKey::DecoGroup)}</label>
                        <select
                            id="template-deco-group"
                            class=style::select
                            prop:value=move || selected_deco_group.get().to_string()
                            on:change=move |event| {
                                if let Ok(id) = event_target_value(&event).parse::<i64>() {
                                    selected_deco_group.set(id);
                                    selected_deco.set(None);
                                }
                            }
                        >
                            <option value="0"></option>
                            {deco_groups.into_iter().map(|group| {
                                view! { <option value=group.id.to_string()>{group.name}</option> }
                            }).collect_view()}
                        </select>

                        <label class=style::label for="template-deco">{tr(TextKey::Deco)}</label>
                        <Suspense fallback=move || view! { <select class=style::select disabled=true></select> }>
                            {move || decos.get().map(|rows| {
                                let selected_value = selected_deco.get().map(|row| row.id).unwrap_or_default();
                                view! {
                                    <select
                                        id="template-deco"
                                        class=style::select
                                        prop:value=selected_value.to_string()
                                        on:change=move |event| {
                                            if let Ok(id) = event_target_value(&event).parse::<i64>() {
                                                let found = decos
                                                    .get_untracked()
                                                    .unwrap_or_default()
                                                    .into_iter()
                                                    .find(|row| row.id == id);
                                                selected_deco.set(found);
                                            }
                                        }
                                    >
                                        <option value="0"></option>
                                        {rows.into_iter().map(|deco| {
                                            view! { <option value=deco.id.to_string()>{deco.name}</option> }
                                        }).collect_view()}
                                    </select>
                                }
                            })}
                        </Suspense>

                        <label class=style::label for="template-deco-start">{tr(TextKey::StartTimeOnOrAfter)}</label>
                        <select
                            id="template-deco-start"
                            class=style::select
                            prop:value=move || selected_start.get().to_string()
                            on:change=move |event| {
                                if let Ok(minutes) = event_target_value(&event).parse::<i32>() {
                                    selected_start.set(minutes);
                                }
                            }
                        >
                            {(0..(24 * 60)).step_by(5).map(|minutes| {
                                view! { <option value=minutes.to_string()>{format_short_time(minutes)}</option> }
                            }).collect_view()}
                        </select>

                        <label class=style::label for="template-deco-duration-hours">{tr(TextKey::Duration)}</label>
                        <input
                            id="template-deco-duration-hours"
                            class=style::input
                            type="number"
                            min="0"
                            max="24"
                            name="duration_hours"
                            value=move || duration_hours.get().to_string()
                            prop:value=move || duration_hours.get().to_string()
                            on:input=move |event| {
                                let value = event_target_value(&event).parse::<i32>().unwrap_or_default();
                                duration_hours.set(value.clamp(0, 24));
                            }
                        />

                        <div></div>
                        <select
                            id="template-deco-duration-minutes"
                            class=style::select
                            prop:value=move || duration_minutes.get().to_string()
                            on:change=move |event| {
                                if let Ok(value) = event_target_value(&event).parse::<i32>() {
                                    duration_minutes.set(value.clamp(0, 55));
                                }
                            }
                        >
                            {(0..60).step_by(5).map(|minutes| {
                                view! { <option value=minutes.to_string()>{format!("{} {}", minutes, tr(TextKey::Minutes))}</option> }
                            }).collect_view()}
                        </select>

                        <div></div>
                        <button
                            type="button"
                            class=style::btn_primary
                            disabled=move || selected_deco.get().is_none()
                            on:click=move |_| add_deco()
                        >
                            {tr(TextKey::AddDecoTemplate)}
                        </button>

                        <label class=style::label for="template-deco-remove">{tr(TextKey::DecoToRemove)}</label>
                        <select
                            id="template-deco-remove"
                            class=style::select
                            prop:value=move || deco_to_remove.get().map(|value| value.to_string()).unwrap_or_default()
                            on:change=move |event| {
                                let value = event_target_value(&event);
                                deco_to_remove.set(value.parse::<usize>().ok());
                            }
                        >
                            <option value="">{format!("({})", tr(TextKey::NoneOption))}</option>
                            {move || items.get().into_iter().enumerate().map(|(index, item)| {
                                view! {
                                    <option value=index.to_string()>
                                        {format!(
                                            "{}: {} - {}",
                                            item.deco_name,
                                            format_short_time(item.start_minutes),
                                            format_short_time(item.end_minutes),
                                        )}
                                    </option>
                                }
                            }).collect_view()}
                        </select>

                        <div></div>
                        <button
                            type="button"
                            class=style::btn_primary
                            disabled=move || deco_to_remove.get().is_none()
                            on:click=move |_| remove_deco()
                        >
                            {tr(TextKey::RemoveDecoFromTemplate)}
                        </button>
                    </div>
                </section>

                <h2 class=style::section_title>{tr(TextKey::Content)}</h2>
                <div class=style::divider></div>
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
                            on:drop=on_timeline_drop
                        >
                            {move || items.get().into_iter().enumerate().map(|(index, item)| {
                                let duration = item.end_minutes - item.start_minutes;
                                let top = f64::from(item.start_minutes) / 60.0 * 3.0;
                                let height = (f64::from(duration.max(5)) / 60.0 * 3.0).max(1.5);
                                view! {
                                    <div
                                        class=style::timeline_item
                                        draggable="true"
                                        style=format!("top: {top:.3}rem; height: {height:.3}rem;")
                                        on:dragstart=move |_| drag_index.set(Some(index))
                                        on:dragend=move |_| drag_index.set(None)
                                    >
                                        <strong>{item.deco_name}</strong>
                                        <span class=style::timeline_time>
                                            {format!("{} - {}", format_short_time(item.start_minutes), format_short_time(item.end_minutes))}
                                        </span>
                                    </div>
                                }
                            }).collect_view()}
                        </div>
                    </div>
                </div>
                    {move || hidden_deco_template_item_inputs(items.get())}
                    <button type="submit" name="action" value="save" class=style::hidden>
                        {tr(TextKey::SaveTemplate)}
                    </button>
                </form>
            </div>
        </div>
    }
}

fn hidden_deco_template_item_inputs(items: Vec<DecoTemplateItemForm>) -> impl IntoView {
    view! {
        <>
            {items.into_iter().map(|item| view! {
                <>
                    <input type="hidden" name="item_deco_id" value=item.deco_id.to_string()/>
                    <input type="hidden" name="item_deco_name" value=item.deco_name/>
                    <input type="hidden" name="item_start_minutes" value=item.start_minutes.to_string()/>
                    <input type="hidden" name="item_end_minutes" value=item.end_minutes.to_string()/>
                </>
            }).collect_view()}
        </>
    }
}

fn load_decos_no_script_rows() -> Option<(Vec<DecoGroupRow>, Vec<DecoRow>)> {
    #[cfg(feature = "ssr")]
    {
        let state = expect_context::<AppState>();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                let groups = list_deco_groups_with_pool(&state.pool).await.ok()?;
                let rows = list_decos_with_pool(&state.pool, "").await.ok()?;
                Some((groups, rows))
            })
        })
    }
    #[cfg(not(feature = "ssr"))]
    {
        None
    }
}

fn load_deco_templates_no_script_rows() -> Option<(Vec<DecoTemplateGroupRow>, Vec<DecoTemplateRow>)>
{
    #[cfg(feature = "ssr")]
    {
        let state = expect_context::<AppState>();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                let groups = list_deco_template_groups_with_pool(&state.pool)
                    .await
                    .ok()?;
                let rows = list_deco_templates_with_pool(&state.pool, "").await.ok()?;
                Some((groups, rows))
            })
        })
    }
    #[cfg(not(feature = "ssr"))]
    {
        None
    }
}

fn load_deco_editor_no_script_data(deco_id: i64) -> Option<(DecoForm, DecoEditorOptions)> {
    #[cfg(feature = "ssr")]
    {
        let state = expect_context::<AppState>();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                let deco = get_deco_with_pool(&state.pool, deco_id)
                    .await
                    .ok()
                    .flatten()?;
                let options = load_deco_editor_options_with_pool(&state.pool).await.ok()?;
                Some((deco, options))
            })
        })
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = deco_id;
        None
    }
}

fn load_deco_template_editor_no_script_data(
    template_id: i64,
) -> Option<(
    DecoTemplateForm,
    Vec<DecoTemplateItemForm>,
    Vec<DecoGroupRow>,
)> {
    #[cfg(feature = "ssr")]
    {
        let state = expect_context::<AppState>();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                let template = get_deco_template_with_pool(&state.pool, template_id)
                    .await
                    .ok()
                    .flatten()?;
                let items = get_deco_template_items_with_pool(&state.pool, template_id)
                    .await
                    .ok()?;
                let groups = list_deco_groups_with_pool(&state.pool).await.ok()?;
                Some((template, items, groups))
            })
        })
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = template_id;
        None
    }
}

#[component]
fn DecoModeAndMultiSelectSection(
    title_key: TextKey,
    mode_label_key: TextKey,
    item_label_key: TextKey,
    use_during_filler_label_key: TextKey,
    options: Vec<DecoOption>,
    form: RwSignal<DecoForm>,
    target: &'static str,
) -> impl IntoView {
    let mode = move || match target {
        "watermark" => form.get().watermark_mode,
        _ => form.get().graphics_elements_mode,
    };
    view! {
        <section class=style::section>
            <h2 class=style::section_title>{tr(title_key)}</h2>
            <div class=style::divider></div>
            <div class=style::form_grid>
                <label class=style::label>{tr(mode_label_key)}</label>
                <select
                    class=style::select
                    name=match target {
                        "watermark" => "watermark_mode",
                        _ => "graphics_elements_mode",
                    }
                    prop:value=move || mode().to_string()
                    on:change=move |event| {
                        if let Ok(value) = event_target_value(&event).parse::<i32>() {
                            form.update(|form| match target {
                                "watermark" => form.watermark_mode = value,
                                _ => form.graphics_elements_mode = value,
                            });
                        }
                    }
                >
                    {mode_options(mode(), true)}
                </select>
                <label class=style::label>{tr(item_label_key)}</label>
                <div>
                    {options.into_iter().map(|option| {
                        let id = option.id;
                        view! {
                            <label class=style::label>
                                <input
                                    type="checkbox"
                                    name=match target {
                                        "watermark" => "watermark_ids",
                                        _ => "graphics_element_ids",
                                    }
                                    value=id.to_string()
                                    checked=move || {
                                        let form = form.get();
                                        match target {
                                            "watermark" => form.watermark_ids.contains(&id),
                                            _ => form.graphics_element_ids.contains(&id),
                                        }
                                    }
                                    disabled=move || mode() != 2 && mode() != 3
                                    on:change=move |event| {
                                        let checked = event_target_checked(&event);
                                        form.update(|form| match target {
                                            "watermark" => toggle_id(&mut form.watermark_ids, id, checked),
                                            _ => toggle_id(&mut form.graphics_element_ids, id, checked),
                                        });
                                    }
                                />
                                " " {option.name}
                            </label>
                        }
                    }).collect_view()}
                </div>
                <label class=style::label>{tr(use_during_filler_label_key)}</label>
                <input
                    type="checkbox"
                    name=match target {
                        "watermark" => "use_watermark_during_filler",
                        _ => "use_graphics_elements_during_filler",
                    }
                    value="true"
                    checked=move || match target {
                        "watermark" => form.get().use_watermark_during_filler,
                        _ => form.get().use_graphics_elements_during_filler,
                    }
                    disabled=move || mode() != 2
                    on:change=move |event| {
                        let checked = event_target_checked(&event);
                        form.update(|form| match target {
                            "watermark" => form.use_watermark_during_filler = checked,
                            _ => form.use_graphics_elements_during_filler = checked,
                        });
                    }
                />
            </div>
        </section>
    }
}

#[component]
fn DecoBreakContentTable(
    form: RwSignal<DecoForm>,
    selected_break_index: RwSignal<Option<usize>>,
    collections: Vec<DecoOption>,
    shows: Vec<DecoOption>,
    seasons: Vec<DecoOption>,
    artists: Vec<DecoOption>,
    multi_collections: Vec<DecoOption>,
    smart_collections: Vec<DecoOption>,
    playlists: Vec<DecoPlaylistOption>,
) -> impl IntoView {
    view! {
        <div class=style::table_wrap style:display=move || if form.get().break_content_mode == 2 { "" } else { "none" }>
            <table>
                <thead>
                    <tr>
                        <th>{tr(TextKey::Type)}</th>
                        <th>{tr(TextKey::Name)}</th>
                        <th>{tr(TextKey::Placement)}</th>
                        <th></th>
                    </tr>
                </thead>
                <tbody>
                    {move || form.get().break_content.into_iter().enumerate().map(|(index, item)| {
                        let selected = selected_break_index.get() == Some(index);
                        let name = collection_name_for_item(
                            &item,
                            &collections,
                            &shows,
                            &seasons,
                            &artists,
                            &multi_collections,
                            &smart_collections,
                            &playlists,
                        );
                        view! {
                            <tr
                                class=if selected { style::selected_row } else { "" }
                                on:click=move |_| selected_break_index.set(Some(index))
                            >
                                <td>{collection_type_label(item.collection_type)}</td>
                                <td>{name}</td>
                                <td>{placement_label(item.placement)}</td>
                                <td>
                                    <button
                                        type="button"
                                        class=format!("{} {}", style::icon_button, style::danger)
                                        title=tr(TextKey::RemoveBreakContent)
                                        on:click=move |event| {
                                            event.stop_propagation();
                                            form.update(|form| {
                                                if index < form.break_content.len() {
                                                    form.break_content.remove(index);
                                                }
                                            });
                                            selected_break_index.set(None);
                                        }
                                    >
                                        <span class=style::delete_icon aria-hidden="true"></span>
                                    </button>
                                </td>
                            </tr>
                        }
                    }).collect_view()}
                </tbody>
            </table>
        </div>
    }
}

#[component]
fn DecoBreakContentEditor(
    form: RwSignal<DecoForm>,
    selected_break_index: RwSignal<Option<usize>>,
    collections: Vec<DecoOption>,
    shows: Vec<DecoOption>,
    seasons: Vec<DecoOption>,
    artists: Vec<DecoOption>,
    multi_collections: Vec<DecoOption>,
    smart_collections: Vec<DecoOption>,
    playlist_groups: Vec<DecoOption>,
    playlists: Vec<DecoPlaylistOption>,
    selected_playlist_group: RwSignal<i64>,
) -> impl IntoView {
    view! {
        <div style:display=move || if selected_break_index.get().is_some() { "" } else { "none" }>
            <h2 class=style::section_title>{tr(TextKey::BreakContentItem)}</h2>
            <div class=style::divider></div>
            <div class=style::form_grid>
                <label class=style::label>{tr(TextKey::Placement)}</label>
                <select
                    class=style::select
                    disabled=true
                    prop:value=move || selected_break_item(form, selected_break_index).map(|item| item.placement.to_string()).unwrap_or_default()
                    on:change=move |event| {
                        if let (Some(index), Ok(value)) = (
                            selected_break_index.get_untracked(),
                            event_target_value(&event).parse::<i32>(),
                        ) {
                            form.update(|form| {
                                if let Some(item) = form.break_content.get_mut(index) {
                                    item.placement = value;
                                }
                            });
                        }
                    }
                >
                    {placement_options(selected_break_item(form, selected_break_index).map(|item| item.placement).unwrap_or_default())}
                </select>

                <label class=style::label>{tr(TextKey::CollectionType)}</label>
                <select
                    class=style::select
                    disabled=true
                    prop:value=move || selected_break_item(form, selected_break_index).map(|item| item.collection_type.to_string()).unwrap_or_default()
                    on:change=move |event| {
                        if let (Some(index), Ok(value)) = (
                            selected_break_index.get_untracked(),
                            event_target_value(&event).parse::<i32>(),
                        ) {
                            form.update(|form| {
                                if let Some(item) = form.break_content.get_mut(index) {
                                    item.collection_type = value;
                                }
                            });
                        }
                    }
                >
                    {collection_type_options(selected_break_item(form, selected_break_index).map(|item| item.collection_type).unwrap_or_default(), true)}
                </select>

                <DecoBreakSelectionRows
                    form=form
                    selected_break_index=selected_break_index
                    collections=collections
                    shows=shows
                    seasons=seasons
                    artists=artists
                    multi_collections=multi_collections
                    smart_collections=smart_collections
                    playlist_groups=playlist_groups
                    playlists=playlists
                    selected_playlist_group=selected_playlist_group
                />
            </div>
        </div>
    }
}

#[component]
fn DecoBreakSelectionRows(
    form: RwSignal<DecoForm>,
    selected_break_index: RwSignal<Option<usize>>,
    collections: Vec<DecoOption>,
    shows: Vec<DecoOption>,
    seasons: Vec<DecoOption>,
    artists: Vec<DecoOption>,
    multi_collections: Vec<DecoOption>,
    smart_collections: Vec<DecoOption>,
    playlist_groups: Vec<DecoOption>,
    playlists: Vec<DecoPlaylistOption>,
    selected_playlist_group: RwSignal<i64>,
) -> impl IntoView {
    view! {
        <CollectionSelectRow label_key=TextKey::Collection collection_type=0 form=form selected_break_index=selected_break_index options=collections field="collection"/>
        <CollectionSelectRow label_key=TextKey::TelevisionShow collection_type=1 form=form selected_break_index=selected_break_index options=shows field="media"/>
        <CollectionSelectRow label_key=TextKey::TelevisionSeason collection_type=2 form=form selected_break_index=selected_break_index options=seasons field="media"/>
        <CollectionSelectRow label_key=TextKey::Artist collection_type=3 form=form selected_break_index=selected_break_index options=artists field="media"/>
        <CollectionSelectRow label_key=TextKey::MultiCollection collection_type=4 form=form selected_break_index=selected_break_index options=multi_collections field="multi"/>
        <CollectionSelectRow label_key=TextKey::SmartCollection collection_type=5 form=form selected_break_index=selected_break_index options=smart_collections field="smart"/>
        <label class=style::label style:display=move || if selected_break_item(form, selected_break_index).is_some_and(|item| item.collection_type == 6) { "" } else { "none" }>{tr(TextKey::PlaylistGroup)}</label>
        <select
            class=style::select
            style:display=move || if selected_break_item(form, selected_break_index).is_some_and(|item| item.collection_type == 6) { "" } else { "none" }
            prop:value=move || selected_playlist_group.get().to_string()
            on:change=move |event| {
                if let Ok(value) = event_target_value(&event).parse::<i64>() {
                    selected_playlist_group.set(value);
                }
            }
        >
            <option value="0"></option>
            {playlist_groups.into_iter().map(|option| view! {
                <option value=option.id.to_string()>{option.name}</option>
            }).collect_view()}
        </select>
        <label class=style::label style:display=move || if selected_break_item(form, selected_break_index).is_some_and(|item| item.collection_type == 6) { "" } else { "none" }>{tr(TextKey::Playlist)}</label>
        <select
            class=style::select
            style:display=move || if selected_break_item(form, selected_break_index).is_some_and(|item| item.collection_type == 6) { "" } else { "none" }
            prop:value=move || selected_break_item(form, selected_break_index).and_then(|item| item.playlist_id).unwrap_or_default().to_string()
            on:change=move |event| {
                if let (Some(index), Ok(value)) = (
                    selected_break_index.get_untracked(),
                    event_target_value(&event).parse::<i64>(),
                ) {
                    form.update(|form| {
                        if let Some(item) = form.break_content.get_mut(index) {
                            item.playlist_id = (value > 0).then_some(value);
                        }
                    });
                }
            }
        >
            <option value="0"></option>
            {move || playlists.iter().filter(|playlist| {
                selected_playlist_group.get() == 0 || playlist.group_id == selected_playlist_group.get()
            }).cloned().map(|option| view! {
                <option value=option.id.to_string()>{option.name}</option>
            }).collect_view()}
        </select>
    }
}

#[component]
fn CollectionSelectRow(
    label_key: TextKey,
    collection_type: i32,
    form: RwSignal<DecoForm>,
    selected_break_index: RwSignal<Option<usize>>,
    options: Vec<DecoOption>,
    field: &'static str,
) -> impl IntoView {
    view! {
        <label
            class=style::label
            style:display=move || if selected_break_item(form, selected_break_index).is_some_and(|item| item.collection_type == collection_type) { "" } else { "none" }
        >
            {tr(label_key)}
        </label>
        <select
            class=style::select
            style:display=move || if selected_break_item(form, selected_break_index).is_some_and(|item| item.collection_type == collection_type) { "" } else { "none" }
            prop:value=move || selected_break_item(form, selected_break_index).and_then(|item| selected_id_for_field(&item, field)).unwrap_or_default().to_string()
            on:change=move |event| {
                if let (Some(index), Ok(value)) = (
                    selected_break_index.get_untracked(),
                    event_target_value(&event).parse::<i64>(),
                ) {
                    form.update(|form| {
                        if let Some(item) = form.break_content.get_mut(index) {
                            set_break_field(item, field, (value > 0).then_some(value));
                        }
                    });
                }
            }
        >
            <option value="0"></option>
            {options.into_iter().map(|option| view! {
                <option value=option.id.to_string()>{option.name}</option>
            }).collect_view()}
        </select>
    }
}

#[component]
fn DecoCollectionSection(
    title_key: TextKey,
    body_key: TextKey,
    target: &'static str,
    form: RwSignal<DecoForm>,
    collections: Vec<DecoOption>,
    shows: Vec<DecoOption>,
    seasons: Vec<DecoOption>,
    artists: Vec<DecoOption>,
    multi_collections: Vec<DecoOption>,
    smart_collections: Vec<DecoOption>,
) -> impl IntoView {
    view! {
        <section class=style::section>
            <h2 class=style::section_title>{tr(title_key)}</h2>
            <div class=style::divider></div>
            <p class=style::label>{tr(body_key)}</p>
            <div class=style::form_grid>
                <label class=style::label>{match target {
                    "default" => tr(TextKey::DefaultFillerMode),
                    _ => tr(TextKey::DeadAirFallbackMode),
                }}</label>
                <select
                    class=style::select
                    name=match target {
                        "default" => "default_filler_mode",
                        _ => "dead_air_fallback_mode",
                    }
                    prop:value=move || collection_mode(form, target).to_string()
                    on:change=move |event| {
                        if let Ok(value) = event_target_value(&event).parse::<i32>() {
                            form.update(|form| set_collection_mode(form, target, value));
                        }
                    }
                >
                    {mode_options(collection_mode(form, target), false)}
                </select>
                <label class=style::label>{match target {
                    "default" => tr(TextKey::DefaultFillerCollectionType),
                    _ => tr(TextKey::DeadAirFallbackCollectionType),
                }}</label>
                <select
                    class=style::select
                    name=match target {
                        "default" => "default_filler_collection_type",
                        _ => "dead_air_fallback_collection_type",
                    }
                    disabled=move || collection_mode(form, target) != 2
                    prop:value=move || collection_type_for_target(form, target).to_string()
                    on:change=move |event| {
                        if let Ok(value) = event_target_value(&event).parse::<i32>() {
                            form.update(|form| set_collection_type_for_target(form, target, value));
                        }
                    }
                >
                    {collection_type_options(collection_type_for_target(form, target), false)}
                </select>
                <TargetCollectionSelect label_key=TextKey::Collection collection_type=0 target=target form=form options=collections field="collection"/>
                <TargetCollectionSelect label_key=TextKey::TelevisionShow collection_type=1 target=target form=form options=shows field="media"/>
                <TargetCollectionSelect label_key=TextKey::TelevisionSeason collection_type=2 target=target form=form options=seasons field="media"/>
                <TargetCollectionSelect label_key=TextKey::Artist collection_type=3 target=target form=form options=artists field="media"/>
                <TargetCollectionSelect label_key=TextKey::MultiCollection collection_type=4 target=target form=form options=multi_collections field="multi"/>
                <TargetCollectionSelect label_key=TextKey::SmartCollection collection_type=5 target=target form=form options=smart_collections field="smart"/>
                {if target == "default" {
                    view! {
                        <label class=style::label>{tr(TextKey::TrimToFit)}</label>
                        <input
                            type="checkbox"
                            name="default_filler_trim_to_fit"
                            value="true"
                            checked=move || form.get().default_filler_trim_to_fit
                            disabled=move || form.get().default_filler_mode != 2
                            on:change=move |event| {
                                let checked = event_target_checked(&event);
                                form.update(|form| form.default_filler_trim_to_fit = checked);
                            }
                        />
                    }.into_any()
                } else {
                    view! { <div></div><div></div> }.into_any()
                }}
            </div>
        </section>
    }
}

#[component]
fn TargetCollectionSelect(
    label_key: TextKey,
    collection_type: i32,
    target: &'static str,
    form: RwSignal<DecoForm>,
    options: Vec<DecoOption>,
    field: &'static str,
) -> impl IntoView {
    view! {
        <label
            class=style::label
            style:display=move || if collection_mode(form, target) == 2 && collection_type_for_target(form, target) == collection_type { "" } else { "none" }
        >
            {tr(label_key)}
        </label>
        <select
            class=style::select
            name=target_collection_field_name(target, field)
            style:display=move || if collection_mode(form, target) == 2 && collection_type_for_target(form, target) == collection_type { "" } else { "none" }
            prop:value=move || selected_target_id(form, target, field).unwrap_or_default().to_string()
            on:change=move |event| {
                if let Ok(value) = event_target_value(&event).parse::<i64>() {
                    form.update(|form| set_target_id(form, target, field, (value > 0).then_some(value)));
                }
            }
        >
            <option value="0"></option>
            {options.into_iter().map(|option| view! {
                <option value=option.id.to_string()>{option.name}</option>
            }).collect_view()}
        </select>
    }
}

fn hidden_deco_inputs(form: DecoForm) -> impl IntoView {
    view! {
        <>
            <input type="hidden" name="id" value=form.id.to_string()/>
            <input type="hidden" name="group_id" value=form.group_id.to_string()/>
            {form.break_content.into_iter().map(|item| view! {
                <>
                    <input type="hidden" name="break_content_id" value=item.id.to_string()/>
                    <input type="hidden" name="break_content_collection_type" value=item.collection_type.to_string()/>
                    <input type="hidden" name="break_content_collection_id" value=option_i64(item.collection_id)/>
                    <input type="hidden" name="break_content_media_item_id" value=option_i64(item.media_item_id)/>
                    <input type="hidden" name="break_content_multi_collection_id" value=option_i64(item.multi_collection_id)/>
                    <input type="hidden" name="break_content_smart_collection_id" value=option_i64(item.smart_collection_id)/>
                    <input type="hidden" name="break_content_playlist_id" value=option_i64(item.playlist_id)/>
                    <input type="hidden" name="break_content_placement" value=item.placement.to_string()/>
                </>
            }).collect_view()}
        </>
    }
}

fn intersects_any(
    items: &[DecoTemplateItemForm],
    start: i32,
    end: i32,
    except_index: Option<usize>,
) -> bool {
    items.iter().enumerate().any(|(index, item)| {
        except_index != Some(index) && start < item.end_minutes && item.start_minutes < end
    })
}

#[cfg(target_arch = "wasm32")]
fn snap_drop_minutes(offset_y: f64, grid_height: f64) -> Option<i32> {
    if !offset_y.is_finite() || !grid_height.is_finite() || grid_height <= 0.0 {
        return None;
    }

    let ratio = (offset_y / grid_height).clamp(0.0, 1.0);
    let minutes = (ratio * f64::from(24 * 60)).round() as i32;
    Some((minutes / 5 * 5).clamp(0, 24 * 60))
}

fn mode_options(selected: i32, allow_merge: bool) -> impl IntoView {
    view! {
        <>
            <option value="0" selected=selected == 0>{tr(TextKey::Inherit)}</option>
            <option value="1" selected=selected == 1>{tr(TextKey::Disable)}</option>
            <option value="2" selected=selected == 2>{if allow_merge { tr(TextKey::Replace) } else { tr(TextKey::Override) }}</option>
            {if allow_merge {
                view! { <option value="3" selected=selected == 3>{tr(TextKey::Merge)}</option> }.into_any()
            } else {
                ().into_any()
            }}
        </>
    }
}

fn collection_type_options(selected: i32, include_playlist: bool) -> impl IntoView {
    view! {
        <>
            <option value="0" selected=selected == 0>{tr(TextKey::Collection)}</option>
            <option value="1" selected=selected == 1>{tr(TextKey::TelevisionShow)}</option>
            <option value="2" selected=selected == 2>{tr(TextKey::TelevisionSeason)}</option>
            <option value="3" selected=selected == 3>{tr(TextKey::Artist)}</option>
            <option value="4" selected=selected == 4>{tr(TextKey::MultiCollection)}</option>
            <option value="5" selected=selected == 5>{tr(TextKey::SmartCollection)}</option>
            {if include_playlist {
                view! { <option value="6" selected=selected == 6>{tr(TextKey::Playlist)}</option> }.into_any()
            } else {
                ().into_any()
            }}
        </>
    }
}

fn placement_options(selected: i32) -> impl IntoView {
    view! {
        <>
            <option value="0" selected=selected == 0>{tr(TextKey::BlockStart)}</option>
            <option value="2" selected=selected == 2>{tr(TextKey::BetweenBlockItems)}</option>
            <option value="3" selected=selected == 3>{tr(TextKey::AtChapterMarkers)}</option>
            <option value="1" selected=selected == 1>{tr(TextKey::BlockFinish)}</option>
        </>
    }
}

fn collection_type_label(value: i32) -> &'static str {
    match value {
        0 => tr(TextKey::Collection),
        1 => tr(TextKey::TelevisionShow),
        2 => tr(TextKey::TelevisionSeason),
        3 => tr(TextKey::Artist),
        4 => tr(TextKey::MultiCollection),
        5 => tr(TextKey::SmartCollection),
        6 => tr(TextKey::Playlist),
        _ => "",
    }
}

fn placement_label(value: i32) -> &'static str {
    match value {
        0 => tr(TextKey::BlockStart),
        1 => tr(TextKey::BlockFinish),
        2 => tr(TextKey::BetweenBlockItems),
        3 => tr(TextKey::AtChapterMarkers),
        _ => "",
    }
}

fn option_i64(value: Option<i64>) -> String {
    value
        .filter(|value| *value > 0)
        .map(|value| value.to_string())
        .unwrap_or_default()
}

fn target_collection_field_name(target: &str, field: &str) -> &'static str {
    match (target, field) {
        ("default", "collection") => "default_filler_collection_id",
        ("default", "media") => "default_filler_media_item_id",
        ("default", "multi") => "default_filler_multi_collection_id",
        ("default", "smart") => "default_filler_smart_collection_id",
        (_, "collection") => "dead_air_fallback_collection_id",
        (_, "media") => "dead_air_fallback_media_item_id",
        (_, "multi") => "dead_air_fallback_multi_collection_id",
        (_, "smart") => "dead_air_fallback_smart_collection_id",
        _ => "",
    }
}

fn toggle_id(ids: &mut Vec<i64>, id: i64, checked: bool) {
    if checked {
        if !ids.contains(&id) {
            ids.push(id);
            ids.sort_unstable();
        }
    } else {
        ids.retain(|existing| *existing != id);
    }
}

fn selected_break_item(
    form: RwSignal<DecoForm>,
    selected_break_index: RwSignal<Option<usize>>,
) -> Option<DecoBreakContentForm> {
    selected_break_index
        .get()
        .and_then(|index| form.get().break_content.get(index).cloned())
}

fn selected_id_for_field(item: &DecoBreakContentForm, field: &str) -> Option<i64> {
    match field {
        "collection" => item.collection_id,
        "media" => item.media_item_id,
        "multi" => item.multi_collection_id,
        "smart" => item.smart_collection_id,
        _ => None,
    }
}

fn set_break_field(item: &mut DecoBreakContentForm, field: &str, value: Option<i64>) {
    match field {
        "collection" => item.collection_id = value,
        "media" => item.media_item_id = value,
        "multi" => item.multi_collection_id = value,
        "smart" => item.smart_collection_id = value,
        _ => {}
    }
}

fn collection_mode(form: RwSignal<DecoForm>, target: &str) -> i32 {
    let form = form.get();
    match target {
        "default" => form.default_filler_mode,
        _ => form.dead_air_fallback_mode,
    }
}

fn set_collection_mode(form: &mut DecoForm, target: &str, value: i32) {
    match target {
        "default" => form.default_filler_mode = value,
        _ => form.dead_air_fallback_mode = value,
    }
}

fn collection_type_for_target(form: RwSignal<DecoForm>, target: &str) -> i32 {
    let form = form.get();
    match target {
        "default" => form.default_filler_collection_type,
        _ => form.dead_air_fallback_collection_type,
    }
}

fn set_collection_type_for_target(form: &mut DecoForm, target: &str, value: i32) {
    match target {
        "default" => form.default_filler_collection_type = value,
        _ => form.dead_air_fallback_collection_type = value,
    }
}

fn selected_target_id(form: RwSignal<DecoForm>, target: &str, field: &str) -> Option<i64> {
    let form = form.get();
    match (target, field) {
        ("default", "collection") => form.default_filler_collection_id,
        ("default", "media") => form.default_filler_media_item_id,
        ("default", "multi") => form.default_filler_multi_collection_id,
        ("default", "smart") => form.default_filler_smart_collection_id,
        (_, "collection") => form.dead_air_fallback_collection_id,
        (_, "media") => form.dead_air_fallback_media_item_id,
        (_, "multi") => form.dead_air_fallback_multi_collection_id,
        (_, "smart") => form.dead_air_fallback_smart_collection_id,
        _ => None,
    }
}

fn set_target_id(form: &mut DecoForm, target: &str, field: &str, value: Option<i64>) {
    match (target, field) {
        ("default", "collection") => form.default_filler_collection_id = value,
        ("default", "media") => form.default_filler_media_item_id = value,
        ("default", "multi") => form.default_filler_multi_collection_id = value,
        ("default", "smart") => form.default_filler_smart_collection_id = value,
        (_, "collection") => form.dead_air_fallback_collection_id = value,
        (_, "media") => form.dead_air_fallback_media_item_id = value,
        (_, "multi") => form.dead_air_fallback_multi_collection_id = value,
        (_, "smart") => form.dead_air_fallback_smart_collection_id = value,
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn collection_name_for_item(
    item: &DecoBreakContentForm,
    collections: &[DecoOption],
    shows: &[DecoOption],
    seasons: &[DecoOption],
    artists: &[DecoOption],
    multi_collections: &[DecoOption],
    smart_collections: &[DecoOption],
    playlists: &[DecoPlaylistOption],
) -> String {
    if !item.collection_name.is_empty() {
        return item.collection_name.clone();
    }
    match item.collection_type {
        0 => option_name(collections, item.collection_id),
        1 => option_name(shows, item.media_item_id),
        2 => option_name(seasons, item.media_item_id),
        3 => option_name(artists, item.media_item_id),
        4 => option_name(multi_collections, item.multi_collection_id),
        5 => option_name(smart_collections, item.smart_collection_id),
        6 => playlists
            .iter()
            .find(|option| Some(option.id) == item.playlist_id)
            .map(|option| option.name.clone())
            .unwrap_or_default(),
        _ => String::new(),
    }
}

fn option_name(options: &[DecoOption], id: Option<i64>) -> String {
    options
        .iter()
        .find(|option| Some(option.id) == id)
        .map(|option| option.name.clone())
        .unwrap_or_default()
}
