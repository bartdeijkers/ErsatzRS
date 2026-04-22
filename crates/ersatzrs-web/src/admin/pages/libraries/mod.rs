#![allow(clippy::too_many_lines, dead_code)]
use leptos::prelude::*;

use crate::admin::server_fns::libraries::{list_libraries, scan_library, LibraryRow};
use crate::admin::ui::primitives::toast::{use_toast, ToastSeverity};

stylance::import_crate_style!(style, "src/admin/pages/libraries/libraries.module.css");

#[component]
pub fn LibrariesPage() -> impl IntoView {
    let libs: Resource<Vec<LibraryRow>> = Resource::new(
        || (),
        |()| async move { list_libraries().await.unwrap_or_default() },
    );

    let scan_action = Action::new(|(id, kind): &(i64, String)| {
        let id = *id;
        let kind = kind.clone();
        async move { scan_library(id, kind).await }
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
                                                                {is_remote
                                                                    .then(move || {
                                                                        view! {
                                                                            <button
                                                                                class=style::btn_scan
                                                                                disabled=scanning
                                                                                on:click=move |_| {
                                                                                    scan_action
                                                                                        .dispatch((
                                                                                            id,
                                                                                            kind_deep.clone(),
                                                                                        ));
                                                                                }
                                                                            >
                                                                                "Deep Scan"
                                                                            </button>
                                                                        }
                                                                    })}
                                                                <button
                                                                    class=style::btn_scan
                                                                    disabled=scanning
                                                                    on:click=move |_| {
                                                                        scan_action
                                                                            .dispatch((
                                                                                id,
                                                                                kind_scan.clone(),
                                                                            ));
                                                                    }
                                                                >
                                                                    "Scan"
                                                                </button>
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
