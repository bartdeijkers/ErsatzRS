use super::*;
use ersatzrs_core::repositories::{ConfigRepository, FfmpegProfileRepository};
use ersatzrs_domain::{FFmpegProfile, FFmpegProfileAudioFormat, FFmpegProfileVideoFormat};

#[tokio::test]
async fn next_runtime_config_preserves_fonts_and_error_reports() {
    let (pool, locker, _, _) = super::super::schedule::tests::fixture().await;
    crate::repositories::make_repositories(pool.clone())
        .ffmpeg_profile
        .upsert(&FFmpegProfile {
            id: 1,
            resolution_id: 1,
            video_format: FFmpegProfileVideoFormat::H264,
            audio_format: FFmpegProfileAudioFormat::Aac,
            ..Default::default()
        })
        .await
        .unwrap();
    let root = tempfile::tempdir().unwrap();
    let host = NextHost::new(pool.clone(), locker, root.path().join("playouts"));
    let request = NextStart {
        channel_id: 1,
        preview: false,
        number: "1".into(),
        executable: "fixture-next".into(),
        output: root.path().join("output"),
        ffmpeg: "fixture-ffmpeg".into(),
        ffprobe: "fixture-ffprobe".into(),
        replacements: Vec::new(),
        idle_timeout: None,
        deadline: Instant::now() + Duration::from_secs(30),
    };
    let key = ersatzrs_domain::CONFIG_KEY_FFMPEG_SAVE_REPORTS;
    sqlx::query(r#"DELETE FROM "ConfigElement" WHERE "Key" = ?"#)
        .bind(key)
        .execute(&pool)
        .await
        .unwrap();
    let initial = serde_json::to_value(host.configuration(&request).await.unwrap()).unwrap();
    assert!(initial["ffmpeg"]["reports_folder"].is_string());
    for raw in [
        "False", "0", "invalid", " true ", "True", "TRUE", "1", "false",
    ] {
        crate::repositories::make_repositories(pool.clone())
            .config
            .set(key, raw)
            .await
            .unwrap();
        // Normal sessions and browser previews always retain error reports.
        for preview in [false, true] {
            let mut request = request.clone();
            request.preview = preview;
            let config = host.configuration(&request).await.unwrap();
            let value = serde_json::to_value(config).unwrap();
            assert_eq!(
                value["normalization"]["subtitle"]["fonts_folder"],
                crate::file_system_layout::fonts_cache_folder()
                    .to_str()
                    .unwrap()
            );
            assert_eq!(
                value["ffmpeg"]["reports_folder"],
                crate::file_system_layout::ffmpeg_reports_folder()
                    .to_str()
                    .unwrap()
            );
            assert!(value["ffmpeg"].get("save_reports").is_none());
        }
    }
    assert_eq!(
        crate::file_system_layout::ffmpeg_reports_folder(),
        crate::file_system_layout::app_data_folder().join("ffmpeg-reports")
    );
}
