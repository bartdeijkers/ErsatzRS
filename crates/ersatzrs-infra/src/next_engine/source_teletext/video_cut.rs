//! Occurrence-bound preparation evidence, deliberately outside the SRT cache.

use super::{
    same_source, selected_video, unavailable, Arc, FfprobeExtractor, Instant,
    LocalProbeCleanupScope, PathBuf, Preparation, PreparedLocalTeletextSource, Result, Scope,
    Source, TeletextWorkHold,
};
use crate::ffprobe::teletext_decode::{
    SourceTeletextPageDecodeError, SourceTeletextVideoCutEvidence,
};

pub(in crate::next_engine) struct PreparedCaption {
    pub(in crate::next_engine) path: PathBuf,
    pub(in crate::next_engine) video_cut: Option<Box<PreparedVideoCut>>,
}

pub(in crate::next_engine) struct PreparedVideoCut {
    evidence: SourceTeletextVideoCutEvidence,
    route: Source,
    hold: TeletextWorkHold,
    deadline: Instant,
}

pub(super) async fn prepare(
    source: Arc<PreparedLocalTeletextSource>,
    extractor: &FfprobeExtractor,
    request: Preparation<'_>,
    hold: TeletextWorkHold,
) -> Result<Option<Box<PreparedVideoCut>>> {
    if !request.pending.burn || request.video_cut != super::super::SourceVideoCutSupport::Version1 {
        return Ok(None);
    }
    let Some(video) = request.video else {
        return Ok(None);
    };
    let Some(index) = selected_video(request.source, Some(video)) else {
        return Ok(None);
    };
    let route = video.source.as_ref().ok_or_else(unavailable)?;
    let Some((out, hint)) = wire_slot(route, index) else {
        // Incomplete ordinary metadata keeps Next's existing probe in control;
        // it is not permission to attach a cut to another/default video.
        return Ok(None);
    };
    if hint.source_frame_cut.is_some() {
        return Err(unavailable());
    }
    let cut = match extractor
        .prepare_local_teletext_video_cut(
            source.clone(),
            index,
            out,
            request.deadline,
            hold.clone(),
        )
        .await
    {
        Ok(cut) => cut,
        Err(
            SourceTeletextPageDecodeError::Decoder | SourceTeletextPageDecodeError::InvalidOutput,
        ) => {
            // Optional video qualification must not discard a valid full-source
            // caption artifact. It grants no cutoff; source ownership must still
            // pass afresh, followed by the ordinary final publication fence.
            source
                .check_source(request.deadline, hold.clone())
                .await
                .map_err(|_| unavailable())?;
            None
        }
        Err(_) => return Err(unavailable()),
    };
    let Some(evidence) = cut else {
        tracing::warn!(
            stream_index = index,
            out_point_ms = out,
            "Finite video cutoff is unqualified; retaining existing caption and output policy"
        );
        return Ok(None);
    };
    if evidence.psi_fingerprint() != request.pending.candidate().psi_fingerprint {
        return Err(unavailable());
    }
    Ok(Some(Box::new(PreparedVideoCut {
        evidence,
        route: route.clone(),
        hold,
        deadline: request.deadline,
    })))
}

impl PreparedVideoCut {
    /// Recheck before the final fresh SQL projection, keeping that projection's
    /// existing last-publication fence free of additional awaited operations.
    pub(in crate::next_engine) async fn check_source(&self) -> Result<()> {
        let scope = Scope(LocalProbeCleanupScope::new());
        let checked = scope
            .0
            .scope(
                self.evidence
                    .source()
                    .check_source(self.deadline, self.hold.clone()),
            )
            .await;
        scope.0.close();
        scope.0.wait_cleanup().await.map_err(|_| unavailable())?;
        checked.map_err(|_| unavailable())
    }

    pub(in crate::next_engine) fn install(
        &self,
        item: &mut super::super::protocol::PlayoutItem,
    ) -> Result<()> {
        let index = self.evidence.stream_index();
        let video = item
            .tracks
            .as_mut()
            .and_then(|tracks| tracks.video.as_mut())
            .filter(|track| track.stream_index == Some(index))
            .ok_or_else(unavailable)?;
        let route = video.source.as_mut().ok_or_else(unavailable)?;
        if !same_source(route, &self.route)
            || wire_slot(route, index).is_none_or(|(out, hint)| {
                out != self.evidence.out_ms() || hint.source_frame_cut.is_some()
            })
        {
            return Err(unavailable());
        }
        let hint = match route {
            Source::Local { probe_hint, .. } | Source::Http { probe_hint, .. } => probe_hint,
            _ => return Err(unavailable()),
        }
        .as_mut()
        .and_then(|hint| {
            hint.video
                .iter_mut()
                .find(|hint| hint.stream_index == index)
        })
        .ok_or_else(unavailable)?;
        hint.source_frame_cut = Some(super::super::protocol::SourceFrameCutHint {
            version: 1,
            out_point_ms: self.evidence.out_ms(),
            first_excluded_pts_90k: self.evidence.cut_pts_90k(),
        });
        Ok(())
    }
}

fn wire_slot(
    source: &Source,
    index: u32,
) -> Option<(u64, &super::super::protocol::VideoProbeHint)> {
    let (hint, out) = match source {
        Source::Local {
            probe_hint,
            out_point_ms,
            ..
        }
        | Source::Http {
            probe_hint,
            out_point_ms,
            ..
        } => (probe_hint.as_ref()?, (*out_point_ms)?),
        _ => return None,
    };
    let mut selected = hint.video.iter().filter(|hint| hint.stream_index == index);
    let video = selected.next()?;
    (selected.next().is_none() && out > 0).then_some((out, video))
}
