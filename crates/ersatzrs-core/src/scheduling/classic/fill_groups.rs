//! ErsatzTV's `PlayoutBuilder` synthetic schedule items and independent full source keys.
use super::{FillWithGroupMode, ProgramScheduleItem, SchedulingReferenceData};
use ersatzrs_domain::EntityId;
use std::collections::BTreeSet;

#[cfg(test)]
use super::{
    item_key, ClassicSchedulingEngine, Duration, PlaybackOrder, PlayoutBuildMode,
    PlayoutSchedulingEngine, SchedulingCollectionKey, SchedulingMediaItem, SchedulingRequest,
    TailMode,
};
#[cfg(test)]
use chrono::Utc;
#[cfg(test)]
use std::collections::BTreeMap;

pub(super) fn groups(
    reference: &SchedulingReferenceData,
    item: &ProgramScheduleItem,
) -> Vec<ProgramScheduleItem> {
    if item.fill_with_group_mode == FillWithGroupMode::None {
        return Vec::new();
    }
    reference
        .classic_fill_groups
        .get(&item.id)
        .into_iter()
        .flatten()
        .filter(|group| !group.media_items.is_empty())
        .map(|group| {
            let key = &group.collection_key;
            ProgramScheduleItem {
                collection_type: key.collection_type,
                collection_id: key.collection_id,
                media_item_id: key.media_item_id,
                multi_collection_id: key.multi_collection_id,
                smart_collection_id: key.smart_collection_id,
                rerun_collection_id: key.rerun_collection_id,
                playlist_id: key.playlist_id,
                search_query: key.search_query.clone(),
                fake_collection_key: key.fake_collection_key.clone(),
                ..item.clone()
            }
        })
        .collect()
}

pub(super) fn source_items(reference: &SchedulingReferenceData) -> Vec<ProgramScheduleItem> {
    reference
        .classic_schedule_items
        .iter()
        .flat_map(|item| std::iter::once(item.clone()).chain(groups(reference, item)))
        .collect()
}

pub(super) fn item_ids(reference: &SchedulingReferenceData) -> BTreeSet<EntityId> {
    reference
        .classic_schedule_items
        .iter()
        .filter(|item| !groups(reference, item).is_empty())
        .map(|item| item.id)
        .collect()
}

#[cfg(test)]
pub(super) fn test_group(
    reference: &mut SchedulingReferenceData,
    id: EntityId,
) -> SchedulingCollectionKey {
    let item = reference
        .classic_schedule_items
        .iter()
        .find(|item| item.id == id)
        .unwrap();
    let key = SchedulingCollectionKey {
        collection_type: ersatzrs_domain::CollectionType::FakeCollection,
        fake_collection_key: Some(format!("group:{id}")),
        ..SchedulingCollectionKey::default()
    };
    let media_items = reference
        .media_by_key
        .get(&item_key(item))
        .cloned()
        .unwrap_or_else(|| vec![SchedulingMediaItem::new(100, Duration::minutes(10))]);
    reference.classic_fill_groups.insert(
        id,
        vec![crate::scheduling::SchedulingPlaylistItem {
            collection_key: key.clone(),
            playback_order: item.playback_order,
            play_all: true,
            count: None,
            media_items,
        }],
    );
    key
}

#[cfg(test)]
mod tests {
    use super::super::tests::schedule_item;
    use super::*;
    use crate::scheduling::SchedulingPlaylistItem;
    use chrono::TimeZone;
    use ersatzrs_domain::{CollectionType, Playout, PlayoutMode, PlayoutScheduleKind};

    fn request() -> SchedulingRequest {
        let item = ProgramScheduleItem {
            fill_with_group_mode: FillWithGroupMode::FillWithOrderedGroups,
            ..schedule_item(1, PlayoutMode::Multiple, 1, Some("1"))
        };
        let groups = [("movies", vec![1, 2, 3]), ("other", vec![4, 5, 6])]
            .into_iter()
            .map(|(key, ids)| SchedulingPlaylistItem {
                collection_key: SchedulingCollectionKey {
                    collection_type: CollectionType::FakeCollection,
                    fake_collection_key: Some(key.into()),
                    ..SchedulingCollectionKey::default()
                },
                playback_order: PlaybackOrder::Chronological,
                play_all: false,
                count: Some(1),
                media_items: ids
                    .into_iter()
                    .map(|id| SchedulingMediaItem::new(id, Duration::hours(1)))
                    .collect(),
            })
            .collect::<Vec<_>>();
        let data = SchedulingReferenceData {
            classic_schedule_items: vec![item.clone()],
            media_by_key: BTreeMap::from([(
                item_key(&item),
                groups.iter().flat_map(|g| g.media_items.clone()).collect(),
            )]),
            classic_fill_groups: BTreeMap::from([(1, groups)]),
            ..SchedulingReferenceData::default()
        };
        let start = Utc.with_ymd_and_hms(2026, 11, 11, 0, 0, 0).unwrap();
        SchedulingRequest {
            playout: Playout {
                id: 50,
                seed: 7,
                schedule_kind: PlayoutScheduleKind::Classic,
                ..Playout::default()
            },
            start,
            finish: start + Duration::hours(6),
            mode: PlayoutBuildMode::Continue,
            reference_data: data,
        }
    }

    #[test]
    fn classic_fill_groups_alternate_actual_key_only_sources() {
        let result = ClassicSchedulingEngine.schedule(request()).unwrap();
        assert_eq!(
            result
                .added_items
                .iter()
                .map(|i| i.media_item_id)
                .collect::<Vec<_>>(),
            [1, 4, 2, 5, 3, 6]
        );
        assert_eq!(
            result
                .program_schedule_anchors
                .iter()
                .filter(|a| a.anchor_date.is_none())
                .count(),
            3
        );
    }

    #[test]
    fn classic_fill_groups_continue_from_each_own_full_key() {
        let original = request();
        let full = ClassicSchedulingEngine.schedule(original.clone()).unwrap();
        let mut partial = original.clone();
        partial.finish = original.start + Duration::hours(3);
        let first = ClassicSchedulingEngine.schedule(partial).unwrap();
        let mut continued = super::super::preservation_tests::published_request(&original, &first);
        continued.mode = PlayoutBuildMode::Continue;
        let second = ClassicSchedulingEngine.schedule(continued).unwrap();
        assert_eq!(
            second
                .added_items
                .iter()
                .map(|i| i.media_item_id)
                .collect::<Vec<_>>(),
            [5, 3, 6]
        );
        assert_eq!(second.added_items, full.added_items[3..]);
    }

    #[test]
    fn classic_fill_groups_empty_descriptor_falls_back_without_group_state() {
        let mut request = request();
        request.reference_data.classic_fill_groups.clear();
        let result = ClassicSchedulingEngine.schedule(request).unwrap();
        assert_eq!(
            result
                .added_items
                .iter()
                .map(|i| i.media_item_id)
                .collect::<Vec<_>>(),
            [1, 2, 3, 4, 5, 6]
        );
        assert!(result.fill_group_indices.is_empty());
    }

    #[test]
    fn classic_custom_order_shrink_resets_to_beginning_and_empty_resume_is_finite() {
        let mut original = request();
        let item = &mut original.reference_data.classic_schedule_items[0];
        item.fill_with_group_mode = FillWithGroupMode::None;
        item.playback_order = PlaybackOrder::None;
        original.reference_data.classic_fill_groups.clear();
        let mut first_request = original.clone();
        first_request.finish = original.start + Duration::hours(3);
        let first = ClassicSchedulingEngine.schedule(first_request).unwrap();
        assert_eq!(
            first
                .added_items
                .iter()
                .map(|i| i.media_item_id)
                .collect::<Vec<_>>(),
            [1, 2, 3]
        );
        for remaining in [2, 0] {
            let mut continued =
                super::super::preservation_tests::published_request(&original, &first);
            continued.mode = PlayoutBuildMode::Continue;
            continued
                .reference_data
                .media_by_key
                .values_mut()
                .for_each(|items| items.truncate(remaining));
            let result = ClassicSchedulingEngine.schedule(continued).unwrap();
            let expected = if remaining == 2 {
                vec![1, 2, 1]
            } else {
                Vec::new()
            };
            assert_eq!(
                result
                    .added_items
                    .iter()
                    .map(|i| i.media_item_id)
                    .collect::<Vec<_>>(),
                expected
            );
        }
    }

    #[test]
    fn classic_fill_groups_supported_orders_and_shuffled_cycles_resume_without_crossing_anchors() {
        for order in [
            PlaybackOrder::None,
            PlaybackOrder::SeasonEpisode,
            PlaybackOrder::ShuffleInOrder,
            PlaybackOrder::MultiEpisodeShuffle,
            PlaybackOrder::Chronological,
            PlaybackOrder::Shuffle,
            PlaybackOrder::Random,
            PlaybackOrder::RandomRotation,
            PlaybackOrder::Marathon,
        ] {
            for fill in [
                FillWithGroupMode::FillWithOrderedGroups,
                FillWithGroupMode::FillWithShuffledGroups,
            ] {
                for mode in [
                    ersatzrs_domain::PlayoutMode::One,
                    ersatzrs_domain::PlayoutMode::Multiple,
                    ersatzrs_domain::PlayoutMode::Duration,
                ] {
                    let mut original = request();
                    original.finish = original.start + Duration::hours(18);
                    let item = &mut original.reference_data.classic_schedule_items[0];
                    item.playback_order = order;
                    item.playout_mode = mode;
                    item.duration = Some(Duration::hours(2));
                    item.multiple_count = Some("2".into());
                    item.fill_with_group_mode = fill;
                    item.marathon_group_by = ersatzrs_domain::MarathonGroupBy::Show;
                    item.tail_mode = TailMode::Offline;
                    let full = ClassicSchedulingEngine.schedule(original.clone()).unwrap();
                    let mut partial = original.clone();
                    partial.finish = original.start + Duration::hours(3);
                    let first = ClassicSchedulingEngine.schedule(partial).unwrap();
                    let mut continued =
                        super::super::preservation_tests::published_request(&original, &first);
                    continued.mode = PlayoutBuildMode::Continue;
                    let second = ClassicSchedulingEngine.schedule(continued).unwrap();
                    assert_eq!(
                        second.added_items,
                        full.added_items[first.added_items.len()..],
                        "{order:?}, {fill:?}, {mode:?}"
                    );
                    assert_eq!(second.fill_group_indices, full.fill_group_indices);
                }
            }
        }
    }

    #[test]
    fn classic_fill_groups_alternate_schedule_retains_inactive_group_rotation() {
        let original = request();
        let mut first_request = original.clone();
        first_request.finish = original.start + Duration::hours(3);
        let first = ClassicSchedulingEngine.schedule(first_request).unwrap();
        let mut alternate = super::super::preservation_tests::published_request(&original, &first);
        alternate.mode = PlayoutBuildMode::Continue;
        alternate.finish = original.start + Duration::hours(6);
        alternate.reference_data.classic_schedule_items[0].id = 2;
        let mut descriptors = alternate
            .reference_data
            .classic_fill_groups
            .remove(&1)
            .unwrap();
        for descriptor in &mut descriptors {
            descriptor
                .collection_key
                .fake_collection_key
                .as_mut()
                .unwrap()
                .push_str(":alternate");
        }
        alternate
            .reference_data
            .classic_fill_groups
            .insert(2, descriptors);
        let changed = ClassicSchedulingEngine.schedule(alternate).unwrap();
        let mut returned = super::super::preservation_tests::published_request(&original, &changed);
        returned.mode = PlayoutBuildMode::Continue;
        returned.finish = original.start + Duration::hours(9);
        let result = ClassicSchedulingEngine.schedule(returned).unwrap();
        assert_eq!(
            result
                .added_items
                .iter()
                .map(|i| i.media_item_id)
                .collect::<Vec<_>>(),
            [5, 3, 6]
        );
        assert_eq!(result.fill_group_indices.len(), 2);
    }
}
