//! Private zero-origin fragments, without source/AV clock or delivery authority.
use std::{
    fmt,
    io::{Read as _, Write as _},
    path::Path,
    sync::Arc,
};

use ersatzrs_core::locking::TeletextWorkHold;
use ersatzrs_ffmpeg::SourceBitmapConcat;
use sha2::{Digest as _, Sha256};
use tokio::time::Instant;

use super::{
    artifact_probe_command, check_deadline, local, local_teletext, parser, probe_error,
    FfprobeCommand, FfprobeExtractor, LocalProbeCleanupScope, SourceTeletextBitmapArtifact,
    SourceTeletextBitmapArtifactObservation, SourceTeletextPageDecodeError,
    SourceTeletextPageDecodeRequest,
};

type Result<T> = std::result::Result<T, SourceTeletextPageDecodeError>;

pub(super) struct OwnedFragment {
    pub(super) file: tempfile::NamedTempFile,
    pub(super) observation: SourceTeletextBitmapArtifactObservation,
    pub(super) sha256: [u8; 32],
    pub(super) byte_count: usize,
}
/// Source-associated fragments with an artifact-coordinate bootstrap recipe.
///
/// No locator, input, selected AV clock or delivery capability is exposed. The
/// recipe preserves an exact requested point, or a checked consumer projection
/// with its millisecond delay and residual bounds. Actual consuming-client
/// behavior remains a separate qualification obligation.
pub struct SourceTeletextBitmapFragments {
    pub(super) artifact: Arc<SourceTeletextBitmapArtifact>,
    pub(super) bootstrap: OwnedFragment,
    pub(super) following: Option<OwnedFragment>,
    pub(super) recipe: SourceBitmapConcat,
    coordinates: Coordinates,
    following_delay: Option<i64>,
}
impl fmt::Debug for SourceTeletextBitmapFragments {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SourceTeletextBitmapFragments")
            .field("coordinates", &self.coordinates)
            .field("following_delay_microseconds", &self.following_delay)
            .field("bootstrap", &self.bootstrap.observation)
            .field(
                "following",
                &self
                    .following
                    .as_ref()
                    .map(|fragment| &fragment.observation),
            )
            .finish_non_exhaustive()
    }
}
impl SourceTeletextBitmapFragments {
    #[must_use]
    pub fn request(&self) -> &SourceTeletextPageDecodeRequest {
        self.artifact.request()
    }
    #[must_use]
    pub const fn start_artifact_microseconds(&self) -> Option<i64> {
        match self.coordinates {
            Coordinates::Exact(start) => Some(start),
            Coordinates::Consumer(_) => None,
        }
    }
    #[must_use]
    pub const fn artifact_instep_bounds_microseconds(&self) -> (i64, i64) {
        match self.coordinates {
            Coordinates::Exact(start) => (start, start),
            Coordinates::Consumer(alignment) => alignment.artifact_instep_bounds_microseconds(),
        }
    }
    #[must_use]
    pub const fn consumer_alignment(
        &self,
    ) -> Option<crate::ffprobe::teletext_consumer_alignment::NativeSourceBitmapAlignment> {
        match self.coordinates {
            Coordinates::Exact(_) => None,
            Coordinates::Consumer(alignment) => Some(alignment),
        }
    }
    #[must_use]
    pub fn recipe_sha256(&self) -> [u8; 32] {
        Sha256::digest(self.recipe.render().as_bytes()).into()
    }
    #[must_use]
    pub const fn bootstrap_sha256(&self) -> &[u8; 32] {
        &self.bootstrap.sha256
    }
    #[must_use]
    pub const fn bootstrap_byte_count(&self) -> usize {
        self.bootstrap.byte_count
    }
    #[must_use]
    pub fn following_sha256(&self) -> Option<&[u8; 32]> {
        self.following.as_ref().map(|fragment| &fragment.sha256)
    }
    #[must_use]
    pub fn following_byte_count(&self) -> Option<usize> {
        self.following.as_ref().map(|fragment| fragment.byte_count)
    }
    #[must_use]
    pub const fn bootstrap_observation(&self) -> &SourceTeletextBitmapArtifactObservation {
        &self.bootstrap.observation
    }
    #[must_use]
    pub fn following_observation(&self) -> Option<&SourceTeletextBitmapArtifactObservation> {
        self.following
            .as_ref()
            .map(|fragment| &fragment.observation)
    }
    #[must_use]
    pub const fn following_delay_microseconds(&self) -> Option<i64> {
        self.following_delay
    }
}

#[derive(Debug, Clone, Copy)]
enum Coordinates {
    Exact(i64),
    Consumer(crate::ffprobe::teletext_consumer_alignment::NativeSourceBitmapAlignment),
}
#[path = "isolator_consumer.rs"]
mod consumer;

#[derive(Debug, PartialEq, Eq)]
struct Selection {
    bootstrap: i64,
    bootstrap_end: Option<i64>,
    following: Option<i64>,
    delay: Option<i64>,
}
fn invalid() -> SourceTeletextPageDecodeError {
    SourceTeletextPageDecodeError::InvalidOutput
}
fn select(observation: &SourceTeletextBitmapArtifactObservation, start: i64) -> Result<Selection> {
    let groups = observation.groups();
    if groups.is_empty() || groups.len() > 4096 {
        return Err(invalid());
    }
    let last = groups.last().ok_or_else(invalid)?;
    if last.clear_frames() == 0 || last.nonempty_frames() != 0 {
        return Err(invalid());
    }
    let following = groups.partition_point(|group| group.pts_microseconds() <= start);
    let bootstrap = if following == 0 {
        groups.len() - 1
    } else {
        following - 1
    };
    let following = groups.get(following).map(|group| group.pts_microseconds());
    let delay = following
        .map(|next| {
            next.checked_sub(start)
                .filter(|delay| *delay > 0)
                .ok_or_else(invalid)
        })
        .transpose()?;
    Ok(Selection {
        bootstrap: groups[bootstrap].pts_microseconds(),
        bootstrap_end: groups
            .get(bootstrap + 1)
            .map(|group| group.pts_microseconds()),
        following,
        delay,
    })
}

impl FfprobeExtractor {
    /// Isolate complete zero-origin groups for one exact artifact-coordinate start.
    ///
    /// Before the first event, the final explicit clear initializes blank state.
    /// Otherwise the latest complete group at or before the requested time is
    /// selected, including same-PTS clear/replacement order. A following fragment
    /// begins at the next group. This interprets no media-relative seek or AV
    /// origin, enables no delivery and retains no preparation slot during idle
    /// ownership. Actual workers retain source, input and work through cleanup.
    /// # Errors
    /// Rejects stale source/artifact, overflowing coordinates, changed payloads,
    /// omitted/reordered groups, deadline/cancellation or uncertain cleanup.
    pub async fn isolate_local_teletext_bitmap_artifact(
        &self,
        artifact: Arc<SourceTeletextBitmapArtifact>,
        start_artifact_microseconds: i64,
        deadline: Instant,
        hold: TeletextWorkHold,
    ) -> Result<SourceTeletextBitmapFragments> {
        check_deadline(deadline)?;
        let selection = select(artifact.observation(), start_artifact_microseconds)?;
        self.isolate_selected_bitmap_fragments(
            artifact,
            selection,
            Coordinates::Exact(start_artifact_microseconds),
            deadline,
            hold,
        )
        .await
    }

    async fn isolate_selected_bitmap_fragments(
        &self,
        artifact: Arc<SourceTeletextBitmapArtifact>,
        selection: Selection,
        coordinates: Coordinates,
        deadline: Instant,
        hold: TeletextWorkHold,
    ) -> Result<SourceTeletextBitmapFragments> {
        check_deadline(deadline)?;
        if LocalProbeCleanupScope::current().is_none() {
            return Err(SourceTeletextPageDecodeError::NoCleanupScope);
        }
        artifact.source.check_source(deadline, hold.clone()).await?;
        let clock = artifact
            .source
            .observe_source_clock(deadline, hold.clone())
            .await?;
        let bootstrap = self
            .isolate_fragment(
                artifact.clone(),
                (selection.bootstrap, selection.bootstrap_end),
                true,
                deadline,
                hold.clone(),
            )
            .await?;
        let following = if let Some(start) = selection.following {
            Some(
                self.isolate_fragment(
                    artifact.clone(),
                    (start, None),
                    false,
                    deadline,
                    hold.clone(),
                )
                .await?,
            )
        } else {
            None
        };
        let recipe = SourceBitmapConcat::new(
            bootstrap.file.path(),
            following
                .as_ref()
                .zip(selection.delay)
                .map(|(fragment, delay)| (fragment.file.path(), delay)),
        )
        .map_err(|_| invalid())?;
        artifact.source.check_source(deadline, hold.clone()).await?;
        let current = artifact.source.observe_source_clock(deadline, hold).await?;
        if clock.source_sha256() != current.source_sha256() {
            return Err(SourceTeletextPageDecodeError::SourceChanged);
        }
        check_deadline(deadline)?;
        Ok(SourceTeletextBitmapFragments {
            artifact,
            bootstrap,
            following,
            recipe,
            coordinates,
            following_delay: selection.delay,
        })
    }
    async fn isolate_fragment(
        &self,
        artifact: Arc<SourceTeletextBitmapArtifact>,
        range: (i64, Option<i64>),
        one_group: bool,
        deadline: Instant,
        hold: TeletextWorkHold,
    ) -> Result<OwnedFragment> {
        let input = copy_input(artifact.clone(), deadline, hold.clone()).await?;
        let command = fragment_command(
            &ersatzrs_ffmpeg::ffmpeg_path(),
            input.path(),
            range.0,
            one_group,
        )?;
        let bytes = local::capture_retained(
            command,
            "source teletext isolated bitmap group".into(),
            Some(deadline),
            Some(
                local::ProbeRetention::new(hold.clone(), artifact.source.guard.clone())
                    .with_snapshot(input),
            ),
        )
        .await
        .map_err(probe_error)?;
        let probe = local_teletext::write_bitmap_snapshot_owned(
            bytes.clone(),
            artifact.source.guard.clone(),
            (
                artifact.source.original.clone(),
                artifact.source.resolved.clone(),
            ),
            deadline,
            hold.clone(),
        )
        .await
        .map_err(probe_error)?;
        let command = artifact_probe_command(&self.ffprobe_path, probe.path())?;
        let output = local::capture_retained(
            command,
            "source teletext isolated bitmap probe".into(),
            Some(deadline),
            Some(
                local::ProbeRetention::new(hold.clone(), artifact.source.guard.clone())
                    .with_snapshot(probe),
            ),
        )
        .await
        .map_err(probe_error)?;
        let observation = parser::parse_fragment_observation(&output)?;
        parser::qualify_fragment(&observation, artifact.observation(), range.0, range.1)?;
        let sha256 = Sha256::digest(&bytes).into();
        let byte_count = bytes.len();
        let file = local_teletext::write_bitmap_snapshot_owned(
            bytes,
            artifact.source.guard.clone(),
            (
                artifact.source.original.clone(),
                artifact.source.resolved.clone(),
            ),
            deadline,
            hold,
        )
        .await
        .map_err(probe_error)?;
        check_deadline(deadline)?;
        Ok(OwnedFragment {
            file,
            observation,
            sha256,
            byte_count,
        })
    }
}

async fn copy_input(
    artifact: Arc<SourceTeletextBitmapArtifact>,
    deadline: Instant,
    hold: TeletextWorkHold,
) -> Result<tempfile::NamedTempFile> {
    local_teletext::run_source_guard_owned(deadline, hold, move |stop| {
        let invalid = || crate::ffprobe::FfprobeError::InvalidOutput {
            path: "source teletext bitmap artifact copy".into(),
        };
        let check = || {
            stop.check_source_guard()?;
            local_teletext::check_source_guard_paths(
                &artifact.source.guard,
                &artifact.source.original,
                &artifact.source.resolved,
            )
        };
        check()?;
        let mut input = artifact.file.reopen().map_err(|_| invalid())?;
        let mut output = tempfile::Builder::new()
            .prefix("ersatzrs-teletext-bitmap-copy-")
            .suffix(".mks")
            .tempfile()
            .map_err(|_| invalid())?;
        let mut digest = Sha256::new();
        let mut bytes = 0usize;
        let mut chunk = [0; 8192];
        loop {
            check()?;
            let count = input.read(&mut chunk).map_err(|_| invalid())?;
            if count == 0 {
                break;
            }
            bytes = bytes
                .checked_add(count)
                .filter(|bytes| *bytes <= crate::ffprobe::MAX_REMOTE_FFPROBE_OUTPUT_BYTES)
                .ok_or_else(invalid)?;
            digest.update(&chunk[..count]);
            output.write_all(&chunk[..count]).map_err(|_| invalid())?;
        }
        output.flush().map_err(|_| invalid())?;
        if bytes != artifact.byte_count || <[u8; 32]>::from(digest.finalize()) != artifact.sha256 {
            return Err(invalid());
        }
        check()?;
        Ok(output)
    })
    .await
    .map_err(probe_error)
}
fn seconds(microseconds: i64) -> Result<String> {
    if microseconds < 0 {
        return Err(invalid());
    }
    Ok(format!(
        "{}.{:06}",
        microseconds / 1_000_000,
        microseconds % 1_000_000
    ))
}
fn fragment_command(
    program: &str,
    path: &Path,
    start: i64,
    one_group: bool,
) -> Result<FfprobeCommand> {
    let path = path.to_str().ok_or_else(invalid)?;
    let mut args = vec![
        "-nostdin".into(),
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-i".into(),
        path.into(),
        "-ss".into(),
        seconds(start)?,
    ];
    if one_group {
        args.extend(["-t".into(), "0.000001".into()]);
    }
    args.extend([
        "-map".into(),
        "0:s:0".into(),
        "-c:s".into(),
        "copy".into(),
        // The original cue duration would stall FFmpeg's dts+duration scheduler
        // past the next group. Explicit DVB clears and payload timeouts remain intact.
        "-bsf:s".into(),
        "setts=pts=PTS:dts=DTS:duration=0".into(),
        "-f".into(),
        "matroska".into(),
        "pipe:1".into(),
    ]);
    Ok(FfprobeCommand {
        program: program.into(),
        args,
    })
}

#[cfg(test)]
#[path = "isolator_tests.rs"]
mod tests;
