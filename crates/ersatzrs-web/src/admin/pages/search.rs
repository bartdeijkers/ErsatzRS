#![allow(dead_code)]

use leptos::prelude::*;
use leptos_router::hooks::{use_navigate, use_query_map};
use leptos_router::NavigateOptions;

use crate::admin::server_fns::search::{search_media, SearchResultRow};

stylance::import_crate_style!(style, "src/admin/pages/media/collections.module.css");

#[component]
pub fn SearchPage() -> impl IntoView {
    let query_map = use_query_map();
    let query =
        Memo::new(move |_| query_map.with(|params| params.get("query").unwrap_or_default()));
    let input = RwSignal::new(query.get_untracked());
    let navigate = use_navigate();
    let keydown_navigate = navigate.clone();

    Effect::new(move |_| {
        input.set(query.get());
    });

    let data: Resource<(String, Vec<SearchResultRow>)> = Resource::new(
        move || query.get(),
        move |query| async move {
            let rows = search_media(query.clone()).await.unwrap_or_default();
            (query, rows)
        },
    );

    view! {
        <div class=style::page>
            <div class=style::toolbar>
                <h1 class=style::page_title>"Search"</h1>
                <input
                    class=style::search
                    type="text"
                    prop:value=move || input.get()
                    on:input=move |event| input.set(event_target_value(&event))
                    on:keydown=move |event| {
                        if event.key() == "Enter" {
                            let query = input.get_untracked();
                            let encoded = urlencoding::encode(query.trim()).into_owned();
                            keydown_navigate(
                                &format!("/search?query={encoded}"),
                                NavigateOptions::default(),
                            );
                        }
                    }
                />
                <button
                    class=style::btn_add
                    disabled=move || input.with(|value| value.trim().is_empty())
                    on:click=move |_| {
                        let query = input.get_untracked();
                        let encoded = urlencoding::encode(query.trim()).into_owned();
                        navigate(
                            &format!("/search?query={encoded}"),
                            NavigateOptions::default(),
                        );
                    }
                >
                    "Search"
                </button>
            </div>

            <Suspense fallback=move || view! { <p class=style::empty>"Loading…"</p> }>
                {move || {
                    data.get().map(|(query, rows)| {
                        if query.trim().is_empty() {
                            return view! {
                                <p class=style::empty>"Enter a search query."</p>
                            }
                            .into_any();
                        }

                        if rows.is_empty() {
                            return view! {
                                <p class=style::empty>"No media matched."</p>
                            }
                            .into_any();
                        }

                        view! {
                            <div class=style::table_wrap>
                                <table>
                                    <thead>
                                        <tr>
                                            <th>"Title"</th>
                                            <th>"Kind"</th>
                                            <th>"Year"</th>
                                            <th>"Duration"</th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {rows.into_iter().map(|row| {
                                            view! {
                                                <tr>
                                                    <td>{row.title}</td>
                                                    <td>{row.media_kind}</td>
                                                    <td>{row.year.map(|year| year.to_string()).unwrap_or_default()}</td>
                                                    <td>{format_duration(row.duration_seconds)}</td>
                                                </tr>
                                            }
                                        }).collect_view()}
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

fn format_duration(seconds: i64) -> String {
    let total = seconds.max(0);
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let seconds = total % 60;

    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}
