//! Block source continuity shares the existing seeded enumerators and membership
//! reconciliation. History carries the next cursor, not a future tail cursor.
use super::{
    block::{history_key_for_block_item, EffectiveBlockItem},
    classic, lookup_media, DateTime, HistoryDetails, MediaEnumerator, PlaybackOrder,
    SchedulingError, SchedulingMediaItem, SchedulingRequest, SchedulingResult,
    SchedulingSmartShuffleState, Utc,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Checkpoint {
    version: u32,
    source: classic::ClassicMediaCheckpoint,
    playout_seed: i32,
    seed: i32,
    draw_index: usize,
    smart_shuffle: SchedulingSmartShuffleState,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct SourceHistory {
    #[serde(flatten)]
    media: HistoryDetails,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    block_checkpoint: Option<Checkpoint>,
}

pub(super) struct BlockSource {
    pub(super) enumerator: MediaEnumerator,
    pub(super) seed: i32,
    playout_seed: i32,
    draw_index: usize,
}

fn invalid(reason: &str) -> SchedulingError {
    SchedulingError::InvalidRebuild(format!("cannot restore Block source continuity: {reason}"))
}

/// Validate complete persisted Block source state before a preserving rebuild.
///
/// # Errors
/// Refuses missing, malformed or incompatible history checkpoint state.
pub fn validate_block_history_checkpoint(details: &str) -> Result<(), SchedulingError> {
    checkpoint_smart_shuffle(details).map(|_| ())
}

pub(super) fn checkpoint_smart_shuffle(
    details: &str,
) -> Result<SchedulingSmartShuffleState, SchedulingError> {
    let history: SourceHistory = serde_json::from_str(details)
        .map_err(|_| invalid("malformed history; use an explicit Reset"))?;
    let checkpoint = history
        .block_checkpoint
        .ok_or_else(|| invalid("missing history checkpoint; use an explicit Reset"))?;
    if checkpoint.version != 1
        || !checkpoint.source.is_valid()
        || checkpoint.smart_shuffle.version != SchedulingSmartShuffleState::default().version
    {
        return Err(invalid(
            "incompatible history checkpoint; use an explicit Reset",
        ));
    }
    Ok(checkpoint.smart_shuffle)
}

impl BlockSource {
    pub(super) fn restore(
        request: &SchedulingRequest,
        block_item: &EffectiveBlockItem,
        current_time: DateTime<Utc>,
        result: &SchedulingResult,
    ) -> Result<Self, SchedulingError> {
        let order = block_item.playback_order;
        let mut source = Self {
            enumerator: lookup_media(&request.reference_data, &block_item.content)
                .with_playback_order(order),
            seed: request
                .playout
                .seed
                .wrapping_add(i32::try_from(block_item.block_id).unwrap_or_default()),
            draw_index: 0,
            playout_seed: request.playout.seed,
        };
        // Repository ordering must not affect seeded draws after a restart.
        source.enumerator.items.sort_by_key(|item| item.id);
        let key = history_key_for_block_item(block_item);
        let explicit_reset = request.mode == super::PlayoutBuildMode::Reset
            && request.reference_data.preservation != super::PlayoutPreservation::EngineDefault;
        let previous = request
            .reference_data
            .existing_history
            .iter()
            .filter(|entry| !explicit_reset && !result.history_to_remove.contains(&entry.id))
            .chain(result.added_history.iter())
            .filter(|entry| entry.key == key && entry.when < current_time)
            .max_by_key(|entry| entry.when);
        if matches!(
            order,
            PlaybackOrder::Chronological | PlaybackOrder::SeasonEpisode
        ) {
            super::sort_media_items_for_playback_order(&mut source.enumerator.items, order);
            if let Some(entry) = previous {
                source.enumerator.apply_history(entry);
                if order == PlaybackOrder::SeasonEpisode {
                    source.enumerator.move_after_history_details(&entry.details);
                }
            }
            return Ok(source);
        }
        if let Some(entry) = previous {
            let history: SourceHistory =
                serde_json::from_str(&entry.details).map_err(|_| invalid("malformed history"))?;
            if let Some(checkpoint) = history.block_checkpoint {
                if checkpoint.version != 1
                    || !checkpoint.source.is_valid()
                    || !checkpoint.source.matches_order(order)
                    || checkpoint.smart_shuffle.version
                        != SchedulingSmartShuffleState::default().version
                {
                    return Err(invalid("incompatible history checkpoint"));
                }
                // Reset changes the playout seed. An archived source that only
                // returns later must not revive a pre-Reset randomized cycle.
                if checkpoint.playout_seed != source.playout_seed {
                    source.prepare_initial(order);
                    return Ok(source);
                }
                source.seed = checkpoint.seed;
                source.draw_index = checkpoint.draw_index;
                let changed =
                    classic::restore_media_checkpoint(&mut source.enumerator, &checkpoint.source);
                if changed && order == PlaybackOrder::Random {
                    source.select_random();
                }
                return Ok(source);
            }
            // Existing databases lack the exact in-cycle order. Preserve legacy
            // continuation from its recorded index; explicit preservation must
            // not pretend that this is a complete boundary checkpoint.
            if request.reference_data.preservation != super::PlayoutPreservation::EngineDefault {
                return Err(invalid("missing history checkpoint; use an explicit Reset"));
            }
            source.draw_index = usize::try_from(entry.index.saturating_add(1)).unwrap_or_default();
        }
        source.prepare_initial(order);
        Ok(source)
    }

    fn select_random(&mut self) {
        classic::select_random_draw(
            &mut self.enumerator,
            u64::from(self.seed.cast_unsigned()),
            self.draw_index,
        );
    }

    fn prepare_initial(&mut self, order: PlaybackOrder) {
        if order == PlaybackOrder::Random {
            self.select_random();
        } else {
            self.seed =
                classic::prepare_next_cycle(&mut self.enumerator, order, self.seed, None, None);
            self.enumerator.set_index(self.draw_index);
        }
    }

    /// Advance before serializing, so wrapping and the next random draw survive
    /// process restart without replaying the item just emitted.
    pub(super) fn history_details(
        &mut self,
        media: &SchedulingMediaItem,
        smart_shuffle: &SchedulingSmartShuffleState,
    ) -> Result<String, SchedulingError> {
        self.draw_index = self
            .draw_index
            .checked_add(1)
            .ok_or_else(|| invalid("draw counter overflow"))?;
        let order = self.enumerator.playback_order;
        if order == PlaybackOrder::Random {
            self.select_random();
        } else {
            self.enumerator.advance();
            if self.enumerator.current_index() == 0 && super::is_randomized_playback_order(order) {
                self.seed = classic::prepare_next_cycle(
                    &mut self.enumerator,
                    order,
                    classic::next_i32_seed(self.seed),
                    Some(media.id),
                    Some(classic::shuffle_group_key(media)),
                );
            }
        }
        serde_json::to_string(&SourceHistory {
            media: HistoryDetails {
                media_item_id: Some(media.id),
                release_date: media.release_date,
                season_number: media.season,
                episode_number: media.episode,
                ..HistoryDetails::default()
            },
            block_checkpoint: Some(Checkpoint {
                version: 1,
                source: classic::media_checkpoint(&self.enumerator),
                playout_seed: self.playout_seed,
                seed: self.seed,
                draw_index: self.draw_index,
                smart_shuffle: smart_shuffle.clone(),
            }),
        })
        .map_err(|_| invalid("cannot serialize history checkpoint"))
    }
}
