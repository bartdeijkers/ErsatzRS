use leptos::{html, prelude::*};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;

use crate::admin::i18n::{tr, TextKey};

use super::button::{Button, ButtonVariant};

stylance::import_crate_style!(style, "src/admin/ui/primitives/dialog.module.css");

#[cfg(target_arch = "wasm32")]
fn focusable_elements(panel: &web_sys::HtmlElement) -> Vec<web_sys::HtmlElement> {
    const SELECTOR: &str = "a[href]:not([tabindex='-1']), button:not([disabled]):not([tabindex='-1']), input:not([disabled]):not([type='hidden']):not([tabindex='-1']), select:not([disabled]):not([tabindex='-1']), textarea:not([disabled]):not([tabindex='-1']), [tabindex]:not([tabindex='-1'])";

    let Ok(nodes) = panel.query_selector_all(SELECTOR) else {
        return Vec::new();
    };
    (0..nodes.length())
        .filter_map(|index| nodes.item(index))
        .filter_map(|node| node.dyn_into::<web_sys::HtmlElement>().ok())
        .filter(|element| element.offset_width() > 0 || element.offset_height() > 0)
        .collect()
}

#[cfg(target_arch = "wasm32")]
fn same_element(left: &web_sys::Element, right: &web_sys::HtmlElement) -> bool {
    js_sys::Object::is(left.as_ref(), right.as_ref())
}

#[cfg(target_arch = "wasm32")]
fn contain_tab_focus(event: &web_sys::KeyboardEvent, panel_ref: NodeRef<html::Div>) {
    if event.key() != "Tab" {
        return;
    }
    let Some(panel) = panel_ref.get() else {
        return;
    };
    let focusable = focusable_elements(&panel);
    let Some(first) = focusable.first() else {
        event.prevent_default();
        let _ = panel.focus();
        return;
    };
    let Some(last) = focusable.last() else {
        return;
    };
    let active = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.active_element());

    if event.shift_key()
        && active
            .as_ref()
            .is_none_or(|element| same_element(element, first) || same_element(element, &panel))
    {
        event.prevent_default();
        let _ = last.focus();
    } else if !event.shift_key()
        && active
            .as_ref()
            .is_some_and(|element| same_element(element, last))
    {
        event.prevent_default();
        let _ = first.focus();
    }
}

/// A modal dialog with a stable DOM shape (no `<Show>` — uses `style:display`).
///
/// The backdrop and panel are always present in the DOM; `open` controls
/// visibility.  This avoids `failed_to_cast_marker_node` hydration mismatches
/// that arise when the dialog is conditionally inserted into an `#[island]`.
#[allow(clippy::needless_pass_by_value)]
#[component]
pub fn Modal(
    /// Page-scoped stable dialog id, also used to associate the title.
    dialog_id: &'static str,
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
    let title_id = format!("{dialog_id}-title");
    let labelled_by = title_id.clone();
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
            id=dialog_id
            class=style::overlay
            role="dialog"
            aria-modal="true"
            aria-labelledby=labelled_by
            style:display=move || if open.get() { "" } else { "none" }
            on:click=move |_| do_cancel()
            on:keydown=move |event| {
                if event.key() == "Escape" {
                    event.prevent_default();
                    do_cancel();
                }
                #[cfg(target_arch = "wasm32")]
                contain_tab_focus(&event, panel_ref);
            }
        >
            <div
                node_ref=panel_ref
                data-modal-panel
                tabindex="-1"
                class=if wide { format!("{} {}", style::panel, style::panel_wide) } else { style::panel.to_owned() }
                on:click=|e| e.stop_propagation()
            >
                <h2 id=title_id class=style::title>{move || title.get()}</h2>
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
    dialog_id: &'static str,
    open: RwSignal<bool>,
    entity_name: Signal<String>,
    on_confirm: Callback<bool>,
) -> impl IntoView {
    view! {
        <Modal
            dialog_id=dialog_id
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
