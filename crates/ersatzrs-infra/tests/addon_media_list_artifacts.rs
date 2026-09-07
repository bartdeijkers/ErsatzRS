mod common;

use std::fmt::Write as _;

use chrono::Duration;
use ersatzrs_addon_contract::{AddonCapability, MediaListPrepareResult};
use ersatzrs_infra::{
    addon_media_list_item_jobs::*, addon_media_list_options::load_draft_item_options,
};
use sha2::{Digest as _, Sha256};
use sqlx::Executor as _;

fn sha256_hex(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(encoded, "{byte:02x}").unwrap();
    }
    encoded
}

async fn fixture() -> (
    sqlx::SqlitePool,
    AddonMediaListItemJobQueue,
    ItemOperationInput,
) {
    let pool = common::test_db().await;
    pool.execute(r#"INSERT INTO "AddonInstallation" ("AddonId","Version","Enabled","PackageSha256","ContentSha256","ManifestJson","InstallPath","InstalledAt","UpdatedAt") VALUES ('org.example.jobs','1',1,'package','content','{}','addons/fixture','2026-09-05T00:00:00Z','2026-09-05T00:00:00Z')"#).await.unwrap();
    let definition: AddonCapability = serde_json::from_str(r#"{"id":"media-list.options.v1","options":[{"key":"enabled","label":{"en-US":"Enabled"},"description":{"en-US":"Fixture option"},"scope":"both","operation_targets":["test_access","preparation"],"applicability":"finite_only","kind":"boolean","default":false}]}"#).unwrap();
    sqlx::query(r#"INSERT INTO "AddonMediaListOptionSchema" ("Id","AddonId","CapabilityId","SchemaDigest","DefinitionJson","CreatedAt") VALUES (1,'org.example.jobs','media-list.options.v1',?,?,'2026-09-05T00:00:00Z')"#)
        .bind(sha256_hex(&serde_json::to_vec(&definition).unwrap())).bind(serde_json::to_string(&definition).unwrap()).execute(&pool).await.unwrap();
    pool.execute(r#"INSERT INTO "AddonMediaList" ("Id","AddonId","ProviderId","SourceUrl","Name","DraftOptionSchemaId") VALUES (1,'org.example.jobs','list','https://example.test/list','Fixture',1)"#).await.unwrap();
    pool.execute(r#"INSERT INTO "AddonMediaListItem" ("Id","AddonMediaListId","ProviderId","DisplayTitle","Kind","Rank","Title","SourceUrl","ProviderLiveness") VALUES (10,1,'item','Fixture',4,0,'Fixture','https://example.test/item',1)"#).await.unwrap();
    let resolved = load_draft_item_options(&pool, 1, 10)
        .await
        .unwrap()
        .unwrap();
    let package_identity = sha256_hex(
        &serde_json::to_vec(&("org.example.jobs", "1", "package", "content", "initial")).unwrap(),
    );
    let input = ItemOperationInput {
        source_url: "https://example.test/item".into(),
        provider_id: "item".into(),
        context_fingerprint: resolved
            .context_fingerprint(
                "https://example.test/item",
                "item",
                &package_identity,
                "1.0",
                Some(1080),
            )
            .unwrap(),
        schema: Some(resolved.schema),
        definition: Some(resolved.definition),
        options: Some(resolved.values),
        addon_id: None,
        preparation_v2: None,
        package_identity,
        tool_version: "1.0".into(),
        item_is_finite: true,
        max_video_height: Some(1080),
        requested_timing: None,
        requested_subtitle_languages: Vec::new(),
    };
    (pool.clone(), AddonMediaListItemJobQueue::new(pool), input)
}

use ersatzrs_infra::addon_media_list_artifacts::*;

#[tokio::test]
async fn artifact_migration_reservation_quota_and_stale_recovery() {
    let (pool, queue, input) = fixture().await;
    let root = tempfile::tempdir().unwrap();
    let now = common::fixed_time();
    queue
        .enqueue(1, 10, 0, ItemOperationKind::Preparation, input, now)
        .await
        .unwrap();
    let first = queue.claim(now, 30).await.unwrap().unwrap();
    let store = AddonMediaListArtifactStore::new(pool.clone());
    pool.execute(r#"UPDATE "AddonMediaList" SET "ProcessedArtifactByteLimit"=1 WHERE "Id"=1"#)
        .await
        .unwrap();
    assert!(store.reserve(&first, root.path(), now).await.is_err());
    pool.execute(r#"UPDATE "AddonMediaList" SET "ProcessedArtifactByteLimit"=NULL WHERE "Id"=1"#)
        .await
        .unwrap();
    let ArtifactAdmission::Reserved(reservation) =
        store.reserve(&first, root.path(), now).await.unwrap()
    else {
        panic!("expected reservation");
    };
    assert_eq!(reservation.maximum_file_bytes, 2 * 1024 * 1024 * 1024);
    assert!(store.reserve(&first, root.path(), now).await.is_err());
    tokio::fs::write(reservation.staging_directory.join("partial"), b"unfinished")
        .await
        .unwrap();
    assert_eq!(store.check_staging(&reservation).await.unwrap(), 10);
    let later = now + Duration::seconds(31);
    let next = queue.claim(later, 30).await.unwrap().unwrap();
    assert!(store
        .release_after_cleanup(&first, &reservation)
        .await
        .is_err());
    store
        .recover_stale_after_process_cleanup(&next, root.path(), later)
        .await
        .unwrap();
    assert!(!reservation.staging_directory.exists());
    let ArtifactAdmission::Reserved(replacement) =
        store.reserve(&next, root.path(), later).await.unwrap()
    else {
        panic!("expected reservation");
    };
    assert_eq!(reservation.id, replacement.id);
    assert!(store
        .release_after_cleanup(&first, &reservation)
        .await
        .is_err());
    store
        .release_after_cleanup(&next, &replacement)
        .await
        .unwrap();
    let reserved: i64 =
        sqlx::query_scalar(r#"SELECT "ReservedBytes" FROM "AddonMediaListArtifact" WHERE "Id"=?"#)
            .bind(replacement.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(reserved, 0);
}

#[tokio::test]
async fn artifact_staging_counts_intermediate_files_and_rejects_oversize() {
    let (pool, queue, input) = fixture().await;
    let root = tempfile::tempdir().unwrap();
    let now = common::fixed_time();
    queue
        .enqueue(1, 10, 0, ItemOperationKind::Preparation, input, now)
        .await
        .unwrap();
    let job = queue.claim(now, 30).await.unwrap().unwrap();
    let store = AddonMediaListArtifactStore::new(pool);
    let ArtifactAdmission::Reserved(mut reservation) =
        store.reserve(&job, root.path(), now).await.unwrap()
    else {
        panic!("expected reservation");
    };
    tokio::fs::write(reservation.staging_directory.join("video"), b"12345")
        .await
        .unwrap();
    tokio::fs::write(reservation.staging_directory.join("audio"), b"67890")
        .await
        .unwrap();
    reservation.maximum_staging_bytes = 9;
    assert!(store.check_staging(&reservation).await.is_err());
    store
        .release_after_cleanup(&job, &reservation)
        .await
        .unwrap();
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Keep removal, reappearance and cache ownership assertions together.
async fn artifact_reappearance_reattaches_only_unowned_matching_cache_entries() {
    for ready in [false, true] {
        let (pool, queue, input) = fixture().await;
        let root = tempfile::tempdir().unwrap();
        let now = common::fixed_time();
        queue
            .enqueue(1, 10, 0, ItemOperationKind::Preparation, input.clone(), now)
            .await
            .unwrap();
        let first = queue.claim(now, 120).await.unwrap().unwrap();
        let store = AddonMediaListArtifactStore::new(pool.clone());
        let ArtifactAdmission::Reserved(reservation) =
            store.reserve(&first, root.path(), now).await.unwrap()
        else {
            panic!("expected reservation");
        };
        store
            .release_after_cleanup(&first, &reservation)
            .await
            .unwrap();
        if ready {
            // Admission itself is covered by the probe test; seed an already
            // verified file here so ownership recovery also runs on Windows.
            tokio::fs::write(root.path().join("objects/verified.media"), b"media")
                .await
                .unwrap();
            sqlx::query(
                r#"UPDATE "AddonMediaListArtifact" SET "State"=2,
                "RelativePath"='objects/verified.media', "ByteLength"=5,
                "DurationMilliseconds"=1000, "SelectedFormat"='fixture', "ToolVersion"='1.0',
                "CompletedAt"='2026-09-05T00:00:00Z' WHERE "Id"=?"#,
            )
            .bind(reservation.id)
            .execute(&pool)
            .await
            .unwrap();
        }
        // This is the item DELETE used by accepted source tombstones: its FK
        // detaches artifacts, unlike ResetReload which clears them entirely.
        pool.execute(r#"DELETE FROM "AddonMediaListItem" WHERE "Id"=10"#)
            .await
            .unwrap();
        let owner: Option<i64> = sqlx::query_scalar(
            r#"SELECT "AddonMediaListItemId" FROM "AddonMediaListArtifact" WHERE "Id"=?"#,
        )
        .bind(reservation.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(owner.is_none());
        pool.execute(r#"INSERT INTO "AddonMediaListItem" ("Id","AddonMediaListId","ProviderId","DisplayTitle","Kind","Rank","Title","SourceUrl","ProviderLiveness")
            VALUES (11,1,'item','Fixture',4,0,'Fixture','https://example.test/item',1),
                   (12,1,'other','Other',4,1,'Other','https://example.test/other',1)"#)
            .await.unwrap();
        queue
            .enqueue(1, 11, 0, ItemOperationKind::Preparation, input, now)
            .await
            .unwrap();
        let job = queue.claim(now, 120).await.unwrap().unwrap();
        // A matching key never authorizes stealing another current item's file.
        sqlx::query(
            r#"UPDATE "AddonMediaListArtifact" SET "AddonMediaListItemId"=12 WHERE "Id"=?"#,
        )
        .bind(reservation.id)
        .execute(&pool)
        .await
        .unwrap();
        assert!(store.reserve(&job, root.path(), now).await.is_err());
        sqlx::query(
            r#"UPDATE "AddonMediaListArtifact" SET "AddonMediaListItemId"=NULL WHERE "Id"=?"#,
        )
        .bind(reservation.id)
        .execute(&pool)
        .await
        .unwrap();
        match store.reserve(&job, root.path(), now).await.unwrap() {
            ArtifactAdmission::Ready(artifact) => {
                assert!(ready);
                assert_eq!(artifact.id, reservation.id);
                queue
                    .complete(
                        &job,
                        ItemOperationCompletion::Prepared {
                            artifact_id: artifact.id,
                            subtitle_artifact_ids: Vec::new(),
                        },
                        now,
                    )
                    .await
                    .unwrap();
                assert_eq!(
                    tokio::fs::read(root.path().join(artifact.relative_path))
                        .await
                        .unwrap(),
                    b"media"
                );
            }
            ArtifactAdmission::Reserved(retry) => {
                assert!(!ready);
                assert_eq!(retry.id, reservation.id);
                store.release_after_cleanup(&job, &retry).await.unwrap();
            }
        }
        let owner: Option<i64> = sqlx::query_scalar(
            r#"SELECT "AddonMediaListItemId" FROM "AddonMediaListArtifact" WHERE "Id"=?"#,
        )
        .bind(reservation.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(owner, Some(11));
    }
}

#[tokio::test]
async fn artifact_recovery_removes_only_older_job_orphans() {
    let (pool, queue, input) = fixture().await;
    let root = tempfile::tempdir().unwrap();
    let now = common::fixed_time();
    queue
        .enqueue(1, 10, 0, ItemOperationKind::Preparation, input, now)
        .await
        .unwrap();
    let first = queue.claim(now, 30).await.unwrap().unwrap();
    let old = root.path().join("staging").join(format!(
        "{}-{}-{}",
        first.id, first.generation, first.lease_owner
    ));
    tokio::fs::create_dir_all(&old).await.unwrap();
    tokio::fs::write(old.join("partial"), b"synthetic partial")
        .await
        .unwrap();
    let unrelated = root.path().join("staging").join("unrelated");
    tokio::fs::create_dir(&unrelated).await.unwrap();
    let later = now + Duration::seconds(31);
    let next = queue.claim(later, 30).await.unwrap().unwrap();
    let current = root.path().join("staging").join(format!(
        "{}-{}-{}",
        next.id, next.generation, next.lease_owner
    ));
    tokio::fs::create_dir(&current).await.unwrap();
    let store = AddonMediaListArtifactStore::new(pool);
    store
        .recover_stale_after_process_cleanup(&next, root.path(), later)
        .await
        .unwrap();
    assert!(!old.exists());
    assert!(unrelated.exists());
    assert!(current.exists());
}

// POSIX executable fixture; Windows staging/quota/recovery tests above remain native.
#[cfg(unix)]
#[tokio::test]
async fn artifact_promotes_verified_file_and_never_replaces_ready_artifact() {
    use std::{io::Write as _, os::unix::fs::PermissionsExt as _};
    let (pool, queue, input) = fixture().await;
    let root = tempfile::tempdir().unwrap();
    let now = common::fixed_time();
    queue
        .enqueue(1, 10, 0, ItemOperationKind::Preparation, input, now)
        .await
        .unwrap();
    let job = queue.claim(now, 120).await.unwrap().unwrap();
    let store = AddonMediaListArtifactStore::new(pool.clone());
    let ArtifactAdmission::Reserved(reservation) =
        store.reserve(&job, root.path(), now).await.unwrap()
    else {
        panic!("expected reservation");
    };
    let bytes = b"synthetic media file";
    tokio::fs::write(reservation.staging_directory.join("video.mkv"), bytes)
        .await
        .unwrap();
    let mut result = MediaListPrepareResult {
        provider_id: job.input.provider_id.clone(),
        context_fingerprint: job.input.context_fingerprint.clone(),
        tool_version: job.input.tool_version.clone(),
        selected_format: "137+140".into(),
        relative_path: "video.mkv".into(),
        byte_length: bytes.len() as u64,
        sha256: sha256_hex(bytes),
        duration_milliseconds: 1000,
        subtitles: Vec::new(),
    };
    let tools = tempfile::tempdir().unwrap();
    let probe = tools.path().join("ffprobe");
    {
        let mut file = std::fs::File::create(&probe).unwrap();
        file.write_all(b"#!/bin/sh\nprintf '%s' '{\"format\":{\"duration\":\"1.0\"},\"streams\":[{\"index\":0,\"codec_type\":\"video\",\"codec_name\":\"h264\",\"width\":16,\"height\":16}]}'\n").unwrap();
        file.sync_all().unwrap();
    }
    std::fs::set_permissions(&probe, std::fs::Permissions::from_mode(0o700)).unwrap();
    result.sha256 = "0".repeat(64);
    assert!(store
        .promote(&job, &reservation, &result, probe.to_str().unwrap(), now)
        .await
        .is_err());
    result.sha256 = sha256_hex(bytes);
    let admitted = store
        .promote(&job, &reservation, &result, probe.to_str().unwrap(), now)
        .await
        .unwrap();
    assert_eq!(admitted.duration_milliseconds, 1000);
    assert_eq!(
        tokio::fs::read(root.path().join(&admitted.relative_path))
            .await
            .unwrap(),
        bytes
    );
    assert!(store
        .release_after_cleanup(&job, &reservation)
        .await
        .is_err());
    let ArtifactAdmission::Ready(reused) = store.reserve(&job, root.path(), now).await.unwrap()
    else {
        panic!("expected ready");
    };
    assert_eq!(reused, admitted);
}

#[cfg(unix)]
#[tokio::test]
async fn artifact_staging_rejects_symlinks() {
    let (pool, queue, input) = fixture().await;
    let root = tempfile::tempdir().unwrap();
    let now = common::fixed_time();
    queue
        .enqueue(1, 10, 0, ItemOperationKind::Preparation, input, now)
        .await
        .unwrap();
    let job = queue.claim(now, 30).await.unwrap().unwrap();
    let store = AddonMediaListArtifactStore::new(pool);
    let ArtifactAdmission::Reserved(reservation) =
        store.reserve(&job, root.path(), now).await.unwrap()
    else {
        panic!("expected reservation");
    };
    std::os::unix::fs::symlink(root.path(), reservation.staging_directory.join("escape")).unwrap();
    assert!(store.check_staging(&reservation).await.is_err());
}
