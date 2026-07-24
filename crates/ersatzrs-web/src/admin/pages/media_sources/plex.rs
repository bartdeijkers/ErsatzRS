#![allow(clippy::too_many_lines, dead_code)]
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::admin::i18n::{
    media_kind_label, media_source_error_message, provider_libraries_title, provider_path_label,
    provider_path_replacements_title, save_failed, tr, TextKey,
};
use crate::admin::server_fns::media_sources::{
    check_plex_pin, has_plex_token, list_plex_libraries, list_plex_paths, list_plex_sources,
    save_plex_library_prefs, save_plex_paths, sign_out_plex, start_plex_pin_flow, LibraryPrefRow,
    PathReplacementRow, PlexSourceRow,
};
use crate::admin::ui::primitives::dialog::DeleteDialog;
use crate::admin::ui::primitives::toast::{load_or_notify, use_toast};

use super::jellyfin::PathsEditor;

stylance::import_crate_style!(style, "src/admin/pages/media_sources/shared.module.css");

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

    Effect::new(move |_| {
        if matches!(sign_out_action.value().get(), Some(Ok(()))) {
            rows.refetch();
            has_token.refetch();
            sign_out_open.set(false);
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
                let show_sign_in = loaded && (sources.is_empty() || !authorized);
                let show_sources = loaded && !sources.is_empty();
                let show_loading = !loaded;

                view! {
                    <p
                        class=style::cell_muted
                        style:display=move || if show_loading { "" } else { "none" }
                    >
                        {tr(TextKey::Loading)}
                    </p>
                    <div style="display:flex;gap:0.75rem;margin-bottom:1rem;flex-wrap:wrap;align-items:center">
                        <div style:display=move || if show_sign_in { "" } else { "none" }>
                            <PlexSignInControls/>
                        </div>
                        <button
                            class=style::btn_danger
                            style:display=move || if show_sign_in { "none" } else { "" }
                            disabled=move || show_sign_in
                            on:click=move |_| sign_out_open.set(true)
                        >
                            {tr(TextKey::SignOutOfPlex)}
                        </button>
                    </div>

                    <p
                        class=style::cell_muted
                        style:display=move || if show_sources { "none" } else { "" }
                    >
                        {tr(TextKey::PlexSignInHelp)}
                    </p>

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
                                view! {
                                    <tr>
                                        <td style="padding:0.5rem;font-size:0.875rem">{r.name.clone()}</td>
                                        <td style="padding:0.5rem;font-size:0.875rem" class=style::cell_muted>{r.address.clone()}</td>
                                        <td style="padding:0.5rem;text-align:right">
                                            <div class=style::row_actions>
                                                <a href=format!("/media/sources/plex/{id}/libraries") class=style::row_btn>{tr(TextKey::Libraries)}</a>
                                                <a href=format!("/media/sources/plex/{id}/paths") class=style::row_btn>{tr(TextKey::Paths)}</a>
                                            </div>
                                        </td>
                                    </tr>
                                }
                            }).collect::<Vec<_>>()}
                        </tbody>
                    </table>
                }
            }}

            <DeleteDialog
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
fn PlexSignInControls() -> impl IntoView {
    #[derive(Clone)]
    enum State {
        Idle,
        Waiting { pin_id: String, auth_url: String },
        Done,
        Error(String),
    }

    let state: RwSignal<State> = RwSignal::new(State::Idle);

    let start_action = Action::new(|()| async move { start_plex_pin_flow().await });

    Effect::new(move |_| {
        match start_action.value().get() {
            Some(Ok((pin_id, auth_url))) => {
                let pin = pin_id.clone();
                state.set(State::Waiting { pin_id, auth_url });

                // Poll every 2 s until Plex confirms the PIN.
                leptos::task::spawn_local(async move {
                    loop {
                        gloo_timers::future::TimeoutFuture::new(2_000).await;
                        match check_plex_pin(pin.clone()).await {
                            Ok(Some(_token)) => {
                                state.set(State::Done);
                                set_browser_location("/media/sources/plex");
                                break;
                            }
                            Ok(None) => {}
                            Err(e) => {
                                state.set(State::Error(e.to_string()));
                                break;
                            }
                        }
                    }
                });
            }
            Some(Err(e)) => state.set(State::Error(e.to_string())),
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
            disabled=move || !matches!(state.get(), State::Idle)
            on:click=move |_| { let _ = start_action.dispatch(()); }
        >
            {tr(TextKey::SignInToPlex)}
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
                disabled=move || !matches!(state.get(), State::Error(_))
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
    let load_toast = use_toast();
    let libs: Resource<Vec<LibraryPrefRow>> = Resource::new(
        move || source_id,
        move |id| async move { load_or_notify(list_plex_libraries(id).await, load_toast) },
    );

    let prefs: RwSignal<Vec<LibraryPrefRow>> = RwSignal::new(vec![]);
    Effect::new(move |_| {
        if let Some(list) = libs.get() {
            prefs.set(list);
        }
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
                style:display=move || if err.get().is_some() { "" } else { "none" }
            >
                {move || err.get().unwrap_or_default()}
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
    let load_toast = use_toast();
    let loaded: Resource<Vec<PathReplacementRow>> = Resource::new(
        move || source_id,
        move |id| async move { load_or_notify(list_plex_paths(id).await, load_toast) },
    );

    let paths: RwSignal<Vec<PathReplacementRow>> = RwSignal::new(vec![]);
    Effect::new(move |_| {
        if let Some(list) = loaded.get() {
            paths.set(list);
        }
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
                style:display=move || if err.get().is_some() { "" } else { "none" }
            >
                {move || err.get().unwrap_or_default()}
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
