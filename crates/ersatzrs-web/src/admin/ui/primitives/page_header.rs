//! Shared page heading for admin pages.
//!
//! Follows the shared UX guide: optional back link, one `h1` title style, an
//! optional subtitle, and the page's primary action on the right. Actions wrap
//! below the title on narrow screens and long labels break instead of
//! stretching the layout.

use leptos::prelude::*;

use super::icon::{icon_style, IconName};

stylance::import_crate_style!(style, "src/admin/ui/primitives/page_header.module.css");

/// Visual weight of a page-header action.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PageActionKind {
    /// The page's primary action (usually "Add …"); shows the plus icon.
    #[default]
    Add,
    /// A secondary navigation action next to the primary one.
    Secondary,
}

/// Page heading with title and optional back link, subtitle and actions.
#[component]
pub fn PageHeader(
    #[prop(into)] title: String,
    #[prop(optional, into)] subtitle: Option<String>,
    /// Back link target and label.
    #[prop(optional)]
    back: Option<(String, &'static str)>,
    #[prop(optional)] children: Option<Children>,
) -> impl IntoView {
    view! {
        <header class=style::header data-page-header>
            <div class=style::heading>
                {back.map(|(href, label)| view! {
                    <a class=style::back href=href>
                        <span style=icon_style(IconName::ChevronLeft, "1rem") aria-hidden="true"></span>
                        {label}
                    </a>
                })}
                <h1 class=style::title>{title}</h1>
                {subtitle.map(|text| view! { <p class=style::subtitle>{text}</p> })}
            </div>
            {children.map(|children| view! {
                <div class=style::actions>{children()}</div>
            })}
        </header>
    }
}

/// A navigation action in a [`PageHeader`].
#[component]
pub fn PageAction(
    #[prop(into)] href: String,
    label: &'static str,
    #[prop(default = PageActionKind::Add)] kind: PageActionKind,
    /// Stable hook rendered as `data-page-action` for tests and scripts.
    #[prop(optional)]
    marker: Option<&'static str>,
) -> impl IntoView {
    let (class, icon) = match kind {
        PageActionKind::Add => (format!("{} {}", style::action, style::primary), Some(IconName::Add)),
        PageActionKind::Secondary => (format!("{} {}", style::action, style::secondary), None),
    };
    view! {
        <a class=class href=href data-page-action=marker>
            {icon.map(|icon| view! { <span style=icon_style(icon, "1.125rem") aria-hidden="true"></span> })}
            <span>{label}</span>
        </a>
    }
}
