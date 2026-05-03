#![allow(clippy::too_many_lines)]
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::admin::server_fns::channels::{
    get_channel_form, list_ffmpeg_profiles_for_select, list_filler_presets_for_select, ChannelForm,
};

stylance::import_crate_style!(style, "src/admin/pages/channels/editor.module.css");

type ChannelEditorPageData = (ChannelForm, Vec<(i64, String)>, Vec<(i64, String)>);

#[component]
pub fn ChannelEditorPage() -> impl IntoView {
    let params = use_params_map();
    let channel_id = move || params.with(|p| p.get("id").and_then(|s| s.parse::<i64>().ok()));

    let data: Resource<ChannelEditorPageData> =
        Resource::new_blocking(channel_id, |id| async move {
            let form = async move {
                match id {
                    Some(id) => get_channel_form(id).await.unwrap_or_default(),
                    None => ChannelForm::new_defaults(),
                }
            };
            let profiles = async { list_ffmpeg_profiles_for_select().await.unwrap_or_default() };
            let fillers = async { list_filler_presets_for_select().await.unwrap_or_default() };
            let (form, profiles, fillers) = tokio::join!(form, profiles, fillers);
            (form, profiles, fillers)
        });

    view! {
        <Suspense fallback=|| view! { <p class=style::loading>"Loading..."</p> }>
            {move || {
                data.get().map(|(form, profiles, fillers)| {
                    view! {
                        <ChannelEditorIsland
                            initial_form=form
                            initial_profiles=profiles
                            initial_fillers=fillers
                        />
                    }
                    .into_any()
                })
            }}
        </Suspense>
    }
}

#[component]
fn ChannelEditorIsland(
    initial_form: ChannelForm,
    initial_profiles: Vec<(i64, String)>,
    initial_fillers: Vec<(i64, String)>,
) -> impl IntoView {
    let channel_id = initial_form.id;
    let number = RwSignal::new(initial_form.number);
    let name = RwSignal::new(initial_form.name);
    let group = RwSignal::new(initial_form.group);
    let ffmpeg_profile_id = RwSignal::new(initial_form.ffmpeg_profile_id);
    let streaming_mode = RwSignal::new(initial_form.streaming_mode);
    let is_enabled = RwSignal::new(initial_form.is_enabled);
    let show_in_epg = RwSignal::new(initial_form.show_in_epg);
    let fallback_filler_id = RwSignal::new(initial_form.fallback_filler_id);

    let number_err: RwSignal<Option<String>> = RwSignal::new(None);
    let name_err: RwSignal<Option<String>> = RwSignal::new(None);
    let profile_err: RwSignal<Option<String>> = RwSignal::new(None);

    let on_save = move || {
        let num = number.get();
        let nm = name.get();
        let pid = ffmpeg_profile_id.get();

        number_err.set(None);
        name_err.set(None);
        profile_err.set(None);

        let mut valid = true;
        if num.trim().is_empty() {
            number_err.set(Some("Number is required.".to_string()));
            valid = false;
        }
        if nm.trim().is_empty() {
            name_err.set(Some("Name is required.".to_string()));
            valid = false;
        }
        if pid == 0 {
            profile_err.set(Some("FFmpeg profile is required.".to_string()));
            valid = false;
        }

        valid
    };

    let page_title = if channel_id != 0 {
        "Edit Channel"
    } else {
        "Add Channel"
    };

    let streaming_mode_items: Vec<(i32, String)> = vec![
        (1, "Transport Stream".to_string()),
        (2, "HLS Direct".to_string()),
        (4, "HLS Segmenter".to_string()),
        (5, "Transport Stream Hybrid".to_string()),
    ];
    let mut profile_items = initial_profiles;
    let has_profiles = !profile_items.is_empty();
    profile_items.insert(0, (0i64, "- Select profile -".to_string()));
    let mut filler_items = initial_fillers;
    filler_items.insert(0, (0i64, "None".to_string()));
    let select_class = format!("{} {}", style::input, style::select);

    view! {
        <>
            <div class=style::page>
                <div class=style::breadcrumb>
                    <a href="/channels" class=style::back_link>"← Channels"</a>
                    <h1 class=style::page_title inner_html=page_title></h1>
                </div>

                <form method="post" action="/api/channels/save">
                <input type="hidden" name="id" value=channel_id.to_string()/>
                <div class=style::card>
                <div class=style::form_grid>
                    <div class=style::field>
                        <label class=style::label>"Channel Number"</label>
                        <input
                            name="number"
                            type="text"
                            placeholder="e.g. 1 or 2.1"
                            class=style::input
                            value=number.get_untracked()
                            prop:value=move || number.get()
                            on:input=move |e| number.set(event_target_value(&e))
                        />
                        <span
                            class=style::error
                            inner_html=move || number_err.get().unwrap_or_default()
                        ></span>
                    </div>
                    <div class=style::field>
                        <label class=style::label>"Name"</label>
                        <input
                            name="name"
                            type="text"
                            placeholder="My Channel"
                            class=style::input
                            value=name.get_untracked()
                            prop:value=move || name.get()
                            on:input=move |e| name.set(event_target_value(&e))
                        />
                        <span
                            class=style::error
                            inner_html=move || name_err.get().unwrap_or_default()
                        ></span>
                    </div>
                </div>

                <div class=style::field>
                    <label class=style::label>"Group (optional)"</label>
                    <input
                        name="group"
                        type="text"
                        placeholder="Leave blank for no group"
                        class=style::input
                        value=group.get_untracked()
                        prop:value=move || group.get()
                        on:input=move |e| group.set(event_target_value(&e))
                    />
                    <span class=style::error></span>
                </div>

                <div class=style::field>
                    <label class=style::label>"Streaming Mode"</label>
                    <select
                        name="streaming_mode"
                        class=select_class.clone()
                        prop:value=move || streaming_mode.get().to_string()
                        on:change=move |e| {
                            if let Ok(parsed) = event_target_value(&e).parse::<i32>() {
                                streaming_mode.set(parsed);
                            }
                        }
                    >
                        {streaming_mode_items
                            .into_iter()
                            .map(|(value, label)| {
                                let selected = value == streaming_mode.get_untracked();
                                view! {
                                    <option value=value.to_string() selected=selected>{label}</option>
                                }
                            })
                            .collect_view()}
                    </select>
                    <span class=style::error></span>
                </div>

                <div class=style::field>
                    <label class=style::label>"FFmpeg Profile"</label>
                    <select
                        name="ffmpeg_profile_id"
                        class=select_class.clone()
                        prop:value=move || ffmpeg_profile_id.get().to_string()
                        on:change=move |e| {
                            if let Ok(parsed) = event_target_value(&e).parse::<i64>() {
                                ffmpeg_profile_id.set(parsed);
                            }
                        }
                    >
                        {profile_items
                            .into_iter()
                            .map(|(value, label)| {
                                let selected = value == ffmpeg_profile_id.get_untracked();
                                view! {
                                    <option value=value.to_string() selected=selected>{label}</option>
                                }
                            })
                            .collect_view()}
                    </select>
                    <span
                        class=style::error
                        inner_html=move || profile_err.get().unwrap_or_default()
                    ></span>
                </div>

                <p class=style::no_profiles hidden=has_profiles>
                    "No FFmpeg profiles found. Create one first."
                </p>

                <div class=style::field>
                    <label class=style::label>"Fallback Filler Preset"</label>
                    <select
                        name="fallback_filler_id"
                        class=select_class
                        prop:value=move || fallback_filler_id.get().to_string()
                        on:change=move |e| {
                            if let Ok(parsed) = event_target_value(&e).parse::<i64>() {
                                fallback_filler_id.set(parsed);
                            }
                        }
                    >
                        {filler_items
                            .into_iter()
                            .map(|(value, label)| {
                                let selected = value == fallback_filler_id.get_untracked();
                                view! {
                                    <option value=value.to_string() selected=selected>{label}</option>
                                }
                            })
                            .collect_view()}
                    </select>
                    <span class=style::error></span>
                </div>

                <div class=style::checkboxes>
                    <label class=style::checkbox_label>
                        <input
                            name="is_enabled"
                            type="checkbox"
                            value="true"
                            class=style::checkbox
                            checked=is_enabled.get_untracked()
                            prop:checked=move || is_enabled.get()
                            on:change=move |e| is_enabled.set(event_target_checked(&e))
                        />
                        <span class=style::checkbox_text>"Enabled"</span>
                    </label>
                    <label class=style::checkbox_label>
                        <input
                            name="show_in_epg"
                            type="checkbox"
                            value="true"
                            class=style::checkbox
                            checked=show_in_epg.get_untracked()
                            prop:checked=move || show_in_epg.get()
                            on:change=move |e| show_in_epg.set(event_target_checked(&e))
                        />
                        <span class=style::checkbox_text>"Show in EPG"</span>
                    </label>
                </div>
            </div>

            <div class=style::form_actions>
                <button
                    type="submit"
                    class=style::btn_primary
                    on:click=move |ev| {
                        if !on_save() {
                            ev.prevent_default();
                        }
                    }
                >
                    "Save"
                </button>
                <a href="/channels" class=style::cancel_link>"Cancel"</a>
            </div>
                </form>
            </div>
        </>
    }
}
