//! Contextual tips and supporting explanations with native no-JavaScript toggling.

use leptos::prelude::*;

use crate::admin::i18n::{tr, TextKey};
use crate::admin::ui::primitives::icon::{icon_style, IconName};

stylance::import_crate_style!(style, "src/admin/ui/primitives/help_disclosure.module.css");

/// A full-width, keyboard-accessible disclosure for optional explanatory content.
#[component]
pub fn HelpDisclosure(title: &'static str, children: Children) -> impl IntoView {
    view! {
        <details class=style::help data-help-disclosure>
            <summary class=style::summary>
                <span class=style::chevron aria-hidden="true">
                    <span style=icon_style(IconName::ChevronRight, "1.125rem")></span>
                </span>
                <span class=style::title>{title}</span>
                <span class=style::action>
                    <span class=style::show>{tr(TextKey::ShowExplanation)}</span>
                    <span class=style::hide>{tr(TextKey::HideExplanation)}</span>
                </span>
            </summary>
            <div class=style::body>{children()}</div>
        </details>
    }
}
