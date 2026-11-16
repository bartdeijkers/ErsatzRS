use super::*;
use crate::next_engine::{dynamic_projection, host::NextStart, protocol, DynamicItemRequest};
use ersatzrs_core::repositories::RemoteMediaSourceKind;

struct ProviderFixture {
    inner: Fixture,
    descriptor: String,
    instance: Uuid,
    identity: dynamic_projection::Identity,
    item: protocol::PlayoutItem,
    request: NextStart,
}

impl ProviderFixture {
    fn callback(&self) -> DynamicItemRequest<'_> {
        let grant = self
            .inner
            .host
            .media
            .decode_caption(&self.inner.encoded, &self.inner.signature)
            .unwrap();
        DynamicItemRequest {
            channel_id: grant.channel,
            item_id: 1,
            digest: &self.descriptor,
            audio: Some(2),
            number: &self.request.number,
            instance: self.instance,
            now: grant.start + chrono::Duration::seconds(1),
            until: grant.finish,
        }
    }

    async fn request(&self) -> NextStart {
        self.inner
            .host
            .dynamic_contexts
            .lock()
            .await
            .get(&self.instance)
            .unwrap()
            .request
            .clone()
    }

    async fn snapshot(&self) -> dynamic_projection::Snapshot {
        let callback = self.callback();
        self.inner
            .host
            .project_session_snapshot(
                &self.request().await,
                self.instance,
                callback.item_id,
                callback.now,
                callback.until,
                Some(2),
            )
            .await
            .unwrap()
    }
}

async fn provider_fixture(kind: RemoteMediaSourceKind, address: Option<&str>) -> ProviderFixture {
    provider_fixture_for(kind, address, 1).await
}

async fn provider_fixture_for(
    kind: RemoteMediaSourceKind,
    address: Option<&str>,
    channel_id: i64,
) -> ProviderFixture {
    let mut inner = fixture_for(channel_id).await;
    let local = inner
        .host
        .media
        .decode_caption(&inner.encoded, &inner.signature)
        .unwrap();
    let pool = &inner.host.pool;
    sqlx::query(r#"DELETE FROM "LocalLibrary""#)
        .execute(pool)
        .await
        .unwrap();
    crate::next_engine::source::tests::seed_provider(pool, kind).await;
    if kind == RemoteMediaSourceKind::Plex {
        sqlx::query(r#"INSERT INTO "PlexMovie" ("Id") VALUES (10)"#)
            .execute(pool)
            .await
            .unwrap();
    }
    let path = match kind {
        RemoteMediaSourceKind::Plex => Some("/library/streams/123"),
        RemoteMediaSourceKind::Jellyfin => None,
        RemoteMediaSourceKind::Emby => Some("source-id"),
    };
    sqlx::query(r#"UPDATE "Subtitle" SET "Path"=? WHERE "Id"=2"#)
        .bind(path)
        .execute(pool)
        .await
        .unwrap();
    if let Some(address) = address {
        configure_provider_connection(pool, kind, address).await;
    }
    let (instance, request) = inner
        .host
        .dynamic_contexts
        .lock()
        .await
        .iter()
        .find(|(_, context)| context.request.channel_id == channel_id)
        .map(|(id, context)| (*id, context.request.clone()))
        .unwrap();
    let snapshot = Box::pin(inner.host.project_session_snapshot(
        &request,
        instance,
        1,
        local.start,
        local.finish,
        Some(2),
    ))
    .await
    .unwrap();
    let channel = crate::repositories::make_repositories(pool.clone())
        .channel
        .get_with_profile(channel_id)
        .await
        .unwrap();
    let locked = schedule::read_locked(
        pool,
        &inner.host.locker,
        &channel,
        local.start,
        local.finish,
    )
    .await
    .unwrap()
    .unwrap();
    let media = locked
        .items
        .iter()
        .find(|media| media.item.id == 1)
        .unwrap();
    let descriptor = inner
        .host
        .publish_provider_caption(channel_id, media, &snapshot.projection, &snapshot.identity)
        .await
        .unwrap()
        .expect("automatic provider caption grant");
    let (encoded, signature) = descriptor
        .strip_prefix("caption.")
        .unwrap()
        .split_once('.')
        .unwrap();
    inner.encoded = encoded.into();
    inner.signature = signature.into();
    drop(locked);
    ProviderFixture {
        inner,
        descriptor,
        instance,
        identity: snapshot.identity,
        item: snapshot.projection.item,
        request,
    }
}

#[tokio::test]
async fn provider_caption_grants_preserve_a_next_preview_mirror_of_a_native_source() {
    let f = provider_fixture_for(RemoteMediaSourceKind::Emby, None, 2).await;
    assert!(f.request.preview);
    sqlx::raw_sql(
        r#"UPDATE "Channel" SET "StreamingEngine"=0 WHERE "Id"=1;
        DELETE FROM "Subtitle" WHERE "Id"=2;"#,
    )
    .execute(&f.inner.host.pool)
    .await
    .unwrap();
    assert!(f
        .inner
        .host
        .resolve_caption_publication(&f.callback())
        .await
        .unwrap()
        .is_some());
    assert_eq!(
        f.inner
            .host
            .read_caption(&f.inner.encoded, &f.inner.signature)
            .await
            .unwrap()
            .text,
        Format::Srt.empty().text
    );
    sqlx::query(r#"UPDATE "Channel" SET "MirrorSourceChannelId"=NULL WHERE "Id"=2"#)
        .execute(&f.inner.host.pool)
        .await
        .unwrap();
    assert!(f
        .inner
        .host
        .resolve_caption_publication(&f.callback())
        .await
        .is_err());
    assert!(f
        .inner
        .host
        .read_caption(&f.inner.encoded, &f.inner.signature)
        .await
        .is_err());
}

#[tokio::test]
async fn automatic_provider_caption_changes_preserve_exact_av_and_never_select_replacement_ids() {
    for kind in RemoteMediaSourceKind::ALL {
        for replacement in [false, true] {
            let f = provider_fixture(kind, None).await;
            let sql = if replacement {
                r#"UPDATE "Subtitle" SET "Id"=200 WHERE "Id"=2"#
            } else {
                r#"DELETE FROM "Subtitle" WHERE "Id"=2"#
            };
            sqlx::query(sql).execute(&f.inner.host.pool).await.unwrap();
            let current = f.snapshot().await;
            assert_ne!(current.identity.descriptor, f.identity.descriptor);
            assert_eq!(current.identity.av_descriptor, f.identity.av_descriptor);
            let callback = f.callback();
            let resolved = f
                .inner
                .host
                .resolve_caption_publication(&callback)
                .await
                .unwrap()
                .unwrap();
            let expected =
                crate::next_engine::dynamic_item::resolve_at(&f.item, callback.now).unwrap();
            let resolved = serde_json::to_value(resolved).unwrap();
            let expected = serde_json::to_value(expected).unwrap();
            for track in ["video", "audio"] {
                assert_eq!(resolved["tracks"][track], expected["tracks"][track]);
            }
            assert_eq!(
                resolved["tracks"]["subtitle"]["source"]["in_point_ms"],
                2000
            );
            assert_eq!(resolved["tracks"]["audio"]["stream_index"], 2);
            let empty = f
                .inner
                .host
                .read_caption(&f.inner.encoded, &f.inner.signature)
                .await
                .unwrap();
            assert_eq!(empty.text, Format::Srt.empty().text);
        }
    }
}

#[tokio::test]
async fn provider_av_authority_refuses_changed_sources_revision_audio_timing_owner_server_and_pins()
{
    for change in [
        "source", "revision", "audio", "policy", "timing", "owner", "server", "pin",
    ] {
        let f = provider_fixture(RemoteMediaSourceKind::Emby, None).await;
        let sql = match change {
            "source" => r#"UPDATE "MediaFile" SET "Path"='changed-source.mkv' WHERE "Id"=1"#,
            "revision" => {
                r#"UPDATE "MediaVersion" SET "DateUpdated"='2026-11-17 12:00:00' WHERE "Id"=100"#
            }
            "audio" => r#"UPDATE "Channel" SET "PreferredAudioLanguageCode"='eng' WHERE "Id"=1"#,
            "policy" => {
                r#"UPDATE "Channel" SET "PreferredSubtitleLanguageCode"='eng' WHERE "Id"=1"#
            }
            "timing" => r#"UPDATE "PlayoutItem" SET "InPoint"='00:00:02' WHERE "Id"=1"#,
            "server" => {
                r#"UPDATE "EmbyConnection" SET "Address"='https://different.example.test' WHERE "Id"=1"#
            }
            "pin" => {
                r#"UPDATE "PlayoutItem" SET "MediaVersionId"=100,"VideoStreamIndex"=0,"AudioStreamIndex"=2,"SubtitleStreamIndex"=9 WHERE "Id"=1"#
            }
            _ => {
                f.inner.host.dynamic_contexts.lock().await.clear();
                "SELECT 1"
            }
        };
        sqlx::query(sql).execute(&f.inner.host.pool).await.unwrap();
        assert!(
            f.inner
                .host
                .resolve_caption_publication(&f.callback())
                .await
                .is_err(),
            "{change}"
        );
        assert!(
            f.inner
                .host
                .read_caption(&f.inner.encoded, &f.inner.signature)
                .await
                .is_err(),
            "{change}"
        );
    }
}

#[tokio::test]
async fn provider_callbacks_refuse_local_grants_and_foreign_owner_audio_number_time_or_tags() {
    let f = fixture().await;
    let descriptor = format!("caption.{}.{}", f.encoded, f.signature);
    let grant = f
        .host
        .media
        .decode_caption(&f.encoded, &f.signature)
        .unwrap();
    let instance = *f.host.dynamic_contexts.lock().await.keys().next().unwrap();
    let callback = DynamicItemRequest {
        channel_id: 1,
        item_id: 1,
        digest: &descriptor,
        audio: None,
        number: "1",
        instance,
        now: grant.start,
        until: grant.finish,
    };
    assert!(f.host.resolve_caption_publication(&callback).await.is_err());
    let f = provider_fixture(RemoteMediaSourceKind::Emby, None).await;
    for change in ["owner", "audio", "number", "time", "tag"] {
        let altered = format!("{}x", f.descriptor);
        let mut callback = f.callback();
        match change {
            "owner" => callback.instance = Uuid::new_v4(),
            "audio" => callback.audio = Some(1),
            "number" => callback.number = "2",
            "time" => callback.now = callback.until,
            _ => callback.digest = &altered,
        }
        assert!(
            f.inner
                .host
                .resolve_caption_publication(&callback)
                .await
                .is_err(),
            "{change}"
        );
    }
}

#[tokio::test]
async fn provider_caption_authentication_and_invalid_unicode_stay_errors() {
    for status in [401, 403, 500, 200] {
        let server = httpmock::MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(httpmock::Method::GET);
                then.status(status).body(vec![0xff]);
            })
            .await;
        let f = provider_fixture(RemoteMediaSourceKind::Emby, Some(&server.base_url())).await;
        assert_eq!(
            f.inner
                .host
                .read_caption(&f.inner.encoded, &f.inner.signature)
                .await
                .err(),
            Some(CaptionError::Source),
            "{status}"
        );
        assert_eq!(mock.calls_async().await, 1);
    }
}

#[tokio::test]
async fn provider_caption_artifact_reuse_stays_independent_from_occurrence_authority() {
    let server = httpmock::MockServer::start_async().await;
    let mock = server
        .mock_async(|when, then| {
            when.method(httpmock::Method::GET);
            then.status(200)
                .body("1\n00:00:01,000 --> 00:00:10,000\nReusable provider caption\n");
        })
        .await;
    let f = provider_fixture(RemoteMediaSourceKind::Emby, Some(&server.base_url())).await;
    let first = f
        .inner
        .host
        .media
        .decode_caption(&f.inner.encoded, &f.inner.signature)
        .unwrap();
    assert_eq!(first.media_binding, f.identity.cache_key);
    assert!(f
        .inner
        .host
        .read_caption(&f.inner.encoded, &f.inner.signature)
        .await
        .unwrap()
        .text
        .contains("Reusable provider caption"));
    let second = repeated_grant(&f).await;
    assert_eq!(first.media_binding, second.media_binding);
    assert_ne!(
        first.authority, second.authority,
        "AV admission still binds each occurrence"
    );
    let uri = f.inner.host.media.encode_caption(&second).unwrap();
    let url = url::Url::parse(&uri).unwrap();
    let parts: Vec<_> = url.path_segments().unwrap().collect();
    assert!(f
        .inner
        .host
        .read_caption(parts[2], parts[3])
        .await
        .unwrap()
        .text
        .contains("Reusable provider caption"));
    let artifacts = std::fs::read_dir(f.inner.root.path().join("burn-subtitles"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "srt"))
        .count();
    assert_eq!(
        artifacts, 1,
        "repeated occurrences must share the existing content-addressed artifact"
    );
    assert_eq!(
        mock.calls_async().await,
        2,
        "each admitted read still authenticates its provider fetch"
    );
}

async fn repeated_grant(f: &ProviderFixture) -> Grant {
    let first = f
        .inner
        .host
        .media
        .decode_caption(&f.inner.encoded, &f.inner.signature)
        .unwrap();
    let start = first.finish;
    let finish = start + chrono::Duration::minutes(1);
    let snapshot = Box::pin(f.inner.host.project_session_snapshot(
        &f.request().await,
        f.instance,
        2,
        start,
        finish,
        Some(2),
    ))
    .await
    .unwrap();
    let channel = crate::repositories::make_repositories(f.inner.host.pool.clone())
        .channel
        .get_with_profile(1)
        .await
        .unwrap();
    let locked = schedule::read_locked(
        &f.inner.host.pool,
        &f.inner.host.locker,
        &channel,
        start,
        finish,
    )
    .await
    .unwrap()
    .unwrap();
    let media = locked
        .items
        .iter()
        .find(|media| media.item.id == 2)
        .unwrap();
    let descriptor = f
        .inner
        .host
        .publish_provider_caption(1, media, &snapshot.projection, &snapshot.identity)
        .await
        .unwrap()
        .unwrap();
    let (encoded, signature) = descriptor
        .strip_prefix("caption.")
        .unwrap()
        .split_once('.')
        .unwrap();
    f.inner
        .host
        .media
        .decode_caption(encoded, signature)
        .unwrap()
}

async fn configure_provider_connection(
    pool: &crate::db::DbPool,
    kind: RemoteMediaSourceKind,
    address: &str,
) {
    let provider = kind.provider_name();
    let column = if kind == RemoteMediaSourceKind::Plex {
        "Uri"
    } else {
        "Address"
    };
    let sql = format!(r#"UPDATE "{provider}Connection" SET "{column}"=? WHERE "Id"=1"#);
    sqlx::query(crate::db::assert_sql_safe(&sql))
        .bind(address)
        .execute(pool)
        .await
        .unwrap();
    if kind != RemoteMediaSourceKind::Plex {
        sqlx::query(r#"INSERT INTO "ConfigElement" ("Key","Value") VALUES (?,'<api-key>')"#)
            .bind(format!("{}.api_key.{address}", provider.to_lowercase()))
            .execute(pool)
            .await
            .unwrap();
    }
}
