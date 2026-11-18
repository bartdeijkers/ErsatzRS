//! The caption grant never replaces the prepared AV publication's authority.
use super::*;
use crate::next_engine::{protocol, DynamicItemRequest};

pub(super) async fn verify(
    host: &host::NextHost,
    published: &serde_json::Value,
    clip: &Path,
    caption: &Path,
) {
    let item: protocol::PlayoutItem = serde_json::from_value(published.clone()).unwrap();
    let Some(protocol::Source::Dynamic { uri, .. }) = &item.source else {
        panic!("prepared item must use a callback");
    };
    let parts: Vec<_> = uri.split('/').collect();
    let digest = parts[parts.len() - 2];
    assert!(digest.starts_with("caption."));
    let (instance, request) = host
        .dynamic_contexts
        .lock()
        .await
        .iter()
        .next()
        .map(|(id, context)| (*id, context.request.clone()))
        .unwrap();
    let callback = DynamicItemRequest {
        channel_id: request.channel_id,
        item_id: parts[parts.len() - 3].parse().unwrap(),
        digest,
        audio: Some(2),
        number: &request.number,
        instance,
        now: item.start + ersatzrs_time::Duration::seconds(1),
        until: item.finish,
    };
    let ready = resolve(host, callback).await.unwrap();
    let tracks = ready.tracks.unwrap();
    assert_eq!(tracks.audio.unwrap().stream_index, Some(2));
    let Some(protocol::Source::Http { uri, .. }) = tracks.subtitle.unwrap().source else {
        panic!("prepared caption must use the bounded relay");
    };
    let uri = url::Url::parse(&uri).unwrap();
    let parts: Vec<_> = uri.path_segments().unwrap().collect();
    let (encoded, signature) = (parts[2], parts[3]);
    assert!(host
        .read_caption(encoded, signature)
        .await
        .unwrap()
        .text
        .contains("Next text"));
    database_fences(host, callback, encoded, signature).await;
    let renamed = clip.with_extension("temporarily-unavailable");
    std::fs::rename(clip, &renamed).unwrap();
    refused(host, callback, encoded, signature).await;
    std::fs::rename(&renamed, clip).unwrap();
    let text = std::fs::read(caption).unwrap();
    std::fs::write(caption, b"unsafe\0caption").unwrap();
    assert!(host.read_caption(encoded, signature).await.is_err());
    std::fs::write(caption, text).unwrap();
    sqlx::query(r#"UPDATE "Subtitle" SET "Id"="Id"+1000 WHERE "SubtitleKind"=1"#)
        .execute(&host.pool)
        .await
        .unwrap();
    assert!(!host
        .read_caption(encoded, signature)
        .await
        .unwrap()
        .text
        .contains("Next text"));
    assert!(resolve(host, callback).await.is_ok());
    sqlx::query(r#"UPDATE "Subtitle" SET "Id"="Id"-1000 WHERE "SubtitleKind"=1"#)
        .execute(&host.pool)
        .await
        .unwrap();
    assert!(resolve(
        host,
        DynamicItemRequest {
            instance: uuid::Uuid::new_v4(),
            ..callback
        }
    )
    .await
    .is_err());
    assert!(resolve(
        host,
        DynamicItemRequest {
            audio: Some(1),
            ..callback
        }
    )
    .await
    .is_err());
    let context = host
        .dynamic_contexts
        .lock()
        .await
        .remove(&instance)
        .unwrap();
    refused(host, callback, encoded, signature).await;
    host.dynamic_contexts.lock().await.insert(instance, context);
    assert!(resolve(host, callback).await.is_ok());
    println!("row292 prepared authority: six SQL fences, missing AV, malformed caption, replacement ID, foreign owner/audio and owner revocation verified");
}

async fn database_fences(
    host: &host::NextHost,
    callback: DynamicItemRequest<'_>,
    encoded: &str,
    signature: &str,
) {
    for (query, update, replacement) in [
        (
            r#"SELECT "DateUpdated" FROM "MediaVersion" WHERE "Id"=100"#,
            r#"UPDATE "MediaVersion" SET "DateUpdated"=? WHERE "Id"=100"#,
            "2026-11-16T00:00:00Z",
        ),
        (
            r#"SELECT "PathHash" FROM "MediaFile" WHERE "Id"=1"#,
            r#"UPDATE "MediaFile" SET "PathHash"=? WHERE "Id"=1"#,
            "replacement",
        ),
        (
            r#"SELECT CAST("ByteLength" AS TEXT) FROM "AddonMediaListArtifact" WHERE "Id"=1"#,
            r#"UPDATE "AddonMediaListArtifact" SET "ByteLength"=? WHERE "Id"=1"#,
            "1",
        ),
        (
            r#"SELECT "RelativePath" FROM "AddonMediaListArtifact" WHERE "Id"=1"#,
            r#"UPDATE "AddonMediaListArtifact" SET "RelativePath"=? WHERE "Id"=1"#,
            "objects/missing.media",
        ),
        (
            r#"SELECT CAST("PublishedRevision" AS TEXT) FROM "AddonMediaList" WHERE "Id"=1"#,
            r#"UPDATE "AddonMediaList" SET "PublishedRevision"=? WHERE "Id"=1"#,
            "2",
        ),
        (
            r#"SELECT "Url" FROM "RemoteStream" WHERE "Id"=10"#,
            r#"UPDATE "RemoteStream" SET "Url"=? WHERE "Id"=10"#,
            "https://example.test/replaced",
        ),
    ] {
        let original: String = sqlx::query_scalar(query)
            .fetch_one(&host.pool)
            .await
            .unwrap();
        sqlx::query(update)
            .bind(replacement)
            .execute(&host.pool)
            .await
            .unwrap();
        refused(host, callback, encoded, signature).await;
        sqlx::query(update)
            .bind(original)
            .execute(&host.pool)
            .await
            .unwrap();
        assert!(resolve(host, callback).await.is_ok());
    }
}

async fn refused(
    host: &host::NextHost,
    callback: DynamicItemRequest<'_>,
    encoded: &str,
    signature: &str,
) {
    assert!(resolve(host, callback).await.is_err());
    assert!(host.read_caption(encoded, signature).await.is_err());
}

async fn resolve(
    host: &host::NextHost,
    callback: DynamicItemRequest<'_>,
) -> ersatzrs_core::repositories::Result<protocol::PlayoutItem> {
    host.resolve_dynamic(
        callback.channel_id,
        callback.item_id,
        callback.digest,
        callback.audio,
        callback.number,
        callback.instance,
        callback.now,
        callback.until,
    )
    .await
}
