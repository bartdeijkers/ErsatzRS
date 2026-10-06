mod common;
#[path = "external_json_snapshot/health.rs"]
mod health;
#[path = "external_json_snapshot/worker.rs"]
mod worker;

use chrono::Duration;
use ersatzrs_core::{
    repositories::ChannelRepository,
    scheduling::{PlayoutBuildMode, PlayoutPreservation},
};
use ersatzrs_infra::{
    db::DbPool,
    external_json::{
        self, adaptation,
        playback::{self, ExternalPlayoutSelection},
        snapshot::{self, Publication, SnapshotError},
    },
    repositories::make_repositories,
};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    Executor,
};
use std::{
    path::Path,
    time::{Duration as StdDuration, Instant},
};

async fn open(path: &Path) -> DbPool {
    SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(true)
                .foreign_keys(true)
                .busy_timeout(StdDuration::from_secs(5)),
        )
        .await
        .unwrap()
}

async fn seed(pool: &DbPool, path: &Path) {
    pool.execute(r#"INSERT OR IGNORE INTO "Resolution" ("Id","Name","Width","Height") VALUES (1,'fixture',320,180)"#).await.unwrap();
    let repos = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repos, 1).await;
    let mut mirror = common::channel(2, "2", true);
    mirror.playout_source = ersatzrs_domain::ChannelPlayoutSource::Mirror;
    mirror.mirror_source_channel_id = Some(1);
    mirror.playout_offset = Some(Duration::hours(1));
    repos.channel.upsert(&mirror).await.unwrap();
    pool.execute(r#"UPDATE "Channel" SET "ShowInEpg"=1"#)
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "Playout" ("Id","ChannelId","ScheduleKind","ScheduleFile","Seed") VALUES (1,1,20,?,1)"#)
        .bind(path.to_str().unwrap()).execute(pool).await.unwrap();
}

fn document(titles: &[&str]) -> Vec<u8> {
    serde_json::to_vec(
        &serde_json::json!({"startTime":common::fixed_time().to_rfc3339(),
            "guideMinimumDurationSeconds": 5,
            "programs": titles.iter().map(|title| serde_json::json!({
                "title":title,"duration":60_000,"file":format!("media/{title}.mkv"),"type":"movie"
            })).collect::<Vec<_>>()
        }),
    )
    .unwrap()
}

fn request(path: &Path, revision: Option<i64>) -> Publication<'_> {
    Publication {
        playout_id: 1,
        expected_revision: revision,
        schedule_file: path.to_str().unwrap(),
        published_at: common::fixed_time(),
        effective_from: None,
        deadline: None,
    }
}

async fn assert_consumers(pool: &DbPool, titles: &[&str]) {
    let start = common::fixed_time();
    let guide =
        ersatzrs_infra::xmltv::load_xmltv_programmes(pool, start, start + Duration::hours(2))
            .await
            .unwrap();
    for (channel, offset) in [(1, Duration::zero()), (2, Duration::hours(1))] {
        let selected: Vec<_> = guide
            .iter()
            .filter(|item| item.channel_number == channel.to_string())
            .collect();
        assert_eq!(selected.len(), titles.len());
        for (index, title) in titles.iter().enumerate() {
            let time = start + offset + Duration::minutes(i64::try_from(index).unwrap());
            let ExternalPlayoutSelection::Item(item) =
                playback::select(pool, channel, time).await.unwrap()
            else {
                panic!("expected an external programme")
            };
            assert_eq!(item.program.title.as_deref(), Some(*title));
            assert_eq!(item.start, time);
            assert_eq!(selected[index].title, *title);
            assert_eq!(selected[index].start, time);
        }
    }
    let counts: (i64, i64, i64) = sqlx::query_as(
        r#"SELECT (SELECT COUNT(*) FROM "PlayoutItem"),
                  (SELECT COUNT(*) FROM "MediaItem"),
                  (SELECT COUNT(*) FROM "PlayoutHistory")"#,
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(counts, (0, 0, 0));
}

#[tokio::test]
async fn accepted_snapshot_survives_reopen_and_source_loss_in_both_consumers() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("schedule.json");
    let db = root.path().join("state.db");
    let pool = open(&db).await;
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    seed(&pool, &path).await;
    std::fs::write(&path, document(&["A", "B", "C"])).unwrap();
    assert!(snapshot::accepted(&pool, 1).await.unwrap().is_none());
    assert_consumers(&pool, &["A", "B", "C"]).await;
    // Direct legacy readers still observe file replacement before opt-in.
    std::fs::write(&path, document(&["D", "E", "F"])).unwrap();
    assert_consumers(&pool, &["D", "E", "F"]).await;
    let initial = external_json::load(&path).await.unwrap().unwrap();
    assert_eq!(
        snapshot::publish(&pool, request(&path, None), &initial)
            .await
            .unwrap(),
        1
    );
    std::fs::write(&path, document(&["NewPast", "NewFuture", "Next"])).unwrap();
    assert_consumers(&pool, &["D", "E", "F"]).await;
    let candidate = external_json::load(&path).await.unwrap().unwrap();
    let adapted = adaptation::reconcile(
        Some(&initial),
        candidate,
        PlayoutBuildMode::Refresh,
        PlayoutPreservation::KeepCurrent,
        common::fixed_time() + Duration::seconds(1),
    )
    .unwrap();
    let mut publication = request(&path, Some(1));
    publication.effective_from = adapted.effective_from;
    assert_eq!(
        snapshot::publish(&pool, publication, &adapted.schedule)
            .await
            .unwrap(),
        2
    );
    std::fs::remove_file(&path).unwrap();
    pool.close().await;
    let reopened = open(&db).await;
    assert_consumers(&reopened, &["D", "NewFuture", "Next"]).await;
    let accepted = snapshot::accepted(&reopened, 1).await.unwrap().unwrap();
    assert_eq!(accepted.revision, 2);
    assert_eq!(accepted.schedule_file, path.to_str().unwrap());
    assert!(accepted.schedule == adapted.schedule);
}

#[tokio::test]
async fn concurrent_revision_source_and_deadline_fences_preserve_the_accepted_snapshot() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("schedule.json");
    let pool = open(&root.path().join("state.db")).await;
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    seed(&pool, &path).await;
    std::fs::write(&path, document(&["Accepted"])).unwrap();
    let accepted = external_json::load(&path).await.unwrap().unwrap();
    snapshot::publish(&pool, request(&path, None), &accepted)
        .await
        .unwrap();
    let (a, b) = tokio::join!(
        snapshot::publish(&pool, request(&path, Some(1)), &accepted),
        snapshot::publish(&pool, request(&path, Some(1)), &accepted)
    );
    assert!(matches!(
        (a, b),
        (Ok(2), Err(SnapshotError::Superseded)) | (Err(SnapshotError::Superseded), Ok(2))
    ));
    let mut writer = pool.begin().await.unwrap();
    sqlx::query(r#"UPDATE "Playout" SET "Seed"="Seed" WHERE "Id"=1"#)
        .execute(&mut *writer)
        .await
        .unwrap();
    let deadline = Instant::now() + StdDuration::from_millis(100);
    let (db, source, schedule) = (pool.clone(), path.clone(), accepted.clone());
    let waiting = tokio::spawn(async move {
        let mut publication = request(&source, Some(2));
        publication.deadline = Some(deadline);
        snapshot::publish(&db, publication, &schedule).await
    });
    tokio::time::sleep(StdDuration::from_millis(200)).await;
    writer.commit().await.unwrap();
    assert_eq!(waiting.await.unwrap(), Err(SnapshotError::Expired));
    pool.execute(r#"UPDATE "Playout" SET "ScheduleFile"='changed.json' WHERE "Id"=1"#)
        .await
        .unwrap();
    assert_eq!(
        snapshot::publish(&pool, request(&path, Some(2)), &accepted).await,
        Err(SnapshotError::ChangedSource)
    );
    let saved = snapshot::accepted(&pool, 1).await.unwrap().unwrap();
    assert_eq!(saved.revision, 2);
    assert!(saved.schedule == accepted);
    assert_consumers(&pool, &["Accepted"]).await;
}

#[tokio::test]
async fn unsupported_snapshot_fails_closed_and_size_limits_do_not_replace_it() {
    let pool = common::test_db().await;
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("schedule.json");
    seed(&pool, &path).await;
    std::fs::write(&path, document(&["Accepted"])).unwrap();
    let accepted = external_json::load(&path).await.unwrap().unwrap();
    snapshot::publish(&pool, request(&path, None), &accepted)
        .await
        .unwrap();
    let mut oversized = accepted.clone();
    oversized.items[0].program.title = Some("é".repeat(16 * 1024 * 1024 + 1));
    assert_eq!(
        snapshot::publish(&pool, request(&path, Some(1)), &oversized).await,
        Err(SnapshotError::TooLarge)
    );
    assert_eq!(
        snapshot::accepted(&pool, 1)
            .await
            .unwrap()
            .unwrap()
            .revision,
        1
    );
    for (version, document) in [(2, "{}"), (1, "{broken")] {
        sqlx::query(r#"UPDATE "PlayoutExternalJsonSnapshot" SET "SchemaVersion"=?, "Document"=? WHERE "PlayoutId"=1"#)
            .bind(version).bind(document).execute(&pool).await.unwrap();
        assert!(matches!(
            snapshot::load_playout(&pool, 1, &path).await,
            Err(external_json::ExternalJsonError::Snapshot)
        ));
        assert!(playback::select(&pool, 1, common::fixed_time())
            .await
            .is_err());
    }
    pool.execute(r#"DELETE FROM "Playout" WHERE "Id"=1"#)
        .await
        .unwrap();
    assert!(snapshot::accepted(&pool, 1).await.unwrap().is_none());
}

#[tokio::test]
async fn snapshot_migration_leaves_existing_external_playouts_in_direct_mode() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let migrations = sqlx::migrate!("../../migrations");
    sqlx::migrate::Migrator::with_migrations(
        migrations
            .iter()
            .filter(|migration| migration.version < 202_610_041_537)
            .cloned()
            .collect(),
    )
    .run(&pool)
    .await
    .unwrap();
    seed(&pool, Path::new("legacy.json")).await;
    migrations.run(&pool).await.unwrap();
    assert!(snapshot::accepted(&pool, 1).await.unwrap().is_none());
    assert_eq!(
        sqlx::query_scalar::<_, String>(r#"SELECT "ScheduleFile" FROM "Playout" WHERE "Id"=1"#)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "legacy.json"
    );
}
