//! Real provider HTTP media, authenticated relay, and selected embedded `WebVTT`.
use super::*;
use ersatzrs_core::repositories::RemoteMediaSourceKind;

#[tokio::test]
#[ignore = "requires official Next and FFmpeg; disposable authenticated provider sidecars"]
async fn real_next_provider_external_text_burns_and_converts_after_seek() {
    let binary = runtime_path("ERSATZRS_NEXT_TEST_BIN");
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    let root = tempfile::tempdir().unwrap();
    let clip = bilingual_clip(&ffmpeg, root.path()).await;
    let bytes = std::sync::Arc::new(tokio::fs::read(&clip).await.unwrap());
    for kind in RemoteMediaSourceKind::ALL {
        for convert in [false, true] {
            let server = fixture_media_server().await;
            let (pool, locker) = provider_text_pool(&clip, &text_profile(), kind).await;
            if kind == RemoteMediaSourceKind::Plex {
                use_plex_other_video(&pool).await;
            }
            sqlx::query(
                r#"UPDATE "Subtitle" SET "SubtitleKind"=1,"Path"=? WHERE "Language"='nld'"#,
            )
            .bind(match kind {
                RemoteMediaSourceKind::Plex => {
                    Some("https://old.example.test/library/streams/123?X-Plex-Token=synthetic-old")
                }
                RemoteMediaSourceKind::Jellyfin => None,
                RemoteMediaSourceKind::Emby => Some("source-id"),
            })
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(r#"UPDATE "MediaStream" SET "MediaStreamKind"=5 WHERE "Language"='nld' AND "MediaStreamKind"=3"#)
                .execute(&pool).await.unwrap();
            sqlx::query(r#"UPDATE "Channel" SET "NextEngineTextSubtitleMode"=? WHERE "Id"=1"#)
                .bind(i32::from(convert))
                .execute(&pool)
                .await
                .unwrap();
            let (header, value, media_path, subtitle_path) = provider_endpoints(kind);
            let media = server
                .mock_async(|when, then| {
                    when.method(httpmock::Method::GET)
                        .path(media_path)
                        .header(header, value);
                    let bytes = bytes.clone();
                    then.respond_with(move |request| fixture_range_response(request, &bytes));
                })
                .await;
            let subtitle = server
                .mock_async(|when, then| {
                    when.method(httpmock::Method::GET)
                        .path(subtitle_path)
                        .header(header, value);
                    then.status(200).body(CUES);
                })
                .await;
            let _unused_plex = fixture_provider_connection(&pool, kind, &server).await;
            if !convert {
                assert_native_provider_sidecar(&pool, &ffmpeg, &clip, root.path()).await;
            }
            let host = std::sync::Arc::new(host::NextHost::new(
                pool,
                locker,
                root.path()
                    .join(format!("{}-{convert}-playout", kind.provider_name())),
            ));
            let mut callback = callback_listener(host.clone()).await;
            assert_provider_conversion(
                &host,
                host::NextStart {
                    channel_id: 1,
                    preview: false,
                    number: "1".into(),
                    executable: binary.clone(),
                    output: root
                        .path()
                        .join(format!("{}-{convert}-output", kind.provider_name())),
                    ffmpeg: ffmpeg.clone(),
                    ffprobe: ffprobe.clone(),
                    replacements: Vec::new(),
                    idle_timeout: None,
                    deadline: Instant::now() + Duration::from_secs(30),
                },
                convert,
            )
            .await;
            assert!(
                subtitle.calls_async().await > 0,
                "callback must fetch the selected provider sidecar"
            );
            assert!(media.calls_async().await > 0);
            callback.stop().await;
        }
    }
}

async fn use_plex_other_video(pool: &crate::db::DbPool) {
    // Exercise the newly supported owner through native rendering and Next.
    sqlx::raw_sql(r#"
        INSERT INTO "OtherVideo" ("Id") VALUES (10);
        INSERT INTO "PlexOtherVideo" ("Id") VALUES (10);
        INSERT INTO "OtherVideoMetadata" ("Id","OtherVideoId","MetadataKind","DateAdded","DateUpdated")
            VALUES (30,10,0,'2026-09-29','2026-09-29');
        UPDATE "Subtitle" SET "MovieMetadataId"=NULL,"OtherVideoMetadataId"=30;
        UPDATE "MediaVersion" SET "MovieId"=NULL,"OtherVideoId"=10 WHERE "MovieId"=10;
        DELETE FROM "Movie" WHERE "Id"=10;
    "#).execute(pool).await.unwrap();
}

fn provider_endpoints(
    kind: RemoteMediaSourceKind,
) -> (&'static str, &'static str, &'static str, &'static str) {
    match kind {
        RemoteMediaSourceKind::Plex => (
            "X-Plex-Token",
            "<token>",
            "/library/parts/11/file.mkv",
            "/library/streams/123",
        ),
        RemoteMediaSourceKind::Jellyfin => (
            "Authorization",
            "MediaBrowser Token=\"<api-key>\"",
            "/Videos/fixture-item/stream",
            "/Videos/fixture-item/fixture-item/Subtitles/3/3/Stream.srt",
        ),
        RemoteMediaSourceKind::Emby => (
            "X-Emby-Token",
            "<api-key>",
            "/Videos/fixture-item/stream",
            "/Videos/fixture-item/source-id/Subtitles/3/Stream.srt",
        ),
    }
}

async fn assert_native_provider_sidecar(
    pool: &crate::db::DbPool,
    ffmpeg: &Path,
    clip: &Path,
    root: &Path,
) {
    let selected = crate::playback::select_playback_subtitle(
        pool,
        10,
        &["nld".into()],
        ersatzrs_domain::ChannelSubtitleMode::Any,
        ersatzrs_domain::StreamingMode::TransportStream,
    )
    .await
    .unwrap()
    .unwrap();
    let prepared =
        crate::provider_subtitles::prepare(pool, &selected, &root.join("native-captions"))
            .await
            .unwrap();
    let input = crate::playback::subtitle_input_for_stream(
        &prepared,
        clip.to_str().unwrap(),
        ersatzrs_ffmpeg::OutputFormat::MpegTs,
        ersatzrs_domain::StreamingMode::TransportStream,
    )
    .unwrap();
    let output = root.join("native-provider.ts");
    let mut command = Command::new(ffmpeg);
    command
        .args(["-v", "error", "-nostdin", "-y", "-i"])
        .arg(clip)
        .args([
            "-ss", "5", "-t", "3", "-map", "0:v:0", "-map", "0:a:1", "-vf",
        ])
        .arg(ersatzrs_ffmpeg::filter::subtitles_filter(&input.path, None))
        .args([
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-c:a",
            "aac",
            "-f",
            "mpegts",
        ])
        .arg(&output);
    tool(command).await;
    assert_text_video_and_audio(ffmpeg, &output, true).await;
}

pub(super) async fn bilingual_clip(ffmpeg: &Path, folder: &Path) -> PathBuf {
    let dutch = generate_text_clip(ffmpeg, folder).await;
    let english = folder.join("english.srt");
    tokio::fs::write(
        &english,
        "1\n00:00:00,000 --> 00:01:00,000\nUNSELECTED ENGLISH PROVIDER CUE\n",
    )
    .await
    .unwrap();
    let output = folder.join("bilingual.mkv");
    let mut mux = Command::new(ffmpeg);
    mux.args(["-v", "error", "-nostdin", "-y", "-i"])
        .arg(dutch)
        .arg("-i")
        .arg(english)
        .args([
            "-map",
            "0",
            "-map",
            "1:0",
            "-c",
            "copy",
            "-metadata:s:s:1",
            "language=eng",
            "-metadata:s:s:1",
            "title=English",
            "-disposition:s:1",
            "default",
        ])
        .arg(&output);
    tool(mux).await;
    output
}

pub(super) async fn provider_text_pool(
    clip: &Path,
    profile: &FFmpegProfile,
    kind: RemoteMediaSourceKind,
) -> (crate::db::DbPool, ersatzrs_core::locking::EntityLocker) {
    let (pool, locker) = prepare_schedule(clip, false, profile).await;
    configure_text_tracks(&pool, true, true, false).await;
    sqlx::query(r#"DELETE FROM "LocalLibrary""#)
        .execute(&pool)
        .await
        .unwrap();
    crate::next_engine::source::tests::seed_provider(&pool, kind).await;
    if kind == RemoteMediaSourceKind::Plex {
        sqlx::query(r#"INSERT INTO "PlexMovie" ("Id") VALUES (10)"#)
            .execute(&pool)
            .await
            .unwrap();
    }
    assert!(sqlx::query_scalar::<_, bool>(
        r#"SELECT EXISTS(SELECT 1 FROM "PlexMovie" WHERE "Id"=10)
            OR EXISTS(SELECT 1 FROM "JellyfinMovie" WHERE "Id"=10)
            OR EXISTS(SELECT 1 FROM "EmbyMovie" WHERE "Id"=10)"#
    )
    .fetch_one(&pool)
    .await
    .unwrap());
    for sql in [
        r#"UPDATE "MediaFile" SET "Path"='server-only/bilingual.mkv' WHERE "Id"=1"#,
        r#"INSERT INTO "MediaStream" ("Index","Codec","MediaStreamKind","Language","Channels","Title","Default","Forced","MediaVersionId")
        VALUES (4,'subrip',3,'eng',0,'English',1,0,100)"#,
        r#"INSERT INTO "Subtitle" ("MovieMetadataId","SubtitleKind","Title","StreamIndex","Codec","Default","Forced","SDH","Language","IsExtracted","DateAdded","DateUpdated")
        VALUES (20,0,'English',4,'subrip',1,0,0,'eng',0,'2026-09-23','2026-09-23')"#,
    ] {
        sqlx::query(sql).execute(&pool).await.unwrap();
    }
    (pool, locker)
}

async fn assert_provider_conversion(
    host: &std::sync::Arc<host::NextHost>,
    request: host::NextStart,
    convert: bool,
) {
    let registry = std::sync::Arc::new(crate::hls::InMemoryHlsSessionRegistry::new());
    let task = host.start(registry.clone(), request.clone()).await.unwrap();
    let handle = registry.try_get("1").await.unwrap();
    timeout(Duration::from_secs(30), async {
        loop {
            match handle.startup_readiness() {
                HlsStartupReadiness::Ready => break,
                HlsStartupReadiness::Failed => panic!("provider text conversion failed"),
                _ => tokio::time::sleep(Duration::from_millis(10)).await,
            }
        }
    })
    .await
    .unwrap();
    let playlist = handle.trim_playlist().await.unwrap();
    let segment = playlist
        .lines()
        .find(|line| {
            !line.starts_with('#')
                && Path::new(line)
                    .extension()
                    .is_some_and(|extension| extension == "ts")
        })
        .unwrap();
    let duration: f64 = playlist
        .lines()
        .find_map(|line| line.strip_prefix("#EXTINF:"))
        .unwrap()
        .trim_end_matches(',')
        .parse()
        .unwrap();
    let vtt = request.output.join(segment).with_extension("vtt");
    let body = timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(body) = tokio::fs::read_to_string(&vtt).await {
                break body;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let _ = assert_text_vtt(&body, convert, duration);
    assert!(!body.contains("UNSELECTED ENGLISH"));
    assert_text_video_and_audio(&request.ffmpeg, &request.output.join(segment), !convert).await;
    assert!(registry.stop_channel("1").await);
    task.await.unwrap();
    assert!(!request.output.exists());
}

#[tokio::test]
#[ignore = "requires built Next engine and FFmpeg runtime; uses disposable synthetic provider media"]
async fn real_next_provider_embedded_text_converts_selected_language_with_seek_and_auth() {
    let binary = runtime_path("ERSATZRS_NEXT_TEST_BIN");
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    let root = tempfile::tempdir().unwrap();
    let clip = bilingual_clip(&ffmpeg, root.path()).await;
    let bytes = std::sync::Arc::new(tokio::fs::read(&clip).await.unwrap());
    let profile = FFmpegProfile {
        video_format: FFmpegProfileVideoFormat::H264,
        audio_format: FFmpegProfileAudioFormat::Aac,
        video_bitrate: 1000,
        video_buffer_size: 2000,
        audio_sample_rate: 48,
        audio_channels: 1,
        ..Default::default()
    };
    for kind in RemoteMediaSourceKind::ALL {
        let server = fixture_media_server().await;
        let (pool, locker) = provider_text_pool(&clip, &profile, kind).await;
        let selected = (kind != RemoteMediaSourceKind::Plex).then_some("native-selected-source");
        if let Some(id) = selected {
            sqlx::query(r#"UPDATE "MediaFile" SET "ProviderMediaSourceId"=? WHERE "Id"=1"#)
                .bind(id)
                .execute(&pool)
                .await
                .unwrap();
        }
        let (path, header, value) = match kind {
            RemoteMediaSourceKind::Plex => {
                ("/library/parts/11/file.mkv", "X-Plex-Token", "<token>")
            }
            RemoteMediaSourceKind::Jellyfin => (
                "/Videos/fixture-item/stream",
                "Authorization",
                "MediaBrowser Token=\"<api-key>\"",
            ),
            RemoteMediaSourceKind::Emby => {
                ("/Videos/fixture-item/stream", "X-Emby-Token", "<api-key>")
            }
        };
        let mock = server
            .mock_async(|when, then| {
                let when = when
                    .method(httpmock::Method::GET)
                    .path(path)
                    .header(header, value);
                if let Some(id) = selected {
                    when.query_param("MediaSourceId", id);
                }
                let bytes = bytes.clone();
                then.respond_with(move |request| fixture_range_response(request, &bytes));
            })
            .await;
        let _wrong_plex_server = fixture_provider_connection(&pool, kind, &server).await;
        let host = std::sync::Arc::new(host::NextHost::new(
            pool.clone(),
            locker,
            root.path()
                .join(format!("{}-playout", kind.provider_name())),
        ));
        let relay = fixture_media_relay(host.clone(), pool).await;
        let denied = reqwest::Client::new()
            .get(format!("{}/ffmpeg/next-media/10/100/1", relay.base_url()))
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .unwrap();
        assert_eq!(denied.status(), reqwest::StatusCode::UNAUTHORIZED);
        assert_eq!(
            mock.calls_async().await,
            0,
            "unauthorized relay must not open provider media"
        );
        assert_provider_conversion(
            &host,
            host::NextStart {
                channel_id: 1,
                preview: false,
                number: "1".into(),
                executable: binary.clone(),
                output: root.path().join(format!("{}-output", kind.provider_name())),
                ffmpeg: ffmpeg.clone(),
                ffprobe: ffprobe.clone(),
                replacements: Vec::new(),
                idle_timeout: None,
                deadline: Instant::now() + Duration::from_secs(30),
            },
            true,
        )
        .await;
        assert!(
            mock.calls_async().await > 0,
            "Next must fetch authenticated provider media"
        );
    }
}
