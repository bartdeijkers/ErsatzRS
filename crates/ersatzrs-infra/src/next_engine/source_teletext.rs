//! Private finite-source page intent and demand-time text preparation (ADR 0121).

use std::{
    future::Future,
    path::{Path, PathBuf},
    sync::Arc,
};

use chrono::{DateTime, Utc};
use ersatzrs_core::{
    locking::{EntityLocker, TeletextWorkGuard, TeletextWorkHold},
    repositories::{ConfigRepository as _, RepositoryError, Result},
};
use ersatzrs_domain::{Channel, ChannelStreamSelectorMode, ChannelSubtitleMode};
use tokio::time::Instant;

use crate::{
    db::DbPool,
    ffprobe::{
        teletext_decode::{PreparedLocalTeletextSource, SourceTeletextTextPreparation},
        FfprobeExtractor, LocalProbeCleanupScope,
    },
    playback::{source_teletext::select_pending_next_source_subtitle, PlaybackSubtitle},
    repositories::SqliteConfigRepository,
    teletext_source::{
        discover_caption_requests, SourceTeletextCaptionCandidate,
        SourceTeletextCaptionDescriptorRequest,
    },
};

use super::{
    burn_cache::{BurnCache, CacheError},
    protocol::Source,
    schedule::ScheduledMedia,
};

pub(super) const CLOCK_POLICY: &str = "complete-source-copyts-start-at-zero-srt-v1";

/// Database intent only. This is deliberately neither serialized nor a ready Track.
pub(crate) struct PendingText {
    descriptor: SourceTeletextCaptionDescriptorRequest,
    pub(super) original_path: String,
}

impl PendingText {
    /// Metadata for private preference ranking, not observed-page authority.
    pub(super) fn candidate(&self) -> &SourceTeletextCaptionCandidate {
        &self.descriptor.candidate
    }

    pub(super) fn identity(&self) -> serde_json::Value {
        let candidate = self.candidate();
        serde_json::json!({"clock_policy":CLOCK_POLICY,"binding":candidate.request.binding,
            "stream":candidate.request.stream_index,"pid":candidate.request.pid,
            "program":candidate.request.program_number,"page":candidate.request.page.get(),
            "language":candidate.language.as_str(),"role":candidate.descriptor_type,
            "psi":candidate.psi_fingerprint,"descriptors":candidate.descriptor_occurrences.iter().map(|entry|
                (entry.descriptor_tag,entry.language.as_str(),entry.descriptor_type,entry.wire_magazine,entry.wire_page)).collect::<Vec<_>>()})
    }
}

pub(super) struct Selection {
    pub(super) subtitle: PlaybackSubtitle,
    pub(super) text: PendingText,
}

pub(super) async fn pending(
    pool: &DbPool,
    channel: &Channel,
    media: &ScheduledMedia,
    source: &Source,
) -> Result<Option<Selection>> {
    let item = &media.item;
    let mode = item.subtitle_mode.unwrap_or(channel.subtitle_mode);
    if mode == ChannelSubtitleMode::None
        || channel.stream_selector_mode == ChannelStreamSelectorMode::Custom
        || item.pinned_stream_selection.is_some()
        || !matches!(source, Source::Local { .. })
        || media.remote_stream_id.is_some()
    {
        return Ok(None);
    }
    let descriptors = discover_caption_requests(
        pool,
        item.media_item_id,
        media.media_version_id,
        media.media_file_id,
        &media.path,
    )
    .await?;
    // The prefix need not contain the preferred page yet. Rank only private
    // intents here; complete held-source header/text validation must succeed
    // before resolve_concrete can install and publish a prepared track.
    let candidates = descriptors
        .into_iter()
        .map(|request| request.candidate)
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return Ok(None);
    }
    let preferences = crate::ffmpeg_config::playback_language_preferences(pool).await?;
    let languages = preferences.subtitle_languages(
        item.preferred_subtitle_language_code.as_deref(),
        channel.preferred_subtitle_language_code.as_deref(),
    );
    Ok(select_pending_next_source_subtitle(
        pool,
        item.media_item_id,
        media.media_version_id,
        &languages,
        mode,
        &candidates,
    )
    .await?
    .map(|selected| Selection {
        subtitle: selected.subtitle,
        text: PendingText {
            descriptor: SourceTeletextCaptionDescriptorRequest {
                candidate: selected.candidate,
            },
            original_path: media.path.clone(),
        },
    }))
}

#[derive(Clone, Copy)]
pub(super) struct Preparation<'a> {
    pub(super) pending: &'a PendingText,
    pub(super) source: &'a Source,
    pub(super) key: &'a str,
    pub(super) expires: DateTime<Utc>,
    pub(super) ffmpeg: &'a Path,
    pub(super) ffprobe: &'a Path,
    pub(super) deadline: Instant,
}

/// Exact finite page preparation, without treating refusal as an ordinary track.
pub(super) enum PreparedText {
    Ready(PathBuf),
    Absent(AbsentPage),
}

/// Complete-source absence tied to the held original and admitted source work.
/// The caller must revalidate its selection policy and this association before
/// installing an independently prepared ordinary replacement.
pub(super) struct AbsentPage {
    source: Arc<PreparedLocalTeletextSource>,
    hold: TeletextWorkHold,
}

impl AbsentPage {
    pub(super) fn page(&self) -> u16 {
        self.source.request().page.get()
    }

    pub(super) async fn check_source(&self, deadline: Instant) -> Result<()> {
        let deadline = deadline.min(Instant::now() + std::time::Duration::from_secs(15));
        let scope = Scope(LocalProbeCleanupScope::new());
        let checked = scope
            .0
            .scope(self.source.check_source(deadline, self.hold.clone()))
            .await;
        scope.0.close();
        scope.0.wait_cleanup().await.map_err(|_| unavailable())?;
        if Instant::now() >= deadline {
            return Err(unavailable());
        }
        checked.map_err(|_| unavailable())
    }
}

pub(super) async fn prepare<F, Fut>(
    pool: &DbPool,
    locker: &EntityLocker,
    cache: &BurnCache,
    request: Preparation<'_>,
    validate: F,
) -> Result<PreparedText>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = bool>,
{
    let request = Preparation {
        deadline: request
            .deadline
            .min(Instant::now() + std::time::Duration::from_secs(15)),
        ..request
    };
    let Source::Local { path, .. } = request.source else {
        return Err(unavailable());
    };
    let configured = tokio::time::timeout_at(
        request.deadline,
        SqliteConfigRepository(pool.clone()).get("teletext.work.max_concurrent"),
    )
    .await
    .map_err(|_| unavailable())??;
    let maximum = configured.map_or(Ok(1), |value| {
        value
            .parse::<std::num::NonZeroUsize>()
            .map(std::num::NonZeroUsize::get)
            .map_err(|_| unavailable())
    })?;
    let work = TeletextWorkGuard::try_acquire(locker, maximum).ok_or_else(unavailable)?;
    let scope = Scope(LocalProbeCleanupScope::new());
    let extractor = FfprobeExtractor::new(request.ffprobe.to_str().ok_or_else(unavailable)?);
    let operation = async {
        let source = Arc::new(
            extractor
                .prepare_local_teletext_source(
                    request.pending.candidate().request.clone(),
                    request.pending.original_path.clone().into(),
                    path.into(),
                    request.deadline,
                    work.hold_work(),
                )
                .await
                .map_err(|_| unavailable())?,
        );
        prepare_cached_text(
            source,
            extractor,
            cache,
            request,
            work.hold_work(),
            validate,
        )
        .await
    };
    let result = scope.0.scope(operation).await;
    scope.0.close();
    scope.0.wait_cleanup().await.map_err(|_| unavailable())?;
    if matches!(&result, Ok(PreparedText::Absent(_))) && Instant::now() >= request.deadline {
        return Err(unavailable());
    }
    result
}

async fn prepare_cached_text<F, Fut>(
    source: Arc<PreparedLocalTeletextSource>,
    extractor: FfprobeExtractor,
    cache: &BurnCache,
    request: Preparation<'_>,
    hold: TeletextWorkHold,
    validate: F,
) -> Result<PreparedText>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = bool>,
{
    let checked_source = source.clone();
    let checked_hold = hold.clone();
    let build_hold = hold.clone();
    let mut validate = Some(validate);
    let mut absent_source = None;
    let missing = &mut absent_source;
    let prepared = cache
        .prepare_artifact(
            request.key,
            "srt",
            request.expires,
            request.deadline,
            || {
                let validate = validate.take();
                async move {
                    let Some(validate) = validate else {
                        return false;
                    };
                    checked_source
                        .check_source(request.deadline, checked_hold)
                        .await
                        .is_ok()
                        && validate().await
                }
            },
            |_path, file| async move {
                let prepared = extractor
                    .prepare_local_teletext_text(
                        source,
                        request.ffmpeg,
                        request.deadline,
                        build_hold.clone(),
                    )
                    .await
                    .map_err(|_| CacheError::Stale)?;
                let artifact = match prepared {
                    SourceTeletextTextPreparation::Ready(artifact) => artifact,
                    SourceTeletextTextPreparation::Absent {
                        psi_fingerprint,
                        source,
                    } => {
                        if psi_fingerprint != request.pending.candidate().psi_fingerprint {
                            return Err(CacheError::Stale);
                        }
                        // The builder returns before cache validation and
                        // promotion. Keep the actual checked source, without
                        // creating a negative artifact or consuming SQL
                        // validation on an unrelated failure.
                        *missing = Some(source);
                        return Err(CacheError::Stale);
                    }
                };
                if artifact.psi_fingerprint != request.pending.candidate().psi_fingerprint {
                    return Err(CacheError::Stale);
                }
                artifact
                    .write(file, request.deadline, build_hold)
                    .await
                    .map_err(|_| CacheError::Storage)
            },
        )
        .await;
    match (prepared, absent_source) {
        (Ok(path), None) => Ok(PreparedText::Ready(path)),
        (Err(CacheError::Stale), Some(source)) => {
            source
                .check_source(request.deadline, hold.clone())
                .await
                .map_err(|_| unavailable())?;
            let validate = validate.take().ok_or_else(unavailable)?;
            let valid = tokio::time::timeout_at(request.deadline, validate())
                .await
                .map_err(|_| unavailable())?;
            if !valid {
                return Err(unavailable());
            }
            source
                .check_source(request.deadline, hold.clone())
                .await
                .map_err(|_| unavailable())?;
            Ok(PreparedText::Absent(AbsentPage { source, hold }))
        }
        _ => Err(unavailable()),
    }
}

struct Scope(LocalProbeCleanupScope);
impl Drop for Scope {
    fn drop(&mut self) {
        self.0.close();
    }
}

fn unavailable() -> RepositoryError {
    RepositoryError::unavailable(
        "Next source teletext",
        "selected finite source page is unavailable",
    )
}
