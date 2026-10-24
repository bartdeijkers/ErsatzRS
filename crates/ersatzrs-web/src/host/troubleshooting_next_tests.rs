use super::*;
use ersatzrs_core::repositories::{FfmpegProfileRepository as _, PlayoutRepository as _};
use ersatzrs_domain::{FFmpegProfile, PinnedStreamSelection};
use ersatzrs_infra::hls_worker::build_hls_command_with_opacity_clock;

#[tokio::test]
async fn next_preview_readiness_requires_generated_playlist_and_current_marker() {
    let root = tempfile::tempdir().unwrap();
    tokio::fs::write(root.path().join("ffmpeg.m3u8"), "#EXTM3U\n")
        .await
        .unwrap();
    tokio::fs::write(root.path().join("live000000.ts"), b"segment")
        .await
        .unwrap();
    assert!(!output_ready(root.path()).await.unwrap());
    tokio::fs::write(root.path().join(".ready"), b"")
        .await
        .unwrap();
    assert!(!output_ready(root.path()).await.unwrap());
    tokio::fs::write(root.path().join("live.m3u8"), "#EXTM3U\nlive000000.ts\n")
        .await
        .unwrap();
    assert!(output_ready(root.path()).await.unwrap());
    tokio::fs::remove_file(root.path().join(".ready"))
        .await
        .unwrap();
    assert!(!output_ready(root.path()).await.unwrap());
}

#[tokio::test]
async fn next_completed_preview_requires_success_endlist_and_a_referenced_segment() {
    let root = tempfile::tempdir().unwrap();
    let playlist = root.path().join("live.m3u8");
    tokio::fs::write(root.path().join("unrelated.ts"), b"other")
        .await
        .unwrap();
    for body in [
        "",
        "#EXTM3U\n",
        "#EXTM3U\n#EXT-X-ENDLIST\n",
        "#EXTM3U\nlive000000.ts\n",
    ] {
        tokio::fs::write(&playlist, body).await.unwrap();
        assert!(!completed_output_ready(root.path(), true).await.unwrap());
    }
    tokio::fs::write(
        &playlist,
        "#EXTM3U\n#EXTINF:1,\nlive000000.ts\n#EXT-X-ENDLIST\n",
    )
    .await
    .unwrap();
    assert!(!completed_output_ready(root.path(), true).await.unwrap());
    tokio::fs::write(root.path().join("live000000.ts"), b"segment")
        .await
        .unwrap();
    assert!(completed_output_ready(root.path(), true).await.unwrap());
    assert!(!completed_output_ready(root.path(), false).await.unwrap());
    assert!(!output_ready(root.path()).await.unwrap());
    for reference in [
        "../unrelated.ts",
        "folder/unrelated.ts",
        "folder\\unrelated.ts",
    ] {
        tokio::fs::write(&playlist, format!("#EXTM3U\n{reference}\n#EXT-X-ENDLIST\n"))
            .await
            .unwrap();
        assert!(!completed_output_ready(root.path(), true).await.unwrap());
    }
    tokio::fs::remove_file(&playlist).await.unwrap();
    tokio::fs::write(
        root.path().join("ffmpeg.m3u8"),
        "#EXTM3U\nlive000000.ts\n#EXT-X-ENDLIST\n",
    )
    .await
    .unwrap();
    assert!(!completed_output_ready(root.path(), true).await.unwrap());
}

struct Fixture {
    root: tempfile::TempDir,
    state: NativeAdminTroubleshootingRuntime,
    at: DateTime<Utc>,
}

impl Fixture {
    async fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let pool = fixture_pool().await;
        let clip = root.path().join("fixture.mkv");
        tokio::fs::write(&clip, b"synthetic identity")
            .await
            .unwrap();
        for (id, version) in [(1, 100), (2, 101)] {
            sqlx::query(r#"INSERT INTO "MediaFile" ("Id","MediaVersionId","Path","PathHash") VALUES (?,?,?,?)"#)
                .bind(id).bind(version).bind(clip.to_str().unwrap()).bind(format!("fixture-{id}"))
                .execute(&pool).await.unwrap();
        }
        let repos = Arc::new(ersatzrs_infra::repositories::make_repositories(
            pool.clone(),
        ));
        for id in [1, 2] {
            repos
                .ffmpeg_profile
                .upsert(&FFmpegProfile {
                    id,
                    resolution_id: 1,
                    audio_sample_rate: 48,
                    audio_format: ersatzrs_domain::FFmpegProfileAudioFormat::Aac,
                    video_format: ersatzrs_domain::FFmpegProfileVideoFormat::H264,
                    ..Default::default()
                })
                .await
                .unwrap();
        }
        repos
            .channel
            .upsert(&Channel {
                id: 1,
                number: "1".into(),
                name: "Fixture".into(),
                ffmpeg_profile_id: 1,
                preferred_audio_language_code: Some("nld".into()),
                ..Default::default()
            })
            .await
            .unwrap();
        sqlx::query(
            r#"INSERT INTO "Playout" ("Id","ChannelId","ScheduleKind","Seed") VALUES (1,1,0,1)"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        let at = DateTime::parse_from_rfc3339("2026-09-26T12:00:20Z")
            .unwrap()
            .with_timezone(&Utc);
        repos
            .playout
            .upsert_item(&PlayoutItem {
                id: 1,
                playout_id: 1,
                media_item_id: 10,
                start: at - TimeDelta::seconds(20),
                finish: at + TimeDelta::seconds(40),
                in_point: TimeDelta::seconds(10),
                out_point: TimeDelta::seconds(70),
                pinned_stream_selection: Some(PinnedStreamSelection {
                    media_version_id: 101,
                    video_stream_index: 0,
                    audio_stream_index: None,
                    subtitle_stream_index: None,
                }),
                ..Default::default()
            })
            .await
            .unwrap();
        let next = Arc::new(ersatzrs_infra::next_engine::host::NextHost::new(
            pool.clone(),
            ersatzrs_core::locking::EntityLocker::default(),
            root.path().join("next"),
        ));
        let state = NativeAdminTroubleshootingRuntime::new(
            pool,
            repos,
            Arc::new(RwLock::new(FfmpegRuntimeReport::for_tests())),
            Arc::default(),
            next,
        );
        Self { root, state, at }
    }

    fn query() -> PlaybackPreviewQuery {
        PlaybackPreviewRequest {
            streaming_engine: StreamingEngine::Next,
            media_item_id: Some(10),
            ffmpeg_profile_id: Some(2),
            seek_seconds: Some(15),
            subtitle_id: Some(0),
            preferred_audio_title: Some("Dutch".into()),
            ..Default::default()
        }
        .into()
    }

    fn paths(&self) -> RuntimePaths {
        RuntimePaths {
            executable: self.root.path().join("unused-next"),
            output: self.root.path().join("output"),
            overlays: None,
            fonts: self.root.path().join("fonts"),
        }
    }
}

#[tokio::test]
async fn media_preview_retains_custom_selector_and_defaults_to_subtitle_off_without_one() {
    let fixture = Fixture::new().await;
    let mut query = Fixture::query();
    query.subtitle_id = None;
    query.stream_selector = Some("fixture.yml".into());
    let selected = select(&fixture.state, &query).await.unwrap();
    assert_eq!(selected.channel.streaming_engine, StreamingEngine::Next);
    assert_eq!(
        selected.channel.stream_selector_mode,
        ersatzrs_domain::ChannelStreamSelectorMode::Custom
    );
    assert_eq!(
        selected.channel.stream_selector.as_deref(),
        Some("fixture.yml")
    );
    assert_eq!(selected.subtitle_id, None);
    query.stream_selector = None;
    assert_eq!(
        select(&fixture.state, &query).await.unwrap().subtitle_id,
        Some(0)
    );
    query.subtitle_id = Some(2);
    assert_eq!(
        select(&fixture.state, &query).await.unwrap().subtitle_id,
        Some(2)
    );
}

#[tokio::test]
async fn live_media_preview_ignores_seek_while_finite_media_retains_it() {
    let fixture = Fixture::new().await;
    for sql in [
        r#"INSERT INTO "RemoteStream" ("Id","IsLive","Liveness","Duration") VALUES (10,1,2,'00:01:00')"#,
        r#"INSERT INTO "RemoteStreamMetadata" ("Id","RemoteStreamId","MetadataKind","Title","DateAdded","DateUpdated") VALUES (30,10,0,'Synthetic','2026-09-30','2026-09-30')"#,
    ] {
        sqlx::query(sql).execute(&fixture.state.pool).await.unwrap();
    }
    let media = PreviewMediaItem {
        media_item_id: 10,
        media_version_id: 100,
        kind: "RemoteStream".into(),
        path: "synthetic.mkv".into(),
        duration_seconds: 60.0,
    };
    assert_eq!(
        preview_media_seek_seconds(&fixture.state, &media, Some(15))
            .await
            .unwrap(),
        0
    );
    sqlx::query(r#"UPDATE "RemoteStream" SET "IsLive"=0,"Liveness"=1 WHERE "Id"=10"#)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    assert_eq!(
        preview_media_seek_seconds(&fixture.state, &media, Some(15))
            .await
            .unwrap(),
        15
    );
}

#[test]
fn media_preview_seek_anchors_programme_time_instead_of_only_seeking_tracks() {
    let now = DateTime::from_timestamp(1_000, 0).unwrap();
    let mut item = PlayoutItem {
        start: now,
        ..PlayoutItem::default()
    };
    seek_media_item(&mut item, Some(15), 60.0).unwrap();
    assert_eq!(item.start, now - TimeDelta::seconds(15));
    assert_eq!(item.finish, now + TimeDelta::seconds(30));
    assert_eq!(item.in_point, TimeDelta::zero());
    assert_eq!(item.out_point, TimeDelta::seconds(45));
}

#[tokio::test]
async fn media_preview_preserves_selected_profile_audio_seek_and_whole_second_start() {
    let fixture = Fixture::new().await;
    let selected = select(&fixture.state, &Fixture::query()).await.unwrap();
    assert_eq!(selected.profile.profile.id, 2);
    assert_eq!(selected.media.media_version_id, 100);
    assert_eq!(selected.media.item.in_point, TimeDelta::zero());
    assert_eq!(
        selected.playback_start - selected.media.item.start,
        TimeDelta::seconds(15)
    );
    assert_eq!(selected.media.item.out_point, TimeDelta::seconds(45));
    assert_eq!(selected.media.item.start.timestamp_subsec_nanos(), 0);
    assert_eq!(
        selected.media.item.preferred_audio_title.as_deref(),
        Some("Dutch")
    );
    assert!(selected.selected_at.is_none());
}

#[tokio::test]
async fn channel_preview_rebases_pinned_version_and_keeps_persisted_schedule() {
    let fixture = Fixture::new().await;
    let mut query = Fixture::query();
    query.channel = Some(1);
    query.seek_seconds = None;
    query.start = Some(fixture.at.to_rfc3339());
    let before: Vec<(i64, String, String)> =
        sqlx::query_as(r#"SELECT "Id","Start","Finish" FROM "PlayoutItem""#)
            .fetch_all(&fixture.state.pool)
            .await
            .unwrap();
    let selected = select(&fixture.state, &query).await.unwrap();
    assert_eq!(selected.media.media_version_id, 101);
    assert_eq!(selected.media.media_file_id, 2);
    assert!((selected.info.duration_seconds - 90.0).abs() < f64::EPSILON);
    assert_eq!(selected.media.item.in_point, TimeDelta::seconds(30));
    assert_eq!(selected.media.item.out_point, TimeDelta::seconds(60));
    assert_eq!(selected.profile.profile.id, 2);
    assert!(selected
        .media
        .item
        .pinned_stream_selection
        .unwrap()
        .audio_stream_index
        .is_none());
    let after: Vec<(i64, String, String)> =
        sqlx::query_as(r#"SELECT "Id","Start","Finish" FROM "PlayoutItem""#)
            .fetch_all(&fixture.state.pool)
            .await
            .unwrap();
    assert_eq!(before, after);
}

#[tokio::test]
async fn preparation_writes_one_isolated_item_with_matching_virtual_start() {
    let fixture = Fixture::new().await;
    let paths = fixture.paths();
    let (manifest, config, number, _) = prepare(
        &fixture.state,
        &Fixture::query(),
        &paths,
        Instant::now() + Duration::from_secs(30),
        &mut None,
    )
    .await
    .unwrap();
    let mut files = tokio::fs::read_dir(manifest.path()).await.unwrap();
    let entry = files.next_entry().await.unwrap().unwrap();
    assert!(files.next_entry().await.unwrap().is_none());
    let playout: protocol::Playout =
        serde_json::from_slice(&tokio::fs::read(entry.path()).await.unwrap()).unwrap();
    assert_eq!(playout.items.len(), 1);
    assert_eq!(
        config.playout.virtual_start,
        Some(playout.items[0].start + TimeDelta::seconds(15))
    );
    assert_eq!(number, ".troubleshooting");
    assert_eq!(
        config.ffmpeg.reports_folder.as_deref(),
        paths.output.to_str()
    );
    let count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "PlayoutItem""#)
        .fetch_one(&fixture.state.pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    assert!(paths.output.join("troubleshooting_info.json").exists());
}

#[tokio::test]
#[ignore = "requires official Next and FFmpeg 9 runtime paths"]
async fn real_next_host_preview_finishes_with_selected_tracks_and_downloadable_reports() {
    run_real_preview(remote_runtime::Mode::Local).await;
}

#[tokio::test]
#[ignore = "requires official Next and FFmpeg 9 runtime paths"]
async fn real_next_custom_preview_selects_dutch_audio_and_subtitle_over_english_default() {
    run_real_preview_selected(remote_runtime::Mode::Local, true).await;
}

#[tokio::test]
#[ignore = "requires official Next and FFmpeg; isolated routed HTTP preview"]
async fn real_next_remote_preview_preserves_tracks_and_revokes_callbacks() {
    run_real_preview(remote_runtime::Mode::Direct).await;
}

#[tokio::test]
#[ignore = "requires official Next and FFmpeg; isolated prepared artifact preview"]
async fn real_next_prepared_preview_preserves_tracks_and_revokes_callbacks() {
    remote_runtime::isolated("fixture_child_prepared_preview").await;
}

#[tokio::test]
#[ignore = "child of real_next_prepared_preview_preserves_tracks_and_revokes_callbacks"]
async fn fixture_child_prepared_preview() {
    run_real_preview(remote_runtime::Mode::Prepared).await;
}

// Reuses the existing installed POSIX producer fixture; other preview cases are portable.
#[cfg(unix)]
#[tokio::test]
#[ignore = "requires official Next and FFmpeg; installed producer preview"]
async fn real_next_producer_preview_preserves_tracks_and_revokes_callbacks() {
    remote_runtime::isolated("fixture_child_producer_preview").await;
}

#[cfg(unix)]
#[tokio::test]
#[ignore = "child of real_next_producer_preview_preserves_tracks_and_revokes_callbacks"]
async fn fixture_child_producer_preview() {
    run_real_preview(remote_runtime::Mode::Producer).await;
}

async fn run_real_preview(mode: remote_runtime::Mode) {
    run_real_preview_selected(mode, false).await;
}

async fn run_real_preview_selected(mode: remote_runtime::Mode, custom: bool) {
    run_real_preview_configured(mode, custom, CanvasMode::None).await;
}

#[tokio::test]
#[ignore = "requires official Next and FFmpeg; decoded private HTTP canvas preview"]
async fn real_next_canvas_preview_composites_alpha_preserves_tracks_and_revokes_callback() {
    run_real_preview_configured(remote_runtime::Mode::Local, false, CanvasMode::FullFrame).await;
}

#[tokio::test]
#[ignore = "requires official Next023 and FFmpeg; decoded signed regional canvas"]
async fn real_next_regional_canvas_preserves_placement_tracks_and_callback_revocation() {
    run_real_preview_configured(remote_runtime::Mode::Local, false, CanvasMode::Region).await;
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CanvasMode {
    None,
    FullFrame,
    Region,
}

impl CanvasMode {
    async fn install(self, fixture: &Fixture, query: &mut PlaybackPreviewQuery) {
        match self {
            CanvasMode::None => {}
            CanvasMode::FullFrame => canvas_runtime::install(fixture, query).await,
            CanvasMode::Region => canvas_runtime::install_region(fixture, query).await,
        }
    }

    async fn assert_frames(self, ffmpeg: &FsPath, playlist: &FsPath) {
        match self {
            CanvasMode::None => {}
            CanvasMode::FullFrame => canvas_runtime::assert_composite(ffmpeg, playlist).await,
            CanvasMode::Region => canvas_runtime::assert_region(ffmpeg, playlist).await,
        }
    }
}

async fn run_real_preview_configured(mode: remote_runtime::Mode, custom: bool, canvas: CanvasMode) {
    let fixture = Fixture::new().await;
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    ersatzrs_ffmpeg::set_resolved_ffmpeg_tools(ffmpeg.to_str().unwrap(), ffprobe.to_str().unwrap());
    let mut paths = fixture.paths();
    paths.executable = runtime_path("ERSATZRS_NEXT_TEST_BIN");
    let clip = fixture.root.path().join("fixture.mkv");
    generate_clip(&ffmpeg, &clip, canvas != CanvasMode::None).await;
    let subtitle = fixture.root.path().join("fixture.nl.srt");
    tokio::fs::write(
        &subtitle,
        "1\n00:00:01,000 --> 00:00:06,000\nNext host subtitle proof\n",
    )
    .await
    .unwrap();
    sqlx::query(r#"UPDATE "MediaVersion" SET "Duration"='00:00:06' WHERE "Id"=100"#)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "Subtitle" ("Id","MovieMetadataId","SubtitleKind","Title","StreamIndex","Codec","Default","Forced","SDH","Language","IsExtracted","Path","DateAdded","DateUpdated") VALUES (2,20,1,'Dutch',9,'subrip',1,0,0,'nld',0,?,'2026-09-26 00:00:00','2026-09-26 00:00:00')"#)
        .bind(subtitle.to_str().unwrap()).execute(&fixture.state.pool).await.unwrap();
    {
        let mut runtime = fixture.state.ffmpeg_runtime.write().await;
        runtime.ffmpeg.path = ffmpeg.to_str().unwrap().to_owned();
        runtime.ffprobe.path = ffprobe.to_str().unwrap().to_owned();
    }
    // Custom selection must beat the English default without a title override.
    sqlx::query(r#"INSERT INTO "ConfigElement" ("Key","Value") VALUES (?,?)"#)
        .bind(ersatzrs_domain::CONFIG_KEY_FFMPEG_PREFERRED_LANGUAGE_CODE)
        .bind(if custom { "eng" } else { "nld" })
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    let mut query = Fixture::query();
    query.seek_seconds = Some(1);
    query.subtitle_id = Some(2);
    let _selector = custom.then(|| configure_runtime_selector(&mut query));
    canvas.install(&fixture, &mut query).await;
    let origin = if mode == remote_runtime::Mode::Local && canvas == CanvasMode::None {
        None
    } else {
        Some(remote_runtime::Origin::start(&fixture, mode).await)
    };
    let mut ownership = Arc::clone(&fixture.state.preview_lock)
        .try_lock_owned()
        .unwrap();
    let (sender, receiver) = oneshot::channel();
    let mut ready = Some(sender);
    if let Err(error) =
        run_with_paths(&fixture.state, &query, &mut ownership, &mut ready, &paths).await
    {
        let root = fixture.root.keep();
        panic!(
            "synthetic preview failed: {error}; reports retained at {}",
            root.display()
        );
    }
    receiver.await.unwrap().unwrap();
    let playlist = paths.output.join("live.m3u8");
    assert!(tokio::fs::read_to_string(&playlist)
        .await
        .unwrap()
        .contains("#EXT-X-ENDLIST"));
    if mode == remote_runtime::Mode::Local && !custom && canvas == CanvasMode::None {
        // The real five-second sought output finishes below Next's four-
        // segment threshold. Reaped final output must still become playable.
        assert!(!paths.output.join(".ready").exists());
        assert!(completed_output_ready(&paths.output, true).await.unwrap());
    }
    assert!(build_troubleshooting_archive_from(&paths.output)
        .await
        .unwrap()
        .is_some());
    assert_decoded_tracks(&ffmpeg, &playlist).await;
    canvas.assert_frames(&ffmpeg, &playlist).await;
    let log = tokio::fs::read_to_string(paths.output.join("logs.txt"))
        .await
        .unwrap();
    assert!(log.contains("[selection]"));
    if custom {
        assert!(log.contains("Custom selector rule 1: audio matched=true, subtitle matched=true"));
        assert!(log.contains("audio stream=Some(2), subtitle id=Some(2)"));
    }
    assert!(progress::completed_speed(&paths.output, &log)
        .await
        .is_some_and(|speed| speed > 0.0));
    if let Some(origin) = origin {
        if canvas == CanvasMode::Region {
            origin.assert_canvas_region().await;
        }
        origin.verify_revoked_and_stop().await;
    }
    remote_runtime::verify_producer(mode, fixture.root.path()).await;
    let count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "PlayoutItem""#)
        .fetch_one(&fixture.state.pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

fn configure_runtime_selector(query: &mut PlaybackPreviewQuery) -> tempfile::NamedTempFile {
    use std::io::Write as _;
    let folder = channel_stream_selectors_folder();
    std::fs::create_dir_all(&folder).unwrap();
    let mut file = tempfile::Builder::new()
        .prefix("ucu94-runtime-")
        .suffix(".yml")
        .tempfile_in(folder)
        .unwrap();
    file.write_all(b"items:\n  - audio_language: [nld]\n    subtitle_language: [nld]\n    subtitle_condition: forced\n").unwrap();
    file.flush().unwrap();
    query.stream_selector = Some(
        file.path()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned(),
    );
    query.preferred_audio_title = None;
    file
}

#[path = "troubleshooting_remote_runtime_tests.rs"]
mod remote_runtime;

#[path = "troubleshooting_canvas_runtime_tests.rs"]
mod canvas_runtime;

#[path = "troubleshooting_image_graphics_runtime_tests.rs"]
mod image_graphics_runtime;

// Process reaping is observed through a disposable POSIX wrapper.
#[cfg(unix)]
#[path = "troubleshooting_canvas_http_tests.rs"]
mod canvas_http;

async fn generate_clip(ffmpeg: &FsPath, clip: &FsPath, canvas: bool) {
    let mut generate = Command::new(ffmpeg);
    generate
        .args([
            "-v",
            "error",
            "-nostdin",
            "-y",
            "-f",
            "lavfi",
            "-i",
            if canvas {
                "color=blue:s=320x180:r=25"
            } else {
                "color=black:s=320x180:r=25"
            },
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=880:sample_rate=48000",
            "-t",
            "6",
            "-map",
            "0:v",
            "-map",
            "1:a",
            "-map",
            "2:a",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-c:a",
            "aac",
            "-metadata:s:a:0",
            "language=eng",
            "-metadata:s:a:0",
            "title=English",
            "-metadata:s:a:1",
            "language=nld",
            "-metadata:s:a:1",
            "title=Dutch",
        ])
        .arg(clip);
    output(generate).await;
}

async fn output(mut command: Command) -> Vec<u8> {
    command.kill_on_drop(true);
    let result = tokio::time::timeout(Duration::from_mins(1), command.output())
        .await
        .unwrap()
        .unwrap();
    assert!(
        result.status.success(),
        "synthetic media tool failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    result.stdout
}

async fn assert_decoded_audio(ffmpeg: &FsPath, playlist: &FsPath) {
    let mut audio = Command::new(ffmpeg);
    audio
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(playlist)
        .args([
            "-map", "0:a:0", "-t", "1", "-ac", "1", "-ar", "8000", "-f", "s16le", "pipe:1",
        ]);
    let pcm = output(audio).await;
    let samples: Vec<i16> = pcm
        .chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]))
        .collect();
    assert!(samples.len() >= 7000);
    let crossings = samples
        .windows(2)
        .filter(|pair| pair[0] <= 0 && pair[1] > 0)
        .count();
    let frequency = f64::from(u32::try_from(crossings).unwrap()) * 8000.0
        / f64::from(u32::try_from(samples.len()).unwrap());
    assert!(
        (820.0..940.0).contains(&frequency),
        "expected Dutch 880 Hz track, found {frequency}"
    );
}

async fn assert_decoded_tracks(ffmpeg: &FsPath, playlist: &FsPath) {
    assert_decoded_audio(ffmpeg, playlist).await;
    let mut video = Command::new(ffmpeg);
    video
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(playlist)
        .args([
            "-map",
            "0:v:0",
            "-frames:v",
            "1",
            "-pix_fmt",
            "rgb24",
            "-f",
            "rawvideo",
            "pipe:1",
        ]);
    let frame = output(video).await;
    assert_eq!(frame.len(), 320 * 180 * 3);
    assert!(
        frame[320 * 120 * 3..]
            .chunks_exact(3)
            .filter(|rgb| rgb.iter().all(|component| *component > 160))
            .count()
            > 30,
        "the sought first frame must contain the subtitle cue"
    );
}

#[tokio::test]
async fn media_preview_uses_the_bound_versions_duration_when_an_earlier_file_is_unavailable() {
    let fixture = Fixture::new().await;
    sqlx::query(r#"UPDATE "MediaFile" SET "Path"=NULL WHERE "Id"=1"#)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    let mut query = Fixture::query();
    query.seek_seconds = Some(70);
    let selected = select(&fixture.state, &query).await.unwrap();
    assert_eq!(selected.media.media_version_id, 101);
    assert_eq!(selected.media.item.in_point, TimeDelta::zero());
    assert_eq!(
        selected.playback_start - selected.media.item.start,
        TimeDelta::seconds(70)
    );
    assert_eq!(selected.media.item.out_point, TimeDelta::seconds(90));
}

async fn fixture_pool() -> DbPool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    for sql in [
        r#"INSERT INTO "MediaSource" ("Id") VALUES (1)"#,
        r#"INSERT INTO "Library" ("Id","Name","MediaKind","MediaSourceId") VALUES (1,'Synthetic',1,1)"#,
        r#"INSERT INTO "LocalLibrary" ("Id") VALUES (1)"#,
        r#"INSERT INTO "LibraryPath" ("Id","LibraryId","Path") VALUES (1,1,'media')"#,
        r#"INSERT INTO "MediaItem" ("Id","LibraryPathId","State") VALUES (10,1,0)"#,
        r#"INSERT INTO "Movie" ("Id") VALUES (10)"#,
        r#"INSERT INTO "MovieMetadata" ("Id","MovieId","Title","MetadataKind","DateAdded","DateUpdated") VALUES (20,10,'Synthetic',0,'2026-09-26 00:00:00','2026-09-26 00:00:00')"#,
        r#"INSERT INTO "MediaVersion" ("Id","MovieId","Duration","Width","Height","VideoScanKind","DateAdded","DateUpdated") VALUES (100,10,'00:01:00',320,180,0,'2026-09-26 00:00:00','2026-09-26 00:00:00'),(101,10,'00:01:30',320,180,0,'2026-09-26 00:00:00','2026-09-26 00:00:00')"#,
        r#"INSERT INTO "MediaStream" ("Id","Index","Codec","MediaStreamKind","Language","Channels","Title","Default","Forced","MediaVersionId") VALUES (1,0,'h264',1,NULL,0,NULL,1,0,100),(2,1,'aac',2,'eng',1,'English',1,0,100),(3,2,'aac',2,'nld',1,'Dutch',0,0,100),(4,0,'h264',1,NULL,0,NULL,1,0,101)"#,
        r#"INSERT INTO "Resolution" ("Id","Name","Width","Height") VALUES (1,'Fixture',320,180)"#,
    ] {
        sqlx::query(sql).execute(&pool).await.unwrap();
    }
    pool
}

#[allow(clippy::disallowed_methods)] // Explicit opt-in test runtime paths, never production configuration.
fn runtime_path(name: &str) -> PathBuf {
    std::env::var_os(name)
        .map(PathBuf::from)
        .expect("set the documented Next runtime test variables")
}

#[tokio::test]
async fn channel_manifest_keeps_pinned_silence_and_configured_track_policy() {
    let fixture = Fixture::new().await;
    let mut query = Fixture::query();
    query.channel = Some(1);
    query.start = Some(fixture.at.to_rfc3339());
    let paths = fixture.paths();
    let (manifest, _, _, _) = prepare(
        &fixture.state,
        &query,
        &paths,
        Instant::now() + Duration::from_secs(30),
        &mut None,
    )
    .await
    .unwrap();
    let mut files = tokio::fs::read_dir(manifest.path()).await.unwrap();
    let path = files.next_entry().await.unwrap().unwrap().path();
    let playout: serde_json::Value =
        serde_json::from_slice(&tokio::fs::read(path).await.unwrap()).unwrap();
    assert_eq!(
        playout["items"][0]["tracks"]["audio"]["source"]["source_type"],
        "lavfi"
    );
    assert!(playout["items"][0]["tracks"].get("subtitle").is_none());
}

#[tokio::test]
async fn media_preview_rejects_overflowing_seek_for_unknown_duration() {
    let fixture = Fixture::new().await;
    sqlx::query(r#"UPDATE "MediaVersion" SET "Duration"='00:00:00' WHERE "Id"=100"#)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    for seek in [i64::MAX, TimeDelta::MAX.num_seconds()] {
        let mut query = Fixture::query();
        query.seek_seconds = Some(seek);
        assert!(select(&fixture.state, &query).await.is_err());
    }
}

#[tokio::test]
async fn music_video_credits_preview_preserves_audio_seek_archive_and_persisted_state() {
    let fixture = Fixture::new().await;
    for sql in [
        r#"INSERT INTO "MediaItem" ("Id","LibraryPathId","State") VALUES (31,1,0)"#,
        r#"INSERT INTO "Artist" ("Id") VALUES (31)"#,
        r#"INSERT INTO "ArtistMetadata" ("Id","ArtistId","Title","MetadataKind","DateAdded","DateUpdated") VALUES (32,31,'Synthetic Artist',0,'2026-09-30','2026-09-30')"#,
        r#"INSERT INTO "MusicVideo" ("Id","ArtistId") VALUES (10,31)"#,
        r#"UPDATE "MediaVersion" SET "MovieId"=NULL,"MusicVideoId"=10 WHERE "Id" IN (100,101)"#,
        r#"DELETE FROM "MovieMetadata" WHERE "MovieId"=10"#,
        r#"DELETE FROM "Movie" WHERE "Id"=10"#,
        r#"INSERT INTO "MusicVideoMetadata" ("Id","MusicVideoId","Title","Album","MetadataKind","DateAdded","DateUpdated") VALUES (33,10,'Synthetic Credit Song','Synthetic Album',0,'2026-09-30','2026-09-30')"#,
    ] {
        sqlx::query(sql).execute(&fixture.state.pool).await.unwrap();
    }
    sqlx::query(r#"INSERT INTO "ConfigElement" ("Key","Value") VALUES (?,'nld')"#)
        .bind(ersatzrs_domain::CONFIG_KEY_FFMPEG_PREFERRED_LANGUAGE_CODE)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    assert_music_video_preview_summary(&fixture).await;
    let mut query = Fixture::query();
    query.music_video_credits_template = Some("_ArtistTitleAlbum_CenterTop".into());
    query.seek_seconds = Some(35);
    query.subtitle_id = Some(999); // Generated credits take precedence over a persisted track.
    let selected = select(&fixture.state, &query).await.unwrap();
    assert_eq!(selected.subtitle_id, Some(0));
    assert_eq!(selected.media.item.in_point, TimeDelta::zero());
    assert_eq!(
        selected.playback_start - selected.media.item.start,
        TimeDelta::seconds(35)
    );
    let paths = fixture.paths();
    let mut session = None;
    let (manifest, _, _, _) = prepare(
        &fixture.state,
        &query,
        &paths,
        Instant::now() + Duration::from_secs(30),
        &mut session,
    )
    .await
    .unwrap();
    let entry = tokio::fs::read_dir(manifest.path())
        .await
        .unwrap()
        .next_entry()
        .await
        .unwrap()
        .unwrap();
    let json: serde_json::Value =
        serde_json::from_slice(&tokio::fs::read(entry.path()).await.unwrap()).unwrap();
    assert_eq!(json["items"][0]["tracks"]["audio"]["stream_index"], 2);
    assert_eq!(
        json["items"][0]["tracks"]["video"]["source"]["in_point_ms"],
        0
    );
    assert_eq!(
        json["items"][0]["tracks"]["subtitle"]["source"]["in_point_ms"],
        0
    );
    let ass = tokio::fs::read_to_string(paths.output.join("music-video-credits-playback.ass"))
        .await
        .unwrap();
    assert!(ass.contains("Synthetic Credit Song"));
    assert!(ass.contains("0:00:44.00")); // Next applies the 35s programme offset to the original cues.
    assert!(
        tokio::fs::read_to_string(paths.output.join("music-video-credits.ass"))
            .await
            .unwrap()
            .contains("0:00:44.00")
    );
    assert!(
        tokio::fs::read_to_string(paths.output.join("music-video-credits-template.sbntxt"))
            .await
            .unwrap()
            .contains("_first_start_ts")
    );
    assert_native_credits_snapshot(&fixture, &selected, &paths).await;
    let count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "PlayoutItem""#)
        .fetch_one(&fixture.state.pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    let channel = fixture
        .state
        .repositories
        .channel
        .get_with_profile(1)
        .await
        .unwrap();
    assert_eq!(
        channel.music_video_credits_mode,
        ersatzrs_domain::ChannelMusicVideoCreditsMode::None
    );
    fixture
        .state
        .next_engine
        .finish_troubleshooting_session(session.unwrap())
        .await
        .unwrap();
}

async fn assert_music_video_preview_summary(fixture: &Fixture) {
    let context = fixture
        .state
        .repositories
        .troubleshooting
        .load_playback_context_admin(10, 0)
        .await
        .unwrap();
    let summary = context.media.unwrap();
    assert_eq!(summary.kind, "MusicVideo");
    assert_eq!(summary.title, "Synthetic Credit Song");
    assert_eq!(summary.duration, "00:01:00");
}

async fn assert_native_credits_snapshot(
    fixture: &Fixture,
    selected: &Selection,
    paths: &RuntimePaths,
) {
    let subtitle = credits::prepare(
        &fixture.state.pool,
        &selected.info,
        &selected.channel,
        &selected.profile,
        Duration::from_secs(35),
        &paths.output,
    )
    .await
    .unwrap()
    .unwrap();
    let input = subtitle_input_for_stream(
        &subtitle,
        "fixture.mkv",
        OutputFormat::Hls,
        StreamingMode::HttpLiveStreamingSegmenter,
    );
    let command = build_hls_command_with_opacity_clock(
        "fixture.mkv",
        &paths.output,
        OutputFormat::Hls,
        true,
        true,
        false,
        Duration::from_secs(35),
        Some(Duration::from_secs(25)),
        None,
        &selected.profile,
        None,
        ersatzrs_ffmpeg::OutputMetadata::default(),
        ersatzrs_ffmpeg::FfmpegCapabilityReport::default(),
        None,
        vec![],
        Some(2usize),
        input,
        vec![],
        vec![],
        None,
    )
    .unwrap();
    let fonts = ersatzrs_infra::file_system_layout::fonts_cache_folder();
    // Redact using the production filter escaping so Windows drive letters,
    // separators and temporary profile paths cannot leak into the snapshot.
    let actual_filter = ersatzrs_ffmpeg::filter::subtitles_filter(
        subtitle.path.as_deref().unwrap(),
        Some(fonts.to_str().unwrap()),
    );
    let stable_filter = ersatzrs_ffmpeg::filter::subtitles_filter(
        "fixture/output/music-video-credits-playback.ass",
        Some("fonts"),
    );
    let args: Vec<_> = command
        .arguments
        .iter()
        .map(|arg| arg.replace(&actual_filter, &stable_filter))
        .collect();
    insta::assert_snapshot!("music_video_credits_native_preview", args.join("\n"));
}

#[tokio::test]
async fn native_channel_preview_retains_music_video_credits_configuration() {
    let fixture = Fixture::new().await;
    let mut channel = fixture
        .state
        .repositories
        .channel
        .get_with_profile(1)
        .await
        .unwrap();
    channel.music_video_credits_mode =
        ersatzrs_domain::ChannelMusicVideoCreditsMode::GenerateSubtitles;
    channel.music_video_credits_template = Some("_MusicFactory_ArtistTitle".into());
    fixture
        .state
        .repositories
        .channel
        .upsert(&channel)
        .await
        .unwrap();
    let preview = fixture
        .state
        .repositories
        .troubleshooting
        .load_preview_channel_admin(1)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        preview.music_video_credits_mode,
        channel.music_video_credits_mode
    );
    assert_eq!(
        preview.music_video_credits_template,
        channel.music_video_credits_template
    );
    sqlx::query(r#"UPDATE "Channel" SET "MusicVideoCreditsMode"=99 WHERE "Id"=1"#)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    assert!(fixture
        .state
        .repositories
        .troubleshooting
        .load_preview_channel_admin(1)
        .await
        .is_err());
}

#[tokio::test]
async fn channel_programme_seek_preserves_original_inpoint_and_full_preview_window() {
    let fixture = Fixture::new().await;
    let mut query = Fixture::query();
    query.channel = Some(1);
    query.start = Some(fixture.at.to_rfc3339());
    for (seek, elapsed) in [(0, 0), (15, 15), (60, 59), (i64::MAX, 59)] {
        query.seek_seconds = Some(seek);
        let selected = select_channel_window(&fixture.state, &query).await.unwrap();
        assert_eq!(
            selected.start,
            fixture.at - TimeDelta::seconds(20) + TimeDelta::seconds(elapsed)
        );
        assert_eq!(selected.finish - selected.start, TimeDelta::seconds(30));
        assert_eq!(selected.media[0].item.media_item_id, 10);
        // Historical programme time drives Next's seek; source in-point is not
        // advanced again and the window can continue after this occurrence.
        assert_eq!(selected.media[0].item.in_point, TimeDelta::seconds(10));
        assert!(selected.media[0].item.out_point <= TimeDelta::seconds(70));
    }
}

async fn channel_window_second_movie(fixture: &Fixture) {
    for sql in [
        r#"INSERT INTO "MediaItem" ("Id","LibraryPathId","State") VALUES (11,1,0)"#,
        r#"INSERT INTO "Movie" ("Id") VALUES (11)"#,
        r#"INSERT INTO "MovieMetadata" ("Id","MovieId","Title","MetadataKind","DateAdded","DateUpdated") VALUES (30,11,'Second synthetic movie',0,'2026-09-26','2026-09-26')"#,
        r#"INSERT INTO "MediaVersion" ("Id","DateAdded","DateUpdated","Duration","MovieId","Height","Width","VideoScanKind","RFrameRate") VALUES (200,'2026-09-26','2026-09-26','00:01:00',11,180,320,0,'25')"#,
        r#"INSERT INTO "MediaStream" ("Id","Index","Codec","MediaStreamKind","Language","Channels","Title","Default","Forced","MediaVersionId") VALUES (20,0,'h264',1,NULL,0,NULL,1,0,200),(21,1,'aac',2,'eng',2,'English',1,0,200),(22,2,'aac',2,'nld',2,'Dutch',0,0,200)"#,
        r#"INSERT INTO "Subtitle" ("Id","MovieMetadataId","SubtitleKind","Title","StreamIndex","Codec","Default","Forced","SDH","Language","IsExtracted","Path","DateAdded","DateUpdated") VALUES (20,30,1,'Second Dutch',9,'subrip',1,0,0,'nld',0,'fixture-second.srt','2026-09-26','2026-09-26')"#,
        r#"UPDATE "MediaVersion" SET "RFrameRate"='30' WHERE "Id" IN (100,101)"#,
    ] {
        sqlx::query(sql).execute(&fixture.state.pool).await.unwrap();
    }
    sqlx::query(r#"INSERT INTO "MediaFile" ("Id","MediaVersionId","Path","PathHash") VALUES (3,200,?,'second-window')"#)
        .bind(fixture.root.path().join("fixture.mkv").to_str().unwrap()).execute(&fixture.state.pool).await.unwrap();
    let repos = &fixture.state.repositories;
    let mut first = repos
        .playout
        .items_in_range(
            1,
            fixture.at - TimeDelta::seconds(30),
            fixture.at + TimeDelta::seconds(60),
        )
        .await
        .unwrap()
        .remove(0);
    first.finish = fixture.at + TimeDelta::seconds(5);
    first.out_point = TimeDelta::seconds(35);
    repos.playout.upsert_item(&first).await.unwrap();
    repos
        .playout
        .upsert_item(&PlayoutItem {
            id: 2,
            playout_id: 1,
            media_item_id: 11,
            start: fixture.at + TimeDelta::seconds(9),
            finish: fixture.at + TimeDelta::seconds(60),
            in_point: TimeDelta::zero(),
            out_point: TimeDelta::seconds(51),
            preferred_audio_title: Some("English".into()),
            pinned_stream_selection: Some(PinnedStreamSelection {
                media_version_id: 200,
                video_stream_index: 0,
                audio_stream_index: Some(1),
                subtitle_stream_index: None,
            }),
            ..Default::default()
        })
        .await
        .unwrap();
}

async fn read_channel_window_manifest(manifest: &tempfile::TempDir) -> protocol::Playout {
    let mut files = tokio::fs::read_dir(manifest.path()).await.unwrap();
    let entry = files.next_entry().await.unwrap().unwrap();
    assert!(files.next_entry().await.unwrap().is_none());
    serde_json::from_slice(&tokio::fs::read(entry.path()).await.unwrap()).unwrap()
}

#[tokio::test]
async fn next_channel_window_crosses_boundaries_and_keeps_pins_and_persisted_state() {
    let fixture = Fixture::new().await;
    channel_window_second_movie(&fixture).await;
    fixture
        .state
        .next_engine
        .media
        .configure("127.0.0.1:8409".parse().unwrap())
        .unwrap();
    let before = fixture
        .state
        .repositories
        .playout
        .items_in_range(
            1,
            fixture.at - TimeDelta::seconds(30),
            fixture.at + TimeDelta::seconds(90),
        )
        .await
        .unwrap();
    let mut query = Fixture::query();
    query.channel = Some(1);
    query.seek_seconds = None;
    query.start = Some((fixture.at + TimeDelta::milliseconds(850)).to_rfc3339());
    let paths = fixture.paths();
    let mut session = None;
    let (manifest, configuration, number, _) = prepare(
        &fixture.state,
        &query,
        &paths,
        Instant::now() + Duration::from_secs(30),
        &mut session,
    )
    .await
    .unwrap();
    let playout = read_channel_window_manifest(&manifest).await;
    assert_eq!(configuration.playout.virtual_start, Some(fixture.at));
    assert_eq!(number, "1");
    assert_eq!(playout.items.len(), 3);
    assert_eq!(
        (playout.items[1].start, playout.items[1].finish),
        (
            fixture.at + TimeDelta::seconds(5),
            fixture.at + TimeDelta::seconds(9)
        )
    );
    assert_eq!(playout.items[2].finish, fixture.at + TimeDelta::seconds(30));
    let wire = serde_json::to_value(&playout).unwrap();
    assert_eq!(
        wire["items"][0]["tracks"]["audio"]["source"]["source_type"],
        "lavfi"
    );
    assert_eq!(wire["items"][2]["tracks"]["audio"]["stream_index"], 1);
    assert!(playout.items[2].tracks.as_ref().unwrap().subtitle.is_none());
    let after = fixture
        .state
        .repositories
        .playout
        .items_in_range(
            1,
            fixture.at - TimeDelta::seconds(30),
            fixture.at + TimeDelta::seconds(90),
        )
        .await
        .unwrap();
    assert_eq!(before, after);
    fixture
        .state
        .next_engine
        .finish_troubleshooting_session(session.unwrap())
        .await
        .unwrap();
}

#[tokio::test]
async fn next_channel_window_historical_mirror_uses_selected_profile_normalization() {
    let fixture = Fixture::new().await;
    channel_window_second_movie(&fixture).await;
    fixture
        .state
        .next_engine
        .media
        .configure("127.0.0.1:8409".parse().unwrap())
        .unwrap();
    let repos = &fixture.state.repositories;
    let mut profile = repos.ffmpeg_profile.get(2).await.unwrap();
    profile.normalize_framerate = true;
    repos.ffmpeg_profile.upsert(&profile).await.unwrap();
    let source = repos.channel.get_with_profile(1).await.unwrap();
    for offset in [TimeDelta::hours(1), -TimeDelta::hours(1)] {
        let mut mirror = source.clone();
        mirror.id = 2;
        mirror.number = "2".into();
        mirror.playout_source = ersatzrs_domain::ChannelPlayoutSource::Mirror;
        mirror.mirror_source_channel_id = Some(1);
        mirror.playout_offset = Some(offset);
        repos.channel.upsert(&mirror).await.unwrap();
        let mut query = Fixture::query();
        query.channel = Some(2);
        query.seek_seconds = None;
        query.start = Some((fixture.at + offset).to_rfc3339());
        let mut session = None;
        let (manifest, configuration, _, _) = prepare(
            &fixture.state,
            &query,
            &fixture.paths(),
            Instant::now() + Duration::from_secs(30),
            &mut session,
        )
        .await
        .unwrap();
        assert_eq!(
            configuration.normalization.video.frame_rate.as_deref(),
            Some("25")
        );
        assert_eq!(
            configuration.playout.virtual_start,
            Some(fixture.at + offset)
        );
        let playout = read_channel_window_manifest(&manifest).await;
        assert_eq!(
            playout.items[2].finish,
            fixture.at + offset + TimeDelta::seconds(30)
        );
        assert_eq!(playout.items[0].id, "1");
        assert_eq!(playout.items[2].id, "2");
        fixture
            .state
            .next_engine
            .finish_troubleshooting_session(session.unwrap())
            .await
            .unwrap();
    }
    profile.normalize_framerate = false;
    repos.ffmpeg_profile.upsert(&profile).await.unwrap();
    let mut query = Fixture::query();
    query.channel = Some(1);
    query.seek_seconds = None;
    query.start = Some(fixture.at.to_rfc3339());
    let mut session = None;
    let (_, configuration, _, _) = prepare(
        &fixture.state,
        &query,
        &fixture.paths(),
        Instant::now() + Duration::from_secs(30),
        &mut session,
    )
    .await
    .unwrap();
    assert!(configuration.normalization.video.frame_rate.is_none());
    fixture
        .state
        .next_engine
        .finish_troubleshooting_session(session.unwrap())
        .await
        .unwrap();
}

#[tokio::test]
async fn next_channel_window_scopes_manual_choices_and_rejects_foreign_initial_subtitle() {
    let fixture = Fixture::new().await;
    channel_window_second_movie(&fixture).await;
    fixture
        .state
        .next_engine
        .media
        .configure("127.0.0.1:8409".parse().unwrap())
        .unwrap();
    let subtitle = fixture.root.path().join("first.nl.srt");
    tokio::fs::write(
        &subtitle,
        b"1\n00:00:00,000 --> 00:01:00,000\nFirst Dutch\n",
    )
    .await
    .unwrap();
    sqlx::query(r#"INSERT INTO "Subtitle" ("Id","MovieMetadataId","SubtitleKind","Title","StreamIndex","Codec","Default","Forced","SDH","Language","IsExtracted","Path","DateAdded","DateUpdated") VALUES (2,20,1,'First Dutch',9,'subrip',1,0,0,'nld',0,?,'2026-09-26','2026-09-26')"#)
        .bind(subtitle.to_str().unwrap())
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    let mut channel = fixture
        .state
        .repositories
        .channel
        .get_with_profile(1)
        .await
        .unwrap();
    channel.next_engine_text_subtitle_mode = ersatzrs_domain::NextEngineTextSubtitleMode::Convert;
    fixture
        .state
        .repositories
        .channel
        .upsert(&channel)
        .await
        .unwrap();
    let mut query = Fixture::query();
    query.channel = Some(1);
    query.seek_seconds = None;
    query.start = Some(fixture.at.to_rfc3339());
    query.subtitle_id = Some(20);
    let mut session = None;
    assert!(prepare(
        &fixture.state,
        &query,
        &fixture.paths(),
        Instant::now() + Duration::from_secs(30),
        &mut session
    )
    .await
    .is_err());
    assert!(session.is_none());
    query.subtitle_id = Some(2);
    let selected = select_channel_window(&fixture.state, &query).await.unwrap();
    assert_eq!(
        selected.media[0].item.preferred_audio_title.as_deref(),
        Some("Dutch")
    );
    assert_eq!(
        selected.media[1].item.preferred_audio_title.as_deref(),
        Some("English")
    );
    let (manifest, _, _, _) = prepare(
        &fixture.state,
        &query,
        &fixture.paths(),
        Instant::now() + Duration::from_secs(30),
        &mut session,
    )
    .await
    .unwrap();
    let playout = read_channel_window_manifest(&manifest).await;
    assert!(playout.items[0].tracks.as_ref().unwrap().subtitle.is_some());
    assert!(playout.items[2].tracks.as_ref().unwrap().subtitle.is_none());
    assert_eq!(
        playout.items[2]
            .tracks
            .as_ref()
            .unwrap()
            .audio
            .as_ref()
            .unwrap()
            .stream_index,
        Some(1)
    );
    fixture
        .state
        .next_engine
        .finish_troubleshooting_session(session.unwrap())
        .await
        .unwrap();
}

#[test]
fn next_channel_window_custom_selector_override_and_reset_are_transient() {
    let original = Channel {
        stream_selector_mode: ersatzrs_domain::ChannelStreamSelectorMode::Custom,
        stream_selector: Some("stored.yml".into()),
        ..Default::default()
    };
    let mut channel = original.clone();
    let mut query = Fixture::query();
    query.stream_selector = Some("selected.yml".into());
    apply_channel_selector(&mut channel, &query);
    assert_eq!(channel.stream_selector.as_deref(), Some("selected.yml"));
    assert_eq!(
        channel.stream_selector_mode,
        ersatzrs_domain::ChannelStreamSelectorMode::Custom
    );
    query.stream_selector = None;
    apply_channel_selector(&mut channel, &query);
    assert!(channel.stream_selector.is_none());
    assert_eq!(
        channel.stream_selector_mode,
        ersatzrs_domain::ChannelStreamSelectorMode::Default
    );
    assert_eq!(original.stream_selector.as_deref(), Some("stored.yml"));
}

async fn channel_window_music_video(fixture: &Fixture, remote: bool) {
    channel_window_second_movie(fixture).await;
    for sql in [
        r#"INSERT INTO "MediaItem" ("Id","LibraryPathId","State") VALUES (31,1,0)"#,
        r#"INSERT INTO "Artist" ("Id") VALUES (31)"#,
        r#"INSERT INTO "ArtistMetadata" ("Id","ArtistId","Title","MetadataKind","DateAdded","DateUpdated") VALUES (32,31,'Window Artist',0,'2026-09-30','2026-09-30')"#,
        r#"INSERT INTO "MusicVideo" ("Id","ArtistId") VALUES (11,31)"#,
        r#"UPDATE "MediaVersion" SET "MovieId"=NULL,"MusicVideoId"=11 WHERE "Id"=200"#,
        r#"DELETE FROM "Subtitle" WHERE "Id"=20"#,
        r#"DELETE FROM "MovieMetadata" WHERE "MovieId"=11"#,
        r#"DELETE FROM "Movie" WHERE "Id"=11"#,
        r#"INSERT INTO "MusicVideoMetadata" ("Id","MusicVideoId","Title","Album","MetadataKind","DateAdded","DateUpdated") VALUES (33,11,'Window Credit Song','Window Album',0,'2026-09-30','2026-09-30')"#,
    ] {
        sqlx::query(sql).execute(&fixture.state.pool).await.unwrap();
    }
    let caption = fixture.root.path().join("second.nl.srt");
    tokio::fs::write(
        &caption,
        b"1\n00:00:00,000 --> 00:01:00,000\nSecond Dutch\n",
    )
    .await
    .unwrap();
    sqlx::query(r#"INSERT INTO "Subtitle" ("Id","MusicVideoMetadataId","SubtitleKind","Title","StreamIndex","Codec","Default","Forced","SDH","Language","IsExtracted","Path","DateAdded","DateUpdated") VALUES (20,33,1,'Second Dutch',9,'subrip',1,1,0,'nld',0,?,'2026-09-30','2026-09-30')"#)
        .bind(caption.to_str().unwrap()).execute(&fixture.state.pool).await.unwrap();
    if remote {
        sqlx::query(r#"INSERT INTO "RemoteStream" ("Id","Url","IsLive","Liveness","Duration") VALUES (11,'https://example.test/window.mp4',0,1,'00:01:00')"#).execute(&fixture.state.pool).await.unwrap();
        sqlx::query(r#"UPDATE "MediaVersion" SET "RemoteStreamId"=11 WHERE "Id"=200"#)
            .execute(&fixture.state.pool)
            .await
            .unwrap();
    }
    let mut channel = fixture
        .state
        .repositories
        .channel
        .get_with_profile(1)
        .await
        .unwrap();
    channel.subtitle_mode = ersatzrs_domain::ChannelSubtitleMode::Default;
    channel.music_video_credits_mode =
        ersatzrs_domain::ChannelMusicVideoCreditsMode::GenerateSubtitles;
    channel.next_engine_text_subtitle_mode = ersatzrs_domain::NextEngineTextSubtitleMode::Convert;
    fixture
        .state
        .repositories
        .channel
        .upsert(&channel)
        .await
        .unwrap();
    fixture
        .state
        .next_engine
        .media
        .configure("127.0.0.1:8409".parse().unwrap())
        .unwrap();
}

fn window_query(fixture: &Fixture) -> PlaybackPreviewQuery {
    let mut query = Fixture::query();
    query.channel = Some(1);
    query.seek_seconds = None;
    query.start = Some(fixture.at.to_rfc3339());
    query.subtitle_id = None;
    query
}

async fn window_second_selection(fixture: &Fixture) -> PlayoutItem {
    fixture
        .state
        .repositories
        .playout
        .items_in_range(1, fixture.at, fixture.at + TimeDelta::seconds(60))
        .await
        .unwrap()
        .into_iter()
        .find(|item| item.id == 2)
        .unwrap()
}

#[tokio::test]
async fn next_channel_window_music_video_preserves_off_and_exact_scheduled_captions() {
    for (mode, pin, caption) in [
        (
            Some(ersatzrs_domain::ChannelSubtitleMode::None),
            false,
            None,
        ),
        (None, true, None),
        (None, true, Some(9)),
    ] {
        let fixture = Fixture::new().await;
        channel_window_music_video(&fixture, false).await;
        let mut second = window_second_selection(&fixture).await;
        second.subtitle_mode = mode;
        second.pinned_stream_selection = pin.then_some(PinnedStreamSelection {
            media_version_id: 200,
            video_stream_index: 0,
            audio_stream_index: Some(1),
            subtitle_stream_index: caption,
        });
        fixture
            .state
            .repositories
            .playout
            .upsert_item(&second)
            .await
            .unwrap();
        let mut session = None;
        let paths = fixture.paths();
        let (manifest, _, _, _) = prepare(
            &fixture.state,
            &window_query(&fixture),
            &paths,
            Instant::now() + Duration::from_secs(30),
            &mut session,
        )
        .await
        .unwrap();
        let playout = read_channel_window_manifest(&manifest).await;
        let tracks = playout.items[2].tracks.as_ref().unwrap();
        assert_eq!(
            tracks.audio.as_ref().unwrap().stream_index,
            Some(if pin { 1 } else { 2 })
        );
        if caption.is_some() {
            let protocol::Source::Local { path, .. } =
                tracks.subtitle.as_ref().unwrap().source.as_ref().unwrap()
            else {
                panic!("selected caption");
            };
            assert_eq!(
                path,
                fixture.root.path().join("second.nl.srt").to_str().unwrap()
            );
        } else {
            assert!(tracks.subtitle.is_none());
        }
        assert!(!paths
            .output
            .join("occurrence-2")
            .join("music-video-credits-playback.ass")
            .exists());
        fixture
            .state
            .next_engine
            .finish_troubleshooting_session(session.unwrap())
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn next_channel_window_music_video_keeps_custom_dutch_caption() {
    let fixture = Fixture::new().await;
    channel_window_music_video(&fixture, false).await;
    let mut second = window_second_selection(&fixture).await;
    second.pinned_stream_selection = None;
    fixture
        .state
        .repositories
        .playout
        .upsert_item(&second)
        .await
        .unwrap();
    let mut query = window_query(&fixture);
    let _selector = configure_runtime_selector(&mut query);
    let mut session = None;
    let (manifest, _, _, _) = prepare(
        &fixture.state,
        &query,
        &fixture.paths(),
        Instant::now() + Duration::from_secs(30),
        &mut session,
    )
    .await
    .unwrap();
    let playout = read_channel_window_manifest(&manifest).await;
    let tracks = playout.items[2].tracks.as_ref().unwrap();
    assert_eq!(tracks.audio.as_ref().unwrap().stream_index, Some(2));
    let protocol::Source::Local { path, .. } =
        tracks.subtitle.as_ref().unwrap().source.as_ref().unwrap()
    else {
        panic!("custom Dutch caption");
    };
    assert_eq!(
        path,
        fixture.root.path().join("second.nl.srt").to_str().unwrap()
    );
    fixture
        .state
        .next_engine
        .finish_troubleshooting_session(session.unwrap())
        .await
        .unwrap();
}

#[tokio::test]
async fn next_channel_window_music_video_keeps_initial_manual_caption() {
    let fixture = Fixture::new().await;
    channel_window_music_video(&fixture, false).await;
    let mut second = window_second_selection(&fixture).await;
    second.pinned_stream_selection = None;
    fixture
        .state
        .repositories
        .playout
        .upsert_item(&second)
        .await
        .unwrap();
    let mut query = window_query(&fixture);
    query.start = Some((fixture.at + TimeDelta::seconds(9)).to_rfc3339());
    query.subtitle_id = Some(20);
    let paths = fixture.paths();
    let mut session = None;
    let (manifest, _, _, _) = prepare(
        &fixture.state,
        &query,
        &paths,
        Instant::now() + Duration::from_secs(30),
        &mut session,
    )
    .await
    .unwrap();
    let playout = read_channel_window_manifest(&manifest).await;
    let tracks = playout.items[0].tracks.as_ref().unwrap();
    let protocol::Source::Local { path, .. } =
        tracks.subtitle.as_ref().unwrap().source.as_ref().unwrap()
    else {
        panic!("manual caption");
    };
    assert_eq!(
        path,
        fixture.root.path().join("second.nl.srt").to_str().unwrap()
    );
    assert!(!paths
        .output
        .join("occurrence-2")
        .join("music-video-credits-playback.ass")
        .exists());
    fixture
        .state
        .next_engine
        .finish_troubleshooting_session(session.unwrap())
        .await
        .unwrap();
}

#[tokio::test]
async fn next_channel_window_remote_music_video_credits_survive_deferred_resolution_and_revoke() {
    let fixture = Fixture::new().await;
    channel_window_music_video(&fixture, true).await;
    let mut second = window_second_selection(&fixture).await;
    second.pinned_stream_selection = None;
    fixture
        .state
        .repositories
        .playout
        .upsert_item(&second)
        .await
        .unwrap();
    let paths = fixture.paths();
    let mut session = None;
    let (manifest, _, _, _) = prepare(
        &fixture.state,
        &window_query(&fixture),
        &paths,
        Instant::now() + Duration::from_secs(30),
        &mut session,
    )
    .await
    .unwrap();
    let playout = read_channel_window_manifest(&manifest).await;
    let item = &playout.items[2];
    assert!(item.tracks.is_none());
    let Some(protocol::Source::Dynamic { uri, .. }) = item.source.as_ref() else {
        panic!("deferred remote credits");
    };
    let segments: Vec<_> = uri.split('/').collect();
    let digest = segments[segments.len() - 2];
    let published_audio: u32 = segments.last().unwrap().parse().unwrap();
    assert_eq!(published_audio, 2);
    let instance = session.unwrap();
    let start = fixture.at + TimeDelta::seconds(9);
    assert!(fixture
        .state
        .next_engine
        .resolve_dynamic(1, 2, digest, Some(1), "1", instance, start, item.finish)
        .await
        .is_err());
    for offset in [0, 2] {
        let resolved = fixture
            .state
            .next_engine
            .resolve_dynamic(
                1,
                2,
                digest,
                Some(published_audio),
                "1",
                instance,
                start + TimeDelta::seconds(offset),
                item.finish,
            )
            .await
            .unwrap();
        let tracks = resolved.tracks.as_ref().unwrap();
        assert_eq!(tracks.audio.as_ref().unwrap().stream_index, Some(2));
        let protocol::Source::Local {
            path, in_point_ms, ..
        } = tracks.subtitle.as_ref().unwrap().source.as_ref().unwrap()
        else {
            panic!("owned generated credits");
        };
        assert_eq!(*in_point_ms, Some(u64::try_from(offset * 1000).unwrap()));
        assert_eq!(
            path,
            paths
                .output
                .join("occurrence-2")
                .join("music-video-credits-playback.ass")
                .to_str()
                .unwrap()
        );
        assert!(tokio::fs::read_to_string(path)
            .await
            .unwrap()
            .contains("Window Credit Song"));
    }
    fixture
        .state
        .next_engine
        .finish_troubleshooting_session(instance)
        .await
        .unwrap();
    assert!(fixture
        .state
        .next_engine
        .resolve_dynamic(
            1,
            2,
            digest,
            Some(published_audio),
            "1",
            instance,
            start,
            item.finish
        )
        .await
        .is_err());
}

#[tokio::test]
async fn next_channel_window_eligible_local_music_video_has_owned_generated_credits() {
    let fixture = Fixture::new().await;
    channel_window_music_video(&fixture, false).await;
    let mut second = window_second_selection(&fixture).await;
    second.pinned_stream_selection = None;
    fixture
        .state
        .repositories
        .playout
        .upsert_item(&second)
        .await
        .unwrap();
    let paths = fixture.paths();
    let mut session = None;
    let (manifest, _, _, _) = prepare(
        &fixture.state,
        &window_query(&fixture),
        &paths,
        Instant::now() + Duration::from_secs(30),
        &mut session,
    )
    .await
    .unwrap();
    let playout = read_channel_window_manifest(&manifest).await;
    let tracks = playout.items[2].tracks.as_ref().unwrap();
    assert_eq!(tracks.audio.as_ref().unwrap().stream_index, Some(2));
    let protocol::Source::Local {
        path, in_point_ms, ..
    } = tracks.subtitle.as_ref().unwrap().source.as_ref().unwrap()
    else {
        panic!("owned generated caption");
    };
    assert_eq!(*in_point_ms, Some(0));
    assert_eq!(
        path,
        paths
            .output
            .join("occurrence-2")
            .join("music-video-credits-playback.ass")
            .to_str()
            .unwrap()
    );
    assert!(tokio::fs::read_to_string(path)
        .await
        .unwrap()
        .contains("Window Credit Song"));
    fixture
        .state
        .next_engine
        .finish_troubleshooting_session(session.unwrap())
        .await
        .unwrap();
}

#[tokio::test]
async fn next_channel_window_music_video_preserves_explicit_manual_zero() {
    let fixture = Fixture::new().await;
    channel_window_music_video(&fixture, false).await;
    let mut second = window_second_selection(&fixture).await;
    second.pinned_stream_selection = None;
    fixture
        .state
        .repositories
        .playout
        .upsert_item(&second)
        .await
        .unwrap();
    let mut query = window_query(&fixture);
    query.subtitle_id = Some(0);
    query.start = Some((fixture.at + TimeDelta::seconds(9)).to_rfc3339());
    let paths = fixture.paths();
    let mut session = None;
    let (manifest, _, _, _) = prepare(
        &fixture.state,
        &query,
        &paths,
        Instant::now() + Duration::from_secs(30),
        &mut session,
    )
    .await
    .unwrap();
    let playout = read_channel_window_manifest(&manifest).await;
    assert!(playout.items[0].tracks.as_ref().unwrap().subtitle.is_none());
    assert!(!paths
        .output
        .join("occurrence-2")
        .join("music-video-credits-playback.ass")
        .exists());
    fixture
        .state
        .next_engine
        .finish_troubleshooting_session(session.unwrap())
        .await
        .unwrap();
}

#[tokio::test]
async fn next_channel_window_initial_manual_zero_does_not_disable_later_pinned_dutch_caption() {
    let fixture = Fixture::new().await;
    channel_window_music_video(&fixture, false).await;
    let mut second = window_second_selection(&fixture).await;
    second.pinned_stream_selection = Some(PinnedStreamSelection {
        media_version_id: 200,
        video_stream_index: 0,
        audio_stream_index: Some(1),
        subtitle_stream_index: Some(9),
    });
    fixture
        .state
        .repositories
        .playout
        .upsert_item(&second)
        .await
        .unwrap();
    let mut query = window_query(&fixture);
    query.subtitle_id = Some(0);
    let mut session = None;
    let (manifest, _, _, _) = prepare(
        &fixture.state,
        &query,
        &fixture.paths(),
        Instant::now() + Duration::from_secs(30),
        &mut session,
    )
    .await
    .unwrap();
    let playout = read_channel_window_manifest(&manifest).await;
    assert!(playout.items[0].tracks.as_ref().unwrap().subtitle.is_none());
    let tracks = playout.items[2].tracks.as_ref().unwrap();
    assert_eq!(tracks.audio.as_ref().unwrap().stream_index, Some(1));
    let protocol::Source::Local { path, .. } =
        tracks.subtitle.as_ref().unwrap().source.as_ref().unwrap()
    else {
        panic!("later pinned Dutch caption");
    };
    assert_eq!(
        path,
        fixture.root.path().join("second.nl.srt").to_str().unwrap()
    );
    fixture
        .state
        .next_engine
        .finish_troubleshooting_session(session.unwrap())
        .await
        .unwrap();
}

#[path = "troubleshooting_channel_window_runtime_tests.rs"]
mod channel_window_runtime;
