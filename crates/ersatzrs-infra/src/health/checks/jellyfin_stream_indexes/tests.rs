use super::*;
use sqlx::Executor;

async fn test_db() -> Pool<Db> {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    pool.execute(r#"
        INSERT INTO "MediaSource" ("Id") VALUES (1);
        INSERT INTO "JellyfinMediaSource" ("Id") VALUES (1);
        INSERT INTO "Library" ("Id","MediaSourceId","Name","MediaKind") VALUES (1,1,'Synthetic',1);
        INSERT INTO "JellyfinLibrary" ("Id","ShouldSyncItems") VALUES (1,0);
        INSERT INTO "LibraryPath" ("Id","LibraryId","Path") VALUES (1,1,'synthetic');
        INSERT INTO "MediaItem" ("Id","LibraryPathId") VALUES (1,1),(2,1),(3,1),(4,1),(5,1);
        INSERT INTO "Movie" ("Id") VALUES (1),(5);
        INSERT INTO "JellyfinMovie" ("Id") VALUES (1);
        INSERT INTO "Show" ("Id") VALUES (3);
        INSERT INTO "Season" ("Id","ShowId") VALUES (4,3);
        INSERT INTO "Episode" ("Id","SeasonId") VALUES (2,4);
        INSERT INTO "JellyfinEpisode" ("Id") VALUES (2);
        INSERT INTO "MediaVersion" ("Id","MovieId","EpisodeId","DateAdded","DateUpdated","Duration","Height","Width","VideoScanKind") VALUES
            (10,1,NULL,'2026-11-16','2026-11-16','00:10:00',180,320,0),
            (20,NULL,2,'2026-11-16','2026-11-16','00:10:00',180,320,0),
            (50,5,NULL,'2026-11-16','2026-11-16','00:10:00',180,320,0);
        INSERT INTO "MediaStream" ("MediaVersionId","Index","MediaStreamKind","Channels","Default","Forced") VALUES
            (10,0,1,0,0,0),(10,1,2,2,0,0),(10,2,5,0,0,0),(10,3,3,0,0,0),
            (20,0,1,0,0,0),(20,1,2,2,0,0);
    "#).await.unwrap();
    pool
}

async fn repair(pool: &Pool<Db>) {
    pool.execute(r#"
        DELETE FROM "ProviderStreamIndex";
        INSERT INTO "ProviderStreamIndex" ("MediaVersionId","StreamIndex","SourceStreamIndex") VALUES
            (10,0,0),(10,1,1),(10,3,2),(10,4,3),(20,0,0),(20,1,1);
        UPDATE "MediaVersion" SET "ProviderStreamIndexVersion"=1 WHERE "Id" IN (10,20);
    "#).await.unwrap();
}

#[tokio::test]
async fn row295_health_counts_versions_recovers_and_preserves_disabled_choices() {
    let pool = test_db().await;
    let check = JellyfinStreamIndexesHealthCheck::new(pool.clone());
    let pending = check.check().await.unwrap();
    assert_eq!(pending.status, HealthCheckStatus::Warning);
    assert!(pending.message.starts_with("2 Jellyfin media versions"));
    repair(&pool).await;
    // External index2 has no physical mapping; attachment4 has no stored stream.
    // Neither is corrupt. Non-Jellyfin version50 remains revision0 and untouched.
    assert_eq!(check.check().await.unwrap().status, HealthCheckStatus::Pass);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            r#"SELECT "ShouldSyncItems" FROM "JellyfinLibrary" WHERE "Id"=1"#
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            r#"SELECT "ProviderStreamIndexVersion" FROM "MediaVersion" WHERE "Id"=50"#
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    let results = vec![pending];
    let store = crate::health::SqlHealthCheckStore::new(pool);
    store.save_results(&results).await.unwrap();
    assert_eq!(store.load_results().await.unwrap(), results);
}

#[tokio::test]
async fn row295_health_detects_missing_maps_even_with_current_revision() {
    let pool = test_db().await;
    let check = JellyfinStreamIndexesHealthCheck::new(pool.clone());
    for defect in [
        r#"DELETE FROM "ProviderStreamIndex" WHERE "MediaVersionId"=10 AND "StreamIndex"=1"#,
        r#"DELETE FROM "ProviderStreamIndex" WHERE "MediaVersionId"=10 AND "StreamIndex"=3"#,
        r#"DELETE FROM "ProviderStreamIndex" WHERE "MediaVersionId"=10"#,
        r#"UPDATE "MediaVersion" SET "ProviderStreamIndexVersion"=2 WHERE "Id"=10"#,
        r#"UPDATE "ProviderStreamIndex" SET "SourceStreamIndex"=9 WHERE "MediaVersionId"=10 AND "StreamIndex"=3"#,
    ] {
        repair(&pool).await;
        pool.execute(defect).await.unwrap();
        let result = check.check().await.unwrap();
        assert_eq!(result.status, HealthCheckStatus::Warning, "{defect}");
        assert!(
            result.message.starts_with("1 Jellyfin media versions"),
            "{defect}"
        );
    }
    repair(&pool).await;
    assert_eq!(check.check().await.unwrap().status, HealthCheckStatus::Pass);
}

#[tokio::test]
async fn row295_health_rejects_ambiguous_native_identities_but_allows_sidecar_reuse() {
    let pool = test_db().await;
    repair(&pool).await;
    let check = JellyfinStreamIndexesHealthCheck::new(pool.clone());
    pool.execute(r#"INSERT INTO "MediaStream" ("MediaVersionId","Index","MediaStreamKind","Channels","Default","Forced") VALUES (10,3,5,0,0,0)"#).await.unwrap();
    assert_eq!(check.check().await.unwrap().status, HealthCheckStatus::Pass);
    pool.execute(r#"UPDATE "MediaStream" SET "MediaStreamKind"=2 WHERE "MediaVersionId"=10 AND "Index"=3 AND "MediaStreamKind"=5"#).await.unwrap();
    assert_eq!(
        check.check().await.unwrap().status,
        HealthCheckStatus::Warning
    );
}
