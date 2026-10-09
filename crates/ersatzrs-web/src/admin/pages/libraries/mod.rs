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
use crate::admin::ui::primitives::dialog::Modal;
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

// Mirrors Libraries.razor and DeepScanDialog.razor at upstream 007b3415.
// A deep action selects its target; only confirmation dispatches the scan.
#[derive(Clone, Debug, PartialEq, Eq)]
enum DeepScanTarget {
    Library { id: i64, kind: String, name: String },
    Collections { id: i64, kind: String, name: String },
}

impl DeepScanTarget {
    fn name(&self) -> &str {
        match self {
            Self::Library { name, .. } | Self::Collections { name, .. } => name,
        }
    }

    const fn title_key(&self) -> TextKey {
        match self {
            Self::Library { .. } => TextKey::DeepScanLibrary,
            Self::Collections { .. } => TextKey::DeepScanCollections,
        }
    }
}

fn take_deep_scan_confirmation(
    target: RwSignal<Option<DeepScanTarget>>,
    confirmed: bool,
) -> Option<DeepScanTarget> {
    let mut selected = None;
    target.update(|value| selected = value.take());
    if confirmed {
        selected
    } else {
        None
    }
}

#[component]
fn DeepScanConfirmation(
    open: RwSignal<bool>,
    target: RwSignal<Option<DeepScanTarget>>,
    on_close: Callback<bool>,
    confirm_disabled: Signal<bool>,
) -> impl IntoView {
    let title = Signal::derive(move || {
        tr(target
            .get()
            .map_or(TextKey::DeepScan, |value| value.title_key()))
        .to_owned()
    });
    let question = move || {
        target.get().map_or_else(String::new, |value| {
            tr(TextKey::DeepScanConfirmationQuestion).replace("{name}", value.name())
        })
    };
    view! {
        <Modal
            dialog_id="library-deep-scan-confirmation"
            open=open
            title=title
            on_close=on_close
            confirm_label=Some(tr(TextKey::DeepScan))
            cancel_label=Some(tr(TextKey::Cancel))
            confirm_disabled=confirm_disabled
        >
            <p data-deep-scan-target>{question}</p>
            <p>{tr(TextKey::DeepScanConfirmationWarning)}</p>
        </Modal>
    }
}

#[island]
fn LibrariesIsland(
    initial_libraries: Vec<LibraryRow>,
    initial_externals: Vec<ExternalCollectionRow>,
) -> impl IntoView {
    // The hydrated island owns its document-language context independently of SSR.
    #[cfg(target_arch = "wasm32")]
    crate::admin::i18n::provide_i18n_context();
    let interactive = RwSignal::new(false);
    #[cfg(target_arch = "wasm32")]
    Effect::new(move |_| interactive.set(true));
    let deep_scan_open = RwSignal::new(false);
    let deep_scan_target = RwSignal::new(None::<DeepScanTarget>);
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
    let deep_scan_disabled = Signal::derive(move || {
        !interactive.get()
            || match deep_scan_target.get() {
                Some(DeepScanTarget::Library { id, .. }) => scanning() || is_locked(id),
                Some(DeepScanTarget::Collections { id, kind, .. }) => {
                    scanning_collections() || are_collections_locked(&kind, id)
                }
                None => true,
            }
    });
    let finish_deep_scan = Callback::new(move |confirmed: bool| {
        // Consume before dispatch: a second confirmation cannot reuse this target.
        let disabled = deep_scan_disabled.get_untracked();
        match take_deep_scan_confirmation(deep_scan_target, confirmed && !disabled) {
            Some(DeepScanTarget::Library { id, kind, .. }) => {
                scan_action.dispatch((id, kind, true));
            }
            Some(DeepScanTarget::Collections { id, kind, .. }) => {
                collections_action.dispatch((id, kind, true));
            }
            None => {}
        }
    });

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
            <p
                data-library-scan-javascript-required
                class=style::scan_status
                style:display=move || if interactive.get() { "none" } else { "" }
            >
                {tr(TextKey::LibraryScanJavascriptRequired)}
            </p>
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
                                        let kind_scan = kind.clone();
                                        let kind_deep = kind.clone();
                                        let name_deep = lib.name.clone();
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
                                                        <button
                                                            class=style::icon_btn
                                                            disabled=move || !interactive.get() || scanning() || is_locked(id)
                                                            title=tr(TextKey::DeepScanLibrary)
                                                            aria-label=tr(TextKey::DeepScanLibrary)
                                                            on:click=move |_| {
                                                                deep_scan_target.set(Some(DeepScanTarget::Library {
                                                                    id,
                                                                    kind: kind_deep.clone(),
                                                                    name: name_deep.clone(),
                                                                }));
                                                                deep_scan_open.set(true);
                                                            }
                                                        >
                                                            <span
                                                                style=icon_style(IconName::FindReplace, "1.125rem")
                                                                aria-hidden="true"
                                                            />
                                                        </button>
                                                        <button
                                                            class=style::icon_btn
                                                            disabled=move || !interactive.get() || scanning() || is_locked(id)
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
                                                let collections_name = format!("{} {}", library_kind_label(&kind), tr(TextKey::Collections));
                                                let name_deep = if srv.is_empty() {
                                                    collections_name
                                                } else {
                                                    format!("{collections_name} ({srv})")
                                                };
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
                                                                        !interactive.get() || scanning_collections()
                                                                            || are_collections_locked(&kind_lock_a, media_source_id)
                                                                    }
                                                                    title=tr(TextKey::DeepScanCollections)
                                                                    aria-label=tr(TextKey::DeepScanCollections)
                                                                    on:click=move |_| {
                                                                        deep_scan_target.set(Some(DeepScanTarget::Collections {
                                                                            id: media_source_id,
                                                                            kind: kind_deep.clone(),
                                                                            name: name_deep.clone(),
                                                                        }));
                                                                        deep_scan_open.set(true);
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
                                                                        !interactive.get() || scanning_collections()
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
            <DeepScanConfirmation
                open=deep_scan_open
                target=deep_scan_target
                on_close=finish_deep_scan
                confirm_disabled=deep_scan_disabled
            />
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

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;

    #[test]
    fn libraries_deep_scan_empty_ssr_preserves_empty_state() {
        let html = Owner::new().with(|| {
            render_libraries_page_state(Ok(LibrariesPageData {
                libraries: Vec::new(),
                external_collections: Vec::new(),
            }))
            .to_html()
        });
        assert!(html.contains(r#"data-state="empty""#));
        assert!(html.contains(tr(TextKey::NoLibrariesConfigured)));
        assert!(!html.contains("<button"));
        assert!(!html.contains("<form"));
        assert!(!html.contains(r#"role="dialog""#));
    }

    #[test]
    fn libraries_deep_scan_confirmation_cancel_and_duplicate_submit_are_inert() {
        Owner::new().with(|| {
            for target in [
                DeepScanTarget::Library {
                    id: 1,
                    kind: "Local".to_owned(),
                    name: "Local library".to_owned(),
                },
                DeepScanTarget::Collections {
                    id: 2,
                    kind: "Plex".to_owned(),
                    name: "Plex collections".to_owned(),
                },
            ] {
                let selected = RwSignal::new(Some(target.clone()));
                assert_eq!(take_deep_scan_confirmation(selected, false), None);
                assert_eq!(selected.get_untracked(), None);
                selected.set(Some(target.clone()));
                assert_eq!(take_deep_scan_confirmation(selected, true), Some(target));
                assert_eq!(take_deep_scan_confirmation(selected, true), None);
            }
        });
    }

    #[test]
    fn libraries_deep_scan_confirmation_ssr_preserves_target_and_localizes_warning() {
        use crate::admin::i18n::{text, UiLanguage};

        for code in [
            "en-us", "nl", "de-de", "fr-fr", "it-it", "es-es", "es-419", "pt-br", "pl",
        ] {
            Owner::new().with(|| {
                let language = UiLanguage::from_config(code);
                provide_context(language);
                let target = RwSignal::new(Some(DeepScanTarget::Library {
                    id: 1,
                    kind: "Local".to_owned(),
                    name: "Library <script> & {name}".to_owned(),
                }));
                let html = view! {
                    <DeepScanConfirmation
                        open=RwSignal::new(true)
                        target=target
                        on_close=Callback::new(|_: bool| {})
                        confirm_disabled=Signal::from(false)
                    />
                }
                .to_html();
                assert!(html.contains("Library &lt;script&gt; &amp; {name}"));
                assert!(!html.contains("<script>"));
                assert!(html.contains(text(language, TextKey::DeepScanConfirmationWarning)));
                assert!(html.contains(text(language, TextKey::Cancel)));
                assert!(html.contains(text(language, TextKey::DeepScan)));
                assert!(html.contains(r#"aria-labelledby="library-deep-scan-confirmation-title""#));
                assert!(!html.contains("<form"));
            });
        }
    }

    #[test]
    fn libraries_deep_scan_ssr_shows_all_kinds_but_requires_hydration() {
        Owner::new().with(|| {
            let libraries = ["Local", "Plex", "Jellyfin", "Emby"].into_iter().enumerate().map(|(index, kind)| {
                LibraryRow {
                    id: i64::try_from(index + 1).expect("fixture id"),
                    name: format!("Fixture {kind}"),
                    kind: kind.to_owned(),
                    media_kind: 1,
                    source_name: None,
                    media_source_id: 1,
                }
            }).collect();
            let html = view! { <LibrariesIsland initial_libraries=libraries initial_externals=Vec::new()/> }.to_html();
            assert_eq!(html.matches(r#"aria-label="Deep Scan Library""#).count(), 4);
            assert_eq!(html.matches("disabled").count(), 9); // Eight row actions and the closed confirmation.
            assert_eq!(html.matches(r#"role="dialog""#).count(), 1);
            assert!(html.contains("data-library-scan-javascript-required"));
            assert!(html.contains(tr(TextKey::LibraryScanJavascriptRequired)));
            assert!(html.contains("/search?query=library_id%3a1"));
            assert!(!html.contains("<form"));
        });
    }

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
