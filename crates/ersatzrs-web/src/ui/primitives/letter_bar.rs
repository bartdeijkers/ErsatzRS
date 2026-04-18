#![allow(dead_code)]
use leptos::prelude::*;
use std::collections::HashSet;

stylance::import_crate_style!(style, "src/ui/primitives/letter_bar.module.css");

/// Sticky alphabetic quick-nav. Each letter links to `#letter-X` anchors
/// rendered by [`FragmentLetterAnchor`]. Maps to ErsatzTV's `LetterBar.razor`.
#[component]
pub fn LetterBar(
    /// Letters (lowercase, plus '#' for digits/symbols) that actually occur in
    /// the current page. Missing letters are rendered disabled.
    #[prop(into)]
    present: Signal<HashSet<char>>,
) -> impl IntoView {
    let letters: Vec<char> = std::iter::once('#').chain('a'..='z').collect();

    view! {
        <div class=style::bar>
            {letters
                .into_iter()
                .map(|letter| {
                    let label = if letter == '#' { "#".to_string() } else { letter.to_ascii_uppercase().to_string() };
                    let anchor = format!("#letter-{letter}");
                    let cls = move || {
                        if present.get().contains(&letter) {
                            style::letter.to_string()
                        } else {
                            format!("{} {}", style::letter, style::letter_disabled)
                        }
                    };
                    view! { <a href=anchor class=cls>{label}</a> }
                })
                .collect_view()}
        </div>
    }
}

/// Returns the first-letter bucket for a sort title: `a`–`z` when the first
/// alphabetic char is latin, `#` otherwise.
#[must_use]
pub fn letter_bucket(sort_title: &str) -> char {
    sort_title
        .trim()
        .chars()
        .next()
        .map(|c| c.to_ascii_lowercase())
        .filter(char::is_ascii_alphabetic)
        .unwrap_or('#')
}

/// Wraps each card in a `<div>` and inserts `<div id="letter-X">` anchors at
/// the first card of each new bucket. Maps to `FragmentLetterAnchor.razor`.
#[component]
pub fn FragmentLetterAnchor<T, F>(
    cards: Vec<T>,
    sort_title: fn(&T) -> &str,
    render: F,
) -> impl IntoView
where
    T: Clone + Send + Sync + 'static,
    F: Fn(T) -> AnyView + Clone + Send + Sync + 'static,
{
    let mut seen: HashSet<char> = HashSet::new();

    cards
        .into_iter()
        .map(move |card| {
            let letter = letter_bucket(sort_title(&card));
            let first = seen.insert(letter);
            let body = (render.clone())(card);
            if first {
                let id = format!("letter-{letter}");
                view! { <div id=id class=style::anchor>{body}</div> }.into_any()
            } else {
                body
            }
        })
        .collect_view()
}
