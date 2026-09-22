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
