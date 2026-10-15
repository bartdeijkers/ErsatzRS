//! Native subtitle ranking with explicitly decoded, source-bound teletext pages.
//!
//! This bridge neither prepares pages nor enables Direct, copy or Next output.
//! The caller owns current database/file validation, actual playback seek-window
//! qualification, generated-credit precedence and the consuming worker lifetime.

use std::sync::Arc;

#[path = "playback_source_teletext_runtime.rs"]
/// Owned source-caption preparation shared by native transport consumers.
pub mod runtime;

use ersatzrs_core::{
    locking::TeletextWorkHold,
    repositories::{RepositoryError, Result},
};
use ersatzrs_domain::{ChannelSubtitleMode, StreamingMode};

use super::{
    is_media_server_subtitle_item, load_subtitles_for_media_version, provider_subtitle_supported,
    select_subtitle_preference_index, subtitle_kind, transient::subtitle_is_playable,
    use_embedded_subtitles, PlaybackSubtitle, SubtitleSelectionEngine,
};
use crate::{
    db::DbPool,
    ffprobe::teletext_decode::{
        DecodedLocalTeletextPage, SourceTeletextDecodeWindowEvidence,
        SourceTeletextPageDecodeRequest,
    },
    teletext_source::SourceTeletextCaptionCandidate,
};

// Acquisition policy matching the bounded durable caption-candidate loader;
// this limits this call's choices, not a DVB service's page inventory.
const MAX_QUALIFIED_SOURCE_CAPTIONS: usize = 1_024;

/// Static qualification failures never contain source paths or revisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SourceCaptionQualificationError {
    #[error("source caption candidate and decoded request differ")]
    Identity,
    #[error("source caption candidate and decoded PSI differ")]
    Psi,
    #[error("source caption candidate has invalid descriptor provenance")]
    Descriptor,
}

/// One candidate and its exact bounded decode, retaining already admitted work.
/// This is not full page readiness or proof of a later source seek/open.
#[derive(Clone)]
pub struct QualifiedSourceCaption {
    candidate: SourceTeletextCaptionCandidate,
    decoded: Arc<DecodedLocalTeletextPage>,
    hold: TeletextWorkHold,
}

impl std::fmt::Debug for QualifiedSourceCaption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QualifiedSourceCaption")
            .field("request", &self.candidate.request)
            .field("language", &self.candidate.language)
            .field("descriptor_type", &self.candidate.descriptor_type)
            .finish_non_exhaustive()
    }
}

impl QualifiedSourceCaption {
    /// Bind a candidate to the exact decoded request and stable PSI generation.
    ///
    /// # Errors
    /// Rejects mismatched request identities, PSI or wire descriptor provenance.
    pub fn new(
        candidate: SourceTeletextCaptionCandidate,
        decoded: Arc<DecodedLocalTeletextPage>,
        hold: TeletextWorkHold,
    ) -> std::result::Result<Self, SourceCaptionQualificationError> {
        qualify(&candidate, decoded.request(), decoded.window())?;
        Ok(Self {
            candidate,
            decoded,
            hold,
        })
    }

    #[must_use]
    pub fn candidate(&self) -> &SourceTeletextCaptionCandidate {
        &self.candidate
    }

    #[must_use]
    pub fn receipt(&self) -> &Arc<DecodedLocalTeletextPage> {
        &self.decoded
    }

    /// Retain the existing admission in the actual consuming worker, without
    /// acquiring another slot. Unconfirmed worker cleanup must retain it.
    pub fn hold_work(&self) -> TeletextWorkHold {
        self.hold.clone()
    }
}

/// Selected real subtitle metadata and its optional exact source-page owner.
pub struct NativePlaybackSubtitle {
    subtitle: PlaybackSubtitle,
    source: Option<QualifiedSourceCaption>,
}

impl NativePlaybackSubtitle {
    #[must_use]
    pub fn subtitle(&self) -> &PlaybackSubtitle {
        &self.subtitle
    }

    #[must_use]
    pub fn source(&self) -> Option<&QualifiedSourceCaption> {
        self.source.as_ref()
    }

    #[must_use]
    pub fn into_parts(self) -> (PlaybackSubtitle, Option<QualifiedSourceCaption>) {
        (self.subtitle, self.source)
    }
}

/// Rank ordinary captions and uniquely matching qualified source pages together.
///
/// Page language and affirmative SDH role augment the real embedded metadata; ID,
/// title, codec, forced/default and extraction facts retain their existing values.
/// Equally ranked conflicting pages/programs/roles on one stream/language are
/// excluded, preserving ordinary caption fallback. Only the winning rich value
/// and its source admission are cloned. Off returns before database reads.
///
/// # Errors
/// Returns existing exact-version metadata, provider and configuration errors.
pub async fn select_native_playback_subtitle_for_version(
    pool: &DbPool,
    media_item_id: i64,
    media_version_id: i64,
    preferred_languages: &[String],
    subtitle_mode: ChannelSubtitleMode,
    streaming_mode: StreamingMode,
    qualified: &[QualifiedSourceCaption],
) -> Result<Option<NativePlaybackSubtitle>> {
    if subtitle_mode == ChannelSubtitleMode::None {
        return Ok(None);
    }
    check_qualified_count(qualified.len())?;
    let mut rows = load_subtitles_for_media_version(pool, media_item_id, media_version_id).await?;
    if !use_embedded_subtitles(pool).await? {
        rows.retain(|row| row.subtitle_kind != subtitle_kind::EMBEDDED);
    }
    let media_server = is_media_server_subtitle_item(pool, media_item_id).await?;
    let sources = qualified
        .iter()
        .map(QualifiedSourceCaption::candidate)
        .collect::<Vec<_>>();
    let choices = metadata_choices(
        &rows,
        media_version_id,
        streaming_mode,
        media_server,
        &sources,
    );
    let selected = select_subtitle_preference_index(
        choices
            .iter()
            .enumerate()
            .map(|(index, choice)| (index, choice.row, choice.language)),
        preferred_languages,
        subtitle_mode,
    );
    Ok(selected.map(|index| {
        let choice = &choices[index];
        let source = choice.source.map(|index| &qualified[index]);
        let subtitle = selected_metadata(choice.row, source.map(QualifiedSourceCaption::candidate));
        NativePlaybackSubtitle {
            subtitle,
            source: source.cloned(),
        }
    }))
}

fn check_qualified_count(count: usize) -> Result<()> {
    if count > MAX_QUALIFIED_SOURCE_CAPTIONS {
        return Err(RepositoryError::unavailable(
            "source teletext captions",
            "too many qualified source caption choices",
        ));
    }
    Ok(())
}

fn selected_metadata(
    row: &PlaybackSubtitle,
    source: Option<&SourceTeletextCaptionCandidate>,
) -> PlaybackSubtitle {
    let mut selected = row.clone();
    if let Some(source) = source {
        selected.language = Some(source.language.as_str().to_owned());
        selected.sdh |= source.is_sdh();
    }
    selected
}

fn qualify(
    candidate: &SourceTeletextCaptionCandidate,
    request: &SourceTeletextPageDecodeRequest,
    window: &SourceTeletextDecodeWindowEvidence,
) -> std::result::Result<(), SourceCaptionQualificationError> {
    if candidate.request != *request {
        return Err(SourceCaptionQualificationError::Identity);
    }
    if candidate.psi_fingerprint != window.psi_fingerprint
        || request.program_number != window.program_number
    {
        return Err(SourceCaptionQualificationError::Psi);
    }
    if !matches!(candidate.descriptor_type, 2 | 5)
        || candidate.descriptor_occurrences.is_empty()
        || candidate.descriptor_occurrences.iter().any(|entry| {
            !matches!(entry.descriptor_tag, 0x56 | 0x46)
                || entry.language != candidate.language
                || entry.descriptor_type != candidate.descriptor_type
                || entry.display_page() != Some(request.page)
        })
    {
        return Err(SourceCaptionQualificationError::Descriptor);
    }
    Ok(())
}

struct MetadataChoice<'a> {
    row: &'a PlaybackSubtitle,
    source: Option<usize>,
    language: Option<&'a str>,
}

fn metadata_choices<'a>(
    rows: &'a [PlaybackSubtitle],
    version: i64,
    mode: StreamingMode,
    media_server: bool,
    sources: &[&'a SourceTeletextCaptionCandidate],
) -> Vec<MetadataChoice<'a>> {
    let mut choices = Vec::new();
    for row in rows {
        let source_eligible = mode != StreamingMode::HttpLiveStreamingDirect
            && is_source_row(row)
            && provider_subtitle_supported(row, media_server)
            && rows
                .iter()
                .filter(|other| is_source_row(other) && other.stream_index == row.stream_index)
                .count()
                == 1;
        let mut replaced = false;
        if source_eligible {
            for (index, source) in sources.iter().enumerate() {
                if source.request.binding.media_version_id != version
                    || source.request.stream_index != row.stream_index
                    || !matches!(source.descriptor_type, 2 | 5)
                    || conflicting_source(source, sources, version)
                    || sources[..index]
                        .iter()
                        .any(|prior| same_source_choice(source, prior))
                {
                    continue;
                }
                choices.push(MetadataChoice {
                    row,
                    source: Some(index),
                    language: Some(source.language.as_str()),
                });
                replaced = true;
            }
        }
        if !replaced
            && (mode == StreamingMode::HttpLiveStreamingDirect || !is_source_row(row))
            && subtitle_is_playable(row, mode, SubtitleSelectionEngine::Native, media_server)
        {
            choices.push(MetadataChoice {
                row,
                source: None,
                language: row.language.as_deref(),
            });
        }
    }
    choices
}

fn is_source_row(row: &PlaybackSubtitle) -> bool {
    row.subtitle_kind == subtitle_kind::EMBEDDED && row.codec.as_deref() == Some("dvb_teletext")
}

fn same_source_choice(
    left: &SourceTeletextCaptionCandidate,
    right: &SourceTeletextCaptionCandidate,
) -> bool {
    left.request == right.request
        && left.descriptor_type == right.descriptor_type
        && left.psi_fingerprint == right.psi_fingerprint
        && super::language_matches_preference(
            Some(right.language.as_str()),
            &ersatzrs_core::language_codes::all_language_codes([left.language.as_str()]),
        )
}

fn conflicting_source(
    source: &SourceTeletextCaptionCandidate,
    sources: &[&SourceTeletextCaptionCandidate],
    version: i64,
) -> bool {
    source_caption_has_competing_choice(source, sources.iter().copied(), version)
}

/// Whether known source facts leave another page, service or role equally
/// eligible for this stream and language. Call before bounded decoding: a page
/// absent from that decode window is not proven absent from the source.
#[must_use]
pub fn source_caption_has_competing_choice<'a>(
    source: &SourceTeletextCaptionCandidate,
    mut sources: impl Iterator<Item = &'a SourceTeletextCaptionCandidate>,
    version: i64,
) -> bool {
    let codes = ersatzrs_core::language_codes::all_language_codes([source.language.as_str()]);
    sources.any(|other| {
        other.request.binding.media_version_id == version
            && other.request.stream_index == source.request.stream_index
            && matches!(other.descriptor_type, 2 | 5)
            && super::language_matches_preference(Some(other.language.as_str()), &codes)
            && !same_source_choice(source, other)
    })
}

#[cfg(test)]
#[path = "playback_source_teletext_tests.rs"]
mod tests;
