//! Server functions for the media browser (movies/shows/images/artists/
//! songs/other videos/trash) and the
//! manual/smart collection editors. Ports:
//!
//! - `Pages/MovieList.razor` → [`list_movies`]
//! - `Pages/TelevisionShowList.razor` → [`list_shows`]
//! - `Pages/ImageList.razor` → [`list_images`]
//! - `Pages/ArtistList.razor` → [`list_artists`]
//! - `Pages/SongList.razor` → [`list_songs`]
//! - `Pages/OtherVideoList.razor` → [`list_other_videos`]
//! - `Pages/Trash.razor` → [`list_trash_items`], [`delete_media_items`],
//!   [`empty_trash`]
//! - `Pages/ManualCollections.razor` + `CollectionEditor.razor` →
//!   [`list_collections`], [`get_collection`], [`save_collection`],
//!   [`delete_collection`]
//! - `Pages/SmartCollections.razor` + `SmartCollectionEditor.razor` →
//!   [`list_smart_collections`], [`get_smart_collection`],
//!   [`save_smart_collection`], [`delete_smart_collection`]
//! - `Pages/MultiCollections.razor` + `MultiCollectionEditor.razor` →
//!   [`list_multi_collections`], [`get_multi_collection`],
//!   [`save_multi_collection`], [`delete_multi_collection`]
//! - `Pages/RerunCollections.razor` + `RerunCollectionEditor.razor` →
//!   [`list_rerun_collections`], [`get_rerun_collection_form`],
//!   [`save_rerun_collection`], [`delete_rerun_collection`]
//! - `Pages/Playlists.razor` + `PlaylistEditor.razor` →
//!   [`get_playlist_page`], [`create_playlist_group`],
//!   [`create_playlist`], [`delete_playlist_group`], [`delete_playlist`],
//!   [`get_playlist_editor`], [`save_playlist`]
#![cfg_attr(test, allow(dead_code))]

use leptos::prelude::*;
use serde::{Deserialize, Serialize};

/// Row for a media-card grid (movies/shows/images share this shape).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MediaCardRow {
    pub id: i64,
    pub title: String,
    pub sort_title: String,
    pub subtitle: Option<String>,
    pub artwork_path: Option<String>,
    /// Valid destination for the card, when this media kind has one.
    #[serde(default)]
    pub href: Option<String>,
    #[serde(default)]
    pub season_number: Option<i64>,
    #[serde(default)]
    pub variant: ContentVariantInfo,
}

/// Shared ADR 0018 content-variant display metadata carried by cards and
/// detail-page sections.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentVariantInfo {
    #[serde(default)]
    pub content_kind: ersatzrs_domain::ContentKind,
    #[serde(default)]
    pub sub_kind: Option<String>,
    #[serde(default)]
    pub parent_title: Option<String>,
}

impl ContentVariantInfo {
    #[must_use]
    pub const fn is_primary(&self) -> bool {
        matches!(self.content_kind, ersatzrs_domain::ContentKind::Primary)
    }
}

fn media_card_search_query(media_kind: &str, search: &str, show_variants: bool) -> String {
    let search = search.trim();
    let variants = if show_variants {
        " AND (content_kind:primary OR content_kind:extra OR content_kind:alternate_version)"
    } else {
        ""
    };
    if search.is_empty() {
        format!("type:{media_kind}{variants}")
    } else {
        format!("type:{media_kind} AND ({search}){variants}")
    }
}

#[cfg(feature = "ssr")]
const fn remote_stream_content_kind_token(
    kind: ersatzrs_domain::AddonMediaListContentKind,
) -> Option<&'static str> {
    use ersatzrs_domain::AddonMediaListContentKind;
    match kind {
        AddonMediaListContentKind::Auto => None,
        AddonMediaListContentKind::TelevisionEpisode => Some("television_episode"),
        AddonMediaListContentKind::Movie => Some("movie"),
        AddonMediaListContentKind::MusicVideo => Some("music_video"),
        AddonMediaListContentKind::OtherVideo => Some("other_video"),
    }
}

#[cfg(feature = "ssr")]
fn browser_card_artwork_url(
    kind: ersatzrs_core::repositories::AdminSearchKind,
    id: i64,
    enrichment: &ersatzrs_core::repositories::AdminSearchEnrichment,
) -> Option<String> {
    use crate::api::artwork::{artwork_kind_for_admin_search, artwork_url, ArtworkUrlKind};

    let artwork_kind = artwork_kind_for_admin_search(kind);
    let raw_artwork = match artwork_kind {
        ArtworkUrlKind::Thumbnail => enrichment
            .thumbnail
            .get(&id)
            .or_else(|| enrichment.poster.get(&id)),
        ArtworkUrlKind::Poster => enrichment
            .poster
            .get(&id)
            .or_else(|| enrichment.season_poster.get(&id))
            .or_else(|| enrichment.show_poster.get(&id)),
        ArtworkUrlKind::FanArt => None,
    };
    raw_artwork.and_then(|path| artwork_url(path, artwork_kind))
}

#[cfg(feature = "ssr")]
fn browser_card_href(
    kind: ersatzrs_core::repositories::AdminSearchKind,
    id: i64,
    enrichment: &ersatzrs_core::repositories::AdminSearchEnrichment,
) -> Option<String> {
    use ersatzrs_core::repositories::AdminSearchKind as Kind;

    match kind {
        Kind::Movie => Some(format!("/media/movies/{id}")),
        Kind::Show => Some(format!("/media/tv/shows/{id}")),
        Kind::Season => Some(format!("/media/tv/seasons/{id}")),
        Kind::Episode => enrichment
            .episode_season_id
            .get(&id)
            .map(|season_id| format!("/media/tv/seasons/{season_id}#episode-{id}")),
        Kind::Artist => Some(format!("/media/music/artists/{id}")),
        Kind::MusicVideo => Some(format!("/media/music/videos/{id}")),
        Kind::Song => Some(format!("/media/music/songs/{id}")),
        Kind::Image => Some(format!("/media/browser/images/{id}")),
        Kind::RemoteStream => None,
        Kind::OtherVideo => Some(format!("/media/other/videos/{id}")),
    }
}

#[cfg(all(test, feature = "ssr"))]
mod browser_card_projection_tests {
    use super::{browser_card_artwork_url, browser_card_href};
    use ersatzrs_core::repositories::{AdminSearchEnrichment, AdminSearchKind as Kind};

    #[test]
    fn browser_projection_retains_poster_and_thumbnail_families() {
        let mut enrichment = AdminSearchEnrichment::default();
        for id in 1..=4 {
            enrichment.poster.insert(id, format!("poster-{id}.jpg"));
        }
        for id in 5..=10 {
            enrichment
                .thumbnail
                .insert(id, format!("thumbnail-{id}.jpg"));
        }

        for (kind, id) in [
            (Kind::Movie, 1),
            (Kind::Show, 2),
            (Kind::Season, 3),
            (Kind::Episode, 4),
        ] {
            assert_eq!(
                browser_card_artwork_url(kind, id, &enrichment),
                Some(format!("/artwork/posters/poster-{id}.jpg"))
            );
        }
        for (kind, id) in [
            (Kind::Artist, 5),
            (Kind::MusicVideo, 6),
            (Kind::OtherVideo, 7),
            (Kind::Song, 8),
            (Kind::Image, 9),
            (Kind::RemoteStream, 10),
        ] {
            assert_eq!(
                browser_card_artwork_url(kind, id, &enrichment),
                Some(format!("/artwork/thumbnails/thumbnail-{id}.jpg"))
            );
        }
    }

    #[test]
    fn browser_projection_uses_only_valid_card_destinations() {
        let mut enrichment = AdminSearchEnrichment::default();
        enrichment.episode_season_id.insert(41, 31);

        assert_eq!(
            browser_card_href(Kind::Season, 31, &enrichment).as_deref(),
            Some("/media/tv/seasons/31")
        );
        assert_eq!(
            browser_card_href(Kind::Episode, 41, &enrichment).as_deref(),
            Some("/media/tv/seasons/31#episode-41")
        );
        assert_eq!(browser_card_href(Kind::Episode, 42, &enrichment), None);
        assert_eq!(
            browser_card_href(Kind::MusicVideo, 51, &enrichment).as_deref(),
            Some("/media/music/videos/51")
        );
    }
}

#[cfg(feature = "ssr")]
async fn list_media_cards_from_application(
    state: &crate::host::state::AppState,
    media_kind: &str,
    kind: ersatzrs_core::repositories::AdminSearchKind,
    search: &str,
    page: u32,
    page_size: u32,
    show_variants: bool,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    list_media_cards_from_application_filtered(
        state,
        media_kind,
        kind,
        search,
        page,
        page_size,
        show_variants,
        None,
    )
    .await
}

#[cfg(feature = "ssr")]
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
async fn list_media_cards_from_application_filtered(
    state: &crate::host::state::AppState,
    media_kind: &str,
    kind: ersatzrs_core::repositories::AdminSearchKind,
    search: &str,
    page: u32,
    page_size: u32,
    show_variants: bool,
    required_state: Option<ersatzrs_domain::MediaItemState>,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    use ersatzrs_domain::ContentKind;

    let query = media_card_search_query(media_kind, search, show_variants);
    let snapshot = state.application.admin_search_snapshot(&query).await?;
    let enrichment = snapshot.enrichment.get(&kind).cloned().unwrap_or_default();
    let mut documents = snapshot
        .documents
        .into_iter()
        .filter(|document| {
            document.media_kind == media_kind
                && (show_variants || document.content_kind == ContentKind::Primary)
                && required_state.is_none_or(|state| {
                    enrichment.state.get(&document.media_item_id) == Some(&state)
                })
        })
        .collect::<Vec<_>>();
    documents.sort_by(|left, right| {
        left.sort_title
            .as_deref()
            .or(left.title.as_deref())
            .unwrap_or_default()
            .to_ascii_lowercase()
            .cmp(
                &right
                    .sort_title
                    .as_deref()
                    .or(right.title.as_deref())
                    .unwrap_or_default()
                    .to_ascii_lowercase(),
            )
            .then_with(|| left.media_item_id.cmp(&right.media_item_id))
    });
    let total = u64::try_from(documents.len()).unwrap_or(u64::MAX);
    let skip = usize::try_from(page)
        .unwrap_or(usize::MAX)
        .saturating_mul(usize::try_from(page_size).unwrap_or(usize::MAX));
    let cards = documents
        .into_iter()
        .skip(skip)
        .take(usize::try_from(page_size).unwrap_or(usize::MAX))
        .map(|document| {
            let id = document.media_item_id;
            let title = document.title.unwrap_or_default();
            let mut sort_title = document.sort_title.unwrap_or_else(|| title.clone());
            let (title, subtitle, season_number) = match kind {
                ersatzrs_core::repositories::AdminSearchKind::Season => {
                    let number = i64::from(document.season_number.unwrap_or_default());
                    sort_title = format!("{sort_title} {number:04}");
                    (season_name(number), document.show_title, Some(number))
                }
                ersatzrs_core::repositories::AdminSearchKind::Episode => {
                    let subtitle = match (
                        document.show_title.as_deref(),
                        document.season_number,
                        document.episode_number,
                    ) {
                        (Some(show), Some(season), Some(episode)) if !show.trim().is_empty() => {
                            Some(format!("{show} - S{season} E{episode}"))
                        }
                        _ => document.show_title,
                    };
                    (title, subtitle, None)
                }
                ersatzrs_core::repositories::AdminSearchKind::Artist => (
                    title,
                    enrichment.artist_disambiguation.get(&id).cloned(),
                    None,
                ),
                ersatzrs_core::repositories::AdminSearchKind::MusicVideo => {
                    (title, enrichment.music_video_artist.get(&id).cloned(), None)
                }
                ersatzrs_core::repositories::AdminSearchKind::Song => (
                    title,
                    enrichment
                        .song_meta
                        .get(&id)
                        .and_then(|(artists, album)| song_subtitle(artists.clone(), album.clone())),
                    None,
                ),
                ersatzrs_core::repositories::AdminSearchKind::Image => (
                    title,
                    document
                        .release_date
                        .map(|date| date.format("%Y-%m-%d").to_string()),
                    None,
                ),
                ersatzrs_core::repositories::AdminSearchKind::RemoteStream
                | ersatzrs_core::repositories::AdminSearchKind::OtherVideo => {
                    (title, document.original_title, None)
                }
                _ => (title, document.year.map(|year| year.to_string()), None),
            };
            MediaCardRow {
                id,
                title,
                sort_title,
                subtitle,
                artwork_path: browser_card_artwork_url(kind, id, &enrichment),
                href: browser_card_href(kind, id, &enrichment),
                season_number,
                variant: ContentVariantInfo {
                    content_kind: document.content_kind,
                    sub_kind: if kind == ersatzrs_core::repositories::AdminSearchKind::RemoteStream
                    {
                        remote_stream_content_kind_token(document.remote_stream_content_kind)
                            .map(str::to_owned)
                            .or(document.sub_kind)
                    } else {
                        document.sub_kind
                    },
                    parent_title: document.parent_title,
                },
            }
        })
        .collect();
    Ok((cards, total))
}

/// Audio stream option surfaced on movie detail pages for commentary preview.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioTrackRow {
    pub media_version_id: i64,
    pub stream_index: i64,
    pub title: String,
    pub language: Option<String>,
    pub codec: Option<String>,
    pub channels: i64,
    pub is_default: bool,
    pub is_commentary: bool,
}

/// Backward-compatible alias for callers that still refer to commentary rows.
pub type CommentaryTrackRow = AudioTrackRow;

/// Playback preferences persisted on collection/playlist item rows.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaybackOverrideForm {
    pub preferred_audio_language_code: Option<String>,
    pub preferred_audio_title: Option<String>,
    pub preferred_subtitle_language_code: Option<String>,
    pub subtitle_mode: Option<i32>,
}

/// Subtitle option intended for commentary playback.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommentarySubtitleRow {
    pub id: i64,
    pub stream_index: i64,
    pub title: String,
    pub language: Option<String>,
    pub codec: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoviePlaybackStreamRow {
    pub index: i64,
    pub kind: i32,
    pub title: Option<String>,
    pub language: Option<String>,
    pub codec: Option<String>,
    pub channels: Option<i64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub is_default: bool,
    pub is_forced: bool,
    pub is_external: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoviePlaybackSourceRow {
    pub media_version_id: i64,
    pub name: String,
    pub width: i64,
    pub height: i64,
    pub streams: Vec<MoviePlaybackStreamRow>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PinnedStreamSelectionForm {
    pub media_version_id: i64,
    pub video_stream_index: i32,
    pub audio_stream_index: Option<i32>,
    pub subtitle_stream_index: Option<i32>,
}

/// Page-map for the alphabetic jump-bar. Mirrors `ErsatzTV`'s
/// `SearchPageMap.PageMap`: for each bucket ('#' or 'a'..='z'), the 1-based
/// page number that contains the first item starting with that letter.
///
/// Stored as a 27-element vector indexed by [`bucket_index`] (0 = '#',
/// 1 = 'a', …, 26 = 'z') so the type is `Serialize`/`Deserialize`-friendly
/// for `server_fn`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LetterPageMap {
    pub pages: Vec<u32>,
}

/// Returns the bucket index for a letter: `'#' => 0`, `'a'..='z' => 1..=26`.
/// Out-of-range characters bucket to '#'.
#[must_use]
pub fn bucket_index(letter: char) -> usize {
    let lc = letter.to_ascii_lowercase();
    if lc.is_ascii_lowercase() {
        (lc as usize - 'a' as usize) + 1
    } else {
        0
    }
}

/// Detailed projection for the movie details page. Mirrors the relevant
/// subset of `ErsatzTV.Application.Movies.MovieViewModel`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MovieDetailRow {
    pub id: i64,
    pub title: String,
    pub year: Option<i64>,
    pub plot: Option<String>,
    pub tagline: Option<String>,
    pub content_ratings: Vec<String>,
    pub release_date: Option<String>,
    pub poster_url: Option<String>,
    pub fanart_url: Option<String>,
    pub file_path: Option<String>,
    pub local_path: Option<String>,
    /// `MediaItem.State`: 0 = Normal, 1 = `FileNotFound`, 2 = Unavailable.
    pub media_item_state: i32,
    pub genres: Vec<String>,
    pub tags: Vec<String>,
    pub languages: Vec<String>,
    pub studios: Vec<String>,
    pub music_labels: Vec<String>,
    pub directors: Vec<String>,
    pub writers: Vec<String>,
    pub actors: Vec<ActorCardRow>,
    pub extras: Vec<MediaCardRow>,
    pub alternate_versions: Vec<MediaCardRow>,
    pub audio_tracks: Vec<AudioTrackRow>,
    pub commentary_subtitles: Vec<CommentarySubtitleRow>,
    pub playback_sources: Vec<MoviePlaybackSourceRow>,
}

/// Detailed projection for the other-video details page. ErsatzTV only ships
/// `OtherVideoList.razor`; this detail projection uses the same
/// `OtherVideoMetadata` graph that `OtherVideoRepository.GetOrAdd` loads and
/// the same `OtherVideoCardViewModel` thumbnail-before-poster artwork rule.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct OtherVideoDetailRow {
    pub id: i64,
    pub title: String,
    pub original_title: Option<String>,
    pub year: Option<i64>,
    pub plot: Option<String>,
    pub tagline: Option<String>,
    pub content_ratings: Vec<String>,
    pub release_date: Option<String>,
    pub artwork_url: Option<String>,
    pub fanart_url: Option<String>,
    pub file_path: Option<String>,
    pub local_path: Option<String>,
    /// `MediaItem.State`: 0 = Normal, 1 = `FileNotFound`, 2 = Unavailable.
    pub media_item_state: i32,
    pub genres: Vec<String>,
    pub tags: Vec<String>,
    pub languages: Vec<String>,
    pub studios: Vec<String>,
    pub music_labels: Vec<String>,
    pub directors: Vec<String>,
    pub writers: Vec<String>,
    pub actors: Vec<ActorCardRow>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FacetedMediaDetailRow {
    pub id: i64,
    pub title: String,
    pub original_title: Option<String>,
    pub year: Option<i64>,
    pub plot: Option<String>,
    pub release_date: Option<String>,
    pub artwork_url: Option<String>,
    pub fanart_url: Option<String>,
    pub file_path: Option<String>,
    pub local_path: Option<String>,
    pub media_item_state: i32,
    pub facets: MediaFacetSetRow,
}

/// WASM-safe transport projection of the shared core metadata-facet contract.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MediaFacetSetRow {
    pub content_ratings: Vec<String>,
    pub studios: Vec<String>,
    pub music_labels: Vec<String>,
    pub languages: Vec<String>,
    pub genres: Vec<String>,
    pub tags: Vec<String>,
}

#[cfg(feature = "ssr")]
impl From<ersatzrs_core::metadata_facets::MediaFacetSet> for MediaFacetSetRow {
    fn from(facets: ersatzrs_core::metadata_facets::MediaFacetSet) -> Self {
        Self {
            content_ratings: facets.content_ratings,
            studios: facets.studios,
            music_labels: facets.music_labels,
            languages: facets.languages,
            genres: facets.genres,
            tags: facets.tags,
        }
    }
}

/// One season card on the show details page. Mirrors
/// `ErsatzTV.Application.MediaCards.TelevisionSeasonCardViewModel`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SeasonCardRow {
    pub id: i64,
    pub season_number: i64,
    /// `"Specials"` when `season_number == 0`, else `"Season {n}"`.
    pub title: String,
    pub poster_url: Option<String>,
}

/// One cast member on the show details page. Mirrors
/// `ErsatzTV.Application.MediaCards.ActorCardViewModel`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ActorCardRow {
    pub id: i64,
    pub name: String,
    pub role: Option<String>,
    pub thumb_url: Option<String>,
}

/// Detailed projection for the TV show details page. Mirrors the relevant
/// subset of `ErsatzTV.Application.Television.TelevisionShowViewModel`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ShowDetailRow {
    pub id: i64,
    pub library_id: i64,
    /// `Local` | `Plex` | `Jellyfin` | `Emby` — drives the Scan menu and
    /// matches `Television/Mapper.cs`'s `MediaSourceKind` resolution.
    pub media_source_kind: String,
    pub title: String,
    pub year: Option<i64>,
    pub plot: Option<String>,
    pub content_ratings: Vec<String>,
    pub poster_url: Option<String>,
    pub fanart_url: Option<String>,
    /// `MediaItem.State`: 0 = Normal, 1 = `FileNotFound`, 2 = Unavailable.
    pub media_item_state: i32,
    pub genres: Vec<String>,
    pub tags: Vec<String>,
    pub languages: Vec<String>,
    pub studios: Vec<String>,
    pub music_labels: Vec<String>,
    pub networks: Vec<String>,
    pub actors: Vec<ActorCardRow>,
    pub seasons: Vec<SeasonCardRow>,
    pub extras: Vec<MediaCardRow>,
}

/// One episode row on the TV season details page. Mirrors
/// `ErsatzTV.Application.MediaCards.TelevisionEpisodeCardViewModel`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct EpisodeCardRow {
    pub episode_id: i64,
    pub aired: String,
    pub show_title: String,
    pub show_id: i64,
    pub season_id: i64,
    pub season_number: i64,
    pub episode_number: i64,
    pub title: String,
    pub sort_title: String,
    pub plot: Option<String>,
    pub thumbnail_url: Option<String>,
    pub directors: Vec<String>,
    pub writers: Vec<String>,
    /// `MediaItem.State`: 0 = Normal, 1 = `FileNotFound`, 2 = Unavailable.
    pub media_item_state: i32,
    pub file_path: Option<String>,
    pub local_path: Option<String>,
}

/// Detailed projection for the TV season details page. Mirrors the relevant
/// subset of `ErsatzTV.Application.Television.TelevisionSeasonViewModel`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SeasonDetailRow {
    pub id: i64,
    pub show_id: i64,
    pub show_title: String,
    pub show_year: Option<i64>,
    /// `"Specials"` when `season_number == 0`, else `"Season {n}"`.
    pub name: String,
    pub season_number: i64,
    pub poster_url: Option<String>,
    pub fanart_url: Option<String>,
    pub episode_count: i64,
    pub episodes: Vec<EpisodeCardRow>,
    pub extras: Vec<MediaCardRow>,
}

/// One music-video row on the artist details page. Mirrors
/// `ErsatzTV.Application.MediaCards.MusicVideoCardViewModel`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ArtistMusicVideoRow {
    pub id: i64,
    pub title: String,
    pub sort_title: String,
    pub plot: Option<String>,
    pub album: Option<String>,
    pub thumbnail_url: Option<String>,
    /// `MediaItem.State`: 0 = Normal, 1 = `FileNotFound`, 2 = Unavailable.
    pub media_item_state: i32,
    pub file_path: Option<String>,
    pub local_path: Option<String>,
}

/// Detailed projection for the artist details page. Mirrors the relevant
/// subset of `ErsatzTV.Application.Artists.ArtistViewModel` plus the first
/// 100 `GetMusicVideoCards` rows.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ArtistDetailRow {
    pub id: i64,
    pub title: String,
    pub disambiguation: Option<String>,
    pub biography: Option<String>,
    pub thumbnail_url: Option<String>,
    pub fanart_url: Option<String>,
    pub genres: Vec<String>,
    pub styles: Vec<String>,
    pub moods: Vec<String>,
    pub languages: Vec<String>,
    pub music_videos: Vec<ArtistMusicVideoRow>,
    pub versions: Vec<ArtistMusicVideoRow>,
}

#[cfg(feature = "ssr")]
#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
struct MediaItemInfoJson {
    id: i64,
    title: String,
    kind: String,
    library_kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    server_name: Option<String>,
    library_name: String,
    state: String,
    duration: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    sample_aspect_ratio: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    display_aspect_ratio: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    r_frame_rate: Option<String>,
    video_scan_kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    interlaced_ratio: Option<f64>,
    width: i64,
    height: i64,
    streams: Vec<MediaItemInfoStreamJson>,
    chapters: Vec<MediaItemInfoChapterJson>,
}

#[cfg(feature = "ssr")]
#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
struct MediaItemInfoStreamJson {
    #[serde(skip_serializing_if = "Option::is_none")]
    index: Option<i64>,
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    codec: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    channels: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    default: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    forced: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    attached_pic: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pixel_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    color_range: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    color_space: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    color_transfer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    color_primaries: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bits_per_raw_sample: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    file_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    is_extracted: Option<bool>,
}

#[cfg(feature = "ssr")]
#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
struct MediaItemInfoChapterJson {
    title: String,
    start_time: String,
    end_time: String,
}

#[cfg(all(test, feature = "ssr"))]
struct SubtitleInfoRow {
    stream_index: i64,
    subtitle_kind: i64,
    title: Option<String>,
    codec: Option<String>,
    language: Option<String>,
    default: bool,
    forced: bool,
    path: Option<String>,
    is_extracted: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CollectionListRow {
    pub id: i64,
    pub name: String,
    /// Only populated for smart collections.
    pub query: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CollectionForm {
    pub id: i64,
    pub name: String,
    /// Only used by smart collections.
    pub query: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MultiCollectionListRow {
    pub id: i64,
    pub name: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MultiCollectionItemForm {
    pub collection_id: Option<i64>,
    pub smart_collection_id: Option<i64>,
    pub name: String,
    pub collection_type: i32,
    pub use_custom_playback_order: bool,
    pub schedule_as_group: bool,
    pub playback_order: i32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MultiCollectionForm {
    pub id: i64,
    pub name: String,
    pub items: Vec<MultiCollectionItemForm>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MultiCollectionSelectOption {
    pub id: i64,
    pub name: String,
    pub collection_type: i32,
    pub use_custom_playback_order: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MultiCollectionEditorOptions {
    pub collections: Vec<MultiCollectionSelectOption>,
    pub smart_collections: Vec<MultiCollectionSelectOption>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RerunCollectionListRow {
    pub id: i64,
    pub name: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RerunCollectionForm {
    pub id: i64,
    pub name: String,
    pub collection_type: i32,
    pub collection_id: Option<i64>,
    pub media_item_id: Option<i64>,
    pub multi_collection_id: Option<i64>,
    pub smart_collection_id: Option<i64>,
    pub first_run_playback_order: i32,
    pub rerun_playback_order: i32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RerunCollectionSelectOption {
    pub id: i64,
    pub name: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RerunCollectionEditorOptions {
    pub collections: Vec<RerunCollectionSelectOption>,
    pub multi_collections: Vec<RerunCollectionSelectOption>,
    pub smart_collections: Vec<RerunCollectionSelectOption>,
    pub shows: Vec<RerunCollectionSelectOption>,
    pub seasons: Vec<RerunCollectionSelectOption>,
    pub artists: Vec<RerunCollectionSelectOption>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlaylistGroupRow {
    pub id: i64,
    pub name: String,
    pub is_system: bool,
    pub playlists: Vec<PlaylistRow>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlaylistRow {
    pub id: i64,
    pub playlist_group_id: i64,
    pub name: String,
    pub is_system: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlaylistPageData {
    pub groups: Vec<PlaylistGroupRow>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlaylistItemForm {
    pub id: i64,
    pub index: i32,
    pub collection_type: i32,
    pub collection_id: Option<i64>,
    pub multi_collection_id: Option<i64>,
    pub smart_collection_id: Option<i64>,
    pub media_item_id: Option<i64>,
    pub item_name: String,
    pub playback_order: i32,
    pub count: Option<i32>,
    pub play_all: bool,
    pub include_in_program_guide: bool,
    pub preferred_audio_language_code: Option<String>,
    pub preferred_audio_title: Option<String>,
    pub preferred_subtitle_language_code: Option<String>,
    pub subtitle_mode: Option<i32>,
    pub pinned_stream_selection: Option<PinnedStreamSelectionForm>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlaylistEditorForm {
    pub id: i64,
    pub playlist_group_id: i64,
    pub name: String,
    pub is_system: bool,
    pub items: Vec<PlaylistItemForm>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlaylistEditorOptions {
    pub collections: Vec<PlaylistItemOption>,
    pub multi_collections: Vec<PlaylistItemOption>,
    pub smart_collections: Vec<PlaylistItemOption>,
    pub shows: Vec<PlaylistItemOption>,
    pub seasons: Vec<PlaylistItemOption>,
    pub artists: Vec<PlaylistItemOption>,
    pub movies: Vec<PlaylistItemOption>,
    pub episodes: Vec<PlaylistItemOption>,
    pub music_videos: Vec<PlaylistItemOption>,
    pub other_videos: Vec<PlaylistItemOption>,
    pub songs: Vec<PlaylistItemOption>,
    pub images: Vec<PlaylistItemOption>,
    pub remote_streams: Vec<PlaylistItemOption>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlaylistItemOption {
    pub id: i64,
    pub name: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlaylistPreviewRow {
    pub start: String,
    pub finish: String,
    pub media_item: String,
    pub duration: String,
}

#[cfg(feature = "ssr")]
fn playlist_row_from_admin(row: ersatzrs_core::repositories::PlaylistAdminRow) -> PlaylistRow {
    PlaylistRow {
        id: row.id,
        playlist_group_id: row.playlist_group_id,
        name: row.name,
        is_system: row.is_system,
    }
}

#[cfg(feature = "ssr")]
fn media_card_from_admin(row: ersatzrs_core::repositories::AdminMediaCard) -> MediaCardRow {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};

    MediaCardRow {
        id: row.id,
        title: row.title,
        sort_title: row.sort_title,
        subtitle: row.subtitle,
        artwork_path: row
            .artwork_path
            .and_then(|path| artwork_url(&path, ArtworkUrlKind::Poster)),
        href: None,
        season_number: row.season_number,
        variant: ContentVariantInfo {
            content_kind: row.content_kind,
            sub_kind: row.sub_kind,
            parent_title: row.parent_title,
        },
    }
}

#[cfg(feature = "ssr")]
fn playlist_group_from_admin(
    row: ersatzrs_core::repositories::PlaylistGroupAdminRow,
) -> PlaylistGroupRow {
    PlaylistGroupRow {
        id: row.id,
        name: row.name,
        is_system: row.is_system,
        playlists: row
            .playlists
            .into_iter()
            .map(playlist_row_from_admin)
            .collect(),
    }
}

#[cfg(feature = "ssr")]
fn playlist_item_from_admin(
    item: ersatzrs_core::repositories::PlaylistItemAdmin,
) -> PlaylistItemForm {
    PlaylistItemForm {
        id: item.id,
        index: item.index,
        collection_type: item.collection_type,
        collection_id: item.collection_id,
        multi_collection_id: item.multi_collection_id,
        smart_collection_id: item.smart_collection_id,
        media_item_id: item.media_item_id,
        item_name: item.item_name,
        playback_order: item.playback_order,
        count: item.count,
        play_all: item.play_all,
        include_in_program_guide: item.include_in_program_guide,
        preferred_audio_language_code: item.preferred_audio_language_code,
        preferred_audio_title: item.preferred_audio_title,
        preferred_subtitle_language_code: item.preferred_subtitle_language_code,
        subtitle_mode: item.subtitle_mode,
        pinned_stream_selection: item.pinned_stream_selection.map(|selection| {
            PinnedStreamSelectionForm {
                media_version_id: selection.media_version_id,
                video_stream_index: selection.video_stream_index,
                audio_stream_index: selection.audio_stream_index,
                subtitle_stream_index: selection.subtitle_stream_index,
            }
        }),
    }
}

#[cfg(feature = "ssr")]
fn playlist_editor_from_admin(
    form: ersatzrs_core::repositories::PlaylistEditorAdmin,
) -> PlaylistEditorForm {
    PlaylistEditorForm {
        id: form.id,
        playlist_group_id: form.playlist_group_id,
        name: form.name,
        is_system: form.is_system,
        items: form
            .items
            .into_iter()
            .map(playlist_item_from_admin)
            .collect(),
    }
}

#[cfg(feature = "ssr")]
pub(crate) fn playlist_editor_into_admin(
    form: PlaylistEditorForm,
) -> ersatzrs_core::repositories::PlaylistEditorAdmin {
    ersatzrs_core::repositories::PlaylistEditorAdmin {
        id: form.id,
        playlist_group_id: form.playlist_group_id,
        name: form.name,
        is_system: form.is_system,
        items: form
            .items
            .into_iter()
            .map(|item| ersatzrs_core::repositories::PlaylistItemAdmin {
                id: item.id,
                index: item.index,
                collection_type: item.collection_type,
                collection_id: item.collection_id,
                multi_collection_id: item.multi_collection_id,
                smart_collection_id: item.smart_collection_id,
                media_item_id: item.media_item_id,
                item_name: item.item_name,
                playback_order: item.playback_order,
                count: item.count,
                play_all: item.play_all,
                include_in_program_guide: item.include_in_program_guide,
                preferred_audio_language_code: item.preferred_audio_language_code,
                preferred_audio_title: item.preferred_audio_title,
                preferred_subtitle_language_code: item.preferred_subtitle_language_code,
                subtitle_mode: item.subtitle_mode,
                pinned_stream_selection: item.pinned_stream_selection.map(|selection| {
                    ersatzrs_domain::PinnedStreamSelection {
                        media_version_id: selection.media_version_id,
                        video_stream_index: selection.video_stream_index,
                        audio_stream_index: selection.audio_stream_index,
                        subtitle_stream_index: selection.subtitle_stream_index,
                    }
                }),
            })
            .collect(),
    }
}

#[cfg(feature = "ssr")]
fn playlist_options_from_admin(
    options: ersatzrs_core::repositories::PlaylistEditorOptions,
) -> PlaylistEditorOptions {
    fn map(rows: Vec<ersatzrs_core::repositories::PlaylistItemOption>) -> Vec<PlaylistItemOption> {
        rows.into_iter()
            .map(|row| PlaylistItemOption {
                id: row.id,
                name: row.name,
            })
            .collect()
    }

    PlaylistEditorOptions {
        collections: map(options.collections),
        multi_collections: map(options.multi_collections),
        smart_collections: map(options.smart_collections),
        shows: map(options.shows),
        seasons: map(options.seasons),
        artists: map(options.artists),
        movies: map(options.movies),
        episodes: map(options.episodes),
        music_videos: map(options.music_videos),
        other_videos: map(options.other_videos),
        songs: map(options.songs),
        images: map(options.images),
        remote_streams: map(options.remote_streams),
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TrashPageData {
    pub movies: Vec<MediaCardRow>,
    pub shows: Vec<MediaCardRow>,
    #[serde(default)]
    pub seasons: Vec<MediaCardRow>,
    #[serde(default)]
    pub episodes: Vec<MediaCardRow>,
    pub artists: Vec<MediaCardRow>,
    #[serde(default)]
    pub music_videos: Vec<MediaCardRow>,
    pub other_videos: Vec<MediaCardRow>,
    pub songs: Vec<MediaCardRow>,
    pub images: Vec<MediaCardRow>,
    #[serde(default)]
    pub remote_streams: Vec<MediaCardRow>,
}

fn song_subtitle(artists: Option<String>, album: Option<String>) -> Option<String> {
    let artists = artists.unwrap_or_default();
    let album = album.unwrap_or_default();
    match (artists.trim(), album.trim()) {
        ("", "") => None,
        ("", album) => Some(album.to_string()),
        (artists, "") => Some(artists.to_string()),
        (artists, album) => Some(format!("{artists} - {album}")),
    }
}

fn season_name(season_number: i64) -> String {
    if season_number == 0 {
        "Specials".to_string()
    } else {
        format!("Season {season_number}")
    }
}

fn short_date(value: Option<&str>) -> String {
    use chrono::{DateTime, Datelike, NaiveDate, Utc};

    let Some(value) = value.filter(|v| !v.trim().is_empty()) else {
        return "1-1-0001".to_string();
    };

    if let Ok(dt) = DateTime::parse_from_rfc3339(value) {
        let date = dt.with_timezone(&Utc).date_naive();
        return format!("{}-{}-{}", date.day(), date.month(), date.year());
    }
    if let Ok(date) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        return format!("{}-{}-{}", date.day(), date.month(), date.year());
    }

    value.to_string()
}

#[cfg(all(test, feature = "ssr"))]
fn content_kind_from_db(value: i32) -> ersatzrs_domain::ContentKind {
    match value {
        1 => ersatzrs_domain::ContentKind::Extra,
        2 => ersatzrs_domain::ContentKind::AlternateVersion,
        _ => ersatzrs_domain::ContentKind::Primary,
    }
}

#[cfg(all(test, feature = "ssr"))]
fn content_variant_from_row(row: &ersatzrs_infra::db::DbRow) -> ContentVariantInfo {
    use sqlx::Row;

    ContentVariantInfo {
        content_kind: content_kind_from_db(row.try_get("ContentKind").unwrap_or(0)),
        sub_kind: row.try_get::<Option<String>, _>("SubKind").unwrap_or(None),
        parent_title: row
            .try_get::<Option<String>, _>("ParentTitle")
            .unwrap_or(None),
    }
}

#[cfg(all(test, feature = "ssr"))]
async fn load_extra_cards(
    pool: &ersatzrs_infra::db::DbPool,
    parent_media_item_id: i64,
) -> Result<Vec<MediaCardRow>, ServerFnError> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT ov."Id" AS "Id",
                  COALESCE(ovm."Title", '') AS "Title",
                  COALESCE(ovm."SortTitle", ovm."Title", '') AS "SortTitle",
                  ovm."Year" AS "Year",
                  COALESCE(thumb."Path", poster."Path") AS "Artwork",
                  COALESCE(mi."ContentKind", 0) AS "ContentKind",
                  mi."SubKind" AS "SubKind",
                  NULL AS "ParentTitle"
           FROM "OtherVideo" ov
           JOIN "OtherVideoMetadata" ovm ON ovm."OtherVideoId" = ov."Id"
           JOIN "MediaItem" mi ON mi."Id" = ov."Id"
           LEFT JOIN "Artwork" thumb
               ON thumb."OtherVideoMetadataId" = ovm."Id" AND thumb."ArtworkKind" = 1
           LEFT JOIN "Artwork" poster
               ON poster."OtherVideoMetadataId" = ovm."Id" AND poster."ArtworkKind" = 0
           WHERE mi."ParentMediaItemId" = ?
             AND COALESCE(mi."ContentKind", 0) = 1
           ORDER BY COALESCE(mi."SubKind", ''), ovm."SortTitle", ovm."Title""#,
    )
    .bind(parent_media_item_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| MediaCardRow {
            id: r.try_get("Id").unwrap_or(0),
            title: r.try_get("Title").unwrap_or_default(),
            sort_title: r.try_get("SortTitle").unwrap_or_default(),
            subtitle: r
                .try_get::<Option<i64>, _>("Year")
                .unwrap_or(None)
                .map(|year| year.to_string()),
            artwork_path: r
                .try_get::<Option<String>, _>("Artwork")
                .unwrap_or(None)
                .and_then(|p| artwork_url(&p, ArtworkUrlKind::Poster)),
            href: None,
            season_number: None,
            variant: content_variant_from_row(r),
        })
        .collect())
}

#[cfg(all(test, feature = "ssr"))]
async fn load_movie_alternate_version_cards(
    pool: &ersatzrs_infra::db::DbPool,
    parent_movie_id: i64,
) -> Result<Vec<MediaCardRow>, ServerFnError> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT m."Id" AS "Id",
                  COALESCE(mm."Title", '') AS "Title",
                  COALESCE(mm."SortTitle", mm."Title", '') AS "SortTitle",
                  mm."Year" AS "Year",
                  a."Path" AS "Artwork",
                  COALESCE(mi."ContentKind", 0) AS "ContentKind",
                  mi."SubKind" AS "SubKind",
                  pmm."Title" AS "ParentTitle"
           FROM "Movie" m
           JOIN "MovieMetadata" mm ON mm."MovieId" = m."Id"
           JOIN "MediaItem" mi ON mi."Id" = m."Id"
           LEFT JOIN "MovieMetadata" pmm ON pmm."MovieId" = mi."ParentMediaItemId"
           LEFT JOIN "Artwork" a
               ON a."MovieMetadataId" = mm."Id" AND a."ArtworkKind" = 0
           WHERE mi."ParentMediaItemId" = ?
             AND COALESCE(mi."ContentKind", 0) = 2
           ORDER BY COALESCE(mi."SubKind", ''), mm."SortTitle", mm."Title""#,
    )
    .bind(parent_movie_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| MediaCardRow {
            id: r.try_get("Id").unwrap_or(0),
            title: r.try_get("Title").unwrap_or_default(),
            sort_title: r.try_get("SortTitle").unwrap_or_default(),
            subtitle: r
                .try_get::<Option<i64>, _>("Year")
                .unwrap_or(None)
                .map(|year| year.to_string()),
            artwork_path: r
                .try_get::<Option<String>, _>("Artwork")
                .unwrap_or(None)
                .and_then(|p| artwork_url(&p, ArtworkUrlKind::Poster)),
            href: None,
            season_number: None,
            variant: content_variant_from_row(r),
        })
        .collect())
}

#[cfg(all(test, feature = "ssr"))]
async fn load_movie_audio_tracks(
    pool: &ersatzrs_infra::db::DbPool,
    movie_id: i64,
) -> Result<Vec<AudioTrackRow>, ServerFnError> {
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT mv."Id" AS "MediaVersionId",
                  ms."Index" AS "StreamIndex",
                  COALESCE(NULLIF(ms."Title", ''), 'Default Audio') AS "Title",
                  ms."Language" AS "Language",
                  ms."Codec" AS "Codec",
                  ms."Channels" AS "Channels",
                  ms."Default" AS "Default"
           FROM "MediaVersion" mv
           JOIN "MediaStream" ms ON ms."MediaVersionId" = mv."Id"
           WHERE mv."MovieId" = ?
             AND ms."MediaStreamKind" = 2
           ORDER BY mv."Id", ms."Index""#,
    )
    .bind(movie_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| {
            let title: String = r.try_get("Title").unwrap_or_default();
            AudioTrackRow {
                media_version_id: r.try_get("MediaVersionId").unwrap_or(0),
                stream_index: r.try_get("StreamIndex").unwrap_or(0),
                is_commentary: title.to_ascii_lowercase().contains("commentary"),
                title,
                language: r.try_get::<Option<String>, _>("Language").unwrap_or(None),
                codec: r.try_get::<Option<String>, _>("Codec").unwrap_or(None),
                channels: r.try_get("Channels").unwrap_or_default(),
                is_default: r.try_get::<i64, _>("Default").unwrap_or_default() != 0,
            }
        })
        .collect())
}

#[cfg(all(test, feature = "ssr"))]
async fn load_movie_commentary_subtitles(
    pool: &ersatzrs_infra::db::DbPool,
    movie_id: i64,
) -> Result<Vec<CommentarySubtitleRow>, ServerFnError> {
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT DISTINCT sub."Id" AS "Id",
                  sub."StreamIndex" AS "StreamIndex",
                  COALESCE(NULLIF(sub."Title", ''), NULLIF(ms."Title", ''), 'Commentary') AS "Title",
                  sub."Language" AS "Language",
                  sub."Codec" AS "Codec"
           FROM "Subtitle" sub
           JOIN "MovieMetadata" mm ON mm."Id" = sub."MovieMetadataId"
           LEFT JOIN "MediaVersion" mv ON mv."MovieId" = mm."MovieId"
           LEFT JOIN "MediaStream" ms
              ON ms."MediaVersionId" = mv."Id"
             AND ms."Index" = sub."StreamIndex"
             AND ms."MediaStreamKind" = 3
           WHERE mm."MovieId" = ?
             AND NOT (
                 sub."SubtitleKind" = 0
                 AND sub."IsExtracted" = 0
                 AND COALESCE(sub."Codec", '') NOT IN ('dvd_subtitle', 'hdmv_pgs_subtitle', 'xsub')
             )
             AND (
                 LOWER(COALESCE(sub."Title", '')) LIKE '%commentary%'
                 OR LOWER(COALESCE(ms."Title", '')) LIKE '%commentary%'
             )
           ORDER BY sub."StreamIndex", sub."Id""#,
    )
    .bind(movie_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| CommentarySubtitleRow {
            id: r.try_get("Id").unwrap_or(0),
            stream_index: r.try_get("StreamIndex").unwrap_or(0),
            title: r.try_get("Title").unwrap_or_default(),
            language: r.try_get::<Option<String>, _>("Language").unwrap_or(None),
            codec: r.try_get::<Option<String>, _>("Codec").unwrap_or(None),
        })
        .collect())
}

mod autocomplete;
mod collection_detail;
mod collections;
mod kinds;
mod playlists;
mod trash;

#[cfg(all(test, feature = "ssr"))]
mod test_support;

pub use autocomplete::{
    CollectionMember, CollectionOption, MediaItemSuggestion, PlaylistGroupOption, PlaylistOption,
    ScheduleOption,
};
pub use collection_detail::{CollectionDetail, CollectionItemCard};
#[cfg(feature = "ssr")]
pub use collections::manual::save_collection_with_state;
#[cfg(feature = "ssr")]
pub use collections::multi::save_multi_collection_with_state;
#[cfg(feature = "ssr")]
pub use collections::rerun::save_rerun_collection_with_state;
#[cfg(all(test, feature = "ssr"))]
pub use collections::smart::list_smart_collections_with_pool;
#[cfg(feature = "ssr")]
pub use collections::smart::save_smart_collection_with_state;
#[cfg(feature = "ssr")]
pub use kinds::movies::media_item_info_json_from_admin;
#[cfg(all(test, feature = "ssr"))]
pub use kinds::movies::{get_media_item_info_json_with_pool, get_movie_details_with_pool};

#[server]
pub async fn list_movies(
    search: String,
    page: u32,
    page_size: u32,
    show_variants: bool,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    kinds::movies::list_movies_impl(search, page, page_size, show_variants).await
}

#[server]
#[allow(clippy::too_many_lines)]
pub async fn get_movie_details(id: i64) -> Result<Option<MovieDetailRow>, ServerFnError> {
    kinds::movies::get_movie_details_impl(id).await
}

#[server]
#[allow(clippy::too_many_lines)]
pub async fn get_show_details(id: i64) -> Result<Option<ShowDetailRow>, ServerFnError> {
    kinds::movies::get_show_details_impl(id).await
}

#[server]
pub async fn get_season_details(id: i64) -> Result<Option<SeasonDetailRow>, ServerFnError> {
    kinds::movies::get_season_details_impl(id).await
}

#[server]
#[allow(clippy::too_many_lines)]
pub async fn get_media_item_info_json(id: i64) -> Result<String, ServerFnError> {
    kinds::movies::get_media_item_info_json_impl(id).await
}

#[server]
pub async fn list_shows(
    search: String,
    page: u32,
    page_size: u32,
    show_variants: bool,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    kinds::shows::list_shows_impl(search, page, page_size, show_variants).await
}

#[server]
pub async fn list_images(
    search: String,
    page: u32,
    page_size: u32,
    show_variants: bool,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    kinds::images::list_images_impl(search, page, page_size, show_variants).await
}

#[server]
pub async fn get_image_details(id: i64) -> Result<Option<FacetedMediaDetailRow>, ServerFnError> {
    get_faceted_media_details_impl(
        id,
        ersatzrs_core::repositories::AdminFacetedMediaKind::Image,
    )
    .await
}

#[server]
pub async fn list_artists(
    search: String,
    page: u32,
    page_size: u32,
    show_variants: bool,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    kinds::artists::list_artists_impl(search, page, page_size, show_variants).await
}

#[server]
pub async fn get_artist_details(id: i64) -> Result<Option<ArtistDetailRow>, ServerFnError> {
    kinds::artists::get_artist_details_impl(id).await
}

#[server]
pub async fn list_other_videos(
    search: String,
    page: u32,
    page_size: u32,
    show_variants: bool,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    kinds::other_videos::list_other_videos_impl(search, page, page_size, show_variants).await
}

#[server]
pub async fn get_music_video_details(
    id: i64,
) -> Result<Option<FacetedMediaDetailRow>, ServerFnError> {
    get_faceted_media_details_impl(
        id,
        ersatzrs_core::repositories::AdminFacetedMediaKind::MusicVideo,
    )
    .await
}

#[server]
pub async fn get_song_details(id: i64) -> Result<Option<FacetedMediaDetailRow>, ServerFnError> {
    get_faceted_media_details_impl(id, ersatzrs_core::repositories::AdminFacetedMediaKind::Song)
        .await
}

#[cfg(feature = "ssr")]
async fn get_faceted_media_details_impl(
    id: i64,
    kind: ersatzrs_core::repositories::AdminFacetedMediaKind,
) -> Result<Option<FacetedMediaDetailRow>, ServerFnError> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state
        .application
        .get_faceted_media_detail(id, kind)
        .await?
        .map(|detail| FacetedMediaDetailRow {
            id: detail.id,
            title: detail.title,
            original_title: detail.original_title,
            year: detail.year,
            plot: detail.plot,
            release_date: detail.release_date,
            artwork_url: detail
                .artwork_path
                .and_then(|path| artwork_url(&path, ArtworkUrlKind::Thumbnail)),
            fanart_url: detail
                .fanart_path
                .and_then(|path| artwork_url(&path, ArtworkUrlKind::FanArt)),
            file_path: detail.file_path,
            local_path: detail.local_path,
            media_item_state: detail.media_item_state,
            facets: detail.facets.into(),
        }))
}

#[server]
#[allow(clippy::too_many_lines)]
pub async fn get_other_video_details(
    id: i64,
) -> Result<Option<OtherVideoDetailRow>, ServerFnError> {
    kinds::other_videos::get_other_video_details_impl(id).await
}

#[server]
pub async fn list_seasons(
    search: String,
    page: u32,
    page_size: u32,
    show_variants: bool,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    kinds::seasons::list_seasons_impl(search, page, page_size, show_variants).await
}

#[server]
pub async fn list_episodes(
    search: String,
    page: u32,
    page_size: u32,
    show_variants: bool,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    kinds::episodes::list_episodes_impl(search, page, page_size, show_variants).await
}

#[server]
pub async fn list_music_videos(
    search: String,
    page: u32,
    page_size: u32,
    show_variants: bool,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    kinds::music_videos::list_music_videos_impl(search, page, page_size, show_variants).await
}

#[server]
pub async fn remote_stream_page_map(
    search: String,
    page_size: u32,
) -> Result<LetterPageMap, ServerFnError> {
    kinds::remote_streams::remote_stream_page_map_impl(search, page_size).await
}

#[server]
pub async fn list_remote_streams(
    search: String,
    page: u32,
    page_size: u32,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    kinds::remote_streams::list_remote_streams_impl(search, page, page_size).await
}

#[server]
pub async fn list_remote_stream_tags() -> Result<Vec<String>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state.application.list_remote_stream_tags().await?)
}

#[server]
pub async fn list_songs(
    search: String,
    page: u32,
    page_size: u32,
    show_variants: bool,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    kinds::songs::list_songs_impl(search, page, page_size, show_variants).await
}

#[server]
pub async fn list_trash_items() -> Result<TrashPageData, ServerFnError> {
    trash::list_trash_items_impl().await
}

#[server]
pub async fn delete_media_items(ids: Vec<i64>) -> Result<u64, ServerFnError> {
    trash::delete_media_items_impl(ids).await
}

#[server]
pub async fn empty_trash() -> Result<u64, ServerFnError> {
    trash::empty_trash_impl().await
}

#[server]
pub async fn list_collections(
    search: String,
    page: u32,
    page_size: u32,
) -> Result<(Vec<CollectionListRow>, u64), ServerFnError> {
    collections::manual::list_collections_impl(search, page, page_size).await
}

#[server]
pub async fn get_collection(id: i64) -> Result<Option<CollectionForm>, ServerFnError> {
    collections::manual::get_collection_impl(id).await
}

#[server]
pub async fn save_collection(form: CollectionForm) -> Result<i64, ServerFnError> {
    collections::manual::save_collection_impl(form).await
}

#[server]
pub async fn delete_collection(id: i64) -> Result<(), ServerFnError> {
    collections::manual::delete_collection_impl(id).await
}

#[server]
pub async fn list_smart_collections(
    search: String,
    page: u32,
    page_size: u32,
) -> Result<(Vec<CollectionListRow>, u64), ServerFnError> {
    collections::smart::list_smart_collections_impl(search, page, page_size).await
}

#[server]
pub async fn get_smart_collection(id: i64) -> Result<Option<CollectionForm>, ServerFnError> {
    collections::smart::get_smart_collection_impl(id).await
}

#[server]
pub async fn save_smart_collection(form: CollectionForm) -> Result<i64, ServerFnError> {
    collections::smart::save_smart_collection_impl(form).await
}

#[server]
pub async fn delete_smart_collection(id: i64) -> Result<(), ServerFnError> {
    collections::smart::delete_smart_collection_impl(id).await
}

#[server]
pub async fn get_multi_collections_page_size() -> Result<u32, ServerFnError> {
    collections::multi::get_multi_collections_page_size_impl().await
}

#[server]
pub async fn list_multi_collections(
    search: String,
    page: u32,
    page_size: u32,
) -> Result<(Vec<MultiCollectionListRow>, u64), ServerFnError> {
    collections::multi::list_multi_collections_impl(search, page, page_size).await
}

#[server]
pub async fn get_multi_collection(id: i64) -> Result<Option<MultiCollectionForm>, ServerFnError> {
    collections::multi::get_multi_collection_impl(id).await
}

#[server]
pub async fn get_multi_collection_editor_options(
) -> Result<MultiCollectionEditorOptions, ServerFnError> {
    collections::multi::get_multi_collection_editor_options_impl().await
}

#[server]
pub async fn save_multi_collection(form: MultiCollectionForm) -> Result<i64, ServerFnError> {
    collections::multi::save_multi_collection_impl(form).await
}

#[server]
pub async fn delete_multi_collection(id: i64) -> Result<(), ServerFnError> {
    collections::multi::delete_multi_collection_impl(id).await
}

#[server]
pub async fn get_rerun_collections_page_size() -> Result<u32, ServerFnError> {
    collections::rerun::get_rerun_collections_page_size_impl().await
}

#[server]
pub async fn list_rerun_collections(
    search: String,
    page: u32,
    page_size: u32,
) -> Result<(Vec<RerunCollectionListRow>, u64), ServerFnError> {
    collections::rerun::list_rerun_collections_impl(search, page, page_size).await
}

#[server]
pub async fn get_rerun_collection_form(
    id: i64,
) -> Result<Option<RerunCollectionForm>, ServerFnError> {
    collections::rerun::get_rerun_collection_form_impl(id).await
}

#[server]
pub async fn get_rerun_collection_editor_options(
) -> Result<RerunCollectionEditorOptions, ServerFnError> {
    collections::rerun::get_rerun_collection_editor_options_impl().await
}

#[server]
pub async fn save_rerun_collection(form: RerunCollectionForm) -> Result<i64, ServerFnError> {
    collections::rerun::save_rerun_collection_impl(form).await
}

#[server]
pub async fn delete_rerun_collection(id: i64) -> Result<(), ServerFnError> {
    collections::rerun::delete_rerun_collection_impl(id).await
}

#[server]
pub async fn suggest_media_items(
    prefix: String,
) -> Result<Vec<MediaItemSuggestion>, ServerFnError> {
    autocomplete::suggest_media_items_impl(prefix).await
}

#[server]
pub async fn add_item_to_collection(
    collection_id: i64,
    media_item_id: i64,
) -> Result<(), ServerFnError> {
    autocomplete::add_item_to_collection_impl(collection_id, media_item_id).await
}

#[server]
pub async fn remove_item_from_collection(
    collection_id: i64,
    media_item_id: i64,
) -> Result<(), ServerFnError> {
    autocomplete::remove_item_from_collection_impl(collection_id, media_item_id).await
}

#[server]
pub async fn list_program_schedules() -> Result<Vec<ScheduleOption>, ServerFnError> {
    autocomplete::list_program_schedules_impl().await
}

#[server]
pub async fn add_show_to_schedule(schedule_id: i64, show_id: i64) -> Result<(), ServerFnError> {
    autocomplete::add_show_to_schedule_impl(schedule_id, show_id).await
}

#[server]
pub async fn add_media_item_to_schedule(
    schedule_id: i64,
    collection_type: i64,
    media_item_id: i64,
) -> Result<(), ServerFnError> {
    autocomplete::add_media_item_to_schedule_impl(schedule_id, collection_type, media_item_id).await
}

#[server]
pub async fn add_media_item_to_schedule_with_stream_selection(
    schedule_id: i64,
    collection_type: i64,
    media_item_id: i64,
    selection: PinnedStreamSelectionForm,
) -> Result<(), ServerFnError> {
    autocomplete::add_media_item_to_schedule_with_stream_selection_impl(
        schedule_id,
        collection_type,
        media_item_id,
        selection,
    )
    .await
}

#[server]
pub async fn list_all_collections() -> Result<Vec<CollectionOption>, ServerFnError> {
    autocomplete::list_all_collections_impl().await
}

#[server]
pub async fn list_smart_collections_for_select() -> Result<Vec<CollectionOption>, ServerFnError> {
    autocomplete::list_smart_collections_for_select_impl().await
}

#[server]
pub async fn create_collection(name: String) -> Result<CollectionOption, ServerFnError> {
    autocomplete::create_collection_impl(name).await
}

#[server]
pub async fn get_playlist_page() -> Result<PlaylistPageData, ServerFnError> {
    playlists::get_playlist_page_impl().await
}

#[server]
pub async fn create_playlist_group(name: String) -> Result<PlaylistGroupRow, ServerFnError> {
    playlists::create_playlist_group_impl(name).await
}

#[server]
pub async fn create_playlist(
    playlist_group_id: i64,
    name: String,
) -> Result<PlaylistRow, ServerFnError> {
    playlists::create_playlist_impl(playlist_group_id, name).await
}

#[server]
pub async fn delete_playlist_group(playlist_group_id: i64) -> Result<(), ServerFnError> {
    playlists::delete_playlist_group_impl(playlist_group_id).await
}

#[server]
pub async fn delete_playlist(playlist_id: i64) -> Result<(), ServerFnError> {
    playlists::delete_playlist_impl(playlist_id).await
}

#[server]
pub async fn get_playlist_editor(
    playlist_id: i64,
) -> Result<Option<PlaylistEditorForm>, ServerFnError> {
    playlists::get_playlist_editor_impl(playlist_id).await
}

#[server]
pub async fn get_playlist_editor_options() -> Result<PlaylistEditorOptions, ServerFnError> {
    playlists::get_playlist_editor_options_impl().await
}

#[server]
pub async fn save_playlist(form: PlaylistEditorForm) -> Result<(), ServerFnError> {
    playlists::save_playlist_impl(form).await
}

#[server]
pub async fn preview_playlist_playout(
    form: PlaylistEditorForm,
) -> Result<Vec<PlaylistPreviewRow>, ServerFnError> {
    playlists::preview_playlist_playout_impl(form).await
}

#[server]
pub async fn list_all_playlist_groups() -> Result<Vec<PlaylistGroupOption>, ServerFnError> {
    playlists::list_all_playlist_groups_impl().await
}

#[server]
pub async fn list_playlists_by_group(
    playlist_group_id: i64,
) -> Result<Vec<PlaylistOption>, ServerFnError> {
    playlists::list_playlists_by_group_impl(playlist_group_id).await
}

#[server]
pub async fn get_collection_detail(id: i64) -> Result<Option<CollectionDetail>, ServerFnError> {
    collection_detail::get_collection_detail_impl(id).await
}

#[server]
pub async fn set_collection_use_custom_order(id: i64, on: bool) -> Result<(), ServerFnError> {
    collection_detail::set_collection_use_custom_order_impl(id, on).await
}

#[server]
pub async fn remove_items_from_collection(
    collection_id: i64,
    collection_item_ids: Vec<i64>,
) -> Result<(), ServerFnError> {
    collection_detail::remove_items_from_collection_impl(collection_id, collection_item_ids).await
}

#[server]
pub async fn update_collection_custom_order(
    collection_id: i64,
    ordered_collection_item_ids: Vec<i64>,
) -> Result<(), ServerFnError> {
    collection_detail::update_collection_custom_order_impl(
        collection_id,
        ordered_collection_item_ids,
    )
    .await
}

#[server]
pub async fn list_collection_members(
    collection_id: i64,
) -> Result<Vec<CollectionMember>, ServerFnError> {
    collection_detail::list_collection_members_impl(collection_id).await
}

// ---------------------------------------------------------------------------
// Page-map + add-to-collection/playlist used by `/media/movies`,
// `/media/tv/shows`, `/media/music/artists`, and `/media/other/videos`.
//
// These functions accept a `kind` discriminant so the same browser shell can
// drive every multi-select media kind with identical SQL shapes (one table
// per kind, joined to its metadata).
// ---------------------------------------------------------------------------

/// Discriminant for the media-browser server functions below. Values match the
/// kind discriminants used by `CollectionItemCard.kind`.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum MediaBrowserKind {
    Movie,
    Show,
    Artist,
    OtherVideo,
    Season,
    Episode,
    MusicVideo,
    RemoteStream,
    Song,
    Image,
}

impl MediaBrowserKind {
    /// Every media-browser discriminant supported by the transport and UI
    /// dispatch registries.
    pub const ALL: [Self; 10] = [
        Self::Movie,
        Self::Show,
        Self::Artist,
        Self::OtherVideo,
        Self::Season,
        Self::Episode,
        Self::MusicVideo,
        Self::RemoteStream,
        Self::Song,
        Self::Image,
    ];
}

#[cfg(feature = "ssr")]
impl MediaBrowserKind {
    const fn media_kind(self) -> &'static str {
        match self {
            Self::Movie => "movie",
            Self::Show => "show",
            Self::Artist => "artist",
            Self::OtherVideo => "other_video",
            Self::Season => "season",
            Self::Episode => "episode",
            Self::MusicVideo => "music_video",
            Self::RemoteStream => "remote_stream",
            Self::Song => "song",
            Self::Image => "image",
        }
    }

    /// `CollectionType` value used when inserting `PlaylistItem` rows.
    /// Matches `ErsatzTV.Core/Domain/CollectionType.cs`.
    const fn collection_type(self) -> i32 {
        match self {
            Self::Movie => 10,
            Self::Show => 1,
            Self::Artist => 3,
            Self::OtherVideo => 40,
            Self::Season => 2,
            Self::Episode => 20,
            Self::MusicVideo => 30,
            Self::RemoteStream => 70,
            Self::Song => 50,
            Self::Image => 60,
        }
    }
}

/// Page map for any browser kind. Ports the
/// `LuceneSearchIndex.GetSearchPageMap` algorithm:
///
/// Walk the filtered, sort-title-ordered item list in `page_size` chunks. For
/// every page, look at the bucket of the last item on the page; every bucket
/// up to and including that letter that has not yet been assigned maps to
/// this page (1-based). Unmatched trailing buckets get the final page number
/// so the rightmost letters still scroll to the last page.
#[server]
pub async fn media_page_map(
    kind: MediaBrowserKind,
    search: String,
    page_size: u32,
    show_variants: bool,
) -> Result<LetterPageMap, ServerFnError> {
    use crate::host::state::AppState;
    use ersatzrs_domain::ContentKind;

    if page_size == 0 {
        return Ok(LetterPageMap { pages: vec![0; 27] });
    }

    let state = expect_context::<AppState>();
    let query = media_card_search_query(kind.media_kind(), &search, show_variants);
    let mut documents = state.application.admin_search_documents(&query).await?;
    documents.retain(|document| show_variants || document.content_kind == ContentKind::Primary);
    documents.sort_by_key(|document| {
        document
            .sort_title
            .as_deref()
            .or(document.title.as_deref())
            .unwrap_or_default()
            .to_ascii_lowercase()
    });
    let buckets: Vec<usize> = documents
        .iter()
        .map(|document| {
            let sort_title = document
                .sort_title
                .as_deref()
                .or(document.title.as_deref())
                .unwrap_or_default();
            let letter = sort_title
                .trim()
                .chars()
                .next()
                .map(|c| c.to_ascii_lowercase())
                .filter(char::is_ascii_alphabetic)
                .unwrap_or('#');
            bucket_index(letter)
        })
        .collect();

    let mut map = vec![0_u32; 27];
    let total = buckets.len();
    if total == 0 {
        return Ok(LetterPageMap { pages: map });
    }

    let mut current = 0_usize;
    let mut page: u32 = 0;
    let page_size_usize = page_size as usize;
    while current < total {
        page += 1;
        current += page_size_usize;
        if current > total {
            current = total;
        }
        let jump = buckets[current - 1];
        for (i, slot) in map.iter_mut().enumerate().take(jump + 1) {
            if *slot == 0 {
                *slot = page;
            }
            let _ = i;
        }
    }

    let max = map.iter().copied().max().unwrap_or(0);
    for slot in &mut map {
        if *slot == 0 {
            *slot = max;
        }
    }

    Ok(LetterPageMap { pages: map })
}

/// Add any media items to a collection. Ports the
/// `AddItemsToCollectionHandler` branches: `CollectionItem` only references a
/// `MediaItemId`, so this works uniformly for every kind. A row with the same
/// media item and an identical playback-override tuple is skipped, so default
/// adds stay idempotent like ErsatzTV. Returns the number of rows actually
/// inserted.
#[server]
pub async fn add_media_items_to_collection(
    collection_id: i64,
    media_item_ids: Vec<i64>,
) -> Result<u64, ServerFnError> {
    add_media_items_to_collection_with_playback(collection_id, media_item_ids, None).await
}

/// Add media items to a collection and optionally persist item-level playback
/// overrides. ErsatzTV `CollectionItem` has no such fields; this is the ADR
/// 0048 ErsatzRS extension. The same media item may appear in one collection
/// several times as long as each row carries a different override tuple (for
/// example one row per commentary track); a row identical to an existing one
/// (media item plus override tuple) is skipped.
#[server]
pub async fn add_media_items_to_collection_with_playback(
    collection_id: i64,
    media_item_ids: Vec<i64>,
    playback: Option<PlaybackOverrideForm>,
) -> Result<u64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .add_collection_items(ersatzrs_app::commands::AddCollectionItems {
            collection_id,
            media_item_ids,
            playback: playback_overrides(playback),
        })
        .await
        .map_err(ServerFnError::new)
}

/// Add media items to a playlist as individual `PlaylistItem` rows. The
/// `CollectionType` is taken from `kind` (see
/// `ErsatzTV.Core/Domain/CollectionType.cs`). Per item:
/// `Index = max(Index) + 1`, `PlaybackOrder = Shuffle (1)`,
/// `IncludeInProgramGuide = 1`, `PlayAll = 0`. Mirrors the per-kind branches
/// of `AddItemsToPlaylistHandler`.
#[server]
pub async fn add_media_items_to_playlist(
    kind: MediaBrowserKind,
    playlist_id: i64,
    media_item_ids: Vec<i64>,
) -> Result<u64, ServerFnError> {
    add_media_items_to_playlist_with_playback(kind, playlist_id, media_item_ids, None).await
}

/// Add media items to a playlist and optionally persist item-level playback
/// overrides. ADR 0048 extends `PlaylistItem` with these nullable fields.
#[server]
pub async fn add_media_items_to_playlist_with_playback(
    kind: MediaBrowserKind,
    playlist_id: i64,
    media_item_ids: Vec<i64>,
    playback: Option<PlaybackOverrideForm>,
) -> Result<u64, ServerFnError> {
    add_media_items_to_playlist_with_selection(kind, playlist_id, media_item_ids, playback, None)
        .await
}

#[server]
pub async fn add_media_items_to_playlist_with_stream_selection(
    kind: MediaBrowserKind,
    playlist_id: i64,
    media_item_ids: Vec<i64>,
    selection: PinnedStreamSelectionForm,
) -> Result<u64, ServerFnError> {
    add_media_items_to_playlist_with_selection(
        kind,
        playlist_id,
        media_item_ids,
        None,
        Some(selection),
    )
    .await
}

#[cfg(feature = "ssr")]
async fn add_media_items_to_playlist_with_selection(
    kind: MediaBrowserKind,
    playlist_id: i64,
    media_item_ids: Vec<i64>,
    playback: Option<PlaybackOverrideForm>,
    selection: Option<PinnedStreamSelectionForm>,
) -> Result<u64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let collection_type = kind.collection_type();
    let playback = playback_overrides(playback);
    let pinned_stream_selection = selection
        .map(validate_pinned_stream_selection)
        .transpose()?;
    if let Some(selection) = pinned_stream_selection {
        for media_item_id in &media_item_ids {
            validate_pinned_stream_selection_for_movie(*media_item_id, selection).await?;
        }
    }
    state
        .application
        .add_playlist_items(ersatzrs_app::commands::AddPlaylistItems {
            playlist_id,
            items: media_item_ids
                .into_iter()
                .map(
                    |media_item_id| ersatzrs_core::repositories::PlaylistItemInsert {
                        collection_type,
                        media_item_id,
                        playback: playback.clone(),
                        pinned_stream_selection,
                    },
                )
                .collect(),
        })
        .await
        .map_err(ServerFnError::new)
}

fn validate_pinned_stream_selection(
    selection: PinnedStreamSelectionForm,
) -> Result<ersatzrs_domain::PinnedStreamSelection, ServerFnError> {
    if selection.media_version_id <= 0 || selection.video_stream_index < 0 {
        return Err(ServerFnError::new(
            "Pinned stream selection is invalid or no longer available.",
        ));
    }
    if selection.audio_stream_index.is_some_and(|index| index < 0)
        || selection
            .subtitle_stream_index
            .is_some_and(|index| index < 0)
    {
        return Err(ServerFnError::new(
            "Pinned stream selection is invalid or no longer available.",
        ));
    }
    Ok(ersatzrs_domain::PinnedStreamSelection {
        media_version_id: selection.media_version_id,
        video_stream_index: selection.video_stream_index,
        audio_stream_index: selection.audio_stream_index,
        subtitle_stream_index: selection.subtitle_stream_index,
    })
}

#[cfg(test)]
mod pinned_stream_selection_tests {
    use super::{validate_pinned_stream_selection, PinnedStreamSelectionForm};

    #[test]
    fn exact_selection_requires_positive_version_and_nonnegative_stream_indexes() {
        let valid = PinnedStreamSelectionForm {
            media_version_id: 42,
            video_stream_index: 0,
            audio_stream_index: Some(2),
            subtitle_stream_index: None,
        };
        let mapped = validate_pinned_stream_selection(valid).expect("valid exact selection");
        assert_eq!(mapped.media_version_id, 42);
        assert_eq!(mapped.video_stream_index, 0);
        assert_eq!(mapped.audio_stream_index, Some(2));
        assert_eq!(mapped.subtitle_stream_index, None);

        for invalid in [
            PinnedStreamSelectionForm {
                media_version_id: 0,
                ..valid
            },
            PinnedStreamSelectionForm {
                video_stream_index: -1,
                ..valid
            },
            PinnedStreamSelectionForm {
                audio_stream_index: Some(-1),
                ..valid
            },
            PinnedStreamSelectionForm {
                subtitle_stream_index: Some(-1),
                ..valid
            },
        ] {
            assert!(validate_pinned_stream_selection(invalid).is_err());
        }
    }
}

#[cfg(feature = "ssr")]
async fn validate_pinned_stream_selection_for_movie(
    media_item_id: i64,
    selection: ersatzrs_domain::PinnedStreamSelection,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let movie = state
        .application
        .get_movie_detail(media_item_id)
        .await?
        .ok_or_else(|| ServerFnError::new("Movie was not found."))?;
    let source = movie
        .playback_sources
        .iter()
        .find(|source| source.media_version_id == selection.media_version_id)
        .ok_or_else(|| {
            ServerFnError::new("Pinned stream selection is invalid or no longer available.")
        })?;
    let has_stream = |kind: i32, index: i32| {
        source
            .streams
            .iter()
            .any(|stream| stream.kind == kind && stream.index == i64::from(index))
    };
    if !has_stream(1, selection.video_stream_index) {
        return Err(ServerFnError::new(
            "Pinned stream selection is invalid or no longer available.",
        ));
    }
    if selection
        .audio_stream_index
        .is_some_and(|index| !has_stream(2, index))
    {
        return Err(ServerFnError::new(
            "Pinned stream selection is invalid or no longer available.",
        ));
    }
    if selection
        .subtitle_stream_index
        .is_some_and(|index| !has_stream(3, index))
    {
        return Err(ServerFnError::new(
            "Pinned stream selection is invalid or no longer available.",
        ));
    }
    Ok(())
}

#[cfg(feature = "ssr")]
fn playback_overrides(
    playback: Option<PlaybackOverrideForm>,
) -> ersatzrs_core::repositories::PlaybackOverrides {
    let playback = playback.unwrap_or_default();
    ersatzrs_core::repositories::PlaybackOverrides {
        preferred_audio_language_code: playback.preferred_audio_language_code,
        preferred_audio_title: playback.preferred_audio_title,
        preferred_subtitle_language_code: playback.preferred_subtitle_language_code,
        subtitle_mode: playback.subtitle_mode,
    }
}

/// One row to be inserted into `PlaylistItem` by
/// `add_media_items_to_playlist_mixed`. Computed by
/// [`plan_mixed_playlist_inserts`] so we can unit-test the per-kind branching
/// and `Index` assignment without touching a database.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MixedPlaylistInsert {
    pub index: i64,
    pub collection_type: i32,
    pub media_item_id: i64,
}

/// Expand a `SearchAllIds` payload into the per-row `PlaylistItem` plan,
/// stamping the matching `CollectionType` per kind. Order matches the C#
/// `AddItemsToPlaylistHandler.ApplyAddItemsRequest` iteration:
/// `Movie` → `TelevisionShow` → `TelevisionSeason` → `Episode` → `Artist` →
/// `MusicVideo` → `OtherVideo` → `Song` → `Image` → `RemoteStream`. `Index`
/// is contiguous starting at `start_index` and crosses kind boundaries
/// (matches the C# `index++`).
///
/// `CollectionType` values come from
/// `$ERSATZTV_SRC/ErsatzTV.Core/Domain/CollectionType.cs`.
#[must_use]
pub fn plan_mixed_playlist_inserts(
    start_index: i64,
    ids: &crate::admin::server_fns::search::SearchAllIds,
) -> Vec<MixedPlaylistInsert> {
    // CollectionType values per ErsatzTV.Core/Domain/CollectionType.cs.
    const MOVIE: i32 = 10;
    const TELEVISION_SHOW: i32 = 1;
    const TELEVISION_SEASON: i32 = 2;
    const EPISODE: i32 = 20;
    const ARTIST: i32 = 3;
    const MUSIC_VIDEO: i32 = 30;
    const OTHER_VIDEO: i32 = 40;
    const SONG: i32 = 50;
    const IMAGE: i32 = 60;
    const REMOTE_STREAM: i32 = 70;

    let groups: [(i32, &[i64]); 10] = [
        (MOVIE, &ids.movie_ids),
        (TELEVISION_SHOW, &ids.show_ids),
        (TELEVISION_SEASON, &ids.season_ids),
        (EPISODE, &ids.episode_ids),
        (ARTIST, &ids.artist_ids),
        (MUSIC_VIDEO, &ids.music_video_ids),
        (OTHER_VIDEO, &ids.other_video_ids),
        (SONG, &ids.song_ids),
        (IMAGE, &ids.image_ids),
        (REMOTE_STREAM, &ids.remote_stream_ids),
    ];

    let mut next_index = start_index;
    let mut out = Vec::new();
    for (collection_type, group) in groups {
        for &media_item_id in group {
            out.push(MixedPlaylistInsert {
                index: next_index,
                collection_type,
                media_item_id,
            });
            next_index += 1;
        }
    }
    out
}

/// Add a mixed-kind id payload (typically the output of `search_all_ids`) to a
/// playlist. Ports `AddItemsToPlaylistHandler` directly: one `PlaylistItem`
/// row per id, `CollectionType` stamped per kind, `Index` continuous across
/// the whole insertion starting at `MAX(Index) + 1`. Per row:
/// `PlaybackOrder = Shuffle (1)`, `IncludeInProgramGuide = 1`, `PlayAll = 0`.
///
/// Sibling to the single-kind [`add_media_items_to_playlist`], which the
/// browser pages still use for their per-kind grids.
#[server]
pub async fn add_media_items_to_playlist_mixed(
    playlist_id: i64,
    ids: crate::admin::server_fns::search::SearchAllIds,
) -> Result<u64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .add_playlist_items(ersatzrs_app::commands::AddPlaylistItems {
            playlist_id,
            items: plan_mixed_playlist_inserts(0, &ids)
                .into_iter()
                .map(|row| ersatzrs_core::repositories::PlaylistItemInsert {
                    collection_type: row.collection_type,
                    media_item_id: row.media_item_id,
                    playback: ersatzrs_core::repositories::PlaybackOverrides::default(),
                    pinned_stream_selection: None,
                })
                .collect(),
        })
        .await
        .map_err(ServerFnError::new)
}

#[cfg(test)]
mod server_fn_path_stability {
    //! Moving a macro-decorated server function changes the hashed endpoint,
    //! even when its Rust import path is re-exported. Keep all macros in this
    //! transport module and guard every generated path while implementations
    //! are organized in child modules.
    use leptos::server_fn::{xxhash_rust::const_xxh64::xxh64, ServerFn};

    fn pinned_route(function_name: &str) -> String {
        let module_path = "ersatzrs_web::admin::server_fns::media_browser";
        let hash = xxh64(format!("ersatzrs:{module_path}").as_bytes(), 0);
        format!("/api/{function_name}{hash}")
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn all_media_browser_server_fn_routes_remain_stable() {
        let routes: [(&str, &str); 76] = [
            (super::ListMovies::PATH, "list_movies"),
            (super::GetMovieDetails::PATH, "get_movie_details"),
            (super::GetShowDetails::PATH, "get_show_details"),
            (super::GetSeasonDetails::PATH, "get_season_details"),
            (
                super::GetMediaItemInfoJson::PATH,
                "get_media_item_info_json",
            ),
            (super::ListShows::PATH, "list_shows"),
            (super::ListImages::PATH, "list_images"),
            (super::GetImageDetails::PATH, "get_image_details"),
            (super::ListArtists::PATH, "list_artists"),
            (super::GetArtistDetails::PATH, "get_artist_details"),
            (super::ListOtherVideos::PATH, "list_other_videos"),
            (super::GetMusicVideoDetails::PATH, "get_music_video_details"),
            (super::GetSongDetails::PATH, "get_song_details"),
            (super::GetOtherVideoDetails::PATH, "get_other_video_details"),
            (super::ListSeasons::PATH, "list_seasons"),
            (super::ListEpisodes::PATH, "list_episodes"),
            (super::ListMusicVideos::PATH, "list_music_videos"),
            (super::RemoteStreamPageMap::PATH, "remote_stream_page_map"),
            (super::ListRemoteStreams::PATH, "list_remote_streams"),
            (super::ListSongs::PATH, "list_songs"),
            (super::ListTrashItems::PATH, "list_trash_items"),
            (super::DeleteMediaItems::PATH, "delete_media_items"),
            (super::EmptyTrash::PATH, "empty_trash"),
            (super::ListCollections::PATH, "list_collections"),
            (super::GetCollection::PATH, "get_collection"),
            (super::SaveCollection::PATH, "save_collection"),
            (super::DeleteCollection::PATH, "delete_collection"),
            (super::ListSmartCollections::PATH, "list_smart_collections"),
            (super::GetSmartCollection::PATH, "get_smart_collection"),
            (super::SaveSmartCollection::PATH, "save_smart_collection"),
            (
                super::DeleteSmartCollection::PATH,
                "delete_smart_collection",
            ),
            (
                super::GetMultiCollectionsPageSize::PATH,
                "get_multi_collections_page_size",
            ),
            (super::ListMultiCollections::PATH, "list_multi_collections"),
            (super::GetMultiCollection::PATH, "get_multi_collection"),
            (
                super::GetMultiCollectionEditorOptions::PATH,
                "get_multi_collection_editor_options",
            ),
            (super::SaveMultiCollection::PATH, "save_multi_collection"),
            (
                super::DeleteMultiCollection::PATH,
                "delete_multi_collection",
            ),
            (
                super::GetRerunCollectionsPageSize::PATH,
                "get_rerun_collections_page_size",
            ),
            (super::ListRerunCollections::PATH, "list_rerun_collections"),
            (
                super::GetRerunCollectionForm::PATH,
                "get_rerun_collection_form",
            ),
            (
                super::GetRerunCollectionEditorOptions::PATH,
                "get_rerun_collection_editor_options",
            ),
            (super::SaveRerunCollection::PATH, "save_rerun_collection"),
            (
                super::DeleteRerunCollection::PATH,
                "delete_rerun_collection",
            ),
            (super::SuggestMediaItems::PATH, "suggest_media_items"),
            (super::AddItemToCollection::PATH, "add_item_to_collection"),
            (
                super::RemoveItemFromCollection::PATH,
                "remove_item_from_collection",
            ),
            (super::ListProgramSchedules::PATH, "list_program_schedules"),
            (super::AddShowToSchedule::PATH, "add_show_to_schedule"),
            (
                super::AddMediaItemToSchedule::PATH,
                "add_media_item_to_schedule",
            ),
            (
                super::AddMediaItemToScheduleWithStreamSelection::PATH,
                "add_media_item_to_schedule_with_stream_selection",
            ),
            (super::ListAllCollections::PATH, "list_all_collections"),
            (
                super::ListSmartCollectionsForSelect::PATH,
                "list_smart_collections_for_select",
            ),
            (super::CreateCollection::PATH, "create_collection"),
            (super::GetPlaylistPage::PATH, "get_playlist_page"),
            (super::CreatePlaylistGroup::PATH, "create_playlist_group"),
            (super::CreatePlaylist::PATH, "create_playlist"),
            (super::DeletePlaylistGroup::PATH, "delete_playlist_group"),
            (super::DeletePlaylist::PATH, "delete_playlist"),
            (super::GetPlaylistEditor::PATH, "get_playlist_editor"),
            (
                super::GetPlaylistEditorOptions::PATH,
                "get_playlist_editor_options",
            ),
            (super::SavePlaylist::PATH, "save_playlist"),
            (
                super::PreviewPlaylistPlayout::PATH,
                "preview_playlist_playout",
            ),
            (
                super::ListAllPlaylistGroups::PATH,
                "list_all_playlist_groups",
            ),
            (super::ListPlaylistsByGroup::PATH, "list_playlists_by_group"),
            (super::GetCollectionDetail::PATH, "get_collection_detail"),
            (
                super::SetCollectionUseCustomOrder::PATH,
                "set_collection_use_custom_order",
            ),
            (
                super::RemoveItemsFromCollection::PATH,
                "remove_items_from_collection",
            ),
            (
                super::UpdateCollectionCustomOrder::PATH,
                "update_collection_custom_order",
            ),
            (
                super::ListCollectionMembers::PATH,
                "list_collection_members",
            ),
            (super::MediaPageMap::PATH, "media_page_map"),
            (
                super::AddMediaItemsToCollection::PATH,
                "add_media_items_to_collection",
            ),
            (
                super::AddMediaItemsToCollectionWithPlayback::PATH,
                "add_media_items_to_collection_with_playback",
            ),
            (
                super::AddMediaItemsToPlaylist::PATH,
                "add_media_items_to_playlist",
            ),
            (
                super::AddMediaItemsToPlaylistWithPlayback::PATH,
                "add_media_items_to_playlist_with_playback",
            ),
            (
                super::AddMediaItemsToPlaylistWithStreamSelection::PATH,
                "add_media_items_to_playlist_with_stream_selection",
            ),
            (
                super::AddMediaItemsToPlaylistMixed::PATH,
                "add_media_items_to_playlist_mixed",
            ),
        ];

        for (actual, function_name) in routes {
            assert_eq!(
                actual,
                pinned_route(function_name),
                "{function_name} must retain the original media-browser module hash",
            );
        }
    }
}
