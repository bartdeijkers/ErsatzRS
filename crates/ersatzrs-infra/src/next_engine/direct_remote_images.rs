//! Finite/live direct HTTP image selection, with both text modes and explicit off.
use super::super::super::image_subtitle_runtime_tests as bitmap;
use super::*;

#[tokio::test]
#[ignore = "requires official Next and FFmpeg; direct HTTP embedded PGS proof"]
async fn real_next_direct_http_images_preserve_selection_and_off() {
    prepared_artifact_runtime::run_isolated_test("next_engine::process::runtime_tests::text_subtitle_runtime_tests::direct_remote_runtime::images::fixture_child_direct_images").await;
}

#[tokio::test]
#[ignore = "child of real_next_direct_http_images_preserve_selection_and_off"]
async fn fixture_child_direct_images() {
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    ersatzrs_ffmpeg::set_resolved_ffmpeg_tools(ffmpeg.to_str().unwrap(), ffprobe.to_str().unwrap());
    let root = tempfile::tempdir().unwrap();
    let clip = bitmap::generate_bitmap_clip(&ffmpeg, root.path()).await;
    let http = servers::Http::start(clip.clone()).await;
    let mut results = Vec::new();
    for live in [false, true] {
        for (index, subtitles) in [Some(false), Some(true), None].into_iter().enumerate() {
            results.push(
                observed_case(
                    &clip,
                    &http.uri,
                    Case {
                        live,
                        subtitles,
                        source: CaptionSource::EmbeddedImage,
                    },
                    root.path().join(format!("{live}-{index}")),
                    &ffmpeg,
                    &ffprobe,
                )
                .await,
            );
        }
    }
    http.stop().await;
    for result in results {
        result.expect("direct HTTP PGS selection and off");
    }
}

pub(super) async fn configure(pool: &crate::db::DbPool, mode: Option<bool>) {
    sqlx::raw_sql(r#"
        DELETE FROM "MediaStream" WHERE "MediaStreamKind"=3;
        INSERT INTO "MediaStream" ("Id","Index","Codec","MediaStreamKind","Language","Channels","Title","Default","Forced","MediaVersionId") VALUES
            (4,3,'hdmv_pgs_subtitle',3,'eng',0,'English',1,0,100),
            (5,4,'hdmv_pgs_subtitle',3,'nld',0,'Dutch',0,0,100);
        INSERT INTO "Subtitle" ("RemoteStreamMetadataId","SubtitleKind","Title","StreamIndex","Codec","Default","Forced","SDH","Language","IsExtracted","DateAdded","DateUpdated") VALUES
            (30,0,'English',3,'hdmv_pgs_subtitle',1,0,0,'eng',0,'2026-09-23','2026-09-23'),
            (30,0,'Dutch',4,'hdmv_pgs_subtitle',0,0,0,'nld',0,'2026-09-23','2026-09-23');
    "#).execute(pool).await.unwrap();
    sqlx::query(
        r#"UPDATE "Channel" SET "SubtitleMode"=?,"NextEngineTextSubtitleMode"=? WHERE "Id"=1"#,
    )
    .bind(if mode.is_some() { 2 } else { 0 })
    .bind(i32::from(mode.unwrap_or(false)))
    .execute(pool)
    .await
    .unwrap();
}

pub(super) async fn assert_output(ffmpeg: &Path, segment: &Path, off: bool) {
    bitmap::assert_bitmap_output(ffmpeg, segment, off).await;
}
