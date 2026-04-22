//! Trash page — port of `Pages/Trash.razor` for the currently implemented
//! media browsers. Displays file-not-found items grouped by media type and
//! allows deleting selected rows or emptying the trash entirely.
#![allow(clippy::too_many_lines)]

use std::collections::HashSet;

use leptos::prelude::*;

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
    title: &'static str,
    browse_href: &'static str,
    rows: Vec<MediaCardRow>,
}

#[component]
pub fn TrashPage() -> impl IntoView {
    let data: Resource<TrashPageData> = Resource::new(
        || (),
        |()| async move { list_trash_items().await.unwrap_or_default() },
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
                    let message = if count == 1 {
                        "Deleted 1 item from the database.".to_string()
                    } else {
                        format!("Deleted {count} items from the database.")
                    };
                    toast.push(message, ToastSeverity::Success);
                    selected_ids.set(HashSet::new());
                    single_delete_target.set(None);
                    data.refetch();
                }
                Err(error) => {
                    toast.push(format!("Delete failed: {error}"), ToastSeverity::Error);
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
                        "Trash is already empty.".to_string()
                    } else {
                        format!("Emptied trash: deleted {count} items.")
                    };
                    toast.push(message, ToastSeverity::Success);
                    selected_ids.set(HashSet::new());
                    data.refetch();
                }
                Err(error) => {
                    toast.push(format!("Empty trash failed: {error}"), ToastSeverity::Error);
                }
            }
            empty_trash_open.set(false);
        }
    });

    let selected_count = Signal::derive(move || selected_ids.with(HashSet::len));

    view! {
        <div class=style::page>
            <div class=style::toolbar>
                <div>
                    <h1 class=style::page_title>"Trash"</h1>
                    <p class=style::subtitle>
                        "Items whose source files are currently missing from disk or the provider."
                    </p>
                </div>
                <div class=style::toolbar_actions>
                    <button
                        class=style::btn_secondary
                        disabled=move || selected_count.get() == 0
                        on:click=move |_| bulk_delete_open.set(true)
                    >
                        {move || format!("Delete Selected ({})", selected_count.get())}
                    </button>
                    <button
                        class=style::btn_danger
                        on:click=move |_| empty_trash_open.set(true)
                    >
                        "Empty Trash"
                    </button>
                </div>
            </div>

            <Suspense fallback=move || view! { <p class=style::empty>"Loading…"</p> }>
                {move || {
                    data.get().map(|trash| {
                        let sections = build_sections(trash);
                        if sections.is_empty() {
                            return view! {
                                <p class=style::empty>"Trash is empty."</p>
                            }
                            .into_any();
                        }

                        let nav_sections = sections.clone();
                        view! {
                            <div class=style::section_links>
                                {nav_sections.into_iter().map(|section| {
                                    view! {
                                        <a href=format!("#{}", section.key) class=style::section_link>
                                            {format!("{} ({})", section.title, section.rows.len())}
                                        </a>
                                    }
                                }).collect_view()}
                            </div>

                            {sections.into_iter().map(|section| {
                                let section_key = section.key;
                                let section_title = section.title;
                                let browse_href = section.browse_href;
                                let rows = section.rows;
                                view! {
                                    <section id=section_key class=style::section>
                                        <div class=style::section_header>
                                            <h2 class=style::section_title>
                                                {format!("{} ({})", section_title, rows.len())}
                                            </h2>
                                            <a href=browse_href class=style::browse_link>
                                                "Open browser"
                                            </a>
                                        </div>
                                        <div class=style::grid>
                                            {rows.into_iter().map(|row| {
                                                let item_id = row.id;
                                                let item_title = row.title.clone();
                                                let subtitle = row.subtitle.clone();
                                                let artwork_path = row.artwork_path.clone();
                                                view! {
                                                    <div class=style::card_shell>
                                                        <MediaCard
                                                            title=item_title.clone()
                                                            subtitle=subtitle
                                                            artwork_url=artwork_path
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
                                                                "Select"
                                                            </label>
                                                            <button
                                                                class=style::btn_delete_item
                                                                on:click=move |_| {
                                                                    single_delete_target
                                                                        .set(Some((item_id, item_title.clone())));
                                                                    single_delete_open.set(true);
                                                                }
                                                            >
                                                                "Delete"
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

            {move || {
                single_delete_target.get().map(|(item_id, item_title)| {
                    view! {
                        <DeleteDialog
                            open=single_delete_open
                            entity_name=item_title
                            on_confirm=Callback::new(move |confirmed: bool| {
                                if confirmed {
                                    delete_action.dispatch(vec![item_id]);
                                } else {
                                    single_delete_open.set(false);
                                }
                            })
                        />
                    }
                })
            }}

            {move || {
                let count = selected_count.get();
                (count > 0).then(|| {
                    let ids = selected_ids.with(|set| set.iter().copied().collect::<Vec<_>>());
                    view! {
                        <DeleteDialog
                            open=bulk_delete_open
                            entity_name=format!("{count} selected items")
                            on_confirm=Callback::new(move |confirmed: bool| {
                                if confirmed {
                                    delete_action.dispatch(ids.clone());
                                } else {
                                    bulk_delete_open.set(false);
                                }
                            })
                        />
                    }
                })
            }}

            <Modal
                open=empty_trash_open
                title="Empty Trash".to_string()
                on_close=Callback::new(move |confirmed: bool| {
                    if confirmed {
                        empty_action.dispatch(());
                    } else {
                        empty_trash_open.set(false);
                    }
                })
                confirm_label="Delete All"
                destructive=true
            >
                <p>"Delete every file-not-found item from the database?"</p>
                <p class=style::modal_note>
                    "This permanently removes the currently missing media items and their dependent metadata rows."
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
            title: "Movies",
            browse_href: "/media/movies",
            rows: data.movies,
        });
    }

    if !data.shows.is_empty() {
        sections.push(TrashSection {
            key: "shows",
            title: "TV Shows",
            browse_href: "/media/tv/shows",
            rows: data.shows,
        });
    }

    if !data.artists.is_empty() {
        sections.push(TrashSection {
            key: "artists",
            title: "Artists",
            browse_href: "/media/music/artists",
            rows: data.artists,
        });
    }

    if !data.other_videos.is_empty() {
        sections.push(TrashSection {
            key: "other-videos",
            title: "Other Videos",
            browse_href: "/media/other/videos",
            rows: data.other_videos,
        });
    }

    if !data.songs.is_empty() {
        sections.push(TrashSection {
            key: "songs",
            title: "Songs",
            browse_href: "/media/music/songs",
            rows: data.songs,
        });
    }

    if !data.images.is_empty() {
        sections.push(TrashSection {
            key: "images",
            title: "Images",
            browse_href: "/media/browser/images",
            rows: data.images,
        });
    }

    sections
}
