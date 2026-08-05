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

use crate::admin::i18n::{tr, TextKey};
use crate::admin::pages::media::add_to_schedule_modal::AddToScheduleModal;
use crate::admin::pages::media::multi_select_browser::{
    AddToCollectionModal, AddToPlaylistModal, SelectionToolbar,
};
use crate::admin::pages::media::multi_select_browser::{BrowserKindConfig, MultiSelectBrowserPage};
#[cfg(feature = "ssr")]
use crate::admin::server_fns::media_browser::get_artist_details;
use crate::admin::server_fns::media_browser::{
    ArtistDetailRow, ArtistMusicVideoRow, MediaBrowserKind,
};
use crate::admin::ui::primitives::resource_state::AdminErrorState;

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
    let artist = load_artist_detail_initial(id);

    match artist {
        Ok(Some(artist)) => view! { <ArtistDetailIsland artist/> }.into_any(),
        Ok(None) => view! {
            <div class=detail_style::not_found>
                <h1>{tr(TextKey::ArtistNotFound)}</h1>
                <a href="/media/music/artists">{tr(TextKey::BackToArtists)}</a>
            </div>
        }
        .into_any(),
        Err(error) => view! { <AdminErrorState error/> }.into_any(),
    }
}

#[island]
fn ArtistDetailIsland(artist: ArtistDetailRow) -> impl IntoView {
    view! { <ArtistDetailView artist/> }
}

fn load_artist_detail_initial(id: i64) -> Result<Option<ArtistDetailRow>, String> {
    #[cfg(feature = "ssr")]
    {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(async move { get_artist_details(id).await })
                .map_err(|error| error.to_string())
        })
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = id;
        Ok(None)
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
        versions,
    } = artist;

    let artist_selected: RwSignal<HashSet<i64>> = RwSignal::new(HashSet::from([id]));
    // Music-video rows share one selection set: row checkboxes toggle
    // membership for the bulk toolbar, while a per-row action replaces the
    // selection with that single row (matching the browser list behavior).
    let music_video_selected: RwSignal<HashSet<i64>> = RwSignal::new(HashSet::new());
    let collection_open = RwSignal::new(false);
    let music_video_collection_open = RwSignal::new(false);
    let music_video_playlist_open = RwSignal::new(false);
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
    let versions_style = if versions.is_empty() {
        "display:none"
    } else {
        ""
    };

    view! {
        <div class=detail_style::page>
            <div class=detail_style::fanart_container>
                <div class=detail_style::fanart_tint/>
                <img class=detail_style::fanart src=fanart_src alt=tr(TextKey::FanArt) style=fanart_style/>
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
                                {tr(TextKey::AddToCollection)}
                            </button>
                            <button
                                type="button"
                                class=style::action_btn
                                on:click=move |_| schedule_open.set(true)
                            >
                                {tr(TextKey::AddToSchedule)}
                            </button>
                            <a class=detail_style::btn_back href="/media/music/artists">{tr(TextKey::Back)}</a>
                        </div>
                    </div>
                </div>

                <dl class=detail_style::facts>
                    {linked_fact_row(TextKey::Languages, "language", &languages)}
                    {linked_fact_row(TextKey::Genres, "genre", &genres)}
                    {linked_fact_row(TextKey::Styles, "style", &styles)}
                    {linked_fact_row(TextKey::Moods, "mood", &moods)}
                </dl>

                <SelectionToolbar
                    selected=music_video_selected
                    collection_open=music_video_collection_open
                    playlist_open=music_video_playlist_open
                />

                <section class=style::music_video_section style=music_video_style>
                    <div class=style::music_video_stack>
                        {music_videos.into_iter().map(|video| {
                            let video_id = video.id;
                            view! {
                                <MusicVideoRow
                                    video=video
                                    selected=music_video_selected
                                    on_add_collection=Callback::new(move |(): ()| {
                                        music_video_selected.set(HashSet::from([video_id]));
                                        music_video_collection_open.set(true);
                                    })
                                />
                            }
                        }).collect_view()}
                    </div>
                </section>

                <section class=style::music_video_section style=versions_style>
                    <h2 class=style::section_title>{tr(TextKey::ContentVariantVersions)}</h2>
                    <div class=style::music_video_stack>
                        {versions.into_iter().map(|video| {
                            let video_id = video.id;
                            view! {
                                <MusicVideoRow
                                    video=video
                                    selected=music_video_selected
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

            <AddToCollectionModal open=collection_open selected=artist_selected/>
            <AddToCollectionModal open=music_video_collection_open selected=music_video_selected/>
            <AddToPlaylistModal
                open=music_video_playlist_open
                selected=music_video_selected
                kind=MediaBrowserKind::MusicVideo
            />
            <AddToScheduleModal
                open=schedule_open
                media_item_id=id
                collection_type=3
                success_label=tr(TextKey::AddedArtistToSchedule)
                redirect_after_add=true
            />
        </div>
    }
}

#[component]
fn MusicVideoRow(
    video: ArtistMusicVideoRow,
    selected: RwSignal<HashSet<i64>>,
    on_add_collection: Callback<()>,
) -> impl IntoView {
    let video_id = video.id;
    let is_selected = Signal::derive(move || selected.with(|s| s.contains(&video_id)));
    let toggle_selected = move || {
        selected.update(|s| {
            if !s.remove(&video_id) {
                s.insert(video_id);
            }
        });
    };
    let title = video.title.clone();
    let thumb_alt = format!("{} {}", title, tr(TextKey::Thumbnail));
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
        tr(TextKey::FileNotFound)
    } else if video.media_item_state == 2 {
        tr(TextKey::Unavailable)
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
                <label class=style::music_video_select aria-label=tr(TextKey::Select)>
                    <input
                        type="checkbox"
                        prop:checked=move || is_selected.get()
                        on:change=move |_| toggle_selected()
                    />
                </label>
                <div class=style::music_video_thumb_wrap>
                    <img class=style::music_video_thumb src=thumb_src alt=thumb_alt style=thumb_style/>
                    <div class=style::music_video_placeholder style=thumb_placeholder_style/>
                    <span class=style::warning_badge style=warning_style>"!"</span>
                </div>
                <div class=style::music_video_body>
                    <h2 class=style::music_video_title>
                        <a href=format!("/media/music/videos/{}", video.id)>{title}</a>
                    </h2>
                    <p class=style::fact_inline style=album_style>
                        {tr(TextKey::Album)} ": "
                        <a href=album_href>{album}</a>
                    </p>
                    <p class=style::music_video_plot style=plot_style>{plot}</p>
                    <div class=style::music_video_actions>
                        <button
                            type="button"
                            class=style::action_btn
                            on:click=move |_| on_add_collection.run(())
                        >
                            {tr(TextKey::AddToCollection)}
                        </button>
                    </div>
                </div>
            </div>
            <div class=style::warning_line style=warning_style>
                <span class=style::warning_icon>"!"</span>
                <span>{warning_label} ": "</span>
                <code>{warning_path}</code>
            </div>
        </article>
    }
}

fn linked_fact_row(label_key: TextKey, field: &'static str, values: &[String]) -> Option<AnyView> {
    if values.is_empty() {
        return None;
    }

    Some(
        view! {
            <div>
                <dt>{tr(label_key)}</dt>
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
