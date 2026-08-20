use leptos::prelude::*;

use super::icon::{icon_style, IconName};

stylance::import_crate_style!(style, "src/admin/ui/primitives/button.module.css");

/// Button variant mirrors `MudBlazor` `Variant`.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonVariant {
    #[default]
    Primary,
    Secondary,
    Text,
    Danger,
}

fn btn_class(variant: ButtonVariant) -> &'static str {
    match variant {
        ButtonVariant::Primary => style::btn_primary,
        ButtonVariant::Secondary => style::btn_secondary,
        ButtonVariant::Text => style::btn_text,
        ButtonVariant::Danger => style::btn_danger,
    }
}

fn icon_btn_class(variant: ButtonVariant) -> &'static str {
    match variant {
        ButtonVariant::Primary => style::icon_btn_primary,
        ButtonVariant::Secondary => style::icon_btn_secondary,
        ButtonVariant::Text => style::icon_btn_text,
        ButtonVariant::Danger => style::icon_btn_danger,
    }
}

#[component]
pub fn Button(
    #[prop(default = ButtonVariant::Primary)] variant: ButtonVariant,
    /// Reactive so a caller can disable the button from live form state; a
    /// plain `bool` still works and stays constant.
    #[prop(into, default = Signal::from(false))]
    disabled: Signal<bool>,
    #[prop(default = None)] on_click: Option<Callback<()>>,
    children: Children,
) -> impl IntoView {
    let cls = btn_class(variant);
    view! {
        <button
            type="button"
            class=cls
            disabled=move || disabled.get()
            on:click=move |_| { if let Some(cb) = on_click { cb.run(()); } }
        >
            {children()}
        </button>
    }
}

/// A button that wraps an `<a href>` for navigation (maps to MudButton Href).
#[component]
pub fn LinkButton(
    href: String,
    #[prop(default = ButtonVariant::Primary)] variant: ButtonVariant,
    children: Children,
) -> impl IntoView {
    let cls = btn_class(variant);
    view! {
        <a href=href class=cls>
            {children()}
        </a>
    }
}

/// An icon-only button (maps to MudIconButton).
#[component]
pub fn IconButton(
    icon: IconName,
    #[prop(optional)] title: Option<&'static str>,
    #[prop(default = ButtonVariant::Text)] variant: ButtonVariant,
    #[prop(default = false)] disabled: bool,
    #[prop(default = None)] on_click: Option<Callback<()>>,
) -> impl IntoView {
    let cls = icon_btn_class(variant);
    view! {
        <button
            type="button"
            class=cls
            disabled=disabled
            title=title
            on:click=move |_| { if let Some(cb) = on_click { cb.run(()); } }
        >
            <span style=icon_style(icon, "1rem") aria-hidden="true"></span>
        </button>
    }
}
