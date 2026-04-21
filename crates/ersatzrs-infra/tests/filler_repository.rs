#![cfg(not(feature = "mysql"))]
mod common;

use ersatzrs_core::repositories::FillerRepository;
use ersatzrs_domain::FillerPreset;
use ersatzrs_infra::repositories::make_repositories;

#[tokio::test]
async fn filler_repository_round_trips_lists_and_deletes() {
    let repos = make_repositories(common::test_db().await);

    repos
        .filler
        .upsert(&FillerPreset {
            id: 1,
            name: "ads".to_owned(),
            ..FillerPreset::default()
        })
        .await
        .unwrap();

    assert_eq!(repos.filler.get(1).await.unwrap().name, "ads");
    assert_eq!(repos.filler.list().await.unwrap().len(), 1);

    repos.filler.delete(1).await.unwrap();
    assert!(repos.filler.list().await.unwrap().is_empty());
}
