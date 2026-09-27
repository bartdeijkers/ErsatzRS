use leptos::prelude::*;

use crate::admin::i18n::{tr, TextKey};

stylance::import_crate_style!(style, "src/admin/release_notes.module.css");

const GITHUB_CHANGELOG_URL: &str =
    "https://github.com/bartdeijkers/ErsatzRS/blob/main/CHANGELOG.md";
#[cfg(not(target_arch = "wasm32"))]
const CHANGELOG: &str = include_str!("../../../../CHANGELOG.md");

#[component]
pub fn ReleaseNotesCard() -> impl IntoView {
    let notes = Resource::new_blocking(|| (), |()| load_release_notes());

    view! {
        <section class=style::card data-release-notes>
            <header class=style::header>
                <h2>{tr(TextKey::ReleaseNotes)}</h2>
                <p>
                    <a href="/changelog.md">{tr(TextKey::FullChangelog)}</a>
                    " · "
                    <a href=GITHUB_CHANGELOG_URL target="_blank" rel="noopener noreferrer">"GitHub"</a>
                </p>
            </header>
            <Suspense fallback=|| ()>
                {move || notes.get().map(|result| {
                    let html = result.unwrap_or_default();
                    view! { <div class=style::body inner_html=html></div> }
                })}
            </Suspense>
        </section>
    }
}

#[server(prefix = "/admin-api")]
#[allow(clippy::unused_async)] // Leptos server functions require an async transport signature.
async fn load_release_notes() -> Result<String, ServerFnError> {
    use std::sync::OnceLock;

    static NOTES: OnceLock<String> = OnceLock::new();
    let notes = NOTES.get_or_init(|| {
        let section = option_env!("GITHUB_REF_NAME")
            .and_then(|reference| reference.strip_prefix('v'))
            .map_or_else(
                || latest_release_section(CHANGELOG),
                |version| release_section(CHANGELOG, version),
            );
        section.map_or_else(String::new, render_markdown)
    });
    Ok(notes.clone())
}

#[cfg(feature = "ssr")]
pub async fn full_changelog() -> impl axum::response::IntoResponse {
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "text/markdown; charset=utf-8",
        )],
        CHANGELOG,
    )
}

#[cfg(not(target_arch = "wasm32"))]
fn release_section<'a>(changelog: &'a str, version: &str) -> Option<&'a str> {
    let heading = format!("## [{version}]");
    let mut start = None;
    let mut offset = 0;
    for line in changelog.split_inclusive('\n') {
        if line.starts_with("## [") {
            if let Some(start) = start {
                return Some(&changelog[start..offset]);
            }
            if line
                .strip_prefix(&heading)
                .is_some_and(|tail| tail.starts_with(' ') || tail.starts_with('\n'))
            {
                start = Some(offset);
            }
        }
        offset += line.len();
    }
    start.map(|start| &changelog[start..])
}

#[cfg(not(target_arch = "wasm32"))]
fn latest_release_section(changelog: &str) -> Option<&str> {
    let version = changelog.lines().find_map(|line| {
        line.strip_prefix("## [")
            .and_then(|remainder| remainder.split_once(']'))
            .map(|(version, _)| version)
            .filter(|version| *version != "Unreleased")
    })?;
    release_section(changelog, version)
}

#[cfg(not(target_arch = "wasm32"))]
fn render_markdown(markdown: &str) -> String {
    let options = pulldown_cmark::Options::all();
    let parser = pulldown_cmark::Parser::new_ext(markdown, options);
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, parser);
    ammonia::clean(&html)
}

#[cfg(test)]
mod tests {
    use super::{latest_release_section, release_section, render_markdown};

    #[test]
    fn release_markdown_is_rendered_and_sanitized() {
        let html =
            render_markdown("**Ready** <script>alert(1)</script> [bad](javascript:alert(1))");
        assert!(html.contains("<strong>Ready</strong>"));
        assert!(!html.contains("<script>"));
        assert!(!html.contains("href=\"javascript:"));
    }

    #[test]
    fn release_section_selects_the_exact_installed_beta() {
        let changelog = "## [Unreleased]\n\nCurrent\n\n## [1.0.37-beta] - 2026-09-22\n\nInstalled\n\n## [1.0.35-beta] - 2026-09-16\n\nOld\n";
        let section = release_section(changelog, "1.0.37-beta").unwrap();
        assert!(section.contains("Installed"));
        assert!(!section.contains("Current"));
        assert!(!section.contains("Old"));
        assert!(release_section(changelog, "1.0.37").is_none());
        assert_eq!(latest_release_section(changelog), Some(section));
    }
}
