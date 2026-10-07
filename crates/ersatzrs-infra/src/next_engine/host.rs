//! Host-owned publishers, capacity and registered Next session lifetime.

use super::{
    config,
    coordinator::{ChannelPublisher, RefreshError, RefreshOutcome},
    process::{NextProcess, ProcessError},
    session::NextSession,
};
use crate::{
    db::DbPool,
    playback::load_channel_playback_profile,
    transcode_capacity::{
        load_active_transcode_limit, try_reserve_active_transcode, ActiveTranscodeReservation,
        ActiveTranscodeSlot, DEFAULT_ACTIVE_TRANSCODE_LIMIT,
    },
};
#[path = "host_publication.rs"]
mod publication;
pub(crate) use publication::AppliedHostPlayout;
#[cfg(test)]
pub(crate) async fn publication_fixture(
    root: &std::path::Path,
) -> (Arc<NextHost>, DbPool, EntityLocker, chrono::DateTime<Utc>) {
    let (host, start) = publication::tests::fixture(root).await;
    (host.clone(), host.pool.clone(), host.locker.clone(), start)
}
pub use publication::PreparedHostPlayout;
use publication::ProjectedSchedule;

use chrono::Utc;
use ersatzrs_core::{
    hls::{HlsAdmissionStatus, HlsSessionHandle, HlsSessionRegistry},
    locking::EntityLocker,
    path_replacement::PathReplacement,
    repositories::{ChannelRepository, RepositoryError},
};
use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Duration};
use tokio::{sync::Mutex, time::Instant};

/// Shared once by the web host, including sessions restarted for the same channel.
/// Keeping publishers alive prevents an old blocking publication from racing a
/// replacement session after cancellation.
pub struct NextHost {
    pub media: Arc<super::media::MediaRelay>,
    pub(super) pool: DbPool,
    pub(super) locker: EntityLocker,
    root: PathBuf,
    config_overlays: Option<PathBuf>,
    // Failed descendant cleanup cannot release capacity or reuse its output.
    // Existing capacity admission bounds this host-lifetime quarantine.
    cleanup_quarantine: Mutex<Vec<ActiveTranscodeReservation>>,
    publishers: Mutex<HashMap<i64, ChannelPublisher>>,
    pub(super) dynamic_contexts: Mutex<HashMap<uuid::Uuid, DynamicContext>>,
    pub(super) burn_cache: super::burn_cache::BurnCache,
}

/// One exact Next process instance; normal playback and preview never share it.
pub(super) struct DynamicContext {
    pub started_at: chrono::DateTime<Utc>,
    pub request: NextStart,
    pub troubleshooting: Option<Arc<super::troubleshooting::Context>>,
    pub transfers: Arc<super::session_transfers::SessionTransfers>,
    pub fallback: Arc<Mutex<Option<super::fallback::Occurrence>>>,
    pub gaps: Arc<Mutex<Vec<super::fallback::Gap>>>,
}

fn retain_dynamic_context(
    contexts: &mut HashMap<uuid::Uuid, DynamicContext>,
    request: &NextStart,
    started_at: chrono::DateTime<Utc>,
) -> uuid::Uuid {
    let instance = uuid::Uuid::new_v4();
    contexts.insert(
        instance,
        DynamicContext {
            started_at,
            request: request.clone(),
            transfers: Arc::default(),
            fallback: Arc::default(),
            gaps: Arc::default(),
            troubleshooting: None,
        },
    );
    instance
}

fn release_dynamic_context(
    contexts: &mut HashMap<uuid::Uuid, DynamicContext>,
    instance: uuid::Uuid,
) -> Option<DynamicContext> {
    contexts.remove(&instance)
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
            media: Arc::new(super::media::MediaRelay::new()),
            pool,
            locker,
            burn_cache: super::burn_cache::BurnCache::new(root.join("burn-subtitles")),
            root,
            config_overlays: None,
            cleanup_quarantine: Mutex::new(Vec::new()),
            dynamic_contexts: Mutex::new(HashMap::new()),
            publishers: Mutex::new(HashMap::new()),
        }
    }

    /// Use operator-managed overlays after the generated profile configuration.
    /// Tests can omit this setting to remain isolated from user configuration.
    #[must_use]
    pub fn with_config_overlays(mut self, folder: PathBuf) -> Self {
        self.config_overlays = Some(folder);
        self
    }

    /// Claim bytes only from the exact active Next process instance.
    /// The HTTP boundary must authenticate the internal capability first.
    /// # Errors
    /// Rejects expired sessions and unavailable or already consumed transfers.
    pub async fn claim_prepared(
        &self,
        instance: uuid::Uuid,
        id: uuid::Uuid,
    ) -> Result<super::session_transfers::TransferReader, crate::remote_stream::RemoteStreamError>
    {
        let contexts = self.dynamic_contexts.lock().await;
        let context = contexts
            .get(&instance)
            .ok_or(crate::remote_stream::RemoteStreamError::PreparedCancelled)?;
        context.transfers.claim(id)
    }

    async fn quarantine_unconfirmed_cleanup(
        &self,
        error: &crate::remote_stream::RemoteStreamError,
        reservation: &mut Option<ActiveTranscodeReservation>,
    ) -> bool {
        if !matches!(
            error,
            crate::remote_stream::RemoteStreamError::PreparedTerminationUnconfirmed
        ) {
            return false;
        }
        if let Some(reservation) = reservation.take() {
            self.cleanup_quarantine.lock().await.push(reservation);
        }
        true
    }

    pub(super) async fn configuration(
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
        let mut configuration = config::channel_config(
            &profile,
            channel.next_engine_text_subtitle_mode,
            &playout_folder,
            &request.ffmpeg,
            &request.ffprobe,
        )
        .map_err(|_| ProcessError::Preparation("profile conversion"))?;
        configuration.normalization.video.frame_rate =
            config::channel_frame_rate(&self.pool, &channel, &profile)
                .await
                .map_err(|_| ProcessError::Preparation("target frame rate read"))?;
        configuration.normalization.subtitle.fonts_folder = Some(
            crate::file_system_layout::fonts_cache_folder()
                .to_str()
                .ok_or(ProcessError::Preparation("fonts path encoding"))?
                .to_owned(),
        );
        // Next retains error dossiers independently of native FFmpeg report opt-in.
        configuration.ffmpeg.reports_folder = Some(
            crate::file_system_layout::ffmpeg_reports_folder()
                .to_str()
                .ok_or(ProcessError::Preparation("reports path encoding"))?
                .to_owned(),
        );
        Ok(configuration)
    }

    /// Reserve before spawning or touching the output folder. Concurrent starts
    /// reuse the existing handle. The returned task owns process cleanup, output
    /// cleanup and registry removal; normal shutdown must cancel, not abort it.
    #[allow(clippy::too_many_lines)] // Keep the supervised start and cleanup lifecycle together.
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
            let instance = retain_dynamic_context(
                &mut *host.dynamic_contexts.lock().await,
                &request,
                session.started_at,
            );
            let publisher = host.publisher(request.channel_id).await;
            let mut reservation = None;
            let checkpoints = super::on_demand::Checkpoints::new(
                host.pool.clone(),
                host.locker.clone(),
                request.channel_id,
                request.preview,
            );
            let startup = async {
                let config = host.configuration(&request).await?;
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
                checkpoints.prepare().await?;
                host.refresh(
                    &publisher,
                    request.channel_id,
                    &request.replacements,
                    request.deadline.into_std(),
                )
                .await
                .map_err(|_| ProcessError::Preparation("playout publication"))?;
                NextProcess::start_with_session(
                    &request.executable,
                    &request.number,
                    &request.output,
                    &config,
                    host.config_overlays.as_deref(),
                    Some(instance),
                    request.deadline,
                )
                .await
            };
            let outcome = tokio::select! {
                outcome = Box::pin(session.run(startup, request.idle_timeout, |deadline| {
                    host.refresh(&publisher, request.channel_id, &request.replacements, deadline)
                })) => outcome,
                never = checkpoints.run(&session) => match never {},
            };
            checkpoints.finish(&session).await;
            if let Err(error) = outcome {
                tracing::warn!(channel_id = request.channel_id, %error, "Next session failed");
            }
            if !host
                .shutdown_producers(instance, request.channel_id, &mut reservation)
                .await
            {
                return;
            }
            // Keep capacity and registry ownership until process and files are gone.
            if let Err(error) = tokio::fs::remove_dir_all(&request.output).await {
                if error.kind() != std::io::ErrorKind::NotFound {
                    tracing::warn!(channel_id = request.channel_id, kind = ?error.kind(), "Next output cleanup failed");
                }
            }
            drop(reservation);
            registry
                .remove_if_current(&(session as Arc<dyn HlsSessionHandle>))
                .await;
        }))
    }

    async fn shutdown_producers(
        &self,
        instance: uuid::Uuid,
        channel_id: i64,
        reservation: &mut Option<ActiveTranscodeReservation>,
    ) -> bool {
        // Revoke callbacks before awaiting cleanup. Unconfirmed termination keeps
        // the HLS registry, output directory and capacity reserved.
        let context = release_dynamic_context(&mut *self.dynamic_contexts.lock().await, instance);
        if let Some(context) = context {
            if let Err(error) = context.transfers.shutdown().await {
                tracing::warn!(channel_id, "Next producer cleanup failed");
                return !self
                    .quarantine_unconfirmed_cleanup(&error, reservation)
                    .await;
            }
        }
        true
    }

    /// Next sessions read one channel manifest, including publications made by
    /// the other session. Admit its exact gaps to every active process reader.
    pub(super) async fn register_published_gaps(
        &self,
        channel_id: i64,
        published: &[super::fallback::Gap],
    ) {
        let registries: Vec<_> = self
            .dynamic_contexts
            .lock()
            .await
            .values()
            .filter(|context| context.request.channel_id == channel_id)
            .map(|context| context.gaps.clone())
            .collect();
        let now = Utc::now();
        for registry in registries {
            let mut current = registry.lock().await;
            current.retain(|gap| gap.finish > now);
            for gap in published {
                if !current.contains(gap) {
                    current.push(*gap);
                }
            }
        }
    }

    pub(super) fn defer_item(
        &self,
        channel_id: i64,
        item_id: i64,
        descriptor: &str,
        mut item: super::protocol::PlayoutItem,
    ) -> ersatzrs_core::repositories::Result<super::protocol::PlayoutItem> {
        let audio = item
            .tracks
            .as_ref()
            .and_then(|tracks| tracks.audio.as_ref())
            .filter(|track| !matches!(track.source, Some(super::protocol::Source::Lavfi { .. })))
            .and_then(|track| track.stream_index);
        item.source = Some(
            self.media
                .dynamic_source(channel_id, item_id, descriptor, audio)?,
        );
        item.tracks = None;
        Ok(item)
    }

    async fn refresh(
        &self,
        publisher: &ChannelPublisher,
        channel_id: i64,
        replacements: &[PathReplacement],
        deadline: std::time::Instant,
    ) -> std::result::Result<(), RefreshError> {
        loop {
            let finish =
                chrono::DateTime::from_timestamp(253_402_300_799, 0).ok_or(RefreshError::Read)?;
            let now = Utc::now();
            // Callback bounds are 100 ns ticks; preserve every representable
            // bit of the publication boundary without moving it into the future.
            let start = chrono::DateTime::from_timestamp(
                now.timestamp(),
                now.timestamp_subsec_nanos() / 100 * 100,
            )
            .ok_or(RefreshError::Read)?;
            let publication_gaps = Arc::new(Mutex::new(Vec::new()));
            let projected_gaps = publication_gaps.clone();
            let outcome = publisher
                .refresh_before(start, finish, deadline, |channel, media| async move {
                    let ProjectedSchedule { items, gaps } = self
                        .project_schedule(channel, media, replacements, start)
                        .await?;
                    *projected_gaps.lock().await = gaps;
                    Ok(items)
                })
                .await?;
            if matches!(outcome, RefreshOutcome::Published { .. }) {
                self.register_published_gaps(channel_id, &publication_gaps.lock().await)
                    .await;
                return Ok(());
            }
            // Rebuild notification can precede release of its scheduler guard.
            // Startup or the session's per-refresh timeout bounds this wait.
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}

fn unavailable(message: &str) -> RepositoryError {
    RepositoryError::unavailable("Next projection", message)
}

/// Resolve the same validated startup runtime for every Next consumer.
/// # Errors
/// Returns a redacted error when the accepted executable is unavailable.
pub fn channel_binary() -> std::io::Result<PathBuf> {
    super::manager::channel_binary()
}

#[cfg(test)]
#[path = "host_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "reports_tests.rs"]
mod reports_tests;
