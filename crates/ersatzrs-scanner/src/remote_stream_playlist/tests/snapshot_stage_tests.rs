use super::*;
use ersatzrs_core::repositories::{
    AddonMediaListLifecycleRepository as _, AddonMediaListRevisionArtifacts,
};

async fn retained_fixture() -> (
    tempfile::TempDir,
    DbPool,
    PathBuf,
    ValidatedPlaylistManifest,
) {
    let temp = tempfile::tempdir().unwrap();
    let package_root = temp.path().join("addon-package");
    std::fs::create_dir_all(&package_root).unwrap();
    let script = package_root.join("fixture.sh");
    write_fixture_playlist_script(&script, "https://example.test/must-not-enumerate", false);
    std::fs::write(&script, "#!/bin/sh\nexit 1\n").unwrap();
    let pool = fixture_addon_pool(&package_root).await;
    let manifest_path = temp.path().join("Managed.playlist.yml");
    let manifest = parse_playlist_manifest(valid_yaml(), &manifest_path).unwrap();
    sqlx::raw_sql(r#"
        INSERT INTO "MediaSource" ("Id") VALUES (1);
        INSERT INTO "Library" ("Id","Name","MediaKind","MediaSourceId") VALUES (1,'Managed',7,1);
        INSERT INTO "AddonMediaList" (
            "Id","AddonId","ProviderId","SourceUrl","Name","ReviewState",
            "DraftRevision","PublishedRevision","ContentKindOverride")
        VALUES (1,'org.ersatzrs.addon.fixture','fixture/list','https://example.test/playlist','Retained list',2,4,2,2);
        INSERT INTO "AddonMediaListItem" (
            "Id","AddonMediaListId","ProviderId","DisplayTitle","Title","Kind","Rank",
            "SourceUrl","DurationSeconds","Published","ContentKind","ProviderLiveness","EffectiveLiveness",
            "ProviderMetadataJson","MetadataOverrideJson")
        VALUES (20,1,'retained','Retained display','Retained title',4,0,'https://example.test/retained',60,1,1,1,1,
            '{"title":"Retained provider","tags":["retained-tag"]}',
            '{"title":"Retained reviewed"}');
    "#).execute(&pool).await.unwrap();
    sqlx::query(r#"INSERT INTO "AddonMediaListLibrary" ("AddonMediaListId","LibraryId","FolderName","ManifestPath") VALUES (1,1,'Managed',?)"#)
        .bind(manifest_path.to_string_lossy().as_ref()).execute(&pool).await.unwrap();
    let repository = ersatzrs_infra::repositories::SqliteAddonMediaListRepository(pool.clone());
    repository
        .retain_published_revision(
            1,
            2,
            4,
            Utc::now(),
            &[],
            &AddonMediaListRevisionArtifacts::default(),
        )
        .await
        .unwrap();
    (temp, pool, manifest_path, manifest)
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // One actual stage call proves the journal-to-generated-file boundary.
async fn staged_source_revision_generates_retained_yaml_and_nfo_not_newer_draft() {
    let (_temp, pool, manifest_path, manifest) = retained_fixture().await;
    // Neither current Published flags nor any newer content may choose the
    // source: the retained owner remains present only to support local bindings.
    sqlx::raw_sql(r#"
        UPDATE "AddonMediaList" SET "Name"='New draft list',"DraftRevision"=7,"ContentKindOverride"=3 WHERE "Id"=1;
        UPDATE "AddonMediaListItem" SET "InDraft"=0,"Included"=0,"Published"=0,
            "Title"='New draft title',"SourceUrl"='https://example.test/new',"DurationSeconds"=99,
            "ProviderMetadataJson"='{"title":"New provider","tags":["new-tag"]}',
            "MetadataOverrideJson"='{"title":"New override"}' WHERE "Id"=20;
        INSERT INTO "AddonMediaListItem" (
            "Id","AddonMediaListId","ProviderId","DisplayTitle","Title","Kind","Rank","SourceUrl","Published")
        VALUES (21,1,'new-only','New only','New only',4,0,'https://example.test/new-only',1);
        INSERT INTO "ManagedLibraryOperation" ("Id","LibraryId","OwnerToken","Kind","State","CreatedAt","UpdatedAt")
        VALUES (100,1,'fixture-owner',1,1,'2026-09-13','2026-09-13');
        INSERT INTO "AddonMediaListPublicationJob" (
            "Id","OperationId","AddonMediaListId","Generation","SavedRevision","TargetPublishedRevision",
            "SourcePublishedRevision","Phase","CreatedAt","UpdatedAt")
        VALUES (100,100,1,1,7,3,2,1,'2026-09-13','2026-09-13');
    "#).execute(&pool).await.unwrap();
    let journal = serde_json::json!({
        "operation_id":100,"generation":1,"saved_revision":7,
        "previous_review_state":ersatzrs_domain::AddonMediaListReviewState::Ready,
        "previous_presentation_kind":ersatzrs_domain::AddonMediaListPresentationKind::RemoteStreams,
        "previous_items":[],"previous_playlist":null
    });
    sqlx::query(
        r#"UPDATE "AddonMediaList" SET "ReviewState"=3,"PublicationJournalJson"=? WHERE "Id"=1"#,
    )
    .bind(journal.to_string())
    .execute(&pool)
    .await
    .unwrap();

    let staged = stage_playlist_files(
        &pool,
        &manifest_path,
        valid_yaml().as_bytes(),
        &manifest,
        &AddonRuntimeContext::default(),
    )
    .await
    .unwrap();
    assert!(!staged.enumerated);
    assert_eq!(staged.definitions.len(), 1);
    assert_eq!(staged.definitions[0].id, "retained");
    let yaml = std::fs::read_to_string(&staged.definitions[0].path).unwrap();
    let parsed = ersatzrs_infra::remote_stream::parse_remote_stream_definition(&yaml).unwrap();
    assert_eq!(parsed.url.as_deref(), Some("https://example.test/retained"));
    assert_eq!(parsed.duration.as_deref(), Some("00:01:00"));
    assert_eq!(parsed.content_kind, AddonMediaListContentKind::Movie);
    let nfo_path = staged.definitions[0].path.with_extension("nfo");
    let nfo = crate::nfo::parse_other_video_nfo(&nfo_path).unwrap();
    assert_eq!(nfo.title.as_deref(), Some("Retained reviewed"));
    assert!(nfo.tags.contains(&"retained-tag".to_owned()));
    let nfo_text = std::fs::read_to_string(nfo_path).unwrap();
    assert!(nfo_text.contains("<movie"));
    assert!(!nfo_text.contains("New override"));
    assert!(!nfo_text.contains("new-tag"));
    let unchanged: (String, i32) =
        sqlx::query_as(r#"SELECT "Name","DraftRevision" FROM "AddonMediaList" WHERE "Id"=1"#)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(unchanged, ("New draft list".into(), 7));
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Exercise ordinary regeneration, fresh-cache invalidation and local exclusions together.
async fn ordinary_refresh_replays_retained_content_and_preserves_local_overrides() {
    let (temp, pool, manifest_path, _) = retained_fixture().await;
    // These are operator-authored YAML settings, not invented database fields.
    let manifest_text = format!(
        "{}\ntags: [manifest-tag]\ndefaults:\n  year: 1999\n",
        valid_yaml()
    );
    let manifest = parse_playlist_manifest(&manifest_text, &manifest_path).unwrap();
    let sidecar_path = temp
        .path()
        .join(&manifest.folder)
        .join(".ersatzrs-playlist.json");
    let runtime = AddonRuntimeContext::default();
    sqlx::raw_sql(r#"
        UPDATE "AddonMediaList" SET "Name"='New draft list',"DraftRevision"=7,"ContentKindOverride"=3,
            "ReviewState"=1,"ChapterInput"='0:00 New list chapter' WHERE "Id"=1;
        UPDATE "AddonMediaListItem" SET "Title"='New draft title',"SourceUrl"='https://example.test/new',
            "DurationSeconds"=99,"ChapterInput"='0:00 New item chapter',
            "ProviderMetadataJson"='{"title":"New provider","tags":["new-tag"],"year":2025}',
            "MetadataOverrideJson"='{"title":"New override"}' WHERE "Id"=20;
        INSERT INTO "AddonMediaListItem" (
            "Id","AddonMediaListId","ProviderId","DisplayTitle","Title","Kind","Rank","SourceUrl","Published")
        VALUES (21,1,'unpublished-only','Unpublished only','Unpublished only',4,1,'https://example.test/unpublished',0);
    "#).execute(&pool).await.unwrap();
    let first = sync_playlist_files(
        &pool,
        &manifest_path,
        manifest_text.as_bytes(),
        &manifest,
        &runtime,
    )
    .await
    .unwrap();
    assert_retained_refresh(&first);
    let first_nfo =
        crate::nfo::parse_other_video_nfo(first.definitions[0].path.with_extension("nfo")).unwrap();
    assert_eq!(first_nfo.year, Some(1999));
    assert!(first_nfo.tags.contains(&"manifest-tag".to_owned()));
    assert!(sidecar_path.is_file());
    let mut expired = first.state;
    expired.last_sync = Utc::now() - chrono::Duration::hours(7);
    expired.overrides.excluded = vec!["unrelated-local-exclusion".into()];
    write_json_atomically(&sidecar_path, &expired).unwrap();
    let repeated = sync_playlist_files(
        &pool,
        &manifest_path,
        manifest_text.as_bytes(),
        &manifest,
        &runtime,
    )
    .await
    .unwrap();
    assert_retained_refresh(&repeated);
    assert_eq!(
        repeated.state.overrides.excluded,
        ["unrelated-local-exclusion"]
    );
    let unchanged: (String, i32, i32, String, String) = sqlx::query_as(
        r#"SELECT "Name","DraftRevision","ContentKindOverride","ChapterInput",
        (SELECT "MetadataOverrideJson" FROM "AddonMediaListItem" WHERE "Id"=20)
        FROM "AddonMediaList" WHERE "Id"=1"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        unchanged,
        (
            "New draft list".into(),
            7,
            3,
            "0:00 New list chapter".into(),
            r#"{"title":"New override"}"#.into()
        )
    );

    // The just-committed sidecar is fresh and its files are intact. Only the
    // newly retained publication changes its cache identity.
    assert!(
        Utc::now().signed_duration_since(repeated.state.last_sync) < chrono::Duration::hours(1)
    );
    sqlx::query(
        r#"UPDATE "AddonMediaList" SET "ContentKindOverride"=2,"PublishedRevision"=3 WHERE "Id"=1"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(r#"UPDATE "AddonMediaListItem" SET "SourceUrl"='https://example.test/retained',"DurationSeconds"=60,"ChapterInput"=NULL,
        "ProviderMetadataJson"='{"title":"Retained provider","tags":["retained-tag"]}',
        "MetadataOverrideJson"='{"title":"Next published title"}' WHERE "Id"=20"#)
        .execute(&pool).await.unwrap();
    let repository = ersatzrs_infra::repositories::SqliteAddonMediaListRepository(pool.clone());
    repository
        .retain_published_revision(
            1,
            3,
            7,
            Utc::now(),
            &[],
            &AddonMediaListRevisionArtifacts::default(),
        )
        .await
        .unwrap();
    sqlx::query(
        r#"UPDATE "AddonMediaList" SET "PublishedRevision"=3,"ContentKindOverride"=3 WHERE "Id"=1"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(r#"UPDATE "AddonMediaListItem" SET "SourceUrl"='https://example.test/new',"DurationSeconds"=99,
        "ChapterInput"='0:00 New item chapter',"ProviderMetadataJson"='{"title":"New provider","tags":["new-tag"],"year":2025}',
        "MetadataOverrideJson"='{"title":"Still newer draft"}' WHERE "Id"=20"#)
        .execute(&pool).await.unwrap();
    let promoted = sync_playlist_files(
        &pool,
        &manifest_path,
        manifest_text.as_bytes(),
        &manifest,
        &runtime,
    )
    .await
    .unwrap();
    assert!(!promoted.enumerated);
    assert_ne!(promoted.state.manifest_etag, repeated.state.manifest_etag);
    assert_eq!(promoted.definitions.len(), 1);
    let nfo = crate::nfo::parse_other_video_nfo(promoted.definitions[0].path.with_extension("nfo"))
        .unwrap();
    assert_eq!(nfo.title.as_deref(), Some("Next published title"));
    assert!(nfo.tags.contains(&"manifest-tag".to_owned()));

    let mut excluded = promoted.state;
    excluded.last_sync = Utc::now() - chrono::Duration::hours(7);
    excluded.overrides.excluded = vec!["retained".into()];
    write_json_atomically(&sidecar_path, &excluded).unwrap();
    let excluded = sync_playlist_files(
        &pool,
        &manifest_path,
        manifest_text.as_bytes(),
        &manifest,
        &runtime,
    )
    .await
    .unwrap();
    assert!(!excluded.enumerated);
    assert!(excluded.definitions.is_empty());
    assert_eq!(excluded.state.overrides.excluded, ["retained"]);
}

fn assert_retained_refresh(sync: &PlaylistFileSync) {
    assert!(!sync.enumerated);
    assert_eq!(sync.definitions.len(), 1);
    assert_eq!(sync.definitions[0].id, "retained");
    let yaml = std::fs::read_to_string(&sync.definitions[0].path).unwrap();
    let parsed = ersatzrs_infra::remote_stream::parse_remote_stream_definition(&yaml).unwrap();
    assert_eq!(parsed.url.as_deref(), Some("https://example.test/retained"));
    assert_eq!(parsed.duration.as_deref(), Some("00:01:00"));
    assert_eq!(parsed.content_kind, AddonMediaListContentKind::Movie);
    assert!(parsed.parent_provider_id.is_none());
    let nfo =
        crate::nfo::parse_other_video_nfo(sync.definitions[0].path.with_extension("nfo")).unwrap();
    assert_eq!(nfo.title.as_deref(), Some("Retained reviewed"));
    assert!(nfo.tags.contains(&"retained-tag".to_owned()));
    assert!(!nfo.tags.contains(&"new-tag".to_owned()));
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Observe provider chapters without changing retained publication geometry or metadata.
async fn ordinary_snapshot_refresh_updates_provider_chapters_and_availability_only() {
    let (temp, pool, manifest_path, manifest) = retained_fixture().await;
    let package = temp.path().join("addon-package");
    std::fs::write(package.join("fixture.sh"), concat!(
        "#!/bin/sh\n",
        "[ \"$1\" = item ] || exit 1\n",
        "printf '%s\\n' '{\"provider_id\":\"retained\",\"plot\":\"New provider plot\",\"year\":2026,\"tags\":[\"new-provider-tag\"],\"availability\":\"available\",\"chapter_input\":\"0:00 New provider intro\\n0:15 New provider chapter\"}'\n",
    )).unwrap();
    update_fixture_content_hash(&pool, &package).await;
    let manifest_json: String = sqlx::query_scalar(r#"SELECT "ManifestJson" FROM "AddonInstallation" WHERE "AddonId"='org.ersatzrs.addon.fixture'"#)
        .fetch_one(&pool).await.unwrap();
    let mut addon: serde_json::Value = serde_json::from_str(&manifest_json).unwrap();
    addon["capabilities"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"id":"remote-stream.item.v2"}));
    sqlx::query(r#"UPDATE "AddonInstallation" SET "ManifestJson"=? WHERE "AddonId"='org.ersatzrs.addon.fixture'"#)
        .bind(addon.to_string()).execute(&pool).await.unwrap();
    sqlx::raw_sql(r#"
        INSERT INTO "AddonMediaListItem" (
            "Id","AddonMediaListId","ProviderId","DisplayTitle","Title","Kind","Rank",
            "SourceUrl","DurationSeconds","Published","ContentKind","ProviderLiveness","EffectiveLiveness",
            "ParentProviderId","FragmentStartSeconds","FragmentEndSeconds","ProviderMetadataJson")
        VALUES (22,1,'retained:chapter:10','Old chapter','Old chapter',4,1,'https://example.test/retained',10,1,1,1,1,
            'retained',10,20,'{"title":"Old chapter","tags":["old-chapter-tag"]}');
        UPDATE "AddonMediaList" SET "PublishedRevision"=3 WHERE "Id"=1;
    "#).execute(&pool).await.unwrap();
    ersatzrs_infra::repositories::SqliteAddonMediaListRepository(pool.clone())
        .retain_published_revision(
            1,
            3,
            4,
            Utc::now(),
            &[],
            &AddonMediaListRevisionArtifacts::default(),
        )
        .await
        .unwrap();
    sqlx::raw_sql(r#"
        UPDATE "AddonMediaList" SET "ReviewState"=1,"DraftRevision"=7,"ContentKindOverride"=3 WHERE "Id"=1;
        UPDATE "AddonMediaListItem" SET "Availability"=1,"AvailabilityReason"='Provider access is required'
            WHERE "AddonMediaListId"=1;
        UPDATE "AddonMediaListItem" SET "ChapterInput"='0:00 Draft operator chapter',
            "MetadataOverrideJson"='{"title":"New draft title"}' WHERE "Id"=20;
        UPDATE "AddonMediaListItem" SET "FragmentStartSeconds"=30,"FragmentEndSeconds"=40,
            "MetadataOverrideJson"='{"title":"New draft chapter"}' WHERE "Id"=22;
    "#).execute(&pool).await.unwrap();

    let sync = sync_playlist_files(
        &pool,
        &manifest_path,
        valid_yaml().as_bytes(),
        &manifest,
        &AddonRuntimeContext::default(),
    )
    .await
    .unwrap();
    assert!(!sync.enumerated);
    assert_eq!(sync.definitions.len(), 2);
    let observed: Option<String> = sqlx::query_scalar(
        r#"SELECT "ProviderChapterInput" FROM "AddonMediaListItem" WHERE "Id"=20"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        observed.as_deref(),
        Some("0:00 New provider intro\n0:15 New provider chapter")
    );
    for definition in &sync.definitions {
        let yaml = std::fs::read_to_string(&definition.path).unwrap();
        let parsed = ersatzrs_infra::remote_stream::parse_remote_stream_definition(&yaml).unwrap();
        assert_eq!(parsed.content_kind, AddonMediaListContentKind::Movie);
        assert_eq!(
            parsed.availability,
            AddonMediaListItemAvailability::Unavailable
        );
        assert_eq!(
            parsed.availability_reason.as_deref(),
            Some("Provider access is required")
        );
        let nfo = crate::nfo::parse_other_video_nfo(definition.path.with_extension("nfo")).unwrap();
        assert!(!nfo.tags.contains(&"new-provider-tag".to_owned()));
        if definition.id == "retained" {
            assert!(parsed.parent_provider_id.is_none());
            assert_eq!(parsed.duration.as_deref(), Some("00:01:00"));
            assert_eq!(nfo.title.as_deref(), Some("Retained reviewed"));
            assert!(nfo.tags.contains(&"retained-tag".to_owned()));
        } else {
            assert_eq!(definition.id, "retained:chapter:10");
            assert_eq!(parsed.parent_provider_id.as_deref(), Some("retained"));
            assert_eq!(
                (parsed.fragment_start_seconds, parsed.fragment_end_seconds),
                (Some(10), Some(20))
            );
            assert_eq!(nfo.title.as_deref(), Some("Old chapter"));
        }
    }
    let draft: (String,Option<i64>,Option<i64>) = sqlx::query_as(r#"SELECT "MetadataOverrideJson","FragmentStartSeconds","FragmentEndSeconds" FROM "AddonMediaListItem" WHERE "Id"=22"#)
        .fetch_one(&pool).await.unwrap();
    assert_eq!(
        draft,
        (
            r#"{"title":"New draft chapter"}"#.into(),
            Some(30),
            Some(40)
        )
    );
}
