use leptos::prelude::*;

use crate::admin::i18n::{range_of, rows_per_page, tr, TextKey};
use crate::admin::server_fns::logs::{load_recent_log_entries, AdminLogEntry, AdminLogSnapshot};

#[component]
pub fn LogsPage() -> impl IntoView {
    view! { <LogsIsland/> }
}

#[island]
fn LogsIsland() -> impl IntoView {
    let search = RwSignal::new(String::new());

    let logs = Resource::new(
        move || search.get(),
        |filter| async move { load_recent_log_entries(filter).await.unwrap_or_default() },
    );

    view! {
        <div style="max-width:80rem;margin:0 auto;padding:2rem 0;display:flex;flex-direction:column;gap:1.5rem">
            <header style="display:flex;flex-direction:column;gap:0.5rem">
                <h1 style="font-size:1.5rem;font-weight:700;color:var(--color-gray-100);margin:0">
                    {tr(TextKey::Logs)}
                </h1>
                <div style="height:1px;background:var(--color-border)"></div>
            </header>

            <div style="display:grid;grid-template-columns:minmax(7rem,10rem) minmax(16rem,28rem);align-items:center;gap:1rem">
                <label for="support-log-search" style="font-size:0.875rem;color:var(--color-gray-200)">
                    {tr(TextKey::Search)}
                </label>
                <input
                    id="support-log-search"
                    type="text"
                    prop:value=move || search.get()
                    on:input=move |event| search.set(event_target_value(&event))
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
        range_of(0, 0, 0)
    } else {
        let end = snapshot.entries.len().min(snapshot.total_count);
        range_of(
            1,
            u32::try_from(end).unwrap_or(u32::MAX),
            u64::try_from(snapshot.total_count).unwrap_or(u64::MAX),
        )
    };

    view! {
        <section style="background:var(--color-surface);overflow:hidden">
            <div style="overflow-x:auto">
                <table style="width:100%;min-width:52rem;border-collapse:collapse;font-size:0.875rem;text-align:left;color:var(--color-gray-100)">
                    <thead style="background:var(--color-appbar)">
                        <tr>
                            <th scope="col" style="height:3.5rem;padding:0 1rem;font-weight:700">
                                <button type="button" aria-label=tr(TextKey::SortByTimestamp) style="color:inherit;font:inherit;font-weight:700">
                                    {tr(TextKey::Timestamp)}
                                </button>
                            </th>
                            <th scope="col" style="height:3.5rem;padding:0 1rem;font-weight:700">
                                <button type="button" aria-label=tr(TextKey::SortByLevel) style="color:inherit;font:inherit;font-weight:700">
                                    {tr(TextKey::Level)}
                                </button>
                            </th>
                            <th scope="col" style="height:3.5rem;padding:0 1rem;font-weight:700">
                                {tr(TextKey::Message)}
                            </th>
                        </tr>
                    </thead>
                    <tbody>
                        {if snapshot.entries.is_empty() {
                            view! {
                                <tr>
                                    <td colspan="3" style="padding:2rem 1rem;text-align:center;color:var(--color-muted);border-top:1px solid var(--color-table-line)">
                                        {tr(TextKey::NoMatchingRecordsFound)}
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
                <span>{rows_per_page(u32::try_from(snapshot.page_size.max(10)).unwrap_or(10))}</span>
                <span>{range}</span>
                <div style="display:inline-flex;align-items:center;gap:0.5rem">
                    <button type="button" aria-label=tr(TextKey::FirstPage) disabled=true style="min-width:2rem;height:2rem;border-radius:999px;color:var(--color-action-default);opacity:0.4">
                        "«"
                    </button>
                    <button type="button" aria-label=tr(TextKey::PreviousPage) disabled=true style="min-width:2rem;height:2rem;border-radius:999px;color:var(--color-action-default);opacity:0.4">
                        "‹"
                    </button>
                    <button type="button" aria-label=tr(TextKey::NextPage) disabled=true style="min-width:2rem;height:2rem;border-radius:999px;color:var(--color-action-default);opacity:0.4">
                        "›"
                    </button>
                    <button type="button" aria-label=tr(TextKey::LastPage) disabled=true style="min-width:2rem;height:2rem;border-radius:999px;color:var(--color-action-default);opacity:0.4">
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
