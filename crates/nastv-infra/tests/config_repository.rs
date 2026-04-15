mod common;

use nastv_core::repositories::ConfigRepository;
use nastv_infra::repositories::make_repositories;

#[tokio::test]
async fn config_repository_sets_and_gets_values() {
    let repos = make_repositories(common::test_db().await);

    repos.config.set("database.path", "nastv.db").await.unwrap();

    assert_eq!(
        repos.config.get("database.path").await.unwrap(),
        Some("nastv.db".to_owned())
    );
    assert_eq!(repos.config.get("missing").await.unwrap(), None);
}
