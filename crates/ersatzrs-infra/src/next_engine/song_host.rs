//! Session-owned finite PNG producers for authenticated song occurrences.

use std::collections::BTreeMap;

use ersatzrs_core::repositories::{RepositoryError, Result};
use ersatzrs_domain::{MediaStream, MediaStreamKind, MediaVersion};
use tokio::{sync::oneshot, time::Instant};
use uuid::Uuid;

use crate::{
    db::DbPool,
    ffprobe::FfprobeMediaVersion,
    remote_stream::{
        PreparedRemoteStreamInput, RemoteStreamCommand, RemoteStreamError, RemoteStreamSpawnContext,
    },
};

use super::{
    canvas_host::OccurrenceSession,
    host::NextHost,
    session_transfers::{PreparedTransferMedia, TransferBinding, TransferReader},
    song_background::SongBinding,
};

impl NextHost {
    /// Check the exact active song occurrence before admitting rendering work.
    /// The HTTP boundary must authenticate the purpose-specific ticket first.
    /// # Errors
    /// Rejects changed, expired, stopped, cross-session or non-song bindings.
    pub async fn validate_song_session(
        &self,
        instance: Uuid,
        binding: &SongBinding,
        channel_number: Option<&str>,
    ) -> Result<()> {
        let session = self.song_session(instance, binding).await?;
        if channel_number.is_some_and(|number| number != session.request.number) {
            return Err(unavailable());
        }
        Ok(())
    }

    async fn song_session(
        &self,
        instance: Uuid,
        binding: &SongBinding,
    ) -> Result<OccurrenceSession> {
        let session = self
            .occurrence_session(instance, &binding.occurrence)
            .await?;
        if binding.width <= 0
            || binding.height <= 0
            || session.profile.resolution.width != binding.width
            || session.profile.resolution.height != binding.height
            || session.channel.song_video_mode != binding.mode
        {
            return Err(unavailable());
        }
        let song: i64 = sqlx::query_scalar(r#"SELECT EXISTS(SELECT 1 FROM "Song" WHERE "Id"=?)"#)
            .bind(session.media.item.media_item_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|_| unavailable())?;
        if song == 0 {
            return Err(unavailable());
        }
        Ok(session)
    }

    /// Generate one PNG under the existing session transfer supervisor.
    /// # Errors
    /// Refuses stale admission and awaits failed preparation cleanup.
    pub async fn render_song_background(
        &self,
        instance: Uuid,
        binding: &SongBinding,
    ) -> Result<TransferReader> {
        let session = self.song_session(instance, binding).await?;
        let transfers = session.transfers.clone();
        let pool = self.pool.clone();
        let seed = if session.troubleshooting.is_some() {
            preview_seed(instance)
        } else {
            session.media.item.id
        };
        let deadline = Instant::now() + std::time::Duration::from_secs(15);
        let ready = transfers
            .prepare(
                TransferBinding {
                    media_item_id: binding.occurrence.media,
                    playout_item_id: binding.occurrence.item,
                    media_version_id: binding.occurrence.version,
                    media_file_id: binding.occurrence.file,
                    publication: "song background".into(),
                    start: binding.occurrence.start,
                    finish: binding.occurrence.finish,
                },
                deadline,
                deadline + std::time::Duration::from_secs(5),
                move |cancellation| prepare(pool, session, seed, deadline, cancellation),
            )
            .await
            .map_err(|_| unavailable())?;
        let mut pending = super::retained_host::PendingClaim {
            transfers: transfers.clone(),
            id: Some(ready.id),
        };
        let result = async {
            self.song_session(instance, binding).await?;
            self.claim_prepared(instance, ready.id)
                .await
                .map_err(|_| unavailable())
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
}

async fn prepare(
    pool: DbPool,
    session: OccurrenceSession,
    seed: i64,
    deadline: Instant,
    mut cancellation: oneshot::Receiver<()>,
) -> std::result::Result<PreparedTransferMedia, RemoteStreamError> {
    let directory = tempfile::Builder::new()
        .prefix("ersatzrs-next-song-")
        .tempdir()
        .map_err(|_| RemoteStreamError::PreparedProducerFailed)?;
    let recipe = tokio::select! {
        biased;
        _ = &mut cancellation => return Err(RemoteStreamError::PreparedCancelled),
        () = tokio::time::sleep_until(deadline) => return Err(RemoteStreamError::PreparedTimeout),
        recipe = crate::song_background::prepare(
            &pool, session.media.item.media_item_id, &session.profile.resolution,
            session.channel.song_video_mode, Some(seed), directory.path(), &session.request.ffmpeg,
        ) => recipe.map_err(|_| RemoteStreamError::PreparedProducerFailed)?,
    };
    // A failed prepared start removes its directory after confirmed reaping.
    // Stage the private fallback first, under the same cancellation/deadline.
    let fallback = if let Some(command) = recipe.fallback_command {
        Some(tokio::select! {
            biased;
            _ = &mut cancellation => return Err(RemoteStreamError::PreparedCancelled),
            () = tokio::time::sleep_until(deadline) => return Err(RemoteStreamError::PreparedTimeout),
            result = stage_fallback(command, directory.path()) => result?,
        })
    } else {
        None
    };
    let input = start_with_fallback(
        &recipe.command,
        &session.request.number,
        deadline,
        &mut cancellation,
        directory,
        fallback,
    )
    .await?;
    Ok(PreparedTransferMedia {
        input,
        probe: FfprobeMediaVersion {
            version: MediaVersion {
                duration: session.media.item.finish - session.media.item.start,
                width: session.profile.resolution.width,
                height: session.profile.resolution.height,
                sample_aspect_ratio: Some("1:1".into()),
                ..MediaVersion::default()
            },
            streams: vec![MediaStream {
                index: 0,
                kind: MediaStreamKind::Video,
                codec: Some("png".into()),
                pixel_format: Some("rgb24".into()),
                bits_per_raw_sample: 8,
                ..MediaStream::default()
            }],
            field_orders: BTreeMap::new(),
            chapters: Vec::new(),
            format_metadata: BTreeMap::new(),
            format_name: Some("png_pipe".into()),
        },
    })
}

async fn stage_fallback(
    mut command: RemoteStreamCommand,
    source: &std::path::Path,
) -> std::result::Result<(RemoteStreamCommand, tempfile::TempDir), RemoteStreamError> {
    let directory = tempfile::Builder::new()
        .prefix("ersatzrs-next-song-fallback-")
        .tempdir()
        .map_err(|_| RemoteStreamError::PreparedProducerFailed)?;
    for name in [
        "default-cover.png",
        "background.png",
        "song.ass",
        "font.otf",
    ] {
        tokio::fs::copy(source.join(name), directory.path().join(name))
            .await
            .map_err(|_| RemoteStreamError::PreparedProducerFailed)?;
    }
    command.working_directory = directory.path().to_path_buf();
    Ok((command, directory))
}

async fn start_with_fallback(
    command: &RemoteStreamCommand,
    number: &str,
    deadline: Instant,
    cancellation: &mut oneshot::Receiver<()>,
    directory: tempfile::TempDir,
    fallback: Option<(RemoteStreamCommand, tempfile::TempDir)>,
) -> std::result::Result<PreparedRemoteStreamInput, RemoteStreamError> {
    match start_png(command, number, deadline, cancellation, directory).await {
        Ok(input) => Ok(input),
        Err(RemoteStreamError::PreparedProducerFailed) => {
            let Some((command, directory)) = fallback else {
                return Err(RemoteStreamError::PreparedProducerFailed);
            };
            if Instant::now() >= deadline {
                return Err(RemoteStreamError::PreparedTimeout);
            }
            if !matches!(
                cancellation.try_recv(),
                Err(oneshot::error::TryRecvError::Empty)
            ) {
                return Err(RemoteStreamError::PreparedCancelled);
            }
            start_png(&command, number, deadline, cancellation, directory).await
        }
        Err(error) => Err(error),
    }
}

async fn start_png(
    command: &RemoteStreamCommand,
    number: &str,
    deadline: Instant,
    cancellation: &mut oneshot::Receiver<()>,
    directory: tempfile::TempDir,
) -> std::result::Result<PreparedRemoteStreamInput, RemoteStreamError> {
    let (cancel, receiver) = oneshot::channel();
    let start = PreparedRemoteStreamInput::start_with_graphics(
        command,
        RemoteStreamSpawnContext::hls(number.to_owned()),
        deadline,
        receiver,
        directory,
        Vec::new(),
    );
    tokio::pin!(start);
    tokio::select! {
        biased;
        _ = cancellation => {
            let _ = cancel.send(());
            match start.await {
                Ok(input) => { input.cancel().await?; }
                Err(RemoteStreamError::PreparedTerminationUnconfirmed) => {
                    return Err(RemoteStreamError::PreparedTerminationUnconfirmed);
                }
                Err(_) => {}
            }
            Err(RemoteStreamError::PreparedCancelled)
        }
        result = &mut start => result,
    }
}

// One stable choice for each owning preview, without a global cache or RNG.
fn preview_seed(instance: Uuid) -> i64 {
    let mut bytes = [0_u8; 8];
    bytes.copy_from_slice(&instance.as_bytes()[..8]);
    bytes[7] &= 0x7f;
    i64::from_le_bytes(bytes)
}

fn unavailable() -> RepositoryError {
    RepositoryError::unavailable(
        "Next song background",
        "session or occurrence is unavailable",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transient_background_seed_is_stable_within_its_owning_session() {
        let first = Uuid::from_bytes([1; 16]);
        let second = Uuid::from_bytes([2; 16]);
        assert_eq!(preview_seed(first), preview_seed(first));
        assert_ne!(preview_seed(first), preview_seed(second));
        assert!(preview_seed(first) >= 0);
        assert!(preview_seed(Uuid::from_bytes([255; 16])) >= 0);
    }
}

#[cfg(test)]
#[path = "song_host_tests.rs"]
mod runtime_tests;
