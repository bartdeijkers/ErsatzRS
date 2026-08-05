//! Read-only detail surfaces for music videos, songs, and images.
//!
//! Hydration shape follows the media-browser detail-page contract: the plain
//! page wrapper resolves its initial payload during SSR, then renders one
//! interactive island with stable serialized props outside `Suspense`. All
//! action buttons and their modals live inside that island.

use std::collections::HashSet;

use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use serde::{Deserialize, Serialize};

use crate::admin::i18n::{tr, TextKey};
use crate::admin::pages::media::movies::search_link;
use crate::admin::pages::media::multi_select_browser::{
    AddToCollectionModal, AddToPlaylistModal, MediaInfoModal,
};
#[cfg(feature = "ssr")]
use crate::admin::server_fns::media_browser::{
    get_image_details, get_music_video_details, get_song_details,
};
use crate::admin::server_fns::media_browser::{
    get_media_item_info_json, FacetedMediaDetailRow, MediaBrowserKind,
};
use crate::admin::ui::primitives::resource_state::AdminErrorState;

#[allow(dead_code)]
mod style {
    stylance::import_crate_style!(s, "src/admin/pages/media/movie_detail.module.css");
    pub use s::*;
}

#[derive(Clone, Copy, Deserialize, Serialize)]
enum DetailKind {
    MusicVideo,
    Song,
    Image,
}

impl DetailKind {
    const fn browser_kind(self) -> MediaBrowserKind {
        match self {
            Self::MusicVideo => MediaBrowserKind::MusicVideo,
            Self::Song => MediaBrowserKind::Song,
            Self::Image => MediaBrowserKind::Image,
        }
    }

    const fn back_href(self) -> &'static str {
        match self {
            Self::MusicVideo => "/media/music/videos",
            Self::Song => "/media/music/songs",
            Self::Image => "/media/browser/images",
        }
    }

    const fn supports_playback(self) -> bool {
        !matches!(self, Self::Image)
    }
}

#[component]
pub fn MusicVideoPage() -> impl IntoView {
    view! { <FacetedDetailPage kind=DetailKind::MusicVideo/> }
}

#[component]
pub fn SongPage() -> impl IntoView {
    view! { <FacetedDetailPage kind=DetailKind::Song/> }
}

#[component]
pub fn ImagePage() -> impl IntoView {
    view! { <FacetedDetailPage kind=DetailKind::Image/> }
}

#[component]
fn FacetedDetailPage(kind: DetailKind) -> impl IntoView {
    let params = use_params_map();
    let id = params.with_untracked(|params| {
        params
            .get("id")
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or_default()
    });
    match load_faceted_detail_initial(id, kind) {
        Ok(Some(detail)) => view! { <FacetedDetailIsland detail kind/> }.into_any(),
        Ok(None) => view! {
            <div class=style::not_found>
                <h1>{tr(TextKey::NotFound)}</h1>
                <a href=kind.back_href()>{tr(TextKey::Back)}</a>
            </div>
        }
        .into_any(),
        Err(error) => view! { <AdminErrorState error/> }.into_any(),
    }
}

fn load_faceted_detail_initial(
    id: i64,
    kind: DetailKind,
) -> Result<Option<FacetedMediaDetailRow>, String> {
    #[cfg(feature = "ssr")]
    {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                let result = match kind {
                    DetailKind::MusicVideo => get_music_video_details(id).await,
                    DetailKind::Song => get_song_details(id).await,
                    DetailKind::Image => get_image_details(id).await,
                };
                result.map_err(|error| error.to_string())
            })
        })
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (id, kind);
        Ok(None)
    }
}

#[island]
fn FacetedDetailIsland(detail: FacetedMediaDetailRow, kind: DetailKind) -> impl IntoView {
    let hydrated = RwSignal::new(false);
    Effect::new(move |_| hydrated.set(true));
    let selected = RwSignal::new(HashSet::from([detail.id]));
    let collection_open = RwSignal::new(false);
    let playlist_open = RwSignal::new(false);
    let media_info_open = RwSignal::new(false);
    let media_info_title = RwSignal::new(detail.title.clone());
    let media_info_body = RwSignal::new(String::new());
    let id = detail.id;
    let info_action = Action::new(move |(): &()| async move { get_media_item_info_json(id).await });
    Effect::new(move |_| {
        if let Some(result) = info_action.value().get() {
            media_info_body.set(result.unwrap_or_else(|error| error.to_string()));
        }
    });

    let facets = detail.facets;
    let original_title = detail.original_title.unwrap_or_default();
    let original_style = if original_title.trim().is_empty() {
        "display:none"
    } else {
        ""
    };
    let year = detail
        .year
        .map(|value| value.to_string())
        .unwrap_or_default();
    let year_style = if year.is_empty() { "display:none" } else { "" };
    let plot = detail.plot.unwrap_or_default();
    let plot_style = if plot.trim().is_empty() {
        "display:none"
    } else {
        ""
    };
    let artwork = detail.artwork_url.unwrap_or_default();
    let artwork_style = if artwork.trim().is_empty() {
        "display:none"
    } else {
        ""
    };
    let fanart = detail.fanart_url.unwrap_or_default();
    let fanart_style = if fanart.trim().is_empty() {
        "display:none"
    } else {
        ""
    };
    let warning = matches!(detail.media_item_state, 1 | 2);
    let warning_path = if detail.media_item_state == 1 {
        detail.file_path.unwrap_or_default()
    } else {
        detail.local_path.unwrap_or_default()
    };
    let warning_label = if detail.media_item_state == 1 {
        tr(TextKey::FileNotFound)
    } else {
        tr(TextKey::Unavailable)
    };
    let title_for_info = detail.title.clone();

    view! {
        <div
            class=style::page
            data-faceted-detail-hydrated=move || if hydrated.get() { "true" } else { "false" }
        >
            <div class=style::fanart_container>
                <div class=style::fanart_tint/>
                <img class=style::fanart src=fanart alt=tr(TextKey::FanArt) style=fanart_style/>
            </div>
            <div class=style::content>
                <div class=style::hero>
                    <div class=style::poster_wrap style=artwork_style>
                        <img class=style::poster src=artwork alt=detail.title.clone()/>
                    </div>
                    <div class=style::header>
                        <h1 class=style::title>{detail.title}</h1>
                        <p class=style::year style=original_style>{original_title}</p>
                        <p class=style::year style=year_style>{year}</p>
                        <div class=style::plot_card style=plot_style><p class=style::plot>{plot}</p></div>
                    </div>
                </div>
                <div class=style::actions>
                    <button type="button" class=style::btn_add on:click=move |_| {
                        selected.set(HashSet::from([id]));
                        collection_open.set(true);
                    }>
                        {tr(TextKey::AddToCollection)}
                    </button>
                    <button type="button" class=style::btn_secondary on:click=move |_| {
                        selected.set(HashSet::from([id]));
                        playlist_open.set(true);
                    }>
                        {tr(TextKey::AddToPlaylist)}
                    </button>
                    <button type="button" class=style::btn_secondary on:click=move |_| {
                        media_info_title.set(title_for_info.clone());
                        media_info_body.set(tr(TextKey::LoadingMediaInfo).to_owned());
                        media_info_open.set(true);
                        info_action.dispatch(());
                    }>{tr(TextKey::ShowMediaInfo)}</button>
                    <a
                        class=style::btn_secondary
                        href=format!("/system/troubleshooting/playback?mediaItem={id}")
                        style=if kind.supports_playback() { "" } else { "display:none" }
                    >{tr(TextKey::TroubleshootPlayback)}</a>
                    <a class=style::btn_back href=kind.back_href()>{tr(TextKey::Back)}</a>
                </div>
                <div class=style::warning style=if warning { "" } else { "display:none" }>
                    <span>{warning_label}": "</span><code>{warning_path}</code>
                </div>
                <dl class=style::facts>
                    {fact_row(TextKey::ContentRatings, "content_rating", &facets.content_ratings, false)}
                    {fact_row(TextKey::Languages, "language", &facets.languages, true)}
                    {fact_row(TextKey::Studios, "studio", &facets.studios, true)}
                    {fact_row(
                        TextKey::MusicLabels,
                        "music_label",
                        &facets.music_labels,
                        true,
                    )}
                    {fact_row(TextKey::Genres, "genre", &facets.genres, true)}
                    {fact_row(TextKey::Tags, "tag", &facets.tags, true)}
                </dl>
                <AddToCollectionModal open=collection_open selected=selected/>
                <AddToPlaylistModal open=playlist_open selected kind=kind.browser_kind()/>
                <MediaInfoModal open=media_info_open title=media_info_title body=media_info_body/>
            </div>
        </div>
    }
}

fn fact_row(label: TextKey, field: &'static str, values: &[String], lowercase: bool) -> AnyView {
    let hidden = if values.iter().all(|value| value.trim().is_empty()) {
        "display:none"
    } else {
        ""
    };
    view! {
        <div class=style::fact_row style=hidden>
            <dt>{tr(label)}</dt>
            <dd>{values.iter().filter(|value| !value.trim().is_empty()).enumerate().map(|(index, value)| {
                let prefix = (index > 0).then_some(", ");
                view! { {prefix}<a href=search_link(field, value, lowercase)>{value.clone()}</a> }
            }).collect_view()}</dd>
        </div>
    }.into_any()
}
