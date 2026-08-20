use leptos::{html, prelude::*};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;

use crate::admin::i18n::{tr, TextKey};

use super::button::{Button, ButtonVariant};

stylance::import_crate_style!(style, "src/admin/ui/primitives/dialog.module.css");

/// A modal dialog with a stable DOM shape (no `<Show>` — uses `style:display`).
///
/// The backdrop and panel are always present in the DOM; `open` controls
/// visibility.  This avoids `failed_to_cast_marker_node` hydration mismatches
/// that arise when the dialog is conditionally inserted into an `#[island]`.
#[allow(clippy::needless_pass_by_value)]
#[component]
pub fn Modal(
    /// Signal controlling visibility.
    open: RwSignal<bool>,
    /// Dialog title shown in the header. Reactive, so one dialog can serve an
    /// add and an edit flow without a second copy of its body in the DOM.
    #[prop(into)]
    title: Signal<String>,
    /// Called when the user confirms or cancels. Receives `true` for confirm.
    on_close: Callback<bool>,
    /// Content inside the dialog body.
    children: ChildrenFn,
    #[prop(default = None)] confirm_label: Option<&'static str>,
    #[prop(default = None)] cancel_label: Option<&'static str>,
    /// Blocks confirmation while the dialog's own content is incomplete.
    #[prop(into, default = Signal::from(false))]
    confirm_disabled: Signal<bool>,
    #[prop(default = false)] destructive: bool,
    #[prop(default = false)] wide: bool,
) -> impl IntoView {
    let panel_ref = NodeRef::<html::Div>::new();
    let confirm_variant = if destructive {
        ButtonVariant::Danger
    } else {
        ButtonVariant::Primary
    };

    let do_cancel = move || {
        open.set(false);
        on_close.run(false);
    };
    let do_confirm = move || {
        open.set(false);
        on_close.run(true);
    };

    #[cfg(target_arch = "wasm32")]
    {
        let was_open = StoredValue::new(false);
        let previous_focus = StoredValue::new(None::<web_sys::HtmlElement>);
        Effect::new(move |_| {
            let is_open = open.get();
            let previously_open = was_open.get_value();
            if is_open && !previously_open {
                let active = web_sys::window()
                    .and_then(|window| window.document())
                    .and_then(|document| document.active_element())
                    .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok());
                previous_focus.set_value(active);
                if let Some(panel) = panel_ref.get() {
                    let _ = panel.focus();
                }
            } else if !is_open && previously_open {
                if let Some(element) = previous_focus.get_value() {
                    let _ = element.focus();
                }
                previous_focus.set_value(None);
            }
            was_open.set_value(is_open);
        });
    }

    view! {
        // Backdrop is purely visual; it sits below the overlay
        // (z-index 40 vs 50) and never receives clicks because the
        // overlay's `inset: 0` covers it. The click-to-dismiss handler
        // therefore lives on the overlay (`do_cancel`), with the inner
        // panel calling `stop_propagation` so clicks inside the dialog
        // body do not close it.
        <div
            class=style::backdrop
            style:display=move || if open.get() { "" } else { "none" }
        />
        <div
            class=style::overlay
            role="dialog"
            aria-modal="true"
            aria-label=move || title.get()
            style:display=move || if open.get() { "" } else { "none" }
            on:click=move |_| do_cancel()
            on:keydown=move |event| {
                if event.key() == "Escape" {
                    event.prevent_default();
                    do_cancel();
                }
            }
        >
            <div
                node_ref=panel_ref
                data-modal-panel
                tabindex="-1"
                class=if wide { format!("{} {}", style::panel, style::panel_wide) } else { style::panel.to_owned() }
                on:click=|e| e.stop_propagation()
            >
                <h2 class=style::title>{move || title.get()}</h2>
                <div class=style::body>{children()}</div>
                <div class=style::footer>
                    <Button variant=ButtonVariant::Text
                            on_click=Some(Callback::new(move |()| do_cancel()))>
                        {cancel_label.unwrap_or_else(|| tr(TextKey::Cancel))}
                    </Button>
                    <Button variant=confirm_variant
                            disabled=confirm_disabled
                            on_click=Some(Callback::new(move |()| do_confirm()))>
                        {confirm_label.unwrap_or_else(|| tr(TextKey::Confirm))}
                    </Button>
                </div>
            </div>
        </div>
    }
}

/// A simple delete-confirmation dialog.
///
/// The dialog is always rendered in the DOM; `open` controls its visibility.
/// `entity_name` is a reactive `Signal<String>` so the name updates when the
/// delete target changes without altering the DOM shape.
#[component]
pub fn DeleteDialog(
    open: RwSignal<bool>,
    entity_name: Signal<String>,
    on_confirm: Callback<bool>,
) -> impl IntoView {
    view! {
        <Modal
            open=open
            title=tr(TextKey::ConfirmDelete).to_string()
            on_close=on_confirm
            confirm_label=Some(tr(TextKey::Delete))
            destructive=true
        >
            <p>{tr(TextKey::AreYouSureDelete)}" "<strong>{move || entity_name.get()}</strong>"?"</p>
            <p class=style::muted_note>{tr(TextKey::ThisActionCannotBeUndone)}</p>
        </Modal>
    }
}
