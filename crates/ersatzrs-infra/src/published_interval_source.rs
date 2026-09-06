//! Frozen publication intent and bounded add-on-owned media transfer (ADR 0087).

use std::{fmt, time::Duration};

use ersatzrs_addon_contract::{AddonMediaListOptionOperation, RemoteStreamRetainedPlayRequest};
use ersatzrs_core::addon_media_list_playback::AddonMediaListPlaybackTimelinePlan;
use ersatzrs_domain::RemoteStreamLiveness;
use ersatzrs_ffmpeg::{InputFile, SourceAudioStream, SourceVideoStream};
use tokio::sync::oneshot;

use crate::{
    addon_media_list_item_runtime::{InstalledItemRuntime, ItemRuntimeError},
    addon_media_list_options::ResolvedAddonItemOptions,
    db::DbPool,
    ffprobe::{FfprobeExtractor, FfprobeMediaVersion},
    playback::{source_audio_streams_from_probe, source_video_stream_from_probe},
    remote_stream::{PreparedRemoteStreamInput, RemoteStreamError, RemoteStreamSpawnContext},
};

/// Borrowed startup services; the caller owns their lifetime and the deadline.
pub struct IntervalPlaybackContext<'a> {
    pub pool: &'a DbPool,
    pub probe: &'a FfprobeExtractor,
    pub spawn_context: RemoteStreamSpawnContext,
}

/// One playback-owned producer and fresh metadata. Never log the request.
pub struct RetainedIntervalPlayback {
    pub prepared: PreparedRemoteStreamInput,
    pub video_input: InputFile,
    pub audio_input: InputFile,
    pub source_video: SourceVideoStream,
    pub source_audio: Vec<SourceAudioStream>,
}

impl fmt::Debug for RetainedIntervalPlayback {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RetainedIntervalPlayback")
            .finish_non_exhaustive()
    }
}

/// Cloneable immutable intent; contains no resolved transport or runtime guard.
#[derive(Clone, PartialEq, Eq)]
pub struct PublishedIntervalSource {
    addon_id: String,
    source_url: String,
    provider_id: String,
    options: ResolvedAddonItemOptions,
    plan: AddonMediaListPlaybackTimelinePlan,
}

impl fmt::Debug for PublishedIntervalSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PublishedIntervalSource")
            .finish_non_exhaustive()
    }
}

impl PublishedIntervalSource {
    /// Start one add-on-owned transfer and probe only its retained header prefix.
    /// EOF precedes provider admission. Cancellation and failures reap the
    /// producer before returning; the same child supplies the encoder afterward.
    ///
    /// # Errors
    /// Rejects invalid seeks, unavailable packages, failed startup and invalid media.
    pub async fn prepare_playback_cancellable(
        &self,
        context: IntervalPlaybackContext<'_>,
        programme_milliseconds: u64,
        max_video_height: Option<u32>,
        deadline: tokio::time::Instant,
        mut cancellation: oneshot::Receiver<()>,
    ) -> Result<Option<RetainedIntervalPlayback>, ItemRuntimeError> {
        let ranges = self
            .plan
            .timeline()
            .map_err(|_| ItemRuntimeError::InvalidRequest)?
            .ranges_from(programme_milliseconds)
            .map_err(|_| ItemRuntimeError::InvalidRequest)?;
        if ranges.is_empty() {
            return Ok(None);
        }
        let deadline = deadline.min(tokio::time::Instant::now() + Duration::from_secs(20));
        let request = self.request(programme_milliseconds, max_video_height)?;
        check_startup(&mut cancellation, deadline)?;
        // Child-free package/schema reads can be dropped on cancellation.
        let preflight = async {
            let runtime = InstalledItemRuntime::load(context.pool, &self.addon_id, None).await?;
            runtime.validate_options(&self.options)?;
            runtime.retained_play_command(&request).await
        };
        let command = tokio::select! {
            biased;
            _ = &mut cancellation => return Err(ItemRuntimeError::Failed),
            result = tokio::time::timeout_at(deadline, preflight) =>
                result.map_err(|_| ItemRuntimeError::Failed)??,
        };
        let input = serde_json::to_vec(&request).map_err(|_| ItemRuntimeError::InvalidRequest)?;
        let (cancel, receiver) = oneshot::channel();
        let startup = PreparedRemoteStreamInput::start(
            &command,
            &input,
            context.spawn_context,
            deadline,
            receiver,
        );
        tokio::pin!(startup);
        let prepared = tokio::select! {
            biased;
            _ = &mut cancellation => {
                let _ = cancel.send(());
                match startup.await {
                    Ok(prepared) => prepared.cancel().await.map_err(playback_error)?,
                    Err(error) => return Err(playback_error(error)),
                }
                return Err(ItemRuntimeError::Failed);
            },
            result = &mut startup => result.map_err(playback_error)?,
        };
        let metadata = async {
            check_startup(&mut cancellation, deadline)?;
            let (cancel, receiver) = oneshot::channel();
            let probe = await_playback_stage(&mut cancellation, deadline, cancel, async {
                context
                    .probe
                    .extract_retained_prefix_cancellable(prepared.prefix(), deadline, receiver)
                    .await
                    .map_err(|_| ItemRuntimeError::Failed)
            })
            .await?;
            check_startup(&mut cancellation, deadline)?;
            // The v4 stream is decoded media, never a CDN locator or a second
            // independently encoded audio stream at each cut.
            if probe.streams.iter().any(|stream| match stream.kind {
                ersatzrs_domain::MediaStreamKind::Video => {
                    stream.codec.as_deref() != Some("rawvideo")
                }
                ersatzrs_domain::MediaStreamKind::Audio => {
                    stream.codec.as_deref() != Some("pcm_f32le")
                }
                _ => true,
            }) {
                return Err(ItemRuntimeError::InvalidResult);
            }
            playback_metadata(&probe, None)
        }
        .await;
        let (source_video, source_audio) = match metadata {
            Ok(metadata) => metadata,
            Err(error) => {
                prepared.cancel().await.map_err(playback_error)?;
                return Err(error);
            }
        };
        let audio_index = source_audio
            .iter()
            .find(|audio| audio.default)
            .unwrap_or(&source_audio[0])
            .index;
        Ok(Some(RetainedIntervalPlayback {
            video_input: InputFile::video("pipe:0")
                .with_input_options(["-f", "nut"])
                .with_stream_index(source_video.index),
            audio_input: InputFile::audio("pipe:0")
                .with_input_options(["-f", "nut"])
                .with_stream_index(audio_index),
            prepared,
            source_video,
            source_audio,
        }))
    }

    pub(crate) fn new(
        addon_id: String,
        source_url: String,
        provider_id: String,
        options: ResolvedAddonItemOptions,
        plan: AddonMediaListPlaybackTimelinePlan,
    ) -> Result<Self, ItemRuntimeError> {
        if options.schema.addon_id != addon_id
            || !options.requires_preparation(RemoteStreamLiveness::Finite)
        {
            return Err(ItemRuntimeError::InvalidRequest);
        }
        let source = Self {
            addon_id,
            source_url,
            provider_id,
            options,
            plan,
        };
        source.request(0, None)?;
        Ok(source)
    }

    /// The frozen source window and exclusions; never recomputed during playback.
    #[must_use]
    pub fn plan(&self) -> &AddonMediaListPlaybackTimelinePlan {
        &self.plan
    }

    fn request(
        &self,
        programme_milliseconds: u64,
        max_video_height: Option<u32>,
    ) -> Result<RemoteStreamRetainedPlayRequest, ItemRuntimeError> {
        let request = RemoteStreamRetainedPlayRequest {
            source_url: self.source_url.clone(),
            provider_id: self.provider_id.clone(),
            context_fingerprint: self.plan.context_fingerprint().to_owned(),
            source_duration_milliseconds: self.plan.source_duration_milliseconds(),
            options: self
                .options
                .for_operation(
                    AddonMediaListOptionOperation::Playback,
                    RemoteStreamLiveness::Finite,
                )
                .map_err(|_| ItemRuntimeError::InvalidRequest)?,
            max_video_height,
            retained_intervals_milliseconds: self
                .plan
                .timeline()
                .map_err(|_| ItemRuntimeError::InvalidRequest)?
                .ranges_from(programme_milliseconds)
                .map_err(|_| ItemRuntimeError::InvalidRequest)?,
        };
        request
            .validate(&self.options.definition)
            .map_err(|_| ItemRuntimeError::InvalidRequest)?;
        Ok(request)
    }
}

fn playback_error(error: RemoteStreamError) -> ItemRuntimeError {
    match error {
        RemoteStreamError::PreparedTerminationUnconfirmed => {
            ItemRuntimeError::TerminationUnconfirmed
        }
        _ => ItemRuntimeError::Failed,
    }
}

fn stage_budget(deadline: tokio::time::Instant) -> Result<Duration, ItemRuntimeError> {
    deadline
        .checked_duration_since(tokio::time::Instant::now())
        .filter(|remaining| !remaining.is_zero())
        .map(|remaining| remaining.min(Duration::from_secs(20)))
        .ok_or(ItemRuntimeError::Failed)
}

fn check_startup(
    cancellation: &mut oneshot::Receiver<()>,
    deadline: tokio::time::Instant,
) -> Result<(), ItemRuntimeError> {
    if !matches!(
        cancellation.try_recv(),
        Err(oneshot::error::TryRecvError::Empty)
    ) {
        return Err(ItemRuntimeError::Failed);
    }
    stage_budget(deadline).map(|_| ())
}

async fn await_playback_stage<T>(
    cancellation: &mut oneshot::Receiver<()>,
    deadline: tokio::time::Instant,
    cancel: oneshot::Sender<()>,
    operation: impl std::future::Future<Output = Result<T, ItemRuntimeError>>,
) -> Result<T, ItemRuntimeError> {
    tokio::pin!(operation);
    tokio::select! {
        biased;
        _ = &mut *cancellation => {},
        () = tokio::time::sleep_until(deadline) => {},
        result = &mut operation => return result,
    }
    let _ = cancel.send(());
    match operation.await {
        Err(ItemRuntimeError::TerminationUnconfirmed) => {
            Err(ItemRuntimeError::TerminationUnconfirmed)
        }
        _ => Err(ItemRuntimeError::Failed),
    }
}

fn playback_metadata(
    video_probe: &FfprobeMediaVersion,
    audio_probe: Option<&FfprobeMediaVersion>,
) -> Result<(SourceVideoStream, Vec<SourceAudioStream>), ItemRuntimeError> {
    // The shared persisted-audio projection permits absent codecs for legacy
    // rows. Fresh resolved transport must actually identify its audio codecs.
    if audio_probe
        .unwrap_or(video_probe)
        .streams
        .iter()
        .any(|stream| {
            stream.kind == ersatzrs_domain::MediaStreamKind::Audio
                && stream
                    .codec
                    .as_deref()
                    .is_none_or(|codec| codec.trim().is_empty())
        })
    {
        return Err(ItemRuntimeError::InvalidResult);
    }
    let video = source_video_stream_from_probe(video_probe)
        .map_err(|_| ItemRuntimeError::InvalidResult)?
        .ok_or(ItemRuntimeError::InvalidResult)?;
    let audio = if let Some(audio_probe) = audio_probe {
        if !source_audio_streams_from_probe(video_probe)
            .map_err(|_| ItemRuntimeError::InvalidResult)?
            .is_empty()
            || source_video_stream_from_probe(audio_probe)
                .map_err(|_| ItemRuntimeError::InvalidResult)?
                .is_some()
        {
            return Err(ItemRuntimeError::InvalidResult);
        }
        source_audio_streams_from_probe(audio_probe)
    } else {
        source_audio_streams_from_probe(video_probe)
    }
    .map_err(|_| ItemRuntimeError::InvalidResult)?;
    if audio.is_empty()
        || (audio_probe.is_none() && audio.iter().any(|audio| audio.index == video.index))
    {
        return Err(ItemRuntimeError::InvalidResult);
    }
    Ok((video, audio))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn retained_playback_preserves_uncertain_process_cleanup() {
        assert_eq!(
            playback_error(RemoteStreamError::PreparedTerminationUnconfirmed),
            ItemRuntimeError::TerminationUnconfirmed,
        );
        assert_eq!(
            playback_error(RemoteStreamError::PreparedCancelled),
            ItemRuntimeError::Failed,
        );
    }

    fn probe(streams: serde_json::Value) -> FfprobeMediaVersion {
        let mut document = serde_json::json!({"format":{"duration":"900"}});
        document["streams"] = streams;
        crate::ffprobe::parse_ffprobe_json(
            Path::new("synthetic-media"),
            &serde_json::to_vec(&document).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn interval_probe_metadata_preserves_muxed_reversed_indexes_and_alternate_audio() {
        let probe = probe(serde_json::json!([
            {"index":0,"codec_type":"audio","codec_name":"aac","tags":{"language":"jpn"}},
            {"index":3,"codec_type":"audio","codec_name":"aac","tags":{"language":"eng"},"disposition":{"default":1}},
            {"index":7,"codec_type":"video","codec_name":"h264","width":1920,"height":1080}
        ]));
        let (video, audio) = playback_metadata(&probe, None).unwrap();
        assert_eq!(video.index, 7);
        assert_eq!(
            audio.iter().map(|audio| audio.index).collect::<Vec<_>>(),
            [0, 3]
        );
        assert_eq!(audio[0].language.as_deref(), Some("jpn"));
        assert!(audio[1].default);
        assert_eq!(probe.version.duration, chrono::Duration::seconds(900));
    }

    #[test]
    fn interval_probe_metadata_validates_separate_layout_without_assumed_indexes() {
        let video = probe(serde_json::json!([
            {"index":0,"codec_type":"video","codec_name":"h264","width":1920,"height":1080}
        ]));
        let audio = probe(serde_json::json!([
            {"index":0,"codec_type":"audio","codec_name":"aac"}
        ]));
        let (source_video, source_audio) = playback_metadata(&video, Some(&audio)).unwrap();
        assert_eq!(source_video.index, 0);
        assert_eq!(source_audio[0].index, 0);
        assert!(playback_metadata(&video, None).is_err());
        assert!(playback_metadata(&audio, Some(&video)).is_err());
        assert!(playback_metadata(&video, Some(&video)).is_err());
        let muxed = probe(serde_json::json!([
            {"index":0,"codec_type":"video","codec_name":"h264","width":1920,"height":1080},
            {"index":1,"codec_type":"audio","codec_name":"aac"}
        ]));
        assert!(playback_metadata(&muxed, Some(&audio)).is_err());
        let invalid = probe(serde_json::json!([
            {"index":0,"codec_type":"video","codec_name":"h264","width":1920,"height":1080},
            {"index":0,"codec_type":"audio","codec_name":"aac"}
        ]));
        assert!(playback_metadata(&invalid, None).is_err());
        let missing_codec = probe(serde_json::json!([
            {"index":0,"codec_type":"audio"}
        ]));
        assert!(playback_metadata(&video, Some(&missing_codec)).is_err());
    }

    #[tokio::test]
    async fn interval_stage_cancellation_and_deadline_await_cleanup() {
        for cancelled in [true, false] {
            let (outer, mut receiver) = oneshot::channel();
            if cancelled {
                outer.send(()).unwrap();
            }
            let (cancel, child_receiver) = oneshot::channel();
            let reaped = std::sync::atomic::AtomicBool::new(false);
            let operation = async {
                child_receiver.await.unwrap();
                tokio::task::yield_now().await;
                reaped.store(true, std::sync::atomic::Ordering::SeqCst);
                Err::<(), _>(ItemRuntimeError::TerminationUnconfirmed)
            };
            let result = await_playback_stage(
                &mut receiver,
                tokio::time::Instant::now(),
                cancel,
                operation,
            )
            .await;
            assert_eq!(result, Err(ItemRuntimeError::TerminationUnconfirmed));
            assert!(reaped.load(std::sync::atomic::Ordering::SeqCst));
        }
    }

    #[tokio::test]
    async fn interval_preparation_eof_and_invalid_seek_precede_all_services() {
        let definition = serde_json::from_value(serde_json::json!({
            "id":"media-list.options.v1", "options":[],
        }))
        .unwrap();
        let source = PublishedIntervalSource {
            addon_id: "org.example.unavailable".into(),
            source_url: "https://example.test/item".into(),
            provider_id: "item".into(),
            options: ResolvedAddonItemOptions {
                schema: ersatzrs_core::repositories::AddonMediaListOptionSchemaRef {
                    addon_id: "org.example.unavailable".into(),
                    capability_id: "media-list.options.v1".into(),
                    schema_digest: "a".repeat(64),
                },
                definition,
                values: ersatzrs_addon_contract::AddonMediaListOptionValues {
                    schema: "media-list.options.v1".into(),
                    values: std::collections::BTreeMap::default(),
                },
            },
            plan: AddonMediaListPlaybackTimelinePlan::new(10_000, 0..10_000, &[], "a".repeat(64))
                .unwrap(),
        };
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect_lazy("sqlite::memory:")
            .unwrap();
        let extractor = FfprobeExtractor::new("nonexistent-synthetic-probe");
        let temporary = tempfile::tempdir().unwrap();
        let directory = temporary.path().join("must-not-be-created");
        for seek in [10_000, 10_001] {
            let (_sender, receiver) = oneshot::channel();
            let result = source
                .prepare_playback_cancellable(
                    IntervalPlaybackContext {
                        pool: &pool,
                        probe: &extractor,
                        spawn_context: RemoteStreamSpawnContext::probe(),
                    },
                    seek,
                    None,
                    tokio::time::Instant::now(),
                    receiver,
                )
                .await;
            if seek == 10_000 {
                assert!(result.unwrap().is_none());
            } else {
                assert!(matches!(result, Err(ItemRuntimeError::InvalidRequest)));
            }
            assert!(!directory.exists());
        }
    }
}
