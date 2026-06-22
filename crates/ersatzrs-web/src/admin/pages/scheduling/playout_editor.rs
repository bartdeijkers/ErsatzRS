//! Add-playout page — ports `Pages/PlayoutEditor.razor`.
//!
//! Routes `/playouts/add` and `/playouts/add/{kind}` render the same form with
//! kind-specific schedule inputs. The native form posts to `/api/playouts/create`
//! so the route works without hydration; the hydrated submit uses the same
//! server-side create path and returns to `/playouts`.

use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::admin::i18n::{tr, TextKey};
use crate::admin::server_fns::playouts::{
    create_playout, list_playout_create_channels, list_playout_create_schedules,
    PlayoutCreateChannel, PlayoutCreateForm, PlayoutCreateSchedule,
};
use crate::admin::ui::primitives::icon::{icon_style, IconName};

stylance::import_crate_style!(
    style,
    "src/admin/pages/scheduling/playout_editor.module.css"
);

#[component]
pub fn PlayoutEditorPage() -> impl IntoView {
    let params = use_params_map();
    let kind = params.with(|p| p.get("kind").unwrap_or_default());
    let initial: Resource<(Vec<PlayoutCreateChannel>, Vec<PlayoutCreateSchedule>)> =
        Resource::new_blocking(
            || (),
            |()| async move {
                let (channels, schedules) = tokio::join!(
                    list_playout_create_channels(),
                    list_playout_create_schedules(),
                );
                (channels.unwrap_or_default(), schedules.unwrap_or_default())
            },
        );

    view! {
        <Suspense fallback=move || view! { <p class=style::caption>{tr(TextKey::Loading)}</p> }>
            {move || {
                initial.get().map(|(initial_channels, initial_schedules)| {
                    view! {
                        <PlayoutEditorIsland
                            initial_kind=kind.clone()
                            initial_channels=initial_channels
                            initial_schedules=initial_schedules
                        />
                    }
                })
            }}
        </Suspense>
    }
}

#[island]
fn PlayoutEditorIsland(
    initial_kind: String,
    initial_channels: Vec<PlayoutCreateChannel>,
    initial_schedules: Vec<PlayoutCreateSchedule>,
) -> impl IntoView {
    let form = RwSignal::new(PlayoutCreateForm {
        kind: canonical_kind(&initial_kind).to_owned(),
        ..PlayoutCreateForm::default()
    });
    let save_error = RwSignal::new(String::new());
    let channels = RwSignal::new(initial_channels);
    let schedules = RwSignal::new(initial_schedules);

    let save_action = Action::new(|form: &PlayoutCreateForm| create_playout(form.clone()));
    Effect::new(move |_| {
        if let Some(result) = save_action.value().get() {
            match result {
                Ok(_) => {
                    save_error.set(String::new());
                    #[cfg(target_arch = "wasm32")]
                    {
                        let _ = window().location().set_href("/playouts");
                    }
                }
                Err(error) => save_error.set(error.to_string()),
            }
        }
    });

    view! {
        <div class=style::page>
            <div class=style::action_bar>
                <button type="submit" form="playout-editor-form" class=style::btn_add>
                    <span style=icon_style(IconName::Add, "1.125rem") aria-hidden="true"></span>
                    <span>{tr(TextKey::AddPlayout)}</span>
                </button>
                <a href="/playouts" class=style::btn_default>
                    {tr(TextKey::Back)}
                </a>
            </div>
            <section class=style::content>
                <h1 class=style::page_title>{tr(TextKey::Playout)}</h1>
                <div class=style::divider/>
                <p class=style::error style:display=move || if save_error.get().is_empty() { "none" } else { "" }>
                    {move || save_error.get()}
                </p>
                <form
                    id="playout-editor-form"
                    class=style::editor_form
                    method="post"
                    action="/api/playouts/create"
                    on:submit=move |event| {
                        event.prevent_default();
                        save_action.dispatch(form.get_untracked());
                    }
                >
                    <input type="hidden" name="kind" value=move || form.get().kind/>
                    {move || channel_field(form, channels.get())}

                    {move || match form.get().kind.as_str() {
                        "" | "Classic" => classic_schedule_field(form, schedules).into_any(),
                        "ExternalJson" => schedule_file_field(form, tr(TextKey::JsonDizqueTvScheduleMenu)).into_any(),
                        "Sequential" => schedule_file_with_help(
                            form,
                            tr(TextKey::SequentialSchedule),
                            tr(TextKey::SequentialScheduleFileHelp),
                        ).into_any(),
                        "Scripted" => schedule_file_field(form, tr(TextKey::ScriptedScheduleMenu)).into_any(),
                        "Block" => view! {
                            <p class=style::caption>{tr(TextKey::BlockTemplatesAddedLater)}</p>
                        }.into_any(),
                        _ => view! {
                            <p class=style::error>{tr(TextKey::UnsupportedPlayoutKind)}</p>
                        }.into_any(),
                    }}
                </form>
            </section>
        </div>
    }
}

fn channel_field(
    form: RwSignal<PlayoutCreateForm>,
    channels: Vec<PlayoutCreateChannel>,
) -> impl IntoView {
    view! {
        <label class=style::field>
            <span class=style::field_text>{tr(TextKey::Channel)}</span>
            <select
                class=style::select_input
                name="channel_id"
                required
                prop:value=move || selected_i64(form.get().channel_id)
                on:change=move |event| {
                    form.update(|form| {
                        form.channel_id = event_target_value(&event).parse().unwrap_or(0);
                    });
                }
            >
                <option value="">{tr(TextKey::SelectValue)}</option>
                <For
                    each=move || channels.clone()
                    key=|channel| channel.id
                    let:channel
                >
                    {channel_option(&channel)}
                </For>
            </select>
            <small class=style::helper_text>
                {tr(TextKey::DisabledChannelsAlreadyHavePlayout)}
            </small>
        </label>
    }
}

fn classic_schedule_field(
    form: RwSignal<PlayoutCreateForm>,
    schedules: RwSignal<Vec<PlayoutCreateSchedule>>,
) -> impl IntoView {
    view! { {move || classic_schedule_select(form, schedules.get())} }
}

fn classic_schedule_select(
    form: RwSignal<PlayoutCreateForm>,
    schedules: Vec<PlayoutCreateSchedule>,
) -> impl IntoView {
    view! {
        <label class=style::field>
            <span class=style::field_text>{tr(TextKey::ProgramSchedule)}</span>
            <select
                class=style::select_input
                name="program_schedule_id"
                required
                prop:value=move || selected_option_i64(form.get().program_schedule_id)
                on:change=move |event| {
                    form.update(|form| {
                        form.program_schedule_id = event_target_value(&event).parse().ok();
                    });
                }
            >
                <option value="">{tr(TextKey::SelectValue)}</option>
                <For
                    each=move || schedules.clone()
                    key=|schedule| schedule.id
                    let:schedule
                >
                    <option value=schedule.id.to_string()>{schedule.name}</option>
                </For>
            </select>
        </label>
    }
}

fn schedule_file_field(form: RwSignal<PlayoutCreateForm>, label: &'static str) -> impl IntoView {
    schedule_file_with_help(form, label, "")
}

fn schedule_file_with_help(
    form: RwSignal<PlayoutCreateForm>,
    label: &'static str,
    help: &'static str,
) -> impl IntoView {
    view! {
        <label class=style::field>
            <span class=style::field_text>{label}</span>
            <input
                class=style::input
                name="schedule_file"
                required
                value=move || form.get().schedule_file
                on:input=move |event| {
                    form.update(|form| form.schedule_file = event_target_value(&event));
                }
            />
            <small class=style::helper_text style:display=if help.is_empty() { "none" } else { "" }>
                {help}
            </small>
        </label>
    }
}

fn canonical_kind(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().as_str() {
        "block" => "Block",
        "sequential" => "Sequential",
        "scripted" => "Scripted",
        "externaljson" | "external-json" | "external_json" => "ExternalJson",
        _ => "Classic",
    }
}

fn channel_label(channel: &PlayoutCreateChannel) -> String {
    if channel.number.trim().is_empty() {
        channel.name.clone()
    } else if channel.name.trim().is_empty() {
        channel.number.clone()
    } else {
        format!("{} - {}", channel.number, channel.name)
    }
}

fn channel_option(channel: &PlayoutCreateChannel) -> impl IntoView {
    let value = channel.id.to_string();
    let label = channel_label(channel);
    if channel.playout_count > 0 {
        view! { <option value=value disabled>{label}</option> }.into_any()
    } else {
        view! { <option value=value>{label}</option> }.into_any()
    }
}

fn selected_i64(value: i64) -> String {
    if value == 0 {
        String::new()
    } else {
        value.to_string()
    }
}

fn selected_option_i64(value: Option<i64>) -> String {
    value
        .filter(|id| *id > 0)
        .map(|id| id.to_string())
        .unwrap_or_default()
}
