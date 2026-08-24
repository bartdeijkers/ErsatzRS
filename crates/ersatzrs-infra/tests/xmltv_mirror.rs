mod common;

use chrono::Duration;
use ersatzrs_core::repositories::{ChannelRepository, FfmpegProfileRepository, PlayoutRepository};
use ersatzrs_infra::{repositories::make_repositories, xmltv::load_xmltv_programmes};
use sqlx::Executor;

#[tokio::test]
async fn xmltv_projects_source_programming_onto_delayed_mirror_number() {
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
