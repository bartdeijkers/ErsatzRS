use super::*;
use crate::next_engine::source;
use ersatzrs_core::repositories::RemoteMediaSourceKind;
use ersatzrs_domain::{
    ChannelStreamSelectorMode, ChannelSubtitleMode, MediaStream, PinnedStreamSelection,
};

fn enabled_channel() -> Channel {
    Channel {
        subtitle_mode: ChannelSubtitleMode::Any,
        ..Channel::default()
    }
}

async fn selected_id(pool: &DbPool, selected: &str) {
    sqlx::query(r#"UPDATE "MediaFile" SET "ProviderMediaSourceId"=? WHERE "Id"=1"#)
        .bind(selected)
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn remote_intent_requires_exact_selected_file_without_fabricating_readiness() {
    for provider in RemoteMediaSourceKind::ALL {
        let (pool, media) = source::tests::fixture(provider).await;
        sqlx::query(r#"UPDATE "MediaStream" SET "Codec"='dvb_teletext' WHERE "Id"=4"#)
            .execute(&pool)
            .await
            .unwrap();
        let channel = enabled_channel();
        let mut resolved = source::resolve(&pool, &media, &[]).await.unwrap();
        if provider != RemoteMediaSourceKind::Plex {
            assert!(
                RetainedSourceIntent::load(&pool, &channel, &media, &resolved)
                    .await
                    .unwrap()
                    .is_none()
            );
            selected_id(&pool, "selected-a").await;
            resolved = source::resolve(&pool, &media, &[]).await.unwrap();
        }
        let intent = RetainedSourceIntent::load(&pool, &channel, &media, &resolved)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(intent.streams.len(), 5);
        assert_eq!(
            intent
                .streams
                .iter()
                .find(|entry| entry.index == 3)
                .unwrap()
                .id,
            4
        );
        let mut changed = media.clone();
        changed.media_file_id = 2;
        assert!(!intent.matches_media(&changed));
        assert!(
            RetainedSourceIntent::load(&pool, &channel, &changed, &resolved)
                .await
                .is_err()
        );
        if provider != RemoteMediaSourceKind::Plex {
            selected_id(&pool, "selected-b").await;
            let new = source::resolve(&pool, &media, &[]).await.unwrap();
            let new = RetainedSourceIntent::load(&pool, &channel, &media, &new)
                .await
                .unwrap()
                .unwrap();
            assert_ne!(intent.identity(), new.identity());
        }
        let reports: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "SourceTeletextMetadata""#)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(reports, 0);
        pool.close().await;
    }
}

#[tokio::test]
async fn exclusions_precede_database_or_network_work_and_embedded_policy_is_respected() {
    let (pool, mut media) = source::tests::fixture(RemoteMediaSourceKind::Jellyfin).await;
    let source = source::resolve(&pool, &media, &[]).await.unwrap();
    sqlx::query(r#"INSERT INTO "ConfigElement" ("Key","Value") VALUES ('ffmpeg.use_embedded_subtitles','false')"#)
        .execute(&pool).await.unwrap();
    assert!(
        RetainedSourceIntent::load(&pool, &enabled_channel(), &media, &source)
            .await
            .unwrap()
            .is_none()
    );
    sqlx::query(r#"DELETE FROM "ConfigElement" WHERE "Key"='ffmpeg.use_embedded_subtitles'"#)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(r#"DELETE FROM "MediaStream" WHERE "MediaVersionId"=?"#)
        .bind(media.media_version_id)
        .execute(&pool)
        .await
        .unwrap();
    // Ordinary HTTP inputs need no embedded layout to decline caption work.
    assert!(
        RetainedSourceIntent::load(&pool, &enabled_channel(), &media, &source)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        RetainedSourceIntent::discover(&pool, &enabled_channel(), &media, &source)
            .await
            .is_none()
    );
    pool.close().await;
    assert!(
        RetainedSourceIntent::load(&pool, &Channel::default(), &media, &source)
            .await
            .unwrap()
            .is_none()
    );
    let custom = Channel {
        stream_selector_mode: ChannelStreamSelectorMode::Custom,
        ..enabled_channel()
    };
    assert!(RetainedSourceIntent::load(&pool, &custom, &media, &source)
        .await
        .unwrap()
        .is_none());
    let pin = PinnedStreamSelection {
        media_version_id: media.media_version_id,
        video_stream_index: 0,
        audio_stream_index: Some(1),
        subtitle_stream_index: None,
    };
    media.item.pinned_stream_selection = Some(pin);
    assert!(
        RetainedSourceIntent::load(&pool, &enabled_channel(), &media, &source)
            .await
            .unwrap()
            .is_none()
    );
    media.item.pinned_stream_selection = None;
    media.remote_stream_id = Some(1);
    assert!(
        RetainedSourceIntent::load(&pool, &enabled_channel(), &media, &source)
            .await
            .unwrap()
            .is_none()
    );
    media.remote_stream_id = None;
    let mut live = source;
    if let Source::Http { is_live, .. } = &mut live {
        *is_live = Some(true);
    }
    assert!(
        RetainedSourceIntent::load(&pool, &enabled_channel(), &media, &live)
            .await
            .unwrap()
            .is_none()
    );
}

fn native() -> FfprobeMediaVersion {
    FfprobeMediaVersion {
        version: ersatzrs_domain::MediaVersion::default(),
        streams: vec![
            MediaStream {
                index: 0,
                kind: MediaStreamKind::Video,
                codec: Some("h264".into()),
                ..MediaStream::default()
            },
            MediaStream {
                index: 1,
                kind: MediaStreamKind::Audio,
                codec: Some("aac".into()),
                language: Some("nld".into()),
                title: Some("Dutch".into()),
                default: true,
                channels: Some(2),
                ..MediaStream::default()
            },
            MediaStream {
                index: 2,
                kind: MediaStreamKind::Subtitle,
                codec: Some("dvb_teletext".into()),
                ..MediaStream::default()
            },
        ],
        field_orders: std::collections::BTreeMap::default(),
        chapters: vec![],
        format_metadata: std::collections::BTreeMap::default(),
        format_name: Some("mpegts".into()),
    }
}

fn stored(native: &FfprobeMediaVersion) -> Vec<Stream> {
    native
        .streams
        .iter()
        .map(|stream| Stream {
            id: i64::from(stream.index) + 1,
            index: stream.index,
            kind: kind_code(stream.kind),
            codec: stream.codec.clone(),
            language: stream.language.clone(),
            title: stream.title.clone(),
            default: stream.default,
            forced: stream.forced,
            channels: stream.channels,
            attached_pic: stream.attached_pic,
        })
        .collect()
}

#[test]
fn qualification_requires_full_native_layout_and_selected_audio_semantics() {
    let native = native();
    let mut stored = stored(&native);
    stored[1].language = Some("dut".into());
    assert!(validate_layout(&stored, &native).is_ok());
    let mut variations = Vec::new();
    let mut changed = native.clone();
    changed.streams.pop();
    variations.push(changed);
    let mut changed = native.clone();
    changed.streams[2].index = 1;
    variations.push(changed);
    let mut changed = native.clone();
    changed.streams[2].kind = MediaStreamKind::Audio;
    variations.push(changed);
    let mut changed = native.clone();
    changed.streams[2].codec = Some("subrip".into());
    variations.push(changed);
    let mut changed = native.clone();
    changed.streams[1].language = Some("eng".into());
    variations.push(changed);
    let mut changed = native.clone();
    changed.streams[1].title = Some("English".into());
    variations.push(changed);
    let mut changed = native.clone();
    changed.streams[1].default = false;
    variations.push(changed);
    let mut changed = native.clone();
    changed.streams[1].forced = true;
    variations.push(changed);
    let mut changed = native.clone();
    changed.streams[1].channels = Some(1);
    variations.push(changed);
    let mut changed = native.clone();
    changed.streams[0].attached_pic = true;
    variations.push(changed);
    for changed in variations {
        assert!(validate_layout(&stored, &changed).is_err());
    }
}
