use leptos::prelude::*;

#[cfg(target_arch = "wasm32")]
use crate::admin::server_fns::media_browser::list_program_schedules;
use crate::admin::server_fns::media_browser::{add_media_item_to_schedule, ScheduleOption};
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
pub(crate) fn AddToScheduleModal(
    open: RwSignal<bool>,
    media_item_id: i64,
    collection_type: i64,
    success_label: &'static str,
    #[prop(optional)] redirect_after_add: bool,
) -> impl IntoView {
    let options: RwSignal<Vec<ScheduleOption>> = RwSignal::new(Vec::new());
    let schedule_id: RwSignal<i64> = RwSignal::new(0);

    #[cfg(target_arch = "wasm32")]
    Effect::new(move |_| {
        if !open.get() {
            return;
        }
        leptos::task::spawn_local(async move {
            let loaded = list_program_schedules().await.unwrap_or_default();
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
        async move {
            if chosen == 0 {
                return Err(ServerFnError::new("Select a schedule"));
            }
            add_media_item_to_schedule(chosen, collection_type, media_item_id).await
        }
    });

    let toast = use_toast();
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
                Err(e) => toast.push(format!("Add to schedule failed: {e}"), ToastSeverity::Error),
            }
        }
    });

    view! {
        <Modal
            open=open
            title="Add To Schedule".to_string()
            on_close=Callback::new(move |confirmed: bool| {
                if confirmed {
                    submit_action.dispatch(());
                }
            })
            confirm_label="Add To Schedule"
            cancel_label="Cancel"
        >
            <div style="display: flex; flex-direction: column; gap: 0.5rem; margin-top: 0.75rem">
                <label>"Schedule"</label>
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
