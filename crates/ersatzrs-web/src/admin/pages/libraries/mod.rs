#![allow(clippy::too_many_lines, dead_code)]
use leptos::prelude::*;

use crate::admin::i18n::{library_kind_label, media_kind_label, tr, TextKey};
use crate::admin::server_fns::libraries::{
    list_external_collections, list_libraries, scan_external_collections, scan_library,
    ExternalCollectionRow, LibraryRow,
};
#[cfg(target_arch = "wasm32")]
use crate::admin::server_fns::libraries::{
    list_locked_collections, list_locked_libraries, list_scan_progress,
};
use crate::admin::ui::primitives::icon::{icon_style, IconName};
use crate::admin::ui::primitives::toast::{use_toast, ToastContainer, ToastSeverity};

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
                Ok(()) => toast.push(tr(TextKey::ScanQueued).to_owned(), ToastSeverity::Success),
                Err(e) => toast.push(
                    format!("{}: {e}", tr(TextKey::ScanFailed)),
                    ToastSeverity::Error,
                ),
            }
        }
    });
    Effect::new(move |_| {
        if let Some(res) = collections_action.value().get() {
            match res {
                Ok(()) => toast.push(
                    tr(TextKey::CollectionsScanQueued).to_owned(),
                    ToastSeverity::Success,
                ),
                Err(e) => toast.push(
                    format!("{}: {e}", tr(TextKey::CollectionsScanFailed)),
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
    let scan_progress: RwSignal<Vec<(i64, f32)>> = RwSignal::new(Vec::new());
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
                    if let Ok(pairs) = list_scan_progress().await {
                        scan_progress.set(pairs);
                    }
                    gloo_timers::future::TimeoutFuture::new(1_500).await;
                }
            });
        }
    });
    let is_locked = move |id: i64| locked.with(|ids| ids.contains(&id));
    let progress_for = move |id: i64| {
        scan_progress.with(|pairs| {
            pairs
                .iter()
                .find(|(library_id, _)| *library_id == id)
                .map(|(_, fraction)| *fraction)
        })
    };
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
            <h1 class=style::page_title>{tr(TextKey::Libraries)}</h1>
            <Suspense fallback=move || {
                view! { <p class=style::empty>{tr(TextKey::Loading)}</p> }
            }>
                {move || {
                    libs.get().map(|rows| {
                        if rows.is_empty() {
                            return view! {
                                <p class=style::empty>
                                    {tr(TextKey::NoLibrariesConfigured)}
                                </p>
                            }
                            .into_any();
                        }

                        view! {
                            <div class=style::table_wrap>
                                <table>
                                    <thead>
                                        <tr>
                                            <th>{tr(TextKey::Kind)}</th>
                                            <th>{tr(TextKey::Server)}</th>
                                            <th>{tr(TextKey::Name)}</th>
                                            <th>{tr(TextKey::MediaKind)}</th>
                                            <th/>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {rows
                                            .into_iter()
                                            .map(|lib| {
                                                let id = lib.id;
                                                let kind = lib.kind.clone();
                                                let display_kind = library_kind_label(&kind);
                                                let display_media_kind = media_kind_label(lib.media_kind);
                                                let is_remote = kind != "Local";
                                                let kind_scan = kind.clone();
                                                let kind_deep = kind.clone();
                                                let srv = lib.source_name.clone().unwrap_or_default();
                                                let search_href = format!("/search?query=library_id%3a{id}");
                                                view! {
                                                    <tr>
                                                        <td>{display_kind}</td>
                                                        <td class=style::cell_muted>{srv}</td>
                                                        <td>{lib.name.clone()}</td>
                                                        <td class=style::cell_muted>{display_media_kind}</td>
                                                        <td>
                                                            <div class=style::actions>
                                                                // Scan indicator goes first (left) so it occupies the
                                                                // free space at the start of the right-justified action
                                                                // row instead of pushing the icon column wider on the
                                                                // right. Mirrors ETV's Libraries.razor, where the
                                                                // percent + spinner appear before the trailing controls.
                                                                <ScanIndicator
                                                                    visible=Signal::derive(move || is_locked(id))
                                                                    fraction=Signal::derive(move || progress_for(id))
                                                                />
                                                                {if is_remote {
                                                                    view! {
                                                                        <button
                                                                            class=style::icon_btn
                                                                            disabled=move || scanning() || is_locked(id)
                                                                            title=tr(TextKey::DeepScanLibrary)
                                                                            aria-label=tr(TextKey::DeepScanLibrary)
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
                                                                    title=tr(TextKey::ScanLibrary)
                                                                    aria-label=tr(TextKey::ScanLibrary)
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
                                                                    title=tr(TextKey::SearchMedia)
                                                                    aria-label=tr(TextKey::SearchMedia)
                                                                >
                                                                    <span
                                                                        style=icon_style(IconName::Search, "1.125rem")
                                                                        aria-hidden="true"
                                                                    />
                                                                </a>
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
                            <h2 class=style::section_title>{tr(TextKey::ExternalCollections)}</h2>
                            <div class=style::table_wrap>
                                <table>
                                    <thead>
                                        <tr>
                                            <th>{tr(TextKey::LibraryKind)}</th>
                                            <th>{tr(TextKey::Server)}</th>
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
                                                                <ScanIndicator
                                                                    visible=Signal::derive(move || are_collections_locked(&kind_lock_c, media_source_id))
                                                                    fraction=Signal::derive(|| None)
                                                                />
                                                                <button
                                                                    class=style::icon_btn
                                                                    disabled=move || {
                                                                        scanning_collections()
                                                                            || are_collections_locked(&kind_lock_a, media_source_id)
                                                                    }
                                                                    title=tr(TextKey::DeepScanCollections)
                                                                    aria-label=tr(TextKey::DeepScanCollections)
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
                                                                    title=tr(TextKey::ScanCollections)
                                                                    aria-label=tr(TextKey::ScanCollections)
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
            <ToastContainer/>
        </div>
    }
}

/// Circular scan indicator: when `fraction` is `Some(f)`, draws a determinate
/// ring with a percentage label; otherwise spins an indeterminate ring.
/// Mirrors `MudProgressCircular` on the ErsatzTV libraries page.
///
/// Always rendered (so DOM shape stays stable across hydration); `visible`
/// toggles `display`.
#[component]
fn ScanIndicator(visible: Signal<bool>, fraction: Signal<Option<f32>>) -> impl IntoView {
    // Ring geometry: radius 8 inside a 20x20 viewBox; circumference ~50.27.
    const CIRCUMFERENCE: f32 = std::f32::consts::TAU * 8.0;

    let display = move || if visible.get() { "inline-flex" } else { "none" };
    let is_determinate = move || fraction.get().is_some();
    let arc_class = move || {
        if is_determinate() {
            style::ring_arc_determinate
        } else {
            style::ring_arc_indeterminate
        }
    };
    let dash_array = move || {
        if is_determinate() {
            format!("{CIRCUMFERENCE}")
        } else {
            // Indeterminate: render ~25% of the ring as a moving arc.
            format!("{} {}", CIRCUMFERENCE * 0.25, CIRCUMFERENCE)
        }
    };
    let dash_offset = move || match fraction.get() {
        Some(f) => (1.0 - f.clamp(0.0, 1.0)) * CIRCUMFERENCE,
        None => 0.0,
    };
    let label = move || match fraction.get() {
        Some(f) => {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let pct = (f.clamp(0.0, 1.0) * 100.0).round() as u32;
            format!("{pct} %")
        }
        None => String::new(),
    };

    view! {
        <span class=style::ring style:display=display title=tr(TextKey::Scanning) aria-label=tr(TextKey::Scanning)>
            <svg viewBox="0 0 20 20" class=style::ring_svg aria-hidden="true">
                <circle cx="10" cy="10" r="8" class=style::ring_track/>
                <circle
                    cx="10"
                    cy="10"
                    r="8"
                    class=arc_class
                    stroke-dasharray=dash_array
                    stroke-dashoffset=dash_offset
                />
            </svg>
            <span class=style::ring_label>{label}</span>
        </span>
    }
}
