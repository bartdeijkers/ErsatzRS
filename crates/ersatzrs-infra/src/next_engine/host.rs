//! Host-owned publishers, capacity and registered Next session lifetime.

use super::{
    config,
    coordinator::{ChannelPublisher, RefreshError, RefreshOutcome},
    process::{NextProcess, ProcessError},
    selection,
    session::NextSession,
    source,
};
use crate::{
    db::DbPool,
    playback::load_channel_playback_profile,
    transcode_capacity::{
        load_active_transcode_limit, try_reserve_active_transcode, ActiveTranscodeSlot,
        DEFAULT_ACTIVE_TRANSCODE_LIMIT,
    },
};
use chrono::Utc;
use ersatzrs_core::{
    hls::{HlsAdmissionStatus, HlsSessionRegistry},
    locking::EntityLocker,
    path_replacement::PathReplacement,
    repositories::{ChannelRepository, RepositoryError},
};
use ersatzrs_domain::{ChannelMusicVideoCreditsMode, StreamingEngine};
use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Duration};
use tokio::{sync::Mutex, time::Instant};

/// Shared once by the web host, including sessions restarted for the same channel.
/// Keeping publishers alive prevents an old blocking publication from racing a
/// replacement session after cancellation.
pub struct NextHost {
    pub media: super::media::MediaRelay,
    pool: DbPool,
    locker: EntityLocker,
    root: PathBuf,
    publishers: Mutex<HashMap<i64, ChannelPublisher>>,
}

#[derive(Clone)]
pub struct NextStart {
    pub channel_id: i64,
    pub preview: bool,
    pub number: String,
    pub executable: PathBuf,
    pub output: PathBuf,
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
    pub replacements: Vec<PathReplacement>,
    pub idle_timeout: Option<Duration>,
    pub deadline: Instant,
}

impl NextHost {
    #[must_use]
    pub fn new(pool: DbPool, locker: EntityLocker, root: PathBuf) -> Self {
        Self {
            media: super::media::MediaRelay::new(),
            pool,
            locker,
            root,
            publishers: Mutex::new(HashMap::new()),
        }
    }

    async fn configuration(
        &self,
        request: &NextStart,
    ) -> Result<super::protocol::ChannelConfig, ProcessError> {
        let profile = load_channel_playback_profile(&self.pool, request.channel_id)
            .await
            .map_err(|_| ProcessError::Preparation("profile read"))?
            .ok_or(ProcessError::Preparation("profile unavailable"))?;
        let profile = if request.preview {
            crate::hls_worker::apply_playback_profile_override(
                profile,
                crate::hls_worker::HlsPlaybackProfileOverride::BrowserCompatible,
            )
        } else {
            profile
        };
        let channel = crate::repositories::make_repositories(self.pool.clone())
            .channel
            .get_with_profile(request.channel_id)
            .await
            .map_err(|_| ProcessError::Preparation("channel read"))?;
        let playout_folder = self.root.join(request.channel_id.to_string());
        config::channel_config(
            &profile,
            channel.next_engine_text_subtitle_mode,
            &playout_folder,
            &request.ffmpeg,
            &request.ffprobe,
        )
        .map_err(|_| ProcessError::Preparation("profile conversion"))
    }

    /// Reserve before spawning or touching the output folder. Concurrent starts
    /// reuse the existing handle. The returned task owns process cleanup, output
    /// cleanup and registry removal; normal shutdown must cancel, not abort it.
    pub async fn start(
        self: &Arc<Self>,
        registry: Arc<dyn HlsSessionRegistry>,
        request: NextStart,
    ) -> Option<tokio::task::JoinHandle<()>> {
        let session = Arc::new(NextSession::new(
            request.number.clone(),
            request.output.clone(),
            request.deadline,
        ));
        if !registry.try_insert(session.clone()).await {
            if let Some(existing) = registry.try_get(&request.number).await {
                existing.touch();
            }
            return None;
        }
        let host = self.clone();
        Some(tokio::spawn(async move {
            let mut publishers = host.publishers.lock().await;
            let publisher = publishers
                .entry(request.channel_id)
                .or_insert_with(|| {
                    ChannelPublisher::new(
                        host.pool.clone(),
                        host.locker.clone(),
                        &host.root,
                        request.channel_id,
                    )
                })
                .clone();
            drop(publishers);
            let mut reservation = None;
            let startup = async {
                let config = host.configuration(&request).await?;
                host.refresh(&publisher, request.channel_id, &request.replacements)
                    .await
                    .map_err(|_| ProcessError::Preparation("playout publication"))?;
                loop {
                    let limit = load_active_transcode_limit(&host.pool)
                        .await
                        .map_err(|_| ProcessError::Preparation("capacity setting"))?
                        .unwrap_or(DEFAULT_ACTIVE_TRANSCODE_LIMIT);
                    if let Some(held) = try_reserve_active_transcode(
                        limit,
                        ActiveTranscodeSlot::new_next_hls_with_capacity_key(
                            &request.number,
                            &if request.preview {
                                format!("preview:{}", request.number)
                            } else {
                                request.number.clone()
                            },
                        ),
                    ) {
                        reservation = Some(held);
                        session.set_admission(HlsAdmissionStatus::Transcoding);
                        break;
                    }
                    session.set_admission(HlsAdmissionStatus::AtCapacity);
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                NextProcess::start(
                    &request.executable,
                    &request.number,
                    &request.output,
                    &config,
                    request.deadline,
                )
                .await
            };
            let outcome = Box::pin(session.run(startup, request.idle_timeout, || {
                host.refresh(&publisher, request.channel_id, &request.replacements)
            }))
            .await;
            if let Err(error) = outcome {
                tracing::warn!(channel_id = request.channel_id, %error, "Next session failed");
            }
            // Keep capacity and registry ownership until process and files are gone.
            if let Err(error) = tokio::fs::remove_dir_all(&request.output).await {
                if error.kind() != std::io::ErrorKind::NotFound {
                    tracing::warn!(channel_id = request.channel_id, kind = ?error.kind(), "Next output cleanup failed");
                }
            }
            drop(reservation);
            registry.remove(&request.number).await;
        }))
    }

    async fn refresh(
        &self,
        publisher: &ChannelPublisher,
        channel_id: i64,
        replacements: &[PathReplacement],
    ) -> std::result::Result<(), RefreshError> {
        loop {
            let finish =
                chrono::DateTime::from_timestamp(253_402_300_799, 0).ok_or(RefreshError::Read)?;
            let outcome = publisher
                .refresh(Utc::now(), finish, |channel, media| async move {
                    if channel.streaming_engine != StreamingEngine::Next {
                        return Err(unavailable("engine changed"));
                    }
                    let profile = load_channel_playback_profile(&self.pool, channel_id)
                        .await?
                        .ok_or_else(|| unavailable("profile unavailable"))?;
                    let mut items = Vec::with_capacity(media.len());
                    for media in media {
                        let source = source::resolve_published(
                            &self.pool,
                            &media,
                            replacements,
                            Some(&self.media),
                        )
                        .await?;
                        let selection_time =
                            crate::channel_playout::ChannelPlayoutContext::for_channel(&channel)
                                .source_time(media.item.start)?;
                        let watermarks = crate::playback::select_playback_watermarks(
                            &self.pool,
                            &channel,
                            media.item.id,
                            media.item.disable_watermarks,
                            selection_time,
                        )
                        .await?;
                        let graphics = crate::playback::select_playback_graphics(
                            &self.pool,
                            &channel,
                            media.item.id,
                            selection_time,
                        )
                        .await?;
                        if !watermarks.is_empty()
                            || !graphics.is_empty()
                            || channel.music_video_credits_mode
                                != ChannelMusicVideoCreditsMode::None
                        {
                            return Err(unavailable("graphics projection is not integrated"));
                        }
                        items.push(
                            selection::project_item(
                                &self.pool,
                                &channel,
                                &media.item,
                                media.media_version_id,
                                source,
                                &profile.resolution,
                            )
                            .await?,
                        );
                    }
                    Ok(items)
                })
                .await?;
            if matches!(outcome, RefreshOutcome::Published { .. }) {
                return Ok(());
            }
            // Rebuild notification can precede release of its scheduler guard.
            // The session's startup deadline or cancellation bounds this wait.
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}

fn unavailable(message: &str) -> RepositoryError {
    RepositoryError::unavailable("Next projection", message)
}

/// Resolve the installed runtime only when a Next channel is requested.
/// # Errors
/// Returns a redacted error when the configured/bundled executable is unavailable.
pub fn channel_binary() -> std::io::Result<PathBuf> {
    let host = std::env::current_exe()
        .map_err(|e| std::io::Error::new(e.kind(), "host executable is unavailable"))?;
    super::runtime::resolve_channel_binary(
        crate::startup_config::next_folder().as_deref(),
        &host,
        crate::startup_config::StartupConfig::load()
            .home_dir
            .as_deref(),
    )
}
