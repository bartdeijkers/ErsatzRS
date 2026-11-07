//! Shared presentation primitives for the teletext admin pages.
//!
//! The channel teletext page and the teletext element library use the same
//! island (card), status badge, step bar and preview frame so that terms,
//! layout and states stay consistent. These helpers render plain SSR markup;
//! they add no interactivity and carry no teletext semantics of their own.

use leptos::prelude::*;

stylance::import_crate_style!(style, "src/admin/ui/primitives/teletext.module.css");

/// Visual tone of a status badge or step. A word is always shown with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextTone {
    /// Working and active (Live, On, Linked).
    Success,
    /// Needs a user action, nothing is broken (unpublished changes, draft).
    Warning,
    /// Blocks publication (error, conflict, invalid).
    Error,
    /// Neutral (Off, not used).
    Neutral,
}

impl TeletextTone {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Warning => "warning",
            Self::Error => "error",
            Self::Neutral => "neutral",
        }
    }
}

/// A dot plus a word; colour is never the only signal.
#[must_use]
pub fn status_badge(tone: TeletextTone, label: impl Into<String>) -> AnyView {
    view! {
        <span class=style::badge data-tone=tone.as_str() data-testid="teletext-status-badge">
            <span class=style::dot aria-hidden="true"></span>
            {label.into()}
        </span>
    }
    .into_any()
}

/// Content of one settings island.
pub struct TeletextIsland {
    pub test_id: &'static str,
    pub title: String,
    /// Optional badge rendered next to the title.
    pub badge: Option<AnyView>,
    /// One sentence that says what this island does.
    pub help: Option<String>,
    /// Extra attributes for automated tests and styling.
    pub data_id: Option<String>,
    pub body: AnyView,
}

/// A bordered card with a title row, one help sentence and its body.
#[must_use]
pub fn island(island: TeletextIsland) -> AnyView {
    let TeletextIsland {
        test_id,
        title,
        badge,
        help,
        data_id,
        body,
    } = island;
    view! {
        <section class=style::island data-testid=test_id data-island-id=data_id>
            <header class=style::island_header>
                <h2>{title}</h2>
                {badge}
            </header>
            {help.map(|help| view! { <p class=style::help>{help}</p> })}
            {body}
        </section>
    }
    .into_any()
}

/// State of one step in a step bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextStepState {
    Done,
    Current,
    Todo,
}

/// Ordered step bar such as Check → Preview → Publish.
#[must_use]
pub fn step_bar(label: &'static str, steps: Vec<(String, TeletextStepState)>) -> AnyView {
    let items = steps
        .into_iter()
        .enumerate()
        .map(|(index, (text, state))| {
            let (value, current) = match state {
                TeletextStepState::Done => ("done", None),
                TeletextStepState::Current => ("current", Some("step")),
                TeletextStepState::Todo => ("todo", None),
            };
            view! {
                <li data-state=value aria-current=current>
                    <span class=style::step_number aria-hidden="true">
                        {if state == TeletextStepState::Done { "✓".to_owned() } else { (index + 1).to_string() }}
                    </span>
                    {text}
                </li>
            }
        })
        .collect_view();
    view! { <ol class=style::steps aria-label=label data-testid="teletext-step-bar">{items}</ol> }
        .into_any()
}

/// One subpage link in a preview frame.
pub struct TeletextSubpageLink {
    pub href: String,
    pub label: String,
    pub current: bool,
}

/// Shared frame around a 40×25 teletext rendering. The content slot holds the
/// renderer's own output (a raster image or an HTML cell grid).
#[must_use]
pub fn preview_frame(
    caption: String,
    content: AnyView,
    subpages: Vec<TeletextSubpageLink>,
    footer: AnyView,
) -> AnyView {
    let subpages = (!subpages.is_empty()).then(|| {
        let links = subpages
            .into_iter()
            .map(|link| {
                view! {
                    <a href=link.href aria-current=link.current.then_some("page")>{link.label}</a>
                }
            })
            .collect_view();
        view! { <nav class=style::subpages aria-label=caption.clone()>{links}</nav> }
    });
    view! {
        <figure class=style::preview data-testid="teletext-preview-frame">
            <div class=style::screen>{content}</div>
            <figcaption class=style::caption>{caption.clone()}</figcaption>
            {subpages}
            {footer}
        </figure>
    }
    .into_any()
}
