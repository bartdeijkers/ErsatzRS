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
        "INSERT INTO media_versions (
            id, media_item_id, name, duration_ms, video_scan_kind, date_added, date_updated, width, height
         ) VALUES (5, 4, 'default', 0, 'Unknown', '2026-04-15T00:00:00+00:00', '2026-04-15T00:00:00+00:00', 0, 0)",
    )
    .await
    .unwrap();
    pool.execute(
        "INSERT INTO media_files (id, media_version_id, path, part) VALUES (6, 5, '/media/movie.mkv', 0)",
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
