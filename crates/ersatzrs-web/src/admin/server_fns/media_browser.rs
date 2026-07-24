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
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
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
            let raw_artwork = enrichment
                .poster
                .get(&id)
                .or_else(|| enrichment.thumbnail.get(&id))
                .or_else(|| enrichment.season_poster.get(&id))
                .or_else(|| enrichment.show_poster.get(&id));
            MediaCardRow {
                id,
                title,
                sort_title,
                subtitle,
                artwork_path: raw_artwork
                    .and_then(|path| artwork_url(path, ArtworkUrlKind::Poster)),
                season_number,
                variant: ContentVariantInfo {
                    content_kind: document.content_kind,
                    sub_kind: document.sub_kind,
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
    pub directors: Vec<String>,
    pub writers: Vec<String>,
    pub actors: Vec<ActorCardRow>,
    pub extras: Vec<MediaCardRow>,
    pub alternate_versions: Vec<MediaCardRow>,
    pub audio_tracks: Vec<AudioTrackRow>,
    pub commentary_subtitles: Vec<CommentarySubtitleRow>,
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
    pub directors: Vec<String>,
    pub writers: Vec<String>,
    pub actors: Vec<ActorCardRow>,
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

// ---------------------------------------------------------------------------
// Movies
// ---------------------------------------------------------------------------

/// Paginated movie list. Results ordered by `SortTitle`.
///
/// Note: `ArtworkKind = 0` is `Poster` in ErsatzTV's enum (see
/// `ErsatzTV.Core/Domain/ArtworkKind.cs`); the previous version of this query
/// used `1` (`Thumbnail`) by mistake and so never returned poster art for
/// movies stored with kind `Poster`.
#[server]
pub async fn list_movies(
    search: String,
    page: u32,
    page_size: u32,
    show_variants: bool,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    list_media_cards_from_application(
        &state,
        "movie",
        ersatzrs_core::repositories::AdminSearchKind::Movie,
        &search,
        page,
        page_size,
        show_variants,
    )
    .await
}

/// Movie details for the `/media/movies/:id` page. Ports
/// `ErsatzTV/Pages/Movie.razor` + `Application.Movies/Queries/GetMovieById`.
#[server]
#[allow(clippy::too_many_lines)]
pub async fn get_movie_details(id: i64) -> Result<Option<MovieDetailRow>, ServerFnError> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state
        .application
        .get_movie_detail(id)
        .await?
        .map(|movie| MovieDetailRow {
            id: movie.id,
            title: movie.title,
            year: movie.year,
            plot: movie.plot,
            tagline: movie.tagline,
            content_ratings: movie.content_ratings,
            release_date: movie.release_date,
            poster_url: movie
                .poster_path
                .and_then(|path| artwork_url(&path, ArtworkUrlKind::Poster)),
            fanart_url: movie
                .fanart_path
                .and_then(|path| artwork_url(&path, ArtworkUrlKind::FanArt)),
            file_path: movie.file_path,
            local_path: movie.local_path,
            media_item_state: movie.media_item_state,
            genres: movie.genres,
            tags: movie.tags,
            languages: movie.languages,
            studios: movie.studios,
            directors: movie.directors,
            writers: movie.writers,
            actors: movie
                .actors
                .into_iter()
                .map(|actor| ActorCardRow {
                    id: actor.id,
                    name: actor.name,
                    role: actor.role,
                    thumb_url: actor
                        .artwork_path
                        .and_then(|path| artwork_url(&path, ArtworkUrlKind::Thumbnail)),
                })
                .collect(),
            extras: movie
                .extras
                .into_iter()
                .map(media_card_from_admin)
                .collect(),
            alternate_versions: movie
                .alternate_versions
                .into_iter()
                .map(media_card_from_admin)
                .collect(),
            audio_tracks: movie
                .audio_tracks
                .into_iter()
                .map(|track| AudioTrackRow {
                    media_version_id: track.media_version_id,
                    stream_index: track.stream_index,
                    title: track.title,
                    language: track.language,
                    codec: track.codec,
                    channels: track.channels,
                    is_default: track.is_default,
                    is_commentary: track.is_commentary,
                })
                .collect(),
            commentary_subtitles: movie
                .commentary_subtitles
                .into_iter()
                .map(|subtitle| CommentarySubtitleRow {
                    id: subtitle.id,
                    stream_index: subtitle.stream_index,
                    title: subtitle.title,
                    language: subtitle.language,
                    codec: subtitle.codec,
                })
                .collect(),
        }))
}

#[cfg(all(test, feature = "ssr"))]
#[allow(clippy::too_many_lines)]
pub async fn get_movie_details_with_pool(
    pool: &ersatzrs_infra::db::DbPool,
    id: i64,
) -> Result<Option<MovieDetailRow>, ServerFnError> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use sqlx::Row;

    let Some(row) = sqlx::query(
        r#"SELECT m."Id" AS "Id",
                  COALESCE(mm."Title", '') AS "Title",
                  mm."Year" AS "Year",
                  mm."Plot" AS "Plot",
                  mm."Tagline" AS "Tagline",
                  mm."ContentRating" AS "ContentRating",
                  mm."ReleaseDate" AS "ReleaseDate",
                  COALESCE(mi."State", 0) AS "State",
                  mi."LibraryPathId" AS "LibraryPathId",
                  mm."Id" AS "MetadataId"
           FROM "Movie" m
           JOIN "MovieMetadata" mm ON mm."MovieId" = m."Id"
           LEFT JOIN "MediaItem" mi ON mi."Id" = m."Id"
           WHERE m."Id" = ?
           LIMIT 1"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    else {
        return Ok(None);
    };

    let metadata_id: i64 = row.try_get("MetadataId").unwrap_or(0);

    // Poster (kind 0) + FanArt (kind 3).
    let poster_path: Option<String> = sqlx::query_scalar(
        r#"SELECT "Path" FROM "Artwork"
           WHERE "MovieMetadataId" = ? AND "ArtworkKind" = 0 LIMIT 1"#,
    )
    .bind(metadata_id)
    .fetch_optional(pool)
    .await?
    .and_then(|v: Option<String>| v);
    let fanart_path: Option<String> = sqlx::query_scalar(
        r#"SELECT "Path" FROM "Artwork"
           WHERE "MovieMetadataId" = ? AND "ArtworkKind" = 3 LIMIT 1"#,
    )
    .bind(metadata_id)
    .fetch_optional(pool)
    .await?
    .and_then(|v: Option<String>| v);

    // First media file path. Mirrors ETV's `movie.GetHeadVersion().MediaFiles.Head().Path`.
    let file_path: Option<String> = sqlx::query_scalar(
        r#"SELECT mf."Path"
           FROM "MediaFile" mf
           JOIN "MediaVersion" mv ON mv."Id" = mf."MediaVersionId"
           WHERE mv."MovieId" = ?
           ORDER BY mv."Id" LIMIT 1"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .and_then(|v: Option<String>| v);
    let library_path_id: Option<i64> = row.try_get("LibraryPathId").unwrap_or(None);
    let local_path = match (file_path.as_deref(), library_path_id) {
        (Some(path), Some(library_path_id)) => {
            let replacements =
                load_path_replacements_for_library_path(pool, library_path_id).await?;
            Some(resolve_local_path(path, &replacements))
        }
        (Some(path), None) => Some(path.to_owned()),
        (None, _) => None,
    };

    let content_ratings: Vec<String> = row
        .try_get::<Option<String>, _>("ContentRating")
        .unwrap_or(None)
        .map(|cr| {
            cr.split('/')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();

    let genres = load_strings(pool, r#""Genre""#, "MovieMetadataId", metadata_id).await?;
    let tags = load_strings(pool, r#""Tag""#, "MovieMetadataId", metadata_id).await?;
    let studios = load_strings(pool, r#""Studio""#, "MovieMetadataId", metadata_id).await?;
    let directors = load_strings(pool, r#""Director""#, "MovieMetadataId", metadata_id).await?;
    let writers = load_strings(pool, r#""Writer""#, "MovieMetadataId", metadata_id).await?;
    let languages = load_movie_languages(pool, id).await?;

    // Actors ordered by `Order` then `Id` (see `Movies/Mapper.cs`).
    let actor_rows = sqlx::query(
        r#"SELECT a."Id" AS "Id",
                  COALESCE(a."Name", '') AS "Name",
                  a."Role" AS "Role",
                  art."Path" AS "Thumb"
           FROM "Actor" a
           LEFT JOIN "Artwork" art ON art."Id" = a."ArtworkId"
           WHERE a."MovieMetadataId" = ?
           ORDER BY a."Order", a."Id""#,
    )
    .bind(metadata_id)
    .fetch_all(pool)
    .await?;
    let actors = actor_rows
        .iter()
        .map(|r| ActorCardRow {
            id: r.try_get("Id").unwrap_or(0),
            name: r.try_get("Name").unwrap_or_default(),
            role: r.try_get::<Option<String>, _>("Role").unwrap_or(None),
            thumb_url: r
                .try_get::<Option<String>, _>("Thumb")
                .unwrap_or(None)
                .and_then(|p| artwork_url(&p, ArtworkUrlKind::Thumbnail)),
        })
        .collect();
    let extras = load_extra_cards(pool, id).await?;
    let alternate_versions = load_movie_alternate_version_cards(pool, id).await?;
    let audio_tracks = load_movie_audio_tracks(pool, id).await?;
    let commentary_subtitles = load_movie_commentary_subtitles(pool, id).await?;

    Ok(Some(MovieDetailRow {
        id: row.try_get("Id").unwrap_or(0),
        title: row.try_get("Title").unwrap_or_default(),
        year: row.try_get::<Option<i64>, _>("Year").unwrap_or(None),
        plot: row.try_get::<Option<String>, _>("Plot").unwrap_or(None),
        tagline: row.try_get::<Option<String>, _>("Tagline").unwrap_or(None),
        content_ratings,
        release_date: row
            .try_get::<Option<String>, _>("ReleaseDate")
            .unwrap_or(None),
        poster_url: poster_path.and_then(|p| artwork_url(&p, ArtworkUrlKind::Poster)),
        fanart_url: fanart_path.and_then(|p| artwork_url(&p, ArtworkUrlKind::FanArt)),
        file_path,
        local_path,
        media_item_state: row.try_get("State").unwrap_or(0),
        genres,
        tags,
        languages,
        studios,
        directors,
        writers,
        actors,
        extras,
        alternate_versions,
        audio_tracks,
        commentary_subtitles,
    }))
}

/// TV show details for the `/media/tv/shows/:id` page. Ports
/// `ErsatzTV/Pages/TelevisionSeasonList.razor` +
/// `Application.Television/Queries/GetTelevisionShowById` +
/// `Application.MediaCards/Queries/GetTelevisionSeasonCards`.
#[server]
#[allow(clippy::too_many_lines)]
pub async fn get_show_details(id: i64) -> Result<Option<ShowDetailRow>, ServerFnError> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state
        .application
        .get_show_detail(id)
        .await?
        .map(|show| ShowDetailRow {
            id: show.id,
            library_id: show.library_id,
            media_source_kind: show.media_source_kind,
            title: show.title,
            year: show.year,
            plot: show.plot,
            content_ratings: show.content_ratings,
            poster_url: show
                .poster_path
                .and_then(|path| artwork_url(&path, ArtworkUrlKind::Poster)),
            fanart_url: show
                .fanart_path
                .and_then(|path| artwork_url(&path, ArtworkUrlKind::FanArt)),
            media_item_state: show.media_item_state,
            genres: show.genres,
            tags: show.tags,
            languages: show.languages,
            studios: show.studios,
            networks: show.networks,
            actors: show
                .actors
                .into_iter()
                .map(|actor| ActorCardRow {
                    id: actor.id,
                    name: actor.name,
                    role: actor.role,
                    thumb_url: actor
                        .artwork_path
                        .and_then(|path| artwork_url(&path, ArtworkUrlKind::Thumbnail)),
                })
                .collect(),
            seasons: show
                .seasons
                .into_iter()
                .map(|season| SeasonCardRow {
                    id: season.id,
                    season_number: season.season_number,
                    title: season_name(season.season_number),
                    poster_url: season
                        .poster_path
                        .and_then(|path| artwork_url(&path, ArtworkUrlKind::Poster)),
                })
                .collect(),
            extras: show.extras.into_iter().map(media_card_from_admin).collect(),
        }))
}

#[cfg(all(test, feature = "ssr"))]
#[allow(clippy::too_many_lines)]
async fn get_show_details_legacy_sql(id: i64) -> Result<Option<ShowDetailRow>, ServerFnError> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use crate::host::state::AppState;
    use sqlx::Row;

    // `Tag.PlexNetworkTypeId` — see `ErsatzTV.Core/Domain/Metadata/Tag.cs`.
    const PLEX_NETWORK_TYPE_ID: &str = "319";

    let state = expect_context::<AppState>();

    let Some(row) = sqlx::query(
        r#"SELECT s."Id" AS "Id",
                  COALESCE(lp."LibraryId", 0) AS "LibraryId",
                  CASE
                      WHEN pl."Id" IS NOT NULL THEN 'Plex'
                      WHEN jl."Id" IS NOT NULL THEN 'Jellyfin'
                      WHEN el."Id" IS NOT NULL THEN 'Emby'
                      ELSE 'Local'
                  END AS "MediaSourceKind",
                  COALESCE(sm."Title", '') AS "Title",
                  sm."Year" AS "Year",
                  sm."Plot" AS "Plot",
                  sm."ContentRating" AS "ContentRating",
                  COALESCE(mi."State", 0) AS "State",
                  sm."Id" AS "MetadataId"
           FROM "Show" s
           JOIN "ShowMetadata" sm ON sm."ShowId" = s."Id"
           LEFT JOIN "MediaItem" mi ON mi."Id" = s."Id"
           LEFT JOIN "LibraryPath" lp ON lp."Id" = mi."LibraryPathId"
           LEFT JOIN "PlexLibrary" pl ON pl."Id" = lp."LibraryId"
           LEFT JOIN "JellyfinLibrary" jl ON jl."Id" = lp."LibraryId"
           LEFT JOIN "EmbyLibrary" el ON el."Id" = lp."LibraryId"
           WHERE s."Id" = ?
           LIMIT 1"#,
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    else {
        return Ok(None);
    };

    let metadata_id: i64 = row.try_get("MetadataId").unwrap_or(0);

    let poster_path: Option<String> = sqlx::query_scalar(
        r#"SELECT "Path" FROM "Artwork"
           WHERE "ShowMetadataId" = ? AND "ArtworkKind" = 0 LIMIT 1"#,
    )
    .bind(metadata_id)
    .fetch_optional(&state.pool)
    .await?
    .and_then(|v: Option<String>| v);
    let fanart_path: Option<String> = sqlx::query_scalar(
        r#"SELECT "Path" FROM "Artwork"
           WHERE "ShowMetadataId" = ? AND "ArtworkKind" = 3 LIMIT 1"#,
    )
    .bind(metadata_id)
    .fetch_optional(&state.pool)
    .await?
    .and_then(|v: Option<String>| v);

    let genres = load_strings(&state.pool, r#""Genre""#, "ShowMetadataId", metadata_id).await?;
    let studios = load_strings(&state.pool, r#""Studio""#, "ShowMetadataId", metadata_id).await?;
    let languages = load_show_languages(&state.pool, id).await?;

    // Tags split into plain tags vs Plex "network" tags, mirroring
    // `Television/Mapper.cs` (`ExternalTypeId` is null/blank for tags,
    // `Tag.PlexNetworkTypeId` for networks).
    let tag_rows = sqlx::query(
        r#"SELECT COALESCE("Name", '') AS "Name",
                  COALESCE("ExternalTypeId", '') AS "ExternalTypeId"
           FROM "Tag" WHERE "ShowMetadataId" = ? ORDER BY "Name""#,
    )
    .bind(metadata_id)
    .fetch_all(&state.pool)
    .await?;
    let mut tags: Vec<String> = Vec::new();
    let mut networks: Vec<String> = Vec::new();
    for r in &tag_rows {
        let name: String = r.try_get("Name").unwrap_or_default();
        if name.trim().is_empty() {
            continue;
        }
        let external: String = r.try_get("ExternalTypeId").unwrap_or_default();
        if external.trim().is_empty() {
            tags.push(name);
        } else if external == PLEX_NETWORK_TYPE_ID {
            networks.push(name);
        }
    }

    // Content rating is a single "/"-separated field in ErsatzTV.
    let content_ratings: Vec<String> = row
        .try_get::<Option<String>, _>("ContentRating")
        .unwrap_or(None)
        .map(|cr| {
            cr.split('/')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();

    // Actors ordered by `Order` then `Id` (see `Television/Mapper.cs`).
    let actor_rows = sqlx::query(
        r#"SELECT a."Id" AS "Id",
                  COALESCE(a."Name", '') AS "Name",
                  a."Role" AS "Role",
                  art."Path" AS "Thumb"
           FROM "Actor" a
           LEFT JOIN "Artwork" art ON art."Id" = a."ArtworkId"
           WHERE a."ShowMetadataId" = ?
           ORDER BY a."Order", a."Id""#,
    )
    .bind(metadata_id)
    .fetch_all(&state.pool)
    .await?;
    let actors = actor_rows
        .iter()
        .map(|r| ActorCardRow {
            id: r.try_get("Id").unwrap_or(0),
            name: r.try_get("Name").unwrap_or_default(),
            role: r.try_get::<Option<String>, _>("Role").unwrap_or(None),
            thumb_url: r
                .try_get::<Option<String>, _>("Thumb")
                .unwrap_or(None)
                .and_then(|p| artwork_url(&p, ArtworkUrlKind::Thumbnail)),
        })
        .collect();

    // Seasons ordered by season number (see `MediaCards/Mapper.cs`).
    let season_rows = sqlx::query(
        r#"SELECT se."Id" AS "Id",
                  se."SeasonNumber" AS "SeasonNumber",
                  a."Path" AS "Poster"
           FROM "Season" se
           JOIN "SeasonMetadata" sem ON sem."SeasonId" = se."Id"
           LEFT JOIN "Artwork" a
               ON a."SeasonMetadataId" = sem."Id" AND a."ArtworkKind" = 0
           WHERE se."ShowId" = ?
           ORDER BY se."SeasonNumber""#,
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;
    let seasons = season_rows
        .iter()
        .map(|r| {
            let season_number: i64 = r.try_get("SeasonNumber").unwrap_or(0);
            SeasonCardRow {
                id: r.try_get("Id").unwrap_or(0),
                season_number,
                title: if season_number == 0 {
                    "Specials".to_string()
                } else {
                    format!("Season {season_number}")
                },
                poster_url: r
                    .try_get::<Option<String>, _>("Poster")
                    .unwrap_or(None)
                    .and_then(|p| artwork_url(&p, ArtworkUrlKind::Poster)),
            }
        })
        .collect();
    let extras = load_extra_cards(&state.pool, id).await?;

    Ok(Some(ShowDetailRow {
        id: row.try_get("Id").unwrap_or(0),
        library_id: row.try_get("LibraryId").unwrap_or(0),
        media_source_kind: row.try_get("MediaSourceKind").unwrap_or_default(),
        title: row.try_get("Title").unwrap_or_default(),
        year: row.try_get::<Option<i64>, _>("Year").unwrap_or(None),
        plot: row.try_get::<Option<String>, _>("Plot").unwrap_or(None),
        content_ratings,
        poster_url: poster_path.and_then(|p| artwork_url(&p, ArtworkUrlKind::Poster)),
        fanart_url: fanart_path.and_then(|p| artwork_url(&p, ArtworkUrlKind::FanArt)),
        media_item_state: row.try_get("State").unwrap_or(0),
        genres,
        tags,
        languages,
        studios,
        networks,
        actors,
        seasons,
        extras,
    }))
}

/// TV season details for the `/media/tv/seasons/:id` page. Ports
/// `ErsatzTV/Pages/TelevisionEpisodeList.razor` +
/// `Application.Television/Queries/GetTelevisionSeasonById` +
/// `Application.MediaCards/Queries/GetTelevisionEpisodeCards`.
#[server]
pub async fn get_season_details(id: i64) -> Result<Option<SeasonDetailRow>, ServerFnError> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let Some(detail) = state.application.get_season_detail(id).await? else {
        return Ok(None);
    };
    let season = detail.season;

    let season_number = i64::from(season.season_number);
    Ok(Some(SeasonDetailRow {
        id: season.id,
        show_id: season.show_id,
        show_title: season.show_title,
        show_year: season.show_year,
        name: season_name(season_number),
        season_number,
        poster_url: season
            .season_poster_path
            .and_then(|p| artwork_url(&p, ArtworkUrlKind::Poster)),
        fanart_url: season
            .show_fanart_path
            .and_then(|p| artwork_url(&p, ArtworkUrlKind::FanArt)),
        episode_count: detail.episode_count,
        episodes: detail
            .episodes
            .into_iter()
            .map(|episode| EpisodeCardRow {
                episode_id: episode.episode_id,
                aired: short_date(episode.release_date.as_deref()),
                show_title: episode.show_title,
                show_id: episode.show_id,
                season_id: episode.season_id,
                season_number: i64::from(episode.season_number),
                episode_number: i64::from(episode.episode_number),
                title: episode.title,
                sort_title: episode.sort_title,
                plot: episode.plot,
                thumbnail_url: episode
                    .thumbnail_path
                    .and_then(|p| artwork_url(&p, ArtworkUrlKind::Thumbnail)),
                directors: episode.directors,
                writers: episode.writers,
                media_item_state: episode.state,
                file_path: episode.file_path,
                local_path: episode.local_path,
            })
            .collect(),
        extras: detail
            .extras
            .into_iter()
            .map(media_card_from_admin)
            .collect(),
    }))
}

/// Media-info JSON for `MediaItemInfoDialog`. Ports the movie branch of
/// `ErsatzTV.Application.MediaItems.Queries.GetMediaItemInfoHandler` and the
/// serialization options used by `Shared/MediaItemInfoDialog.razor`
/// (`JsonStringEnumConverter`, null fields omitted, indented JSON).
#[server]
#[allow(clippy::too_many_lines)]
pub async fn get_media_item_info_json(id: i64) -> Result<String, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let Some(info) = state.application.get_media_item_info(id).await? else {
        return Err(ServerFnError::new("Unable to locate media item"));
    };
    media_item_info_json_from_admin(info)
}

#[cfg(feature = "ssr")]
pub fn media_item_info_json_from_admin(
    info: ersatzrs_core::repositories::AdminMediaItemInfo,
) -> Result<String, ServerFnError> {
    use ersatzrs_core::repositories::AdminMediaInfoKind;

    let kind = match info.kind {
        AdminMediaInfoKind::Movie => "Movie",
        AdminMediaInfoKind::OtherVideo => "OtherVideo",
        AdminMediaInfoKind::Episode => "Episode",
    };
    let json = MediaItemInfoJson {
        id: info.id,
        title: info.title,
        kind: kind.to_owned(),
        library_kind: info.library_kind,
        server_name: info.server_name,
        library_name: info.library_name,
        state: media_item_state_name(i64::from(info.state)).to_owned(),
        duration: info.duration,
        sample_aspect_ratio: info.sample_aspect_ratio,
        display_aspect_ratio: info.display_aspect_ratio,
        r_frame_rate: info.r_frame_rate,
        video_scan_kind: video_scan_kind_name(i64::from(info.video_scan_kind)).to_owned(),
        interlaced_ratio: info.interlaced_ratio,
        width: info.width,
        height: info.height,
        streams: info
            .streams
            .into_iter()
            .map(|stream| MediaItemInfoStreamJson {
                index: stream.index,
                kind: if stream.external_subtitle {
                    "ExternalSubtitle".to_owned()
                } else {
                    media_stream_kind_name(i64::from(stream.kind)).to_owned()
                },
                title: stream.title,
                codec: stream.codec,
                profile: stream.profile,
                language: stream.language,
                channels: stream.channels,
                default: stream.is_default,
                forced: stream.is_forced,
                attached_pic: stream.attached_pic,
                pixel_format: stream.pixel_format,
                color_range: stream.color_range,
                color_space: stream.color_space,
                color_transfer: stream.color_transfer,
                color_primaries: stream.color_primaries,
                bits_per_raw_sample: stream.bits_per_raw_sample,
                mime_type: stream.mime_type,
                file_name: stream.file_name,
                is_extracted: stream.is_extracted,
            })
            .collect(),
        chapters: info
            .chapters
            .into_iter()
            .map(|chapter| MediaItemInfoChapterJson {
                title: chapter.title,
                start_time: chapter.start_time,
                end_time: chapter.end_time,
            })
            .collect(),
    };
    serde_json::to_string_pretty(&json).map_err(ServerFnError::new)
}

#[cfg(all(test, feature = "ssr"))]
#[allow(clippy::too_many_lines)]
pub async fn get_media_item_info_json_with_pool(
    pool: &ersatzrs_infra::db::DbPool,
    id: i64,
) -> Result<String, ServerFnError> {
    use ersatzrs_core::repositories::TelevisionRepository;
    use ersatzrs_infra::repositories::SqliteTelevisionRepository;
    use sqlx::Row;

    let movie_row = sqlx::query(
        r#"SELECT m."Id" AS "Id",
                  COALESCE(mm."Title", '') AS "Title",
                  mm."Id" AS "MetadataId",
                  COALESCE(mi."State", 0) AS "State",
                  COALESCE(l."Name", '') AS "LibraryName",
                  CASE
                      WHEN ll."Id" IS NOT NULL THEN 'LocalLibrary'
                      WHEN pl."Id" IS NOT NULL THEN 'PlexLibrary'
                      WHEN jl."Id" IS NOT NULL THEN 'JellyfinLibrary'
                      WHEN el."Id" IS NOT NULL THEN 'EmbyLibrary'
                      ELSE 'Library'
                  END AS "LibraryKind",
                  COALESCE(pms."ServerName", jms."ServerName", ems."ServerName") AS "ServerName",
                  mv."Id" AS "MediaVersionId",
                  mv."Duration" AS "Duration",
                  mv."SampleAspectRatio" AS "SampleAspectRatio",
                  mv."DisplayAspectRatio" AS "DisplayAspectRatio",
                  mv."RFrameRate" AS "RFrameRate",
                  COALESCE(mv."VideoScanKind", 0) AS "VideoScanKind",
                  mv."InterlacedRatio" AS "InterlacedRatio",
                  COALESCE(mv."Width", 0) AS "Width",
                  COALESCE(mv."Height", 0) AS "Height"
           FROM "Movie" m
           JOIN "MovieMetadata" mm ON mm."MovieId" = m."Id"
           LEFT JOIN "MediaItem" mi ON mi."Id" = m."Id"
           LEFT JOIN "LibraryPath" lp ON lp."Id" = mi."LibraryPathId"
           LEFT JOIN "Library" l ON l."Id" = lp."LibraryId"
           LEFT JOIN "LocalLibrary" ll ON ll."Id" = l."Id"
           LEFT JOIN "PlexLibrary" pl ON pl."Id" = l."Id"
           LEFT JOIN "JellyfinLibrary" jl ON jl."Id" = l."Id"
           LEFT JOIN "EmbyLibrary" el ON el."Id" = l."Id"
           LEFT JOIN "MediaSource" ms ON ms."Id" = l."MediaSourceId"
           LEFT JOIN "PlexMediaSource" pms ON pms."Id" = ms."Id"
           LEFT JOIN "JellyfinMediaSource" jms ON jms."Id" = ms."Id"
           LEFT JOIN "EmbyMediaSource" ems ON ems."Id" = ms."Id"
           LEFT JOIN "MediaVersion" mv ON mv."MovieId" = m."Id"
           WHERE mi."Id" = ?
           ORDER BY mv."Id"
           LIMIT 1"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    if let Some(row) = movie_row {
        let metadata_id: i64 = row.try_get("MetadataId").unwrap_or(0);
        let version_id: Option<i64> = row.try_get("MediaVersionId").unwrap_or(None);
        let is_local_library =
            row.try_get::<String, _>("LibraryKind").unwrap_or_default() == "LocalLibrary";

        let subtitles = load_movie_subtitle_info(pool, metadata_id).await?;
        let streams = if let Some(version_id) = version_id {
            load_media_info_streams(pool, version_id, &subtitles, is_local_library).await?
        } else {
            Vec::new()
        };
        let chapters = if let Some(version_id) = version_id {
            load_media_info_chapters(pool, version_id).await?
        } else {
            Vec::new()
        };

        let info = MediaItemInfoJson {
            id: row.try_get("Id").unwrap_or(0),
            title: row.try_get("Title").unwrap_or_default(),
            kind: "Movie".to_string(),
            library_kind: row.try_get("LibraryKind").unwrap_or_default(),
            server_name: row
                .try_get::<Option<String>, _>("ServerName")
                .unwrap_or(None),
            library_name: row.try_get("LibraryName").unwrap_or_default(),
            state: media_item_state_name(row.try_get("State").unwrap_or(0)).to_string(),
            duration: row
                .try_get::<Option<String>, _>("Duration")
                .unwrap_or(None)
                .unwrap_or_else(|| "00:00:00".to_string()),
            sample_aspect_ratio: row
                .try_get::<Option<String>, _>("SampleAspectRatio")
                .unwrap_or(None),
            display_aspect_ratio: row
                .try_get::<Option<String>, _>("DisplayAspectRatio")
                .unwrap_or(None),
            r_frame_rate: row
                .try_get::<Option<String>, _>("RFrameRate")
                .unwrap_or(None),
            video_scan_kind: video_scan_kind_name(row.try_get("VideoScanKind").unwrap_or(0))
                .to_string(),
            interlaced_ratio: row
                .try_get::<Option<f64>, _>("InterlacedRatio")
                .unwrap_or(None),
            width: row.try_get("Width").unwrap_or(0),
            height: row.try_get("Height").unwrap_or(0),
            streams,
            chapters,
        };

        return serde_json::to_string_pretty(&info).map_err(ServerFnError::new);
    }

    let other_video_row = sqlx::query(
        r#"SELECT ov."Id" AS "Id",
                  COALESCE(ovm."Title", '') AS "Title",
                  ovm."Id" AS "MetadataId",
                  COALESCE(mi."State", 0) AS "State",
                  COALESCE(l."Name", '') AS "LibraryName",
                  CASE
                      WHEN ll."Id" IS NOT NULL THEN 'LocalLibrary'
                      WHEN pl."Id" IS NOT NULL THEN 'PlexLibrary'
                      WHEN jl."Id" IS NOT NULL THEN 'JellyfinLibrary'
                      WHEN el."Id" IS NOT NULL THEN 'EmbyLibrary'
                      ELSE 'Library'
                  END AS "LibraryKind",
                  COALESCE(pms."ServerName", jms."ServerName", ems."ServerName") AS "ServerName",
                  mv."Id" AS "MediaVersionId",
                  mv."Duration" AS "Duration",
                  mv."SampleAspectRatio" AS "SampleAspectRatio",
                  mv."DisplayAspectRatio" AS "DisplayAspectRatio",
                  mv."RFrameRate" AS "RFrameRate",
                  COALESCE(mv."VideoScanKind", 0) AS "VideoScanKind",
                  mv."InterlacedRatio" AS "InterlacedRatio",
                  COALESCE(mv."Width", 0) AS "Width",
                  COALESCE(mv."Height", 0) AS "Height"
           FROM "OtherVideo" ov
           JOIN "OtherVideoMetadata" ovm ON ovm."OtherVideoId" = ov."Id"
           LEFT JOIN "MediaItem" mi ON mi."Id" = ov."Id"
           LEFT JOIN "LibraryPath" lp ON lp."Id" = mi."LibraryPathId"
           LEFT JOIN "Library" l ON l."Id" = lp."LibraryId"
           LEFT JOIN "LocalLibrary" ll ON ll."Id" = l."Id"
           LEFT JOIN "PlexLibrary" pl ON pl."Id" = l."Id"
           LEFT JOIN "JellyfinLibrary" jl ON jl."Id" = l."Id"
           LEFT JOIN "EmbyLibrary" el ON el."Id" = l."Id"
           LEFT JOIN "MediaSource" ms ON ms."Id" = l."MediaSourceId"
           LEFT JOIN "PlexMediaSource" pms ON pms."Id" = ms."Id"
           LEFT JOIN "JellyfinMediaSource" jms ON jms."Id" = ms."Id"
           LEFT JOIN "EmbyMediaSource" ems ON ems."Id" = ms."Id"
           LEFT JOIN "MediaVersion" mv ON mv."OtherVideoId" = ov."Id"
           WHERE mi."Id" = ?
           ORDER BY mv."Id"
           LIMIT 1"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    if let Some(row) = other_video_row {
        let metadata_id: i64 = row.try_get("MetadataId").unwrap_or(0);
        let version_id: Option<i64> = row.try_get("MediaVersionId").unwrap_or(None);
        let is_local_library =
            row.try_get::<String, _>("LibraryKind").unwrap_or_default() == "LocalLibrary";

        let subtitles = load_other_video_subtitle_info(pool, metadata_id).await?;
        let streams = if let Some(version_id) = version_id {
            load_media_info_streams(pool, version_id, &subtitles, is_local_library).await?
        } else {
            Vec::new()
        };
        let chapters = if let Some(version_id) = version_id {
            load_media_info_chapters(pool, version_id).await?
        } else {
            Vec::new()
        };

        let info = MediaItemInfoJson {
            id: row.try_get("Id").unwrap_or(0),
            title: row.try_get("Title").unwrap_or_default(),
            kind: "OtherVideo".to_string(),
            library_kind: row.try_get("LibraryKind").unwrap_or_default(),
            server_name: row
                .try_get::<Option<String>, _>("ServerName")
                .unwrap_or(None),
            library_name: row.try_get("LibraryName").unwrap_or_default(),
            state: media_item_state_name(row.try_get("State").unwrap_or(0)).to_string(),
            duration: row
                .try_get::<Option<String>, _>("Duration")
                .unwrap_or(None)
                .unwrap_or_else(|| "00:00:00".to_string()),
            sample_aspect_ratio: row
                .try_get::<Option<String>, _>("SampleAspectRatio")
                .unwrap_or(None),
            display_aspect_ratio: row
                .try_get::<Option<String>, _>("DisplayAspectRatio")
                .unwrap_or(None),
            r_frame_rate: row
                .try_get::<Option<String>, _>("RFrameRate")
                .unwrap_or(None),
            video_scan_kind: video_scan_kind_name(row.try_get("VideoScanKind").unwrap_or(0))
                .to_string(),
            interlaced_ratio: row
                .try_get::<Option<f64>, _>("InterlacedRatio")
                .unwrap_or(None),
            width: row.try_get("Width").unwrap_or(0),
            height: row.try_get("Height").unwrap_or(0),
            streams,
            chapters,
        };

        return serde_json::to_string_pretty(&info).map_err(ServerFnError::new);
    }

    let repo = SqliteTelevisionRepository(pool.clone());
    if let Some(episode) = repo.get_episode_media_item_info(id).await? {
        let subtitles = load_episode_subtitle_info(pool, episode.metadata_id).await?;
        let streams = if let Some(version_id) = episode.media_version_id {
            load_media_info_streams(pool, version_id, &subtitles, episode.is_local_library).await?
        } else {
            Vec::new()
        };
        let chapters = if let Some(version_id) = episode.media_version_id {
            load_media_info_chapters(pool, version_id).await?
        } else {
            Vec::new()
        };
        let title = format!(
            "{} - s{:02}e{:02} - {}",
            episode.show_title, episode.season_number, episode.episode_number, episode.title
        );
        let info = MediaItemInfoJson {
            id: episode.id,
            title,
            kind: "Episode".to_string(),
            library_kind: episode.library_kind,
            server_name: episode.server_name,
            library_name: episode.library_name,
            state: media_item_state_name(i64::from(episode.state)).to_string(),
            duration: episode.duration.unwrap_or_else(|| "00:00:00".to_string()),
            sample_aspect_ratio: episode.sample_aspect_ratio,
            display_aspect_ratio: episode.display_aspect_ratio,
            r_frame_rate: episode.r_frame_rate,
            video_scan_kind: video_scan_kind_name(i64::from(episode.video_scan_kind)).to_string(),
            interlaced_ratio: episode.interlaced_ratio,
            width: episode.width,
            height: episode.height,
            streams,
            chapters,
        };

        return serde_json::to_string_pretty(&info).map_err(ServerFnError::new);
    }

    Err(ServerFnError::new("Unable to locate media item"))
}

#[cfg(all(test, feature = "ssr"))]
async fn load_show_languages(
    pool: &ersatzrs_infra::db::DbPool,
    show_id: i64,
) -> Result<Vec<String>, ServerFnError> {
    // Mirrors `ErsatzTV.Infrastructure/Data/Repositories/SearchRepository.cs`
    // `GetLanguagesForShow`; `MediaStreamKind = 2` is the ErsatzTV persisted
    // enum value for audio streams.
    let codes: Vec<Option<String>> = sqlx::query_scalar(
        r#"SELECT DISTINCT ms."Language"
           FROM "MediaStream" ms
           JOIN "MediaVersion" mv ON mv."Id" = ms."MediaVersionId"
           JOIN "Episode" e ON e."Id" = mv."EpisodeId"
           JOIN "Season" s ON s."Id" = e."SeasonId"
           WHERE ms."MediaStreamKind" = 2 AND s."ShowId" = ?"#,
    )
    .bind(show_id)
    .fetch_all(pool)
    .await?;

    let media_codes = codes
        .into_iter()
        .flatten()
        .filter(|code| !code.trim().is_empty());

    let mut languages: Vec<String> = ersatzrs_core::language_codes::all_language_codes(media_codes)
        .into_iter()
        .filter_map(|code| ersatzrs_core::language_codes::english_name(&code))
        .filter(|name| !name.trim().is_empty())
        .collect();
    languages.sort();
    languages.dedup();
    Ok(languages)
}

#[cfg(all(test, feature = "ssr"))]
async fn load_movie_languages(
    pool: &ersatzrs_infra::db::DbPool,
    movie_id: i64,
) -> Result<Vec<String>, ServerFnError> {
    // Mirrors `GetMovieByIdHandler.cs`: inspect the movie head version's
    // audio streams, then expand any ISO-639 aliases to English names.
    let codes: Vec<Option<String>> = sqlx::query_scalar(
        r#"SELECT ms."Language"
           FROM "MediaStream" ms
           JOIN "MediaVersion" mv ON mv."Id" = ms."MediaVersionId"
           WHERE ms."MediaStreamKind" = 2
             AND mv."MovieId" = ?
             AND mv."Id" = (
                 SELECT "Id"
                 FROM "MediaVersion"
                 WHERE "MovieId" = ?
                 ORDER BY "Id"
                 LIMIT 1
             )
           ORDER BY ms."Index", ms."Id""#,
    )
    .bind(movie_id)
    .bind(movie_id)
    .fetch_all(pool)
    .await?;

    let media_codes = codes
        .into_iter()
        .flatten()
        .filter(|code| !code.trim().is_empty());

    let mut languages: Vec<String> = ersatzrs_core::language_codes::all_language_codes(media_codes)
        .into_iter()
        .filter_map(|code| ersatzrs_core::language_codes::english_name(&code))
        .filter(|name| !name.trim().is_empty())
        .collect();
    languages.sort();
    languages.dedup();
    Ok(languages)
}

#[cfg(all(test, feature = "ssr"))]
async fn load_other_video_languages(
    pool: &ersatzrs_infra::db::DbPool,
    other_video_id: i64,
) -> Result<Vec<String>, ServerFnError> {
    let codes: Vec<Option<String>> = sqlx::query_scalar(
        r#"SELECT ms."Language"
           FROM "MediaStream" ms
           JOIN "MediaVersion" mv ON mv."Id" = ms."MediaVersionId"
           WHERE ms."MediaStreamKind" = 2
             AND mv."OtherVideoId" = ?
             AND mv."Id" = (
                 SELECT "Id"
                 FROM "MediaVersion"
                 WHERE "OtherVideoId" = ?
                 ORDER BY "Id"
                 LIMIT 1
             )
           ORDER BY ms."Index", ms."Id""#,
    )
    .bind(other_video_id)
    .bind(other_video_id)
    .fetch_all(pool)
    .await?;

    let media_codes = codes
        .into_iter()
        .flatten()
        .filter(|code| !code.trim().is_empty());

    let mut languages: Vec<String> = ersatzrs_core::language_codes::all_language_codes(media_codes)
        .into_iter()
        .filter_map(|code| ersatzrs_core::language_codes::english_name(&code))
        .filter(|name| !name.trim().is_empty())
        .collect();
    languages.sort();
    languages.dedup();
    Ok(languages)
}

#[cfg(all(test, feature = "ssr"))]
async fn load_artist_languages(
    pool: &ersatzrs_infra::db::DbPool,
    artist_id: i64,
) -> Result<Vec<String>, ServerFnError> {
    // Mirrors `SearchRepository.GetLanguagesForArtist`; `MediaStreamKind = 2`
    // is the ErsatzTV persisted enum value for audio streams.
    let codes: Vec<Option<String>> = sqlx::query_scalar(
        r#"SELECT DISTINCT ms."Language"
           FROM "MediaStream" ms
           JOIN "MediaVersion" mv ON mv."Id" = ms."MediaVersionId"
           JOIN "MusicVideo" mv_item ON mv_item."Id" = mv."MusicVideoId"
           WHERE ms."MediaStreamKind" = 2
             AND mv_item."ArtistId" = ?"#,
    )
    .bind(artist_id)
    .fetch_all(pool)
    .await?;

    let media_codes = codes
        .into_iter()
        .flatten()
        .filter(|code| !code.trim().is_empty());

    let mut languages: Vec<String> = ersatzrs_core::language_codes::all_language_codes(media_codes)
        .into_iter()
        .filter_map(|code| ersatzrs_core::language_codes::english_name(&code))
        .filter(|name| !name.trim().is_empty())
        .collect();
    languages.sort();
    languages.dedup();
    Ok(languages)
}

#[cfg(all(test, feature = "ssr"))]
async fn load_path_replacements_for_library_path(
    pool: &ersatzrs_infra::db::DbPool,
    library_path_id: i64,
) -> Result<Vec<ersatzrs_core::path_replacement::PathReplacement>, ServerFnError> {
    use ersatzrs_core::path_replacement::PathReplacement;
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT ppr."PlexPath" AS "Source",
                  ppr."LocalPath" AS "Local",
                  pms."Platform" AS "Platform",
                  ppr."Id" AS "SortId"
           FROM "LibraryPath" lp
           JOIN "Library" l ON l."Id" = lp."LibraryId"
           JOIN "PlexMediaSource" pms ON pms."Id" = l."MediaSourceId"
           JOIN "PlexPathReplacement" ppr ON ppr."PlexMediaSourceId" = pms."Id"
           WHERE lp."Id" = ?
           UNION ALL
           SELECT jpr."JellyfinPath" AS "Source",
                  jpr."LocalPath" AS "Local",
                  jms."OperatingSystem" AS "Platform",
                  jpr."Id" AS "SortId"
           FROM "LibraryPath" lp
           JOIN "Library" l ON l."Id" = lp."LibraryId"
           JOIN "JellyfinMediaSource" jms ON jms."Id" = l."MediaSourceId"
           JOIN "JellyfinPathReplacement" jpr ON jpr."JellyfinMediaSourceId" = jms."Id"
           WHERE lp."Id" = ?
           UNION ALL
           SELECT epr."EmbyPath" AS "Source",
                  epr."LocalPath" AS "Local",
                  ems."OperatingSystem" AS "Platform",
                  epr."Id" AS "SortId"
           FROM "LibraryPath" lp
           JOIN "Library" l ON l."Id" = lp."LibraryId"
           JOIN "EmbyMediaSource" ems ON ems."Id" = l."MediaSourceId"
           JOIN "EmbyPathReplacement" epr ON epr."EmbyMediaSourceId" = ems."Id"
           WHERE lp."Id" = ?
           ORDER BY "SortId""#,
    )
    .bind(library_path_id)
    .bind(library_path_id)
    .bind(library_path_id)
    .fetch_all(pool)
    .await?;

    let mut replacements = Vec::new();
    for row in rows {
        let source_path: Option<String> = row.try_get("Source")?;
        let replacement_path: Option<String> = row.try_get("Local")?;
        let platform: Option<String> = row.try_get("Platform")?;
        if let (Some(source_path), Some(replacement_path)) = (source_path, replacement_path) {
            replacements.push(PathReplacement {
                source_platform: source_platform(&source_path, platform.as_deref()),
                source_path,
                replacement_path,
            });
        }
    }

    Ok(replacements)
}

#[cfg(all(test, feature = "ssr"))]
fn resolve_local_path(
    path: &str,
    replacements: &[ersatzrs_core::path_replacement::PathReplacement],
) -> String {
    if replacements.is_empty() {
        path.to_owned()
    } else {
        ersatzrs_core::path_replacement::replace_path(replacements, path, target_platform())
    }
}

#[cfg(all(test, feature = "ssr"))]
fn source_platform(
    source_path: &str,
    platform: Option<&str>,
) -> ersatzrs_core::path_replacement::PathPlatform {
    use ersatzrs_core::path_replacement::PathPlatform;

    let source_looks_windows = source_path.contains('\\') || source_path.contains(':');
    let source_platform_is_windows = platform.is_some_and(|platform| {
        platform
            .trim_start()
            .to_ascii_lowercase()
            .starts_with("windows")
    });
    if source_looks_windows || source_platform_is_windows {
        PathPlatform::Windows
    } else {
        PathPlatform::Unix
    }
}

#[cfg(all(test, feature = "ssr", target_os = "windows"))]
fn target_platform() -> ersatzrs_core::path_replacement::PathPlatform {
    ersatzrs_core::path_replacement::PathPlatform::Windows
}

#[cfg(all(test, feature = "ssr", not(target_os = "windows")))]
fn target_platform() -> ersatzrs_core::path_replacement::PathPlatform {
    ersatzrs_core::path_replacement::PathPlatform::Unix
}

#[cfg(all(test, feature = "ssr"))]
async fn enqueue_schedule_playout_refresh(
    state: &crate::host::state::AppState,
    schedule_id: i64,
) -> Result<(), ServerFnError> {
    let playout_ids = playout_ids_using_schedule(&state.pool, schedule_id).await?;
    enqueue_playout_refreshes(state, playout_ids);
    Ok(())
}

#[cfg(all(test, feature = "ssr"))]
async fn playout_ids_using_schedule(
    pool: &ersatzrs_infra::db::DbPool,
    schedule_id: i64,
) -> Result<Vec<i64>, sqlx::Error> {
    sqlx::query_scalar(
        r#"SELECT "Id"
           FROM "Playout"
           WHERE "ProgramScheduleId" = ?
           ORDER BY "Id""#,
    )
    .bind(schedule_id)
    .fetch_all(pool)
    .await
}

#[cfg(all(test, feature = "ssr"))]
async fn playout_ids_using_collection(
    pool: &ersatzrs_infra::db::DbPool,
    collection_id: i64,
) -> Result<Vec<i64>, sqlx::Error> {
    // Mirrors `ErsatzTV.Infrastructure/Data/Repositories/
    // MediaCollectionRepository.cs` `PlayoutIdsUsingCollection`.
    sqlx::query_scalar(
        r#"SELECT DISTINCT "PlayoutId"
           FROM "PlayoutProgramScheduleAnchor"
           WHERE "CollectionId" = ?
           ORDER BY "PlayoutId""#,
    )
    .bind(collection_id)
    .fetch_all(pool)
    .await
}

#[cfg(all(test, feature = "ssr"))]
fn enqueue_playout_refreshes(state: &crate::host::state::AppState, playout_ids: Vec<i64>) {
    use ersatzrs_app::commands::{RequestPlayoutBuild, RequestPlayoutBuildOutcome};
    use ersatzrs_core::scheduling::PlayoutBuildMode;

    for playout_id in playout_ids {
        match state
            .application
            .request_playout_build(RequestPlayoutBuild::new(
                playout_id,
                PlayoutBuildMode::Refresh,
            )) {
            RequestPlayoutBuildOutcome::Accepted
            | RequestPlayoutBuildOutcome::AlreadyQueued
            | RequestPlayoutBuildOutcome::AlreadyBuilding => {}
            RequestPlayoutBuildOutcome::Unavailable { message } => {
                tracing::warn!(
                    playout_id,
                    %message,
                    "unable to enqueue requested playout refresh; worker queue is unavailable"
                );
            }
            RequestPlayoutBuildOutcome::Rejected(reason) => {
                tracing::warn!(
                    playout_id,
                    ?reason,
                    "unable to enqueue invalid playout refresh request"
                );
            }
        }
    }
}

#[cfg(all(test, feature = "ssr"))]
async fn load_strings(
    pool: &ersatzrs_infra::db::DbPool,
    table: &str,
    column: &str,
    metadata_id: i64,
) -> Result<Vec<String>, ServerFnError> {
    use sqlx::Row;

    // `table` and `column` are private constants supplied by the call sites
    // in this module; not user input. Identifiers are inlined because the
    // sqlx query cache uses the rendered SQL as the key.
    let sql = format!(r#"SELECT "Name" FROM {table} WHERE "{column}" = ? ORDER BY "Name""#);
    let rows = sqlx::query(ersatzrs_infra::db::assert_sql_safe(&sql))
        .bind(metadata_id)
        .fetch_all(pool)
        .await?;
    Ok(rows
        .iter()
        .filter_map(|r| r.try_get::<Option<String>, _>("Name").ok().flatten())
        .filter(|name| !name.trim().is_empty())
        .collect())
}

#[cfg(all(test, feature = "ssr"))]
async fn load_movie_subtitle_info(
    pool: &ersatzrs_infra::db::DbPool,
    metadata_id: i64,
) -> Result<Vec<SubtitleInfoRow>, ServerFnError> {
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT "StreamIndex",
                  "SubtitleKind",
                  "Title",
                  "Codec",
                  "Language",
                  "Default",
                  "Forced",
                  "Path",
                  "IsExtracted"
           FROM "Subtitle"
           WHERE "MovieMetadataId" = ?
           ORDER BY "StreamIndex", "Id""#,
    )
    .bind(metadata_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| SubtitleInfoRow {
            stream_index: r.try_get("StreamIndex").unwrap_or(0),
            subtitle_kind: r.try_get("SubtitleKind").unwrap_or(0),
            title: r.try_get::<Option<String>, _>("Title").unwrap_or(None),
            codec: r.try_get::<Option<String>, _>("Codec").unwrap_or(None),
            language: r.try_get::<Option<String>, _>("Language").unwrap_or(None),
            default: r.try_get::<i64, _>("Default").unwrap_or(0) != 0,
            forced: r.try_get::<i64, _>("Forced").unwrap_or(0) != 0,
            path: r.try_get::<Option<String>, _>("Path").unwrap_or(None),
            is_extracted: r.try_get::<i64, _>("IsExtracted").unwrap_or(0) != 0,
        })
        .collect())
}

#[cfg(all(test, feature = "ssr"))]
async fn load_other_video_subtitle_info(
    pool: &ersatzrs_infra::db::DbPool,
    metadata_id: i64,
) -> Result<Vec<SubtitleInfoRow>, ServerFnError> {
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT "StreamIndex",
                  "SubtitleKind",
                  "Title",
                  "Codec",
                  "Language",
                  "Default",
                  "Forced",
                  "Path",
                  "IsExtracted"
           FROM "Subtitle"
           WHERE "OtherVideoMetadataId" = ?
           ORDER BY "StreamIndex", "Id""#,
    )
    .bind(metadata_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| SubtitleInfoRow {
            stream_index: r.try_get("StreamIndex").unwrap_or(0),
            subtitle_kind: r.try_get("SubtitleKind").unwrap_or(0),
            title: r.try_get::<Option<String>, _>("Title").unwrap_or(None),
            codec: r.try_get::<Option<String>, _>("Codec").unwrap_or(None),
            language: r.try_get::<Option<String>, _>("Language").unwrap_or(None),
            default: r.try_get::<i64, _>("Default").unwrap_or(0) != 0,
            forced: r.try_get::<i64, _>("Forced").unwrap_or(0) != 0,
            path: r.try_get::<Option<String>, _>("Path").unwrap_or(None),
            is_extracted: r.try_get::<i64, _>("IsExtracted").unwrap_or(0) != 0,
        })
        .collect())
}

#[cfg(all(test, feature = "ssr"))]
async fn load_episode_subtitle_info(
    pool: &ersatzrs_infra::db::DbPool,
    metadata_id: i64,
) -> Result<Vec<SubtitleInfoRow>, ServerFnError> {
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT "StreamIndex",
                  "SubtitleKind",
                  "Title",
                  "Codec",
                  "Language",
                  "Default",
                  "Forced",
                  "Path",
                  "IsExtracted"
           FROM "Subtitle"
           WHERE "EpisodeMetadataId" = ?
           ORDER BY "StreamIndex", "Id""#,
    )
    .bind(metadata_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| SubtitleInfoRow {
            stream_index: r.try_get("StreamIndex").unwrap_or(0),
            subtitle_kind: r.try_get("SubtitleKind").unwrap_or(0),
            title: r.try_get::<Option<String>, _>("Title").unwrap_or(None),
            codec: r.try_get::<Option<String>, _>("Codec").unwrap_or(None),
            language: r.try_get::<Option<String>, _>("Language").unwrap_or(None),
            default: r.try_get::<i64, _>("Default").unwrap_or(0) != 0,
            forced: r.try_get::<i64, _>("Forced").unwrap_or(0) != 0,
            path: r.try_get::<Option<String>, _>("Path").unwrap_or(None),
            is_extracted: r.try_get::<i64, _>("IsExtracted").unwrap_or(0) != 0,
        })
        .collect())
}

#[cfg(all(test, feature = "ssr"))]
#[allow(clippy::too_many_lines)]
async fn load_media_info_streams(
    pool: &ersatzrs_infra::db::DbPool,
    media_version_id: i64,
    subtitles: &[SubtitleInfoRow],
    is_local_library: bool,
) -> Result<Vec<MediaItemInfoStreamJson>, ServerFnError> {
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT "Index",
                  "MediaStreamKind",
                  "Title",
                  "Codec",
                  "Profile",
                  "Language",
                  "Channels",
                  "Default",
                  "Forced",
                  "AttachedPic",
                  "PixelFormat",
                  "ColorRange",
                  "ColorSpace",
                  "ColorTransfer",
                  "ColorPrimaries",
                  "BitsPerRawSample",
                  "MimeType",
                  "FileName"
           FROM "MediaStream"
           WHERE "MediaVersionId" = ?
           ORDER BY "Index", "Id""#,
    )
    .bind(media_version_id)
    .fetch_all(pool)
    .await?;

    let mut streams = rows
        .iter()
        .map(|r| {
            let index = r.try_get::<i64, _>("Index").unwrap_or(0);
            let kind_value = r.try_get::<i64, _>("MediaStreamKind").unwrap_or(0);
            let mut stream = MediaItemInfoStreamJson {
                index: Some(index),
                kind: media_stream_kind_name(kind_value).to_string(),
                title: r.try_get::<Option<String>, _>("Title").unwrap_or(None),
                codec: r.try_get::<Option<String>, _>("Codec").unwrap_or(None),
                profile: r.try_get::<Option<String>, _>("Profile").unwrap_or(None),
                language: r.try_get::<Option<String>, _>("Language").unwrap_or(None),
                channels: positive_i64(r.try_get("Channels").unwrap_or(0)),
                default: true_or_none(r.try_get("Default").unwrap_or(0)),
                forced: true_or_none(r.try_get("Forced").unwrap_or(0)),
                attached_pic: true_or_none(r.try_get("AttachedPic").unwrap_or(0)),
                pixel_format: r
                    .try_get::<Option<String>, _>("PixelFormat")
                    .unwrap_or(None),
                color_range: r.try_get::<Option<String>, _>("ColorRange").unwrap_or(None),
                color_space: r.try_get::<Option<String>, _>("ColorSpace").unwrap_or(None),
                color_transfer: r
                    .try_get::<Option<String>, _>("ColorTransfer")
                    .unwrap_or(None),
                color_primaries: r
                    .try_get::<Option<String>, _>("ColorPrimaries")
                    .unwrap_or(None),
                bits_per_raw_sample: positive_i64(r.try_get("BitsPerRawSample").unwrap_or(0)),
                mime_type: r.try_get::<Option<String>, _>("MimeType").unwrap_or(None),
                file_name: r.try_get::<Option<String>, _>("FileName").unwrap_or(None),
                is_extracted: None,
            };

            if stream.kind == "Subtitle" {
                if let Some(subtitle) = subtitles.iter().find(|s| s.stream_index == index) {
                    if !is_image_subtitle(subtitle.codec.as_deref()) {
                        stream.is_extracted = Some(subtitle.is_extracted);
                    }
                    stream.file_name = clean_string(subtitle.path.clone());
                }
            }

            stream
        })
        .collect::<Vec<_>>();

    if is_local_library {
        streams.extend(subtitles.iter().filter(|s| s.subtitle_kind == 1).map(|s| {
            MediaItemInfoStreamJson {
                index: None,
                kind: "ExternalSubtitle".to_string(),
                title: clean_string(s.title.clone()),
                codec: clean_string(s.codec.clone()),
                profile: None,
                language: clean_string(s.language.clone()),
                channels: None,
                default: s.default.then_some(true),
                forced: s.forced.then_some(true),
                attached_pic: None,
                pixel_format: None,
                color_range: None,
                color_space: None,
                color_transfer: None,
                color_primaries: None,
                bits_per_raw_sample: None,
                mime_type: None,
                file_name: s.path.as_ref().and_then(|p| {
                    std::path::Path::new(p)
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                }),
                is_extracted: None,
            }
        }));
    }

    Ok(streams)
}

#[cfg(all(test, feature = "ssr"))]
async fn load_media_info_chapters(
    pool: &ersatzrs_infra::db::DbPool,
    media_version_id: i64,
) -> Result<Vec<MediaItemInfoChapterJson>, ServerFnError> {
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT COALESCE("Title", '') AS "Title", "StartTime", "EndTime"
           FROM "MediaChapter"
           WHERE "MediaVersionId" = ?
           ORDER BY "StartTime", "ChapterId""#,
    )
    .bind(media_version_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| MediaItemInfoChapterJson {
            title: r.try_get("Title").unwrap_or_default(),
            start_time: r.try_get("StartTime").unwrap_or_default(),
            end_time: r.try_get("EndTime").unwrap_or_default(),
        })
        .collect())
}

#[cfg(all(test, feature = "ssr"))]
fn clean_string(value: Option<String>) -> Option<String> {
    value.filter(|s| !s.trim().is_empty())
}

#[cfg(all(test, feature = "ssr"))]
fn true_or_none(value: i64) -> Option<bool> {
    (value != 0).then_some(true)
}

#[cfg(all(test, feature = "ssr"))]
fn positive_i64(value: i64) -> Option<i64> {
    (value > 0).then_some(value)
}

#[cfg(feature = "ssr")]
fn media_item_state_name(value: i64) -> &'static str {
    match value {
        1 => "FileNotFound",
        2 => "Unavailable",
        3 => "RemoteOnly",
        _ => "Normal",
    }
}

#[cfg(feature = "ssr")]
fn media_stream_kind_name(value: i64) -> &'static str {
    match value {
        2 => "Audio",
        3 => "Subtitle",
        4 => "Attachment",
        5 => "ExternalSubtitle",
        // Some current ErsatzRS scanner/provider paths still write zero-based
        // stream kinds. Accept `0` as video when displaying media info, while
        // preserving the ErsatzTV one-based names for canonical data.
        _ => "Video",
    }
}

#[cfg(feature = "ssr")]
fn video_scan_kind_name(value: i64) -> &'static str {
    match value {
        1 => "Progressive",
        2 => "Interlaced",
        _ => "Unknown",
    }
}

#[cfg(all(test, feature = "ssr"))]
fn is_image_subtitle(codec: Option<&str>) -> bool {
    matches!(
        codec,
        Some("hdmv_pgs_subtitle" | "dvd_subtitle" | "dvdsub" | "vobsub" | "pgssub" | "pgs")
    )
}

// ---------------------------------------------------------------------------
// Shows
// ---------------------------------------------------------------------------

#[server]
pub async fn list_shows(
    search: String,
    page: u32,
    page_size: u32,
    show_variants: bool,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    list_media_cards_from_application(
        &state,
        "show",
        ersatzrs_core::repositories::AdminSearchKind::Show,
        &search,
        page,
        page_size,
        show_variants,
    )
    .await
}

#[cfg(all(test, feature = "ssr"))]
async fn list_shows_legacy_sql(
    search: String,
    page: u32,
    page_size: u32,
    show_variants: bool,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let offset = i64::from(page) * i64::from(page_size);
    let like = format!("%{}%", search.trim());

    let total: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "ShowMetadata" sm
           JOIN "MediaItem" mi ON mi."Id" = sm."ShowId"
           WHERE (? = '' OR sm."Title" LIKE ?)
             AND (? = 1 OR COALESCE(mi."ContentKind", 0) = 0)"#,
    )
    .bind(search.trim())
    .bind(&like)
    .bind(i64::from(show_variants))
    .fetch_one(&state.pool)
    .await?;

    let rows = sqlx::query(
        r#"SELECT s."Id" AS "Id",
                  COALESCE(sm."Title", '') AS "Title",
                  COALESCE(sm."SortTitle", sm."Title", '') AS "SortTitle",
                  sm."Year" AS "Year",
                  a."Path" AS "Poster",
                  COALESCE(mi."ContentKind", 0) AS "ContentKind",
                  mi."SubKind" AS "SubKind"
           FROM "Show" s
           JOIN "ShowMetadata" sm ON sm."ShowId" = s."Id"
           JOIN "MediaItem" mi ON mi."Id" = s."Id"
           LEFT JOIN "Artwork" a
               ON a."ShowMetadataId" = sm."Id" AND a."ArtworkKind" = 0
           WHERE (? = '' OR sm."Title" LIKE ?)
             AND (? = 1 OR COALESCE(mi."ContentKind", 0) = 0)
           ORDER BY sm."SortTitle", sm."Title"
           LIMIT ? OFFSET ?"#,
    )
    .bind(search.trim())
    .bind(&like)
    .bind(i64::from(show_variants))
    .bind(i64::from(page_size))
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;

    let cards = rows
        .iter()
        .map(|r| MediaCardRow {
            id: r.try_get("Id").unwrap_or(0),
            title: r.try_get("Title").unwrap_or_default(),
            sort_title: r.try_get("SortTitle").unwrap_or_default(),
            subtitle: r
                .try_get::<Option<i64>, _>("Year")
                .unwrap_or(None)
                .map(|y| y.to_string()),
            artwork_path: r
                .try_get::<Option<String>, _>("Poster")
                .unwrap_or(None)
                .and_then(|p| artwork_url(&p, ArtworkUrlKind::Poster)),
            season_number: None,
            variant: content_variant_from_row(r),
        })
        .collect();

    #[allow(clippy::cast_sign_loss)]
    Ok((cards, total.max(0) as u64))
}

// ---------------------------------------------------------------------------
// Images
// ---------------------------------------------------------------------------

#[server]
pub async fn list_images(
    search: String,
    page: u32,
    page_size: u32,
    show_variants: bool,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    list_media_cards_from_application(
        &state,
        "image",
        ersatzrs_core::repositories::AdminSearchKind::Image,
        &search,
        page,
        page_size,
        show_variants,
    )
    .await
}

#[cfg(all(test, feature = "ssr"))]
async fn list_images_legacy_sql(
    search: String,
    page: u32,
    page_size: u32,
    show_variants: bool,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let offset = i64::from(page) * i64::from(page_size);
    let like = format!("%{}%", search.trim());

    let total: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "ImageMetadata" im
           JOIN "MediaItem" mi ON mi."Id" = im."ImageId"
           WHERE (? = '' OR im."Title" LIKE ?)
             AND (? = 1 OR COALESCE(mi."ContentKind", 0) = 0)"#,
    )
    .bind(search.trim())
    .bind(&like)
    .bind(i64::from(show_variants))
    .fetch_one(&state.pool)
    .await?;

    let rows = sqlx::query(
        r#"SELECT i."Id" AS "Id",
                  COALESCE(im."Title", '') AS "Title",
                  COALESCE(im."SortTitle", im."Title", '') AS "SortTitle",
                  im."ReleaseDate" AS "ReleaseDate",
                  a."Path" AS "Thumb",
                  COALESCE(mi."ContentKind", 0) AS "ContentKind",
                  mi."SubKind" AS "SubKind"
           FROM "Image" i
           JOIN "ImageMetadata" im ON im."ImageId" = i."Id"
           JOIN "MediaItem" mi ON mi."Id" = i."Id"
           LEFT JOIN "Artwork" a
               ON a."ImageMetadataId" = im."Id"
           WHERE (? = '' OR im."Title" LIKE ?)
             AND (? = 1 OR COALESCE(mi."ContentKind", 0) = 0)
           ORDER BY im."SortTitle", im."Title"
           LIMIT ? OFFSET ?"#,
    )
    .bind(search.trim())
    .bind(&like)
    .bind(i64::from(show_variants))
    .bind(i64::from(page_size))
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;

    let cards = rows
        .iter()
        .map(|r| MediaCardRow {
            id: r.try_get("Id").unwrap_or(0),
            title: r.try_get("Title").unwrap_or_default(),
            sort_title: r.try_get("SortTitle").unwrap_or_default(),
            subtitle: r
                .try_get::<Option<String>, _>("ReleaseDate")
                .unwrap_or(None),
            artwork_path: r
                .try_get::<Option<String>, _>("Thumb")
                .unwrap_or(None)
                .and_then(|p| artwork_url(&p, ArtworkUrlKind::Thumbnail)),
            season_number: None,
            variant: content_variant_from_row(r),
        })
        .collect();

    #[allow(clippy::cast_sign_loss)]
    Ok((cards, total.max(0) as u64))
}

// ---------------------------------------------------------------------------
// Artists
// ---------------------------------------------------------------------------

#[server]
pub async fn list_artists(
    search: String,
    page: u32,
    page_size: u32,
    show_variants: bool,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    list_media_cards_from_application(
        &state,
        "artist",
        ersatzrs_core::repositories::AdminSearchKind::Artist,
        &search,
        page,
        page_size,
        show_variants,
    )
    .await
}

#[cfg(all(test, feature = "ssr"))]
async fn list_artists_legacy_sql(
    search: String,
    page: u32,
    page_size: u32,
    show_variants: bool,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let offset = i64::from(page) * i64::from(page_size);
    let like = format!("%{}%", search.trim());

    let total: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "ArtistMetadata" am
           JOIN "MediaItem" mi ON mi."Id" = am."ArtistId"
           WHERE (? = '' OR am."Title" LIKE ?)
             AND (? = 1 OR COALESCE(mi."ContentKind", 0) = 0)"#,
    )
    .bind(search.trim())
    .bind(&like)
    .bind(i64::from(show_variants))
    .fetch_one(&state.pool)
    .await?;

    let rows = sqlx::query(
        r#"SELECT a."Id" AS "Id",
                  COALESCE(am."Title", '') AS "Title",
                  COALESCE(am."SortTitle", am."Title", '') AS "SortTitle",
                  am."Disambiguation" AS "Subtitle",
                  art."Path" AS "Thumb",
                  COALESCE(mi."ContentKind", 0) AS "ContentKind",
                  mi."SubKind" AS "SubKind"
           FROM "Artist" a
           JOIN "ArtistMetadata" am ON am."ArtistId" = a."Id"
           JOIN "MediaItem" mi ON mi."Id" = a."Id"
           LEFT JOIN "Artwork" art
               ON art."ArtistMetadataId" = am."Id" AND art."ArtworkKind" = 1
           WHERE (? = '' OR am."Title" LIKE ?)
             AND (? = 1 OR COALESCE(mi."ContentKind", 0) = 0)
           ORDER BY am."SortTitle", am."Title"
           LIMIT ? OFFSET ?"#,
    )
    .bind(search.trim())
    .bind(&like)
    .bind(i64::from(show_variants))
    .bind(i64::from(page_size))
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;

    let cards = rows
        .iter()
        .map(|r| MediaCardRow {
            id: r.try_get("Id").unwrap_or(0),
            title: r.try_get("Title").unwrap_or_default(),
            sort_title: r.try_get("SortTitle").unwrap_or_default(),
            subtitle: r.try_get::<Option<String>, _>("Subtitle").unwrap_or(None),
            artwork_path: r
                .try_get::<Option<String>, _>("Thumb")
                .unwrap_or(None)
                .and_then(|p| artwork_url(&p, ArtworkUrlKind::Thumbnail)),
            season_number: None,
            variant: content_variant_from_row(r),
        })
        .collect();

    #[allow(clippy::cast_sign_loss)]
    Ok((cards, total.max(0) as u64))
}

/// Artist details for `/media/music/artists/:id`. Ports
/// `Pages/Artist.razor`, `GetArtistByIdHandler`, and
/// `GetMusicVideoCardsHandler` for the currently implemented DB shape:
/// artist metadata/artwork/facts plus the first 100 music videos sorted by
/// `SortTitle`.
#[server]
pub async fn get_artist_details(id: i64) -> Result<Option<ArtistDetailRow>, ServerFnError> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let Some(detail) = state.application.get_artist_detail(id).await? else {
        return Ok(None);
    };
    let map_music_video =
        |video: ersatzrs_core::repositories::AdminArtistMusicVideo| ArtistMusicVideoRow {
            id: video.id,
            title: video.title,
            sort_title: video.sort_title,
            plot: video.plot,
            album: video.album,
            thumbnail_url: video
                .thumbnail_path
                .and_then(|path| artwork_url(&path, ArtworkUrlKind::Thumbnail)),
            media_item_state: video.media_item_state,
            file_path: video.file_path,
            local_path: video.local_path,
        };

    Ok(Some(ArtistDetailRow {
        id: detail.id,
        title: detail.title,
        disambiguation: detail.disambiguation,
        biography: detail.biography,
        thumbnail_url: detail
            .thumbnail_path
            .and_then(|path| artwork_url(&path, ArtworkUrlKind::Thumbnail)),
        fanart_url: detail
            .fanart_path
            .and_then(|path| artwork_url(&path, ArtworkUrlKind::FanArt)),
        genres: detail.genres,
        styles: detail.styles,
        moods: detail.moods,
        languages: detail.languages,
        music_videos: detail
            .music_videos
            .into_iter()
            .map(map_music_video)
            .collect(),
        versions: detail.versions.into_iter().map(map_music_video).collect(),
    }))
}

#[cfg(all(test, feature = "ssr"))]
async fn load_artist_music_videos(
    pool: &ersatzrs_infra::db::DbPool,
    artist_id: i64,
    versions: bool,
) -> Result<Vec<ArtistMusicVideoRow>, ServerFnError> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT mv."Id" AS "Id",
                  COALESCE(mvm."Title", '') AS "Title",
                  COALESCE(mvm."SortTitle", mvm."Title", '') AS "SortTitle",
                  mvm."Plot" AS "Plot",
                  mvm."Album" AS "Album",
                  art."Path" AS "Thumbnail",
                  mi."State" AS "State",
                  mi."LibraryPathId" AS "LibraryPathId",
                  mf."Path" AS "Path"
           FROM "MusicVideo" mv
           JOIN "MusicVideoMetadata" mvm ON mvm."MusicVideoId" = mv."Id"
           JOIN "MediaItem" mi ON mi."Id" = mv."Id"
           LEFT JOIN "Artwork" art
               ON art."MusicVideoMetadataId" = mvm."Id" AND art."ArtworkKind" = 1
           LEFT JOIN "MediaVersion" ver
               ON ver."MusicVideoId" = mv."Id"
              AND ver."Id" = (
                  SELECT "Id" FROM "MediaVersion"
                  WHERE "MusicVideoId" = mv."Id"
                  ORDER BY "Id"
                  LIMIT 1
              )
           LEFT JOIN "MediaFile" mf
               ON mf."MediaVersionId" = ver."Id"
              AND mf."Id" = (
                  SELECT "Id" FROM "MediaFile"
                  WHERE "MediaVersionId" = ver."Id"
                  ORDER BY "Id"
                  LIMIT 1
              )
           WHERE mv."ArtistId" = ?
             AND COALESCE(mi."ContentKind", 0) = ?
           ORDER BY mvm."SortTitle", mvm."Title"
           LIMIT 100"#,
    )
    .bind(artist_id)
    .bind(if versions { 2_i64 } else { 0_i64 })
    .fetch_all(pool)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let file_path: Option<String> = r.try_get::<Option<String>, _>("Path").unwrap_or(None);
        let library_path_id: Option<i64> = r.try_get("LibraryPathId").unwrap_or(None);
        let local_path = match (file_path.as_deref(), library_path_id) {
            (Some(path), Some(library_path_id)) => {
                let replacements =
                    load_path_replacements_for_library_path(pool, library_path_id).await?;
                Some(resolve_local_path(path, &replacements))
            }
            (Some(path), None) => Some(path.to_owned()),
            (None, _) => None,
        };

        out.push(ArtistMusicVideoRow {
            id: r.try_get("Id").unwrap_or(0),
            title: r.try_get("Title").unwrap_or_default(),
            sort_title: r.try_get("SortTitle").unwrap_or_default(),
            plot: r.try_get::<Option<String>, _>("Plot").unwrap_or(None),
            album: r.try_get::<Option<String>, _>("Album").unwrap_or(None),
            thumbnail_url: r
                .try_get::<Option<String>, _>("Thumbnail")
                .unwrap_or(None)
                .and_then(|p| artwork_url(&p, ArtworkUrlKind::Thumbnail)),
            media_item_state: r.try_get("State").unwrap_or(0),
            file_path,
            local_path,
        });
    }

    Ok(out)
}

// ---------------------------------------------------------------------------
// Other videos
// ---------------------------------------------------------------------------

#[server]
pub async fn list_other_videos(
    search: String,
    page: u32,
    page_size: u32,
    show_variants: bool,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    list_media_cards_from_application(
        &state,
        "other_video",
        ersatzrs_core::repositories::AdminSearchKind::OtherVideo,
        &search,
        page,
        page_size,
        show_variants,
    )
    .await
}

#[cfg(all(test, feature = "ssr"))]
async fn list_other_videos_legacy_sql(
    search: String,
    page: u32,
    page_size: u32,
    show_variants: bool,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let offset = i64::from(page) * i64::from(page_size);
    let like = format!("%{}%", search.trim());

    let total: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "OtherVideoMetadata" ovm
           JOIN "MediaItem" mi ON mi."Id" = ovm."OtherVideoId"
           WHERE (? = '' OR ovm."Title" LIKE ?)
             AND (? = 1 OR COALESCE(mi."ContentKind", 0) = 0)"#,
    )
    .bind(search.trim())
    .bind(&like)
    .bind(i64::from(show_variants))
    .fetch_one(&state.pool)
    .await?;

    let rows = sqlx::query(
        r#"SELECT ov."Id" AS "Id",
                  COALESCE(ovm."Title", '') AS "Title",
                  COALESCE(ovm."SortTitle", ovm."Title", '') AS "SortTitle",
                  ovm."OriginalTitle" AS "Subtitle",
                  COALESCE(thumb."Path", poster."Path") AS "Artwork",
                  COALESCE(mi."ContentKind", 0) AS "ContentKind",
                  mi."SubKind" AS "SubKind"
           FROM "OtherVideo" ov
           JOIN "OtherVideoMetadata" ovm ON ovm."OtherVideoId" = ov."Id"
           JOIN "MediaItem" mi ON mi."Id" = ov."Id"
           LEFT JOIN "Artwork" thumb
               ON thumb."OtherVideoMetadataId" = ovm."Id" AND thumb."ArtworkKind" = 1
           LEFT JOIN "Artwork" poster
               ON poster."OtherVideoMetadataId" = ovm."Id" AND poster."ArtworkKind" = 0
           WHERE (? = '' OR ovm."Title" LIKE ?)
             AND (? = 1 OR COALESCE(mi."ContentKind", 0) = 0)
           ORDER BY ovm."SortTitle", ovm."Title"
           LIMIT ? OFFSET ?"#,
    )
    .bind(search.trim())
    .bind(&like)
    .bind(i64::from(show_variants))
    .bind(i64::from(page_size))
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;

    let cards = rows
        .iter()
        .map(|r| MediaCardRow {
            id: r.try_get("Id").unwrap_or(0),
            title: r.try_get("Title").unwrap_or_default(),
            sort_title: r.try_get("SortTitle").unwrap_or_default(),
            subtitle: r.try_get::<Option<String>, _>("Subtitle").unwrap_or(None),
            artwork_path: r
                .try_get::<Option<String>, _>("Artwork")
                .unwrap_or(None)
                .and_then(|p| artwork_url(&p, ArtworkUrlKind::Poster)),
            season_number: None,
            variant: content_variant_from_row(r),
        })
        .collect();

    #[allow(clippy::cast_sign_loss)]
    Ok((cards, total.max(0) as u64))
}

/// Other-video details for `/media/other/videos/:id`. ErsatzTV has no
/// separate Razor detail page for this route, so this ports the authoritative
/// other-video data graph and action surface from `OtherVideoList.razor`,
/// `OtherVideoCardViewModel`, and `OtherVideoRepository`.
#[server]
#[allow(clippy::too_many_lines)]
pub async fn get_other_video_details(
    id: i64,
) -> Result<Option<OtherVideoDetailRow>, ServerFnError> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let Some(detail) = state.application.get_other_video_detail(id).await? else {
        return Ok(None);
    };

    Ok(Some(OtherVideoDetailRow {
        id: detail.id,
        title: detail.title,
        original_title: detail.original_title,
        year: detail.year,
        plot: detail.plot,
        tagline: detail.tagline,
        content_ratings: detail.content_ratings,
        release_date: detail.release_date,
        artwork_url: detail
            .thumbnail_path
            .and_then(|path| artwork_url(&path, ArtworkUrlKind::Thumbnail))
            .or_else(|| {
                detail
                    .poster_path
                    .and_then(|path| artwork_url(&path, ArtworkUrlKind::Poster))
            }),
        fanart_url: detail
            .fanart_path
            .and_then(|path| artwork_url(&path, ArtworkUrlKind::FanArt)),
        file_path: detail.file_path,
        local_path: detail.local_path,
        media_item_state: detail.media_item_state,
        genres: detail.genres,
        tags: detail.tags,
        languages: detail.languages,
        studios: detail.studios,
        directors: detail.directors,
        writers: detail.writers,
        actors: detail
            .actors
            .into_iter()
            .map(|actor| ActorCardRow {
                id: actor.id,
                name: actor.name,
                role: actor.role,
                thumb_url: actor
                    .artwork_path
                    .and_then(|path| artwork_url(&path, ArtworkUrlKind::Thumbnail)),
            })
            .collect(),
    }))
}

// ---------------------------------------------------------------------------
// Seasons — "See All" target for the search page's Seasons section.
//
// Mirrors the season projection in `search.rs`: the card title is
// "Specials" for season 0 else "Season {n}"; subtitle is the show title;
// artwork falls back from the season poster to the show poster.
// ---------------------------------------------------------------------------

#[server]
pub async fn list_seasons(
    search: String,
    page: u32,
    page_size: u32,
    show_variants: bool,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    list_media_cards_from_application(
        &state,
        "season",
        ersatzrs_core::repositories::AdminSearchKind::Season,
        &search,
        page,
        page_size,
        show_variants,
    )
    .await
}

#[cfg(all(test, feature = "ssr"))]
async fn list_seasons_legacy_sql(
    search: String,
    page: u32,
    page_size: u32,
    show_variants: bool,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let offset = i64::from(page) * i64::from(page_size);
    let like = format!("%{}%", search.trim());

    // Seasons are searched by their show's title (seasons have no title of
    // their own); ordering matches `media_page_map`'s SortTitle ordering.
    let total: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "Season" s
           JOIN "Show" sh ON sh."Id" = s."ShowId"
           JOIN "ShowMetadata" shm ON shm."ShowId" = sh."Id"
           JOIN "MediaItem" mi ON mi."Id" = s."Id"
           WHERE (? = '' OR shm."Title" LIKE ?)
             AND (? = 1 OR COALESCE(mi."ContentKind", 0) = 0)"#,
    )
    .bind(search.trim())
    .bind(&like)
    .bind(i64::from(show_variants))
    .fetch_one(&state.pool)
    .await?;

    let rows = sqlx::query(
        r#"SELECT s."Id" AS "Id",
                  s."SeasonNumber" AS "SeasonNumber",
                  COALESCE(shm."Title", '') AS "ShowTitle",
                  COALESCE(shm."SortTitle", shm."Title", '') AS "ShowSortTitle",
                  COALESCE(sa."Path", sha."Path") AS "Poster",
                  COALESCE(mi."ContentKind", 0) AS "ContentKind",
                  mi."SubKind" AS "SubKind"
           FROM "Season" s
           JOIN "Show" sh ON sh."Id" = s."ShowId"
           JOIN "ShowMetadata" shm ON shm."ShowId" = sh."Id"
           JOIN "MediaItem" mi ON mi."Id" = s."Id"
           LEFT JOIN "SeasonMetadata" sm ON sm."SeasonId" = s."Id"
           LEFT JOIN "Artwork" sa
               ON sa."SeasonMetadataId" = sm."Id" AND sa."ArtworkKind" = 0
           LEFT JOIN "Artwork" sha
               ON sha."ShowMetadataId" = shm."Id" AND sha."ArtworkKind" = 0
           WHERE (? = '' OR shm."Title" LIKE ?)
             AND (? = 1 OR COALESCE(mi."ContentKind", 0) = 0)
           ORDER BY shm."SortTitle", shm."Title", s."SeasonNumber"
           LIMIT ? OFFSET ?"#,
    )
    .bind(search.trim())
    .bind(&like)
    .bind(i64::from(show_variants))
    .bind(i64::from(page_size))
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;

    let cards = rows
        .iter()
        .map(|r| {
            let number: i64 = r.try_get("SeasonNumber").unwrap_or(0);
            let show_title: String = r.try_get("ShowTitle").unwrap_or_default();
            let show_sort: String = r.try_get("ShowSortTitle").unwrap_or_default();
            let title = if number == 0 {
                "Specials".to_string()
            } else {
                format!("Season {number}")
            };
            MediaCardRow {
                id: r.try_get("Id").unwrap_or(0),
                title,
                // Sort within a show by season number, scoped under the
                // show's sort title so the letter-bar groups by show.
                sort_title: format!("{show_sort} {number:04}"),
                subtitle: (!show_title.is_empty()).then_some(show_title),
                artwork_path: r
                    .try_get::<Option<String>, _>("Poster")
                    .unwrap_or(None)
                    .and_then(|p| artwork_url(&p, ArtworkUrlKind::Poster)),
                season_number: Some(number),
                variant: content_variant_from_row(r),
            }
        })
        .collect();

    #[allow(clippy::cast_sign_loss)]
    Ok((cards, total.max(0) as u64))
}

// ---------------------------------------------------------------------------
// Episodes — "See All" target for the search page's Episodes section.
//
// Subtitle mirrors `search.rs`: "{ShowTitle} - S{season} E{episode}".
// Artwork falls back episode → season → show poster.
// ---------------------------------------------------------------------------

#[server]
pub async fn list_episodes(
    search: String,
    page: u32,
    page_size: u32,
    show_variants: bool,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    list_media_cards_from_application(
        &state,
        "episode",
        ersatzrs_core::repositories::AdminSearchKind::Episode,
        &search,
        page,
        page_size,
        show_variants,
    )
    .await
}

#[cfg(all(test, feature = "ssr"))]
async fn list_episodes_legacy_sql(
    search: String,
    page: u32,
    page_size: u32,
    show_variants: bool,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let offset = i64::from(page) * i64::from(page_size);
    let like = format!("%{}%", search.trim());

    let total: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "EpisodeMetadata" em
           JOIN "MediaItem" mi ON mi."Id" = em."EpisodeId"
           WHERE (? = '' OR em."Title" LIKE ?)
             AND (? = 1 OR COALESCE(mi."ContentKind", 0) = 0)"#,
    )
    .bind(search.trim())
    .bind(&like)
    .bind(i64::from(show_variants))
    .fetch_one(&state.pool)
    .await?;

    let rows = sqlx::query(
        r#"SELECT e."Id" AS "Id",
                  COALESCE(em."Title", '') AS "Title",
                  COALESCE(em."SortTitle", em."Title", '') AS "SortTitle",
                  em."EpisodeNumber" AS "EpisodeNumber",
                  s."SeasonNumber" AS "SeasonNumber",
                  COALESCE(shm."Title", '') AS "ShowTitle",
                  COALESCE(ea."Path", sa."Path", sha."Path") AS "Poster",
                  COALESCE(mi."ContentKind", 0) AS "ContentKind",
                  mi."SubKind" AS "SubKind"
           FROM "Episode" e
           JOIN "EpisodeMetadata" em ON em."EpisodeId" = e."Id"
           JOIN "MediaItem" mi ON mi."Id" = e."Id"
           JOIN "Season" s ON s."Id" = e."SeasonId"
           JOIN "Show" sh ON sh."Id" = s."ShowId"
           JOIN "ShowMetadata" shm ON shm."ShowId" = sh."Id"
           LEFT JOIN "SeasonMetadata" sm ON sm."SeasonId" = s."Id"
           LEFT JOIN "Artwork" ea
               ON ea."EpisodeMetadataId" = em."Id" AND ea."ArtworkKind" = 0
           LEFT JOIN "Artwork" sa
               ON sa."SeasonMetadataId" = sm."Id" AND sa."ArtworkKind" = 0
           LEFT JOIN "Artwork" sha
               ON sha."ShowMetadataId" = shm."Id" AND sha."ArtworkKind" = 0
           WHERE (? = '' OR em."Title" LIKE ?)
             AND (? = 1 OR COALESCE(mi."ContentKind", 0) = 0)
           ORDER BY em."SortTitle", em."Title"
           LIMIT ? OFFSET ?"#,
    )
    .bind(search.trim())
    .bind(&like)
    .bind(i64::from(show_variants))
    .bind(i64::from(page_size))
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;

    let cards = rows
        .iter()
        .map(|r| {
            let show_title: String = r.try_get("ShowTitle").unwrap_or_default();
            let season_number: i64 = r.try_get("SeasonNumber").unwrap_or(0);
            let episode_number: i64 = r.try_get("EpisodeNumber").unwrap_or(0);
            let subtitle = (!show_title.is_empty())
                .then(|| format!("{show_title} - S{season_number} E{episode_number}"));
            MediaCardRow {
                id: r.try_get("Id").unwrap_or(0),
                title: r.try_get("Title").unwrap_or_default(),
                sort_title: r.try_get("SortTitle").unwrap_or_default(),
                subtitle,
                artwork_path: r
                    .try_get::<Option<String>, _>("Poster")
                    .unwrap_or(None)
                    .and_then(|p| artwork_url(&p, ArtworkUrlKind::Poster)),
                season_number: None,
                variant: content_variant_from_row(r),
            }
        })
        .collect();

    #[allow(clippy::cast_sign_loss)]
    Ok((cards, total.max(0) as u64))
}

// ---------------------------------------------------------------------------
// Music videos — "See All" target for the search page's Music Videos section.
//
// Subtitle is the owning artist's name; artwork is the thumbnail (kind 1).
// ---------------------------------------------------------------------------

#[server]
pub async fn list_music_videos(
    search: String,
    page: u32,
    page_size: u32,
    show_variants: bool,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    list_media_cards_from_application(
        &state,
        "music_video",
        ersatzrs_core::repositories::AdminSearchKind::MusicVideo,
        &search,
        page,
        page_size,
        show_variants,
    )
    .await
}

#[cfg(all(test, feature = "ssr"))]
async fn list_music_videos_legacy_sql(
    search: String,
    page: u32,
    page_size: u32,
    show_variants: bool,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let offset = i64::from(page) * i64::from(page_size);
    let like = format!("%{}%", search.trim());

    let total: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "MusicVideoMetadata" mvm
           JOIN "MediaItem" mi ON mi."Id" = mvm."MusicVideoId"
           WHERE (? = '' OR mvm."Title" LIKE ?)
             AND (? = 1 OR COALESCE(mi."ContentKind", 0) = 0)"#,
    )
    .bind(search.trim())
    .bind(&like)
    .bind(i64::from(show_variants))
    .fetch_one(&state.pool)
    .await?;

    let rows = sqlx::query(
        r#"SELECT mv."Id" AS "Id",
                  COALESCE(mvm."Title", '') AS "Title",
                  COALESCE(mvm."SortTitle", mvm."Title", '') AS "SortTitle",
                  am."Title" AS "ArtistName",
                  art."Path" AS "Thumb",
                  COALESCE(mi."ContentKind", 0) AS "ContentKind",
                  mi."SubKind" AS "SubKind"
           FROM "MusicVideo" mv
           JOIN "MusicVideoMetadata" mvm ON mvm."MusicVideoId" = mv."Id"
           JOIN "MediaItem" mi ON mi."Id" = mv."Id"
           LEFT JOIN "Artist" a ON a."Id" = mv."ArtistId"
           LEFT JOIN "ArtistMetadata" am ON am."ArtistId" = a."Id"
           LEFT JOIN "Artwork" art
               ON art."MusicVideoMetadataId" = mvm."Id" AND art."ArtworkKind" = 1
           WHERE (? = '' OR mvm."Title" LIKE ?)
             AND (? = 1 OR COALESCE(mi."ContentKind", 0) = 0)
           ORDER BY mvm."SortTitle", mvm."Title"
           LIMIT ? OFFSET ?"#,
    )
    .bind(search.trim())
    .bind(&like)
    .bind(i64::from(show_variants))
    .bind(i64::from(page_size))
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;

    let cards = rows
        .iter()
        .map(|r| MediaCardRow {
            id: r.try_get("Id").unwrap_or(0),
            title: r.try_get("Title").unwrap_or_default(),
            sort_title: r.try_get("SortTitle").unwrap_or_default(),
            subtitle: r
                .try_get::<Option<String>, _>("ArtistName")
                .unwrap_or(None)
                .filter(|s| !s.trim().is_empty()),
            artwork_path: r
                .try_get::<Option<String>, _>("Thumb")
                .unwrap_or(None)
                .and_then(|p| artwork_url(&p, ArtworkUrlKind::Thumbnail)),
            season_number: None,
            variant: content_variant_from_row(r),
        })
        .collect();

    #[allow(clippy::cast_sign_loss)]
    Ok((cards, total.max(0) as u64))
}

// ---------------------------------------------------------------------------
// Remote streams — ports `Pages/RemoteStreamList.razor`.
//
// ErsatzTV queries the search index with `type:remote_stream` plus the user's
// query text, so this path must support field queries such as `library_id:7`
// and not just title LIKE filtering.
// ---------------------------------------------------------------------------

#[cfg(feature = "ssr")]
fn remote_stream_search_query(search: &str) -> String {
    let trimmed = search.trim();
    if trimmed.is_empty() {
        "type:remote_stream".to_owned()
    } else {
        format!("type:remote_stream AND ({trimmed})")
    }
}

#[server]
pub async fn remote_stream_page_map(
    search: String,
    page_size: u32,
) -> Result<LetterPageMap, ServerFnError> {
    use crate::host::state::AppState;

    if page_size == 0 {
        return Ok(LetterPageMap { pages: vec![0; 27] });
    }

    let state = expect_context::<AppState>();
    let mut pages = vec![0_u32; 27];
    let mut documents = state
        .application
        .admin_search_documents(&remote_stream_search_query(&search))
        .await?;
    documents.sort_by_key(|document| {
        document
            .sort_title
            .as_deref()
            .or(document.title.as_deref())
            .unwrap_or_default()
            .to_ascii_lowercase()
    });
    for (index, document) in documents.iter().enumerate() {
        let first = document
            .sort_title
            .as_deref()
            .or(document.title.as_deref())
            .and_then(|title| title.chars().next())
            .unwrap_or('#');
        let bucket = bucket_index(first);
        if pages[bucket] == 0 {
            let page = index / usize::try_from(page_size).unwrap_or(usize::MAX) + 1;
            pages[bucket] = u32::try_from(page).unwrap_or(u32::MAX);
        }
    }

    Ok(LetterPageMap { pages })
}

#[server]
pub async fn list_remote_streams(
    search: String,
    page: u32,
    page_size: u32,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    list_media_cards_from_application(
        &state,
        "remote_stream",
        ersatzrs_core::repositories::AdminSearchKind::RemoteStream,
        &search,
        page,
        page_size,
        true,
    )
    .await
}

#[cfg(all(test, feature = "ssr"))]
async fn list_remote_streams_legacy_sql(
    search: String,
    page: u32,
    page_size: u32,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use crate::host::state::AppState;
    use sqlx::Row;
    use std::collections::HashMap;

    let state = expect_context::<AppState>();
    let skip = page as usize * page_size as usize;
    let (hits, total, _) = state
        .repositories
        .scheduling
        .search_documents_page(
            &remote_stream_search_query(&search),
            skip,
            page_size as usize,
        )
        .await?;

    let mut thumbs = HashMap::new();
    let ids: Vec<i64> = hits.iter().map(|hit| hit.media_item_id).collect();
    if !ids.is_empty() {
        let mut query = sqlx::QueryBuilder::<ersatzrs_infra::db::Db>::new(
            r#"SELECT rsm."RemoteStreamId" AS "ItemId", art."Path" AS "Thumb"
               FROM "RemoteStreamMetadata" rsm
               LEFT JOIN "Artwork" art
                   ON art."RemoteStreamMetadataId" = rsm."Id" AND art."ArtworkKind" = 1
               WHERE rsm."RemoteStreamId" IN ("#,
        );
        let mut separated = query.separated(", ");
        for id in &ids {
            separated.push_bind(*id);
        }
        separated.push_unseparated(")");

        for row in query.build().fetch_all(&state.pool).await? {
            let id: i64 = row.try_get("ItemId").unwrap_or_default();
            if let Ok(Some(path)) = row.try_get::<Option<String>, _>("Thumb") {
                thumbs.insert(id, path);
            }
        }
    }

    let cards = hits
        .into_iter()
        .map(|hit| {
            let title = hit.title.unwrap_or_default();
            let sort_title = hit.sort_title.unwrap_or_else(|| title.clone());
            MediaCardRow {
                id: hit.media_item_id,
                title,
                sort_title,
                subtitle: hit.original_title.filter(|s| !s.trim().is_empty()),
                artwork_path: thumbs
                    .get(&hit.media_item_id)
                    .and_then(|p| artwork_url(p, ArtworkUrlKind::Poster)),
                season_number: None,
                variant: ContentVariantInfo {
                    content_kind: hit.content_kind,
                    sub_kind: hit.sub_kind,
                    parent_title: hit.parent_title,
                },
            }
        })
        .collect();

    Ok((cards, total as u64))
}

// ---------------------------------------------------------------------------
// Songs
// ---------------------------------------------------------------------------

#[server]
pub async fn list_songs(
    search: String,
    page: u32,
    page_size: u32,
    show_variants: bool,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    list_media_cards_from_application(
        &state,
        "song",
        ersatzrs_core::repositories::AdminSearchKind::Song,
        &search,
        page,
        page_size,
        show_variants,
    )
    .await
}

#[cfg(all(test, feature = "ssr"))]
async fn list_songs_legacy_sql(
    search: String,
    page: u32,
    page_size: u32,
    show_variants: bool,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let offset = i64::from(page) * i64::from(page_size);
    let like = format!("%{}%", search.trim());

    let total: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "SongMetadata" sm
           JOIN "MediaItem" mi ON mi."Id" = sm."SongId"
           WHERE (? = '' OR sm."Title" LIKE ?)
             AND (? = 1 OR COALESCE(mi."ContentKind", 0) = 0)"#,
    )
    .bind(search.trim())
    .bind(&like)
    .bind(i64::from(show_variants))
    .fetch_one(&state.pool)
    .await?;

    let rows = sqlx::query(
        r#"SELECT s."Id" AS "Id",
                  COALESCE(sm."Title", '') AS "Title",
                  COALESCE(sm."SortTitle", sm."Title", '') AS "SortTitle",
                  sm."Artists",
                  sm."Album",
                  art."Path" AS "Thumb",
                  COALESCE(mi."ContentKind", 0) AS "ContentKind",
                  mi."SubKind" AS "SubKind"
           FROM "Song" s
           JOIN "SongMetadata" sm ON sm."SongId" = s."Id"
           JOIN "MediaItem" mi ON mi."Id" = s."Id"
           LEFT JOIN "Artwork" art
               ON art."SongMetadataId" = sm."Id" AND art."ArtworkKind" = 1
           WHERE (? = '' OR sm."Title" LIKE ?)
             AND (? = 1 OR COALESCE(mi."ContentKind", 0) = 0)
           ORDER BY sm."SortTitle", sm."Title"
           LIMIT ? OFFSET ?"#,
    )
    .bind(search.trim())
    .bind(&like)
    .bind(i64::from(show_variants))
    .bind(i64::from(page_size))
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;

    let cards = rows
        .iter()
        .map(|r| MediaCardRow {
            id: r.try_get("Id").unwrap_or(0),
            title: r.try_get("Title").unwrap_or_default(),
            sort_title: r.try_get("SortTitle").unwrap_or_default(),
            subtitle: song_subtitle(
                r.try_get::<Option<String>, _>("Artists").unwrap_or(None),
                r.try_get::<Option<String>, _>("Album").unwrap_or(None),
            ),
            artwork_path: r
                .try_get::<Option<String>, _>("Thumb")
                .unwrap_or(None)
                .and_then(|p| artwork_url(&p, ArtworkUrlKind::Poster)),
            season_number: None,
            variant: content_variant_from_row(r),
        })
        .collect();

    #[allow(clippy::cast_sign_loss)]
    Ok((cards, total.max(0) as u64))
}

// ---------------------------------------------------------------------------
// Trash
// ---------------------------------------------------------------------------

#[server]
pub async fn list_trash_items() -> Result<TrashPageData, ServerFnError> {
    use crate::host::state::AppState;
    use ersatzrs_core::repositories::AdminSearchKind as K;
    use ersatzrs_domain::MediaItemState;

    let state = expect_context::<AppState>();
    let load = |media_kind, kind| {
        list_media_cards_from_application_filtered(
            &state,
            media_kind,
            kind,
            "",
            0,
            u32::MAX,
            true,
            Some(MediaItemState::FileNotFound),
        )
    };
    let (movies, shows, seasons, episodes, artists) = tokio::try_join!(
        load("movie", K::Movie),
        load("show", K::Show),
        load("season", K::Season),
        load("episode", K::Episode),
        load("artist", K::Artist),
    )?;
    let (music_videos, other_videos, songs, images, remote_streams) = tokio::try_join!(
        load("music_video", K::MusicVideo),
        load("other_video", K::OtherVideo),
        load("song", K::Song),
        load("image", K::Image),
        load("remote_stream", K::RemoteStream),
    )?;

    Ok(TrashPageData {
        movies: movies.0,
        shows: shows.0,
        seasons: seasons.0,
        episodes: episodes.0,
        artists: artists.0,
        music_videos: music_videos.0,
        other_videos: other_videos.0,
        songs: songs.0,
        images: images.0,
        remote_streams: remote_streams.0,
    })
}

#[server]
pub async fn delete_media_items(ids: Vec<i64>) -> Result<u64, ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    Ok(state.application.delete_media_items(&ids).await?)
}

#[server]
pub async fn empty_trash() -> Result<u64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state.application.empty_media_trash().await?)
}

#[cfg(all(test, feature = "ssr"))]
async fn load_trash_movies(
    pool: &ersatzrs_infra::db::DbPool,
) -> Result<Vec<MediaCardRow>, sqlx::Error> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT m."Id" AS "Id",
                  COALESCE(mm."Title", '') AS "Title",
                  COALESCE(mm."SortTitle", mm."Title", '') AS "SortTitle",
                  mm."Year" AS "Year",
                  a."Path" AS "Poster"
           FROM "Movie" m
           JOIN "MediaItem" mi ON mi."Id" = m."Id" AND mi."State" = 1
           JOIN "MovieMetadata" mm ON mm."MovieId" = m."Id"
           LEFT JOIN "Artwork" a
               ON a."MovieMetadataId" = mm."Id" AND a."ArtworkKind" = 0
           ORDER BY mm."SortTitle", mm."Title""#,
    )
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
                .try_get::<Option<String>, _>("Poster")
                .unwrap_or(None)
                .and_then(|p| artwork_url(&p, ArtworkUrlKind::Poster)),
            season_number: None,
            variant: ContentVariantInfo::default(),
        })
        .collect())
}

#[cfg(all(test, feature = "ssr"))]
async fn load_trash_shows(
    pool: &ersatzrs_infra::db::DbPool,
) -> Result<Vec<MediaCardRow>, sqlx::Error> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT s."Id" AS "Id",
                  COALESCE(sm."Title", '') AS "Title",
                  COALESCE(sm."SortTitle", sm."Title", '') AS "SortTitle",
                  sm."Year" AS "Year",
                  a."Path" AS "Poster"
           FROM "Show" s
           JOIN "MediaItem" mi ON mi."Id" = s."Id" AND mi."State" = 1
           JOIN "ShowMetadata" sm ON sm."ShowId" = s."Id"
           LEFT JOIN "Artwork" a
               ON a."ShowMetadataId" = sm."Id" AND a."ArtworkKind" = 0
           ORDER BY sm."SortTitle", sm."Title""#,
    )
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
                .try_get::<Option<String>, _>("Poster")
                .unwrap_or(None)
                .and_then(|p| artwork_url(&p, ArtworkUrlKind::Poster)),
            season_number: None,
            variant: ContentVariantInfo::default(),
        })
        .collect())
}

#[cfg(all(test, feature = "ssr"))]
async fn load_trash_seasons(
    pool: &ersatzrs_infra::db::DbPool,
) -> Result<Vec<MediaCardRow>, sqlx::Error> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT s."Id" AS "Id",
                  s."SeasonNumber" AS "SeasonNumber",
                  COALESCE(shm."Title", '') AS "ShowTitle",
                  COALESCE(shm."SortTitle", shm."Title", '') AS "ShowSortTitle",
                  COALESCE(sa."Path", sha."Path") AS "Poster"
           FROM "Season" s
           JOIN "MediaItem" mi ON mi."Id" = s."Id" AND mi."State" = 1
           JOIN "Show" sh ON sh."Id" = s."ShowId"
           JOIN "ShowMetadata" shm ON shm."ShowId" = sh."Id"
           LEFT JOIN "SeasonMetadata" sm ON sm."SeasonId" = s."Id"
           LEFT JOIN "Artwork" sa
               ON sa."SeasonMetadataId" = sm."Id" AND sa."ArtworkKind" = 0
           LEFT JOIN "Artwork" sha
               ON sha."ShowMetadataId" = shm."Id" AND sha."ArtworkKind" = 0
           ORDER BY shm."SortTitle", shm."Title", s."SeasonNumber""#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| {
            let number: i64 = r.try_get("SeasonNumber").unwrap_or(0);
            let show_title: String = r.try_get("ShowTitle").unwrap_or_default();
            let show_sort: String = r.try_get("ShowSortTitle").unwrap_or_default();
            MediaCardRow {
                id: r.try_get("Id").unwrap_or(0),
                title: season_name(number),
                sort_title: format!("{show_sort} {number:04}"),
                subtitle: (!show_title.is_empty()).then_some(show_title),
                artwork_path: r
                    .try_get::<Option<String>, _>("Poster")
                    .unwrap_or(None)
                    .and_then(|p| artwork_url(&p, ArtworkUrlKind::Poster)),
                season_number: Some(number),
                variant: ContentVariantInfo::default(),
            }
        })
        .collect())
}

#[cfg(all(test, feature = "ssr"))]
async fn load_trash_episodes(
    pool: &ersatzrs_infra::db::DbPool,
) -> Result<Vec<MediaCardRow>, sqlx::Error> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT e."Id" AS "Id",
                  COALESCE(em."Title", '') AS "Title",
                  COALESCE(em."SortTitle", em."Title", '') AS "SortTitle",
                  em."EpisodeNumber" AS "EpisodeNumber",
                  s."SeasonNumber" AS "SeasonNumber",
                  COALESCE(shm."Title", '') AS "ShowTitle",
                  COALESCE(ea."Path", sa."Path", sha."Path") AS "Poster"
           FROM "Episode" e
           JOIN "MediaItem" mi ON mi."Id" = e."Id" AND mi."State" = 1
           JOIN "EpisodeMetadata" em ON em."EpisodeId" = e."Id"
           JOIN "Season" s ON s."Id" = e."SeasonId"
           JOIN "Show" sh ON sh."Id" = s."ShowId"
           JOIN "ShowMetadata" shm ON shm."ShowId" = sh."Id"
           LEFT JOIN "SeasonMetadata" sm ON sm."SeasonId" = s."Id"
           LEFT JOIN "Artwork" ea
               ON ea."EpisodeMetadataId" = em."Id" AND ea."ArtworkKind" = 0
           LEFT JOIN "Artwork" sa
               ON sa."SeasonMetadataId" = sm."Id" AND sa."ArtworkKind" = 0
           LEFT JOIN "Artwork" sha
               ON sha."ShowMetadataId" = shm."Id" AND sha."ArtworkKind" = 0
           ORDER BY shm."SortTitle", s."SeasonNumber", em."EpisodeNumber", em."SortTitle", em."Title""#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| {
            let show_title: String = r.try_get("ShowTitle").unwrap_or_default();
            let season_number: i64 = r.try_get("SeasonNumber").unwrap_or(0);
            let episode_number: i64 = r.try_get("EpisodeNumber").unwrap_or(0);
            let subtitle = (!show_title.is_empty())
                .then(|| format!("{show_title} - S{season_number} E{episode_number}"));
            MediaCardRow {
                id: r.try_get("Id").unwrap_or(0),
                title: r.try_get("Title").unwrap_or_default(),
                sort_title: r.try_get("SortTitle").unwrap_or_default(),
                subtitle,
                artwork_path: r
                    .try_get::<Option<String>, _>("Poster")
                    .unwrap_or(None)
                    .and_then(|p| artwork_url(&p, ArtworkUrlKind::Poster)),
                season_number: None,
                variant: ContentVariantInfo::default(),
            }
        })
        .collect())
}

#[cfg(all(test, feature = "ssr"))]
async fn load_trash_artists(
    pool: &ersatzrs_infra::db::DbPool,
) -> Result<Vec<MediaCardRow>, sqlx::Error> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT a."Id" AS "Id",
                  COALESCE(am."Title", '') AS "Title",
                  COALESCE(am."SortTitle", am."Title", '') AS "SortTitle",
                  am."Disambiguation" AS "Subtitle",
                  art."Path" AS "Thumb"
           FROM "Artist" a
           JOIN "MediaItem" mi ON mi."Id" = a."Id" AND mi."State" = 1
           JOIN "ArtistMetadata" am ON am."ArtistId" = a."Id"
           LEFT JOIN "Artwork" art
               ON art."ArtistMetadataId" = am."Id" AND art."ArtworkKind" = 1
           ORDER BY am."SortTitle", am."Title""#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| MediaCardRow {
            id: r.try_get("Id").unwrap_or(0),
            title: r.try_get("Title").unwrap_or_default(),
            sort_title: r.try_get("SortTitle").unwrap_or_default(),
            subtitle: r.try_get::<Option<String>, _>("Subtitle").unwrap_or(None),
            artwork_path: r
                .try_get::<Option<String>, _>("Thumb")
                .unwrap_or(None)
                .and_then(|p| artwork_url(&p, ArtworkUrlKind::Poster)),
            season_number: None,
            variant: ContentVariantInfo::default(),
        })
        .collect())
}

#[cfg(all(test, feature = "ssr"))]
async fn load_trash_music_videos(
    pool: &ersatzrs_infra::db::DbPool,
) -> Result<Vec<MediaCardRow>, sqlx::Error> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT mv."Id" AS "Id",
                  COALESCE(mvm."Title", '') AS "Title",
                  COALESCE(mvm."SortTitle", mvm."Title", '') AS "SortTitle",
                  am."Title" AS "ArtistName",
                  art."Path" AS "Thumb"
           FROM "MusicVideo" mv
           JOIN "MediaItem" mi ON mi."Id" = mv."Id" AND mi."State" = 1
           JOIN "MusicVideoMetadata" mvm ON mvm."MusicVideoId" = mv."Id"
           LEFT JOIN "Artist" a ON a."Id" = mv."ArtistId"
           LEFT JOIN "ArtistMetadata" am ON am."ArtistId" = a."Id"
           LEFT JOIN "Artwork" art
               ON art."MusicVideoMetadataId" = mvm."Id" AND art."ArtworkKind" = 1
           ORDER BY mvm."SortTitle", mvm."Title""#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| MediaCardRow {
            id: r.try_get("Id").unwrap_or(0),
            title: r.try_get("Title").unwrap_or_default(),
            sort_title: r.try_get("SortTitle").unwrap_or_default(),
            subtitle: r
                .try_get::<Option<String>, _>("ArtistName")
                .unwrap_or(None)
                .filter(|s| !s.trim().is_empty()),
            artwork_path: r
                .try_get::<Option<String>, _>("Thumb")
                .unwrap_or(None)
                .and_then(|p| artwork_url(&p, ArtworkUrlKind::Poster)),
            season_number: None,
            variant: ContentVariantInfo::default(),
        })
        .collect())
}

#[cfg(all(test, feature = "ssr"))]
async fn load_trash_other_videos(
    pool: &ersatzrs_infra::db::DbPool,
) -> Result<Vec<MediaCardRow>, sqlx::Error> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT ov."Id" AS "Id",
                  COALESCE(ovm."Title", '') AS "Title",
                  COALESCE(ovm."SortTitle", ovm."Title", '') AS "SortTitle",
                  ovm."OriginalTitle" AS "Subtitle",
                  COALESCE(thumb."Path", poster."Path") AS "Artwork"
           FROM "OtherVideo" ov
           JOIN "MediaItem" mi ON mi."Id" = ov."Id" AND mi."State" = 1
           JOIN "OtherVideoMetadata" ovm ON ovm."OtherVideoId" = ov."Id"
           LEFT JOIN "Artwork" thumb
               ON thumb."OtherVideoMetadataId" = ovm."Id" AND thumb."ArtworkKind" = 1
           LEFT JOIN "Artwork" poster
               ON poster."OtherVideoMetadataId" = ovm."Id" AND poster."ArtworkKind" = 0
           ORDER BY ovm."SortTitle", ovm."Title""#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| MediaCardRow {
            id: r.try_get("Id").unwrap_or(0),
            title: r.try_get("Title").unwrap_or_default(),
            sort_title: r.try_get("SortTitle").unwrap_or_default(),
            subtitle: r.try_get::<Option<String>, _>("Subtitle").unwrap_or(None),
            artwork_path: r
                .try_get::<Option<String>, _>("Artwork")
                .unwrap_or(None)
                .and_then(|p| artwork_url(&p, ArtworkUrlKind::Poster)),
            season_number: None,
            variant: ContentVariantInfo::default(),
        })
        .collect())
}

#[cfg(all(test, feature = "ssr"))]
async fn load_trash_songs(
    pool: &ersatzrs_infra::db::DbPool,
) -> Result<Vec<MediaCardRow>, sqlx::Error> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT s."Id" AS "Id",
                  COALESCE(sm."Title", '') AS "Title",
                  COALESCE(sm."SortTitle", sm."Title", '') AS "SortTitle",
                  sm."Artists",
                  sm."Album",
                  art."Path" AS "Thumb"
           FROM "Song" s
           JOIN "MediaItem" mi ON mi."Id" = s."Id" AND mi."State" = 1
           JOIN "SongMetadata" sm ON sm."SongId" = s."Id"
           LEFT JOIN "Artwork" art
               ON art."SongMetadataId" = sm."Id" AND art."ArtworkKind" = 1
           ORDER BY sm."SortTitle", sm."Title""#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| MediaCardRow {
            id: r.try_get("Id").unwrap_or(0),
            title: r.try_get("Title").unwrap_or_default(),
            sort_title: r.try_get("SortTitle").unwrap_or_default(),
            subtitle: song_subtitle(
                r.try_get::<Option<String>, _>("Artists").unwrap_or(None),
                r.try_get::<Option<String>, _>("Album").unwrap_or(None),
            ),
            artwork_path: r
                .try_get::<Option<String>, _>("Thumb")
                .unwrap_or(None)
                .and_then(|p| artwork_url(&p, ArtworkUrlKind::Poster)),
            season_number: None,
            variant: ContentVariantInfo::default(),
        })
        .collect())
}

#[cfg(all(test, feature = "ssr"))]
async fn load_trash_images(
    pool: &ersatzrs_infra::db::DbPool,
) -> Result<Vec<MediaCardRow>, sqlx::Error> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT i."Id" AS "Id",
                  COALESCE(im."Title", '') AS "Title",
                  COALESCE(im."SortTitle", im."Title", '') AS "SortTitle",
                  im."ReleaseDate" AS "ReleaseDate",
                  a."Path" AS "Thumb"
           FROM "Image" i
           JOIN "MediaItem" mi ON mi."Id" = i."Id" AND mi."State" = 1
           JOIN "ImageMetadata" im ON im."ImageId" = i."Id"
           LEFT JOIN "Artwork" a ON a."ImageMetadataId" = im."Id"
           ORDER BY im."SortTitle", im."Title""#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| MediaCardRow {
            id: r.try_get("Id").unwrap_or(0),
            title: r.try_get("Title").unwrap_or_default(),
            sort_title: r.try_get("SortTitle").unwrap_or_default(),
            subtitle: r
                .try_get::<Option<String>, _>("ReleaseDate")
                .unwrap_or(None),
            artwork_path: r
                .try_get::<Option<String>, _>("Thumb")
                .unwrap_or(None)
                .and_then(|p| artwork_url(&p, ArtworkUrlKind::Poster)),
            season_number: None,
            variant: ContentVariantInfo::default(),
        })
        .collect())
}

#[cfg(all(test, feature = "ssr"))]
async fn load_trash_remote_streams(
    pool: &ersatzrs_infra::db::DbPool,
) -> Result<Vec<MediaCardRow>, sqlx::Error> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT rs."Id" AS "Id",
                  COALESCE(rsm."Title", '') AS "Title",
                  COALESCE(rsm."SortTitle", rsm."Title", '') AS "SortTitle",
                  rsm."OriginalTitle" AS "Subtitle",
                  art."Path" AS "Thumb"
           FROM "RemoteStream" rs
           JOIN "MediaItem" mi ON mi."Id" = rs."Id" AND mi."State" = 1
           JOIN "RemoteStreamMetadata" rsm ON rsm."RemoteStreamId" = rs."Id"
           LEFT JOIN "Artwork" art
               ON art."RemoteStreamMetadataId" = rsm."Id" AND art."ArtworkKind" = 1
           ORDER BY rsm."SortTitle", rsm."Title""#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| MediaCardRow {
            id: r.try_get("Id").unwrap_or(0),
            title: r.try_get("Title").unwrap_or_default(),
            sort_title: r.try_get("SortTitle").unwrap_or_default(),
            subtitle: r.try_get::<Option<String>, _>("Subtitle").unwrap_or(None),
            artwork_path: r
                .try_get::<Option<String>, _>("Thumb")
                .unwrap_or(None)
                .and_then(|p| artwork_url(&p, ArtworkUrlKind::Poster)),
            season_number: None,
            variant: ContentVariantInfo::default(),
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Manual collections
// ---------------------------------------------------------------------------

#[server]
pub async fn list_collections(
    search: String,
    page: u32,
    page_size: u32,
) -> Result<(Vec<CollectionListRow>, u64), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let (rows, total) = state
        .application
        .list_collections(ersatzrs_app::queries::ListCollections {
            search,
            page,
            page_size,
        })
        .await
        .map_err(ServerFnError::new)?;
    Ok((
        rows.into_iter()
            .map(|row| CollectionListRow {
                id: row.id,
                name: row.name,
                query: None,
            })
            .collect(),
        total,
    ))
}

#[server]
pub async fn get_collection(id: i64) -> Result<Option<CollectionForm>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state
        .application
        .get_collection(id)
        .await
        .map_err(ServerFnError::new)?
        .map(|row| CollectionForm {
            id: row.id,
            name: row.name,
            query: String::new(),
        }))
}

#[server]
pub async fn save_collection(form: CollectionForm) -> Result<i64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    save_collection_with_state(&state, form).await
}

#[cfg(feature = "ssr")]
pub async fn save_collection_with_state(
    state: &crate::host::state::AppState,
    form: CollectionForm,
) -> Result<i64, ServerFnError> {
    state
        .application
        .save_collection(ersatzrs_app::commands::SaveCollection {
            id: form.id,
            name: form.name,
        })
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn delete_collection(id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    state
        .application
        .delete_collection(ersatzrs_app::commands::DeleteCollection { id })
        .await
        .map_err(ServerFnError::new)
}

// ---------------------------------------------------------------------------
// Smart collections
// ---------------------------------------------------------------------------

#[server]
pub async fn list_smart_collections(
    search: String,
    page: u32,
    page_size: u32,
) -> Result<(Vec<CollectionListRow>, u64), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let (rows, total) = state
        .application
        .list_smart_collections(ersatzrs_app::queries::ListCollections {
            search,
            page,
            page_size,
        })
        .await
        .map_err(ServerFnError::new)?;
    Ok((smart_collection_list_rows(rows), total))
}

#[cfg(all(test, feature = "ssr"))]
pub async fn list_smart_collections_with_pool(
    pool: &ersatzrs_infra::db::DbPool,
    search: String,
    page: u32,
    page_size: u32,
) -> Result<(Vec<CollectionListRow>, u64), ServerFnError> {
    let repository = ersatzrs_infra::repositories::SqliteCollectionRepository(pool.clone());
    let (rows, total) = ersatzrs_app::queries::list_smart_collections(
        ersatzrs_app::queries::ListCollections {
            search,
            page,
            page_size,
        },
        &repository,
    )
    .await
    .map_err(ServerFnError::new)?;
    Ok((smart_collection_list_rows(rows), total))
}

#[cfg(feature = "ssr")]
fn smart_collection_list_rows(
    rows: Vec<ersatzrs_core::repositories::SmartCollectionAdminRow>,
) -> Vec<CollectionListRow> {
    rows.into_iter()
        .map(|row| CollectionListRow {
            id: row.id,
            name: row.name,
            query: Some(row.query),
        })
        .collect()
}

#[server]
pub async fn get_smart_collection(id: i64) -> Result<Option<CollectionForm>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state
        .application
        .get_smart_collection(id)
        .await
        .map_err(ServerFnError::new)?
        .map(|row| CollectionForm {
            id: row.id,
            name: row.name,
            query: row.query,
        }))
}

#[server]
pub async fn save_smart_collection(form: CollectionForm) -> Result<i64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    save_smart_collection_with_state(&state, form).await
}

#[cfg(feature = "ssr")]
pub async fn save_smart_collection_with_state(
    state: &crate::host::state::AppState,
    form: CollectionForm,
) -> Result<i64, ServerFnError> {
    state
        .application
        .save_smart_collection(ersatzrs_app::commands::SaveSmartCollection {
            id: form.id,
            name: form.name,
            query: form.query,
        })
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn delete_smart_collection(id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    state
        .application
        .delete_smart_collection(ersatzrs_app::commands::DeleteSmartCollection { id })
        .await
        .map_err(ServerFnError::new)
}

// ---------------------------------------------------------------------------
// Multi-collections
// ---------------------------------------------------------------------------

#[server]
pub async fn get_multi_collections_page_size() -> Result<u32, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .get_multi_collections_page_size()
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn list_multi_collections(
    search: String,
    page: u32,
    page_size: u32,
) -> Result<(Vec<MultiCollectionListRow>, u64), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let (rows, total) = state
        .application
        .list_multi_collections(ersatzrs_app::queries::ListCollections {
            search,
            page,
            page_size,
        })
        .await
        .map_err(ServerFnError::new)?;
    Ok((
        rows.into_iter()
            .map(|row| MultiCollectionListRow {
                id: row.id,
                name: row.name,
            })
            .collect(),
        total,
    ))
}

#[server]
pub async fn get_multi_collection(id: i64) -> Result<Option<MultiCollectionForm>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state
        .application
        .get_multi_collection(id)
        .await
        .map_err(ServerFnError::new)?
        .map(multi_collection_form))
}

#[server]
pub async fn get_multi_collection_editor_options(
) -> Result<MultiCollectionEditorOptions, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let options = state
        .application
        .get_multi_collection_editor_options()
        .await
        .map_err(ServerFnError::new)?;
    Ok(MultiCollectionEditorOptions {
        collections: options
            .collections
            .into_iter()
            .map(multi_collection_select_option)
            .collect(),
        smart_collections: options
            .smart_collections
            .into_iter()
            .map(multi_collection_select_option)
            .collect(),
    })
}

#[cfg(feature = "ssr")]
fn multi_collection_form(
    form: ersatzrs_core::repositories::MultiCollectionAdmin,
) -> MultiCollectionForm {
    MultiCollectionForm {
        id: form.id,
        name: form.name,
        items: form
            .items
            .into_iter()
            .map(multi_collection_item_form)
            .collect(),
    }
}

#[cfg(feature = "ssr")]
fn multi_collection_item_form(
    item: ersatzrs_core::repositories::MultiCollectionItem,
) -> MultiCollectionItemForm {
    MultiCollectionItemForm {
        collection_id: item.collection_id,
        smart_collection_id: item.smart_collection_id,
        name: item.name,
        collection_type: item.collection_type,
        use_custom_playback_order: item.use_custom_playback_order,
        schedule_as_group: item.schedule_as_group,
        playback_order: item.playback_order,
    }
}

#[cfg(feature = "ssr")]
fn multi_collection_item(
    item: MultiCollectionItemForm,
) -> ersatzrs_core::repositories::MultiCollectionItem {
    ersatzrs_core::repositories::MultiCollectionItem {
        collection_id: item.collection_id,
        smart_collection_id: item.smart_collection_id,
        name: item.name,
        collection_type: item.collection_type,
        use_custom_playback_order: item.use_custom_playback_order,
        schedule_as_group: item.schedule_as_group,
        playback_order: item.playback_order,
    }
}

#[cfg(feature = "ssr")]
fn multi_collection_select_option(
    option: ersatzrs_core::repositories::MultiCollectionSelectOption,
) -> MultiCollectionSelectOption {
    MultiCollectionSelectOption {
        id: option.id,
        name: option.name,
        collection_type: option.collection_type,
        use_custom_playback_order: option.use_custom_playback_order,
    }
}

#[server]
pub async fn save_multi_collection(form: MultiCollectionForm) -> Result<i64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    save_multi_collection_with_state(&state, form).await
}

#[cfg(feature = "ssr")]
pub async fn save_multi_collection_with_state(
    state: &crate::host::state::AppState,
    form: MultiCollectionForm,
) -> Result<i64, ServerFnError> {
    state
        .application
        .save_multi_collection(ersatzrs_app::commands::SaveMultiCollection {
            id: form.id,
            name: form.name,
            items: form.items.into_iter().map(multi_collection_item).collect(),
        })
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn delete_multi_collection(id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .delete_multi_collection(ersatzrs_app::commands::DeleteMultiCollection { id })
        .await
        .map_err(ServerFnError::new)
}

// ---------------------------------------------------------------------------
// Rerun collections
// ---------------------------------------------------------------------------

#[server]
pub async fn get_rerun_collections_page_size() -> Result<u32, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .get_rerun_collections_page_size()
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn list_rerun_collections(
    search: String,
    page: u32,
    page_size: u32,
) -> Result<(Vec<RerunCollectionListRow>, u64), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let (rows, total) = state
        .application
        .list_rerun_collections(ersatzrs_app::queries::ListCollections {
            search,
            page,
            page_size,
        })
        .await
        .map_err(ServerFnError::new)?;
    Ok((
        rows.into_iter()
            .map(|row| RerunCollectionListRow {
                id: row.id,
                name: row.name,
            })
            .collect(),
        total,
    ))
}

#[server]
pub async fn get_rerun_collection_form(
    id: i64,
) -> Result<Option<RerunCollectionForm>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state
        .application
        .get_rerun_collection(id)
        .await
        .map_err(ServerFnError::new)?
        .map(rerun_collection_form))
}

#[server]
pub async fn get_rerun_collection_editor_options(
) -> Result<RerunCollectionEditorOptions, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let options = state
        .application
        .get_rerun_collection_editor_options()
        .await
        .map_err(ServerFnError::new)?;
    Ok(rerun_collection_editor_options(options))
}

#[cfg(feature = "ssr")]
fn rerun_collection_form(
    form: ersatzrs_core::repositories::RerunCollectionAdmin,
) -> RerunCollectionForm {
    RerunCollectionForm {
        id: form.id,
        name: form.name,
        collection_type: form.collection_type,
        collection_id: form.collection_id,
        media_item_id: form.media_item_id,
        multi_collection_id: form.multi_collection_id,
        smart_collection_id: form.smart_collection_id,
        first_run_playback_order: form.first_run_playback_order,
        rerun_playback_order: form.rerun_playback_order,
    }
}

#[cfg(feature = "ssr")]
fn rerun_collection_admin(
    form: RerunCollectionForm,
) -> ersatzrs_core::repositories::RerunCollectionAdmin {
    ersatzrs_core::repositories::RerunCollectionAdmin {
        id: form.id,
        name: form.name,
        collection_type: form.collection_type,
        collection_id: form.collection_id,
        media_item_id: form.media_item_id,
        multi_collection_id: form.multi_collection_id,
        smart_collection_id: form.smart_collection_id,
        first_run_playback_order: form.first_run_playback_order,
        rerun_playback_order: form.rerun_playback_order,
    }
}

#[cfg(feature = "ssr")]
fn rerun_collection_editor_options(
    options: ersatzrs_core::repositories::RerunCollectionEditorOptions,
) -> RerunCollectionEditorOptions {
    fn map(
        rows: Vec<ersatzrs_core::repositories::RerunCollectionSelectOption>,
    ) -> Vec<RerunCollectionSelectOption> {
        rows.into_iter()
            .map(|row| RerunCollectionSelectOption {
                id: row.id,
                name: row.name,
            })
            .collect()
    }

    RerunCollectionEditorOptions {
        collections: map(options.collections),
        multi_collections: map(options.multi_collections),
        smart_collections: map(options.smart_collections),
        shows: map(options.shows),
        seasons: map(options.seasons),
        artists: map(options.artists),
    }
}

#[server]
pub async fn save_rerun_collection(form: RerunCollectionForm) -> Result<i64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    save_rerun_collection_with_state(&state, form).await
}

#[cfg(feature = "ssr")]
pub async fn save_rerun_collection_with_state(
    state: &crate::host::state::AppState,
    form: RerunCollectionForm,
) -> Result<i64, ServerFnError> {
    state
        .application
        .save_rerun_collection(ersatzrs_app::commands::SaveRerunCollection {
            form: rerun_collection_admin(form),
        })
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn delete_rerun_collection(id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .delete_rerun_collection(ersatzrs_app::commands::DeleteRerunCollection { id })
        .await
        .map_err(ServerFnError::new)
}

// ---------------------------------------------------------------------------
// Autocomplete for collection editors
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MediaItemSuggestion {
    pub id: i64,
    pub title: String,
    /// "movie" | "show"
    pub kind: String,
}

/// Debounced autocomplete. Returns up to 20 movies+shows matching the prefix.
#[server]
pub async fn suggest_media_items(
    prefix: String,
) -> Result<Vec<MediaItemSuggestion>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state
        .application
        .suggest_media_items(&prefix)
        .await
        .map_err(ServerFnError::new)?
        .into_iter()
        .map(|row| MediaItemSuggestion {
            id: row.id,
            title: row.title,
            kind: row.kind,
        })
        .collect())
}

#[server]
pub async fn add_item_to_collection(
    collection_id: i64,
    media_item_id: i64,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    state
        .application
        .add_default_collection_item(ersatzrs_app::commands::ChangeCollectionItem {
            collection_id,
            media_item_id,
        })
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn remove_item_from_collection(
    collection_id: i64,
    media_item_id: i64,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    state
        .application
        .remove_collection_item(ersatzrs_app::commands::ChangeCollectionItem {
            collection_id,
            media_item_id,
        })
        .await
        .map_err(ServerFnError::new)
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CollectionMember {
    pub media_item_id: i64,
    pub title: String,
    pub kind: String,
}

// ---------------------------------------------------------------------------
// Multi-select: add selected media items to a collection or playlist.
//
// Ports `ErsatzTV.Application.MediaCollections.AddItemsToCollectionHandler`
// (collection branch) and `AddItemsToPlaylistHandler` (playlist branch).
// The MovieList page only ever passes movie ids, but the underlying tables
// are media-item agnostic; we accept a typed kind so future ports of
// `ShowList`, `OtherVideoList`, etc. can reuse these endpoints unchanged.
// ---------------------------------------------------------------------------

/// Row for the schedule dropdown in the show "Add To Schedule" dialog.
/// Mirrors the id + name subset of
/// `ErsatzTV.Application.ProgramSchedules.ProgramScheduleViewModel`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ScheduleOption {
    pub id: i64,
    pub name: String,
}

/// All program schedules, ordered by name. Mirrors
/// `GetAllProgramSchedulesHandler` (every row from `ProgramSchedule`).
#[server]
pub async fn list_program_schedules() -> Result<Vec<ScheduleOption>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state
        .application
        .list_all_schedules()
        .await?
        .into_iter()
        .map(|row| ScheduleOption {
            id: row.id,
            name: row.name,
        })
        .collect())
}

/// Adds a show to a program schedule as a single `PlayoutMode.One` item.
/// Ports `AddProgramScheduleItem.ForMediaItem(scheduleId,
/// CollectionType.TelevisionShow, showId)` +
/// `AddProgramScheduleItemHandler` / `ProgramScheduleItemCommandBase`.
///
#[server]
pub async fn add_show_to_schedule(schedule_id: i64, show_id: i64) -> Result<(), ServerFnError> {
    const COLLECTION_TYPE_TELEVISION_SHOW: i64 = 1;

    add_media_item_to_schedule_impl(schedule_id, COLLECTION_TYPE_TELEVISION_SHOW, show_id).await
}

/// Adds any media item to a program schedule as a single `PlayoutMode.One`
/// item. Ports `AddProgramScheduleItem.ForMediaItem`.
#[server]
pub async fn add_media_item_to_schedule(
    schedule_id: i64,
    collection_type: i64,
    media_item_id: i64,
) -> Result<(), ServerFnError> {
    add_media_item_to_schedule_impl(schedule_id, collection_type, media_item_id).await
}

#[cfg(feature = "ssr")]
async fn add_media_item_to_schedule_impl(
    schedule_id: i64,
    collection_type: i64,
    media_item_id: i64,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state
        .application
        .append_schedule_media_item(ersatzrs_app::commands::AppendScheduleMediaItem {
            schedule_id,
            collection_type: i32::try_from(collection_type)
                .map_err(|error| ServerFnError::new(error.to_string()))?,
            media_item_id,
        })
        .await?)
}

/// Row for the "(New Collection)" / existing-collection dropdown in
/// `AddToCollectionDialog`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CollectionOption {
    pub id: i64,
    pub name: String,
}

/// Row for the "Playlist Group" dropdown in `AddToPlaylistDialog`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PlaylistGroupOption {
    pub id: i64,
    pub name: String,
}

/// Row for the "Playlist" dropdown in `AddToPlaylistDialog`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PlaylistOption {
    pub id: i64,
    pub name: String,
    pub playlist_group_id: i64,
}

/// All collections, ordered by name. Mirrors `GetAllCollectionsHandler`,
/// which loads every row from `Collection`.
#[server]
pub async fn list_all_collections() -> Result<Vec<CollectionOption>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state
        .application
        .list_all_collections()
        .await
        .map_err(ServerFnError::new)?
        .into_iter()
        .map(|row| CollectionOption {
            id: row.id,
            name: row.name,
        })
        .collect())
}

/// All smart collections, ordered by name. Used by the search page's
/// Save-As-Smart-Collection dialog.
#[server]
pub async fn list_smart_collections_for_select() -> Result<Vec<CollectionOption>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let (rows, _) = state
        .application
        .list_smart_collections(ersatzrs_app::queries::ListCollections {
            search: String::new(),
            page: 0,
            page_size: u32::MAX,
        })
        .await
        .map_err(ServerFnError::new)?;
    Ok(rows
        .into_iter()
        .map(|row| CollectionOption {
            id: row.id,
            name: row.name,
        })
        .collect())
}

/// Creates a new collection with the given name and returns it. Mirrors
/// `CreateCollectionHandler`.
#[server]
pub async fn create_collection(name: String) -> Result<CollectionOption, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let trimmed = name.trim().to_owned();
    let id = state
        .application
        .save_collection(ersatzrs_app::commands::SaveCollection {
            id: 0,
            name: trimmed.clone(),
        })
        .await
        .map_err(ServerFnError::new)?;
    Ok(CollectionOption { id, name: trimmed })
}

#[cfg(test)]
const PLAYLIST_COLLECTION_TYPE_COLLECTION: i32 = 0;
#[cfg(test)]
const PLAYLIST_COLLECTION_TYPE_TELEVISION_SHOW: i32 = 1;
#[cfg(test)]
const PLAYLIST_COLLECTION_TYPE_TELEVISION_SEASON: i32 = 2;
#[cfg(test)]
const PLAYLIST_COLLECTION_TYPE_ARTIST: i32 = 3;
#[cfg(test)]
const PLAYLIST_COLLECTION_TYPE_MULTI_COLLECTION: i32 = 4;
#[cfg(test)]
const PLAYLIST_COLLECTION_TYPE_SMART_COLLECTION: i32 = 5;
#[cfg(test)]
const PLAYLIST_COLLECTION_TYPE_MOVIE: i32 = 10;
#[cfg(test)]
const PLAYLIST_COLLECTION_TYPE_EPISODE: i32 = 20;
#[cfg(test)]
const PLAYLIST_COLLECTION_TYPE_MUSIC_VIDEO: i32 = 30;
#[cfg(test)]
const PLAYLIST_COLLECTION_TYPE_OTHER_VIDEO: i32 = 40;
#[cfg(test)]
const PLAYLIST_COLLECTION_TYPE_SONG: i32 = 50;
#[cfg(test)]
const PLAYLIST_COLLECTION_TYPE_IMAGE: i32 = 60;
#[cfg(test)]
const PLAYLIST_COLLECTION_TYPE_REMOTE_STREAM: i32 = 70;

#[cfg(test)]
fn playlist_collection_type(value: i32) -> ersatzrs_domain::CollectionType {
    match value {
        PLAYLIST_COLLECTION_TYPE_TELEVISION_SHOW => ersatzrs_domain::CollectionType::TelevisionShow,
        PLAYLIST_COLLECTION_TYPE_TELEVISION_SEASON => {
            ersatzrs_domain::CollectionType::TelevisionSeason
        }
        PLAYLIST_COLLECTION_TYPE_ARTIST => ersatzrs_domain::CollectionType::Artist,
        PLAYLIST_COLLECTION_TYPE_MULTI_COLLECTION => {
            ersatzrs_domain::CollectionType::MultiCollection
        }
        PLAYLIST_COLLECTION_TYPE_SMART_COLLECTION => {
            ersatzrs_domain::CollectionType::SmartCollection
        }
        PLAYLIST_COLLECTION_TYPE_MOVIE => ersatzrs_domain::CollectionType::Movie,
        PLAYLIST_COLLECTION_TYPE_EPISODE => ersatzrs_domain::CollectionType::Episode,
        PLAYLIST_COLLECTION_TYPE_MUSIC_VIDEO => ersatzrs_domain::CollectionType::MusicVideo,
        PLAYLIST_COLLECTION_TYPE_OTHER_VIDEO => ersatzrs_domain::CollectionType::OtherVideo,
        PLAYLIST_COLLECTION_TYPE_SONG => ersatzrs_domain::CollectionType::Song,
        PLAYLIST_COLLECTION_TYPE_IMAGE => ersatzrs_domain::CollectionType::Image,
        PLAYLIST_COLLECTION_TYPE_REMOTE_STREAM => ersatzrs_domain::CollectionType::RemoteStream,
        _ => ersatzrs_domain::CollectionType::Collection,
    }
}

fn playlist_playback_order(value: i32) -> ersatzrs_domain::PlaybackOrder {
    match value {
        1 => ersatzrs_domain::PlaybackOrder::Chronological,
        2 => ersatzrs_domain::PlaybackOrder::Random,
        3 => ersatzrs_domain::PlaybackOrder::Shuffle,
        4 => ersatzrs_domain::PlaybackOrder::ShuffleInOrder,
        5 => ersatzrs_domain::PlaybackOrder::MultiEpisodeShuffle,
        6 => ersatzrs_domain::PlaybackOrder::SeasonEpisode,
        7 => ersatzrs_domain::PlaybackOrder::RandomRotation,
        8 => ersatzrs_domain::PlaybackOrder::Marathon,
        _ => ersatzrs_domain::PlaybackOrder::None,
    }
}

fn channel_subtitle_mode_from_i32(value: i32) -> Option<ersatzrs_domain::ChannelSubtitleMode> {
    match value {
        1 => Some(ersatzrs_domain::ChannelSubtitleMode::Forced),
        2 => Some(ersatzrs_domain::ChannelSubtitleMode::Default),
        3 => Some(ersatzrs_domain::ChannelSubtitleMode::Any),
        _ => None,
    }
}

#[server]
pub async fn get_playlist_page() -> Result<PlaylistPageData, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let groups = state
        .application
        .list_playlist_groups()
        .await
        .map_err(ServerFnError::new)?
        .into_iter()
        .map(playlist_group_from_admin)
        .collect();
    Ok(PlaylistPageData { groups })
}

#[server]
pub async fn create_playlist_group(name: String) -> Result<PlaylistGroupRow, ServerFnError> {
    use crate::host::state::AppState;
    use ersatzrs_app::commands::CreatePlaylistGroup;

    let state = expect_context::<AppState>();
    state
        .application
        .create_playlist_group(CreatePlaylistGroup { name })
        .await
        .map(playlist_group_from_admin)
        .map_err(ServerFnError::new)
}

#[server]
pub async fn create_playlist(
    playlist_group_id: i64,
    name: String,
) -> Result<PlaylistRow, ServerFnError> {
    use crate::host::state::AppState;
    use ersatzrs_app::commands::CreatePlaylist;

    let state = expect_context::<AppState>();
    state
        .application
        .create_playlist(CreatePlaylist {
            playlist_group_id,
            name,
        })
        .await
        .map(playlist_row_from_admin)
        .map_err(ServerFnError::new)
}

#[server]
pub async fn delete_playlist_group(playlist_group_id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    use ersatzrs_app::commands::DeletePlaylistGroup;

    let state = expect_context::<AppState>();
    state
        .application
        .delete_playlist_group(DeletePlaylistGroup {
            id: playlist_group_id,
        })
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn delete_playlist(playlist_id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    use ersatzrs_app::commands::DeletePlaylist;

    let state = expect_context::<AppState>();
    state
        .application
        .delete_playlist(DeletePlaylist { id: playlist_id })
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn get_playlist_editor(
    playlist_id: i64,
) -> Result<Option<PlaylistEditorForm>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .get_playlist_editor(playlist_id)
        .await
        .map(|form| form.map(playlist_editor_from_admin))
        .map_err(ServerFnError::new)
}

#[server]
pub async fn get_playlist_editor_options() -> Result<PlaylistEditorOptions, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .get_playlist_editor_options()
        .await
        .map(playlist_options_from_admin)
        .map_err(ServerFnError::new)
}

#[server]
pub async fn save_playlist(form: PlaylistEditorForm) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    use ersatzrs_app::commands::SavePlaylist;

    let state = expect_context::<AppState>();
    state
        .application
        .save_playlist(SavePlaylist {
            form: playlist_editor_into_admin(form),
        })
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn preview_playlist_playout(
    form: PlaylistEditorForm,
) -> Result<Vec<PlaylistPreviewRow>, ServerFnError> {
    use crate::host::state::AppState;
    use ersatzrs_app::queries::{PreviewPlaylistItem, PreviewPlaylistPlayout};

    let state = expect_context::<AppState>();
    state
        .application
        .preview_playlist_playout(PreviewPlaylistPlayout {
            playlist_id: form.id,
            items: form
                .items
                .into_iter()
                .map(|item| PreviewPlaylistItem {
                    collection_type: item.collection_type,
                    collection_id: item.collection_id,
                    multi_collection_id: item.multi_collection_id,
                    smart_collection_id: item.smart_collection_id,
                    media_item_id: item.media_item_id,
                    playback_order: playlist_playback_order(item.playback_order),
                    play_all: item.play_all,
                    count: item
                        .count
                        .and_then(|count| usize::try_from(count.max(1)).ok()),
                    include_in_program_guide: item.include_in_program_guide,
                    preferred_audio_language_code: item.preferred_audio_language_code,
                    preferred_audio_title: item.preferred_audio_title,
                    preferred_subtitle_language_code: item.preferred_subtitle_language_code,
                    subtitle_mode: item.subtitle_mode.and_then(channel_subtitle_mode_from_i32),
                })
                .collect(),
        })
        .await
        .map(|rows| {
            rows.into_iter()
                .map(|row| PlaylistPreviewRow {
                    start: format_duration(row.start),
                    finish: format_duration(row.finish),
                    media_item: row.title,
                    duration: format_duration(row.duration),
                })
                .collect()
        })
        .map_err(ServerFnError::new)
}

#[cfg(all(test, feature = "ssr"))]
async fn playlist_preview_reference_data(
    pool: &ersatzrs_infra::db::DbPool,
    form: &PlaylistEditorForm,
) -> Result<
    (
        ersatzrs_core::scheduling::SchedulingReferenceData,
        chrono::Duration,
        usize,
    ),
    ServerFnError,
> {
    use chrono::Duration;
    use ersatzrs_core::scheduling::{
        classic_schedule_item_key, SchedulingCollectionKey, SchedulingPlaylistItem,
        SchedulingReferenceData,
    };
    use ersatzrs_domain::{
        CollectionType, FixedStartTimeBehavior, PlaybackOrder, PlayoutMode, ProgramScheduleItem,
        StartType,
    };
    use std::collections::BTreeMap;

    let mut playlist_items = Vec::new();
    let mut one_pass_duration = Duration::zero();
    let mut max_items = 0usize;

    for item in form
        .items
        .iter()
        .filter(|item| item.include_in_program_guide)
    {
        let media_items = preview_media_items_for_playlist_item(pool, item).await?;
        let take = if item.play_all {
            media_items.len()
        } else {
            usize::try_from(item.count.unwrap_or(1).max(1)).unwrap_or(1)
        };
        max_items = max_items.saturating_add(take);
        one_pass_duration += media_items
            .iter()
            .cycle()
            .take(take)
            .fold(Duration::zero(), |duration, media| {
                duration + media.duration
            });
        let preferred_audio_language_code = item.preferred_audio_language_code.clone();
        let preferred_audio_title = item.preferred_audio_title.clone();
        let preferred_subtitle_language_code = item.preferred_subtitle_language_code.clone();
        let subtitle_mode = item.subtitle_mode.and_then(channel_subtitle_mode_from_i32);
        playlist_items.push(SchedulingPlaylistItem {
            collection_key: SchedulingCollectionKey {
                collection_type: playlist_collection_type(item.collection_type),
                collection_id: item.collection_id,
                multi_collection_id: item.multi_collection_id,
                smart_collection_id: item.smart_collection_id,
                media_item_id: item.media_item_id,
                ..SchedulingCollectionKey::default()
            },
            playback_order: playlist_playback_order(item.playback_order),
            play_all: item.play_all,
            count: item
                .count
                .and_then(|count| usize::try_from(count.max(1)).ok()),
            media_items: media_items
                .into_iter()
                .map(|media| ersatzrs_core::scheduling::SchedulingMediaItem {
                    id: media.id,
                    duration: media.duration,
                    title: Some(media.title),
                    show_id: None,
                    season_id: None,
                    artist_id: None,
                    season: None,
                    episode: None,
                    release_date: None,
                    preferred_audio_language_code: preferred_audio_language_code.clone(),
                    preferred_audio_title: preferred_audio_title.clone(),
                    preferred_subtitle_language_code: preferred_subtitle_language_code.clone(),
                    subtitle_mode,
                })
                .collect(),
        });
    }

    if max_items == 0 || one_pass_duration <= Duration::zero() {
        return Err(ServerFnError::new("Playlist playout preview has no items"));
    }

    let schedule_item = ProgramScheduleItem {
        id: 1,
        program_schedule_id: 1,
        collection_type: CollectionType::Playlist,
        playlist_id: Some(form.id),
        playout_mode: PlayoutMode::Flood,
        playback_order: PlaybackOrder::Shuffle,
        start_type: StartType::Dynamic,
        fixed_start_time_behavior: Some(FixedStartTimeBehavior::Strict),
        ..ProgramScheduleItem::default()
    };
    let schedule_key = classic_schedule_item_key(&schedule_item);
    Ok((
        SchedulingReferenceData {
            classic_schedule_items: vec![schedule_item],
            playlist_items_by_key: BTreeMap::from([(schedule_key, playlist_items)]),
            classic_fixed_start_time_behavior: FixedStartTimeBehavior::Strict,
            ..SchedulingReferenceData::default()
        },
        one_pass_duration,
        max_items,
    ))
}

#[cfg(test)]
fn validate_playlist_items(items: &[PlaylistItemForm]) -> Result<(), ServerFnError> {
    for item in items {
        match item.collection_type {
            PLAYLIST_COLLECTION_TYPE_COLLECTION if item.collection_id.is_none() => {
                return Err(ServerFnError::new(
                    "[Collection] is required for collection type 'Collection'",
                ));
            }
            PLAYLIST_COLLECTION_TYPE_TELEVISION_SHOW
            | PLAYLIST_COLLECTION_TYPE_TELEVISION_SEASON
            | PLAYLIST_COLLECTION_TYPE_ARTIST
            | PLAYLIST_COLLECTION_TYPE_MOVIE
            | PLAYLIST_COLLECTION_TYPE_EPISODE
            | PLAYLIST_COLLECTION_TYPE_MUSIC_VIDEO
            | PLAYLIST_COLLECTION_TYPE_OTHER_VIDEO
            | PLAYLIST_COLLECTION_TYPE_SONG
            | PLAYLIST_COLLECTION_TYPE_IMAGE
            | PLAYLIST_COLLECTION_TYPE_REMOTE_STREAM
                if item.media_item_id.is_none() =>
            {
                return Err(ServerFnError::new(format!(
                    "[MediaItem] is required for type '{}'",
                    collection_type_name(item.collection_type)
                )));
            }
            PLAYLIST_COLLECTION_TYPE_MULTI_COLLECTION if item.multi_collection_id.is_none() => {
                return Err(ServerFnError::new(
                    "[MultiCollection] is required for collection type 'MultiCollection'",
                ));
            }
            PLAYLIST_COLLECTION_TYPE_SMART_COLLECTION if item.smart_collection_id.is_none() => {
                return Err(ServerFnError::new(
                    "[SmartCollection] is required for collection type 'SmartCollection'",
                ));
            }
            PLAYLIST_COLLECTION_TYPE_COLLECTION
            | PLAYLIST_COLLECTION_TYPE_TELEVISION_SHOW
            | PLAYLIST_COLLECTION_TYPE_TELEVISION_SEASON
            | PLAYLIST_COLLECTION_TYPE_ARTIST
            | PLAYLIST_COLLECTION_TYPE_MULTI_COLLECTION
            | PLAYLIST_COLLECTION_TYPE_SMART_COLLECTION
            | PLAYLIST_COLLECTION_TYPE_MOVIE
            | PLAYLIST_COLLECTION_TYPE_EPISODE
            | PLAYLIST_COLLECTION_TYPE_MUSIC_VIDEO
            | PLAYLIST_COLLECTION_TYPE_OTHER_VIDEO
            | PLAYLIST_COLLECTION_TYPE_SONG
            | PLAYLIST_COLLECTION_TYPE_IMAGE
            | PLAYLIST_COLLECTION_TYPE_REMOTE_STREAM => {}
            _ => return Err(ServerFnError::new("[CollectionType] is invalid")),
        }
    }
    Ok(())
}

#[cfg(test)]
fn collection_type_name(collection_type: i32) -> &'static str {
    match collection_type {
        PLAYLIST_COLLECTION_TYPE_COLLECTION => "Collection",
        PLAYLIST_COLLECTION_TYPE_TELEVISION_SHOW => "TelevisionShow",
        PLAYLIST_COLLECTION_TYPE_TELEVISION_SEASON => "TelevisionSeason",
        PLAYLIST_COLLECTION_TYPE_ARTIST => "Artist",
        PLAYLIST_COLLECTION_TYPE_MULTI_COLLECTION => "MultiCollection",
        PLAYLIST_COLLECTION_TYPE_SMART_COLLECTION => "SmartCollection",
        PLAYLIST_COLLECTION_TYPE_MOVIE => "Movie",
        PLAYLIST_COLLECTION_TYPE_EPISODE => "Episode",
        PLAYLIST_COLLECTION_TYPE_MUSIC_VIDEO => "MusicVideo",
        PLAYLIST_COLLECTION_TYPE_OTHER_VIDEO => "OtherVideo",
        PLAYLIST_COLLECTION_TYPE_SONG => "Song",
        PLAYLIST_COLLECTION_TYPE_IMAGE => "Image",
        PLAYLIST_COLLECTION_TYPE_REMOTE_STREAM => "RemoteStream",
        _ => "Unknown",
    }
}

#[derive(Debug)]
#[cfg(all(test, feature = "ssr"))]
struct PreviewMediaItem {
    id: i64,
    title: String,
    duration: chrono::Duration,
}

#[cfg(all(test, feature = "ssr"))]
async fn preview_media_items_for_playlist_item(
    pool: &ersatzrs_infra::db::DbPool,
    item: &PlaylistItemForm,
) -> Result<Vec<PreviewMediaItem>, ServerFnError> {
    let media_item_ids = match item.collection_type {
        PLAYLIST_COLLECTION_TYPE_COLLECTION => {
            media_ids_for_collection(pool, item.collection_id).await?
        }
        PLAYLIST_COLLECTION_TYPE_MULTI_COLLECTION => {
            media_ids_for_multi_collection(pool, item.multi_collection_id).await?
        }
        PLAYLIST_COLLECTION_TYPE_SMART_COLLECTION => {
            media_ids_for_smart_collection(pool, item.smart_collection_id).await?
        }
        PLAYLIST_COLLECTION_TYPE_TELEVISION_SHOW => {
            media_ids_for_show(pool, item.media_item_id).await?
        }
        PLAYLIST_COLLECTION_TYPE_TELEVISION_SEASON => {
            media_ids_for_season(pool, item.media_item_id).await?
        }
        PLAYLIST_COLLECTION_TYPE_ARTIST
        | PLAYLIST_COLLECTION_TYPE_MOVIE
        | PLAYLIST_COLLECTION_TYPE_EPISODE
        | PLAYLIST_COLLECTION_TYPE_MUSIC_VIDEO
        | PLAYLIST_COLLECTION_TYPE_OTHER_VIDEO
        | PLAYLIST_COLLECTION_TYPE_SONG
        | PLAYLIST_COLLECTION_TYPE_IMAGE
        | PLAYLIST_COLLECTION_TYPE_REMOTE_STREAM => item.media_item_id.into_iter().collect(),
        _ => Vec::new(),
    };
    load_preview_media_items(pool, media_item_ids).await
}

#[cfg(all(test, feature = "ssr"))]
async fn media_ids_for_smart_collection(
    pool: &ersatzrs_infra::db::DbPool,
    smart_collection_id: Option<i64>,
) -> Result<Vec<i64>, ServerFnError> {
    use std::collections::BTreeSet;

    use ersatzrs_infra::repositories::SqliteSchedulingContentRepository;

    let Some(smart_collection_id) = smart_collection_id else {
        return Ok(Vec::new());
    };
    let Some(query) =
        sqlx::query_scalar::<_, String>(r#"SELECT "Query" FROM "SmartCollection" WHERE "Id" = ?"#)
            .bind(smart_collection_id)
            .fetch_optional(pool)
            .await?
    else {
        return Ok(Vec::new());
    };

    let repository = SqliteSchedulingContentRepository::new(pool.clone());
    let (hits, _, _) = repository.search_documents_page(&query, 0, 0).await?;
    let mut ids = Vec::new();
    let mut seen = BTreeSet::new();
    for hit in hits {
        let hit_ids = match hit.media_kind.as_str() {
            "show" => media_ids_for_show(pool, Some(hit.media_item_id)).await?,
            "season" => media_ids_for_season(pool, Some(hit.media_item_id)).await?,
            "artist" => media_ids_for_artist(pool, Some(hit.media_item_id)).await?,
            _ => vec![hit.media_item_id],
        };
        for id in hit_ids {
            if seen.insert(id) {
                ids.push(id);
            }
        }
    }

    Ok(ids)
}

#[cfg(all(test, feature = "ssr"))]
async fn media_ids_for_collection(
    pool: &ersatzrs_infra::db::DbPool,
    collection_id: Option<i64>,
) -> Result<Vec<i64>, ServerFnError> {
    let Some(collection_id) = collection_id else {
        return Ok(Vec::new());
    };
    Ok(sqlx::query_scalar(
        r#"SELECT "MediaItemId" FROM "CollectionItem" WHERE "CollectionId" = ? ORDER BY "MediaItemId", "Id""#,
    )
    .bind(collection_id)
    .fetch_all(pool)
    .await?)
}

#[cfg(all(test, feature = "ssr"))]
async fn media_ids_for_multi_collection(
    pool: &ersatzrs_infra::db::DbPool,
    multi_collection_id: Option<i64>,
) -> Result<Vec<i64>, ServerFnError> {
    let Some(multi_collection_id) = multi_collection_id else {
        return Ok(Vec::new());
    };
    Ok(sqlx::query_scalar(
        r#"SELECT ci."MediaItemId"
           FROM "MultiCollectionItem" mci
           JOIN "CollectionItem" ci ON ci."CollectionId" = mci."CollectionId"
           WHERE mci."MultiCollectionId" = ?
           ORDER BY mci."CollectionId", ci."MediaItemId""#,
    )
    .bind(multi_collection_id)
    .fetch_all(pool)
    .await?)
}

#[cfg(all(test, feature = "ssr"))]
async fn media_ids_for_artist(
    pool: &ersatzrs_infra::db::DbPool,
    artist_id: Option<i64>,
) -> Result<Vec<i64>, ServerFnError> {
    let Some(artist_id) = artist_id else {
        return Ok(Vec::new());
    };
    Ok(
        sqlx::query_scalar(r#"SELECT "Id" FROM "MusicVideo" WHERE "ArtistId" = ? ORDER BY "Id""#)
            .bind(artist_id)
            .fetch_all(pool)
            .await?,
    )
}

#[cfg(all(test, feature = "ssr"))]
async fn media_ids_for_show(
    pool: &ersatzrs_infra::db::DbPool,
    show_id: Option<i64>,
) -> Result<Vec<i64>, ServerFnError> {
    let Some(show_id) = show_id else {
        return Ok(Vec::new());
    };
    Ok(sqlx::query_scalar(
        r#"SELECT ep."Id"
           FROM "Episode" ep
           JOIN "Season" se ON se."Id" = ep."SeasonId"
           WHERE se."ShowId" = ?
           ORDER BY se."SeasonNumber", ep."Id""#,
    )
    .bind(show_id)
    .fetch_all(pool)
    .await?)
}

#[cfg(all(test, feature = "ssr"))]
async fn media_ids_for_season(
    pool: &ersatzrs_infra::db::DbPool,
    season_id: Option<i64>,
) -> Result<Vec<i64>, ServerFnError> {
    let Some(season_id) = season_id else {
        return Ok(Vec::new());
    };
    Ok(
        sqlx::query_scalar(r#"SELECT "Id" FROM "Episode" WHERE "SeasonId" = ? ORDER BY "Id""#)
            .bind(season_id)
            .fetch_all(pool)
            .await?,
    )
}

#[cfg(all(test, feature = "ssr"))]
async fn load_preview_media_items(
    pool: &ersatzrs_infra::db::DbPool,
    ids: Vec<i64>,
) -> Result<Vec<PreviewMediaItem>, ServerFnError> {
    use chrono::Duration;
    use sqlx::Row;

    let mut result = Vec::new();
    for id in ids {
        let Some(row) = sqlx::query(
            r#"SELECT COALESCE(mm."Title", sm."Title", sem."Title", am."Title",
                              em."Title", mvm."Title", ovm."Title", songm."Title",
                              im."Title", 'Media Item ' || CAST(mi."Id" AS TEXT)) AS "Title",
                      COALESCE(mv."Duration", ev."Duration", muv."Duration",
                               ovv."Duration", sv."Duration", iv."Duration", '00:30:00') AS "Duration"
               FROM "MediaItem" mi
               LEFT JOIN "Movie" movie_item ON movie_item."Id" = mi."Id"
               LEFT JOIN "MovieMetadata" mm ON mm."MovieId" = movie_item."Id"
               LEFT JOIN "MediaVersion" mv ON mv."MovieId" = movie_item."Id"
               LEFT JOIN "Show" show_item ON show_item."Id" = mi."Id"
               LEFT JOIN "ShowMetadata" sm ON sm."ShowId" = show_item."Id"
               LEFT JOIN "Season" season_item ON season_item."Id" = mi."Id"
               LEFT JOIN "SeasonMetadata" sem ON sem."SeasonId" = season_item."Id"
               LEFT JOIN "Artist" artist_item ON artist_item."Id" = mi."Id"
               LEFT JOIN "ArtistMetadata" am ON am."ArtistId" = artist_item."Id"
               LEFT JOIN "Episode" episode_item ON episode_item."Id" = mi."Id"
               LEFT JOIN "EpisodeMetadata" em ON em."EpisodeId" = episode_item."Id"
               LEFT JOIN "MediaVersion" ev ON ev."EpisodeId" = episode_item."Id"
               LEFT JOIN "MusicVideo" music_video_item ON music_video_item."Id" = mi."Id"
               LEFT JOIN "MusicVideoMetadata" mvm ON mvm."MusicVideoId" = music_video_item."Id"
               LEFT JOIN "MediaVersion" muv ON muv."MusicVideoId" = music_video_item."Id"
               LEFT JOIN "OtherVideo" other_video_item ON other_video_item."Id" = mi."Id"
               LEFT JOIN "OtherVideoMetadata" ovm ON ovm."OtherVideoId" = other_video_item."Id"
               LEFT JOIN "MediaVersion" ovv ON ovv."OtherVideoId" = other_video_item."Id"
               LEFT JOIN "Song" song_item ON song_item."Id" = mi."Id"
               LEFT JOIN "SongMetadata" songm ON songm."SongId" = song_item."Id"
               LEFT JOIN "MediaVersion" sv ON sv."SongId" = song_item."Id"
               LEFT JOIN "Image" image_item ON image_item."Id" = mi."Id"
               LEFT JOIN "ImageMetadata" im ON im."ImageId" = image_item."Id"
               LEFT JOIN "MediaVersion" iv ON iv."ImageId" = image_item."Id"
               WHERE mi."Id" = ?
               LIMIT 1"#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await?
        else {
            continue;
        };
        let duration_text: String = row
            .try_get::<Option<String>, _>("Duration")
            .unwrap_or_default()
            .unwrap_or_else(|| "00:30:00".to_string());
        result.push(PreviewMediaItem {
            id,
            title: row
                .try_get("Title")
                .unwrap_or_else(|_| format!("Media Item {id}")),
            duration: parse_hhmmss_duration(&duration_text)
                .unwrap_or_else(|| Duration::minutes(30)),
        });
    }
    Ok(result)
}

#[cfg(test)]
fn parse_hhmmss_duration(value: &str) -> Option<chrono::Duration> {
    let mut parts = value.split(':');
    let hours = parts.next()?.parse::<i64>().ok()?;
    let minutes = parts.next()?.parse::<i64>().ok()?;
    let seconds = parts
        .next()
        .and_then(|part| part.split('.').next())
        .and_then(|part| part.parse::<i64>().ok())?;
    Some(chrono::Duration::seconds(
        hours * 3600 + minutes * 60 + seconds,
    ))
}

fn format_duration(duration: chrono::Duration) -> String {
    let seconds = duration.num_seconds().max(0);
    format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600,
        (seconds % 3600) / 60,
        seconds % 60
    )
}

/// All playlist groups, ordered system-first then by name. Mirrors
/// `GetAllPlaylistGroupsHandler` (`OrderByDescending(pg => pg.IsSystem)`,
/// `ThenBy(pg => pg.Name)`).
#[server]
pub async fn list_all_playlist_groups() -> Result<Vec<PlaylistGroupOption>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state
        .application
        .list_playlist_groups()
        .await
        .map_err(ServerFnError::new)?
        .into_iter()
        .map(|row| PlaylistGroupOption {
            id: row.id,
            name: row.name,
        })
        .collect())
}

/// Playlists in a given group, ordered by name. Mirrors
/// `GetPlaylistsByPlaylistGroupIdHandler`.
#[server]
pub async fn list_playlists_by_group(
    playlist_group_id: i64,
) -> Result<Vec<PlaylistOption>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let groups = state
        .application
        .list_playlist_groups()
        .await
        .map_err(ServerFnError::new)?;
    Ok(groups
        .into_iter()
        .find(|group| group.id == playlist_group_id)
        .map(|group| {
            group
                .playlists
                .into_iter()
                .map(|row| PlaylistOption {
                    id: row.id,
                    name: row.name,
                    playlist_group_id: row.playlist_group_id,
                })
                .collect()
        })
        .unwrap_or_default())
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
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let collection_type = kind.collection_type();
    let playback = playback_overrides(playback);
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
                    },
                )
                .collect(),
        })
        .await
        .map_err(ServerFnError::new)
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
                })
                .collect(),
        })
        .await
        .map_err(ServerFnError::new)
}

// ---------------------------------------------------------------------------
// Collection detail page (`/media/collections/:id`).
//
// Ports `ErsatzTV/Pages/CollectionItems.razor` +
// `ErsatzTV.Application.MediaCards.GetCollectionCardsHandler`. The C# handler
// projects to a `CollectionCardResultsViewModel` with one list per kind. We
// flatten the projection into a single `Vec<CollectionItemCard>` carrying a
// `kind` discriminant — the UI groups by kind for rendering but operates on a
// uniform `(media_item_id, custom_index)` everywhere else.

#[cfg(all(test, feature = "ssr"))]
mod remote_stream_tests {
    use super::{list_remote_streams, remote_stream_page_map};
    use crate::host::state::{AppState, IptvAccessTokenVerifier, SystemStartup};
    use ersatzrs_ffmpeg::FfmpegRuntimeReport;
    use ersatzrs_infra::db::{self, DatabaseBackend, DatabaseSchemaStatus};
    use leptos::prelude::{provide_context, Owner};
    use tempfile::NamedTempFile;
    use tokio::sync::mpsc;
    use uuid::Uuid;

    #[tokio::test(flavor = "current_thread")]
    async fn remote_stream_list_uses_search_index_query_and_projects_cards() {
        let (_db_file, pool) = test_pool().await;
        seed_remote_stream_fixture(&pool).await;

        let (playout_build_tx, _playout_build_rx) = mpsc::unbounded_channel();
        let state = AppState::new(
            pool,
            DatabaseBackend::Sqlite,
            SystemStartup {
                database_ready: true,
                schema_status: DatabaseSchemaStatus::default(),
            },
            FfmpegRuntimeReport::for_tests(),
            Uuid::nil(),
            1,
            IptvAccessTokenVerifier::disabled(),
            playout_build_tx,
            ersatzrs_core::locking::EntityLocker::default(),
        );

        let owner = Owner::new();
        owner.with(|| provide_context(state));
        owner.set();

        let (cards, total) = list_remote_streams("library_id:7".to_owned(), 0, 100)
            .await
            .unwrap();

        assert_eq!(total, 1);
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].id, 701);
        assert_eq!(cards[0].title, "Nurburgring broadcast");
        assert_eq!(cards[0].sort_title, "nurburgring broadcast");
        assert_eq!(cards[0].subtitle.as_deref(), Some("Racing"));

        let page_map = remote_stream_page_map("library_id:7".to_owned(), 100)
            .await
            .unwrap();
        assert_eq!(page_map.pages[14], 1); // n

        owner.cleanup();
    }

    async fn test_pool() -> (NamedTempFile, ersatzrs_infra::db::DbPool) {
        let file = NamedTempFile::new().unwrap();
        let database_url = format!("sqlite:{}", file.path().display());
        let pool = db::init(&database_url).await.unwrap();
        (file, pool)
    }

    #[allow(clippy::too_many_lines)]
    async fn seed_remote_stream_fixture(pool: &ersatzrs_infra::db::DbPool) {
        let now = "2026-06-14T00:00:00Z";
        for (source_id, library_id, path_id, item_id, title, original_title, sort_title) in [
            (
                70,
                7,
                700,
                701,
                "Nurburgring broadcast",
                "Racing",
                "nurburgring broadcast",
            ),
            (80, 8, 800, 801, "Visdeurbel", "Vis", "visdeurbel"),
        ] {
            sqlx::query(r#"INSERT INTO "MediaSource" ("Id") VALUES (?)"#)
                .bind(source_id)
                .execute(pool)
                .await
                .unwrap();
            sqlx::query(r#"INSERT INTO "LocalMediaSource" ("Id") VALUES (?)"#)
                .bind(source_id)
                .execute(pool)
                .await
                .unwrap();
            sqlx::query(
                r#"INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
                   VALUES (?, ?, 7, ?)"#,
            )
            .bind(library_id)
            .bind(format!("Remote Streams {library_id}"))
            .bind(source_id)
            .execute(pool)
            .await
            .unwrap();
            sqlx::query(r#"INSERT INTO "LocalLibrary" ("Id") VALUES (?)"#)
                .bind(library_id)
                .execute(pool)
                .await
                .unwrap();
            sqlx::query(
                r#"INSERT INTO "LibraryPath" ("Id", "Path", "LibraryId")
                   VALUES (?, ?, ?)"#,
            )
            .bind(path_id)
            .bind(format!("/remote/{library_id}"))
            .bind(library_id)
            .execute(pool)
            .await
            .unwrap();
            sqlx::query(
                r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State")
                   VALUES (?, ?, 0)"#,
            )
            .bind(item_id)
            .bind(path_id)
            .execute(pool)
            .await
            .unwrap();
            sqlx::query(
                r#"INSERT INTO "RemoteStream" ("Id", "Url", "Script", "IsLive")
                   VALUES (?, ?, NULL, 1)"#,
            )
            .bind(item_id)
            .bind(format!("https://example.test/{item_id}.m3u8"))
            .execute(pool)
            .await
            .unwrap();
            sqlx::query(
                r#"INSERT INTO "RemoteStreamMetadata" (
                       "RemoteStreamId", "MetadataKind", "Title", "OriginalTitle",
                       "SortTitle", "DateAdded", "DateUpdated"
                   )
                   VALUES (?, 0, ?, ?, ?, ?, ?)"#,
            )
            .bind(item_id)
            .bind(title)
            .bind(original_title)
            .bind(sort_title)
            .bind(now)
            .bind(now)
            .execute(pool)
            .await
            .unwrap();
        }
    }
}

#[cfg(test)]
mod playlist_tests {
    use super::{
        validate_playlist_items, PlaylistItemForm, PLAYLIST_COLLECTION_TYPE_COLLECTION,
        PLAYLIST_COLLECTION_TYPE_MOVIE, PLAYLIST_COLLECTION_TYPE_REMOTE_STREAM,
    };

    #[test]
    fn playlist_item_validation_requires_matching_target_id() {
        let mut item = PlaylistItemForm {
            collection_type: PLAYLIST_COLLECTION_TYPE_COLLECTION,
            ..PlaylistItemForm::default()
        };
        assert!(validate_playlist_items(&[item.clone()]).is_err());

        item.collection_id = Some(10);
        assert!(validate_playlist_items(&[item.clone()]).is_ok());

        item.collection_type = PLAYLIST_COLLECTION_TYPE_MOVIE;
        item.collection_id = None;
        assert!(validate_playlist_items(&[item.clone()]).is_err());

        item.media_item_id = Some(20);
        assert!(validate_playlist_items(&[item]).is_ok());
    }

    #[test]
    fn playlist_item_validation_accepts_remote_stream_items_created_elsewhere() {
        let item = PlaylistItemForm {
            collection_type: PLAYLIST_COLLECTION_TYPE_REMOTE_STREAM,
            media_item_id: Some(70),
            ..PlaylistItemForm::default()
        };

        assert!(validate_playlist_items(&[item]).is_ok());
    }
}

#[cfg(test)]
mod season_detail_tests {
    use super::{
        get_media_item_info_json, get_season_details, list_other_videos, list_trash_items,
    };
    use crate::host::state::{AppState, IptvAccessTokenVerifier, SystemStartup};
    use ersatzrs_ffmpeg::FfmpegRuntimeReport;
    use ersatzrs_infra::db::{self, DatabaseBackend, DatabaseSchemaStatus};
    use leptos::prelude::{provide_context, Owner};
    use serde_json::Value;
    use sqlx::Executor;
    use tempfile::NamedTempFile;
    use tokio::sync::mpsc;
    use uuid::Uuid;

    #[tokio::test(flavor = "current_thread")]
    async fn season_details_and_episode_media_info_use_television_repository_projection() {
        let (_db_file, pool) = test_pool().await;
        seed_season_detail_fixture(&pool).await;

        let (playout_build_tx, _playout_build_rx) = mpsc::unbounded_channel();
        let state = AppState::new(
            pool,
            DatabaseBackend::Sqlite,
            SystemStartup {
                database_ready: true,
                schema_status: DatabaseSchemaStatus::default(),
            },
            FfmpegRuntimeReport::for_tests(),
            Uuid::nil(),
            1,
            IptvAccessTokenVerifier::disabled(),
            playout_build_tx,
            ersatzrs_core::locking::EntityLocker::default(),
        );

        let owner = Owner::new();
        owner.with(|| provide_context(state));
        owner.set();

        let season = get_season_details(20).await.unwrap().unwrap();
        assert_eq!(season.id, 20);
        assert_eq!(season.show_id, 10);
        assert_eq!(season.show_title, "Example Show");
        assert_eq!(season.name, "Season 1");
        assert_eq!(
            season.poster_url.as_deref(),
            Some("/artwork/posters/season-poster.jpg")
        );
        assert_eq!(
            season.fanart_url.as_deref(),
            Some("/artwork/fanart/show-fanart.jpg")
        );
        assert_eq!(season.episode_count, 2);
        assert_eq!(season.episodes.len(), 2);
        assert_eq!(season.episodes[0].episode_id, 30);
        assert_eq!(season.episodes[0].episode_number, 1);
        assert_eq!(season.episodes[0].title, "Pilot");
        assert_eq!(
            season.episodes[0].thumbnail_url.as_deref(),
            Some("/artwork/thumbnails/pilot-thumb.jpg")
        );
        assert_eq!(season.episodes[0].aired, "1-5-2026");
        assert_eq!(season.episodes[0].media_item_state, 1);
        assert_eq!(
            season.episodes[0].file_path.as_deref(),
            Some("/media/show/s01e01.mkv")
        );
        assert_eq!(
            season.episodes[0].directors,
            vec!["Director A", "Director B"]
        );
        assert_eq!(season.episodes[0].writers, vec!["Writer A"]);
        assert_eq!(season.episodes[1].aired, "1-1-0001");

        let json = get_media_item_info_json(30).await.unwrap();
        let value: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["Kind"], "Episode");
        assert_eq!(value["Title"], "Example Show - s01e01 - Pilot");
        assert_eq!(value["LibraryKind"], "LocalLibrary");
        assert_eq!(value["State"], "FileNotFound");
        assert_eq!(value["Duration"], "00:22:00");
        assert_eq!(value["Width"], 1920);
        assert_eq!(value["Height"], 1080);
        assert_eq!(value["Streams"][0]["Kind"], "Video");
        assert_eq!(value["Streams"][1]["Kind"], "Subtitle");
        assert_eq!(value["Streams"][1]["IsExtracted"], true);
        assert_eq!(value["Chapters"][0]["Title"], "Opening");

        owner.cleanup();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn trash_items_include_missing_episodes() {
        let (_db_file, pool) = test_pool().await;
        seed_season_detail_fixture(&pool).await;

        let (playout_build_tx, _playout_build_rx) = mpsc::unbounded_channel();
        let state = AppState::new(
            pool,
            DatabaseBackend::Sqlite,
            SystemStartup {
                database_ready: true,
                schema_status: DatabaseSchemaStatus::default(),
            },
            FfmpegRuntimeReport::for_tests(),
            Uuid::nil(),
            1,
            IptvAccessTokenVerifier::disabled(),
            playout_build_tx,
            ersatzrs_core::locking::EntityLocker::default(),
        );

        let owner = Owner::new();
        owner.with(|| provide_context(state));
        owner.set();

        let trash = list_trash_items().await.unwrap();

        assert!(trash.shows.is_empty());
        assert_eq!(trash.episodes.len(), 1);
        assert_eq!(trash.episodes[0].id, 30);
        assert_eq!(trash.episodes[0].title, "Pilot");
        assert_eq!(
            trash.episodes[0].subtitle.as_deref(),
            Some("Example Show - S1 E1")
        );

        owner.cleanup();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn linked_extras_load_on_detail_and_server_filter_supports_primary_only() {
        let (_db_file, pool) = test_pool().await;
        seed_season_detail_fixture(&pool).await;

        let (playout_build_tx, _playout_build_rx) = mpsc::unbounded_channel();
        let state = AppState::new(
            pool,
            DatabaseBackend::Sqlite,
            SystemStartup {
                database_ready: true,
                schema_status: DatabaseSchemaStatus::default(),
            },
            FfmpegRuntimeReport::for_tests(),
            Uuid::nil(),
            1,
            IptvAccessTokenVerifier::disabled(),
            playout_build_tx,
            ersatzrs_core::locking::EntityLocker::default(),
        );

        let owner = Owner::new();
        owner.with(|| provide_context(state));
        owner.set();

        let season = get_season_details(20).await.unwrap().unwrap();
        assert_eq!(season.extras.len(), 1);
        assert_eq!(season.extras[0].id, 40);
        assert_eq!(season.extras[0].title, "Featurette");
        assert_eq!(
            season.extras[0].variant.content_kind,
            ersatzrs_domain::ContentKind::Extra
        );
        assert_eq!(
            season.extras[0].variant.sub_kind.as_deref(),
            Some("featurette")
        );

        let (hidden_cards, hidden_total) = list_other_videos(String::new(), 0, 50, false)
            .await
            .unwrap();
        assert_eq!(hidden_total, 0);
        assert!(hidden_cards.is_empty());

        let (visible_cards, visible_total) =
            list_other_videos(String::new(), 0, 50, true).await.unwrap();
        assert_eq!(visible_total, 1);
        assert_eq!(visible_cards[0].id, 40);
        assert_eq!(
            visible_cards[0].variant.content_kind,
            ersatzrs_domain::ContentKind::Extra
        );

        owner.cleanup();
    }

    async fn test_pool() -> (NamedTempFile, ersatzrs_infra::db::DbPool) {
        let file = NamedTempFile::new().unwrap();
        let database_url = format!("sqlite:{}", file.path().display());
        let pool = db::init(&database_url).await.unwrap();
        (file, pool)
    }

    #[allow(clippy::too_many_lines)]
    async fn seed_season_detail_fixture(pool: &ersatzrs_infra::db::DbPool) {
        let now = "2026-05-22T00:00:00Z";
        pool.execute(r#"INSERT INTO "MediaSource" ("Id") VALUES (10)"#)
            .await
            .unwrap();
        pool.execute(r#"INSERT INTO "LocalMediaSource" ("Id") VALUES (10)"#)
            .await
            .unwrap();
        pool.execute(
            r#"INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
               VALUES (10, 'TV', 1, 10)"#,
        )
        .await
        .unwrap();
        pool.execute(r#"INSERT INTO "LocalLibrary" ("Id") VALUES (10)"#)
            .await
            .unwrap();
        pool.execute(
            r#"INSERT INTO "LibraryPath" ("Id", "Path", "LibraryId") VALUES (10, '/media', 10)"#,
        )
        .await
        .unwrap();

        for (id, state) in [(10, 0), (20, 0), (30, 1), (31, 0)] {
            sqlx::query(
                r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State")
                   VALUES (?, 10, ?)"#,
            )
            .bind(id)
            .bind(state)
            .execute(pool)
            .await
            .unwrap();
        }
        sqlx::query(
            r#"INSERT INTO "MediaItem" (
                   "Id", "LibraryPathId", "State", "ContentKind", "SubKind", "ParentMediaItemId"
               ) VALUES (40, 10, 0, 1, 'featurette', 20)"#,
        )
        .execute(pool)
        .await
        .unwrap();

        pool.execute(r#"INSERT INTO "Show" ("Id") VALUES (10)"#)
            .await
            .unwrap();
        pool.execute(r#"INSERT INTO "Season" ("Id", "ShowId", "SeasonNumber") VALUES (20, 10, 1)"#)
            .await
            .unwrap();
        pool.execute(r#"INSERT INTO "Episode" ("Id", "SeasonId") VALUES (30, 20), (31, 20)"#)
            .await
            .unwrap();
        pool.execute(r#"INSERT INTO "OtherVideo" ("Id") VALUES (40)"#)
            .await
            .unwrap();

        sqlx::query(
            r#"INSERT INTO "ShowMetadata" (
                   "Id", "ShowId", "MetadataKind", "Title", "SortTitle", "Year",
                   "DateAdded", "DateUpdated"
               ) VALUES (100, 10, 0, 'Example Show', 'example show', 2026, ?, ?)"#,
        )
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            r#"INSERT INTO "SeasonMetadata" (
                   "Id", "SeasonId", "MetadataKind", "Title", "SortTitle",
                   "DateAdded", "DateUpdated"
               ) VALUES (200, 20, 0, 'Season 1', 'season 1', ?, ?)"#,
        )
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            r#"INSERT INTO "EpisodeMetadata" (
                   "Id", "EpisodeId", "MetadataKind", "Title", "SortTitle",
                   "ReleaseDate", "DateAdded", "DateUpdated", "EpisodeNumber", "Plot"
               ) VALUES
                   (300, 30, 0, 'Pilot', 'pilot', '2026-05-01T00:00:00Z', ?, ?, 1, 'The beginning.'),
                   (301, 31, 0, 'Finale', 'finale', NULL, ?, ?, 2, 'The ending.')"#,
        )
        .bind(now)
        .bind(now)
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            r#"INSERT INTO "OtherVideoMetadata" (
                   "Id", "OtherVideoId", "MetadataKind", "Title", "OriginalTitle",
                   "SortTitle", "Year", "DateAdded", "DateUpdated"
               ) VALUES (
                   400, 40, 0, 'Featurette', 'Behind the scenes', 'featurette',
                   2026, ?, ?
               )"#,
        )
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .unwrap();

        sqlx::query(
            r#"INSERT INTO "Artwork" (
                   "Id", "ArtworkKind", "Path", "SeasonMetadataId", "ShowMetadataId",
                   "EpisodeMetadataId", "OtherVideoMetadataId", "DateAdded", "DateUpdated"
               ) VALUES
                   (1, 0, 'season-poster.jpg', 200, NULL, NULL, NULL, ?, ?),
                   (2, 3, 'show-fanart.jpg', NULL, 100, NULL, NULL, ?, ?),
                   (3, 1, 'pilot-thumb.jpg', NULL, NULL, 300, NULL, ?, ?),
                   (4, 1, 'featurette-thumb.jpg', NULL, NULL, NULL, 400, ?, ?)"#,
        )
        .bind(now)
        .bind(now)
        .bind(now)
        .bind(now)
        .bind(now)
        .bind(now)
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .unwrap();

        pool.execute(
            r#"INSERT INTO "Director" ("EpisodeMetadataId", "Name")
               VALUES (300, 'Director A'), (300, 'Director B')"#,
        )
        .await
        .unwrap();
        pool.execute(
            r#"INSERT INTO "Writer" ("EpisodeMetadataId", "Name")
               VALUES (300, 'Writer A')"#,
        )
        .await
        .unwrap();

        sqlx::query(
            r#"INSERT INTO "MediaVersion" (
                   "Id", "DateAdded", "DateUpdated", "Duration", "EpisodeId",
                   "Height", "Width", "VideoScanKind", "SampleAspectRatio",
                   "DisplayAspectRatio", "RFrameRate"
               ) VALUES (3000, ?, ?, '00:22:00', 30, 1080, 1920, 1, '1:1', '16:9', '24000/1001')"#,
        )
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .unwrap();
        pool.execute(
            r#"INSERT INTO "MediaFile" ("Id", "MediaVersionId", "Path", "PathHash")
               VALUES (4000, 3000, '/media/show/s01e01.mkv', 'hash')"#,
        )
        .await
        .unwrap();
        pool.execute(
            r#"INSERT INTO "MediaStream" (
                   "Id", "Index", "Codec", "Profile", "MediaStreamKind", "Language",
                   "Channels", "Title", "Default", "Forced", "MediaVersionId"
               ) VALUES
                   (5000, 0, 'h264', 'High', 1, NULL, 0, NULL, 1, 0, 3000),
                   (5001, 1, 'srt', NULL, 3, 'eng', 0, 'English', 1, 0, 3000)"#,
        )
        .await
        .unwrap();
        sqlx::query(
            r#"INSERT INTO "Subtitle" (
                   "Id", "EpisodeMetadataId", "StreamIndex", "SubtitleKind", "Title",
                   "Codec", "Language", "Default", "Forced", "Path", "IsExtracted",
                   "DateAdded", "DateUpdated"
               ) VALUES (6000, 300, 1, 0, 'English', 'srt', 'eng', 1, 0, '/cache/sub.srt', 1, ?, ?)"#,
        )
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .unwrap();
        pool.execute(
            r#"INSERT INTO "MediaChapter" (
                   "Id", "MediaVersionId", "ChapterId", "StartTime", "EndTime", "Title"
               ) VALUES (7000, 3000, 1, '00:00:00', '00:01:00', 'Opening')"#,
        )
        .await
        .unwrap();
    }
}
// ---------------------------------------------------------------------------

/// One card in the collection detail grid. `kind` is the discriminant used by
/// the UI to group cards into sections and to build `href`s.
///
/// `kind` values match those used by `suggest_media_items` and
/// `list_collection_members`: `"movie" | "show" | "season" | "episode" |
/// "artist" | "music_video" | "other_video" | "song" | "image" |
/// "remote_stream"`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CollectionItemCard {
    /// Surrogate `CollectionItem.Id` — the row identity. ADR 0048 allows the
    /// same media item to appear several times with different playback
    /// overrides, so selection/removal/reordering key on this, not on
    /// `media_item_id`.
    pub collection_item_id: i64,
    pub media_item_id: i64,
    pub kind: String,
    pub title: String,
    pub sort_title: String,
    pub subtitle: Option<String>,
    pub artwork_path: Option<String>,
    pub href: Option<String>,
    pub custom_index: i64,
    pub preferred_audio_language_code: Option<String>,
    pub preferred_audio_title: Option<String>,
    pub preferred_subtitle_language_code: Option<String>,
    pub subtitle_mode: Option<i32>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CollectionDetail {
    pub id: i64,
    pub name: String,
    pub use_custom_playback_order: bool,
    pub items: Vec<CollectionItemCard>,
}

/// Loads the full collection detail (header + all card rows). Mirrors the
/// projection performed by `GetCollectionCardsHandler` over the joined
/// `Collection` + `CollectionItem` + per-kind metadata graph.
///
/// Artwork URLs are resolved server-side via `artwork_url(...)` so the
/// returned `artwork_path` is ready to drop into `<img src=...>`.
#[server]
pub async fn get_collection_detail(id: i64) -> Result<Option<CollectionDetail>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .get_collection_detail(id)
        .await
        .map(|detail| detail.map(collection_detail_from_admin))
        .map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
fn collection_detail_from_admin(
    detail: ersatzrs_core::repositories::CollectionDetailAdmin,
) -> CollectionDetail {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};

    CollectionDetail {
        id: detail.id,
        name: detail.name,
        use_custom_playback_order: detail.use_custom_playback_order,
        items: detail
            .items
            .into_iter()
            .map(|item| {
                let href = match item.kind.as_str() {
                    "movie" => Some(format!("/media/movies/{}", item.media_item_id)),
                    "show" => Some(format!("/media/tv/shows/{}", item.media_item_id)),
                    "other_video" => Some(format!("/media/other/videos/{}", item.media_item_id)),
                    _ => None,
                };
                CollectionItemCard {
                    collection_item_id: item.collection_item_id,
                    media_item_id: item.media_item_id,
                    kind: item.kind,
                    title: item.title,
                    sort_title: item.sort_title,
                    subtitle: item.subtitle,
                    artwork_path: item
                        .artwork_path
                        .and_then(|path| artwork_url(&path, ArtworkUrlKind::Poster)),
                    href,
                    custom_index: item.custom_index,
                    preferred_audio_language_code: item.playback.preferred_audio_language_code,
                    preferred_audio_title: item.playback.preferred_audio_title,
                    preferred_subtitle_language_code: item
                        .playback
                        .preferred_subtitle_language_code,
                    subtitle_mode: item.playback.subtitle_mode,
                }
            })
            .collect(),
    }
}

#[cfg(all(test, feature = "ssr"))]
#[allow(dead_code, clippy::too_many_lines)]
async fn get_collection_detail_legacy_sql(
    id: i64,
) -> Result<Option<CollectionDetail>, ServerFnError> {
    use crate::api::artwork::{artwork_url, ArtworkUrlKind};
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();

    let Some(collection_row) = sqlx::query(
        r#"SELECT "Id",
                  COALESCE("Name", '') AS "Name",
                  COALESCE("UseCustomPlaybackOrder", 0) AS "UseCustomPlaybackOrder"
           FROM "Collection" WHERE "Id" = ? LIMIT 1"#,
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    else {
        return Ok(None);
    };

    let name: String = collection_row.try_get("Name").unwrap_or_default();
    let use_custom: i64 = collection_row
        .try_get("UseCustomPlaybackOrder")
        .unwrap_or(0);

    // Pull every CollectionItem joined with each possible per-kind metadata.
    // We do this as a single UNION ALL to keep the round-trip count to two.
    // `ArtworkKind = 0` is Poster; `1` is Thumbnail (used for artists, music
    // videos, songs, other videos, images, remote streams in ErsatzTV's
    // `MediaCard` defaults).
    let rows = sqlx::query(
        r#"
        SELECT ci."Id" AS "CollectionItemId",
               ci."MediaItemId" AS "Id",
               ci."CustomIndex" AS "CustomIndex",
               ci."PreferredAudioLanguageCode" AS "PreferredAudioLanguageCode",
               ci."PreferredAudioTitle" AS "PreferredAudioTitle",
               ci."PreferredSubtitleLanguageCode" AS "PreferredSubtitleLanguageCode",
               ci."SubtitleMode" AS "SubtitleMode",
               'movie' AS "Kind",
               COALESCE(mm."Title", '') AS "Title",
               COALESCE(mm."SortTitle", mm."Title", '') AS "SortTitle",
               CAST(mm."Year" AS TEXT) AS "Subtitle",
               (SELECT a."Path" FROM "Artwork" a
                WHERE a."MovieMetadataId" = mm."Id" AND a."ArtworkKind" = 0 LIMIT 1) AS "Artwork",
               0 AS "ArtworkKindHint"
          FROM "CollectionItem" ci
          JOIN "Movie" m ON m."Id" = ci."MediaItemId"
          JOIN "MovieMetadata" mm ON mm."MovieId" = m."Id"
         WHERE ci."CollectionId" = ?

        UNION ALL

        SELECT ci."Id" AS "CollectionItemId",
               ci."MediaItemId" AS "Id",
               ci."CustomIndex" AS "CustomIndex",
               ci."PreferredAudioLanguageCode" AS "PreferredAudioLanguageCode",
               ci."PreferredAudioTitle" AS "PreferredAudioTitle",
               ci."PreferredSubtitleLanguageCode" AS "PreferredSubtitleLanguageCode",
               ci."SubtitleMode" AS "SubtitleMode",
               'show' AS "Kind",
               COALESCE(sm."Title", '') AS "Title",
               COALESCE(sm."SortTitle", sm."Title", '') AS "SortTitle",
               CAST(sm."Year" AS TEXT) AS "Subtitle",
               (SELECT a."Path" FROM "Artwork" a
                WHERE a."ShowMetadataId" = sm."Id" AND a."ArtworkKind" = 0 LIMIT 1) AS "Artwork",
               0 AS "ArtworkKindHint"
          FROM "CollectionItem" ci
          JOIN "Show" s ON s."Id" = ci."MediaItemId"
          JOIN "ShowMetadata" sm ON sm."ShowId" = s."Id"
         WHERE ci."CollectionId" = ?

        UNION ALL

        SELECT ci."Id" AS "CollectionItemId",
               ci."MediaItemId" AS "Id",
               ci."CustomIndex" AS "CustomIndex",
               ci."PreferredAudioLanguageCode" AS "PreferredAudioLanguageCode",
               ci."PreferredAudioTitle" AS "PreferredAudioTitle",
               ci."PreferredSubtitleLanguageCode" AS "PreferredSubtitleLanguageCode",
               ci."SubtitleMode" AS "SubtitleMode",
               'season' AS "Kind",
               COALESCE(sh_meta."Title", '') AS "Title",
               COALESCE(sh_meta."SortTitle", sh_meta."Title", '') AS "SortTitle",
               COALESCE(sea_meta."Title", 'Season ' || CAST(se."SeasonNumber" AS TEXT)) AS "Subtitle",
               (SELECT a."Path" FROM "Artwork" a
                WHERE a."SeasonMetadataId" = sea_meta."Id" AND a."ArtworkKind" = 0 LIMIT 1) AS "Artwork",
               0 AS "ArtworkKindHint"
          FROM "CollectionItem" ci
          JOIN "Season" se ON se."Id" = ci."MediaItemId"
          LEFT JOIN "SeasonMetadata" sea_meta ON sea_meta."SeasonId" = se."Id"
          JOIN "Show" sh ON sh."Id" = se."ShowId"
          JOIN "ShowMetadata" sh_meta ON sh_meta."ShowId" = sh."Id"
         WHERE ci."CollectionId" = ?

        UNION ALL

        SELECT ci."Id" AS "CollectionItemId",
               ci."MediaItemId" AS "Id",
               ci."CustomIndex" AS "CustomIndex",
               ci."PreferredAudioLanguageCode" AS "PreferredAudioLanguageCode",
               ci."PreferredAudioTitle" AS "PreferredAudioTitle",
               ci."PreferredSubtitleLanguageCode" AS "PreferredSubtitleLanguageCode",
               ci."SubtitleMode" AS "SubtitleMode",
               'episode' AS "Kind",
               COALESCE(em."Title", '') AS "Title",
               COALESCE(em."SortTitle", em."Title", '') AS "SortTitle",
               sh_meta."Title" || ' - S' || CAST(COALESCE(se."SeasonNumber", 0) AS TEXT)
                                || ' E' || CAST(COALESCE(em."EpisodeNumber", 0) AS TEXT) AS "Subtitle",
               (SELECT a."Path" FROM "Artwork" a
                WHERE a."EpisodeMetadataId" = em."Id" AND a."ArtworkKind" = 0 LIMIT 1) AS "Artwork",
               0 AS "ArtworkKindHint"
          FROM "CollectionItem" ci
          JOIN "Episode" ep ON ep."Id" = ci."MediaItemId"
          JOIN "EpisodeMetadata" em ON em."EpisodeId" = ep."Id"
          JOIN "Season" se ON se."Id" = ep."SeasonId"
          JOIN "Show" sh ON sh."Id" = se."ShowId"
          JOIN "ShowMetadata" sh_meta ON sh_meta."ShowId" = sh."Id"
         WHERE ci."CollectionId" = ?

        UNION ALL

        SELECT ci."Id" AS "CollectionItemId",
               ci."MediaItemId" AS "Id",
               ci."CustomIndex" AS "CustomIndex",
               ci."PreferredAudioLanguageCode" AS "PreferredAudioLanguageCode",
               ci."PreferredAudioTitle" AS "PreferredAudioTitle",
               ci."PreferredSubtitleLanguageCode" AS "PreferredSubtitleLanguageCode",
               ci."SubtitleMode" AS "SubtitleMode",
               'artist' AS "Kind",
               COALESCE(am."Title", '') AS "Title",
               COALESCE(am."SortTitle", am."Title", '') AS "SortTitle",
               am."Disambiguation" AS "Subtitle",
               (SELECT a."Path" FROM "Artwork" a
                WHERE a."ArtistMetadataId" = am."Id" AND a."ArtworkKind" = 1 LIMIT 1) AS "Artwork",
               1 AS "ArtworkKindHint"
          FROM "CollectionItem" ci
          JOIN "Artist" ar ON ar."Id" = ci."MediaItemId"
          JOIN "ArtistMetadata" am ON am."ArtistId" = ar."Id"
         WHERE ci."CollectionId" = ?

        UNION ALL

        SELECT ci."Id" AS "CollectionItemId",
               ci."MediaItemId" AS "Id",
               ci."CustomIndex" AS "CustomIndex",
               ci."PreferredAudioLanguageCode" AS "PreferredAudioLanguageCode",
               ci."PreferredAudioTitle" AS "PreferredAudioTitle",
               ci."PreferredSubtitleLanguageCode" AS "PreferredSubtitleLanguageCode",
               ci."SubtitleMode" AS "SubtitleMode",
               'music_video' AS "Kind",
               COALESCE(mvm."Title", '') AS "Title",
               COALESCE(mvm."SortTitle", mvm."Title", '') AS "SortTitle",
               mvm."Album" AS "Subtitle",
               (SELECT a."Path" FROM "Artwork" a
                WHERE a."MusicVideoMetadataId" = mvm."Id" AND a."ArtworkKind" = 1 LIMIT 1) AS "Artwork",
               1 AS "ArtworkKindHint"
          FROM "CollectionItem" ci
          JOIN "MusicVideo" mv ON mv."Id" = ci."MediaItemId"
          JOIN "MusicVideoMetadata" mvm ON mvm."MusicVideoId" = mv."Id"
         WHERE ci."CollectionId" = ?

        UNION ALL

        SELECT ci."Id" AS "CollectionItemId",
               ci."MediaItemId" AS "Id",
               ci."CustomIndex" AS "CustomIndex",
               ci."PreferredAudioLanguageCode" AS "PreferredAudioLanguageCode",
               ci."PreferredAudioTitle" AS "PreferredAudioTitle",
               ci."PreferredSubtitleLanguageCode" AS "PreferredSubtitleLanguageCode",
               ci."SubtitleMode" AS "SubtitleMode",
               'other_video' AS "Kind",
               COALESCE(ovm."Title", '') AS "Title",
               COALESCE(ovm."SortTitle", ovm."Title", '') AS "SortTitle",
               NULL AS "Subtitle",
               (SELECT a."Path" FROM "Artwork" a
                WHERE a."OtherVideoMetadataId" = ovm."Id" AND a."ArtworkKind" = 1 LIMIT 1) AS "Artwork",
               1 AS "ArtworkKindHint"
          FROM "CollectionItem" ci
          JOIN "OtherVideo" ov ON ov."Id" = ci."MediaItemId"
          JOIN "OtherVideoMetadata" ovm ON ovm."OtherVideoId" = ov."Id"
         WHERE ci."CollectionId" = ?

        UNION ALL

        SELECT ci."Id" AS "CollectionItemId",
               ci."MediaItemId" AS "Id",
               ci."CustomIndex" AS "CustomIndex",
               ci."PreferredAudioLanguageCode" AS "PreferredAudioLanguageCode",
               ci."PreferredAudioTitle" AS "PreferredAudioTitle",
               ci."PreferredSubtitleLanguageCode" AS "PreferredSubtitleLanguageCode",
               ci."SubtitleMode" AS "SubtitleMode",
               'song' AS "Kind",
               COALESCE(sgm."Title", '') AS "Title",
               COALESCE(sgm."SortTitle", sgm."Title", '') AS "SortTitle",
               sgm."Album" AS "Subtitle",
               (SELECT a."Path" FROM "Artwork" a
                WHERE a."SongMetadataId" = sgm."Id" AND a."ArtworkKind" = 1 LIMIT 1) AS "Artwork",
               1 AS "ArtworkKindHint"
          FROM "CollectionItem" ci
          JOIN "Song" sg ON sg."Id" = ci."MediaItemId"
          JOIN "SongMetadata" sgm ON sgm."SongId" = sg."Id"
         WHERE ci."CollectionId" = ?

        UNION ALL

        SELECT ci."Id" AS "CollectionItemId",
               ci."MediaItemId" AS "Id",
               ci."CustomIndex" AS "CustomIndex",
               ci."PreferredAudioLanguageCode" AS "PreferredAudioLanguageCode",
               ci."PreferredAudioTitle" AS "PreferredAudioTitle",
               ci."PreferredSubtitleLanguageCode" AS "PreferredSubtitleLanguageCode",
               ci."SubtitleMode" AS "SubtitleMode",
               'image' AS "Kind",
               COALESCE(im."Title", '') AS "Title",
               COALESCE(im."SortTitle", im."Title", '') AS "SortTitle",
               NULL AS "Subtitle",
               (SELECT a."Path" FROM "Artwork" a
                WHERE a."ImageMetadataId" = im."Id" AND a."ArtworkKind" = 1 LIMIT 1) AS "Artwork",
               1 AS "ArtworkKindHint"
          FROM "CollectionItem" ci
          JOIN "Image" img ON img."Id" = ci."MediaItemId"
          JOIN "ImageMetadata" im ON im."ImageId" = img."Id"
         WHERE ci."CollectionId" = ?

        UNION ALL

        SELECT ci."Id" AS "CollectionItemId",
               ci."MediaItemId" AS "Id",
               ci."CustomIndex" AS "CustomIndex",
               ci."PreferredAudioLanguageCode" AS "PreferredAudioLanguageCode",
               ci."PreferredAudioTitle" AS "PreferredAudioTitle",
               ci."PreferredSubtitleLanguageCode" AS "PreferredSubtitleLanguageCode",
               ci."SubtitleMode" AS "SubtitleMode",
               'remote_stream' AS "Kind",
               COALESCE(rsm."Title", '') AS "Title",
               COALESCE(rsm."SortTitle", rsm."Title", '') AS "SortTitle",
               NULL AS "Subtitle",
               (SELECT a."Path" FROM "Artwork" a
                WHERE a."RemoteStreamMetadataId" = rsm."Id" AND a."ArtworkKind" = 1 LIMIT 1) AS "Artwork",
               1 AS "ArtworkKindHint"
          FROM "CollectionItem" ci
          JOIN "RemoteStream" rs ON rs."Id" = ci."MediaItemId"
          JOIN "RemoteStreamMetadata" rsm ON rsm."RemoteStreamId" = rs."Id"
         WHERE ci."CollectionId" = ?

        ORDER BY "Kind", "SortTitle", "Title", "CollectionItemId"
        "#,
    )
    .bind(id)
    .bind(id)
    .bind(id)
    .bind(id)
    .bind(id)
    .bind(id)
    .bind(id)
    .bind(id)
    .bind(id)
    .bind(id)
    .fetch_all(&state.pool)
    .await?;

    let items: Vec<CollectionItemCard> = rows
        .iter()
        .map(|r| {
            let kind: String = r.try_get("Kind").unwrap_or_default();
            let item_id: i64 = r.try_get("Id").unwrap_or(0);
            // ErsatzTV's `MediaCard` defaults to poster artwork; for artists,
            // music videos, etc. the card explicitly opts into thumbnail
            // artwork. We don't yet have a `Thumbnail` variant in
            // `ArtworkUrlKind` (it maps to the same posters cache anyway),
            // so reuse `Poster` for both — the underlying `ArtworkController`
            // routes by stored hash, not by the requested kind.
            let _ = r.try_get::<i64, _>("ArtworkKindHint").unwrap_or(0);
            let artwork_path = r
                .try_get::<Option<String>, _>("Artwork")
                .unwrap_or(None)
                .and_then(|p| artwork_url(&p, ArtworkUrlKind::Poster));

            let href = match kind.as_str() {
                "movie" => Some(format!("/media/movies/{item_id}")),
                "show" => Some(format!("/media/tv/shows/{item_id}")),
                "other_video" => Some(format!("/media/other/videos/{item_id}")),
                _ => None,
            };

            CollectionItemCard {
                collection_item_id: r.try_get("CollectionItemId").unwrap_or(0),
                media_item_id: item_id,
                kind,
                title: r.try_get("Title").unwrap_or_default(),
                sort_title: r.try_get("SortTitle").unwrap_or_default(),
                subtitle: r.try_get::<Option<String>, _>("Subtitle").unwrap_or(None),
                artwork_path,
                href,
                custom_index: r.try_get("CustomIndex").unwrap_or(0),
                preferred_audio_language_code: r
                    .try_get("PreferredAudioLanguageCode")
                    .unwrap_or_default(),
                preferred_audio_title: r.try_get("PreferredAudioTitle").unwrap_or_default(),
                preferred_subtitle_language_code: r
                    .try_get("PreferredSubtitleLanguageCode")
                    .unwrap_or_default(),
                subtitle_mode: r.try_get("SubtitleMode").unwrap_or_default(),
            }
        })
        .collect();

    Ok(Some(CollectionDetail {
        id,
        name,
        use_custom_playback_order: use_custom != 0,
        items,
    }))
}

/// Toggles `Collection.UseCustomPlaybackOrder`. Mirrors the
/// `UseCustomPlaybackOrder` branch of `UpdateCollectionHandler`.
#[server]
pub async fn set_collection_use_custom_order(id: i64, on: bool) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    state
        .application
        .set_collection_custom_order(ersatzrs_app::commands::SetCollectionCustomOrder {
            collection_id: id,
            enabled: on,
        })
        .await
        .map_err(ServerFnError::new)
}

/// Removes one or many rows from a collection. Mirrors
/// `RemoveItemsFromCollectionHandler`, but keys on the surrogate
/// `CollectionItem.Id` so an ADR 0048 duplicate row (same media item,
/// different playback override) can be removed without touching its
/// siblings.
#[server]
pub async fn remove_items_from_collection(
    collection_id: i64,
    collection_item_ids: Vec<i64>,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    state
        .application
        .remove_collection_rows(ersatzrs_app::commands::RemoveCollectionRows {
            collection_id,
            row_ids: collection_item_ids,
        })
        .await
        .map_err(ServerFnError::new)
}

/// Persists the manual playback order. Mirrors
/// `UpdateCollectionCustomOrderHandler` — writes `CustomIndex` based on
/// position in `ordered_collection_item_ids` (0-based, matching the C#
/// handler's input), keyed on the surrogate `CollectionItem.Id` so ADR 0048
/// duplicate rows order independently.
#[server]
pub async fn update_collection_custom_order(
    collection_id: i64,
    ordered_collection_item_ids: Vec<i64>,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    state
        .application
        .update_collection_custom_order(ersatzrs_app::commands::UpdateCollectionCustomOrder {
            collection_id,
            ordered_row_ids: ordered_collection_item_ids,
        })
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn list_collection_members(
    collection_id: i64,
) -> Result<Vec<CollectionMember>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state
        .application
        .list_collection_members(collection_id)
        .await
        .map_err(ServerFnError::new)?
        .into_iter()
        .map(|row| CollectionMember {
            media_item_id: row.media_item_id,
            title: row.title,
            kind: row.kind,
        })
        .collect())
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;
    use crate::admin::server_fns::search::SearchAllIds;
    use ersatzrs_core::repositories::CollectionRepository;

    #[test]
    fn variant_browser_query_disables_the_search_contract_primary_only_default() {
        use ersatzrs_core::search::{SearchDocument, SearchExpression};

        let extra = SearchDocument {
            media_item_id: 40,
            media_kind: "other_video".to_owned(),
            content_kind: ersatzrs_domain::ContentKind::Extra,
            ..SearchDocument::default()
        };

        assert!(
            !SearchExpression::parse(&media_card_search_query("other_video", "", false,))
                .matches(&extra)
        );
        assert!(
            SearchExpression::parse(&media_card_search_query("other_video", "", true,))
                .matches(&extra)
        );
    }

    #[test]
    fn multi_collection_transport_mapping_preserves_item_fields() {
        let item = MultiCollectionItemForm {
            collection_id: Some(3),
            smart_collection_id: None,
            name: "Collection".to_owned(),
            collection_type: 0,
            use_custom_playback_order: true,
            schedule_as_group: true,
            playback_order: 2,
        };

        assert_eq!(
            multi_collection_item_form(multi_collection_item(item.clone())),
            item
        );
    }

    #[test]
    fn rerun_collection_transport_mapping_preserves_form_fields() {
        let form = RerunCollectionForm {
            id: 4,
            name: "Reruns".to_owned(),
            collection_type: 5,
            collection_id: None,
            media_item_id: None,
            multi_collection_id: None,
            smart_collection_id: Some(9),
            first_run_playback_order: 2,
            rerun_playback_order: 3,
        };

        assert_eq!(
            rerun_collection_form(rerun_collection_admin(form.clone())),
            form
        );
    }

    #[tokio::test]
    async fn save_collection_with_state_creates_manual_collection() {
        let (_database_dir, pool) = collection_test_pool("collection-create.db").await;
        let state = collection_test_state(pool.clone());

        let id = save_collection_with_state(
            &state,
            CollectionForm {
                id: 0,
                name: "  New Collection  ".to_string(),
                query: String::new(),
            },
        )
        .await
        .expect("create collection");

        let row: (String, i64) = sqlx::query_as(
            r#"SELECT "Name", "UseCustomPlaybackOrder" FROM "Collection" WHERE "Id" = ?"#,
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .expect("load created collection");

        assert_eq!(row.0, "New Collection");
        assert_eq!(row.1, 0);
    }

    #[tokio::test]
    async fn save_collection_with_state_rejects_case_insensitive_duplicate_name() {
        let (_database_dir, pool) = collection_test_pool("collection-duplicate.db").await;
        sqlx::query(
            r#"INSERT INTO "Collection" ("Id", "Name", "UseCustomPlaybackOrder")
               VALUES (1, 'Favorites', 0)"#,
        )
        .execute(&pool)
        .await
        .expect("seed collection");
        let state = collection_test_state(pool);

        let error = save_collection_with_state(
            &state,
            CollectionForm {
                id: 0,
                name: "favorites".to_string(),
                query: String::new(),
            },
        )
        .await
        .expect_err("duplicate collection name should fail");

        assert!(error.to_string().contains("Collection name must be unique"));
    }

    #[tokio::test]
    async fn save_collection_with_state_updates_existing_collection() {
        let (_database_dir, pool) = collection_test_pool("collection-update.db").await;
        sqlx::query(
            r#"INSERT INTO "Collection" ("Id", "Name", "UseCustomPlaybackOrder")
               VALUES (1, 'Old Name', 0)"#,
        )
        .execute(&pool)
        .await
        .expect("seed collection");
        let state = collection_test_state(pool.clone());

        let id = save_collection_with_state(
            &state,
            CollectionForm {
                id: 1,
                name: "New Name".to_string(),
                query: String::new(),
            },
        )
        .await
        .expect("update collection");

        let name: String = sqlx::query_scalar(r#"SELECT "Name" FROM "Collection" WHERE "Id" = 1"#)
            .fetch_one(&pool)
            .await
            .expect("load updated collection");

        assert_eq!(id, 1);
        assert_eq!(name, "New Name");
    }

    #[tokio::test]
    async fn save_smart_collection_with_state_updates_and_refreshes_playouts() {
        let (_database_dir, pool) = collection_test_pool("smart-collection-update.db").await;
        seed_playout_refresh_fixture(&pool).await;
        let (state, mut playout_rx) = collection_test_state_with_playout_rx(pool.clone());

        let id = save_smart_collection_with_state(
            &state,
            CollectionForm {
                id: 40,
                name: "Updated Smart".to_string(),
                query: "movie".to_string(),
            },
        )
        .await
        .expect("update smart collection");

        let name: String =
            sqlx::query_scalar(r#"SELECT "Name" FROM "SmartCollection" WHERE "Id" = 40"#)
                .fetch_one(&pool)
                .await
                .expect("load updated smart collection");
        let first_refresh = playout_rx.recv().await.expect("first playout refresh");
        let second_refresh = playout_rx.recv().await.expect("second playout refresh");

        assert_eq!(id, 40);
        assert_eq!(name, "Updated Smart");
        assert_eq!(first_refresh.playout_id, 100);
        assert_eq!(second_refresh.playout_id, 101);
        assert_eq!(
            first_refresh.mode,
            ersatzrs_core::scheduling::PlayoutBuildMode::Refresh
        );
        assert_eq!(
            second_refresh.mode,
            ersatzrs_core::scheduling::PlayoutBuildMode::Refresh
        );
    }

    #[tokio::test]
    async fn load_show_languages_resolves_episode_audio_stream_codes() {
        let database_dir = tempfile::tempdir().expect("temp database dir");
        let database_url = format!(
            "sqlite:{}",
            database_dir.path().join("show-languages.db").display()
        );
        let pool = ersatzrs_infra::db::init(&database_url)
            .await
            .expect("initialize test database");

        seed_show_language_fixture(&pool).await;

        let languages = load_show_languages(&pool, 10)
            .await
            .expect("load show languages");

        assert_eq!(languages, vec!["Dutch".to_string(), "English".to_string()]);
    }

    #[tokio::test]
    async fn load_movie_languages_resolves_head_version_audio_stream_codes() {
        let database_dir = tempfile::tempdir().expect("temp database dir");
        let database_url = format!(
            "sqlite:{}",
            database_dir.path().join("movie-languages.db").display()
        );
        let pool = ersatzrs_infra::db::init(&database_url)
            .await
            .expect("initialize test database");

        seed_movie_language_fixture(&pool).await;

        let languages = load_movie_languages(&pool, 10)
            .await
            .expect("load movie languages");

        assert_eq!(languages, vec!["Dutch".to_string(), "English".to_string()]);
    }

    #[tokio::test]
    async fn load_movie_metadata_facts_resolves_studios_genres_and_tags() {
        let database_dir = tempfile::tempdir().expect("temp database dir");
        let database_url = format!(
            "sqlite:{}",
            database_dir.path().join("movie-facts.db").display()
        );
        let pool = ersatzrs_infra::db::init(&database_url)
            .await
            .expect("initialize test database");

        seed_movie_metadata_fact_fixture(&pool).await;

        let studios = load_strings(&pool, r#""Studio""#, "MovieMetadataId", 100)
            .await
            .expect("load studios");
        let genres = load_strings(&pool, r#""Genre""#, "MovieMetadataId", 100)
            .await
            .expect("load genres");
        let tags = load_strings(&pool, r#""Tag""#, "MovieMetadataId", 100)
            .await
            .expect("load tags");

        assert_eq!(
            studios,
            vec!["A Studio".to_string(), "Z Studio".to_string()]
        );
        assert_eq!(
            genres,
            vec!["Drama".to_string(), "Science Fiction".to_string()]
        );
        assert_eq!(tags, vec!["Cult".to_string(), "Director's Cut".to_string()]);
    }

    #[tokio::test]
    async fn movie_detail_loads_extras_versions_and_audio_tracks() {
        let (_database_dir, pool) = collection_test_pool("movie-detail-variants.db").await;
        seed_movie_detail_variant_fixture(&pool).await;

        let movie = get_movie_details_with_pool(&pool, 10)
            .await
            .expect("load movie details")
            .expect("movie exists");

        assert_eq!(movie.title, "Example Movie");
        assert_eq!(movie.extras.len(), 1);
        assert_eq!(movie.extras[0].id, 30);
        assert_eq!(movie.extras[0].title, "Behind the Scenes");
        assert_eq!(
            movie.extras[0].variant.content_kind,
            ersatzrs_domain::ContentKind::Extra
        );
        assert_eq!(
            movie.extras[0].variant.sub_kind.as_deref(),
            Some("featurette")
        );

        assert_eq!(movie.alternate_versions.len(), 1);
        assert_eq!(movie.alternate_versions[0].id, 11);
        assert_eq!(
            movie.alternate_versions[0].title,
            "Example Movie Director's Cut"
        );
        assert_eq!(
            movie.alternate_versions[0].variant.content_kind,
            ersatzrs_domain::ContentKind::AlternateVersion
        );
        assert_eq!(
            movie.alternate_versions[0].variant.sub_kind.as_deref(),
            Some("directors_cut")
        );
        assert_eq!(
            movie.alternate_versions[0].variant.parent_title.as_deref(),
            Some("Example Movie")
        );

        assert_eq!(movie.audio_tracks.len(), 2);
        assert_eq!(movie.audio_tracks[0].media_version_id, 1000);
        assert_eq!(movie.audio_tracks[0].stream_index, 1);
        assert_eq!(movie.audio_tracks[0].title, "Main Audio");
        assert!(movie.audio_tracks[0].is_default);
        assert!(!movie.audio_tracks[0].is_commentary);
        assert_eq!(movie.audio_tracks[1].media_version_id, 1000);
        assert_eq!(movie.audio_tracks[1].stream_index, 2);
        assert_eq!(movie.audio_tracks[1].title, "Director Commentary");
        assert_eq!(movie.audio_tracks[1].language.as_deref(), Some("eng"));
        assert!(movie.audio_tracks[1].is_commentary);

        assert_eq!(movie.commentary_subtitles.len(), 1);
        assert_eq!(movie.commentary_subtitles[0].id, 1200);
        assert_eq!(movie.commentary_subtitles[0].stream_index, 3);
        assert_eq!(
            movie.commentary_subtitles[0].title,
            "Director Commentary Subtitles"
        );
        assert_eq!(
            movie.commentary_subtitles[0].language.as_deref(),
            Some("eng")
        );
    }

    #[tokio::test]
    async fn movie_extra_collection_and_playlist_actions_use_other_video_type() {
        let (_database_dir, pool) = collection_test_pool("movie-extra-actions.db").await;
        seed_movie_detail_variant_fixture(&pool).await;
        let state = collection_test_state(pool.clone());
        let owner = leptos::prelude::Owner::new();
        owner.with(|| leptos::prelude::provide_context(state));
        owner.set();

        let collection_added = add_media_items_to_collection(900, vec![30])
            .await
            .expect("add extra to collection");
        let playlist_added =
            add_media_items_to_playlist(MediaBrowserKind::OtherVideo, 902, vec![30])
                .await
                .expect("add extra to playlist");

        let collection_count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM "CollectionItem"
               WHERE "CollectionId" = 900 AND "MediaItemId" = 30"#,
        )
        .fetch_one(&pool)
        .await
        .expect("load collection item count");
        let collection_type: i64 = sqlx::query_scalar(
            r#"SELECT "CollectionType" FROM "PlaylistItem"
               WHERE "PlaylistId" = 902 AND "MediaItemId" = 30"#,
        )
        .fetch_one(&pool)
        .await
        .expect("load playlist item collection type");

        assert_eq!(collection_added, 1);
        assert_eq!(playlist_added, 1);
        assert_eq!(collection_count, 1);
        assert_eq!(collection_type, 40);

        owner.cleanup();
    }

    #[tokio::test]
    async fn media_item_collection_and_playlist_actions_persist_playback_overrides() {
        let (_database_dir, pool) = collection_test_pool("movie-playback-overrides.db").await;
        seed_movie_detail_variant_fixture(&pool).await;
        let state = collection_test_state(pool.clone());
        let owner = leptos::prelude::Owner::new();
        owner.with(|| leptos::prelude::provide_context(state));
        owner.set();

        let playback = PlaybackOverrideForm {
            preferred_audio_language_code: Some("eng".to_string()),
            preferred_audio_title: Some("Director Commentary".to_string()),
            preferred_subtitle_language_code: Some("eng".to_string()),
            subtitle_mode: Some(3),
        };
        let collection_added =
            add_media_items_to_collection_with_playback(900, vec![10], Some(playback.clone()))
                .await
                .expect("add movie to collection with playback override");
        let playlist_added = add_media_items_to_playlist_with_playback(
            MediaBrowserKind::Movie,
            902,
            vec![10],
            Some(playback),
        )
        .await
        .expect("add movie to playlist with playback override");

        let collection_row = sqlx::query(
            r#"SELECT "PreferredAudioLanguageCode", "PreferredAudioTitle",
                      "PreferredSubtitleLanguageCode", "SubtitleMode"
               FROM "CollectionItem"
               WHERE "CollectionId" = 900 AND "MediaItemId" = 10"#,
        )
        .fetch_one(&pool)
        .await
        .expect("load collection item override");
        let playlist_row = sqlx::query(
            r#"SELECT "PreferredAudioLanguageCode", "PreferredAudioTitle",
                      "PreferredSubtitleLanguageCode", "SubtitleMode"
               FROM "PlaylistItem"
               WHERE "PlaylistId" = 902 AND "MediaItemId" = 10"#,
        )
        .fetch_one(&pool)
        .await
        .expect("load playlist item override");

        assert_eq!(collection_added, 1);
        assert_eq!(playlist_added, 1);
        assert_playback_override_row(&collection_row);
        assert_playback_override_row(&playlist_row);

        let collection_detail = get_collection_detail(900)
            .await
            .expect("load collection detail")
            .expect("collection exists");
        let collection_card = collection_detail
            .items
            .iter()
            .find(|item| item.media_item_id == 10)
            .expect("collection item card exists");
        assert_eq!(
            collection_card.preferred_audio_title.as_deref(),
            Some("Director Commentary")
        );
        assert_eq!(
            collection_card.preferred_subtitle_language_code.as_deref(),
            Some("eng")
        );
        assert_eq!(collection_card.subtitle_mode, Some(3));

        owner.cleanup();
    }

    #[tokio::test]
    async fn duplicate_collection_rows_with_distinct_overrides_persist_and_remove_independently() {
        let (_database_dir, pool) = collection_test_pool("collection-duplicate-rows.db").await;
        seed_movie_detail_variant_fixture(&pool).await;
        let state = collection_test_state(pool.clone());
        let owner = leptos::prelude::Owner::new();
        owner.with(|| leptos::prelude::provide_context(state));
        owner.set();

        // Default add is idempotent.
        let default_added = add_media_items_to_collection(900, vec![10])
            .await
            .expect("add default row");
        let default_repeat = add_media_items_to_collection(900, vec![10])
            .await
            .expect("repeat default add");

        // Same movie again with two distinct commentary overrides — both must
        // persist as separate rows; an identical override tuple is skipped.
        let commentary_a = PlaybackOverrideForm {
            preferred_audio_title: Some("Director Commentary".to_string()),
            ..PlaybackOverrideForm::default()
        };
        let commentary_b = PlaybackOverrideForm {
            preferred_audio_title: Some("Isolated Score".to_string()),
            ..PlaybackOverrideForm::default()
        };
        let a_added =
            add_media_items_to_collection_with_playback(900, vec![10], Some(commentary_a.clone()))
                .await
                .expect("add commentary A row");
        let a_repeat =
            add_media_items_to_collection_with_playback(900, vec![10], Some(commentary_a))
                .await
                .expect("repeat commentary A add");
        let b_added =
            add_media_items_to_collection_with_playback(900, vec![10], Some(commentary_b))
                .await
                .expect("add commentary B row");

        assert_eq!(default_added, 1);
        assert_eq!(default_repeat, 0);
        assert_eq!(a_added, 1);
        assert_eq!(a_repeat, 0);
        assert_eq!(b_added, 1);

        // Collection detail projects one card per row with distinct row ids
        // and each card's own override summary fields.
        let detail = get_collection_detail(900)
            .await
            .expect("load collection detail")
            .expect("collection exists");
        let movie_cards: Vec<_> = detail
            .items
            .iter()
            .filter(|card| card.media_item_id == 10)
            .collect();
        assert_eq!(movie_cards.len(), 3, "one card per duplicate row");
        let mut row_ids: Vec<i64> = movie_cards
            .iter()
            .map(|card| card.collection_item_id)
            .collect();
        row_ids.sort_unstable();
        row_ids.dedup();
        assert_eq!(row_ids.len(), 3, "row ids must be distinct");
        assert!(movie_cards
            .iter()
            .any(|card| card.preferred_audio_title.is_none()));
        assert!(movie_cards
            .iter()
            .any(|card| card.preferred_audio_title.as_deref() == Some("Director Commentary")));

        // Removing one duplicate row leaves its siblings untouched.
        let commentary_a_row: i64 = sqlx::query_scalar(
            r#"SELECT "Id" FROM "CollectionItem"
               WHERE "CollectionId" = 900 AND "MediaItemId" = 10
                 AND "PreferredAudioTitle" = 'Director Commentary'"#,
        )
        .fetch_one(&pool)
        .await
        .expect("load commentary A row id");
        remove_items_from_collection(900, vec![commentary_a_row])
            .await
            .expect("remove commentary A row");

        let remaining: Vec<Option<String>> = sqlx::query_scalar(
            r#"SELECT "PreferredAudioTitle" FROM "CollectionItem"
               WHERE "CollectionId" = 900 AND "MediaItemId" = 10
               ORDER BY "Id""#,
        )
        .fetch_all(&pool)
        .await
        .expect("load remaining rows");
        assert_eq!(
            remaining,
            vec![None, Some("Isolated Score".to_string())],
            "only the removed duplicate row disappears"
        );

        owner.cleanup();
    }

    fn assert_playback_override_row(row: &ersatzrs_infra::db::DbRow) {
        use sqlx::Row;

        assert_eq!(
            row.try_get::<Option<String>, _>("PreferredAudioLanguageCode")
                .unwrap()
                .as_deref(),
            Some("eng")
        );
        assert_eq!(
            row.try_get::<Option<String>, _>("PreferredAudioTitle")
                .unwrap()
                .as_deref(),
            Some("Director Commentary")
        );
        assert_eq!(
            row.try_get::<Option<String>, _>("PreferredSubtitleLanguageCode")
                .unwrap()
                .as_deref(),
            Some("eng")
        );
        assert_eq!(
            row.try_get::<Option<i32>, _>("SubtitleMode").unwrap(),
            Some(3)
        );
    }

    #[tokio::test]
    async fn load_artist_languages_resolves_music_video_audio_stream_codes() {
        let database_dir = tempfile::tempdir().expect("temp database dir");
        let database_url = format!(
            "sqlite:{}",
            database_dir.path().join("artist-languages.db").display()
        );
        let pool = ersatzrs_infra::db::init(&database_url)
            .await
            .expect("initialize test database");

        seed_artist_language_fixture(&pool).await;

        let languages = load_artist_languages(&pool, 10)
            .await
            .expect("load artist languages");

        assert_eq!(languages, vec!["Dutch".to_string(), "English".to_string()]);
    }

    #[tokio::test]
    async fn load_other_video_languages_resolves_head_version_audio_stream_codes() {
        let database_dir = tempfile::tempdir().expect("temp database dir");
        let database_url = format!(
            "sqlite:{}",
            database_dir
                .path()
                .join("other-video-languages.db")
                .display()
        );
        let pool = ersatzrs_infra::db::init(&database_url)
            .await
            .expect("initialize test database");

        seed_other_video_media_info_fixture(&pool).await;

        let languages = load_other_video_languages(&pool, 10)
            .await
            .expect("load other-video languages");

        assert_eq!(languages, vec!["Dutch".to_string(), "English".to_string()]);
    }

    #[tokio::test]
    async fn media_item_info_json_supports_other_videos() {
        let database_dir = tempfile::tempdir().expect("temp database dir");
        let database_url = format!(
            "sqlite:{}",
            database_dir.path().join("other-video-info.db").display()
        );
        let pool = ersatzrs_infra::db::init(&database_url)
            .await
            .expect("initialize test database");

        seed_other_video_media_info_fixture(&pool).await;

        let json = get_media_item_info_json_with_pool(&pool, 10)
            .await
            .expect("load other-video media info");
        let value: serde_json::Value =
            serde_json::from_str(&json).expect("media info should be JSON");

        assert_eq!(value["Kind"], "OtherVideo");
        assert_eq!(value["Title"], "Example Other Video");
        assert_eq!(value["LibraryKind"], "LocalLibrary");
        assert_eq!(value["Streams"][0]["Kind"], "Video");
        assert_eq!(value["Streams"][1]["Language"], "eng");
        let subtitle_stream = value["Streams"]
            .as_array()
            .expect("streams should be an array")
            .iter()
            .find(|stream| stream["Kind"] == "Subtitle")
            .expect("embedded subtitle stream");
        assert_eq!(subtitle_stream["IsExtracted"], true);
        assert_eq!(subtitle_stream["FileName"], "/cache/other.srt");
        assert_eq!(value["Chapters"][0]["Title"], "Opening");
    }

    #[tokio::test]
    async fn playout_refresh_lookups_match_schedule_and_collection_contracts() {
        let database_dir = tempfile::tempdir().expect("temp database dir");
        let database_url = format!(
            "sqlite:{}",
            database_dir
                .path()
                .join("playout-refresh-lookups.db")
                .display()
        );
        let pool = ersatzrs_infra::db::init(&database_url)
            .await
            .expect("initialize test database");

        seed_playout_refresh_fixture(&pool).await;

        assert_eq!(
            playout_ids_using_schedule(&pool, 10).await.unwrap(),
            vec![100, 101]
        );
        assert_eq!(
            playout_ids_using_collection(&pool, 20).await.unwrap(),
            vec![101, 102]
        );
        assert_eq!(
            ersatzrs_infra::repositories::SqliteCollectionRepository(pool.clone())
                .playout_ids_using_smart(40)
                .await
                .unwrap(),
            vec![100, 101]
        );
        assert_eq!(
            ersatzrs_infra::repositories::SqliteCollectionRepository(pool.clone())
                .playout_ids_using_rerun(30)
                .await
                .unwrap(),
            vec![100, 102]
        );
    }

    #[test]
    fn plan_mixed_playlist_inserts_is_empty_for_empty_payload() {
        let ids = SearchAllIds::default();
        assert!(plan_mixed_playlist_inserts(0, &ids).is_empty());
    }

    #[test]
    fn plan_mixed_playlist_inserts_stamps_collection_type_per_kind() {
        let ids = SearchAllIds {
            movie_ids: vec![101],
            show_ids: vec![201],
            season_ids: vec![301],
            episode_ids: vec![401],
            artist_ids: vec![501],
            music_video_ids: vec![601],
            other_video_ids: vec![701],
            song_ids: vec![801],
            image_ids: vec![901],
            remote_stream_ids: vec![1001],
        };

        let plan = plan_mixed_playlist_inserts(0, &ids);

        // (collection_type, media_item_id) in ErsatzTV iteration order.
        let observed: Vec<(i32, i64)> = plan
            .iter()
            .map(|row| (row.collection_type, row.media_item_id))
            .collect();
        assert_eq!(
            observed,
            vec![
                (10, 101),  // Movie
                (1, 201),   // TelevisionShow
                (2, 301),   // TelevisionSeason
                (20, 401),  // Episode
                (3, 501),   // Artist
                (30, 601),  // MusicVideo
                (40, 701),  // OtherVideo
                (50, 801),  // Song
                (60, 901),  // Image
                (70, 1001), // RemoteStream
            ]
        );
    }

    #[test]
    fn media_browser_kind_stamps_song_and_image_playlist_types() {
        assert_eq!(MediaBrowserKind::Song.collection_type(), 50);
        assert_eq!(MediaBrowserKind::Image.collection_type(), 60);
    }

    #[test]
    fn plan_mixed_playlist_inserts_indexes_are_contiguous_across_kinds() {
        let ids = SearchAllIds {
            movie_ids: vec![1, 2],
            show_ids: vec![3],
            episode_ids: vec![4, 5, 6],
            song_ids: vec![7],
            ..SearchAllIds::default()
        };

        let plan = plan_mixed_playlist_inserts(42, &ids);

        let indexes: Vec<i64> = plan.iter().map(|row| row.index).collect();
        assert_eq!(indexes, vec![42, 43, 44, 45, 46, 47, 48]);
        assert_eq!(plan.len(), 7);
        // Empty kinds (seasons/artists/etc.) must not consume an index slot.
        let kinds: Vec<i32> = plan.iter().map(|row| row.collection_type).collect();
        assert_eq!(kinds, vec![10, 10, 1, 20, 20, 20, 50]);
    }

    #[test]
    fn plan_mixed_playlist_inserts_respects_start_index() {
        let ids = SearchAllIds {
            movie_ids: vec![99],
            ..SearchAllIds::default()
        };

        let plan = plan_mixed_playlist_inserts(1000, &ids);
        assert_eq!(
            plan,
            vec![MixedPlaylistInsert {
                index: 1000,
                collection_type: 10,
                media_item_id: 99,
            }]
        );
    }

    #[test]
    fn movie_fact_search_links_match_ersatztv_lowercase_queries() {
        assert_eq!(
            crate::admin::pages::media::movies::search_link("studio", "Z Studio", true),
            "/search?query=studio%3A%22z%20studio%22"
        );
        assert_eq!(
            crate::admin::pages::media::movies::search_link("genre", "Science Fiction", true),
            "/search?query=genre%3A%22science%20fiction%22"
        );
        assert_eq!(
            crate::admin::pages::media::movies::search_link("tag", "Director's Cut", true),
            "/search?query=tag%3A%22director%27s%20cut%22"
        );
    }

    async fn seed_show_language_fixture(pool: &ersatzrs_infra::db::DbPool) {
        sqlx::query(r#"INSERT INTO "MediaSource" ("Id") VALUES (1)"#)
            .execute(pool)
            .await
            .expect("seed media source");
        sqlx::query(
            r#"INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
               VALUES (1, 'Shows', 2, 1)"#,
        )
        .execute(pool)
        .await
        .expect("seed library");
        sqlx::query(
            r#"INSERT INTO "LibraryPath" ("Id", "Path", "LibraryId") VALUES (1, '/tv', 1)"#,
        )
        .execute(pool)
        .await
        .expect("seed library path");

        for id in [10_i64, 11, 12] {
            sqlx::query(
                r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (?, 1, 0)"#,
            )
            .bind(id)
            .execute(pool)
            .await
            .expect("seed media item");
        }

        sqlx::query(r#"INSERT INTO "Show" ("Id") VALUES (10)"#)
            .execute(pool)
            .await
            .expect("seed show");
        sqlx::query(r#"INSERT INTO "Season" ("Id", "ShowId", "SeasonNumber") VALUES (11, 10, 1)"#)
            .execute(pool)
            .await
            .expect("seed season");
        sqlx::query(r#"INSERT INTO "Episode" ("Id", "SeasonId") VALUES (12, 11)"#)
            .execute(pool)
            .await
            .expect("seed episode");
        sqlx::query(
            r#"INSERT INTO "MediaVersion" (
                   "Id", "DateAdded", "DateUpdated", "Duration", "EpisodeId",
                   "Height", "VideoScanKind", "Width"
               ) VALUES (
                   20, '2026-05-18T00:00:00Z', '2026-05-18T00:00:00Z',
                   '00:22:00', 12, 1080, 0, 1920
               )"#,
        )
        .execute(pool)
        .await
        .expect("seed media version");
        sqlx::query(
            r#"INSERT INTO "MediaStream" (
                   "Id", "Index", "Codec", "MediaStreamKind", "Language", "Channels",
                   "Default", "Forced", "MediaVersionId"
               ) VALUES
                   (30, 0, 'aac', 2, 'eng', 2, 1, 0, 20),
                   (31, 1, 'aac', 2, 'en', 2, 0, 0, 20),
                   (32, 2, 'aac', 2, 'nld', 2, 0, 0, 20),
                   (33, 3, 'ass', 3, 'eng', 0, 0, 0, 20),
                   (34, 4, 'aac', 2, NULL, 2, 0, 0, 20)"#,
        )
        .execute(pool)
        .await
        .expect("seed media streams");
    }

    async fn seed_artist_language_fixture(pool: &ersatzrs_infra::db::DbPool) {
        sqlx::query(r#"INSERT INTO "MediaSource" ("Id") VALUES (1)"#)
            .execute(pool)
            .await
            .expect("seed media source");
        sqlx::query(
            r#"INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
               VALUES (1, 'Music Videos', 3, 1)"#,
        )
        .execute(pool)
        .await
        .expect("seed library");
        sqlx::query(
            r#"INSERT INTO "LibraryPath" ("Id", "Path", "LibraryId") VALUES (1, '/music', 1)"#,
        )
        .execute(pool)
        .await
        .expect("seed library path");

        for id in [10_i64, 11] {
            sqlx::query(
                r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (?, 1, 0)"#,
            )
            .bind(id)
            .execute(pool)
            .await
            .expect("seed media item");
        }

        sqlx::query(r#"INSERT INTO "Artist" ("Id") VALUES (10)"#)
            .execute(pool)
            .await
            .expect("seed artist");
        sqlx::query(r#"INSERT INTO "MusicVideo" ("Id", "ArtistId") VALUES (11, 10)"#)
            .execute(pool)
            .await
            .expect("seed music video");
        sqlx::query(
            r#"INSERT INTO "MediaVersion" (
                   "Id", "DateAdded", "DateUpdated", "Duration", "MusicVideoId",
                   "Height", "VideoScanKind", "Width"
               ) VALUES (
                   20, '2026-05-18T00:00:00Z', '2026-05-18T00:00:00Z',
                   '00:04:00', 11, 1080, 0, 1920
               )"#,
        )
        .execute(pool)
        .await
        .expect("seed media version");
        sqlx::query(
            r#"INSERT INTO "MediaStream" (
                   "Id", "Index", "Codec", "MediaStreamKind", "Language", "Channels",
                   "Default", "Forced", "MediaVersionId"
               ) VALUES
                   (30, 0, 'aac', 2, 'eng', 2, 1, 0, 20),
                   (31, 1, 'aac', 2, 'en', 2, 0, 0, 20),
                   (32, 2, 'aac', 2, 'nld', 2, 0, 0, 20),
                   (33, 3, 'ass', 3, 'eng', 0, 0, 0, 20),
                   (34, 4, 'aac', 2, NULL, 2, 0, 0, 20)"#,
        )
        .execute(pool)
        .await
        .expect("seed media streams");
    }

    async fn seed_movie_metadata_fact_fixture(pool: &ersatzrs_infra::db::DbPool) {
        sqlx::query(r#"INSERT INTO "MediaSource" ("Id") VALUES (1)"#)
            .execute(pool)
            .await
            .expect("seed media source");
        sqlx::query(
            r#"INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
               VALUES (1, 'Movies', 1, 1)"#,
        )
        .execute(pool)
        .await
        .expect("seed library");
        sqlx::query(
            r#"INSERT INTO "LibraryPath" ("Id", "Path", "LibraryId") VALUES (1, '/movies', 1)"#,
        )
        .execute(pool)
        .await
        .expect("seed library path");
        sqlx::query(
            r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State")
               VALUES (10, 1, 0), (11, 1, 0)"#,
        )
        .execute(pool)
        .await
        .expect("seed media items");
        sqlx::query(r#"INSERT INTO "Movie" ("Id") VALUES (10), (11)"#)
            .execute(pool)
            .await
            .expect("seed movies");
        sqlx::query(
            r#"INSERT INTO "MovieMetadata" (
                   "Id", "DateAdded", "DateUpdated", "MetadataKind", "MovieId", "Title"
               ) VALUES (
                   100, '2026-05-22T00:00:00Z', '2026-05-22T00:00:00Z', 0,
                   10, 'Example Movie'
               ), (
                   101, '2026-05-22T00:00:00Z', '2026-05-22T00:00:00Z', 0,
                   11, 'Other Movie'
               )"#,
        )
        .execute(pool)
        .await
        .expect("seed movie metadata");
        sqlx::query(
            r#"INSERT INTO "Studio" ("Name", "MovieMetadataId") VALUES
                   ('Z Studio', 100),
                   ('A Studio', 100),
                   ('', 100),
                   (NULL, 100),
                   ('Other Movie Studio', 101)"#,
        )
        .execute(pool)
        .await
        .expect("seed studios");
        sqlx::query(
            r#"INSERT INTO "Genre" ("Name", "MovieMetadataId") VALUES
                   ('Science Fiction', 100),
                   ('Drama', 100),
                   ('Other Movie Genre', 101)"#,
        )
        .execute(pool)
        .await
        .expect("seed genres");
        sqlx::query(
            r#"INSERT INTO "Tag" ("Name", "MovieMetadataId") VALUES
                   ('Director''s Cut', 100),
                   ('Cult', 100),
                   ('Other Movie Tag', 101)"#,
        )
        .execute(pool)
        .await
        .expect("seed tags");
    }

    #[allow(clippy::too_many_lines)]
    async fn seed_movie_detail_variant_fixture(pool: &ersatzrs_infra::db::DbPool) {
        let now = "2026-07-01T00:00:00Z";
        sqlx::query(r#"INSERT INTO "MediaSource" ("Id") VALUES (1)"#)
            .execute(pool)
            .await
            .expect("seed media source");
        sqlx::query(r#"INSERT INTO "LocalMediaSource" ("Id") VALUES (1)"#)
            .execute(pool)
            .await
            .expect("seed local media source");
        sqlx::query(
            r#"INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
               VALUES (1, 'Movies', 1, 1)"#,
        )
        .execute(pool)
        .await
        .expect("seed library");
        sqlx::query(r#"INSERT INTO "LocalLibrary" ("Id") VALUES (1)"#)
            .execute(pool)
            .await
            .expect("seed local library");
        sqlx::query(
            r#"INSERT INTO "LibraryPath" ("Id", "Path", "LibraryId") VALUES (1, '/movies', 1)"#,
        )
        .execute(pool)
        .await
        .expect("seed library path");
        sqlx::query(
            r#"INSERT INTO "MediaItem" (
                   "Id", "LibraryPathId", "State", "ContentKind", "SubKind", "ParentMediaItemId"
               ) VALUES
                   (10, 1, 0, 0, NULL, NULL),
                   (11, 1, 0, 2, 'directors_cut', 10),
                   (30, 1, 0, 1, 'featurette', 10)"#,
        )
        .execute(pool)
        .await
        .expect("seed media items");
        sqlx::query(r#"INSERT INTO "Movie" ("Id") VALUES (10), (11)"#)
            .execute(pool)
            .await
            .expect("seed movies");
        sqlx::query(r#"INSERT INTO "OtherVideo" ("Id") VALUES (30)"#)
            .execute(pool)
            .await
            .expect("seed other video");
        sqlx::query(
            r#"INSERT INTO "MovieMetadata" (
                   "Id", "MovieId", "MetadataKind", "Title", "SortTitle", "Year",
                   "Plot", "DateAdded", "DateUpdated"
               ) VALUES
                   (100, 10, 0, 'Example Movie', 'example movie', 2026,
                    'Primary movie.', ?, ?),
                   (101, 11, 0, 'Example Movie Director''s Cut',
                    'example movie directors cut', 2026, 'Alternate edition.', ?, ?)"#,
        )
        .bind(now)
        .bind(now)
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .expect("seed movie metadata");
        sqlx::query(
            r#"INSERT INTO "OtherVideoMetadata" (
                   "Id", "OtherVideoId", "MetadataKind", "Title", "OriginalTitle",
                   "SortTitle", "Year", "DateAdded", "DateUpdated"
               ) VALUES (
                   300, 30, 0, 'Behind the Scenes', 'Featurette',
                   'behind the scenes', 2026, ?, ?
               )"#,
        )
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .expect("seed other-video metadata");
        sqlx::query(
            r#"INSERT INTO "MediaVersion" (
                   "Id", "DateAdded", "DateUpdated", "Duration", "MovieId",
                   "Height", "Width", "VideoScanKind", "SampleAspectRatio",
                   "DisplayAspectRatio", "RFrameRate"
               ) VALUES (
                   1000, ?, ?, '02:00:00', 10, 1080, 1920, 1, '1:1', '16:9', '24000/1001'
               )"#,
        )
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .expect("seed media version");
        sqlx::query(
            r#"INSERT INTO "MediaStream" (
                   "Id", "Index", "Codec", "Profile", "MediaStreamKind", "Language",
                   "Channels", "Title", "Default", "Forced", "MediaVersionId"
               ) VALUES
                   (1100, 0, 'h264', 'High', 1, NULL, 0, NULL, 1, 0, 1000),
                   (1101, 1, 'aac', NULL, 2, 'eng', 2, 'Main Audio', 1, 0, 1000),
                   (1102, 2, 'aac', NULL, 2, 'eng', 2, 'Director Commentary', 0, 0, 1000),
                   (1103, 3, 'srt', NULL, 3, 'eng', 0, 'Director Commentary Subtitles', 0, 0, 1000)"#,
        )
        .execute(pool)
        .await
        .expect("seed media streams");
        sqlx::query(
            r#"INSERT INTO "Subtitle" (
                   "Id", "MovieMetadataId", "StreamIndex", "SubtitleKind", "Title",
                   "Codec", "Language", "Default", "Forced", "Path", "IsExtracted",
                   "DateAdded", "DateUpdated"
               ) VALUES (
                   1200, 100, 3, 0, 'Director Commentary Subtitles', 'srt', 'eng',
                   0, 0, '/cache/movie-commentary.srt', 1, ?, ?
               )"#,
        )
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .expect("seed commentary subtitle");
        sqlx::query(
            r#"INSERT INTO "Collection" ("Id", "Name", "UseCustomPlaybackOrder")
               VALUES (900, 'Favorites', 0)"#,
        )
        .execute(pool)
        .await
        .expect("seed collection");
        sqlx::query(
            r#"INSERT INTO "PlaylistGroup" ("Id", "Name", "IsSystem")
               VALUES (901, 'Movies', 0)"#,
        )
        .execute(pool)
        .await
        .expect("seed playlist group");
        sqlx::query(
            r#"INSERT INTO "Playlist" ("Id", "Name", "PlaylistGroupId", "IsSystem")
               VALUES (902, 'Featurettes', 901, 0)"#,
        )
        .execute(pool)
        .await
        .expect("seed playlist");
    }

    #[allow(clippy::too_many_lines)]
    async fn seed_other_video_media_info_fixture(pool: &ersatzrs_infra::db::DbPool) {
        sqlx::query(r#"INSERT INTO "MediaSource" ("Id") VALUES (1)"#)
            .execute(pool)
            .await
            .expect("seed media source");
        sqlx::query(r#"INSERT INTO "LocalMediaSource" ("Id") VALUES (1)"#)
            .execute(pool)
            .await
            .expect("seed local media source");
        sqlx::query(
            r#"INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
               VALUES (1, 'Other Videos', 4, 1)"#,
        )
        .execute(pool)
        .await
        .expect("seed library");
        sqlx::query(r#"INSERT INTO "LocalLibrary" ("Id") VALUES (1)"#)
            .execute(pool)
            .await
            .expect("seed local library");
        sqlx::query(
            r#"INSERT INTO "LibraryPath" ("Id", "Path", "LibraryId") VALUES (1, '/other', 1)"#,
        )
        .execute(pool)
        .await
        .expect("seed library path");
        sqlx::query(
            r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (10, 1, 0)"#,
        )
        .execute(pool)
        .await
        .expect("seed media item");
        sqlx::query(r#"INSERT INTO "OtherVideo" ("Id") VALUES (10)"#)
            .execute(pool)
            .await
            .expect("seed other video");
        sqlx::query(
            r#"INSERT INTO "OtherVideoMetadata" (
                   "Id", "DateAdded", "DateUpdated", "MetadataKind", "OtherVideoId",
                   "Title", "OriginalTitle"
               ) VALUES (
                   100, '2026-05-22T00:00:00Z', '2026-05-22T00:00:00Z', 0,
                   10, 'Example Other Video', 'folder/example.mkv'
               )"#,
        )
        .execute(pool)
        .await
        .expect("seed other-video metadata");
        sqlx::query(
            r#"INSERT INTO "MediaVersion" (
                   "Id", "DateAdded", "DateUpdated", "Duration", "OtherVideoId",
                   "Height", "Width", "VideoScanKind", "SampleAspectRatio",
                   "DisplayAspectRatio", "RFrameRate"
               ) VALUES (
                   20, '2026-05-22T00:00:00Z', '2026-05-22T00:00:00Z',
                   '00:30:00', 10, 1080, 1920, 1, '1:1', '16:9', '24000/1001'
               ), (
                   21, '2026-05-22T00:00:00Z', '2026-05-22T00:00:00Z',
                   '00:30:00', 10, 1080, 1920, 1, '1:1', '16:9', '24000/1001'
               )"#,
        )
        .execute(pool)
        .await
        .expect("seed media versions");
        sqlx::query(
            r#"INSERT INTO "MediaFile" ("Id", "MediaVersionId", "Path", "PathHash")
               VALUES (200, 20, '/other/example.mkv', 'hash')"#,
        )
        .execute(pool)
        .await
        .expect("seed media file");
        sqlx::query(
            r#"INSERT INTO "MediaStream" (
                   "Id", "Index", "Codec", "Profile", "MediaStreamKind", "Language",
                   "Channels", "Title", "Default", "Forced", "MediaVersionId"
               ) VALUES
                   (300, 0, 'h264', 'High', 1, NULL, 0, NULL, 1, 0, 20),
                   (301, 1, 'aac', NULL, 2, 'eng', 2, 'English', 1, 0, 20),
                   (302, 2, 'aac', NULL, 2, 'en', 2, 'English Alt', 0, 0, 20),
                   (303, 3, 'aac', NULL, 2, 'nld', 2, 'Dutch', 0, 0, 20),
                   (304, 4, 'srt', NULL, 3, 'eng', 0, 'English SDH', 0, 0, 20),
                   (305, 0, 'aac', NULL, 2, 'fra', 2, 'French', 1, 0, 21)"#,
        )
        .execute(pool)
        .await
        .expect("seed media streams");
        sqlx::query(
            r#"INSERT INTO "Subtitle" (
                   "Id", "OtherVideoMetadataId", "StreamIndex", "SubtitleKind", "Title",
                   "Codec", "Language", "Default", "Forced", "Path", "IsExtracted",
                   "DateAdded", "DateUpdated"
               ) VALUES (
                   400, 100, 4, 0, 'English SDH', 'srt', 'eng', 0, 0,
                   '/cache/other.srt', 1, '2026-05-22T00:00:00Z', '2026-05-22T00:00:00Z'
               )"#,
        )
        .execute(pool)
        .await
        .expect("seed subtitle");
        sqlx::query(
            r#"INSERT INTO "MediaChapter" (
                   "Id", "MediaVersionId", "ChapterId", "StartTime", "EndTime", "Title"
               ) VALUES (500, 20, 1, '00:00:00', '00:01:00', 'Opening')"#,
        )
        .execute(pool)
        .await
        .expect("seed chapter");
    }

    async fn seed_movie_language_fixture(pool: &ersatzrs_infra::db::DbPool) {
        sqlx::query(r#"INSERT INTO "MediaSource" ("Id") VALUES (1)"#)
            .execute(pool)
            .await
            .expect("seed media source");
        sqlx::query(
            r#"INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
               VALUES (1, 'Movies', 1, 1)"#,
        )
        .execute(pool)
        .await
        .expect("seed library");
        sqlx::query(
            r#"INSERT INTO "LibraryPath" ("Id", "Path", "LibraryId") VALUES (1, '/movies', 1)"#,
        )
        .execute(pool)
        .await
        .expect("seed library path");
        sqlx::query(
            r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (10, 1, 0)"#,
        )
        .execute(pool)
        .await
        .expect("seed media item");
        sqlx::query(r#"INSERT INTO "Movie" ("Id") VALUES (10)"#)
            .execute(pool)
            .await
            .expect("seed movie");
        sqlx::query(
            r#"INSERT INTO "MediaVersion" (
                   "Id", "DateAdded", "DateUpdated", "Duration", "MovieId",
                   "Height", "VideoScanKind", "Width"
               ) VALUES
                   (20, '2026-05-18T00:00:00Z', '2026-05-18T00:00:00Z',
                    '02:20:00', 10, 1080, 0, 1920),
                   (21, '2026-05-18T00:00:00Z', '2026-05-18T00:00:00Z',
                    '02:20:00', 10, 1080, 0, 1920)"#,
        )
        .execute(pool)
        .await
        .expect("seed media versions");
        sqlx::query(
            r#"INSERT INTO "MediaStream" (
                   "Id", "Index", "Codec", "MediaStreamKind", "Language", "Channels",
                   "Default", "Forced", "MediaVersionId"
               ) VALUES
                   (30, 0, 'aac', 2, 'eng', 2, 1, 0, 20),
                   (31, 1, 'aac', 2, 'en', 2, 0, 0, 20),
                   (32, 2, 'aac', 2, 'nld', 2, 0, 0, 20),
                   (33, 3, 'ass', 3, 'eng', 0, 0, 0, 20),
                   (34, 0, 'aac', 2, 'fra', 2, 1, 0, 21)"#,
        )
        .execute(pool)
        .await
        .expect("seed media streams");
    }

    async fn seed_playout_refresh_fixture(pool: &ersatzrs_infra::db::DbPool) {
        use ersatzrs_core::repositories::{ChannelRepository, FfmpegProfileRepository};

        sqlx::query(
            r#"INSERT OR IGNORE INTO "Resolution" ("Id", "Name", "Width", "Height")
               VALUES (1, '1920x1080', 1920, 1080)"#,
        )
        .execute(pool)
        .await
        .expect("seed resolution");

        let repos = ersatzrs_infra::repositories::make_repositories(pool.clone());
        repos
            .ffmpeg_profile
            .upsert(&ersatzrs_domain::FFmpegProfile {
                id: 1,
                name: "default".to_string(),
                resolution_id: 1,
                ..ersatzrs_domain::FFmpegProfile::default()
            })
            .await
            .expect("seed ffmpeg profile");
        repos
            .channel
            .upsert(&ersatzrs_domain::Channel {
                id: 1,
                unique_id: uuid::Uuid::from_u128(1),
                number: "1".to_string(),
                sort_number: 1.0,
                name: "Channel 1".to_string(),
                ffmpeg_profile_id: 1,
                is_enabled: true,
                ..ersatzrs_domain::Channel::default()
            })
            .await
            .expect("seed channel");

        sqlx::query(
            r#"INSERT INTO "ProgramSchedule" (
                   "Id", "FixedStartTimeBehavior", "KeepMultiPartEpisodesTogether",
                   "Name", "RandomStartPoint", "ShuffleScheduleItems",
                   "TreatCollectionsAsShows"
               ) VALUES
                   (10, 0, 0, 'Schedule 10', 0, 0, 0),
                   (11, 0, 0, 'Schedule 11', 0, 0, 0)"#,
        )
        .execute(pool)
        .await
        .expect("seed schedules");
        sqlx::query(r#"INSERT INTO "Collection" ("Id", "Name", "UseCustomPlaybackOrder") VALUES (20, 'Collection 20', 0)"#)
            .execute(pool)
            .await
            .expect("seed collection");
        sqlx::query(r#"INSERT INTO "SmartCollection" ("Id", "Name", "Query") VALUES (40, 'Smart 40', 'movie')"#)
            .execute(pool)
            .await
            .expect("seed smart collection");
        sqlx::query(
            r#"INSERT INTO "RerunCollection" (
                   "Id", "Name", "CollectionType", "CollectionId",
                   "FirstRunPlaybackOrder", "RerunPlaybackOrder"
               ) VALUES (30, 'Rerun 30', 0, 20, 1, 1)"#,
        )
        .execute(pool)
        .await
        .expect("seed rerun collection");
        sqlx::query(
            r#"INSERT INTO "Playout" ("Id", "ChannelId", "ProgramScheduleId", "ScheduleKind", "Seed")
               VALUES
                   (100, 1, 10, 1, 0),
                   (101, 1, 10, 1, 0),
                   (102, 1, 11, 1, 0)"#,
        )
        .execute(pool)
        .await
        .expect("seed playouts");
        sqlx::query(
            r#"INSERT INTO "PlayoutProgramScheduleAnchor" (
                   "Id", "CollectionId", "RerunCollectionId", "SmartCollectionId", "CollectionType", "PlayoutId"
               ) VALUES
                   (200, 20, NULL, NULL, 10, 101),
                   (201, 20, NULL, NULL, 10, 102),
                   (202, 20, NULL, NULL, 10, 102),
                   (203, NULL, 30, NULL, 7, 100),
                   (204, NULL, 30, NULL, 8, 102),
                   (205, NULL, NULL, 40, 5, 100),
                   (206, NULL, NULL, 40, 5, 101)"#,
        )
        .execute(pool)
            .await
            .expect("seed playout anchors");
    }

    async fn collection_test_pool(
        database_name: &str,
    ) -> (tempfile::TempDir, ersatzrs_infra::db::DbPool) {
        let database_dir = tempfile::tempdir().expect("temp database dir");
        let database_url = format!(
            "sqlite:{}",
            database_dir.path().join(database_name).display()
        );
        let pool = ersatzrs_infra::db::init(&database_url)
            .await
            .expect("initialize test database");
        (database_dir, pool)
    }

    fn collection_test_state(pool: ersatzrs_infra::db::DbPool) -> crate::host::state::AppState {
        collection_test_state_with_playout_rx(pool).0
    }

    fn collection_test_state_with_playout_rx(
        pool: ersatzrs_infra::db::DbPool,
    ) -> (
        crate::host::state::AppState,
        tokio::sync::mpsc::UnboundedReceiver<ersatzrs_core::scheduling::PlayoutBuildRequest>,
    ) {
        let (playout_build_tx, playout_build_rx) = tokio::sync::mpsc::unbounded_channel();
        let state = crate::host::state::AppState::new(
            pool,
            ersatzrs_infra::db::DatabaseBackend::Sqlite,
            crate::host::state::SystemStartup {
                database_ready: true,
                schema_status: ersatzrs_infra::db::DatabaseSchemaStatus::default(),
            },
            ersatzrs_ffmpeg::FfmpegRuntimeReport::for_tests(),
            uuid::Uuid::nil(),
            1,
            crate::host::state::IptvAccessTokenVerifier::disabled(),
            playout_build_tx,
            ersatzrs_core::locking::EntityLocker::default(),
        );
        (state, playout_build_rx)
    }
}
