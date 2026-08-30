mod common;

use ersatzrs_core::repositories::SavedSearchRepository;
use ersatzrs_infra::repositories::SqliteSavedSearchRepository;

#[tokio::test]
async fn saved_searches_can_be_created_recalled_updated_and_deleted() {
    let repository = SqliteSavedSearchRepository(common::test_db().await);

    let id = repository
        .save_admin(0, "Weekend movies", "type:movie AND genre:comedy")
        .await
        .expect("create saved search");
    assert!(id > 0);
    assert!(repository.name_exists(0, "weekend movies").await.unwrap());

    let recalled = repository
        .get_admin(id)
        .await
        .expect("recall saved search")
        .expect("saved search exists");
    assert_eq!(recalled.name, "Weekend movies");
    assert_eq!(recalled.query, "type:movie AND genre:comedy");

    repository
        .save_admin(id, "Weekend movies", "type:movie AND genre:drama")
        .await
        .expect("update saved search");
    let rows = repository.list_admin().await.expect("list saved searches");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].query, "type:movie AND genre:drama");

    repository
        .delete_admin(id)
        .await
        .expect("delete saved search");
    assert!(repository.list_admin().await.unwrap().is_empty());
}

#[tokio::test]
async fn saved_search_names_are_case_insensitively_unique() {
    let repository = SqliteSavedSearchRepository(common::test_db().await);
    let first = repository
        .save_admin(0, "Curated sci-fi", "genre:sci-fi")
        .await
        .expect("create first saved search");

    assert!(repository.name_exists(0, "CURATED SCI-FI").await.unwrap());
    assert!(!repository
        .name_exists(first, "CURATED SCI-FI")
        .await
        .unwrap());
}
