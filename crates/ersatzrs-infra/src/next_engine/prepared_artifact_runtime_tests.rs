//! Immutable artifact publication used by the real Next subtitle/audio matrix.
use super::*;
use ersatzrs_core::repositories::{
    AddonMediaListLifecycleRepository, AddonMediaListRevisionArtifacts,
};

pub(super) async fn temporary_root() -> tempfile::TempDir {
    let data = runtime_path(crate::file_system_layout::APP_DATA_DIR_ENV_VAR);
    assert!(
        data.starts_with(std::env::temp_dir()),
        "fixture requires temporary app data"
    );
    assert_eq!(crate::file_system_layout::app_data_folder(), data);
    let folder = crate::file_system_layout::addon_processed_media_folder();
    tokio::fs::create_dir_all(&folder).await.unwrap();
    tempfile::tempdir_in(folder).unwrap()
}

pub(super) async fn run_isolated() {
    run_isolated_test("next_engine::process::runtime_tests::text_subtitle_runtime_tests::fixture_child_prepared_artifacts").await;
}

pub(super) async fn run_isolated_test(test: &str) {
    let root = tempfile::tempdir().unwrap();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", test, "--ignored"])
        .env(crate::file_system_layout::APP_DATA_DIR_ENV_VAR, root.path())
        .env(
            crate::file_system_layout::APP_STATE_DIR_ENV_VAR,
            root.path().join("state"),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = spawn_managed_addon_process(command).unwrap();
    let mut stdout = child.stdout().take().unwrap().take(128 * 1024);
    let observed = timeout(Duration::from_mins(3), async {
        let mut output = String::new();
        stdout.read_to_string(&mut output).await.unwrap();
        (child.wait().await.unwrap(), output)
    })
    .await;
    let Ok((status, output)) = observed else {
        let _ = child.start_kill();
        if !matches!(
            timeout(Duration::from_secs(15), child.wait()).await,
            Ok(Ok(_))
        ) {
            let _ = root.keep();
        }
        panic!("isolated artifact fixture timed out");
    };
    assert!(
        status.success(),
        "isolated artifact fixture failed: {output}; retained at {}",
        root.keep().display()
    );
    assert!(
        output.contains("1 passed"),
        "artifact fixture was not selected"
    );
}

pub(super) async fn publish_fixture(pool: &crate::db::DbPool, clip: &Path) {
    sqlx::raw_sql(r#"
        INSERT INTO "AddonInstallation" ("AddonId","Version","Enabled","PackageSha256","ContentSha256","ManifestJson","InstallPath","InstalledAt","UpdatedAt")
          VALUES ('org.example.fixture','1.0.0',0,'fixture','fixture','{}','addons/fixture','2026-09-05T00:00:00Z','2026-09-05T00:00:00Z');
        INSERT INTO "RemoteStream" ("Id","AddonId","ProviderId","Url","Liveness","IsLive")
          VALUES (10,'org.example.fixture','item','https://example.test/unavailable',0,0);
        UPDATE "MediaVersion" SET "MovieId"=NULL,"RemoteStreamId"=10 WHERE "Id"=100;
        INSERT INTO "RemoteStreamMetadata" ("Id","RemoteStreamId","MetadataKind","Title","DateAdded","DateUpdated")
          VALUES (30,10,0,'Synthetic','2026-09-23','2026-09-23');
        UPDATE "Subtitle" SET "MovieMetadataId"=NULL,"RemoteStreamMetadataId"=30;
        DELETE FROM "MovieMetadata" WHERE "MovieId"=10;
        DELETE FROM "Movie" WHERE "Id"=10;
        UPDATE "MediaFile" SET "Path"='unavailable/published-only.mkv' WHERE "Id"=1;
        INSERT INTO "AddonMediaList" ("Id","AddonId","ProviderId","SourceUrl","Name","PublishedRevision")
          VALUES (1,'org.example.fixture','list','https://example.test/list','Fixture',1);
        INSERT INTO "AddonMediaListLibrary" ("AddonMediaListId","LibraryId","FolderName","ManifestPath") VALUES (1,1,'fixture','streams/list.yml');
        INSERT INTO "AddonMediaListItem" ("Id","AddonMediaListId","ProviderId","DisplayTitle","Kind","Rank","Title","SourceUrl","ProviderLiveness","EffectiveLiveness","Published","MediaItemId")
          VALUES (11,1,'item','Fixture',4,0,'Fixture','https://example.test/unavailable',1,1,1,10);
    "#).execute(pool).await.unwrap();
    let processed_root = crate::file_system_layout::addon_processed_media_folder();
    // Published artifact paths use the portable slash-separated storage format.
    let relative = clip
        .strip_prefix(&processed_root)
        .unwrap()
        .components()
        .map(|part| part.as_os_str().to_str().unwrap())
        .collect::<Vec<_>>()
        .join("/");
    assert_eq!(
        crate::addon_media_list_artifacts::confined_file(&processed_root, &relative)
            .await
            .expect("published fixture path must resolve before playback"),
        clip
    );
    let bytes = i64::try_from(tokio::fs::metadata(clip).await.unwrap().len()).unwrap();
    sqlx::query(r#"INSERT INTO "AddonMediaListArtifact" ("Id","AddonMediaListId","AddonMediaListItemId","CacheKey","Kind","State","RelativePath","ByteLength","DurationMilliseconds","CreatedAt","UpdatedAt")
          VALUES (1,1,11,'fixture',1,2,?,?,60000,'2026-09-23','2026-09-23')"#)
        .bind(relative).bind(bytes).execute(pool).await.unwrap();
    crate::repositories::SqliteAddonMediaListRepository(pool.clone())
        .retain_published_revision(
            1,
            1,
            0,
            chrono::Utc::now(),
            &[],
            &AddonMediaListRevisionArtifacts {
                ids: vec![1],
                prepared_media: [("item".into(), 1)].into_iter().collect(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
}
