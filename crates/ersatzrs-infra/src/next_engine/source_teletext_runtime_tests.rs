//! Finite Local acquisition/projection and actual Next page-qualified conversion.
//! This reuses real probe metadata; it does not execute the scanner crate.

use super::*;
use ersatzrs_core::repositories::PlayoutRepository as _;

#[path = "source_teletext_runtime_tests/cache.rs"]
mod cache;
#[path = "source_teletext_runtime_tests/clock.rs"]
mod clock;
#[path = "source_teletext_runtime_tests/fixture.rs"]
mod fixture;

#[path = "source_teletext_runtime_tests/eof.rs"]
mod eof;

// The bounded executable decoder gate intentionally requires Unix permissions.
#[cfg(unix)]
#[path = "source_teletext_runtime_tests/replacement.rs"]
mod replacement;

struct Case {
    name: &'static str,
    start_ms: i64,
    end_ms: i64,
    captions: bool,
    callback_required: bool,
    caption_track_required: bool,
}

#[tokio::test]
#[ignore = "requires pinned Next0.2.2, FFmpeg9, Python3 and fresh ERSATZRS_NEXT_TELETEXT_PROOF_DIR"]
async fn real_next_local_teletext_convert_selects_page889_and_preserves_copy_and_audio() {
    let root = fixture::proof_root();
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    let clip = fixture::source(&ffmpeg, &root).await;
    fixture::independent_pages(&ffmpeg, &clip, &root).await;
    run_selected_page(&ffmpeg, &ffprobe, &root, &clip, &[888, 889]).await;
}

#[tokio::test]
#[ignore = "requires pinned Next0.2.2, FFmpeg9, Python3 and fresh ERSATZRS_NEXT_TELETEXT_PROOF_DIR"]
async fn real_next_local_teletext_convert_retains_page889_beyond_first_source_window() {
    let root = fixture::proof_root();
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    let clip = fixture::source_with_mux_rate(&ffmpeg, &root, 8_000_000).await;
    fixture::independent_pages(&ffmpeg, &clip, &root).await;
    fixture::assert_page_spans_source_window(&ffmpeg, &clip, &root).await;
    run_selected_page(&ffmpeg, &ffprobe, &root, &clip, &[888, 889]).await;
}

#[tokio::test]
#[ignore = "requires pinned Next0.2.2, FFmpeg9, Python3 and fresh ERSATZRS_NEXT_TELETEXT_PROOF_DIR"]
async fn real_next_local_teletext_convert_selects_page889_first_observed_after_prefix() {
    let root = fixture::proof_root();
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    let clip = fixture::source_with_late_caption(&ffmpeg, &root).await;
    fixture::independent_pages(&ffmpeg, &clip, &root).await;
    run_selected_page(&ffmpeg, &ffprobe, &root, &clip, &[888]).await;
}

#[tokio::test]
#[ignore = "requires pinned Next0.2.2, FFmpeg9, Python3 and fresh ERSATZRS_NEXT_TELETEXT_PROOF_DIR"]
async fn real_next_ordinary_srt_copy_control_keeps_original_source_outpoint() {
    let root = fixture::proof_root();
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    let clip = fixture::source_with_late_caption(&ffmpeg, &root).await;
    fixture::independent_pages(&ffmpeg, &clip, &root).await;
    let profile = FFmpegProfile {
        video_format: FFmpegProfileVideoFormat::Copy,
        audio_format: FFmpegProfileAudioFormat::Copy,
        audio_sample_rate: 48,
        audio_channels: 1,
        ..text_profile()
    };
    let (pool, locker) = prepare_schedule(&clip, false, &profile).await;
    fixture::acquire_and_store_observed(&pool, &ffprobe, &clip, &root, &[888]).await;
    sqlx::query(r#"DELETE FROM "SourceTeletextMetadata""#)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(r#"DELETE FROM "Subtitle""#)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "Subtitle" ("MovieMetadataId","SubtitleKind","Title","StreamIndex","Codec","Default","Forced","SDH","Language","IsExtracted","Path","DateAdded","DateUpdated") VALUES (20,1,'Independent Dutch SRT',0,'subrip',1,0,0,'nld',1,?,'2026-10-18','2026-10-18')"#)
        .bind(root.join("independent-889.srt").to_str().unwrap()).execute(&pool).await.unwrap();
    sqlx::query(
        r#"UPDATE "Channel" SET "SubtitleMode"=3,"NextEngineTextSubtitleMode"=1 WHERE "Id"=1"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    let host = std::sync::Arc::new(host::NextHost::new(
        pool.clone(),
        locker.clone(),
        root.join("playout"),
    ));
    run_case(
        host,
        &pool,
        (&ffmpeg, &ffprobe),
        &root,
        &clip,
        Case {
            name: "ordinary-spanning",
            start_ms: 2000,
            end_ms: 9200,
            captions: true,
            callback_required: false,
            caption_track_required: true,
        },
    )
    .await;
    assert_eq!(locker.active_teletext_work(), 0);
}

#[tokio::test]
#[ignore = "requires pinned Next0.2.2, FFmpeg9, Python3 and fresh ERSATZRS_NEXT_TELETEXT_PROOF_DIR"]
async fn real_next_untransmitted_descriptor_keeps_programme_without_ready_captions() {
    let root = fixture::proof_root();
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    let clip = fixture::source_with_untransmitted_caption(&ffmpeg, &root).await;
    let profile = FFmpegProfile {
        video_format: FFmpegProfileVideoFormat::Copy,
        audio_format: FFmpegProfileAudioFormat::Copy,
        audio_sample_rate: 48,
        audio_channels: 1,
        ..text_profile()
    };
    let (pool, locker) = prepare_schedule(&clip, false, &profile).await;
    fixture::acquire_and_store_observed(&pool, &ffprobe, &clip, &root, &[888]).await;
    let refused = cache::prepare_complete_text(&pool, &locker, &ffmpeg, &ffprobe, &clip).await;
    assert_eq!(
        refused.unwrap_err(),
        crate::ffprobe::teletext_decode::SourceTeletextPageDecodeError::UnobservedPage
    );
    tokio::fs::write(root.join("untransmitted-preconditions.json"), br#"{"observed_pages":[888],"descriptor_pages":[888,889],"complete_source_refused_unobserved_page":889,"joined_work_zero":true,"scope":"No programme callback or AV delivery proven yet"}"#).await.unwrap();
    sqlx::query(
        r#"UPDATE "Channel" SET "SubtitleMode"=3,"NextEngineTextSubtitleMode"=1 WHERE "Id"=1"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    let host = std::sync::Arc::new(host::NextHost::new(
        pool.clone(),
        locker.clone(),
        root.join("playout"),
    ));
    run_case(
        host,
        &pool,
        (&ffmpeg, &ffprobe),
        &root,
        &clip,
        Case {
            name: "advertised-absent",
            start_ms: 2000,
            end_ms: 9200,
            captions: false,
            callback_required: true,
            caption_track_required: false,
        },
    )
    .await;
    assert_eq!(locker.active_teletext_work(), 0);
}

async fn run_selected_page(
    ffmpeg: &Path,
    ffprobe: &Path,
    root: &Path,
    clip: &Path,
    observed: &[u16],
) {
    let profile = FFmpegProfile {
        video_format: FFmpegProfileVideoFormat::Copy,
        audio_format: FFmpegProfileAudioFormat::Copy,
        audio_sample_rate: 48,
        audio_channels: 1,
        ..text_profile()
    };
    let (pool, locker) = prepare_schedule(clip, false, &profile).await;
    fixture::acquire_and_store_observed(&pool, ffprobe, clip, root, observed).await;
    cache::assert_complete_text_preparation(&pool, &locker, ffmpeg, ffprobe, clip).await;
    sqlx::query(
        r#"UPDATE "Channel" SET "SubtitleMode"=3,"NextEngineTextSubtitleMode"=1 WHERE "Id"=1"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    let mut paths = Vec::new();
    for case in [
        Case {
            name: "spanning",
            start_ms: 2000,
            end_ms: 9200,
            captions: true,
            callback_required: true,
            caption_track_required: true,
        },
        Case {
            name: "after-final",
            start_ms: 10000,
            end_ms: 17000,
            captions: false,
            callback_required: true,
            caption_track_required: true,
        },
    ] {
        let host = std::sync::Arc::new(host::NextHost::new(
            pool.clone(),
            locker.clone(),
            root.join("playout"),
        ));
        paths.push(run_case(host, &pool, (ffmpeg, ffprobe), root, clip, case).await);
        assert_eq!(locker.active_teletext_work(), 0);
    }
    assert_eq!(
        paths[0], paths[1],
        "full-source sidecar cache key must ignore occurrence seek"
    );
}

async fn run_case(
    host: std::sync::Arc<host::NextHost>,
    pool: &crate::db::DbPool,
    tools: (&Path, &Path),
    root: &Path,
    clip: &Path,
    case: Case,
) -> PathBuf {
    let proof = root.join(case.name);
    tokio::fs::create_dir(&proof).await.unwrap();
    let mut item = selection::tests::item();
    item.id = 1;
    item.playout_id = 1;
    item.start = chrono::Utc::now() - chrono::Duration::milliseconds(100);
    item.in_point = chrono::Duration::milliseconds(case.start_ms);
    item.out_point = chrono::Duration::milliseconds(case.end_ms);
    item.finish = item.start + chrono::Duration::milliseconds(case.end_ms - case.start_ms);
    crate::repositories::make_repositories(pool.clone())
        .playout
        .upsert_item(&item)
        .await
        .unwrap();
    let callbacks = std::sync::Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let mut callback = observed_callback_listener(host.clone(), callbacks.clone()).await;
    let registry = std::sync::Arc::new(crate::hls::InMemoryHlsSessionRegistry::new());
    let output = proof.join("output");
    let task = host
        .start(
            registry.clone(),
            host::NextStart {
                channel_id: 1,
                preview: false,
                number: "1".into(),
                executable: runtime_path("ERSATZRS_NEXT_TEST_BIN"),
                output: output.clone(),
                ffmpeg: tools.0.into(),
                ffprobe: tools.1.into(),
                replacements: Vec::new(),
                idle_timeout: None,
                deadline: Instant::now() + Duration::from_secs(30),
            },
        )
        .await
        .unwrap();
    let observed = capture(&registry, &output, &proof, case.captions).await;
    registry.stop_channel("1").await;
    task.await.unwrap();
    callback.stop().await;
    assert!(!output.exists(), "actual Next output cleanup must complete");
    let callbacks = callbacks.lock().await;
    tokio::fs::write(
        proof.join("callbacks.json"),
        serde_json::to_vec_pretty(&*callbacks).unwrap(),
    )
    .await
    .unwrap();
    let cues = observed.expect("bounded finite Next conversion must complete");
    let first = if case.callback_required {
        callbacks
            .iter()
            .find(|item| item.id == "1")
            .expect("actual authenticated item callback")
            .clone()
    } else {
        assert!(
            callbacks.is_empty(),
            "ready ordinary sidecar needs no deferred callback"
        );
        static_published_item(root).await
    };
    check_programme_output(tools, clip, &proof, &case, &first, &cues).await
}

async fn check_programme_output(
    tools: (&Path, &Path),
    clip: &Path,
    proof: &Path,
    case: &Case,
    first: &crate::next_engine::protocol::PlayoutItem,
    cues: &[String],
) -> PathBuf {
    let tracks = first.tracks.as_ref().unwrap();
    let video = tracks.video.as_ref().unwrap();
    assert_eq!(tracks.audio.as_ref().unwrap().stream_index, Some(3));
    let captured = proof.join("captured.ts");
    let mut decode = Command::new(tools.0);
    decode
        .args(["-v", "error", "-nostdin", "-xerror", "-i"])
        .arg(&captured)
        .args(["-f", "null", "-"]);
    tool(decode).await;
    if !case.caption_track_required {
        assert!(
            tracks.subtitle.is_none(),
            "no ready track for a proven absent caption page"
        );
        for document in cues {
            let mut lines = document.lines();
            assert_eq!(lines.next(), Some("WEBVTT"));
            lines
                .next()
                .unwrap()
                .strip_prefix("X-TIMESTAMP-MAP=LOCAL:00:00:00.000,MPEGTS:")
                .unwrap()
                .parse::<u64>()
                .unwrap();
            assert!(
                lines.all(|line| line.trim().is_empty()),
                "absent page permits only empty Convert segments, never manufactured cues"
            );
        }
        clock::copied_video(tools.0, clip, &captured).await;
        clock::copied_audio(tools.1, clip, &captured).await;
        return PathBuf::new();
    }
    let caption = tracks.subtitle.as_ref().unwrap();
    assert_eq!(caption.stream_index, Some(0));
    let (seek, end) = clock::timing(video);
    assert_eq!(clock::timing(caption), (seek, end));
    assert_eq!(end, Some(u64::try_from(case.end_ms).unwrap()));
    assert!(seek.unwrap() >= u64::try_from(case.start_ms).unwrap());
    let crate::next_engine::protocol::Source::Local { path, .. } = caption.source.as_ref().unwrap()
    else {
        panic!("complete owned local sidecar required")
    };
    let text = tokio::fs::read_to_string(path).await.unwrap();
    assert!(text.contains("NEDERLANDSE PAGINA 889 FINAL"));
    assert!(!text.contains("ENGLISH PAGE 888"));
    assert!(!cues.iter().any(|body| body.contains("ENGLISH PAGE 888")));
    clock::check(
        tools,
        clip,
        &captured,
        cues,
        &text,
        (seek.unwrap(), end.unwrap(), case.captions),
        proof,
    )
    .await;
    PathBuf::from(path)
}

async fn static_published_item(root: &Path) -> crate::next_engine::protocol::PlayoutItem {
    let mut directory = tokio::fs::read_dir(root.join("playout/1")).await.unwrap();
    let file = directory.next_entry().await.unwrap().unwrap();
    assert!(directory.next_entry().await.unwrap().is_none());
    let publication: serde_json::Value =
        serde_json::from_slice(&tokio::fs::read(file.path()).await.unwrap()).unwrap();
    let items = publication["items"].as_array().unwrap();
    let selected = items
        .iter()
        .filter(|item| item["id"] == "1")
        .collect::<Vec<_>>();
    assert_eq!(
        selected.len(),
        1,
        "actual ready publication contains one exact item"
    );
    serde_json::from_value(selected[0].clone()).unwrap()
}

async fn capture(
    registry: &crate::hls::InMemoryHlsSessionRegistry,
    output: &Path,
    proof: &Path,
    captioned: bool,
) -> Result<Vec<String>, &'static str> {
    timeout(Duration::from_secs(25), async {
        let handle = registry.try_get("1").await.unwrap();
        loop {
            if handle.startup_readiness() == HlsStartupReadiness::Failed {
                return Err("Next startup failed");
            }
            if let Some(playlist) = handle.trim_playlist().await {
                if let Some(segment) = playlist.lines().find(|line| {
                    Path::new(line)
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("ts"))
                }) {
                    let mut cues = Vec::new();
                    let mut entries = tokio::fs::read_dir(output).await.unwrap();
                    while let Some(entry) = entries.next_entry().await.unwrap() {
                        if entry
                            .path()
                            .extension()
                            .is_some_and(|extension| extension == "vtt")
                        {
                            let body = tokio::fs::read_to_string(entry.path()).await.unwrap();
                            tokio::fs::write(proof.join(entry.file_name()), &body)
                                .await
                                .unwrap();
                            cues.push(body);
                        }
                    }
                    if (captioned && cues.iter().any(|body| body.contains("889 FINAL")))
                        || (!captioned && handle.startup_readiness() == HlsStartupReadiness::Ready)
                    {
                        tokio::fs::copy(output.join(segment), proof.join("captured.ts"))
                            .await
                            .unwrap();
                        return Ok(cues);
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .map_err(|_| "Next finite text timed out")?
}
