use leptos::prelude::*;

use crate::admin::i18n::{library_kind_label, media_kind_label, tr, TextKey};
#[cfg(target_arch = "wasm32")]
use crate::admin::server_fns::layout::media_state_revision;
use crate::admin::server_fns::libraries::{
    list_external_collections, list_libraries, load_libraries_page, scan_external_collections,
    scan_library, ExternalCollectionRow, LibrariesPageData, LibraryRow,
};
#[cfg(target_arch = "wasm32")]
use crate::admin::server_fns::libraries::{
    list_locked_collections, list_locked_libraries, list_scan_progress,
};
use crate::admin::ui::primitives::icon::{icon_style, IconName};
use crate::admin::ui::primitives::resource_state::{
    AdminErrorState, AdminLoadState, AdminLoadingState, AdminResourceState,
};
use crate::admin::ui::primitives::toast::{use_toast, ToastSeverity};

stylance::import_crate_style!(style, "src/admin/pages/libraries/libraries.module.css");

#[component]
pub fn LibrariesPage() -> impl IntoView {
    let initial: Resource<Result<LibrariesPageData, ServerFnError>> =
        Resource::new_blocking(|| (), |()| load_libraries_page());

    view! {
        <Suspense fallback=move || view! {
            <AdminResourceState state=Signal::derive(|| AdminLoadState::Loading)>
                <AdminLoadingState/>
            </AdminResourceState>
        }>
            {move || initial.get().map(render_libraries_page_state)}
        </Suspense>
    }
}

fn render_libraries_page_state(result: Result<LibrariesPageData, ServerFnError>) -> AnyView {
    match result {
        Err(error) => view! {
            <AdminResourceState state=Signal::derive(|| AdminLoadState::Failed)>
                <AdminErrorState error=error.to_string()/>
            </AdminResourceState>
        }
        .into_any(),
        Ok(data) if data.libraries.is_empty() && data.external_collections.is_empty() => view! {
            <AdminResourceState state=Signal::derive(|| AdminLoadState::Empty)>
                <p class=style::empty>{tr(TextKey::NoLibrariesConfigured)}</p>
            </AdminResourceState>
        }
        .into_any(),
        Ok(data) => view! {
            <LibrariesIsland
                initial_libraries=data.libraries
                initial_externals=data.external_collections
            />
        }
        .into_any(),
    }
}

#[island]
fn LibrariesIsland(
    initial_libraries: Vec<LibraryRow>,
    initial_externals: Vec<ExternalCollectionRow>,
) -> impl IntoView {
    let media_revision = RwSignal::new(String::new());
    let libraries_error = RwSignal::new(String::new());
    let externals_error = RwSignal::new(String::new());
    let polling_error = RwSignal::new(String::new());
    let current_libraries = RwSignal::new(initial_libraries);
    let libs: LocalResource<Result<Vec<LibraryRow>, ServerFnError>> =
        LocalResource::new(move || {
            let _ = media_revision.get();
            list_libraries()
        });
    Effect::new(move |_| match libs.get() {
        Some(Ok(rows)) => {
            current_libraries.set(rows);
            libraries_error.set(String::new());
        }
        Some(Err(error)) => libraries_error.set(error.to_string()),
        None => {}
    });
    let scan_action = Action::new(|(id, kind, deep_scan): &(i64, String, bool)| {
        let id = *id;
        let kind = kind.clone();
        let deep_scan = *deep_scan;
        async move { scan_library(id, kind, deep_scan).await }
    });
    let toast = use_toast();
    let scan_status = RwSignal::new(String::new());
    let scan_failed = RwSignal::new(false);
    Effect::new(move |_| {
        if let Some(result) = scan_action.value().get() {
            match result {
                Ok(_) => {
                    scan_status.set(tr(TextKey::ScanQueued).to_string());
                    scan_failed.set(false);
                    toast.push(tr(TextKey::ScanQueued), ToastSeverity::Success);
                }
                Err(error) => {
                    let message = format!("{}: {error}", tr(TextKey::ScanFailed));
                    scan_status.set(message.clone());
                    scan_failed.set(true);
                    toast.push(message, ToastSeverity::Error);
                }
            }
        }
    });

    let current_externals = RwSignal::new(initial_externals);
    let externals: LocalResource<Result<Vec<ExternalCollectionRow>, ServerFnError>> =
        LocalResource::new(move || {
            let _ = media_revision.get();
            list_external_collections()
        });
    Effect::new(move |_| match externals.get() {
        Some(Ok(rows)) => {
            current_externals.set(rows);
            externals_error.set(String::new());
        }
        Some(Err(error)) => externals_error.set(error.to_string()),
        None => {}
    });
    let collections_action =
        Action::new(|(media_source_id, kind, deep_scan): &(i64, String, bool)| {
            let media_source_id = *media_source_id;
            let kind = kind.clone();
            let deep_scan = *deep_scan;
            async move { scan_external_collections(media_source_id, kind, deep_scan).await }
        });
    let collections_toast = use_toast();
    Effect::new(move |_| {
        if let Some(result) = collections_action.value().get() {
            match result {
                Ok(_) => {
                    scan_status.set(tr(TextKey::ScanQueued).to_string());
                    scan_failed.set(false);
                    collections_toast.push(tr(TextKey::ScanQueued), ToastSeverity::Success);
                }
                Err(error) => {
                    let message = format!("{}: {error}", tr(TextKey::ScanFailed));
                    scan_status.set(message.clone());
                    scan_failed.set(true);
                    collections_toast.push(message, ToastSeverity::Error);
                }
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
                    let mut cycle_error = None;
                    match list_locked_libraries().await {
                        Ok(ids) => locked.set(ids),
                        Err(error) => record_polling_error(&mut cycle_error, &error),
                    }
                    match list_locked_collections().await {
                        Ok(pairs) => locked_collections.set(pairs),
                        Err(error) => record_polling_error(&mut cycle_error, &error),
                    }
                    match list_scan_progress().await {
                        Ok(pairs) => scan_progress.set(pairs),
                        Err(error) => record_polling_error(&mut cycle_error, &error),
                    }
                    match media_state_revision().await {
                        Ok(next) => {
                            if media_revision.get_untracked() != next {
                                media_revision.set(next);
                            }
                        }
                        Err(error) => record_polling_error(&mut cycle_error, &error),
                    }
                    finish_polling_cycle(polling_error, cycle_error);
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

    let load_error = Signal::derive(move || {
        [libraries_error, externals_error, polling_error]
            .into_iter()
            .map(|error| error.get())
            .find(|error| !error.is_empty())
            .unwrap_or_default()
    });

    let resource_state = Signal::derive(move || {
        if !load_error.get().is_empty() {
            AdminLoadState::Failed
        } else if current_libraries.get().is_empty() && current_externals.get().is_empty() {
            AdminLoadState::Empty
        } else {
            AdminLoadState::Ready
        }
    });

    view! {
        <AdminResourceState state=resource_state>
        <div class=style::page>
            <h1 class=style::page_title>{tr(TextKey::Libraries)}</h1>
            <div
                role="alert"
                class=style::empty
                style:display=move || if load_error.get().is_empty() { "none" } else { "" }
            >
                <strong>{tr(TextKey::ErrorOccurred)}</strong>
                " "
                {move || load_error.get()}
            </div>
            <p
                role="status"
                class=style::scan_status
                style:display=move || if scan_status.get().is_empty() { "none" } else { "" }
                style:color=move || if scan_failed.get() { "var(--color-error)" } else { "var(--color-success)" }
            >
                {move || scan_status.get()}
            </p>
            {move || {
                let rows = current_libraries.get();
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
            }}
            {move || {
                let rows = current_externals.get();
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
            }}
        </div>
        </AdminResourceState>
    }
}

// Only reachable from the wasm32 polling loop above; a native (non-test) build
// never calls it, which clippy otherwise reports as dead code.
#[allow(dead_code)]
fn record_polling_error(cycle_error: &mut Option<String>, error: &ServerFnError) {
    if cycle_error.is_none() {
        *cycle_error = Some(error.to_string());
    }
}

// Reachable from the wasm32 polling loop above and directly from the test
// below; a native non-test build never calls it, which clippy otherwise
// reports as dead code.
#[allow(dead_code)]
fn finish_polling_cycle(polling_error: RwSignal<String>, cycle_error: Option<String>) {
    polling_error.set(cycle_error.unwrap_or_default());
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

#[cfg(all(test, feature = "ssr", not(feature = "mysql")))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn closed_pool_renders_error_state_without_a_form() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .expect("test pool");
        pool.close().await;

        let error = crate::admin::server_fns::libraries::load_libraries_page_with_pool(&pool)
            .await
            .expect_err("closed pool should fail");
        let transport_error: ServerFnError = error.into();
        let owner = Owner::new();
        let html = owner.with(|| render_libraries_page_state(Err(transport_error)).to_html());

        assert!(html.contains(r#"data-state="error""#));
        assert!(!html.contains("<form"));
    }

    #[test]
    fn successful_polling_cycle_clears_a_transient_error() {
        let owner = Owner::new();
        owner.with(|| {
            let polling_error = RwSignal::new("temporary failure".to_owned());

            finish_polling_cycle(polling_error, None);

            assert_eq!(polling_error.get_untracked(), "");
        });
    }
}
