//! Bind the private fragment choice to observed same-source enabled AV starts.
use super::{
    check_deadline, invalid, Arc, Coordinates, FfprobeExtractor, Instant, LocalProbeCleanupScope,
    Result, Selection, SourceTeletextBitmapArtifact, SourceTeletextBitmapArtifactObservation,
    SourceTeletextBitmapFragments, SourceTeletextPageDecodeError, TeletextWorkHold,
};
use crate::ffprobe::teletext_consumer_alignment::{
    project_bitmap_consumer_origin, NativeSourceBitmapAlignment,
};

impl FfprobeExtractor {
    /// Project observed enabled AV starts onto this artifact before isolation.
    ///
    /// The requested AV indices are observed from the same held source; no
    /// caller-selected bootstrap index or midpoint becomes authority. Exact
    /// microsecond bounds, one-millisecond delay quantization and its residual
    /// remain available in the returned observation. This still certifies no
    /// final command policy, actual child seek landing or caption delivery.
    /// # Errors
    /// Rejects stale sources, non-AV/ambiguous origins, uncertain group/delay
    /// projection, overflowing coordinates or ordinary isolation failures.
    pub async fn isolate_local_teletext_bitmap_artifact_for_consumer(
        &self,
        artifact: Arc<SourceTeletextBitmapArtifact>,
        enabled_av: &[i32],
        effective_seek_us: i64,
        deadline: Instant,
        hold: TeletextWorkHold,
    ) -> Result<SourceTeletextBitmapFragments> {
        check_deadline(deadline)?;
        if LocalProbeCleanupScope::current().is_none() {
            return Err(SourceTeletextPageDecodeError::NoCleanupScope);
        }
        artifact.source.check_source(deadline, hold.clone()).await?;
        let av = self
            .observe_local_teletext_consumer_origin(
                &artifact.source,
                enabled_av,
                deadline,
                hold.clone(),
            )
            .await?;
        let points = artifact
            .observation()
            .groups()
            .iter()
            .map(|group| group.pts_microseconds())
            .collect::<Vec<_>>();
        let alignment = project_bitmap_consumer_origin(
            &av,
            artifact.caption_origin(),
            artifact.caption_origin_microseconds(),
            &points,
            effective_seek_us,
        )
        .ok_or_else(invalid)?;
        let selection = from_alignment(artifact.observation(), alignment)?;
        self.isolate_selected_bitmap_fragments(
            artifact,
            selection,
            Coordinates::Consumer(alignment),
            deadline,
            hold,
        )
        .await
    }
}
fn from_alignment(
    observation: &SourceTeletextBitmapArtifactObservation,
    alignment: NativeSourceBitmapAlignment,
) -> Result<Selection> {
    let groups = observation.groups();
    let last = groups.last().ok_or_else(invalid)?;
    if last.clear_frames() == 0 || last.nonempty_frames() != 0 {
        return Err(invalid());
    }
    let bootstrap = alignment.active_group_index().unwrap_or(groups.len() - 1);
    let following = alignment
        .following_group_index()
        .map(|index| {
            groups
                .get(index)
                .map(|group| group.pts_microseconds())
                .ok_or_else(invalid)
        })
        .transpose()?;
    let delay = alignment.following_delay_microseconds();
    if following.is_some() != delay.is_some() || delay.is_some_and(|delay| delay <= 0) {
        return Err(invalid());
    }
    Ok(Selection {
        bootstrap: groups
            .get(bootstrap)
            .ok_or_else(invalid)?
            .pts_microseconds(),
        bootstrap_end: groups
            .get(bootstrap + 1)
            .map(|group| group.pts_microseconds()),
        following,
        delay,
    })
}
