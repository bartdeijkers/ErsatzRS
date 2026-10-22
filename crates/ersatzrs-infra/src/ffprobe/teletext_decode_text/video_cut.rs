//! Owned source-frame evidence for a finite outpoint before encoder rounding.

use super::{
    check_deadline, finite_video_cut, local, local_teletext, probe_error, FfprobeExtractor,
    Instant, LocalProbeCleanupScope, PreparedLocalTeletextSource, SourceTeletextPageDecodeError,
    TeletextWorkHold,
};
use std::sync::Arc;

/// Actual selected-video complete-frame boundary from one validated source.
///
/// This retains the source association, not preparation admission. It does not
/// authorize publication, certify natural video EOF, or prove that a downstream
/// encoder honored the boundary. The caller must revalidate its selection and
/// source before attaching it to the same video stream and exact outpoint.
pub struct SourceTeletextVideoCutEvidence {
    source: Arc<PreparedLocalTeletextSource>,
    stream_index: u32,
    psi_fingerprint: [u8; 32],
    cut: finite_video_cut::FiniteVideoCut,
}

impl SourceTeletextVideoCutEvidence {
    /// Held source whose native origin and selected program qualified the cut.
    #[must_use]
    pub fn source(&self) -> &Arc<PreparedLocalTeletextSource> {
        &self.source
    }

    /// Global video stream index, not its PID or video ordinal.
    #[must_use]
    pub const fn stream_index(&self) -> u32 {
        self.stream_index
    }

    /// Complete-source PSI fingerprint observed alongside the copied bytes.
    #[must_use]
    pub const fn psi_fingerprint(&self) -> [u8; 32] {
        self.psi_fingerprint
    }

    /// Exact selected outpoint, independent of seek and encoder time base.
    #[must_use]
    pub fn out_ms(&self) -> u64 {
        self.cut.out_ms()
    }

    /// First excluded frame start in the source-normalized 90 kHz clock.
    ///
    /// Observed frames in the decoded window before this boundary have actual
    /// ends at or before the selected outpoint. Apply it before framerate conversion;
    /// must not replace the selected outpoint or the complete source duration.
    #[must_use]
    pub fn cut_pts_90k(&self) -> i64 {
        self.cut.cut_pts_90k()
    }
}

impl FfprobeExtractor {
    /// Qualify a finite source-video cutoff using actual decoded presentation.
    ///
    /// Uses the existing complete-source PSI/PES validation, private snapshot,
    /// owned process/reader cleanup, output bound and fifteen-second budget.
    /// The bounded selected-video observation starts at the actual source beginning;
    /// nominal framerates, format duration and decoder exit status never
    /// substitute for a complete-frame boundary. `None` means the observation
    /// cannot certify that boundary, including an unproven natural EOF.
    /// Existing text preparation and playback are unaffected by this evidence.
    ///
    /// # Errors
    /// Rejects missing ownership, invalid/stale source or stream associations,
    /// invalid decoder output, deadline/cancellation and unconfirmed cleanup.
    pub async fn prepare_local_teletext_video_cut(
        &self,
        source: Arc<PreparedLocalTeletextSource>,
        video_stream_index: u32,
        out_ms: u64,
        deadline: Instant,
        hold: TeletextWorkHold,
    ) -> Result<Option<SourceTeletextVideoCutEvidence>, SourceTeletextPageDecodeError> {
        let deadline = deadline.min(Instant::now() + super::super::MAX_STARTUP);
        check_deadline(deadline)?;
        if LocalProbeCleanupScope::current().is_none() {
            return Err(SourceTeletextPageDecodeError::NoCleanupScope);
        }
        if source.native_origin().format_name() != Some("mpegts") {
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
        let bytes = local::capture_retained_silent(
            finite_video_cut::command(
                &self.ffprobe_path,
                video_stream_index,
                captured.snapshot.path(),
            )?,
            "finite source video cutoff".into(),
            Some(deadline),
            Some(
                local::ProbeRetention::new(hold.clone(), source.guard.clone())
                    .with_shared_snapshot(captured.snapshot),
            ),
        )
        .await
        .map_err(probe_error)?;
        let cut = finite_video_cut::qualify(&source, video_stream_index, out_ms, &bytes, deadline)?;
        source.check_source(deadline, hold).await?;
        Ok(cut.map(|cut| SourceTeletextVideoCutEvidence {
            source,
            stream_index: video_stream_index,
            psi_fingerprint: captured.psi_fingerprint,
            cut,
        }))
    }
}
