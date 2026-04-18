#![allow(clippy::too_many_lines, dead_code)]
use leptos::prelude::*;
use leptos_router::hooks::{use_navigate, use_params_map};
use leptos_router::NavigateOptions;

use crate::server::media_sources::{
    delete_jellyfin_source, get_jellyfin_connection, list_jellyfin_libraries, list_jellyfin_paths,
    list_jellyfin_sources, save_jellyfin_connection, save_jellyfin_library_prefs,
    save_jellyfin_paths, ConnectionForm, LibraryPrefRow, PathReplacementRow, RemoteSourceRow,
};
use crate::ui::primitives::dialog::DeleteDialog;

stylance::import_crate_style!(style, "src/pages/media_sources/shared.module.css");

// ─── Sources list ─────────────────────────────────────────────────────────────

#[component]
pub fn JellyfinSourcesPage() -> impl IntoView {
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
                        "Configure Connection"
                    </a>
                </div>
            </div>

            <Suspense fallback=|| view! { <p class=style::cell_muted>"Loading…"</p> }>
                {move || {
                    let list = rows.get().unwrap_or_default();
                    if list.is_empty() {
                        view! {
                            <p class=style::cell_muted>
                                "No Jellyfin source configured. Click "
                                <strong>"Configure Connection"</strong>
                                " to add one."
                            </p>
                        }.into_any()
                    } else {
                        view! {
                            <table style="width:100%;border-collapse:collapse">
                                <thead>
                                    <tr>
                                        <th style="text-align:left;padding:0.5rem;font-weight:600;font-size:0.875rem">"Name"</th>
                                        <th style="text-align:left;padding:0.5rem;font-weight:600;font-size:0.875rem">"Address"</th>
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
                                                        <a href=format!("/media/sources/jellyfin/{id}/libraries") class=style::row_btn>"Libraries"</a>
                                                        <a href=format!("/media/sources/jellyfin/{id}/paths") class=style::row_btn>"Paths"</a>
                                                        <a href="/media/sources/jellyfin/edit" class=style::row_btn>"Edit"</a>
                                                        <button
                                                            class=style::row_btn_del
                                                            on:click=move |_| {
                                                                delete_target.set(Some(r_clone.clone()));
                                                                delete_open.set(true);
                                                            }
                                                        >"Delete"</button>
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

            {move || {
                delete_target.get().map(|r| {
                    let name = r.name.clone();
                    view! {
                        <DeleteDialog
                            open=delete_open
                            entity_name=name
                            on_confirm=Callback::new(move |confirmed: bool| {
                                if confirmed { delete_action.dispatch(r.id); }
                                else { delete_open.set(false); }
                            })
                        />
                    }
                })
            }}
        </div>
    }
}

// ─── Connection editor ────────────────────────────────────────────────────────

#[component]
pub fn JellyfinConnectionPage() -> impl IntoView {
    let initial: Resource<ConnectionForm> = Resource::new(
        || (),
        |()| async { get_jellyfin_connection().await.unwrap_or_default() },
    );

    let address = RwSignal::new(String::new());
    let api_key = RwSignal::new(String::new());
    let err: RwSignal<Option<String>> = RwSignal::new(None);

    Effect::new(move |_| {
        if let Some(f) = initial.get() {
            address.set(f.address);
            api_key.set(f.api_key);
        }
    });

    let navigate = use_navigate();
    let save_action = Action::new(move |form: &ConnectionForm| {
        let form = form.clone();
        async move { save_jellyfin_connection(form).await }
    });

    Effect::new(move |_| match save_action.value().get() {
        Some(Ok(_)) => navigate("/media/sources/jellyfin", NavigateOptions::default()),
        Some(Err(e)) => err.set(Some(format!("Save failed: {e}"))),
        None => {}
    });

    let on_save = move |_| {
        let a = address.get_untracked();
        if a.trim().is_empty() {
            err.set(Some("Address is required.".into()));
            return;
        }
        err.set(None);
        let source_id = initial.get().map_or(0, |f| f.source_id);
        save_action.dispatch(ConnectionForm {
            source_id,
            address: a,
            api_key: api_key.get_untracked(),
        });
    };

    view! {
        <div class=style::page>
            <div class=style::toolbar>
                <div>
                    <a href="/media/sources/jellyfin" class=style::back_link>"← Jellyfin"</a>
                    <h1 class=style::page_title>"Jellyfin Connection"</h1>
                </div>
                <div class=style::toolbar_actions>
                    <button class=style::btn_primary on:click=on_save>"Save"</button>
                </div>
            </div>

            <div class=style::editor_form>
                <div class=style::form_row>
                    <label class=style::form_label>"Server Address"</label>
                    <input
                        class=style::form_input
                        type="url"
                        placeholder="http://jellyfin:8096"
                        prop:value=move || address.get()
                        on:input=move |e| address.set(event_target_value(&e))
                    />
                </div>
                <div class=style::form_row>
                    <label class=style::form_label>"API Key"</label>
                    <input
                        class=style::form_input
                        type="password"
                        prop:value=move || api_key.get()
                        on:input=move |e| api_key.set(event_target_value(&e))
                    />
                </div>
                {move || err.get().map(|e| view! { <span class=style::form_error>{e}</span> })}
            </div>
        </div>
    }
}

// ─── Libraries editor ─────────────────────────────────────────────────────────

#[component]
pub fn JellyfinLibrariesPage() -> impl IntoView {
    let params = use_params_map();
    let source_id =
        move || params.with(|p| p.get("id").and_then(|s| s.parse::<i64>().ok()).unwrap_or(0));

    let libs: Resource<Vec<LibraryPrefRow>> = Resource::new(source_id, |id| async move {
        list_jellyfin_libraries(id).await.unwrap_or_default()
    });

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
        Some(Err(e)) => err.set(Some(format!("Save failed: {e}"))),
        None => {}
    });

    view! {
        <div class=style::page>
            <div class=style::toolbar>
                <div>
                    <a href="/media/sources/jellyfin" class=style::back_link>"← Jellyfin"</a>
                    <h1 class=style::page_title>"Jellyfin Libraries"</h1>
                </div>
                <div class=style::toolbar_actions>
                    <button
                        class=style::btn_primary
                        on:click=move |_| {
                            saved.set(false);
                            save_action.dispatch(prefs.get_untracked());
                        }
                    >"Save"</button>
                </div>
            </div>

            <Suspense fallback=|| view! { <p class=style::cell_muted>"Loading…"</p> }>
                {move || {
                    let list = prefs.get();
                    view! {
                        <table style="width:100%;border-collapse:collapse">
                            <thead>
                                <tr>
                                    <th style="text-align:left;padding:0.5rem;font-weight:600;font-size:0.875rem">"Library"</th>
                                    <th style="text-align:left;padding:0.5rem;font-weight:600;font-size:0.875rem">"Kind"</th>
                                    <th style="text-align:center;padding:0.5rem;font-weight:600;font-size:0.875rem">"Sync"</th>
                                </tr>
                            </thead>
                            <tbody>
                                {list.into_iter().enumerate().map(|(i, lib)| {
                                    view! {
                                        <tr>
                                            <td style="padding:0.5rem;font-size:0.875rem">{lib.name.clone()}</td>
                                            <td style="padding:0.5rem;font-size:0.875rem" class=style::cell_muted>
                                                {crate::server::media_sources::media_kind_label(lib.media_kind)}
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
            {move || saved.get().then(|| view! { <p class=style::success_msg>"Saved."</p> })}
        </div>
    }
}

// ─── Path replacements editor ─────────────────────────────────────────────────

#[component]
pub fn JellyfinPathsPage() -> impl IntoView {
    let params = use_params_map();
    let source_id =
        move || params.with(|p| p.get("id").and_then(|s| s.parse::<i64>().ok()).unwrap_or(0));

    let loaded: Resource<Vec<PathReplacementRow>> = Resource::new(source_id, |id| async move {
        list_jellyfin_paths(id).await.unwrap_or_default()
    });

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
        Some(Err(e)) => err.set(Some(format!("Save failed: {e}"))),
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
                    <h1 class=style::page_title>"Jellyfin Path Replacements"</h1>
                </div>
                <div class=style::toolbar_actions>
                    <button
                        class=style::btn_primary
                        on:click=move |_| {
                            saved.set(false);
                            save_action.dispatch((source_id(), paths.get_untracked()));
                        }
                    >"Save"</button>
                </div>
            </div>

            <PathsEditor
                paths=paths
                new_remote=new_remote
                new_local=new_local
                on_add=on_add
                remote_label="Jellyfin Path"
            />

            {move || err.get().map(|e| view! { <p class=style::form_error>{e}</p> })}
            {move || saved.get().then(|| view! { <p class=style::success_msg>"Saved."</p> })}
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
    remote_label: &'static str,
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
                    <label class=style::form_label>"Local Path"</label>
                    <input
                        class=style::form_input
                        type="text"
                        placeholder="/local/path"
                        prop:value=move || new_local.get()
                        on:input=move |e| new_local.set(event_target_value(&e))
                    />
                </div>
                <button class=style::btn_secondary on:click=on_add>"Add"</button>
            </div>

            {move || {
                let list = paths.get();
                if list.is_empty() {
                    view! { <p class=style::cell_muted>"No path replacements."</p> }.into_any()
                } else {
                    view! {
                        <table style="width:100%;border-collapse:collapse">
                            <thead>
                                <tr>
                                    <th style="text-align:left;padding:0.5rem;font-weight:600;font-size:0.875rem">"Remote"</th>
                                    <th style="text-align:left;padding:0.5rem;font-weight:600;font-size:0.875rem">"Local"</th>
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
                                                >"Remove"</button>
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
