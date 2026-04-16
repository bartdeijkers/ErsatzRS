mod common;

use ersatzrs_core::repositories::MediaItemRepository;
use ersatzrs_infra::repositories::make_repositories;
use sqlx::Executor;

#[tokio::test]
async fn media_item_repository_lists_by_library_path_and_finds_file_path() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    common::seed_media_path(&pool, 1, 1, 3).await;

    repos
        .media_item
        .upsert(&common::media_item(4, 3))
        .await
        .unwrap();
    pool.execute(
        r#"INSERT INTO "Movie" ("Id") VALUES (4)"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "MediaVersion" (
            "Id", "MovieId", "DateAdded", "DateUpdated", "Duration",
            "Height", "VideoScanKind", "Width"
         ) VALUES (
            5, 4, '2026-04-15T00:00:00+00:00', '2026-04-15T00:00:00+00:00',
            '00:00:00', 0, 0, 0
         )"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "MediaFile" ("Id", "MediaVersionId", "Path", "PathHash")
         VALUES (6, 5, '/media/movie.mkv', 'movie-hash')"#,
    )
    .await
    .unwrap();

    assert_eq!(
        repos.media_item.list_for_library_path(3).await.unwrap()[0].id,
        4
    );
    assert_eq!(
        repos
            .media_item
            .find_by_path("/media/movie.mkv")
            .await
            .unwrap()
            .id,
        4
    );
}
