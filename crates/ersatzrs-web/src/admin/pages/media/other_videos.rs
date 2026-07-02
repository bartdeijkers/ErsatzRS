//! Other video browser — port of `Pages/OtherVideoList.razor`.
//!
//! Delegates to the shared [`MultiSelectBrowserPage`].
//! `OtherVideoCardViewModel.HasMediaInfo = true`, so the per-card "Show Media
//! Info" entry is enabled.
use std::collections::HashSet;

use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::admin::i18n::{tr, TextKey};
use crate::admin::pages::media::movies::search_link;
use crate::admin::pages::media::multi_select_browser::{
    AddToCollectionModal, BrowserKindConfig, MediaInfoModal, MultiSelectBrowserPage,
};
use crate::admin::server_fns::media_browser::{
    get_media_item_info_json, get_other_video_details, OtherVideoDetailRow,
};
use crate::admin::ui::primitives::card::{MediaCard, MediaItemState};
use crate::admin::ui::primitives::icon::{icon_style, IconName};

#[allow(dead_code)]
mod detail_style {
    stylance::import_crate_style!(s, "src/admin/pages/media/movie_detail.module.css");
    pub use s::*;
}

#[component]
pub fn OtherVideosPage() -> impl IntoView {
    view! { <MultiSelectBrowserPage config=BrowserKindConfig::OTHER_VIDEOS/> }
}

#[component]
pub fn OtherVideoPage() -> impl IntoView {
    let params = use_params_map();
    let id = params.with_untracked(|p| {
        p.get("id")
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or_default()
    });

    let data: Resource<Option<OtherVideoDetailRow>> = Resource::new_blocking(
        move || id,
        |id| async move { get_other_video_details(id).await.ok().flatten() },
    );

    view! {
        <Suspense fallback=move || view! { <p class=detail_style::loading>{tr(TextKey::Loading)}</p> }>
            {move || {
                match data.get() {
                    Some(Some(other_video)) => view! { <OtherVideoDetailView other_video/> }.into_any(),
                    Some(None) => view! {
                        <div class=detail_style::not_found>
                            <h1>{tr(TextKey::OtherVideoNotFound)}</h1>
                            <a href="/media/other/videos">{tr(TextKey::BackToOtherVideos)}</a>
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
fn OtherVideoDetailView(other_video: OtherVideoDetailRow) -> impl IntoView {
    let OtherVideoDetailRow {
        id,
        title,
        original_title,
        year,
        plot,
        tagline: _,
        content_ratings,
        release_date: _,
        artwork_url,
        fanart_url,
        file_path,
        local_path,
        media_item_state,
        genres,
        tags,
        languages,
        studios,
        directors,
        writers,
        actors,
    } = other_video;

    let title_for_artwork = title.clone();
    let poster_state = media_state(media_item_state);
    let fanart_src = fanart_url.unwrap_or_default();
    let fanart_style = if fanart_src.trim().is_empty() {
        "display:none"
    } else {
        ""
    };
    let artwork_src = artwork_url.unwrap_or_default();
    let artwork_style = if artwork_src.trim().is_empty() {
        "display:none"
    } else {
        ""
    };
    let subtitle_text = original_title.unwrap_or_default();
    let subtitle_style = if subtitle_text.trim().is_empty() {
        "display:none"
    } else {
        ""
    };
    let year_text = year.map(|y| y.to_string()).unwrap_or_default();
    let year_style = if year_text.is_empty() {
        "display:none"
    } else {
        ""
    };
    let plot_text = plot.unwrap_or_default();
    let plot_style = if plot_text.trim().is_empty() {
        "display:none"
    } else {
        ""
    };
    let warning_style = if matches!(
        poster_state,
        MediaItemState::FileNotFound | MediaItemState::Unavailable
    ) {
        ""
    } else {
        "display:none"
    };
    let warning_label = match poster_state {
        MediaItemState::FileNotFound => tr(TextKey::FileNotFound),
        MediaItemState::Unavailable => tr(TextKey::Unavailable),
        MediaItemState::Normal | MediaItemState::RemoteOnly => "",
    };
    let warning_path = match poster_state {
        MediaItemState::FileNotFound => file_path.unwrap_or_default(),
        MediaItemState::Unavailable => local_path.unwrap_or_default(),
        MediaItemState::Normal | MediaItemState::RemoteOnly => String::new(),
    };
    let file_not_found_style = if poster_state == MediaItemState::FileNotFound {
        ""
    } else {
        "display:none"
    };
    let unavailable_style = if poster_state == MediaItemState::Unavailable {
        ""
    } else {
        "display:none"
    };
    let actors_style = if actors.is_empty() {
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
                    <div class=detail_style::poster_wrap style=artwork_style>
                        <img class=detail_style::poster src=artwork_src alt=title_for_artwork/>
                        <div
                            class=format!("{} {}", detail_style::state_badge, detail_style::state_badge_error)
                            aria-label=tr(TextKey::FileNotFound)
                            style=file_not_found_style
                        >
                            <span style=icon_style(IconName::Warning, "1.4rem") aria-hidden="true"></span>
                        </div>
                        <div
                            class=format!("{} {}", detail_style::state_badge, detail_style::state_badge_warning)
                            aria-label=tr(TextKey::Unavailable)
                            style=unavailable_style
                        >
                            <span style=icon_style(IconName::Warning, "1.4rem") aria-hidden="true"></span>
                        </div>
                    </div>
                    <div class=detail_style::header>
                        <h1 class=detail_style::title>{title.clone()}</h1>
                        <p class=detail_style::year style=subtitle_style>{subtitle_text}</p>
                        <p class=detail_style::year style=year_style>{year_text}</p>
                        <div class=detail_style::plot_card style=plot_style>
                            <p class=detail_style::plot>{plot_text}</p>
                        </div>
                    </div>
                </div>
                <OtherVideoActions id title=title.clone()/>
                <div class=detail_style::warning style=warning_style>
                    <span style=icon_style(IconName::Warning, "1.2rem") aria-hidden="true"></span>
                    <span>{warning_label}": "</span>
                    <code>{warning_path}</code>
                </div>
                <dl class=detail_style::facts>
                    {linked_fact_row(TextKey::ContentRatings, "content_rating", &content_ratings, false)}
                    {linked_fact_row(TextKey::Languages, "language", &languages, true)}
                    {linked_fact_row(TextKey::Studios, "studio", &studios, true)}
                    {linked_fact_row(TextKey::Directors, "director", &directors, true)}
                    {linked_fact_row(TextKey::Writers, "writer", &writers, true)}
                    {linked_fact_row(TextKey::Genres, "genre", &genres, true)}
                    {linked_fact_row(TextKey::Tags, "tag", &tags, true)}
                </dl>

                <section class=detail_style::actors_section style=actors_style>
                    <h2 class=detail_style::section_title>{tr(TextKey::Actors)}</h2>
                    <div class=detail_style::actor_grid>
                        {actors.into_iter().map(|actor| {
                            let href = search_link("actor", &actor.name, true);
                            view! {
                                <MediaCard
                                    title=actor.name.clone()
                                    subtitle=actor.role.filter(|role| !role.trim().is_empty())
                                    artwork_url=actor.thumb_url
                                    href=Some(href)
                                    placeholder=actor.name
                                    state=MediaItemState::Normal
                                />
                            }
                        }).collect_view()}
                    </div>
                </section>
            </div>
        </div>
    }
}

#[component]
fn OtherVideoActions(id: i64, title: String) -> impl IntoView {
    let selected: RwSignal<HashSet<i64>> = RwSignal::new(HashSet::new());
    let collection_open = RwSignal::new(false);
    let media_info_open = RwSignal::new(false);
    let media_info_title = RwSignal::new(title.clone());
    let media_info_body = RwSignal::new(String::new());
    let media_info_action =
        Action::new(move |(): &()| async move { get_media_item_info_json(id).await });
    let troubleshoot_href = format!("/system/troubleshooting/playback?mediaItem={id}");

    Effect::new(move |_| {
        if let Some(result) = media_info_action.value().get() {
            match result {
                Ok(json) => media_info_body.set(json),
                Err(error) => {
                    media_info_body.set(format!("{}: {error}", tr(TextKey::UnableToLoadMediaInfo)));
                }
            }
        }
    });

    view! {
        <div class=detail_style::actions>
            <button
                type="button"
                class=detail_style::btn_add
                on:click=move |_| {
                    selected.update(|items| {
                        items.clear();
                        items.insert(id);
                    });
                    collection_open.set(true);
                }
            >
                <span>{tr(TextKey::AddToCollection)}</span>
            </button>
            <button
                type="button"
                class=detail_style::btn_secondary
                on:click=move |_| {
                    media_info_title.set(title.clone());
                    media_info_body.set(tr(TextKey::LoadingMediaInfo).to_string());
                    media_info_open.set(true);
                    media_info_action.dispatch(());
                }
            >
                <span>{tr(TextKey::ShowMediaInfo)}</span>
            </button>
            <a class=detail_style::btn_secondary href=troubleshoot_href>
                <span>{tr(TextKey::TroubleshootPlayback)}</span>
            </a>
            <a class=detail_style::btn_back href="/media/other/videos">
                <span>{tr(TextKey::Back)}</span>
            </a>
            <AddToCollectionModal open=collection_open selected=selected/>
            <MediaInfoModal
                open=media_info_open
                title=media_info_title
                body=media_info_body
            />
        </div>
    }
}

fn linked_fact_row(
    label: TextKey,
    search_field: &'static str,
    values: &[String],
    lowercase_query: bool,
) -> AnyView {
    let row_style = if values.iter().all(|s| s.trim().is_empty()) {
        "display:none"
    } else {
        ""
    };

    view! {
        <div class=detail_style::fact_row style=row_style>
            <dt>{tr(label)}</dt>
            <dd>
                {values.iter()
                    .filter(|s| !s.trim().is_empty())
                    .enumerate()
                    .map(|(index, value)| {
                        let href = search_link(search_field, value, lowercase_query);
                        let value = value.clone();
                        view! {
                            {if index > 0 { Some(view! { <span>", "</span> }) } else { None }}
                            <a href=href>{value}</a>
                        }
                    })
                    .collect_view()}
            </dd>
        </div>
    }
    .into_any()
}

fn media_state(state: i32) -> MediaItemState {
    match state {
        1 => MediaItemState::FileNotFound,
        2 => MediaItemState::Unavailable,
        3 => MediaItemState::RemoteOnly,
        _ => MediaItemState::Normal,
    }
}
