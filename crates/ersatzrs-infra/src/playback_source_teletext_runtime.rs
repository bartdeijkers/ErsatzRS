//! Shared native preparation for ordinary source captions, independent of UI.
//!
//! Until source seek-window indexing exists, later reads qualify only when the
//! bounded initial transport was complete and unomitted. This is an explicit
//! coverage gap, not a permanent product limit or full source acceptance.

use std::{num::NonZeroUsize, path::PathBuf, sync::Arc, time::Duration};

use ersatzrs_core::{
    locking::{EntityLocker, TeletextWorkGuard},
    repositories::{ConfigRepository, RepositoryError, Result},
};
use ersatzrs_domain::{ChannelSubtitleMode, StreamingMode};
use tokio::time::Instant;

use super::{
    select_native_playback_subtitle_for_version, source_caption_has_competing_choice,
    QualifiedSourceCaption,
};
use crate::{
    db::DbPool,
    ffprobe::{
        teletext_decode::{DecodedLocalTeletextPage, SourceTeletextPageDecodeError},
        FfprobeExtractor, LocalProbeCleanupScope,
    },
    playback::PlaybackSubtitle,
    repositories::SqliteConfigRepository,
    teletext_source::{load_caption_candidates, SourceTeletextCaptionCandidate},
};

const MAX_STARTUP: Duration = Duration::from_secs(15);
const MAX_CONCURRENT_SETTING: &str = "teletext.work.max_concurrent";

#[path = "playback_source_teletext_interval.rs"]
mod interval;
pub use interval::{SourceTeletextEffectiveInterval, SourceTeletextPlaybackInterval};

/// Persisted source identity plus the actual resolved playback locator.
/// Transient/external inputs have no durable identity and remain inapplicable.
pub struct RuntimeSelectionContext<'a> {
    pub pool: &'a DbPool,
    pub locker: &'a EntityLocker,
    pub media_item_id: Option<i64>,
    pub media_version_id: Option<i64>,
    pub media_file_id: Option<i64>,
    pub original_path: &'a str,
    pub resolved_path: &'a str,
    pub languages: &'a [String],
    pub subtitle_mode: ChannelSubtitleMode,
    pub streaming_mode: StreamingMode,
    /// Actual finite media-relative request, not source-clock qualification.
    pub playback_interval: Option<SourceTeletextPlaybackInterval>,
    pub deadline: Instant,
}

/// An applicable source-caption path's final ordinary or exact-page selection.
pub struct Selection {
    pub subtitle: Option<PlaybackSubtitle>,
    pub source: Option<Arc<PlaybackSource>>,
}

/// Source state retained by the actual native playback worker, without holding
/// short preparation capacity throughout playback. Each later check reacquires
/// the existing process-wide admission and retains it in the real worker only.
pub struct PlaybackSource {
    receipt: Arc<DecodedLocalTeletextPage>,
    candidate: SourceTeletextCaptionCandidate,
    item: i64,
    original: String,
    pool: DbPool,
    locker: EntityLocker,
    deadline: Instant,
    playback_interval: Option<SourceTeletextPlaybackInterval>,
}

impl PlaybackSource {
    /// Exact requested media interval retained through consumer/retry ownership.
    /// This is caller intent, not proof that the receipt covers that source time.
    #[must_use]
    pub const fn playback_interval(&self) -> Option<SourceTeletextPlaybackInterval> {
        self.playback_interval
    }

    /// Share the held exact-source association with its actual consuming child.
    #[must_use]
    pub fn receipt(&self) -> Arc<DecodedLocalTeletextPage> {
        self.receipt.clone()
    }

    /// Construct native Burn for the previously decoded exact page.
    #[must_use]
    pub fn input(&self) -> ersatzrs_ffmpeg::InputFile {
        self.receipt.native_burn_input()
    }

    /// Match the generated final consumer command and its exact caption input.
    ///
    /// This pure fence grants no delayed-cue, timing or source readiness. The
    /// caller must still run the existing source recheck and retain this owner
    /// with actual playback admission through consuming-child cleanup.
    ///
    /// # Errors
    /// Returns a static unavailable error for missing/stale command plans or a
    /// caption path, instance, method, stream or selected-page mismatch.
    pub fn check_consumer_command(
        &self,
        command: &ersatzrs_ffmpeg::FfmpegCommand,
        plan: Option<&ersatzrs_ffmpeg::SourceTeletextInputPlan>,
    ) -> Result<()> {
        let plan = plan.ok_or_else(unavailable)?;
        if !plan.matches_command(command) {
            return Err(unavailable());
        }
        let actual = plan.caption_input();
        let expected = self.input();
        if actual.path != expected.path
            || actual.input_instance != expected.input_instance
            || actual.kind != expected.kind
            || actual.subtitle_method != expected.subtitle_method
            || actual.is_image_based_subtitle != expected.is_image_based_subtitle
            || actual.streams.first() != expected.streams.first()
        {
            return Err(unavailable());
        }
        Ok(())
    }

    /// Original startup budget, never restarted during initial preparation.
    #[must_use]
    pub const fn initial_deadline(&self) -> Instant {
        self.deadline
    }

    /// Recheck durable candidates and both filesystem locators before a child.
    /// This is an observed association, not an immutable-file lease.
    ///
    /// # Errors
    /// Returns a static unavailable error for stale metadata/source, expired
    /// deadline, occupied capacity or unconfirmed process/read cleanup.
    pub async fn recheck(&self, deadline: Instant) -> Result<()> {
        let deadline = deadline.min(Instant::now() + MAX_STARTUP);
        let request = self.receipt.request();
        let current = before(
            deadline,
            load_caption_candidates(
                &self.pool,
                self.item,
                request.binding.media_version_id,
                request.binding.media_file_id,
                &self.original,
            ),
        )
        .await?;
        if !current.contains(&self.candidate)
            || source_caption_has_competing_choice(
                &self.candidate,
                current.iter(),
                request.binding.media_version_id,
            )
        {
            return Err(unavailable());
        }
        let work = begin(&self.pool, &self.locker, deadline).await?;
        let scope = ClosingScope(LocalProbeCleanupScope::new());
        let checked = scope
            .0
            .scope(self.receipt.check_source(deadline, work.hold_work()))
            .await;
        scope.0.close();
        scope.0.wait_cleanup().await.map_err(|_| unavailable())?;
        checked.map_err(|_| unavailable())
    }
}

/// Prepare and rank exact pages together with existing ordinary captions.
///
/// `None` means inapplicable: Off, Direct, missing durable identity, embedded
/// captions disabled. Missing source candidates still use ordinary exact-version
/// ranking that excludes unqualified teletext. `Some` is final even
/// when no subtitle wins; callers must not reselect an unqualified source row.
/// Recognized preparation failures preserve ordinary-caption ranking and AV.
/// Preparation and source checks share one absolute deadline of at most fifteen
/// seconds. Ordinary fallback ranking retains the caller's original startup
/// budget. Cleanup has its existing fresh budget.
///
/// # Errors
/// Returns static timeout failures and existing ordinary metadata/configuration
/// errors; source preparation failures alone do not reject ordinary playback.
pub async fn select(context: RuntimeSelectionContext<'_>) -> Result<Option<Selection>> {
    if context.subtitle_mode == ChannelSubtitleMode::None
        || context.streaming_mode == StreamingMode::HttpLiveStreamingDirect
    {
        return Ok(None);
    }
    let (Some(item), Some(version), Some(file)) = (
        context.media_item_id,
        context.media_version_id,
        context.media_file_id,
    ) else {
        return Ok(None);
    };
    let deadline = context.deadline;
    if !before(
        deadline,
        crate::ffmpeg_config::use_embedded_subtitles(context.pool),
    )
    .await?
    {
        return Ok(None);
    }
    let candidates = before(
        deadline,
        load_caption_candidates(context.pool, item, version, file, context.original_path),
    )
    .await?;
    let work_deadline = deadline.min(Instant::now() + MAX_STARTUP);
    let qualified = if candidates.is_empty() {
        // Missing observations never authorize a page. Keep ordinary captions,
        // without falling back to the old whole-stream teletext extraction path.
        Vec::new()
    } else if let Ok(qualified) =
        prepare(&context, item, version, file, &candidates, work_deadline).await
    {
        qualified
    } else {
        tracing::warn!(
            media_item_id = item,
            "source teletext preparation unavailable; retaining ordinary caption preferences"
        );
        Vec::new()
    };
    let selected = before(
        deadline,
        select_native_playback_subtitle_for_version(
            context.pool,
            item,
            version,
            context.languages,
            context.subtitle_mode,
            context.streaming_mode,
            &qualified,
        ),
    )
    .await?;
    let Some(selected) = selected else {
        return Ok(Some(Selection {
            subtitle: None,
            source: None,
        }));
    };
    let (subtitle, source) = selected.into_parts();
    let source = source.map(|source| {
        Arc::new(PlaybackSource {
            receipt: source.receipt().clone(),
            candidate: source.candidate().clone(),
            item,
            original: context.original_path.to_owned(),
            pool: context.pool.clone(),
            locker: context.locker.clone(),
            deadline: work_deadline,
            playback_interval: context.playback_interval,
        })
    });
    Ok(Some(Selection {
        subtitle: Some(subtitle),
        source,
    }))
}

async fn prepare(
    context: &RuntimeSelectionContext<'_>,
    item: i64,
    version: i64,
    file: i64,
    candidates: &[SourceTeletextCaptionCandidate],
    deadline: Instant,
) -> Result<Vec<QualifiedSourceCaption>> {
    let work = begin(context.pool, context.locker, deadline).await?;
    let scope = ClosingScope(LocalProbeCleanupScope::new());
    let extractor = FfprobeExtractor::new(ersatzrs_ffmpeg::ffprobe_path());
    let operation = async {
        let mut qualified = Vec::new();
        for candidate in candidates {
            if source_caption_has_competing_choice(candidate, candidates.iter(), version) {
                continue;
            }
            let decoded = extractor
                .decode_local_teletext_page(
                    candidate.request.clone(),
                    PathBuf::from(context.original_path),
                    PathBuf::from(context.resolved_path),
                    deadline,
                    work.hold_work(),
                )
                .await;
            let decoded = match decoded {
                Ok(decoded) => decoded,
                Err(
                    SourceTeletextPageDecodeError::CleanupUnconfirmed
                    | SourceTeletextPageDecodeError::Cancelled
                    | SourceTeletextPageDecodeError::Deadline,
                ) => return Err(unavailable()),
                Err(_) => continue,
            };
            let window = decoded.window();
            if !window.covers_entire_source_bytes() {
                tracing::warn!(media_item_id = item,
                    "source teletext interval is not fully qualified; ordinary playback remains available");
                continue;
            }
            qualified.push(
                QualifiedSourceCaption::new(candidate.clone(), Arc::new(decoded), work.hold_work())
                    .map_err(|_| unavailable())?,
            );
        }
        Ok(qualified)
    };
    let result = scope.0.scope(operation).await;
    scope.0.close();
    scope.0.wait_cleanup().await.map_err(|_| unavailable())?;
    let qualified = result?;
    let current = before(
        deadline,
        load_caption_candidates(context.pool, item, version, file, context.original_path),
    )
    .await?;
    if current != candidates {
        return Err(unavailable());
    }
    Ok(qualified)
}

async fn begin(
    pool: &DbPool,
    locker: &EntityLocker,
    deadline: Instant,
) -> Result<TeletextWorkGuard> {
    if Instant::now() >= deadline {
        return Err(unavailable());
    }
    let configuration = SqliteConfigRepository(pool.clone());
    let configured = before(deadline, configuration.get(MAX_CONCURRENT_SETTING)).await?;
    let maximum = match configured {
        None => NonZeroUsize::MIN,
        Some(value) => value.parse::<NonZeroUsize>().map_err(|_| unavailable())?,
    };
    if Instant::now() >= deadline {
        return Err(unavailable());
    }
    TeletextWorkGuard::try_acquire(locker, maximum.get()).ok_or_else(unavailable)
}

struct ClosingScope(LocalProbeCleanupScope);

impl Drop for ClosingScope {
    fn drop(&mut self) {
        self.0.close();
    }
}

async fn before<T>(
    deadline: Instant,
    operation: impl std::future::Future<Output = Result<T>>,
) -> Result<T> {
    tokio::time::timeout_at(deadline, operation)
        .await
        .map_err(|_| unavailable())?
}

fn unavailable() -> RepositoryError {
    RepositoryError::unavailable(
        "source teletext captions",
        "source caption selection is unavailable",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    #[tokio::test]
    async fn off_direct_and_transient_paths_return_before_touching_a_closed_database() {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        pool.close().await;
        let locker = EntityLocker::new();
        for (subtitle_mode, streaming_mode, item) in [
            (
                ChannelSubtitleMode::None,
                StreamingMode::HttpLiveStreamingSegmenter,
                Some(1),
            ),
            (
                ChannelSubtitleMode::Any,
                StreamingMode::HttpLiveStreamingDirect,
                Some(1),
            ),
            (
                ChannelSubtitleMode::Any,
                StreamingMode::HttpLiveStreamingSegmenter,
                None,
            ),
        ] {
            assert!(select(RuntimeSelectionContext {
                pool: &pool,
                locker: &locker,
                media_item_id: item,
                media_version_id: Some(2),
                media_file_id: Some(3),
                original_path: "synthetic-source.ts",
                resolved_path: "synthetic-source.ts",
                languages: &[],
                subtitle_mode,
                streaming_mode,
                playback_interval: None,
                deadline: Instant::now(),
            })
            .await
            .unwrap()
            .is_none());
        }
        assert_eq!(locker.active_teletext_work(), 0);
    }

    #[tokio::test]
    async fn existing_configuration_and_locker_enforce_immediate_shared_admission() {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query(
            r#"CREATE TABLE "ConfigElement" ("Key" TEXT PRIMARY KEY, "Value" TEXT NOT NULL)"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        let config = SqliteConfigRepository(pool.clone());
        let locker = EntityLocker::new();
        let deadline = Instant::now() + MAX_STARTUP;
        let work = begin(&pool, &locker, deadline).await.unwrap();
        assert!(begin(&pool, &locker.clone(), deadline).await.is_err());
        config.set(MAX_CONCURRENT_SETTING, "2").await.unwrap();
        let second = begin(&pool, &locker, deadline).await.unwrap();
        assert_eq!(locker.active_teletext_work(), 2);
        assert!(begin(&pool, &locker, deadline).await.is_err());
        config.set(MAX_CONCURRENT_SETTING, "1").await.unwrap();
        assert!(begin(&pool, &locker, deadline).await.is_err());
        drop(second);
        drop(work);
        for value in ["0", "", " 1", "-1", "invalid"] {
            config.set(MAX_CONCURRENT_SETTING, value).await.unwrap();
            assert!(begin(&pool, &locker, deadline).await.is_err());
            assert_eq!(locker.active_teletext_work(), 0);
        }
        config.set(MAX_CONCURRENT_SETTING, "1").await.unwrap();
        assert!(begin(&pool, &locker, Instant::now()).await.is_err());
        assert_eq!(locker.active_teletext_work(), 0);
    }
}
