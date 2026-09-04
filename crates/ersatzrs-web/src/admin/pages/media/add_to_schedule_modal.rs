use leptos::prelude::*;

use crate::admin::i18n::{media_browser_error_message, tr, TextKey};
#[cfg(target_arch = "wasm32")]
use crate::admin::server_fns::media_browser::list_program_schedules;
use crate::admin::server_fns::media_browser::{
    add_media_item_to_schedule, add_media_item_to_schedule_with_stream_selection,
    PinnedStreamSelectionForm, ScheduleOption,
};
use crate::admin::ui::primitives::dialog::Modal;
use crate::admin::ui::primitives::toast::{use_toast, ToastSeverity};

#[cfg(target_arch = "wasm32")]
fn set_browser_location(path: &str) {
    if let Some(window) = web_sys::window() {
        let _ = window.location().set_href(path);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn set_browser_location(_path: &str) {}

#[component]
#[allow(clippy::too_many_lines)]
pub(crate) fn AddToScheduleModal(
    open: RwSignal<bool>,
    media_item_id: i64,
    collection_type: i64,
    success_label: &'static str,
    #[prop(optional)] redirect_after_add: bool,
    #[prop(optional)] pinned_streams: Option<RwSignal<Option<PinnedStreamSelectionForm>>>,
) -> impl IntoView {
    let options: RwSignal<Vec<ScheduleOption>> = RwSignal::new(Vec::new());
    let schedule_id: RwSignal<i64> = RwSignal::new(0);
    let toast = use_toast();

    #[cfg(target_arch = "wasm32")]
    Effect::new(move |_| {
        if !open.get() {
            return;
        }
        leptos::task::spawn_local(async move {
            let loaded = match list_program_schedules().await {
                Ok(loaded) => loaded,
                Err(error) => {
                    toast.push(
                        format!("{}: {error}", tr(TextKey::ErrorOccurred)),
                        ToastSeverity::Error,
                    );
                    return;
                }
            };
            if schedule_id.get_untracked() == 0 {
                if let Some(first) = loaded.first() {
                    schedule_id.set(first.id);
                }
            }
            options.set(loaded);
        });
    });

    let submit_action = Action::new(move |(): &()| {
        let chosen = schedule_id.get_untracked();
        let pinned_streams = pinned_streams.and_then(|signal| signal.get_untracked());
        async move {
            if chosen == 0 {
                return Err(ServerFnError::new(tr(TextKey::SelectSchedule)));
            }
            if let Some(selection) = pinned_streams {
                add_media_item_to_schedule_with_stream_selection(
                    chosen,
                    collection_type,
                    media_item_id,
                    selection,
                )
                .await
            } else {
                add_media_item_to_schedule(chosen, collection_type, media_item_id).await
            }
        }
    });

    Effect::new(move |_| {
        if let Some(res) = submit_action.value().get() {
            match res {
                Ok(()) => {
                    toast.push(success_label, ToastSeverity::Success);
                    open.set(false);
                    if redirect_after_add {
                        set_browser_location(&format!(
                            "/schedules/{}/items",
                            schedule_id.get_untracked()
                        ));
                    }
                }
                Err(e) => toast.push(
                    format!(
                        "{}: {}",
                        tr(TextKey::AddFailed),
                        media_browser_error_message(e)
                    ),
                    ToastSeverity::Error,
                ),
            }
        }
    });

    view! {
        <Modal
            dialog_id="media-add-to-schedule-dialog"
            open=open
            title=tr(TextKey::AddToSchedule).to_string()
            on_close=Callback::new(move |confirmed: bool| {
                if confirmed {
                    submit_action.dispatch(());
                }
            })
            confirm_label=Some(tr(TextKey::AddToSchedule))
            cancel_label=Some(tr(TextKey::Cancel))
        >
            <div style="display: flex; flex-direction: column; gap: 0.5rem; margin-top: 0.75rem">
                <label>{tr(TextKey::Schedule)}</label>
                <select
                    prop:value=move || schedule_id.get().to_string()
                    on:change=move |e| {
                        if let Ok(v) = event_target_value(&e).parse::<i64>() {
                            schedule_id.set(v);
                        }
                    }
                >
                    {move || {
                        options.get().into_iter().map(|s| view! {
                            <option value=s.id.to_string()>{s.name}</option>
                        }).collect_view()
                    }}
                </select>
            </div>
        </Modal>
    }
}
