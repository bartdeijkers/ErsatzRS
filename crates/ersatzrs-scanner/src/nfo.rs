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
    /// All repeated MPAA/content-rating elements.
    pub content_ratings: Vec<String>,
    /// Premiered date.
    pub premiered: Option<DateTime<Utc>>,
    /// Genre rows.
    pub genres: Vec<String>,
    /// Tag rows.
    pub tags: Vec<String>,
    /// Studio rows.
    pub studios: Vec<String>,
    /// Publisher/music-label rows (ErsatzRS extension).
    pub music_labels: Vec<String>,
    /// Explicit metadata-language rows (ErsatzRS extension).
    pub languages: Vec<String>,
    /// Actor rows.
    pub actors: Vec<NfoActor>,
    /// Provider GUID rows.
    pub guids: Vec<NfoGuid>,
}

/// Parsed `<movie>` sidecar metadata.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MovieNfo {
    /// Movie title.
    pub title: Option<String>,
    /// Explicit sort title, if supplied by the sidecar.
    pub sort_title: Option<String>,
    /// Short outline.
    pub outline: Option<String>,
    /// Full plot.
    pub plot: Option<String>,
    /// Release year.
    pub year: Option<i32>,
    /// MPAA/content rating.
    pub content_rating: Option<String>,
    /// All repeated MPAA/content-rating elements.
    pub content_ratings: Vec<String>,
    /// Premiered date.
    pub premiered: Option<DateTime<Utc>>,
    /// Genre rows.
    pub genres: Vec<String>,
    /// Tag rows.
    pub tags: Vec<String>,
    /// Production countries from repeated `<country>` elements.
    pub countries: Vec<String>,
    /// Studio rows.
    pub studios: Vec<String>,
    /// Publisher/music-label rows (ErsatzRS extension).
    pub music_labels: Vec<String>,
    /// Explicit metadata-language rows (ErsatzRS extension).
    pub languages: Vec<String>,
    /// Actor rows.
    pub actors: Vec<NfoActor>,
    /// Writer names from `<credits>`.
    pub writers: Vec<String>,
    /// Director names.
    pub directors: Vec<String>,
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
    /// All repeated MPAA/content-rating elements.
    pub content_ratings: Vec<String>,
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
    /// Studio rows.
    pub studios: Vec<String>,
    /// Publisher/music-label rows (ErsatzRS extension).
    pub music_labels: Vec<String>,
    /// Explicit metadata-language rows (ErsatzRS extension).
    pub languages: Vec<String>,
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
    /// MPAA/content-rating rows.
    pub content_ratings: Vec<String>,
    /// Publisher/music-label rows (ErsatzRS extension).
    pub music_labels: Vec<String>,
    /// Explicit metadata-language rows (ErsatzRS extension).
    pub languages: Vec<String>,
}

/// Parsed Kodi-compatible Other Video sidecar metadata.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OtherVideoNfo {
    pub title: Option<String>,
    pub original_title: Option<String>,
    pub sort_title: Option<String>,
    pub outline: Option<String>,
    pub plot: Option<String>,
    pub tagline: Option<String>,
    pub year: Option<i32>,
    pub premiered: Option<DateTime<Utc>>,
    pub content_ratings: Vec<String>,
    pub studios: Vec<String>,
    pub music_labels: Vec<String>,
    pub languages: Vec<String>,
    pub genres: Vec<String>,
    pub tags: Vec<String>,
    pub actors: Vec<NfoActor>,
    pub writers: Vec<String>,
    pub directors: Vec<String>,
    pub artists: Vec<String>,
    pub guids: Vec<NfoGuid>,
    pub show_title: Option<String>,
    pub season: Option<i32>,
    pub episode: Option<i32>,
    pub release_datetime: Option<DateTime<Utc>>,
    pub collection: Option<String>,
    pub producers: Vec<String>,
    pub original_broadcasters: Vec<String>,
    pub broadcasters: Vec<String>,
}

/// Parsed ErsatzRS `<song>` sidecar metadata.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SongNfo {
    pub title: Option<String>,
    pub sort_title: Option<String>,
    pub year: Option<i32>,
    pub release_date: Option<DateTime<Utc>>,
    pub album: Option<String>,
    pub artists: Vec<String>,
    pub album_artists: Vec<String>,
    pub track: Option<String>,
    pub content_ratings: Vec<String>,
    pub studios: Vec<String>,
    pub music_labels: Vec<String>,
    pub languages: Vec<String>,
    pub genres: Vec<String>,
    pub tags: Vec<String>,
}

/// Parsed ErsatzRS `<image>` sidecar metadata.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ImageNfo {
    pub title: Option<String>,
    pub sort_title: Option<String>,
    pub year: Option<i32>,
    pub release_date: Option<DateTime<Utc>>,
    pub content_ratings: Vec<String>,
    pub studios: Vec<String>,
    pub music_labels: Vec<String>,
    pub languages: Vec<String>,
    pub genres: Vec<String>,
    pub tags: Vec<String>,
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

/// Parse a movie NFO file.
///
/// # Errors
///
/// Returns an error when the file cannot be read, the expected root is absent,
/// or XML deserialization fails.
pub fn parse_movie_nfo(path: impl AsRef<Path>) -> Result<MovieNfo> {
    parse_file::<RawMovieNfo, _>(path, "movie", MovieNfo::from)
}

/// Parse a movie NFO XML string.
///
/// # Errors
///
/// Returns an error when the expected root is absent or XML deserialization
/// fails.
pub fn parse_movie_nfo_str(input: &str) -> Result<MovieNfo> {
    parse_str::<RawMovieNfo, _>(input, "movie", MovieNfo::from)
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

/// Parse an Other Video NFO using the same compatible roots as ErsatzTV.
///
/// # Errors
///
/// Returns an error when the file cannot be read, no compatible root exists,
/// or XML deserialization fails.
pub fn parse_other_video_nfo(path: impl AsRef<Path>) -> Result<OtherVideoNfo> {
    let input = read_sanitized_file(path)?;
    parse_other_video_nfo_str(&input)
}

/// Parse an Other Video NFO XML string.
///
/// # Errors
///
/// Returns an error when no compatible root exists or XML deserialization
/// fails.
pub fn parse_other_video_nfo_str(input: &str) -> Result<OtherVideoNfo> {
    for root in ["episodedetails", "movie", "musicvideo"] {
        if let Some(fragment) = extract_root_fragment(&sanitize_nfo_text(input), root) {
            let raw: RawOtherVideoNfo = de::from_str(&fragment)?;
            return Ok(OtherVideoNfo::from(raw));
        }
    }
    Err(NfoError::MissingRoot("movie|episodedetails|musicvideo"))
}

/// Parse an ErsatzRS song sidecar.
///
/// # Errors
///
/// Returns an error when the file cannot be read, the `song` root is absent,
/// or XML deserialization fails.
pub fn parse_song_nfo(path: impl AsRef<Path>) -> Result<SongNfo> {
    parse_file::<RawSongNfo, _>(path, "song", SongNfo::from)
}

/// Parse an ErsatzRS song sidecar XML string.
///
/// # Errors
///
/// Returns an error when the `song` root is absent or XML deserialization
/// fails.
pub fn parse_song_nfo_str(input: &str) -> Result<SongNfo> {
    parse_str::<RawSongNfo, _>(input, "song", SongNfo::from)
}

/// Parse an ErsatzRS image sidecar.
///
/// # Errors
///
/// Returns an error when the file cannot be read, the `image` root is absent,
/// or XML deserialization fails.
pub fn parse_image_nfo(path: impl AsRef<Path>) -> Result<ImageNfo> {
    parse_file::<RawImageNfo, _>(path, "image", ImageNfo::from)
}

/// Parse an ErsatzRS image sidecar XML string.
///
/// # Errors
///
/// Returns an error when the `image` root is absent or XML deserialization
/// fails.
pub fn parse_image_nfo_str(input: &str) -> Result<ImageNfo> {
    parse_str::<RawImageNfo, _>(input, "image", ImageNfo::from)
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

    if title
        .get(..4)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("the "))
    {
        return title[4..].to_owned();
    }

    if title
        .get(..2)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("a "))
    {
        return title[2..].to_owned();
    }

    if title
        .get(..3)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("an "))
    {
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
    let Some(fragment) = extract_root_fragment(&sanitized, root) else {
        return Err(NfoError::MissingRoot(root));
    };

    let raw: T = de::from_str(&fragment)?;
    Ok(map(raw))
}

fn read_sanitized_file(path: impl AsRef<Path>) -> Result<String> {
    let path = path.as_ref();
    let bytes = fs::read(path).map_err(|source| NfoError::Io {
        path: path.display().to_string(),
        source,
    })?;
    Ok(sanitize_nfo_text(&decode_nfo_bytes(bytes)))
}

/// Decode sidecar bytes as UTF-8, falling back to Windows-1252.
///
/// Sidecars are not reliably UTF-8. Kodi files written by older tools use
/// western-European codepages, and the scene-release `.nfo` that often sits
/// beside a movie under the same basename is usually CP437 box-drawing art.
/// Refusing to decode those hid the real sidecar behind an IO error, so decode
/// with a fallback that cannot fail and let the root check decide whether the
/// file carries metadata at all.
fn decode_nfo_bytes(bytes: Vec<u8>) -> String {
    match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(error) => error
            .into_bytes()
            .into_iter()
            .map(windows_1252_char)
            .collect(),
    }
}

/// Map one byte to its Windows-1252 character.
///
/// Bytes outside `0x80..=0x9f` share Latin-1's identity mapping. The five
/// positions Windows-1252 leaves undefined keep their C1 control codepoints,
/// which [`sanitize_nfo_text`] then replaces.
fn windows_1252_char(byte: u8) -> char {
    /// Windows-1252 replacements for the C1 range, `0x80..=0x9f`.
    const C1: [char; 32] = [
        '\u{20ac}', '\u{0081}', '\u{201a}', '\u{0192}', '\u{201e}', '\u{2026}', '\u{2020}',
        '\u{2021}', '\u{02c6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{008d}',
        '\u{017d}', '\u{008f}', '\u{0090}', '\u{2018}', '\u{2019}', '\u{201c}', '\u{201d}',
        '\u{2022}', '\u{2013}', '\u{2014}', '\u{02dc}', '\u{2122}', '\u{0161}', '\u{203a}',
        '\u{0153}', '\u{009d}', '\u{017e}', '\u{0178}',
    ];

    match byte {
        0x80..=0x9f => C1[usize::from(byte - 0x80)],
        _ => char::from(byte),
    }
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
    extract_root_fragment(input, root).is_some()
}

fn extract_root_fragment(input: &str, root: &str) -> Option<String> {
    let mut reader = Reader::from_str(input);
    reader.config_mut().trim_text(true);
    let mut start = None;
    let mut depth = 0usize;

    loop {
        let before = usize::try_from(reader.buffer_position()).ok()?;
        match reader.read_event() {
            Ok(Event::Empty(element)) if element.name().as_ref().eq_ignore_ascii_case(root) => {
                let end = usize::try_from(reader.buffer_position()).ok()?;
                return input.get(before..end).map(str::to_owned);
            }
            Ok(Event::Start(element)) if element.name().as_ref().eq_ignore_ascii_case(root) => {
                if start.is_none() {
                    start = Some(before);
                    depth = 1;
                    continue;
                }
                depth = depth.saturating_add(1);
            }
            Ok(Event::End(element))
                if start.is_some() && element.name().as_ref().eq_ignore_ascii_case(root) =>
            {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    let end = usize::try_from(reader.buffer_position()).ok()?;
                    return input.get(start?..end).map(str::to_owned);
                }
            }
            Ok(Event::Eof) | Err(_) => return None,
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
    content_ratings: Vec<String>,
    premiered: Option<String>,
    #[serde(rename = "genre")]
    genres: Vec<String>,
    #[serde(rename = "tag")]
    tags: Vec<String>,
    #[serde(rename = "studio")]
    studios: Vec<String>,
    #[serde(rename = "label")]
    music_labels: Vec<String>,
    #[serde(rename = "language")]
    languages: Vec<String>,
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
            content_rating: value.content_ratings.first().cloned(),
            content_ratings: value.content_ratings,
            premiered: parse_datetime(value.premiered),
            genres: value.genres,
            tags: value.tags,
            studios: value.studios,
            music_labels: value.music_labels,
            languages: value.languages,
            actors: value.actors.into_iter().map(NfoActor::from).collect(),
            guids: value.guids.into_iter().map(NfoGuid::from).collect(),
        }
    }
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct RawMovieNfo {
    title: Option<String>,
    sorttitle: Option<String>,
    outline: Option<String>,
    plot: Option<String>,
    year: Option<String>,
    #[serde(rename = "mpaa")]
    content_ratings: Vec<String>,
    premiered: Option<String>,
    #[serde(rename = "genre")]
    genres: Vec<String>,
    #[serde(rename = "tag")]
    tags: Vec<String>,
    #[serde(rename = "country")]
    countries: Vec<String>,
    #[serde(rename = "studio")]
    studios: Vec<String>,
    #[serde(rename = "label")]
    music_labels: Vec<String>,
    #[serde(rename = "language")]
    languages: Vec<String>,
    #[serde(rename = "actor")]
    actors: Vec<RawActorNfo>,
    #[serde(rename = "credits")]
    writers: Vec<String>,
    #[serde(rename = "director")]
    directors: Vec<String>,
    #[serde(rename = "uniqueid")]
    guids: Vec<RawUniqueIdNfo>,
}

impl From<RawMovieNfo> for MovieNfo {
    fn from(value: RawMovieNfo) -> Self {
        Self {
            title: value.title,
            sort_title: value.sorttitle,
            outline: value.outline,
            plot: value.plot,
            year: parse_i32(value.year),
            content_rating: value.content_ratings.first().cloned(),
            content_ratings: value.content_ratings,
            premiered: parse_datetime(value.premiered),
            genres: value.genres,
            tags: value.tags,
            countries: value.countries,
            studios: value.studios,
            music_labels: value.music_labels,
            languages: value.languages,
            actors: value.actors.into_iter().map(NfoActor::from).collect(),
            writers: value.writers,
            directors: value.directors,
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
    content_ratings: Vec<String>,
    aired: Option<String>,
    premiered: Option<String>,
    year: Option<String>,
    plot: Option<String>,
    #[serde(rename = "genre")]
    genres: Vec<String>,
    #[serde(rename = "tag")]
    tags: Vec<String>,
    #[serde(rename = "studio")]
    studios: Vec<String>,
    #[serde(rename = "label")]
    music_labels: Vec<String>,
    #[serde(rename = "language")]
    languages: Vec<String>,
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
            content_rating: value.content_ratings.first().cloned(),
            content_ratings: value.content_ratings,
            aired: parse_datetime(value.aired),
            premiered: parse_datetime(value.premiered),
            year: parse_i32(value.year),
            plot: value.plot,
            genres: value.genres,
            tags: value.tags,
            studios: value.studios,
            music_labels: value.music_labels,
            languages: value.languages,
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
    #[serde(rename = "mpaa")]
    content_ratings: Vec<String>,
    #[serde(rename = "label")]
    music_labels: Vec<String>,
    #[serde(rename = "language")]
    languages: Vec<String>,
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
            content_ratings: value.content_ratings,
            music_labels: value.music_labels,
            languages: value.languages,
        }
    }
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct RawOtherVideoNfo {
    title: Option<String>,
    originaltitle: Option<String>,
    sorttitle: Option<String>,
    outline: Option<String>,
    plot: Option<String>,
    tagline: Option<String>,
    year: Option<String>,
    premiered: Option<String>,
    #[serde(rename = "mpaa")]
    content_ratings: Vec<String>,
    #[serde(rename = "studio")]
    studios: Vec<String>,
    #[serde(rename = "label")]
    music_labels: Vec<String>,
    #[serde(rename = "language")]
    languages: Vec<String>,
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
    #[serde(rename = "artist")]
    artists: Vec<String>,
    #[serde(rename = "uniqueid")]
    guids: Vec<RawUniqueIdNfo>,
    showtitle: Option<String>,
    season: Option<String>,
    episode: Option<String>,
    #[serde(rename = "ersatzrs:release_datetime", alias = "release_datetime")]
    release_datetime: Option<String>,
    #[serde(rename = "ersatzrs:collection", alias = "collection")]
    collection: Option<String>,
    #[serde(rename = "ersatzrs:producer", alias = "producer")]
    producers: Vec<String>,
    #[serde(
        rename = "ersatzrs:original_broadcaster",
        alias = "original_broadcaster"
    )]
    original_broadcasters: Vec<String>,
    #[serde(rename = "ersatzrs:broadcaster", alias = "broadcaster")]
    broadcasters: Vec<String>,
}

impl From<RawOtherVideoNfo> for OtherVideoNfo {
    fn from(value: RawOtherVideoNfo) -> Self {
        Self {
            title: value.title,
            original_title: value.originaltitle,
            sort_title: value.sorttitle,
            outline: value.outline,
            plot: value.plot,
            tagline: value.tagline,
            year: parse_i32(value.year),
            premiered: parse_datetime(value.premiered),
            content_ratings: value.content_ratings,
            studios: value.studios,
            music_labels: value.music_labels,
            languages: value.languages,
            genres: value.genres,
            tags: value.tags,
            actors: value.actors.into_iter().map(NfoActor::from).collect(),
            writers: value.writers,
            directors: value.directors,
            artists: value.artists,
            guids: value.guids.into_iter().map(NfoGuid::from).collect(),
            show_title: value.showtitle,
            season: parse_i32(value.season),
            episode: parse_i32(value.episode),
            release_datetime: parse_datetime(value.release_datetime),
            collection: value.collection,
            producers: value.producers,
            original_broadcasters: value.original_broadcasters,
            broadcasters: value.broadcasters,
        }
    }
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct RawSongNfo {
    title: Option<String>,
    sorttitle: Option<String>,
    year: Option<String>,
    released: Option<String>,
    album: Option<String>,
    #[serde(rename = "artist")]
    artists: Vec<String>,
    #[serde(rename = "albumartist")]
    album_artists: Vec<String>,
    track: Option<String>,
    #[serde(rename = "mpaa")]
    content_ratings: Vec<String>,
    #[serde(rename = "studio")]
    studios: Vec<String>,
    #[serde(rename = "label")]
    music_labels: Vec<String>,
    #[serde(rename = "language")]
    languages: Vec<String>,
    #[serde(rename = "genre")]
    genres: Vec<String>,
    #[serde(rename = "tag")]
    tags: Vec<String>,
}

impl From<RawSongNfo> for SongNfo {
    fn from(value: RawSongNfo) -> Self {
        Self {
            title: value.title,
            sort_title: value.sorttitle,
            year: parse_i32(value.year),
            release_date: parse_datetime(value.released),
            album: value.album,
            artists: value.artists,
            album_artists: value.album_artists,
            track: value.track,
            content_ratings: value.content_ratings,
            studios: value.studios,
            music_labels: value.music_labels,
            languages: value.languages,
            genres: value.genres,
            tags: value.tags,
        }
    }
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct RawImageNfo {
    title: Option<String>,
    sorttitle: Option<String>,
    year: Option<String>,
    released: Option<String>,
    #[serde(rename = "mpaa")]
    content_ratings: Vec<String>,
    #[serde(rename = "studio")]
    studios: Vec<String>,
    #[serde(rename = "label")]
    music_labels: Vec<String>,
    #[serde(rename = "language")]
    languages: Vec<String>,
    #[serde(rename = "genre")]
    genres: Vec<String>,
    #[serde(rename = "tag")]
    tags: Vec<String>,
}

impl From<RawImageNfo> for ImageNfo {
    fn from(value: RawImageNfo) -> Self {
        Self {
            title: value.title,
            sort_title: value.sorttitle,
            year: parse_i32(value.year),
            release_date: parse_datetime(value.released),
            content_ratings: value.content_ratings,
            studios: value.studios,
            music_labels: value.music_labels,
            languages: value.languages,
            genres: value.genres,
            tags: value.tags,
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
    fn parses_movie_nfo() {
        let parsed = parse_movie_nfo_str(
            r#"<movie>
                <title>The Matrix</title>
                <sorttitle>Matrix</sorttitle>
                <outline>Reality is not what it seems.</outline>
                <plot>A hacker discovers the truth.</plot>
                <year>1999</year>
                <mpaa>R</mpaa>
                <premiered>1999-03-31</premiered>
                <genre>Science Fiction</genre>
                <tag>cyberpunk</tag>
                <studio>Warner Bros.</studio>
                <actor><name>Keanu Reeves</name><role>Neo</role><order>1</order></actor>
                <credits>Lana Wachowski</credits>
                <director>Lilly Wachowski</director>
                <uniqueid type="imdb" default="true">tt0133093</uniqueid>
            </movie>"#,
        )
        .expect("movie nfo should parse");

        assert_eq!(parsed.title.as_deref(), Some("The Matrix"));
        assert_eq!(parsed.sort_title.as_deref(), Some("Matrix"));
        assert_eq!(parsed.year, Some(1999));
        assert_eq!(parsed.content_rating.as_deref(), Some("R"));
        assert_eq!(parsed.genres, ["Science Fiction"]);
        assert_eq!(parsed.actors[0].role.as_deref(), Some("Neo"));
        assert_eq!(parsed.writers, ["Lana Wachowski"]);
        assert_eq!(parsed.directors, ["Lilly Wachowski"]);
        assert_eq!(
            parsed.guids[0].as_metadata_guid().as_deref(),
            Some("imdb://tt0133093")
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
    fn parses_music_video_nfo_from_fragment_stream() {
        let parsed = parse_music_video_nfo_str(
            r"https://example.invalid/metadata
            <musicvideo>
                <artist>Talking Heads</artist>
                <title>Once in a Lifetime</title>
                <album>Remain in Light</album>
                <track>4</track>
                <genre>New Wave</genre>
            </musicvideo>
            <ignored><title>Trailing fragment</title></ignored>",
        )
        .expect("music-video fragment should parse");

        assert_eq!(parsed.title.as_deref(), Some("Once in a Lifetime"));
        assert_eq!(parsed.album.as_deref(), Some("Remain in Light"));
        assert_eq!(parsed.track, Some(4));
        assert_eq!(parsed.artists, ["Talking Heads"]);
        assert_eq!(parsed.genres, ["New Wave"]);
    }

    #[test]
    fn repeated_uniform_facets_remain_distinct_and_studio_is_not_label() {
        let parsed = parse_other_video_nfo_str(
            r"<movie>
                <title>Facet Sample</title>
                <mpaa>NL:12</mpaa><mpaa>US:PG-13</mpaa>
                <studio>Studio One</studio><studio>Studio Two</studio>
                <label>Label One</label><label>Label Two</label>
                <language>nld</language><language>eng</language>
                <genre>Documentary</genre><genre>Music</genre>
                <tag>archive</tag><tag>concert</tag>
            </movie>",
        )
        .expect("other-video nfo should parse");

        assert_eq!(parsed.content_ratings, ["NL:12", "US:PG-13"]);
        assert_eq!(parsed.studios, ["Studio One", "Studio Two"]);
        assert_eq!(parsed.music_labels, ["Label One", "Label Two"]);
        assert_eq!(parsed.languages, ["nld", "eng"]);
        assert_eq!(parsed.genres, ["Documentary", "Music"]);
        assert_eq!(parsed.tags, ["archive", "concert"]);
    }

    #[test]
    fn parses_documented_song_and_image_sidecar_roots() {
        let song = parse_song_nfo_str(
            r"<song><title>Song</title><label>Indie Label</label><language>nl</language></song>",
        )
        .expect("song sidecar should parse");
        let image = parse_image_nfo_str(
            r"<image><title>Photo</title><genre>Portrait</genre><language>fr</language></image>",
        )
        .expect("image sidecar should parse");

        assert_eq!(song.music_labels, ["Indie Label"]);
        assert_eq!(song.languages, ["nl"]);
        assert_eq!(image.genres, ["Portrait"]);
        assert_eq!(image.languages, ["fr"]);
    }

    #[test]
    fn reads_sidecar_written_in_windows_1252() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("movie.nfo");
        // `Am\xe9lie` and a `\x92` right single quote, neither of which is UTF-8.
        let mut bytes = b"<movie><title>Am\xe9lie".to_vec();
        bytes.extend_from_slice(b"\x92s Caf\xe9</title><year>2001</year></movie>");
        fs::write(&path, bytes).expect("write sidecar");

        let parsed = parse_movie_nfo(&path).expect("windows-1252 sidecar should parse");

        assert_eq!(
            parsed.title.as_deref(),
            Some("Am\u{e9}lie\u{2019}s Caf\u{e9}")
        );
        assert_eq!(parsed.year, Some(2001));
    }

    #[test]
    fn release_art_sidecar_reports_missing_root_rather_than_an_io_error() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("Movie (1984).nfo");
        // CP437 box-drawing art, as scene releases ship it.
        fs::write(&path, [0xdb_u8; 64]).expect("write sidecar");

        let error = parse_movie_nfo(&path).expect_err("release art carries no metadata");

        assert!(
            matches!(error, NfoError::MissingRoot("movie")),
            "expected a missing-root error, got {error:?}"
        );
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

    #[test]
    fn sort_title_does_not_slice_inside_unicode_scalars() {
        assert_eq!(sort_title("L\u{00e8}ve-toi"), "L\u{00e8}ve-toi");
        assert_eq!(sort_title("\u{00e9}on Flux"), "\u{00e9}on Flux");
    }
}
