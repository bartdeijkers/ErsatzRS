//! Complete finite text from a held exact page, using existing owned process/read work.

use std::io::Write as _;
use std::{path::Path, sync::Arc};

use ersatzrs_core::locking::TeletextWorkHold;
use tokio::time::Instant;

use super::{
    check_deadline, local, local_teletext, probe_error, FfprobeCommand, FfprobeExtractor,
    LocalProbeCleanupScope, PreparedLocalTeletextSource, SourceTeletextPageDecodeError,
};

const MAX_TEXT_BYTES: usize = 8 * 1024 * 1024;

#[path = "teletext_decode_text/qualification.rs"]
mod qualification;

/// Complete finite-source text observation, never a default track selection.
pub(crate) enum SourceTeletextTextPreparation {
    Ready(SourceTeletextTextArtifact),
    /// No requested header occurred in the complete validated held-source copy.
    /// Callers must compare actual PSI and recheck SQL/source before any fallback.
    Absent {
        psi_fingerprint: [u8; 32],
        source: Arc<PreparedLocalTeletextSource>,
    },
}

/// Readonly qualification, not permission to change a selection or publish it.
pub(crate) struct SourceTeletextTextArtifact {
    pub(crate) content: String,
    pub(crate) psi_fingerprint: [u8; 32],
    pub(crate) source: Arc<PreparedLocalTeletextSource>,
}

impl SourceTeletextTextArtifact {
    pub(crate) async fn write(
        self,
        mut file: std::fs::File,
        deadline: Instant,
        hold: TeletextWorkHold,
    ) -> Result<(), SourceTeletextPageDecodeError> {
        local_teletext::run_source_guard_owned(deadline, hold, move |stop| {
            for chunk in self.content.as_bytes().chunks(64 * 1024) {
                stop.check_source_guard()?;
                file.write_all(chunk)
                    .map_err(|_| super::FfprobeError::InvalidOutput {
                        path: "finite source teletext text".into(),
                    })?;
            }
            file.flush()
                .map_err(|_| super::FfprobeError::InvalidOutput {
                    path: "finite source teletext text".into(),
                })?;
            stop.check_source_guard()?;
            // Keep the original association alive until the actual writer exits.
            drop(self.source);
            Ok(())
        })
        .await
        .map_err(probe_error)
    }
}

impl FfprobeExtractor {
    /// Decode only the selected page from a complete original-byte snapshot.
    ///
    /// The original full-source format origin remains held separately. `FFmpeg`'s
    /// `copyts/start_at_zero` normalizes that complete source timeline; snapshots
    /// with omitted bytes cannot substitute a different demux origin. No bitmap
    /// decoder pass or inferred native-AV clock model is involved.
    pub(crate) async fn prepare_local_teletext_text(
        &self,
        source: Arc<PreparedLocalTeletextSource>,
        ffmpeg: &Path,
        video_stream_index: Option<u32>,
        deadline: Instant,
        hold: TeletextWorkHold,
    ) -> Result<SourceTeletextTextPreparation, SourceTeletextPageDecodeError> {
        check_deadline(deadline)?;
        if LocalProbeCleanupScope::current().is_none() {
            return Err(SourceTeletextPageDecodeError::NoCleanupScope);
        }
        if source.native_origin().format_name() != Some("mpegts")
            || source.native_origin().format_start().is_none()
        {
            return Err(SourceTeletextPageDecodeError::InvalidOutput);
        }
        let captured = local_teletext::capture_complete_text_source_owned(
            source.guard.clone(),
            (source.original.clone(), source.resolved.clone()),
            source.request.clone(),
            deadline,
            hold.clone(),
        )
        .await
        .map_err(probe_error)?;
        if !captured.selected_page_observed {
            return Ok(SourceTeletextTextPreparation::Absent {
                psi_fingerprint: captured.psi_fingerprint,
                source,
            });
        }
        let snapshot = Arc::new(captured.snapshot);
        let frames = local::capture_retained(
            qualification::command(&self.ffprobe_path, &source.request, snapshot.path())?,
            "finite source teletext text events".into(),
            Some(deadline),
            Some(
                local::ProbeRetention::new(hold.clone(), source.guard.clone())
                    .with_shared_snapshot(snapshot.clone()),
            ),
        )
        .await
        .map_err(probe_error)?;
        let command = text_command(ffmpeg, &source.request, snapshot.path())?;
        let bytes = local::capture_retained(
            command,
            "finite source teletext text".into(),
            Some(deadline),
            Some(
                local::ProbeRetention::new(hold.clone(), source.guard.clone())
                    .with_shared_snapshot(snapshot.clone()),
            ),
        )
        .await
        .map_err(probe_error)?;
        let needs_end = qualification::needs_eof_end(&frames)?;
        let mut content = if needs_end && bytes.is_empty() {
            String::new()
        } else {
            normalize(bytes)?
        };
        if needs_end {
            let index = video_stream_index.ok_or(SourceTeletextPageDecodeError::InvalidOutput)?;
            let video = local::capture_retained(
                qualification::video_command(&self.ffprobe_path, index, snapshot.path())?,
                "finite source teletext selected video end".into(),
                Some(deadline),
                Some(
                    local::ProbeRetention::new(hold.clone(), source.guard.clone())
                        .with_shared_snapshot(snapshot.clone()),
                ),
            )
            .await
            .map_err(probe_error)?;
            let end = qualification::video_end(&source, index, &video, deadline)?;
            let raw = local::capture_retained(
                raw_text_command(ffmpeg, &source.request, snapshot.path())?,
                "finite source teletext final text".into(),
                Some(deadline),
                Some(
                    local::ProbeRetention::new(hold.clone(), source.guard.clone())
                        .with_shared_snapshot(snapshot),
                ),
            )
            .await
            .map_err(probe_error)?;
            content = qualification::recover(
                &source,
                &frames,
                &content,
                &normalize(raw)?,
                end,
                deadline,
            )?;
        } else {
            qualification::check(&source, &frames, &content, deadline)?;
        }
        source.check_source(deadline, hold).await?;
        Ok(SourceTeletextTextPreparation::Ready(
            SourceTeletextTextArtifact {
                content,
                psi_fingerprint: captured.psi_fingerprint,
                source,
            },
        ))
    }
}

fn normalize(bytes: Vec<u8>) -> Result<String, SourceTeletextPageDecodeError> {
    crate::subtitles::rebase_text_cues_milliseconds(
        bytes,
        ersatzrs_ffmpeg::SubtitleTextFormat::Srt,
        0,
        MAX_TEXT_BYTES,
    )
    .map_err(|_| SourceTeletextPageDecodeError::InvalidOutput)?
    .ok_or(SourceTeletextPageDecodeError::InvalidOutput)
}

fn raw_text_command(
    ffmpeg: &Path,
    request: &super::SourceTeletextPageDecodeRequest,
    snapshot: &Path,
) -> Result<FfprobeCommand, SourceTeletextPageDecodeError> {
    let mut command = text_command(ffmpeg, request, snapshot)?;
    let option = format!("-fix_sub_duration:{}", request.stream_index);
    command.args.retain(|arg| arg != &option);
    Ok(command)
}

fn text_command(
    ffmpeg: &Path,
    request: &super::SourceTeletextPageDecodeRequest,
    snapshot: &Path,
) -> Result<FfprobeCommand, SourceTeletextPageDecodeError> {
    let source = snapshot
        .to_str()
        .ok_or(SourceTeletextPageDecodeError::InvalidRequest)?;
    let program = ffmpeg
        .to_str()
        .ok_or(SourceTeletextPageDecodeError::InvalidRequest)?;
    let index = request.stream_index;
    Ok(FfprobeCommand {
        program: program.to_owned(),
        args: vec![
            "-nostdin".into(),
            "-hide_banner".into(),
            "-loglevel".into(),
            "error".into(),
            "-copyts".into(),
            "-start_at_zero".into(),
            format!("-txt_page:{index}"),
            request.page.get().to_string(),
            format!("-txt_format:{index}"),
            "text".into(),
            format!("-fix_sub_duration:{index}"),
            "-f".into(),
            "mpegts".into(),
            "-i".into(),
            source.to_owned(),
            "-map".into(),
            format!("0:{index}"),
            "-c:s".into(),
            "srt".into(),
            "-f".into(),
            "srt".into(),
            "pipe:1".into(),
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ersatzrs_domain::{
        MediaSourceKind, SourceTeletextBindingIdentity, SourceTeletextRevision, TeletextPageNumber,
    };

    #[test]
    fn finite_text_command_snapshots_exact_page_and_complete_source_clock_policy() {
        let request = super::super::SourceTeletextPageDecodeRequest {
            binding: SourceTeletextBindingIdentity::new(
                1,
                MediaSourceKind::Local,
                2,
                3,
                4,
                SourceTeletextRevision::new("synthetic-revision".into()).unwrap(),
            )
            .unwrap(),
            stream_index: 2,
            pid: 258,
            program_number: 1,
            page: TeletextPageNumber::new(889).unwrap(),
        };
        let command = text_command(
            Path::new("ffmpeg"),
            &request,
            Path::new("synthetic-source-window.ts"),
        )
        .unwrap();
        // ADR 0121: finite prepared text is page-qualified before Next sees a track.
        insta::assert_snapshot!(format!("{}\n{}",command.program,command.args.join("\n")),@r"
ffmpeg
-nostdin
-hide_banner
-loglevel
error
-copyts
-start_at_zero
-txt_page:2
889
-txt_format:2
text
-fix_sub_duration:2
-f
mpegts
-i
synthetic-source-window.ts
-map
0:2
-c:s
srt
-f
srt
pipe:1
");
        let raw =
            raw_text_command(Path::new("ffmpeg"), &request, Path::new("held-source.ts")).unwrap();
        insta::assert_snapshot!(raw.args.join("\n"), @r"
-nostdin
-hide_banner
-loglevel
error
-copyts
-start_at_zero
-txt_page:2
889
-txt_format:2
text
-f
mpegts
-i
held-source.ts
-map
0:2
-c:s
srt
-f
srt
pipe:1
");
    }
}
