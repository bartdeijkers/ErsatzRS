use leptos::prelude::*;

use crate::admin::i18n::{health_summary, tr, TextKey};
use crate::admin::server_fns::layout::{load_health_snapshot, AdminHealthRow, AdminHealthSnapshot};

#[component]
pub fn HealthPage() -> impl IntoView {
    let snapshot = Resource::new(
        || (),
        |()| async move { load_health_snapshot().await.unwrap_or_default() },
    );

    view! {
        <div style="max-width:64rem;margin:0 auto;padding:2rem 0;display:flex;flex-direction:column;gap:1.5rem">
            <h1 style="font-size:1.5rem;font-weight:700;color:var(--color-gray-100);margin:0">
                {tr(TextKey::HealthChecks)}
            </h1>
            <p style="font-size:0.875rem;color:var(--color-muted);margin:0">
                {tr(TextKey::HealthPersistedRunHelp)}
            </p>
            <Suspense fallback=move || view! { <p>{tr(TextKey::Loading)}</p> }>
                {move || snapshot.get().map(render_snapshot)}
            </Suspense>
        </div>
    }
}

fn render_snapshot(mut snapshot: AdminHealthSnapshot) -> AnyView {
    if snapshot.rows.is_empty() {
        return view! {
            <p style="font-size:0.875rem;color:var(--color-muted)">
                {tr(TextKey::HealthNoPersistedRun)}
            </p>
        }
        .into_any();
    }

    let summary = health_summary(snapshot.errors, snapshot.warnings);

    let rows = std::mem::take(&mut snapshot.rows);
    view! {
        <p style="font-size:0.875rem;color:var(--color-gray-300);margin:0">{summary}</p>
        <table style="width:100%;border-collapse:collapse;font-size:0.875rem">
            <thead>
                <tr style="border-bottom:1px solid var(--color-border);color:var(--color-gray-300);text-align:left">
                    <th style="padding:0.5rem 0.75rem;font-weight:600">{tr(TextKey::Check)}</th>
                    <th style="padding:0.5rem 0.75rem;font-weight:600">{tr(TextKey::Message)}</th>
                </tr>
            </thead>
            <tbody>
                {rows.into_iter().map(render_row).collect_view()}
            </tbody>
        </table>
    }
    .into_any()
}

fn render_row(row: AdminHealthRow) -> AnyView {
    let color = status_color(row.status.as_str());
    let status = status_label(row.status.as_str());
    let message = if row.message.is_empty() {
        row.brief_message.clone()
    } else {
        row.message.clone()
    };
    let link = row.link.clone();

    view! {
        <tr style="border-bottom:1px solid var(--color-border-subtle)">
            <td style="padding:0.5rem 0.75rem;vertical-align:top;color:var(--color-gray-100)">
                <div style="align-items:center;display:flex;gap:0.5rem">
                    <span
                        aria-label=status
                        title=status
                        style=format!("background:{color};border-radius:999px;display:inline-block;flex:0 0 auto;height:0.65rem;width:0.65rem")
                    ></span>
                    <span>{row.title}</span>
                </div>
            </td>
            <td style="padding:0.5rem 0.75rem;vertical-align:top;color:var(--color-gray-300)">
                {match link {
                    Some(href) => view! {
                        <a href=href.clone() target=link_target(&href) rel="noopener" style="color:var(--color-primary)">
                            {message}
                        </a>
                    }.into_any(),
                    None => view! { <span>{message}</span> }.into_any(),
                }}
            </td>
        </tr>
    }
    .into_any()
}

fn status_label(status: &str) -> &'static str {
    match status {
        "Fail" => tr(TextKey::Fail),
        "Warning" => tr(TextKey::Warning),
        "Info" => tr(TextKey::Info),
        "Pass" => tr(TextKey::Pass),
        _ => tr(TextKey::Unavailable),
    }
}

fn status_color(status: &str) -> &'static str {
    match status {
        "Fail" => "var(--color-error, #f44336)",
        "Warning" => "var(--color-warning, #ff9800)",
        "Info" => "var(--color-primary, #00c0c0)",
        "Pass" => "var(--color-success, #4caf50)",
        _ => "var(--color-muted)",
    }
}

fn link_target(href: &str) -> &'static str {
    if href.starts_with("http://") || href.starts_with("https://") {
        "_blank"
    } else {
        "_self"
    }
}
