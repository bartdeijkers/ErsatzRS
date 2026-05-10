use leptos::prelude::*;

use crate::admin::server_fns::logs::{load_recent_log_entries, AdminLogEntry, AdminLogSnapshot};

#[component]
pub fn LogsPage() -> impl IntoView {
    let logs = Resource::new(
        || (),
        |()| async move { load_recent_log_entries().await.unwrap_or_default() },
    );

    view! {
        <div style="max-width:80rem;margin:0 auto;padding:2rem 0;display:flex;flex-direction:column;gap:1.5rem">
            <header style="display:flex;flex-direction:column;gap:0.5rem">
                <h1 style="font-size:1.5rem;font-weight:700;color:var(--color-gray-100);margin:0">
                    "Logs"
                </h1>
                <div style="height:1px;background:var(--color-border)"></div>
            </header>

            <div style="display:grid;grid-template-columns:minmax(7rem,10rem) minmax(16rem,28rem);align-items:center;gap:1rem">
                <label for="support-log-search" style="font-size:0.875rem;color:var(--color-gray-200)">
                    "Search"
                </label>
                <input
                    id="support-log-search"
                    type="text"
                    style="width:100%;border:1px solid var(--color-border);border-radius:var(--radius);background:var(--color-surface);color:var(--color-gray-100);font:inherit;font-size:0.875rem;padding:0.5rem 0.75rem"
                />
            </div>

            <Suspense fallback=move || view! { {render_snapshot(AdminLogSnapshot::default())} }>
                {move || logs.get().map(render_snapshot)}
            </Suspense>
        </div>
    }
}

fn render_snapshot(snapshot: AdminLogSnapshot) -> impl IntoView {
    let range = if snapshot.total_count == 0 {
        "0-0 of 0".to_owned()
    } else {
        let end = snapshot.entries.len().min(snapshot.total_count);
        format!("1-{end} of {}", snapshot.total_count)
    };

    view! {
        <section style="background:var(--color-surface);overflow:hidden">
            <div style="overflow-x:auto">
                <table style="width:100%;min-width:52rem;border-collapse:collapse;font-size:0.875rem;text-align:left;color:var(--color-gray-100)">
                    <thead style="background:var(--color-appbar)">
                        <tr>
                            <th scope="col" style="height:3.5rem;padding:0 1rem;font-weight:700">
                                <button type="button" aria-label="Sort by Timestamp" style="color:inherit;font:inherit;font-weight:700">
                                    "Timestamp"
                                </button>
                            </th>
                            <th scope="col" style="height:3.5rem;padding:0 1rem;font-weight:700">
                                <button type="button" aria-label="Sort by Level" style="color:inherit;font:inherit;font-weight:700">
                                    "Level"
                                </button>
                            </th>
                            <th scope="col" style="height:3.5rem;padding:0 1rem;font-weight:700">
                                "Message"
                            </th>
                        </tr>
                    </thead>
                    <tbody>
                        {if snapshot.entries.is_empty() {
                            view! {
                                <tr>
                                    <td colspan="3" style="padding:2rem 1rem;text-align:center;color:var(--color-muted);border-top:1px solid var(--color-table-line)">
                                        "No matching records found"
                                    </td>
                                </tr>
                            }
                            .into_any()
                        } else {
                            snapshot
                                .entries
                                .into_iter()
                                .map(render_row)
                                .collect_view()
                                .into_any()
                        }}
                    </tbody>
                </table>
            </div>

            <div style="min-height:3.25rem;padding:0 1rem;border-top:1px solid var(--color-table-line);display:flex;justify-content:flex-end;align-items:center;gap:2rem;font-size:0.875rem;color:var(--color-gray-200)">
                <span>{format!("Rows per page: {}", snapshot.page_size.max(10))}</span>
                <span>{range}</span>
                <div style="display:inline-flex;align-items:center;gap:0.5rem">
                    <button type="button" aria-label="First page" disabled=true style="min-width:2rem;height:2rem;border-radius:999px;color:var(--color-action-default);opacity:0.4">
                        "«"
                    </button>
                    <button type="button" aria-label="Previous page" disabled=true style="min-width:2rem;height:2rem;border-radius:999px;color:var(--color-action-default);opacity:0.4">
                        "‹"
                    </button>
                    <button type="button" aria-label="Next page" disabled=true style="min-width:2rem;height:2rem;border-radius:999px;color:var(--color-action-default);opacity:0.4">
                        "›"
                    </button>
                    <button type="button" aria-label="Last page" disabled=true style="min-width:2rem;height:2rem;border-radius:999px;color:var(--color-action-default);opacity:0.4">
                        "»"
                    </button>
                </div>
            </div>
        </section>
    }
}

fn render_row(entry: AdminLogEntry) -> impl IntoView {
    view! {
        <tr>
            <td style="padding:0.625rem 1rem;border-top:1px solid var(--color-table-line);white-space:nowrap">
                {entry.timestamp}
            </td>
            <td style="padding:0.625rem 1rem;border-top:1px solid var(--color-table-line);white-space:nowrap">
                {entry.level}
            </td>
            <td style="padding:0.625rem 1rem;border-top:1px solid var(--color-table-line)">
                {entry.message}
            </td>
        </tr>
    }
}
