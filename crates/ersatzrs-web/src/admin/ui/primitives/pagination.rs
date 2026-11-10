use leptos::prelude::*;

use crate::admin::i18n::{page_of, range_of, tr, TextKey};
use crate::admin::ui::primitives::icon::{icon_style, IconName};

stylance::import_crate_style!(style, "src/admin/ui/primitives/pagination.module.css");

const DEFAULT_PAGE_SIZE_OPTIONS: [u32; 5] = [10, 25, 50, 100, 0];

/// Effective size for client-side slicing and page counts; zero selects all rows.
pub const fn page_size_limit(page_size: u32) -> u64 {
    if page_size == 0 {
        u64::MAX
    } else {
        page_size as u64
    }
}

/// Hidden query state preserved by the SSR-safe page-size form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaginationFormField {
    pub name: String,
    pub value: String,
}

fn resolved_page_size_options(options: Option<Vec<u32>>, current: u32) -> Vec<u32> {
    let mut options = options.unwrap_or_else(|| DEFAULT_PAGE_SIZE_OPTIONS.to_vec());
    options.push(0);
    if !options.contains(&current) {
        options.push(current);
    }
    options.sort_unstable_by_key(|size| (*size == 0, *size));
    options.dedup();
    options
}

/// Resolved offset-pagination state shared by button and link controls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PaginationMetrics {
    current_page: u32,
    total_pages: u32,
    range_start: u32,
    range_end: u32,
    previous_disabled: bool,
    next_disabled: bool,
}

fn pagination_metrics(page: u32, total: u64, page_size: u32) -> PaginationMetrics {
    let total_pages = if page_size == 0 || total == 0 {
        1
    } else {
        u32::try_from(total.div_ceil(u64::from(page_size)))
            .unwrap_or(u32::MAX)
            .max(1)
    };
    let current_page = page.min(total_pages.saturating_sub(1));
    let (range_start, range_end) = if total == 0 {
        (0, 0)
    } else if page_size == 0 {
        (1, u32::try_from(total).unwrap_or(u32::MAX))
    } else {
        let start = u64::from(current_page)
            .saturating_mul(u64::from(page_size))
            .saturating_add(1);
        let end = u64::from(current_page.saturating_add(1))
            .saturating_mul(u64::from(page_size))
            .min(total);
        (
            u32::try_from(start).unwrap_or(u32::MAX),
            u32::try_from(end).unwrap_or(u32::MAX),
        )
    };

    PaginationMetrics {
        current_page,
        total_pages,
        range_start,
        range_end,
        previous_disabled: current_page == 0,
        next_disabled: current_page.saturating_add(1) >= total_pages,
    }
}

/// Localized pagination controls for client-owned offset state.
///
/// Page-size selection is part of the control contract. Consumers may supply
/// route-specific choices; All is always offered after the numeric choices.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
#[component]
pub fn PaginationControls(
    page: Signal<u32>,
    total: Signal<Option<u64>>,
    page_size: Signal<u32>,
    on_previous: Callback<()>,
    on_next: Callback<()>,
    on_page_size_change: Callback<u32>,
    #[prop(default = None)] page_size_options: Option<Vec<u32>>,
) -> impl IntoView {
    let metrics = Signal::derive(move || {
        total
            .get()
            .map(|total| pagination_metrics(page.get(), total, page_size.get()))
    });

    let options = resolved_page_size_options(page_size_options, page_size.get_untracked());
    let page_size_view = view! {
        <label class=style::page_size>
            <span>{tr(TextKey::RowsPerPage)}</span>
            <select
                data-pagination-page-size
                aria-label=tr(TextKey::RowsPerPage)
                prop:value=move || page_size.get().to_string()
                on:change=move |event| {
                    if let Ok(value) = event_target_value(&event).parse::<u32>() {
                        on_page_size_change.run(value);
                    }
                }
            >
                {options.into_iter().map(|size| view! {
                    <option value=size.to_string()>
                        {if size == 0 { tr(TextKey::All).to_owned() } else { size.to_string() }}
                    </option>
                }).collect_view()}
            </select>
        </label>
    };

    view! {
        <nav
            class=style::pagination
            data-pagination
            aria-label=move || {
                metrics
                    .get()
                    .map_or_else(|| tr(TextKey::Loading).to_owned(), |value| {
                        page_of(value.current_page + 1, value.total_pages)
                    })
            }
        >
            {page_size_view}
            <div class=style::status>
                <span class=style::range role="status" aria-live="polite">
                    {move || {
                        metrics.get().map_or_else(
                            || tr(TextKey::Loading).to_owned(),
                            |value| {
                                range_of(
                                    value.range_start,
                                    value.range_end,
                                    total.get().unwrap_or_default(),
                                )
                            },
                        )
                    }}
                </span>
                <span class=style::page>
                    {move || {
                        metrics.get().map_or_else(String::new, |value| {
                            page_of(value.current_page + 1, value.total_pages)
                        })
                    }}
                </span>
            </div>
            <div class=style::buttons>
                <button
                    type="button"
                    class=style::button
                    title=tr(TextKey::PreviousPage)
                    aria-label=tr(TextKey::PreviousPage)
                    disabled=move || {
                        metrics
                            .get()
                            .is_none_or(|value| value.previous_disabled)
                    }
                    on:click=move |_| on_previous.run(())
                >
                    <span style=icon_style(IconName::ChevronLeft, "1.25rem") aria-hidden="true"></span>
                </button>
                <button
                    type="button"
                    class=style::button
                    title=tr(TextKey::NextPage)
                    aria-label=tr(TextKey::NextPage)
                    disabled=move || {
                        metrics.get().is_none_or(|value| value.next_disabled)
                    }
                    on:click=move |_| on_next.run(())
                >
                    <span style=icon_style(IconName::ChevronRight, "1.25rem") aria-hidden="true"></span>
                </button>
            </div>
        </nav>
    }
}

/// Localized SSR-safe pagination links for URL-owned offset state.
#[component]
pub fn PaginationLinks(
    page: u32,
    total: u64,
    page_size: u32,
    previous_href: String,
    next_href: String,
    page_size_action: String,
    #[prop(default = Vec::new())] page_size_fields: Vec<PaginationFormField>,
    #[prop(default = None)] page_size_options: Option<Vec<u32>>,
    /// `false` renders only the counter and page links, for a repeated pager
    /// below long result lists; the page-size choice stays in the top pager.
    #[prop(default = true)]
    show_page_size: bool,
) -> impl IntoView {
    let metrics = pagination_metrics(page, total, page_size);
    let previous_href = (!metrics.previous_disabled).then_some(previous_href);
    let next_href = (!metrics.next_disabled).then_some(next_href);
    let options = resolved_page_size_options(page_size_options, page_size);

    view! {
        <nav
            class=style::pagination
            data-pagination
            aria-label=page_of(metrics.current_page + 1, metrics.total_pages)
        >
            {show_page_size.then(|| view! {
                <form method="get" action=page_size_action class=style::page_size_form data-pagination-form>
                    {page_size_fields.into_iter().map(|field| view! {
                        <input type="hidden" name=field.name value=field.value/>
                    }).collect_view()}
                    <label class=style::page_size>
                        <span>{tr(TextKey::RowsPerPage)}</span>
                        <select
                            name="page_size"
                            data-pagination-page-size
                            aria-label=tr(TextKey::RowsPerPage)
                        >
                            {options.into_iter().map(|size| view! {
                                <option value=size.to_string() selected=size == page_size>
                                    {if size == 0 { tr(TextKey::All).to_owned() } else { size.to_string() }}
                                </option>
                            }).collect_view()}
                        </select>
                    </label>
                    <button type="submit" class=style::apply>{tr(TextKey::Apply)}</button>
                </form>
            })}
            <div class=style::status>
                <span class=style::range>
                    {range_of(metrics.range_start, metrics.range_end, total)}
                </span>
                <span class=style::page>
                    {page_of(metrics.current_page + 1, metrics.total_pages)}
                </span>
            </div>
            <div class=style::buttons>
                <a
                    class=if metrics.previous_disabled {
                        format!("{} {}", style::button, style::disabled)
                    } else {
                        style::button.to_owned()
                    }
                    href=previous_href
                    title=tr(TextKey::PreviousPage)
                    aria-label=tr(TextKey::PreviousPage)
                    aria-disabled=metrics.previous_disabled.to_string()
                >
                    <span style=icon_style(IconName::ChevronLeft, "1.25rem") aria-hidden="true"></span>
                </a>
                <a
                    class=if metrics.next_disabled {
                        format!("{} {}", style::button, style::disabled)
                    } else {
                        style::button.to_owned()
                    }
                    href=next_href
                    title=tr(TextKey::NextPage)
                    aria-label=tr(TextKey::NextPage)
                    aria-disabled=metrics.next_disabled.to_string()
                >
                    <span style=icon_style(IconName::ChevronRight, "1.25rem") aria-hidden="true"></span>
                </a>
            </div>
        </nav>
    }
}

#[cfg(test)]
mod tests {
    use super::{pagination_metrics, resolved_page_size_options, PaginationMetrics};

    #[test]
    fn default_page_size_choices_include_a_nonstandard_current_value() {
        assert_eq!(
            resolved_page_size_options(None, 5),
            vec![5, 10, 25, 50, 100, 0]
        );
    }

    #[test]
    fn custom_choices_keep_all_last_without_duplicates() {
        assert_eq!(
            resolved_page_size_options(Some(vec![25, 0, 10, 25]), 0),
            vec![10, 25, 0]
        );
        assert_eq!(
            resolved_page_size_options(Some(vec![10, 25]), 25),
            vec![10, 25, 0]
        );
    }

    #[test]
    fn empty_results_keep_one_disabled_page() {
        assert_eq!(
            pagination_metrics(0, 0, 25),
            PaginationMetrics {
                current_page: 0,
                total_pages: 1,
                range_start: 0,
                range_end: 0,
                previous_disabled: true,
                next_disabled: true,
            }
        );
    }

    #[test]
    fn final_partial_page_reports_the_visible_range() {
        assert_eq!(
            pagination_metrics(2, 51, 25),
            PaginationMetrics {
                current_page: 2,
                total_pages: 3,
                range_start: 51,
                range_end: 51,
                previous_disabled: false,
                next_disabled: true,
            }
        );
    }

    #[test]
    fn all_rows_option_is_one_page() {
        assert_eq!(
            pagination_metrics(4, 72, 0),
            PaginationMetrics {
                current_page: 0,
                total_pages: 1,
                range_start: 1,
                range_end: 72,
                previous_disabled: true,
                next_disabled: true,
            }
        );
    }
}
