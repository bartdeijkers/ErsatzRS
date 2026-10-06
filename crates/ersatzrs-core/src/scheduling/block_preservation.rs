use super::{block_anchor_context_version, cleanup_history, BlockAnchorContext};
use crate::scheduling::{
    block_sources::checkpoint_smart_shuffle, PlayoutBuildMode, PlayoutPreservation,
    SchedulingError, SchedulingRequest, SchedulingResult, SchedulingSmartShuffleState,
};
use chrono::{DateTime, Duration, Utc};
use ersatzrs_domain::FillerKind;

pub(super) struct Preservation {
    pub(super) rebuild: bool,
    pub(super) history_start: Option<DateTime<Utc>>,
    pub(super) smart_shuffle: SchedulingSmartShuffleState,
    original_start: DateTime<Utc>,
    keep: bool,
    remove_before: DateTime<Utc>,
}

impl Preservation {
    pub(super) fn prepare(request: &mut SchedulingRequest) -> Result<Self, SchedulingError> {
        let policy = request.reference_data.preservation;
        let rebuild = policy != PlayoutPreservation::EngineDefault
            && request.mode != PlayoutBuildMode::Continue;
        let reset = rebuild && request.mode == PlayoutBuildMode::Reset;
        let context = if reset {
            BlockAnchorContext::default()
        } else {
            request
                .playout
                .anchor
                .as_ref()
                .and_then(|anchor| anchor.context.as_deref())
                .map(serde_json::from_str::<BlockAnchorContext>)
                .transpose()
                .map_err(|_| {
                    SchedulingError::InvalidRebuild(
                        "incompatible Block anchor; use an explicit Reset".into(),
                    )
                })?
                .unwrap_or_default()
        };
        if context.version != block_anchor_context_version()
            || context.smart_shuffle.version != SchedulingSmartShuffleState::default().version
        {
            return Err(SchedulingError::InvalidRebuild(
                "incompatible Block anchor; use an explicit Reset".into(),
            ));
        }
        let mut state = Self {
            rebuild,
            history_start: context.history_start,
            smart_shuffle: if request.mode == PlayoutBuildMode::Continue {
                context.smart_shuffle
            } else {
                SchedulingSmartShuffleState::default()
            },
            original_start: request.start,
            keep: policy == PlayoutPreservation::KeepCurrent,
            remove_before: request.start - Duration::hours(4),
        };
        if rebuild {
            state.prepare_rebuild(request, reset)?;
        }
        if let Some(floor) = state.history_start {
            request
                .reference_data
                .existing_history
                .retain(|entry| entry.when >= floor);
        }
        Ok(state)
    }

    fn prepare_rebuild(
        &mut self,
        request: &mut SchedulingRequest,
        reset: bool,
    ) -> Result<(), SchedulingError> {
        let existing = &request.reference_data.existing_items;
        if self.keep {
            let now = crate::scheduling::programme::preservation_clock(
                &request.reference_data,
                request.start,
            );
            // In an unfilled remainder, the currently advertised block still
            // owns its guide interval. Retain that interval through its end.
            let clock = if existing
                .iter()
                .any(|item| item.start <= now && item.finish > now)
            {
                now
            } else {
                existing
                    .iter()
                    .filter(|item| {
                        item.guide_start.is_some_and(|start| start <= now)
                            && item.guide_finish.is_some_and(|finish| finish > now)
                    })
                    .max_by_key(|item| item.start)
                    .map_or(now, |item| item.start)
            };
            request.start =
                crate::scheduling::programme::preserved_programme_finish(existing, clock)?;
            if let Some(start) = existing
                .iter()
                .find(|item| item.start <= clock && item.finish > clock)
                .and_then(|item| item.guide_start)
            {
                self.remove_before = self.remove_before.min(start);
            }
        }
        let boundary = request.start;
        request.reference_data.existing_history.retain(|entry| {
            entry.when < boundary
                && entry.finish.is_none_or(|finish| finish <= boundary)
                && self.history_start.is_none_or(|floor| entry.when >= floor)
        });
        if reset {
            self.history_start = Some(crate::scheduling::truncate_to_persisted_precision(boundary));
        } else {
            self.smart_shuffle = restore_boundary_state(request, self.history_start)?;
        }
        request
            .reference_data
            .existing_items
            .retain(|item| self.keep && item.finish <= boundary);
        // A retained overrun can consume the start of the next configured block.
        // Keep its original finish while moving the new output after the seam.
        request
            .reference_data
            .effective_blocks
            .retain(|block| block.start + block.duration > boundary);
        for block in &mut request.reference_data.effective_blocks {
            if block.start < boundary {
                block.duration -= boundary - block.start;
                block.start = boundary;
            }
        }
        Ok(())
    }

    pub(super) fn finish(
        &self,
        request: &SchedulingRequest,
        result: &mut SchedulingResult,
        smart_shuffle: SchedulingSmartShuffleState,
    ) -> Result<(), SchedulingError> {
        if self.rebuild {
            crate::scheduling::validate_scripted_timeline(
                result
                    .added_items
                    .iter()
                    .map(|item| (item.start, item.finish)),
                Some(request.start),
            )
            .map_err(|_| {
                SchedulingError::InvalidRebuild(
                    "Block rebuild overlaps the retained programme or contains invalid intervals"
                        .into(),
                )
            })?;
            result.clear_items = true;
            result.retain_through = self.keep.then_some(request.start);
            result.remove_before = Some(self.remove_before);
        }
        result.history_to_remove.extend(cleanup_history(
            &request.reference_data.existing_history,
            self.original_start,
            request.reference_data.max_playout_offset,
        ));
        if self.rebuild
            || self.history_start.is_some()
            || request.reference_data.has_active_smart_shuffle()
        {
            result.anchor = Some(ersatzrs_domain::PlayoutAnchor {
                next_start: result
                    .added_items
                    .iter()
                    .map(|item| item.finish)
                    .max()
                    .or_else(|| self.rebuild.then_some(request.start))
                    .or_else(|| {
                        request
                            .playout
                            .anchor
                            .as_ref()
                            .map(|anchor| anchor.next_start)
                    })
                    .unwrap_or(request.start),
                context: Some(
                    serde_json::to_string(&BlockAnchorContext {
                        version: block_anchor_context_version(),
                        smart_shuffle,
                        history_start: self.history_start,
                    })
                    .map_err(|_| {
                        SchedulingError::InvalidRebuild("cannot serialize Block anchor".into())
                    })?,
                ),
                ..ersatzrs_domain::PlayoutAnchor::default()
            });
        }
        Ok(())
    }
}

fn restore_boundary_state(
    request: &SchedulingRequest,
    history_start: Option<DateTime<Utc>>,
) -> Result<SchedulingSmartShuffleState, SchedulingError> {
    for item in request.reference_data.existing_items.iter().filter(|item| {
        item.finish <= request.start
            && matches!(item.filler_kind, FillerKind::None | FillerKind::GuideMode)
            && history_start.is_none_or(|floor| item.start >= floor)
    }) {
        let entry = request
            .reference_data
            .existing_history
            .iter()
            .find(|entry| {
                entry.when == item.start
                    && entry.finish == Some(item.finish)
                    && entry.key.contains("\"BlockId\"")
            })
            .ok_or_else(|| {
                SchedulingError::InvalidRebuild(
                    "missing Block programme history; use an explicit Reset".into(),
                )
            })?;
        checkpoint_smart_shuffle(&entry.details)?;
    }
    if let Some(entry) = request
        .reference_data
        .existing_history
        .iter()
        .filter(|entry| entry.key.contains("\"BlockId\""))
        .max_by_key(|entry| entry.when)
    {
        return checkpoint_smart_shuffle(&entry.details);
    }
    Ok(SchedulingSmartShuffleState::default())
}
