use chrono::{NaiveDate, TimeZone};
use ersatzrs_domain::{Playout, PlayoutMode, PlayoutScheduleKind};

use super::*;

fn fixed_item(hour: i64, minute: i64) -> ProgramScheduleItem {
    ProgramScheduleItem {
        start_type: StartType::Fixed,
        start_time: Some(Duration::hours(hour) + Duration::minutes(minute)),
        ..tests::schedule_item(1, PlayoutMode::One, 1, None)
    }
}

fn state(current_time: DateTime<Utc>) -> ClassicState {
    ClassicState {
        current_time,
        schedule_state: CollectionEnumeratorState::default(),
        multiple_remaining: None,
        duration_finish: None,
        in_flood: false,
        in_duration_filler: false,
        next_guide_group: 1,
    }
}

#[test]
fn flexible_fixed_start_midnight_and_twelve_hour_boundary() {
    for (current, hour, minute, expected) in [
        ("2026-04-26T00:10:00Z", 0, 5, "2026-04-26T00:10:00Z"),
        ("2026-04-26T23:54:58Z", 0, 0, "2026-04-27T00:00:00Z"),
        ("2026-04-26T23:50:00Z", 0, 30, "2026-04-27T00:30:00Z"),
        ("2026-04-26T22:30:00Z", 0, 0, "2026-04-27T00:00:00Z"),
        ("2026-04-26T00:20:01Z", 0, 0, "2026-04-26T00:20:01Z"),
        ("2026-04-26T06:10:00Z", 6, 0, "2026-04-26T06:10:00Z"),
        ("2026-04-26T12:00:00Z", 18, 0, "2026-04-26T18:00:00Z"),
        ("2026-04-26T00:01:00Z", 23, 59, "2026-04-26T00:01:00Z"),
        ("2026-04-26T12:00:00Z", 0, 0, "2026-04-27T00:00:00Z"),
        ("2026-04-26T11:59:59Z", 0, 0, "2026-04-26T11:59:59Z"),
        ("2026-04-26T00:00:00Z", 0, 0, "2026-04-26T00:00:00Z"),
    ] {
        let current = current.parse().unwrap();
        assert_eq!(
            get_start_time_after(
                current,
                &fixed_item(hour, minute),
                FixedStartTimeBehavior::Flexible,
                &state(current),
                false,
                &ClassicClock::default()
            )
            .unwrap(),
            expected.parse::<DateTime<Utc>>().unwrap()
        );
    }
}

#[test]
fn every_2026_local_midnight_waits_for_tomorrow() {
    for zone in [
        "UTC",
        "Europe/Amsterdam",
        "America/New_York",
        "Australia/Lord_Howe",
    ] {
        let clock = ClassicClock::named(zone).unwrap();
        let mut day = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        for _ in 0..365 {
            // Resolve noon to identify this civil day without relying on host TZ.
            let noon = clock
                .occurrences(
                    day.and_hms_opt(12, 0, 0).unwrap().and_utc(),
                    Duration::hours(12),
                )
                .unwrap()
                .0;
            let (_, tomorrow) = clock.occurrences(noon, Duration::zero()).unwrap();
            let current = tomorrow - Duration::minutes(10);
            assert_eq!(
                get_start_time_after(
                    current,
                    &fixed_item(0, 0),
                    FixedStartTimeBehavior::Flexible,
                    &state(current),
                    false,
                    &clock
                )
                .unwrap(),
                tomorrow,
                "{zone} {day}"
            );
            day = day.succ_opt().unwrap();
        }
    }
}

#[test]
fn incomplete_modes_keep_current_time_but_flood_peek_waits() {
    let current = Utc.with_ymd_and_hms(2026, 4, 26, 0, 10, 0).unwrap();
    for mode in [
        PlayoutMode::Multiple,
        PlayoutMode::Duration,
        PlayoutMode::Flood,
    ] {
        let mut item = fixed_item(0, 30);
        item.playout_mode = mode;
        let mut state = state(current);
        state.multiple_remaining = Some(2);
        state.duration_finish = Some(current + Duration::hours(1));
        state.in_flood = true;
        assert_eq!(
            get_start_time_after(
                current,
                &item,
                FixedStartTimeBehavior::Flexible,
                &state,
                false,
                &ClassicClock::default()
            )
            .unwrap(),
            current
        );
    }
    let mut state = state(current);
    state.in_flood = true;
    assert_eq!(
        get_start_time_after(
            current,
            &fixed_item(0, 0),
            FixedStartTimeBehavior::Flexible,
            &state,
            true,
            &ClassicClock::default()
        )
        .unwrap(),
        Utc.with_ymd_and_hms(2026, 4, 27, 0, 0, 0).unwrap()
    );
}

#[test]
fn flexible_builder_keeps_three_fixed_items_across_midnight() {
    let start = Utc.with_ymd_and_hms(2026, 4, 26, 23, 0, 0).unwrap();
    let mut data = SchedulingReferenceData {
        classic_fixed_start_time_behavior: FixedStartTimeBehavior::Flexible,
        ..SchedulingReferenceData::default()
    };
    for (id, hour, minute) in [(1, 23, 30), (2, 0, 0), (3, 0, 30)] {
        let mut item = fixed_item(hour, minute);
        item.id = id;
        item.collection_id = Some(id);
        item.sort_order = i32::try_from(id).unwrap();
        data.media_by_key.insert(
            item_key(&item),
            vec![SchedulingMediaItem::new(id, Duration::minutes(25))],
        );
        data.classic_schedule_items.push(item);
    }
    let result = ClassicSchedulingEngine
        .schedule(SchedulingRequest {
            playout: Playout {
                id: 50,
                schedule_kind: PlayoutScheduleKind::Classic,
                seed: 7,
                ..Playout::default()
            },
            start,
            finish: start + Duration::minutes(150),
            mode: PlayoutBuildMode::Reset,
            reference_data: data,
        })
        .unwrap();
    assert_eq!(
        result
            .added_items
            .iter()
            .map(|item| (item.media_item_id, (item.start - start).num_minutes()))
            .collect::<Vec<_>>(),
        vec![(1, 30), (2, 60), (3, 90), (1, 115), (2, 140)]
    );
}

#[test]
fn strict_dst_gap_keeps_upstream_wall_time_comparison() {
    // 03:10 local has passed the missing 02:30 start. Upstream Strict carries
    // today's resolved standard offset into AddDays(1); Flexible resolves anew.
    let current = "2026-03-29T01:10:00Z".parse().unwrap();
    assert_eq!(
        get_start_time_after(
            current,
            &fixed_item(2, 30),
            FixedStartTimeBehavior::Strict,
            &state(current),
            false,
            &ClassicClock::named("Europe/Amsterdam").unwrap()
        )
        .unwrap(),
        "2026-03-30T01:30:00Z".parse::<DateTime<Utc>>().unwrap()
    );
}

#[test]
fn first_flexible_item_does_not_pre_advance_almost_a_day() {
    let start = "2026-04-26T00:01:00Z".parse::<DateTime<Utc>>().unwrap();
    let mut item = fixed_item(23, 59);
    item.fixed_start_time_behavior = Some(FixedStartTimeBehavior::Flexible);
    let mut reference_data = SchedulingReferenceData {
        classic_fixed_start_time_behavior: FixedStartTimeBehavior::Strict,
        classic_schedule_items: vec![item.clone()],
        ..SchedulingReferenceData::default()
    };
    reference_data.media_by_key.insert(
        item_key(&item),
        vec![SchedulingMediaItem::new(101, Duration::minutes(25))],
    );
    let result = ClassicSchedulingEngine
        .schedule(SchedulingRequest {
            playout: Playout {
                id: 50,
                schedule_kind: PlayoutScheduleKind::Classic,
                seed: 7,
                ..Playout::default()
            },
            start,
            finish: start + Duration::minutes(20),
            mode: PlayoutBuildMode::Reset,
            reference_data,
        })
        .unwrap();
    assert_eq!(result.added_items.len(), 1);
    assert_eq!(result.added_items[0].start, start);
}
