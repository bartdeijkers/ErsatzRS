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
//! * **Targeted single-show scan** — `ErsatzTV`'s `QueueShowScanByLibraryId`
//!   syncs just the one show on Plex/Jellyfin/Emby. `ErsatzRS` scanning is
//!   library-level, so the Scan actions enqueue the owning library scan
//!   (quick/deep) via [`scan_library`]; the menu is still gated to remote
//!   sources to match the Razor condition.
//! * **Post-add navigation** — `ErsatzTV` navigates to the
//!   collection/playlist/schedule after adding. `ErsatzRS` keeps the
//!   established multi-select-browser behaviour (toast + stay on page).
#![allow(clippy::too_many_lines)]

use std::collections::HashSet;

use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::admin::pages::media::multi_select_browser::{AddToCollectionModal, AddToPlaylistModal};
use crate::admin::server_fns::libraries::scan_library;
#[cfg(target_arch = "wasm32")]
use crate::admin::server_fns::media_browser::list_program_schedules;
use crate::admin::server_fns::media_browser::{
    add_show_to_schedule, get_show_details, MediaBrowserKind, ScheduleOption, ShowDetailRow,
};
use crate::admin::ui::primitives::card::{MediaCard, MediaItemState};
use crate::admin::ui::primitives::dialog::Modal;
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
        <Suspense fallback=move || view! { <p class=detail_style::loading>"Loading…"</p> }>
            {move || {
                match data.get() {
                    Some(Some(show)) => view! { <ShowDetailView show/> }.into_any(),
                    Some(None) => view! {
                        <div class=detail_style::not_found>
                            <h1>"Show not found"</h1>
                            <a href="/media/tv/shows">"Back to TV shows"</a>
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
        async move { scan_library(library_id, kind, deep).await }
    });
    Effect::new(move |_| {
        if let Some(res) = scan_action.value().get() {
            match res {
                Ok(()) => toast.push("Scan queued", ToastSeverity::Success),
                Err(e) => toast.push(format!("Scan failed: {e}"), ToastSeverity::Error),
            }
        }
    });

    let warning = match media_item_state {
        1 => Some(view! { <div class=detail_style::warning>"File not found"</div> }.into_any()),
        2 => Some(view! { <div class=detail_style::warning>"Source unavailable"</div> }.into_any()),
        _ => None,
    };

    let title_for_poster = title.clone();

    view! {
        <div class=detail_style::page>
            <div class=detail_style::fanart_container>
                <div class=detail_style::fanart_tint/>
                {fanart_url.map(|src| view! {
                    <img class=detail_style::fanart src=src alt="fan art"/>
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
                                "Add to Collection"
                            </button>
                            <button
                                type="button"
                                class=style::action_btn
                                on:click=move |_| open_with_selection(playlist_open)
                            >
                                "Add to Playlist"
                            </button>
                            <button
                                type="button"
                                class=style::action_btn
                                on:click=move |_| schedule_open.set(true)
                            >
                                "Add to Schedule"
                            </button>
                            {is_remote.then(|| view! {
                                <button
                                    type="button"
                                    class=style::action_btn
                                    on:click=move |_| { scan_action.dispatch(false); }
                                >
                                    "Quick Scan"
                                </button>
                                <button
                                    type="button"
                                    class=style::action_btn
                                    on:click=move |_| { scan_action.dispatch(true); }
                                >
                                    "Deep Scan"
                                </button>
                            })}
                            <a class=detail_style::btn_back href="/media/tv/shows">"Back"</a>
                        </div>
                    </div>
                </div>

                <dl class=detail_style::facts>
                    {fact_row("Content Ratings", &content_ratings)}
                    {fact_row("Studios", &studios)}
                    {fact_row("Networks", &networks)}
                    {fact_row("Languages", &languages)}
                    {fact_row("Genres", &genres)}
                    {fact_row("Tags", &tags)}
                </dl>

                <h2 class=style::section_title>"Seasons"</h2>
                <div class=style::divider/>
                <div class=style::grid>
                    {seasons.into_iter().map(|s| {
                        let placeholder = if s.season_number == 0 {
                            "S".to_string()
                        } else {
                            s.season_number.to_string()
                        };
                        view! {
                            <MediaCard
                                title=s.title
                                artwork_url=s.poster_url
                                href=Some(format!("/media/tv/seasons/{}", s.id))
                                placeholder=placeholder
                                state=MediaItemState::Normal
                            />
                        }
                    }).collect_view()}
                </div>

                {(!actors.is_empty()).then(|| view! {
                    <h2 class=style::section_title>"Cast"</h2>
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
            <AddToScheduleModal open=schedule_open show_id=id/>
        </div>
    }
}

/// "Add To Schedule" dialog — ports `Shared/AddToScheduleDialog.razor` for a
/// single show (`AddProgramScheduleItem.ForMediaItem`).
#[component]
fn AddToScheduleModal(open: RwSignal<bool>, show_id: i64) -> impl IntoView {
    let options: RwSignal<Vec<ScheduleOption>> = RwSignal::new(Vec::new());
    let schedule_id: RwSignal<i64> = RwSignal::new(0);

    #[cfg(target_arch = "wasm32")]
    Effect::new(move |_| {
        if !open.get() {
            return;
        }
        leptos::task::spawn_local(async move {
            let loaded = list_program_schedules().await.unwrap_or_default();
            if schedule_id.get_untracked() == 0 {
                if let Some(first) = loaded.first() {
                    schedule_id.set(first.id);
                }
            }
            options.set(loaded);
        });
    });

    let submit_action = Action::new(move |(): &()| {
        let chosen = schedule_id.get_untracked();
        async move {
            if chosen == 0 {
                return Err(ServerFnError::new("Select a schedule"));
            }
            add_show_to_schedule(chosen, show_id).await
        }
    });

    let toast = use_toast();
    Effect::new(move |_| {
        if let Some(res) = submit_action.value().get() {
            match res {
                Ok(()) => {
                    toast.push("Added show to schedule", ToastSeverity::Success);
                    open.set(false);
                }
                Err(e) => toast.push(format!("Add to schedule failed: {e}"), ToastSeverity::Error),
            }
        }
    });

    view! {
        <Modal
            open=open
            title="Add To Schedule".to_string()
            on_close=Callback::new(move |confirmed: bool| {
                if confirmed {
                    submit_action.dispatch(());
                }
            })
            confirm_label="Add To Schedule"
            cancel_label="Cancel"
        >
            <div style="display: flex; flex-direction: column; gap: 0.5rem; margin-top: 0.75rem">
                <label>"Schedule"</label>
                <select
                    prop:value=move || schedule_id.get().to_string()
                    on:change=move |e| {
                        if let Ok(v) = event_target_value(&e).parse::<i64>() {
                            schedule_id.set(v);
                        }
                    }
                >
                    {move || {
                        options.get().into_iter().map(|s| view! {
                            <option value=s.id.to_string()>{s.name}</option>
                        }).collect_view()
                    }}
                </select>
            </div>
        </Modal>
    }
}

/// Definition-list fact row — same contract as the movies detail page:
/// `None` when every value is blank, comma-joined otherwise.
fn fact_row(label: &'static str, values: &[String]) -> Option<AnyView> {
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
                <dt>{label}</dt>
                <dd>{joined}</dd>
            </div>
        }
        .into_any(),
    )
}
