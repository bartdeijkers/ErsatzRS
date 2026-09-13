use super::*;
use ersatzrs_core::repositories::{
    AddonMediaListLifecycleRepository as _, AddonMediaListRevisionArtifacts,
};

#[tokio::test]
#[allow(clippy::too_many_lines)] // One actual stage call proves the journal-to-generated-file boundary.
async fn staged_source_revision_generates_retained_yaml_and_nfo_not_newer_draft() {
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
            '{"title":"Retained provider","year":2020,"tags":["retained-tag"]}',
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
