use leptos::prelude::*;

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
    /// Dialog title shown in the header.
    title: String,
    /// Called when the user confirms or cancels. Receives `true` for confirm.
    on_close: Callback<bool>,
    /// Content inside the dialog body.
    children: ChildrenFn,
    #[prop(default = "Confirm")] confirm_label: &'static str,
    #[prop(default = "Cancel")] cancel_label: &'static str,
    #[prop(default = false)] destructive: bool,
) -> impl IntoView {
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
            style:display=move || if open.get() { "" } else { "none" }
            on:click=move |_| do_cancel()
        >
            <div class=style::panel on:click=|e| e.stop_propagation()>
                <h2 class=style::title>{title}</h2>
                <div class=style::body>{children()}</div>
                <div class=style::footer>
                    <Button variant=ButtonVariant::Text
                            on_click=Some(Callback::new(move |()| do_cancel()))>
                        {cancel_label}
                    </Button>
                    <Button variant=confirm_variant
                            on_click=Some(Callback::new(move |()| do_confirm()))>
                        {confirm_label}
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
            title="Confirm Delete".to_string()
            on_close=on_confirm
            confirm_label="Delete"
            destructive=true
        >
            <p>"Are you sure you want to delete "<strong>{move || entity_name.get()}</strong>"?"</p>
            <p class=style::muted_note>"This action cannot be undone."</p>
        </Modal>
    }
}
