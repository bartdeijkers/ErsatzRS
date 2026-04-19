//! Playouts list page — ports `Pages/Playouts.razor`.
//!
//! Phase 6.6: read-only navigation hub. Actions column shows an edit link for
//! Sequential (`ScheduleKind = 3`, YAML) playouts only; other kinds are future scope.
#![allow(clippy::too_many_lines)]
use leptos::prelude::*;

use crate::server::playouts::{list_playouts, PlayoutListRow};

stylance::import_crate_style!(style, "src/pages/scheduling/playouts.module.css");

/// `ScheduleKind` integer value for Sequential (YAML) playouts.
const SEQUENTIAL: i32 = 3;

#[component]
pub fn PlayoutsPage() -> impl IntoView {
    let rows: Resource<Vec<PlayoutListRow>> = Resource::new(
        || (),
        |()| async { list_playouts().await.unwrap_or_default() },
    );

    view! {
        <div class=style::page>
            <h1 class=style::page_title>"Playouts"</h1>

            <Suspense fallback=move || view! { <p class=style::empty>"Loading\u{2026}"</p> }>
                {move || {
                    rows.get().map(|data| {
                        if data.is_empty() {
                            return view! {
                                <p class=style::empty>"No playouts found."</p>
                            }
                            .into_any();
                        }

                        view! {
                            <div class=style::table_wrap>
                                <table>
                                    <thead>
                                        <tr>
                                            <th>"Channel"</th>
                                            <th>"Schedule Kind"</th>
                                            <th>"Schedule Name"</th>
                                            <th/>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {data
                                            .into_iter()
                                            .map(|row| {
                                                let id = row.id;
                                                let channel = if row.channel_number.is_empty() {
                                                    row.channel_name.clone()
                                                } else {
                                                    format!(
                                                        "{} \u{2014} {}",
                                                        row.channel_number,
                                                        row.channel_name,
                                                    )
                                                };
                                                let kind_label = row.schedule_kind_label.clone();
                                                let sched_name = row.schedule_name.clone();
                                                let is_sequential = row.schedule_kind == SEQUENTIAL;
                                                view! {
                                                    <tr>
                                                        <td>{channel}</td>
                                                        <td class=style::cell_muted>
                                                            {kind_label}
                                                        </td>
                                                        <td class=style::cell_muted>
                                                            {sched_name}
                                                        </td>
                                                        <td>
                                                            {if is_sequential {
                                                                view! {
                                                                    <a
                                                                        class=style::btn_edit
                                                                        href=format!(
                                                                            "/playouts/sequential/{id}",
                                                                        )
                                                                    >
                                                                        "Edit"
                                                                    </a>
                                                                }
                                                                    .into_any()
                                                            } else {
                                                                view! { <span/> }.into_any()
                                                            }}
                                                        </td>
                                                    </tr>
                                                }
                                            })
                                            .collect_view()}
                                    </tbody>
                                </table>
                            </div>
                        }
                        .into_any()
                    })
                }}
            </Suspense>
        </div>
    }
}
