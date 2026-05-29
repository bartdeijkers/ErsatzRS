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
//! Hydration shape mirrors the proven movies/collection-detail pattern
//! (`TEST.md`): the page wrapper is a plain `#[component]` that reads the
//! `:id` route param and renders the interactive [`ShowDetailIsland`] with
//! that scalar prop; the island owns a `Resource::new_blocking` and renders
//! `<Suspense>` internally so no prop-bearing island crosses a Suspense.
//!
//! Parity notes / deferred from the C# behaviour:
//!
//! * **Provider single-show scan** — Plex now uses the targeted
//!   `scan_show` path matching `QueueShowScanByLibraryId`; Jellyfin/Emby
//!   still fall back to the owning library scan until their provider slices
//!   land. Local single-show scan remains unsupported upstream.
//! * **Post-add navigation** — `ErsatzTV` navigates to the
//!   collection/playlist/schedule after adding. `ErsatzRS` keeps the
//!   established multi-select-browser behaviour (toast + stay on page).
#![allow(clippy::too_many_lines)]

use std::collections::HashSet;

use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::admin::i18n::{season_display_name, tr, TextKey};
use crate::admin::pages::media::add_to_schedule_modal::AddToScheduleModal;
use crate::admin::pages::media::multi_select_browser::{AddToCollectionModal, AddToPlaylistModal};
use crate::admin::server_fns::libraries::scan_show;
use crate::admin::server_fns::media_browser::{get_show_details, MediaBrowserKind, ShowDetailRow};
use crate::admin::ui::primitives::card::{MediaCard, MediaItemState};
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

    view! { <ShowDetailIsland id/> }
}

#[island]
fn ShowDetailIsland(id: i64) -> impl IntoView {
    let data: Resource<Option<ShowDetailRow>> = Resource::new_blocking(
        move || id,
        |id| async move { get_show_details(id).await.ok().flatten() },
    );

    view! {
        <Suspense fallback=move || view! { <p class=detail_style::loading>{tr(TextKey::Loading)}</p> }>
            {move || {
                match data.get() {
                    Some(Some(show)) => view! { <ShowDetailView show/> }.into_any(),
                    Some(None) => view! {
                        <div class=detail_style::not_found>
                            <h1>{tr(TextKey::ShowNotFound)}</h1>
                            <a href="/media/tv/shows">{tr(TextKey::BackToTvShows)}</a>
                        </div>
                    }
                    .into_any(),
                    None => view! { <p class=detail_style::loading>{tr(TextKey::Loading)}</p> }.into_any(),
                }
            }}
        </Suspense>
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
        networks,
        actors,
        seasons,
    } = show;

    let is_remote = matches!(media_source_kind.as_str(), "Plex" | "Jellyfin" | "Emby");

    // Single-item selection set reused by the shared Add-To modals.
    let selected: RwSignal<HashSet<i64>> = RwSignal::new(HashSet::from([id]));
    let collection_open = RwSignal::new(false);
    let playlist_open = RwSignal::new(false);
    let schedule_open = RwSignal::new(false);

    let open_with_selection = move |target: RwSignal<bool>| {
        selected.set(HashSet::from([id]));
        target.set(true);
    };

    let toast = use_toast();
    let scan_action = Action::new(move |deep: &bool| {
        let deep = *deep;
        let kind = media_source_kind.clone();
        async move { scan_show(library_id, id, kind, deep).await }
    });
    Effect::new(move |_| {
        if let Some(res) = scan_action.value().get() {
            match res {
                Ok(()) => toast.push(tr(TextKey::ScanQueued), ToastSeverity::Success),
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
                    {fact_row(TextKey::ContentRatings, &content_ratings)}
                    {fact_row(TextKey::Studios, &studios)}
                    {fact_row(TextKey::Networks, &networks)}
                    {fact_row(TextKey::Languages, &languages)}
                    {fact_row(TextKey::Genres, &genres)}
                    {fact_row(TextKey::Tags, &tags)}
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
            <AddToScheduleModal
                open=schedule_open
                media_item_id=id
                collection_type=1
                success_label=tr(TextKey::AddedShowToSchedule)
            />
        </div>
    }
}

/// Definition-list fact row — same contract as the movies detail page:
/// `None` when every value is blank, comma-joined otherwise.
fn fact_row(label: TextKey, values: &[String]) -> Option<AnyView> {
    if values.iter().all(|s| s.trim().is_empty()) {
        return None;
    }
    let joined = values
        .iter()
        .filter(|s| !s.trim().is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    Some(
        view! {
            <div class=detail_style::fact_row>
                <dt>{tr(label)}</dt>
                <dd>{joined}</dd>
            </div>
        }
        .into_any(),
    )
}
