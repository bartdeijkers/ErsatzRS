use super::*;

#[tokio::test]
async fn build_gaps_follow_final_items_and_roll_back_with_them() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repos, 1).await;
    common::seed_media_graph(&pool).await;
    pool.execute(r#"INSERT INTO "Playout" ("Id", "ChannelId", "ScheduleKind", "Seed") VALUES (1, 1, 'Classic', 42), (2, 1, 'Classic', 42);
        INSERT INTO "PlayoutGap" ("PlayoutId", "Start", "Finish") VALUES (1, '2026-01-01 00:00:00', '2026-01-02 00:00:00'), (1, '2026-01-01 00:00:00', '2026-01-02 00:00:00'), (2, '2026-01-01 00:00:00', '2026-01-02 00:00:00');"#).await.unwrap();
    let start = common::fixed_time();
    let item = |begin, end| ersatzrs_domain::PlayoutItem {
        start: start + Duration::minutes(begin),
        finish: start + Duration::minutes(end),
        ..common::playout_item(0, 1, 1)
    };
    let playout = repos.playout.get_with_anchor(1).await.unwrap();
    let result = SchedulingResult {
        clear_items: true,
        added_items: vec![item(0, 20), item(10, 30), item(30, 40), item(50, 60)],
        ..SchedulingResult::default()
    };
    for _ in 0..2 {
        repos
            .playout
            .apply_build_result(&result, &playout, Duration::zero())
            .await
            .unwrap();
        let detail = repos
            .playout
            .list_detail_admin(1, true, start, 0, 100)
            .await
            .unwrap();
        let gaps = detail
            .iter()
            .filter(|row| row.filler_kind == -1)
            .collect::<Vec<_>>();
        assert_eq!(
            repos
                .playout
                .count_detail_admin(1, true, start)
                .await
                .unwrap(),
            5
        );
        assert_eq!(
            gaps.len(),
            1,
            "overlap and touching items must not create gaps"
        );
        assert_eq!(
            gaps[0].start,
            (start + Duration::minutes(40))
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
        );
        assert_eq!(
            gaps[0].finish,
            (start + Duration::minutes(50))
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
        );
    }
    // Continue can fill an existing gap; no old projection may survive it.
    let continuation = SchedulingResult {
        added_items: vec![item(40, 50)],
        ..SchedulingResult::default()
    };
    repos
        .playout
        .apply_build_result(&continuation, &playout, Duration::zero())
        .await
        .unwrap();
    let count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "PlayoutGap" WHERE "PlayoutId"=1"#)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    let other: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "PlayoutGap" WHERE "PlayoutId"=2"#)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(other, 1);
    pool.execute(r#"CREATE TRIGGER reject_gap BEFORE INSERT ON "PlayoutGap" BEGIN SELECT RAISE(ABORT, 'synthetic gap failure'); END"#).await.unwrap();
    assert!(repos
        .playout
        .apply_build_result(&result, &playout, Duration::zero())
        .await
        .is_err());
    let item_count: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "PlayoutItem" WHERE "PlayoutId"=1"#)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        item_count, 5,
        "failed gap projection must roll back Reset's item replacement"
    );
}
