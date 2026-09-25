use ersatzrs_core::scheduling::{ClassicClock, SchedulingResult};
use ersatzrs_domain::{FixedStartTimeBehavior, StartType};

use super::*;

#[tokio::test]
async fn classic_local_midnight_dst_survives_persisted_continue_and_refresh() {
    for start in ["2026-03-28T22:50:00Z", "2026-10-24T21:50:00Z"] {
        let start = start.parse::<DateTime<Utc>>().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("fixed-start.sqlite");
        let pool = open_database(&path, true).await;
        seed_acceptance_fixture(&pool).await;
        let repositories = make_repositories(pool.clone());
        let mut item = repositories.program_schedule.get_item(401).await.unwrap();
        item.start_type = StartType::Fixed;
        item.start_time = Some(Duration::zero());
        item.fixed_start_time_behavior = Some(FixedStartTimeBehavior::Flexible);
        item.playout_mode = PlayoutMode::One;
        repositories
            .program_schedule
            .upsert_item(&item)
            .await
            .unwrap();
        let seam = start + Duration::days(2);
        let finish = start + Duration::days(4);
        let oracle = build_clock_window(&pool, PlayoutBuildMode::Reset, start, finish, false).await;
        assert_eq!(oracle.added_items[0].start, start + Duration::minutes(10));
        let first = build_clock_window(&pool, PlayoutBuildMode::Reset, start, seam, true).await;
        pool.close().await;
        let pool = open_database(&path, false).await;
        let continued =
            build_clock_window(&pool, PlayoutBuildMode::Continue, seam, finish, true).await;
        let timeline = |items: &[ersatzrs_domain::PlayoutItem]| {
            items
                .iter()
                .map(|item| {
                    (
                        item.media_item_id,
                        item.start,
                        item.finish,
                        item.scheduling_context.clone(),
                        item.preferred_audio_language_code.clone(),
                        item.preferred_subtitle_language_code.clone(),
                    )
                })
                .collect::<Vec<_>>()
        };
        let mut combined = timeline(&first.added_items);
        combined.extend(timeline(&continued.added_items));
        assert_eq!(
            combined,
            timeline(&oracle.added_items),
            "restart Continue at {start}"
        );
        pool.close().await;
        let pool = open_database(&path, false).await;
        let before = playlist_timeline(&pool).await;
        build_clock_window(&pool, PlayoutBuildMode::Refresh, seam, finish, true).await;
        let after = playlist_timeline(&pool).await;
        let future = |rows: Vec<PlaylistTimelineRow>| {
            rows.into_iter()
                .filter(|row| {
                    chrono::NaiveDateTime::parse_from_str(&row.1, "%Y-%m-%d %H:%M:%S%.f")
                        .unwrap()
                        .and_utc()
                        >= seam
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(future(after), future(before), "restart Refresh at {start}");
        pool.close().await;
    }
}

async fn build_clock_window(
    pool: &SqlitePool,
    mode: PlayoutBuildMode,
    start: DateTime<Utc>,
    finish: DateTime<Utc>,
    persist: bool,
) -> SchedulingResult {
    let repositories = make_repositories(pool.clone());
    let playout = repositories
        .playout
        .get_with_anchor(RELATED_PLAYOUT_ID)
        .await
        .unwrap();
    let mut reference_data = repositories
        .scheduling
        .classic_reference_data(RELATED_PLAYOUT_ID, start, Duration::zero())
        .await
        .unwrap();
    assert_eq!(
        reference_data.classic_clock,
        ClassicClock::system().unwrap(),
        "production loader must supply the host clock"
    );
    // Keep this DST fixture independent of the test runner's operating-system zone.
    reference_data.classic_clock = ClassicClock::named("Europe/Amsterdam").unwrap();
    let result = PlayoutBuilder::default()
        .build(SchedulingRequest {
            playout: playout.clone(),
            start,
            finish,
            mode,
            reference_data,
        })
        .unwrap();
    if persist {
        repositories
            .playout
            .apply_build_result(&result, &playout, Duration::zero())
            .await
            .unwrap();
    }
    result
}
