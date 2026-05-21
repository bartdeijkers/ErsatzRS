#![cfg(not(feature = "mysql"))]
mod common;

use ersatzrs_core::repositories::ConfigRepository;
use ersatzrs_infra::ffmpeg_config::{
    extract_embedded_subtitles, use_embedded_subtitles,
    CONFIG_KEY_FFMPEG_EXTRACT_EMBEDDED_SUBTITLES, CONFIG_KEY_FFMPEG_USE_EMBEDDED_SUBTITLES,
};
use ersatzrs_infra::repositories::make_repositories;

#[tokio::test]
async fn config_repository_sets_and_gets_values() {
    let repos = make_repositories(common::test_db().await);

    repos
        .config
        .set("database.path", "ErsatzRS.db")
        .await
        .unwrap();

    assert_eq!(
        repos.config.get("database.path").await.unwrap(),
        Some("ErsatzRS.db".to_owned())
    );
    assert_eq!(repos.config.get("missing").await.unwrap(), None);
}

#[tokio::test]
async fn ffmpeg_subtitle_config_defaults_when_keys_absent() {
    let pool = common::test_db().await;

    // ErsatzTV defaults: use=true, extract=false.
    assert!(use_embedded_subtitles(&pool).await.unwrap());
    assert!(!extract_embedded_subtitles(&pool).await.unwrap());
}

#[tokio::test]
async fn ffmpeg_subtitle_config_reads_stored_etv_values() {
    let pool = common::test_db().await;

    sqlx::query(r#"REPLACE INTO "ConfigElement" ("Key", "Value") VALUES (?, 'False')"#)
        .bind(CONFIG_KEY_FFMPEG_USE_EMBEDDED_SUBTITLES)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(r#"REPLACE INTO "ConfigElement" ("Key", "Value") VALUES (?, 'True')"#)
        .bind(CONFIG_KEY_FFMPEG_EXTRACT_EMBEDDED_SUBTITLES)
        .execute(&pool)
        .await
        .unwrap();

    assert!(!use_embedded_subtitles(&pool).await.unwrap());
    assert!(extract_embedded_subtitles(&pool).await.unwrap());
}
