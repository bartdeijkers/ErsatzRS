#![allow(clippy::too_many_lines, dead_code)]
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::admin::i18n::{
    media_kind_label, media_source_error_message, no_media_source_configured,
    provider_connection_title, provider_libraries_title, provider_path_label,
    provider_path_replacements_title, save_failed, tr, TextKey,
};
use crate::admin::server_fns::media_sources::{
    delete_jellyfin_source, get_jellyfin_connection, list_jellyfin_libraries, list_jellyfin_paths,
    list_jellyfin_sources, save_jellyfin_connection, save_jellyfin_library_prefs,
    save_jellyfin_paths, ConnectionForm, LibraryPrefRow, PathReplacementRow, RemoteSourceRow,
};
use crate::admin::ui::primitives::dialog::DeleteDialog;

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
pub fn JellyfinSourcesPage() -> impl IntoView {
    view! { <JellyfinSourcesIsland/> }
}

#[island]
fn JellyfinSourcesIsland() -> impl IntoView {
    let rows: Resource<Vec<RemoteSourceRow>> = Resource::new(
        || (),
        |()| async { list_jellyfin_sources().await.unwrap_or_default() },
    );

    let delete_action = Action::new(|id: &i64| delete_jellyfin_source(*id));
    let delete_target: RwSignal<Option<RemoteSourceRow>> = RwSignal::new(None);
    let delete_open = RwSignal::new(false);

    Effect::new(move |_| {
        if matches!(delete_action.value().get(), Some(Ok(()))) {
            rows.refetch();
            delete_open.set(false);
        }
    });

    view! {
        <div class=style::page>
            <div class=style::toolbar>
                <h1 class=style::page_title>"Jellyfin"</h1>
                <div class=style::toolbar_actions>
                    <a href="/media/sources/jellyfin/edit" class=style::btn_primary>
                        {tr(TextKey::ConfigureConnection)}
                    </a>
                </div>
            </div>

            <Suspense fallback=|| view! { <p class=style::cell_muted>{tr(TextKey::Loading)}</p> }>
                {move || {
                    let list = rows.get().unwrap_or_default();
                    if list.is_empty() {
                        view! {
                            <p class=style::cell_muted>
                                {no_media_source_configured("Jellyfin")}
                            </p>
                        }.into_any()
                    } else {
                        view! {
                            <table style="width:100%;border-collapse:collapse">
                                <thead>
                                    <tr>
                                        <th style="text-align:left;padding:0.5rem;font-weight:600;font-size:0.875rem">{tr(TextKey::Name)}</th>
                                        <th style="text-align:left;padding:0.5rem;font-weight:600;font-size:0.875rem">{tr(TextKey::Address)}</th>
                                        <th/>
                                    </tr>
                                </thead>
                                <tbody>
                                    {list.into_iter().map(|r| {
                                        let id = r.id;
                                        let r_clone = r.clone();
                                        view! {
                                            <tr>
                                                <td style="padding:0.5rem;font-size:0.875rem">{r.name.clone()}</td>
                                                <td style="padding:0.5rem;font-size:0.875rem" class=style::cell_muted>{r.address.clone()}</td>
                                                <td style="padding:0.5rem;text-align:right">
                                                    <div class=style::row_actions>
                                                        <a href=format!("/media/sources/jellyfin/{id}/libraries") class=style::row_btn>{tr(TextKey::Libraries)}</a>
                                                        <a href=format!("/media/sources/jellyfin/{id}/paths") class=style::row_btn>{tr(TextKey::Paths)}</a>
                                                        <a href="/media/sources/jellyfin/edit" class=style::row_btn>{tr(TextKey::Edit)}</a>
                                                        <button
                                                            class=style::row_btn_del
                                                            on:click=move |_| {
                                                                delete_target.set(Some(r_clone.clone()));
                                                                delete_open.set(true);
                                                            }
                                                        >{tr(TextKey::Delete)}</button>
                                                    </div>
                                                </td>
                                            </tr>
                                        }
                                    }).collect::<Vec<_>>()}
                                </tbody>
                            </table>
                        }.into_any()
                    }
                }}
            </Suspense>

            <DeleteDialog
                open=delete_open
                entity_name=Signal::derive(move || {
                    delete_target.get().map(|r| r.name.clone()).unwrap_or_default()
                })
                on_confirm=Callback::new(move |confirmed: bool| {
                    if let Some(r) = delete_target.get_untracked() {
                        if confirmed { delete_action.dispatch(r.id); }
                        else { delete_open.set(false); }
                    } else {
                        delete_open.set(false);
                    }
                })
            />
        </div>
    }
}

// ─── Connection editor ────────────────────────────────────────────────────────

#[component]
pub fn JellyfinConnectionPage() -> impl IntoView {
    view! { <JellyfinConnectionEditorIsland/> }
}

#[island]
fn JellyfinConnectionEditorIsland() -> impl IntoView {
    let initial: Resource<ConnectionForm> = Resource::new_blocking(
        || (),
        |()| async { get_jellyfin_connection().await.unwrap_or_default() },
    );

    view! {
        <Suspense fallback=|| view! { <p class=style::cell_muted>{tr(TextKey::Loading)}</p> }>
            {move || {
                initial.get().map(|form| view! {
                    <JellyfinConnectionEditorForm initial_form=form/>
                })
            }}
        </Suspense>
    }
}

#[component]
fn JellyfinConnectionEditorForm(initial_form: ConnectionForm) -> impl IntoView {
    let source_id = initial_form.source_id;
    let address = RwSignal::new(initial_form.address);
    let api_key = RwSignal::new(initial_form.api_key);
    let err: RwSignal<Option<String>> = RwSignal::new(None);
    let saving = RwSignal::new(false);

    let save_action = Action::new(move |form: &ConnectionForm| {
        let form = form.clone();
        async move { save_jellyfin_connection(form).await }
    });

    Effect::new(move |_| match save_action.value().get() {
        Some(Ok(_)) => {
            saving.set(false);
            set_browser_location("/media/sources/jellyfin");
        }
        Some(Err(e)) => {
            saving.set(false);
            err.set(Some(save_failed(media_source_error_message(e))));
        }
        None => {}
    });

    let on_save = move |_| {
        let a = address.get_untracked().trim().to_string();
        let k = api_key.get_untracked().trim().to_string();
        if a.is_empty() {
            err.set(Some(tr(TextKey::AddressRequired).to_owned()));
            return;
        }
        if k.is_empty() {
            err.set(Some(tr(TextKey::ApiKeyRequired).to_owned()));
            return;
        }
        err.set(None);
        saving.set(true);
        save_action.dispatch(ConnectionForm {
            source_id,
            address: a,
            api_key: k,
        });
    };

    view! {
        <div class=style::page>
            <div class=style::toolbar>
                <div>
                    <a href="/media/sources/jellyfin" class=style::back_link>"← Jellyfin"</a>
                    <h1 class=style::page_title>{provider_connection_title("Jellyfin")}</h1>
                </div>
                <div class=style::toolbar_actions>
                    <button
                        class=style::btn_primary
                        on:click=on_save
                        disabled=move || saving.get()
                    >
                        {move || if saving.get() { tr(TextKey::Saving) } else { tr(TextKey::Save) }}
                    </button>
                </div>
            </div>

            <div class=style::editor_form>
                <div class=style::form_row>
                    <label class=style::form_label>{tr(TextKey::Address)}</label>
                    <input
                        class=style::form_input
                        type="url"
                        placeholder="http://jellyfin:8096"
                        prop:value=move || address.get()
                        on:input=move |e| address.set(event_target_value(&e))
                    />
                </div>
                <div class=style::form_row>
                    <label class=style::form_label>{tr(TextKey::ApiKey)}</label>
                    <input
                        class=style::form_input
                        type="password"
                        prop:value=move || api_key.get()
                        on:input=move |e| api_key.set(event_target_value(&e))
                    />
                </div>
                <span
                    class=style::form_error
                    style:display=move || if err.get().is_some() { "" } else { "none" }
                >
                    {move || err.get().unwrap_or_default()}
                </span>
            </div>
        </div>
    }
}

// ─── Libraries editor ─────────────────────────────────────────────────────────

#[component]
pub fn JellyfinLibrariesPage() -> impl IntoView {
    let params = use_params_map();
    let source_id = params.with(|p| p.get("id").and_then(|s| s.parse::<i64>().ok()).unwrap_or(0));
    view! { <JellyfinLibrariesIsland source_id=source_id/> }
}

#[island]
fn JellyfinLibrariesIsland(source_id: i64) -> impl IntoView {
    let libs: Resource<Vec<LibraryPrefRow>> = Resource::new(
        move || source_id,
        |id| async move { list_jellyfin_libraries(id).await.unwrap_or_default() },
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
        async move { save_jellyfin_library_prefs(p).await }
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
                    <a href="/media/sources/jellyfin" class=style::back_link>"← Jellyfin"</a>
                    <h1 class=style::page_title>{provider_libraries_title("Jellyfin")}</h1>
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

            {move || err.get().map(|e| view! { <p class=style::form_error>{e}</p> })}
            {move || saved.get().then(|| view! { <p class=style::success_msg>{tr(TextKey::Saved)}</p> })}
        </div>
    }
}

// ─── Path replacements editor ─────────────────────────────────────────────────

#[component]
pub fn JellyfinPathsPage() -> impl IntoView {
    let params = use_params_map();
    let source_id = params.with(|p| p.get("id").and_then(|s| s.parse::<i64>().ok()).unwrap_or(0));
    view! { <JellyfinPathsIsland source_id=source_id/> }
}

#[island]
fn JellyfinPathsIsland(source_id: i64) -> impl IntoView {
    let loaded: Resource<Vec<PathReplacementRow>> = Resource::new(
        move || source_id,
        |id| async move { list_jellyfin_paths(id).await.unwrap_or_default() },
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
        async move { save_jellyfin_paths(id, list).await }
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
                    <a href="/media/sources/jellyfin" class=style::back_link>"← Jellyfin"</a>
                    <h1 class=style::page_title>{provider_path_replacements_title("Jellyfin")}</h1>
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
                remote_label=provider_path_label("Jellyfin")
            />

            {move || err.get().map(|e| view! { <p class=style::form_error>{e}</p> })}
            {move || saved.get().then(|| view! { <p class=style::success_msg>{tr(TextKey::Saved)}</p> })}
        </div>
    }
}

// ─── Shared path editor sub-component ────────────────────────────────────────

#[component]
pub fn PathsEditor(
    paths: RwSignal<Vec<PathReplacementRow>>,
    new_remote: RwSignal<String>,
    new_local: RwSignal<String>,
    on_add: impl Fn(leptos::ev::MouseEvent) + 'static + Clone,
    remote_label: String,
) -> impl IntoView {
    view! {
        <div class=style::editor_form>
            <div style="display:flex;gap:0.5rem;align-items:flex-end">
                <div class=style::form_row style="flex:1">
                    <label class=style::form_label>{remote_label}</label>
                    <input
                        class=style::form_input
                        type="text"
                        placeholder="/remote/path"
                        prop:value=move || new_remote.get()
                        on:input=move |e| new_remote.set(event_target_value(&e))
                    />
                </div>
                <div class=style::form_row style="flex:1">
                    <label class=style::form_label>{tr(TextKey::LocalPath)}</label>
                    <input
                        class=style::form_input
                        type="text"
                        placeholder="/local/path"
                        prop:value=move || new_local.get()
                        on:input=move |e| new_local.set(event_target_value(&e))
                    />
                </div>
                <button class=style::btn_secondary on:click=on_add>{tr(TextKey::Add)}</button>
            </div>

            {move || {
                let list = paths.get();
                if list.is_empty() {
                    view! { <p class=style::cell_muted>{tr(TextKey::NoPathReplacements)}</p> }.into_any()
                } else {
                    view! {
                        <table style="width:100%;border-collapse:collapse">
                            <thead>
                                <tr>
                                    <th style="text-align:left;padding:0.5rem;font-weight:600;font-size:0.875rem">{tr(TextKey::Remote)}</th>
                                    <th style="text-align:left;padding:0.5rem;font-weight:600;font-size:0.875rem">{tr(TextKey::Local)}</th>
                                    <th/>
                                </tr>
                            </thead>
                            <tbody>
                                {list.into_iter().enumerate().map(|(i, p)| {
                                    view! {
                                        <tr>
                                            <td style="padding:0.5rem;font-size:0.875rem" class=style::cell_muted>{p.remote_path.clone()}</td>
                                            <td style="padding:0.5rem;font-size:0.875rem" class=style::cell_muted>{p.local_path.clone()}</td>
                                            <td style="padding:0.5rem;text-align:right">
                                                <button
                                                    class=style::row_btn_del
                                                    on:click=move |_| {
                                                        paths.update(|list| { list.remove(i); });
                                                    }
                                                >{tr(TextKey::Remove)}</button>
                                            </td>
                                        </tr>
                                    }
                                }).collect::<Vec<_>>()}
                            </tbody>
                        </table>
                    }.into_any()
                }
            }}
        </div>
    }
}
