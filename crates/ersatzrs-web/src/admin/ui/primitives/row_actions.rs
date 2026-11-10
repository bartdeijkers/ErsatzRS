//! Shared row actions for admin tables.
//!
//! Every action is a round icon button with a translated tooltip and
//! accessible name, in a fixed slot. Rows of the same table render the same
//! slots in the same order, so one action always lines up under itself; an
//! action that does not apply to a row keeps its slot (hidden) instead of
//! shifting its neighbours. Delete goes last.
//!
//! When the table switches to cards on narrow containers, the buttons grow
//! to 44 px touch targets.

use leptos::prelude::*;

use super::icon::{icon_style, IconName};

stylance::import_crate_style!(style, "src/admin/ui/primitives/row_actions.module.css");

/// The icon of a row action. All icons share one size; watching is green and
/// deleting is a red trash can.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowIcon {
    Watch,
    Stop,
    Edit,
    Troubleshoot,
    Copy,
    Delete,
}

impl RowIcon {
    const fn name(self) -> IconName {
        match self {
            Self::Watch => IconName::Play,
            Self::Stop => IconName::Stop,
            Self::Edit => IconName::Edit,
            Self::Troubleshoot => IconName::Search,
            Self::Copy => IconName::Copy,
            Self::Delete => IconName::Delete,
        }
    }

    const fn tone(self) -> Option<&'static str> {
        match self {
            Self::Watch => Some(style::watch),
            Self::Delete => Some(style::delete),
            _ => None,
        }
    }
}

fn row_icon(icon: RowIcon) -> impl IntoView {
    view! { <span style=icon_style(icon.name(), "1.25rem") aria-hidden="true"></span> }
}

/// Container for the action slots of one table row.
#[component]
pub fn RowActions(children: Children) -> impl IntoView {
    view! { <div class=style::slots data-row-actions="">{children()}</div> }
}

fn action_class(icon: RowIcon) -> String {
    match icon.tone() {
        Some(tone) => format!("{} {tone}", style::action),
        None => style::action.to_owned(),
    }
}

/// A row action that runs client-side code.
#[component]
pub fn RowActionButton(
    icon: RowIcon,
    /// Tooltip and accessible name.
    label: &'static str,
    on_click: Callback<()>,
    /// Whether the action applies to this row. A hidden action keeps its slot.
    #[prop(optional, into)]
    visible: MaybeProp<bool>,
    #[prop(optional)] disabled: bool,
    /// Tooltip shown instead of `label`, for example why it is disabled.
    #[prop(optional)]
    title: Option<&'static str>,
) -> impl IntoView {
    let shown = move || visible.get().unwrap_or(true);
    view! {
        <button
            type="button"
            class=action_class(icon)
            title=title.unwrap_or(label)
            aria-label=label
            disabled=disabled
            style:visibility=move || if shown() { "visible" } else { "hidden" }
            aria-hidden=move || if shown() { "false" } else { "true" }
            tabindex=move || if shown() { "0" } else { "-1" }
            on:click=move |_| on_click.run(())
        >
            {row_icon(icon)}
        </button>
    }
}

/// A row action that navigates.
#[component]
pub fn RowActionLink(
    icon: RowIcon,
    #[prop(into)] href: String,
    /// Tooltip and accessible name.
    label: &'static str,
    /// The action does not apply to this row; it keeps its slot.
    #[prop(optional)]
    hidden: bool,
) -> impl IntoView {
    view! {
        <a
            href=href
            class=action_class(icon)
            title=label
            aria-label=label
            style:visibility=if hidden { "hidden" } else { "visible" }
            aria-hidden=if hidden { "true" } else { "false" }
            tabindex=if hidden { "-1" } else { "0" }
        >
            {row_icon(icon)}
        </a>
    }
}
