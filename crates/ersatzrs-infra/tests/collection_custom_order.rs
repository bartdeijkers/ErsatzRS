mod common;

use ersatzrs_core::repositories::{CollectionRepository, RepositoryError};
use ersatzrs_infra::repositories::make_repositories;
use sqlx::Executor;

#[tokio::test]
async fn collection_custom_order_rejects_foreign_rows_without_partial_writes() {
    let pool = common::test_db().await;
    common::seed_media_graph(&pool).await;
    pool.execute(
        r#"INSERT INTO "Collection" ("Id", "Name", "UseCustomPlaybackOrder")
        VALUES (3160, 'Synthetic movies', 1), (3161, 'Synthetic other collection', 1);
        INSERT INTO "CollectionItem" ("Id", "CollectionId", "MediaItemId", "CustomIndex", "PreferredAudioLanguageCode")
        VALUES (3160, 3160, 1, 0, 'eng'), (3161, 3160, 1, 1, 'nld'), (3162, 3161, 1, 7, 'fra');"#,
    )
    .await
    .unwrap();
    let repos = make_repositories(pool.clone());
    let error = repos
        .collection
        .update_collection_custom_order(3160, &[3161, 3162])
        .await
        .expect_err("foreign row must reject the entire order");
    assert!(matches!(error, RepositoryError::Validation { .. }));
    let rows: Vec<(i64, i64, String)> = sqlx::query_as(
        r#"SELECT "Id", "CustomIndex", "PreferredAudioLanguageCode" FROM "CollectionItem" ORDER BY "Id""#,
    ).fetch_all(&pool).await.unwrap();
    assert_eq!(
        rows,
        [
            (3160, 0, "eng".to_owned()),
            (3161, 1, "nld".to_owned()),
            (3162, 7, "fra".to_owned())
        ]
    );
    assert_eq!(
        repos
            .collection
            .update_collection_custom_order(3160, &[3161, 3160])
            .await
            .unwrap(),
        2
    );
    let rows: Vec<(i64, i64, String)> = sqlx::query_as(
        r#"SELECT "Id", "CustomIndex", "PreferredAudioLanguageCode" FROM "CollectionItem" ORDER BY "Id""#,
    ).fetch_all(&pool).await.unwrap();
    assert_eq!(
        rows,
        [
            (3160, 1, "eng".to_owned()),
            (3161, 0, "nld".to_owned()),
            (3162, 7, "fra".to_owned())
        ]
    );
}

#[tokio::test]
async fn collection_custom_order_missing_collection_is_not_found_even_with_empty_order() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool);
    for order in [vec![], vec![3160]] {
        assert!(matches!(
            repos
                .collection
                .update_collection_custom_order(3160, &order)
                .await,
            Err(RepositoryError::NotFound {
                kind: "collection",
                id: 3160
            })
        ));
    }
}
