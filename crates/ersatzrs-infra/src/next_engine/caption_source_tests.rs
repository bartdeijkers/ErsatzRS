use super::*;

#[path = "provider_caption_source_tests.rs"]
mod provider;

struct Fixture {
    host: std::sync::Arc<NextHost>,
    encoded: String,
    signature: String,
    caption: std::path::PathBuf,
    root: tempfile::TempDir,
}

async fn fixture() -> Fixture {
    fixture_for(1).await
}

async fn current_schedule(pool: &crate::db::DbPool) -> DateTime<Utc> {
    use crate::repositories::common::encode_datetime;
    // Grants are time bounded; the shared historical schedule is moved into
    // this trial's clock without changing its two consecutive occurrences.
    let start = DateTime::from_timestamp(Utc::now().timestamp(), 0).unwrap();
    for (id, offset) in [(1, 0), (2, 60)] {
        sqlx::query(r#"UPDATE "PlayoutItem" SET "Start"=?, "Finish"=?, "GuideStart"=?, "GuideFinish"=?, "InPoint"='00:00:01', "OutPoint"='00:01:01' WHERE "Id"=?"#)
            .bind(encode_datetime(start + chrono::Duration::seconds(offset)))
            .bind(encode_datetime(start + chrono::Duration::seconds(offset + 60)))
            .bind(encode_datetime(start + chrono::Duration::seconds(offset)))
            .bind(encode_datetime(start + chrono::Duration::seconds(offset + 60)))
            .bind(id)
            .execute(pool).await.unwrap();
    }
    start
}

async fn fixture_for(channel_id: i64) -> Fixture {
    let root = tempfile::tempdir().unwrap();
    let (host, pool, locker, _) = crate::next_engine::host::publication_fixture(root.path()).await;
    let start = current_schedule(&pool).await;
    let clip = root.path().join("video.mkv");
    std::fs::write(&clip, b"synthetic metadata fixture").unwrap();
    let caption = root.path().join("caption.srt");
    std::fs::write(
        &caption,
        "1\n00:00:05,000 --> 00:00:10,000\nSynthetic caption\n",
    )
    .unwrap();
    sqlx::query(r#"UPDATE "MediaFile" SET "Path"=? WHERE "Id"=1"#)
        .bind(clip.to_str().unwrap())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "Subtitle" SET "Path"=? WHERE "Id"=2"#)
        .bind(caption.to_str().unwrap())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "Channel" SET "SubtitleMode"=2,"PreferredSubtitleLanguageCode"='nld' WHERE "Id" IN (1,2)"#).execute(&pool).await.unwrap();
    let channel = crate::repositories::make_repositories(pool.clone())
        .channel
        .get_with_profile(channel_id)
        .await
        .unwrap();
    let snapshot = schedule::read_locked(
        &pool,
        &locker,
        &channel,
        start,
        start + chrono::Duration::minutes(2),
    )
    .await
    .unwrap()
    .unwrap();
    let media = snapshot
        .items
        .iter()
        .find(|media| media.item.id == 1)
        .unwrap();
    let source =
        crate::next_engine::source::resolve_published(&pool, media, &[], Some(&host.media))
            .await
            .unwrap();
    let mut projected = selection::project_scheduled_item(
        &pool,
        &channel,
        media,
        source,
        &ersatzrs_domain::Resolution {
            width: 320,
            height: 180,
            ..Default::default()
        },
        selection::AudioDirective::Select,
    )
    .await
    .unwrap();
    host.publish_local_caption(&channel, media, &mut projected, None)
        .await
        .unwrap();
    let Source::Http {
        uri,
        in_point_ms,
        out_point_ms,
        ..
    } = projected
        .item
        .tracks
        .unwrap()
        .subtitle
        .unwrap()
        .source
        .unwrap()
    else {
        panic!("caption must use the scoped relay");
    };
    assert_eq!(in_point_ms, Some(1000));
    assert_eq!(out_point_ms, Some(61_000));
    let url = url::Url::parse(&uri).unwrap();
    let parts: Vec<_> = url.path_segments().unwrap().collect();
    let (encoded, signature) = (parts[2].to_owned(), parts[3].to_owned());
    let payload = String::from_utf8(BASE64.decode(&encoded).unwrap()).unwrap();
    assert!(
        !payload.contains(caption.to_str().unwrap()),
        "grant must contain no source path"
    );
    drop(snapshot);
    Fixture {
        host,
        encoded,
        signature,
        caption,
        root,
    }
}

#[tokio::test]
async fn caption_grant_reads_exact_text_and_uses_matching_empty_on_automatic_disappearance() {
    for change in ["file", "path", "row"] {
        let f = fixture().await;
        assert!(f
            .host
            .read_caption(&f.encoded, &f.signature)
            .await
            .unwrap()
            .text
            .contains("Synthetic caption"));
        match change {
            "file" => std::fs::remove_file(&f.caption).unwrap(),
            "path" => {
                sqlx::query(r#"UPDATE "Subtitle" SET "Path"=NULL WHERE "Id"=2"#)
                    .execute(&f.host.pool)
                    .await
                    .unwrap();
            }
            _ => {
                sqlx::query(r#"DELETE FROM "Subtitle" WHERE "Id"=2"#)
                    .execute(&f.host.pool)
                    .await
                    .unwrap();
            }
        }
        let empty = f.host.read_caption(&f.encoded, &f.signature).await.unwrap();
        assert_eq!(empty.content_type, Format::Srt.content_type());
        assert_eq!(empty.text, Format::Srt.empty().text);
    }
}

fn signed(f: &Fixture, grant: &Grant) -> (String, String) {
    let uri = f.host.media.encode_caption(grant).unwrap();
    let url = url::Url::parse(&uri).unwrap();
    let parts: Vec<_> = url.path_segments().unwrap().collect();
    (parts[2].into(), parts[3].into())
}

#[tokio::test]
async fn caption_grants_refuse_tampering_expiry_unknown_fields_and_cross_host_before_storage() {
    let f = fixture().await;
    let mut expired = f
        .host
        .media
        .decode_caption(&f.encoded, &f.signature)
        .unwrap();
    expired.expires = Utc::now().timestamp() - 1;
    let expired = signed(&f, &expired);
    let mut extended: serde_json::Value =
        serde_json::from_slice(&BASE64.decode(&f.encoded).unwrap()).unwrap();
    extended["extra"] = serde_json::json!(true);
    let extended = BASE64.encode(serde_json::to_vec(&extended).unwrap());
    let signature = BASE64.encode(
        f.host
            .media
            .caption_mac(&extended)
            .unwrap()
            .finalize()
            .into_bytes(),
    );
    f.host.pool.close().await;
    // A worker can see the general media bearer. It must never be able to
    // sign caption grants using that bearer, in either wire representation.
    for key in [
        f.host.media.capability.as_bytes().to_vec(),
        f.host.media.capability.to_string().into_bytes(),
    ] {
        let mut forged = Hmac::<Sha256>::new_from_slice(&key).unwrap();
        forged.update(DOMAIN);
        forged.update(f.encoded.as_bytes());
        let signature = BASE64.encode(forged.finalize().into_bytes());
        assert_eq!(
            f.host.read_caption(&f.encoded, &signature).await.err(),
            Some(CaptionError::Unauthorized)
        );
    }
    for (encoded, signature) in [
        (&*f.encoded, "invalid"),
        (&*f.encoded, &*f.encoded),
        (&*expired.0, &*expired.1),
        (&*extended, &*signature),
        ("bad", "bad"),
    ] {
        assert_eq!(
            f.host.read_caption(encoded, signature).await.err(),
            Some(CaptionError::Unauthorized)
        );
    }
    let other = MediaRelay::new();
    assert_eq!(
        other.decode_caption(&f.encoded, &f.signature).err(),
        Some(CaptionError::Unauthorized)
    );
}

#[tokio::test]
async fn caption_grants_revoke_stopped_owners_and_changed_media_without_grace() {
    for change in ["owner", "replacement-owner", "file", "item", "engine"] {
        let f = fixture().await;
        match change {
            "owner" => f.host.dynamic_contexts.lock().await.clear(),
            "replacement-owner" => {
                let mut contexts = f.host.dynamic_contexts.lock().await;
                *contexts = std::mem::take(&mut *contexts)
                    .into_values()
                    .map(|context| (Uuid::new_v4(), context))
                    .collect();
            }
            "file" => {
                sqlx::query(r#"UPDATE "MediaFile" SET "Path"='different-source.mkv' WHERE "Id"=1"#)
                    .execute(&f.host.pool)
                    .await
                    .unwrap();
            }
            "item" => {
                sqlx::query(r#"DELETE FROM "PlayoutItem" WHERE "Id"=1"#)
                    .execute(&f.host.pool)
                    .await
                    .unwrap();
            }
            _ => {
                sqlx::query(r#"UPDATE "Channel" SET "StreamingEngine"=0 WHERE "Id"=1"#)
                    .execute(&f.host.pool)
                    .await
                    .unwrap();
            }
        }
        let error = f
            .host
            .read_caption(&f.encoded, &f.signature)
            .await
            .err()
            .unwrap();
        assert!(matches!(
            error,
            CaptionError::Unauthorized | CaptionError::Binding
        ));
    }
}

#[tokio::test]
async fn caption_grants_follow_the_output_mirror_and_revoke_changed_source_relationships() {
    let f = fixture_for(2).await;
    sqlx::query(r#"UPDATE "Channel" SET "StreamingEngine"=0 WHERE "Id"=1"#)
        .execute(&f.host.pool)
        .await
        .unwrap();
    assert!(f
        .host
        .read_caption(&f.encoded, &f.signature)
        .await
        .unwrap()
        .text
        .contains("Synthetic caption"));
    sqlx::query(r#"UPDATE "Subtitle" SET "Path"=NULL WHERE "Id"=2"#)
        .execute(&f.host.pool)
        .await
        .unwrap();
    assert_eq!(
        f.host
            .read_caption(&f.encoded, &f.signature)
            .await
            .unwrap()
            .text,
        Format::Srt.empty().text
    );
    sqlx::query(r#"UPDATE "Channel" SET "MirrorSourceChannelId"=NULL WHERE "Id"=2"#)
        .execute(&f.host.pool)
        .await
        .unwrap();
    assert_eq!(
        f.host.read_caption(&f.encoded, &f.signature).await.err(),
        Some(CaptionError::Binding)
    );
}

#[tokio::test]
async fn caption_grants_preserve_strict_selection_charset_and_read_limits() {
    let f = fixture().await;
    let mut strict = f
        .host
        .media
        .decode_caption(&f.encoded, &f.signature)
        .unwrap();
    strict.soft = false;
    let strict = signed(&f, &strict);
    let charset = f.caption.with_extension("srt.charset");
    std::fs::write(&charset, "windows-1252").unwrap();
    std::fs::write(
        &f.caption,
        b"1\n00:00:01,000 --> 00:00:02,000\nFran\xe7ais\n",
    )
    .unwrap();
    assert!(f
        .host
        .read_caption(&f.encoded, &f.signature)
        .await
        .unwrap()
        .text
        .contains("Fran\u{e7}ais"));
    std::fs::write(&charset, "invalid-charset").unwrap();
    assert_eq!(
        f.host.read_caption(&f.encoded, &f.signature).await.err(),
        Some(CaptionError::Source)
    );
    std::fs::remove_file(&charset).unwrap();
    std::fs::File::create(&f.caption)
        .unwrap()
        .set_len(u64::try_from(crate::subtitle_text::MAX_TEXT_BYTES + 1).unwrap())
        .unwrap();
    assert_eq!(
        f.host.read_caption(&f.encoded, &f.signature).await.err(),
        Some(CaptionError::Source)
    );
    std::fs::remove_file(&f.caption).unwrap();
    assert_eq!(
        f.host.read_caption(&strict.0, &strict.1).await.err(),
        Some(CaptionError::Source)
    );
}
