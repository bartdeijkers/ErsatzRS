//! TV season detail page.
//!
//! `/media/tv/seasons/:id` ports
//! `ErsatzTV/Pages/TelevisionEpisodeList.razor`: fanart/poster season header,
//! season-level Add To actions, and the vertical episode card list with
//! per-episode Add To, media-info, troubleshooting, file-state, release date,
//! director, and writer rows.
#![allow(clippy::too_many_lines)]

use std::collections::HashSet;

use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::admin::i18n::{season_display_name, tr, TextKey};
use crate::admin::pages::media::add_to_schedule_modal::AddToScheduleModal;
use crate::admin::pages::media::multi_select_browser::{
    AddToCollectionModal, AddToPlaylistModal, MediaInfoModal,
};
use crate::admin::pages::media::tv_specials::TvSpecialFeatureRow;
use crate::admin::server_fns::media_browser::{
    get_media_item_info_json, EpisodeCardRow, MediaBrowserKind, SeasonDetailRow,
};
use crate::admin::ui::primitives::toast::{use_toast, ToastSeverity};

stylance::import_crate_style!(
    shell_style,
    "src/admin/pages/media/season_detail_shell.module.css"
);
stylance::import_crate_style!(style, "src/admin/pages/media/season_detail.module.css");

#[component]
pub fn SeasonPage() -> impl IntoView {
    let params = use_params_map();
    let id = params.with_untracked(|p| {
        p.get("id")
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or_default()
    });
    let season = load_season_detail_initial(id);

    match season {
        Some(season) => view! { <SeasonDetailIsland season/> }.into_any(),
        None => view! {
            <div class=shell_style::not_found>
                <h1>{tr(TextKey::SeasonNotFound)}</h1>
                <a href="/media/tv/shows">{tr(TextKey::BackToTvShows)}</a>
            </div>
        }
        .into_any(),
    }
}

#[island]
fn SeasonDetailIsland(season: SeasonDetailRow) -> impl IntoView {
    view! { <SeasonDetailView season/> }
}

fn load_season_detail_initial(id: i64) -> Option<SeasonDetailRow> {
    #[cfg(feature = "ssr")]
    {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                crate::admin::server_fns::media_browser::get_season_details(id)
                    .await
                    .ok()
                    .flatten()
            })
        })
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = id;
        None
    }
}

#[component]
fn SeasonDetailView(season: SeasonDetailRow) -> impl IntoView {
    let SeasonDetailRow {
        id,
        show_id,
        show_title,
        show_year: _,
        name: _,
        season_number,
        poster_url,
        fanart_url,
        episode_count: _,
        episodes,
        extras,
    } = season;

    let season_collection_selected: RwSignal<HashSet<i64>> = RwSignal::new(HashSet::from([id]));
    let season_playlist_selected: RwSignal<HashSet<i64>> = RwSignal::new(HashSet::from([id]));
    let episode_collection_selected: RwSignal<HashSet<i64>> = RwSignal::new(HashSet::new());
    let episode_playlist_selected: RwSignal<HashSet<i64>> = RwSignal::new(HashSet::new());
    let extra_selected: RwSignal<HashSet<i64>> = RwSignal::new(HashSet::new());

    let season_collection_open = RwSignal::new(false);
    let season_playlist_open = RwSignal::new(false);
    let episode_collection_open = RwSignal::new(false);
    let episode_playlist_open = RwSignal::new(false);
    let extra_collection_open = RwSignal::new(false);
    let extra_playlist_open = RwSignal::new(false);
    let schedule_open = RwSignal::new(false);

    let media_info_open = RwSignal::new(false);
    let media_info_title = RwSignal::new(String::new());
    let media_info_body = RwSignal::new(String::new());
    let toast = use_toast();
    let media_info_action = Action::new(move |target: &(i64, String)| {
        let (episode_id, title) = target.clone();
        async move {
            get_media_item_info_json(episode_id)
                .await
                .map(|json| (title, json))
        }
    });
    Effect::new(move |_| {
        if let Some(result) = media_info_action.value().get() {
            match result {
                Ok((title, json)) => {
                    media_info_title.set(title);
                    media_info_body.set(json);
                }
                Err(error) => {
                    media_info_body.set(format!("{}: {error}", tr(TextKey::UnableToLoadMediaInfo)));
                    toast.push(tr(TextKey::UnableToLoadMediaInfo), ToastSeverity::Error);
                }
            }
        }
    });
    let show_media_info = Callback::new(move |(media_item_id, title): (i64, String)| {
        media_info_title.set(title.clone());
        media_info_body.set(tr(TextKey::LoadingMediaInfo).to_string());
        media_info_open.set(true);
        media_info_action.dispatch((media_item_id, title));
    });

    let show_href = format!("/media/tv/shows/{show_id}");
    let show_href_for_back = show_href.clone();
    let localized_season_name = season_display_name(season_number);
    let poster_alt = format!("{show_title} {localized_season_name}");

    view! {
        <div class=shell_style::page>
            <div class=shell_style::fanart_container>
                <div class=shell_style::fanart_tint/>
                {fanart_url.map(|src| view! {
                    <img class=shell_style::fanart src=src alt=tr(TextKey::FanArt)/>
                })}
            </div>

            <div class=shell_style::content>
                <div class=shell_style::hero>
                    {poster_url.map(move |src| view! {
                        <div class=shell_style::poster_wrap>
                            <img class=shell_style::poster src=src alt=poster_alt/>
                        </div>
                    })}
                    <div class=shell_style::header>
                        <a class=style::show_link href=show_href>
                            <h1 class=shell_style::title>{show_title.clone()}</h1>
                        </a>
                        <p class=shell_style::year>{localized_season_name.clone()}</p>
                        <div class=shell_style::action_bar>
                            <button
                                type="button"
                                class=shell_style::action_btn
                                on:click=move |_| {
                                    season_collection_selected.set(HashSet::from([id]));
                                    season_collection_open.set(true);
                                }
                            >
                                {tr(TextKey::AddToCollection)}
                            </button>
                            <button
                                type="button"
                                class=shell_style::action_btn
                                on:click=move |_| {
                                    season_playlist_selected.set(HashSet::from([id]));
                                    season_playlist_open.set(true);
                                }
                            >
                                {tr(TextKey::AddToPlaylist)}
                            </button>
                            <button
                                type="button"
                                class=shell_style::action_btn
                                on:click=move |_| schedule_open.set(true)
                            >
                                {tr(TextKey::AddToSchedule)}
                            </button>
                            <a class=shell_style::btn_back href=show_href_for_back>{tr(TextKey::Back)}</a>
                        </div>
                    </div>
                </div>

                <div class=style::episode_stack>
                    {episodes.into_iter().map(|episode| {
                        let episode_id = episode.episode_id;
                        let title_for_info = format!(
                            "{} - s{:02}e{:02} - {}",
                            episode.show_title,
                            episode.season_number,
                            episode.episode_number,
                            episode.title
                        );
                        view! {
                            <EpisodeRow
                                episode=episode
                                on_add_collection=Callback::new(move |episode_id: i64| {
                                    episode_collection_selected.set(HashSet::from([episode_id]));
                                    episode_collection_open.set(true);
                                })
                                on_add_playlist=Callback::new(move |episode_id: i64| {
                                    episode_playlist_selected.set(HashSet::from([episode_id]));
                                    episode_playlist_open.set(true);
                                })
                                on_show_info=Callback::new(move |(): ()| {
                                    media_info_title.set(title_for_info.clone());
                                    media_info_body.set(tr(TextKey::LoadingMediaInfo).to_string());
                                    media_info_open.set(true);
                                    media_info_action.dispatch((episode_id, title_for_info.clone()));
                                })
                            />
                        }
                    }).collect_view()}
                </div>

                {(!extras.is_empty()).then(|| view! {
                    <h2 class=shell_style::section_title>{tr(TextKey::ContentVariantSpecialFeatures)}</h2>
                    <div class=shell_style::divider/>
                    <div class=style::episode_stack>
                        {extras.into_iter().map(|extra| {
                            view! {
                                <TvSpecialFeatureRow
                                    extra=extra
                                    on_add_collection=Callback::new(move |extra_id: i64| {
                                        extra_selected.set(HashSet::from([extra_id]));
                                        extra_collection_open.set(true);
                                    })
                                    on_add_playlist=Callback::new(move |extra_id: i64| {
                                        extra_selected.set(HashSet::from([extra_id]));
                                        extra_playlist_open.set(true);
                                    })
                                    on_show_info=show_media_info
                                />
                            }
                        }).collect_view()}
                    </div>
                })}
            </div>

            <AddToCollectionModal open=season_collection_open selected=season_collection_selected/>
            <AddToPlaylistModal
                open=season_playlist_open
                selected=season_playlist_selected
                kind=MediaBrowserKind::Season
            />
            <AddToCollectionModal open=episode_collection_open selected=episode_collection_selected/>
            <AddToPlaylistModal
                open=episode_playlist_open
                selected=episode_playlist_selected
                kind=MediaBrowserKind::Episode
            />
            <AddToCollectionModal open=extra_collection_open selected=extra_selected/>
            <AddToPlaylistModal
                open=extra_playlist_open
                selected=extra_selected
                kind=MediaBrowserKind::OtherVideo
            />
            <AddToScheduleModal
                open=schedule_open
                media_item_id=id
                collection_type=2
                success_label=tr(TextKey::AddedSeasonToSchedule)
            />
            <MediaInfoModal open=media_info_open title=media_info_title body=media_info_body/>
        </div>
    }
}

#[component]
fn EpisodeRow(
    episode: EpisodeCardRow,
    on_add_collection: Callback<i64>,
    on_add_playlist: Callback<i64>,
    on_show_info: Callback<()>,
) -> impl IntoView {
    let episode_id = episode.episode_id;
    let title = episode.title.clone();
    let thumbnail_alt = format!("{} {}", episode.title, tr(TextKey::Thumbnail));
    let troubleshoot_href = format!("/system/troubleshooting/playback?mediaItem={episode_id}");
    let status = file_status(&episode);
    let mut directors = episode.directors.clone();
    directors.sort();
    let mut writers = episode.writers.clone();
    writers.sort();

    view! {
        <article class=style::episode_card id=format!("episode-{episode_id}")>
            <div class=style::episode_main>
                <div class=style::thumb_wrap>
                    {episode.thumbnail_url.map(move |src| view! {
                        <img class=style::thumb src=src alt=thumbnail_alt loading="lazy"/>
                    })}
                    {warning_badge(episode.media_item_state)}
                </div>
                <div class=style::episode_body>
                    <h2 class=style::episode_title>
                        {format!("{}. {title}", episode.episode_number)}
                    </h2>
                    {episode.plot.filter(|p| !p.trim().is_empty()).map(|plot| view! {
                        <p class=style::plot>{plot}</p>
                    })}
                    <div class=style::episode_actions>
                        <button
                            type="button"
                            class=shell_style::action_btn
                            on:click=move |_| on_add_collection.run(episode_id)
                        >
                            {tr(TextKey::AddToCollection)}
                        </button>
                        <button
                            type="button"
                            class=shell_style::action_btn
                            on:click=move |_| on_add_playlist.run(episode_id)
                        >
                            {tr(TextKey::AddToPlaylist)}
                        </button>
                        <button
                            type="button"
                            class=style::secondary_btn
                            on:click=move |_| on_show_info.run(())
                        >
                            {tr(TextKey::ShowMediaInfo)}
                        </button>
                        <a class=style::secondary_btn href=troubleshoot_href>
                            {tr(TextKey::TroubleshootPlayback)}
                        </a>
                    </div>
                </div>
            </div>
            <div class=style::episode_meta>
                {status}
                <p>{tr(TextKey::Released)} ": " {episode.aired}</p>
                {people_row(tr(TextKey::Directors), "director", directors)}
                {people_row(tr(TextKey::Writers), "writer", writers)}
            </div>
        </article>
    }
}

fn warning_badge(state: i32) -> Option<AnyView> {
    match state {
        1 | 2 => Some(view! { <span class=style::warning_badge>"!"</span> }.into_any()),
        _ => None,
    }
}

fn file_status(episode: &EpisodeCardRow) -> Option<AnyView> {
    match episode.media_item_state {
        1 => Some(
            view! {
                <p class=style::status_line>
                    <span class=style::status_icon>"!"</span>
                    {tr(TextKey::FileNotFound)} ": "
                    <code>{episode.file_path.clone().unwrap_or_default()}</code>
                </p>
            }
            .into_any(),
        ),
        2 => Some(
            view! {
                <p class=style::status_line>
                    <span class=style::status_icon>"!"</span>
                    {tr(TextKey::Unavailable)} ": "
                    <code>{episode.local_path.clone().unwrap_or_default()}</code>
                </p>
            }
            .into_any(),
        ),
        _ => None,
    }
}

fn people_row(label: &'static str, field: &'static str, people: Vec<String>) -> Option<AnyView> {
    if people.is_empty() {
        return None;
    }

    Some(
        view! {
            <p class=style::people_row>
                {label} ": "
                {people.into_iter().enumerate().map(|(index, person)| {
                    let query = urlencoding::encode(&format!("{field}:\"{}\"", person.to_lowercase()))
                        .into_owned();
                    view! {
                        {if index > 0 { Some(view! { <span>", "</span> }) } else { None }}
                        <a href=format!("/search?query={query}")>{person}</a>
                    }
                }).collect_view()}
            </p>
        }
        .into_any(),
    )
}
