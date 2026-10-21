use super::*;
use crate::provider_source_storage::{
    MAXIMUM_SOURCE_BYTES_KEY, MINIMUM_FREE_BYTES_KEY, TOTAL_BYTES_KEY,
};

async fn configure(pool: &Pool<Db>, key: &str, value: Option<&str>) {
    sqlx::query(r#"INSERT OR REPLACE INTO "ConfigElement" VALUES (?,?)"#)
        .bind(key)
        .bind(value)
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn source_storage_budget_changes_warn_opaquely_without_overriding_work_errors() {
    let pool = pool().await;
    let check = TeletextWorkHealthCheck::new(pool.clone());
    assert_eq!(check.check().await.unwrap().message, DEFAULT_MESSAGE);
    for (key, value) in [
        (TOTAL_BYTES_KEY, "4096"),
        (MAXIMUM_SOURCE_BYTES_KEY, "1024"),
        (MINIMUM_FREE_BYTES_KEY, "0"),
    ] {
        configure(&pool, key, Some(value)).await;
    }
    assert_eq!(check.check().await.unwrap().message, DEFAULT_MESSAGE);
    configure(&pool, KEY, Some("2")).await;
    let valid_work = check.check().await.unwrap();
    assert_eq!(valid_work.status, HealthCheckStatus::Pass);
    assert!(valid_work.message.starts_with("The configured limit is 2 "));
    for (key, value) in [
        (TOTAL_BYTES_KEY, Some("private-invalid-value")),
        (TOTAL_BYTES_KEY, Some("0")),
        (TOTAL_BYTES_KEY, Some("512")),
        (MAXIMUM_SOURCE_BYTES_KEY, Some("0")),
        (MAXIMUM_SOURCE_BYTES_KEY, Some("8192")),
        (MINIMUM_FREE_BYTES_KEY, Some("-1")),
        (MINIMUM_FREE_BYTES_KEY, Some("184467440737095516160")),
        (MINIMUM_FREE_BYTES_KEY, None),
    ] {
        configure(&pool, TOTAL_BYTES_KEY, Some("4096")).await;
        configure(&pool, MAXIMUM_SOURCE_BYTES_KEY, Some("1024")).await;
        configure(&pool, MINIMUM_FREE_BYTES_KEY, Some("0")).await;
        configure(&pool, key, value).await;
        let result = check.check().await.unwrap();
        assert_eq!(result.status, HealthCheckStatus::Warning);
        assert_eq!(result.message, INVALID_STORAGE_MESSAGE);
        assert_eq!(result.brief_message, INVALID_STORAGE_MESSAGE);
        assert_eq!(HealthCheckSummary::from_results(&[result]).errors, 0);
    }
    configure(&pool, KEY, Some("0")).await;
    assert_eq!(check.check().await.unwrap().message, INVALID_MESSAGE);
    configure(&pool, KEY, Some("2")).await;
    configure(&pool, MINIMUM_FREE_BYTES_KEY, Some("0")).await;
    assert_eq!(check.check().await.unwrap().message, valid_work.message);
    pool.close().await;
}
