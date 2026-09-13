use super::*;

use chrono::{TimeZone as _, Utc};
use ersatzrs_core::repositories::{
    AddonMediaListLifecycleRepository as _, AddonMediaListRevisionArtifacts,
    AddonMediaListRevisionFile,
};

#[test]
fn saved_publication_metadata_preserves_person_thumbnail_matching_artwork_source() {
    let source = "https://example.invalid/person.png";
    let artwork = PublishedArtwork {
        entries: vec![PublishedArtworkEntry {
            provider_id: None,
            role: 1,
            source: source.into(),
            file: Some(SavedFile {
                relative_path: ".ersatzrs-saved/published/person.png".into(),
                sha256: "a".repeat(64),
                byte_length: 3,
            }),
        }],
    };
    let people = serde_json::json!([
        {"name":"Remote", "thumb":source},
        {"name":"Literal", "thumb":"./person.png"}
    ]);
    let metadata = serde_json::json!({"people":people, "artwork_urls":[source]});
    let captured = artwork.metadata(&metadata.to_string(), None).unwrap();
    assert_eq!(captured["people"], people);
    assert_eq!(
        captured["artwork_urls"][0],
        "./.ersatzrs-saved/published/person.png"
    );
}

#[tokio::test]
async fn saved_publication_capture_rejects_person_upload_handle() {
    let pool = database().await;
    sqlx::query(r#"UPDATE "AddonMediaList" SET "ProviderMetadataJson"=? WHERE "Id"=1"#)
        .bind(r#"{"people":[{"name":"Example","thumb":"ersatzrs-upload://41"}]}"#)
        .execute(&pool)
        .await
        .unwrap();
    retain(&pool, &[], &AddonMediaListRevisionArtifacts::default()).await;
    let mut tx = pool.begin().await.unwrap();
    assert!(in_tx(&mut tx, 1).await.is_err());
    tx.rollback().await.unwrap();
}

async fn database() -> crate::db::DbPool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    sqlx::raw_sql(
        r#"
        INSERT INTO "AddonInstallation" ("AddonId","Version","Enabled","PackageSha256",
            "ContentSha256","ManifestJson","InstallPath","InstalledAt","UpdatedAt")
        VALUES ('org.example.saved','1.0.0',1,'fixture','fixture','{}','addons/saved',
            '2026-09-13 00:00:00','2026-09-13 00:00:00');
        INSERT INTO "AddonMediaList" ("Id","AddonId","ProviderId","SourceUrl","Name",
            "DraftRevision","PublishedRevision","ProviderMetadataJson")
        VALUES (1,'org.example.saved','list','https://example.invalid/list','Published list',6,1,
            '{"title":"Published metadata"}');
        INSERT INTO "AddonMediaListItem" ("Id","AddonMediaListId","ProviderId","DisplayTitle",
            "Kind","Rank","Title","Included","Published","ProviderLiveness","EffectiveLiveness",
            "DurationSeconds","SourceUrl","ProviderMetadataJson")
        VALUES (10,1,'published','Published item',4,0,'Published item',1,1,1,1,20,
            'https://example.invalid/video','{"title":"Published item metadata"}'),
            (11,1,'excluded','Excluded item',4,1,'Excluded item',0,0,1,1,30,
            'https://example.invalid/excluded','{}');
    "#,
    )
    .execute(&pool)
    .await
    .unwrap();
    pool
}

async fn retain(
    pool: &crate::db::DbPool,
    files: &[AddonMediaListRevisionFile],
    artifacts: &AddonMediaListRevisionArtifacts,
) {
    crate::repositories::make_repositories(pool.clone())
        .addon_media_lists
        .retain_published_revision(
            1,
            1,
            6,
            Utc.with_ymd_and_hms(2026, 9, 13, 0, 0, 0).unwrap(),
            files,
            artifacts,
        )
        .await
        .unwrap();
}

fn streaming_plan() -> ersatzrs_core::addon_media_list_playback::AddonMediaListPlaybackTimelinePlan
{
    ersatzrs_core::addon_media_list_playback::AddonMediaListPlaybackTimelinePlan::new(
        20_000,
        0..20_000,
        std::slice::from_ref(&(5_000..7_125)),
        "a".repeat(64),
    )
    .unwrap()
}

#[tokio::test]
async fn saved_publication_capture_keeps_old_membership_metadata_and_timing_apart_from_draft() {
    let pool = database().await;
    retain(
        &pool,
        &[],
        &AddonMediaListRevisionArtifacts {
            streaming_plans: BTreeMap::from([("published".into(), streaming_plan())]),
            ..Default::default()
        },
    )
    .await;
    sqlx::raw_sql(
        r#"
        UPDATE "AddonMediaList" SET "DraftRevision"=8,"Name"='New draft',
            "ProviderMetadataJson"='{"title":"New draft metadata"}';
        UPDATE "AddonMediaListItem" SET "Title"='Draft edited title',"Included"=0 WHERE "Id"=10;
        UPDATE "AddonMediaListItem" SET "Included"=1 WHERE "Id"=11;
    "#,
    )
    .execute(&pool)
    .await
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    let captured = in_tx(&mut tx, 1).await.unwrap().unwrap();
    let draft = super::super::capture::draft_in_tx(&mut tx, 1)
        .await
        .unwrap();
    assert_eq!(captured.publication.presentation.name, "Published list");
    assert_eq!(captured.publication.metadata["title"], "Published metadata");
    assert_eq!(captured.publication.source_draft_revision, 6);
    assert_eq!(captured.publication.items.len(), 1);
    let item = &captured.publication.items[0];
    assert_eq!(item.content.provider_id, "published");
    assert_eq!(item.content.title, "Published item");
    assert!(item.content.included);
    let SavedPlayback::Stream {
        timeline: Some(timeline),
    } = &item.playback
    else {
        panic!("streamed interval plan missing")
    };
    assert_eq!(
        timeline.excluded_intervals_milliseconds.as_slice(),
        std::slice::from_ref(&(5_000..7_125))
    );
    assert_eq!(draft.draft.presentation.name, "New draft");
    assert_eq!(draft.draft.items.len(), 2);
    assert!(!draft.draft.items[0].content.included);
    assert!(draft.draft.items[1].content.included);
    assert_eq!(draft.draft.revision, 8);
    let json = serde_json::to_string(&captured.publication).unwrap();
    for local in [
        "context_fingerprint",
        "media_item_id",
        "artifact_id",
        "list_id",
        "availability_reason\"",
    ] {
        assert!(!json.contains(local), "local field escaped: {local}");
    }
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn saved_publication_capture_refuses_missing_or_corrupt_snapshot_without_draft_fallback() {
    let pool = database().await;
    let mut tx = pool.begin().await.unwrap();
    assert!(in_tx(&mut tx, 1).await.is_err());
    assert!(in_tx(&mut tx, 999).await.is_err());
    tx.rollback().await.unwrap();
    retain(&pool, &[], &AddonMediaListRevisionArtifacts::default()).await;
    sqlx::query(r#"UPDATE "AddonMediaListRevision" SET "MetadataJson"='{"title":"Tampered"}'"#)
        .execute(&pool)
        .await
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    assert!(in_tx(&mut tx, 1).await.is_err());
    tx.rollback().await.unwrap();
    sqlx::query(r#"UPDATE "AddonMediaList" SET "PublishedRevision"=0"#)
        .execute(&pool)
        .await
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    assert!(in_tx(&mut tx, 1).await.unwrap().is_none());
    tx.rollback().await.unwrap();
}

async fn artifacts(pool: &crate::db::DbPool, prepared: bool) -> AddonMediaListRevisionArtifacts {
    sqlx::query(
        r#"INSERT INTO "AddonMediaListArtifact" ("Id","AddonMediaListId","AddonMediaListItemId",
        "CacheKey","Kind","State","RelativePath","ByteLength","Sha256","DurationMilliseconds",
        "SubtitleLanguage","SubtitleTimeline","CreatedAt","UpdatedAt","CompletedAt")
        VALUES (30,1,10,'prepared',1,2,'objects/old.media',1024,?,12500,NULL,NULL,
            '2026-09-13 00:00:00','2026-09-13 00:00:00','2026-09-13 00:00:00'),
            (31,1,10,'manual-nl',0,2,'objects/manual.srt',40,?,NULL,'nl-NL',?,
            '2026-09-13 00:00:00','2026-09-13 00:00:00','2026-09-13 00:00:00')"#,
    )
    .bind("a".repeat(64))
    .bind("b".repeat(64))
    .bind(i32::from(prepared))
    .execute(pool)
    .await
    .unwrap();
    AddonMediaListRevisionArtifacts {
        ids: if prepared { vec![30, 31] } else { vec![31] },
        prepared_media: if prepared {
            BTreeMap::from([("published".into(), 30)])
        } else {
            BTreeMap::new()
        },
        subtitles: BTreeMap::from([("published".into(), vec![31])]),
        ..Default::default()
    }
}

#[tokio::test]
async fn saved_publication_capture_keeps_prepared_selection_and_subtitle_clock_without_sender_pins()
{
    for prepared in [false, true] {
        let pool = database().await;
        let selection = artifacts(&pool, prepared).await;
        retain(&pool, &[], &selection).await;
        let mut tx = pool.begin().await.unwrap();
        let captured = in_tx(&mut tx, 1).await.unwrap().unwrap();
        let item = &captured.publication.items[0];
        assert_eq!(item.subtitles[0].language, "nl-NL");
        assert_eq!(
            item.subtitles[0].timeline,
            if prepared {
                MediaListSubtitleTimeline::Prepared
            } else {
                MediaListSubtitleTimeline::Source
            }
        );
        assert_eq!(item.subtitles[0].file.sha256, "b".repeat(64));
        if prepared {
            let SavedPlayback::DownloadAhead {
                file,
                duration_milliseconds,
            } = &item.playback
            else {
                panic!("prepared selection changed to streaming")
            };
            assert_eq!(*duration_milliseconds, 12_500);
            assert_eq!(file.byte_length, 1024);
            assert_eq!(file.sha256, "a".repeat(64));
        } else {
            assert!(matches!(item.playback, SavedPlayback::Stream { .. }));
        }
        assert_eq!(captured.artifacts.len(), if prepared { 2 } else { 1 });
        assert!(!serde_json::to_string(&captured.publication)
            .unwrap()
            .contains("artifact_id"));
        // Reference capture neither reads nor copies bytes from the local cache.
        assert_eq!(
            captured.artifacts.last().unwrap().storage_relative_path,
            "objects/manual.srt"
        );
        tx.rollback().await.unwrap();
        sqlx::query(r#"UPDATE "AddonMediaListArtifact" SET "Sha256"=NULL WHERE "Id"=31"#)
            .execute(&pool)
            .await
            .unwrap();
        let mut tx = pool.begin().await.unwrap();
        assert!(in_tx(&mut tx, 1).await.is_err());
        tx.rollback().await.unwrap();
    }
}

async fn retained_artwork(pool: crate::db::DbPool, source: &str) -> EntityId {
    sqlx::raw_sql(r#"
        UPDATE "AddonMediaList" SET "ProviderMetadataJson"='{"artwork":[{"role":"poster","url":"ersatzrs-upload://41"}]}';
        INSERT INTO "AddonMediaListArtworkMaterialization"
            ("AddonMediaListId","ScopeItemId","Role","Source","FileName","Sha256","PublishedRevision")
        VALUES (1,0,1,'ersatzrs-upload://41','poster.png','placeholder',0);
    "#).execute(&pool).await.unwrap();
    sqlx::query(r#"UPDATE "AddonMediaList" SET "ProviderMetadataJson"=?"#)
        .bind(serde_json::json!({"artwork":[{"role":"poster","url":source}]}).to_string())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "AddonMediaListArtworkMaterialization" SET "Source"=?"#)
        .bind(source)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "AddonMediaListArtworkMaterialization" SET "Sha256"=?"#)
        .bind("c".repeat(64))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "AddonMediaListArtworkUpload"
            ("Id","AddonMediaListId","ScopeItemId","Role","ContentType","Extension",
            "Bytes","Sha256","CreatedAt","UpdatedAt")
            VALUES (41,1,0,1,'image/png','png',zeroblob(80),?,'2026-09-13 00:00:00','2026-09-13 00:00:00')"#)
            .bind("c".repeat(64)).execute(&pool).await.unwrap();
    let blob = crate::repositories::make_repositories(pool.clone())
        .addon_media_lists
        .register_revision_file_blob(
            1,
            "blobs/retained.png",
            &"c".repeat(64),
            80,
            Utc.with_ymd_and_hms(2026, 9, 13, 0, 0, 0).unwrap(),
        )
        .await
        .unwrap();
    let file = AddonMediaListRevisionFile {
        blob_id: blob.id,
        provider_id: None,
        published_relative_path: "streams/poster.png".into(),
        storage_relative_path: blob.storage_relative_path,
        sha256: blob.sha256,
        byte_length: blob.byte_length,
    };
    let duplicate_bytes = AddonMediaListRevisionFile {
        published_relative_path: "streams/z-copy.png".into(),
        ..file.clone()
    };
    retain(
        &pool,
        &[file, duplicate_bytes],
        &AddonMediaListRevisionArtifacts::default(),
    )
    .await;
    blob.id
}

#[tokio::test]
async fn saved_publication_capture_maps_artwork_to_hash_matched_retained_bytes_not_new_uploads() {
    for source in ["ersatzrs-upload://41", "https://example.invalid/poster.png"] {
        let pool = database().await;
        let blob_id = retained_artwork(pool.clone(), source).await;
        sqlx::query(r#"UPDATE "AddonMediaList" SET "DraftRevision"=8,"ProviderMetadataJson"='{"artwork":[{"role":"poster","url":"ersatzrs-upload://42"}]}'"#)
        .execute(&pool).await.unwrap();
        let mut tx = pool.begin().await.unwrap();
        let captured = in_tx(&mut tx, 1).await.unwrap().unwrap();
        assert_eq!(
            captured.publication.metadata["artwork"][0]["url"],
            format!("./.ersatzrs-saved/published/{}.png", "c".repeat(64))
        );
        assert_eq!(captured.revision_files[0].blob_id, blob_id);
        assert!(!serde_json::to_string(&captured.publication)
            .unwrap()
            .contains("ersatzrs-upload://"));
        tx.rollback().await.unwrap();
        // Simulate the real draft-B projection upsert after retaining publication A.
        // Capture consumes identities only; image-byte verification is a later step.
        sqlx::query(r#"INSERT INTO "AddonMediaListArtworkUpload"
            ("Id","AddonMediaListId","ScopeItemId","Role","ContentType","Extension",
            "Bytes","Sha256","CreatedAt","UpdatedAt")
            VALUES (42,1,0,1,'image/png','png',zeroblob(81),?,'2026-09-13 00:00:00','2026-09-13 00:00:00')"#)
            .bind("d".repeat(64)).execute(&pool).await.unwrap();
        sqlx::query(
            r#"UPDATE "AddonMediaListArtworkMaterialization"
            SET "Source"='ersatzrs-upload://42', "Sha256"=?"#,
        )
        .bind("d".repeat(64))
        .execute(&pool)
        .await
        .unwrap();
        let mut tx = pool.begin().await.unwrap();
        let captured = in_tx(&mut tx, 1).await.unwrap().unwrap();
        let expected = if source.starts_with(crate::addon_artwork::UPLOAD_URL_PREFIX) {
            format!("./.ersatzrs-saved/published/{}.png", "c".repeat(64))
        } else {
            source.to_owned()
        };
        assert_eq!(captured.publication.metadata["artwork"][0]["url"], expected);
        let draft = super::super::capture::draft_in_tx(&mut tx, 1)
            .await
            .unwrap();
        assert_eq!(
            draft.draft.metadata.provider["artwork"][0]["url"],
            format!("./.ersatzrs-saved/artwork/{}.png", "d".repeat(64))
        );
        tx.rollback().await.unwrap();
        if source.starts_with(crate::addon_artwork::UPLOAD_URL_PREFIX) {
            // A hash match never grants another scope or role access to a handle.
            for change in [
                r#"UPDATE "AddonMediaListArtworkUpload" SET "Role"=2 WHERE "Id"=41"#,
                r#"UPDATE "AddonMediaListArtworkUpload" SET "Role"=1,"ScopeItemId"=11 WHERE "Id"=41"#,
            ] {
                sqlx::query(change).execute(&pool).await.unwrap();
                let mut tx = pool.begin().await.unwrap();
                assert!(in_tx(&mut tx, 1).await.is_err());
                tx.rollback().await.unwrap();
            }
        }
    }
}

#[tokio::test]
async fn saved_publication_capture_retains_owned_operator_artwork_only_when_snapshot_matches() {
    let pool = database().await;
    retained_artwork(pool.clone(), "ersatzrs-upload://41").await;
    // The operator's published image differs from the original upload bytes.
    sqlx::query(r#"UPDATE "AddonMediaListArtworkUpload" SET "Sha256"=? WHERE "Id"=41"#)
        .bind("e".repeat(64))
        .execute(&pool)
        .await
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    let captured = in_tx(&mut tx, 1).await.unwrap().unwrap();
    assert_eq!(
        captured.publication.metadata["artwork"][0]["url"],
        format!("./.ersatzrs-saved/published/{}.png", "c".repeat(64))
    );
    tx.rollback().await.unwrap();
    // A materialization cannot bypass handle ownership or substitute new bytes.
    for change in [
        r#"UPDATE "AddonMediaListArtworkUpload" SET "Role"=2 WHERE "Id"=41"#,
        r#"UPDATE "AddonMediaListArtworkUpload" SET "Role"=1; UPDATE "AddonMediaListArtworkMaterialization" SET "Sha256"='different'"#,
    ] {
        sqlx::raw_sql(change).execute(&pool).await.unwrap();
        let mut tx = pool.begin().await.unwrap();
        assert!(in_tx(&mut tx, 1).await.is_err());
        tx.rollback().await.unwrap();
    }
}

#[tokio::test]
async fn saved_publication_capture_isolates_retained_bytes_and_deduplicates_destinations() {
    let pool = database().await;
    let blob_id = retained_artwork(pool.clone(), "ersatzrs-upload://41").await;
    let mut tx = pool.begin().await.unwrap();
    let captured = in_tx(&mut tx, 1).await.unwrap().unwrap();
    // Both original paths remain available to the local copy coordinator, while
    // identical retained bytes require only one portable destination.
    assert_eq!(captured.revision_files.len(), 2);
    assert_eq!(captured.publication.files.len(), 1);
    assert_eq!(captured.revision_files[0].blob_id, blob_id);
    assert_eq!(
        captured.revision_files[0].published_relative_path,
        "streams/poster.png"
    );
    assert_eq!(
        captured.revision_files[1].published_relative_path,
        "streams/z-copy.png"
    );
    let retained = &captured.publication.files[0];
    assert_eq!(
        retained.relative_path,
        format!(".ersatzrs-saved/published/{}.png", "c".repeat(64))
    );
    assert_eq!(retained.byte_length, 80);
    assert_eq!(
        captured.publication.metadata["artwork"][0]["url"],
        format!("./{}", retained.relative_path)
    );
    let mut draft_projection = captured.revision_files[0].clone();
    draft_projection.sha256 = "d".repeat(64);
    draft_projection.byte_length = 81;
    // A new projection at the same display path cannot overwrite retained A.
    let different_bytes = saved_file(&draft_projection);
    assert_ne!(retained.relative_path, different_bytes.relative_path);
    assert_ne!(retained.sha256, different_bytes.sha256);
    draft_projection.published_relative_path = "renamed/draft-poster.png".into();
    assert_eq!(saved_file(&draft_projection), different_bytes);
    assert_eq!(saved_file(&captured.revision_files[1]), *retained);
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn saved_publication_capture_omits_regenerated_files_and_preserves_content_extensions() {
    let pool = database().await;
    let blob = crate::repositories::make_repositories(pool.clone())
        .addon_media_lists
        .register_revision_file_blob(
            1,
            "blobs/retained-content",
            &"f".repeat(64),
            80,
            Utc.with_ymd_and_hms(2026, 9, 13, 0, 0, 0).unwrap(),
        )
        .await
        .unwrap();
    let files = [
        "streams/item.NfO",
        "streams/item.YAML",
        "streams/item.yMl",
        "streams/.ersatzrs-scan.json",
        "streams/.ersatzrs-trash/old.png",
        ".ersatzrs-state/retained.png",
        "streams/poster.PNG",
        "streams/photo.JPG",
        "streams/thumb.WebP",
        "streams/retained.Custom",
        "streams/extensionless",
        "streams/operator.SRT",
    ]
    .into_iter()
    .map(|path| AddonMediaListRevisionFile {
        blob_id: blob.id,
        provider_id: None,
        published_relative_path: path.into(),
        storage_relative_path: blob.storage_relative_path.clone(),
        sha256: blob.sha256.clone(),
        byte_length: blob.byte_length,
    })
    .collect::<Vec<_>>();
    retain(&pool, &files, &AddonMediaListRevisionArtifacts::default()).await;
    let mut tx = pool.begin().await.unwrap();
    let captured = in_tx(&mut tx, 1).await.unwrap().unwrap();
    assert_eq!(captured.revision_files.len(), 6);
    assert_eq!(captured.publication.files.len(), 6);
    for original in files.iter().filter(|file| portable_revision_file(file)) {
        assert!(captured.revision_files.contains(original));
    }
    for extension in ["png", "jpg", "webp", "custom", "blob", "srt"] {
        assert!(captured.publication.files.iter().any(|file| {
            file.relative_path
                == format!(".ersatzrs-saved/published/{}.{extension}", "f".repeat(64))
        }));
    }
    tx.rollback().await.unwrap();
}

#[test]
fn saved_publication_capture_uses_blob_for_unsafe_extensions_without_discarding_bytes() {
    let mut file = AddonMediaListRevisionFile {
        blob_id: 1,
        provider_id: None,
        published_relative_path: String::new(),
        storage_relative_path: "blobs/retained".into(),
        sha256: "a".repeat(64),
        byte_length: 80,
    };
    for path in [
        "streams/file.png ",
        "streams/file.png!",
        "streams/file.we-bp",
        "streams/file.é",
    ] {
        file.published_relative_path = path.into();
        assert!(portable_revision_file(&file));
        let saved = saved_file(&file);
        assert_eq!(
            saved.relative_path,
            format!(".ersatzrs-saved/published/{}.blob", file.sha256)
        );
        assert_eq!(saved.byte_length, file.byte_length);
        assert_eq!(saved.sha256, file.sha256);
    }
    file.published_relative_path = format!("streams/file.{}", "A".repeat(32));
    assert_eq!(
        saved_file(&file).relative_path,
        format!(
            ".ersatzrs-saved/published/{}.{}",
            file.sha256,
            "a".repeat(32)
        )
    );
}
