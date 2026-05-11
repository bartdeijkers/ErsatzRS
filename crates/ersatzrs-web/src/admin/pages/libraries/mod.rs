#![allow(clippy::too_many_lines, dead_code)]
use leptos::prelude::*;

use crate::admin::server_fns::libraries::{
    list_external_collections, list_libraries, scan_external_collections, scan_library,
    ExternalCollectionRow, LibraryRow,
};
#[cfg(target_arch = "wasm32")]
use crate::admin::server_fns::libraries::{list_locked_collections, list_locked_libraries};
use crate::admin::ui::primitives::icon::{icon_style, IconName};
use crate::admin::ui::primitives::toast::{use_toast, ToastSeverity};

stylance::import_crate_style!(style, "src/admin/pages/libraries/libraries.module.css");

#[component]
pub fn LibrariesPage() -> impl IntoView {
    view! { <LibrariesIsland/> }
}

#[island]
fn LibrariesIsland() -> impl IntoView {
    let libs: Resource<Vec<LibraryRow>> = Resource::new(
        || (),
        |()| async move { list_libraries().await.unwrap_or_default() },
    );
    let scan_action = Action::new(|(id, kind, deep_scan): &(i64, String, bool)| {
        let id = *id;
        let kind = kind.clone();
        let deep_scan = *deep_scan;
        async move { scan_library(id, kind, deep_scan).await }
    });

    let externals: Resource<Vec<ExternalCollectionRow>> = Resource::new(
        || (),
        |()| async move { list_external_collections().await.unwrap_or_default() },
    );
    let collections_action =
        Action::new(|(media_source_id, kind, deep_scan): &(i64, String, bool)| {
            let media_source_id = *media_source_id;
            let kind = kind.clone();
            let deep_scan = *deep_scan;
            async move { scan_external_collections(media_source_id, kind, deep_scan).await }
        });

    let toast = use_toast();
    Effect::new(move |_| {
        if let Some(res) = scan_action.value().get() {
            match res {
                Ok(()) => toast.push("Scan queued", ToastSeverity::Success),
                Err(e) => toast.push(format!("Scan failed: {e}"), ToastSeverity::Error),
            }
        }
    });
    Effect::new(move |_| {
        if let Some(res) = collections_action.value().get() {
            match res {
                Ok(()) => toast.push("Collections scan queued", ToastSeverity::Success),
                Err(e) => toast.push(
                    format!("Collections scan failed: {e}"),
                    ToastSeverity::Error,
                ),
            }
        }
    });

    // Poll the entity locker so each row knows whether its scan task is still
    // running. Mirrors ETV's `IEntityLocker.OnLibraryChanged` poll loop, but
    // implemented client-side with `gloo-timers` (no SignalR).
    let locked: RwSignal<Vec<i64>> = RwSignal::new(Vec::new());
    let locked_collections: RwSignal<Vec<(String, i64)>> = RwSignal::new(Vec::new());
    Effect::new(move |_| {
        #[cfg(target_arch = "wasm32")]
        {
            leptos::task::spawn_local(async move {
                loop {
                    if let Ok(ids) = list_locked_libraries().await {
                        locked.set(ids);
                    }
                    if let Ok(pairs) = list_locked_collections().await {
                        locked_collections.set(pairs);
                    }
                    gloo_timers::future::TimeoutFuture::new(1_500).await;
                }
            });
        }
    });
    let is_locked = move |id: i64| locked.with(|ids| ids.contains(&id));
    let are_collections_locked = move |kind: &str, media_source_id: i64| {
        locked_collections.with(|pairs| {
            pairs
                .iter()
                .any(|(k, id)| k == kind && *id == media_source_id)
        })
    };
    let scanning = move || scan_action.pending().get();
    let scanning_collections = move || collections_action.pending().get();

    view! {
        <div class=style::page>
            <h1 class=style::page_title>"Libraries"</h1>
            <Suspense fallback=move || {
                view! { <p class=style::empty>"Loading…"</p> }
            }>
                {move || {
                    libs.get().map(|rows| {
                        if rows.is_empty() {
                            return view! {
                                <p class=style::empty>
                                    "No libraries configured. Add a media source first."
                                </p>
                            }
                            .into_any();
                        }

                        view! {
                            <div class=style::table_wrap>
                                <table>
                                    <thead>
                                        <tr>
                                            <th>"Kind"</th>
                                            <th>"Server"</th>
                                            <th>"Name"</th>
                                            <th>"Media Kind"</th>
                                            <th/>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {rows
                                            .into_iter()
                                            .map(|lib| {
                                                let id = lib.id;
                                                let kind = lib.kind.clone();
                                                let is_remote = kind != "Local";
                                                let kind_scan = kind.clone();
                                                let kind_deep = kind.clone();
                                                let srv = lib.source_name.clone().unwrap_or_default();
                                                let search_href = format!("/search?query=library_id%3a{id}");
                                                view! {
                                                    <tr>
                                                        <td>{kind}</td>
                                                        <td class=style::cell_muted>{srv}</td>
                                                        <td>{lib.name.clone()}</td>
                                                        <td class=style::cell_muted>{lib.media_kind_label()}</td>
                                                        <td>
                                                            <div class=style::actions>
                                                                {if is_remote {
                                                                    view! {
                                                                        <button
                                                                            class=style::icon_btn
                                                                            disabled=move || scanning() || is_locked(id)
                                                                            title="Deep Scan Library"
                                                                            aria-label="Deep Scan Library"
                                                                            on:click=move |_| {
                                                                                scan_action.dispatch((id, kind_deep.clone(), true));
                                                                            }
                                                                        >
                                                                            <span
                                                                                style=icon_style(IconName::FindReplace, "1.125rem")
                                                                                aria-hidden="true"
                                                                            />
                                                                        </button>
                                                                    }
                                                                        .into_any()
                                                                } else {
                                                                    view! { <span class=style::icon_slot aria-hidden="true"/> }
                                                                        .into_any()
                                                                }}
                                                                <button
                                                                    class=style::icon_btn
                                                                    disabled=move || scanning() || is_locked(id)
                                                                    title="Scan Library"
                                                                    aria-label="Scan Library"
                                                                    on:click=move |_| {
                                                                        scan_action.dispatch((id, kind_scan.clone(), false));
                                                                    }
                                                                >
                                                                    <span
                                                                        style=icon_style(IconName::Refresh, "1.125rem")
                                                                        aria-hidden="true"
                                                                    />
                                                                </button>
                                                                <a
                                                                    class=style::icon_btn
                                                                    href=search_href
                                                                    title="Search Library"
                                                                    aria-label="Search Library"
                                                                >
                                                                    <span
                                                                        style=icon_style(IconName::Search, "1.125rem")
                                                                        aria-hidden="true"
                                                                    />
                                                                </a>
                                                                <span
                                                                    class=style::spinner
                                                                    style:display=move || {
                                                                        if is_locked(id) { "inline-block" } else { "none" }
                                                                    }
                                                                    title="Scanning…"
                                                                    aria-label="Scanning"
                                                                />
                                                            </div>
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
            <Suspense fallback=|| ()>
                {move || {
                    externals.get().map(|rows| {
                        if rows.is_empty() {
                            return view! { <span/> }.into_any();
                        }
                        view! {
                            <h2 class=style::section_title>"External Collections"</h2>
                            <div class=style::table_wrap>
                                <table>
                                    <thead>
                                        <tr>
                                            <th>"Library Kind"</th>
                                            <th>"Server"</th>
                                            <th/>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {rows
                                            .into_iter()
                                            .map(|ext| {
                                                let media_source_id = ext.media_source_id;
                                                let kind = ext.kind.clone();
                                                let kind_scan = kind.clone();
                                                let kind_deep = kind.clone();
                                                let kind_lock_a = kind.clone();
                                                let kind_lock_b = kind.clone();
                                                let kind_lock_c = kind.clone();
                                                let srv = ext.server_name.clone().unwrap_or_default();
                                                view! {
                                                    <tr>
                                                        <td>{kind.clone()}</td>
                                                        <td class=style::cell_muted>{srv}</td>
                                                        <td>
                                                            <div class=style::actions>
                                                                <button
                                                                    class=style::icon_btn
                                                                    disabled=move || {
                                                                        scanning_collections()
                                                                            || are_collections_locked(&kind_lock_a, media_source_id)
                                                                    }
                                                                    title="Deep Scan Collections"
                                                                    aria-label="Deep Scan Collections"
                                                                    on:click=move |_| {
                                                                        collections_action
                                                                            .dispatch((media_source_id, kind_deep.clone(), true));
                                                                    }
                                                                >
                                                                    <span
                                                                        style=icon_style(IconName::FindReplace, "1.125rem")
                                                                        aria-hidden="true"
                                                                    />
                                                                </button>
                                                                <button
                                                                    class=style::icon_btn
                                                                    disabled=move || {
                                                                        scanning_collections()
                                                                            || are_collections_locked(&kind_lock_b, media_source_id)
                                                                    }
                                                                    title="Scan Collections"
                                                                    aria-label="Scan Collections"
                                                                    on:click=move |_| {
                                                                        collections_action
                                                                            .dispatch((media_source_id, kind_scan.clone(), false));
                                                                    }
                                                                >
                                                                    <span
                                                                        style=icon_style(IconName::Refresh, "1.125rem")
                                                                        aria-hidden="true"
                                                                    />
                                                                </button>
                                                                <span
                                                                    class=style::spinner
                                                                    style:display=move || {
                                                                        if are_collections_locked(&kind_lock_c, media_source_id) {
                                                                            "inline-block"
                                                                        } else {
                                                                            "none"
                                                                        }
                                                                    }
                                                                    title="Scanning…"
                                                                    aria-label="Scanning"
                                                                />
                                                            </div>
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
