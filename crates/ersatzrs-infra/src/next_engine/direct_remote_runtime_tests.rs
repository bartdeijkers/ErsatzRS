//! Unmanaged HTTP/RTSP sources through demand-time callbacks and official Next.
use super::*;
use ersatzrs_core::repositories::PlayoutRepository as _;
use std::sync::Arc;

#[path = "direct_remote_endings.rs"]
mod endings;
#[path = "direct_remote_images.rs"]
mod images;
use super::super::direct_remote_servers as servers;

#[derive(Clone, Copy, PartialEq, Eq)]
enum CaptionSource {
    Sidecar,
    EmbeddedText,
    EmbeddedImage,
}

#[derive(Clone, Copy)]
struct Case {
    live: bool,
    subtitles: Option<bool>,
    source: CaptionSource,
}

#[tokio::test]
#[ignore = "requires official Next, FFmpeg and pinned MediaMTX; isolated direct remote sources"]
async fn real_next_direct_http_and_rtsp_preserve_audio_and_source_clock() {
    prepared_artifact_runtime::run_isolated_test("next_engine::process::runtime_tests::text_subtitle_runtime_tests::direct_remote_runtime::fixture_child_direct_remote").await;
}

#[tokio::test]
#[ignore = "requires official Next, FFmpeg and pinned MediaMTX; direct remote sidecars"]
async fn real_next_direct_http_and_rtsp_preserve_external_captions() {
    prepared_artifact_runtime::run_isolated_test("next_engine::process::runtime_tests::text_subtitle_runtime_tests::direct_remote_runtime::fixture_child_direct_captions").await;
}

#[tokio::test]
#[ignore = "requires official Next and FFmpeg; finite HTTP embedded captions"]
async fn real_next_direct_finite_http_embedded_text_and_off() {
    prepared_artifact_runtime::run_isolated_test("next_engine::process::runtime_tests::text_subtitle_runtime_tests::direct_remote_runtime::fixture_child_direct_embedded_text").await;
}

#[tokio::test]
#[ignore = "child of real_next_direct_finite_http_embedded_text_and_off"]
async fn fixture_child_direct_embedded_text() {
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    ersatzrs_ffmpeg::set_resolved_ffmpeg_tools(ffmpeg.to_str().unwrap(), ffprobe.to_str().unwrap());
    let root = tempfile::tempdir().unwrap();
    let clip = generate_text_clip(&ffmpeg, root.path()).await;
    let http = servers::Http::start(clip.clone()).await;
    let mut results = Vec::new();
    for (index, subtitles) in [Some(false), Some(true), None].into_iter().enumerate() {
        results.push(
            observed_case(
                &clip,
                &http.uri,
                Case {
                    live: false,
                    subtitles,
                    source: CaptionSource::EmbeddedText,
                },
                root.path().join(format!("embedded-{index}")),
                &ffmpeg,
                &ffprobe,
            )
            .await,
        );
    }
    http.stop().await;
    for result in results {
        result.expect("finite HTTP embedded text and off");
    }
}

async fn configure_embedded(pool: &crate::db::DbPool, enabled: bool) {
    sqlx::raw_sql(r#"
        UPDATE "Subtitle" SET "SubtitleKind"=0,"StreamIndex"=3,"Path"=NULL,"IsExtracted"=0;
        DELETE FROM "MediaStream" WHERE "MediaStreamKind"=3;
        INSERT INTO "MediaStream" ("Index","Codec","MediaStreamKind","Language","Channels","Title","Default","Forced","MediaVersionId")
            VALUES (3,'subrip',3,'nld',0,'Dutch',1,0,100);
    "#).execute(pool).await.unwrap();
    if enabled {
        sqlx::query(r#"UPDATE "PlayoutItem" SET "MediaVersionId"=100,"VideoStreamIndex"=0,"AudioStreamIndex"=2,"SubtitleStreamIndex"=3"#)
            .execute(pool).await.unwrap();
    }
}

#[tokio::test]
#[ignore = "child of real_next_direct_http_and_rtsp_preserve_external_captions"]
async fn fixture_child_direct_captions() {
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    ersatzrs_ffmpeg::set_resolved_ffmpeg_tools(ffmpeg.to_str().unwrap(), ffprobe.to_str().unwrap());
    let root = tempfile::tempdir().unwrap();
    let clip = root.path().join("original.mkv");
    generate_clip(&ffmpeg, &clip).await;
    let rtsp = servers::Rtsp::start(&ffmpeg, &clip, root.path()).await;
    let http = servers::Http::start(clip.clone()).await;
    let mut results = Vec::new();
    for (index, uri, live) in [
        (0, &http.uri, false),
        (1, &http.uri, true),
        (2, &rtsp.uri, true),
    ] {
        for convert in [false, true] {
            results.push(
                observed_case(
                    &clip,
                    uri,
                    Case {
                        live,
                        subtitles: Some(convert),
                        source: CaptionSource::Sidecar,
                    },
                    root.path().join(format!("{index}-{convert}")),
                    &ffmpeg,
                    &ffprobe,
                )
                .await,
            );
        }
    }
    http.stop().await;
    rtsp.stop().await;
    for result in results {
        result.expect("direct remote caption rendering and timing");
    }
}

#[tokio::test]
#[ignore = "child of real_next_direct_http_and_rtsp_preserve_audio_and_source_clock"]
async fn fixture_child_direct_remote() {
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    ersatzrs_ffmpeg::set_resolved_ffmpeg_tools(ffmpeg.to_str().unwrap(), ffprobe.to_str().unwrap());
    let root = tempfile::tempdir().unwrap();
    let original = root.path().join("original.mkv");
    generate_clip(&ffmpeg, &original).await;
    let clock = root.path().join("clock.mkv");
    let mut command = Command::new(&ffmpeg);
    command.args(["-v","error","-nostdin","-i"]).arg(&original)
        .args(["-map","0","-vf","drawbox=color=red:t=fill:enable='lt(t,5)',drawbox=color=green:t=fill:enable='gte(t,12)'",
            "-c:v","libx264","-preset","ultrafast","-g","25","-threads","2","-c:a","copy"]).arg(&clock);
    tool(command).await;
    let http = servers::Http::start(clock.clone()).await;
    let finite = observed_case(
        &clock,
        &http.uri,
        Case {
            live: false,
            subtitles: None,
            source: CaptionSource::Sidecar,
        },
        root.path().join("finite"),
        &ffmpeg,
        &ffprobe,
    )
    .await;
    let live = observed_case(
        &clock,
        &http.uri,
        Case {
            live: true,
            subtitles: None,
            source: CaptionSource::Sidecar,
        },
        root.path().join("live"),
        &ffmpeg,
        &ffprobe,
    )
    .await;
    http.stop().await;
    finite.expect("finite HTTP callback playback");
    live.expect("live HTTP callback playback");
    let rtsp = servers::Rtsp::start(&ffmpeg, &original, root.path()).await;
    let result = observed_case(
        &original,
        &rtsp.uri,
        Case {
            live: true,
            subtitles: None,
            source: CaptionSource::Sidecar,
        },
        root.path().join("rtsp"),
        &ffmpeg,
        &ffprobe,
    )
    .await;
    rtsp.stop().await;
    assert!(
        result.is_ok(),
        "live RTSP callback playback: {result:?}; retained at {}",
        root.keep().display()
    );
}

async fn configure(pool: &crate::db::DbPool, uri: &str, live: bool) {
    sqlx::raw_sql(r#"
        INSERT INTO "RemoteStream" ("Id","Url","Liveness","IsLive") VALUES (10,'https://example.test/replaced',1,0);
        UPDATE "MediaVersion" SET "MovieId"=NULL,"RemoteStreamId"=10 WHERE "Id"=100;
        DELETE FROM "Subtitle";
        DELETE FROM "MovieMetadata" WHERE "MovieId"=10;
        DELETE FROM "Movie" WHERE "Id"=10;
        INSERT INTO "RemoteStreamMetadata" ("Id","RemoteStreamId","MetadataKind","Title","DateAdded","DateUpdated") VALUES (30,10,0,'Synthetic','2026-09-23','2026-09-23');
        UPDATE "Channel" SET "SubtitleMode"=0 WHERE "Id"=1;
    "#).execute(pool).await.unwrap();
    sqlx::query(r#"UPDATE "RemoteStream" SET "Url"=?,"Liveness"=?,"IsLive"=? WHERE "Id"=10"#)
        .bind(uri)
        .bind(if live { 2 } else { 1 })
        .bind(i32::from(live))
        .execute(pool)
        .await
        .unwrap();
    let mut item = selection::tests::item();
    item.id = 1;
    item.playout_id = 1;
    item.in_point = chrono::Duration::seconds(6);
    item.out_point = chrono::Duration::seconds(30);
    item.finish = item.start + chrono::Duration::seconds(24);
    crate::repositories::make_repositories(pool.clone())
        .playout
        .upsert_item(&item)
        .await
        .unwrap();
}

async fn run_case(
    clip: &Path,
    uri: &str,
    case: Case,
    root: PathBuf,
    ffmpeg: &Path,
    ffprobe: &Path,
) -> Result<(), String> {
    tokio::fs::create_dir_all(&root).await.unwrap();
    let (pool, locker) = prepare_schedule(clip, false, &text_profile()).await;
    configure(&pool, uri, case.live).await;
    if case.source == CaptionSource::EmbeddedImage {
        images::configure(&pool, case.subtitles).await;
    } else {
        remote_subtitle_runtime_helpers::configure_sidecar(&pool, &root, case.subtitles, false)
            .await;
    }
    if case.source == CaptionSource::EmbeddedText {
        configure_embedded(&pool, case.subtitles.is_some()).await;
    }
    let host = Arc::new(host::NextHost::new(pool, locker, root.join("playout")));
    let mut callback = callback_listener(host.clone()).await;
    let registry = Arc::new(crate::hls::InMemoryHlsSessionRegistry::new());
    let output = root.join("output");
    let task = host
        .start(
            registry.clone(),
            host::NextStart {
                channel_id: 1,
                preview: false,
                number: "1".into(),
                executable: runtime_path("ERSATZRS_NEXT_TEST_BIN"),
                output: output.clone(),
                ffmpeg: ffmpeg.into(),
                ffprobe: ffprobe.into(),
                replacements: Vec::new(),
                idle_timeout: None,
                deadline: Instant::now() + Duration::from_secs(30),
            },
        )
        .await;
    let Some(task) = task else {
        callback.stop().await;
        return Err("direct session admission".into());
    };
    let handle = registry.try_get("1").await.unwrap();
    let saved = root.join("proof.ts");
    let captured = capture(handle.as_ref(), &output, &saved).await;
    let vtt = if case.source != CaptionSource::EmbeddedImage
        && case.subtitles == Some(true)
        && captured.is_ok()
    {
        Some(remote_subtitle_runtime_helpers::capture_vtt(handle.as_ref(), &output).await)
    } else {
        None
    };
    let stopped = registry.stop_channel("1").await;
    task.await.unwrap();
    callback.stop().await;
    captured?;
    assert!(stopped);
    assert!(!output.exists());
    retained_nut_runtime_tests::assert_selected_audio(ffmpeg, &saved).await;
    if case.source == CaptionSource::EmbeddedImage {
        images::assert_output(ffmpeg, &saved, case.subtitles.is_none()).await;
    } else if let Some(convert) = case.subtitles {
        assert_text_video_and_audio(ffmpeg, &saved, !convert).await;
        if convert {
            let (body, duration) = vtt.expect("VTT capture attempted").unwrap();
            assert_text_vtt(&body, true, duration);
        }
    }
    if case.source == CaptionSource::EmbeddedText && case.subtitles.is_none() {
        assert_text_video_and_audio(ffmpeg, &saved, false).await;
    }
    assert_clock(ffmpeg, &saved, uri, case).await;
    Ok(())
}

async fn assert_clock(ffmpeg: &Path, saved: &Path, uri: &str, case: Case) {
    let mut command = Command::new(ffmpeg);
    command
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(saved)
        .args([
            "-ss",
            "0.5",
            "-map",
            "0:v:0",
            "-frames:v",
            "1",
            "-vf",
            "scale=1:1",
            "-pix_fmt",
            "rgb24",
            "-f",
            "rawvideo",
            "pipe:1",
        ]);
    let rgb = tool(command).await;
    assert_eq!(rgb.len(), 3);
    // The clock fixture uses red only for its live HTTP case; the independent
    // caption matrix uses the original blue clip for every transport.
    let red = case.source == CaptionSource::Sidecar
        && case.subtitles.is_none()
        && case.live
        && uri.starts_with("http:");
    assert!(
        if red {
            rgb[0] > 180 && rgb[1] < 70 && rgb[2] < 70
        } else {
            rgb[2] > 180 && rgb[0] < 70 && rgb[1] < 70
        },
        "direct source clock mismatch"
    );
}

async fn capture(
    handle: &dyn ersatzrs_core::hls::HlsSessionHandle,
    output: &Path,
    saved: &Path,
) -> Result<(), String> {
    timeout(Duration::from_secs(30), async {
        loop {
            match handle.startup_readiness() {
                HlsStartupReadiness::Ready => break,
                HlsStartupReadiness::Failed => return Err("direct session failed".into()),
                _ => tokio::time::sleep(Duration::from_millis(10)).await,
            }
        }
        let playlist = handle
            .trim_playlist()
            .await
            .ok_or("missing direct playlist")?;
        let segment = playlist
            .lines()
            .find(|line| {
                !line.starts_with('#')
                    && Path::new(line)
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("ts"))
            })
            .ok_or("missing direct segment")?;
        tokio::fs::copy(output.join(segment), saved)
            .await
            .map_err(|error| error.to_string())?;
        Ok(())
    })
    .await
    .map_err(|_| "direct readiness timed out".to_string())?
}

async fn observed_case(
    clip: &Path,
    uri: &str,
    case: Case,
    root: PathBuf,
    ffmpeg: &Path,
    ffprobe: &Path,
) -> Result<(), String> {
    let clip = clip.to_owned();
    let uri = uri.to_owned();
    let ffmpeg = ffmpeg.to_owned();
    let ffprobe = ffprobe.to_owned();
    tokio::spawn(async move { run_case(&clip, &uri, case, root, &ffmpeg, &ffprobe).await })
        .await
        .map_err(|error| error.to_string())?
}
