//! Artist browser and detail page.
//!
//! * `/media/music/artists` — port of `Pages/ArtistList.razor`, delegates to
//!   the shared [`MultiSelectBrowserPage`].
//! * `/media/music/artists/:id` — port of `Pages/Artist.razor`: fanart /
//!   thumbnail header, disambiguation, 400-character biography excerpt,
//!   linked Languages / Genres / Styles / Moods, artist Add To actions, and
//!   the first 100 music videos with per-video collection actions.
//!
//! `ArtistCardViewModel.HasMediaInfo = false`, so the per-card "Show Media
//! Info" entry is suppressed (artists are containers; their music videos
//! carry the media-info metadata).
#![allow(clippy::too_many_lines)]

use std::collections::HashSet;

use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::admin::pages::media::add_to_schedule_modal::AddToScheduleModal;
use crate::admin::pages::media::multi_select_browser::AddToCollectionModal;
use crate::admin::pages::media::multi_select_browser::{BrowserKindConfig, MultiSelectBrowserPage};
use crate::admin::server_fns::media_browser::{
    get_artist_details, ArtistDetailRow, ArtistMusicVideoRow,
};

#[allow(dead_code)]
mod detail_style {
    stylance::import_crate_style!(s, "src/admin/pages/media/movie_detail.module.css");
    pub use s::*;
}
stylance::import_crate_style!(style, "src/admin/pages/media/artist_detail.module.css");

#[component]
pub fn ArtistsPage() -> impl IntoView {
    view! { <MultiSelectBrowserPage config=BrowserKindConfig::ARTISTS/> }
}

#[component]
pub fn ArtistPage() -> impl IntoView {
    let params = use_params_map();
    let id = params.with_untracked(|p| {
        p.get("id")
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or_default()
    });

    view! { <ArtistDetailIsland id/> }
}

#[island]
fn ArtistDetailIsland(id: i64) -> impl IntoView {
    let data: Resource<Option<ArtistDetailRow>> = Resource::new_blocking(
        move || id,
        |id| async move { get_artist_details(id).await.ok().flatten() },
    );

    view! {
        <Suspense fallback=move || view! { <p class=detail_style::loading>"Loading…"</p> }>
            {move || {
                match data.get() {
                    Some(Some(artist)) => view! { <ArtistDetailView artist/> }.into_any(),
                    Some(None) => view! {
                        <div class=detail_style::not_found>
                            <h1>"Artist not found"</h1>
                            <a href="/media/music/artists">"Back to artists"</a>
                        </div>
                    }
                    .into_any(),
                    None => view! { <p class=detail_style::loading>"Loading…"</p> }.into_any(),
                }
            }}
        </Suspense>
    }
}

#[component]
fn ArtistDetailView(artist: ArtistDetailRow) -> impl IntoView {
    let ArtistDetailRow {
        id,
        title,
        disambiguation,
        biography,
        thumbnail_url,
        fanart_url,
        genres,
        styles,
        moods,
        languages,
        music_videos,
    } = artist;

    let artist_selected: RwSignal<HashSet<i64>> = RwSignal::new(HashSet::from([id]));
    let music_video_selected: RwSignal<HashSet<i64>> = RwSignal::new(HashSet::new());
    let collection_open = RwSignal::new(false);
    let music_video_collection_open = RwSignal::new(false);
    let schedule_open = RwSignal::new(false);
    let title_for_thumb = title.clone();
    let bio_text = biography
        .map(|value| truncate_chars(&value, 400))
        .unwrap_or_default();
    let bio_style = if bio_text.trim().is_empty() {
        "display:none"
    } else {
        ""
    };
    let subtitle_text = disambiguation.unwrap_or_default();
    let subtitle_style = if subtitle_text.trim().is_empty() {
        "display:none"
    } else {
        ""
    };
    let fanart_src = fanart_url.unwrap_or_default();
    let fanart_style = if fanart_src.trim().is_empty() {
        "display:none"
    } else {
        ""
    };
    let thumb_src = thumbnail_url.unwrap_or_default();
    let thumb_style = if thumb_src.trim().is_empty() {
        "display:none"
    } else {
        ""
    };
    let thumb_placeholder_style = if thumb_src.trim().is_empty() {
        ""
    } else {
        "display:none"
    };
    let music_video_style = if music_videos.is_empty() {
        "display:none"
    } else {
        ""
    };

    view! {
        <div class=detail_style::page>
            <div class=detail_style::fanart_container>
                <div class=detail_style::fanart_tint/>
                <img class=detail_style::fanart src=fanart_src alt="fan art" style=fanart_style/>
            </div>

            <div class=detail_style::content>
                <div class=detail_style::hero>
                    <div class=style::artist_thumb_wrap>
                        <img class=style::artist_thumb src=thumb_src alt=title_for_thumb style=thumb_style/>
                        <div class=style::artist_thumb_placeholder style=thumb_placeholder_style/>
                    </div>
                    <div class=detail_style::header>
                        <h1 class=detail_style::title>{title.clone()}</h1>
                        <p class=detail_style::year style=subtitle_style>{subtitle_text}</p>
                        <div class=detail_style::plot_card style=bio_style>
                            <p class=detail_style::plot>{bio_text}</p>
                        </div>
                        <div class=style::action_bar>
                            <button
                                type="button"
                                class=style::action_btn
                                on:click=move |_| {
                                    artist_selected.set(HashSet::from([id]));
                                    collection_open.set(true);
                                }
                            >
                                "Add To Collection"
                            </button>
                            <button
                                type="button"
                                class=style::action_btn
                                on:click=move |_| schedule_open.set(true)
                            >
                                "Add To Schedule"
                            </button>
                            <a class=detail_style::btn_back href="/media/music/artists">"Back"</a>
                        </div>
                    </div>
                </div>

                <dl class=detail_style::facts>
                    {linked_fact_row("Languages", "language", &languages)}
                    {linked_fact_row("Genres", "genre", &genres)}
                    {linked_fact_row("Styles", "style", &styles)}
                    {linked_fact_row("Moods", "mood", &moods)}
                </dl>

                <section class=style::music_video_section style=music_video_style>
                    <div class=style::music_video_stack>
                        {music_videos.into_iter().map(|video| {
                            let video_id = video.id;
                            view! {
                                <MusicVideoRow
                                    video=video
                                    on_add_collection=Callback::new(move |(): ()| {
                                        music_video_selected.set(HashSet::from([video_id]));
                                        music_video_collection_open.set(true);
                                    })
                                />
                            }
                        }).collect_view()}
                    </div>
                </section>
            </div>

            <AddToCollectionModal
                open=collection_open
                selected=artist_selected
                redirect_after_add=true
            />
            <AddToCollectionModal open=music_video_collection_open selected=music_video_selected/>
            <AddToScheduleModal
                open=schedule_open
                media_item_id=id
                collection_type=3
                success_label="Added artist to schedule"
                redirect_after_add=true
            />
        </div>
    }
}

#[component]
fn MusicVideoRow(video: ArtistMusicVideoRow, on_add_collection: Callback<()>) -> impl IntoView {
    let title = video.title.clone();
    let thumb_alt = format!("{title} thumbnail");
    let album = video.album.unwrap_or_default();
    let album_style = if album.trim().is_empty() {
        "display:none"
    } else {
        ""
    };
    let album_href = search_link("album", &album);
    let plot = video.plot.unwrap_or_default();
    let plot_style = if plot.trim().is_empty() {
        "display:none"
    } else {
        ""
    };
    let thumb_src = video.thumbnail_url.unwrap_or_default();
    let thumb_style = if thumb_src.trim().is_empty() {
        "display:none"
    } else {
        ""
    };
    let thumb_placeholder_style = if thumb_src.trim().is_empty() {
        ""
    } else {
        "display:none"
    };
    let warning_style = if matches!(video.media_item_state, 1 | 2) {
        ""
    } else {
        "display:none"
    };
    let warning_label = if video.media_item_state == 1 {
        "File Not Found: "
    } else if video.media_item_state == 2 {
        "Unavailable: "
    } else {
        ""
    };
    let warning_path = if video.media_item_state == 1 {
        video.file_path.unwrap_or_default()
    } else if video.media_item_state == 2 {
        video.local_path.unwrap_or_default()
    } else {
        String::new()
    };

    view! {
        <article class=style::music_video_card id=format!("music-video-{}", video.id)>
            <div class=style::music_video_main>
                <div class=style::music_video_thumb_wrap>
                    <img class=style::music_video_thumb src=thumb_src alt=thumb_alt style=thumb_style/>
                    <div class=style::music_video_placeholder style=thumb_placeholder_style/>
                    <span class=style::warning_badge style=warning_style>"!"</span>
                </div>
                <div class=style::music_video_body>
                    <h2 class=style::music_video_title>{title}</h2>
                    <p class=style::fact_inline style=album_style>
                        "Album: "
                        <a href=album_href>{album}</a>
                    </p>
                    <p class=style::music_video_plot style=plot_style>{plot}</p>
                    <div class=style::music_video_actions>
                        <button
                            type="button"
                            class=style::action_btn
                            on:click=move |_| on_add_collection.run(())
                        >
                            "Add To Collection"
                        </button>
                    </div>
                </div>
            </div>
            <div class=style::warning_line style=warning_style>
                <span class=style::warning_icon>"!"</span>
                <span>{warning_label}</span>
                <code>{warning_path}</code>
            </div>
        </article>
    }
}

fn linked_fact_row(label: &'static str, field: &'static str, values: &[String]) -> Option<AnyView> {
    if values.is_empty() {
        return None;
    }

    Some(
        view! {
            <div>
                <dt>{label}</dt>
                <dd>
                    {values.iter().enumerate().map(|(index, value)| {
                        view! {
                            {if index > 0 { Some(view! { <span>", "</span> }) } else { None }}
                            <a href=search_link(field, value)>{value.clone()}</a>
                        }
                    }).collect_view()}
                </dd>
            </div>
        }
        .into_any(),
    )
}

fn search_link(field: &str, value: &str) -> String {
    let query = format!(r#"{field}:"{}""#, value.to_lowercase());
    format!("/search?query={}", urlencoding::encode(&query))
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let mut out: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        out.push_str("...");
    }
    out
}
