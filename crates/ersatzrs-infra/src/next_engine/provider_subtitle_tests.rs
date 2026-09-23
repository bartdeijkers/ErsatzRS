//! Provider closed-caption eligibility is independent of local/HTTP transport.
use super::*;

const PROVIDERS: [(&str, bool); 6] = [
    (r#"INSERT INTO "PlexMovie" ("Id") VALUES (10)"#, false),
    (r#"INSERT INTO "PlexEpisode" ("Id") VALUES (10)"#, true),
    (r#"INSERT INTO "JellyfinMovie" ("Id") VALUES (10)"#, false),
    (r#"INSERT INTO "JellyfinEpisode" ("Id") VALUES (10)"#, true),
    (r#"INSERT INTO "EmbyMovie" ("Id") VALUES (10)"#, false),
    (r#"INSERT INTO "EmbyEpisode" ("Id") VALUES (10)"#, true),
];

async fn captions_fixture(provider: Option<(&'static str, bool)>) -> DbPool {
    let pool = fixture_pool().await;
    for sql in [
        r#"UPDATE "MediaStream" SET "Codec"='eia_608' WHERE "Id"=5"#,
        r#"UPDATE "Subtitle" SET "Codec"='eia_608' WHERE "Id"=1"#,
        r#"INSERT INTO "MediaStream" ("Index","Codec","MediaStreamKind","Language","Channels","Title","Default","Forced","MediaVersionId")
            VALUES (6,'subrip',3,'eng',0,'Fallback',0,0,100)"#,
        r#"INSERT INTO "Subtitle" ("Id","MovieMetadataId","SubtitleKind","Title","StreamIndex","Codec","Default","Forced","SDH","Language","IsExtracted","Path","DateAdded","DateUpdated")
            VALUES (4,20,0,'Fallback',6,'subrip',0,0,0,'eng',1,'synthetic/fallback.srt','2026-09-23','2026-09-23')"#,
    ] {
        sqlx::query(sql).execute(&pool).await.unwrap();
    }
    if let Some((insert, episode)) = provider {
        if episode {
            for sql in [
                r#"INSERT INTO "MediaItem" ("Id","LibraryPathId","State") VALUES (11,1,0),(12,1,0)"#,
                r#"INSERT INTO "Show" ("Id") VALUES (11)"#,
                r#"INSERT INTO "Season" ("Id","ShowId") VALUES (12,11)"#,
                r#"INSERT INTO "Episode" ("Id","SeasonId") VALUES (10,12)"#,
                r#"INSERT INTO "EpisodeMetadata" ("Id","EpisodeId","MetadataKind","DateAdded","DateUpdated") VALUES (30,10,0,'2026-09-23','2026-09-23')"#,
                r#"UPDATE "Subtitle" SET "MovieMetadataId"=NULL,"EpisodeMetadataId"=30"#,
                r#"UPDATE "MediaVersion" SET "MovieId"=NULL,"EpisodeId"=10 WHERE "Id"=100"#,
                r#"DELETE FROM "Movie" WHERE "Id"=10"#,
            ] {
                sqlx::query(sql).execute(&pool).await.unwrap();
            }
        }
        sqlx::query(insert).execute(&pool).await.unwrap();
    }
    pool
}

fn caption_channel() -> Channel {
    Channel {
        next_engine_text_subtitle_mode: ersatzrs_domain::NextEngineTextSubtitleMode::Convert,
        preferred_subtitle_language_code: Some("eng".into()),
        subtitle_mode: ChannelSubtitleMode::Default,
        ..Default::default()
    }
}

fn http_source() -> wire::Source {
    wire::Source::Http {
        probe_hint: None,
        is_live: None,
        uri: "https://media.example.test/synthetic.mkv".into(),
        headers: Vec::new(),
        user_agent: None,
        keep_alive: Some(false),
        reconnect: Some(true),
        reconnect_delay_max: None,
        timeout_us: None,
        in_point_ms: None,
        out_point_ms: None,
    }
}

#[tokio::test]
async fn all_provider_item_types_exclude_captions_before_ranking_on_local_and_http_sources() {
    for provider in PROVIDERS {
        let pool = captions_fixture(Some(provider)).await;
        let channel = caption_channel();
        let local = project(&pool, &channel, &item()).await.unwrap();
        assert_eq!(
            local.tracks.unwrap().subtitle.unwrap().stream_index,
            Some(6)
        );
        let remote = project_item(
            &pool,
            &channel,
            &item(),
            100,
            http_source(),
            &Resolution::default(),
        )
        .await
        .unwrap();
        assert_eq!(
            remote.tracks.unwrap().subtitle.unwrap().stream_index,
            Some(6)
        );
        // Native selection remains unchanged, including its extracted-text policy.
        let native = crate::playback::select_playback_subtitle(
            &pool,
            10,
            &["eng".into()],
            ChannelSubtitleMode::Default,
            StreamingMode::HttpLiveStreamingSegmenter,
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(native.codec.as_deref(), Some("eia_608"));
    }
}

#[tokio::test]
async fn provider_caption_pins_fail_and_explicit_off_and_valid_pins_stay_exact() {
    for provider in PROVIDERS {
        let pool = captions_fixture(Some(provider)).await;
        let channel = caption_channel();
        let mut pinned = item();
        pinned.pinned_stream_selection = Some(PinnedStreamSelection {
            media_version_id: 100,
            video_stream_index: 0,
            audio_stream_index: Some(2),
            subtitle_stream_index: Some(4),
        });
        let error = project(&pool, &channel, &pinned).await.err().unwrap();
        assert!(error
            .to_string()
            .contains("selected subtitle is unsupported by Next"));
        assert!(!error.to_string().contains("synthetic/"));
        pinned
            .pinned_stream_selection
            .as_mut()
            .unwrap()
            .subtitle_stream_index = Some(6);
        assert_eq!(
            project(&pool, &channel, &pinned)
                .await
                .unwrap()
                .tracks
                .unwrap()
                .subtitle
                .unwrap()
                .stream_index,
            Some(6)
        );
        pinned
            .pinned_stream_selection
            .as_mut()
            .unwrap()
            .subtitle_stream_index = None;
        assert!(project(&pool, &channel, &pinned)
            .await
            .unwrap()
            .tracks
            .unwrap()
            .subtitle
            .is_none());
    }
}

#[tokio::test]
async fn custom_provider_selection_filters_captions_but_local_item_captions_remain_eligible() {
    use std::io::Write as _;
    let folder = crate::file_system_layout::channel_stream_selectors_folder();
    std::fs::create_dir_all(&folder).unwrap();
    let mut selector = tempfile::Builder::new()
        .prefix("ucu29-")
        .suffix(".yml")
        .tempfile_in(folder)
        .unwrap();
    selector
        .write_all(b"items:\n  - audio_language: [eng]\n    subtitle_language: [eng]\n")
        .unwrap();
    selector.flush().unwrap();
    let mut channel = caption_channel();
    channel.stream_selector_mode = ChannelStreamSelectorMode::Custom;
    channel.stream_selector = Some(
        selector
            .path()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .into(),
    );
    for provider in PROVIDERS {
        let pool = captions_fixture(Some(provider)).await;
        assert_eq!(
            project(&pool, &channel, &item())
                .await
                .unwrap()
                .tracks
                .unwrap()
                .subtitle
                .unwrap()
                .stream_index,
            Some(6)
        );
    }
    let local = captions_fixture(None).await;
    assert_eq!(
        project(&local, &channel, &item())
            .await
            .unwrap()
            .tracks
            .unwrap()
            .subtitle
            .unwrap()
            .stream_index,
        Some(4)
    );
    channel.stream_selector_mode = ChannelStreamSelectorMode::Default;
    assert_eq!(
        project(&local, &channel, &item())
            .await
            .unwrap()
            .tracks
            .unwrap()
            .subtitle
            .unwrap()
            .stream_index,
        Some(4)
    );
    let mut pinned = item();
    pinned.pinned_stream_selection = Some(PinnedStreamSelection {
        media_version_id: 100,
        video_stream_index: 0,
        audio_stream_index: Some(2),
        subtitle_stream_index: Some(4),
    });
    assert_eq!(
        project(&local, &channel, &pinned)
            .await
            .unwrap()
            .tracks
            .unwrap()
            .subtitle
            .unwrap()
            .stream_index,
        Some(4)
    );
}
