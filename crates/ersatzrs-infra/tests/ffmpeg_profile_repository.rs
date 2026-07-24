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

#[tokio::test]
async fn ffmpeg_profile_admin_create_copy_and_absence_are_typed() {
    let repos = make_repositories(common::test_db().await);
    let mut profile = common::profile(0, "admin");
    let id = repos.ffmpeg_profile.save_admin(&profile).await.unwrap();
    assert!(id > 0);
    assert!(repos.ffmpeg_profile.name_exists(0, "admin").await.unwrap());
    assert_eq!(
        repos
            .ffmpeg_profile
            .get_optional_admin(id)
            .await
            .unwrap()
            .unwrap()
            .name,
        "admin"
    );

    let copy_id = repos
        .ffmpeg_profile
        .copy_admin(id, "admin copy")
        .await
        .unwrap();
    assert_ne!(copy_id, id);
    assert_eq!(
        repos.ffmpeg_profile.get(copy_id).await.unwrap().name,
        "admin copy"
    );
    assert!(repos
        .ffmpeg_profile
        .get_optional_admin(99_999)
        .await
        .unwrap()
        .is_none());

    profile.id = id;
    profile.name = "updated".to_owned();
    assert_eq!(repos.ffmpeg_profile.save_admin(&profile).await.unwrap(), id);
}

#[tokio::test]
async fn ffmpeg_profile_admin_reads_report_closed_pool_errors() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    pool.close().await;
    assert!(repos.ffmpeg_profile.list_resolutions().await.is_err());
    assert!(repos.ffmpeg_profile.name_exists(0, "name").await.is_err());
    assert!(repos.ffmpeg_profile.get_optional_admin(1).await.is_err());
}
