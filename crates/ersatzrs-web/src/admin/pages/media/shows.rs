//! Television show browser + show details page.
//!
//! * `/media/tv/shows` — port of `Pages/TelevisionShowList.razor`, delegates
//!   to the shared [`MultiSelectBrowserPage`].
//! * `/media/tv/shows/:id` — port of `Pages/TelevisionSeasonList.razor`
//!   (the per-show "series" page): fanart/poster header, plot, metadata
//!   facts (content ratings, studios, networks, languages, genres, tags), the
//!   seasons grid, the cast grid, and the "Add To" (Collection / Playlist /
//!   Schedule) + "Scan" (Quick / Deep) actions.
//!
//! Hydration shape mirrors the proven movie/season detail pattern (`TEST.md`):
//! the page wrapper is a plain `#[component]` that reads the `:id` route param
//! and loads the initial row on the server, then renders the interactive
//! [`ShowDetailIsland`] with the stable detail payload.
//!
//! Current scan and navigation behavior:
//!
//! * **Provider single-show scan** — Plex, Jellyfin, and Emby use the targeted
//!   `scan_show` path matching `QueueShowScanByLibraryId`; Local falls back to
//!   the owning library scan, matching ErsatzTV's local scanner behavior.
//! * **Post-add navigation** — collection and playlist adds keep the current
//!   curation context and expose an explicit destination link in the success
//!   toast; schedule adds keep the existing modal feedback flow.
#![allow(clippy::too_many_lines)]

use std::collections::HashSet;

use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::admin::i18n::{season_display_name, tr, TextKey};
use crate::admin::pages::media::add_to_schedule_modal::AddToScheduleModal;
use crate::admin::pages::media::movies::search_link;
use crate::admin::pages::media::multi_select_browser::{
    AddToCollectionModal, AddToPlaylistModal, MediaInfoModal,
};
use crate::admin::pages::media::tv_specials::TvSpecialFeatureRow;
use crate::admin::server_fns::libraries::scan_show;
#[cfg(feature = "ssr")]
use crate::admin::server_fns::media_browser::get_show_details;
use crate::admin::server_fns::media_browser::{
    get_media_item_info_json, MediaBrowserKind, ShowDetailRow,
};
use crate::admin::ui::primitives::card::{MediaCard, MediaItemState};
use crate::admin::ui::primitives::resource_state::AdminErrorState;
use crate::admin::ui::primitives::toast::{use_toast, ToastSeverity};

// Hero/fanart/facts styles are shared with the movies detail page. Wrapped
// in an `#[allow(dead_code)]` module because this page does not use every
// class the shared sheet defines (e.g. `actions`).
#[allow(dead_code)]
mod detail_style {
    stylance::import_crate_style!(s, "src/admin/pages/media/movie_detail.module.css");
    pub use s::*;
}
stylance::import_crate_style!(style, "src/admin/pages/media/show_detail.module.css");

#[component]
pub fn ShowsPage() -> impl IntoView {
    use crate::admin::pages::media::multi_select_browser::{
        BrowserKindConfig, MultiSelectBrowserPage,
    };
    view! { <MultiSelectBrowserPage config=BrowserKindConfig::SHOWS/> }
}

// ---------------------------------------------------------------------------
// `/media/tv/shows/:id` — details page port of `TelevisionSeasonList.razor`.
// ---------------------------------------------------------------------------

/// Page wrapper — plain `#[component]`, reads the `:id` route param and
/// renders the interactive island with that scalar prop.
#[component]
pub fn ShowPage() -> impl IntoView {
    let params = use_params_map();
    let id = params.with_untracked(|p| {
        p.get("id")
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or_default()
    });
    let show = load_show_detail_initial(id);

    match show {
        Ok(Some(show)) => view! { <ShowDetailIsland show/> }.into_any(),
        Ok(None) => view! {
            <div class=detail_style::not_found>
                <h1>{tr(TextKey::ShowNotFound)}</h1>
                <a href="/media/tv/shows">{tr(TextKey::BackToTvShows)}</a>
            </div>
        }
        .into_any(),
        Err(error) => view! { <AdminErrorState error/> }.into_any(),
    }
}

#[island]
fn ShowDetailIsland(show: ShowDetailRow) -> impl IntoView {
    view! { <ShowDetailView show/> }
}

fn load_show_detail_initial(id: i64) -> Result<Option<ShowDetailRow>, String> {
    #[cfg(feature = "ssr")]
    {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(async move { get_show_details(id).await })
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
fn ShowDetailView(show: ShowDetailRow) -> impl IntoView {
    let ShowDetailRow {
        id,
        library_id,
        media_source_kind,
        title,
        year,
        plot,
        content_ratings,
        poster_url,
        fanart_url,
        media_item_state,
        genres,
        tags,
        languages,
        studios,
        music_labels,
        networks,
        actors,
        seasons,
        extras,
    } = show;

    let is_remote = matches!(media_source_kind.as_str(), "Plex" | "Jellyfin" | "Emby");

    // Single-item selection set reused by the shared Add-To modals.
    let selected: RwSignal<HashSet<i64>> = RwSignal::new(HashSet::from([id]));
    let extra_selected: RwSignal<HashSet<i64>> = RwSignal::new(HashSet::new());
    let collection_open = RwSignal::new(false);
    let playlist_open = RwSignal::new(false);
    let extra_collection_open = RwSignal::new(false);
    let extra_playlist_open = RwSignal::new(false);
    let schedule_open = RwSignal::new(false);
    let media_info_open = RwSignal::new(false);
    let media_info_title = RwSignal::new(String::new());
    let media_info_body = RwSignal::new(String::new());

    let open_with_selection = move |target: RwSignal<bool>| {
        selected.set(HashSet::from([id]));
        target.set(true);
    };

    let toast = use_toast();
    let media_info_action = Action::new(move |target: &(i64, String)| {
        let (media_item_id, title) = target.clone();
        async move {
            get_media_item_info_json(media_item_id)
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

    let scan_action = Action::new(move |deep: &bool| {
        let deep = *deep;
        let kind = media_source_kind.clone();
        async move { scan_show(library_id, id, kind, deep).await }
    });
    Effect::new(move |_| {
        if let Some(res) = scan_action.value().get() {
            match res {
                Ok(_) => toast.push(tr(TextKey::ScanQueued), ToastSeverity::Success),
                Err(e) => toast.push(
                    format!("{}: {e}", tr(TextKey::ScanFailed)),
                    ToastSeverity::Error,
                ),
            }
        }
    });

    let warning = match media_item_state {
        1 => Some(
            view! { <div class=detail_style::warning>{tr(TextKey::FileNotFound)}</div> }.into_any(),
        ),
        2 => Some(
            view! { <div class=detail_style::warning>{tr(TextKey::Unavailable)}</div> }.into_any(),
        ),
        _ => None,
    };

    let title_for_poster = title.clone();

    view! {
        <div class=detail_style::page>
            <div class=detail_style::fanart_container>
                <div class=detail_style::fanart_tint/>
                {fanart_url.map(|src| view! {
                    <img class=detail_style::fanart src=src alt=tr(TextKey::FanArt)/>
                })}
            </div>

            <div class=detail_style::content>
                <div class=detail_style::hero>
                    {poster_url.map(move |src| view! {
                        <div class=detail_style::poster_wrap>
                            <img class=detail_style::poster src=src alt=title_for_poster/>
                        </div>
                    })}
                    <div class=detail_style::header>
                        <h1 class=detail_style::title>{title.clone()}</h1>
                        {year.map(|y| view! { <p class=detail_style::year>{y.to_string()}</p> })}
                        {plot.filter(|p| !p.trim().is_empty()).map(|p| {
                            view! { <p class=detail_style::plot>{p}</p> }
                        })}
                        {warning}
                        <div class=style::action_bar>
                            <button
                                type="button"
                                class=style::action_btn
                                on:click=move |_| open_with_selection(collection_open)
                            >
                                {tr(TextKey::AddToCollection)}
                            </button>
                            <button
                                type="button"
                                class=style::action_btn
                                on:click=move |_| open_with_selection(playlist_open)
                            >
                                {tr(TextKey::AddToPlaylist)}
                            </button>
                            <button
                                type="button"
                                class=style::action_btn
                                on:click=move |_| schedule_open.set(true)
                            >
                                {tr(TextKey::AddToSchedule)}
                            </button>
                            {is_remote.then(|| view! {
                                <button
                                    type="button"
                                    class=style::action_btn
                                    on:click=move |_| { scan_action.dispatch(false); }
                                >
                                    {tr(TextKey::QuickScan)}
                                </button>
                                <button
                                    type="button"
                                    class=style::action_btn
                                    on:click=move |_| { scan_action.dispatch(true); }
                                >
                                    {tr(TextKey::DeepScan)}
                                </button>
                            })}
                            <a class=detail_style::btn_back href="/media/tv/shows">{tr(TextKey::Back)}</a>
                        </div>
                    </div>
                </div>

                <dl class=detail_style::facts>
                    {fact_row(TextKey::ContentRatings, "content_rating", &content_ratings, false)}
                    {fact_row(TextKey::Studios, "studio", &studios, true)}
                    {fact_row(
                        TextKey::MusicLabels,
                        "music_label",
                        &music_labels,
                        true,
                    )}
                    {fact_row(TextKey::Networks, "network", &networks, true)}
                    {fact_row(TextKey::Languages, "language", &languages, true)}
                    {fact_row(TextKey::Genres, "genre", &genres, true)}
                    {fact_row(TextKey::Tags, "tag", &tags, true)}
                </dl>

                <h2 class=style::section_title>{tr(TextKey::Seasons)}</h2>
                <div class=style::divider/>
                <div class=style::grid>
                    {seasons.into_iter().map(|s| {
                        let season_id = s.id;
                        let placeholder = if s.season_number == 0 {
                            "S".to_string()
                        } else {
                            s.season_number.to_string()
                        };
                        view! {
                            <MediaCard
                                title=season_display_name(s.season_number)
                                artwork_url=s.poster_url
                                href=Some(format!("/media/tv/seasons/{season_id}"))
                                placeholder=placeholder
                                state=MediaItemState::Normal
                                on_add_to_collection=Some(Callback::new(move |()| {
                                    selected.set(HashSet::from([season_id]));
                                    collection_open.set(true);
                                }))
                            />
                        }
                    }).collect_view()}
                </div>

                {(!extras.is_empty()).then(|| view! {
                    <h2 class=style::section_title>{tr(TextKey::ContentVariantSpecialFeatures)}</h2>
                    <div class=style::divider/>
                    <div class=style::special_stack>
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

                {(!actors.is_empty()).then(|| view! {
                    <h2 class=style::section_title>{tr(TextKey::Actors)}</h2>
                    <div class=style::divider/>
                    <div class=style::grid>
                        {actors.into_iter().map(|a| {
                            let q = urlencoding::encode(&format!("actor:\"{}\"", a.name.to_lowercase()))
                                .into_owned();
                            view! {
                                <MediaCard
                                    title=a.name.clone()
                                    subtitle=a.role.filter(|r| !r.trim().is_empty())
                                    artwork_url=a.thumb_url
                                    href=Some(format!("/search?query={q}"))
                                    placeholder=a.name
                                    state=MediaItemState::Normal
                                />
                            }
                        }).collect_view()}
                    </div>
                })}
            </div>

            <AddToCollectionModal open=collection_open selected=selected/>
            <AddToPlaylistModal open=playlist_open selected=selected kind=MediaBrowserKind::Show/>
            <AddToCollectionModal open=extra_collection_open selected=extra_selected/>
            <AddToPlaylistModal
                open=extra_playlist_open
                selected=extra_selected
                kind=MediaBrowserKind::OtherVideo
            />
            <AddToScheduleModal
                open=schedule_open
                media_item_id=id
                collection_type=1
                success_label=tr(TextKey::AddedShowToSchedule)
            />
            <MediaInfoModal open=media_info_open title=media_info_title body=media_info_body/>
        </div>
    }
}

/// Definition-list fact row — same contract as the movies detail page:
/// `None` when every value is blank, comma-joined otherwise.
fn fact_row(
    label: TextKey,
    field: &'static str,
    values: &[String],
    lowercase: bool,
) -> Option<AnyView> {
    if values.iter().all(|s| s.trim().is_empty()) {
        return None;
    }
    Some(
        view! {
            <div class=detail_style::fact_row>
                <dt>{tr(label)}</dt>
                <dd>{values.iter().filter(|value| !value.trim().is_empty()).enumerate().map(|(index, value)| {
                    view! {
                        {if index > 0 { Some(view! { <span>", "</span> }) } else { None }}
                        <a href=search_link(field, value, lowercase)>{value.clone()}</a>
                    }
                }).collect_view()}</dd>
            </div>
        }
        .into_any(),
    )
}
