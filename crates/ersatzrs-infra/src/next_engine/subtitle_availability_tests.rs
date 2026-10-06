use super::*;

fn channel() -> Channel {
    Channel {
        subtitle_mode: ChannelSubtitleMode::Any,
        preferred_subtitle_language_code: Some("nld".into()),
        ..Default::default()
    }
}

#[tokio::test]
async fn unavailable_automatic_sidecars_are_omitted_but_exact_pins_fail() {
    let pool = fixture_pool().await;
    let root = tempfile::tempdir().unwrap();
    let absent = root.path().join("absent.srt");
    for path in [
        None,
        Some(""),
        Some(" "),
        Some("http://"),
        absent.to_str(),
        root.path().to_str(),
    ] {
        sqlx::query(r#"UPDATE "Subtitle" SET "Path"=? WHERE "Id"=2"#)
            .bind(path)
            .execute(&pool)
            .await
            .unwrap();
        let projected = project(&pool, &channel(), &item()).await.unwrap();
        assert!(projected.tracks.unwrap().subtitle.is_none());
        let mut pinned = item();
        pinned.pinned_stream_selection = Some(PinnedStreamSelection {
            media_version_id: 100,
            video_stream_index: 0,
            audio_stream_index: Some(2),
            subtitle_stream_index: Some(9),
        });
        let error = project(&pool, &channel(), &pinned).await.err().unwrap();
        assert!(error
            .to_string()
            .contains("selected subtitle is unavailable"));
        assert!(!error.to_string().contains("absent.srt"));
    }
}

#[tokio::test]
async fn existing_local_and_remote_sidecars_keep_media_seek() {
    let pool = fixture_pool().await;
    let root = tempfile::tempdir().unwrap();
    let sidecar = root.path().join("captions.srt");
    tokio::fs::write(&sidecar, "1\n00:00:01,000 --> 00:00:02,000\nCaption\n")
        .await
        .unwrap();
    for (id, path) in [
        (2, sidecar.to_str().unwrap()),
        (2, "https://captions.example.test/captions.srt"),
        (0, "HTTPS://captions.example.test/captions.srt"),
        (0, "http://localhost/captions.srt"),
        (0, "http://127.0.0.1/captions.srt"),
    ] {
        sqlx::query(r#"UPDATE "Subtitle" SET "Id"=?,"Path"=? WHERE "Id" IN (0,2)"#)
            .bind(id)
            .bind(path)
            .execute(&pool)
            .await
            .unwrap();
        let projected = project(&pool, &channel(), &item()).await.unwrap();
        let source = projected.tracks.unwrap().subtitle.unwrap().source.unwrap();
        if let wire::Source::Http { uri, .. } = &source {
            assert!(uri.starts_with("http://") || uri.starts_with("https://"));
        }
        match source {
            wire::Source::Local {
                in_point_ms,
                out_point_ms,
                ..
            }
            | wire::Source::Http {
                in_point_ms,
                out_point_ms,
                ..
            } => {
                assert_eq!(in_point_ms, Some(1000));
                assert_eq!(out_point_ms, Some(61_000));
            }
            _ => panic!("standalone source expected"),
        }
    }
    tokio::fs::remove_file(sidecar).await.unwrap();
}

#[tokio::test]
async fn unresolvable_provider_sidecars_are_omitted_without_hiding_database_errors() {
    let pool = fixture_pool().await;
    sqlx::raw_sql(
        r#"
        INSERT INTO "EmbyMediaSource" ("Id") VALUES (1);
        INSERT INTO "EmbyLibrary" ("Id","ShouldSyncItems") VALUES (1,1);
        INSERT INTO "EmbyMovie" ("Id","ItemId") VALUES (10,'synthetic-item');
        UPDATE "Subtitle" SET "Path"=NULL WHERE "Id"=2;
    "#,
    )
    .execute(&pool)
    .await
    .unwrap();
    assert!(project(&pool, &channel(), &item())
        .await
        .unwrap()
        .tracks
        .unwrap()
        .subtitle
        .is_none());
    let selected = crate::playback::load_subtitles_for_media_item(&pool, 10)
        .await
        .unwrap()
        .into_iter()
        .find(|subtitle| subtitle.id == 2)
        .unwrap();
    pool.close().await;
    let mut selected = Some(selected);
    assert!(availability::retain(
        &pool,
        &mut selected,
        &wire::Source::Local {
            path: "synthetic.mkv".into(),
            probe_hint: None,
            in_point_ms: None,
            out_point_ms: None,
        },
        false,
        10,
        false
    )
    .await
    .is_err());
    assert!(selected.is_some());
}

#[tokio::test]
async fn music_video_metadata_sidecars_remain_selected_without_generated_credits() {
    let pool = fixture_pool().await;
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("music.srt");
    tokio::fs::write(&path, "1\n00:00:01,000 --> 00:00:02,000\nMusic caption\n")
        .await
        .unwrap();
    sqlx::raw_sql(r#"
        INSERT INTO "MediaItem" ("Id","LibraryPathId","State") VALUES (11,1,0);
        INSERT INTO "Artist" ("Id") VALUES (11);
        INSERT INTO "MusicVideo" ("Id","ArtistId") VALUES (10,11);
        INSERT INTO "MusicVideoMetadata" ("Id","MusicVideoId","MetadataKind","DateAdded","DateUpdated") VALUES (30,10,0,'2026-09-29','2026-09-29');
        UPDATE "Subtitle" SET "MovieMetadataId"=NULL,"MusicVideoMetadataId"=30;
        UPDATE "MediaVersion" SET "MovieId"=NULL,"MusicVideoId"=10 WHERE "Id"=100;
        DELETE FROM "Movie" WHERE "Id"=10;
    "#).execute(&pool).await.unwrap();
    sqlx::query(r#"UPDATE "Subtitle" SET "Path"=? WHERE "Id"=2"#)
        .bind(path.to_str().unwrap())
        .execute(&pool)
        .await
        .unwrap();
    let channel = Channel {
        music_video_credits_mode: ersatzrs_domain::ChannelMusicVideoCreditsMode::None,
        ..channel()
    };
    let before = crate::playback::load_subtitles_for_media_item(&pool, 10)
        .await
        .unwrap();
    let projected = project(&pool, &channel, &item()).await.unwrap();
    let track = projected.tracks.unwrap().subtitle.unwrap();
    assert!(
        matches!(track.source, Some(wire::Source::Local { path: selected, .. })
        if selected == path.to_str().unwrap())
    );
    assert_eq!(
        before,
        crate::playback::load_subtitles_for_media_item(&pool, 10)
            .await
            .unwrap()
    );
}
