//! Playlist group/list and editor pages. Ports
//! `$ERSATZTV_SRC/ErsatzTV/Pages/Playlists.razor` and
//! `$ERSATZTV_SRC/ErsatzTV/Pages/PlaylistEditor.razor`.
#![allow(clippy::too_many_lines, dead_code)]

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_params_map;

use crate::admin::i18n::{delete_failed, media_browser_error_message, tr, TextKey};
use crate::admin::server_fns::media_browser::{
    create_playlist, create_playlist_group, delete_playlist, delete_playlist_group,
    get_playlist_editor, get_playlist_editor_options, get_playlist_page, preview_playlist_playout,
    PlaylistEditorForm, PlaylistEditorOptions, PlaylistItemForm, PlaylistItemOption,
    PlaylistPreviewRow, PlaylistRow,
};
use crate::admin::ui::primitives::{
    dialog::DeleteDialog,
    icon::{icon_style, IconName},
    toast::{use_toast, ToastSeverity},
};

stylance::import_crate_style!(style, "src/admin/pages/media/playlists.module.css");

const COLLECTION_TYPE_COLLECTION: i32 = 0;
const COLLECTION_TYPE_TELEVISION_SHOW: i32 = 1;
const COLLECTION_TYPE_TELEVISION_SEASON: i32 = 2;
const COLLECTION_TYPE_ARTIST: i32 = 3;
const COLLECTION_TYPE_MULTI_COLLECTION: i32 = 4;
const COLLECTION_TYPE_SMART_COLLECTION: i32 = 5;
const COLLECTION_TYPE_MOVIE: i32 = 10;
const COLLECTION_TYPE_EPISODE: i32 = 20;
const COLLECTION_TYPE_MUSIC_VIDEO: i32 = 30;
const COLLECTION_TYPE_OTHER_VIDEO: i32 = 40;
const COLLECTION_TYPE_SONG: i32 = 50;
const COLLECTION_TYPE_IMAGE: i32 = 60;
const COLLECTION_TYPE_REMOTE_STREAM: i32 = 70;
const PLAYBACK_ORDER_CHRONOLOGICAL: i32 = 1;
const PLAYBACK_ORDER_RANDOM: i32 = 2;
const PLAYBACK_ORDER_SHUFFLE: i32 = 3;
const PLAYBACK_ORDER_SHUFFLE_IN_ORDER: i32 = 4;
const PLAYBACK_ORDER_SEASON_EPISODE: i32 = 6;

#[component]
pub fn PlaylistsPage() -> impl IntoView {
    view! { <PlaylistsIsland/> }
}

#[island]
fn PlaylistsIsland() -> impl IntoView {
    let data = Resource::new(
        || (),
        |()| async { get_playlist_page().await.unwrap_or_default() },
    );
    let group_name = RwSignal::new(String::new());
    let playlist_name = RwSignal::new(String::new());
    let selected_group = RwSignal::new(0_i64);
    let toast = use_toast();

    let add_group = Action::new(move |name: &String| create_playlist_group(name.clone()));
    let add_playlist = Action::new(move |(group_id, name): &(i64, String)| {
        create_playlist(*group_id, name.clone())
    });
    let delete_group = Action::new(|id: &i64| delete_playlist_group(*id));
    let delete_item = Action::new(|id: &i64| delete_playlist(*id));
    let delete_target: RwSignal<Option<(bool, i64, String)>> = RwSignal::new(None);
    let delete_open = RwSignal::new(false);

    Effect::new(move |_| {
        if let Some(result) = add_group.value().get() {
            match result {
                Ok(_) => {
                    group_name.set(String::new());
                    toast.push(tr(TextKey::PlaylistGroupAdded), ToastSeverity::Success);
                    data.refetch();
                }
                Err(error) => toast.push(
                    format!(
                        "{}: {}",
                        tr(TextKey::AddFailed),
                        media_browser_error_message(error)
                    ),
                    ToastSeverity::Error,
                ),
            }
        }
    });
    Effect::new(move |_| {
        if let Some(result) = add_playlist.value().get() {
            match result {
                Ok(_) => {
                    playlist_name.set(String::new());
                    toast.push(tr(TextKey::PlaylistAdded), ToastSeverity::Success);
                    data.refetch();
                }
                Err(error) => toast.push(
                    format!(
                        "{}: {}",
                        tr(TextKey::AddFailed),
                        media_browser_error_message(error)
                    ),
                    ToastSeverity::Error,
                ),
            }
        }
    });
    Effect::new(move |_| {
        if let Some(result) = delete_group.value().get() {
            match result {
                Ok(()) => {
                    toast.push(tr(TextKey::PlaylistGroupDeleted), ToastSeverity::Success);
                    data.refetch();
                }
                Err(error) => toast.push(
                    delete_failed(media_browser_error_message(error)),
                    ToastSeverity::Error,
                ),
            }
            delete_open.set(false);
        }
    });
    Effect::new(move |_| {
        if let Some(result) = delete_item.value().get() {
            match result {
                Ok(()) => {
                    toast.push(tr(TextKey::PlaylistDeleted), ToastSeverity::Success);
                    data.refetch();
                }
                Err(error) => toast.push(
                    delete_failed(media_browser_error_message(error)),
                    ToastSeverity::Error,
                ),
            }
            delete_open.set(false);
        }
    });

    view! {
        <div class=style::page>
            <section class=style::playlist_section>
                <h1 class=style::page_title>{tr(TextKey::PlaylistGroups)}</h1>
                <div class=style::divider/>
                <form method="post" action="/api/playlists/groups/create" class=style::playlist_form>
                    <div class=style::playlist_form_row>
                        <label for="playlist-group-name">{tr(TextKey::PlaylistGroupName)}</label>
                        <input
                            id="playlist-group-name"
                            name="name"
                            class=style::search
                            prop:value=move || group_name.get()
                            on:input=move |event| group_name.set(event_target_value(&event))
                        />
                    </div>
                    <div class=style::playlist_form_row>
                        <span class=style::form_label_spacer aria-hidden="true"></span>
                        <button
                            type="submit"
                            class=style::btn_add
                            on:click=move |event| {
                                event.prevent_default();
                                add_group.dispatch(group_name.get_untracked());
                            }
                        >
                            <span style=icon_style(IconName::Add, "1.125rem") aria-hidden="true"></span>
                            <span>{tr(TextKey::AddPlaylistGroup)}</span>
                        </button>
                    </div>
                </form>
            </section>

            <section class=style::playlist_section>
                <h2 class=style::page_title>{tr(TextKey::Playlists)}</h2>
                <div class=style::divider/>
                <Suspense fallback=move || view! { <p class=style::empty>{tr(TextKey::Loading)}</p> }>
                    {move || data.get().map(|page| {
                        if selected_group.get_untracked() == 0 {
                            if let Some(first) = page.groups.first() {
                                selected_group.set(first.id);
                            }
                        }
                        view! {
                            <form method="post" action="/api/playlists/create" class=style::playlist_form>
                                <div class=style::playlist_form_row>
                                    <label for="playlist-group-select">{tr(TextKey::PlaylistGroup)}</label>
                                    <select
                                        id="playlist-group-select"
                                        name="playlist_group_id"
                                        class=style::select
                                        prop:value=move || selected_group.get().to_string()
                                        on:change=move |event| {
                                            selected_group.set(event_target_value(&event).parse().unwrap_or(0));
                                        }
                                    >
                                        {page.groups.iter().map(|group| view! {
                                            <option value=group.id.to_string()>{group.name.clone()}</option>
                                        }).collect_view()}
                                    </select>
                                </div>
                                <div class=style::playlist_form_row>
                                    <label for="playlist-name">{tr(TextKey::PlaylistName)}</label>
                                    <input
                                        id="playlist-name"
                                        name="name"
                                        class=style::search
                                        prop:value=move || playlist_name.get()
                                        on:input=move |event| playlist_name.set(event_target_value(&event))
                                    />
                                </div>
                                <div class=style::playlist_form_row>
                                    <span class=style::form_label_spacer aria-hidden="true"></span>
                                    <button
                                        type="submit"
                                        class=style::btn_add
                                        on:click=move |event| {
                                            event.prevent_default();
                                            add_playlist.dispatch((selected_group.get_untracked(), playlist_name.get_untracked()));
                                        }
                                    >
                                        <span style=icon_style(IconName::Add, "1.125rem") aria-hidden="true"></span>
                                        <span>{tr(TextKey::AddPlaylist)}</span>
                                    </button>
                                </div>
                            </form>
                            <div class=style::playlist_form_row>
                                <span class=style::form_label_spacer aria-hidden="true"></span>
                                <div class=style::playlist_tree>
                                    {page.groups.into_iter().map(|group| {
                                        let group_id = group.id;
                                        let group_name = group.name.clone();
                                        let group_name_for_delete = group.name.clone();
                                        let disabled = group.is_system;
                                        view! {
                                            <div class=style::playlist_group>
                                                <div class=style::playlist_group_header>
                                                    <div class=style::tree_item_text>
                                                        <span class=style::tree_folder style=icon_style(IconName::Folder, "1.25rem") aria-hidden="true"></span>
                                                        <span>{group_name}</span>
                                                    </div>
                                                    <form method="post" action="/api/playlists/groups/delete">
                                                        <input type="hidden" name="id" value=group_id.to_string()/>
                                                        <button
                                                            type="submit"
                                                            class=style::icon_button
                                                            title=tr(TextKey::Delete)
                                                            aria-label=tr(TextKey::DeletePlaylistGroup)
                                                            disabled=disabled
                                                            on:click=move |event| {
                                                                event.prevent_default();
                                                                delete_target.set(Some((true, group_id, group_name_for_delete.clone())));
                                                                delete_open.set(true);
                                                            }
                                                        >
                                                            <span style=icon_style(IconName::Delete, "1.25rem") aria-hidden="true"></span>
                                                        </button>
                                                    </form>
                                                </div>
                                                <ul class=style::members_list>
                                                    {group.playlists.into_iter().map(|playlist| playlist_row(playlist, delete_target, delete_open)).collect_view()}
                                                </ul>
                                            </div>
                                        }
                                    }).collect_view()}
                                </div>
                            </div>
                        }
                    })}
                </Suspense>
            </section>
            <DeleteDialog
                open=delete_open
                entity_name=Signal::derive(move || {
                    delete_target.get().map(|(_, _, name)| name).unwrap_or_default()
                })
                on_confirm=Callback::new(move |confirmed: bool| {
                    if confirmed {
                        if let Some((is_group, id, _)) = delete_target.get_untracked() {
                            if is_group {
                                delete_group.dispatch(id);
                            } else {
                                delete_item.dispatch(id);
                            }
                        }
                    } else {
                        delete_open.set(false);
                    }
                })
            />
        </div>
    }
}

fn playlist_row(
    playlist: PlaylistRow,
    delete_target: RwSignal<Option<(bool, i64, String)>>,
    delete_open: RwSignal<bool>,
) -> impl IntoView {
    let PlaylistRow {
        id,
        name,
        is_system,
        ..
    } = playlist;
    let name_for_delete = name.clone();
    view! {
        <li class=style::member>
            <span class=style::member_title>{name}</span>
            <a
                class=style::icon_button
                href=format!("/media/playlists/{id}")
                title=tr(TextKey::Edit)
                aria-label=tr(TextKey::EditPlaylist)
            >
                <span style=icon_style(IconName::Edit, "1.25rem") aria-hidden="true"></span>
            </a>
            <form method="post" action="/api/playlists/delete">
                <input type="hidden" name="id" value=id.to_string()/>
                <button
                    type="submit"
                    class=style::icon_button
                    title=tr(TextKey::Delete)
                    aria-label=tr(TextKey::DeletePlaylist)
                    disabled=is_system
                    on:click=move |event| {
                        event.prevent_default();
                        delete_target.set(Some((false, id, name_for_delete.clone())));
                        delete_open.set(true);
                    }
                >
                    <span style=icon_style(IconName::Delete, "1.25rem") aria-hidden="true"></span>
                </button>
            </form>
        </li>
    }
}

#[component]
pub fn PlaylistEditorPage() -> impl IntoView {
    let params = use_params_map();
    let id = params.with(|p| p.get("id").and_then(|s| s.parse::<i64>().ok()).unwrap_or(0));
    view! { <PlaylistEditorIsland id/> }
}

#[island]
fn PlaylistEditorIsland(id: i64) -> impl IntoView {
    let initial = Resource::new_blocking(
        move || id,
        |id| async move { get_playlist_editor(id).await.unwrap_or_default() },
    );
    let options = Resource::new_blocking(
        || (),
        |()| async { get_playlist_editor_options().await.unwrap_or_default() },
    );
    let name = RwSignal::new(String::new());
    let is_system = RwSignal::new(false);
    let items: RwSignal<Vec<PlaylistItemForm>> = RwSignal::new(Vec::new());
    let selected = RwSignal::new(None::<usize>);
    let preview_rows: RwSignal<Vec<PlaylistPreviewRow>> = RwSignal::new(Vec::new());
    let preview_message = RwSignal::new(String::new());

    Effect::new(move |_| {
        if let Some(Some(form)) = initial.get() {
            name.set(form.name);
            is_system.set(form.is_system);
            items.set(form.items);
            selected.set(None);
        }
    });

    let current_form = move || PlaylistEditorForm {
        id,
        playlist_group_id: 0,
        name: name.get_untracked(),
        is_system: is_system.get_untracked(),
        items: normalized_items(items.get_untracked()),
    };

    let preview = move || {
        preview_rows.set(Vec::new());
        preview_message.set(String::new());
        let form = current_form();
        if form.items.iter().all(|item| !item.include_in_program_guide) {
            preview_message.set(tr(TextKey::PlaylistPreviewEmpty).to_string());
            return;
        }
        spawn_local(async move {
            match preview_playlist_playout(form).await {
                Ok(rows) => preview_rows.set(rows),
                Err(error) => preview_message.set(media_browser_error_message(error)),
            }
        });
    };

    view! {
        <form method="post" action="/api/playlists/save" class=style::page>
            <input type="hidden" name="id" value=id.to_string()/>
            {move || playlist_item_hidden_fields(normalized_items(items.get()))}
            <div class=style::editor_action_bar>
                <button
                    type="submit"
                    class=style::btn_add
                    disabled=move || is_system.get()
                >
                    {tr(TextKey::SavePlaylist)}
                </button>
                <button
                    type="button"
                    class=style::btn_add
                    disabled=move || is_system.get()
                    on:click=move |_| {
                        items.update(|rows| {
                            rows.push(PlaylistItemForm {
                                index: i32::try_from(rows.len()).unwrap_or(0),
                                collection_type: COLLECTION_TYPE_COLLECTION,
                                playback_order: PLAYBACK_ORDER_CHRONOLOGICAL,
                                count: Some(1),
                                include_in_program_guide: true,
                                ..PlaylistItemForm::default()
                            });
                            selected.set(Some(rows.len().saturating_sub(1)));
                        });
                    }
                >
                    {tr(TextKey::AddPlaylistItem)}
                </button>
                <button type="button" class=style::btn_add on:click=move |_| preview()>
                    {tr(TextKey::PreviewPlaylistPlayout)}
                </button>
            </div>

            <Suspense fallback=move || view! { <p class=style::empty>{tr(TextKey::Loading)}</p> }>
                {move || {
                    let opts = options.get().unwrap_or_default();
                    initial.get().map(move |maybe| {
                        if maybe.is_none() {
                            return view! { <p class=style::empty>{tr(TextKey::PlaylistNotFound)}</p> }.into_any();
                        }
                        view! {
                            <section class=style::editor>
                                <h1 class=style::page_title>{tr(TextKey::Playlist)}</h1>
                                <label>
                                    <span>{tr(TextKey::Name)}</span>
                                    <input
                                        name="name"
                                        prop:value=move || name.get()
                                        disabled=move || is_system.get()
                                        on:input=move |event| name.set(event_target_value(&event))
                                    />
                                </label>
                            </section>

                            <section class=style::playlist_editor_grid>
                                <div>
                                    <h2 class=style::page_title>{tr(TextKey::PlaylistItems)}</h2>
                                    <div class=format!("{} {}", style::table_wrap, style::playlist_items_table_wrap)>
                                        <table>
                                            <thead>
                                                <tr>
                                                    <th>{tr(TextKey::ItemType)}</th>
                                                    <th>{tr(TextKey::Item)}</th>
                                                    <th>{tr(TextKey::PlaybackOrder)}</th>
                                                    <th>{tr(TextKey::Count)}</th>
                                                    <th>{tr(TextKey::PlayAll)}</th>
                                                    <th>{tr(TextKey::ShowInEpg)}</th>
                                                    <th/>
                                                </tr>
                                            </thead>
                                            <tbody>
                                                {move || items.get().into_iter().enumerate().map(|(idx, item)| {
                                                    let readonly = is_system.get();
                                                    let selected_row = selected.get() == Some(idx);
                                                    let item_name = item.item_name.clone();
                                                    let count_label = playlist_count_label(&item, readonly);
                                                    let play_all = item.play_all;
                                                    let include_in_program_guide = item.include_in_program_guide;
                                                    let collection_type = item.collection_type;
                                                    let playback_order = item.playback_order;
                                                    view! {
                                                        <tr
                                                            class=if selected_row { style::selected_row } else { "" }
                                                            on:click=move |_| {
                                                                selected.update(|current| {
                                                                    *current = if *current == Some(idx) { None } else { Some(idx) };
                                                                });
                                                            }
                                                        >
                                                            <td>{item_type_label_i18n(collection_type)}</td>
                                                            <td>{item_name}</td>
                                                            <td>{playback_order_label_i18n(playback_order)}</td>
                                                            <td>{count_label}</td>
                                                            <td>
                                                                {if readonly {
                                                                    view! { <span class=style::readonly_value>{bool_label(play_all)}</span> }.into_any()
                                                                } else {
                                                                    view! {
                                                                        <input
                                                                            type="checkbox"
                                                                            name="item_play_all_index"
                                                                            value=idx.to_string()
                                                                            checked=play_all
                                                                            on:change=move |event| update_item(items, idx, |row| {
                                                                                row.play_all = event_target_checked(&event);
                                                                                if row.play_all {
                                                                                    row.count = None;
                                                                                } else if row.count.is_none() {
                                                                                    row.count = Some(1);
                                                                                }
                                                                            })
                                                                        />
                                                                    }.into_any()
                                                                }}
                                                            </td>
                                                            <td>
                                                                {if readonly {
                                                                    view! { <span class=style::readonly_value>{bool_label(include_in_program_guide)}</span> }.into_any()
                                                                } else {
                                                                    view! {
                                                                        <input
                                                                            type="checkbox"
                                                                            name="item_include_in_program_guide_index"
                                                                            value=idx.to_string()
                                                                            checked=include_in_program_guide
                                                                            on:change=move |event| update_item(items, idx, |row| {
                                                                                row.include_in_program_guide = event_target_checked(&event);
                                                                            })
                                                                        />
                                                                    }.into_any()
                                                                }}
                                                            </td>
                                                            <td>
                                                                {if readonly {
                                                                    view! { <span class=style::readonly_value aria-hidden="true"></span> }.into_any()
                                                                } else {
                                                                    view! {
                                                                        <div class=style::row_actions>
                                                                            <button type="button" class=style::btn_edit on:click=move |_| copy_item(items, selected, idx)>{tr(TextKey::Copy)}</button>
                                                                            <button type="button" class=style::btn_edit disabled=idx == 0 on:click=move |_| move_item(items, selected, idx, -1)>{tr(TextKey::MoveUp)}</button>
                                                                            <button type="button" class=style::btn_edit disabled=move || { idx + 1 >= items.get().len() } on:click=move |_| move_item(items, selected, idx, 1)>{tr(TextKey::MoveDown)}</button>
                                                                            <button type="button" class=style::btn_delete on:click=move |_| remove_item(items, selected, idx)>{tr(TextKey::Delete)}</button>
                                                                        </div>
                                                                    }.into_any()
                                                                }}
                                                            </td>
                                                        </tr>
                                                    }
                                                }).collect_view()}
                                            </tbody>
                                        </table>
                                    </div>
                                </div>
                                <div class=style::playlist_item_panel style:display=move || if selected.get().is_some() && !is_system.get() { "" } else { "none" }>
                                    <div class=style::panel_header>
                                        <h2 class=style::page_title>{tr(TextKey::PlaylistItem)}</h2>
                                        <button
                                            type="button"
                                            class=style::icon_button
                                            title=tr(TextKey::Close)
                                            aria-label=tr(TextKey::Close)
                                            on:click=move |_| selected.set(None)
                                        >
                                            <span style=icon_style(IconName::Close, "1.25rem") aria-hidden="true"></span>
                                        </button>
                                    </div>
                                    {move || {
                                        selected
                                            .get()
                                            .and_then(|idx| {
                                                items
                                                    .get()
                                                    .get(idx)
                                                    .cloned()
                                                    .map(|item| {
                                                        playlist_item_editor(
                                                            idx,
                                                            items,
                                                            &opts,
                                                            item.collection_type,
                                                            item.playback_order,
                                                        )
                                                    })
                                            })
                                            .collect_view()
                                    }}
                                </div>
                            </section>

                            <section class=style::playlist_section style:display=move || if !preview_message.get().is_empty() || !preview_rows.get().is_empty() { "" } else { "none" }>
                                <h2 class=style::page_title>{tr(TextKey::Preview)}</h2>
                                <p class=style::empty style:display=move || if preview_message.get().is_empty() { "none" } else { "" }>
                                    {move || preview_message.get()}
                                </p>
                                <div class=style::table_wrap style:display=move || if preview_rows.get().is_empty() { "none" } else { "" }>
                                    <table>
                                        <thead>
                                            <tr>
                                                <th>{tr(TextKey::Start)}</th>
                                                <th>{tr(TextKey::Finish)}</th>
                                                <th>{tr(TextKey::MediaItem)}</th>
                                                <th>{tr(TextKey::Duration)}</th>
                                            </tr>
                                        </thead>
                                        <tbody>
                                            {move || preview_rows.get().into_iter().map(|row| view! {
                                                <tr>
                                                    <td>{row.start}</td>
                                                    <td>{row.finish}</td>
                                                    <td>{row.media_item}</td>
                                                    <td>{row.duration}</td>
                                                </tr>
                                            }).collect_view()}
                                        </tbody>
                                    </table>
                                </div>
                            </section>
                        }.into_any()
                    })
                }}
            </Suspense>
        </form>
    }
}

fn playlist_item_editor(
    idx: usize,
    items: RwSignal<Vec<PlaylistItemForm>>,
    options: &PlaylistEditorOptions,
    collection_type: i32,
    playback_order: i32,
) -> impl IntoView {
    view! {
        <div class=style::editor>
            <label>
                <span>{tr(TextKey::CollectionType)}</span>
                <select
                    prop:value=collection_type.to_string()
                    on:change=move |event| {
                        let collection_type = event_target_value(&event).parse().unwrap_or(COLLECTION_TYPE_COLLECTION);
                        update_item(items, idx, |row| {
                            row.collection_type = collection_type;
                            row.collection_id = None;
                            row.multi_collection_id = None;
                            row.smart_collection_id = None;
                            row.media_item_id = None;
                            row.item_name.clear();
                            row.playback_order = if collection_type == COLLECTION_TYPE_MULTI_COLLECTION {
                                PLAYBACK_ORDER_SHUFFLE
                            } else {
                                PLAYBACK_ORDER_CHRONOLOGICAL
                            };
                        });
                    }
                >
                    {collection_type_options()}
                </select>
            </label>
            {source_select(idx, items, collection_type, options)}
            <label>
                <span>{tr(TextKey::PlaybackOrder)}</span>
                <select
                    prop:value=playback_order.to_string()
                    disabled=individual_media_type(collection_type)
                    on:change=move |event| update_item(items, idx, |row| {
                        row.playback_order = event_target_value(&event).parse().unwrap_or(PLAYBACK_ORDER_CHRONOLOGICAL);
                    })
                >
                    {playback_order_options(collection_type)}
                </select>
            </label>
            <label>
                <span>{tr(TextKey::Count)}</span>
                <input
                    type="number"
                    min="1"
                    prop:value=move || {
                        items.get().get(idx).and_then(|row| row.count).unwrap_or(1).to_string()
                    }
                    on:input=move |event| update_item(items, idx, |row| {
                        row.count = event_target_value(&event).parse::<i32>().ok();
                        if row.count.is_some() {
                            row.play_all = false;
                        }
                    })
                />
            </label>
        </div>
    }
}

fn source_select(
    idx: usize,
    items: RwSignal<Vec<PlaylistItemForm>>,
    collection_type: i32,
    options: &PlaylistEditorOptions,
) -> AnyView {
    let (label, selected, source): (TextKey, Option<i64>, Vec<PlaylistItemOption>) =
        match collection_type {
            COLLECTION_TYPE_COLLECTION => (
                TextKey::Collection,
                items
                    .get_untracked()
                    .get(idx)
                    .and_then(|row| row.collection_id),
                options.collections.clone(),
            ),
            COLLECTION_TYPE_MULTI_COLLECTION => (
                TextKey::MultiCollection,
                items
                    .get_untracked()
                    .get(idx)
                    .and_then(|row| row.multi_collection_id),
                options.multi_collections.clone(),
            ),
            COLLECTION_TYPE_SMART_COLLECTION => (
                TextKey::SmartCollection,
                items
                    .get_untracked()
                    .get(idx)
                    .and_then(|row| row.smart_collection_id),
                options.smart_collections.clone(),
            ),
            COLLECTION_TYPE_TELEVISION_SHOW => (
                TextKey::TelevisionShow,
                items
                    .get_untracked()
                    .get(idx)
                    .and_then(|row| row.media_item_id),
                options.shows.clone(),
            ),
            COLLECTION_TYPE_TELEVISION_SEASON => (
                TextKey::TelevisionSeason,
                items
                    .get_untracked()
                    .get(idx)
                    .and_then(|row| row.media_item_id),
                options.seasons.clone(),
            ),
            COLLECTION_TYPE_ARTIST => (
                TextKey::Artist,
                items
                    .get_untracked()
                    .get(idx)
                    .and_then(|row| row.media_item_id),
                options.artists.clone(),
            ),
            COLLECTION_TYPE_MOVIE => (
                TextKey::Movie,
                items
                    .get_untracked()
                    .get(idx)
                    .and_then(|row| row.media_item_id),
                options.movies.clone(),
            ),
            COLLECTION_TYPE_EPISODE => (
                TextKey::Episode,
                items
                    .get_untracked()
                    .get(idx)
                    .and_then(|row| row.media_item_id),
                options.episodes.clone(),
            ),
            COLLECTION_TYPE_MUSIC_VIDEO => (
                TextKey::MusicVideo,
                items
                    .get_untracked()
                    .get(idx)
                    .and_then(|row| row.media_item_id),
                options.music_videos.clone(),
            ),
            COLLECTION_TYPE_OTHER_VIDEO => (
                TextKey::OtherVideo,
                items
                    .get_untracked()
                    .get(idx)
                    .and_then(|row| row.media_item_id),
                options.other_videos.clone(),
            ),
            COLLECTION_TYPE_SONG => (
                TextKey::Song,
                items
                    .get_untracked()
                    .get(idx)
                    .and_then(|row| row.media_item_id),
                options.songs.clone(),
            ),
            COLLECTION_TYPE_IMAGE => (
                TextKey::Image,
                items
                    .get_untracked()
                    .get(idx)
                    .and_then(|row| row.media_item_id),
                options.images.clone(),
            ),
            COLLECTION_TYPE_REMOTE_STREAM => (
                TextKey::RemoteStream,
                items
                    .get_untracked()
                    .get(idx)
                    .and_then(|row| row.media_item_id),
                options.remote_streams.clone(),
            ),
            _ => (TextKey::Item, None, Vec::new()),
        };
    let selected_value = selected.unwrap_or(0).to_string();
    let source_for_handler = source.clone();
    view! {
        <label>
            <span>{tr(label)}</span>
            <select
                prop:value=selected_value
                on:change=move |event| {
                    let selected_id = event_target_value(&event).parse::<i64>().ok().filter(|id| *id > 0);
                    let selected_name = selected_id
                        .and_then(|id| source_for_handler.iter().find(|option| option.id == id).map(|option| option.name.clone()))
                        .unwrap_or_default();
                    update_item(items, idx, |row| {
                        match row.collection_type {
                            COLLECTION_TYPE_COLLECTION => row.collection_id = selected_id,
                            COLLECTION_TYPE_MULTI_COLLECTION => row.multi_collection_id = selected_id,
                            COLLECTION_TYPE_SMART_COLLECTION => row.smart_collection_id = selected_id,
                            _ => row.media_item_id = selected_id,
                        }
                        row.item_name = selected_name;
                    });
                }
            >
                <option value="0">{tr(TextKey::TypeToSearch)}</option>
                {source.iter().map(|option| view! {
                    <option value=option.id.to_string()>{option.name.clone()}</option>
                }).collect_view()}
            </select>
        </label>
    }.into_any()
}

fn collection_type_options() -> impl IntoView {
    [
        (COLLECTION_TYPE_COLLECTION, TextKey::Collection),
        (COLLECTION_TYPE_TELEVISION_SHOW, TextKey::TelevisionShow),
        (COLLECTION_TYPE_TELEVISION_SEASON, TextKey::TelevisionSeason),
        (COLLECTION_TYPE_ARTIST, TextKey::Artist),
        (COLLECTION_TYPE_MULTI_COLLECTION, TextKey::MultiCollection),
        (COLLECTION_TYPE_SMART_COLLECTION, TextKey::SmartCollection),
        (COLLECTION_TYPE_MOVIE, TextKey::Movie),
        (COLLECTION_TYPE_EPISODE, TextKey::Episode),
        (COLLECTION_TYPE_MUSIC_VIDEO, TextKey::MusicVideo),
        (COLLECTION_TYPE_OTHER_VIDEO, TextKey::OtherVideo),
        (COLLECTION_TYPE_SONG, TextKey::Song),
        (COLLECTION_TYPE_IMAGE, TextKey::Image),
        (COLLECTION_TYPE_REMOTE_STREAM, TextKey::RemoteStream),
    ]
    .into_iter()
    .map(|(value, label)| view! { <option value=value.to_string()>{tr(label)}</option> })
    .collect_view()
}

fn playback_order_options(collection_type: i32) -> impl IntoView {
    let options: Vec<(i32, TextKey)> = match collection_type {
        COLLECTION_TYPE_MULTI_COLLECTION => vec![(PLAYBACK_ORDER_SHUFFLE, TextKey::Shuffle)],
        COLLECTION_TYPE_COLLECTION | COLLECTION_TYPE_SMART_COLLECTION => vec![
            (PLAYBACK_ORDER_CHRONOLOGICAL, TextKey::Chronological),
            (PLAYBACK_ORDER_SHUFFLE, TextKey::Shuffle),
            (PLAYBACK_ORDER_RANDOM, TextKey::Random),
            (PLAYBACK_ORDER_SHUFFLE_IN_ORDER, TextKey::ShuffleInOrder),
        ],
        COLLECTION_TYPE_TELEVISION_SHOW => vec![
            (PLAYBACK_ORDER_SEASON_EPISODE, TextKey::SeasonEpisode),
            (PLAYBACK_ORDER_CHRONOLOGICAL, TextKey::Chronological),
            (PLAYBACK_ORDER_SHUFFLE, TextKey::Shuffle),
            (PLAYBACK_ORDER_RANDOM, TextKey::Random),
        ],
        _ => vec![
            (PLAYBACK_ORDER_CHRONOLOGICAL, TextKey::Chronological),
            (PLAYBACK_ORDER_SHUFFLE, TextKey::Shuffle),
            (PLAYBACK_ORDER_RANDOM, TextKey::Random),
        ],
    };
    options
        .into_iter()
        .map(|(value, label)| view! { <option value=value.to_string()>{tr(label)}</option> })
        .collect_view()
}

fn update_item(
    items: RwSignal<Vec<PlaylistItemForm>>,
    idx: usize,
    update: impl FnOnce(&mut PlaylistItemForm),
) {
    items.update(|rows| {
        if let Some(row) = rows.get_mut(idx) {
            update(row);
        }
    });
}

fn copy_item(
    items: RwSignal<Vec<PlaylistItemForm>>,
    selected: RwSignal<Option<usize>>,
    idx: usize,
) {
    items.update(|rows| {
        if let Some(item) = rows.get(idx).cloned() {
            rows.insert(idx + 1, item);
            selected.set(Some(idx + 1));
        }
    });
}

fn move_item(
    items: RwSignal<Vec<PlaylistItemForm>>,
    selected: RwSignal<Option<usize>>,
    idx: usize,
    delta: isize,
) {
    items.update(|rows| {
        let target = idx.saturating_add_signed(delta);
        if target < rows.len() {
            rows.swap(idx, target);
            selected.set(Some(target));
        }
    });
}

fn remove_item(
    items: RwSignal<Vec<PlaylistItemForm>>,
    selected: RwSignal<Option<usize>>,
    idx: usize,
) {
    items.update(|rows| {
        if idx < rows.len() {
            rows.remove(idx);
            selected.set((!rows.is_empty()).then_some(idx.min(rows.len().saturating_sub(1))));
        }
    });
}

fn normalized_items(mut items: Vec<PlaylistItemForm>) -> Vec<PlaylistItemForm> {
    for (idx, item) in items.iter_mut().enumerate() {
        item.index = i32::try_from(idx).unwrap_or(0);
    }
    items
}

fn playlist_item_hidden_fields(items: Vec<PlaylistItemForm>) -> impl IntoView {
    items
        .into_iter()
        .map(|item| {
            view! {
                <input type="hidden" name="item_index" value=item.index.to_string()/>
                <input type="hidden" name="item_id" value=item.id.to_string()/>
                <input type="hidden" name="item_collection_type" value=item.collection_type.to_string()/>
                <input
                    type="hidden"
                    name="item_collection_id"
                    value=item.collection_id.map_or_else(String::new, |id| id.to_string())
                />
                <input
                    type="hidden"
                    name="item_multi_collection_id"
                    value=item.multi_collection_id.map_or_else(String::new, |id| id.to_string())
                />
                <input
                    type="hidden"
                    name="item_smart_collection_id"
                    value=item.smart_collection_id.map_or_else(String::new, |id| id.to_string())
                />
                <input
                    type="hidden"
                    name="item_media_item_id"
                    value=item.media_item_id.map_or_else(String::new, |id| id.to_string())
                />
                <input type="hidden" name="item_playback_order" value=item.playback_order.to_string()/>
                <input
                    type="hidden"
                    name="item_count"
                    value=item.count.map_or_else(String::new, |count| count.to_string())
                />
            }
        })
        .collect_view()
}

fn item_type_label_i18n(collection_type: i32) -> &'static str {
    match collection_type {
        COLLECTION_TYPE_COLLECTION => tr(TextKey::Collection),
        COLLECTION_TYPE_TELEVISION_SHOW => tr(TextKey::Show),
        COLLECTION_TYPE_TELEVISION_SEASON => tr(TextKey::Season),
        COLLECTION_TYPE_ARTIST => tr(TextKey::Artist),
        COLLECTION_TYPE_MULTI_COLLECTION => tr(TextKey::MultiCollection),
        COLLECTION_TYPE_SMART_COLLECTION => tr(TextKey::SmartCollection),
        COLLECTION_TYPE_MOVIE => tr(TextKey::Movie),
        COLLECTION_TYPE_EPISODE => tr(TextKey::Episode),
        COLLECTION_TYPE_MUSIC_VIDEO => tr(TextKey::MusicVideo),
        COLLECTION_TYPE_OTHER_VIDEO => tr(TextKey::OtherVideo),
        COLLECTION_TYPE_SONG => tr(TextKey::Song),
        COLLECTION_TYPE_IMAGE => tr(TextKey::Image),
        COLLECTION_TYPE_REMOTE_STREAM => tr(TextKey::RemoteStream),
        _ => "",
    }
}

fn playback_order_label_i18n(playback_order: i32) -> &'static str {
    match playback_order {
        PLAYBACK_ORDER_CHRONOLOGICAL => tr(TextKey::Chronological),
        PLAYBACK_ORDER_RANDOM => tr(TextKey::Random),
        PLAYBACK_ORDER_SHUFFLE => tr(TextKey::Shuffle),
        PLAYBACK_ORDER_SHUFFLE_IN_ORDER => tr(TextKey::ShuffleInOrder),
        PLAYBACK_ORDER_SEASON_EPISODE => tr(TextKey::SeasonEpisode),
        _ => "",
    }
}

fn playlist_count_label(item: &PlaylistItemForm, _readonly: bool) -> String {
    if item.play_all {
        tr(TextKey::All).to_string()
    } else if let Some(count) = item.count {
        count_per_pass_label(count)
    } else {
        count_per_pass_label(1)
    }
}

fn count_per_pass_label(count: i32) -> String {
    format!("{count} {}", tr(TextKey::PerPass))
}

fn bool_label(value: bool) -> &'static str {
    if value {
        tr(TextKey::Yes)
    } else {
        tr(TextKey::No)
    }
}

fn individual_media_type(collection_type: i32) -> bool {
    matches!(
        collection_type,
        COLLECTION_TYPE_MOVIE
            | COLLECTION_TYPE_EPISODE
            | COLLECTION_TYPE_MUSIC_VIDEO
            | COLLECTION_TYPE_OTHER_VIDEO
            | COLLECTION_TYPE_SONG
            | COLLECTION_TYPE_IMAGE
            | COLLECTION_TYPE_REMOTE_STREAM
    )
}
