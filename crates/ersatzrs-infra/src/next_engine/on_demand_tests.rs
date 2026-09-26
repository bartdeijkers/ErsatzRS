use super::*;
use crate::repositories::{common::encode_datetime, make_repositories};
use chrono::Duration as TimeDelta;
use ersatzrs_core::{
    playout_builder::PlayoutBuilder,
    repositories::PlayoutRepository as _,
    scheduling::{
        PlayoutBuildMode, SchedulingMediaItem, SchedulingReferenceData, SchedulingRequest,
    },
};

async fn fixture() -> (tempfile::TempDir, DbPool, EntityLocker) {
    let root = tempfile::tempdir().unwrap();
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(root.path().join("restart.sqlite"))
                .create_if_missing(true),
        )
        .await
        .unwrap();
    let pool = super::super::selection::tests::seed_fixture_pool(pool).await;
    let (pool, locker, _, start) = super::super::schedule::tests::fixture_with_pool(pool).await;
    let yaml = "content:\n  - key: movies\nplayout:\n  - content: movies\n    count: 2\n  - repeat: true\n";
    sqlx::query(
        r#"UPDATE "Playout" SET "ScheduleKind"='Sequential',"ScheduleFile"=? WHERE "Id"=1"#,
    )
    .bind(yaml)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::raw_sql(
        r#"UPDATE "Channel" SET "PlayoutMode"=1;
        INSERT INTO "MediaItem" ("Id","LibraryPathId","State") VALUES (11,1,0);
        INSERT INTO "Movie" ("Id") VALUES (11);"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    let repos = make_repositories(pool.clone());
    sqlx::query(r#"INSERT OR REPLACE INTO "ConfigElement" ("Key","Value") VALUES (?, '0')"#)
        .bind(ersatzrs_domain::CONFIG_KEY_FFMPEG_SEGMENTER_TIMEOUT)
        .execute(&pool)
        .await
        .unwrap();
    let playout = repos.playout.get_with_anchor(1).await.unwrap();
    let result = PlayoutBuilder::default()
        .build(SchedulingRequest {
            playout: playout.clone(),
            start,
            finish: start + TimeDelta::minutes(5),
            mode: PlayoutBuildMode::Reset,
            reference_data: SchedulingReferenceData {
                media_by_key: std::collections::BTreeMap::from([(
                    "movies".into(),
                    vec![
                        SchedulingMediaItem::new(10, TimeDelta::seconds(60)),
                        SchedulingMediaItem::new(11, TimeDelta::seconds(60)),
                    ],
                )]),
                ..SchedulingReferenceData::default()
            },
        })
        .unwrap();
    assert!(result.added_items.len() >= 5);
    assert!(result.anchor.is_some());
    repos
        .playout
        .apply_build_result(&result, &playout, TimeDelta::zero())
        .await
        .unwrap();
    (root, pool, locker)
}

async fn checkpoint(pool: &DbPool) -> Option<String> {
    sqlx::query_scalar(r#"SELECT "OnDemandCheckpoint" FROM "Playout" WHERE "Id"=1"#)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn items(pool: &DbPool) -> Vec<ersatzrs_domain::PlayoutItem> {
    make_repositories(pool.clone())
        .playout
        .items_in_range(1, DateTime::UNIX_EPOCH, Utc::now() + TimeDelta::days(2))
        .await
        .unwrap()
}

#[tokio::test]
async fn generated_timeline_resumes_after_database_restart_without_resetting_future_items() {
    let (root, pool, locker) = fixture().await;
    let progress = Checkpoints::new(pool.clone(), locker.clone(), 1, false);
    progress.prepare().await.unwrap();
    let before = items(&pool).await;
    let touch = before[0].start + TimeDelta::seconds(75);
    assert!(progress.save(touch).await);
    assert_eq!(checkpoint(&pool).await, Some(encode_datetime(touch)));
    let database = root.path().join("restart.sqlite");
    pool.close().await;
    let restarted = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(sqlx::sqlite::SqliteConnectOptions::new().filename(database))
        .await
        .unwrap();
    Checkpoints::new(restarted.clone(), EntityLocker::default(), 1, false)
        .prepare()
        .await
        .unwrap();
    let after = items(&restarted).await;
    let new_checkpoint = checkpoint(&restarted).await.unwrap();
    let resumed =
        crate::repositories::common::decode_datetime("on-demand checkpoint", &new_checkpoint)
            .unwrap();
    let offset = resumed - touch;
    assert_eq!(after.len(), before.len() - 1);
    for (old, new) in before.iter().skip(1).zip(&after) {
        assert_eq!(
            (old.id, old.media_item_id, old.in_point, old.out_point),
            (new.id, new.media_item_id, new.in_point, new.out_point)
        );
        assert_eq!(new.start, old.start + offset);
        assert_eq!(new.finish, old.finish + offset);
        assert_eq!(
            new.preferred_audio_language_code,
            old.preferred_audio_language_code
        );
        assert_eq!(new.subtitle_mode, old.subtitle_mode);
    }
    assert_eq!(resumed - after[0].start, TimeDelta::seconds(15));
}

#[tokio::test]
async fn previews_and_continuous_channels_do_not_shift_or_checkpoint() {
    for preview in [true, false] {
        let (_root, pool, locker) = fixture().await;
        if !preview {
            sqlx::query(r#"UPDATE "Channel" SET "PlayoutMode"=0"#)
                .execute(&pool)
                .await
                .unwrap();
        }
        let before = items(&pool).await;
        let progress = Checkpoints::new(pool.clone(), locker, 1, preview);
        progress.prepare().await.unwrap();
        assert!(!progress.save(Utc::now()).await);
        assert_eq!(items(&pool).await, before);
        assert_eq!(checkpoint(&pool).await, None);
    }
}

#[tokio::test]
async fn locked_playout_defers_startup_and_checkpoint_timeout_keeps_existing_position() {
    let (_root, pool, locker) = fixture().await;
    let progress = Checkpoints::new(pool.clone(), locker.clone(), 1, false);
    let held = PlayoutLockGuard::try_acquire(&locker, 1).unwrap();
    assert!(timeout(Duration::from_millis(50), progress.prepare())
        .await
        .is_err());
    assert_eq!(checkpoint(&pool).await, None);
    drop(held);
    progress.prepare().await.unwrap();
    let old = checkpoint(&pool).await;
    let _held = PlayoutLockGuard::try_acquire(&locker, 1).unwrap();
    tokio::time::pause();
    assert!(!progress.save(Utc::now() + TimeDelta::seconds(10)).await);
    tokio::time::resume();
    assert_eq!(checkpoint(&pool).await, old);
}

#[tokio::test]
async fn periodic_and_final_checkpoints_use_last_viewer_touch_not_heartbeat_time() {
    use ersatzrs_core::hls::HlsSessionHandle as _;
    use std::sync::Arc;
    let (_root, pool, locker) = fixture().await;
    let progress = Arc::new(Checkpoints::new(pool.clone(), locker, 1, false));
    progress.prepare().await.unwrap();
    let initial = checkpoint(&pool).await;
    let root = tempfile::tempdir().unwrap();
    let process = super::super::process::fixture_process(root.path(), "ready")
        .await
        .unwrap();
    let session = Arc::new(NextSession::new(
        "1".into(),
        root.path().into(),
        Instant::now() + Duration::from_secs(30),
    ));
    let (tx, rx) = tokio::sync::oneshot::channel();
    let mut tx = Some(tx);
    let running = session.clone();
    let task = tokio::spawn(async move {
        running
            .run(async { Ok(process) }, None, move || {
                if let Some(tx) = tx.take() {
                    tx.send(()).unwrap();
                }
                async { Ok(()) }
            })
            .await
    });
    session.playout_updated();
    rx.await.unwrap();
    session.touch();
    let touch = session.checkpoint_time().unwrap();
    let observer = progress.clone();
    let active_session = session.clone();
    tokio::time::pause();
    let timer = tokio::spawn(async move { observer.run(&active_session).await });
    tokio::task::yield_now().await;
    tokio::time::advance(
        CHECKPOINT_INTERVAL
            .checked_sub(Duration::from_secs(1))
            .unwrap(),
    )
    .await;
    assert_eq!(checkpoint(&pool).await, initial);
    tokio::time::advance(Duration::from_secs(1)).await;
    tokio::time::resume();
    timeout(Duration::from_secs(3), async {
        while checkpoint(&pool).await != Some(encode_datetime(touch)) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    session.cancel().await;
    task.await.unwrap().unwrap();
    timer.abort();
    assert!(timer.await.unwrap_err().is_cancelled());
    progress.finish(&session).await;
    assert_eq!(checkpoint(&pool).await, Some(encode_datetime(touch)));
}

#[tokio::test]
async fn checkpoint_preserves_segmenter_safety_interval_and_never_rewinds() {
    let (_root, pool, locker) = fixture().await;
    sqlx::query(r#"DELETE FROM "ConfigElement" WHERE "Key"=?"#)
        .bind(ersatzrs_domain::CONFIG_KEY_FFMPEG_SEGMENTER_TIMEOUT)
        .execute(&pool)
        .await
        .unwrap();
    let progress = Checkpoints::new(pool.clone(), locker, 1, false);
    progress.prepare().await.unwrap();
    let start = items(&pool).await[0].start;
    assert!(progress.save(start + TimeDelta::seconds(30)).await);
    assert_eq!(checkpoint(&pool).await, Some(encode_datetime(start)));
    assert!(progress.save(start + TimeDelta::seconds(90)).await);
    assert_eq!(
        checkpoint(&pool).await,
        Some(encode_datetime(start + TimeDelta::seconds(30)))
    );
    assert!(progress.save(start + TimeDelta::seconds(80)).await);
    assert_eq!(
        checkpoint(&pool).await,
        Some(encode_datetime(start + TimeDelta::seconds(30)))
    );
}
