#![cfg(not(feature = "mysql"))]
mod common;

use ersatzrs_core::repositories::FfmpegProfileRepository;
use ersatzrs_infra::repositories::make_repositories;

#[tokio::test]
async fn ffmpeg_profile_repository_round_trips_lists_and_deletes() {
    let repos = make_repositories(common::test_db().await);

    repos
        .ffmpeg_profile
        .upsert(&common::profile(1, "default"))
        .await
        .unwrap();

    assert_eq!(repos.ffmpeg_profile.get(1).await.unwrap().name, "default");
    assert_eq!(repos.ffmpeg_profile.list().await.unwrap().len(), 1);

    repos.ffmpeg_profile.delete(1).await.unwrap();
    assert!(repos.ffmpeg_profile.list().await.unwrap().is_empty());
}
