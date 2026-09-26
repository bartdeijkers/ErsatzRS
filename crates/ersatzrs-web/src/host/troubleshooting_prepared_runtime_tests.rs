//! Immutable published artifact with a disabled provider and unreachable source URL.
use super::*;
use ersatzrs_core::repositories::{
    AddonMediaListLifecycleRepository as _, AddonMediaListRevisionArtifacts,
};

pub(super) async fn install(fixture: &Fixture) -> tempfile::TempDir {
    let data = runtime_path(ersatzrs_infra::file_system_layout::APP_DATA_DIR_ENV_VAR);
    assert!(
        data.starts_with(std::env::temp_dir()),
        "prepared fixture needs isolated app data"
    );
    let processed = ersatzrs_infra::file_system_layout::addon_processed_media_folder();
    assert!(processed.starts_with(&data));
    tokio::fs::create_dir_all(&processed).await.unwrap();
    let artifact = tempfile::tempdir_in(&processed).unwrap();
    let clip = artifact.path().join("prepared.mkv");
    tokio::fs::copy(fixture.root.path().join("fixture.mkv"), &clip)
        .await
        .unwrap();
    let pool = &fixture.state.pool;
    sqlx::raw_sql(r#"
        INSERT INTO "AddonInstallation" ("AddonId","Version","Enabled","PackageSha256","ContentSha256","ManifestJson","InstallPath","InstalledAt","UpdatedAt")
          VALUES ('org.example.fixture','1.0.0',0,'fixture','fixture','{}','addons/fixture','2026-09-26','2026-09-26');
        UPDATE "RemoteStream" SET "AddonId"='org.example.fixture',"ProviderId"='item',"Url"='https://example.test/unavailable',"Liveness"=0 WHERE "Id"=10;
        UPDATE "MediaFile" SET "Path"='unavailable/published-only.mkv' WHERE "Id"=1;
        INSERT INTO "AddonMediaList" ("Id","AddonId","ProviderId","SourceUrl","Name","PublishedRevision")
          VALUES (1,'org.example.fixture','list','https://example.test/list','Fixture',1);
        INSERT INTO "AddonMediaListLibrary" ("AddonMediaListId","LibraryId","FolderName","ManifestPath") VALUES (1,1,'fixture','streams/list.yml');
        INSERT INTO "AddonMediaListItem" ("Id","AddonMediaListId","ProviderId","DisplayTitle","Kind","Rank","Title","SourceUrl","ProviderLiveness","EffectiveLiveness","Published","MediaItemId")
          VALUES (11,1,'item','Fixture',4,0,'Fixture','https://example.test/unavailable',1,1,1,10);
    "#).execute(pool).await.unwrap();
    let relative = clip.strip_prefix(&processed).unwrap().to_str().unwrap();
    let bytes = i64::try_from(tokio::fs::metadata(&clip).await.unwrap().len()).unwrap();
    sqlx::query(r#"INSERT INTO "AddonMediaListArtifact" ("Id","AddonMediaListId","AddonMediaListItemId","CacheKey","Kind","State","RelativePath","ByteLength","DurationMilliseconds","CreatedAt","UpdatedAt")
          VALUES (1,1,11,'fixture',1,2,?,?,6000,'2026-09-26','2026-09-26')"#)
        .bind(relative).bind(bytes).execute(pool).await.unwrap();
    ersatzrs_infra::repositories::SqliteAddonMediaListRepository(pool.clone())
        .retain_published_revision(
            1,
            1,
            0,
            Utc::now(),
            &[],
            &AddonMediaListRevisionArtifacts {
                ids: vec![1],
                prepared_media: [("item".into(), 1)].into_iter().collect(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    artifact
}
