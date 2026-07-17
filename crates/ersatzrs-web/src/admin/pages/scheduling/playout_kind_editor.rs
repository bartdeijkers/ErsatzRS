//! Classic, Block, and Scripted playout editors.
//!
//! Ports the top-level pages:
//! - `Pages/ClassicPlayoutEditor.razor`
//! - `Pages/BlockPlayoutEditor.razor`
//! - `Pages/ScriptedPlayoutEditor.razor`
//!
//! Nested Classic alternate-schedule and Block template editors remain visible
//! follow-up pages; these top-level editors intentionally keep the same links.

use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::admin::i18n::{save_failed, tr, TextKey};
use crate::admin::server_fns::playouts::{
    erase_playout_items, erase_playout_items_and_history, get_playout_kind_editor,
    list_locked_playouts, save_playout_cooldown, save_playout_default_deco, save_scripted_playout,
    PlayoutCooldownForm, PlayoutDecoOption, PlayoutKindEditorForm, KIND_BLOCK, KIND_CLASSIC,
    KIND_SCRIPTED,
};
use crate::admin::ui::primitives::toast::{use_toast, ToastSeverity};

stylance::import_crate_style!(
    style,
    "src/admin/pages/scheduling/playout_kind_editor.module.css"
);

const CHANNEL_MODE_ON_DEMAND: i32 = 1;

#[component]
pub fn ClassicPlayoutEditorPage() -> impl IntoView {
    view! { <PlayoutKindEditorPage schedule_kind=KIND_CLASSIC/> }
}

#[component]
pub fn BlockPlayoutEditorPage() -> impl IntoView {
    view! { <PlayoutKindEditorPage schedule_kind=KIND_BLOCK/> }
}

#[component]
pub fn ScriptedPlayoutEditorPage() -> impl IntoView {
    view! { <PlayoutKindEditorPage schedule_kind=KIND_SCRIPTED/> }
}

#[component]
fn PlayoutKindEditorPage(schedule_kind: i32) -> impl IntoView {
    let params = use_params_map();
    let playout_id =
        move || params.with(|p| p.get("id").and_then(|s| s.parse::<i64>().ok()).unwrap_or(0));

    view! { <PlayoutKindEditorIsland playout_id=playout_id() schedule_kind=schedule_kind/> }
}

#[island]
fn PlayoutKindEditorIsland(playout_id: i64, schedule_kind: i32) -> impl IntoView {
    let form: Resource<Option<PlayoutKindEditorForm>> = Resource::new_blocking(
        move || (playout_id, schedule_kind),
        |(id, kind)| async move { get_playout_kind_editor(id, kind).await.unwrap_or(None) },
    );
    let locked_ids: Resource<Vec<i64>> = Resource::new(
        move || (),
        |()| async move { list_locked_playouts().await.unwrap_or_default() },
    );

    view! {
        <Suspense fallback=move || view! { <p class=style::empty>{tr(TextKey::Loading)}</p> }>
            {move || {
                let locked_ids = locked_ids.get().unwrap_or_default();
                form.get().map(|maybe| match maybe {
                    Some(form) => view! {
                        <PlayoutKindEditorForm form=form locked_ids=locked_ids.clone()/>
                    }
                        .into_any(),
                    None => view! {
                        <p class=style::empty>{tr(TextKey::PlayoutNotFound)}</p>
                    }
                        .into_any(),
                })
            }}
        </Suspense>
    }
}

#[component]
fn PlayoutKindEditorForm(form: PlayoutKindEditorForm, locked_ids: Vec<i64>) -> impl IntoView {
    let playout_id = form.id;
    let is_locked = locked_ids.contains(&playout_id);
    let title_key = match form.schedule_kind {
        KIND_CLASSIC => TextKey::ClassicPlayout,
        KIND_BLOCK => TextKey::BlockPlayout,
        KIND_SCRIPTED => TextKey::ScriptedPlayout,
        _ => TextKey::Playout,
    };
    let schedule_file = RwSignal::new(form.schedule_file.clone());
    let enable_default_deco = RwSignal::new(form.default_deco_id.is_some());
    let selected_deco_id = RwSignal::new(form.default_deco_id.unwrap_or(0));
    let initial_group_id = form
        .default_deco_id
        .and_then(|id| {
            form.decos
                .iter()
                .find(|deco| deco.id == id)
                .map(|deco| deco.group_id)
        })
        .or_else(|| form.deco_groups.first().map(|group| group.id))
        .unwrap_or(0);
    let selected_group_id = RwSignal::new(initial_group_id);
    let deco_groups = form.deco_groups.clone();
    let decos = form.decos.clone();
    let cooldown_preset = RwSignal::new(form.cooldown.preset.clone());
    let cooldown_scope = RwSignal::new(form.cooldown.scope.clone());
    let cooldown_after_items = RwSignal::new(form.cooldown.after_items);
    let cooldown_after_duration_minutes = RwSignal::new(form.cooldown.after_duration_minutes);
    let cooldown_include_filler = RwSignal::new(form.cooldown.include_filler);
    let cooldown_rules = RwSignal::new(form.cooldown.rules.clone());
    let cooldown_filter_rules = RwSignal::new(form.cooldown.filter_rules.clone());

    let toast = use_toast();
    let save_scripted = Action::new(|(id, file): &(i64, String)| {
        let id = *id;
        let file = file.clone();
        async move { save_scripted_playout(id, file).await }
    });
    let save_deco = Action::new(|(id, deco_id): &(i64, Option<i64>)| {
        let id = *id;
        let deco_id = *deco_id;
        async move { save_playout_default_deco(id, deco_id).await }
    });
    let save_cooldown = Action::new(|(id, kind, form): &(i64, i32, PlayoutCooldownForm)| {
        let (id, kind, form) = (*id, *kind, form.clone());
        async move { save_playout_cooldown(id, kind, form).await }
    });
    let erase_items_action = Action::new(|id: &i64| {
        let id = *id;
        async move { erase_playout_items(id).await }
    });
    let erase_history_action = Action::new(|id: &i64| {
        let id = *id;
        async move { erase_playout_items_and_history(id).await }
    });

    Effect::new(move |_| {
        if let Some(result) = save_scripted.value().get() {
            match result {
                Ok(()) => toast.push(tr(TextKey::SavedScriptedSchedule), ToastSeverity::Success),
                Err(error) => toast.push(save_failed(error), ToastSeverity::Error),
            }
        }
    });
    Effect::new(move |_| {
        if let Some(Err(error)) = save_cooldown.value().get() {
            toast.push(save_failed(error), ToastSeverity::Error);
        }
    });

    let current_cooldown_form = move || PlayoutCooldownForm {
        preset: cooldown_preset.get_untracked(),
        scope: cooldown_scope.get_untracked(),
        after_items: cooldown_after_items.get_untracked(),
        after_duration_minutes: cooldown_after_duration_minutes.get_untracked(),
        include_filler: cooldown_include_filler.get_untracked(),
        rules: cooldown_rules.get_untracked(),
        filter_rules: cooldown_filter_rules.get_untracked(),
    };
    Effect::new(move |_| {
        if let Some(result) = save_deco.value().get() {
            match result {
                Ok(()) => toast.push(tr(TextKey::SavedDefaultDeco), ToastSeverity::Success),
                Err(error) => toast.push(save_failed(error), ToastSeverity::Error),
            }
        }
    });
    Effect::new(move |_| {
        if let Some(result) = erase_items_action.value().get() {
            match result {
                Ok(()) => toast.push(tr(TextKey::ErasedPlayoutItems), ToastSeverity::Success),
                Err(error) => toast.push(save_failed(error), ToastSeverity::Error),
            }
        }
    });
    Effect::new(move |_| {
        if let Some(result) = erase_history_action.value().get() {
            match result {
                Ok(()) => toast.push(
                    tr(TextKey::ErasedPlayoutItemsAndHistory),
                    ToastSeverity::Success,
                ),
                Err(error) => toast.push(save_failed(error), ToastSeverity::Error),
            }
        }
    });

    view! {
        <div class=style::page>
            <div class=style::action_bar>
                {move || match form.schedule_kind {
                    KIND_SCRIPTED => view! {
                        <button
                            class=style::btn_primary
                            disabled=move || is_locked || save_scripted.pending().get() || save_cooldown.pending().get()
                            on:click=move |_| {
                                save_scripted.dispatch((playout_id, schedule_file.get_untracked()));
                                save_cooldown.dispatch((playout_id, KIND_SCRIPTED, current_cooldown_form()));
                            }
                        >
                            {tr(TextKey::SaveScriptedSchedule)}
                        </button>
                    }
                        .into_any(),
                    KIND_BLOCK => view! {
                        <button
                            class=style::btn_primary
                            disabled=move || is_locked || save_deco.pending().get() || save_cooldown.pending().get()
                            on:click=move |_| {
                                let deco_id = if enable_default_deco.get_untracked() {
                                    Some(selected_deco_id.get_untracked()).filter(|id| *id > 0)
                                } else {
                                    None
                                };
                                save_deco.dispatch((playout_id, deco_id));
                                save_cooldown.dispatch((playout_id, KIND_BLOCK, current_cooldown_form()));
                            }
                        >
                            {tr(TextKey::SaveDefaultDeco)}
                        </button>
                    }
                        .into_any(),
                    _ => view! { <span></span> }.into_any(),
                }}
            </div>
            <main class=style::content>
                <h1 class=style::title>
                    {form.channel_label.clone()} " - " {tr(title_key)}
                </h1>
                <div class=style::divider></div>
                {move || match form.schedule_kind {
                    KIND_CLASSIC => classic_fields(playout_id, form.channel_playout_mode, is_locked).into_any(),
                    KIND_BLOCK => block_fields(
                        playout_id,
                        is_locked,
                        enable_default_deco,
                        selected_group_id,
                        selected_deco_id,
                        deco_groups.clone(),
                        decos.clone(),
                    )
                        .into_any(),
                    KIND_SCRIPTED => scripted_fields(schedule_file).into_any(),
                    _ => view! { <p class=style::empty>{tr(TextKey::UnsupportedPlayoutKind)}</p> }.into_any(),
                }}

                <div style:display=move || {
                    if matches!(form.schedule_kind, KIND_BLOCK | KIND_SCRIPTED) { "" } else { "none" }
                }>
                    {cooldown_fields(
                        is_locked,
                        cooldown_preset,
                        cooldown_scope,
                        cooldown_after_items,
                        cooldown_after_duration_minutes,
                        cooldown_include_filler,
                        cooldown_rules,
                        cooldown_filter_rules,
                    )}
                </div>

                <h2 class=style::section_title>{tr(TextKey::Maintenance)}</h2>
                <div class=style::divider></div>
                {move || maintenance_fields(form.schedule_kind, playout_id, is_locked, erase_items_action, erase_history_action)}
            </main>
        </div>
    }
}

fn classic_fields(playout_id: i64, channel_playout_mode: i32, is_locked: bool) -> impl IntoView {
    let disabled = is_locked || channel_playout_mode == CHANNEL_MODE_ON_DEMAND;
    let class = if disabled {
        format!("{} {}", style::btn_primary, style::btn_disabled)
    } else {
        style::btn_primary.to_owned()
    };
    view! {
        <div class=style::field_row>
            <div class=style::label>{tr(TextKey::AlternateSchedules)}</div>
            <a
                class=class
                href=format!("/playouts/{playout_id}/alternate-schedules")
                aria-disabled=disabled.to_string()
            >
                {tr(TextKey::EditAlternateSchedules)}
            </a>
        </div>
    }
}

#[allow(clippy::needless_pass_by_value)]
fn block_fields(
    playout_id: i64,
    is_locked: bool,
    enable_default_deco: RwSignal<bool>,
    selected_group_id: RwSignal<i64>,
    selected_deco_id: RwSignal<i64>,
    deco_groups: Vec<crate::admin::server_fns::playouts::PlayoutDecoGroupOption>,
    decos: Vec<PlayoutDecoOption>,
) -> impl IntoView {
    let templates_class = if is_locked {
        format!("{} {}", style::btn_primary, style::btn_disabled)
    } else {
        style::btn_primary.to_owned()
    };

    view! {
        <div class=style::field_row>
            <div class=style::label>{tr(TextKey::PlayoutTemplates)}</div>
            <a
                class=templates_class
                href=format!("/playouts/{playout_id}/templates")
                aria-disabled=is_locked.to_string()
            >
                {tr(TextKey::EditTemplates)}
            </a>
        </div>
        <h2 class=style::section_title>{tr(TextKey::DefaultDeco)}</h2>
        <div class=style::divider></div>
        <div class=style::field_row>
            <label class=style::label for="enable-default-deco">{tr(TextKey::EnableDefaultDeco)}</label>
            <input
                id="enable-default-deco"
                class=style::checkbox
                type="checkbox"
                prop:checked=move || enable_default_deco.get()
                disabled=is_locked
                on:change=move |event| enable_default_deco.set(event_target_checked(&event))
            />
        </div>
        <div style:display=move || if enable_default_deco.get() { "" } else { "none" }>
            <div class=style::field_row>
                <label class=style::label for="deco-group">{tr(TextKey::DecoGroup)}</label>
                <select
                    id="deco-group"
                    class=style::select
                    prop:value=move || selected_group_id.get().to_string()
                    disabled=is_locked
                    on:change=move |event| {
                        let group_id = event_target_value(&event).parse::<i64>().unwrap_or(0);
                        selected_group_id.set(group_id);
                        selected_deco_id.set(0);
                    }
                >
                    {deco_groups
                        .iter()
                        .map(|group| {
                            view! {
                                <option value=group.id.to_string()>{group.name.clone()}</option>
                            }
                        })
                        .collect_view()}
                </select>
            </div>
            <div class=style::field_row>
                <label class=style::label for="deco">{tr(TextKey::Deco)}</label>
                <select
                    id="deco"
                    class=style::select
                    prop:value=move || selected_deco_id.get().to_string()
                    disabled=is_locked
                    on:change=move |event| {
                        selected_deco_id.set(event_target_value(&event).parse::<i64>().unwrap_or(0));
                    }
                >
                    <option value="0">{tr(TextKey::SelectValue)}</option>
                    {move || {
                        decos
                            .iter()
                            .filter(|deco| deco.group_id == selected_group_id.get())
                            .map(|deco| {
                                view! {
                                    <option value=deco.id.to_string()>{deco.name.clone()}</option>
                                }
                            })
                            .collect_view()
                    }}
                </select>
            </div>
        </div>
    }
}

fn scripted_fields(schedule_file: RwSignal<String>) -> impl IntoView {
    view! {
        <div class=style::field_row>
            <label class=style::label for="scripted-schedule">{tr(TextKey::ScriptedScheduleMenu)}</label>
            <input
                id="scripted-schedule"
                class=style::input
                type="text"
                prop:value=move || schedule_file.get()
                on:input=move |event| schedule_file.set(event_target_value(&event))
            />
        </div>
    }
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn cooldown_fields(
    is_locked: bool,
    preset: RwSignal<String>,
    scope: RwSignal<String>,
    after_items: RwSignal<u32>,
    after_duration_minutes: RwSignal<u32>,
    include_filler: RwSignal<bool>,
    rules: RwSignal<Vec<String>>,
    filter_rules: RwSignal<String>,
) -> impl IntoView {
    view! {
        <h2 class=style::section_title>{tr(TextKey::SimilarVideoCooldown)}</h2>
        <div class=style::divider></div>
        <div class=style::field_row>
            <label class=style::label for="playout-cooldown-preset">{tr(TextKey::CooldownPreset)}</label>
            <select
                id="playout-cooldown-preset"
                class=style::select
                prop:value=move || preset.get()
                disabled=is_locked
                on:change=move |event| {
                    let value = event_target_value(&event);
                    apply_playout_cooldown_preset(
                        &value,
                        after_items,
                        after_duration_minutes,
                        rules,
                    );
                    preset.set(value);
                }
            >
                <option value="off">{tr(TextKey::Off)}</option>
                <option value="light">{tr(TextKey::Light)}</option>
                <option value="balanced">{tr(TextKey::Balanced)}</option>
                <option value="custom">{tr(TextKey::Custom)}</option>
            </select>
        </div>
        <div style:display=move || if preset.get() == "off" { "none" } else { "" }>
            <div class=style::field_row>
                <label class=style::label for="playout-cooldown-scope">{tr(TextKey::CooldownScope)}</label>
                <select
                    id="playout-cooldown-scope"
                    class=style::select
                    prop:value=move || scope.get()
                    disabled=is_locked
                    on:change=move |event| {
                        scope.set(event_target_value(&event));
                        preset.set("custom".to_owned());
                    }
                >
                    <option value="playout">{tr(TextKey::CooldownScopePlayout)}</option>
                    <option value="schedule_item">{tr(TextKey::CooldownScopeScheduleItem)}</option>
                    <option value="content_source">{tr(TextKey::CooldownScopeContentSource)}</option>
                </select>
            </div>
            <div class=style::field_row>
                <label class=style::label for="playout-cooldown-items">{tr(TextKey::CooldownAfterItems)}</label>
                <input
                    id="playout-cooldown-items"
                    class=style::input
                    type="number"
                    min="0"
                    prop:value=move || after_items.get().to_string()
                    disabled=is_locked
                    on:input=move |event| {
                        after_items.set(event_target_value(&event).parse().unwrap_or_default());
                        preset.set("custom".to_owned());
                    }
                />
            </div>
            <div class=style::field_row>
                <label class=style::label for="playout-cooldown-duration">{tr(TextKey::CooldownAfterDurationMinutes)}</label>
                <input
                    id="playout-cooldown-duration"
                    class=style::input
                    type="number"
                    min="0"
                    prop:value=move || after_duration_minutes.get().to_string()
                    disabled=is_locked
                    on:input=move |event| {
                        after_duration_minutes.set(event_target_value(&event).parse().unwrap_or_default());
                        preset.set("custom".to_owned());
                    }
                />
            </div>
            <div class=style::field_row>
                <label class=style::label for="playout-cooldown-filler">{tr(TextKey::CooldownIncludeFiller)}</label>
                <input
                    id="playout-cooldown-filler"
                    class=style::checkbox
                    type="checkbox"
                    prop:checked=move || include_filler.get()
                    disabled=is_locked
                    on:change=move |event| {
                        include_filler.set(event_target_checked(&event));
                        preset.set("custom".to_owned());
                    }
                />
            </div>
            <div class=style::field_row>
                <div class=style::label>{tr(TextKey::SimilarityRules)}</div>
                <div class=style::rule_grid>
                    {[
                        ("artist", TextKey::Artist),
                        ("album_artist", TextKey::AlbumArtist),
                        ("show", TextKey::Show),
                        ("season", TextKey::Season),
                        ("content_parent", TextKey::ContentParent),
                        ("collection", TextKey::Collection),
                        ("tag", TextKey::Tags),
                        ("genre", TextKey::Genres),
                        ("studio", TextKey::Studios),
                    ].into_iter().map(|(name, label)| {
                        let name = name.to_owned();
                        let checked_name = name.clone();
                        view! {
                            <label class=style::rule_option>
                                <input
                                    type="checkbox"
                                    prop:checked=move || rules.with(|values| values.contains(&checked_name))
                                    disabled=is_locked
                                    on:change=move |event| {
                                        let checked = event_target_checked(&event);
                                        rules.update(|values| {
                                            values.retain(|value| value != &name);
                                            if checked { values.push(name.clone()); }
                                        });
                                        preset.set("custom".to_owned());
                                    }
                                />
                                <span>{tr(label)}</span>
                            </label>
                        }
                    }).collect_view()}
                </div>
            </div>
            <div class=style::field_row>
                <label class=style::label for="playout-cooldown-filters">{tr(TextKey::CooldownFilterRules)}</label>
                <textarea
                    id="playout-cooldown-filters"
                    class=style::textarea
                    rows="4"
                    prop:value=move || filter_rules.get()
                    disabled=is_locked
                    on:input=move |event| {
                        filter_rules.set(event_target_value(&event));
                        preset.set("custom".to_owned());
                    }
                ></textarea>
            </div>
        </div>
    }
}

fn apply_playout_cooldown_preset(
    preset: &str,
    after_items: RwSignal<u32>,
    after_duration_minutes: RwSignal<u32>,
    rules: RwSignal<Vec<String>>,
) {
    match preset {
        "light" => {
            after_items.set(2);
            after_duration_minutes.set(0);
            rules.set(
                vec!["artist", "show", "content_parent"]
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
            );
        }
        "balanced" => {
            after_items.set(4);
            after_duration_minutes.set(30);
            rules.set(
                vec![
                    "artist",
                    "show",
                    "season",
                    "content_parent",
                    "collection",
                    "tag",
                ]
                .into_iter()
                .map(str::to_owned)
                .collect(),
            );
        }
        "off" => {
            after_items.set(0);
            after_duration_minutes.set(0);
            rules.set(Vec::new());
        }
        _ => {}
    }
}

fn maintenance_fields(
    schedule_kind: i32,
    playout_id: i64,
    is_locked: bool,
    erase_items_action: Action<i64, Result<(), ServerFnError>>,
    erase_history_action: Action<i64, Result<(), ServerFnError>>,
) -> impl IntoView {
    view! {
        <div class=style::field_row>
            <div class=style::label>{tr(TextKey::PlayoutItemsAndHistory)}</div>
            <div>
                {if schedule_kind == KIND_BLOCK {
                    view! {
                        <button
                            class=style::btn_warning
                            disabled=move || is_locked || erase_items_action.pending().get()
                            on:click=move |_| {
                                erase_items_action.dispatch(playout_id);
                            }
                        >
                            {tr(TextKey::EraseItems)}
                        </button>
                    }
                        .into_any()
                } else {
                    view! { <span></span> }.into_any()
                }}
            </div>
        </div>
        <div class=style::field_row>
            <div class=style::label></div>
            <button
                class=style::btn_danger
                disabled=move || is_locked || erase_history_action.pending().get()
                on:click=move |_| {
                    erase_history_action.dispatch(playout_id);
                }
            >
                {tr(TextKey::EraseItemsAndHistory)}
            </button>
        </div>
    }
}
