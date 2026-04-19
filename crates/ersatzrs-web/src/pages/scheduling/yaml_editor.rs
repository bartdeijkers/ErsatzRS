//! YAML sequential playout editor — ports `Pages/ScriptedPlayoutEditor.razor`.
//!
//! Divergence: `ErsatzTV` stores a *file path* in `ScheduleFile`. `ErsatzRS`
//! stores the raw YAML *content* directly; this is the intended Phase 6.6
//! divergence.
//!
//! Two Leptos components live here:
//! - [`YamlEditorPage`] — thin SSR wrapper that loads the playout and mounts
//!   the island (or shows "not found" and redirects).
//! - [`YamlEditorIsland`] — the fully hydrated editor with debounced
//!   server-side YAML validation and a save/erase action bar.
#![allow(clippy::too_many_lines)]
use leptos::prelude::*;
use leptos_router::hooks::{use_navigate, use_params_map};
use leptos_router::NavigateOptions;

use crate::server::playouts::{
    erase_playout_items_and_history, get_sequential_playout, save_sequential_playout,
    validate_yaml_playout, SequentialPlayoutForm,
};
use crate::ui::primitives::dialog::Modal;
use crate::ui::primitives::toast::{use_toast, ToastSeverity};

stylance::import_crate_style!(style, "src/pages/scheduling/yaml_editor.module.css");

// ---------------------------------------------------------------------------
// SSR wrapper
// ---------------------------------------------------------------------------

/// SSR page for `/playouts/sequential/:id`.
///
/// Loads the Sequential playout from the database and mounts
/// [`YamlEditorIsland`]. If the id is absent or has the wrong
/// `ScheduleKind`, renders an "not found" message.
#[component]
pub fn YamlEditorPage() -> impl IntoView {
    let params = use_params_map();
    let playout_id =
        move || params.with(|p| p.get("id").and_then(|s| s.parse::<i64>().ok()).unwrap_or(0));

    let playout_data: Resource<Option<SequentialPlayoutForm>> =
        Resource::new(playout_id, |id| async move {
            get_sequential_playout(id).await.unwrap_or(None)
        });

    view! {
        <Suspense fallback=move || view! { <p class=style::empty>"Loading\u{2026}"</p> }>
            {move || {
                playout_data.get().map(|maybe| match maybe {
                    Some(form) => view! { <YamlEditorIsland initial_form=form/> }.into_any(),
                    None => view! {
                        <p class=style::empty>"Playout not found."</p>
                    }
                        .into_any(),
                })
            }}
        </Suspense>
    }
}

// ---------------------------------------------------------------------------
// Island — fully hydrated YAML editor
// ---------------------------------------------------------------------------

/// Fully hydrated YAML editor for a Sequential playout.
///
/// Props are serialized to the HTML shell by Leptos and rehydrated in the
/// browser, so [`SequentialPlayoutForm`] must implement `Serialize`/`Deserialize`.
#[island]
fn YamlEditorIsland(initial_form: SequentialPlayoutForm) -> impl IntoView {
    // ── Signals ─────────────────────────────────────────────────────────
    let playout_id = initial_form.id;
    let channel_label = initial_form.channel_label.clone();

    let yaml_content = RwSignal::new(initial_form.yaml_content.clone());
    let validation_error: RwSignal<Option<String>> = RwSignal::new(None);
    // Generation counter — incremented before each debounced spawn so stale
    // async closures can self-cancel.
    let validation_gen: RwSignal<u32> = RwSignal::new(0);
    let pending_erase = RwSignal::new(false);

    // ── Actions ─────────────────────────────────────────────────────────
    let save_action = Action::new(|(id, content): &(i64, String)| {
        let id = *id;
        let content = content.clone();
        async move {
            // Re-validate before persisting to guard against the debounce race
            // where the user saves before the 500 ms validation completes.
            validate_yaml_playout(content.clone()).await?;
            save_sequential_playout(id, content).await
        }
    });

    let erase_action = Action::new(|id: &i64| {
        let id = *id;
        async move { erase_playout_items_and_history(id).await }
    });

    let toast = use_toast();
    let navigate = use_navigate();

    // ── Save Effect — navigate to /playouts on Ok ────────────────────────
    Effect::new(move |_| {
        if let Some(res) = save_action.value().get() {
            match res {
                Ok(()) => {
                    navigate("/playouts", NavigateOptions::default());
                }
                Err(e) => toast.push(format!("Save failed: {e}"), ToastSeverity::Error),
            }
        }
    });

    // ── Erase Effect — toast + close dialog on Ok ────────────────────────
    Effect::new(move |_| {
        if let Some(res) = erase_action.value().get() {
            match res {
                Ok(()) => {
                    toast.push("Erased playout items and history.", ToastSeverity::Success);
                    pending_erase.set(false);
                }
                Err(e) => toast.push(format!("Erase failed: {e}"), ToastSeverity::Error),
            }
        }
    });

    // ── Debounced validation Effect ──────────────────────────────────────
    //
    // Skips the initial run using the `prev: Option<()>` pattern (same as the
    // block editor's `col_type` effect). Bumps `validation_gen` before each
    // spawn so any pending spawn from a previous keystroke self-cancels.
    Effect::new(move |prev: Option<()>| {
        let content = yaml_content.get();
        if prev.is_none() {
            return;
        }
        let gen = validation_gen.get_untracked() + 1;
        validation_gen.set(gen);
        leptos::task::spawn_local(async move {
            gloo_timers::future::TimeoutFuture::new(500).await;
            if validation_gen.get_untracked() != gen {
                return;
            }
            match validate_yaml_playout(content).await {
                Ok(()) => validation_error.set(None),
                Err(e) => validation_error.set(Some(e.to_string())),
            }
        });
    });

    // ── View ─────────────────────────────────────────────────────────────
    view! {
        <div class=style::page>
            // ── Sticky action bar ──────────────────────────────────────
            <div class=style::action_bar>
                <span class=style::action_bar_title>
                    {channel_label.clone()} " \u{2014} Sequential Schedule"
                </span>
                <button
                    class=style::btn_primary
                    disabled=move || save_action.pending().get() || validation_error.get().is_some()
                    on:click=move |_| {
                        let content = yaml_content.get_untracked();
                        save_action.dispatch((playout_id, content));
                    }
                >
                    "Save"
                </button>
                <button
                    class=style::btn_danger
                    on:click=move |_| pending_erase.set(true)
                >
                    "Erase Items \u{26} History"
                </button>
            </div>

            // ── Inline validation error banner ────────────────────────
            {move || {
                validation_error.get().map(|msg| {
                    view! { <div class=style::error_banner>{msg}</div> }
                })
            }}

            // ── YAML textarea ──────────────────────────────────────────
            <div class=style::editor_wrap>
                <textarea
                    class=style::yaml_textarea
                    prop:value=move || yaml_content.get()
                    on:input=move |e| yaml_content.set(event_target_value(&e))
                    rows=30
                    spellcheck="false"
                />
            </div>

            // ── Erase confirm dialog ───────────────────────────────────
            <Modal
                open=pending_erase
                title="Erase Items and History".to_string()
                on_close=Callback::new(move |confirmed: bool| {
                    if confirmed {
                        erase_action.dispatch(playout_id);
                    }
                })
                confirm_label="Erase"
                destructive=true
            >
                <p>
                    "This will delete all playout items and history for this sequential playout. Continue?"
                </p>
            </Modal>
        </div>
    }
}
