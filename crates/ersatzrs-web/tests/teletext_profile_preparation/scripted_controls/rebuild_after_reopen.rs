//! Actual Lua rebuilds and failed candidates preserve the published SQL boundary.

use super::continue_after_reopen::{assert_no_activation, reference, Configuration};
use super::{build, publish_result, seed_publication_inputs, Fixture, ADDON};
use chrono::{DateTime, Duration, Utc};
use ersatzrs_core::repositories::PlayoutRepository as _;
use ersatzrs_core::scheduling::{
    PlayoutBuildMode, PlayoutPreservation, SchedulingError, SchedulingMediaItem,
    SchedulingReferenceData, SchedulingRequest, SchedulingResult, ScriptedTeletextControl,
};
use ersatzrs_domain::{Playout, TeletextOverrideInterval, TeletextOverrideState};
use ersatzrs_infra::repositories::SqlitePlayoutRepository;
use std::sync::Arc;

const MODES: [(PlayoutBuildMode, PlayoutPreservation, &str); 3] = [
    (
        PlayoutBuildMode::Refresh,
        PlayoutPreservation::KeepCurrent,
        "refresh",
    ),
    (
        PlayoutBuildMode::Reset,
        PlayoutPreservation::KeepCurrent,
        "reset",
    ),
    (
        PlayoutBuildMode::Reset,
        PlayoutPreservation::EngineDefault,
        "reset",
    ),
];

#[derive(Debug, PartialEq)]
struct Published {
    reference: [SchedulingReferenceData; 2],
    playout: [Playout; 2],
    intervals: Vec<Vec<TeletextOverrideInterval>>,
    configuration: Configuration,
}

impl Published {
    async fn read(fixture: &Fixture, start: DateTime<Utc>) -> Self {
        let repository = SqlitePlayoutRepository(fixture.state.pool.clone());
        let mut intervals = Vec::new();
        for channel in [1, 2] {
            for seconds in [0, 30, 60, 90, 120, 150, 180] {
                intervals.push(
                    repository
                        .published_teletext_intervals_at(
                            channel,
                            start + Duration::seconds(seconds),
                        )
                        .await
                        .unwrap(),
                );
            }
        }
        Self {
            reference: [reference(fixture, 1).await, reference(fixture, 2).await],
            playout: [
                repository.get_with_anchor(93).await.unwrap(),
                repository.get_with_anchor(94).await.unwrap(),
            ],
            intervals,
            configuration: Configuration::read(fixture).await,
        }
    }
}

async fn seed() -> (Fixture, SchedulingResult, i64) {
    let fixture = Fixture::new_persistent().await;
    let day = fixture.profile("DayBase", "day", "good").await;
    let night = fixture.profile("NightOverride", "night", "good").await;
    fixture.bind(1, &day, None).await;
    fixture.bind(2, &night, None).await;
    seed_publication_inputs(&fixture).await;
    let mut original = None;
    for (channel, profile) in [(1, night.id), (2, day.id)] {
        let result = build(
            &fixture,
            &format!(
                "assert(teletext_profile('{ADDON}', {profile}).ok)\n\
                 assert(teletext_enabled('{ADDON}', true).ok)\nadd_count('clip', 3)"
            ),
            channel,
            true,
        )
        .await
        .unwrap();
        assert_eq!(result.added_items.len(), 3);
        assert_eq!(result.teletext_intervals.len(), 1);
        publish_result(&fixture, &result).await;
        if channel == 1 {
            original = Some(result);
        }
    }
    (fixture, original.unwrap(), day.id)
}

async fn rebuild(
    fixture: &Fixture,
    original: &SchedulingResult,
    mode: PlayoutBuildMode,
    preservation: PlayoutPreservation,
    script: &str,
) -> Result<SchedulingResult, SchedulingError> {
    let playout = SqlitePlayoutRepository(fixture.state.pool.clone())
        .get_with_anchor(93)
        .await
        .unwrap();
    std::fs::write(playout.schedule_file.as_ref().unwrap(), script).unwrap();
    let mut reference_data = reference(fixture, 1).await;
    reference_data.preservation = preservation;
    reference_data.media_by_key.insert(
        "clip".to_owned(),
        vec![SchedulingMediaItem::new(71, Duration::minutes(1))],
    );
    let request = SchedulingRequest {
        playout,
        start: original.added_items[0].start + Duration::seconds(30),
        finish: original.added_items.last().unwrap().finish,
        mode,
        reference_data,
    };
    let controls: Arc<dyn ScriptedTeletextControl> = fixture.state.application.clone();
    let builder = ersatzrs_infra::scripted_scheduling::playout_builder_with_teletext(
        fixture.state.pool.clone(),
        tokio::runtime::Handle::current(),
        None,
        Some(controls),
    );
    tokio::task::spawn_blocking(move || builder.build(request))
        .await
        .unwrap()
}

fn assert_candidate(result: &SchedulingResult, boundary: DateTime<Utc>, day: i64) {
    assert!(result.clear_items);
    assert_eq!(result.added_items.len(), 2);
    assert_eq!(result.added_items[0].start, boundary);
    assert_eq!(result.teletext_intervals.len(), 2);
    for (item, interval) in result.added_items.iter().zip(&result.teletext_intervals) {
        assert_eq!((interval.start, interval.finish), (item.start, item.finish));
    }
    assert!(matches!(&result.teletext_intervals[0].state,
        TeletextOverrideState::Enabled(candidate) if candidate.channel_id == 1
            && candidate.profile_id == day));
    assert!(matches!(&result.teletext_intervals[1].state,
        TeletextOverrideState::Off { channel_id: 1, addon_id } if addon_id == ADDON));
}

fn assert_history(before: &Published, accepted: &Published, result: &SchedulingResult) {
    let old = &before.reference[0].existing_history;
    let current = &accepted.reference[0].existing_history;
    assert!(!old.is_empty());
    for history in old {
        let retained = result.retain_through.is_some_and(|seam| {
            history.when < seam && history.finish.is_some_and(|finish| finish <= seam)
        });
        assert_eq!(current.contains(history), retained, "history {history:?}");
        if !retained {
            assert!(current.iter().all(|row| row.id != history.id));
        }
    }
    assert_eq!(
        current.len(),
        result.added_history.len() + usize::from(result.retain_through.is_some())
    );
}

async fn assert_published(
    fixture: &Fixture,
    original: &SchedulingResult,
    result: &SchedulingResult,
    before: &Published,
) {
    let start = original.added_items[0].start;
    let accepted = Published::read(fixture, start).await;
    assert_eq!(accepted.reference[1], before.reference[1]);
    assert_eq!(accepted.playout[1], before.playout[1]);
    assert_eq!(accepted.intervals[7..], before.intervals[7..]);
    assert_eq!(accepted.configuration, before.configuration);
    assert_history(before, &accepted, result);
    let prefix = usize::from(result.retain_through.is_some());
    let items = &accepted.reference[0].existing_items;
    assert_eq!(items.len(), prefix + result.added_items.len());
    assert_eq!(
        items[..prefix],
        before.reference[0].existing_items[..prefix]
    );
    for (stored, candidate) in items[prefix..].iter().zip(&result.added_items) {
        assert_eq!(stored.media_item_id, candidate.media_item_id);
        assert_eq!(
            (stored.start, stored.finish),
            (candidate.start, candidate.finish)
        );
    }
    let repository = SqlitePlayoutRepository(fixture.state.pool.clone());
    if let Some(seam) = result.retain_through {
        let mut clipped = original.teletext_intervals[0].clone();
        clipped.finish = seam;
        assert_eq!(
            repository
                .published_teletext_intervals_at(1, start)
                .await
                .unwrap(),
            [clipped]
        );
        let just_before = repository
            .published_teletext_intervals_at(1, seam - Duration::milliseconds(1))
            .await
            .unwrap();
        assert_eq!(just_before.len(), 1);
        assert_eq!(just_before[0].state, original.teletext_intervals[0].state);
        assert_eq!(just_before[0].finish, seam);
    } else {
        assert!(repository
            .published_teletext_intervals_at(1, start)
            .await
            .unwrap()
            .is_empty());
    }
    for interval in &result.teletext_intervals {
        assert_eq!(
            repository
                .published_teletext_intervals_at(1, interval.start)
                .await
                .unwrap(),
            std::slice::from_ref(interval)
        );
        assert_eq!(
            repository
                .published_teletext_intervals_at(1, interval.finish - Duration::milliseconds(1))
                .await
                .unwrap(),
            std::slice::from_ref(interval)
        );
    }
    let interval_count: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "PlayoutTeletextInterval" WHERE "PlayoutId" = 93"#,
    )
    .fetch_one(&fixture.state.pool)
    .await
    .unwrap();
    assert_eq!(
        interval_count,
        i64::from(result.retain_through.is_some()) + 2
    );
    assert!(repository
        .published_teletext_intervals_at(1, result.added_items.last().unwrap().finish)
        .await
        .unwrap()
        .is_empty());
    assert_eq!(accepted.playout[0].anchor, result.anchor);
    assert_eq!(
        accepted.reference[0].existing_scripted_checkpoints,
        result.scripted_checkpoints
    );
}

#[tokio::test]
async fn actual_lua_rebuild_modes_publish_fresh_base_and_off_at_durable_programme_seams() {
    for (mode, preservation, name) in MODES {
        let (fixture, original, day) = seed().await;
        let start = original.added_items[0].start;
        let before = Published::read(&fixture, start).await;
        let mut fixture = fixture.reopen().await;
        assert_eq!(Published::read(&fixture, start).await, before);
        let script = format!(
            "assert(build_mode() == '{name}')\n\
             assert(teletext_enabled('{ADDON}', true).ok)\nadd_count('clip', 1)\n\
             assert(teletext_enabled('{ADDON}', false).ok)\nadd_count('clip', 1)"
        );
        let result = rebuild(&fixture, &original, mode, preservation, &script)
            .await
            .unwrap();
        let keep = preservation == PlayoutPreservation::KeepCurrent;
        let seam = if keep {
            original.added_items[0].finish
        } else {
            start + Duration::seconds(30)
        };
        assert_eq!(result.retain_through, keep.then_some(seam));
        assert_candidate(&result, seam, day);
        // Candidate evaluation alone cannot overwrite any accepted timeline.
        assert_eq!(Published::read(&fixture, start).await, before);
        publish_result(&fixture, &result).await;
        assert_published(&fixture, &original, &result, &before).await;
        assert_no_activation(&mut fixture).await;
        let published = Published::read(&fixture, start).await;
        fixture = fixture.reopen().await;
        assert_eq!(Published::read(&fixture, start).await, published);
        assert_no_activation(&mut fixture).await;
    }
}

#[tokio::test]
async fn actual_lua_rebuild_errors_leave_prior_publication_and_other_channel_unchanged() {
    for (mode, preservation, name) in MODES {
        let (fixture, original, _) = seed().await;
        let start = original.added_items[0].start;
        let mut fixture = fixture.reopen().await;
        let before = Published::read(&fixture, start).await;
        let script = format!(
            "assert(build_mode() == '{name}')\n\
             assert(teletext_enabled('{ADDON}', false).ok)\nadd_count('clip', 1)\n\
             local r=teletext_profile('{ADDON}', 999999)\n\
             assert(not r.ok and r.code == 'missing_profile')\nadd_count('clip', 1)"
        );
        let handled = rebuild(&fixture, &original, mode, preservation, &script)
            .await
            .unwrap();
        assert_eq!(handled.teletext_intervals.len(), 1);
        assert!(matches!(&handled.teletext_intervals[0].state,
            TeletextOverrideState::Off { channel_id: 1, addon_id } if addon_id == ADDON));
        assert_eq!(
            handled.teletext_intervals[0].start,
            handled.added_items[0].start
        );
        assert_eq!(
            handled.teletext_intervals[0].finish,
            handled.added_items.last().unwrap().finish
        );
        assert_eq!(Published::read(&fixture, start).await, before);
        for rejected in [
            format!("{script}\nerror('synthetic rebuild rollback')"),
            "this is not valid lua!".to_owned(),
        ] {
            assert!(matches!(
                rebuild(&fixture, &original, mode, preservation, &rejected).await,
                Err(SchedulingError::Script(_))
            ));
            assert_eq!(Published::read(&fixture, start).await, before);
            assert_no_activation(&mut fixture).await;
        }
        fixture = fixture.reopen().await;
        assert_eq!(Published::read(&fixture, start).await, before);
        assert_no_activation(&mut fixture).await;
    }
}
