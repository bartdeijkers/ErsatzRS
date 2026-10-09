use chrono::Duration;
use ersatzrs_core::{
    repositories::TroubleshootingRepository as _, scheduling::SchedulingCollectionKey,
};
use ersatzrs_domain::CollectionType;

use crate::{
    db::DbPool,
    repositories::{make_repositories, SqliteSchedulingContentRepository},
};

pub(crate) async fn image_fixture() -> (
    DbPool,
    ersatzrs_domain::Channel,
    chrono::DateTime<chrono::Utc>,
) {
    let (pool, _, channel, start) = crate::next_engine::schedule::tests::fixture().await;
    sqlx::raw_sql(r#"
        INSERT INTO "Image" ("Id") VALUES (10);
        UPDATE "MediaVersion" SET "MovieId"=NULL,"ImageId"=10,"Duration"='00:00:00' WHERE "Id"=100;
        DELETE FROM "Movie" WHERE "Id"=10;
        UPDATE "MediaStream" SET "Codec"='png',"PixelFormat"='rgb24' WHERE "MediaVersionId"=100 AND "Index"=0;
        DELETE FROM "MediaStream" WHERE "MediaVersionId"=100 AND "Index"<>0;
    "#).execute(&pool).await.unwrap();
    (pool, channel, start)
}

async fn assert_consumers(pool: &DbPool, kind: CollectionType, expected: Duration) {
    let items = SqliteSchedulingContentRepository::new(pool.clone())
        .items_for_collection_key(
            &SchedulingCollectionKey {
                collection_type: kind,
                media_item_id: Some(10),
                ..Default::default()
            },
            0,
        )
        .await
        .unwrap();
    assert_eq!(
        items.len(),
        1,
        "metadata/files/versions must not duplicate candidates"
    );
    assert_eq!(items[0].duration, expected);
    let preview = make_repositories(pool.clone())
        .troubleshooting
        .load_preview_media_admin(10, None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(preview.media_version_id, 100);
    assert!((preview.duration_seconds - expected.as_seconds_f64()).abs() < f64::EPSILON);
    let hint = crate::next_engine::probe_hint::persisted(pool, 100)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        hint.duration_ms,
        u64::try_from(expected.num_milliseconds())
            .ok()
            .filter(|v| *v > 0)
    );
}

#[tokio::test]
async fn image_duration_is_shared_without_metadata_multiplication_or_playout_writes() {
    let (pool, mut channel, start) = image_fixture().await;
    assert_consumers(&pool, CollectionType::Image, Duration::seconds(15)).await;
    sqlx::raw_sql(r#"
        INSERT INTO "ImageMetadata" ("Id","ImageId","Title","DurationSeconds","MetadataKind","DateAdded","DateUpdated")
        VALUES (30,10,'First',NULL,0,'2026-10-06','2026-10-06'),(31,10,'Second',999,0,'2026-10-06','2026-10-06');
        INSERT INTO "FillerPreset" ("Id","AllowWatermarks","CollectionType","MediaItemId","FillerKind","FillerMode","UseChaptersAsMediaItems") VALUES (1,0,60,10,4,0,1);
    "#).execute(&pool).await.unwrap();
    channel.fallback_filler_id = Some(1);
    for (seconds, expected_ms) in [
        (None, 15_000),
        (Some(18.125), 18_125),
        (Some(600.0), 600_000),
        (Some(0.0), 0),
    ] {
        sqlx::query(r#"UPDATE "ImageMetadata" SET "DurationSeconds"=? WHERE "Id"=30"#)
            .bind(seconds)
            .execute(&pool)
            .await
            .unwrap();
        let expected = Duration::milliseconds(expected_ms);
        assert_consumers(&pool, CollectionType::Image, expected).await;
        let now = start + Duration::seconds(120);
        let candidates = crate::fallback_playback::load_candidates(
            &pool,
            &channel,
            now,
            now + Duration::seconds(30),
        )
        .await
        .unwrap()
        .unwrap();
        let item = candidates.materialize(0);
        if expected_ms == 0 {
            assert!(item.is_none());
        } else {
            let item = item.unwrap();
            assert_eq!(item.finish - now, expected.min(Duration::seconds(30)));
            assert_eq!(item.out_point, expected.min(Duration::seconds(30)));
        }
    }
    let count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "PlayoutItem""#)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        count, 2,
        "dynamic duration reads must not persist filler items"
    );
}

#[tokio::test]
async fn remote_duration_only_fills_zero_head_version_and_explicit_version_is_preserved() {
    let (pool, _, _, _) = crate::next_engine::schedule::tests::fixture().await;
    sqlx::raw_sql(r#"
        INSERT INTO "RemoteStream" ("Id","Url","Duration","Liveness","IsLive") VALUES (10,'https://example.test/synthetic','00:01:23.125',1,0);
        UPDATE "MediaVersion" SET "MovieId"=NULL,"RemoteStreamId"=10,"Duration"='00:00:00' WHERE "Id"=100;
        DELETE FROM "Movie" WHERE "Id"=10;
        INSERT INTO "MediaVersion" ("Id","DateAdded","DateUpdated","Duration","RemoteStreamId","Height","Width","VideoScanKind")
        VALUES (101,'2026-10-06','2026-10-06','00:04:00',10,180,320,0);
        INSERT INTO "MediaFile" ("Id","MediaVersionId","Path","PathHash") VALUES (3,101,'synthetic/alternate.mkv','alternate');
    "#).execute(&pool).await.unwrap();
    for (version, fallback, expected_ms) in [
        ("00:00:00", Some("00:01:23.125"), 83_125),
        ("00:00:30", Some("00:01:23.125"), 30_000),
        ("00:00:00.0000000", Some("00:00:12"), 12_000),
        ("00:00:00", None, 0),
    ] {
        sqlx::query(r#"UPDATE "MediaVersion" SET "Duration"=? WHERE "Id"=100"#)
            .bind(version)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(r#"UPDATE "RemoteStream" SET "Duration"=? WHERE "Id"=10"#)
            .bind(fallback)
            .execute(&pool)
            .await
            .unwrap();
        assert_consumers(
            &pool,
            CollectionType::RemoteStream,
            Duration::milliseconds(expected_ms),
        )
        .await;
    }
    let preview = make_repositories(pool.clone())
        .troubleshooting
        .load_preview_media_admin(10, Some(101))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(preview.media_version_id, 101);
    assert!((preview.duration_seconds - 240.0).abs() < f64::EPSILON);
}
