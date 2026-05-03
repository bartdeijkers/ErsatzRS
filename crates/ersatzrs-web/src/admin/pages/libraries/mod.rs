#![allow(clippy::too_many_lines, dead_code)]
use leptos::prelude::*;

#[cfg(target_arch = "wasm32")]
use crate::admin::server_fns::libraries::list_locked_libraries;
use crate::admin::server_fns::libraries::{list_libraries, scan_library, LibraryRow};
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

    let toast = use_toast();
    Effect::new(move |_| {
        if let Some(res) = scan_action.value().get() {
            match res {
                Ok(()) => toast.push("Scan queued", ToastSeverity::Success),
                Err(e) => toast.push(format!("Scan failed: {e}"), ToastSeverity::Error),
            }
        }
    });

    // Poll the entity locker so each row knows whether its scan task is still
    // running. Mirrors ETV's `IEntityLocker.OnLibraryChanged` poll loop, but
    // implemented client-side with `gloo-timers` (no SignalR). The interval
    // is short enough that the spinner appears responsive but long enough to
    // avoid hammering SQLite during a scan.
    let locked: RwSignal<Vec<i64>> = RwSignal::new(Vec::new());
    Effect::new(move |_| {
        #[cfg(target_arch = "wasm32")]
        {
            leptos::task::spawn_local(async move {
                loop {
                    if let Ok(ids) = list_locked_libraries().await {
                        locked.set(ids);
                    }
                    gloo_timers::future::TimeoutFuture::new(1_500).await;
                }
            });
        }
    });
    let is_locked = move |id: i64| locked.with(|ids| ids.contains(&id));

    let scanning = move || scan_action.pending().get();

    view! {
        <div class=style::page>
            <h1 class=style::page_title>"Libraries"</h1>
            <Suspense fallback=move || {
                view! { <p class=style::empty>"Loading…"</p> }
            }>
                {move || {
                    libs.get().map(|rows| {
                        let has_server = rows
                            .iter()
                            .any(|r| r.kind == "Plex" || r.kind == "Jellyfin" || r.kind == "Emby");

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
                                            {has_server.then(|| view! { <th>"Server"</th> })}
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
                                                view! {
                                                    <tr>
                                                        <td>{kind}</td>
                                                        {has_server
                                                            .then(|| {
                                                                let srv = lib
                                                                    .source_name
                                                                    .clone()
                                                                    .unwrap_or_default();
                                                                view! {
                                                                    <td class=style::cell_muted>
                                                                        {srv}
                                                                    </td>
                                                                }
                                                            })}
                                                        <td>{lib.name.clone()}</td>
                                                        <td class=style::cell_muted>
                                                            {lib.media_kind_label()}
                                                        </td>
                                                        <td>
                                                            <div class=style::actions>
                                                                <button
                                                                    class=style::btn_scan
                                                                    style:display=move || {
                                                                        if is_remote { "" } else { "none" }
                                                                    }
                                                                    disabled=move || {
                                                                        scanning() || is_locked(id)
                                                                    }
                                                                    on:click=move |_| {
                                                                        scan_action.dispatch((id, kind_deep.clone(), true));
                                                                    }
                                                                >
                                                                    "Deep Scan"
                                                                </button>
                                                                <button
                                                                    class=style::btn_scan
                                                                    disabled=move || {
                                                                        scanning() || is_locked(id)
                                                                    }
                                                                    on:click=move |_| {
                                                                        scan_action.dispatch((id, kind_scan.clone(), false));
                                                                    }
                                                                >
                                                                    "Scan"
                                                                </button>
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
        </div>
    }
}
