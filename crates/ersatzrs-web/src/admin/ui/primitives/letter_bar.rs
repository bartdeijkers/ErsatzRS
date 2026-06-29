#![allow(dead_code)]
use std::collections::HashSet;
use std::fmt::Write as _;

use leptos::prelude::*;

use crate::admin::server_fns::media_browser::{bucket_index, LetterPageMap};

stylance::import_crate_style!(style, "src/admin/ui/primitives/letter_bar.module.css");

/// Cross-page jump target for the [`LetterBar`]. When provided, each letter
/// links to `{base_uri}?{query}&page=N#letter-X` where N is read from the
/// [`LetterPageMap`]. Mirrors `ErsatzTV`'s `LetterBar.razor` behavior of
/// jumping to the page that contains the first item starting with the
/// letter.
#[derive(Clone, Debug)]
pub struct LetterJump {
    /// Path component without trailing slash, e.g. `"/media/movies"`.
    pub base_uri: String,
    /// Optional `query` parameter to preserve through the jump.
    pub query: String,
    /// 27-element vector of 1-based page numbers; index 0 is '#', 1..=26 is
    /// 'a'..='z'.
    pub page_map: LetterPageMap,
}

/// Sticky alphabetic quick-nav. Each letter links to either:
///
/// * a `#letter-X` anchor on the current page (default — no `jump`), or
/// * a cross-page link of the form
///   `{base_uri}?query=…&page=N#letter-X` when `jump` is supplied. This
///   ports the `ErsatzTV` `LetterBar.razor` behavior driven by
///   `SearchPageMap.PageMap` (see `LuceneSearchIndex.GetSearchPageMap`).
///
/// In both cases the in-page anchors are emitted by [`FragmentLetterAnchor`]
/// at the first card of each bucket so the browser scrolls to the matching
/// row after navigation.
#[component]
pub fn LetterBar(
    /// Letters (lowercase, plus '#' for digits/symbols) that actually occur in
    /// the current page. Missing letters are rendered disabled. Ignored when
    /// `jump` is supplied — page-map-driven letters are always clickable.
    #[prop(into)]
    present: Signal<Option<HashSet<char>>>,
    /// Optional cross-page jump target. When set, the bar is rendered on the
    /// right edge of the viewport (vertical orientation) and clicking a
    /// letter navigates to the page that contains its first item.
    #[prop(optional)]
    jump: Option<LetterJump>,
) -> impl IntoView {
    let letters: Vec<char> = std::iter::once('#').chain('a'..='z').collect();
    let fallback_letters = letters.clone();
    let resolved_letters = letters;

    // Choose CSS variants based on layout.
    let bar_class = if jump.is_some() {
        format!("{} {}", style::bar, style::bar_side)
    } else {
        style::bar.to_string()
    };
    let bar_class_for_fallback = bar_class.clone();

    let fallback_jump = jump.clone();

    view! {
        <Suspense fallback=move || {
            let fb_jump = fallback_jump.clone();
            view! {
                <div class=bar_class_for_fallback.clone()>
                    {fallback_letters
                        .iter()
                        .map(|letter| {
                            let label = letter_label(*letter);
                            // During suspense fallback we still emit a real
                            // link if we have a page-map, so it's clickable
                            // before hydration completes (matches the SSR
                            // behavior of ErsatzTV's `MudLink`).
                            let href = fb_jump.as_ref().map_or_else(
                                || "#".to_string(),
                                |j| build_jump_href(j, *letter),
                            );
                            let class_name = if fb_jump.is_some() {
                                style::letter.to_string()
                            } else {
                                format!("{} {}", style::letter, style::letter_disabled)
                            };
                            view! { <a href=href class=class_name>{label}</a> }
                        })
                        .collect_view()}
                </div>
            }
        }>
            {move || {
                let resolved_jump = jump.clone();
                present.get().map(|present_letters| {
                    view! {
                        <div class=bar_class.clone()>
                            {resolved_letters
                                .iter()
                                .map(|letter| {
                                    let label = letter_label(*letter);
                                    // Cross-page jump (`Some`) is always
                                    // enabled. Same-page anchor (`None`) is
                                    // enabled only when the letter occurs in
                                    // the current page slice.
                                    let (href, class_name) = if let Some(target) =
                                        &resolved_jump
                                    {
                                        (
                                            build_jump_href(target, *letter),
                                            style::letter.to_string(),
                                        )
                                    } else {
                                        let is_present = present_letters.contains(letter);
                                        let class_name = if is_present {
                                            style::letter.to_string()
                                        } else {
                                            format!(
                                                "{} {}",
                                                style::letter,
                                                style::letter_disabled,
                                            )
                                        };
                                        (format!("#letter-{letter}"), class_name)
                                    };
                                    view! {
                                        <a href=href class=class_name>{label}</a>
                                    }
                                })
                                .collect_view()}
                        </div>
                    }
                })
            }}
        </Suspense>
    }
}

fn letter_label(letter: char) -> String {
    if letter == '#' {
        "#".to_string()
    } else {
        letter.to_ascii_uppercase().to_string()
    }
}

fn build_jump_href(jump: &LetterJump, letter: char) -> String {
    let idx = bucket_index(letter);
    let page = jump.page_map.pages.get(idx).copied().unwrap_or(1).max(1);
    let mut url = String::from(&jump.base_uri);
    let mut params: Vec<String> = Vec::new();
    let trimmed = jump.query.trim();
    if !trimmed.is_empty() {
        params.push(format!("query={}", urlencoding::encode(trimmed)));
    }
    if page > 1 {
        params.push(format!("page={page}"));
    }
    if !params.is_empty() {
        url.push('?');
        url.push_str(&params.join("&"));
    }
    let _ = write!(url, "#letter-{letter}");
    url
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
