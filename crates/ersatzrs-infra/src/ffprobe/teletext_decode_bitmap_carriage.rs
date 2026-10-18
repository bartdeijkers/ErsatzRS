//! Bounded source-bound bitmap preparation, without delivery or seek admission.
use std::{fmt, path::Path, sync::Arc};

use ersatzrs_core::locking::TeletextWorkHold;
use sha2::{Digest as _, Sha256};
use tokio::time::Instant;

use super::{
    check_deadline, local, local_teletext, probe_error, DecodedLocalTeletextPage, FfprobeCommand,
    FfprobeExtractor, LocalProbeCleanupScope, SourceTeletextPageDecodeError,
    SourceTeletextPageDecodeRequest,
};

#[path = "teletext_decode_bitmap_carriage/parser.rs"]
mod parser;
use parser::parse;
pub use parser::{SourceTeletextBitmapArtifactGroup, SourceTeletextBitmapArtifactObservation};

/// An immutable prepared candidate retained with its exact decoded source.
///
/// This grants no Copy input, seek initializer, consumer readiness or playback
/// authority. Its private file is deleted when the last owner drops it. Source
/// bookends retain the documented observed-filesystem association limits.
pub struct SourceTeletextBitmapArtifact {
    source: Arc<DecodedLocalTeletextPage>,
    _file: tempfile::NamedTempFile,
    sha256: [u8; 32],
    byte_count: usize,
    caption_origin: crate::ffprobe::teletext_consumer_origin::NativeSourceConsumerOrigin,
    caption_origin_microseconds: i64,
    observation: SourceTeletextBitmapArtifactObservation,
}
impl fmt::Debug for SourceTeletextBitmapArtifact {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SourceTeletextBitmapArtifact")
            .field("byte_count", &self.byte_count)
            .field(
                "caption_origin_microseconds",
                &self.caption_origin_microseconds,
            )
            .field("observation", &self.observation)
            .finish_non_exhaustive()
    }
}
impl SourceTeletextBitmapArtifact {
    #[must_use]
    pub fn request(&self) -> &SourceTeletextPageDecodeRequest {
        self.source.request()
    }
    #[must_use]
    pub const fn byte_count(&self) -> usize {
        self.byte_count
    }
    #[must_use]
    pub const fn sha256(&self) -> &[u8; 32] {
        &self.sha256
    }
    /// Caption-only native presentation zero; not the format or AV origin.
    /// Original native start PTS/time-base observations, kept separate from artifact coordinates.
    #[must_use]
    pub const fn caption_origin(
        &self,
    ) -> &crate::ffprobe::teletext_consumer_origin::NativeSourceConsumerOrigin {
        &self.caption_origin
    }
    #[must_use]
    pub const fn caption_origin_microseconds(&self) -> i64 {
        self.caption_origin_microseconds
    }
    #[must_use]
    pub const fn observation(&self) -> &SourceTeletextBitmapArtifactObservation {
        &self.observation
    }
}

impl FfprobeExtractor {
    /// Prepare a finite exact-page DVB-subtitle candidate from the original source.
    ///
    /// Current qualification requires a decoder snapshot covering every source
    /// byte and endpoint coordinates exactly representable in Matroska's 1 ms
    /// time base. These are explicit remaining coverage/precision gaps, not
    /// permanent source limits. No active-cue-at-seek or actual output is claimed.
    /// Source/process/file workers share the original bounded deadline and hold.
    /// # Errors
    /// Rejects partial/stale sources, missing scope/origin, malformed or lost
    /// events, ambiguous endpoint quantization, deadline or uncertain cleanup.
    pub async fn prepare_local_teletext_bitmap_artifact(
        &self,
        source: Arc<DecodedLocalTeletextPage>,
        deadline: Instant,
        hold: TeletextWorkHold,
    ) -> Result<SourceTeletextBitmapArtifact, SourceTeletextPageDecodeError> {
        check_deadline(deadline)?;
        if LocalProbeCleanupScope::current().is_none() {
            return Err(SourceTeletextPageDecodeError::NoCleanupScope);
        }
        source.request.validate()?;
        if !source.window().covers_entire_source_bytes() {
            return Err(SourceTeletextPageDecodeError::InvalidTransportWindow);
        }
        source.check_source(deadline, hold.clone()).await?;
        let source_clock = source.observe_source_clock(deadline, hold.clone()).await?;
        let origin = self
            .observe_local_teletext_consumer_origin(
                &source,
                &[source.request.stream_index],
                deadline,
                hold.clone(),
            )
            .await?;
        let streams = origin.streams();
        if streams.len() != 1
            || streams[0].index() != source.request.stream_index
            || streams[0].pid() != source.request.pid
            || streams[0].codec_type() != "subtitle"
        {
            return Err(SourceTeletextPageDecodeError::StreamChanged);
        }
        let (first, last) = origin
            .timing(0)
            .ok_or(SourceTeletextPageDecodeError::InvalidOutput)?
            .presentation_zero_bounds_microseconds();
        if first != last {
            return Err(SourceTeletextPageDecodeError::InvalidOutput);
        }
        let command = bitmap_command(
            &ersatzrs_ffmpeg::ffmpeg_path(),
            &source.request,
            &source.resolved,
        )?;
        let bytes = local::capture_retained(
            command,
            "source teletext bitmap artifact".into(),
            Some(deadline),
            Some(local::ProbeRetention::new(
                hold.clone(),
                source.guard.clone(),
            )),
        )
        .await
        .map_err(probe_error)?;
        source.check_source(deadline, hold.clone()).await?;
        let current_clock = source.observe_source_clock(deadline, hold.clone()).await?;
        if source_clock.source_sha256() != current_clock.source_sha256() {
            return Err(SourceTeletextPageDecodeError::SourceChanged);
        }
        let observation = self
            .inspect_bitmap_artifact(&source, bytes.clone(), first, deadline, hold.clone())
            .await?;
        let sha256 = Sha256::digest(&bytes).into();
        let byte_count = bytes.len();
        let file = local_teletext::write_bitmap_snapshot_owned(
            bytes,
            source.guard.clone(),
            (source.original.clone(), source.resolved.clone()),
            deadline,
            hold.clone(),
        )
        .await
        .map_err(probe_error)?;
        source.check_source(deadline, hold).await?;
        Ok(SourceTeletextBitmapArtifact {
            source,
            _file: file,
            sha256,
            byte_count,
            caption_origin: origin,
            caption_origin_microseconds: first,
            observation,
        })
    }

    async fn inspect_bitmap_artifact(
        &self,
        source: &DecodedLocalTeletextPage,
        bytes: Vec<u8>,
        origin: i64,
        deadline: Instant,
        hold: TeletextWorkHold,
    ) -> Result<SourceTeletextBitmapArtifactObservation, SourceTeletextPageDecodeError> {
        let file = local_teletext::write_bitmap_snapshot_owned(
            bytes,
            source.guard.clone(),
            (source.original.clone(), source.resolved.clone()),
            deadline,
            hold.clone(),
        )
        .await
        .map_err(probe_error)?;
        let command = artifact_probe_command(&self.ffprobe_path, file.path())?;
        let output = local::capture_retained(
            command,
            "source teletext bitmap artifact probe".into(),
            Some(deadline),
            Some(local::ProbeRetention::new(hold, source.guard.clone()).with_snapshot(file)),
        )
        .await
        .map_err(probe_error)?;
        check_deadline(deadline)?;
        parse(&output, source.events(), origin)
    }
}

fn bitmap_command(
    program: &str,
    request: &SourceTeletextPageDecodeRequest,
    source: &Path,
) -> Result<FfprobeCommand, SourceTeletextPageDecodeError> {
    let path = source
        .to_str()
        .ok_or(SourceTeletextPageDecodeError::SourceChanged)?;
    let stream = request.stream_index;
    let mut args = ersatzrs_ffmpeg::standard_format_flags()
        .global_options()
        .to_vec();
    args.extend([
        "-nostdin".into(),
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-threads".into(),
        "1".into(),
        format!("-txt_page:{stream}"),
        request.page.get().to_string(),
        format!("-txt_format:{stream}"),
        "bitmap".into(),
        format!("-txt_default_region:{stream}"),
        "-1".into(),
        format!("-fix_sub_duration:{stream}"),
        "-i".into(),
        path.into(),
        "-map".into(),
        format!("0:{stream}"),
        "-c:s".into(),
        "dvbsub".into(),
        "-f".into(),
        "matroska".into(),
        "pipe:1".into(),
    ]);
    Ok(FfprobeCommand {
        program: program.into(),
        args,
    })
}
fn artifact_probe_command(
    program: &str,
    path: &Path,
) -> Result<FfprobeCommand, SourceTeletextPageDecodeError> {
    let path = path
        .to_str()
        .ok_or(SourceTeletextPageDecodeError::InvalidOutput)?;
    Ok(FfprobeCommand { program: program.into(), args: vec![
        "-v".into(), "error".into(), "-show_streams".into(), "-show_format".into(),
        "-show_packets".into(), "-show_frames".into(), "-show_entries".into(),
        "stream=index,codec_name,codec_type,time_base:format=format_name,nb_streams:packet=codec_type,stream_index,pts,dts,duration,flags:frame=media_type,pts,format,start_display_time,end_display_time,num_rects".into(),
        "-of".into(), "json".into(), path.into(),
    ] })
}

#[cfg(test)]
#[path = "teletext_decode_bitmap_carriage/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "teletext_decode_bitmap_carriage/native_tests.rs"]
mod native_tests;
