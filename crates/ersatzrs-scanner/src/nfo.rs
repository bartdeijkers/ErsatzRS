//! Local sidecar NFO parsing for Kodi/Plex-style metadata files.
//!
//! `ErsatzTV` reads these files with tolerant XML readers: input is sanitized
//! first, missing elements keep their defaults, and malformed integer/date
//! fields are ignored. This module keeps that behavior while using
//! `quick-xml`/`serde` for the field mapping.

use std::fs;
use std::path::Path;

use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use quick_xml::de;
use quick_xml::events::Event;
use quick_xml::Reader;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use thiserror::Error;

/// Error returned while loading or parsing an NFO sidecar.
#[derive(Debug, Error)]
pub enum NfoError {
    /// The NFO file could not be read.
    #[error("failed to read NFO file {path}: {source}")]
    Io {
        /// Path that failed to load.
        path: String,
        /// Underlying filesystem error.
        source: std::io::Error,
    },
    /// XML deserialization failed after the file was sanitized.
    #[error("failed to parse NFO XML: {0}")]
    Xml(#[from] de::DeError),
    /// The expected root element was not present.
    #[error("NFO did not contain a <{0}> root element")]
    MissingRoot(&'static str),
}

/// Result alias for NFO parsing.
pub type Result<T> = std::result::Result<T, NfoError>;

/// One `<actor>` entry in a local NFO.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NfoActor {
    /// Actor display name.
    pub name: Option<String>,
    /// Character or role name.
    pub role: Option<String>,
    /// Source-provided actor order.
    pub order: Option<i32>,
    /// Actor thumbnail path or URL, when present.
    pub thumb: Option<String>,
}

/// One `<uniqueid>` entry in a local NFO.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NfoGuid {
    /// Whether the source marked this as the default id.
    pub is_default: bool,
    /// Provider kind from the `type` attribute, for example `imdb`.
    pub source: Option<String>,
    /// Provider id text.
    pub value: Option<String>,
}

impl NfoGuid {
    /// Return the `ErsatzTV` persisted GUID shape, `source://value`, when both
    /// pieces are present.
    #[must_use]
    pub fn as_metadata_guid(&self) -> Option<String> {
        Some(format!(
            "{}://{}",
            self.source.as_ref()?,
            self.value.as_ref()?
        ))
    }
}

/// Parsed `<tvshow>` sidecar metadata.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TvShowNfo {
    /// Show title.
    pub title: Option<String>,
    /// Explicit sort title, if supplied by the sidecar.
    pub sort_title: Option<String>,
    /// Release year.
    pub year: Option<i32>,
    /// Full plot.
    pub plot: Option<String>,
    /// Short outline.
    pub outline: Option<String>,
    /// Tagline.
    pub tagline: Option<String>,
    /// MPAA/content rating.
    pub content_rating: Option<String>,
    /// Premiered date.
    pub premiered: Option<DateTime<Utc>>,
    /// Genre rows.
    pub genres: Vec<String>,
    /// Tag rows.
    pub tags: Vec<String>,
    /// Studio rows.
    pub studios: Vec<String>,
    /// Actor rows.
    pub actors: Vec<NfoActor>,
    /// Provider GUID rows.
    pub guids: Vec<NfoGuid>,
}

/// Parsed season sidecar metadata.
///
/// `ErsatzTV` does not currently have a dedicated season NFO reader, but the
/// local TV scanner slice needs this small shape for future `season.nfo`
/// support and harmless fallback when no file exists.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SeasonNfo {
    /// Season title.
    pub title: Option<String>,
    /// Explicit sort title, if supplied by the sidecar.
    pub sort_title: Option<String>,
    /// Release year.
    pub year: Option<i32>,
    /// Premiered date.
    pub premiered: Option<DateTime<Utc>>,
}

/// Parsed `<episodedetails>` sidecar metadata.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EpisodeNfo {
    /// Parent show title.
    pub show_title: Option<String>,
    /// Episode title.
    pub title: Option<String>,
    /// Season number.
    pub season: Option<i32>,
    /// Episode number.
    pub episode: Option<i32>,
    /// MPAA/content rating.
    pub content_rating: Option<String>,
    /// Aired date.
    pub aired: Option<DateTime<Utc>>,
    /// Premiered date, accepted for Kodi variants that use this name.
    pub premiered: Option<DateTime<Utc>>,
    /// Release year, when supplied explicitly.
    pub year: Option<i32>,
    /// Full plot.
    pub plot: Option<String>,
    /// Genre rows.
    pub genres: Vec<String>,
    /// Tag rows.
    pub tags: Vec<String>,
    /// Actor rows.
    pub actors: Vec<NfoActor>,
    /// Writer names from `<credits>`.
    pub writers: Vec<String>,
    /// Director names.
    pub directors: Vec<String>,
    /// Provider GUID rows.
    pub guids: Vec<NfoGuid>,
}

/// Parsed `<artist>` sidecar metadata.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ArtistNfo {
    /// Artist name.
    pub name: Option<String>,
    /// Explicit sort title, if supplied by the sidecar.
    pub sort_title: Option<String>,
    /// Disambiguation text.
    pub disambiguation: Option<String>,
    /// Biography text.
    pub biography: Option<String>,
    /// Artist year.
    pub year: Option<i32>,
    /// Formed date/year text normalized to a date when possible.
    pub formed: Option<DateTime<Utc>>,
    /// Genre rows.
    pub genres: Vec<String>,
    /// Style rows.
    pub styles: Vec<String>,
    /// Mood rows.
    pub moods: Vec<String>,
}

/// Parsed `<musicvideo>` sidecar metadata.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MusicVideoNfo {
    /// Song/video title.
    pub title: Option<String>,
    /// Album title.
    pub album: Option<String>,
    /// Full plot.
    pub plot: Option<String>,
    /// Release year.
    pub year: Option<i32>,
    /// Aired date.
    pub aired: Option<DateTime<Utc>>,
    /// Premiered date, accepted for Kodi variants that use this name.
    pub premiered: Option<DateTime<Utc>>,
    /// Track number.
    pub track: Option<i32>,
    /// Artist names.
    pub artists: Vec<String>,
    /// Director names.
    pub directors: Vec<String>,
    /// Genre rows.
    pub genres: Vec<String>,
    /// Tag rows.
    pub tags: Vec<String>,
    /// Studio rows.
    pub studios: Vec<String>,
}

/// Parse a `tvshow.nfo` file.
///
/// # Errors
///
/// Returns an error when the file cannot be read, the expected root is absent,
/// or XML deserialization fails.
pub fn parse_tvshow_nfo(path: impl AsRef<Path>) -> Result<TvShowNfo> {
    parse_file::<RawTvShowNfo, _>(path, "tvshow", TvShowNfo::from)
}

/// Parse a `tvshow.nfo` XML string.
///
/// # Errors
///
/// Returns an error when the expected root is absent or XML deserialization
/// fails.
pub fn parse_tvshow_nfo_str(input: &str) -> Result<TvShowNfo> {
    parse_str::<RawTvShowNfo, _>(input, "tvshow", TvShowNfo::from)
}

/// Parse a season NFO file.
///
/// # Errors
///
/// Returns an error when the file cannot be read, the expected root is absent,
/// or XML deserialization fails.
pub fn parse_season_nfo(path: impl AsRef<Path>) -> Result<SeasonNfo> {
    parse_file::<RawSeasonNfo, _>(path, "season", SeasonNfo::from)
}

/// Parse a season NFO XML string.
///
/// # Errors
///
/// Returns an error when the expected root is absent or XML deserialization
/// fails.
pub fn parse_season_nfo_str(input: &str) -> Result<SeasonNfo> {
    parse_str::<RawSeasonNfo, _>(input, "season", SeasonNfo::from)
}

/// Parse an episode NFO file and return the first `<episodedetails>` entry.
///
/// # Errors
///
/// Returns an error when the file cannot be read, no episode entry exists, or
/// XML deserialization fails.
pub fn parse_episode_nfo(path: impl AsRef<Path>) -> Result<EpisodeNfo> {
    parse_episode_nfos(path)?
        .into_iter()
        .next()
        .ok_or(NfoError::MissingRoot("episodedetails"))
}

/// Parse an episode NFO XML string and return the first `<episodedetails>`.
///
/// # Errors
///
/// Returns an error when no episode entry exists or XML deserialization fails.
pub fn parse_episode_nfo_str(input: &str) -> Result<EpisodeNfo> {
    parse_episode_nfos_str(input)?
        .into_iter()
        .next()
        .ok_or(NfoError::MissingRoot("episodedetails"))
}

/// Parse an episode NFO file. Some Kodi files contain multiple
/// `<episodedetails>` fragments in one sidecar, matching `ErsatzTV`'s reader.
///
/// # Errors
///
/// Returns an error when the file cannot be read, no episode entry exists, or
/// XML deserialization fails.
pub fn parse_episode_nfos(path: impl AsRef<Path>) -> Result<Vec<EpisodeNfo>> {
    let input = read_sanitized_file(path)?;
    parse_episode_nfos_str(&input)
}

/// Parse an episode NFO XML string that may contain one or more
/// `<episodedetails>` fragments.
///
/// # Errors
///
/// Returns an error when no episode entry exists or XML deserialization fails.
pub fn parse_episode_nfos_str(input: &str) -> Result<Vec<EpisodeNfo>> {
    let sanitized = sanitize_nfo_text(input);
    if !contains_root(&sanitized, "episodedetails") {
        return Err(NfoError::MissingRoot("episodedetails"));
    }

    let wrapped = format!("<episodes>{}</episodes>", strip_xml_declaration(&sanitized));
    let raw: RawEpisodeList = de::from_str(&wrapped)?;
    Ok(raw.episodes.into_iter().map(EpisodeNfo::from).collect())
}

/// Parse an `artist.nfo` file.
///
/// # Errors
///
/// Returns an error when the file cannot be read, the expected root is absent,
/// or XML deserialization fails.
pub fn parse_artist_nfo(path: impl AsRef<Path>) -> Result<ArtistNfo> {
    parse_file::<RawArtistNfo, _>(path, "artist", ArtistNfo::from)
}

/// Parse an `artist.nfo` XML string.
///
/// # Errors
///
/// Returns an error when the expected root is absent or XML deserialization
/// fails.
pub fn parse_artist_nfo_str(input: &str) -> Result<ArtistNfo> {
    parse_str::<RawArtistNfo, _>(input, "artist", ArtistNfo::from)
}

/// Parse a music-video NFO file.
///
/// # Errors
///
/// Returns an error when the file cannot be read, the expected root is absent,
/// or XML deserialization fails.
pub fn parse_music_video_nfo(path: impl AsRef<Path>) -> Result<MusicVideoNfo> {
    parse_file::<RawMusicVideoNfo, _>(path, "musicvideo", MusicVideoNfo::from)
}

/// Parse a music-video NFO XML string.
///
/// # Errors
///
/// Returns an error when the expected root is absent or XML deserialization
/// fails.
pub fn parse_music_video_nfo_str(input: &str) -> Result<MusicVideoNfo> {
    parse_str::<RawMusicVideoNfo, _>(input, "musicvideo", MusicVideoNfo::from)
}

/// Return the ErsatzTV-compatible sort title.
///
/// This ports `$ERSATZTV_SRC/ErsatzTV.Core/Metadata/SortTitle.cs`: blank input
/// is returned unchanged, leading English articles are removed, and a leading
/// uppercase `AE` ligature is normalized to `E`.
#[must_use]
pub fn sort_title(title: &str) -> String {
    if title.trim().is_empty() {
        return title.to_owned();
    }

    if title.len() >= 4 && title[..4].eq_ignore_ascii_case("the ") {
        return title[4..].to_owned();
    }

    if title.len() >= 2 && title[..2].eq_ignore_ascii_case("a ") {
        return title[2..].to_owned();
    }

    if title.len() >= 3 && title[..3].eq_ignore_ascii_case("an ") {
        return title[3..].to_owned();
    }

    if title.starts_with('\u{00c6}') || title.starts_with('\u{00e6}') {
        return title.replace('\u{00c6}', "E");
    }

    title.to_owned()
}

fn parse_file<T, U>(
    path: impl AsRef<Path>,
    root: &'static str,
    map: impl FnOnce(T) -> U,
) -> Result<U>
where
    T: DeserializeOwned,
{
    let input = read_sanitized_file(path)?;
    parse_str(&input, root, map)
}

fn parse_str<T, U>(input: &str, root: &'static str, map: impl FnOnce(T) -> U) -> Result<U>
where
    T: DeserializeOwned,
{
    let sanitized = sanitize_nfo_text(input);
    if !contains_root(&sanitized, root) {
        return Err(NfoError::MissingRoot(root));
    }

    let raw: T = de::from_str(&sanitized)?;
    Ok(map(raw))
}

fn read_sanitized_file(path: impl AsRef<Path>) -> Result<String> {
    let path = path.as_ref();
    let input = fs::read_to_string(path).map_err(|source| NfoError::Io {
        path: path.display().to_string(),
        source,
    })?;
    Ok(sanitize_nfo_text(&input))
}

fn sanitize_nfo_text(input: &str) -> String {
    input
        .trim_matches(|c| matches!(c, '\u{feff}' | '\u{200b}'))
        .chars()
        .map(|c| {
            if c.is_control() && !matches!(c, '\r' | '\n' | '\t') {
                '\u{fffd}'
            } else {
                c
            }
        })
        .collect()
}

fn contains_root(input: &str, root: &str) -> bool {
    let mut reader = Reader::from_str(input);
    reader.config_mut().trim_text(true);

    loop {
        match reader.read_event() {
            Ok(Event::Start(element) | Event::Empty(element))
                if element
                    .name()
                    .as_ref()
                    .eq_ignore_ascii_case(root.as_bytes()) =>
            {
                return true;
            }
            Ok(Event::Eof) | Err(_) => return false,
            _ => {}
        }
    }
}

fn strip_xml_declaration(input: &str) -> &str {
    let trimmed = input.trim_start();
    if let Some(rest) = trimmed.strip_prefix("<?xml") {
        if let Some(end) = rest.find("?>") {
            return rest[end + 2..].trim_start();
        }
    }
    input
}

fn parse_i32(value: Option<String>) -> Option<i32> {
    value?.trim().parse().ok()
}

fn parse_bool(value: Option<&str>) -> bool {
    value.is_some_and(|value| value.eq_ignore_ascii_case("true"))
}

fn parse_datetime(value: Option<String>) -> Option<DateTime<Utc>> {
    let value = value?;
    let value = value.trim();
    if value.is_empty() {
        return None;
    }

    if let Ok(value) = DateTime::parse_from_rfc3339(value) {
        return Some(value.with_timezone(&Utc));
    }

    if let Ok(value) = NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S") {
        return Some(DateTime::from_naive_utc_and_offset(value, Utc));
    }

    if let Ok(value) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        return value
            .and_hms_opt(0, 0, 0)
            .map(|value| DateTime::from_naive_utc_and_offset(value, Utc));
    }

    value
        .parse::<i32>()
        .ok()
        .and_then(|year| NaiveDate::from_ymd_opt(year, 1, 1))
        .and_then(|value| value.and_hms_opt(0, 0, 0))
        .map(|value| DateTime::from_naive_utc_and_offset(value, Utc))
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct RawActorNfo {
    name: Option<String>,
    role: Option<String>,
    order: Option<String>,
    thumb: Option<String>,
}

impl From<RawActorNfo> for NfoActor {
    fn from(value: RawActorNfo) -> Self {
        Self {
            name: value.name,
            role: value.role,
            order: parse_i32(value.order),
            thumb: value.thumb,
        }
    }
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct RawUniqueIdNfo {
    #[serde(rename = "@default")]
    is_default: Option<String>,
    #[serde(rename = "@type")]
    source: Option<String>,
    #[serde(rename = "$text")]
    value: Option<String>,
}

impl From<RawUniqueIdNfo> for NfoGuid {
    fn from(value: RawUniqueIdNfo) -> Self {
        Self {
            is_default: parse_bool(value.is_default.as_deref()),
            source: value.source,
            value: value.value,
        }
    }
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct RawTvShowNfo {
    title: Option<String>,
    sorttitle: Option<String>,
    year: Option<String>,
    plot: Option<String>,
    outline: Option<String>,
    tagline: Option<String>,
    #[serde(rename = "mpaa")]
    content_rating: Option<String>,
    premiered: Option<String>,
    #[serde(rename = "genre")]
    genres: Vec<String>,
    #[serde(rename = "tag")]
    tags: Vec<String>,
    #[serde(rename = "studio")]
    studios: Vec<String>,
    #[serde(rename = "actor")]
    actors: Vec<RawActorNfo>,
    #[serde(rename = "uniqueid")]
    guids: Vec<RawUniqueIdNfo>,
}

impl From<RawTvShowNfo> for TvShowNfo {
    fn from(value: RawTvShowNfo) -> Self {
        Self {
            title: value.title,
            sort_title: value.sorttitle,
            year: parse_i32(value.year),
            plot: value.plot,
            outline: value.outline,
            tagline: value.tagline,
            content_rating: value.content_rating,
            premiered: parse_datetime(value.premiered),
            genres: value.genres,
            tags: value.tags,
            studios: value.studios,
            actors: value.actors.into_iter().map(NfoActor::from).collect(),
            guids: value.guids.into_iter().map(NfoGuid::from).collect(),
        }
    }
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct RawSeasonNfo {
    title: Option<String>,
    sorttitle: Option<String>,
    year: Option<String>,
    premiered: Option<String>,
}

impl From<RawSeasonNfo> for SeasonNfo {
    fn from(value: RawSeasonNfo) -> Self {
        Self {
            title: value.title,
            sort_title: value.sorttitle,
            year: parse_i32(value.year),
            premiered: parse_datetime(value.premiered),
        }
    }
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct RawEpisodeList {
    #[serde(rename = "episodedetails")]
    episodes: Vec<RawEpisodeNfo>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct RawEpisodeNfo {
    showtitle: Option<String>,
    title: Option<String>,
    season: Option<String>,
    episode: Option<String>,
    #[serde(rename = "mpaa")]
    content_rating: Option<String>,
    aired: Option<String>,
    premiered: Option<String>,
    year: Option<String>,
    plot: Option<String>,
    #[serde(rename = "genre")]
    genres: Vec<String>,
    #[serde(rename = "tag")]
    tags: Vec<String>,
    #[serde(rename = "actor")]
    actors: Vec<RawActorNfo>,
    #[serde(rename = "credits")]
    writers: Vec<String>,
    #[serde(rename = "director")]
    directors: Vec<String>,
    #[serde(rename = "uniqueid")]
    guids: Vec<RawUniqueIdNfo>,
}

impl From<RawEpisodeNfo> for EpisodeNfo {
    fn from(value: RawEpisodeNfo) -> Self {
        Self {
            show_title: value.showtitle,
            title: value.title,
            season: parse_i32(value.season),
            episode: parse_i32(value.episode),
            content_rating: value.content_rating,
            aired: parse_datetime(value.aired),
            premiered: parse_datetime(value.premiered),
            year: parse_i32(value.year),
            plot: value.plot,
            genres: value.genres,
            tags: value.tags,
            actors: value.actors.into_iter().map(NfoActor::from).collect(),
            writers: value.writers,
            directors: value.directors,
            guids: value.guids.into_iter().map(NfoGuid::from).collect(),
        }
    }
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct RawArtistNfo {
    name: Option<String>,
    sorttitle: Option<String>,
    disambiguation: Option<String>,
    biography: Option<String>,
    year: Option<String>,
    formed: Option<String>,
    #[serde(rename = "genre")]
    genres: Vec<String>,
    #[serde(rename = "style")]
    styles: Vec<String>,
    #[serde(rename = "mood")]
    moods: Vec<String>,
}

impl From<RawArtistNfo> for ArtistNfo {
    fn from(value: RawArtistNfo) -> Self {
        Self {
            name: value.name,
            sort_title: value.sorttitle,
            disambiguation: value.disambiguation,
            biography: value.biography,
            year: parse_i32(value.year),
            formed: parse_datetime(value.formed),
            genres: value.genres,
            styles: value.styles,
            moods: value.moods,
        }
    }
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct RawMusicVideoNfo {
    title: Option<String>,
    album: Option<String>,
    plot: Option<String>,
    year: Option<String>,
    aired: Option<String>,
    premiered: Option<String>,
    track: Option<String>,
    #[serde(rename = "artist")]
    artists: Vec<String>,
    #[serde(rename = "director")]
    directors: Vec<String>,
    #[serde(rename = "genre")]
    genres: Vec<String>,
    #[serde(rename = "tag")]
    tags: Vec<String>,
    #[serde(rename = "studio")]
    studios: Vec<String>,
}

impl From<RawMusicVideoNfo> for MusicVideoNfo {
    fn from(value: RawMusicVideoNfo) -> Self {
        Self {
            title: value.title,
            album: value.album,
            plot: value.plot,
            year: parse_i32(value.year),
            aired: parse_datetime(value.aired),
            premiered: parse_datetime(value.premiered),
            track: parse_i32(value.track),
            artists: value.artists,
            directors: value.directors,
            genres: value.genres,
            tags: value.tags,
            studios: value.studios,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_show_nfo() {
        let parsed = parse_tvshow_nfo_str(
            r#"<tvshow>
                <title>The Expanse</title>
                <year>2015</year>
                <premiered>2015-12-14</premiered>
                <genre>Science Fiction</genre>
                <tag>space</tag>
                <studio>Syfy</studio>
                <actor><name>Shohreh Aghdashloo</name><role>Avasarala</role><order>1</order></actor>
                <uniqueid type="imdb" default="true">tt3230854</uniqueid>
            </tvshow>"#,
        )
        .expect("show nfo should parse");

        assert_eq!(parsed.title.as_deref(), Some("The Expanse"));
        assert_eq!(parsed.year, Some(2015));
        assert_eq!(parsed.genres, ["Science Fiction"]);
        assert_eq!(parsed.actors[0].order, Some(1));
        assert_eq!(
            parsed.guids[0].as_metadata_guid().as_deref(),
            Some("imdb://tt3230854")
        );
    }

    #[test]
    fn parses_episode_fragment_list() {
        let parsed = parse_episode_nfos_str(
            r"<episodedetails><title>One</title><season>1</season><episode>1</episode></episodedetails>
               <episodedetails><title>Two</title><season>1</season><episode>2</episode></episodedetails>",
        )
        .expect("episode fragments should parse");

        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[1].title.as_deref(), Some("Two"));
        assert_eq!(parsed[1].episode, Some(2));
    }

    #[test]
    fn parses_artist_and_music_video_nfo() {
        let artist = parse_artist_nfo_str(
            r"<artist>
                <name>Talking Heads</name>
                <genre>New Wave</genre>
                <style>Art Punk</style>
                <mood>Angular</mood>
                <biography>Band biography</biography>
             </artist>",
        )
        .expect("artist nfo should parse");
        assert_eq!(artist.name.as_deref(), Some("Talking Heads"));
        assert_eq!(artist.styles, ["Art Punk"]);

        let video = parse_music_video_nfo_str(
            r"<musicvideo>
                <artist>Talking Heads</artist>
                <title>Once in a Lifetime</title>
                <album>Remain in Light</album>
                <track>4</track>
                <aired>1980</aired>
                <director>Toni Basil</director>
             </musicvideo>",
        )
        .expect("music video nfo should parse");
        assert_eq!(video.track, Some(4));
        assert_eq!(video.directors, ["Toni Basil"]);
        assert!(video.aired.is_some());
    }

    #[test]
    fn sort_title_matches_ersatztv_rules() {
        assert_eq!(sort_title("The Matrix"), "Matrix");
        assert_eq!(sort_title("A Scanner Darkly"), "Scanner Darkly");
        assert_eq!(sort_title("An Education"), "Education");
        assert_eq!(sort_title(""), "");
        assert_eq!(sort_title("\t"), "\t");
        assert_eq!(sort_title("\u{00c6}on Flux"), "Eon Flux");
    }
}
