use super::*;
use crate::addon_media_list_saved::model::{SavedListDocument, SAVED_LIST_FILE_NAME};
use crate::addon_media_list_saved::{admission, assembly, capture, document_io, published};
use crate::repositories::SqliteAddonMediaListRepository;
use std::path::{Path, PathBuf};

const ADDON: &str = "org.example.saved-detail";

async fn profile(root: &Path) -> (DbPool, ersatzrs_addon_contract::AddonManifest) {
    std::fs::create_dir_all(root).unwrap();
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    let manifest: ersatzrs_addon_contract::AddonManifest = serde_json::from_value(serde_json::json!({
        "manifest_version":5,"id":ADDON,"version":"1.0.0","name":{"en-US":"Fixture"},
        "summary":{"en-US":"Fixture"},"license":"MIT","source_url":"https://example.invalid/source",
        "host_version":"*","entrypoints":{},"capabilities":[{"id":"media-list.list.v1"}],
        "settings":[{"key":"MEDIA_STORAGE_PATH","label":{"en-US":"Storage"},"kind":"path","required":false}],
        "media_list_storage":{"path_setting":"MEDIA_STORAGE_PATH","default_subdirectory":"media","source_name":"Fixture"}
    })).unwrap();
    sqlx::query(r#"INSERT INTO "AddonInstallation" ("AddonId","Version","Enabled","PackageSha256","ContentSha256","ManifestJson","InstallPath","InstalledAt","UpdatedAt")
        VALUES (?, '1.0.0',1,'fixture','fixture',?,'addons/fixture','2026-09-13','2026-09-13')"#)
        .bind(ADDON).bind(serde_json::to_string(&manifest).unwrap()).execute(&pool).await.unwrap();
    let root = dunce::canonicalize(root).unwrap();
    let mut tx = pool.begin().await.unwrap();
    crate::addon_media_storage::ensure_media_source_in_transaction(
        &mut tx,
        ADDON,
        manifest.media_list_storage.as_ref().unwrap(),
        &root,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    (pool, manifest)
}

pub(super) struct SavedFixture {
    _directory: tempfile::TempDir,
    pub(super) pool: DbPool,
    pub(super) path: PathBuf,
    pub(super) list_id: i64,
    pub(super) item_id: i64,
}

pub(super) async fn saved_fixture() -> SavedFixture {
    let directory = tempfile::tempdir().unwrap();
    let source_root = directory.path().join("source");
    let (source, installed) = profile(&source_root).await;
    sqlx::query(r#"INSERT INTO "AddonMediaList" ("Id","AddonId","ProviderId","SourceUrl","Name","DraftRevision","PublishedRevision","PresentationKind","RequestedPresentationKind")
        VALUES (10,?,'list','https://example.invalid/list','Fixture',1,1,1,1)"#)
        .bind(ADDON).execute(&source).await.unwrap();
    sqlx::raw_sql(r#"INSERT INTO "AddonMediaListItem" ("Id","AddonMediaListId","ProviderId","DisplayTitle","Title","Kind","Rank","SourceUrl","DurationSeconds","Published","ProviderLiveness","EffectiveLiveness")
        VALUES (20,10,'provider-item','Before','Before',4,0,'https://media.example.test/item',600,1,1,1);"#)
        .execute(&source).await.unwrap();
    SqliteAddonMediaListRepository(source.clone())
        .retain_published_revision(
            10,
            1,
            1,
            Utc::now(),
            &[],
            &ersatzrs_core::repositories::AddonMediaListRevisionArtifacts::default(),
        )
        .await
        .unwrap();
    let mut tx = source.begin().await.unwrap();
    let draft = capture::draft_in_tx(&mut tx, 10).await.unwrap();
    let publication = published::in_tx(&mut tx, 10).await.unwrap();
    tx.commit().await.unwrap();
    let folder = source_root.join("List");
    std::fs::create_dir(&folder).unwrap();
    let tree = assembly::lock_managed_tree(&folder).unwrap();
    let document = assembly::assemble_initial(
        &source,
        &assembly::AssemblyContext {
            list_id: 10,
            tree,
            storage_parent: directory.path(),
            storage_owner: "fixture",
            artifact_root: directory.path(),
            installed: &installed,
        },
        draft,
        publication,
    )
    .await
    .unwrap();
    let receiver_root = directory.path().join("receiver");
    let (pool, _) = profile(&receiver_root).await;
    let folder = receiver_root.join("Copied list");
    std::fs::create_dir(&folder).unwrap();
    let path = folder.join(SAVED_LIST_FILE_NAME);
    document_io::write_document(&mut std::fs::File::create(&path).unwrap(), &document).unwrap();
    let report = admission::discover_copied_lists(&pool).await.unwrap();
    assert_eq!(report.refused, 0);
    assert_eq!(report.admitted.len(), 1);
    let list_id = report.admitted[0].job.list_id;
    let item_id: i64 = sqlx::query_scalar(r#"SELECT "Id" FROM "AddonMediaListItem" WHERE "AddonMediaListId"=? AND "ProviderId"='provider-item'"#)
        .bind(list_id).fetch_one(&pool).await.unwrap();
    let checkpoint: ersatzrs_addon_contract::AddonMediaListRecord = serde_json::from_value(serde_json::json!({
        "record_type":"item","provider_id":"provider-item","rank":0,"display_title":"Detailed",
        "title":"Detailed","year":2026,"season":2,"episode":3,"kind":"remote_stream",
        "source_url":"https://media.example.test/item","availability":"available","content_kind":"movie",
        "liveness":"live","duration_seconds":600,"metadata":{"title":"Detailed","plot":"Detailed plot"}
    })).unwrap();
    sqlx::query(r#"INSERT INTO "AddonMediaListDetailJob" ("AddonMediaListItemId","AddonMediaListId","ProviderHost","State","OverviewFingerprint","OverviewRecordJson","LeaseOwner","LeaseExpiresAt","UpdatedAt","Outcome","ResultRecordJson")
        VALUES (?,?,'media.example.test',3,'fixture','{}','detail-owner',? ,?,'success',?)"#)
        .bind(item_id).bind(list_id).bind(encode_datetime(Utc::now()+chrono::Duration::hours(1)))
        .bind(encode_datetime(Utc::now())).bind(serde_json::to_string(&checkpoint).unwrap()).execute(&pool).await.unwrap();
    SavedFixture {
        _directory: directory,
        pool,
        path,
        list_id,
        item_id,
    }
}

pub(super) fn document(fixture: &SavedFixture) -> SavedListDocument {
    serde_json::from_slice(&std::fs::read(&fixture.path).unwrap()).unwrap()
}

fn detail() -> EnumeratedAddonMediaListItem {
    let mut item = source_item("Detailed", 0);
    item.season = Some(2);
    item.episode = Some(3);
    item.liveness = RemoteStreamLiveness::Live;
    item.metadata.title = "Detailed".into();
    item.metadata.plot = Some("Detailed plot".into());
    item
}

async fn state(f: &SavedFixture) -> (i32, String, String, i32) {
    sqlx::query_as(r#"SELECT "list"."DraftRevision", "list"."SavedDocumentDigest", "item"."ProviderMetadataJson", "job"."State"
        FROM "AddonMediaList" AS "list" JOIN "AddonMediaListItem" AS "item" ON "item"."AddonMediaListId"="list"."Id"
        JOIN "AddonMediaListDetailJob" AS "job" ON "job"."AddonMediaListItemId"="item"."Id"
        WHERE "item"."Id"=?"#).bind(f.item_id).fetch_one(&f.pool).await.unwrap()
}

#[tokio::test]
async fn saved_detail_merge_persists_content_and_rejects_stale_lease() {
    let f = saved_fixture().await;
    let before = document(&f);
    let sync = AddonMediaListSynchronizer::new(f.pool.clone());
    assert!(sync
        .merge_detail_item_with_lease(f.list_id, f.item_id, "stale", &detail())
        .await
        .is_err());
    assert_eq!(document(&f), before);
    assert!(matches!(
        sync.merge_detail_item_with_lease(f.list_id, f.item_id, "detail-owner", &detail())
            .await
            .unwrap(),
        DetailMergeOutcome::Applied
    ));
    let saved = document(&f);
    assert_eq!(saved.published, before.published);
    let item = &saved.draft.items[0];
    assert_eq!(item.content.season, Some(2));
    assert_eq!(item.content.episode, Some(3));
    assert_eq!(item.content.provider_liveness, RemoteStreamLiveness::Live);
    assert_eq!(item.metadata.provider["title"], "Detailed");
    assert_eq!(state(&f).await.3, 5);
    let completed = state(&f).await;
    assert!(sync
        .merge_detail_item_with_lease(f.list_id, f.item_id, "detail-owner", &detail())
        .await
        .is_err());
    assert_eq!(state(&f).await, completed);
    assert_eq!(document(&f), saved);
}

#[tokio::test]
async fn saved_detail_merge_defers_publication_without_mutation() {
    let f = saved_fixture().await;
    sqlx::query(r#"UPDATE "AddonMediaList" SET "ReviewState"=3 WHERE "Id"=?"#)
        .bind(f.list_id)
        .execute(&f.pool)
        .await
        .unwrap();
    // Restore can materialize draft membership not yet present in the saved file.
    // Publication deferral must precede capture/reuse of that transitional state.
    sqlx::query(
        r#"INSERT INTO "AddonMediaListItem"
        ("AddonMediaListId","ProviderId","DisplayTitle","Title","Kind","Rank","InDraft")
        VALUES (?,'restored-only','Restored item','Restored item',4,1,1)"#,
    )
    .bind(f.list_id)
    .execute(&f.pool)
    .await
    .unwrap();
    let before = state(&f).await;
    let bytes = std::fs::read(&f.path).unwrap();
    let sync = AddonMediaListSynchronizer::new(f.pool.clone());
    assert!(matches!(
        sync.merge_detail_item_with_lease(f.list_id, f.item_id, "detail-owner", &detail())
            .await
            .unwrap(),
        DetailMergeOutcome::DeferredForPublication
    ));
    assert_eq!(state(&f).await, before);
    assert_eq!(std::fs::read(&f.path).unwrap(), bytes);
}

#[tokio::test]
async fn saved_detail_file_first_failure_preserves_new_content_for_recovery() {
    let f = saved_fixture().await;
    let before = state(&f).await;
    let published = document(&f).published;
    sqlx::raw_sql(r#"CREATE TRIGGER "FixtureDetailDigestFailure" BEFORE UPDATE OF "SavedDocumentDigest" ON "AddonMediaList"
        BEGIN SELECT RAISE(ABORT,'fixture detail failure'); END;"#).execute(&f.pool).await.unwrap();
    let sync = AddonMediaListSynchronizer::new(f.pool.clone());
    let error = sync
        .merge_detail_item_with_lease(f.list_id, f.item_id, "detail-owner", &detail())
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("database refresh is pending"), "{error}");
    assert_eq!(state(&f).await, before);
    let bytes = std::fs::read(&f.path).unwrap();
    assert_eq!(
        document(&f).draft.items[0].metadata.provider["title"],
        "Detailed"
    );
    assert_eq!(document(&f).published, published);
    sqlx::query(r#"DROP TRIGGER "FixtureDetailDigestFailure""#)
        .execute(&f.pool)
        .await
        .unwrap();
    assert!(sync
        .merge_detail_item_with_lease(f.list_id, f.item_id, "detail-owner", &detail())
        .await
        .is_err());
    assert_eq!(state(&f).await, before);
    assert_eq!(std::fs::read(&f.path).unwrap(), bytes);
    let recovery = admission::discover_copied_lists(&f.pool).await.unwrap();
    assert_eq!(recovery.refused, 0);
    assert_eq!(recovery.repaired, 1);
    assert!(recovery.admitted.is_empty());
    assert_eq!(std::fs::read(&f.path).unwrap(), bytes);
    let recovered_document = document(&f);
    assert!(matches!(
        sync.merge_detail_item_with_lease(f.list_id, f.item_id, "detail-owner", &detail())
            .await
            .unwrap(),
        DetailMergeOutcome::Applied
    ));
    let completed = state(&f).await;
    assert_eq!(completed.3, 5);
    let completed_document = document(&f);
    assert_eq!(completed_document.published, recovered_document.published);
    assert_eq!(
        completed_document.draft.items[0].content,
        recovered_document.draft.items[0].content
    );
    assert_eq!(
        completed_document.draft.items[0].metadata,
        recovered_document.draft.items[0].metadata
    );
    let completed_bytes = std::fs::read(&f.path).unwrap();
    assert!(sync
        .merge_detail_item_with_lease(f.list_id, f.item_id, "detail-owner", &detail())
        .await
        .is_err());
    assert_eq!(state(&f).await, completed);
    assert_eq!(std::fs::read(&f.path).unwrap(), completed_bytes);
}
