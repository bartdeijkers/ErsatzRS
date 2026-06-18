//! Trash page — port of `Pages/Trash.razor` for the currently implemented
//! media browsers. Displays file-not-found items grouped by media type and
//! allows deleting selected rows or emptying the trash entirely.
#![allow(clippy::too_many_lines)]

use std::collections::HashSet;

use leptos::prelude::*;

use crate::admin::i18n::{
    delete_failed, delete_selected_count, deleted_items_from_database, emptied_trash_count,
    empty_trash_failed, media_browser_error_message, selected_items_count, tr, TextKey,
};
#[cfg(target_arch = "wasm32")]
use crate::admin::server_fns::layout::media_state_revision;
use crate::admin::server_fns::media_browser::{
    delete_media_items, empty_trash, list_trash_items, MediaCardRow, TrashPageData,
};
use crate::admin::ui::primitives::card::MediaCard;
use crate::admin::ui::primitives::dialog::{DeleteDialog, Modal};
use crate::admin::ui::primitives::toast::{use_toast, ToastSeverity};

stylance::import_crate_style!(style, "src/admin/pages/media/trash.module.css");

#[derive(Clone)]
struct TrashSection {
    key: &'static str,
    title_key: TextKey,
    browse_href: &'static str,
    rows: Vec<MediaCardRow>,
}

#[component]
pub fn TrashPage() -> impl IntoView {
    view! { <TrashIsland/> }
}

#[island]
fn TrashIsland() -> impl IntoView {
    let media_revision = RwSignal::new(String::new());
    let data: Resource<TrashPageData> = Resource::new(
        move || media_revision.get(),
        |_| async move { list_trash_items().await.unwrap_or_default() },
    );

    let selected_ids = RwSignal::new(HashSet::<i64>::new());
    let single_delete_target = RwSignal::new(None::<(i64, String)>);
    let single_delete_open = RwSignal::new(false);
    let bulk_delete_open = RwSignal::new(false);
    let empty_trash_open = RwSignal::new(false);

    let delete_action = Action::new(|ids: &Vec<i64>| {
        let ids = ids.clone();
        async move { delete_media_items(ids).await }
    });
    let empty_action = Action::new(|(): &()| async move { empty_trash().await });

    let toast = use_toast();

    Effect::new(move |_| {
        if let Some(result) = delete_action.value().get() {
            match result {
                Ok(count) => {
                    toast.push(deleted_items_from_database(count), ToastSeverity::Success);
                    selected_ids.set(HashSet::new());
                    single_delete_target.set(None);
                    data.refetch();
                }
                Err(error) => {
                    toast.push(
                        delete_failed(media_browser_error_message(error)),
                        ToastSeverity::Error,
                    );
                }
            }
            single_delete_open.set(false);
            bulk_delete_open.set(false);
        }
    });

    Effect::new(move |_| {
        if let Some(result) = empty_action.value().get() {
            match result {
                Ok(count) => {
                    let message = if count == 0 {
                        tr(TextKey::TrashAlreadyEmpty).to_owned()
                    } else {
                        emptied_trash_count(count)
                    };
                    toast.push(message, ToastSeverity::Success);
                    selected_ids.set(HashSet::new());
                    data.refetch();
                }
                Err(error) => {
                    toast.push(
                        empty_trash_failed(media_browser_error_message(error)),
                        ToastSeverity::Error,
                    );
                }
            }
            empty_trash_open.set(false);
        }
    });

    let selected_count = Signal::derive(move || selected_ids.with(HashSet::len));

    Effect::new(move |_| {
        #[cfg(target_arch = "wasm32")]
        {
            leptos::task::spawn_local(async move {
                loop {
                    if let Ok(next) = media_state_revision().await {
                        if media_revision.get_untracked() != next {
                            media_revision.set(next);
                        }
                    }
                    gloo_timers::future::TimeoutFuture::new(1_500).await;
                }
            });
        }
    });

    view! {
        <div class=style::page>
            <div class=style::toolbar>
                <div>
                    <h1 class=style::page_title>{tr(TextKey::Trash)}</h1>
                    <p class=style::subtitle>
                        {tr(TextKey::TrashSubtitle)}
                    </p>
                </div>
                <div class=style::toolbar_actions>
                    <button
                        class=style::btn_secondary
                        disabled=move || selected_count.get() == 0
                        on:click=move |_| bulk_delete_open.set(true)
                    >
                        {move || delete_selected_count(selected_count.get())}
                    </button>
                    <button
                        class=style::btn_danger
                        on:click=move |_| empty_trash_open.set(true)
                    >
                        {tr(TextKey::EmptyTrash)}
                    </button>
                </div>
            </div>

            <Suspense fallback=move || view! { <p class=style::empty>{tr(TextKey::Loading)}</p> }>
                {move || {
                    data.get().map(|trash| {
                        let sections = build_sections(trash);
                        if sections.is_empty() {
                            return view! {
                                <p class=style::empty>{tr(TextKey::TrashEmpty)}</p>
                            }
                            .into_any();
                        }

                        let nav_sections = sections.clone();
                        view! {
                            <div class=style::section_links>
                                {nav_sections.into_iter().map(|section| {
                                    let section_title = tr(section.title_key);
                                    view! {
                                        <a href=format!("#{}", section.key) class=style::section_link>
                                            {format!("{} ({})", section_title, section.rows.len())}
                                        </a>
                                    }
                                }).collect_view()}
                            </div>

                            {sections.into_iter().map(|section| {
                                let section_key = section.key;
                                let section_title = tr(section.title_key);
                                let browse_href = section.browse_href;
                                let rows = section.rows;
                                view! {
                                    <section id=section_key class=style::section>
                                        <div class=style::section_header>
                                            <h2 class=style::section_title>
                                                {format!("{} ({})", section_title, rows.len())}
                                            </h2>
                                            <a href=browse_href class=style::browse_link>
                                                {tr(TextKey::OpenBrowser)}
                                            </a>
                                        </div>
                                        <div class=style::grid>
                                            {rows.into_iter().map(|row| {
                                                let item_id = row.id;
                                                let item_title = row.title.clone();
                                                let subtitle = row.subtitle.clone();
                                                let artwork_path = row.artwork_path.clone();
                                                let placeholder = crate::admin::ui::primitives::card::first_letter_placeholder(&row.sort_title);
                                                view! {
                                                    <div class=style::card_shell>
                                                        <MediaCard
                                                            title=item_title.clone()
                                                            subtitle=subtitle
                                                            artwork_url=artwork_path
                                                            placeholder=placeholder
                                                        />
                                                        <div class=style::card_actions>
                                                            <label class=style::checkbox_label>
                                                                <input
                                                                    type="checkbox"
                                                                    prop:checked=move || {
                                                                        selected_ids.with(|ids| ids.contains(&item_id))
                                                                    }
                                                                    on:change=move |event| {
                                                                        let checked = event_target_checked(&event);
                                                                        selected_ids.update(|ids| {
                                                                            if checked {
                                                                                ids.insert(item_id);
                                                                            } else {
                                                                                ids.remove(&item_id);
                                                                            }
                                                                        });
                                                                    }
                                                                />
                                                                {tr(TextKey::Select)}
                                                            </label>
                                                            <button
                                                                class=style::btn_delete_item
                                                                on:click=move |_| {
                                                                    single_delete_target
                                                                        .set(Some((item_id, item_title.clone())));
                                                                    single_delete_open.set(true);
                                                                }
                                                            >
                                                                {tr(TextKey::Delete)}
                                                            </button>
                                                        </div>
                                                    </div>
                                                }
                                            }).collect_view()}
                                        </div>
                                    </section>
                                }
                            }).collect_view()}
                        }
                        .into_any()
                    })
                }}
            </Suspense>

            <DeleteDialog
                open=single_delete_open
                entity_name=Signal::derive(move || {
                    single_delete_target.get().map(|(_, title)| title).unwrap_or_default()
                })
                on_confirm=Callback::new(move |confirmed: bool| {
                    if let Some((item_id, _)) = single_delete_target.get_untracked() {
                        if confirmed {
                            delete_action.dispatch(vec![item_id]);
                        } else {
                            single_delete_open.set(false);
                        }
                    } else {
                        single_delete_open.set(false);
                    }
                })
            />
            <DeleteDialog
                open=bulk_delete_open
                entity_name=Signal::derive(move || {
                    selected_items_count(selected_count.get())
                })
                on_confirm=Callback::new(move |confirmed: bool| {
                    if confirmed {
                        let ids = selected_ids.with(|set| set.iter().copied().collect::<Vec<_>>());
                        delete_action.dispatch(ids);
                    } else {
                        bulk_delete_open.set(false);
                    }
                })
            />

            <Modal
                open=empty_trash_open
                title=tr(TextKey::EmptyTrash).to_owned()
                on_close=Callback::new(move |confirmed: bool| {
                    if confirmed {
                        empty_action.dispatch(());
                    } else {
                        empty_trash_open.set(false);
                    }
                })
                confirm_label=Some(tr(TextKey::DeleteAll))
                destructive=true
            >
                <p>{tr(TextKey::TrashEmptyConfirm)}</p>
                <p class=style::modal_note>
                    {tr(TextKey::TrashEmptyConfirmHelp)}
                </p>
            </Modal>
        </div>
    }
}

fn build_sections(data: TrashPageData) -> Vec<TrashSection> {
    let mut sections = Vec::new();

    if !data.movies.is_empty() {
        sections.push(TrashSection {
            key: "movies",
            title_key: TextKey::Movies,
            browse_href: "/media/movies",
            rows: data.movies,
        });
    }

    if !data.shows.is_empty() {
        sections.push(TrashSection {
            key: "shows",
            title_key: TextKey::TvShows,
            browse_href: "/media/tv/shows",
            rows: data.shows,
        });
    }

    if !data.seasons.is_empty() {
        sections.push(TrashSection {
            key: "seasons",
            title_key: TextKey::Seasons,
            browse_href: "/media/tv/seasons",
            rows: data.seasons,
        });
    }

    if !data.episodes.is_empty() {
        sections.push(TrashSection {
            key: "episodes",
            title_key: TextKey::Episodes,
            browse_href: "/media/tv/episodes",
            rows: data.episodes,
        });
    }

    if !data.artists.is_empty() {
        sections.push(TrashSection {
            key: "artists",
            title_key: TextKey::Artists,
            browse_href: "/media/music/artists",
            rows: data.artists,
        });
    }

    if !data.music_videos.is_empty() {
        sections.push(TrashSection {
            key: "music-videos",
            title_key: TextKey::MusicVideos,
            browse_href: "/media/music/videos",
            rows: data.music_videos,
        });
    }

    if !data.other_videos.is_empty() {
        sections.push(TrashSection {
            key: "other-videos",
            title_key: TextKey::OtherVideos,
            browse_href: "/media/other/videos",
            rows: data.other_videos,
        });
    }

    if !data.songs.is_empty() {
        sections.push(TrashSection {
            key: "songs",
            title_key: TextKey::Songs,
            browse_href: "/media/music/songs",
            rows: data.songs,
        });
    }

    if !data.images.is_empty() {
        sections.push(TrashSection {
            key: "images",
            title_key: TextKey::Images,
            browse_href: "/media/browser/images",
            rows: data.images,
        });
    }

    if !data.remote_streams.is_empty() {
        sections.push(TrashSection {
            key: "remote-streams",
            title_key: TextKey::RemoteStreams,
            browse_href: "/media/remote/streams",
            rows: data.remote_streams,
        });
    }

    sections
}
