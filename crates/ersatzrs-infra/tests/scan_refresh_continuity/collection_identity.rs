use super::*;

#[tokio::test]
async fn classic_long_playlist_progress_survives_three_weeks_then_restart_refresh() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("long-playlist.sqlite");
    let pool = open_database(&path, true).await;
    seed_acceptance_fixture(&pool).await;
    pool.execute(r#"
        INSERT INTO "Collection" ("Id", "Name", "UseCustomPlaybackOrder") VALUES (91, 'Long playlist', 1);
        UPDATE "PlaylistItem" SET "CollectionType"='Collection', "CollectionId"=91,
            "MediaItemId"=NULL WHERE "Id"=101;
        UPDATE "ProgramScheduleItem" SET
            "PreferredAudioLanguageCode"='nld', "PreferredSubtitleLanguageCode"='eng' WHERE "Id"=401;
        UPDATE "ConfigElement" SET "Value"='2' WHERE "Key"='playout.days_to_build';
    "#).await.unwrap();
    let repositories = make_repositories(pool.clone());
    let mut item = repositories.program_schedule.get_item(401).await.unwrap();
    item.playout_mode = PlayoutMode::Flood;
    repositories
        .program_schedule
        .upsert_item(&item)
        .await
        .unwrap();
    for offset in 0..300 {
        let id = 1000 + offset;
        common::seed_media_item(&pool, id, 1).await;
        seed_movie(&pool, id, &format!("Long playlist {offset:03}")).await;
        sqlx::query(r#"INSERT INTO "CollectionItem" ("CollectionId", "MediaItemId", "CustomIndex") VALUES (91, ?, ?)"#)
            .bind(id).bind(offset).execute(&pool).await.unwrap();
    }
    pool.execute(r#"UPDATE "MediaVersion" SET "Duration"='06:00:00' WHERE "MovieId">=1000"#)
        .await
        .unwrap();
    let start = common::fixed_time()
        .date_naive()
        .and_hms_opt(20, 0, 0)
        .unwrap()
        .and_utc();
    run_classic_build(&pool, RELATED_PLAYOUT_ID, PlayoutBuildMode::Reset, start).await;
    for day in 1..=21 {
        run_classic_build(
            &pool,
            RELATED_PLAYOUT_ID,
            PlayoutBuildMode::Continue,
            start + Duration::days(day),
        )
        .await;
        assert_build_succeeded(&pool, RELATED_PLAYOUT_ID, "continue").await;
    }
    let refresh_at = start + Duration::days(21);
    let before = future_timeline(&pool, refresh_at).await;
    assert_eq!(
        before.first().unwrap().0,
        1084,
        "three weeks must consume 84 six-hour items"
    );
    pool.close().await;
    let pool = open_database(&path, false).await;
    run_classic_build(
        &pool,
        RELATED_PLAYOUT_ID,
        PlayoutBuildMode::Refresh,
        refresh_at,
    )
    .await;
    assert_build_succeeded(&pool, RELATED_PLAYOUT_ID, "refresh").await;
    let after = future_timeline(&pool, refresh_at).await;
    assert_eq!(
        after, before,
        "Refresh must preserve future identities, timestamps, settings and contexts"
    );
    assert!(after
        .iter()
        .all(|row| row.3.as_deref() == Some("nld") && row.4.as_deref() == Some("eng")));
    pool.close().await;
}

async fn future_timeline(pool: &SqlitePool, now: DateTime<Utc>) -> Vec<PlaylistTimelineRow> {
    playlist_timeline(pool)
        .await
        .into_iter()
        .filter(|row| {
            chrono::NaiveDateTime::parse_from_str(&row.1, "%Y-%m-%d %H:%M:%S%.f")
                .unwrap()
                .and_utc()
                >= now
        })
        .collect()
}

#[tokio::test]
async fn classic_rerun_collection_identities_survive_restart_and_reject_legacy_state() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("rerun-identities.sqlite");
    let pool = open_database(&path, true).await;
    seed_acceptance_fixture(&pool).await;
    pool.execute(r#"
        INSERT INTO "RerunCollection" ("Id", "Name", "CollectionType", "MediaItemId", "FirstRunPlaybackOrder", "RerunPlaybackOrder")
            VALUES (91, 'First source', 'Movie', 101, 'Chronological', 'Chronological'),
                   (92, 'Second source', 'Movie', 102, 'Chronological', 'Chronological');
        UPDATE "ProgramScheduleItem" SET "CollectionType"='RerunFirstRun', "PlaylistId"=NULL, "RerunCollectionId"=91 WHERE "Id"=401;
    "#).await.unwrap();
    let repositories = make_repositories(pool.clone());
    let mut second = repositories.program_schedule.get_item(401).await.unwrap();
    second.id = 403;
    second.sort_order += 1;
    second.rerun_collection_id = Some(92);
    repositories
        .program_schedule
        .upsert_item(&second)
        .await
        .unwrap();
    let start = common::fixed_time();
    run_classic_build(&pool, RELATED_PLAYOUT_ID, PlayoutBuildMode::Reset, start).await;
    assert_build_succeeded(&pool, RELATED_PLAYOUT_ID, "reset").await;
    let before = playlist_timeline(&pool).await;
    assert_eq!(
        before.iter().take(4).map(|row| row.0).collect::<Vec<_>>(),
        [101, 102, 101, 102]
    );
    pool.close().await;
    let pool = open_database(&path, false).await;
    run_classic_build(&pool, RELATED_PLAYOUT_ID, PlayoutBuildMode::Refresh, start).await;
    assert_build_succeeded(&pool, RELATED_PLAYOUT_ID, "refresh").await;
    assert_eq!(playlist_timeline(&pool).await, before);
    let ids: Vec<i64> = sqlx::query_scalar(r#"SELECT DISTINCT "RerunCollectionId" FROM "PlayoutProgramScheduleAnchor" WHERE "PlayoutId"=? ORDER BY "RerunCollectionId""#)
        .bind(RELATED_PLAYOUT_ID).fetch_all(&pool).await.unwrap();
    assert_eq!(ids, [91, 92]);

    // Older builds omitted this part of the key. Do not guess which source owned it.
    pool.execute(r#"UPDATE "PlayoutProgramScheduleAnchor" SET "RerunCollectionId"=NULL"#)
        .await
        .unwrap();
    let reference =
        ersatzrs_infra::repositories::SqliteSchedulingContentRepository::new(pool.clone())
            .classic_reference_data(RELATED_PLAYOUT_ID, start, Duration::zero())
            .await
            .unwrap();
    assert!(
        !ersatzrs_core::scheduling::audit_classic_refresh_checkpoint(
            &reference,
            RELATED_PLAYOUT_ID,
            start
        )
        .is_complete(),
        "the shared health/preflight audit must reject ambiguous source identities"
    );
    let mut old = persisted_classic_rows(&pool, RELATED_PLAYOUT_ID).await;
    let refused = run_requested_playout_build(
        &pool,
        &EntityLocker::new(),
        PlayoutBuildRequest {
            playout_id: RELATED_PLAYOUT_ID,
            mode: PlayoutBuildMode::Refresh,
        },
        start,
        None,
    )
    .await;
    assert!(
        refused.is_err(),
        "ambiguous legacy anchors must refuse Refresh"
    );
    let mut after = persisted_classic_rows(&pool, RELATED_PLAYOUT_ID).await;
    old.build_status.clear();
    after.build_status.clear();
    assert!(
        old == after,
        "failed preflight must leave the old timeline and state intact"
    );
    pool.close().await;
}
