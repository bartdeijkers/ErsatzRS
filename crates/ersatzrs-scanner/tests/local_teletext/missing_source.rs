//! SI2: disappearing/restored physical input retains metadata, never authority.

use super::*;
use ersatzrs_core::locking::EntityLocker;
use ersatzrs_domain::{ChannelSubtitleMode, SourceTeletextCaptionDelivery, StreamingMode};
use tokio::time::Instant;

#[tokio::test]
#[ignore = "requires pinned FFprobe 9 and synthetic two-audio AV/200/888 input"]
async fn native_missing_source_rescan_preserves_tracks_and_restores_fresh_page_identity() {
    let (probe, fixture) = inputs();
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("Synthetic.ts");
    std::fs::copy(&fixture, &path).unwrap();
    std::fs::write(
        path.with_extension("nld.srt"),
        "1\n00:00:00,000 --> 00:00:01,000\nSynthetic ordinary caption\n",
    )
    .unwrap();
    let pool = database(root.path(), 4).await;
    let config = config(&pool, &probe).await;
    scan(&pool, &config).await;
    let before_tracks = tracks(&pool, 1).await;
    let before_subtitles = subtitles(&pool, 1).await;
    let before_reports = reports(&pool, 1).await;
    let modified = path.metadata().unwrap().modified().unwrap();
    let (item, version, file) = source_ids(&pool).await;
    assert_eq!(
        teletext_source::load_caption_candidates(
            &pool,
            item,
            version,
            file,
            path.to_str().unwrap()
        )
        .await
        .unwrap()
        .len(),
        1
    );
    std::fs::remove_file(&path).unwrap();
    scan(&pool, &config).await;
    let state: i64 = sqlx::query_scalar(r#"SELECT "State" FROM "MediaItem" WHERE "Id"=?"#)
        .bind(item)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(state, 1, "the scan must mark the missing physical item");
    assert_ordinary_fallback(&pool, &path, (item, version, file)).await;
    assert_eq!(tracks(&pool, 1).await, before_tracks);
    assert_eq!(subtitles(&pool, 1).await, before_subtitles);
    std::fs::copy(&fixture, &path).unwrap();
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(
            std::fs::FileTimes::new().set_modified(modified + std::time::Duration::from_secs(1)),
        )
        .unwrap();
    scan(&pool, &config).await;
    let state: i64 = sqlx::query_scalar(r#"SELECT "State" FROM "MediaItem" WHERE "Id"=?"#)
        .bind(item)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(state, 0);
    let after_reports = reports(&pool, 1).await;
    assert_eq!(after_reports[0].0, before_reports[0].0);
    assert_ne!(after_reports[0].1, before_reports[0].1);
    assert_eq!(tracks(&pool, 1).await, before_tracks);
    assert_eq!(subtitles(&pool, 1).await, before_subtitles);
    assert_eq!((item, version, file), source_ids(&pool).await);
    let candidates = teletext_source::load_caption_candidates(
        &pool,
        item,
        version,
        file,
        path.to_str().unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(
        candidates[0].request.binding.source_revision,
        after_reports[0].1
    );
    pool.close().await;
}

async fn assert_ordinary_fallback(pool: &SqlitePool, path: &Path, ids: (i64, i64, i64)) {
    let (item, version, file) = ids;
    let locker = EntityLocker::new();
    let selected = Box::pin(runtime::select(RuntimeSelectionContext {
        pool,
        locker: &locker,
        media_item_id: Some(item),
        media_version_id: Some(version),
        media_file_id: Some(file),
        original_path: path.to_str().unwrap(),
        resolved_path: path.to_str().unwrap(),
        languages: &["en".to_owned(), "nl".to_owned()],
        subtitle_mode: ChannelSubtitleMode::Any,
        streaming_mode: StreamingMode::TransportStream,
        source_teletext_delivery: SourceTeletextCaptionDelivery::Burn,
        playback_interval: None,
        deadline: Instant::now() + std::time::Duration::from_secs(20),
    }))
    .await
    .unwrap()
    .unwrap();
    assert!(
        selected.source.is_none(),
        "persisted observations cannot qualify a missing input"
    );
    assert_eq!(selected.subtitle.unwrap().language.as_deref(), Some("dut"));
    assert_eq!(locker.active_teletext_work(), 0);
}

async fn source_ids(pool: &SqlitePool) -> (i64, i64, i64) {
    sqlx::query_as(
        r#"SELECT v."OtherVideoId",v."Id",f."Id" FROM "MediaVersion" v JOIN "MediaFile" f ON f."MediaVersionId"=v."Id""#,
    ).fetch_one(pool).await.unwrap()
}
