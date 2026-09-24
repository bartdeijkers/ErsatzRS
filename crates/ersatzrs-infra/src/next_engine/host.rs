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
        load_active_transcode_limit, try_reserve_active_transcode, ActiveTranscodeReservation,
        ActiveTranscodeSlot, DEFAULT_ACTIVE_TRANSCODE_LIMIT,
    },
};
use chrono::Utc;
use ersatzrs_core::{
    hls::{HlsAdmissionStatus, HlsSessionRegistry},
    locking::EntityLocker,
    path_replacement::PathReplacement,
    repositories::{ChannelRepository, RepositoryError},
};
use ersatzrs_domain::StreamingEngine;
use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Duration};
use tokio::{sync::Mutex, time::Instant};

/// Shared once by the web host, including sessions restarted for the same channel.
/// Keeping publishers alive prevents an old blocking publication from racing a
/// replacement session after cancellation.
pub struct NextHost {
    pub media: super::media::MediaRelay,
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
    pub request: NextStart,
    pub transfers: Arc<super::session_transfers::SessionTransfers>,
    pub fallback: Arc<Mutex<Option<super::fallback::Occurrence>>>,
    pub gaps: Arc<Mutex<Vec<super::fallback::Gap>>>,
}

fn retain_dynamic_context(
    contexts: &mut HashMap<uuid::Uuid, DynamicContext>,
    request: &NextStart,
) -> uuid::Uuid {
    let instance = uuid::Uuid::new_v4();
    contexts.insert(
        instance,
        DynamicContext {
            request: request.clone(),
            transfers: Arc::default(),
            fallback: Arc::default(),
            gaps: Arc::default(),
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
            media: super::media::MediaRelay::new(),
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
        let mut configuration = config::channel_config(
            &profile,
            channel.next_engine_text_subtitle_mode,
            &playout_folder,
            &request.ffmpeg,
            &request.ffprobe,
        )
        .map_err(|_| ProcessError::Preparation("profile conversion"))?;
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
            let instance =
                retain_dynamic_context(&mut *host.dynamic_contexts.lock().await, &request);
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
            let outcome = Box::pin(session.run(startup, request.idle_timeout, || {
                host.refresh(&publisher, request.channel_id, &request.replacements)
            }))
            .await;
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
            registry.remove(&request.number).await;
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

    fn defer_item(
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

    #[allow(clippy::too_many_lines)] // Keep the atomic publication steps together.
    async fn refresh(
        &self,
        publisher: &ChannelPublisher,
        channel_id: i64,
        replacements: &[PathReplacement],
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
                .refresh(start, finish, |channel, media| async move {
                    if channel.streaming_engine != StreamingEngine::Next {
                        return Err(unavailable("engine changed"));
                    }
                    let profile = load_channel_playback_profile(&self.pool, channel_id)
                        .await?
                        .ok_or_else(|| unavailable("profile unavailable"))?;
                    let mut items = Vec::with_capacity(media.len());
                    for media in media {
                        if media.remote_stream_id.is_some() {
                            let snapshot = super::remote_projection::project_publishable(
                                &self.pool,
                                &channel,
                                &media,
                                &profile.resolution,
                                selection::AudioDirective::Select,
                            )
                            .await?;
                            items.push(self.defer_item(
                                channel_id,
                                media.item.id,
                                &snapshot.identity.descriptor,
                                snapshot.projection.item,
                            )?);
                            continue;
                        }
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
                        if !graphics.is_empty() {
                            return Err(unavailable("graphics projection is not integrated"));
                        }
                        let mut projection = selection::project_deferred_item(
                            &self.pool,
                            &channel,
                            &media.item,
                            media.media_version_id,
                            source,
                            &profile.resolution,
                        )
                        .await?;
                        super::credits::attach(
                            &self.pool,
                            &self.media,
                            &channel,
                            &media.item,
                            &mut projection,
                        )
                        .await?;
                        projection.item.watermark = super::watermark::project(&watermarks)?;
                        if projection.pending_burn.is_some() {
                            let identity = super::dynamic_projection::fingerprint(
                                &self.pool,
                                &media,
                                &projection,
                                &channel,
                            )
                            .await?;
                            projection.item = self.defer_item(
                                channel_id,
                                media.item.id,
                                &identity.descriptor,
                                projection.item,
                            )?;
                        }
                        items.push(projection.item);
                    }
                    let (items, gaps) =
                        super::fallback::insert_gaps(&self.media, channel_id, start, items)?;
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

#[cfg(test)]
#[path = "host_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "reports_tests.rs"]
mod reports_tests;
