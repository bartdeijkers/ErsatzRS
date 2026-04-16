mod common;

use ersatzrs_core::repositories::{ChannelRepository, FfmpegProfileRepository};
use ersatzrs_infra::repositories::make_repositories;
use sqlx::Executor;

#[tokio::test]
async fn channel_repository_round_trips_and_filters_enabled_channels() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    repos
        .ffmpeg_profile
        .upsert(&common::profile(1, "default"))
        .await
        .unwrap();

    let mut enabled = common::channel(10, "10.1", true);
    enabled.artwork_ids = vec![100, 101];
    let disabled = common::channel(11, "11", false);

    pool.execute(
        r#"INSERT INTO "Artwork" ("Id", "Path", "ArtworkKind", "DateAdded", "DateUpdated") VALUES
         (100, '/art/100.png', 'Poster', '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z'),
         (101, '/art/101.png', 'Poster', '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z')"#,
    )
    .await
    .unwrap();

    repos.channel.upsert(&enabled).await.unwrap();
    repos.channel.upsert(&disabled).await.unwrap();

    let loaded = repos.channel.get_with_profile(10).await.unwrap();
    assert_eq!(loaded.number, "10.1");
    assert_eq!(loaded.artwork_ids, vec![100, 101]);

    let by_number = repos.channel.get_by_number("10.1").await.unwrap();
    assert_eq!(by_number.id, 10);

    let enabled_channels = repos.channel.list_enabled().await.unwrap();
    assert_eq!(enabled_channels.len(), 1);
    assert_eq!(enabled_channels[0].id, 10);
    assert!(enabled_channels[0].artwork_ids.is_empty());
}
