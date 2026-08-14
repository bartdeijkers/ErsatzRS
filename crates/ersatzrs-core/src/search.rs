use std::collections::BTreeMap;

use chrono::{DateTime, Datelike, Duration, Months, NaiveDate, Utc};
use ersatzrs_domain::{normalize_sub_kind_token, ContentKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchExpression {
    MatchAll,
    Term(SearchTerm),
    And(Vec<SearchExpression>),
    Or(Vec<SearchExpression>),
    Not(Box<SearchExpression>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchTerm {
    pub field: SearchField,
    pub value: SearchValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchField {
    Default,
    Title,
    SortTitle,
    OriginalTitle,
    Year,
    ReleaseDate,
    AddedDate,
    Genre,
    Tag,
    Collection,
    Studio,
    MusicLabel,
    Network,
    Actor,
    Director,
    Writer,
    Language,
    LanguageTag,
    SubtitleLanguage,
    SubtitleLanguageTag,
    VideoCodec,
    VideoBitDepth,
    VideoDynamicRange,
    AudioCodec,
    AudioTitle,
    SubtitleCodec,
    SubtitleTitle,
    HasCommentary,
    Width,
    Height,
    Chapters,
    Minutes,
    Seconds,
    MediaKind,
    LibraryName,
    LibraryFolderId,
    ContentKind,
    SubKind,
    ParentTitle,
    ContentRating,
    ShowTitle,
    ShowGenre,
    ShowTag,
    ShowStudio,
    ShowNetwork,
    ShowContentRating,
    SeasonNumber,
    EpisodeNumber,
    MetadataKind,
    AddonList,
    Style,
    Mood,
    Album,
    Artist,
    AlbumArtist,
    LibraryId,
    ReleasedOnThisDay,
    ReleasedInTheLast,
    ReleasedNotInTheLast,
    AddedInTheLast,
    AddedNotInTheLast,
    ProviderGuid(Option<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchValue {
    Text(String),
    Wildcard(String),
    Fuzzy {
        value: String,
        max_edits: usize,
    },
    Phrase {
        value: String,
        slop: usize,
    },
    Range {
        start: Option<String>,
        end: Option<String>,
        inclusive: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SearchDocument {
    pub media_item_id: i64,
    pub parent_media_item_id: Option<i64>,
    pub media_kind: String,
    pub metadata_kind: String,
    pub title: Option<String>,
    pub sort_title: Option<String>,
    pub original_title: Option<String>,
    pub year: Option<i32>,
    pub release_date: Option<DateTime<Utc>>,
    pub added_date: Option<DateTime<Utc>>,
    pub genres: Vec<String>,
    pub tags: Vec<String>,
    pub collections: Vec<String>,
    pub studios: Vec<String>,
    pub music_labels: Vec<String>,
    pub networks: Vec<String>,
    pub actors: Vec<String>,
    pub directors: Vec<String>,
    pub writers: Vec<String>,
    pub languages: Vec<String>,
    pub language_tags: Vec<String>,
    pub subtitle_languages: Vec<String>,
    pub subtitle_language_tags: Vec<String>,
    pub video_codecs: Vec<String>,
    pub video_bit_depths: Vec<i32>,
    pub video_dynamic_ranges: Vec<String>,
    pub audio_codecs: Vec<String>,
    pub audio_titles: Vec<String>,
    pub subtitle_codecs: Vec<String>,
    pub subtitle_titles: Vec<String>,
    pub has_commentary: bool,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub chapter_counts: Vec<i32>,
    pub durations_seconds: Vec<i64>,
    pub library_name: Option<String>,
    pub library_folder_ids: Vec<i64>,
    pub content_kind: ContentKind,
    pub sub_kind: Option<String>,
    pub parent_title: Option<String>,
    pub content_ratings: Vec<String>,
    pub show_title: Option<String>,
    pub show_genres: Vec<String>,
    pub show_tags: Vec<String>,
    pub show_studios: Vec<String>,
    pub show_networks: Vec<String>,
    pub show_content_ratings: Vec<String>,
    pub season_number: Option<i32>,
    pub episode_number: Option<i32>,
    pub addon_lists: Vec<String>,
    pub styles: Vec<String>,
    pub moods: Vec<String>,
    pub albums: Vec<String>,
    pub artists: Vec<String>,
    pub album_artists: Vec<String>,
    pub library_id: Option<i64>,
    pub provider_guids: Vec<SearchProviderGuid>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchProviderGuid {
    pub source: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchPageMap {
    pub page_map: BTreeMap<char, usize>,
}

impl SearchExpression {
    #[must_use]
    pub fn parse(query: &str) -> Self {
        SearchParser::new(query)
            .parse()
            .with_default_content_kind_filter()
    }

    #[must_use]
    pub fn matches(&self, document: &SearchDocument) -> bool {
        match self {
            Self::MatchAll => true,
            Self::Term(term) => term.matches(document),
            Self::And(expressions) => expressions
                .iter()
                .all(|expression| expression.matches(document)),
            Self::Or(expressions) => expressions
                .iter()
                .any(|expression| expression.matches(document)),
            Self::Not(expression) => !expression.matches(document),
        }
    }

    fn with_default_content_kind_filter(self) -> Self {
        if self.references_variant_selector() {
            return self;
        }

        let primary = Self::Term(SearchTerm {
            field: SearchField::ContentKind,
            value: SearchValue::Text(ContentKind::Primary.search_token().to_owned()),
        });

        match self {
            Self::MatchAll => primary,
            expression => Self::And(vec![primary, expression]),
        }
    }

    fn references_variant_selector(&self) -> bool {
        match self {
            Self::MatchAll => false,
            Self::Term(term) => matches!(
                term.field,
                SearchField::ContentKind | SearchField::SubKind | SearchField::ParentTitle
            ),
            Self::And(expressions) | Self::Or(expressions) => {
                expressions.iter().any(Self::references_variant_selector)
            }
            Self::Not(expression) => expression.references_variant_selector(),
        }
    }
}

impl SearchTerm {
    #[must_use]
    pub fn matches(&self, document: &SearchDocument) -> bool {
        if let Some(matches) = self.field.relative_date_matches(document, &self.value) {
            return matches;
        }

        let values = self.field.values(document);
        match &self.value {
            SearchValue::Text(expected) => values
                .iter()
                .any(|actual| self.field.text_matches(actual, expected, false)),
            SearchValue::Wildcard(pattern) => values
                .iter()
                .any(|actual| self.field.text_matches(actual, pattern, true)),
            SearchValue::Fuzzy { value, max_edits } => values
                .iter()
                .any(|actual| fuzzy_matches(actual, value, *max_edits)),
            SearchValue::Phrase { value, slop } => values
                .iter()
                .any(|actual| phrase_matches(actual, value, *slop)),
            SearchValue::Range {
                start,
                end,
                inclusive,
            } => values.iter().any(|actual| {
                self.field
                    .range_matches(actual, start.as_deref(), end.as_deref(), *inclusive)
            }),
        }
    }
}

impl SearchField {
    #[allow(clippy::too_many_lines)]
    fn values(&self, document: &SearchDocument) -> Vec<String> {
        match self {
            Self::Default => option_values([
                document.title.as_deref(),
                document.sort_title.as_deref(),
                document.original_title.as_deref(),
                document.show_title.as_deref(),
            ]),
            Self::Title => option_values([document.title.as_deref()]),
            Self::SortTitle => option_values([document.sort_title.as_deref()]),
            Self::OriginalTitle => option_values([document.original_title.as_deref()]),
            Self::Year => document
                .year
                .map(|year| year.to_string())
                .into_iter()
                .collect(),
            Self::ReleaseDate => document
                .release_date
                .map(|date| format!("{:04}{:02}{:02}", date.year(), date.month(), date.day()))
                .into_iter()
                .collect(),
            Self::AddedDate => document
                .added_date
                .map(|date| format!("{:04}{:02}{:02}", date.year(), date.month(), date.day()))
                .into_iter()
                .collect(),
            Self::Genre => document.genres.clone(),
            Self::Tag => document.tags.clone(),
            Self::Collection => document.collections.clone(),
            Self::Studio => document.studios.clone(),
            Self::MusicLabel => document.music_labels.clone(),
            Self::Network => document.networks.clone(),
            Self::Actor => document.actors.clone(),
            Self::Director => document.directors.clone(),
            Self::Writer => document.writers.clone(),
            Self::Language => document.languages.clone(),
            Self::LanguageTag => document.language_tags.clone(),
            Self::SubtitleLanguage => document.subtitle_languages.clone(),
            Self::SubtitleLanguageTag => document.subtitle_language_tags.clone(),
            Self::VideoCodec => document.video_codecs.clone(),
            Self::VideoBitDepth => document
                .video_bit_depths
                .iter()
                .map(ToString::to_string)
                .collect(),
            Self::VideoDynamicRange => document.video_dynamic_ranges.clone(),
            Self::AudioCodec => document.audio_codecs.clone(),
            Self::AudioTitle => document.audio_titles.clone(),
            Self::SubtitleCodec => document.subtitle_codecs.clone(),
            Self::SubtitleTitle => document.subtitle_titles.clone(),
            Self::HasCommentary => {
                if document.has_commentary {
                    vec!["true".to_owned(), "yes".to_owned(), "1".to_owned()]
                } else {
                    vec!["false".to_owned(), "no".to_owned(), "0".to_owned()]
                }
            }
            Self::Width => document
                .width
                .map(|width| width.to_string())
                .into_iter()
                .collect(),
            Self::Height => document
                .height
                .map(|height| height.to_string())
                .into_iter()
                .collect(),
            Self::Chapters => document
                .chapter_counts
                .iter()
                .map(ToString::to_string)
                .collect(),
            Self::Minutes => document
                .durations_seconds
                .iter()
                .map(|seconds| ((*seconds + 59) / 60).to_string())
                .collect(),
            Self::Seconds => document
                .durations_seconds
                .iter()
                .map(ToString::to_string)
                .collect(),
            Self::MediaKind => vec![document.media_kind.clone()],
            Self::LibraryName => option_values([document.library_name.as_deref()]),
            Self::LibraryFolderId => document
                .library_folder_ids
                .iter()
                .map(ToString::to_string)
                .collect(),
            Self::ContentKind => vec![document.content_kind.search_token().to_owned()],
            Self::SubKind => option_values([document.sub_kind.as_deref()]),
            Self::ParentTitle => option_values([document.parent_title.as_deref()]),
            Self::ContentRating => document.content_ratings.clone(),
            Self::ShowTitle => option_values([document.show_title.as_deref()]),
            Self::ShowGenre => document.show_genres.clone(),
            Self::ShowTag => document.show_tags.clone(),
            Self::ShowStudio => document.show_studios.clone(),
            Self::ShowNetwork => document.show_networks.clone(),
            Self::ShowContentRating => document.show_content_ratings.clone(),
            Self::SeasonNumber => document
                .season_number
                .map(|number| number.to_string())
                .into_iter()
                .collect(),
            Self::EpisodeNumber => document
                .episode_number
                .map(|number| number.to_string())
                .into_iter()
                .collect(),
            Self::MetadataKind => vec![document.metadata_kind.clone()],
            Self::AddonList => document.addon_lists.clone(),
            Self::Style => document.styles.clone(),
            Self::Mood => document.moods.clone(),
            Self::Album => document.albums.clone(),
            Self::Artist => document.artists.clone(),
            Self::AlbumArtist => document.album_artists.clone(),
            Self::LibraryId => document
                .library_id
                .map(|id| id.to_string())
                .into_iter()
                .collect(),
            Self::ReleasedOnThisDay
            | Self::ReleasedInTheLast
            | Self::ReleasedNotInTheLast
            | Self::AddedInTheLast
            | Self::AddedNotInTheLast => Vec::new(),
            Self::ProviderGuid(None) => document
                .provider_guids
                .iter()
                .flat_map(|guid| {
                    [
                        guid.value.clone(),
                        format!("{}://{}", guid.source, guid.value),
                    ]
                })
                .collect(),
            Self::ProviderGuid(Some(source)) => document
                .provider_guids
                .iter()
                .filter(|guid| guid.source.eq_ignore_ascii_case(source))
                .map(|guid| guid.value.clone())
                .collect(),
        }
    }

    fn parse(field: &str) -> Self {
        match field.to_ascii_lowercase().as_str() {
            "title" => Self::Title,
            "sort_title" | "sorttitle" => Self::SortTitle,
            "original_title" | "originaltitle" => Self::OriginalTitle,
            "year" => Self::Year,
            "release_date" | "released" | "released_on" => Self::ReleaseDate,
            "added_date" | "added" | "added_on" => Self::AddedDate,
            "genre" => Self::Genre,
            "tag" | "tag_full" => Self::Tag,
            "collection" => Self::Collection,
            "studio" => Self::Studio,
            "music_label" => Self::MusicLabel,
            "network" => Self::Network,
            "actor" => Self::Actor,
            "director" => Self::Director,
            "writer" => Self::Writer,
            "language" | "audio_language" => Self::Language,
            "language_tag" | "audio_language_tag" => Self::LanguageTag,
            "sub_language" | "subtitle_language" => Self::SubtitleLanguage,
            "sub_language_tag" | "subtitle_language_tag" => Self::SubtitleLanguageTag,
            "video_codec" => Self::VideoCodec,
            "video_bit_depth" => Self::VideoBitDepth,
            "video_dynamic_range" => Self::VideoDynamicRange,
            "audio_codec" => Self::AudioCodec,
            "audio_title" | "audio_stream_title" => Self::AudioTitle,
            "subtitle_codec" | "sub_codec" => Self::SubtitleCodec,
            "subtitle_title" | "sub_title" | "subtitle_stream_title" => Self::SubtitleTitle,
            "has_commentary" | "commentary" => Self::HasCommentary,
            "chapters" => Self::Chapters,
            "minutes" => Self::Minutes,
            "seconds" => Self::Seconds,
            "width" => Self::Width,
            "height" => Self::Height,
            "type" | "kind" | "media_kind" => Self::MediaKind,
            "library_name" | "libraryname" => Self::LibraryName,
            "library_folder_id" | "libraryfolderid" => Self::LibraryFolderId,
            "content_kind" | "contentkind" => Self::ContentKind,
            "sub_kind" | "subkind" => Self::SubKind,
            "parent_title" | "parenttitle" => Self::ParentTitle,
            "content_rating" => Self::ContentRating,
            "show_title" => Self::ShowTitle,
            "show_genre" => Self::ShowGenre,
            "show_tag" => Self::ShowTag,
            "show_studio" => Self::ShowStudio,
            "show_network" => Self::ShowNetwork,
            "show_content_rating" => Self::ShowContentRating,
            "season_number" | "season" => Self::SeasonNumber,
            "episode_number" | "episode" => Self::EpisodeNumber,
            "metadata_kind" => Self::MetadataKind,
            "addon_list" | "addonlist" => Self::AddonList,
            "style" => Self::Style,
            "mood" => Self::Mood,
            "album" => Self::Album,
            "artist" => Self::Artist,
            "album_artist" | "albumartist" => Self::AlbumArtist,
            "library_id" | "libraryid" => Self::LibraryId,
            "released_onthisday" => Self::ReleasedOnThisDay,
            "released_inthelast" => Self::ReleasedInTheLast,
            "released_notinthelast" => Self::ReleasedNotInTheLast,
            "added_inthelast" => Self::AddedInTheLast,
            "added_notinthelast" => Self::AddedNotInTheLast,
            "guid" | "provider_guid" => Self::ProviderGuid(None),
            source => Self::ProviderGuid(Some(source.to_owned())),
        }
    }

    fn is_numeric(&self) -> bool {
        matches!(
            self,
            Self::Year
                | Self::Width
                | Self::Height
                | Self::Chapters
                | Self::Minutes
                | Self::Seconds
                | Self::SeasonNumber
                | Self::EpisodeNumber
                | Self::VideoBitDepth
                | Self::LibraryId
                | Self::LibraryFolderId
        )
    }

    fn is_case_sensitive(&self) -> bool {
        matches!(self, Self::ContentRating | Self::ShowContentRating)
    }

    fn text_matches(&self, actual: &str, expected: &str, wildcard: bool) -> bool {
        if matches!(self, Self::ContentKind) {
            return ContentKind::from_search_token(actual)
                .zip(ContentKind::from_search_token(expected))
                .is_some_and(|(actual, expected)| actual == expected);
        }

        if matches!(self, Self::SubKind) {
            return normalize_sub_kind_token(actual)
                .zip(normalize_sub_kind_token(expected))
                .is_some_and(|(actual, expected)| actual == expected);
        }

        if self.is_numeric() && !wildcard {
            return actual
                .parse::<i64>()
                .ok()
                .zip(expected.parse::<i64>().ok())
                .is_some_and(|(actual, expected)| actual == expected);
        }

        if self.is_case_sensitive() {
            return if wildcard || expected.contains(['*', '?']) {
                wildcard_matches(actual, expected)
            } else {
                actual == expected
            };
        }

        text_matches(actual, expected, wildcard)
    }

    fn range_matches(
        &self,
        actual: &str,
        start: Option<&str>,
        end: Option<&str>,
        inclusive: bool,
    ) -> bool {
        if self.is_numeric() {
            return numeric_range_matches(actual, start, end, inclusive);
        }

        range_matches(actual, start, end, inclusive)
    }

    fn relative_date_matches(
        &self,
        document: &SearchDocument,
        value: &SearchValue,
    ) -> Option<bool> {
        let today = Utc::now().date_naive();
        match self {
            Self::ReleasedOnThisDay => {
                let date = document.release_date?.date_naive();
                Some(date.month() == today.month() && date.day() == today.day())
            }
            Self::ReleasedInTheLast => {
                let start = relative_date_start(search_value_text(value), today)?;
                let date = document.release_date?.date_naive();
                Some(date >= start && date <= today)
            }
            Self::ReleasedNotInTheLast => {
                let finish = relative_date_start(search_value_text(value), today)?;
                let date = document.release_date?.date_naive();
                Some(date < finish)
            }
            Self::AddedInTheLast => {
                let start = relative_date_start(search_value_text(value), today)?;
                let date = document.added_date?.date_naive();
                Some(date >= start && date <= today)
            }
            Self::AddedNotInTheLast => {
                let finish = relative_date_start(search_value_text(value), today)?;
                let date = document.added_date?.date_naive();
                Some(date < finish)
            }
            _ => None,
        }
    }
}

fn search_value_text(value: &SearchValue) -> &str {
    match value {
        SearchValue::Text(value)
        | SearchValue::Wildcard(value)
        | SearchValue::Fuzzy { value, .. }
        | SearchValue::Phrase { value, .. } => value,
        SearchValue::Range { .. } => "",
    }
}

fn relative_date_start(text: &str, today: NaiveDate) -> Option<NaiveDate> {
    let number = text.split_whitespace().next()?.parse::<i64>().ok()?;
    let lower = text.to_ascii_lowercase();
    if lower.contains("day") {
        today.checked_sub_signed(Duration::days(number))
    } else if lower.contains("week") {
        today.checked_sub_signed(Duration::weeks(number))
    } else if lower.contains("month") {
        today.checked_sub_months(Months::new(u32::try_from(number).ok()?))
    } else if lower.contains("year") {
        today.checked_sub_months(Months::new(u32::try_from(number.checked_mul(12)?).ok()?))
    } else {
        None
    }
}

fn option_values<const N: usize>(values: [Option<&str>; N]) -> Vec<String> {
    values
        .into_iter()
        .flatten()
        .map(ToOwned::to_owned)
        .collect()
}

fn text_matches(actual: &str, expected: &str, wildcard: bool) -> bool {
    let actual = normalize_search_text(actual);
    let expected = normalize_search_text(expected);
    if wildcard || expected.contains('*') || expected.contains('?') {
        wildcard_matches(&actual, &expected)
    } else {
        expected.split_whitespace().all(|token| {
            actual
                .split_whitespace()
                .any(|actual| actual.contains(token))
        })
    }
}

fn fuzzy_matches(actual: &str, expected: &str, max_edits: usize) -> bool {
    let expected = normalize_search_text(expected);
    let expected_tokens = expected.split_whitespace().collect::<Vec<_>>();
    if expected_tokens.is_empty() {
        return false;
    }

    let actual = normalize_search_text(actual);
    let actual_tokens = actual.split_whitespace().collect::<Vec<_>>();
    expected_tokens.iter().all(|expected| {
        actual_tokens
            .iter()
            .any(|actual| levenshtein_distance(actual, expected) <= max_edits)
    })
}

fn phrase_matches(actual: &str, expected: &str, slop: usize) -> bool {
    let actual = normalize_search_text(actual);
    let expected = normalize_search_text(expected);
    let expected_tokens = expected.split_whitespace().collect::<Vec<_>>();
    if expected_tokens.is_empty() {
        return false;
    }

    if slop == 0 {
        return actual.contains(&expected);
    }

    let actual_tokens = actual.split_whitespace().collect::<Vec<_>>();
    let Some(first) = expected_tokens.first() else {
        return false;
    };

    actual_tokens
        .iter()
        .enumerate()
        .filter(|(_, token)| *token == first)
        .any(|(start, _)| phrase_matches_from(&actual_tokens, &expected_tokens, start, slop))
}

fn phrase_matches_from(
    actual_tokens: &[&str],
    expected_tokens: &[&str],
    start: usize,
    slop: usize,
) -> bool {
    let mut cursor = start;
    let mut consumed_slop = 0usize;
    for expected in expected_tokens.iter().skip(1) {
        let Some((found, _)) = actual_tokens
            .iter()
            .enumerate()
            .skip(cursor + 1)
            .find(|(_, actual)| *actual == expected)
        else {
            return false;
        };
        consumed_slop += found.saturating_sub(cursor + 1);
        if consumed_slop > slop {
            return false;
        }
        cursor = found;
    }
    true
}

fn numeric_range_matches(
    actual: &str,
    start: Option<&str>,
    end: Option<&str>,
    inclusive: bool,
) -> bool {
    let Some(actual) = actual.parse::<i64>().ok() else {
        return false;
    };

    let start_matches = start
        .filter(|value| *value != "*")
        .and_then(|value| value.parse::<i64>().ok())
        .is_none_or(|start| {
            if inclusive {
                actual >= start
            } else {
                actual > start
            }
        });
    let end_matches = end
        .filter(|value| *value != "*")
        .and_then(|value| value.parse::<i64>().ok())
        .is_none_or(|end| {
            if inclusive {
                actual <= end
            } else {
                actual < end
            }
        });
    start_matches && end_matches
}

fn range_matches(actual: &str, start: Option<&str>, end: Option<&str>, inclusive: bool) -> bool {
    let actual = normalize_range_value(actual);
    let start_matches = start.map(normalize_range_value).is_none_or(|start| {
        if inclusive {
            actual >= start
        } else {
            actual > start
        }
    });
    let end_matches = end.map(normalize_range_value).is_none_or(|end| {
        if inclusive {
            actual <= end
        } else {
            actual < end
        }
    });
    start_matches && end_matches
}

fn normalize_range_value(value: &str) -> String {
    normalize_search_text(value)
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
}

fn wildcard_matches(actual: &str, pattern: &str) -> bool {
    let actual = actual.as_bytes();
    let pattern = pattern.as_bytes();
    let (mut ai, mut pi) = (0, 0);
    let mut star = None;
    let mut match_after_star = 0;

    while ai < actual.len() {
        if pi < pattern.len() && (pattern[pi] == b'?' || pattern[pi] == actual[ai]) {
            ai += 1;
            pi += 1;
        } else if pi < pattern.len() && pattern[pi] == b'*' {
            star = Some(pi);
            match_after_star = ai;
            pi += 1;
        } else if let Some(star_index) = star {
            pi = star_index + 1;
            match_after_star += 1;
            ai = match_after_star;
        } else {
            return false;
        }
    }

    while pi < pattern.len() && pattern[pi] == b'*' {
        pi += 1;
    }

    pi == pattern.len()
}

fn normalize_search_text(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len());
    for ch in value.chars().flat_map(char::to_lowercase) {
        if let Some(folded) = fold_accent(ch) {
            normalized.push_str(folded);
        } else {
            normalized.push(ch);
        }
    }
    normalized
}

fn fold_accent(ch: char) -> Option<&'static str> {
    match ch {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => Some("a"),
        'æ' => Some("ae"),
        'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => Some("c"),
        'ď' | 'đ' => Some("d"),
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => Some("e"),
        'ƒ' => Some("f"),
        'ĝ' | 'ğ' | 'ġ' | 'ģ' => Some("g"),
        'ĥ' | 'ħ' => Some("h"),
        'ì' | 'í' | 'î' | 'ï' | 'ĩ' | 'ī' | 'ĭ' | 'į' | 'ı' => Some("i"),
        'ĵ' => Some("j"),
        'ķ' | 'ĸ' => Some("k"),
        'ĺ' | 'ļ' | 'ľ' | 'ŀ' | 'ł' => Some("l"),
        'ñ' | 'ń' | 'ņ' | 'ň' | 'ŉ' => Some("n"),
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ŏ' | 'ő' => Some("o"),
        'œ' => Some("oe"),
        'ŕ' | 'ŗ' | 'ř' => Some("r"),
        'ś' | 'ŝ' | 'ş' | 'š' | 'ß' => Some("s"),
        'ţ' | 'ť' | 'ŧ' => Some("t"),
        'ù' | 'ú' | 'û' | 'ü' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => Some("u"),
        'ŵ' => Some("w"),
        'ý' | 'ÿ' | 'ŷ' => Some("y"),
        'ź' | 'ż' | 'ž' => Some("z"),
        _ => None,
    }
}

fn levenshtein_distance(left: &str, right: &str) -> usize {
    let right_len = right.chars().count();
    let mut costs = (0..=right_len).collect::<Vec<_>>();

    for (left_index, left_ch) in left.chars().enumerate() {
        let mut previous = costs[0];
        costs[0] = left_index + 1;
        for (right_index, right_ch) in right.chars().enumerate() {
            let current = costs[right_index + 1];
            costs[right_index + 1] = if left_ch == right_ch {
                previous
            } else {
                1 + previous.min(current).min(costs[right_index])
            };
            previous = current;
        }
    }

    costs[right_len]
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Word(String),
    Phrase(String),
    Colon,
    LeftParen,
    RightParen,
    LeftBracket,
    RightBracket,
    LeftBrace,
    RightBrace,
    Caret,
    To,
    And,
    Or,
    Not,
    Plus,
    Minus,
}

struct SearchParser {
    tokens: Vec<Token>,
    position: usize,
}

impl SearchParser {
    fn new(query: &str) -> Self {
        Self {
            tokens: tokenize(query),
            position: 0,
        }
    }

    fn parse(mut self) -> SearchExpression {
        if self.tokens.is_empty() {
            return SearchExpression::MatchAll;
        }

        self.parse_or()
    }

    fn parse_or(&mut self) -> SearchExpression {
        let mut expressions = vec![self.parse_and()];
        while self.consume(&Token::Or) {
            expressions.push(self.parse_and());
        }

        if expressions.len() == 1 {
            expressions.remove(0)
        } else {
            SearchExpression::Or(expressions)
        }
    }

    fn parse_and(&mut self) -> SearchExpression {
        let mut expressions = vec![self.parse_unary()];
        while !self.is_done() && !self.check(&Token::RightParen) && !self.check(&Token::Or) {
            let _ = self.consume(&Token::And);
            expressions.push(self.parse_unary());
        }

        if expressions.len() == 1 {
            expressions.remove(0)
        } else {
            SearchExpression::And(expressions)
        }
    }

    fn parse_unary(&mut self) -> SearchExpression {
        if self.consume(&Token::Not) || self.consume(&Token::Minus) {
            SearchExpression::Not(Box::new(self.parse_unary()))
        } else {
            let _ = self.consume(&Token::Plus);
            self.parse_primary()
        }
    }

    fn parse_primary(&mut self) -> SearchExpression {
        if self.consume(&Token::LeftParen) {
            let expression = self.parse_or();
            let _ = self.consume(&Token::RightParen);
            return expression;
        }

        let Some(token) = self.advance() else {
            return SearchExpression::MatchAll;
        };

        let (field, value_token) = if let Token::Word(field_name) = &token {
            if self.consume(&Token::Colon)
                || self.consume_word("=")
                || self.consume_word("==")
                || self.consume_word("contains")
            {
                (SearchField::parse(field_name), self.advance())
            } else if field_name.eq_ignore_ascii_case("has_commentary") {
                (
                    SearchField::HasCommentary,
                    Some(Token::Word("true".to_owned())),
                )
            } else {
                (SearchField::Default, Some(token))
            }
        } else {
            (SearchField::Default, Some(token))
        };

        let value = match value_token {
            Some(Token::LeftBracket) => self.parse_range(true),
            Some(Token::LeftBrace) => self.parse_range(false),
            Some(Token::Phrase(value)) => self.parse_text_value(value, true),
            Some(Token::Word(value)) => self.parse_text_value(value, false),
            Some(other) => SearchValue::Text(token_text(&other)),
            None => SearchValue::Text(String::new()),
        };

        self.consume_boost();

        SearchExpression::Term(SearchTerm { field, value })
    }

    fn parse_text_value(&mut self, mut value: String, quoted: bool) -> SearchValue {
        let mut slop = 0usize;
        let mut fuzzy = None;

        if quoted {
            if let Some(Token::Word(modifier)) = self.tokens.get(self.position).cloned() {
                if let Some(rest) = modifier.strip_prefix('~') {
                    self.position += 1;
                    let (slop_text, _) = split_boost(rest);
                    slop = slop_text.parse::<usize>().unwrap_or(0);
                }
            }
        } else if let Some((head, rest)) = value.rsplit_once('~') {
            let head = head.to_owned();
            let rest = rest.to_owned();
            value = head;
            let (fuzzy_text, _) = split_boost(&rest);
            fuzzy = Some(parse_fuzzy_edits(fuzzy_text));
        }

        let (value, _) = split_boost(&value);
        let value = value.to_owned();

        if value.contains(['*', '?']) {
            SearchValue::Wildcard(value)
        } else if let Some(max_edits) = fuzzy {
            SearchValue::Fuzzy { value, max_edits }
        } else if quoted {
            SearchValue::Phrase { value, slop }
        } else {
            SearchValue::Text(value)
        }
    }

    fn parse_range(&mut self, inclusive: bool) -> SearchValue {
        let start = self.parse_range_part();
        let _ = self.consume(&Token::To);
        let end = self.parse_range_part();
        if inclusive {
            let _ = self.consume(&Token::RightBracket);
        } else {
            let _ = self.consume(&Token::RightBrace);
        }

        SearchValue::Range {
            start: start.filter(|value| value != "*"),
            end: end.filter(|value| value != "*"),
            inclusive,
        }
    }

    fn parse_range_part(&mut self) -> Option<String> {
        match self.advance() {
            Some(Token::Word(value) | Token::Phrase(value)) => Some(value),
            Some(token) => Some(token_text(&token)),
            None => None,
        }
    }

    fn advance(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.position).cloned();
        if token.is_some() {
            self.position += 1;
        }
        token
    }

    fn consume(&mut self, expected: &Token) -> bool {
        if self.check(expected) {
            self.position += 1;
            true
        } else {
            false
        }
    }

    fn consume_word(&mut self, expected: &str) -> bool {
        if self.tokens.get(self.position).is_some_and(
            |token| matches!(token, Token::Word(value) if value.eq_ignore_ascii_case(expected)),
        ) {
            self.position += 1;
            true
        } else {
            false
        }
    }

    fn consume_boost(&mut self) {
        if self.consume(&Token::Caret) {
            let _ = self.advance();
        }
    }

    fn check(&self, expected: &Token) -> bool {
        self.tokens
            .get(self.position)
            .is_some_and(|token| token == expected)
    }

    fn is_done(&self) -> bool {
        self.position >= self.tokens.len()
    }
}

fn tokenize(query: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = query.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            ch if ch.is_whitespace() => {}
            ':' => tokens.push(Token::Colon),
            '(' => tokens.push(Token::LeftParen),
            ')' => tokens.push(Token::RightParen),
            '[' => tokens.push(Token::LeftBracket),
            ']' => tokens.push(Token::RightBracket),
            '{' => tokens.push(Token::LeftBrace),
            '}' => tokens.push(Token::RightBrace),
            '^' => tokens.push(Token::Caret),
            '+' => tokens.push(Token::Plus),
            '-' => tokens.push(Token::Minus),
            '"' | '\'' => {
                let quote = ch;
                let mut value = String::new();
                for next in chars.by_ref() {
                    if next == quote {
                        break;
                    }
                    value.push(next);
                }
                tokens.push(Token::Phrase(value));
            }
            _ => {
                let mut value = String::from(ch);
                while let Some(next) = chars.peek().copied() {
                    if next.is_whitespace()
                        || matches!(next, ':' | '(' | ')' | '[' | ']' | '{' | '}')
                    {
                        break;
                    }
                    value.push(next);
                    let _ = chars.next();
                }
                tokens.push(word_token(value));
            }
        }
    }

    tokens
}

fn word_token(value: String) -> Token {
    match value.to_ascii_uppercase().as_str() {
        "AND" => Token::And,
        "OR" => Token::Or,
        "NOT" => Token::Not,
        "TO" => Token::To,
        _ => Token::Word(value),
    }
}

fn split_boost(value: &str) -> (&str, Option<&str>) {
    value
        .split_once('^')
        .map_or((value, None), |(head, tail)| (head, Some(tail)))
}

fn parse_fuzzy_edits(value: &str) -> usize {
    if value.is_empty() {
        return 2;
    }

    value.parse::<usize>().map_or_else(
        |_| {
            value
                .parse::<f32>()
                .ok()
                .map_or(2, |similarity| if similarity >= 0.5 { 2 } else { 1 })
        },
        |edits| edits.min(2),
    )
}

fn token_text(token: &Token) -> String {
    match token {
        Token::Word(value) | Token::Phrase(value) => value.clone(),
        Token::Colon => ":".to_owned(),
        Token::LeftParen => "(".to_owned(),
        Token::RightParen => ")".to_owned(),
        Token::LeftBracket => "[".to_owned(),
        Token::RightBracket => "]".to_owned(),
        Token::LeftBrace => "{".to_owned(),
        Token::RightBrace => "}".to_owned(),
        Token::Caret => "^".to_owned(),
        Token::To => "TO".to_owned(),
        Token::And => "AND".to_owned(),
        Token::Or => "OR".to_owned(),
        Token::Not => "NOT".to_owned(),
        Token::Plus => "+".to_owned(),
        Token::Minus => "-".to_owned(),
    }
}

#[must_use]
pub fn search_page_map<'a, I>(documents: I, limit: usize) -> SearchPageMap
where
    I: IntoIterator<Item = &'a SearchDocument>,
{
    let letters = "#abcdefghijklmnopqrstuvwxyz".chars().collect::<Vec<_>>();
    let mut page_map = letters
        .iter()
        .copied()
        .map(|letter| (letter, 0usize))
        .collect::<BTreeMap<_, _>>();
    let documents = documents.into_iter().collect::<Vec<_>>();

    if limit == 0 || documents.is_empty() {
        return SearchPageMap { page_map };
    }

    let mut current = 0usize;
    let mut page = 0usize;
    while current < documents.len() {
        page += 1;
        current = (current + limit).min(documents.len());
        let jump_letter = jump_letter(documents[current - 1]);
        let Some(jump_index) = letters.iter().position(|letter| *letter == jump_letter) else {
            continue;
        };

        for letter in letters.iter().copied().take(jump_index + 1) {
            page_map.entry(letter).and_modify(|value| {
                if *value == 0 {
                    *value = page;
                }
            });
        }
    }

    let max_page = page_map.values().copied().max().unwrap_or_default();
    for value in page_map.values_mut().filter(|value| **value == 0) {
        *value = max_page;
    }

    SearchPageMap { page_map }
}

fn jump_letter(document: &SearchDocument) -> char {
    let letter = document
        .sort_title
        .as_deref()
        .or(document.title.as_deref())
        .unwrap_or_default()
        .chars()
        .next()
        .unwrap_or('#')
        .to_ascii_lowercase();

    if letter.is_ascii_lowercase() {
        letter
    } else {
        '#'
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document() -> SearchDocument {
        SearchDocument {
            media_item_id: 1,
            media_kind: "movie".to_owned(),
            metadata_kind: "Movie".to_owned(),
            title: Some("The Great Escape".to_owned()),
            sort_title: Some("great escape".to_owned()),
            original_title: Some("Original Escape".to_owned()),
            year: Some(1963),
            release_date: DateTime::parse_from_rfc3339("1963-07-04T00:00:00Z")
                .ok()
                .map(|date| date.with_timezone(&Utc)),
            genres: vec!["Adventure".to_owned()],
            tags: vec!["Classic".to_owned()],
            studios: vec!["United Artists".to_owned()],
            actors: vec!["Steve McQueen".to_owned()],
            directors: vec!["John Sturges".to_owned()],
            writers: vec!["James Clavell".to_owned()],
            languages: vec!["English".to_owned()],
            language_tags: vec!["eng".to_owned()],
            subtitle_languages: vec!["Dutch".to_owned()],
            subtitle_language_tags: vec!["nld".to_owned()],
            video_codecs: vec!["h264".to_owned()],
            audio_codecs: vec!["aac".to_owned()],
            subtitle_codecs: vec!["srt".to_owned()],
            width: Some(1920),
            height: Some(1080),
            chapter_counts: vec![3],
            durations_seconds: vec![3661],
            show_title: Some("Not a Show".to_owned()),
            provider_guids: vec![SearchProviderGuid {
                source: "tmdb".to_owned(),
                value: "5925".to_owned(),
            }],
            ..SearchDocument::default()
        }
    }

    #[test]
    fn parses_fielded_boolean_queries() {
        let query =
            SearchExpression::parse("genre:Adventure AND actor:\"Steve McQueen\" -tag:Noir");
        assert!(query.matches(&document()));
    }

    #[test]
    fn parses_ranges_and_provider_guid_fields() {
        let doc = document();
        assert!(SearchExpression::parse("year:[1960 TO 1970] AND tmdb:5925").matches(&doc));
        assert!(SearchExpression::parse("release_date:[19630701 TO 19630731]").matches(&doc));
        assert!(!SearchExpression::parse("type:episode OR guid:missing").matches(&doc));
    }

    #[test]
    fn matches_stream_credit_and_dimension_fields() {
        let doc = document();
        assert!(SearchExpression::parse(
            "director:Sturges AND writer:Clavell AND language_tag:eng AND sub_language_tag:nld"
        )
        .matches(&doc));
        assert!(
            SearchExpression::parse(
                "video_codec:h264 AND audio_codec:aac AND subtitle_codec:srt AND width:1920 AND height:1080"
            )
            .matches(&doc)
        );
    }

    #[test]
    fn defaults_to_primary_content_kind_unless_variant_query_opts_in() {
        let primary = document();
        let mut extra = document();
        extra.content_kind = ContentKind::Extra;
        extra.sub_kind = Some("featurette".to_owned());
        extra.parent_title = Some("The Great Escape".to_owned());

        assert!(SearchExpression::parse("title:Great").matches(&primary));
        assert!(!SearchExpression::parse("title:Great").matches(&extra));
        assert!(
            SearchExpression::parse("content_kind:extra AND sub_kind:featurettes").matches(&extra)
        );
        assert!(SearchExpression::parse("parent_title:\"Great Escape\"").matches(&extra));
    }

    #[test]
    fn matches_stream_titles_and_commentary_role() {
        let mut doc = document();
        doc.audio_titles = vec!["Director Commentary".to_owned()];
        doc.subtitle_titles = vec!["Commentary subtitles".to_owned()];
        doc.has_commentary = true;

        assert!(SearchExpression::parse("audio_title:commentary").matches(&doc));
        assert!(SearchExpression::parse("subtitle_title:commentary").matches(&doc));
        assert!(SearchExpression::parse("has_commentary").matches(&doc));
        assert!(!SearchExpression::parse("has_commentary:false").matches(&doc));
    }

    #[test]
    fn analyzer_matches_case_accents_fuzzy_proximity_and_boosts() {
        let mut doc = document();
        doc.title = Some("Amélie has a Great Long Escape".to_owned());
        assert!(SearchExpression::parse("title:amelie").matches(&doc));
        assert!(SearchExpression::parse("title:escap~1").matches(&doc));
        assert!(SearchExpression::parse("title:\"great escape\"~2").matches(&doc));
        assert!(SearchExpression::parse("title:Great^4").matches(&doc));
    }

    #[test]
    fn matches_numeric_chapter_duration_ranges_and_exact_terms() {
        let doc = document();
        assert!(SearchExpression::parse("chapters:3 AND minutes:62").matches(&doc));
        assert!(SearchExpression::parse("seconds:[3600 TO 3700]").matches(&doc));
        assert!(!SearchExpression::parse("width:192").matches(&doc));
    }

    #[test]
    fn filters_by_library_id() {
        let mut doc = document();
        doc.library_id = Some(7);
        assert!(SearchExpression::parse("library_id:7").matches(&doc));
        assert!(!SearchExpression::parse("library_id:8").matches(&doc));
        // Documents without a library id never match the filter.
        doc.library_id = None;
        assert!(!SearchExpression::parse("library_id:7").matches(&doc));
    }

    #[test]
    fn matches_documented_library_collection_music_and_video_fields() {
        let mut doc = document();
        doc.library_name = Some("Music Videos".to_owned());
        doc.library_folder_ids = vec![3];
        doc.collections = vec!["Pruned".to_owned()];
        doc.content_ratings = vec!["TV-MA".to_owned()];
        doc.networks = vec!["HBO".to_owned()];
        doc.show_networks = vec!["AMC".to_owned()];
        doc.show_content_ratings = vec!["TV-14".to_owned()];
        doc.styles = vec!["Lush".to_owned()];
        doc.moods = vec!["Melancholy".to_owned()];
        doc.albums = vec!["Dirt".to_owned()];
        doc.artists = vec!["Alice in Chains".to_owned()];
        doc.album_artists = vec!["Various Artists".to_owned()];
        doc.video_bit_depths = vec![10];
        doc.video_dynamic_ranges = vec!["hdr".to_owned()];

        assert!(SearchExpression::parse(
            "library_name:music AND library_folder_id:3 AND collection:pruned AND \
             network:hbo AND show_network:amc AND show_content_rating:TV-14 AND \
             style:lush AND mood:melancholy AND album:dirt AND artist:\"Alice in Chains\" AND \
             album_artist:various AND video_bit_depth:10 AND video_dynamic_range:hdr"
        )
        .matches(&doc));
        assert!(SearchExpression::parse("content_rating:TV-MA").matches(&doc));
        assert!(!SearchExpression::parse("content_rating:tv-ma").matches(&doc));
        assert!(!SearchExpression::parse("NOT artist:\"Alice in Chains\"").matches(&doc));
    }

    #[test]
    fn music_label_is_a_first_class_search_field() {
        let mut doc = document();
        doc.music_labels = vec!["Blue Note".to_owned()];

        assert!(SearchExpression::parse("music_label:\"Blue Note\"").matches(&doc));
        assert!(!SearchExpression::parse("studio:\"Blue Note\"").matches(&doc));
    }

    #[test]
    fn matches_added_and_relative_date_helpers() {
        let today = Utc::now().date_naive();
        let yesterday = today
            .checked_sub_signed(Duration::days(1))
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();
        let old = today
            .checked_sub_signed(Duration::days(14))
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();

        let mut recent = document();
        recent.release_date = Some(yesterday);
        recent.added_date = Some(yesterday);
        assert!(SearchExpression::parse("released_inthelast:\"2 days\"").matches(&recent));
        assert!(SearchExpression::parse("added_inthelast:\"2 days\"").matches(&recent));
        assert!(SearchExpression::parse("added_date:[20000101 TO 29991231]").matches(&recent));

        let mut older = document();
        older.release_date = Some(old);
        older.added_date = Some(old);
        assert!(SearchExpression::parse("released_notinthelast:\"1 week\"").matches(&older));
        assert!(SearchExpression::parse("added_notinthelast:\"1 week\"").matches(&older));
    }

    #[test]
    fn builds_lucene_style_jump_letter_page_map() {
        let docs = [
            SearchDocument {
                sort_title: Some("alpha".to_owned()),
                ..SearchDocument::default()
            },
            SearchDocument {
                sort_title: Some("delta".to_owned()),
                ..SearchDocument::default()
            },
            SearchDocument {
                sort_title: Some("zulu".to_owned()),
                ..SearchDocument::default()
            },
        ];

        let map = search_page_map(docs.iter(), 2);
        assert_eq!(map.page_map[&'#'], 1);
        assert_eq!(map.page_map[&'a'], 1);
        assert_eq!(map.page_map[&'d'], 1);
        assert_eq!(map.page_map[&'e'], 2);
        assert_eq!(map.page_map[&'z'], 2);
    }
}
