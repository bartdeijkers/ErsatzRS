#![allow(clippy::too_many_lines)]
use leptos::prelude::*;

stylance::import_crate_style!(style, "src/ui/primitives/table.module.css");

/// A column definition for `PagedTable`.
pub struct Column<T: 'static> {
    pub header: &'static str,
    pub render: Box<dyn Fn(&T) -> AnyView + Send + Sync>,
}

impl<T: 'static> Column<T> {
    pub fn new(
        header: &'static str,
        render: impl Fn(&T) -> AnyView + Send + Sync + 'static,
    ) -> Self {
        Self {
            header,
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
    rows: Resource<Vec<T>>,
    /// Total item count across all pages (for footer).
    total: Signal<u64>,
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
    let go_prev = move |_| {
        params.update(|p| {
            if p.page > 0 {
                p.page -= 1;
            }
        });
    };
    let go_next = move |_| {
        params.update(|p| {
            let max = total.get().saturating_sub(1) / u64::from(p.page_size);
            if u64::from(p.page) < max {
                p.page += 1;
            }
        });
    };

    view! {
        <div class=style::wrapper>
            <div class=style::scroll>
                <table class=style::table>
                    <thead class=style::thead>
                        <tr>
                            {columns.iter().map(|col| {
                                let header = col.header;
                                view! {
                                    <th
                                        scope="col"
                                        class=style::th
                                        on:click=move |_| {
                                            params.update(|p| p.sort.toggle(header));
                                        }
                                    >
                                        <span class=style::sort_label>
                                            {header}
                                            {move || {
                                                let p = params.get();
                                                if p.sort.column == Some(header) {
                                                    if p.sort.ascending { "▲" } else { "▼" }
                                                } else { "" }
                                            }}
                                        </span>
                                    </th>
                                }
                            }).collect_view()}
                            {row_actions.as_ref().map(|_| view! { <th class=style::th_actions/> })}
                        </tr>
                    </thead>
                    <tbody>
                        <Suspense fallback=|| view! {
                            <tr><td colspan="99" class=style::td_empty>
                                "Loading..."
                            </td></tr>
                        }>
                            {move || {
                                rows.get().map(|items| {
                                    if items.is_empty() {
                                        view! {
                                            <tr>
                                                <td colspan="99" class=style::td_empty>
                                                    "No items."
                                                </td>
                                            </tr>
                                        }.into_any()
                                    } else {
                                        items.iter().map(|row| {
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
                                        }).collect_view().into_any()
                                    }
                                })
                            }}
                        </Suspense>
                    </tbody>
                </table>
            </div>

            <div class=style::pagination>
                <span>
                    {move || {
                        let p = params.get();
                        let start = p.page * p.page_size + 1;
                        let t = total.get();
                        let end = ((p.page + 1) * p.page_size)
                            .min(u32::try_from(t).unwrap_or(u32::MAX));
                        if t == 0 { "0 items".to_string() }
                        else { format!("{start}–{end} of {t}") }
                    }}
                </span>
                <div class=style::page_btns>
                    <button
                        class=style::page_btn
                        disabled=move || params.get().page == 0
                        on:click=go_prev
                    >"‹ Prev"</button>
                    <button
                        class=style::page_btn
                        disabled=move || {
                            let p = params.get();
                            let t = total.get();
                            t == 0 || u64::from((p.page + 1) * p.page_size) >= t
                        }
                        on:click=go_next
                    >"Next ›"</button>
                </div>
            </div>
        </div>
    }
}
