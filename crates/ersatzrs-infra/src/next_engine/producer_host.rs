//! Session-owned ordinary installed producers; no independent probe or script fallback.
use chrono::{DateTime, Utc};
use ersatzrs_core::repositories::{RepositoryError, Result};
use ersatzrs_domain::RemoteStreamLiveness;
use tokio::{sync::oneshot, time::Instant};

use super::{
    dynamic_projection::Snapshot,
    host::{NextHost, NextStart},
    protocol,
    remote_projection::ProducerContext,
    retained_host::{install_fresh_tracks, PendingClaim},
    session_transfers::{PreparedTransferMedia, TransferBinding},
};
use crate::{
    ffprobe::FfprobeExtractor,
    remote_stream::{
        PreparedRemoteStreamInput, RemoteStreamError, RemoteStreamSource, RemoteStreamSpawnContext,
    },
};

impl NextHost {
    #[allow(
        clippy::too_many_arguments,
        reason = "Producer resolution carries the source, session, selected tracks, and deadline."
    )]
    pub(super) async fn resolve_producer(
        &self,
        before: Snapshot,
        context: ProducerContext,
        request: &NextStart,
        instance: uuid::Uuid,
        audio: Option<usize>,
        now: DateTime<Utc>,
        deadline: Instant,
    ) -> Result<protocol::PlayoutItem> {
        let (image, subtitle) =
            producer_subtitles(&before.projection, &context.media.path, now, deadline).await?;
        let seek = (context.media.item.in_point + (now - context.media.item.start))
            .to_std()
            .map_err(|_| unavailable())?;
        let transfers = self
            .dynamic_contexts
            .lock()
            .await
            .get(&instance)
            .map(|context| context.transfers.clone())
            .ok_or_else(unavailable)?;
        let binding = TransferBinding {
            media_item_id: context.media.item.media_item_id,
            playout_item_id: context.media.item.id,
            media_version_id: context.media.media_version_id,
            media_file_id: context.media.media_file_id,
            publication: before.identity.descriptor.clone(),
            start: before.projection.item.start,
            finish: before.projection.item.finish,
        };
        let preparation_deadline = deadline - std::time::Duration::from_secs(6);
        let preparation = ProducerPreparation {
            pool: self.pool.clone(),
            media_id: context.media.item.media_item_id,
            ffmpeg: request.ffmpeg.to_str().ok_or_else(unavailable)?.into(),
            ffprobe: request.ffprobe.to_str().ok_or_else(unavailable)?.into(),
            number: request.number.clone(),
            max_height: context.max_height,
            seek,
            deadline: preparation_deadline,
        };
        let (timing_tx, timing_rx) = oneshot::channel();
        let ready = Box::pin(transfers.prepare(
            binding,
            preparation_deadline,
            deadline + std::time::Duration::from_secs(10),
            move |cancel| preparation.run(cancel, timing_tx),
        ))
        .await
        .map_err(|_| unavailable())?;
        let mut pending = PendingClaim {
            transfers: transfers.clone(),
            id: Some(ready.id),
        };
        let result = async {
            let (remaining_seek, liveness) = timing_rx.await.map_err(|_| unavailable())?;
            let after = Box::pin(context.origin.snapshot(
                &self.pool,
                &self.locker,
                request.channel_id,
                &context.media.item,
                audio,
            ))
            .await?;
            if after.identity.descriptor != before.identity.descriptor
                || after.prepared
                || after.retained
                || !self.dynamic_contexts.lock().await.contains_key(&instance)
            {
                return Err(unavailable());
            }
            let source =
                self.producer_source(instance, ready.id, &ready.probe, liveness, remaining_seek)?;
            let subtitle = if let Some(selected) = image.as_ref() {
                Some(producer_images::project(
                    selected,
                    &ready.probe,
                    source.clone(),
                )?)
            } else {
                subtitle
            };
            let mut item = install_fresh_tracks(
                before.projection.item,
                &ready.probe,
                &context.previous_audio,
                audio,
                source,
                None,
                now,
            )?;
            item.tracks.as_mut().ok_or_else(unavailable)?.subtitle = subtitle;
            Ok(item)
        }
        .await;
        if result.is_err() {
            transfers
                .cancel(ready.id)
                .await
                .map_err(|_| unavailable())?;
        } else {
            pending.id = None;
        }
        result
    }
    fn producer_source(
        &self,
        instance: uuid::Uuid,
        id: uuid::Uuid,
        probe: &crate::ffprobe::FfprobeMediaVersion,
        liveness: RemoteStreamLiveness,
        remaining_seek: std::time::Duration,
    ) -> Result<protocol::Source> {
        let hint = super::probe_hint::producer(probe).map_err(|_| unavailable())?;
        let mut source = self.media.prepared_source(
            instance,
            id,
            hint,
            liveness == RemoteStreamLiveness::Live,
        )?;
        if !remaining_seek.is_zero() {
            let protocol::Source::Http { in_point_ms, .. } = &mut source else {
                return Err(unavailable());
            };
            *in_point_ms =
                Some(u64::try_from(remaining_seek.as_millis()).map_err(|_| unavailable())?);
        }
        Ok(source)
    }
}

async fn producer_subtitles(
    projection: &super::selection::DeferredProjection,
    media_path: &str,
    now: DateTime<Utc>,
    deadline: Instant,
) -> Result<(
    Option<crate::playback::PlaybackSubtitle>,
    Option<protocol::Track>,
)> {
    let image = projection.selected_subtitle.as_ref().filter(|subtitle| {
        subtitle.is_image() && subtitle.subtitle_kind == crate::metadata::subtitle_kind::EMBEDDED
    });
    if let Some(image) = image {
        Ok((Some(image.clone()), None))
    } else {
        Ok((
            None,
            producer_subtitle(projection, media_path, now, deadline).await?,
        ))
    }
}

async fn producer_subtitle(
    projection: &super::selection::DeferredProjection,
    media_path: &str,
    now: DateTime<Utc>,
    deadline: Instant,
) -> Result<Option<protocol::Track>> {
    let timed =
        super::dynamic_item::resolve_at(&projection.item, now).map_err(|_| unavailable())?;
    let Some(selected) = projection.selected_subtitle.as_ref() else {
        return Ok(None);
    };
    // Legacy extracted caches are item/index keyed, not source/version bound.
    if selected.subtitle_kind == crate::metadata::subtitle_kind::EMBEDDED {
        return Err(RepositoryError::unavailable(
            "Next producer subtitles",
            "selected embedded text subtitles require an independent source/version-bound artifact",
        ));
    }
    let input = crate::playback::subtitle_input_for_stream(
        selected,
        media_path,
        ersatzrs_ffmpeg::OutputFormat::MpegTs,
        ersatzrs_domain::StreamingMode::HttpLiveStreamingSegmenter,
    )
    .ok_or_else(unavailable)?;
    let mut track = timed
        .tracks
        .and_then(|tracks| tracks.subtitle)
        .ok_or_else(unavailable)?;
    if input.path.starts_with("http://") || input.path.starts_with("https://") {
        return Ok(Some(track));
    }
    let path = tokio::time::timeout_at(deadline, async {
        let path = tokio::fs::canonicalize(&input.path)
            .await
            .map_err(|_| unavailable())?;
        if !tokio::fs::metadata(&path)
            .await
            .map_err(|_| unavailable())?
            .is_file()
        {
            return Err(unavailable());
        }
        let file = tokio::fs::File::open(&path)
            .await
            .map_err(|_| unavailable())?;
        if !file.metadata().await.map_err(|_| unavailable())?.is_file() {
            return Err(unavailable());
        }
        dunce::simplified(&path)
            .to_str()
            .map(str::to_owned)
            .ok_or_else(unavailable)
    })
    .await
    .map_err(|_| unavailable())??;
    let Some(
        protocol::Source::Local {
            in_point_ms,
            out_point_ms,
            ..
        }
        | protocol::Source::Http {
            in_point_ms,
            out_point_ms,
            ..
        },
    ) = track.source
    else {
        return Err(unavailable());
    };
    track.source = Some(protocol::Source::Local {
        path,
        in_point_ms,
        out_point_ms,

        probe_hint: None,
    });
    Ok(Some(track))
}

struct ProducerPreparation {
    pool: crate::db::DbPool,
    media_id: i64,
    ffmpeg: String,
    ffprobe: String,
    number: String,
    max_height: Option<u32>,
    seek: std::time::Duration,
    deadline: Instant,
}

impl ProducerPreparation {
    async fn run(
        self,
        mut cancellation: oneshot::Receiver<()>,
        timing: oneshot::Sender<(std::time::Duration, RemoteStreamLiveness)>,
    ) -> std::result::Result<PreparedTransferMedia, RemoteStreamError> {
        let runtime = crate::addons::AddonRuntimeContext {
            ffmpeg_path: Some(self.ffmpeg.clone()),
            ffprobe_path: Some(self.ffprobe.clone()),
        };
        let root = crate::file_system_layout::addon_processed_media_folder();
        let preflight = crate::addon_media_list_playback::resolve_persisted_remote_stream(
            &self.pool,
            self.media_id,
            &runtime,
            &root,
        );
        tokio::pin!(preflight);
        let stream = tokio::select! {
            biased;
            _=&mut cancellation=>return Err(RemoteStreamError::PreparedCancelled),
            ()=tokio::time::sleep_until(self.deadline)=>return Err(RemoteStreamError::PreparedTimeout),
            result=&mut preflight=>result.map_err(|_|RemoteStreamError::PreparedProducerFailed)?,
        };
        let stream = if let Some(height) = self.max_height.and_then(|h| u16::try_from(h).ok()) {
            crate::remote_stream::with_remote_stream_video_height_ceiling(&stream, height)
        } else {
            stream
        };
        let RemoteStreamSource::Addon(command) = stream.source else {
            return Err(RemoteStreamError::PreparedProducerFailed);
        };
        let (command, remaining) = command.with_playback_seek(stream.liveness, self.seek);
        let (cancel, receiver) = oneshot::channel();
        let startup = PreparedRemoteStreamInput::start_without_request(
            &command,
            RemoteStreamSpawnContext::hls(self.number),
            self.deadline,
            receiver,
        );
        tokio::pin!(startup);
        let input = tokio::select! {
            biased;
            _=&mut cancellation=> {
                let _=cancel.send(());
                match startup.await {
                    Ok(input)=>input.cancel().await?,
                    Err(RemoteStreamError::PreparedTerminationUnconfirmed)=>return Err(RemoteStreamError::PreparedTerminationUnconfirmed),
                    Err(_)=>{},
                }
                return Err(RemoteStreamError::PreparedCancelled);
            },
            result=&mut startup=>result?,
        };
        let extractor = FfprobeExtractor::new(self.ffprobe);
        let probe = probe_owned(&extractor, &input, self.deadline, &mut cancellation).await;
        match probe {
            Ok(probe) => {
                let _ = timing.send((remaining, stream.liveness));
                Ok(PreparedTransferMedia { input, probe })
            }
            Err(error) => {
                input.cancel().await?;
                Err(error)
            }
        }
    }
}

async fn probe_owned(
    extractor: &FfprobeExtractor,
    input: &PreparedRemoteStreamInput,
    deadline: Instant,
    cancellation: &mut oneshot::Receiver<()>,
) -> std::result::Result<crate::ffprobe::FfprobeMediaVersion, RemoteStreamError> {
    let (cancel, receiver) = oneshot::channel();
    let operation =
        extractor.extract_producer_prefix_cancellable(input.prefix(), deadline, receiver);
    tokio::pin!(operation);
    tokio::select! {
        biased;
        _=cancellation=>{let _=cancel.send(());let _=operation.await;Err(RemoteStreamError::PreparedCancelled)},
        result=&mut operation=>result.map_err(|_|RemoteStreamError::PreparedProducerFailed),
    }
}

fn unavailable() -> RepositoryError {
    RepositoryError::unavailable(
        "Next producer playback",
        "producer preparation or publication unavailable",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{next_engine::selection::DeferredProjection, playback::PlaybackSubtitle};

    fn projection(path: &str) -> DeferredProjection {
        let start = Utc::now();
        DeferredProjection {
            selected_subtitle: Some(PlaybackSubtitle {
                id: 1,
                subtitle_kind: crate::metadata::subtitle_kind::SIDECAR,
                title: None,
                stream_index: 0,
                codec: Some("subrip".into()),
                default: true,
                forced: false,
                sdh: false,
                language: Some("nld".into()),
                is_extracted: false,
                path: Some(path.into()),
            }),
            pending_burn: None,
            item: protocol::PlayoutItem {
                id: "fixture".into(),
                start,
                finish: start + chrono::Duration::seconds(30),
                watermark: None,
                source: None,
                tracks: Some(protocol::Tracks {
                    video: None,
                    audio: None,
                    subtitle: Some(protocol::Track {
                        source: Some(protocol::Source::Local {
                            path: path.into(),
                            in_point_ms: Some(6000),
                            out_point_ms: Some(36000),

                            probe_hint: None,
                        }),
                        stream_index: Some(0),
                    }),
                }),
            },
        }
    }

    #[tokio::test]
    async fn embedded_images_use_fresh_streams_while_embedded_text_cache_is_refused() {
        let mut projection = projection("unavailable-cache.srt");
        let now = projection.item.start;
        let deadline = Instant::now() + std::time::Duration::from_secs(5);
        let selected = projection.selected_subtitle.as_mut().unwrap();
        selected.subtitle_kind = crate::metadata::subtitle_kind::EMBEDDED;
        selected.is_extracted = true;
        let Err(error) = producer_subtitles(&projection, "media.mkv", now, deadline).await else {
            panic!("unbound embedded text cache must be refused");
        };
        assert!(error.to_string().contains(
            "selected embedded text subtitles require an independent source/version-bound artifact"
        ));
        projection.selected_subtitle.as_mut().unwrap().codec = Some("hdmv_pgs_subtitle".into());
        let (image, sidecar) = producer_subtitles(&projection, "media.mkv", now, deadline)
            .await
            .unwrap();
        assert!(image.is_some());
        assert!(sidecar.is_none());
    }

    #[tokio::test]
    async fn producer_subtitle_preserves_absolute_sidecar_clock_and_refuses_unresolved_selection() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("fixture.nl.srt");
        tokio::fs::write(&path, b"fixture").await.unwrap();
        let path = path.to_str().unwrap();
        let mut projection = projection(path);
        let now = projection.item.start + chrono::Duration::seconds(2);
        let subtitle = producer_subtitle(
            &projection,
            path,
            now,
            Instant::now() + std::time::Duration::from_secs(5),
        )
        .await
        .unwrap()
        .unwrap();
        let Some(protocol::Source::Local {
            path: actual,
            in_point_ms,
            out_point_ms,
            ..
        }) = subtitle.source
        else {
            panic!("sidecar source lost")
        };
        assert_eq!(actual, path);
        assert_eq!(in_point_ms, Some(8000));
        assert_eq!(out_point_ms, Some(36000));
        projection.selected_subtitle.as_mut().unwrap().path = Some("fixture.nl.srt".into());
        let relative = producer_subtitle(
            &projection,
            path,
            now,
            Instant::now() + std::time::Duration::from_secs(5),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(
            matches!(relative.source,Some(protocol::Source::Local {path:resolved,in_point_ms:Some(8000),..}) if resolved==path)
        );
        projection.selected_subtitle.as_mut().unwrap().path =
            Some("relative/fixture.nl.srt".into());
        assert!(producer_subtitle(
            &projection,
            path,
            now,
            Instant::now() + std::time::Duration::from_secs(5)
        )
        .await
        .is_err());
        projection.selected_subtitle.as_mut().unwrap().path =
            Some(root.path().to_str().unwrap().into());
        assert!(
            producer_subtitle(
                &projection,
                path,
                now,
                Instant::now() + std::time::Duration::from_secs(5)
            )
            .await
            .is_err(),
            "a directory is not a readable sidecar file"
        );
        projection.selected_subtitle.as_mut().unwrap().path = Some(path.into());
        projection.selected_subtitle.as_mut().unwrap().subtitle_kind =
            crate::metadata::subtitle_kind::EMBEDDED;
        assert!(producer_subtitle(
            &projection,
            path,
            now,
            Instant::now() + std::time::Duration::from_secs(5)
        )
        .await
        .is_err());
        projection.selected_subtitle.as_mut().unwrap().subtitle_kind =
            crate::metadata::subtitle_kind::SIDECAR;
        projection.item.tracks.as_mut().unwrap().subtitle = None;
        assert!(
            producer_subtitle(
                &projection,
                path,
                now,
                Instant::now() + std::time::Duration::from_secs(5)
            )
            .await
            .is_err(),
            "selected subtitle may not disappear"
        );
        projection.selected_subtitle = None;
        assert!(producer_subtitle(
            &projection,
            path,
            now,
            Instant::now() + std::time::Duration::from_secs(5)
        )
        .await
        .unwrap()
        .is_none());
    }
}

#[path = "producer_images.rs"]
mod producer_images;
