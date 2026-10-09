use super::*;

#[test]
fn seeded_choices_and_native_artist_serialization_are_stable() {
    let mut first = Choices::new(Some(42));
    let mut second = Choices::new(Some(42));
    let a = (0..16).map(|_| first.next(3)).collect::<Vec<_>>();
    let b = (0..16).map(|_| second.next(3)).collect::<Vec<_>>();
    assert_eq!(a, b);
    assert_eq!(artist_names("Björk; Sigur Rós"), ["Björk", "Sigur Rós"]);
    assert_eq!(
        artist_names(r#"["Björk","Sigur Rós"]"#),
        ["Björk", "Sigur Rós"]
    );
}

#[tokio::test]
async fn prepared_song_uses_ordered_thumbnail_and_target_mirror_geometry() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    for sql in [
        r#"CREATE TABLE "SongMetadata" ("Id" INTEGER, "SongId" INTEGER, "Title" TEXT, "Album" TEXT, "Artists" TEXT, "AlbumArtists" TEXT)"#,
        r#"CREATE TABLE "Artwork" ("Id" INTEGER, "SongMetadataId" INTEGER, "ArtworkKind" INTEGER, "Path" TEXT, "BlurHash43" TEXT, "BlurHash54" TEXT, "BlurHash64" TEXT)"#,
        r#"INSERT INTO "SongMetadata" VALUES(1,42,'日本語','Album','Artist; Guest','Artist; Producer')"#,
    ] {
        sqlx::query(sql).execute(&pool).await.unwrap();
    }
    let cover = tempfile::NamedTempFile::new().unwrap();
    tokio::fs::write(cover.path(), COVER).await.unwrap();
    sqlx::query(r#"INSERT INTO "Artwork" VALUES(1,1,1,?,'LEHV6nWB2yk8pyo0adR*.7kCMdnj',NULL,NULL),(2,1,1,'missing',NULL,NULL,NULL)"#).bind(cover.path().to_string_lossy().as_ref()).execute(&pool).await.unwrap();
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let target = Resolution {
        width: 640,
        height: 480,
        ..Resolution::default()
    };
    let a = prepare(
        &pool,
        42,
        &target,
        ChannelSongVideoMode::WithProgress,
        Some(99),
        first.path(),
        Path::new("ffmpeg"),
    )
    .await
    .unwrap();
    let b = prepare(
        &pool,
        42,
        &target,
        ChannelSongVideoMode::WithProgress,
        Some(99),
        second.path(),
        Path::new("ffmpeg"),
    )
    .await
    .unwrap();
    assert_eq!(a.command.args, b.command.args);
    assert_eq!(a.stream.frame_size, Some(FrameSize::new(640, 480)));
    assert_eq!(a.stream.index, 0);
    assert!(a.stream.is_still_image);
    assert_eq!(
        tokio::fs::read(first.path().join("cover.png"))
            .await
            .unwrap(),
        COVER
    );
    assert_eq!(
        tokio::fs::read(first.path().join("blur.rgba"))
            .await
            .unwrap(),
        tokio::fs::read(second.path().join("blur.rgba"))
            .await
            .unwrap()
    );
    let text = tokio::fs::read_to_string(first.path().join("song.ass"))
        .await
        .unwrap();
    assert!(text.contains("日本語"));
    assert!(text.contains("Artist, Guest"));
}

#[test]
fn malformed_blurhash_is_bounded_and_uses_cover_blur_fallback() {
    let mut choices = Choices::new(Some(3));
    for hash in ["", "☃not-ascii", "!!!!!!", "a", "0\0bad"] {
        assert!(decoded_hash(&[hash.to_owned()], &mut choices).is_none());
    }
    let bytes = decoded_hash(
        &["☃".to_owned(), "LEHV6nWB2yk8pyo0adR*.7kCMdnj".to_owned()],
        &mut choices,
    )
    .expect("valid fallback hash");
    assert_eq!(bytes.len(), 32 * 32 * 4);
    assert!(bytes.chunks_exact(4).all(|pixel| pixel[3] == 255));
}

#[test]
fn song_metadata_layout_keeps_unicode_and_escapes_formatting() {
    let metadata = Metadata {
        title: "Björk {\\fs999} 日本語".to_owned(),
        album: "Debut".to_owned(),
        artists: vec!["Björk".to_owned()],
        album_artists: vec!["Björk".to_owned(), "Guest".to_owned()],
        ..Metadata::default()
    };
    let rendered = subtitle(&metadata, FrameSize::new(1920, 1080), true, false, true);
    assert!(rendered.contains("OPTIKabel-Heavy,54"));
    assert!(rendered.contains("595,58,162"));
    assert!(rendered.contains("Björk \\{\\\u{2060}fs999\\} 日本語"));
    assert!(rendered.contains(r"\NGuest\NDebut"));
    assert!(!rendered.contains(r"{\fs999}"));
}

#[test]
fn song_picture_command_pins_rgb_target_and_preserves_progress_space() {
    let args = arguments(
        &PictureLayout {
            size: FrameSize::new(1280, 720),
            left_cover: false,
            progress: true,
        },
        true,
        false,
    );
    let graph = args
        .iter()
        .position(|s| s == "-filter_complex")
        .map(|i| &args[i + 1])
        .unwrap();
    assert!(graph.contains("scale=1280:720"));
    assert!(graph.contains("boxblur=20:1"));
    assert!(graph.contains("H-h-H*0.15"));
    assert!(args.windows(2).any(|pair| pair == ["-pix_fmt", "rgb24"]));
    assert_eq!(args.last().map(String::as_str), Some("pipe:1"));
    assert!(
        !graph.contains("sbar"),
        "Next PNG never embeds native progress"
    );
    assert!(dimensions(&Resolution {
        width: 0,
        height: 720,
        ..Resolution::default()
    })
    .is_err());
}

#[tokio::test]
async fn song_resource_override_precedes_extracted_and_embedded_defaults() {
    let root = tempfile::tempdir().unwrap();
    let destination = root.path().join("output.png");
    tokio::fs::write(root.path().join("_song_background_1.png"), b"extracted")
        .await
        .unwrap();
    tokio::fs::write(root.path().join("song_background_1.png"), b"custom")
        .await
        .unwrap();
    resource_from(
        root.path(),
        "song_background_1.png",
        b"embedded",
        &destination,
    )
    .await
    .unwrap();
    assert_eq!(tokio::fs::read(&destination).await.unwrap(), b"custom");
    tokio::fs::remove_file(root.path().join("song_background_1.png"))
        .await
        .unwrap();
    resource_from(
        root.path(),
        "song_background_1.png",
        b"embedded",
        &destination,
    )
    .await
    .unwrap();
    assert_eq!(tokio::fs::read(&destination).await.unwrap(), b"extracted");
}

#[tokio::test]
async fn optional_artwork_404_and_nonimage_200_keep_default_song_picture() {
    let server = httpmock::MockServer::start_async().await;
    let missing = server
        .mock_async(|when, then| {
            when.path("/missing");
            then.status(404);
        })
        .await;
    let corrupt = server
        .mock_async(|when, then| {
            when.path("/corrupt");
            then.status(200)
                .header("content-type", "image/png")
                .body("<html>not an image</html>");
        })
        .await;
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    for sql in [
        r#"CREATE TABLE "SongMetadata" ("Id" INTEGER, "SongId" INTEGER, "Title" TEXT, "Album" TEXT, "Artists" TEXT, "AlbumArtists" TEXT)"#,
        r#"CREATE TABLE "Artwork" ("Id" INTEGER, "SongMetadataId" INTEGER, "ArtworkKind" INTEGER, "Path" TEXT, "BlurHash43" TEXT, "BlurHash54" TEXT, "BlurHash64" TEXT)"#,
        r#"INSERT INTO "SongMetadata" VALUES(1,42,'Available Song','Album','Artist','Artist')"#,
        r#"INSERT INTO "Artwork" VALUES(1,1,1,'',NULL,NULL,NULL)"#,
    ] {
        sqlx::query(sql).execute(&pool).await.unwrap();
    }
    for route in ["/missing", "/corrupt"] {
        sqlx::query(r#"UPDATE "Artwork" SET "Path" = ?"#)
            .bind(server.url(route))
            .execute(&pool)
            .await
            .unwrap();
        let root = tempfile::tempdir().unwrap();
        let picture = prepare(
            &pool,
            42,
            &Resolution {
                width: 320,
                height: 180,
                ..Resolution::default()
            },
            ChannelSongVideoMode::Default,
            Some(99),
            root.path(),
            Path::new("ffmpeg"),
        )
        .await
        .unwrap();
        assert!(
            picture.fallback_command.is_none(),
            "default picture selected before rendering"
        );
        assert!(picture
            .command
            .args
            .iter()
            .any(|arg| arg == "background.png"));
        assert_eq!(
            tokio::fs::read(root.path().join("cover.png"))
                .await
                .unwrap(),
            tokio::fs::read(root.path().join("default-cover.png"))
                .await
                .unwrap()
        );
    }
    missing.assert_async().await;
    corrupt.assert_async().await;
    sqlx::query("DROP TABLE Artwork")
        .execute(&pool)
        .await
        .unwrap();
    let root = tempfile::tempdir().unwrap();
    assert!(
        prepare(
            &pool,
            42,
            &Resolution {
                width: 320,
                height: 180,
                ..Resolution::default()
            },
            ChannelSongVideoMode::Default,
            Some(99),
            root.path(),
            Path::new("ffmpeg")
        )
        .await
        .is_err(),
        "database failures do not become optional-art fallback"
    );
}

#[test]
#[ignore = "Owned managed child fixture, invoked only by its cancellation regression"]
#[expect(
    clippy::disallowed_methods,
    reason = "Reads only the explicit opt-in marker for this owned managed child fixture"
)]
fn native_song_blocking_child_fixture() {
    if std::env::var_os("ERSATZRS_TEST_SONG_BLOCKING_CHILD").is_none() {
        return;
    }
    std::fs::write("started", std::process::id().to_string()).unwrap();
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}

#[tokio::test]
#[expect(
    clippy::await_holding_lock,
    reason = "Serializes the global admission registry for the managed child regression"
)]
async fn dropping_native_render_request_reaps_child_and_releases_its_owned_assets_and_slot() {
    use crate::transcode_capacity::{
        active_transcode_snapshot, active_transcode_test_guard_for_tests,
        reset_active_transcode_state_for_tests, try_reserve_active_transcode, ActiveTranscodeSlot,
    };
    let _guard = active_transcode_test_guard_for_tests();
    reset_active_transcode_state_for_tests();
    let reservation =
        try_reserve_active_transcode(1, ActiveTranscodeSlot::new_mpeg_ts("song-drop")).unwrap();
    let directory = tempfile::Builder::new()
        .prefix("ersatzrs-native-song-drop-test-")
        .tempdir()
        .unwrap();
    let path = directory.path().to_owned();
    let command = RemoteStreamCommand {
        program: std::env::current_exe()
            .unwrap()
            .to_string_lossy()
            .into_owned(),
        args: vec![
            "--exact".into(),
            "song_background::tests::native_song_blocking_child_fixture".into(),
            "--ignored".into(),
            "--nocapture".into(),
        ],
        working_directory: path.clone(),
        environment: BTreeMap::from([(
            "ERSATZRS_TEST_SONG_BLOCKING_CHILD".to_owned(),
            "1".to_owned(),
        )]),
        tool_paths: Vec::new(),
    };
    let song = std::sync::Arc::new(NativeSongBackground {
        used_artwork_fallback: AtomicBool::new(false),
        path: path.join("song.png"),
        stream: SourceVideoStream::new(SourceVideoCodec::Unknown),
        with_progress: false,
        progress_mask: path.join("progress.png"),
        directory: Some(directory),
        retain_on_drop: AtomicBool::new(false),
        rendered: AtomicBool::new(false),
        render_commands: std::sync::Mutex::new(Some((command, None))),
    });
    let (_sender, receiver) = oneshot::channel();
    let request = tokio::spawn(render_native(
        song,
        Instant::now() + Duration::from_secs(30),
        receiver,
        Some(reservation.clone()),
    ));
    tokio::time::timeout(Duration::from_secs(10), async {
        while !path.join("started").exists() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    drop(reservation);
    assert_eq!(
        active_transcode_snapshot(1).active,
        1,
        "owned renderer holds the admitted slot after its request owner is dropped"
    );
    request.abort();
    assert!(request.await.unwrap_err().is_cancelled());
    tokio::time::timeout(Duration::from_secs(10), async {
        while path.exists() || active_transcode_snapshot(1).active != 0 {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert!(!path.exists());
    assert_eq!(active_transcode_snapshot(1).active, 0);
    reset_active_transcode_state_for_tests();
}
