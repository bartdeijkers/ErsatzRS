#![allow(dead_code)]

use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::admin::i18n::{tr, TextKey};
use crate::admin::server_fns::libraries::scan_library;
use crate::admin::server_fns::remote_stream_playlists::{
    delete_remote_stream_playlist, exclude_remote_stream_playlist_entry,
    list_remote_stream_playlist_extractors, list_remote_stream_playlists,
    save_remote_stream_playlist, troubleshoot_remote_stream_playlist,
    PlaylistTroubleshootingResult, RemoteStreamPlaylistEditor, RemoteStreamPlaylistRow,
};
use crate::admin::ui::primitives::toast::{load_or_notify, use_toast, ToastSeverity};

stylance::import_crate_style!(style, "src/admin/pages/media_sources/shared.module.css");

#[component]
pub fn RemoteStreamPlaylistsPage() -> impl IntoView {
    let params = use_params_map();
    let library_id = params.with_untracked(|params| {
        params
            .get("library_id")
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or_default()
    });
    view! { <RemoteStreamPlaylistsIsland library_id=library_id/> }
}

#[island]
fn RemoteStreamPlaylistsIsland(library_id: i64) -> impl IntoView {
    let toast = use_toast();
    let rows: LocalResource<Vec<RemoteStreamPlaylistRow>> =
        LocalResource::new(move || async move {
            load_or_notify(list_remote_stream_playlists(library_id).await, toast)
        });
    let current_rows = RwSignal::new(Vec::<RemoteStreamPlaylistRow>::new());
    let extractor_names = RwSignal::new(Vec::<String>::new());
    let extractors: LocalResource<Vec<String>> = LocalResource::new(move || async move {
        load_or_notify(list_remote_stream_playlist_extractors().await, toast)
    });
    Effect::new(move |_| {
        if let Some(next) = rows.get() {
            current_rows.set(next);
        }
    });
    Effect::new(move |_| {
        if let Some(next) = extractors.get() {
            extractor_names.set(next);
        }
    });

    let editor_open = RwSignal::new(false);
    let editor_root_index = RwSignal::new(0usize);
    let editor_relative_path = RwSignal::new(None::<String>);
    let editor_name = RwSignal::new(String::new());
    let editor_url = RwSignal::new(String::new());
    let editor_extractor = RwSignal::new(String::new());
    let editor_folder = RwSignal::new(String::new());
    let editor_interval = RwSignal::new("24h".to_owned());
    let editor_max_items = RwSignal::new(500usize);
    let editor_order = RwSignal::new("upstream".to_owned());
    let editor_on_removed = RwSignal::new("trash".to_owned());
    let editor_create_playlist = RwSignal::new(false);
    let editor_tags = RwSignal::new(String::new());

    let reset_editor = move || {
        editor_root_index.set(0);
        editor_relative_path.set(None);
        editor_name.set(String::new());
        editor_url.set(String::new());
        editor_extractor.set(String::new());
        editor_folder.set(String::new());
        editor_interval.set("24h".to_owned());
        editor_max_items.set(500);
        editor_order.set("upstream".to_owned());
        editor_on_removed.set("trash".to_owned());
        editor_create_playlist.set(false);
        editor_tags.set(String::new());
    };

    let save_action = Action::new(move |editor: &RemoteStreamPlaylistEditor| {
        save_remote_stream_playlist(editor.clone())
    });
    Effect::new(move |_| match save_action.value().get() {
        Some(Ok(_)) => {
            editor_open.set(false);
            rows.refetch();
        }
        Some(Err(error)) => toast.push(error.to_string(), ToastSeverity::Error),
        None => {}
    });
    let delete_action = Action::new(move |target: &(usize, String)| {
        let (root_index, relative_path) = target.clone();
        delete_remote_stream_playlist(library_id, root_index, relative_path)
    });
    Effect::new(move |_| match delete_action.value().get() {
        Some(Ok(())) => rows.refetch(),
        Some(Err(error)) => toast.push(error.to_string(), ToastSeverity::Error),
        None => {}
    });
    let sync_action =
        Action::new(move |(): &()| scan_library(library_id, "Local".to_owned(), false));
    Effect::new(move |_| {
        if let Some(Err(error)) = sync_action.value().get() {
            toast.push(error.to_string(), ToastSeverity::Error);
        }
    });
    let troubleshoot_action = Action::new(move |target: &(usize, String)| {
        let (root_index, relative_path) = target.clone();
        troubleshoot_remote_stream_playlist(library_id, root_index, relative_path)
    });
    let exclude_action = Action::new(move |target: &(usize, String, String)| {
        let (root_index, relative_path, entry_id) = target.clone();
        exclude_remote_stream_playlist_entry(library_id, root_index, relative_path, entry_id)
    });
    Effect::new(move |_| match exclude_action.value().get() {
        Some(Ok(())) => {
            sync_action.dispatch(());
        }
        Some(Err(error)) => toast.push(error.to_string(), ToastSeverity::Error),
        None => {}
    });

    view! {
        <div class=style::page data-testid="remote-stream-playlists-page">
            <div class=style::toolbar>
                <div>
                    <a href="/media/remote/streams" class=style::back_link>
                        {format!("← {}", tr(TextKey::RemoteStreams))}
                    </a>
                    <h1 class=style::page_title>{tr(TextKey::RemoteStreamPlaylists)}</h1>
                </div>
                <div class=style::toolbar_actions>
                    <button
                        type="button"
                        class=style::btn_secondary
                        data-testid="remote-playlist-sync"
                        disabled=move || sync_action.pending().get()
                        on:click=move |_| { sync_action.dispatch(()); }
                    >
                        {tr(TextKey::SyncNow)}
                    </button>
                    <button
                        type="button"
                        class=style::btn_primary
                        data-testid="remote-playlist-add"
                        on:click=move |_| {
                            reset_editor();
                            editor_open.set(true);
                        }
                    >
                        {format!("+ {}", tr(TextKey::Add))}
                    </button>
                </div>
            </div>

            <div
                class=style::editor_form
                style:display=move || if editor_open.get() { "" } else { "none" }
                data-testid="remote-playlist-editor"
            >
                <label class=style::form_label for="remote-playlist-name">{tr(TextKey::Name)}</label>
                <input id="remote-playlist-name" class=style::form_input prop:value=move || editor_name.get() on:input=move |event| editor_name.set(event_target_value(&event))/>
                <label class=style::form_label for="remote-playlist-url">{tr(TextKey::PlaylistUrl)}</label>
                <input id="remote-playlist-url" class=style::form_input prop:value=move || editor_url.get() on:input=move |event| editor_url.set(event_target_value(&event))/>
                <label class=style::form_label for="remote-playlist-extractor">{tr(TextKey::Extractor)}</label>
                <select id="remote-playlist-extractor" class=style::form_select prop:value=move || editor_extractor.get() on:change=move |event| editor_extractor.set(event_target_value(&event))>
                    <option value="" disabled=true>{tr(TextKey::SelectValue)}</option>
                    {move || extractor_names.get().into_iter().map(|name| view! { <option value=name.clone()>{name.clone()}</option> }).collect_view()}
                </select>
                <label class=style::form_label for="remote-playlist-folder">{tr(TextKey::DestinationFolder)}</label>
                <input id="remote-playlist-folder" class=style::form_input prop:value=move || editor_folder.get() on:input=move |event| editor_folder.set(event_target_value(&event))/>
                <label class=style::form_label for="remote-playlist-interval">{tr(TextKey::SyncInterval)}</label>
                <input id="remote-playlist-interval" class=style::form_input prop:value=move || editor_interval.get() on:input=move |event| editor_interval.set(event_target_value(&event))/>
                <label class=style::form_label for="remote-playlist-max">{tr(TextKey::MaxItems)}</label>
                <input id="remote-playlist-max" class=style::form_input type="number" min="0" max="10000" prop:value=move || editor_max_items.get().to_string() on:input=move |event| {
                    if let Ok(value) = event_target_value(&event).parse() { editor_max_items.set(value); }
                }/>
                <label class=style::form_label for="remote-playlist-order">{tr(TextKey::PlaybackOrder)}</label>
                <select id="remote-playlist-order" class=style::form_select prop:value=move || editor_order.get() on:change=move |event| editor_order.set(event_target_value(&event))>
                    <option value="upstream">{tr(TextKey::Upstream)}</option>
                    <option value="newest_first">{tr(TextKey::NewestFirst)}</option>
                    <option value="title">{tr(TextKey::Title)}</option>
                </select>
                <label class=style::form_label for="remote-playlist-removed">{tr(TextKey::RemovedEntries)}</label>
                <select id="remote-playlist-removed" class=style::form_select prop:value=move || editor_on_removed.get() on:change=move |event| editor_on_removed.set(event_target_value(&event))>
                    <option value="trash">{tr(TextKey::Trash)}</option>
                    <option value="keep">{tr(TextKey::Keep)}</option>
                </select>
                <label class=style::form_label for="remote-playlist-tags">{tr(TextKey::Tags)}</label>
                <input id="remote-playlist-tags" class=style::form_input prop:value=move || editor_tags.get() on:input=move |event| editor_tags.set(event_target_value(&event))/>
                <label class=style::form_label>
                    <input type="checkbox" prop:checked=move || editor_create_playlist.get() on:change=move |event| editor_create_playlist.set(event_target_checked(&event))/>
                    {tr(TextKey::CreateOrderedPlaylist)}
                </label>
                <div class=style::toolbar_actions>
                    <button type="button" class=style::btn_secondary on:click=move |_| editor_open.set(false)>{tr(TextKey::Cancel)}</button>
                    <button
                        type="button"
                        class=style::btn_primary
                        data-testid="remote-playlist-save"
                        disabled=move || save_action.pending().get()
                        on:click=move |_| {
                            save_action.dispatch(RemoteStreamPlaylistEditor {
                                library_id,
                                root_index: editor_root_index.get_untracked(),
                                relative_path: editor_relative_path.get_untracked(),
                                name: editor_name.get_untracked(),
                                url: editor_url.get_untracked(),
                                extractor: editor_extractor.get_untracked(),
                                folder: editor_folder.get_untracked(),
                                interval: editor_interval.get_untracked(),
                                max_items: editor_max_items.get_untracked(),
                                order: editor_order.get_untracked(),
                                on_removed: editor_on_removed.get_untracked(),
                                create_playlist: editor_create_playlist.get_untracked(),
                                tags: editor_tags.get_untracked(),
                            });
                        }
                    >{tr(TextKey::Save)}</button>
                </div>
            </div>

            <p class=style::cell_muted style:display=move || if current_rows.get().is_empty() { "" } else { "none" }>
                {tr(TextKey::NoItemsToDisplay)}
            </p>
            <div>
                {move || current_rows.get().into_iter().map(|row| {
                    let edit_row = row.clone();
                    let delete_target = (row.root_index, row.relative_path.clone());
                    let troubleshoot_target = delete_target.clone();
                    let entry_root_index = row.root_index;
                    let entry_relative_path = row.relative_path.clone();
                    let last_sync = row.last_sync.clone().unwrap_or_else(|| tr(TextKey::NeverSynced).to_string());
                    view! {
                        <section class=style::editor_form data-testid="remote-playlist-row">
                            <div class=style::toolbar>
                                <div>
                                    <h2 class=style::page_title>{row.name.clone()}</h2>
                                    <p class=style::cell_muted>{format!("{} · {} · {}", row.extractor, state_label(&row.state), last_sync)}</p>
                                </div>
                                <div class=style::toolbar_actions>
                                    <button type="button" class=style::btn_secondary on:click=move |_| { troubleshoot_action.dispatch(troubleshoot_target.clone()); }>{tr(TextKey::Troubleshoot)}</button>
                                    <button
                                        type="button"
                                        class=style::btn_secondary
                                        disabled=!row.managed_by_ersatzrs
                                        title=if row.managed_by_ersatzrs { String::new() } else { tr(TextKey::ManagedManifestReadOnly).to_string() }
                                        on:click=move |_| {
                                            editor_root_index.set(edit_row.root_index);
                                            editor_relative_path.set(Some(edit_row.relative_path.clone()));
                                            editor_name.set(edit_row.name.clone());
                                            editor_url.set(edit_row.url.clone());
                                            editor_extractor.set(edit_row.extractor.clone());
                                            editor_folder.set(edit_row.folder.clone());
                                            editor_interval.set(edit_row.interval.clone());
                                            editor_max_items.set(edit_row.max_items);
                                            editor_order.set(edit_row.order.clone());
                                            editor_on_removed.set(edit_row.on_removed.clone());
                                            editor_create_playlist.set(edit_row.create_playlist);
                                            editor_tags.set(edit_row.tags.join(", "));
                                            editor_open.set(true);
                                        }
                                    >{tr(TextKey::Edit)}</button>
                                    <button type="button" class=style::btn_secondary disabled=!row.managed_by_ersatzrs on:click=move |_| { delete_action.dispatch(delete_target.clone()); }>{tr(TextKey::Delete)}</button>
                                </div>
                            </div>
                            <dl>
                                <dt>{tr(TextKey::PlaylistUrl)}</dt><dd>{row.url}</dd>
                                <dt>{tr(TextKey::DestinationFolder)}</dt><dd>{row.folder}</dd>
                                <dt>{tr(TextKey::SyncInterval)}</dt><dd>{row.interval}</dd>
                                <dt>{tr(TextKey::MaxItems)}</dt><dd>{row.max_items}</dd>
                                <dt>{tr(TextKey::Count)}</dt><dd>{row.item_count}</dd>
                            </dl>
                            <p class=style::cell_muted style:display=if row.truncated { "" } else { "none" }>{tr(TextKey::Truncated)}</p>
                            <table>
                                <thead><tr><th>{tr(TextKey::Title)}</th><th>{tr(TextKey::Duration)}</th><th>{tr(TextKey::State)}</th><th></th></tr></thead>
                                <tbody>{row.entries.into_iter().map(|entry| {
                                    let exclude_target = (entry_root_index, entry_relative_path.clone(), entry.id.clone());
                                    view! {
                                        <tr>
                                            <td>{entry.title}</td>
                                            <td>{format_duration(entry.duration_seconds)}</td>
                                            <td>{state_label(&entry.state)}</td>
                                            <td><button type="button" class=style::btn_secondary on:click=move |_| { exclude_action.dispatch(exclude_target.clone()); }>{tr(TextKey::Remove)}</button></td>
                                        </tr>
                                    }
                                }).collect_view()}</tbody>
                            </table>
                        </section>
                    }
                }).collect_view()}
            </div>

            <TroubleshootingResult result=Signal::derive(move || troubleshoot_action.value().get())/>
        </div>
    }
}

#[component]
fn TroubleshootingResult(
    result: Signal<Option<Result<PlaylistTroubleshootingResult, ServerFnError>>>,
) -> impl IntoView {
    view! {
        <output data-testid="remote-playlist-troubleshooting">
            {move || match result.get() {
                Some(Ok(value)) => format!("{}: {}\n{}", tr(TextKey::Count), value.entry_count, value.diagnostic_tail),
                Some(Err(error)) => error.to_string(),
                None => String::new(),
            }}
        </output>
    }
}

fn state_label(state: &str) -> String {
    match state {
        "never_synced" => tr(TextKey::NeverSynced).to_string(),
        "missing_duration" => tr(TextKey::MissingDuration).to_string(),
        "orphaned" => tr(TextKey::Orphaned).to_string(),
        "materialized" | "ok" => tr(TextKey::Materialized).to_string(),
        "failed" => tr(TextKey::Failed).to_string(),
        value => value.to_owned(),
    }
}

fn format_duration(seconds: Option<u64>) -> String {
    seconds.map_or_else(
        || tr(TextKey::MissingDuration).to_string(),
        |seconds| {
            format!(
                "{:02}:{:02}:{:02}",
                seconds / 3600,
                (seconds % 3600) / 60,
                seconds % 60
            )
        },
    )
}
