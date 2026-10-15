//! Automatic local source captions in the existing owned HLS item worker.

use ersatzrs_core::{hls::HlsSessionHandle as _, locking::EntityLocker};
use ersatzrs_domain::{Channel, ChannelSubtitleMode, Resolution, StreamingMode};

use super::{external, ActiveHlsItem, DbPool, HlsSessionWorker, RepositoryResult};
use crate::playback::source_teletext::runtime::{self, RuntimeSelectionContext, Selection};

pub(super) use crate::playback::source_teletext::runtime::SourceTeletextPlaybackInterval;

pub(super) struct AutomaticCaptionRequest<'a> {
    pub active: &'a ActiveHlsItem,
    pub languages: &'a [String],
    pub mode: ChannelSubtitleMode,
    pub channel: &'a Channel,
    pub resolution: &'a Resolution,
    pub start: i64,
    pub playback_interval: Option<SourceTeletextPlaybackInterval>,
    pub ordinary_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CaptionOccurrence {
    playout_item: i64,
    start: chrono::DateTime<chrono::Utc>,
    item: i64,
    version: i64,
    file: Option<i64>,
}

impl From<&ActiveHlsItem> for CaptionOccurrence {
    fn from(active: &ActiveHlsItem) -> Self {
        Self {
            playout_item: active.playout_item_id,
            start: active.playout_start,
            item: active.media_item_id,
            version: active.media_version_id,
            file: active.media_file_id,
        }
    }
}

impl HlsSessionWorker {
    /// Share the application's existing teletext preparation admission before
    /// starting the worker. Playback itself retains its transcode reservation.
    ///
    /// # Errors
    /// Returns the supplied locker when one was already configured.
    pub fn configure_source_teletext_locker(
        &self,
        locker: EntityLocker,
    ) -> Result<(), EntityLocker> {
        self.source_teletext_locker.set(locker)
    }

    pub(super) async fn automatic_source_caption(
        &self,
        pool: &DbPool,
        request: AutomaticCaptionRequest<'_>,
    ) -> RepositoryResult<Selection> {
        let active = request.active;
        let Some(locker) = self
            .source_teletext_locker
            .get()
            .filter(|_| active.external.is_none() && active.remote_stream.is_none())
        else {
            return external::subtitle(
                pool,
                active,
                request.languages,
                request.mode,
                request.channel,
                request.resolution,
                request.start,
            )
            .await
            .map(|subtitle| Selection {
                subtitle,
                source: None,
            });
        };
        // Preserve existing generated music-video credits ahead of all source
        // subtitle preferences, including their established independent policy.
        if let Some(subtitle) = crate::music_video_credits::generate_music_video_credits_subtitle(
            pool,
            active.media_item_id,
            request.channel,
            request.resolution,
            request.start,
        )
        .await?
        {
            return Ok(Selection {
                subtitle: Some(subtitle),
                source: None,
            });
        }
        if request.ordinary_only {
            return ordinary_caption_fallback(pool, &request).await;
        }
        let selected = runtime::select(RuntimeSelectionContext {
            pool,
            locker,
            media_item_id: Some(active.media_item_id),
            media_version_id: Some(active.media_version_id),
            media_file_id: active.media_file_id,
            original_path: &active.original_media_path,
            resolved_path: &active.media_path,
            languages: request.languages,
            subtitle_mode: request.mode,
            streaming_mode: StreamingMode::HttpLiveStreamingSegmenter,
            playback_interval: request.playback_interval,
            deadline: external::deadline(self),
        })
        .await;
        let Ok(selected) = selected else {
            tracing::warn!(
                media_item_id = active.media_item_id,
                "HLS source caption unavailable; retaining ordinary caption preferences"
            );
            return ordinary_caption_fallback(pool, &request).await;
        };
        if let Some(selected) = selected {
            return Ok(selected);
        }
        crate::playback::select_playback_subtitle(
            pool,
            active.media_item_id,
            request.languages,
            request.mode,
            StreamingMode::HttpLiveStreamingSegmenter,
        )
        .await
        .map(|subtitle| Selection {
            subtitle,
            source: None,
        })
    }

    pub(super) fn source_caption_check_deadline(
        &self,
        initial: tokio::time::Instant,
    ) -> tokio::time::Instant {
        caption_check_deadline(
            self.startup_readiness(),
            initial,
            tokio::time::Instant::now(),
        )
    }
}

fn caption_check_deadline(
    readiness: super::HlsStartupReadiness,
    initial: tokio::time::Instant,
    now: tokio::time::Instant,
) -> tokio::time::Instant {
    let bounded = now + std::time::Duration::from_secs(15);
    match readiness {
        // Initial session startup is never renewed by caption checks.
        super::HlsStartupReadiness::Pending { deadline } => {
            tokio::time::Instant::from_std(deadline).min(bounded)
        }
        super::HlsStartupReadiness::Failed => now,
        super::HlsStartupReadiness::Legacy | super::HlsStartupReadiness::Ready => {
            if initial > now {
                initial.min(bounded)
            } else {
                bounded
            }
        }
    }
}

async fn ordinary_caption_fallback(
    pool: &DbPool,
    request: &AutomaticCaptionRequest<'_>,
) -> RepositoryResult<Selection> {
    let selected = crate::playback::source_teletext::select_native_playback_subtitle_for_version(
        pool,
        request.active.media_item_id,
        request.active.media_version_id,
        request.languages,
        request.mode,
        StreamingMode::HttpLiveStreamingSegmenter,
        &[],
    )
    .await?;
    Ok(Selection {
        subtitle: selected.map(|selected| selected.into_parts().0),
        source: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::Instant;

    #[test]
    fn pending_startup_deadline_is_not_renewed_by_source_rechecks() {
        let now = Instant::now();
        let deadline = now + Duration::from_secs(5);
        assert_eq!(
            caption_check_deadline(
                super::super::HlsStartupReadiness::Pending {
                    deadline: deadline.into_std()
                },
                now - Duration::from_secs(1),
                now,
            ),
            deadline
        );
        let expired = now - Duration::from_secs(1);
        assert_eq!(
            caption_check_deadline(
                super::super::HlsStartupReadiness::Pending {
                    deadline: expired.into_std()
                },
                now + Duration::from_secs(15),
                now,
            ),
            expired
        );
    }

    #[test]
    fn established_worker_can_recheck_for_a_later_retry_after_initial_budget_expires() {
        let now = Instant::now();
        for readiness in [
            super::super::HlsStartupReadiness::Legacy,
            super::super::HlsStartupReadiness::Ready,
        ] {
            assert_eq!(
                caption_check_deadline(readiness, now - Duration::from_secs(1), now),
                now + Duration::from_secs(15)
            );
            assert_eq!(
                caption_check_deadline(readiness, now + Duration::from_secs(5), now),
                now + Duration::from_secs(5)
            );
        }
        assert_eq!(
            caption_check_deadline(
                super::super::HlsStartupReadiness::Failed,
                now + Duration::from_secs(15),
                now
            ),
            now
        );
    }
}
