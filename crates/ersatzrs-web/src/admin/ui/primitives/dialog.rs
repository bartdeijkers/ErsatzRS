use leptos::prelude::*;

use super::button::{Button, ButtonVariant};

stylance::import_crate_style!(style, "src/admin/ui/primitives/dialog.module.css");

/// A modal dialog wrapping a Leptos `<Show>` + backdrop pattern.
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
        <Show when=move || open.get()>
            <div class=style::backdrop on:click=move |_| do_cancel()/>
            <div class=style::overlay>
                <div class=style::panel on:click=|e| e.stop_propagation()>
                    <h2 class=style::title>{title.clone()}</h2>
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
        </Show>
    }
}

/// A simple delete-confirmation dialog.
#[component]
pub fn DeleteDialog(
    open: RwSignal<bool>,
    entity_name: String,
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
            <p>"Are you sure you want to delete "<strong>{entity_name.clone()}</strong>"?"</p>
            <p class=style::muted_note>"This action cannot be undone."</p>
        </Modal>
    }
}
