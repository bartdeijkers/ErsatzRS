#![cfg_attr(test, allow(dead_code))]

//! Server functions for the global `/search` page.
//!
//! Ports the per-kind search-card projection performed by
//! `$ERSATZTV_SRC/ErsatzTV/Pages/Search.razor` together with the per-kind
//! `QuerySearchIndex*Handler` query handlers and the
//! `$ERSATZTV_SRC/ErsatzTV.Application/MediaCards/Mapper.cs` view-model
//! projection rules.
//!
//! The shape mirrors `ErsatzTV` exactly: ten typed sections (Movies, Shows,
//! Seasons, Episodes, Artists, Music Videos, Other Videos, Songs, Images,
//! Remote Streams), each with up to 50 cards and a total hit count. Every
//! section runs `SchedulingRepository::search_documents_page` with the query
//! `type:<kind> AND (<user_query>)` so the parent (show/season/artist) shows
//! up as one card and not as its expanded leaves — matching the `ErsatzTV`
//! search-results page semantics.

use leptos::prelude::*;
use serde::{Deserialize, Serialize};

use crate::admin::server_fns::media_browser::ContentVariantInfo;

/// Bulk-id payload returned by the `search_all_ids` query (ported from
/// `ErsatzTV`'s `QuerySearchIndexAllItems`) and consumed by mixed-kind bulk
/// actions like `add_media_items_to_playlist_mixed`. One `Vec<i64>` per
/// `CollectionType`-eligible search kind.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchAllIds {
    pub movie_ids: Vec<i64>,
    pub show_ids: Vec<i64>,
    pub season_ids: Vec<i64>,
    pub episode_ids: Vec<i64>,
    pub artist_ids: Vec<i64>,
    pub music_video_ids: Vec<i64>,
    pub other_video_ids: Vec<i64>,
    pub song_ids: Vec<i64>,
    pub image_ids: Vec<i64>,
    pub remote_stream_ids: Vec<i64>,
}

/// Discriminant for a card in the search-results grid. Mirrors the
/// `media_kind` strings the search index returns (`movie`, `show`, …).
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SearchCardKind {
    #[default]
    Movie,
    Show,
    Season,
    Episode,
    Artist,
    MusicVideo,
    OtherVideo,
    Song,
    Image,
    RemoteStream,
}

impl SearchAllIds {
    /// All ids across every kind, flattened in section order, for the
    /// kind-agnostic `add_media_items_to_collection` path.
    #[must_use]
    pub fn flatten(&self) -> Vec<i64> {
        let mut out = Vec::new();
        out.extend(&self.movie_ids);
        out.extend(&self.show_ids);
        out.extend(&self.season_ids);
        out.extend(&self.episode_ids);
        out.extend(&self.artist_ids);
        out.extend(&self.music_video_ids);
        out.extend(&self.other_video_ids);
        out.extend(&self.song_ids);
        out.extend(&self.image_ids);
        out.extend(&self.remote_stream_ids);
        out
    }

    /// True when no kind has any ids.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.movie_ids.is_empty()
            && self.show_ids.is_empty()
            && self.season_ids.is_empty()
            && self.episode_ids.is_empty()
            && self.artist_ids.is_empty()
            && self.music_video_ids.is_empty()
            && self.other_video_ids.is_empty()
            && self.song_ids.is_empty()
            && self.image_ids.is_empty()
            && self.remote_stream_ids.is_empty()
    }
}

impl SearchCardKind {
    /// String value the SQL-backed search index emits for this kind.
    /// Matches `SEARCH_DOCUMENT_SQL` in `scheduling.rs`.
    #[must_use]
    pub fn media_kind(self) -> &'static str {
        match self {
            Self::Movie => "movie",
            Self::Show => "show",
            Self::Season => "season",
            Self::Episode => "episode",
            Self::Artist => "artist",
            Self::MusicVideo => "music_video",
            Self::OtherVideo => "other_video",
            Self::Song => "song",
            Self::Image => "image",
            Self::RemoteStream => "remote_stream",
        }
    }
}

/// Which artwork kind a card prefers. Ports the
/// `Mapper.GetPoster`/`Mapper.GetThumbnail` choice. The actual URL is built by
/// `crate::api::artwork::artwork_url` server-side so the wire shape is ready
/// for `<img src>`.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum SearchArtworkKind {
    #[default]
    Poster,
    Thumbnail,
}

/// One card in a search section. Mirrors `MediaCardViewModel` together with
/// the per-section `Href`/`Subtitle` overrides built inline in `Search.razor`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchCard {
    pub media_item_id: i64,
    pub kind: SearchCardKind,
    pub season_number: Option<i32>,
    pub episode_number: Option<i32>,
    pub show_title: Option<String>,
    pub title: String,
    pub sort_title: String,
    pub subtitle: Option<String>,
    pub artwork_url: Option<String>,
    pub artwork_kind: SearchArtworkKind,
    pub placeholder: String,
    pub state: ersatzrs_domain::MediaItemState,
    pub href: String,
    #[serde(default)]
    pub variant: ContentVariantInfo,
}

/// Per-kind card-results section. Equals `Cards` + `TotalCount` in
/// `ErsatzTV`'s `*CardResultsViewModel`. Cards are already sorted by
/// `sort_title` (the page-projection step sorts by it).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchSection {
    pub cards: Vec<SearchCard>,
    pub total_count: u32,
}

/// All ten sections plus an echo of the user query, returned by
/// [`search_results`].
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchResults {
    pub query: String,
    pub movies: SearchSection,
    pub shows: SearchSection,
    pub seasons: SearchSection,
    pub episodes: SearchSection,
    pub artists: SearchSection,
    pub music_videos: SearchSection,
    pub other_videos: SearchSection,
    pub songs: SearchSection,
    pub images: SearchSection,
    pub remote_streams: SearchSection,
}

// ---------------------------------------------------------------------------
// Pure projection helpers (unit-tested below).
// ---------------------------------------------------------------------------

/// Build the placeholder string shown when a card has no artwork.
/// Mirrors `$ERSATZTV_SRC/ErsatzTV/Shared/MediaCard.razor`'s `GetPlaceholder`:
///
/// * Season 0 → `"S"`, other seasons → the season number.
/// * Otherwise: uppercase first letter of `sort_title` (or `title` fallback)
///   when ASCII alphabetic, else `"#"`.
#[must_use]
pub fn search_placeholder(
    kind: SearchCardKind,
    sort_title: &str,
    title: &str,
    season_number: Option<i32>,
) -> String {
    if kind == SearchCardKind::Season {
        return match season_number {
            Some(n) if n != 0 => n.to_string(),
            _ => "S".to_string(),
        };
    }

    let source = if sort_title.trim().is_empty() {
        title
    } else {
        sort_title
    };
    let Some(first) = source.trim().chars().next() else {
        return "#".to_string();
    };
    if first.is_ascii_alphabetic() {
        first.to_ascii_uppercase().to_string()
    } else {
        "#".to_string()
    }
}

/// Episode subtitle: `"{ShowTitle} - S{season} E{episode}"`. Mirrors the
/// inline `Search.razor` subtitle for the Episodes section.
#[must_use]
pub fn episode_subtitle(
    show_title: Option<&str>,
    season_number: Option<i32>,
    episode_number: Option<i32>,
) -> Option<String> {
    let show = show_title.unwrap_or("").trim();
    match (season_number, episode_number) {
        (Some(season), Some(episode)) if !show.is_empty() => {
            Some(format!("{show} - S{season} E{episode}"))
        }
        (Some(season), Some(episode)) => Some(format!("S{season} E{episode}")),
        _ => (!show.is_empty()).then(|| show.to_string()),
    }
}

/// Season subtitle: `"Specials"` for season 0, else `"Season {n}"`.
/// Matches `Mapper.GetSeasonName`.
#[must_use]
pub fn season_subtitle(season_number: Option<i32>) -> Option<String> {
    match season_number {
        Some(0) => Some("Specials".to_string()),
        Some(n) => Some(format!("Season {n}")),
        None => None,
    }
}

/// Song subtitle: `"{artists} - {album}"`, gracefully skipping empties.
/// Matches `Mapper.ProjectToViewModel(SongMetadata)` (album is prefixed
/// `" - "` only when non-empty).
#[must_use]
pub fn song_subtitle(artists: Option<&str>, album: Option<&str>) -> Option<String> {
    let artists = artists.unwrap_or("").trim();
    let album = album.unwrap_or("").trim();
    match (artists, album) {
        ("", "") => None,
        ("", album) => Some(album.to_string()),
        (artists, "") => Some(artists.to_string()),
        (artists, album) => Some(format!("{artists} - {album}")),
    }
}

/// Per-kind navigation href. Mirrors the `Href`/`OnClick` rules in
/// `Search.razor`'s per-section card-grid bindings, extended for the detail
/// routes ErsatzRS now has for other videos and artist-hosted music videos.
#[must_use]
pub fn search_href(kind: SearchCardKind, media_item_id: i64, related_id: Option<i64>) -> String {
    match kind {
        SearchCardKind::Movie => format!("/media/movies/{media_item_id}"),
        SearchCardKind::Show => format!("/media/tv/shows/{media_item_id}"),
        SearchCardKind::Season => format!("/media/tv/seasons/{media_item_id}"),
        SearchCardKind::Episode => match related_id {
            Some(sid) => format!("/media/tv/seasons/{sid}#episode-{media_item_id}"),
            None => String::new(),
        },
        SearchCardKind::Artist => format!("/media/music/artists/{media_item_id}"),
        SearchCardKind::MusicVideo => format!("/media/music/videos/{media_item_id}"),
        SearchCardKind::OtherVideo => format!("/media/other/videos/{media_item_id}"),
        SearchCardKind::Song => format!("/media/music/songs/{media_item_id}"),
        SearchCardKind::Image => format!("/media/browser/images/{media_item_id}"),
        SearchCardKind::RemoteStream => String::new(),
    }
}

/// Preferred artwork kind per section. Mirrors the `Mapper.ProjectToViewModel`
/// per-kind choice between `GetPoster` and `GetThumbnail`.
#[must_use]
pub fn search_artwork_kind(kind: SearchCardKind) -> SearchArtworkKind {
    match kind {
        SearchCardKind::Movie
        | SearchCardKind::Show
        | SearchCardKind::Season
        | SearchCardKind::Episode => SearchArtworkKind::Poster,
        SearchCardKind::Artist
        | SearchCardKind::MusicVideo
        | SearchCardKind::OtherVideo
        | SearchCardKind::Song
        | SearchCardKind::Image
        | SearchCardKind::RemoteStream => SearchArtworkKind::Thumbnail,
    }
}

// ---------------------------------------------------------------------------
// Server functions.
// ---------------------------------------------------------------------------

/// Run the per-kind search-card projection once per section. Mirrors the ten
/// `QuerySearchIndex*Handler` calls in `Search.razor.OnParametersSetAsync`.
///
/// Each section runs
/// `SchedulingRepository::search_documents_page("type:<kind> AND (<q>)", 0,
/// 50)`. Empty queries return ten empty sections without touching the DB.
#[server(name = SearchResultsFn)]
pub async fn search_results(query: String) -> Result<SearchResults, ServerFnError> {
    use crate::host::state::AppState;

    let query = query.trim().to_owned();
    let mut results = SearchResults {
        query: query.clone(),
        ..SearchResults::default()
    };
    if query.is_empty() {
        return Ok(results);
    }

    let state = expect_context::<AppState>();
    let snapshot = state.application.admin_search_snapshot(&query).await?;
    let hits = snapshot.documents;

    let sections = futures_util::future::try_join_all(SEARCH_CARD_KINDS.into_iter().map(|kind| {
        let section_hits = hits
            .iter()
            .filter(|hit| hit.media_kind == kind.media_kind())
            .cloned()
            .collect::<Vec<_>>();
        let enrichment = snapshot
            .enrichment
            .get(&admin_search_kind(kind))
            .cloned()
            .unwrap_or_default();
        async move {
            let total = section_hits.len();
            let section_hits = section_hits.into_iter().take(50).collect::<Vec<_>>();
            let cards = project_section_from_enrichment(kind, &section_hits, &enrichment);
            #[allow(clippy::cast_possible_truncation)]
            let section = SearchSection {
                cards,
                total_count: total as u32,
            };
            Ok::<_, ServerFnError>((kind, section))
        }
    }))
    .await?;

    for (kind, section) in sections {
        *section_slot_mut(&mut results, kind) = section;
    }

    Ok(results)
}

#[server(name = SearchAllIdsFn)]
pub async fn search_all_ids(query: String) -> Result<SearchAllIds, ServerFnError> {
    use crate::host::state::AppState;

    let query = query.trim().to_owned();
    if query.is_empty() {
        return Ok(SearchAllIds::default());
    }

    let state = expect_context::<AppState>();

    let hits = state.application.admin_search_documents(&query).await?;

    Ok(bucket_search_all_ids(hits))
}

#[cfg(feature = "ssr")]
#[must_use]
fn bucket_search_all_ids(hits: Vec<ersatzrs_core::search::SearchDocument>) -> SearchAllIds {
    let mut out = SearchAllIds::default();
    for hit in hits {
        match hit.media_kind.as_str() {
            "movie" => out.movie_ids.push(hit.media_item_id),
            "show" => out.show_ids.push(hit.media_item_id),
            "season" => out.season_ids.push(hit.media_item_id),
            "episode" => out.episode_ids.push(hit.media_item_id),
            "artist" => out.artist_ids.push(hit.media_item_id),
            "music_video" => out.music_video_ids.push(hit.media_item_id),
            "other_video" => out.other_video_ids.push(hit.media_item_id),
            "song" => out.song_ids.push(hit.media_item_id),
            "image" => out.image_ids.push(hit.media_item_id),
            "remote_stream" => out.remote_stream_ids.push(hit.media_item_id),
            _ => {}
        }
    }
    out
}

const SEARCH_CARD_KINDS: [SearchCardKind; 10] = [
    SearchCardKind::Movie,
    SearchCardKind::Show,
    SearchCardKind::Season,
    SearchCardKind::Episode,
    SearchCardKind::Artist,
    SearchCardKind::MusicVideo,
    SearchCardKind::OtherVideo,
    SearchCardKind::Song,
    SearchCardKind::Image,
    SearchCardKind::RemoteStream,
];

#[cfg(feature = "ssr")]
const fn admin_search_kind(kind: SearchCardKind) -> ersatzrs_core::repositories::AdminSearchKind {
    use ersatzrs_core::repositories::AdminSearchKind as A;
    match kind {
        SearchCardKind::Movie => A::Movie,
        SearchCardKind::Show => A::Show,
        SearchCardKind::Season => A::Season,
        SearchCardKind::Episode => A::Episode,
        SearchCardKind::Artist => A::Artist,
        SearchCardKind::MusicVideo => A::MusicVideo,
        SearchCardKind::OtherVideo => A::OtherVideo,
        SearchCardKind::Song => A::Song,
        SearchCardKind::Image => A::Image,
        SearchCardKind::RemoteStream => A::RemoteStream,
    }
}

// ---------------------------------------------------------------------------
// Server-only helpers: per-section enrichment + projection.
// ---------------------------------------------------------------------------

#[cfg(feature = "ssr")]
fn section_slot_mut(results: &mut SearchResults, kind: SearchCardKind) -> &mut SearchSection {
    match kind {
        SearchCardKind::Movie => &mut results.movies,
        SearchCardKind::Show => &mut results.shows,
        SearchCardKind::Season => &mut results.seasons,
        SearchCardKind::Episode => &mut results.episodes,
        SearchCardKind::Artist => &mut results.artists,
        SearchCardKind::MusicVideo => &mut results.music_videos,
        SearchCardKind::OtherVideo => &mut results.other_videos,
        SearchCardKind::Song => &mut results.songs,
        SearchCardKind::Image => &mut results.images,
        SearchCardKind::RemoteStream => &mut results.remote_streams,
    }
}

#[cfg(feature = "ssr")]
type SectionEnrichment = ersatzrs_core::repositories::AdminSearchEnrichment;

#[cfg(feature = "ssr")]
fn project_section_from_enrichment(
    kind: SearchCardKind,
    hits: &[ersatzrs_core::search::SearchDocument],
    enrichment: &SectionEnrichment,
) -> Vec<SearchCard> {
    hits.iter()
        .map(|hit| build_card(kind, hit, enrichment))
        .collect()
}

#[cfg(all(test, feature = "ssr"))]
async fn project_section(
    pool: &ersatzrs_infra::db::DbPool,
    kind: SearchCardKind,
    hits: Vec<ersatzrs_core::search::SearchDocument>,
) -> Result<Vec<SearchCard>, ServerFnError> {
    if hits.is_empty() {
        return Ok(Vec::new());
    }

    let ids: Vec<i64> = hits.iter().map(|h| h.media_item_id).collect();
    let enrichment = load_enrichment(pool, kind, &ids).await?;

    Ok(hits
        .iter()
        .map(|hit| build_card(kind, hit, &enrichment))
        .collect())
}

#[cfg(feature = "ssr")]
fn build_card(
    kind: SearchCardKind,
    hit: &ersatzrs_core::search::SearchDocument,
    enrichment: &SectionEnrichment,
) -> SearchCard {
    let title = hit.title.clone().unwrap_or_default();
    let sort_title = hit
        .sort_title
        .clone()
        .or_else(|| hit.title.clone())
        .unwrap_or_default();

    let placeholder = search_placeholder(kind, &sort_title, &title, hit.season_number);
    let subtitle = card_subtitle(kind, hit, enrichment);
    let artwork_kind = search_artwork_kind(kind);

    let raw_path = resolve_artwork_path(kind, hit.media_item_id, enrichment);
    let artwork_url = raw_path.and_then(|path| {
        use crate::api::artwork::{artwork_kind_for_admin_search, artwork_url};
        artwork_url(
            &path,
            artwork_kind_for_admin_search(admin_search_kind(kind)),
        )
    });

    let href = search_href(
        kind,
        hit.media_item_id,
        match kind {
            SearchCardKind::Episode => enrichment
                .episode_season_id
                .get(&hit.media_item_id)
                .copied(),
            SearchCardKind::MusicVideo => enrichment
                .music_video_artist_id
                .get(&hit.media_item_id)
                .copied(),
            _ => None,
        },
    );

    let state = enrichment
        .state
        .get(&hit.media_item_id)
        .copied()
        .unwrap_or_default();

    SearchCard {
        media_item_id: hit.media_item_id,
        kind,
        season_number: hit.season_number,
        episode_number: hit.episode_number,
        show_title: hit.show_title.clone(),
        title,
        sort_title,
        subtitle,
        artwork_url,
        artwork_kind,
        placeholder,
        state,
        href,
        variant: ContentVariantInfo {
            content_kind: hit.content_kind,
            sub_kind: hit.sub_kind.clone(),
            parent_title: hit.parent_title.clone(),
        },
    }
}

#[cfg(feature = "ssr")]
fn card_subtitle(
    kind: SearchCardKind,
    hit: &ersatzrs_core::search::SearchDocument,
    enrichment: &SectionEnrichment,
) -> Option<String> {
    match kind {
        SearchCardKind::Movie | SearchCardKind::Show => hit.year.map(|y| y.to_string()),
        SearchCardKind::Season => season_subtitle(hit.season_number),
        SearchCardKind::Episode => episode_subtitle(
            hit.show_title.as_deref(),
            hit.season_number,
            hit.episode_number,
        ),
        SearchCardKind::Artist => enrichment
            .artist_disambiguation
            .get(&hit.media_item_id)
            .cloned()
            .filter(|s| !s.trim().is_empty()),
        SearchCardKind::MusicVideo => enrichment
            .music_video_artist
            .get(&hit.media_item_id)
            .cloned()
            .filter(|s| !s.is_empty()),
        SearchCardKind::OtherVideo | SearchCardKind::Image | SearchCardKind::RemoteStream => {
            hit.original_title.clone().filter(|s| !s.trim().is_empty())
        }
        SearchCardKind::Song => enrichment
            .song_meta
            .get(&hit.media_item_id)
            .and_then(|(artists, album)| song_subtitle(artists.as_deref(), album.as_deref())),
    }
}

#[cfg(feature = "ssr")]
fn resolve_artwork_path(
    kind: SearchCardKind,
    media_item_id: i64,
    enrichment: &SectionEnrichment,
) -> Option<String> {
    let map = match search_artwork_kind(kind) {
        SearchArtworkKind::Poster => &enrichment.poster,
        SearchArtworkKind::Thumbnail => &enrichment.thumbnail,
    };

    if let Some(path) = map.get(&media_item_id) {
        return Some(path.clone());
    }

    match kind {
        SearchCardKind::Season | SearchCardKind::Episode => enrichment
            .season_poster
            .get(&media_item_id)
            .cloned()
            .or_else(|| enrichment.show_poster.get(&media_item_id).cloned()),
        SearchCardKind::OtherVideo => enrichment.poster.get(&media_item_id).cloned(),
        _ => None,
    }
}

#[cfg(all(test, feature = "ssr"))]
async fn fetch_with_ids(
    pool: &ersatzrs_infra::db::DbPool,
    sql: &str,
    ids: &[i64],
) -> Result<Vec<ersatzrs_infra::db::DbRow>, ServerFnError> {
    let mut q = sqlx::query(ersatzrs_infra::db::assert_sql_safe(sql));
    for id in ids {
        q = q.bind(*id);
    }
    Ok(q.fetch_all(pool).await?)
}

#[cfg(all(test, feature = "ssr"))]
#[allow(clippy::too_many_lines)]
async fn load_enrichment(
    pool: &ersatzrs_infra::db::DbPool,
    kind: SearchCardKind,
    ids: &[i64],
) -> Result<SectionEnrichment, ServerFnError> {
    use sqlx::Row;

    let mut enrichment = SectionEnrichment::default();
    if ids.is_empty() {
        return Ok(enrichment);
    }

    let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");

    // Common: MediaItem.State for every kind.
    {
        let sql =
            format!(r#"SELECT "Id", "State" FROM "MediaItem" WHERE "Id" IN ({placeholders})"#);
        let rows = fetch_with_ids(pool, &sql, ids).await?;
        for row in rows {
            let id: i64 = row.try_get("Id")?;
            let state_value: i32 = row.try_get("State").unwrap_or(0);
            let state = match state_value {
                1 => ersatzrs_domain::MediaItemState::FileNotFound,
                2 => ersatzrs_domain::MediaItemState::Unavailable,
                3 => ersatzrs_domain::MediaItemState::RemoteOnly,
                _ => ersatzrs_domain::MediaItemState::Normal,
            };
            enrichment.state.insert(id, state);
        }
    }

    match kind {
        SearchCardKind::Movie => {
            let sql = format!(
                r#"SELECT mm."MovieId" AS "ItemId", a."Path" AS "Path"
                   FROM "MovieMetadata" mm
                   JOIN "Artwork" a ON a."MovieMetadataId" = mm."Id" AND a."ArtworkKind" = 0
                   WHERE mm."MovieId" IN ({placeholders})"#
            );
            let rows = fetch_with_ids(pool, &sql, ids).await?;
            for row in rows {
                enrichment
                    .poster
                    .insert(row.try_get("ItemId")?, row.try_get("Path")?);
            }
        }
        SearchCardKind::Show => {
            let sql = format!(
                r#"SELECT sm."ShowId" AS "ItemId", a."Path" AS "Path"
                   FROM "ShowMetadata" sm
                   JOIN "Artwork" a ON a."ShowMetadataId" = sm."Id" AND a."ArtworkKind" = 0
                   WHERE sm."ShowId" IN ({placeholders})"#
            );
            let rows = fetch_with_ids(pool, &sql, ids).await?;
            for row in rows {
                enrichment
                    .poster
                    .insert(row.try_get("ItemId")?, row.try_get("Path")?);
            }
        }
        SearchCardKind::Season => {
            // Own poster (SeasonMetadata) + show poster fallback (keyed by season id).
            let sql = format!(
                r#"SELECT sea."Id" AS "SeasonId",
                          sea_art."Path" AS "OwnPath",
                          show_art."Path" AS "ShowPath"
                   FROM "Season" sea
                   LEFT JOIN "SeasonMetadata" sem ON sem."SeasonId" = sea."Id"
                   LEFT JOIN "Artwork" sea_art
                       ON sea_art."SeasonMetadataId" = sem."Id" AND sea_art."ArtworkKind" = 0
                   LEFT JOIN "ShowMetadata" shm ON shm."ShowId" = sea."ShowId"
                   LEFT JOIN "Artwork" show_art
                       ON show_art."ShowMetadataId" = shm."Id" AND show_art."ArtworkKind" = 0
                   WHERE sea."Id" IN ({placeholders})"#
            );
            let rows = fetch_with_ids(pool, &sql, ids).await?;
            for row in rows {
                let id: i64 = row.try_get("SeasonId")?;
                if let Ok(Some(path)) = row.try_get::<Option<String>, _>("OwnPath") {
                    enrichment.poster.insert(id, path);
                }
                if let Ok(Some(path)) = row.try_get::<Option<String>, _>("ShowPath") {
                    enrichment.season_poster.insert(id, path);
                }
            }
        }
        SearchCardKind::Episode => {
            // Own poster + season + show fallback chain, keyed by episode id.
            let sql = format!(
                r#"SELECT ep."Id" AS "EpisodeId",
                          ep."SeasonId" AS "SeasonId",
                          ep_art."Path" AS "OwnPath",
                          sea_art."Path" AS "SeasonPath",
                          show_art."Path" AS "ShowPath"
                   FROM "Episode" ep
                   LEFT JOIN "EpisodeMetadata" em ON em."EpisodeId" = ep."Id"
                   LEFT JOIN "Artwork" ep_art
                       ON ep_art."EpisodeMetadataId" = em."Id" AND ep_art."ArtworkKind" = 0
                   LEFT JOIN "Season" sea ON sea."Id" = ep."SeasonId"
                   LEFT JOIN "SeasonMetadata" sem ON sem."SeasonId" = sea."Id"
                   LEFT JOIN "Artwork" sea_art
                       ON sea_art."SeasonMetadataId" = sem."Id" AND sea_art."ArtworkKind" = 0
                   LEFT JOIN "ShowMetadata" shm ON shm."ShowId" = sea."ShowId"
                   LEFT JOIN "Artwork" show_art
                       ON show_art."ShowMetadataId" = shm."Id" AND show_art."ArtworkKind" = 0
                   WHERE ep."Id" IN ({placeholders})"#
            );
            let rows = fetch_with_ids(pool, &sql, ids).await?;
            for row in rows {
                let id: i64 = row.try_get("EpisodeId")?;
                if let Ok(season_id) = row.try_get::<i64, _>("SeasonId") {
                    enrichment.episode_season_id.insert(id, season_id);
                }
                if let Ok(Some(path)) = row.try_get::<Option<String>, _>("OwnPath") {
                    enrichment.poster.insert(id, path);
                }
                if let Ok(Some(path)) = row.try_get::<Option<String>, _>("SeasonPath") {
                    enrichment.season_poster.insert(id, path);
                }
                if let Ok(Some(path)) = row.try_get::<Option<String>, _>("ShowPath") {
                    enrichment.show_poster.insert(id, path);
                }
            }
        }
        SearchCardKind::Artist => {
            let sql = format!(
                r#"SELECT am."ArtistId" AS "ItemId",
                          am."Disambiguation" AS "Disambiguation",
                          a."Path" AS "Path"
                   FROM "ArtistMetadata" am
                   LEFT JOIN "Artwork" a ON a."ArtistMetadataId" = am."Id" AND a."ArtworkKind" = 1
                   WHERE am."ArtistId" IN ({placeholders})"#
            );
            let rows = fetch_with_ids(pool, &sql, ids).await?;
            for row in rows {
                let id: i64 = row.try_get("ItemId")?;
                if let Ok(Some(path)) = row.try_get::<Option<String>, _>("Path") {
                    enrichment.thumbnail.insert(id, path);
                }
                if let Ok(Some(disambiguation)) = row.try_get::<Option<String>, _>("Disambiguation")
                {
                    enrichment.artist_disambiguation.insert(id, disambiguation);
                }
            }
        }
        SearchCardKind::MusicVideo => {
            let sql = format!(
                r#"SELECT mv."Id" AS "ItemId",
                          mv."ArtistId" AS "ArtistId",
                          a."Path" AS "Path",
                          am."Title" AS "ArtistTitle"
                   FROM "MusicVideo" mv
                   LEFT JOIN "MusicVideoMetadata" mvm ON mvm."MusicVideoId" = mv."Id"
                   LEFT JOIN "Artwork" a
                       ON a."MusicVideoMetadataId" = mvm."Id" AND a."ArtworkKind" = 1
                   LEFT JOIN "ArtistMetadata" am ON am."ArtistId" = mv."ArtistId"
                   WHERE mv."Id" IN ({placeholders})"#
            );
            let rows = fetch_with_ids(pool, &sql, ids).await?;
            for row in rows {
                let id: i64 = row.try_get("ItemId")?;
                if let Ok(Some(artist_id)) = row.try_get::<Option<i64>, _>("ArtistId") {
                    enrichment.music_video_artist_id.insert(id, artist_id);
                }
                if let Ok(Some(path)) = row.try_get::<Option<String>, _>("Path") {
                    enrichment.thumbnail.insert(id, path);
                }
                if let Ok(Some(artist)) = row.try_get::<Option<String>, _>("ArtistTitle") {
                    enrichment.music_video_artist.insert(id, artist);
                }
            }
        }
        SearchCardKind::OtherVideo => {
            // Thumbnail-first, then poster fallback (matches `Mapper`).
            let sql = format!(
                r#"SELECT ovm."OtherVideoId" AS "ItemId",
                          thumb."Path" AS "ThumbPath",
                          poster."Path" AS "PosterPath"
                   FROM "OtherVideoMetadata" ovm
                   LEFT JOIN "Artwork" thumb
                       ON thumb."OtherVideoMetadataId" = ovm."Id" AND thumb."ArtworkKind" = 1
                   LEFT JOIN "Artwork" poster
                       ON poster."OtherVideoMetadataId" = ovm."Id" AND poster."ArtworkKind" = 0
                   WHERE ovm."OtherVideoId" IN ({placeholders})"#
            );
            let rows = fetch_with_ids(pool, &sql, ids).await?;
            for row in rows {
                let id: i64 = row.try_get("ItemId")?;
                if let Ok(Some(path)) = row.try_get::<Option<String>, _>("ThumbPath") {
                    enrichment.thumbnail.insert(id, path);
                }
                if let Ok(Some(path)) = row.try_get::<Option<String>, _>("PosterPath") {
                    enrichment.poster.insert(id, path);
                }
            }
        }
        SearchCardKind::Song => {
            let sql = format!(
                r#"SELECT som."SongId" AS "ItemId",
                          a."Path" AS "Path",
                          som."Artists" AS "Artists",
                          som."Album" AS "Album"
                   FROM "SongMetadata" som
                   LEFT JOIN "Artwork" a
                       ON a."SongMetadataId" = som."Id" AND a."ArtworkKind" = 1
                   WHERE som."SongId" IN ({placeholders})"#
            );
            let rows = fetch_with_ids(pool, &sql, ids).await?;
            for row in rows {
                let id: i64 = row.try_get("ItemId")?;
                if let Ok(Some(path)) = row.try_get::<Option<String>, _>("Path") {
                    enrichment.thumbnail.insert(id, path);
                }
                let artists: Option<String> = row.try_get("Artists").ok().flatten();
                let album: Option<String> = row.try_get("Album").ok().flatten();
                enrichment.song_meta.insert(id, (artists, album));
            }
        }
        SearchCardKind::Image => {
            let sql = format!(
                r#"SELECT im."ImageId" AS "ItemId", a."Path" AS "Path"
                   FROM "ImageMetadata" im
                   LEFT JOIN "Artwork" a
                       ON a."ImageMetadataId" = im."Id" AND a."ArtworkKind" = 1
                   WHERE im."ImageId" IN ({placeholders})"#
            );
            let rows = fetch_with_ids(pool, &sql, ids).await?;
            for row in rows {
                let id: i64 = row.try_get("ItemId")?;
                if let Ok(Some(path)) = row.try_get::<Option<String>, _>("Path") {
                    enrichment.thumbnail.insert(id, path);
                }
            }
        }
        SearchCardKind::RemoteStream => {
            let sql = format!(
                r#"SELECT rsm."RemoteStreamId" AS "ItemId", a."Path" AS "Path"
                   FROM "RemoteStreamMetadata" rsm
                   LEFT JOIN "Artwork" a
                       ON a."RemoteStreamMetadataId" = rsm."Id" AND a."ArtworkKind" = 1
                   WHERE rsm."RemoteStreamId" IN ({placeholders})"#
            );
            let rows = fetch_with_ids(pool, &sql, ids).await?;
            for row in rows {
                let id: i64 = row.try_get("ItemId")?;
                if let Ok(Some(path)) = row.try_get::<Option<String>, _>("Path") {
                    enrichment.thumbnail.insert(id, path);
                }
            }
        }
    }

    Ok(enrichment)
}

// ---------------------------------------------------------------------------
// Tests for pure projection helpers.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_uses_uppercase_first_letter_for_default_kinds() {
        assert_eq!(
            search_placeholder(SearchCardKind::Movie, "alpha", "Alpha", None),
            "A"
        );
        assert_eq!(
            search_placeholder(SearchCardKind::Show, "zebra", "Zebra", None),
            "Z"
        );
    }

    #[test]
    fn placeholder_falls_back_to_hash_for_non_alpha_and_empty() {
        assert_eq!(
            search_placeholder(SearchCardKind::Movie, "9 to 5", "9 to 5", None),
            "#"
        );
        assert_eq!(search_placeholder(SearchCardKind::Movie, "", "", None), "#");
    }

    #[test]
    fn placeholder_for_season_zero_is_capital_s() {
        assert_eq!(
            search_placeholder(SearchCardKind::Season, "Specials", "Specials", Some(0)),
            "S"
        );
    }

    #[test]
    fn placeholder_for_season_uses_number() {
        assert_eq!(
            search_placeholder(SearchCardKind::Season, "Season 03", "Season 3", Some(3)),
            "3"
        );
    }

    #[test]
    fn season_subtitle_specials_vs_numbered() {
        assert_eq!(season_subtitle(Some(0)).as_deref(), Some("Specials"));
        assert_eq!(season_subtitle(Some(2)).as_deref(), Some("Season 2"));
        assert_eq!(season_subtitle(None), None);
    }

    #[test]
    fn episode_subtitle_full_form_and_fallbacks() {
        assert_eq!(
            episode_subtitle(Some("Severance"), Some(2), Some(5)).as_deref(),
            Some("Severance - S2 E5")
        );
        assert_eq!(
            episode_subtitle(None, Some(1), Some(1)).as_deref(),
            Some("S1 E1")
        );
        assert_eq!(
            episode_subtitle(Some("Show"), None, None).as_deref(),
            Some("Show")
        );
        assert_eq!(episode_subtitle(None, None, None), None);
    }

    #[test]
    fn song_subtitle_joins_artists_and_album() {
        assert_eq!(
            song_subtitle(Some("Boards of Canada"), Some("Geogaddi")).as_deref(),
            Some("Boards of Canada - Geogaddi")
        );
        assert_eq!(
            song_subtitle(Some("Just Artists"), None).as_deref(),
            Some("Just Artists")
        );
        assert_eq!(
            song_subtitle(None, Some("Only Album")).as_deref(),
            Some("Only Album")
        );
        assert_eq!(song_subtitle(Some(""), Some("")), None);
        assert_eq!(song_subtitle(None, None), None);
    }

    #[test]
    fn href_targets_match_per_section_rules() {
        assert_eq!(
            search_href(SearchCardKind::Movie, 42, None),
            "/media/movies/42"
        );
        assert_eq!(
            search_href(SearchCardKind::Show, 7, None),
            "/media/tv/shows/7"
        );
        assert_eq!(
            search_href(SearchCardKind::Season, 11, None),
            "/media/tv/seasons/11"
        );
        assert_eq!(
            search_href(SearchCardKind::Episode, 99, Some(11)),
            "/media/tv/seasons/11#episode-99"
        );
        assert_eq!(search_href(SearchCardKind::Episode, 99, None), "");
        assert_eq!(
            search_href(SearchCardKind::Artist, 5, None),
            "/media/music/artists/5"
        );
        assert_eq!(
            search_href(SearchCardKind::MusicVideo, 15, Some(5)),
            "/media/music/videos/15"
        );
        assert_eq!(
            search_href(SearchCardKind::MusicVideo, 15, None),
            "/media/music/videos/15"
        );
        assert_eq!(
            search_href(SearchCardKind::OtherVideo, 17, None),
            "/media/other/videos/17"
        );
        assert_eq!(
            search_href(SearchCardKind::Song, 1, None),
            "/media/music/songs/1"
        );
        assert_eq!(
            search_href(SearchCardKind::Image, 1, None),
            "/media/browser/images/1"
        );
        assert_eq!(search_href(SearchCardKind::RemoteStream, 1, None), "");
    }

    #[test]
    fn artwork_kind_per_section_matches_mapper() {
        assert_eq!(
            search_artwork_kind(SearchCardKind::Movie),
            SearchArtworkKind::Poster
        );
        assert_eq!(
            search_artwork_kind(SearchCardKind::Show),
            SearchArtworkKind::Poster
        );
        assert_eq!(
            search_artwork_kind(SearchCardKind::Season),
            SearchArtworkKind::Poster
        );
        assert_eq!(
            search_artwork_kind(SearchCardKind::Episode),
            SearchArtworkKind::Poster
        );
        assert_eq!(
            search_artwork_kind(SearchCardKind::Artist),
            SearchArtworkKind::Thumbnail
        );
        assert_eq!(
            search_artwork_kind(SearchCardKind::MusicVideo),
            SearchArtworkKind::Thumbnail
        );
        assert_eq!(
            search_artwork_kind(SearchCardKind::Song),
            SearchArtworkKind::Thumbnail
        );
    }

    #[cfg(feature = "ssr")]
    #[test]
    fn card_projection_builds_correct_url_family_for_every_kind() {
        use ersatzrs_core::search::SearchDocument;

        let kinds = [
            SearchCardKind::Movie,
            SearchCardKind::Show,
            SearchCardKind::Season,
            SearchCardKind::Episode,
            SearchCardKind::Artist,
            SearchCardKind::MusicVideo,
            SearchCardKind::OtherVideo,
            SearchCardKind::Song,
            SearchCardKind::Image,
            SearchCardKind::RemoteStream,
        ];
        for (offset, kind) in kinds.into_iter().enumerate() {
            let id = i64::try_from(offset + 1).unwrap();
            let mut enrichment = SectionEnrichment::default();
            let expected_family = match search_artwork_kind(kind) {
                SearchArtworkKind::Poster => {
                    enrichment.poster.insert(id, format!("poster-{id}.jpg"));
                    "posters"
                }
                SearchArtworkKind::Thumbnail => {
                    enrichment
                        .thumbnail
                        .insert(id, format!("thumbnail-{id}.jpg"));
                    "thumbnails"
                }
            };
            let hit = SearchDocument {
                media_item_id: id,
                media_kind: kind.media_kind().to_owned(),
                title: Some(format!("Card {id}")),
                sort_title: Some(format!("card {id}")),
                ..SearchDocument::default()
            };

            assert_eq!(
                build_card(kind, &hit, &enrichment).artwork_url,
                Some(format!(
                    "/artwork/{expected_family}/{}-{id}.jpg",
                    if expected_family == "posters" {
                        "poster"
                    } else {
                        "thumbnail"
                    }
                ))
            );
        }
    }

    #[test]
    fn search_card_kind_media_kind_strings_match_index() {
        assert_eq!(SearchCardKind::Movie.media_kind(), "movie");
        assert_eq!(SearchCardKind::MusicVideo.media_kind(), "music_video");
        assert_eq!(SearchCardKind::RemoteStream.media_kind(), "remote_stream");
    }
}

#[cfg(all(test, feature = "ssr"))]
mod bulk_id_tests {
    use super::*;
    use ersatzrs_core::search::SearchDocument;

    fn hit(media_item_id: i64, media_kind: &str) -> SearchDocument {
        SearchDocument {
            media_item_id,
            media_kind: media_kind.to_string(),
            ..SearchDocument::default()
        }
    }

    #[test]
    fn bucket_search_all_ids_groups_every_supported_kind() {
        let ids = bucket_search_all_ids(vec![
            hit(1, "movie"),
            hit(2, "show"),
            hit(3, "season"),
            hit(4, "episode"),
            hit(5, "artist"),
            hit(6, "music_video"),
            hit(7, "other_video"),
            hit(8, "song"),
            hit(9, "image"),
            hit(10, "remote_stream"),
            hit(99, "unknown"),
        ]);

        assert_eq!(ids.movie_ids, vec![1]);
        assert_eq!(ids.show_ids, vec![2]);
        assert_eq!(ids.season_ids, vec![3]);
        assert_eq!(ids.episode_ids, vec![4]);
        assert_eq!(ids.artist_ids, vec![5]);
        assert_eq!(ids.music_video_ids, vec![6]);
        assert_eq!(ids.other_video_ids, vec![7]);
        assert_eq!(ids.song_ids, vec![8]);
        assert_eq!(ids.image_ids, vec![9]);
        assert_eq!(ids.remote_stream_ids, vec![10]);
    }
}

#[cfg(all(test, feature = "ssr", not(feature = "mysql")))]
mod section_tests {
    //! DB-backed projection tests. Seed a tiny fixture (one show + two
    //! seasons, one episode without its own poster, one season-0) and verify
    //! the poster-fallback chain in `project_section`.

    use super::*;
    use ersatzrs_core::search::SearchDocument;

    async fn fresh_pool() -> ersatzrs_infra::db::DbPool {
        let dir = tempfile::tempdir().expect("temp db dir");
        let url = format!("sqlite:{}", dir.path().join("search.db").display());
        // Leak the tempdir so the file outlives this future.
        std::mem::forget(dir);
        ersatzrs_infra::db::init(&url)
            .await
            .expect("initialize test database")
    }

    async fn seed_one_library_and_path(pool: &ersatzrs_infra::db::DbPool) {
        sqlx::query(r#"INSERT INTO "MediaSource" ("Id") VALUES (1)"#)
            .execute(pool)
            .await
            .expect("seed media source");
        sqlx::query(
            r#"INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
               VALUES (1, 'lib', 2, 1)"#,
        )
        .execute(pool)
        .await
        .expect("seed library");
        sqlx::query(r#"INSERT INTO "LibraryPath" ("Id", "Path", "LibraryId") VALUES (1, '/m', 1)"#)
            .execute(pool)
            .await
            .expect("seed library path");
    }

    async fn insert_media_item(pool: &ersatzrs_infra::db::DbPool, id: i64) {
        sqlx::query(r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (?, 1, 0)"#)
            .bind(id)
            .execute(pool)
            .await
            .expect("insert media item");
    }

    async fn insert_artwork(
        pool: &ersatzrs_infra::db::DbPool,
        metadata_id: i64,
        fk_column: &str,
        kind: i32,
        path: &str,
    ) {
        let sql = format!(
            r#"INSERT INTO "Artwork" ("ArtworkKind", "Path", "{fk_column}", "DateAdded", "DateUpdated")
               VALUES (?, ?, ?, '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z')"#
        );
        sqlx::query(ersatzrs_infra::db::assert_sql_safe(&sql))
            .bind(kind)
            .bind(path)
            .bind(metadata_id)
            .execute(pool)
            .await
            .expect("insert artwork");
    }

    #[tokio::test]
    async fn project_section_episode_falls_back_to_season_then_show_poster() {
        let pool = fresh_pool().await;
        seed_one_library_and_path(&pool).await;

        // Show 100 with poster.
        insert_media_item(&pool, 100).await;
        sqlx::query(r#"INSERT INTO "Show" ("Id") VALUES (100)"#)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            r#"INSERT INTO "ShowMetadata"
                   ("Id", "ShowId", "MetadataKind", "Title", "SortTitle",
                    "DateAdded", "DateUpdated")
               VALUES (200, 100, 0, 'Show A', 'show a',
                       '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z')"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        insert_artwork(&pool, 200, "ShowMetadataId", 0, "show-poster.jpg").await;

        // Season 0 of Show 100 (no own poster).
        insert_media_item(&pool, 110).await;
        sqlx::query(
            r#"INSERT INTO "Season" ("Id", "ShowId", "SeasonNumber") VALUES (110, 100, 0)"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"INSERT INTO "SeasonMetadata"
                   ("Id", "SeasonId", "MetadataKind", "Title", "SortTitle",
                    "DateAdded", "DateUpdated")
               VALUES (210, 110, 0, 'Specials', 'specials',
                       '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z')"#,
        )
        .execute(&pool)
        .await
        .unwrap();

        // Episode 120 of Season 110 (no own poster, no season poster).
        insert_media_item(&pool, 120).await;
        sqlx::query(r#"INSERT INTO "Episode" ("Id", "SeasonId") VALUES (120, 110)"#)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            r#"INSERT INTO "EpisodeMetadata"
                   ("Id", "EpisodeId", "MetadataKind", "Title", "SortTitle",
                    "EpisodeNumber", "DateAdded", "DateUpdated")
               VALUES (220, 120, 0, 'Pilot', 'pilot', 1,
                       '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z')"#,
        )
        .execute(&pool)
        .await
        .unwrap();

        // Build a SearchDocument for the episode hit.
        let hit = SearchDocument {
            media_item_id: 120,
            media_kind: "episode".to_string(),
            metadata_kind: "Episode".to_string(),
            title: Some("Pilot".to_string()),
            sort_title: Some("pilot".to_string()),
            show_title: Some("Show A".to_string()),
            season_number: Some(0),
            episode_number: Some(1),
            ..SearchDocument::default()
        };

        let cards = project_section(&pool, SearchCardKind::Episode, vec![hit])
            .await
            .expect("project episode section");

        assert_eq!(cards.len(), 1);
        let card = &cards[0];
        assert_eq!(card.media_item_id, 120);
        assert_eq!(card.kind, SearchCardKind::Episode);
        // Episode has no own poster, season has no poster → falls back to show poster.
        let url = card.artwork_url.as_deref().expect("artwork url");
        assert!(
            url.contains("show-poster.jpg"),
            "expected show-poster fallback, got {url}"
        );
        // Episode href links to the season anchor.
        assert_eq!(card.href, "/media/tv/seasons/110#episode-120");
        // Subtitle is "{show} - S{season} E{episode}".
        assert_eq!(card.subtitle.as_deref(), Some("Show A - S0 E1"));
    }

    #[tokio::test]
    async fn project_section_season_zero_uses_capital_s_placeholder() {
        let pool = fresh_pool().await;
        seed_one_library_and_path(&pool).await;

        insert_media_item(&pool, 300).await;
        sqlx::query(r#"INSERT INTO "Show" ("Id") VALUES (300)"#)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            r#"INSERT INTO "ShowMetadata"
                   ("Id", "ShowId", "MetadataKind", "Title", "SortTitle",
                    "DateAdded", "DateUpdated")
               VALUES (400, 300, 0, 'Show', 'show',
                       '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z')"#,
        )
        .execute(&pool)
        .await
        .unwrap();

        // Season 0, no artwork.
        insert_media_item(&pool, 310).await;
        sqlx::query(
            r#"INSERT INTO "Season" ("Id", "ShowId", "SeasonNumber") VALUES (310, 300, 0)"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"INSERT INTO "SeasonMetadata"
                   ("Id", "SeasonId", "MetadataKind", "Title", "SortTitle",
                    "DateAdded", "DateUpdated")
               VALUES (410, 310, 0, 'Specials', 'specials',
                       '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z')"#,
        )
        .execute(&pool)
        .await
        .unwrap();

        let hit = SearchDocument {
            media_item_id: 310,
            media_kind: "season".to_string(),
            metadata_kind: "Season".to_string(),
            title: Some("Specials".to_string()),
            sort_title: Some("specials".to_string()),
            season_number: Some(0),
            ..SearchDocument::default()
        };

        let cards = project_section(&pool, SearchCardKind::Season, vec![hit])
            .await
            .expect("project season section");

        let card = &cards[0];
        assert_eq!(card.placeholder, "S");
        assert_eq!(card.subtitle.as_deref(), Some("Specials"));
        assert_eq!(card.href, "/media/tv/seasons/310");
    }

    #[tokio::test]
    async fn project_section_season_falls_back_to_show_poster() {
        let pool = fresh_pool().await;
        seed_one_library_and_path(&pool).await;

        insert_media_item(&pool, 500).await;
        sqlx::query(r#"INSERT INTO "Show" ("Id") VALUES (500)"#)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            r#"INSERT INTO "ShowMetadata"
                   ("Id", "ShowId", "MetadataKind", "Title", "SortTitle",
                    "DateAdded", "DateUpdated")
               VALUES (600, 500, 0, 'Show', 'show',
                       '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z')"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        insert_artwork(&pool, 600, "ShowMetadataId", 0, "show-fallback.jpg").await;

        // Season without its own poster.
        insert_media_item(&pool, 510).await;
        sqlx::query(
            r#"INSERT INTO "Season" ("Id", "ShowId", "SeasonNumber") VALUES (510, 500, 2)"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"INSERT INTO "SeasonMetadata"
                   ("Id", "SeasonId", "MetadataKind", "Title", "SortTitle",
                    "DateAdded", "DateUpdated")
               VALUES (610, 510, 0, 'Season 2', 'season 2',
                       '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z')"#,
        )
        .execute(&pool)
        .await
        .unwrap();

        let hit = SearchDocument {
            media_item_id: 510,
            media_kind: "season".to_string(),
            metadata_kind: "Season".to_string(),
            title: Some("Season 2".to_string()),
            sort_title: Some("season 2".to_string()),
            season_number: Some(2),
            ..SearchDocument::default()
        };

        let cards = project_section(&pool, SearchCardKind::Season, vec![hit])
            .await
            .expect("project season section");
        let url = cards[0]
            .artwork_url
            .as_deref()
            .expect("show fallback artwork");
        assert!(
            url.contains("show-fallback.jpg"),
            "expected show fallback, got {url}"
        );
    }
}
