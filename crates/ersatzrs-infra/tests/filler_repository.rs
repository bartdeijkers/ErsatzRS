#![cfg(not(feature = "mysql"))]
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
