use leptos::prelude::*;

use crate::admin::i18n::{log_level_label, tr, TextKey};
use crate::admin::server_fns::logs::{
    get_logs_page_size, load_recent_log_entries, AdminLogEntry, AdminLogSnapshot, AdminLogSortField,
};
use crate::admin::ui::primitives::button::{Button, ButtonVariant};
use crate::admin::ui::primitives::pagination::PaginationControls;
use crate::admin::ui::primitives::resource_state::{
    admin_loading_resource_view, admin_result_view,
};
use crate::admin::ui::primitives::system_navigation::SystemPageLayout;
use crate::admin::ui::primitives::toast::{load_or_notify, use_toast};

stylance::import_crate_style!(style, "src/admin/pages/system/logs.module.css");

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
        <SystemPageLayout>
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
        </SystemPageLayout>
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
        <main class=style::page data-testid="logs-page">
            <header class=style::page_header>
                <h1>{tr(TextKey::Logs)}</h1>
            </header>

            <div class=style::toolbar>
                <label class=style::search_field for="support-log-search">
                    <span>{tr(TextKey::Search)}</span>
                    <input
                        id="support-log-search"
                        type="search"
                        prop:value=move || search.get()
                        on:input=move |event| {
                            search.set(event_target_value(&event));
                            page.set(0);
                        }
                    />
                </label>
                <Button
                    variant=ButtonVariant::Secondary
                    on_click=Some(Callback::new(move |()| {
                        search.set(String::new());
                        page.set(0);
                    }))
                >
                    {tr(TextKey::Clear)}
                </Button>
                <span class=style::result_count role="status" aria-live="polite">
                    {move || current_snapshot.get().total_count}
                </span>
            </div>

            {move || render_snapshot(
                current_snapshot.get(),
                page,
                page_size,
                sort_field,
                sort_descending,
            )}
        </main>
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

    view! {
        <section class=style::data_surface aria-label=tr(TextKey::Logs)>
            <div class=style::table_scroll>
                <table class=style::table>
                    <caption class=style::sr_only>{tr(TextKey::Logs)}</caption>
                    <colgroup>
                        <col class=style::timestamp_column/>
                        <col class=style::level_column/>
                        <col/>
                    </colgroup>
                    <thead>
                        <tr>
                            <th
                                scope="col"
                                aria-sort=move || sort_aria(
                                    sort_field.get(),
                                    sort_descending.get(),
                                    AdminLogSortField::Timestamp,
                                )
                            >
                                <button
                                    type="button"
                                    class=style::sort_button
                                    aria-label=tr(TextKey::SortByTimestamp)
                                    on:click=move |_| toggle_sort(
                                        AdminLogSortField::Timestamp,
                                        sort_field,
                                        sort_descending,
                                        page,
                                    )
                                >
                                    {tr(TextKey::Timestamp)}
                                    <span aria-hidden="true">{move || sort_indicator(
                                        sort_field.get(),
                                        sort_descending.get(),
                                        AdminLogSortField::Timestamp,
                                    )}</span>
                                </button>
                            </th>
                            <th
                                scope="col"
                                aria-sort=move || sort_aria(
                                    sort_field.get(),
                                    sort_descending.get(),
                                    AdminLogSortField::Level,
                                )
                            >
                                <button
                                    type="button"
                                    class=style::sort_button
                                    aria-label=tr(TextKey::SortByLevel)
                                    on:click=move |_| toggle_sort(
                                        AdminLogSortField::Level,
                                        sort_field,
                                        sort_descending,
                                        page,
                                    )
                                >
                                    {tr(TextKey::Level)}
                                    <span aria-hidden="true">{move || sort_indicator(
                                        sort_field.get(),
                                        sort_descending.get(),
                                        AdminLogSortField::Level,
                                    )}</span>
                                </button>
                            </th>
                            <th scope="col">{tr(TextKey::Message)}</th>
                        </tr>
                    </thead>
                    <tbody>
                        {if snapshot.entries.is_empty() {
                            view! {
                                <tr>
                                    <td colspan="3" class=style::empty>
                                        {tr(TextKey::NoMatchingRecordsFound)}
                                    </td>
                                </tr>
                            }
                            .into_any()
                        } else {
                            snapshot.entries.into_iter().map(render_row).collect_view().into_any()
                        }}
                    </tbody>
                </table>
            </div>
            <div class=style::pagination_footer>
                <PaginationControls
                    page=Signal::derive(move || page.get())
                    total=Signal::derive(move || Some(total_count))
                    page_size=Signal::derive(move || page_size.get())
                    on_previous=Callback::new(move |()| {
                        page.update(|value| *value = value.saturating_sub(1));
                    })
                    on_next=Callback::new(move |()| {
                        let last_page = total_count
                            .saturating_sub(1)
                            .checked_div(u64::from(page_size.get().max(1)))
                            .unwrap_or(0);
                        page.update(|value| {
                            *value = value
                                .saturating_add(1)
                                .min(u32::try_from(last_page).unwrap_or(u32::MAX));
                        });
                    })
                    on_page_size_change=Callback::new(move |value: u32| {
                        page_size.set(value.max(1));
                        page.set(0);
                    })
                />
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
        Some(false) => "▲",
        Some(true) => "▼",
        None => "",
    }
}

fn sort_aria(
    active_field: AdminLogSortField,
    direction: Option<bool>,
    field: AdminLogSortField,
) -> &'static str {
    if active_field != field {
        return "none";
    }
    match direction {
        Some(false) => "ascending",
        Some(true) => "descending",
        None => "none",
    }
}

fn render_row(entry: AdminLogEntry) -> impl IntoView {
    let level_class = match entry.level.to_ascii_lowercase().as_str() {
        "warning" | "warn" => format!("{} {}", style::level, style::level_warning),
        "error" | "fatal" => format!("{} {}", style::level, style::level_error),
        _ => format!("{} {}", style::level, style::level_information),
    };
    view! {
        <tr>
            <td class=style::timestamp>{entry.timestamp}</td>
            <td class=level_class>{log_level_label(&entry.level)}</td>
            <td class=style::message>{entry.message}</td>
        </tr>
    }
}

#[cfg(test)]
mod tests {
    use super::{sort_aria, sort_indicator};
    use crate::admin::server_fns::logs::AdminLogSortField;

    #[test]
    fn visible_and_accessible_sort_states_stay_in_sync() {
        assert_eq!(
            sort_indicator(
                AdminLogSortField::Timestamp,
                Some(true),
                AdminLogSortField::Timestamp
            ),
            "▼"
        );
        assert_eq!(
            sort_aria(
                AdminLogSortField::Timestamp,
                Some(true),
                AdminLogSortField::Timestamp
            ),
            "descending"
        );
        assert_eq!(
            sort_aria(
                AdminLogSortField::Timestamp,
                Some(true),
                AdminLogSortField::Level
            ),
            "none"
        );
    }
}
