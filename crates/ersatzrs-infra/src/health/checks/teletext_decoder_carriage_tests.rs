use ersatzrs_domain::channel_source_teletext_delivery_key;
use sqlx::sqlite::SqlitePoolOptions;
use uuid::Uuid;

use super::*;

fn capabilities() -> FfmpegCapabilityReport {
    let mut result = FfmpegCapabilityReport::default();
    result.encoders.insert("dvbsub".into());
    result.decoders.insert("dvbsub".into());
    result.bitstream_filters.insert("setts".into());
    result
        .muxers
        .extend(["matroska".into(), "mpegts".into(), "segment".into()]);
    result.demuxers.extend(["matroska".into(), "concat".into()]);
    result
}

fn decoder() -> TeletextDecoderCapability {
    TeletextDecoderCapability {
        status: TeletextDecoderStatus::Ready,
        missing_options: Vec::new(),
        original_pts: false,
    }
}

async fn pool() -> DbPool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query(
        r#"CREATE TABLE "Channel" ("Id" INTEGER PRIMARY KEY, "UniqueId" TEXT NOT NULL,
           "StreamingEngine" INTEGER NOT NULL, "StreamingMode" INTEGER NOT NULL,
           "IsEnabled" INTEGER NOT NULL)"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(r#"CREATE TABLE "ConfigElement" ("Key" TEXT PRIMARY KEY, "Value" TEXT)"#)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "Channel" VALUES (1, ?, 0, 1, 1)"#)
        .bind(Uuid::from_u128(1).to_string())
        .execute(&pool)
        .await
        .unwrap();
    pool
}

async fn setting(pool: &DbPool, uuid: u128, value: Option<&str>) {
    sqlx::query(
        r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES (?, ?)
           ON CONFLICT ("Key") DO UPDATE SET "Value" = excluded."Value""#,
    )
    .bind(channel_source_teletext_delivery_key(&Uuid::from_u128(uuid)))
    .bind(value)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn explicit_carriage_health_ignores_defaults_burn_disabled_and_orphaned_keys() {
    let pool = pool().await;
    let context = CarriageContext::new(pool.clone(), &FfmpegCapabilityReport::default());
    assert_eq!(context.check(&decoder()).await.unwrap(), None);
    setting(&pool, 1, Some("0")).await;
    setting(&pool, 2, Some("1")).await;
    assert_eq!(context.check(&decoder()).await.unwrap(), None);
    setting(&pool, 1, Some("1")).await;
    sqlx::query(r#"UPDATE "Channel" SET "IsEnabled" = 0"#)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(context.check(&decoder()).await.unwrap(), None);
    pool.close().await;
}

#[tokio::test]
async fn explicit_carriage_health_names_requirements_without_claiming_source_or_client_readiness() {
    let pool = pool().await;
    setting(&pool, 1, Some("1")).await;
    let context = CarriageContext::new(pool.clone(), &capabilities());
    for mode in [1, 5] {
        sqlx::query(r#"UPDATE "Channel" SET "StreamingMode" = ?"#)
            .bind(mode)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            context.check(&decoder()).await.unwrap(),
            Some((HealthCheckStatus::Pass, READY))
        );
        let mut without_segmenter = capabilities();
        without_segmenter.muxers.remove("segment");
        assert_eq!(
            CarriageContext::new(pool.clone(), &without_segmenter)
                .check(&decoder())
                .await
                .unwrap(),
            Some(if mode == 5 {
                (HealthCheckStatus::Warning, MISSING)
            } else {
                (HealthCheckStatus::Pass, READY)
            })
        );
    }
    let mut missing = capabilities();
    missing.encoders.remove("dvbsub");
    assert_eq!(
        CarriageContext::new(pool.clone(), &missing)
            .check(&decoder())
            .await
            .unwrap(),
        Some((HealthCheckStatus::Warning, MISSING))
    );
    let mut missing = capabilities();
    missing.bitstream_filters.remove("setts");
    assert_eq!(
        CarriageContext::new(pool.clone(), &missing)
            .check(&decoder())
            .await
            .unwrap(),
        Some((HealthCheckStatus::Warning, MISSING))
    );
    assert_eq!(
        context
            .check(&TeletextDecoderCapability::default())
            .await
            .unwrap(),
        Some((HealthCheckStatus::Warning, MISSING))
    );
    pool.close().await;
}

#[tokio::test]
async fn explicit_carriage_health_reports_invalid_values_and_incompatible_output_without_echoing_them(
) {
    let pool = pool().await;
    let context = CarriageContext::new(pool.clone(), &capabilities());
    for value in [Some("private-invalid-choice"), Some("+1"), Some(""), None] {
        setting(&pool, 1, value).await;
        assert_eq!(
            context.check(&decoder()).await.unwrap(),
            Some((HealthCheckStatus::Warning, INVALID))
        );
    }
    setting(&pool, 1, Some("1")).await;
    for (engine, mode) in [(1, 5), (0, 2), (0, 4)] {
        sqlx::query(r#"UPDATE "Channel" SET "StreamingEngine" = ?, "StreamingMode" = ?"#)
            .bind(engine)
            .bind(mode)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            context.check(&decoder()).await.unwrap(),
            Some((HealthCheckStatus::Warning, OUTPUT))
        );
    }
    pool.close().await;
}
