//! Programme-boundary state includes the unexecuted part of a multi-item
//! instruction; a future tail anchor cannot restore an earlier boundary.
use super::{
    BTreeMap, BTreeSet, DateTime, Deserialize, Duration, EntityId, FillerKind, PlayoutAnchor,
    PlayoutBuildMode, SchedulingError, SchedulingRequest, Serialize, SerializedYamlContext, Utc,
    YamlContext, YamlListState, YamlPlayoutDefinition,
};
use crate::scheduling::{classic, truncate_to_persisted_precision};
use ersatzrs_domain::PlayoutYamlCheckpoint;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub(super) enum PendingInstruction {
    Count(usize),
    Until(DateTime<Utc>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SourceState {
    media: BTreeMap<String, classic::ClassicMediaCheckpoint>,
    playlists: BTreeMap<String, classic::ClassicPlaylistCheckpoint>,
    playlist_cycles: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Checkpoint {
    version: u32,
    date: DateTime<Utc>,
    definition: String,
    context: String,
    pending: Option<PendingInstruction>,
    pending_shuffles: Vec<String>,
    list_states: BTreeMap<String, YamlListState>,
    visited_instructions: BTreeSet<usize>,
    sources: SourceState,
}

fn refused(reason: &str) -> SchedulingError {
    SchedulingError::InvalidRebuild(format!(
        "cannot restore YAML programme continuity: {reason}; use an explicit Reset to create compatible boundary checkpoints"
    ))
}

pub(super) fn validate_rebuilt_timeline(
    context: &YamlContext,
    request: &SchedulingRequest,
) -> Result<(), SchedulingError> {
    use crate::scheduling::PlayoutPreservation;
    if request.reference_data.preservation == PlayoutPreservation::EngineDefault {
        return Ok(());
    }
    let earliest_start = if request.mode == PlayoutBuildMode::Continue {
        Some(
            request
                .reference_data
                .existing_items
                .iter()
                .map(|item| item.finish)
                .max()
                .unwrap_or(request.start),
        )
    } else if request.reference_data.preservation == PlayoutPreservation::KeepCurrent {
        Some(request.start)
    } else {
        None
    };
    crate::scheduling::validate_scripted_timeline(
        context.added_items.iter().map(|item| (item.start, item.finish)), earliest_start,
    ).map_err(|_| SchedulingError::InvalidRebuild(
        "YAML rebuild rewound into the retained programme or produced overlapping/non-positive intervals; correct the YAML instructions".into(),
    ))
}

/// A YAML programme unit includes the rolls emitted by the same instruction.
/// Its checkpoint is captured after those rolls, before the next main item.
pub(super) fn preservation_boundary(
    request: &SchedulingRequest,
) -> Result<DateTime<Utc>, SchedulingError> {
    let clock = super::super::programme::preservation_clock(&request.reference_data, request.start);
    let mut finish = super::super::programme::preserved_programme_finish(
        &request.reference_data.existing_items,
        clock,
    )?;
    let mut awaiting_main = request
        .reference_data
        .existing_items
        .iter()
        .find(|item| item.start <= clock && item.finish > clock)
        .is_some_and(|item| {
            item.filler_kind == FillerKind::PreRoll
                && !request.reference_data.existing_items.iter().any(|main| {
                    main.filler_kind == FillerKind::None
                        && main.start >= item.start
                        && main.finish <= finish
                })
        });
    let mut items = request
        .reference_data
        .existing_items
        .iter()
        .collect::<Vec<_>>();
    items.sort_by_key(|item| item.start);
    for item in items {
        if item.start < finish {
            continue;
        }
        if item.start != finish {
            break;
        }
        match item.filler_kind {
            FillerKind::PreRoll if awaiting_main => {}
            FillerKind::None if awaiting_main => {
                awaiting_main = false;
            }
            FillerKind::MidRoll | FillerKind::PostRoll => {}
            _ => break,
        }
        finish = item.finish.max(item.guide_finish.unwrap_or(item.finish));
    }
    Ok(finish)
}

/// Validate a persisted YAML checkpoint before publishing or restoring it.
///
/// # Errors
/// Rejects malformed, incompatible or incorrectly owned boundary state.
pub fn validate_yaml_checkpoint(
    checkpoint: &PlayoutYamlCheckpoint,
    playout_id: EntityId,
) -> Result<(), SchedulingError> {
    decode(checkpoint, playout_id).map(|_| ())
}

fn decode(
    checkpoint: &PlayoutYamlCheckpoint,
    playout_id: EntityId,
) -> Result<Checkpoint, SchedulingError> {
    let state: Checkpoint =
        serde_json::from_str(&checkpoint.context).map_err(|_| refused("malformed checkpoint"))?;
    let context: SerializedYamlContext = serde_json::from_str(&state.context)
        .map_err(|_| refused("malformed scheduling context"))?;
    if state.version != 1
        || checkpoint.playout_id != playout_id
        || state.date != checkpoint.checkpoint_date
        || context.history_start.is_some_and(|date| date > state.date)
        || context.smart_shuffle.version
            != super::super::SchedulingSmartShuffleState::default().version
        || state
            .sources
            .media
            .values()
            .any(|source| !source.is_valid())
        || state
            .sources
            .playlists
            .values()
            .any(|source| !source.is_valid())
        || state
            .sources
            .playlists
            .keys()
            .ne(state.sources.playlist_cycles.keys())
        || state
            .sources
            .media
            .keys()
            .any(|key| state.sources.playlists.contains_key(key))
        || matches!(state.pending, Some(PendingInstruction::Count(0)))
        || matches!(state.pending, Some(PendingInstruction::Until(target)) if target <= state.date)
        || !valid_instruction_cursor(&state, &context)
    {
        return Err(refused("incompatible checkpoint"));
    }
    Ok(state)
}

fn valid_instruction_cursor(state: &Checkpoint, context: &SerializedYamlContext) -> bool {
    use super::instructions::YamlInstructionKind;
    let Ok(definition) = serde_json::from_str::<YamlPlayoutDefinition>(&state.definition) else {
        return false;
    };
    let instructions = if let Some(name) = &context.active_schedule {
        let Some(schedule) = definition
            .schedules
            .iter()
            .find(|schedule| &schedule.name == name)
        else {
            return false;
        };
        &schedule.playout
    } else {
        &definition.playout
    };
    match state.pending {
        None => context.instruction_index <= instructions.len(),
        Some(PendingInstruction::Count(_)) => instructions
            .get(context.instruction_index)
            .is_some_and(|instruction| {
                matches!(
                    instruction.kind(),
                    YamlInstructionKind::Count | YamlInstructionKind::All
                )
            }),
        Some(PendingInstruction::Until(_)) => instructions
            .get(context.instruction_index)
            .is_some_and(|instruction| {
                matches!(
                    instruction.kind(),
                    YamlInstructionKind::Duration
                        | YamlInstructionKind::PadToNext
                        | YamlInstructionKind::PadUntil
                )
            }),
    }
}

impl YamlContext {
    pub(super) fn restore_refresh_checkpoint(
        &mut self,
        definition: &mut YamlPlayoutDefinition,
        request: &SchedulingRequest,
    ) -> Result<(), SchedulingError> {
        let date = if request.reference_data.preservation
            == super::super::PlayoutPreservation::KeepCurrent
        {
            request.start
        } else {
            request
                .reference_data
                .existing_yaml_checkpoints
                .iter()
                .filter(|checkpoint| checkpoint.checkpoint_date <= request.start)
                .max_by_key(|checkpoint| checkpoint.checkpoint_date)
                .map_or(request.start, |checkpoint| checkpoint.checkpoint_date)
        };
        let date = truncate_to_persisted_precision(date);
        let checkpoint = request
            .reference_data
            .existing_yaml_checkpoints
            .iter()
            .find(|checkpoint| checkpoint.checkpoint_date == date)
            .or_else(|| gap_checkpoint(request))
            .ok_or_else(|| refused("the programme boundary has no checkpoint"))?;
        self.restore_checkpoint(definition, checkpoint)?;
        self.current_time = request.start;
        Ok(())
    }

    pub(super) fn published_checkpoints(
        &self,
        request: &SchedulingRequest,
        remove_before: Option<DateTime<Utc>>,
    ) -> Vec<PlayoutYamlCheckpoint> {
        let published = self
            .added_items
            .iter()
            .chain(request.reference_data.existing_items.iter().filter(|item| {
                request.mode == PlayoutBuildMode::Continue
                    || (request.reference_data.preservation
                        == super::super::PlayoutPreservation::KeepCurrent
                        && item.finish <= request.start)
            }))
            .flat_map(|item| [item.start, item.finish])
            .chain(self.gaps.iter().flat_map(|gap| [gap.start, gap.finish]))
            .map(truncate_to_persisted_precision)
            .collect::<BTreeSet<_>>();
        let cutoff = remove_before.map(|date| {
            date - request
                .reference_data
                .max_playout_offset
                .max(Duration::zero())
        });
        let mut checkpoints = request
            .reference_data
            .existing_yaml_checkpoints
            .iter()
            .filter(|checkpoint| {
                (request.mode == PlayoutBuildMode::Continue
                    || (request.reference_data.preservation
                        == super::super::PlayoutPreservation::KeepCurrent
                        && checkpoint.checkpoint_date <= request.start))
                    && cutoff.is_none_or(|date| checkpoint.checkpoint_date >= date)
            })
            .map(|checkpoint| (checkpoint.checkpoint_date, checkpoint.clone()))
            .collect::<BTreeMap<_, _>>();
        checkpoints.extend(self.checkpoints.clone());
        checkpoints
            .into_values()
            .filter(|checkpoint| published.contains(&checkpoint.checkpoint_date))
            .collect()
    }

    pub(super) fn capture_checkpoint(
        &mut self,
        definition: &YamlPlayoutDefinition,
        pending: Option<PendingInstruction>,
    ) -> Result<(), SchedulingError> {
        if !self.filler_stack.is_empty() {
            return Ok(());
        }
        self.sequence_state.capture_orders(definition)?;
        let date = truncate_to_persisted_precision(self.current_time);
        let state = Checkpoint {
            version: 1,
            date,
            definition: serde_json::to_string(definition)
                .map_err(|_| refused("invalid definition"))?,
            context: self.serialize()?,
            pending,
            pending_shuffles: self.pending_shuffles.clone(),
            list_states: self.list_states.clone(),
            visited_instructions: self.visited_instructions.clone(),
            sources: SourceState {
                media: self
                    .enumerators
                    .iter()
                    .map(|(key, source)| (key.clone(), classic::media_checkpoint(source)))
                    .collect(),
                playlists: self
                    .playlist_enumerators
                    .iter()
                    .map(|(key, source)| (key.clone(), classic::playlist_checkpoint(source)))
                    .collect(),
                playlist_cycles: self
                    .playlist_enumerators
                    .iter()
                    .map(|(key, source)| (key.clone(), source.cycle))
                    .collect(),
            },
        };
        self.checkpoints.insert(
            date,
            PlayoutYamlCheckpoint {
                playout_id: self.playout.id,
                checkpoint_date: date,
                context: serde_json::to_string(&state).map_err(|_| refused("invalid state"))?,
            },
        );
        Ok(())
    }

    pub(super) fn restore_checkpoint(
        &mut self,
        definition: &mut YamlPlayoutDefinition,
        checkpoint: &PlayoutYamlCheckpoint,
    ) -> Result<(), SchedulingError> {
        let state = decode(checkpoint, self.playout.id)?;
        // Compare the restored flattened order, rather than a newly shuffled
        // sequence, before accepting the saved instruction position.
        self.restore(&PlayoutAnchor {
            next_start: state.date,
            context: Some(state.context),
            ..PlayoutAnchor::default()
        })?;
        self.restore_sequence_orders(definition)?;
        if serde_json::to_string(definition).map_err(|_| refused("invalid definition"))?
            != state.definition
        {
            return Err(refused("the YAML instructions changed"));
        }
        self.pending_shuffles = state.pending_shuffles;
        self.list_states = state.list_states;
        self.visited_instructions = state.visited_instructions;
        for (key, saved) in state.sources.media {
            if self.playlist_items(&key).is_some() {
                return Err(refused("a source changed kind"));
            }
            let source = self.enumerator(&key);
            if !saved.matches_order(source.playback_order) {
                return Err(refused("a source changed playback order"));
            }
            classic::restore_media_checkpoint(source, &saved);
        }
        for (key, saved) in state.sources.playlists {
            let source = self
                .playlist_enumerator(&key)
                .ok_or_else(|| refused("a playlist changed kind"))?;
            classic::restore_playlist_checkpoint(source, &saved);
            source.cycle = state.sources.playlist_cycles[&key];
        }
        self.pending_instruction = state.pending;
        Ok(())
    }
}

fn gap_checkpoint(request: &SchedulingRequest) -> Option<&PlayoutYamlCheckpoint> {
    if request
        .reference_data
        .existing_items
        .iter()
        .any(|item| item.start <= request.start && item.finish > request.start)
    {
        return None;
    }
    let checkpoint = request
        .reference_data
        .existing_yaml_checkpoints
        .iter()
        .filter(|checkpoint| checkpoint.checkpoint_date <= request.start)
        .max_by_key(|checkpoint| checkpoint.checkpoint_date)?;
    // Waiting after a complete unit does not consume media. A gap inside an
    // unfinished roll sequence has no such proof and must not guess its state.
    (!request
        .reference_data
        .existing_items
        .iter()
        .any(|item| item.finish > checkpoint.checkpoint_date && item.finish <= request.start))
    .then_some(checkpoint)
}
