//! Shared facet rail for media browsers that filter by a metadata facet.
//!
//! ErsatzTV has no upstream equivalent — its `MudBlazor` browsers only reach
//! facets through the search box — so ErsatzRS owns the presentation. The
//! first implementation (remote streams) rendered an unbounded wrapped chip
//! cloud above the action bar, which pushed the card grid off-screen on
//! tag-heavy libraries. This primitive fixes the footprint at one row and
//! moves the "show everything" affordance into a CSS-only disclosure, so the
//! rail costs no hydration and keeps working inside the `<noscript>` fallback.

use leptos::prelude::*;

use crate::admin::ui::primitives::icon::{icon_style, IconName};

stylance::import_crate_style!(style, "src/admin/ui/primitives/facet_bar.module.css");

/// One facet value and the browser query it links to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FacetLink {
    /// Display text, also matched against the active facet value.
    pub label: String,
    /// Target that applies this facet to the browser.
    pub href: String,
}

/// Renders a facet group as a single-row chip rail that expands in place.
///
/// Renders nothing when `items` is empty so pages can mount it
/// unconditionally.
// `active` is only read, but a Leptos prop has to be owned: the component
// signature is the public API callers build through the generated builder.
#[allow(clippy::needless_pass_by_value)]
#[component]
pub fn FacetBar(
    /// Localized group label, e.g. `tr(TextKey::Tags)`.
    label: &'static str,
    /// Facet values in display order.
    items: Vec<FacetLink>,
    /// Target that clears the facet filter.
    #[prop(into)]
    reset_href: String,
    /// Localized label for the "no filter" chip, e.g. `tr(TextKey::All)`.
    reset_label: &'static str,
    /// Facet value currently applied, highlighted in the rail.
    #[prop(default = None)]
    active: Option<String>,
    /// Facet slug; disambiguates the disclosure id when a page shows several
    /// rails, and gives browser tests a stable `data-facet-bar` hook.
    #[prop(default = "facet")]
    id: &'static str,
) -> AnyView {
    if items.is_empty() {
        return ().into_any();
    }

    let count = items.len();
    let toggle_id = format!("facet-bar-{id}-toggle");
    let chip_class = |active: bool| {
        if active {
            format!("{} {}", style::chip, style::chip_active)
        } else {
            style::chip.to_owned()
        }
    };
    let reset_active = active.is_none();

    view! {
        <section class=style::bar data-facet-bar=id>
            <input id=toggle_id.clone() class=style::toggle type="checkbox" aria-label=label/>
            <p class=style::label>
                <span style=icon_style(IconName::FilterList, "0.875rem") aria-hidden="true"></span>
                <span class=style::label_text>{label}</span>
                <span class=style::count>{count}</span>
            </p>
            <div class=style::rail>
                <a
                    class=chip_class(reset_active)
                    href=reset_href
                    aria-current=reset_active.then_some("page")
                >
                    {reset_label}
                </a>
                {items.into_iter().map(|item| {
                    let is_active = active.as_deref() == Some(item.label.as_str());
                    let title = item.label.clone();
                    view! {
                        <a
                            class=chip_class(is_active)
                            href=item.href
                            title=title
                            aria-current=is_active.then_some("page")
                        >
                            {item.label}
                        </a>
                    }
                }).collect_view()}
            </div>
            <label class=style::more for=toggle_id title=label>
                <span
                    class=style::chevron
                    style=icon_style(IconName::ChevronDown, "1rem")
                    aria-hidden="true"
                ></span>
            </label>
        </section>
    }
    .into_any()
}
