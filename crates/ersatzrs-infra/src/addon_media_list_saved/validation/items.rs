//! Stored item and publication timing invariants, independent of provider enumeration limits.

use std::collections::BTreeSet;

use ersatzrs_addon_contract::{
    subtitle_logical_language, validate_requested_subtitle_languages, MediaListSubtitleTimeline,
};

use crate::addon_media_list_item_jobs::ItemOperationTiming;
use crate::addon_media_list_saved::model::{
    SavedItemContent, SavedListDocument, SavedPlayback, SavedPublishedItem,
};

use super::SavedContentError;

pub(super) fn validate_items(document: &SavedListDocument) -> Result<(), SavedContentError> {
    let mut draft_ids = BTreeSet::new();
    for item in &document.draft.items {
        validate_content(&item.content)?;
        if !draft_ids.insert(item.content.provider_id.as_str()) {
            return Err(SavedContentError::Items);
        }
    }
    for item in &document.draft.items {
        if item
            .content
            .parent_provider_id
            .as_deref()
            .is_some_and(|parent| !draft_ids.contains(parent))
        {
            return Err(SavedContentError::Items);
        }
    }
    if let Some(publication) = &document.published {
        let mut published_ids = BTreeSet::new();
        for item in &publication.items {
            validate_content(&item.content)?;
            if !published_ids.insert(item.content.provider_id.as_str()) {
                return Err(SavedContentError::Items);
            }
            validate_playback(item)?;
        }
    }
    Ok(())
}

fn validate_content(content: &SavedItemContent) -> Result<(), SavedContentError> {
    if content.provider_id.trim().is_empty()
        || [
            content.duration_seconds,
            content.fragment_start_seconds,
            content.fragment_end_seconds,
        ]
        .into_iter()
        .flatten()
        .any(|seconds| i64::try_from(seconds).is_err())
    {
        return Err(SavedContentError::Items);
    }
    match (
        content.parent_provider_id.as_deref(),
        content.fragment_start_seconds,
        content.fragment_end_seconds,
    ) {
        (None, None, None) => Ok(()),
        (Some(parent), Some(start), end)
            if !parent.trim().is_empty()
                && parent != content.provider_id
                && end.is_none_or(|end| end > start) =>
        {
            Ok(())
        }
        _ => Err(SavedContentError::Items),
    }
}

fn validate_playback(item: &SavedPublishedItem) -> Result<(), SavedContentError> {
    match &item.playback {
        SavedPlayback::Stream {
            timeline: Some(timeline),
        } => {
            // This synthetic context exists only to invoke geometry validation;
            // it is neither persisted nor used as receiving execution identity.
            let plan = timeline
                .bind("0".repeat(64))
                .map_err(|_| SavedContentError::Playback)?;
            let timing = ItemOperationTiming {
                item_duration_seconds: item.content.duration_seconds,
                fragment_start_seconds: item.content.fragment_start_seconds,
                fragment_end_seconds: item.content.fragment_end_seconds,
            };
            let window = timing
                .source_window_milliseconds(timeline.source_duration_milliseconds)
                .map_err(|_| SavedContentError::Playback)?;
            if plan.source_window_milliseconds() != window {
                return Err(SavedContentError::Playback);
            }
        }
        SavedPlayback::DownloadAhead {
            duration_milliseconds,
            ..
        } if *duration_milliseconds == 0 || i64::try_from(*duration_milliseconds).is_err() => {
            return Err(SavedContentError::Playback);
        }
        _ => {}
    }
    let languages = item
        .subtitles
        .iter()
        .map(|subtitle| subtitle_logical_language(&subtitle.language))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| SavedContentError::Playback)?;
    validate_requested_subtitle_languages(&languages, true)
        .map_err(|_| SavedContentError::Playback)?;
    for subtitle in &item.subtitles {
        let invalid_clock = match item.playback {
            SavedPlayback::Stream { .. } => subtitle.timeline != MediaListSubtitleTimeline::Source,
            SavedPlayback::DownloadAhead { .. } => {
                subtitle.timeline != MediaListSubtitleTimeline::Prepared
            }
            SavedPlayback::Replacement { .. } => false,
        };
        if invalid_clock {
            return Err(SavedContentError::Playback);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
