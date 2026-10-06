mod common;

use chrono::Duration;
use ersatzrs_core::repositories::{ChannelRepository, FfmpegProfileRepository, PlayoutRepository};
use ersatzrs_infra::{repositories::make_repositories, xmltv::load_xmltv_programmes};
use sqlx::Executor;

async fn fixture() -> ersatzrs_infra::db::DbPool {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    repos
        .ffmpeg_profile
        .upsert(&common::profile(1, "default"))
        .await
        .unwrap();
    let mut source = common::channel(10, "3", true);
    source.show_in_epg = true;
    let mut mirror = common::channel(11, "9", true);
    mirror.show_in_epg = true;
    mirror.playout_source = ersatzrs_domain::ChannelPlayoutSource::Mirror;
    mirror.mirror_source_channel_id = Some(source.id);
    mirror.playout_offset = Some(Duration::hours(1));
    repos.channel.upsert(&source).await.unwrap();
    repos.channel.upsert(&mirror).await.unwrap();
    common::seed_media_graph(&pool).await;
    pool.execute(r#"INSERT INTO "Movie" ("Id") VALUES (1)"#)
        .await
        .unwrap();
    pool.execute(
        r#"INSERT INTO "MovieMetadata"
           ("Id", "DateAdded", "DateUpdated", "MetadataKind", "MovieId", "Title")
           VALUES (1, '2026-04-15 00:00:00', '2026-04-15 00:00:00', 0, 1, 'Mirror Movie')"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "Playout" ("Id", "ChannelId", "ScheduleKind", "Seed")
           VALUES (20, 10, 'Classic', 0)"#,
    )
    .await
    .unwrap();
    pool
}

#[tokio::test]
async fn external_json_guide_reads_live_timeline_without_persisted_items() {
    use ersatzrs_core::xmltv::{IconSizeHint, XmltvItemKind};

    let pool = fixture().await;
    let now = common::fixed_time();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("external.json");
    let document = serde_json::json!({
        "startTime": now.to_rfc3339(),
        "guideMinimumDurationSeconds": 30,
        "programs": [
            {"type":"episode","title":"Episode title","showTitle":"Show title",
             "season":2,"episode":3,"duration":60_000,"icon":"https://example.invalid/show.png"},
            {"title":"Short filler","duration":5_000},
            {"type":"movie","title":"Movie title","year":2025,"duration":120_000}
        ]
    });
    std::fs::write(&path, serde_json::to_vec(&document).unwrap()).unwrap();
    sqlx::query(r#"UPDATE "Playout" SET "ScheduleKind"=20, "ScheduleFile"=? WHERE "Id"=20"#)
        .bind(path.to_str().unwrap())
        .execute(&pool)
        .await
        .unwrap();

    let programmes = load_xmltv_programmes(&pool, now, now + Duration::hours(2))
        .await
        .unwrap();
    assert_eq!(programmes.len(), 4);
    for (number, offset) in [("3", Duration::zero()), ("9", Duration::hours(1))] {
        let channel = programmes
            .iter()
            .filter(|item| item.channel_number == number)
            .collect::<Vec<_>>();
        assert_eq!(channel.len(), 2);
        let episode = channel[0];
        assert_eq!(episode.start, now + offset);
        assert_eq!(episode.stop, now + offset + Duration::seconds(60));
        assert_eq!(episode.title, "Show title");
        assert_eq!(episode.content.kind, XmltvItemKind::Episode);
        assert_eq!(episode.content.sub_title.as_deref(), Some("Episode title"));
        assert_eq!(episode.content.season_number, Some(2));
        assert_eq!(episode.content.episode_number, Some(3));
        assert_eq!(episode.content.icon_size_hint, IconSizeHint::Thumbnail);
        assert_eq!(
            episode.content.icon_artwork.as_ref().unwrap().path,
            "https://example.invalid/show.png"
        );
        let movie = channel[1];
        assert_eq!(movie.start, now + offset + Duration::seconds(65));
        assert_eq!(movie.stop, now + offset + Duration::seconds(185));
        assert_eq!(movie.title, "Movie title");
        assert_eq!(movie.content.year, Some(2025));
    }
    let count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "PlayoutItem""#)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);

    // A previous schedule's persisted rows must not leak into the external guide.
    let mut stale = common::playout_item(30, 20, 1);
    stale.start = now;
    stale.finish = now + Duration::minutes(10);
    make_repositories(pool.clone())
        .playout
        .upsert_item(&stale)
        .await
        .unwrap();
    assert_eq!(
        load_xmltv_programmes(&pool, now, now + Duration::hours(2))
            .await
            .unwrap(),
        programmes
    );

    std::fs::write(
        &path,
        br#"{"startTime":"private-invalid-input","programs":[]}"#,
    )
    .unwrap();
    let error = load_xmltv_programmes(&pool, now, now + Duration::hours(2))
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("invalid start time"));
    assert!(!error.contains("private-invalid-input"));
    assert!(!error.contains(path.to_str().unwrap()));
    std::fs::remove_file(&path).unwrap();
    assert!(load_xmltv_programmes(&pool, now, now + Duration::hours(2))
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn xmltv_projects_source_programming_onto_delayed_mirror_number() {
    let pool = fixture().await;
    let repos = make_repositories(pool.clone());
    let now = common::fixed_time();
    let mut item = common::playout_item(30, 20, 1);
    item.start = now - Duration::hours(1) - Duration::minutes(10);
    item.finish = now - Duration::hours(1) + Duration::minutes(20);
    repos.playout.upsert_item(&item).await.unwrap();
    sqlx::query(
        r#"REPLACE INTO "ConfigElement" ("Key", "Value")
           VALUES (?, 'UseActualTimes')"#,
    )
    .bind(ersatzrs_domain::CONFIG_KEY_XMLTV_BLOCK_BEHAVIOR)
    .execute(&pool)
    .await
    .unwrap();

    let programmes =
        load_xmltv_programmes(&pool, now - Duration::hours(2), now + Duration::hours(1))
            .await
            .unwrap();
    let source_programme = programmes
        .iter()
        .find(|programme| programme.channel_number == "3")
        .unwrap();
    let mirror_programme = programmes
        .iter()
        .find(|programme| programme.channel_number == "9")
        .unwrap();
    assert_eq!(source_programme.title, mirror_programme.title);
    assert_eq!(
        mirror_programme.start - source_programme.start,
        Duration::hours(1)
    );
    assert_eq!(
        mirror_programme.stop - source_programme.stop,
        Duration::hours(1)
    );
}

#[tokio::test]
async fn xmltv_mirror_offsets_actual_and_guide_windows_once_without_mutating_source() {
    let pool = fixture().await;
    // This test exercises the persisted Block behavior. Classic has its own
    // programme grouping and must not inherit the Block guide-window setting.
    sqlx::query(r#"UPDATE "Playout" SET "ScheduleKind" = 'Block' WHERE "Id" = 20"#)
        .execute(&pool)
        .await
        .unwrap();
    let repos = make_repositories(pool.clone());
    let now = common::fixed_time();
    let mut item = common::playout_item(30, 20, 1);
    item.start = now;
    item.finish = now + Duration::minutes(20);
    item.guide_start = Some(now - Duration::minutes(5));
    item.guide_finish = Some(now + Duration::minutes(30));
    repos.playout.upsert_item(&item).await.unwrap();
    let original = repos
        .playout
        .items_in_range(20, now, item.finish)
        .await
        .unwrap();
    for hours in [-2, 1] {
        let mut mirror = repos.channel.get_with_profile(11).await.unwrap();
        mirror.playout_offset = Some(Duration::hours(hours));
        repos.channel.upsert(&mirror).await.unwrap();
        for (behavior, expected_start, expected_stop) in [
            ("UseActualTimes", item.start, item.finish),
            (
                "SplitTimeEvenly",
                item.guide_start.unwrap(),
                item.guide_finish.unwrap(),
            ),
        ] {
            sqlx::query(r#"REPLACE INTO "ConfigElement" ("Key", "Value") VALUES (?, ?)"#)
                .bind(ersatzrs_domain::CONFIG_KEY_XMLTV_BLOCK_BEHAVIOR)
                .bind(behavior)
                .execute(&pool)
                .await
                .unwrap();
            // Repeated reads must not apply the mirror offset cumulatively.
            for _ in 0..2 {
                let programmes = load_xmltv_programmes(
                    &pool,
                    now - Duration::hours(3),
                    now + Duration::hours(3),
                )
                .await
                .unwrap();
                assert_eq!(programmes.len(), 2);
                for (number, offset) in [("3", Duration::zero()), ("9", Duration::hours(hours))] {
                    let programme = programmes
                        .iter()
                        .find(|p| p.channel_number == number)
                        .unwrap();
                    assert_eq!(programme.start, expected_start + offset);
                    assert_eq!(programme.stop, expected_stop + offset);
                }
            }
        }
    }
    let persisted = repos
        .playout
        .items_in_range(20, now, item.finish)
        .await
        .unwrap();
    assert_eq!(persisted.len(), original.len());
    assert_eq!(persisted[0].start, original[0].start);
    assert_eq!(persisted[0].finish, original[0].finish);
    assert_eq!(persisted[0].guide_start, original[0].guide_start);
    assert_eq!(persisted[0].guide_finish, original[0].guide_finish);
}

#[tokio::test]
async fn classic_guide_groups_film_segments_and_rolls_without_changing_rows() {
    let pool = fixture().await;
    let repos = make_repositories(pool.clone());
    let now = common::fixed_time();
    for (id, group, start, finish, filler, in_point, out_point) in [
        (30, 1, 0, 10, ersatzrs_domain::FillerKind::PreRoll, 0, 10),
        (31, 1, 10, 60, ersatzrs_domain::FillerKind::None, 0, 50),
        (32, 1, 60, 65, ersatzrs_domain::FillerKind::MidRoll, 0, 5),
        (33, 1, 65, 115, ersatzrs_domain::FillerKind::None, 50, 100),
        (34, 2, 115, 120, ersatzrs_domain::FillerKind::PostRoll, 0, 5),
        (35, 2, 120, 170, ersatzrs_domain::FillerKind::None, 0, 50),
    ] {
        let mut item = common::playout_item(id, 20, 1);
        item.guide_group = group;
        item.guide_start = None;
        item.guide_finish = None;
        item.start = now + Duration::seconds(start);
        item.finish = now + Duration::seconds(finish);
        item.filler_kind = filler;
        item.in_point = Duration::seconds(in_point);
        item.out_point = Duration::seconds(out_point);
        repos.playout.upsert_item(&item).await.unwrap();
    }
    let before = repos
        .playout
        .items_in_range(20, now, now + Duration::seconds(180))
        .await
        .unwrap();
    for _ in 0..2 {
        let programmes =
            load_xmltv_programmes(&pool, now + Duration::seconds(62), now + Duration::hours(2))
                .await
                .unwrap();
        assert_eq!(programmes.len(), 4);
        for (number, offset) in [("3", Duration::zero()), ("9", Duration::hours(1))] {
            let rows = programmes
                .iter()
                .filter(|programme| programme.channel_number == number)
                .collect::<Vec<_>>();
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[0].start, now + offset);
            assert_eq!(rows[0].stop, now + offset + Duration::seconds(120));
            assert_eq!(rows[1].start, now + offset + Duration::seconds(120));
            assert_eq!(rows[0].title, "Mirror Movie");
        }
    }
    let after = repos
        .playout
        .items_in_range(20, now, now + Duration::seconds(180))
        .await
        .unwrap();
    assert_eq!(before, after);
}
