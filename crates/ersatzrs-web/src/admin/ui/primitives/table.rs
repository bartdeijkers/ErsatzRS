#![allow(clippy::too_many_lines)]
use leptos::prelude::*;

use crate::admin::i18n::{tr, TextKey};
use crate::admin::ui::primitives::empty_state::EmptyState;
use crate::admin::ui::primitives::pagination::PaginationControls;

stylance::import_crate_style!(style, "src/admin/ui/primitives/table.module.css");

/// A column definition for `PagedTable`.
pub struct Column<T: 'static> {
    pub header: &'static str,
    pub sort_key: Option<&'static str>,
    pub render: Box<dyn Fn(&T) -> AnyView + Send + Sync>,
}

impl<T: 'static> Column<T> {
    pub fn new(
        header: &'static str,
        render: impl Fn(&T) -> AnyView + Send + Sync + 'static,
    ) -> Self {
        Self {
            header,
            sort_key: None,
            render: Box::new(render),
        }
    }

    /// Define a column whose parent resource implements the given sort key.
    pub fn sortable(
        header: &'static str,
        sort_key: &'static str,
        render: impl Fn(&T) -> AnyView + Send + Sync + 'static,
    ) -> Self {
        Self {
            header,
            sort_key: Some(sort_key),
            render: Box::new(render),
        }
    }
}

/// Sort state shared between the parent and the table.
#[derive(Clone, PartialEq, Eq)]
pub struct SortState {
    pub column: Option<&'static str>,
    pub ascending: bool,
}

impl SortState {
    pub fn new() -> Self {
        Self {
            column: None,
            ascending: true,
        }
    }

    /// Toggle sort on the given column header; clicking the same column flips direction.
    pub fn toggle(&mut self, col: &'static str) {
        if self.column == Some(col) {
            self.ascending = !self.ascending;
        } else {
            self.column = Some(col);
            self.ascending = true;
        }
    }
}

/// Pagination parameters used by the parent to fetch data.
#[derive(Clone, PartialEq, Eq)]
pub struct PageParams {
    pub page: u32,
    pub page_size: u32,
    pub sort: SortState,
}

impl PageParams {
    pub fn new(page_size: u32) -> Self {
        Self {
            page: 0,
            page_size,
            sort: SortState::new(),
        }
    }
}

/// Type alias for a boxed row-action renderer, used in `PagedTable`.
pub type RowActionsRenderer<T> = Box<dyn Fn(&T) -> AnyView + Send + Sync>;

/// A paginated table of rows of type `T`.
#[allow(clippy::too_many_lines)]
#[component]
pub fn PagedTable<T>(
    /// The current (fetched) rows to display.
    rows: impl Get<Value = Option<Vec<T>>> + Clone + Send + Sync + 'static,
    /// Total item count across all pages (for footer).
    total: Signal<Option<u64>>,
    /// Shared page/sort params — the parent derives its resource key from this.
    params: RwSignal<PageParams>,
    /// Column definitions.
    columns: Vec<Column<T>>,
    /// Optional: per-row actions rendered in a trailing cell.
    #[prop(default = None)]
    row_actions: Option<RowActionsRenderer<T>>,
) -> impl IntoView
where
    T: Clone + Send + Sync + 'static,
{
    let go_prev = Callback::new(move |()| {
        params.update(|p| {
            if p.page > 0 {
                p.page -= 1;
            }
        });
    });
    let go_next = Callback::new(move |()| {
        params.update(|p| {
            let total_items = total.get().unwrap_or(0);
            let max = total_items.saturating_sub(1) / u64::from(p.page_size);
            if u64::from(p.page) < max {
                p.page += 1;
            }
        });
    });

    view! {
        <div class=style::wrapper>
            <div class=style::scroll>
                <table class=style::table>
                    <thead class=style::thead>
                        <tr>
                            {columns.iter().map(|col| {
                                let header = col.header;
                                if let Some(sort_key) = col.sort_key {
                                    view! {
                                        <th
                                            scope="col"
                                            class=style::th
                                            aria-sort=move || {
                                                let sort = params.get().sort;
                                                if sort.column == Some(sort_key) {
                                                    if sort.ascending { "ascending" } else { "descending" }
                                                } else {
                                                    "none"
                                                }
                                            }
                                        >
                                            <button
                                                type="button"
                                                class=style::sort_button
                                                on:click=move |_| {
                                                    params.update(|p| {
                                                        p.sort.toggle(sort_key);
                                                        p.page = 0;
                                                    });
                                                }
                                            >
                                                {header}
                                                <span aria-hidden="true">
                                                    {move || {
                                                        let p = params.get();
                                                        if p.sort.column == Some(sort_key) {
                                                            if p.sort.ascending { "▲" } else { "▼" }
                                                        } else { "" }
                                                    }}
                                                </span>
                                            </button>
                                        </th>
                                    }.into_any()
                                } else {
                                    view! {
                                        <th scope="col" class=style::th>{header}</th>
                                    }.into_any()
                                }
                            }).collect_view()}
                            {row_actions.as_ref().map(|_| view! { <th class=style::th_actions/> })}
                        </tr>
                    </thead>
                    <tbody>
                        {move || {
                            match rows.get() {
                                None => view! {
                                    <tr><td colspan="99" class=style::td_empty>
                                        {tr(TextKey::Loading)}
                                    </td></tr>
                                }.into_any(),
                                Some(items) if items.is_empty() => view! {
                                    <tr>
                                        <td colspan="99">
                                            <EmptyState
                                                title=tr(TextKey::EmptyListTitle)
                                                help=tr(TextKey::EmptyListHelp)
                                            />
                                        </td>
                                    </tr>
                                }.into_any(),
                                Some(items) => items.iter().map(|row| {
                                    let cells = columns.iter().map(|col| {
                                        view! {
                                            <td class=style::td>
                                                {(col.render)(row)}
                                            </td>
                                        }
                                    }).collect_view();
                                    let action_cell = row_actions.as_ref().map(|ra| {
                                        view! {
                                            <td class=style::td_actions>
                                                {(ra)(row)}
                                            </td>
                                        }
                                    });
                                    view! {
                                        <tr class=style::tr>
                                            {cells}
                                            {action_cell}
                                        </tr>
                                    }
                                }).collect_view().into_any(),
                            }
                        }}
                    </tbody>
                </table>
            </div>
            // An empty list has nothing to page through; the empty state
            // explains the list instead of a "0–0" counter.
            <div
                class=style::pagination_footer
                style:display=move || if total.get() == Some(0) { "none" } else { "" }
            >
                <PaginationControls
                    page=Signal::derive(move || params.get().page)
                    total=total
                    page_size=Signal::derive(move || params.get().page_size)
                    on_previous=go_prev
                    on_next=go_next
                    on_page_size_change=Callback::new(move |value: u32| {
                        params.update(|current| {
                            current.page_size = value.max(1);
                            current.page = 0;
                        });
                    })
                />
            </div>
        </div>
    }
}
