//! Real ASS `CodecPrivate` NUL -> extraction/legacy repair -> Native and Next.
use super::*;
use crate::{ffmpeg_config, file_system_layout, subtitle_cache_nul};
use ersatzrs_domain::{ChannelSubtitleMode, FFmpegProfileVideoFormat, StreamingMode};
use sqlx::Row as _;

#[tokio::test]
#[ignore = "requires exact Next/FFmpeg9 and dedicated app-data; decoded NUL-repaired captions"]
async fn real_native_next_nul_extraction_and_existing_cache_preserve_selected_captions() {
    let _owned_data = runtime_path(file_system_layout::APP_DATA_DIR_ENV_VAR);
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    let root = tempfile::tempdir().unwrap().keep();
    println!("Subtitle NUL proof retained at {}", root.display());
    let clip = nul_ass_fixture(&ffmpeg, &root).await;
    let profile = FFmpegProfile {
        video_format: FFmpegProfileVideoFormat::H264,
        audio_format: FFmpegProfileAudioFormat::Aac,
        audio_channels: 1,
        audio_sample_rate: 48,
        video_bitrate: 1000,
        video_buffer_size: 2000,
        ..Default::default()
    };
    let (pool, locker) = prepare_nul_schedule(&root, &clip, &profile).await;
    prepare_subtitle(&pool).await;
    crate::subtitles::extract_embedded_playout_subtitles(&pool, 1)
        .await
        .unwrap();
    let row = sqlx::query(
        r#"SELECT "Id","IsExtracted","Path" FROM "Subtitle" WHERE "MovieMetadataId"=20"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.get::<i64, _>("IsExtracted"), 1);
    let relative: String = row.get("Path");
    let cache = file_system_layout::subtitle_cache_folder();
    let extracted = cache.join(&relative);
    let clean = tokio::fs::read(&extracted).await.unwrap();
    assert!(String::from_utf8(clean.clone()).unwrap().contains("café"));
    let damaged = nul_copy_bytes(&ffmpeg, &clip, &root, &clean).await;
    for legacy in [false, true] {
        if legacy {
            tokio::fs::write(&extracted, &damaged).await.unwrap();
            native_caption(
                &pool,
                &profile,
                &clip,
                &ffmpeg,
                &root.join("native-negative.ts"),
                false,
            )
            .await;
            let repair = subtitle_cache_nul::repair_once(&pool, &cache)
                .await
                .unwrap();
            assert!(repair.complete);
            assert_eq!(repair.repaired, 1);
            assert_eq!(tokio::fs::read(&extracted).await.unwrap(), clean);
            let unchanged: (i64, String) =
                sqlx::query_as(r#"SELECT "IsExtracted","Path" FROM "Subtitle" WHERE "Id"=?"#)
                    .bind(row.get::<i64, _>("Id"))
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(unchanged, (1, relative.clone()));
            assert_eq!(
                subtitle_cache_nul::repair_once(&pool, &cache)
                    .await
                    .unwrap()
                    .repaired,
                0
            );
        }
        native_caption(
            &pool,
            &profile,
            &clip,
            &ffmpeg,
            &root.join(format!("native-{legacy}.ts")),
            true,
        )
        .await;
        let host = std::sync::Arc::new(host::NextHost::new(
            pool.clone(),
            locker.clone(),
            root.join(format!("playout-{legacy}")),
        ));
        let mut callback = text_subtitle_runtime_tests::callback_listener(host.clone()).await;
        assert_next_session(
            &host,
            host::NextStart {
                channel_id: 1,
                preview: false,
                number: "1".into(),
                executable: runtime_path("ERSATZRS_NEXT_TEST_BIN"),
                output: root.join(format!("next-{legacy}")),
                ffmpeg: ffmpeg.clone(),
                ffprobe: ffprobe.clone(),
                replacements: Vec::new(),
                idle_timeout: None,
                deadline: Instant::now() + Duration::from_secs(30),
            },
            false,
        )
        .await;
        callback.stop().await;
    }
}

async fn prepare_nul_schedule(
    root: &Path,
    clip: &Path,
    profile: &FFmpegProfile,
) -> (crate::db::DbPool, ersatzrs_core::locking::EntityLocker) {
    // Cancelled callbacks may retire the pool's only connection. Persist this
    // session-spanning fixture so its caption rows survive connection replacement.
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(root.join("captions.sqlite"))
                .create_if_missing(true),
        )
        .await
        .unwrap();
    let pool = selection::tests::seed_fixture_pool(pool).await;
    prepare_schedule_with_pool(pool, clip, false, profile).await
}

#[tokio::test]
async fn nul_caption_fixture_survives_connection_replacement() {
    let root = tempfile::tempdir().unwrap();
    let (pool, _) = prepare_nul_schedule(
        root.path(),
        &root.path().join("fixture.mkv"),
        &FFmpegProfile::default(),
    )
    .await;
    prepare_subtitle(&pool).await;
    let sql = r#"SELECT "StreamIndex", "Codec", "Language", "IsExtracted"
                 FROM "Subtitle" WHERE "MovieMetadataId"=20"#;
    let before: (i32, String, String, i64) = sqlx::query_as(sql).fetch_one(&pool).await.unwrap();

    // Cancellation may retire a connection; persisted caption selection must survive it.
    pool.acquire().await.unwrap().close().await.unwrap();

    let after: (i32, String, String, i64) = sqlx::query_as(sql).fetch_one(&pool).await.unwrap();
    assert_eq!(after, before);
    assert_eq!(after, (3, "ass".into(), "nld".into(), 0));
    pool.close().await;
}

async fn nul_copy_bytes(ffmpeg: &Path, clip: &Path, root: &Path, clean: &[u8]) -> Vec<u8> {
    let mut raw = Command::new(ffmpeg);
    raw.args(["-v", "error", "-nostdin", "-y", "-i"])
        .arg(clip)
        .args(["-map", "0:3", "-c:s", "copy"])
        .arg(root.join("raw.ass"));
    tool(raw).await;
    let damaged = tokio::fs::read(root.join("raw.ass")).await.unwrap();
    assert!(
        damaged.contains(&0),
        "the exact runtime must retain the source's header NUL on copy"
    );
    assert_eq!(
        clean,
        damaged
            .iter()
            .copied()
            .filter(|b| *b != 0)
            .collect::<Vec<_>>()
    );
    damaged
}

async fn prepare_subtitle(pool: &crate::db::DbPool) {
    sqlx::raw_sql(r#"
        DELETE FROM "Subtitle";
        DELETE FROM "MediaStream" WHERE "MediaStreamKind"=3;
        INSERT INTO "MediaStream" ("MediaVersionId","Index","MediaStreamKind","Codec","Language","Channels","Default","Forced")
            VALUES (100,3,3,'ass','nld',0,1,0);
        INSERT INTO "Subtitle" ("MovieMetadataId","SubtitleKind","StreamIndex","Codec","Language","Default","Forced","SDH","IsExtracted","DateAdded","DateUpdated")
            VALUES (20,0,3,'ass','nld',1,0,0,0,'2026-10-24','2026-10-24');
        UPDATE "Channel" SET "NextEngineTextSubtitleMode"=0 WHERE "Id"=1;
    "#).execute(pool).await.unwrap();
    for key in [
        ffmpeg_config::CONFIG_KEY_FFMPEG_USE_EMBEDDED_SUBTITLES,
        ffmpeg_config::CONFIG_KEY_FFMPEG_EXTRACT_EMBEDDED_SUBTITLES,
    ] {
        sqlx::query(r#"INSERT INTO "ConfigElement" ("Key","Value") VALUES (?,'true') ON CONFLICT("Key") DO UPDATE SET "Value"='true'"#)
            .bind(key).execute(pool).await.unwrap();
    }
}

async fn nul_ass_fixture(ffmpeg: &Path, root: &Path) -> PathBuf {
    let source = text_subtitle_runtime_tests::generate_text_clip_with_cues(
        ffmpeg,
        root,
        "1\n00:00:00,000 --> 00:01:00,000\nNext subtitle proof – café\n",
    )
    .await;
    let clip = root.join("nul-ass.mkv");
    let mut mux = Command::new(ffmpeg);
    mux.args(["-v", "error", "-nostdin", "-y", "-i"])
        .arg(source)
        .args([
            "-map",
            "0",
            "-c",
            "copy",
            "-c:s",
            "ass",
            "-write_crc32",
            "0",
        ])
        .arg(&clip);
    tool(mux).await;
    let mut bytes = tokio::fs::read(&clip).await.unwrap();
    let marker = b"; Script generated by FFmpeg";
    let positions: Vec<_> = bytes
        .windows(marker.len())
        .enumerate()
        .filter(|(_, v)| *v == marker)
        .map(|(i, _)| i)
        .collect();
    assert_eq!(
        positions.len(),
        1,
        "patch exactly one known ASS CodecPrivate comment"
    );
    // Same element length, with CRC disabled by the standard muxer option.
    // A leading NUL makes the extracted ASS reader stop before its events.
    bytes[positions[0]] = 0;
    tokio::fs::write(&clip, bytes).await.unwrap();
    clip
}

async fn native_caption(
    pool: &crate::db::DbPool,
    profile: &FFmpegProfile,
    clip: &Path,
    ffmpeg: &Path,
    output: &Path,
    expected: bool,
) {
    use ersatzrs_ffmpeg::{
        DesiredVideoState, FrameRate, FrameSize, InputFile, OutputFormat, PipelineBuilder,
        PipelineRequest, SoftwarePipelineBuilder, SourceAudioCodec, SourceAudioStream,
        SourceVideoCodec, SourceVideoStream,
    };
    let subtitle = crate::playback::select_playback_subtitle(
        pool,
        10,
        &["nld".into()],
        ChannelSubtitleMode::Default,
        StreamingMode::TransportStream,
    )
    .await
    .unwrap()
    .unwrap();
    let input = crate::playback::subtitle_input_for_stream(
        &subtitle,
        clip.to_str().unwrap(),
        OutputFormat::MpegTs,
        StreamingMode::TransportStream,
    )
    .unwrap();
    let mut video = SourceVideoStream::new(SourceVideoCodec::H264);
    video.frame_size = Some(FrameSize::new(320, 180));
    let desired = DesiredVideoState::from_profile(
        profile,
        &Resolution {
            width: 320,
            height: 180,
            ..Default::default()
        },
        Some(&video),
        Some(FrameRate::new(25, 1)),
        StreamingMode::TransportStream,
    );
    let mut audio = SourceAudioStream::new(SourceAudioCodec::Aac).with_index(2);
    audio.channels = Some(1);
    audio.sample_rate = Some(48000);
    let mut request = PipelineRequest::new(InputFile::video(clip.to_str().unwrap()), desired)
        .with_source_video_stream(video)
        .with_source_audio_streams(vec![audio]);
    request.inputs.push(input);
    request.state.duration = Some(Duration::from_secs(2));
    request.state.report_progress = false;
    request.state.thread_count = Some(2);
    request.state.metadata.audio_language = Some("nld".into());
    let mut args = SoftwarePipelineBuilder
        .build(request.clone())
        .command(&request.inputs)
        .arguments;
    assert_eq!(args.pop().as_deref(), Some("pipe:1"));
    tokio::fs::write(
        output.with_extension("args.json"),
        serde_json::to_vec_pretty(&args).unwrap(),
    )
    .await
    .unwrap();
    let mut encode = Command::new(ffmpeg);
    encode.args(args).arg(output);
    tool(encode).await;
    text_subtitle_runtime_tests::assert_text_video_and_audio(ffmpeg, output, expected).await;
}
