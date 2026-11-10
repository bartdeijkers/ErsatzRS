//! Shared empty state for lists and overviews (UX guide §7).

use leptos::prelude::*;

use super::icon::{icon_style, IconName};

stylance::import_crate_style!(style, "src/admin/ui/primitives/empty_state.module.css");

/// Title, one-sentence explanation and an optional primary next step.
#[component]
pub fn EmptyState(
    title: &'static str,
    #[prop(optional)] help: Option<&'static str>,
    /// Primary next step as `(href, label)`, for example the add page.
    #[prop(optional)]
    action: Option<(String, &'static str)>,
) -> impl IntoView {
    view! {
        <div class=style::empty data-empty-state>
            <p class=style::title>{title}</p>
            {help.map(|help| view! { <p class=style::help>{help}</p> })}
            {action.map(|(href, label)| view! {
                <a class=style::action href=href>
                    <span style=icon_style(IconName::Add, "1.125rem") aria-hidden="true"></span>
                    <span>{label}</span>
                </a>
            })}
        </div>
    }
}
