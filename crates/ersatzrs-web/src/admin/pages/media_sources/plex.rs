use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::admin::i18n::{
    media_kind_label, media_source_error_message, provider_libraries_title, provider_path_label,
    provider_path_replacements_title, save_failed, tr, TextKey,
};
use crate::admin::server_fns::media_sources::{
    check_plex_pin, delete_missing_plex_source, has_plex_token, list_plex_libraries,
    list_plex_paths, list_plex_sources, refresh_plex_libraries, save_plex_library_prefs,
    save_plex_paths, sign_out_plex, start_plex_pin_flow, LibraryPrefRow, PathReplacementRow,
    PlexSourceRow,
};
use crate::admin::ui::primitives::dialog::{DeleteDialog, Modal};
use crate::admin::ui::primitives::toast::{load_failed_message, load_or_notify, use_toast};

use super::jellyfin::PathsEditor;

// `shared.module.css` is shared across the media-source pages (emby, jellyfin,
// local, plex); not every class is used by this particular page.
#[allow(dead_code)]
mod style_mod {
    stylance::import_crate_style!(s, "src/admin/pages/media_sources/shared.module.css");
    pub use s::*;
}
use style_mod as style;

#[cfg(target_arch = "wasm32")]
fn set_browser_location(path: &str) {
    if let Some(window) = web_sys::window() {
        let _ = window.location().set_href(path);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn set_browser_location(_path: &str) {}

// ─── Sources list ─────────────────────────────────────────────────────────────

#[component]
pub fn PlexSourcesPage() -> impl IntoView {
    view! { <PlexSourcesIsland/> }
}

#[island]
fn PlexSourcesIsland() -> impl IntoView {
    let current_sources = RwSignal::new(Vec::<PlexSourceRow>::new());
    let current_authorized = RwSignal::new(false);
    let sources_loaded = RwSignal::new(false);
    let token_loaded = RwSignal::new(false);
    let load_toast = use_toast();
    let rows: LocalResource<Vec<PlexSourceRow>> =
        LocalResource::new(
            move || async move { load_or_notify(list_plex_sources().await, load_toast) },
        );
    let has_token: LocalResource<bool> =
        LocalResource::new(
            move || async move { load_or_notify(has_plex_token().await, load_toast) },
        );
    Effect::new(move |_| {
        if let Some(next_rows) = rows.get() {
            current_sources.set(next_rows);
            sources_loaded.set(true);
        }
    });
    Effect::new(move |_| {
        if let Some(next_authorized) = has_token.get() {
            current_authorized.set(next_authorized);
            token_loaded.set(true);
        }
    });

    let sign_out_action = Action::new(|()| sign_out_plex());
    let sign_out_open = RwSignal::new(false);
    let remove_open = RwSignal::new(false);
    let remove_target = RwSignal::new((0_i64, String::new()));
    let remove_action = Action::new(|id: &i64| delete_missing_plex_source(*id));
    let remove_error = RwSignal::new(String::new());
    let authenticating = RwSignal::new(false);
    let refresh_action = Action::new(|id: &i64| refresh_plex_libraries(*id));
    Effect::new(move |_| match refresh_action.value().get() {
        Some(Ok(())) => {
            rows.refetch();
            remove_error.set(String::new());
        }
        Some(Err(_)) => remove_error.set(tr(TextKey::PlexRefreshFailed).to_owned()),
        None => {}
    });
    Effect::new(move |_| match remove_action.value().get() {
        Some(Ok(())) => {
            rows.refetch();
            remove_error.set(String::new());
        }
        Some(Err(_)) => {
            remove_error.set(tr(TextKey::RemoveFailed).to_owned());
            rows.refetch();
        }
        None => {}
    });

    Effect::new(move |_| {
        if matches!(sign_out_action.value().get(), Some(Ok(()))) {
            rows.refetch();
            has_token.refetch();
            sign_out_open.set(false);
        } else if matches!(sign_out_action.value().get(), Some(Err(_))) {
            remove_error.set(tr(TextKey::RemoveFailed).to_owned());
        }
    });

    view! {
        <div class=style::page>
            <div class=style::toolbar>
                <h1 class=style::page_title>"Plex"</h1>
            </div>

            {move || {
                let sources = current_sources.get();
                let authorized = current_authorized.get();
                let loaded = sources_loaded.get() && token_loaded.get();
                let show_sources = loaded && !sources.is_empty();
                let show_missing = sources.iter().any(|source| source.missing_since.is_some());
                let show_loading = !loaded;

                view! {
                    <p
                        class=style::cell_muted
                        style:display=move || if show_loading { "" } else { "none" }
                    >
                        {tr(TextKey::Loading)}
                    </p>
                    <div style="display:flex;gap:0.75rem;margin-bottom:1rem;flex-wrap:wrap;align-items:center">
                        <div style:display=move || if loaded { "" } else { "none" }>
                            <PlexSignInControls force_new_credentials=authorized || show_sources
                                busy=authenticating
                                blocked=Signal::derive(move || refresh_action.pending().get()
                                    || remove_action.pending().get() || sign_out_action.pending().get())/>
                        </div>
                        <button
                            class=style::btn_danger
                            style:display=move || if loaded && (authorized || show_sources) { "" } else { "none" }
                            disabled=move || !loaded || authenticating.get() || sign_out_action.pending().get()
                            on:click=move |_| sign_out_open.set(true)
                        >
                            {tr(TextKey::SignOutOfPlex)}
                        </button>
                    </div>

                    <p class=style::error_msg role="status"
                        style:display=move || if show_sources && !authorized { "" } else { "none" }>
                        {tr(TextKey::PlexSignedOutHelp)}
                    </p>
                    <p role="status"
                        style:display=move || if show_missing { "" } else { "none" }>
                        {tr(TextKey::PlexMissingHelp)}
                    </p>

                    <p
                        class=style::cell_muted
                        style:display=move || if show_sources { "none" } else { "" }
                    >
                        {tr(TextKey::PlexSignInHelp)}
                    </p>

                    <div style="overflow-x:auto">
                    <table
                        style:display=move || if show_sources { "" } else { "none" }
                        style="width:100%;border-collapse:collapse"
                    >
                        <thead>
                            <tr>
                                <th style="text-align:left;padding:0.5rem;font-weight:600;font-size:0.875rem">{tr(TextKey::Name)}</th>
                                <th style="text-align:left;padding:0.5rem;font-weight:600;font-size:0.875rem">{tr(TextKey::Address)}</th>
                                <th/>
                            </tr>
                        </thead>
                        <tbody>
                            {sources.into_iter().map(|r| {
                                let id = r.id;
                                let missing = r.missing_since.is_some();
                                let name = r.name.clone();
                                view! {
                                    <tr>
                                        <td style="padding:0.5rem;font-size:0.875rem">{r.name.clone()}
                                            <p style:display=move || if missing { "" } else { "none" }
                                                title=tr(TextKey::PlexMissingHelp)>{tr(TextKey::PlexMissingServer)}</p>
                                        </td>
                                        <td style="padding:0.5rem;font-size:0.875rem;overflow-wrap:anywhere" class=style::cell_muted>{r.address.clone()}</td>
                                        <td style="padding:0.5rem;text-align:right">
                                            <div class=style::row_actions style="flex-wrap:wrap">
                                                <button class=style::row_btn
                                                    disabled=move || missing || !authorized || authenticating.get() || refresh_action.pending().get()
                                                    on:click=move |_| { refresh_action.dispatch(id); }>
                                                    {tr(TextKey::Refresh)}
                                                </button>
                                                <a href=format!("/media/sources/plex/{id}/libraries") class=style::row_btn>{tr(TextKey::Libraries)}</a>
                                                <a href=format!("/media/sources/plex/{id}/paths") class=style::row_btn>{tr(TextKey::Paths)}</a>
                                                <button class=style::btn_danger
                                                    style:display=move || if missing { "" } else { "none" }
                                                    disabled=move || !missing || authenticating.get() || remove_action.pending().get()
                                                    on:click=move |_| { remove_target.set((id, name.clone())); remove_open.set(true); }>
                                                    {tr(TextKey::Delete)}
                                                </button>
                                            </div>
                                        </td>
                                    </tr>
                                }
                            }).collect::<Vec<_>>()}
                        </tbody>
                    </table>
                    </div>
                }
            }}

            <p class=style::error_msg role="alert">{move || remove_error.get()}</p>
            <Modal dialog_id="plex-remove-server-dialog" open=remove_open
                title=tr(TextKey::ConfirmDelete).to_string()
                confirm_label=Some(tr(TextKey::Delete)) destructive=true
                on_close=Callback::new(move |confirmed: bool| {
                    if confirmed { remove_action.dispatch(remove_target.get().0); }
                })>
                <p><strong>{move || remove_target.get().1}</strong></p>
                <p>{tr(TextKey::PlexRemoveServerHelp)}</p>
            </Modal>

            <DeleteDialog
                dialog_id="plex-sign-out-dialog"
                open=sign_out_open
                entity_name=Signal::derive(|| "Plex".to_string())
                on_confirm=Callback::new(move |confirmed: bool| {
                    if confirmed { sign_out_action.dispatch(()); }
                    else { sign_out_open.set(false); }
                })
            />
        </div>
    }
}

// ─── Plex sign-in controls ────────────────────────────────────────────────────

/// Island for the Plex OAuth PIN flow (no web_sys — uses navigate on success).
///
/// Calls `start_plex_pin_flow`, renders the plex.tv URL as a link the user
/// clicks, then polls `check_plex_pin` every 2 s via `spawn_local` +
/// `gloo-timers`. On success navigates to `/media/sources/plex` to reload.
#[component]
fn PlexSignInControls(
    force_new_credentials: bool,
    busy: RwSignal<bool>,
    blocked: Signal<bool>,
) -> impl IntoView {
    #[derive(Clone)]
    enum State {
        Idle,
        Waiting { auth_url: String },
        Done,
        Error(String),
    }

    let state: RwSignal<State> = RwSignal::new(State::Idle);
    let active = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let cleanup_active = active.clone();
    on_cleanup(move || cleanup_active.store(false, std::sync::atomic::Ordering::Relaxed));

    let start_action =
        Action::new(move |()| async move { start_plex_pin_flow(force_new_credentials).await });
    Effect::new(move |_| {
        busy.set(start_action.pending().get() || matches!(state.get(), State::Waiting { .. }));
    });

    Effect::new(move |_| {
        match start_action.value().get() {
            Some(Ok((pin_id, auth_url))) => {
                let pin = pin_id.clone();
                let active = active.clone();
                state.set(State::Waiting { auth_url });

                // Poll every 2 s until Plex confirms the PIN.
                leptos::task::spawn_local(async move {
                    loop {
                        gloo_timers::future::TimeoutFuture::new(2_000).await;
                        if !active.load(std::sync::atomic::Ordering::Relaxed) {
                            break;
                        }
                        let result = check_plex_pin(pin.clone()).await;
                        if !active.load(std::sync::atomic::Ordering::Relaxed) {
                            break;
                        }
                        match result {
                            Ok(true) => {
                                state.set(State::Done);
                                set_browser_location("/media/sources/plex");
                                break;
                            }
                            Ok(false) => {}
                            Err(_) => {
                                state.set(State::Error(tr(TextKey::PlexSignInFailed).to_owned()));
                                break;
                            }
                        }
                    }
                });
            }
            Some(Err(_)) => state.set(State::Error(tr(TextKey::PlexSignInFailed).to_owned())),
            None => {}
        }
    });

    // All four state variants are always rendered; only the matching one is visible.
    // This gives a stable DOM shape on SSR and client hydration.
    view! {
        // Idle: sign-in button
        <button
            class=style::btn_primary
            style:display=move || if matches!(state.get(), State::Idle) { "" } else { "none" }
            disabled=move || !matches!(state.get(), State::Idle) || start_action.pending().get() || blocked.get()
            on:click=move |_| { let _ = start_action.dispatch(()); }
        >
            {tr(if force_new_credentials { TextKey::ReauthenticatePlex } else { TextKey::SignInToPlex })}
        </button>

        // Waiting: authorization link + polling message
        <div
            class=style::pin_waiting
            style:display=move || if matches!(state.get(), State::Waiting { .. }) { "" } else { "none" }
        >
            <p>
                {tr(TextKey::WaitingForPlexAuthorization)} " "
                <a
                    href=move || match state.get() {
                        State::Waiting { auth_url, .. } => auth_url,
                        _ => String::new(),
                    }
                    target="_blank"
                    class=style::pin_link
                >
                    {tr(TextKey::ClickAuthorizePlex)}
                </a>
            </p>
            <p class=style::cell_muted>{tr(TextKey::CheckingEveryTwoSeconds)}</p>
        </div>

        // Done: success message
        <p
            class=style::success_msg
            style:display=move || if matches!(state.get(), State::Done) { "" } else { "none" }
        >
            {tr(TextKey::AuthorizedRedirecting)}
        </p>

        // Error: error message + retry button
        <div
            style:display=move || if matches!(state.get(), State::Error(_)) { "" } else { "none" }
        >
            <p class=style::error_msg>
                {tr(TextKey::Error)} ": "
                {move || match state.get() {
                    State::Error(msg) => msg,
                    _ => String::new(),
                }}
            </p>
            <button
                class=style::btn_secondary
                disabled=move || !matches!(state.get(), State::Error(_)) || blocked.get()
                on:click=move |_| {
                    state.set(State::Idle);
                    let _ = start_action.dispatch(());
                }
            >
                {tr(TextKey::Retry)}
            </button>
        </div>
    }
}

// ─── Libraries editor ─────────────────────────────────────────────────────────

#[component]
pub fn PlexLibrariesPage() -> impl IntoView {
    let params = use_params_map();
    let source_id = params.with(|p| p.get("id").and_then(|s| s.parse::<i64>().ok()).unwrap_or(0));
    view! { <PlexLibrariesIsland source_id=source_id/> }
}

#[island]
fn PlexLibrariesIsland(source_id: i64) -> impl IntoView {
    let libs: Resource<Result<Vec<LibraryPrefRow>, String>> = Resource::new(
        move || source_id,
        move |id| async move { list_plex_libraries(id).await.map_err(load_failed_message) },
    );
    // A failed load must stay visible and must not offer to persist an empty
    // list over real data. The toast channel cannot do that here: this loader
    // runs during SSR, where `ToastStore::push` is inert by construction
    // (hydration-lint rule-3).
    let load_error: RwSignal<Option<String>> = RwSignal::new(None);

    let prefs: RwSignal<Vec<LibraryPrefRow>> = RwSignal::new(vec![]);
    Effect::new(move |_| match libs.get() {
        Some(Ok(list)) => {
            load_error.set(None);
            prefs.set(list);
        }
        Some(Err(message)) => load_error.set(Some(message)),
        None => {}
    });

    let err: RwSignal<Option<String>> = RwSignal::new(None);
    let saved = RwSignal::new(false);

    let save_action = Action::new(move |p: &Vec<LibraryPrefRow>| {
        let p = p.clone();
        async move { save_plex_library_prefs(p).await }
    });

    Effect::new(move |_| match save_action.value().get() {
        Some(Ok(())) => saved.set(true),
        Some(Err(e)) => err.set(Some(save_failed(media_source_error_message(e)))),
        None => {}
    });

    view! {
        <div class=style::page>
            <div class=style::toolbar>
                <div>
                    <a href="/media/sources/plex" class=style::back_link>"← Plex"</a>
                    <h1 class=style::page_title>{provider_libraries_title("Plex")}</h1>
                </div>
                <div class=style::toolbar_actions>
                    <button
                        class=style::btn_primary
                        disabled=move || load_error.get().is_some()
                        on:click=move |_| {
                            saved.set(false);
                            save_action.dispatch(prefs.get_untracked());
                        }
                    >{tr(TextKey::Save)}</button>
                </div>
            </div>

            <Suspense fallback=|| view! { <p class=style::cell_muted>{tr(TextKey::Loading)}</p> }>
                {move || {
                    let list = prefs.get();
                    view! {
                        <table style="width:100%;border-collapse:collapse">
                            <thead>
                                <tr>
                                    <th style="text-align:left;padding:0.5rem;font-weight:600;font-size:0.875rem">{tr(TextKey::Library)}</th>
                                    <th style="text-align:left;padding:0.5rem;font-weight:600;font-size:0.875rem">{tr(TextKey::Kind)}</th>
                                    <th style="text-align:center;padding:0.5rem;font-weight:600;font-size:0.875rem">{tr(TextKey::Sync)}</th>
                                </tr>
                            </thead>
                            <tbody>
                                {list.into_iter().enumerate().map(|(i, lib)| {
                                    view! {
                                        <tr>
                                            <td style="padding:0.5rem;font-size:0.875rem">{lib.name.clone()}</td>
                                            <td style="padding:0.5rem;font-size:0.875rem" class=style::cell_muted>
                                                {media_kind_label(i64::from(lib.media_kind))}
                                            </td>
                                            <td style="padding:0.5rem;text-align:center">
                                                <input
                                                    type="checkbox"
                                                    prop:checked=move || prefs.get().get(i).is_some_and(|p| p.should_sync)
                                                    on:change=move |e| {
                                                        prefs.update(|list| {
                                                            if let Some(p) = list.get_mut(i) {
                                                                p.should_sync = event_target_checked(&e);
                                                            }
                                                        });
                                                    }
                                                />
                                            </td>
                                        </tr>
                                    }
                                }).collect::<Vec<_>>()}
                            </tbody>
                        </table>
                    }
                }}
            </Suspense>

            <p
                class=style::form_error
                style:display=move || {
                    if load_error.get().or_else(|| err.get()).is_some() { "" } else { "none" }
                }
            >
                {move || load_error.get().or_else(|| err.get()).unwrap_or_default()}
            </p>
            <p
                class=style::success_msg
                style:display=move || if saved.get() { "" } else { "none" }
            >
                {tr(TextKey::Saved)}
            </p>
        </div>
    }
}

// ─── Path replacements editor ─────────────────────────────────────────────────

#[component]
pub fn PlexPathsPage() -> impl IntoView {
    let params = use_params_map();
    let source_id = params.with(|p| p.get("id").and_then(|s| s.parse::<i64>().ok()).unwrap_or(0));
    view! { <PlexPathsIsland source_id=source_id/> }
}

#[island]
fn PlexPathsIsland(source_id: i64) -> impl IntoView {
    let loaded: Resource<Result<Vec<PathReplacementRow>, String>> = Resource::new(
        move || source_id,
        move |id| async move { list_plex_paths(id).await.map_err(load_failed_message) },
    );
    // A failed load must stay visible and must not offer to persist an empty
    // list over real data. The toast channel cannot do that here: this loader
    // runs during SSR, where `ToastStore::push` is inert by construction
    // (hydration-lint rule-3).
    let load_error: RwSignal<Option<String>> = RwSignal::new(None);

    let paths: RwSignal<Vec<PathReplacementRow>> = RwSignal::new(vec![]);
    Effect::new(move |_| match loaded.get() {
        Some(Ok(list)) => {
            load_error.set(None);
            paths.set(list);
        }
        Some(Err(message)) => load_error.set(Some(message)),
        None => {}
    });

    let new_remote = RwSignal::new(String::new());
    let new_local = RwSignal::new(String::new());
    let err: RwSignal<Option<String>> = RwSignal::new(None);
    let saved = RwSignal::new(false);

    let save_action = Action::new(move |args: &(i64, Vec<PathReplacementRow>)| {
        let (id, list) = args.clone();
        async move { save_plex_paths(id, list).await }
    });

    Effect::new(move |_| match save_action.value().get() {
        Some(Ok(())) => saved.set(true),
        Some(Err(e)) => err.set(Some(save_failed(media_source_error_message(e)))),
        None => {}
    });

    let on_add = move |_: leptos::ev::MouseEvent| {
        let r = new_remote.get_untracked().trim().to_string();
        let l = new_local.get_untracked().trim().to_string();
        if r.is_empty() || l.is_empty() {
            return;
        }
        paths.update(|list| {
            list.push(PathReplacementRow {
                id: 0,
                remote_path: r,
                local_path: l,
            });
        });
        new_remote.set(String::new());
        new_local.set(String::new());
    };

    view! {
        <div class=style::page>
            <div class=style::toolbar>
                <div>
                    <a href="/media/sources/plex" class=style::back_link>"← Plex"</a>
                    <h1 class=style::page_title>{provider_path_replacements_title("Plex")}</h1>
                </div>
                <div class=style::toolbar_actions>
                    <button
                        class=style::btn_primary
                        disabled=move || load_error.get().is_some()
                        on:click=move |_| {
                            saved.set(false);
                            save_action.dispatch((source_id, paths.get_untracked()));
                        }
                    >{tr(TextKey::Save)}</button>
                </div>
            </div>

            <PathsEditor
                paths=paths
                new_remote=new_remote
                new_local=new_local
                on_add=on_add
                remote_label=provider_path_label("Plex")
            />

            <p
                class=style::form_error
                style:display=move || {
                    if load_error.get().or_else(|| err.get()).is_some() { "" } else { "none" }
                }
            >
                {move || load_error.get().or_else(|| err.get()).unwrap_or_default()}
            </p>
            <p
                class=style::success_msg
                style:display=move || if saved.get() { "" } else { "none" }
            >
                {tr(TextKey::Saved)}
            </p>
        </div>
    }
}
