use leptos::prelude::*;

use crate::admin::i18n::{tr, TextKey};

stylance::import_crate_style!(style, "src/admin/ui/primitives/resource_state.module.css");

/// Presentation state for one server-backed admin resource.
///
/// The failure variant deliberately does not introduce another data-access
/// error vocabulary. Loaders keep returning `RepositoryError` (mapped through
/// the server-function transport); this enum only controls stable markup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdminLoadState {
    Loading,
    Ready,
    Empty,
    Failed,
}

impl AdminLoadState {
    const fn attribute(self) -> &'static str {
        match self {
            Self::Loading => "loading",
            Self::Ready => "ready",
            Self::Empty => "empty",
            Self::Failed => "error",
        }
    }
}

/// Stable wrapper for loading, ready, empty, and failed admin resources.
///
/// Keeping the same element and selector in every state avoids placing a
/// state-dependent wrapper across a `Suspense` boundary and gives browser
/// tests one durable assertion surface.
#[component]
pub fn AdminResourceState(state: Signal<AdminLoadState>, children: Children) -> impl IntoView {
    view! {
        <section
            class=style::resource_state
            data-admin-resource
            data-state=move || state.get().attribute()
            aria-busy=move || (state.get() == AdminLoadState::Loading).to_string()
        >
            {children()}
        </section>
    }
}

#[component]
pub fn AdminLoadingState() -> impl IntoView {
    view! { <p class=style::message role="status">{tr(TextKey::Loading)}</p> }
}

#[component]
pub fn AdminErrorState(error: String) -> impl IntoView {
    view! {
        <div class=style::error role="alert">
            <strong>{tr(TextKey::ErrorOccurred)}</strong>
            <span>{error}</span>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_wrapper_exposes_stable_state_selector() {
        let owner = Owner::new();
        for (state, expected) in [
            (AdminLoadState::Loading, "loading"),
            (AdminLoadState::Ready, "ready"),
            (AdminLoadState::Empty, "empty"),
            (AdminLoadState::Failed, "error"),
        ] {
            let html = owner.with(|| {
                view! {
                    <AdminResourceState state=Signal::derive(move || state)>
                        <span>"content"</span>
                    </AdminResourceState>
                }
                .into_view()
                .to_html()
            });

            assert!(html.contains("data-admin-resource"));
            assert!(html.contains(&format!(r#"data-state="{expected}""#)));
        }
    }
}
