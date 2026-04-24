#![allow(clippy::too_many_lines)]
use leptos::prelude::*;
use leptos_router::hooks::{use_navigate, use_params_map};
use leptos_router::NavigateOptions;

use crate::admin::server_fns::channels::{
    get_channel_form, list_ffmpeg_profiles_for_select, list_filler_presets_for_select,
    save_channel, ChannelForm,
};
use crate::admin::ui::primitives::{
    button::{Button, ButtonVariant},
    form::{CheckBox, SelectField, TextField},
    toast::{use_toast, ToastSeverity},
};

stylance::import_crate_style!(style, "src/admin/pages/channels/editor.module.css");

type ChannelEditorPageData = (ChannelForm, Vec<(i64, String)>, Vec<(i64, String)>);

#[component]
pub fn ChannelEditorPage() -> impl IntoView {
    let params = use_params_map();
    let channel_id = move || params.with(|p| p.get("id").and_then(|s| s.parse::<i64>().ok()));

    let data: Resource<ChannelEditorPageData> = Resource::new(channel_id, |id| async move {
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
                })
            }}
        </Suspense>
    }
}

#[island]
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

    let navigate = use_navigate();
    let toast = use_toast();

    let save_action = Action::new(move |form: &ChannelForm| {
        let form = form.clone();
        async move { save_channel(form).await }
    });

    Effect::new(move |_| match save_action.value().get() {
        Some(Ok(_)) => {
            toast.push("Channel saved.", ToastSeverity::Success);
            navigate("/channels", NavigateOptions::default());
        }
        Some(Err(e)) => {
            toast.push(format!("Save failed: {e}"), ToastSeverity::Error);
        }
        None => {}
    });

    let on_save = move |()| {
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

        if valid {
            save_action.dispatch(ChannelForm {
                id: channel_id,
                number: num,
                name: nm,
                group: group.get(),
                ffmpeg_profile_id: pid,
                streaming_mode: streaming_mode.get(),
                is_enabled: is_enabled.get(),
                show_in_epg: show_in_epg.get(),
                fallback_filler_id: fallback_filler_id.get(),
            });
        }
    };

    let page_title = move || {
        if channel_id != 0 {
            "Edit Channel"
        } else {
            "Add Channel"
        }
    };

    let streaming_mode_items: Vec<(i32, String)> = vec![
        (1, "Transport Stream".to_string()),
        (2, "HLS Direct".to_string()),
        (4, "HLS Segmenter".to_string()),
        (5, "Transport Stream Hybrid".to_string()),
    ];

    view! {
        <div class=style::page>
            <div class=style::breadcrumb>
                <a href="/channels" class=style::back_link>"← Channels"</a>
                <h1 class=style::page_title>{page_title}</h1>
            </div>

            <Suspense fallback=|| view! { <p class=style::loading>"Loading..."</p> }>
                <div class=style::card>
                    <div class=style::form_grid>
                        <TextField
                            label="Channel Number"
                            value=number
                            placeholder="e.g. 1 or 2.1"
                            error=Some(Signal::derive(move || number_err.get()))
                        />
                        <TextField
                            label="Name"
                            value=name
                            placeholder="My Channel"
                            error=Some(Signal::derive(move || name_err.get()))
                        />
                    </div>

                    <TextField
                        label="Group (optional)"
                        value=group
                        placeholder="Leave blank for no group"
                    />

                    <SelectField<i32>
                        label="Streaming Mode"
                        value=streaming_mode
                        items=streaming_mode_items
                    />

                    {{
                        let mut items = initial_profiles;
                        if items.is_empty() {
                            view! {
                                <p class=style::no_profiles>
                                    "No FFmpeg profiles found. Create one first."
                                </p>
                            }.into_any()
                        } else {
                            items.insert(0, (0i64, "— Select profile —".to_string()));
                            view! {
                                <SelectField<i64>
                                    label="FFmpeg Profile"
                                    value=ffmpeg_profile_id
                                    items=items
                                    error=Some(Signal::derive(move || profile_err.get()))
                                />
                            }.into_any()
                        }
                    }}

                    {{
                        let mut items = initial_fillers;
                        items.insert(0, (0i64, "None".to_string()));
                        view! {
                            <SelectField<i64>
                                label="Fallback Filler Preset"
                                value=fallback_filler_id
                                items=items
                            />
                        }
                    }}

                    <div class=style::checkboxes>
                        <CheckBox label="Enabled" value=is_enabled/>
                        <CheckBox label="Show in EPG" value=show_in_epg/>
                    </div>
                </div>

                <div class=style::form_actions>
                    <Button
                        variant=ButtonVariant::Primary
                        on_click=Some(Callback::new(on_save))
                    >
                        {move || if save_action.pending().get() { "Saving…" } else { "Save" }}
                    </Button>
                    <a href="/channels" class=style::cancel_link>"Cancel"</a>
                </div>
            </Suspense>
        </div>
    }
}
