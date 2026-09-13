use super::*;
use crate::addon_media_list_saved::model::{
    tests::fixture, SavedPlaybackTimeline, SavedReplacement,
};

#[test]
fn preserves_long_generated_ids_and_incomplete_draft_content() {
    let mut document = fixture();
    let item = &mut document.draft.items[0];
    item.content.provider_id = format!("{}:chapter:123", "p".repeat(384));
    item.content.duration_seconds = Some(0);
    item.content.source_url = Some("provider:opaque-source".into());
    item.content.provider_chapter_input = "unparsed provider chapters".into();
    item.content.provider_liveness = ersatzrs_domain::RemoteStreamLiveness::Unknown;
    item.replacement = Some(SavedReplacement { file: None });
    let original = document.clone();
    assert!(validate_items(&document).is_ok());
    assert_eq!(document, original);
    document.draft.items[0].content.duration_seconds = None;
    document.draft.items[0].content.provider_liveness = ersatzrs_domain::RemoteStreamLiveness::Live;
    assert!(validate_items(&document).is_ok());
}

#[test]
fn rejects_duplicate_and_blank_ids_within_each_independent_collection() {
    let document = fixture();
    let mut duplicate = document.clone();
    duplicate.draft.items.push(duplicate.draft.items[0].clone());
    assert!(validate_items(&duplicate).is_err());
    let mut duplicate = document.clone();
    let publication = duplicate.published.as_mut().unwrap();
    publication.items.push(publication.items[0].clone());
    assert!(validate_items(&duplicate).is_err());
    let mut blank = document.clone();
    blank.draft.items[0].content.provider_id = "  ".into();
    assert!(validate_items(&blank).is_err());
    let mut independent = document;
    independent.published.as_mut().unwrap().items[0]
        .content
        .provider_id = independent.draft.items[0].content.provider_id.clone();
    assert!(validate_items(&independent).is_ok());
}

#[test]
fn validates_fragment_shape_and_only_draft_parent_membership() {
    let mut document = fixture();
    let mut fragment = document.draft.items[0].clone();
    fragment.content.provider_id = "fragment".into();
    fragment.content.parent_provider_id = Some("unpublished".into());
    fragment.content.fragment_start_seconds = Some(2);
    fragment.content.fragment_end_seconds = Some(3);
    document.draft.items.push(fragment);
    assert!(validate_items(&document).is_ok());
    for (parent, start, end) in [
        (None, Some(2), Some(3)),
        (Some("unpublished"), None, None),
        (Some("fragment"), Some(2), Some(3)),
        (Some(" "), Some(2), Some(3)),
        (Some("missing"), Some(2), Some(3)),
        (Some("unpublished"), Some(2), Some(2)),
    ] {
        let mut invalid = document.clone();
        let content = &mut invalid.draft.items[1].content;
        content.parent_provider_id = parent.map(str::to_owned);
        content.fragment_start_seconds = start;
        content.fragment_end_seconds = end;
        assert!(validate_items(&invalid).is_err());
    }
    let published = &mut document.published.as_mut().unwrap().items[0];
    published.content.parent_provider_id = Some("excluded_old_parent".into());
    published.content.fragment_start_seconds = Some(2);
    assert!(validate_items(&document).is_ok());
    document.draft.items[0].content.duration_seconds = Some(u64::MAX);
    assert!(validate_items(&document).is_err());
}

#[test]
fn validates_probed_millisecond_precision_and_saved_source_window() {
    let mut document = fixture();
    let item = &mut document.published.as_mut().unwrap().items[0];
    item.content.duration_seconds = Some(12);
    item.subtitles.clear();
    item.playback = SavedPlayback::Stream {
        timeline: Some(SavedPlaybackTimeline {
            source_duration_milliseconds: 12_500,
            source_window_milliseconds: 0..12_500,
            excluded_intervals_milliseconds: std::iter::once(2_000..3_000).collect(),
        }),
    };
    assert!(validate_items(&document).is_ok());
    let item = &mut document.published.as_mut().unwrap().items[0];
    item.content.parent_provider_id = Some("old_parent".into());
    item.content.fragment_start_seconds = Some(1);
    assert!(validate_items(&document).is_err());
    let item = &mut document.published.as_mut().unwrap().items[0];
    let SavedPlayback::Stream {
        timeline: Some(timeline),
    } = &mut item.playback
    else {
        panic!("stream fixture");
    };
    timeline.source_window_milliseconds = 1_000..12_500;
    assert!(validate_items(&document).is_ok());
    let item = &mut document.published.as_mut().unwrap().items[0];
    let SavedPlayback::Stream {
        timeline: Some(timeline),
    } = &mut item.playback
    else {
        panic!("stream fixture");
    };
    timeline.excluded_intervals_milliseconds = std::iter::once(1_000..12_500).collect();
    assert!(validate_items(&document).is_err());
}

#[test]
fn validates_prepared_duration_and_preserves_unresolved_replacement() {
    for duration in [0, u64::MAX] {
        let mut document = fixture();
        let SavedPlayback::DownloadAhead {
            duration_milliseconds,
            ..
        } = &mut document.published.as_mut().unwrap().items[0].playback
        else {
            panic!("prepared fixture");
        };
        *duration_milliseconds = duration;
        assert!(validate_items(&document).is_err());
    }
    let mut document = fixture();
    let item = &mut document.published.as_mut().unwrap().items[0];
    item.playback = SavedPlayback::Replacement {
        replacement: SavedReplacement { file: None },
    };
    let mut source = item.subtitles[0].clone();
    source.language = "en".into();
    source.timeline = MediaListSubtitleTimeline::Source;
    item.subtitles.push(source);
    assert!(validate_items(&document).is_ok());
}

#[test]
fn validates_subtitle_languages_and_mode_specific_clocks() {
    let document = fixture();
    for language in ["*", "und", "", "en--US"] {
        let mut invalid = document.clone();
        invalid.published.as_mut().unwrap().items[0].subtitles[0].language = language.into();
        assert!(validate_items(&invalid).is_err());
    }
    let mut duplicate = document.clone();
    let item = &mut duplicate.published.as_mut().unwrap().items[0];
    let mut subtitle = item.subtitles[0].clone();
    subtitle.language = "nl-NL".into();
    item.subtitles.push(subtitle);
    assert!(validate_items(&duplicate).is_err());
    let mut too_many = document.clone();
    let item = &mut too_many.published.as_mut().unwrap().items[0];
    for language in ["en", "fr"] {
        let mut subtitle = item.subtitles[0].clone();
        subtitle.language = language.into();
        item.subtitles.push(subtitle);
    }
    assert!(validate_items(&too_many).is_err());
    let mut wrong_clock = document;
    wrong_clock.published.as_mut().unwrap().items[0].subtitles[0].timeline =
        MediaListSubtitleTimeline::Source;
    assert!(validate_items(&wrong_clock).is_err());
    wrong_clock.published.as_mut().unwrap().items[0].playback =
        SavedPlayback::Stream { timeline: None };
    assert!(validate_items(&wrong_clock).is_ok());
    wrong_clock.published.as_mut().unwrap().items[0].subtitles[0].timeline =
        MediaListSubtitleTimeline::Prepared;
    assert!(validate_items(&wrong_clock).is_err());
}
