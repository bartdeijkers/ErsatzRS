use super::*;
use crate::addon_media_list_saved::{
    capture, model::*, projection, receiving_artwork::ArtworkStaging,
};
use ersatzrs_core::repositories::AddonMediaListOptionSchemaRef;
use ersatzrs_domain::{AddonMediaListItemKind, RemoteStreamLiveness};

fn schema(key: &str) -> SavedOptionSchema {
    let definition: AddonCapability = serde_json::from_value(serde_json::json!({
        "id":"media-list.options.v1", "options":[{
            "key":key, "label":{"en-US":"Fixture"},
            "description":{"en-US":"Fixture option"}, "scope":"both",
            "operation_targets":["test_access"], "applicability":"finite_only",
            "kind":"boolean", "default":false
        }]
    }))
    .unwrap();
    SavedOptionSchema {
        identity: AddonMediaListOptionSchemaRef {
            addon_id: "org.example.media".into(),
            capability_id: definition.id.clone(),
            schema_digest: sha256(json(&definition).unwrap().as_bytes()),
        },
        definition: serde_json::to_value(definition).unwrap(),
    }
}

async fn fixture() -> (tempfile::TempDir, crate::db::DbPool, SavedListDocument) {
    let root = tempfile::tempdir().unwrap();
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(root.path().join("activation.db"))
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    sqlx::raw_sql(r#"INSERT INTO "AddonInstallation"
        ("AddonId","Version","Enabled","PackageSha256","ContentSha256","ManifestJson",
         "InstallPath","InstalledAt","UpdatedAt")
        VALUES ('org.example.media','1',1,'fixture','fixture','{}','addons/fixture','2026-09-13','2026-09-13');
        INSERT INTO "AddonMediaList"
        ("Id","AddonId","ProviderId","SourceUrl","Name","DraftRevision","AdminHidden")
        VALUES (1,'org.example.media','shared-list','https://example.invalid/list','Local draft',17,1);"#)
        .execute(&pool).await.unwrap();
    let mut document = super::super::model::tests::fixture();
    document.draft.options.schema = Some(schema("enabled"));
    document.draft.options.list_values.insert(
        "enabled".into(),
        ersatzrs_domain::AddonMediaListOptionValue::Boolean(false),
    );
    let publication = document.published.as_mut().unwrap();
    publication.options.schema = Some(schema("enabled"));
    publication.options.list_values.insert(
        "enabled".into(),
        ersatzrs_domain::AddonMediaListOptionValue::Boolean(true),
    );
    publication.files.clear();
    let item = &mut publication.items[0];
    item.playback = SavedPlayback::Stream { timeline: None };
    item.subtitles.clear();
    item.content.kind = AddonMediaListItemKind::RemoteStream;
    item.content.title = "Published title".into();
    item.content.source_url = Some("https://example.invalid/published".into());
    item.content.provider_liveness = RemoteStreamLiveness::Finite;
    item.content.duration_seconds = Some(60);
    item.metadata
        .insert("title".into(), serde_json::json!("Published title"));
    (root, pool, document)
}

#[tokio::test]
async fn plain_stream_snapshot_keeps_draft_and_publication_independent() {
    let (root, pool, mut document) = fixture().await;
    let mut bytes = vec![0; 64];
    bytes[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
    std::fs::write(root.path().join("poster.png"), &bytes).unwrap();
    let publication = document.published.as_mut().unwrap();
    publication.files.push(SavedFile {
        relative_path: "poster.png".into(),
        sha256: sha256(&bytes),
        byte_length: 64,
    });
    publication.items[0].metadata.insert(
        "artwork".into(),
        serde_json::json!([{"role":"poster","url":"./poster.png"}]),
    );
    let original = document.clone();
    let mut staging = ArtworkStaging::new(&pool).await.unwrap();
    staging
        .stage_document(root.path(), &document)
        .await
        .unwrap();
    let mut tx = staging.begin_projection().await.unwrap();
    let now = document.published.as_ref().unwrap().completed_at
        + chrono::Duration::days(1)
        + chrono::Duration::nanoseconds(123_456_789);
    let projected = projection::project_draft_in_tx(&mut tx, 1, &document, 17, now)
        .await
        .unwrap();
    let before = capture::draft_in_tx(&mut tx, 1).await.unwrap().draft;
    let snapshot = snapshot_in_tx(
        &mut tx,
        1,
        document.published.as_ref().unwrap(),
        &projected.item_ids,
        23,
        17,
        now,
    )
    .await
    .unwrap();
    assert_eq!(
        (
            snapshot.list_id,
            snapshot.revision,
            snapshot.source_draft_revision
        ),
        (1, 23, 17)
    );
    assert_eq!(snapshot.name, "Published name");
    assert_eq!(
        snapshot.completed_at,
        crate::repositories::common::decode_datetime(
            "test completion",
            &crate::repositories::common::encode_datetime(now)
        )
        .unwrap()
    );
    assert_eq!(snapshot.items.len(), 1);
    assert_eq!(snapshot.items[0].provider_id, "published");
    assert_eq!(
        snapshot.items[0].source_url.as_deref(),
        Some("https://example.invalid/published")
    );
    assert!(snapshot.items[0]
        .metadata_json
        .contains(crate::addon_artwork::UPLOAD_URL_PREFIX));
    assert!(snapshot.items[0].published_media_item_id.is_none());
    assert!(snapshot.items[0].playback_timeline.is_none());
    assert_eq!(
        snapshot.options.schema,
        document
            .published
            .as_ref()
            .unwrap()
            .options
            .schema
            .as_ref()
            .map(|schema| schema.identity.clone())
    );
    assert_ne!(snapshot.options.list_values, before.options.list_values);
    assert_eq!(
        capture::draft_in_tx(&mut tx, 1).await.unwrap().draft,
        before
    );
    let local: (bool, i32, bool) = sqlx::query_as(r#"SELECT "AdminHidden","PublishedRevision",
        (SELECT "InDraft" FROM "AddonMediaListItem" WHERE "ProviderId"='published') FROM "AddonMediaList" WHERE "Id"=1"#)
        .fetch_one(&mut *tx).await.unwrap();
    assert_eq!(local, (true, 0, false));
    crate::repositories::insert_revision_snapshot_in_tx(&mut tx, &snapshot)
        .await
        .unwrap();
    let retained = crate::repositories::load_valid_revision_snapshot_in_tx(&mut tx, 1, 23)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(retained, snapshot);
    assert_eq!(document, original);
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn unsupported_playback_bindings_refuse_without_downgrading() {
    let (_root, pool, document) = fixture().await;
    let mut staging = ArtworkStaging::new(&pool).await.unwrap();
    let mut tx = staging.begin_projection().await.unwrap();
    let file = SavedFile {
        relative_path: "media.mkv".into(),
        sha256: "a".repeat(64),
        byte_length: 1,
    };
    let mut variants = Vec::new();
    for playback in [
        SavedPlayback::DownloadAhead {
            file: file.clone(),
            duration_milliseconds: 60_000,
        },
        SavedPlayback::Replacement {
            replacement: SavedReplacement { file: None },
        },
        SavedPlayback::Stream {
            timeline: Some(SavedPlaybackTimeline {
                source_duration_milliseconds: 60_000,
                source_window_milliseconds: 0..60_000,
                excluded_intervals_milliseconds: Vec::new(),
            }),
        },
    ] {
        let mut publication = document.published.clone().unwrap();
        publication.items[0].playback = playback;
        variants.push(publication);
    }
    let mut subtitles = document.published.clone().unwrap();
    subtitles.items[0].subtitles.push(SavedSubtitle {
        file,
        language: "en".into(),
        timeline: ersatzrs_addon_contract::MediaListSubtitleTimeline::Source,
    });
    variants.push(subtitles);
    for publication in variants {
        let before = publication.clone();
        assert!(snapshot_in_tx(
            &mut tx,
            1,
            &publication,
            &BTreeMap::new(),
            23,
            17,
            publication.completed_at
        )
        .await
        .is_err());
        assert_eq!(publication, before);
    }
    let schemas: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "AddonMediaListOptionSchema""#)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(schemas, 0);
    tx.rollback().await.unwrap();
}
