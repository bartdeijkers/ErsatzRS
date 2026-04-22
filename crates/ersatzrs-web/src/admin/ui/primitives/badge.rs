use leptos::prelude::*;

stylance::import_crate_style!(style, "src/admin/ui/primitives/badge.module.css");

/// A count overlay badge, positioned top-right of its relative-positioned parent.
/// Usage: wrap a nav icon in a relative container and place `<Badge count=signal/>` inside.
#[component]
pub fn Badge(count: Signal<u32>) -> impl IntoView {
    view! {
        <Show when=move || count.get() != 0>
            <span class=style::badge>
                {move || {
                    let n = count.get();
                    if n > 99 { "99+".to_string() } else { n.to_string() }
                }}
            </span>
        </Show>
    }
}
