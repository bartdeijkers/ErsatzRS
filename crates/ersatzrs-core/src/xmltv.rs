//! Core XMLTV guide projection rules.
//!
//! The transport layer renders XML, but the guide block behavior is a persisted
//! `ErsatzTV` configuration contract and belongs with the non-web scheduling
//! model.

use std::collections::BTreeMap;

use crate::repositories::ArtworkReference;
use chrono::{DateTime, Duration, Utc};
use ersatzrs_domain::FillerKind;

/// `ErsatzTV` `XmltvBlockBehavior`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum XmltvBlockBehavior {
    /// Divide a guide block's configured guide window evenly between its
    /// non-filler items.
    #[default]
    SplitTimeEvenly = 0,
    /// Use each playout item's actual start/finish timestamps.
    UseActualTimes = 1,
}

impl XmltvBlockBehavior {
    /// Decode the persisted `ConfigElement` value.
    ///
    /// `ErsatzTV` stores enum values through `ToString()`, but older or imported
    /// values may be numeric.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "0" | "splittimeevenly" | "split_time_evenly" | "split time evenly" => {
                Some(Self::SplitTimeEvenly)
            }
            "1" | "useactualtimes" | "use_actual_times" | "use actual times" => {
                Some(Self::UseActualTimes)
            }
            _ => None,
        }
    }
}

/// `ErsatzTV` `XmltvTimeZone` — controls whether `<programme>` start/stop
/// timestamps are rendered in the server's local UTC offset or in UTC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum XmltvTimeZone {
    #[default]
    Local,
    Utc,
}

impl XmltvTimeZone {
    /// Decode the persisted `ConfigElement` value (`"Local"` or `"Utc"`).
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "local" => Some(Self::Local),
            "utc" => Some(Self::Utc),
            _ => None,
        }
    }
}

/// The `MediaItem` subtype backing a guide item, driving which
/// category-literal/episode-num/rating-system rules a `<programme>` uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum XmltvItemKind {
    #[default]
    Movie,
    Episode,
    MusicVideo,
    Song,
    OtherVideo,
    RemoteStream,
}

/// Which artwork kind `icon_artwork` actually resolved to, so the web layer
/// can request the matching resize budget from `artwork_url` (poster-sized
/// vs. thumbnail-sized) instead of guessing from `XmltvItemKind` alone —
/// e.g. an episode that fell back to its show's poster should be resized
/// like a poster, not like the thumbnail episodes usually have.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IconSizeHint {
    #[default]
    Poster,
    Thumbnail,
}

/// Rich per-item metadata riding alongside the block-behavior timing
/// projection. Populated by the SQL loader, consumed by the XML template
/// renderer; the timing math in this module never inspects it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct XmltvProgrammeContent {
    pub kind: XmltvItemKind,
    /// Episode title (episodes) or video title (music videos/songs) — the
    /// item's own title, distinct from the `<title>` shown for the item
    /// (which is the show/artist name for those kinds).
    pub sub_title: Option<String>,
    pub description: Option<String>,
    pub year: Option<i32>,
    pub genres: Vec<String>,
    /// Provider subject labels and other searchable keywords.
    pub tags: Vec<String>,
    /// Artist-level genres, kept separate from music-video genres to match
    /// ErsatzTV's custom-template context.
    pub artist_genres: Vec<String>,
    /// Raw, possibly `/`-delimited multi-value `ContentRating` string.
    pub content_rating: Option<String>,
    /// Persisted artwork identity, resolved to a servable URL at render time.
    pub icon_artwork: Option<ArtworkReference>,
    /// Ignored when `icon_artwork` is `None`.
    pub icon_size_hint: IconSizeHint,
    /// Episode-level still artwork. Episode `icon_artwork` is deliberately the
    /// parent show's prioritized artwork, matching ErsatzTV.
    pub thumbnail_artwork: Option<ArtworkReference>,
    /// Ignored when `thumbnail_artwork` is `None`.
    pub thumbnail_size_hint: IconSizeHint,
    pub season_number: Option<i32>,
    pub episode_number: Option<i32>,
    pub guids: Vec<String>,
    pub episode_guids: Vec<String>,
    pub track: Option<String>,
    pub album: Option<String>,
    pub release_date: Option<String>,
    pub collection: Option<String>,
    pub people: Vec<XmltvCredit>,
    pub original_broadcasters: Vec<String>,
    pub broadcasters: Vec<String>,
    pub artists: Vec<String>,
    pub album_artists: Vec<String>,
    pub comment: Option<String>,
    pub studios: Vec<String>,
    pub directors: Vec<String>,
}

/// One credited person or production company projected into XMLTV.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XmltvCredit {
    pub name: String,
    pub role: String,
}

/// Raw playout item data needed to project an XMLTV programme.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XmltvGuideItem {
    pub channel_number: String,
    pub start: DateTime<Utc>,
    pub finish: DateTime<Utc>,
    pub guide_start: Option<DateTime<Utc>>,
    pub guide_finish: Option<DateTime<Utc>>,
    pub guide_group: i32,
    pub filler_kind: FillerKind,
    pub custom_title: Option<String>,
    pub title: String,
    pub content: XmltvProgrammeContent,
}

/// XMLTV programme data after configured block behavior has been applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XmltvProgramme {
    pub channel_number: String,
    pub start: DateTime<Utc>,
    pub stop: DateTime<Utc>,
    pub custom_title: Option<String>,
    pub title: String,
    pub content: XmltvProgrammeContent,
}

/// Apply `ErsatzTV` XMLTV block behavior to sorted playout items.
#[must_use]
pub fn project_programmes(
    items: &[XmltvGuideItem],
    behavior: XmltvBlockBehavior,
) -> Vec<XmltvProgramme> {
    match behavior {
        XmltvBlockBehavior::UseActualTimes => items
            .iter()
            .filter(|item| item.filler_kind == FillerKind::None)
            .map(|item| XmltvProgramme {
                channel_number: item.channel_number.clone(),
                start: item.start,
                stop: item.finish,
                custom_title: item.custom_title.clone(),
                title: item.title.clone(),
                content: item.content.clone(),
            })
            .collect(),
        XmltvBlockBehavior::SplitTimeEvenly => split_time_evenly(items),
    }
}

fn split_time_evenly(items: &[XmltvGuideItem]) -> Vec<XmltvProgramme> {
    type GuideGroupKey = (String, DateTime<Utc>, DateTime<Utc>, i32);

    let mut groups: BTreeMap<GuideGroupKey, Vec<&XmltvGuideItem>> = BTreeMap::new();

    for item in items {
        if item.filler_kind != FillerKind::None {
            continue;
        }

        let guide_start = item.guide_start.unwrap_or(item.start);
        let guide_finish = item.guide_finish.unwrap_or(item.finish);
        groups
            .entry((
                item.channel_number.clone(),
                guide_start,
                guide_finish,
                item.guide_group,
            ))
            .or_default()
            .push(item);
    }

    let mut result = Vec::new();
    for ((channel_number, group_start, group_finish, _), group_items) in groups {
        let count = i32::try_from(group_items.len()).unwrap_or(i32::MAX).max(1);
        let per_item = divide_duration(group_finish - group_start, count);
        let mut current_start = group_start;

        for item in group_items {
            let current_stop = current_start + per_item;
            result.push(XmltvProgramme {
                channel_number: channel_number.clone(),
                start: current_start,
                stop: current_stop,
                custom_title: item.custom_title.clone(),
                title: item.title.clone(),
                content: item.content.clone(),
            });
            current_start = current_stop;
        }
    }

    result.sort_by(|left, right| {
        left.channel_number
            .cmp(&right.channel_number)
            .then(left.start.cmp(&right.start))
            .then(left.stop.cmp(&right.stop))
            .then(left.title.cmp(&right.title))
    });
    result
}

fn divide_duration(duration: Duration, divisor: i32) -> Duration {
    Duration::milliseconds(duration.num_milliseconds() / i64::from(divisor.max(1)))
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn parses_named_time_zone_case_insensitively() {
        assert_eq!(XmltvTimeZone::parse("Local"), Some(XmltvTimeZone::Local));
        assert_eq!(XmltvTimeZone::parse("utc"), Some(XmltvTimeZone::Utc));
        assert_eq!(XmltvTimeZone::parse("bogus"), None);
    }

    #[test]
    fn split_time_evenly_groups_by_guide_window() {
        let items = vec![
            item("1", 20, 0, 20, 10, "First"),
            item("1", 20, 10, 20, 20, "Second"),
            XmltvGuideItem {
                filler_kind: FillerKind::Fallback,
                title: "Fallback".to_owned(),
                ..item("1", 20, 20, 20, 30, "Fallback")
            },
        ];

        let programmes = project_programmes(&items, XmltvBlockBehavior::SplitTimeEvenly);

        assert_eq!(programmes.len(), 2);
        assert_eq!(programmes[0].title, "First");
        assert_eq!(
            programmes[0].stop,
            Utc.with_ymd_and_hms(2026, 4, 17, 20, 30, 0).unwrap()
        );
        assert_eq!(
            programmes[1].start,
            Utc.with_ymd_and_hms(2026, 4, 17, 20, 30, 0).unwrap()
        );
        assert_eq!(
            programmes[1].stop,
            Utc.with_ymd_and_hms(2026, 4, 17, 21, 0, 0).unwrap()
        );
    }

    #[test]
    fn content_rides_through_both_block_behaviors_unchanged() {
        let content = XmltvProgrammeContent {
            kind: XmltvItemKind::Episode,
            sub_title: Some("Pilot".to_owned()),
            description: Some("First episode".to_owned()),
            year: Some(2026),
            genres: vec!["Drama".to_owned()],
            content_rating: Some("us:TV-14".to_owned()),
            icon_artwork: Some(ArtworkReference {
                id: 1,
                kind: ersatzrs_domain::ArtworkKind::Thumbnail,
                path: "path/to/art.jpg".to_owned(),
            }),
            icon_size_hint: IconSizeHint::Thumbnail,
            thumbnail_artwork: Some(ArtworkReference {
                id: 2,
                kind: ersatzrs_domain::ArtworkKind::Thumbnail,
                path: "path/to/still.jpg".to_owned(),
            }),
            thumbnail_size_hint: IconSizeHint::Thumbnail,
            season_number: Some(1),
            episode_number: Some(1),
            ..XmltvProgrammeContent::default()
        };
        let source_item = XmltvGuideItem {
            content: content.clone(),
            ..item("1", 20, 0, 20, 30, "Episode Title")
        };

        let split = project_programmes(
            std::slice::from_ref(&source_item),
            XmltvBlockBehavior::SplitTimeEvenly,
        );
        let actual = project_programmes(
            std::slice::from_ref(&source_item),
            XmltvBlockBehavior::UseActualTimes,
        );

        assert_eq!(split[0].content, content);
        assert_eq!(actual[0].content, content);
    }

    #[test]
    fn use_actual_times_keeps_item_times() {
        let items = vec![item("1", 20, 5, 20, 25, "Actual")];

        let programmes = project_programmes(&items, XmltvBlockBehavior::UseActualTimes);

        assert_eq!(
            programmes[0].start,
            Utc.with_ymd_and_hms(2026, 4, 17, 20, 5, 0).unwrap()
        );
        assert_eq!(
            programmes[0].stop,
            Utc.with_ymd_and_hms(2026, 4, 17, 20, 25, 0).unwrap()
        );
    }

    fn item(
        channel_number: &str,
        start_hour: u32,
        start_minute: u32,
        finish_hour: u32,
        finish_minute: u32,
        title: &str,
    ) -> XmltvGuideItem {
        XmltvGuideItem {
            channel_number: channel_number.to_owned(),
            start: Utc
                .with_ymd_and_hms(2026, 4, 17, start_hour, start_minute, 0)
                .unwrap(),
            finish: Utc
                .with_ymd_and_hms(2026, 4, 17, finish_hour, finish_minute, 0)
                .unwrap(),
            guide_start: Some(Utc.with_ymd_and_hms(2026, 4, 17, 20, 0, 0).unwrap()),
            guide_finish: Some(Utc.with_ymd_and_hms(2026, 4, 17, 21, 0, 0).unwrap()),
            guide_group: 10,
            filler_kind: FillerKind::None,
            custom_title: None,
            title: title.to_owned(),
            content: XmltvProgrammeContent::default(),
        }
    }
}
