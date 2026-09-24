use super::*;
use ersatzrs_core::repositories::PlayoutRepository as _;
use ersatzrs_domain::ChannelSubtitleMode;
use std::{path::PathBuf, sync::Arc};

fn wire(start: DateTime<Utc>, a: i64, b: i64) -> protocol::PlayoutItem {
    protocol::PlayoutItem {
        id: format!("{a}"),
        start: start + Duration::seconds(a),
        finish: start + Duration::seconds(b),
        source: None,
        tracks: None,
        watermark: None,
    }
}

#[test]
fn callback_bounds_preserve_persisted_fractional_precision() {
    for seconds in [-1_700_000_000, -1, 0, 1_700_000_000] {
        let time = DateTime::from_timestamp(seconds, 123_456_700).unwrap();
        assert_eq!(
            from_timestamp_ticks(timestamp_ticks(time).unwrap()).unwrap(),
            time
        );
    }
    assert!(timestamp_ticks(DateTime::from_timestamp(0, 123_456_789).unwrap()).is_err());
}

#[test]
fn gaps_preserve_overlap_union_and_have_stable_authenticated_sources() {
    let relay = super::super::media::MediaRelay::new();
    relay.configure("127.0.0.1:8409".parse().unwrap()).unwrap();
    let start = DateTime::from_timestamp(1_700_000_000, 123_456_700).unwrap();
    let result = insert_gaps(
        &relay,
        1,
        start,
        vec![
            wire(start, 90, 120),
            wire(start, 0, 60),
            wire(start, 10, 20),
        ],
    )
    .unwrap()
    .0;
    assert_eq!(result.len(), 4);
    let gap = &result[2];
    assert_eq!(
        (gap.start, gap.finish),
        (start + Duration::seconds(60), start + Duration::seconds(90))
    );
    let value = serde_json::to_value(gap).unwrap();
    assert_eq!(value["source"]["source_type"], "dynamic");
    assert!(value["source"]["uri"]
        .as_str()
        .unwrap()
        .contains("/next-fallback/1/"));
    let headers = value["source"]["headers"].as_array().unwrap();
    assert_eq!(headers.len(), 2);
    let ticket = headers[0].as_str().unwrap().split_once(": ").unwrap().1;
    assert!(relay.authorized(ticket));
    assert!(!super::super::media::MediaRelay::new().authorized(ticket));
    let again = insert_gaps(
        &relay,
        1,
        start,
        vec![wire(start, 0, 60), wire(start, 90, 120)],
    )
    .unwrap()
    .0;
    assert_eq!(again[1].id, gap.id);
    assert!(insert_gaps(&relay, 1, start, Vec::new())
        .unwrap()
        .0
        .is_empty());
}

#[test]
fn initial_gap_starts_at_publication_time_without_outer_gap_for_empty_schedule() {
    let relay = super::super::media::MediaRelay::new();
    relay.configure("127.0.0.1:8409".parse().unwrap()).unwrap();
    let start = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
    let (items, gaps) = insert_gaps(&relay, 1, start, vec![wire(start, 20, 40)]).unwrap();
    assert_eq!(
        gaps,
        vec![Gap {
            start,
            finish: start + Duration::seconds(20)
        }]
    );
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].id, gaps[0].id());
    assert_eq!(items[1].start, start + Duration::seconds(20));
    assert!(insert_gaps(&relay, 1, start, Vec::new())
        .unwrap()
        .0
        .is_empty());
}

#[tokio::test]
async fn shared_manifest_gaps_reach_normal_and_preview_sessions_only_on_their_channel() {
    let (host, normal, start, _root) = fixture().await;
    let published = Gap {
        start: start + Duration::seconds(20),
        finish: start + Duration::seconds(40),
    };
    let preview = Uuid::new_v4();
    let other_channel = Uuid::new_v4();
    {
        let mut contexts = host.dynamic_contexts.lock().await;
        let normal_request = contexts.get(&normal).unwrap().request.clone();
        for (instance, channel_id) in [(preview, 1), (other_channel, 2)] {
            let mut request = normal_request.clone();
            request.channel_id = channel_id;
            request.preview = true;
            contexts.insert(
                instance,
                super::super::host::DynamicContext {
                    request,
                    transfers: Arc::default(),
                    fallback: Arc::default(),
                    gaps: Arc::default(),
                },
            );
        }
    }
    host.register_published_gaps(1, &[published]).await;
    host.register_published_gaps(1, &[published]).await;
    let contexts = host.dynamic_contexts.lock().await;
    for instance in [normal, preview] {
        let gaps = contexts.get(&instance).unwrap().gaps.lock().await;
        assert!(gaps.contains(&published));
        assert_eq!(gaps.iter().filter(|gap| **gap == published).count(), 1);
    }
    assert!(contexts
        .get(&other_channel)
        .unwrap()
        .gaps
        .lock()
        .await
        .is_empty());
}

pub(super) async fn fixture() -> (Arc<NextHost>, Uuid, DateTime<Utc>, tempfile::TempDir) {
    let root = tempfile::tempdir().unwrap();
    let (pool, locker, mut channel, _) = schedule::tests::fixture().await;
    let start = DateTime::from_timestamp(Utc::now().timestamp(), 123_456_700).unwrap();
    let path = root.path().join("fixture.mkv");
    std::fs::write(&path, b"synthetic identity fixture").unwrap();
    sqlx::query(r#"INSERT INTO "LocalLibrary" ("Id") VALUES (1)"#)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "MediaFile" SET "Path"=? WHERE "Id"=1"#)
        .bind(path.to_str().unwrap())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "FillerPreset" ("Id","AllowWatermarks","CollectionType","MediaItemId","FillerKind","FillerMode","UseChaptersAsMediaItems") VALUES (1,0,10,10,4,0,0)"#).execute(&pool).await.unwrap();
    channel.streaming_engine = StreamingEngine::Next;
    channel.subtitle_mode = ChannelSubtitleMode::None;
    channel.preferred_audio_language_code = Some("nld".into());
    channel.fallback_filler_id = Some(1);
    let repos = make_repositories(pool.clone());
    repos.channel.upsert(&channel).await.unwrap();
    repos
        .playout
        .upsert_item(&PlayoutItem {
            id: 1,
            playout_id: 1,
            media_item_id: 10,
            start,
            finish: start + Duration::seconds(60),
            ..Default::default()
        })
        .await
        .unwrap();
    repos
        .playout
        .upsert_item(&PlayoutItem {
            id: 2,
            playout_id: 1,
            media_item_id: 10,
            start: start + Duration::seconds(180),
            finish: start + Duration::seconds(240),
            ..Default::default()
        })
        .await
        .unwrap();
    let host = Arc::new(NextHost::new(pool, locker, root.path().join("playout")));
    host.media
        .configure("127.0.0.1:8409".parse().unwrap())
        .unwrap();
    let instance = Uuid::new_v4();
    host.dynamic_contexts.lock().await.insert(
        instance,
        super::super::host::DynamicContext {
            request: super::super::host::NextStart {
                channel_id: 1,
                preview: false,
                number: "1".into(),
                executable: PathBuf::from("fixture-next"),
                output: root.path().join("output"),
                ffmpeg: PathBuf::from("fixture-ffmpeg"),
                ffprobe: PathBuf::from("fixture-ffprobe"),
                replacements: Vec::new(),
                idle_timeout: None,
                deadline: Instant::now() + std::time::Duration::from_secs(30),
            },
            transfers: Arc::default(),
            fallback: Arc::default(),
            gaps: Arc::new(tokio::sync::Mutex::new(vec![Gap {
                start: start + Duration::seconds(60),
                finish: start + Duration::seconds(180),
            }])),
        },
    );
    (host, instance, start, root)
}

async fn resolve(
    host: &NextHost,
    instance: Uuid,
    start: DateTime<Utc>,
    seconds: i64,
) -> Result<protocol::PlayoutItem> {
    let gap = Gap {
        start: start + Duration::seconds(60),
        finish: start + Duration::seconds(180),
    };
    host.resolve_fallback(
        1,
        timestamp_ticks(gap.start)?,
        timestamp_ticks(gap.finish)?,
        "1",
        instance,
        &gap.id(),
        start + Duration::seconds(seconds),
        gap.finish,
    )
    .await
}

#[tokio::test]
async fn repeated_callbacks_seek_one_occurrence_and_clip_the_next() {
    let (host, instance, start, _root) = fixture().await;
    let initial = resolve(&host, instance, start, 90).await.unwrap();
    assert_eq!(initial.finish, start + Duration::seconds(150));
    let json = serde_json::to_value(&initial).unwrap();
    assert_eq!(json["tracks"]["audio"]["stream_index"], 2);
    assert!(json["tracks"].get("subtitle").is_none());
    let again = resolve(&host, instance, start, 105).await.unwrap();
    assert_eq!(again.finish, initial.finish);
    assert_eq!(
        serde_json::to_value(again).unwrap()["tracks"]["video"]["source"]["in_point_ms"],
        15_000
    );
    let next = resolve(&host, instance, start, 155).await.unwrap();
    assert_eq!(next.finish, start + Duration::seconds(180));
    let rows: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "PlayoutItem""#)
        .fetch_one(&host.pool)
        .await
        .unwrap();
    assert_eq!(rows, 2);
    assert!(resolve(&host, instance, start, 180).await.is_err());
    host.dynamic_contexts.lock().await.remove(&instance);
    assert!(resolve(&host, instance, start, 160).await.is_err());
}

#[tokio::test]
async fn leading_gap_callback_requires_the_exact_published_interval() {
    let (host, instance, start, _root) = fixture().await;
    let gap = Gap {
        start: start - Duration::seconds(20),
        finish: start,
    };
    let dynamic_id = gap.id();
    let resolve_leading = || {
        host.resolve_fallback(
            1,
            timestamp_ticks(gap.start).unwrap(),
            timestamp_ticks(gap.finish).unwrap(),
            "1",
            instance,
            &dynamic_id,
            start - Duration::seconds(10),
            gap.finish,
        )
    };
    assert!(resolve_leading().await.is_err());
    let gaps = host
        .dynamic_contexts
        .lock()
        .await
        .get(&instance)
        .unwrap()
        .gaps
        .clone();
    *gaps.lock().await = vec![gap];
    let item = resolve_leading().await.unwrap();
    assert_eq!(item.start, start - Duration::seconds(10));
    assert_eq!(item.finish, start);
    gaps.lock().await.clear();
    assert!(resolve_leading().await.is_err());
}

#[tokio::test]
async fn playlist_fallback_pins_override_language_and_preserve_explicit_off() {
    let (host, instance, start, _root) = fixture().await;
    for sql in [
        r#"INSERT INTO "PlaylistGroup" ("Id","Name","IsSystem") VALUES (1,'Fixture',0)"#,
        r#"INSERT INTO "Playlist" ("Id","Name","PlaylistGroupId","IsSystem") VALUES (1,'Pinned fallback',1,0)"#,
        r#"INSERT INTO "PlaylistItem" ("Id","Index","PlaylistId","CollectionType","MediaItemId","MediaVersionId","VideoStreamIndex","AudioStreamIndex","SubtitleStreamIndex") VALUES (1,0,1,10,10,100,0,1,NULL)"#,
        r#"UPDATE "FillerPreset" SET "CollectionType"=6,"MediaItemId"=NULL,"PlaylistId"=1 WHERE "Id"=1"#,
        r#"UPDATE "Channel" SET "SubtitleMode"=2 WHERE "Id"=1"#,
    ] {
        sqlx::query(sql).execute(&host.pool).await.unwrap();
    }
    let initial = resolve(&host, instance, start, 90).await.unwrap();
    let json = serde_json::to_value(initial).unwrap();
    assert_eq!(json["tracks"]["video"]["stream_index"], 0);
    assert_eq!(json["tracks"]["audio"]["stream_index"], 1);
    assert!(json["tracks"].get("subtitle").is_none());

    sqlx::query(r#"UPDATE "PlaylistItem" SET "AudioStreamIndex"=NULL WHERE "Id"=1"#)
        .execute(&host.pool)
        .await
        .unwrap();
    assert!(
        resolve(&host, instance, start, 95).await.is_err(),
        "changed pins must invalidate the cached occurrence"
    );
    let silent = resolve(&host, instance, start, 155).await.unwrap();
    let json = serde_json::to_value(silent).unwrap();
    assert!(
        json["tracks"]["audio"]["source"]["params"]
            == "anullsrc=channel_layout=stereo:sample_rate=48000",
        "an explicit audio-off pin must remain silent"
    );
    assert!(json["tracks"].get("subtitle").is_none());
    let rows: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "PlayoutItem""#)
        .fetch_one(&host.pool)
        .await
        .unwrap();
    assert_eq!(rows, 2);
}

#[tokio::test]
async fn changed_gap_or_policy_rejects_cached_occurrence() {
    let (host, instance, start, _root) = fixture().await;
    resolve(&host, instance, start, 90).await.unwrap();
    sqlx::query(r#"UPDATE "FillerPreset" SET "AllowWatermarks"=1 WHERE "Id"=1"#)
        .execute(&host.pool)
        .await
        .unwrap();
    assert!(resolve(&host, instance, start, 95).await.is_err());
    sqlx::query(r#"UPDATE "FillerPreset" SET "AllowWatermarks"=0 WHERE "Id"=1"#)
        .execute(&host.pool)
        .await
        .unwrap();
    make_repositories(host.pool.clone())
        .playout
        .upsert_item(&PlayoutItem {
            id: 3,
            playout_id: 1,
            media_item_id: 10,
            start: start + Duration::seconds(120),
            finish: start + Duration::seconds(130),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(resolve(&host, instance, start, 95).await.is_err());
}

#[tokio::test]
async fn cached_occurrence_rejects_changed_track_preferences_and_media_cut() {
    let (host, _instance, start, _root) = fixture().await;
    let channel = make_repositories(host.pool.clone())
        .channel
        .get_with_profile(1)
        .await
        .unwrap();
    let mut candidates = fallback_playback::load_candidates(
        &host.pool,
        &channel,
        start + Duration::seconds(90),
        start + Duration::seconds(180),
    )
    .await
    .unwrap()
    .unwrap();
    let item = candidates.materialize(0).unwrap();
    candidates.start += Duration::seconds(15);
    assert!(candidate_matches(&candidates, &item));
    candidates.items[0].preferred_audio_language_code = Some("eng".into());
    assert!(!candidate_matches(&candidates, &item));
    candidates.items[0].preferred_audio_language_code = item.preferred_audio_language_code.clone();
    candidates.items[0].subtitle_mode = Some(ChannelSubtitleMode::None);
    assert!(!candidate_matches(&candidates, &item));
    candidates.items[0].subtitle_mode = item.subtitle_mode;
    candidates.items[0].duration -= Duration::seconds(1);
    assert!(!candidate_matches(&candidates, &item));
}

#[tokio::test]
async fn ephemeral_credits_use_bound_cache_and_reject_changed_metadata() {
    let (host, instance, start, _root) = fixture().await;
    for sql in [
        r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (31, 1, 0)"#,
        r#"INSERT INTO "Artist" ("Id") VALUES (31)"#,
        r#"INSERT INTO "ArtistMetadata" ("Id", "ArtistId", "Title", "MetadataKind", "DateAdded", "DateUpdated") VALUES (32, 31, 'Synthetic Artist', 0, '2026-09-24', '2026-09-24')"#,
        r#"INSERT INTO "MusicVideo" ("Id", "ArtistId") VALUES (10, 31)"#,
        r#"UPDATE "MediaVersion" SET "MovieId"=NULL, "MusicVideoId"=10 WHERE "Id"=100"#,
        r#"DELETE FROM "Movie" WHERE "Id"=10"#,
        r#"INSERT INTO "MusicVideoMetadata" ("Id", "MusicVideoId", "Title", "MetadataKind", "DateAdded", "DateUpdated") VALUES (33, 10, 'Synthetic Credit Song', 0, '2026-09-24', '2026-09-24')"#,
    ] {
        sqlx::query(sql).execute(&host.pool).await.unwrap();
    }
    let repos = make_repositories(host.pool.clone());
    let mut channel = repos.channel.get_with_profile(1).await.unwrap();
    channel.subtitle_mode = ChannelSubtitleMode::Default;
    channel.music_video_credits_mode =
        ersatzrs_domain::ChannelMusicVideoCreditsMode::GenerateSubtitles;
    repos.channel.upsert(&channel).await.unwrap();
    let first = resolve(&host, instance, start, 90).await.unwrap();
    let tracks = first.tracks.unwrap();
    assert_eq!(tracks.audio.unwrap().stream_index, Some(2));
    let Some(protocol::Source::Local { path, .. }) = tracks.subtitle.unwrap().source else {
        panic!("ephemeral credits require a cache artifact, not a persisted-item route")
    };
    assert!(tokio::fs::read_to_string(&path)
        .await
        .unwrap()
        .contains("Synthetic Credit Song"));
    let again = resolve(&host, instance, start, 95).await.unwrap();
    let Some(protocol::Source::Local {
        path: reused,
        in_point_ms,
        ..
    }) = again.tracks.unwrap().subtitle.unwrap().source
    else {
        panic!("cached credits source must remain local")
    };
    assert_eq!(path, reused);
    assert_eq!(in_point_ms, Some(5000));
    sqlx::query(r#"UPDATE "MusicVideoMetadata" SET "Title"='Changed Credit Song' WHERE "Id"=33"#)
        .execute(&host.pool)
        .await
        .unwrap();
    assert!(resolve(&host, instance, start, 100).await.is_err());
}

async fn remote_fixture(host: &NextHost) {
    for sql in [
        r#"INSERT INTO "RemoteStream" ("Id","Url","Liveness","IsLive") VALUES (10,'https://example.test/finite',1,0)"#,
        r#"UPDATE "MediaVersion" SET "MovieId"=NULL,"RemoteStreamId"=10 WHERE "Id"=100"#,
        r#"INSERT INTO "RemoteStreamMetadata" ("Id","RemoteStreamId","MetadataKind","Title","DateAdded","DateUpdated") VALUES (30,10,0,'Synthetic','2026-09-24','2026-09-24')"#,
        r#"UPDATE "Subtitle" SET "MovieMetadataId"=NULL,"RemoteStreamMetadataId"=30"#,
        r#"DELETE FROM "MovieMetadata" WHERE "MovieId"=10"#,
        r#"DELETE FROM "Movie" WHERE "Id"=10"#,
    ] {
        sqlx::query(sql).execute(&host.pool).await.unwrap();
    }
}

#[tokio::test]
async fn remote_callbacks_keep_occurrence_selection_but_refresh_concrete_sources() {
    let (host, instance, start, _root) = fixture().await;
    remote_fixture(&host).await;
    let first = resolve(&host, instance, start, 90).await.unwrap();
    assert_eq!(first.finish, start + Duration::seconds(150));
    assert_eq!(
        first
            .tracks
            .as_ref()
            .unwrap()
            .audio
            .as_ref()
            .unwrap()
            .stream_index,
        Some(2)
    );
    let again = resolve(&host, instance, start, 105).await.unwrap();
    let value = serde_json::to_value(&again).unwrap();
    assert_eq!(value["tracks"]["video"]["source"]["in_point_ms"], 15_000);
    let state = host
        .dynamic_contexts
        .lock()
        .await
        .get(&instance)
        .unwrap()
        .fallback
        .clone();
    let guard = state.lock().await;
    let current = guard.as_ref().unwrap();
    assert!(
        current.concrete.is_none(),
        "remote transfers must not reuse a claimed concrete URL"
    );
    let current_snapshot = super::super::remote_projection::Origin::Fallback(current.gap)
        .snapshot(&host.pool, &host.locker, 1, &current.item, current.audio)
        .await
        .unwrap();
    assert_eq!(current_snapshot.identity.descriptor, current.descriptor);
    assert!(super::super::remote_projection::Origin::Persisted
        .snapshot(&host.pool, &host.locker, 1, &current.item, current.audio)
        .await
        .is_err());
    drop(guard);
    sqlx::query(r#"UPDATE "RemoteStream" SET "Url"='https://example.test/replaced' WHERE "Id"=10"#)
        .execute(&host.pool)
        .await
        .unwrap();
    assert!(resolve(&host, instance, start, 110).await.is_err());
}

#[tokio::test]
async fn ephemeral_remote_validation_refuses_schedule_changes_after_preparation() {
    let (host, instance, start, _root) = fixture().await;
    remote_fixture(&host).await;
    resolve(&host, instance, start, 90).await.unwrap();
    let state = host
        .dynamic_contexts
        .lock()
        .await
        .get(&instance)
        .unwrap()
        .fallback
        .clone();
    let guard = state.lock().await;
    let current = guard.as_ref().unwrap();
    make_repositories(host.pool.clone())
        .playout
        .upsert_item(&PlayoutItem {
            id: 3,
            playout_id: 1,
            media_item_id: 10,
            start: start + Duration::seconds(100),
            finish: start + Duration::seconds(120),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(
        super::super::remote_projection::Origin::Fallback(current.gap)
            .snapshot(&host.pool, &host.locker, 1, &current.item, current.audio)
            .await
            .is_err()
    );
}
