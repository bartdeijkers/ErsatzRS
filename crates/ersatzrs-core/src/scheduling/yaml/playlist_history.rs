//! Sparse playlist snapshots retain the original membership for exact replay.
//! Reconcile against current media only after reconstructing the saved position.
use super::{
    cleanup_history, BTreeMap, BTreeSet, DateTime, Deserialize, Duration, EntityId,
    PlaylistEnumerator, SchedulingError, Serialize, Utc, YamlContext,
};
use ersatzrs_domain::PlayoutHistory;

#[derive(Serialize, Deserialize)]
struct PlaylistHistory {
    version: u32,
    cycle: u64,
    index: usize,
    seed: Option<i32>,
    media_details: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    snapshot: Option<PlaylistEnumerator>,
}

impl YamlContext {
    pub(super) fn playlist_history_details(
        &mut self,
        content: &str,
        media_details: String,
    ) -> String {
        let playlist = &self.playlist_enumerators[content];
        let previous = self
            .playlist_history_positions
            .insert(content.to_owned(), (playlist.cycle, playlist.index));
        let snapshot =
            previous.is_none_or(|(cycle, index)| cycle != playlist.cycle || index > playlist.index);
        serde_json::to_string(&PlaylistHistory {
            version: 1,
            cycle: playlist.cycle,
            index: playlist.index,
            seed: playlist.shuffle_seed,
            media_details,
            snapshot: snapshot.then(|| playlist.clone()),
        })
        .expect("playlist history contains JSON-compatible values")
    }

    pub(super) fn restore_playlist_history(
        &mut self,
        content: &str,
        history_key: &str,
        ignored: &BTreeSet<EntityId>,
    ) -> Result<(), SchedulingError> {
        let histories = self
            .reference_data
            .existing_history
            .iter()
            .filter(|entry| entry.key == history_key && entry.child_key.is_none())
            .filter(|entry| entry.when <= self.current_time && !ignored.contains(&entry.id))
            .collect::<Vec<_>>();
        let unsafe_history = || {
            SchedulingError::InvalidYaml(
            "YAML playlist history cannot safely restore its cycle; use Reset Playout to create compatible history".to_owned()
        )
        };
        let Some(latest) = histories.iter().max_by_key(|entry| (entry.when, entry.id)) else {
            let previously_used = self
                .reference_data
                .existing_history
                .iter()
                .any(|entry| entry.key == history_key)
                || self.reference_data.existing_items.iter().any(|item| {
                    item.scheduling_context
                        .as_deref()
                        .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok())
                        .is_some_and(|value| value["content"].as_str() == Some(content))
                });
            return if previously_used {
                Err(unsafe_history())
            } else {
                Ok(())
            };
        };
        let target: PlaylistHistory =
            serde_json::from_str(&latest.details).map_err(|_| unsafe_history())?;
        if target.version != 1 {
            return Err(unsafe_history());
        }
        let mut snapshot = histories
            .iter()
            .filter_map(|entry| {
                let state: PlaylistHistory = serde_json::from_str(&entry.details).ok()?;
                (state.version == 1
                    && state.cycle == target.cycle
                    && state.seed == target.seed
                    && state.index <= target.index)
                    .then_some((entry.when, entry.id, state.snapshot?))
            })
            .max_by_key(|(when, id, _)| (*when, *id))
            .map(|(_, _, snapshot)| snapshot)
            .ok_or_else(unsafe_history)?;
        let media_ids = snapshot
            .children
            .iter()
            .flat_map(|child| child.enumerator.items.iter().map(|item| item.id))
            .collect::<BTreeSet<_>>();
        if snapshot.child_index >= snapshot.children.len()
            || snapshot
                .children
                .iter()
                .any(|child| child.enumerator.index >= child.enumerator.items.len())
            || snapshot.all_media_ids != media_ids
            || !snapshot.remaining_media_ids.is_subset(&media_ids)
        {
            return Err(unsafe_history());
        }
        while snapshot.index < target.index && snapshot.cycle == target.cycle {
            snapshot.advance();
        }
        if snapshot.cycle != target.cycle
            || snapshot.index != target.index
            || snapshot.shuffle_seed != target.seed
        {
            return Err(unsafe_history());
        }
        if latest.when < self.current_time {
            snapshot.advance();
        }
        if let Some(playlist) = self.playlist_enumerator(content) {
            super::super::classic::restore_playlist_checkpoint(
                playlist,
                &super::super::classic::playlist_checkpoint(&snapshot),
            );
        }
        Ok(())
    }
}

pub(super) fn cleanup_with_playlist_snapshots(
    histories: &[PlayoutHistory],
    start: DateTime<Utc>,
    offset: Duration,
) -> Vec<EntityId> {
    let mut removed = cleanup_history(histories, start, offset)
        .into_iter()
        .collect::<BTreeSet<_>>();
    let mut earliest_kept = BTreeMap::<&str, DateTime<Utc>>::new();
    for history in histories
        .iter()
        .filter(|entry| entry.child_key.is_none() && !removed.contains(&entry.id))
    {
        earliest_kept
            .entry(&history.key)
            .and_modify(|when| *when = (*when).min(history.when))
            .or_insert(history.when);
    }
    for (key, when) in earliest_kept {
        let baseline = histories
            .iter()
            .filter(|entry| entry.key == key && entry.child_key.is_none() && entry.when <= when)
            .filter(|entry| {
                serde_json::from_str::<PlaylistHistory>(&entry.details)
                    .is_ok_and(|state| state.version == 1 && state.snapshot.is_some())
            })
            .max_by_key(|entry| (entry.when, entry.id));
        if let Some(baseline) = baseline {
            removed.remove(&baseline.id);
        }
    }
    removed.into_iter().collect()
}
