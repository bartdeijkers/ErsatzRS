use leptos::prelude::*;

use crate::admin::i18n::{log_level_label, range_of, tr, TextKey};
use crate::admin::server_fns::logs::{
    get_logs_page_size, load_recent_log_entries, AdminLogEntry, AdminLogSnapshot, AdminLogSortField,
};
use crate::admin::ui::primitives::resource_state::{
    admin_loading_resource_view, admin_result_view,
};
use crate::admin::ui::primitives::toast::{load_or_notify, use_toast};

#[component]
pub fn LogsPage() -> impl IntoView {
    let initial = Resource::new_blocking(
        || (),
        |()| async move {
            let page_size = get_logs_page_size().await?.max(1);
            let snapshot = load_recent_log_entries(
                String::new(),
                0,
                page_size,
                AdminLogSortField::Timestamp,
                None,
            )
            .await?;
            Ok::<_, ServerFnError>((page_size, snapshot))
        },
    );

    view! {
        <Suspense fallback=admin_loading_resource_view>
            {move || {
                initial.get().map(|result| admin_result_view(result, |(initial_page_size, initial_snapshot)| {
                    view! {
                        <LogsIsland
                            initial_page_size=initial_page_size
                            initial_snapshot=initial_snapshot
                        />
                    }
                    .into_any()
                }))
            }}
        </Suspense>
    }
}

#[island]
fn LogsIsland(initial_page_size: u32, initial_snapshot: AdminLogSnapshot) -> impl IntoView {
    let search = RwSignal::new(String::new());
    let page = RwSignal::new(0u32);
    let page_size = RwSignal::new(initial_page_size.max(1));
    let sort_field = RwSignal::new(AdminLogSortField::Timestamp);
    let sort_descending = RwSignal::new(None::<bool>);
    let current_snapshot = RwSignal::new(initial_snapshot);
    let load_toast = use_toast();

    let saved_page_size: LocalResource<u32> = LocalResource::new(move || async move {
        load_or_notify(get_logs_page_size().await, load_toast)
    });
    Effect::new(move |_| {
        if let Some(saved) = saved_page_size.get() {
            let saved = saved.max(1);
            if page_size.get_untracked() != saved {
                page_size.set(saved);
            }
        }
    });

    let logs: LocalResource<AdminLogSnapshot> = LocalResource::new(move || {
        let filter = search.get();
        let page = page.get();
        let page_size = page_size.get();
        let sort_field = sort_field.get();
        let sort_descending = sort_descending.get();
        async move {
            load_or_notify(
                load_recent_log_entries(filter, page, page_size, sort_field, sort_descending).await,
                load_toast,
            )
        }
    });

    Effect::new(move |_| {
        if let Some(snapshot) = logs.get() {
            current_snapshot.set(snapshot);
        }
    });

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
                    on:input=move |event| {
                        search.set(event_target_value(&event));
                        page.set(0);
                    }
                    style="width:100%;border:1px solid var(--color-border);border-radius:var(--radius);background:var(--color-surface);color:var(--color-gray-100);font:inherit;font-size:0.875rem;padding:0.5rem 0.75rem"
                />
            </div>

            {move || {
                render_snapshot(
                    current_snapshot.get(),
                    page,
                    page_size,
                    sort_field,
                    sort_descending,
                )
            }}
        </div>
    }
}

#[allow(clippy::too_many_lines)]
fn render_snapshot(
    snapshot: AdminLogSnapshot,
    page: RwSignal<u32>,
    page_size: RwSignal<u32>,
    sort_field: RwSignal<AdminLogSortField>,
    sort_descending: RwSignal<Option<bool>>,
) -> impl IntoView {
    let total_count = u64::try_from(snapshot.total_count).unwrap_or(u64::MAX);
    let page_size_value = u64::try_from(snapshot.page_size.max(1)).unwrap_or(1);
    let current_page = u64::from(snapshot.page);
    let last_page = total_count
        .saturating_sub(1)
        .checked_div(page_size_value)
        .unwrap_or(0);
    let range = if snapshot.total_count == 0 {
        range_of(0, 0, 0)
    } else {
        let start = current_page
            .saturating_mul(page_size_value)
            .saturating_add(1);
        let end = start
            .saturating_add(u64::try_from(snapshot.entries.len()).unwrap_or(0))
            .saturating_sub(1)
            .min(total_count);
        range_of(
            u32::try_from(start).unwrap_or(u32::MAX),
            u32::try_from(end).unwrap_or(u32::MAX),
            total_count,
        )
    };

    view! {
        <section style="background:var(--color-surface);overflow:hidden">
            <div style="overflow-x:auto">
                <table style="width:100%;min-width:52rem;border-collapse:collapse;font-size:0.875rem;text-align:left;color:var(--color-gray-100)">
                    <thead style="background:var(--color-appbar)">
                        <tr>
                            <th scope="col" style="height:3.5rem;padding:0 1rem;font-weight:700">
                                <button
                                    type="button"
                                    aria-label=tr(TextKey::SortByTimestamp)
                                    on:click=move |_| toggle_sort(AdminLogSortField::Timestamp, sort_field, sort_descending, page)
                                    style="color:inherit;font:inherit;font-weight:700;display:inline-flex;align-items:center;gap:0.25rem"
                                >
                                    {tr(TextKey::Timestamp)}
                                    <span>{move || sort_indicator(sort_field.get(), sort_descending.get(), AdminLogSortField::Timestamp)}</span>
                                </button>
                            </th>
                            <th scope="col" style="height:3.5rem;padding:0 1rem;font-weight:700">
                                <button
                                    type="button"
                                    aria-label=tr(TextKey::SortByLevel)
                                    on:click=move |_| toggle_sort(AdminLogSortField::Level, sort_field, sort_descending, page)
                                    style="color:inherit;font:inherit;font-weight:700;display:inline-flex;align-items:center;gap:0.25rem"
                                >
                                    {tr(TextKey::Level)}
                                    <span>{move || sort_indicator(sort_field.get(), sort_descending.get(), AdminLogSortField::Level)}</span>
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
                <label style="display:inline-flex;align-items:center;gap:0.5rem">
                    <span>{tr(TextKey::RowsPerPage)}</span>
                    <select
                        aria-label=tr(TextKey::RowsPerPage)
                        prop:value=move || page_size.get().to_string()
                        on:change=move |event| {
                            if let Ok(value) = event_target_value(&event).parse::<u32>() {
                                page_size.set(value.max(1));
                                page.set(0);
                            }
                        }
                        style="border:1px solid var(--color-border);border-radius:var(--radius);background:var(--color-surface);color:var(--color-gray-100);font:inherit;font-size:0.875rem;padding:0.25rem 0.5rem"
                    >
                        <option value="10">"10"</option>
                        <option value="25">"25"</option>
                        <option value="50">"50"</option>
                        <option value="100">"100"</option>
                    </select>
                </label>
                <span>{range}</span>
                <div style="display:inline-flex;align-items:center;gap:0.5rem">
                    <button
                        type="button"
                        aria-label=tr(TextKey::FirstPage)
                        disabled=move || page.get() == 0
                        on:click=move |_| page.set(0)
                        style="min-width:2rem;height:2rem;border-radius:999px;color:var(--color-action-default)"
                    >
                        "«"
                    </button>
                    <button
                        type="button"
                        aria-label=tr(TextKey::PreviousPage)
                        disabled=move || page.get() == 0
                        on:click=move |_| page.update(|value| *value = value.saturating_sub(1))
                        style="min-width:2rem;height:2rem;border-radius:999px;color:var(--color-action-default)"
                    >
                        "‹"
                    </button>
                    <button
                        type="button"
                        aria-label=tr(TextKey::NextPage)
                        disabled=move || { u64::from(page.get()) >= last_page }
                        on:click=move |_| {
                            page.update(|value| {
                                *value = (*value + 1).min(u32::try_from(last_page).unwrap_or(u32::MAX));
                            });
                        }
                        style="min-width:2rem;height:2rem;border-radius:999px;color:var(--color-action-default)"
                    >
                        "›"
                    </button>
                    <button
                        type="button"
                        aria-label=tr(TextKey::LastPage)
                        disabled=move || { u64::from(page.get()) >= last_page }
                        on:click=move |_| page.set(u32::try_from(last_page).unwrap_or(u32::MAX))
                        style="min-width:2rem;height:2rem;border-radius:999px;color:var(--color-action-default)"
                    >
                        "»"
                    </button>
                </div>
            </div>
        </section>
    }
}

fn toggle_sort(
    field: AdminLogSortField,
    sort_field: RwSignal<AdminLogSortField>,
    sort_descending: RwSignal<Option<bool>>,
    page: RwSignal<u32>,
) {
    let next = if sort_field.get_untracked() == field {
        match sort_descending.get_untracked() {
            None => Some(false),
            Some(false) => Some(true),
            Some(true) => None,
        }
    } else {
        Some(false)
    };
    sort_field.set(field);
    sort_descending.set(next);
    page.set(0);
}

fn sort_indicator(
    active_field: AdminLogSortField,
    direction: Option<bool>,
    field: AdminLogSortField,
) -> &'static str {
    if active_field != field {
        return "";
    }
    match direction {
        Some(false) => "^",
        Some(true) => "v",
        None => "",
    }
}

fn render_row(entry: AdminLogEntry) -> impl IntoView {
    view! {
        <tr>
            <td style="padding:0.625rem 1rem;border-top:1px solid var(--color-table-line);white-space:nowrap">
                {entry.timestamp}
            </td>
            <td style="padding:0.625rem 1rem;border-top:1px solid var(--color-table-line);white-space:nowrap">
                {log_level_label(&entry.level)}
            </td>
            <td style="padding:0.625rem 1rem;border-top:1px solid var(--color-table-line)">
                {entry.message}
            </td>
        </tr>
    }
}
