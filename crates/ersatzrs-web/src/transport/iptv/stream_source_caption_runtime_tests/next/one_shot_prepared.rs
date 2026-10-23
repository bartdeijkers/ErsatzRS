//! Existing immutable publication lifecycle for the source-bound SO3 alternative.
use super::*;
use ersatzrs_core::repositories::{
    AddonMediaListLifecycleRepository as _, AddonMediaListRevisionArtifacts,
};

pub(super) async fn publish(
    state: &AppState,
    item: i64,
    subtitle: i64,
    original: &Path,
    original_sidecar: &Path,
    ffprobe: &str,
) -> (PathBuf, PathBuf) {
    let storage = ersatzrs_infra::file_system_layout::addon_processed_media_folder();
    tokio::fs::create_dir_all(&storage).await.unwrap();
    let root = tempfile::tempdir_in(&storage).unwrap().keep();
    let clip = root.join("published.ts");
    let sidecar = root.join("independent-889.srt");
    tokio::fs::copy(original, &clip).await.unwrap();
    tokio::fs::copy(original_sidecar, &sidecar).await.unwrap();
    let original_bytes = tokio::fs::read(original).await.unwrap();
    assert_eq!(tokio::fs::read(&clip).await.unwrap(), original_bytes);
    assert_eq!(
        tokio::fs::read(&sidecar).await.unwrap(),
        tokio::fs::read(original_sidecar).await.unwrap()
    );
    let mut probe = Command::new(ffprobe);
    probe
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "json",
        ])
        .arg(&clip);
    let facts: serde_json::Value = serde_json::from_slice(&tool(probe).await).unwrap();
    let duration = i64::try_from(
        Duration::from_secs_f64(
            facts["format"]["duration"]
                .as_str()
                .unwrap()
                .parse::<f64>()
                .unwrap(),
        )
        .as_millis(),
    )
    .unwrap();
    assert!(duration >= 9200);
    register(
        state,
        item,
        subtitle,
        &clip,
        &sidecar,
        i64::try_from(original_bytes.len()).unwrap(),
        duration,
    )
    .await;
    (clip, sidecar)
}

async fn register(
    state: &AppState,
    item: i64,
    subtitle: i64,
    clip: &Path,
    sidecar: &Path,
    bytes: i64,
    duration: i64,
) {
    let storage = ersatzrs_infra::file_system_layout::addon_processed_media_folder();
    let relative = clip
        .strip_prefix(&storage)
        .unwrap()
        .components()
        .map(|part| part.as_os_str().to_str().unwrap())
        .collect::<Vec<_>>()
        .join("/");
    sqlx::raw_sql(r#"
        INSERT INTO "AddonMediaList" ("Id","AddonId","ProviderId","SourceUrl","Name","PublishedRevision")
          VALUES (1,'org.ersatzrs.addon.fixture','list','https://example.test/list','Prepared source fixture',1);
    "#).execute(&state.pool).await.unwrap();
    let linked = sqlx::query(r#"INSERT INTO "AddonMediaListLibrary" ("AddonMediaListId","LibraryId","FolderName","ManifestPath")
        SELECT 1, path."LibraryId", 'fixture', 'streams/list.yml' FROM "MediaItem" AS item
        JOIN "LibraryPath" AS path ON path."Id"=item."LibraryPathId" WHERE item."Id"=?"#)
        .bind(item).execute(&state.pool).await.unwrap();
    assert_eq!(linked.rows_affected(), 1);
    sqlx::query(r#"INSERT INTO "AddonMediaListItem" ("Id","AddonMediaListId","ProviderId","DisplayTitle","Kind","Rank","Title","SourceUrl","ProviderLiveness","EffectiveLiveness","Published","MediaItemId")
        VALUES (11,1,'item','Prepared fixture',4,0,'Prepared fixture','https://example.test/one-shot',1,1,1,?)"#)
        .bind(item).execute(&state.pool).await.unwrap();
    sqlx::query(r#"INSERT INTO "AddonMediaListArtifact" ("Id","AddonMediaListId","AddonMediaListItemId","CacheKey","Kind","State","RelativePath","ByteLength","DurationMilliseconds","CreatedAt","UpdatedAt")
        VALUES (1,1,11,'source-bound-fixture',1,2,?,?,?,'2026-10-23','2026-10-23')"#)
        .bind(relative).bind(bytes).bind(duration)
        .execute(&state.pool).await.unwrap();
    state
        .repositories
        .addon_media_lists
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
    sqlx::query(r#"UPDATE "Subtitle" SET "Path"=? WHERE "Id"=?"#)
        .bind(sidecar.to_str().unwrap())
        .bind(subtitle)
        .execute(&state.pool)
        .await
        .unwrap();
}
