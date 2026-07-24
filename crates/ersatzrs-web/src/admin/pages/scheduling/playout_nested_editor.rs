//! Nested Classic alternate-schedule and Block template editors.

use chrono::Datelike;
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::admin::i18n::{save_failed, tr, TextKey};
use crate::admin::server_fns::playouts::{
    get_playout_alternate_schedules, get_playout_templates, save_playout_alternate_schedules,
    save_playout_templates, PlayoutAlternateScheduleItemForm, PlayoutAlternateSchedulesForm,
    PlayoutDateFilter, PlayoutNestedChildOption, PlayoutNestedOption, PlayoutTemplateItemForm,
    PlayoutTemplatesForm,
};
use crate::admin::ui::primitives::icon::{icon_style, IconName};
use crate::admin::ui::primitives::resource_state::{
    admin_loading_resource_view, admin_result_view,
};
use crate::admin::ui::primitives::toast::{use_toast, ToastSeverity};

stylance::import_crate_style!(
    style,
    "src/admin/pages/scheduling/playout_nested_editor.module.css"
);

#[component]
pub fn PlayoutAlternateSchedulesEditorPage() -> impl IntoView {
    let params = use_params_map();
    let playout_id =
        move || params.with(|p| p.get("id").and_then(|s| s.parse::<i64>().ok()).unwrap_or(0));

    view! { <AlternateSchedulesIsland playout_id=playout_id()/> }
}

#[component]
pub fn PlayoutTemplatesEditorPage() -> impl IntoView {
    let params = use_params_map();
    let playout_id =
        move || params.with(|p| p.get("id").and_then(|s| s.parse::<i64>().ok()).unwrap_or(0));

    view! { <TemplatesIsland playout_id=playout_id()/> }
}

#[island]
fn AlternateSchedulesIsland(playout_id: i64) -> impl IntoView {
    let form = Resource::new_blocking(move || playout_id, get_playout_alternate_schedules);

    view! {
        <Suspense fallback=admin_loading_resource_view>
            {move || form.get().map(|result| admin_result_view(result, |maybe| {
                match maybe {
                    Some(form) => view! { <AlternateSchedulesFormView form=form/> }.into_any(),
                    None => view! { <p class=style::empty>{tr(TextKey::PlayoutNotFound)}</p> }.into_any(),
                }
            }))}
        </Suspense>
    }
}

#[island]
fn TemplatesIsland(playout_id: i64) -> impl IntoView {
    let form = Resource::new_blocking(move || playout_id, get_playout_templates);

    view! {
        <Suspense fallback=admin_loading_resource_view>
            {move || form.get().map(|result| admin_result_view(result, |maybe| {
                match maybe {
                    Some(form) => view! { <TemplatesFormView form=form/> }.into_any(),
                    None => view! { <p class=style::empty>{tr(TextKey::PlayoutNotFound)}</p> }.into_any(),
                }
            }))}
        </Suspense>
    }
}

#[component]
fn AlternateSchedulesFormView(form: PlayoutAlternateSchedulesForm) -> impl IntoView {
    let items = RwSignal::new(form.items.clone());
    let selected = RwSignal::new((!form.items.is_empty()).then_some(0usize));
    let schedules = form.schedules.clone();
    let schedules_for_add_desktop = schedules.clone();
    let schedules_for_add_mobile = schedules.clone();
    let schedules_for_editor = schedules.clone();
    let save = Action::new(move |items: &Vec<PlayoutAlternateScheduleItemForm>| {
        let items = items.clone();
        async move { save_playout_alternate_schedules(form.playout_id, items).await }
    });
    let toast = use_toast();

    Effect::new(move |_| {
        if let Some(result) = save.value().get() {
            match result {
                Ok(()) => toast.push(tr(TextKey::Saved), ToastSeverity::Success),
                Err(error) => toast.push(save_failed(error), ToastSeverity::Error),
            }
        }
    });

    view! {
        <div class=style::page>
            <div class=style::action_bar>
                <div class=style::desktop_actions>
                    <button class=style::btn_primary disabled=move || save.pending().get() on:click=move |_| {
                        save.dispatch(items.get_untracked());
                    }>
                        <span style=icon_style(IconName::Save, "1rem") aria-hidden="true"></span>
                        {tr(TextKey::SaveAlternateSchedules)}
                    </button>
                    <button class=style::btn_secondary on:click=move |_| add_alternate(items, selected, &schedules_for_add_desktop)>
                        <span style=icon_style(IconName::Add, "1rem") aria-hidden="true"></span>
                        {tr(TextKey::AddAlternateSchedule)}
                    </button>
                </div>
                <details class=style::mobile_actions>
                    <summary class=style::icon_btn aria-label=tr(TextKey::Actions)>
                        <span style=icon_style(IconName::MoreVert, "1.25rem") aria-hidden="true"></span>
                    </summary>
                    <div class=style::mobile_menu>
                        <button class=style::btn_primary disabled=move || save.pending().get() on:click=move |_| {
                            save.dispatch(items.get_untracked());
                        }>
                            <span style=icon_style(IconName::Save, "1rem") aria-hidden="true"></span>
                            {tr(TextKey::SaveAlternateSchedules)}
                        </button>
                        <button class=style::btn_secondary on:click=move |_| add_alternate(items, selected, &schedules_for_add_mobile)>
                            <span style=icon_style(IconName::Add, "1rem") aria-hidden="true"></span>
                            {tr(TextKey::AddAlternateSchedule)}
                        </button>
                    </div>
                </details>
            </div>
            <main class=style::content>
                <h1 class=style::title>{form.channel_label.clone()} " - " {tr(TextKey::Content)}</h1>
                <p class=style::body>{tr(TextKey::PlayoutNestedCatchAllHelp)}</p>
                {alternate_table(items, selected)}
                {move || {
                    selected
                        .get()
                        .and_then(|index| items.get().get(index).cloned())
                        .map_or_else(
                            || view! { <p class=style::empty>{tr(TextKey::SelectScheduleItemPrompt)}</p> }.into_any(),
                            |item| alternate_editor(items, selected, item, schedules_for_editor.clone()).into_any(),
                        )
                }}
            </main>
        </div>
    }
}

#[component]
fn TemplatesFormView(form: PlayoutTemplatesForm) -> impl IntoView {
    let items = RwSignal::new(form.items.clone());
    let selected = RwSignal::new((!form.items.is_empty()).then_some(0usize));
    let template_groups = form.template_groups.clone();
    let templates = form.templates.clone();
    let deco_template_groups = form.deco_template_groups.clone();
    let deco_templates = form.deco_templates.clone();
    let preview_month = RwSignal::new(current_year_month());
    let template_groups_for_add_desktop = template_groups.clone();
    let templates_for_add_desktop = templates.clone();
    let template_groups_for_add_mobile = template_groups.clone();
    let templates_for_add_mobile = templates.clone();
    let template_groups_for_editor = template_groups.clone();
    let templates_for_editor = templates.clone();
    let deco_template_groups_for_editor = deco_template_groups.clone();
    let deco_templates_for_editor = deco_templates.clone();
    let save = Action::new(move |items: &Vec<PlayoutTemplateItemForm>| {
        let items = items.clone();
        async move { save_playout_templates(form.playout_id, items).await }
    });
    let toast = use_toast();

    Effect::new(move |_| {
        if let Some(result) = save.value().get() {
            match result {
                Ok(()) => toast.push(tr(TextKey::Saved), ToastSeverity::Success),
                Err(error) => toast.push(save_failed(error), ToastSeverity::Error),
            }
        }
    });

    view! {
        <div class=style::page>
            <div class=style::action_bar>
                <div class=style::desktop_actions>
                    <button class=style::btn_primary disabled=move || save.pending().get() on:click=move |_| {
                        save.dispatch(items.get_untracked());
                    }>
                        <span style=icon_style(IconName::Save, "1rem") aria-hidden="true"></span>
                        {tr(TextKey::SaveTemplates)}
                    </button>
                    <button class=style::btn_secondary on:click=move |_| add_template(items, selected, &template_groups_for_add_desktop, &templates_for_add_desktop)>
                        <span style=icon_style(IconName::Add, "1rem") aria-hidden="true"></span>
                        {tr(TextKey::AddTemplate)}
                    </button>
                    <button class=style::btn_secondary on:click=move |_| selected.set(None)>
                        <span style=icon_style(IconName::Calendar, "1rem") aria-hidden="true"></span>
                        {tr(TextKey::PreviewCalendar)}
                    </button>
                </div>
                <details class=style::mobile_actions>
                    <summary class=style::icon_btn aria-label=tr(TextKey::Actions)>
                        <span style=icon_style(IconName::MoreVert, "1.25rem") aria-hidden="true"></span>
                    </summary>
                    <div class=style::mobile_menu>
                        <button class=style::btn_primary disabled=move || save.pending().get() on:click=move |_| {
                            save.dispatch(items.get_untracked());
                        }>
                            <span style=icon_style(IconName::Save, "1rem") aria-hidden="true"></span>
                            {tr(TextKey::SaveTemplates)}
                        </button>
                        <button class=style::btn_secondary on:click=move |_| add_template(items, selected, &template_groups_for_add_mobile, &templates_for_add_mobile)>
                            <span style=icon_style(IconName::Add, "1rem") aria-hidden="true"></span>
                            {tr(TextKey::AddTemplate)}
                        </button>
                        <button class=style::btn_secondary on:click=move |_| selected.set(None)>
                            <span style=icon_style(IconName::Calendar, "1rem") aria-hidden="true"></span>
                            {tr(TextKey::PreviewCalendar)}
                        </button>
                    </div>
                </details>
            </div>
            <main class=style::content>
                <h1 class=style::title>{form.channel_label.clone()} " - " {tr(TextKey::PlayoutTemplates)}</h1>
                <p class=style::body>{tr(TextKey::PlayoutNestedPriorityHelp)}</p>
                {template_table(items, selected)}
                {move || {
                    selected
                        .get()
                        .and_then(|index| items.get().get(index).cloned())
                        .map_or_else(
                            || template_preview(items, preview_month).into_any(),
                            |item| {
                                template_editor(
                                    items,
                                    selected,
                                    item,
                                    template_groups_for_editor.clone(),
                                    templates_for_editor.clone(),
                                    deco_template_groups_for_editor.clone(),
                                    deco_templates_for_editor.clone(),
                                )
                                .into_any()
                            },
                        )
                }}
            </main>
        </div>
    }
}

fn alternate_table(
    items: RwSignal<Vec<PlayoutAlternateScheduleItemForm>>,
    selected: RwSignal<Option<usize>>,
) -> impl IntoView {
    view! {
        <div class=style::table_wrap>
            <table class=style::table>
                <thead><tr>
                    <th>{tr(TextKey::Schedule)}</th>
                    <th>{tr(TextKey::DaysOfWeek)}</th>
                    <th>{tr(TextKey::DaysOfMonth)}</th>
                    <th>{tr(TextKey::Months)}</th>
                    <th></th>
                </tr></thead>
                <tbody>
                    {move || items.get().into_iter().enumerate().map(|(index, item)| {
                        let class = move || if selected.get() == Some(index) { style::selected.to_owned() } else { String::new() };
                        view! {
                            <tr class=class on:click=move |_| selected.set(Some(index))>
                                <td>{item.program_schedule_name}</td>
                                <td>{range_summary(&item.date_filter.days_of_week, 0, 6)}</td>
                                <td>{range_summary(&item.date_filter.days_of_month, 1, 31)}</td>
                                <td>{range_summary(&item.date_filter.months_of_year, 1, 12)}</td>
                                <td><RowActions items=items selected=selected index=index min_len=1/></td>
                            </tr>
                        }
                    }).collect_view()}
                </tbody>
            </table>
        </div>
    }
}

fn template_table(
    items: RwSignal<Vec<PlayoutTemplateItemForm>>,
    selected: RwSignal<Option<usize>>,
) -> impl IntoView {
    view! {
        <div class=style::table_wrap>
            <table class=style::table>
                <thead><tr>
                    <th>{tr(TextKey::Template)}</th>
                    <th>{tr(TextKey::DaysOfWeek)}</th>
                    <th>{tr(TextKey::DaysOfMonth)}</th>
                    <th>{tr(TextKey::Months)}</th>
                    <th></th>
                </tr></thead>
                <tbody>
                    {move || items.get().into_iter().enumerate().map(|(index, item)| {
                        let class = move || if selected.get() == Some(index) { style::selected.to_owned() } else { String::new() };
                        view! {
                            <tr class=class on:click=move |_| selected.set(Some(index))>
                                <td>{item.template_name}</td>
                                <td>{range_summary(&item.date_filter.days_of_week, 0, 6)}</td>
                                <td>{range_summary(&item.date_filter.days_of_month, 1, 31)}</td>
                                <td>{range_summary(&item.date_filter.months_of_year, 1, 12)}</td>
                                <td><RowActions items=items selected=selected index=index min_len=0/></td>
                            </tr>
                        }
                    }).collect_view()}
                </tbody>
            </table>
        </div>
    }
}

#[component]
fn RowActions<T: Clone + Send + Sync + 'static>(
    items: RwSignal<Vec<T>>,
    selected: RwSignal<Option<usize>>,
    index: usize,
    min_len: usize,
) -> impl IntoView {
    view! {
        <div class=style::actions>
            <button
                class=style::icon_btn
                title=tr(TextKey::Up)
                aria-label=tr(TextKey::Up)
                disabled=index == 0
                on:click=move |event| {
                event.stop_propagation();
                move_item(items, selected, index, -1);
            }>
                <span style=icon_style(IconName::ArrowUp, "1.25rem") aria-hidden="true"></span>
            </button>
            <button
                class=style::icon_btn
                title=tr(TextKey::Down)
                aria-label=tr(TextKey::Down)
                disabled=move || { index + 1 >= items.with(Vec::len) }
                on:click=move |event| {
                event.stop_propagation();
                move_item(items, selected, index, 1);
            }>
                <span style=icon_style(IconName::ArrowDown, "1.25rem") aria-hidden="true"></span>
            </button>
            <button
                class=style::icon_btn
                title=tr(TextKey::Delete)
                aria-label=tr(TextKey::Delete)
                disabled=move || { items.with(Vec::len) <= min_len }
                on:click=move |event| {
                event.stop_propagation();
                items.update(|rows| {
                    if index < rows.len() {
                        rows.remove(index);
                    }
                });
                selected.set(Some(index.saturating_sub(1)));
            }>
                <span style=icon_style(IconName::Delete, "1.25rem") aria-hidden="true"></span>
            </button>
        </div>
    }
}

fn alternate_editor(
    items: RwSignal<Vec<PlayoutAlternateScheduleItemForm>>,
    selected: RwSignal<Option<usize>>,
    item: PlayoutAlternateScheduleItemForm,
    schedules: Vec<PlayoutNestedOption>,
) -> impl IntoView {
    view! {
        <h2 class=style::section_title>{tr(TextKey::EditContent)}</h2>
        <div class=style::divider></div>
        <div class=style::field_grid>
            <label class=style::label for="program-schedule">{tr(TextKey::Schedule)}</label>
            <select id="program-schedule" class=style::select prop:value=item.program_schedule_id.to_string() on:change=move |event| {
                let id = event_target_value(&event).parse::<i64>().unwrap_or(0);
                let name = schedules.iter().find(|schedule| schedule.id == id).map(|schedule| schedule.name.clone()).unwrap_or_default();
                update_selected(items, selected, move |row: &mut PlayoutAlternateScheduleItemForm| {
                    row.program_schedule_id = id;
                    row.program_schedule_name.clone_from(&name);
                });
            }>
                {schedules.iter().map(|schedule| view! { <option value=schedule.id.to_string()>{schedule.name.clone()}</option> }).collect_view()}
            </select>
            {date_filter_editor(item.date_filter, move |filter| {
                update_selected(items, selected, move |row: &mut PlayoutAlternateScheduleItemForm| row.date_filter = filter.clone());
            })}
        </div>
    }
}

#[allow(clippy::needless_pass_by_value)]
fn template_editor(
    items: RwSignal<Vec<PlayoutTemplateItemForm>>,
    selected: RwSignal<Option<usize>>,
    item: PlayoutTemplateItemForm,
    template_groups: Vec<PlayoutNestedOption>,
    templates: Vec<PlayoutNestedChildOption>,
    deco_template_groups: Vec<PlayoutNestedOption>,
    deco_templates: Vec<PlayoutNestedChildOption>,
) -> impl IntoView {
    let selected_group = RwSignal::new(item.template_group_id);
    let selected_deco_group = RwSignal::new(item.deco_template_group_id.unwrap_or(0));
    let templates_for_change = templates.clone();
    let templates_for_options = templates.clone();
    let deco_templates_for_change = deco_templates.clone();
    let deco_templates_for_options = deco_templates.clone();

    view! {
        <h2 class=style::section_title>{tr(TextKey::Template)}</h2>
        <div class=style::divider></div>
        <div class=style::field_grid>
            <label class=style::label for="template-group">{tr(TextKey::TemplateGroup)}</label>
            <select id="template-group" class=style::select prop:value=move || selected_group.get().to_string() on:change=move |event| {
                selected_group.set(event_target_value(&event).parse::<i64>().unwrap_or(0));
            }>
                {template_groups.iter().map(|group| view! { <option value=group.id.to_string()>{group.name.clone()}</option> }).collect_view()}
            </select>
            <label class=style::label for="template">{tr(TextKey::Template)}</label>
            <select id="template" class=style::select prop:value=item.template_id.to_string() on:change=move |event| {
                let id = event_target_value(&event).parse::<i64>().unwrap_or(0);
                let template = templates_for_change.iter().find(|template| template.id == id).cloned();
                update_selected(items, selected, move |row: &mut PlayoutTemplateItemForm| {
                    row.template_id = id;
                    if let Some(template) = template.clone() {
                        row.template_group_id = template.group_id;
                        row.template_name = template.name;
                    }
                });
            }>
                <option value="0">{tr(TextKey::SelectValue)}</option>
                {move || templates_for_options.iter().filter(|template| template.group_id == selected_group.get()).map(|template| view! { <option value=template.id.to_string()>{template.name.clone()}</option> }).collect_view()}
            </select>
            <label class=style::label for="deco-template-group">{tr(TextKey::DecoTemplateGroup)}</label>
            <select id="deco-template-group" class=style::select prop:value=move || selected_deco_group.get().to_string() on:change=move |event| {
                selected_deco_group.set(event_target_value(&event).parse::<i64>().unwrap_or(0));
            }>
                <option value="0">{tr(TextKey::SelectValue)}</option>
                {deco_template_groups.iter().map(|group| view! { <option value=group.id.to_string()>{group.name.clone()}</option> }).collect_view()}
            </select>
            <label class=style::label for="deco-template">{tr(TextKey::DecoTemplate)}</label>
            <select id="deco-template" class=style::select prop:value=item.deco_template_id.unwrap_or(0).to_string() on:change=move |event| {
                let id = event_target_value(&event).parse::<i64>().unwrap_or(0);
                let template = deco_templates_for_change.iter().find(|template| template.id == id).cloned();
                update_selected(items, selected, move |row: &mut PlayoutTemplateItemForm| {
                    row.deco_template_id = (id > 0).then_some(id);
                    row.deco_template_group_id = template.as_ref().map(|template| template.group_id);
                    row.deco_template_name = template.as_ref().map(|template| template.name.clone());
                });
            }>
                <option value="0">{tr(TextKey::SelectValue)}</option>
                {move || deco_templates_for_options.iter().filter(|template| template.group_id == selected_deco_group.get()).map(|template| view! { <option value=template.id.to_string()>{template.name.clone()}</option> }).collect_view()}
            </select>
            {date_filter_editor(item.date_filter, move |filter| {
                update_selected(items, selected, move |row: &mut PlayoutTemplateItemForm| row.date_filter = filter.clone());
            })}
        </div>
    }
}

fn date_filter_editor(
    filter: PlayoutDateFilter,
    update: impl Fn(PlayoutDateFilter) + Copy + 'static,
) -> impl IntoView {
    let filter = RwSignal::new(filter);
    let emit = move || update(filter.get_untracked());

    view! {
        <label class=style::label for="limit-date-range">{tr(TextKey::LimitToDateRange)}</label>
        <label class=style::check>
            <input id="limit-date-range" type="checkbox" prop:checked=move || filter.get().limit_to_date_range on:change=move |event| {
                filter.update(|value| value.limit_to_date_range = event_target_checked(&event));
                emit();
            }/>
            {tr(TextKey::Enable)}
        </label>
        <div class=style::label>{tr(TextKey::StartDay)}</div>
        {date_inputs(filter, emit, true)}
        <div class=style::label>{tr(TextKey::EndDay)}</div>
        {date_inputs(filter, emit, false)}
        <div class=style::label>{tr(TextKey::DaysOfWeek)}</div>
        {check_grid(filter, emit, 0, 6)}
        <div class=style::label></div>
        <div class=style::quick_actions>
            <button type="button" class=style::btn_secondary on:click=move |_| {
                filter.update(|value| value.days_of_week = vec![1, 2, 3, 4, 5]);
                emit();
            }>{tr(TextKey::Weekdays)}</button>
            <button type="button" class=style::btn_secondary on:click=move |_| {
                filter.update(|value| value.days_of_week = vec![0, 6]);
                emit();
            }>{tr(TextKey::Weekends)}</button>
        </div>
        <div class=style::label>{tr(TextKey::DaysOfMonth)}</div>
        {check_grid(filter, emit, 1, 31)}
        <div class=style::label>{tr(TextKey::Months)}</div>
        {check_grid(filter, emit, 1, 12)}
    }
}

fn date_inputs(
    filter: RwSignal<PlayoutDateFilter>,
    emit: impl Fn() + Copy + 'static,
    start: bool,
) -> impl IntoView {
    view! {
        <div class=style::date_grid>
            <select class=style::select disabled=move || !filter.get().limit_to_date_range prop:value=move || if start { filter.get().start_month.to_string() } else { filter.get().end_month.to_string() } on:change=move |event| {
                let value = event_target_value(&event).parse::<i32>().unwrap_or(1).clamp(1, 12);
                filter.update(|filter| if start { filter.start_month = value } else { filter.end_month = value });
                emit();
            }>
                {(1..=12).map(|month| view! {
                    <option value=month.to_string()>{month_name(month)}</option>
                }).collect_view()}
            </select>
            <select class=style::select disabled=move || !filter.get().limit_to_date_range prop:value=move || if start { filter.get().start_day.to_string() } else { filter.get().end_day.to_string() } on:change=move |event| {
                let value = event_target_value(&event).parse::<i32>().unwrap_or(1).clamp(1, 31);
                filter.update(|filter| if start { filter.start_day = value } else { filter.end_day = value });
                emit();
            }>
                {(1..=31).map(|day| view! {
                    <option value=day.to_string()>{day.to_string()}</option>
                }).collect_view()}
            </select>
            <select class=style::select disabled=move || !filter.get().limit_to_date_range prop:value=move || {
                if start { filter.get().start_year } else { filter.get().end_year }
                    .map(|value| value.to_string())
                    .unwrap_or_default()
            } on:change=move |event| {
                let value = event_target_value(&event).parse::<i32>().ok();
                filter.update(|filter| if start { filter.start_year = value } else { filter.end_year = value });
                emit();
            }>
                <option value=""></option>
                {year_options().into_iter().map(|year| view! {
                    <option value=year.to_string()>{year.to_string()}</option>
                }).collect_view()}
            </select>
        </div>
    }
}

fn check_grid(
    filter: RwSignal<PlayoutDateFilter>,
    emit: impl Fn() + Copy + 'static,
    min: i32,
    max: i32,
) -> impl IntoView {
    view! {
        <div class=style::check_grid>
            {(min..=max).map(|value| view! {
                <label class=style::check>
                    <input type="checkbox" prop:checked=move || selected_values(&filter.get(), min, max).contains(&value) on:change=move |event| {
                        filter.update(|filter| {
                            let values = selected_values(filter, min, max);
                            let mut values = if event_target_checked(&event) {
                                values.into_iter().chain(std::iter::once(value)).collect::<Vec<_>>()
                            } else {
                                values.into_iter().filter(|current| *current != value).collect::<Vec<_>>()
                            };
                            values.sort_unstable();
                            values.dedup();
                            if min == 0 {
                                filter.days_of_week = values;
                            } else if max == 31 {
                                filter.days_of_month = values;
                            } else {
                                filter.months_of_year = values;
                            }
                        });
                        emit();
                    }/>
                    {filter_label(min, max, value)}
                </label>
            }).collect_view()}
        </div>
    }
}

fn selected_values(filter: &PlayoutDateFilter, min: i32, max: i32) -> Vec<i32> {
    if min == 0 {
        filter.days_of_week.clone()
    } else if max == 31 {
        filter.days_of_month.clone()
    } else {
        filter.months_of_year.clone()
    }
}

fn template_preview(
    items: RwSignal<Vec<PlayoutTemplateItemForm>>,
    preview_month: RwSignal<(i32, u32)>,
) -> impl IntoView {
    view! {
        <h2 class=style::section_title>{tr(TextKey::Preview)}</h2>
        <div class=style::divider></div>
        <div class=style::calendar_head>
            <button type="button" class=style::icon_btn title=tr(TextKey::PreviousPage) aria-label=tr(TextKey::PreviousPage) on:click=move |_| {
                preview_month.update(|month| *month = shift_month(*month, -1));
            }>
                <span style=icon_style(IconName::ChevronLeft, "1.25rem") aria-hidden="true"></span>
            </button>
            <div class=style::calendar_title>{move || calendar_title(preview_month.get())}</div>
            <button type="button" class=style::icon_btn title=tr(TextKey::NextPage) aria-label=tr(TextKey::NextPage) on:click=move |_| {
                preview_month.update(|month| *month = shift_month(*month, 1));
            }>
                <span style=icon_style(IconName::ChevronRight, "1.25rem") aria-hidden="true"></span>
            </button>
        </div>
        <div class=style::table_wrap>
            <div class=style::calendar_grid>
                {(1..=7).map(|day| view! {
                    <div class=style::calendar_weekday>{day_name(day % 7)}</div>
                }).collect_view()}
                {move || preview_days(preview_month.get())
                    .into_iter()
                    .map(|date| {
                        let label = date.day().to_string();
                        let template = items
                            .get()
                            .into_iter()
                            .filter(|item| item.template_id > 0)
                            .filter(|item| date_filter_applies(&item.date_filter, date))
                            .min_by_key(|item| item.index)
                            .map(|item| item.template_name);
                        view! {
                            <div class=style::calendar_cell>
                                <span class=style::calendar_day>{label}</span>
                                {template.map(|name| view! { <span class=style::calendar_item>{name}</span> })}
                            </div>
                        }
                    })
                    .collect_view()
                }
            </div>
        </div>
    }
}

fn preview_days((year, month): (i32, u32)) -> Vec<chrono::NaiveDate> {
    let Some(first) = chrono::NaiveDate::from_ymd_opt(year, month, 1) else {
        return Vec::new();
    };
    let offset = i64::from(first.weekday().num_days_from_monday());
    let start = first - chrono::Duration::days(offset);
    (0..42)
        .filter_map(|offset| start.checked_add_signed(chrono::Duration::days(offset)))
        .collect()
}

fn date_filter_applies(filter: &PlayoutDateFilter, date: chrono::NaiveDate) -> bool {
    let day_of_week = i32::try_from(date.weekday().num_days_from_sunday()).unwrap_or(0);
    let day = i32::try_from(date.day()).unwrap_or(0);
    let month = i32::try_from(date.month()).unwrap_or(0);
    if !filter.days_of_week.contains(&day_of_week)
        || !filter.days_of_month.contains(&day)
        || !filter.months_of_year.contains(&month)
    {
        return false;
    }
    if !filter.limit_to_date_range {
        return true;
    }

    let start_year = filter.start_year.unwrap_or(date.year());
    let end_year = filter.end_year.unwrap_or(date.year());
    let Some(start) = chrono::NaiveDate::from_ymd_opt(
        start_year,
        u32::try_from(filter.start_month).unwrap_or(1),
        u32::try_from(filter.start_day).unwrap_or(1),
    ) else {
        return false;
    };
    let Some(end) = chrono::NaiveDate::from_ymd_opt(
        end_year,
        u32::try_from(filter.end_month).unwrap_or(12),
        u32::try_from(filter.end_day).unwrap_or(31),
    ) else {
        return false;
    };
    date >= start && date <= end
}

fn current_year_month() -> (i32, u32) {
    #[cfg(target_arch = "wasm32")]
    {
        let date = js_sys::Date::new_0();
        (
            date.get_full_year() as i32,
            u32::try_from(date.get_month() as i32 + 1).unwrap_or(1),
        )
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        use chrono::Datelike;
        let today = chrono::Local::now().date_naive();
        (today.year(), today.month())
    }
}

fn shift_month((year, month): (i32, u32), delta: i32) -> (i32, u32) {
    let zero_based = year
        .saturating_mul(12)
        .saturating_add(i32::try_from(month).unwrap_or(1) - 1)
        .saturating_add(delta);
    let year = zero_based.div_euclid(12);
    let month = u32::try_from(zero_based.rem_euclid(12) + 1).unwrap_or(1);
    (year, month)
}

fn calendar_title((year, month): (i32, u32)) -> String {
    format!("{} {year}", month_name(i32::try_from(month).unwrap_or(1)))
}

fn year_options() -> Vec<i32> {
    let start = current_year_month().0;
    (start..start + 50).collect()
}

fn filter_label(min: i32, max: i32, value: i32) -> String {
    if min == 0 {
        day_name(value).to_owned()
    } else if max == 12 {
        month_name(value).to_owned()
    } else {
        value.to_string()
    }
}

fn add_alternate(
    items: RwSignal<Vec<PlayoutAlternateScheduleItemForm>>,
    selected: RwSignal<Option<usize>>,
    schedules: &[PlayoutNestedOption],
) {
    let schedule = schedules.first().cloned().unwrap_or_default();
    items.update(|rows| {
        rows.push(PlayoutAlternateScheduleItemForm {
            id: 0,
            index: i32::try_from(rows.len() + 1).unwrap_or(i32::MAX),
            program_schedule_id: schedule.id,
            program_schedule_name: schedule.name.clone(),
            date_filter: PlayoutDateFilter::default(),
        });
        selected.set(Some(rows.len().saturating_sub(1)));
    });
}

fn add_template(
    items: RwSignal<Vec<PlayoutTemplateItemForm>>,
    selected: RwSignal<Option<usize>>,
    groups: &[PlayoutNestedOption],
    templates: &[PlayoutNestedChildOption],
) {
    let group_id = groups.first().map_or(0, |group| group.id);
    let template = templates
        .iter()
        .find(|template| template.group_id == group_id)
        .cloned();
    items.update(|rows| {
        rows.push(PlayoutTemplateItemForm {
            id: 0,
            index: i32::try_from(rows.len() + 1).unwrap_or(i32::MAX),
            template_group_id: group_id,
            template_id: template.as_ref().map_or(0, |template| template.id),
            template_name: template.map(|template| template.name).unwrap_or_default(),
            deco_template_group_id: None,
            deco_template_id: None,
            deco_template_name: None,
            date_filter: PlayoutDateFilter::default(),
        });
        selected.set(Some(rows.len().saturating_sub(1)));
    });
}

fn move_item<T: Send + Sync + 'static>(
    items: RwSignal<Vec<T>>,
    selected: RwSignal<Option<usize>>,
    index: usize,
    delta: isize,
) {
    items.update(|rows| {
        let next = index.saturating_add_signed(delta);
        if index < rows.len() && next < rows.len() {
            rows.swap(index, next);
            selected.set(Some(next));
        }
    });
}

fn update_selected<T: Send + Sync + 'static>(
    items: RwSignal<Vec<T>>,
    selected: RwSignal<Option<usize>>,
    update: impl Fn(&mut T),
) {
    items.update(|rows| {
        if let Some(row) = selected
            .get_untracked()
            .and_then(|index| rows.get_mut(index))
        {
            update(row);
        }
    });
}

fn range_summary(values: &[i32], min: i32, max: i32) -> String {
    if values.len() == usize::try_from(max - min + 1).unwrap_or(0) {
        "*any*".to_owned()
    } else if min == 0 {
        let mut values = values.to_vec();
        values.sort_by_key(|day| (*day + 6) % 7);
        values
            .into_iter()
            .map(day_name)
            .collect::<Vec<_>>()
            .join(", ")
    } else if max == 12 {
        let mut values = values.to_vec();
        values.sort_unstable();
        values
            .into_iter()
            .map(month_abbrev)
            .collect::<Vec<_>>()
            .join(", ")
    } else {
        range_number_summary(values)
    }
}

fn range_number_summary(values: &[i32]) -> String {
    let mut values = values.to_vec();
    values.sort_unstable();
    values.dedup();
    let mut parts = Vec::new();
    let mut index = 0;
    while index < values.len() {
        let start = values[index];
        let mut end = start;
        while index + 1 < values.len() && values[index + 1] == end + 1 {
            index += 1;
            end = values[index];
        }
        if start == end {
            parts.push(start.to_string());
        } else {
            parts.push(format!("{start}-{end}"));
        }
        index += 1;
    }
    parts.join(", ")
}

fn day_name(value: i32) -> &'static str {
    match value {
        0 => tr(TextKey::Sunday),
        1 => tr(TextKey::Monday),
        2 => tr(TextKey::Tuesday),
        3 => tr(TextKey::Wednesday),
        4 => tr(TextKey::Thursday),
        5 => tr(TextKey::Friday),
        6 => tr(TextKey::Saturday),
        _ => "",
    }
}

fn month_name(value: i32) -> &'static str {
    match value {
        1 => tr(TextKey::January),
        2 => tr(TextKey::February),
        3 => tr(TextKey::March),
        4 => tr(TextKey::April),
        5 => tr(TextKey::May),
        6 => tr(TextKey::June),
        7 => tr(TextKey::July),
        8 => tr(TextKey::August),
        9 => tr(TextKey::September),
        10 => tr(TextKey::October),
        11 => tr(TextKey::November),
        12 => tr(TextKey::December),
        _ => "",
    }
}

fn month_abbrev(value: i32) -> &'static str {
    match value {
        1 => tr(TextKey::JanuaryShort),
        2 => tr(TextKey::FebruaryShort),
        3 => tr(TextKey::MarchShort),
        4 => tr(TextKey::AprilShort),
        5 => tr(TextKey::MayShort),
        6 => tr(TextKey::JuneShort),
        7 => tr(TextKey::JulyShort),
        8 => tr(TextKey::AugustShort),
        9 => tr(TextKey::SeptemberShort),
        10 => tr(TextKey::OctoberShort),
        11 => tr(TextKey::NovemberShort),
        12 => tr(TextKey::DecemberShort),
        _ => "",
    }
}
