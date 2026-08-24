mod common;

use ersatzrs_core::repositories::FillerRepository;
use ersatzrs_domain::{FillerKind, FillerMode, FillerPreset};
use ersatzrs_infra::repositories::make_repositories;

#[tokio::test]
async fn filler_repository_round_trips_lists_and_deletes() {
    let repos = make_repositories(common::test_db().await);

    repos
        .filler
        .upsert(&FillerPreset {
            id: 1,
            name: "ads".to_owned(),
            filler_kind: FillerKind::MidRoll,
            filler_mode: FillerMode::RandomCount,
            count: Some(3),
            allow_watermarks: true,
            expression: Some("num == 1".to_string()),
            use_chapters_as_media_items: true,
            ..FillerPreset::default()
        })
        .await
        .unwrap();

    let preset = repos.filler.get(1).await.unwrap();
    assert_eq!(preset.name, "ads");
    assert_eq!(preset.filler_kind, FillerKind::MidRoll);
    assert_eq!(preset.filler_mode, FillerMode::RandomCount);
    assert_eq!(preset.count, Some(3));
    assert!(preset.allow_watermarks);
    assert_eq!(preset.expression.as_deref(), Some("num == 1"));
    assert!(preset.use_chapters_as_media_items);
    assert_eq!(repos.filler.list().await.unwrap().len(), 1);

    repos.filler.delete(1).await.unwrap();
    assert!(repos.filler.list().await.unwrap().is_empty());
}

#[tokio::test]
async fn filler_admin_create_update_absence_and_checked_delete_are_typed() {
    let repos = make_repositories(common::test_db().await);
    let mut preset = FillerPreset {
        name: "bumper".to_owned(),
        filler_kind: FillerKind::PreRoll,
        filler_mode: FillerMode::Duration,
        duration: Some(chrono::Duration::seconds(15)),
        ..FillerPreset::default()
    };
    let id = repos.filler.save_admin(&preset).await.unwrap();
    assert!(id > 0);
    assert!(repos.filler.name_exists(0, "bumper").await.unwrap());
    assert_eq!(
        repos
            .filler
            .get_optional_admin(id)
            .await
            .unwrap()
            .unwrap()
            .duration,
        Some(chrono::Duration::seconds(15))
    );

    preset.id = id;
    preset.name = "bumper updated".to_owned();
    assert_eq!(repos.filler.save_admin(&preset).await.unwrap(), id);
    assert!(repos
        .filler
        .get_optional_admin(99_999)
        .await
        .unwrap()
        .is_none());
    repos.filler.delete_admin(id).await.unwrap();
    assert!(repos.filler.delete_admin(id).await.is_err());
}

#[tokio::test]
async fn filler_admin_reads_report_closed_pool_errors() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    pool.close().await;
    assert!(repos.filler.get_optional_admin(1).await.is_err());
    assert!(repos.filler.name_exists(0, "name").await.is_err());
}
