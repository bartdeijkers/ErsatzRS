use super::*;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::Executor as _;

async fn fixture() -> SqliteAddonMediaListRepository {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("connect fixture");
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("migrate fixture");
    pool.execute("PRAGMA foreign_keys = OFF")
        .await
        .expect("allow synthetic provider references");
    for id in 1..=5 {
        sqlx::query(
            r#"INSERT INTO "AddonMediaList"
               ("Id", "AddonId", "ProviderId", "SourceUrl", "Name")
               VALUES (?, 'fixture.addon', ?, 'https://media.example.test/list', ?)"#,
        )
        .bind(id)
        .bind(format!("list-{id}"))
        .bind(format!("List {id}"))
        .execute(&pool)
        .await
        .expect("seed list");
    }
    SqliteAddonMediaListRepository(pool)
}

#[tokio::test]
async fn visibility_filters_counts_pages_and_stable_order() {
    let repo = fixture().await;
    let (rows, count) = repo
        .list_admin(
            0,
            0,
            AddonMediaListAdminSortField::default(),
            AddonMediaListAdminSortDirection::default(),
        )
        .await
        .expect("default visible");
    assert_eq!(count, 5);
    assert_eq!(rows.len(), 5);
    for id in [2, 4] {
        repo.set_admin_hidden(id, true).await.expect("hide");
    }
    for (hidden, expected) in [(false, vec![1, 3, 5]), (true, vec![2, 4])] {
        let (rows, count) = repo
            .list_admin_visibility(
                0,
                0,
                AddonMediaListAdminSortField::default(),
                AddonMediaListAdminSortDirection::default(),
                hidden,
            )
            .await
            .expect("filtered rows");
        assert_eq!(count, expected.len() as u64);
        assert_eq!(rows.iter().map(|row| row.id).collect::<Vec<_>>(), expected);
        for (page, id) in expected.iter().enumerate() {
            let (rows, total) = repo
                .list_admin_visibility(
                    u32::try_from(page).expect("page"),
                    1,
                    AddonMediaListAdminSortField::default(),
                    AddonMediaListAdminSortDirection::default(),
                    hidden,
                )
                .await
                .expect("page");
            assert_eq!(total, count);
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].id, *id);
        }
        let (rows, _) = repo
            .list_admin_visibility(
                u32::MAX,
                1,
                AddonMediaListAdminSortField::Id,
                AddonMediaListAdminSortDirection::Descending,
                hidden,
            )
            .await
            .expect("clamped descending page");
        assert_eq!(rows[0].id, expected[0]);
    }
    let (visible, count) = repo
        .list_admin(
            0,
            0,
            AddonMediaListAdminSortField::default(),
            AddonMediaListAdminSortDirection::default(),
        )
        .await
        .expect("legacy query excludes hidden");
    assert_eq!(count, 3);
    assert_eq!(
        visible.iter().map(|row| row.id).collect::<Vec<_>>(),
        [1, 3, 5]
    );
    for id in [2, 4] {
        repo.set_admin_hidden(id, false).await.expect("show");
    }
    let (rows, count) = repo
        .list_admin_visibility(
            0,
            10,
            AddonMediaListAdminSortField::default(),
            AddonMediaListAdminSortDirection::default(),
            true,
        )
        .await
        .expect("empty hidden filter");
    assert!(rows.is_empty());
    assert_eq!(count, 0);
}

/// Snapshot every stored value, excluding only the visibility bit. This also
/// protects future publication/refresh columns without enumerating them here.
async fn retained_state(pool: &Pool<crate::db::Db>) -> Vec<Vec<String>> {
    let mut result = Vec::new();
    for table in [
        "AddonMediaList",
        "AddonMediaListItem",
        "Library",
        "AddonMediaListLibrary",
        "Playlist",
        "PlaylistItem",
    ] {
        // Table names come only from the fixed fixture whitelist above.
        let columns = sqlx::query(sqlx::AssertSqlSafe(format!(
            "PRAGMA table_info(\"{table}\")"
        )))
        .fetch_all(pool)
        .await
        .expect("columns");
        let projection = columns
            .iter()
            .map(|row| row.get::<String, _>("name"))
            .filter(|name| name != "AdminHidden")
            .map(|name| {
                // Schema-owned names are still checked before interpolation.
                assert!(name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_'));
                format!("quote(\"{name}\")")
            })
            .collect::<Vec<_>>()
            .join(", ");
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {projection} FROM \"{table}\" ORDER BY rowid"
        )))
        .fetch_all(pool)
        .await
        .expect("retained data");
        result.extend(rows.iter().map(|row| {
            (0..row.len())
                .map(|index| row.get::<String, _>(index))
                .collect()
        }));
    }
    result
}

#[tokio::test]
async fn visibility_preserves_published_content_refresh_and_library_identity() {
    let repo = fixture().await;
    repo.0
        .execute(
            r#"
        INSERT INTO "Library" ("Id", "MediaKind", "MediaSourceId", "Name")
        VALUES (1, 4, 1, 'Fixture library');
        INSERT INTO "AddonMediaListLibrary"
            ("AddonMediaListId", "LibraryId", "FolderName", "ManifestPath")
        VALUES (1, 1, 'fixture-list', 'fixture-list/manifest.json');
        INSERT INTO "Playlist" ("Id", "IsSystem", "Name", "PlaylistGroupId")
        VALUES (1, 1, 'Fixture playlist', 1);
        INSERT INTO "PlaylistItem" ("PlaylistId", "Index", "CollectionType")
        VALUES (1, 0, 0);
        INSERT INTO "AddonMediaListItem"
            ("AddonMediaListId", "ProviderId", "DisplayTitle", "Kind", "Rank", "Title", "Published")
        VALUES (1, 'item', 'Published item', 0, 0, 'Published item', 1);
        UPDATE "AddonMediaList" SET "PlaylistId" = 1, "GeneratePlaylist" = 1,
            "ReviewState" = 2, "PublishedRevision" = 7, "DraftRevision" = 8,
            "RefreshCadence" = 0, "NextRefreshAt" = '2026-09-13T00:00:00Z',
            "LastRefreshSucceededAt" = '2026-09-12T00:00:00Z',
            "RefreshRetryAt" = '2026-09-13T00:05:00Z', "ItemCount" = 1
        WHERE "Id" = 1;
    "#,
        )
        .await
        .expect("seed runtime relationships");
    let before = retained_state(&repo.0).await;
    for hidden in [true, true, false, false] {
        repo.set_admin_hidden(1, hidden)
            .await
            .expect("idempotent visibility");
        assert_eq!(retained_state(&repo.0).await, before);
        let restarted = SqliteAddonMediaListRepository(repo.0.clone());
        let (rows, _) = restarted
            .list_admin_visibility(
                0,
                0,
                AddonMediaListAdminSortField::default(),
                AddonMediaListAdminSortDirection::default(),
                hidden,
            )
            .await
            .expect("reload visibility");
        assert!(rows.iter().any(|row| row.id == 1));
        assert!(restarted
            .get_admin_form(1)
            .await
            .expect("retained editor")
            .is_some());
    }
    for hidden in [true, false] {
        assert!(matches!(
            repo.set_admin_hidden(999, hidden).await,
            Err(RepositoryError::NotFound { id: 999, .. })
        ));
        assert_eq!(retained_state(&repo.0).await, before);
    }
}
