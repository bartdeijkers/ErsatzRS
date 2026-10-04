use super::{
    timeline, BTreeMap, DateTime, Deserialize, Duration, EntityId, MediaEnumerator,
    PlaylistEnumerator, PlayoutBuildMode, SchedulingError, SchedulingRequest, ScriptState,
    Serialize, Utc,
};
use crate::scheduling::{classic, truncate_to_persisted_precision, PlayoutPreservation};
use ersatzrs_domain::PlayoutScriptedCheckpoint;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct CheckpointContext {
    version: u32,
    next_start: DateTime<Utc>,
    pub(super) guide_group: i32,
    /// An explicit Reset starts a new rotation without deleting retained history.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) history_start: Option<DateTime<Utc>>,
    pub(super) smart_shuffle: super::super::SchedulingSmartShuffleState,
    sources: BTreeMap<String, SourceCheckpoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum SourceCheckpoint {
    Media {
        state: classic::ClassicMediaCheckpoint,
        seed: Option<i32>,
    },
    Playlist {
        state: classic::ClassicPlaylistCheckpoint,
        cycle: u64,
    },
}

fn refused(reason: &str) -> SchedulingError {
    SchedulingError::InvalidScriptedTimeline(format!(
        "cannot restore Scripted programme continuity: {reason}; use an explicit Reset to create compatible boundary checkpoints"
    ))
}

pub(super) struct PreparedBuild {
    pub start: DateTime<Utc>,
    pub retain_through: Option<DateTime<Utc>>,
    pub checkpoint: Option<CheckpointContext>,
}

/// Validate a persisted Scripted boundary before publication or restoration.
///
/// # Errors
/// Returns an error for malformed, incompatible or incorrectly owned state.
pub fn validate_scripted_checkpoint(
    checkpoint: &PlayoutScriptedCheckpoint,
    playout_id: EntityId,
) -> Result<(), SchedulingError> {
    decode_checkpoint(checkpoint, playout_id).map(|_| ())
}

fn decode_checkpoint(
    checkpoint: &PlayoutScriptedCheckpoint,
    playout_id: EntityId,
) -> Result<CheckpointContext, SchedulingError> {
    let context: CheckpointContext = serde_json::from_str(&checkpoint.context)
        .map_err(|_| refused("the boundary checkpoint is malformed"))?;
    if checkpoint.playout_id != playout_id
        || context.version != 1
        || context.smart_shuffle.version
            != super::super::SchedulingSmartShuffleState::default().version
        || context.next_start != checkpoint.checkpoint_date
        || context
            .history_start
            .is_some_and(|start| start > context.next_start)
        || context.sources.values().any(|source| match source {
            SourceCheckpoint::Media { state, .. } => !state.is_valid(),
            SourceCheckpoint::Playlist { state, .. } => !state.is_valid(),
        })
    {
        return Err(refused("the boundary checkpoint is incompatible"));
    }
    Ok(context)
}

pub(super) fn prepare(request: &SchedulingRequest) -> Result<PreparedBuild, SchedulingError> {
    crate::scheduling::PlayoutBuildRequest::new(request.playout.id, request.mode)
        .with_preservation(request.reference_data.preservation)
        .validate_preservation(request.playout.schedule_kind)?;
    let keep_reset = request.mode == PlayoutBuildMode::Reset
        && request.reference_data.preservation == PlayoutPreservation::KeepCurrent;
    let start = if request.mode == PlayoutBuildMode::Refresh || keep_reset {
        let boundary = crate::scheduling::programme::preserved_programme_finish(
            &request.reference_data.existing_items,
            crate::scheduling::programme::preservation_clock(
                &request.reference_data,
                request.start,
            ),
        )?;
        if request.reference_data.preservation == PlayoutPreservation::ReplaceCurrent {
            request.start
        } else {
            boundary
        }
    } else {
        timeline::continuation_start(request)?
    };
    let retain = ((request.mode == PlayoutBuildMode::Refresh || keep_reset)
        && !request.reference_data.existing_items.is_empty())
    .then_some(start);
    if request.mode == PlayoutBuildMode::Reset {
        return Ok(PreparedBuild {
            start,
            retain_through: retain,
            checkpoint: None,
        });
    }
    let last_finish = request
        .reference_data
        .existing_items
        .iter()
        .filter(|item| item.finish <= start)
        .map(|item| item.finish)
        .max();
    let Some(last_finish) = last_finish else {
        return Ok(PreparedBuild {
            start,
            retain_through: retain,
            checkpoint: None,
        });
    };
    let checkpoint = request
        .reference_data
        .existing_scripted_checkpoints
        .iter()
        .find(|checkpoint| {
            checkpoint.checkpoint_date == truncate_to_persisted_precision(last_finish)
        });
    let Some(checkpoint) = checkpoint else {
        if request.mode == PlayoutBuildMode::Refresh {
            return Err(refused("the published boundary has no checkpoint"));
        }
        // Older Continue state remains compatible; history and the tail anchor
        // still restore it. Only replacement requires a complete boundary proof.
        return Ok(PreparedBuild {
            start,
            retain_through: retain,
            checkpoint: None,
        });
    };
    let context = decode_checkpoint(checkpoint, request.playout.id)?;
    Ok(PreparedBuild {
        start,
        retain_through: retain,
        checkpoint: Some(context),
    })
}

impl ScriptState {
    pub(super) fn validate_restored_sources(&self) -> Result<(), SchedulingError> {
        let Some(context) = &self.restore_checkpoint else {
            return Ok(());
        };
        for (key, source) in &context.sources {
            if let Some(media) = self.enumerators.get(key) {
                if !matches!(source, SourceCheckpoint::Media { state, .. } if state.matches_order(media.playback_order))
                {
                    return Err(refused(
                        "a checkpoint source changed kind or playback order",
                    ));
                }
            }
            if self.playlist_enumerators.contains_key(key)
                && !matches!(source, SourceCheckpoint::Playlist { .. })
            {
                return Err(refused("a checkpoint source changed kind"));
            }
        }
        Ok(())
    }

    pub(super) fn capture_checkpoint(&mut self) {
        self.capture_checkpoint_at(self.current_time);
    }

    pub(super) fn capture_checkpoint_at(&mut self, finish: DateTime<Utc>) {
        let next_start = truncate_to_persisted_precision(finish);
        let mut smart_shuffle = self.smart_shuffle_state.clone();
        smart_shuffle.normalize_timestamps(truncate_to_persisted_precision);
        let mut sources = self
            .enumerators
            .iter()
            .map(|(key, enumerator)| {
                (
                    key.clone(),
                    SourceCheckpoint::Media {
                        state: classic::media_checkpoint(enumerator),
                        seed: enumerator.shuffle_seed,
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        sources.extend(self.playlist_enumerators.iter().map(|(key, playlist)| {
            (
                key.clone(),
                SourceCheckpoint::Playlist {
                    state: classic::playlist_checkpoint(playlist),
                    cycle: playlist.cycle,
                },
            )
        }));
        self.checkpoints.insert(
            next_start,
            CheckpointContext {
                version: 1,
                next_start,
                guide_group: self.guide_group,
                history_start: self.history_start,
                smart_shuffle,
                sources,
            },
        );
    }

    pub(super) fn restore_media_source(&self, key: &str, enumerator: &mut MediaEnumerator) -> bool {
        self.restore_checkpoint
            .as_ref()
            .is_some_and(|context| context.restore_media_source(key, enumerator))
    }

    pub(super) fn restore_playlist_source(&self, key: &str, playlist: &mut PlaylistEnumerator) {
        if let Some(SourceCheckpoint::Playlist { state, cycle }) = self
            .restore_checkpoint
            .as_ref()
            .and_then(|context| context.sources.get(key))
        {
            classic::restore_playlist_checkpoint(playlist, state);
            playlist.cycle = *cycle;
        }
    }

    pub(super) fn persisted_checkpoints(
        &self,
        remove_before: Option<DateTime<Utc>>,
    ) -> Result<Vec<PlayoutScriptedCheckpoint>, SchedulingError> {
        let cutoff = remove_before
            .map(|date| date - self.reference_data.max_playout_offset.max(Duration::zero()));
        let published = self
            .reference_data
            .existing_items
            .iter()
            .map(|item| truncate_to_persisted_precision(item.finish))
            .collect::<std::collections::BTreeSet<_>>();
        let mut result = self
            .reference_data
            .existing_scripted_checkpoints
            .iter()
            .filter(|checkpoint| {
                (self.mode != PlayoutBuildMode::Reset
                    || (self.reference_data.preservation == PlayoutPreservation::KeepCurrent
                        && checkpoint.checkpoint_date < self.start))
                    && (self.mode == PlayoutBuildMode::Continue
                        || checkpoint.checkpoint_date <= self.start)
                    && published.contains(&checkpoint.checkpoint_date)
                    && cutoff.is_none_or(|cutoff| checkpoint.checkpoint_date >= cutoff)
            })
            .map(|checkpoint| (checkpoint.checkpoint_date, checkpoint.clone()))
            .collect::<BTreeMap<_, _>>();
        for (date, context) in &self.checkpoints {
            result.insert(
                *date,
                PlayoutScriptedCheckpoint {
                    playout_id: self.playout_id,
                    checkpoint_date: *date,
                    context: serde_json::to_string(context)
                        .map_err(|_| refused("checkpoint serialization failed"))?,
                },
            );
        }
        Ok(result.into_values().collect())
    }
}

impl CheckpointContext {
    pub(super) fn restore_media_source(&self, key: &str, enumerator: &mut MediaEnumerator) -> bool {
        let Some(SourceCheckpoint::Media { state, seed }) = self.sources.get(key) else {
            return false;
        };
        classic::restore_media_checkpoint(enumerator, state);
        enumerator.shuffle_seed = *seed;
        true
    }
}
