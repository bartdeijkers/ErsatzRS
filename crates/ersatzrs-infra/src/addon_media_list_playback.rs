//! Playback follows the current immutable publication, never mutable draft options.

use std::path::Path;

use ersatzrs_core::repositories::{
    AddonMediaListLifecycleRepository as _, RepositoryError, Result,
};
use ersatzrs_domain::RemoteStreamLiveness;
use sqlx::Row as _;

use crate::{
    addon_media_list_options::load_published_item_options,
    addons::AddonRuntimeContext,
    db::DbPool,
    remote_stream::{RemoteStreamDefinition, RemoteStreamSource, ResolvedRemoteStream},
    repositories::{SqliteAddonMediaListRepository, SqlxResultExt as _},
};

/// Resolve a selected media version's remote-stream identity through its owning list.
/// `None` means that the stream has no managed-list owner. A scheduled version
/// does not pin a publication: this selects the list's current published revision.
///
/// # Errors
/// Rejects incomplete or inconsistent managed ownership, invalid snapshots,
/// unavailable prepared files, and unsupported options-aware playback.
#[allow(clippy::too_many_lines)] // Keep managed ownership, explicit legacy adaptation and fail-closed selection adjacent.
pub async fn resolve_published_remote_stream(
    pool: &DbPool,
    remote_stream_id: i64,
    runtime: &AddonRuntimeContext,
    artifact_root: &Path,
) -> Result<Option<ResolvedRemoteStream>> {
    let owner = sqlx::query(
        r#"SELECT "list"."Id" AS "ListId", "list"."PublishedRevision", "list"."AddonId",
                  "stream"."AddonId" AS "StreamAddonId", "stream"."ProviderId", "stream"."Url",
                  "stream"."FallbackQuery", "stream"."Duration", "stream"."Liveness", "stream"."IsLive"
           FROM "RemoteStream" AS "stream"
           JOIN "MediaItem" AS "media" ON "media"."Id"="stream"."Id"
           JOIN "LibraryPath" AS "path" ON "path"."Id"="media"."LibraryPathId"
           JOIN "AddonMediaListLibrary" AS "binding" ON "binding"."LibraryId"="path"."LibraryId"
           JOIN "AddonMediaList" AS "list" ON "list"."Id"="binding"."AddonMediaListId"
           WHERE "stream"."Id"=?"#,
    )
    .bind(remote_stream_id)
    .fetch_optional(pool)
    .await
    .repository()?;
    let Some(owner) = owner else {
        return Ok(None);
    };
    let list_id: i64 = owner.try_get("ListId").repository()?;
    let revision: i32 = owner.try_get("PublishedRevision").repository()?;
    let addon_id: String = owner.try_get("AddonId").repository()?;
    if revision <= 0
        || owner
            .try_get::<Option<String>, _>("StreamAddonId")
            .repository()?
            .as_deref()
            != Some(&addon_id)
    {
        return Err(unavailable());
    }
    let provider_id: String = owner
        .try_get::<Option<String>, _>("ProviderId")
        .repository()?
        .ok_or_else(unavailable)?;
    let source_url: String = owner
        .try_get::<Option<String>, _>("Url")
        .repository()?
        .ok_or_else(unavailable)?;
    let snapshot = SqliteAddonMediaListRepository(pool.clone())
        .revision_snapshot(list_id, revision)
        .await?;
    let Some(snapshot) = snapshot else {
        // K-01 backfilled current legacy revisions without invented completion
        // times or snapshots. Only their explicitly unbound state keeps v1/v2.
        let legacy: bool = sqlx::query_scalar(r#"SELECT EXISTS (
            SELECT 1 FROM "AddonMediaListRevision" AS "revision"
            JOIN "AddonMediaListItem" AS "item" ON "item"."AddonMediaListId"="revision"."AddonMediaListId"
            WHERE "revision"."AddonMediaListId"=? AND "revision"."Revision"=?
              AND "revision"."OptionSchemaId" IS NULL AND "revision"."OptionValuesJson"='{}'
              AND "revision"."SnapshotVersion" IS NULL AND "revision"."SnapshotDigest" IS NULL
              AND "item"."ProviderId"=? AND "item"."MediaItemId"=? AND "item"."Published"=1
              AND NOT EXISTS (SELECT 1 FROM "AddonMediaListRevisionItemOption" AS "option"
                  WHERE "option"."AddonMediaListRevisionId"="revision"."Id")
              AND NOT EXISTS (SELECT 1 FROM "AddonMediaListRevisionArtifact" AS "artifact"
                  WHERE "artifact"."AddonMediaListRevisionId"="revision"."Id"))"#)
            .bind(list_id).bind(revision).bind(&provider_id).bind(remote_stream_id)
            .fetch_one(pool).await.repository()?;
        if !legacy {
            return Err(unavailable());
        }
        let is_live = owner.try_get::<i64, _>("IsLive").repository()? != 0;
        let liveness = crate::remote_stream::decode_persisted_liveness(
            owner.try_get("Liveness").repository()?,
            is_live,
        )
        .map_err(|_| unavailable())?;
        let definition = RemoteStreamDefinition {
            provider_id: Some(provider_id),
            addon: Some(addon_id),
            url: Some(source_url),
            duration: owner.try_get("Duration").repository()?,
            fallback_query: owner.try_get("FallbackQuery").repository()?,
            liveness: Some(liveness),
            is_live,
            ..Default::default()
        };
        let stream =
            crate::remote_stream::resolve_remote_stream_with_addons(pool, &definition, runtime)
                .await
                .map_err(|_| unavailable())?;
        require_current_revision(pool, list_id, revision).await?;
        return Ok(Some(stream));
    };
    let item = snapshot
        .items
        .iter()
        .find(|item| item.provider_id == provider_id && item.published)
        .ok_or_else(unavailable)?;
    if item.source_url.as_deref() != Some(&source_url) || item.replacement_media_item_id.is_some() {
        return Err(unavailable());
    }
    let (liveness, options) =
        load_published_item_options(pool, list_id, revision, &provider_id, &source_url).await?;
    let fallback_query = owner.try_get("FallbackQuery").repository()?;
    let resolved = if let Some(plan) = &item.playback_timeline {
        if liveness != RemoteStreamLiveness::Finite || item.prepared_artifact_id.is_some() {
            return Err(unavailable());
        }
        let pending = crate::published_interval_source::PublishedIntervalSource::new(
            addon_id,
            source_url,
            provider_id,
            options.ok_or_else(unavailable)?,
            plan.clone(),
        )
        .map_err(|_| unavailable())?;
        ResolvedRemoteStream {
            source: RemoteStreamSource::RetainedIntervals(Box::new(pending)),
            liveness,
            is_live: false,
            configured_duration: Some(clock_duration(plan.duration_milliseconds())),
            fallback_query,
            uses_addon: true,
        }
    } else if let Some(artifact_id) = item.prepared_artifact_id {
        if liveness != RemoteStreamLiveness::Finite || !snapshot.artifact_ids.contains(&artifact_id)
        {
            return Err(unavailable());
        }
        resolve_prepared(pool, list_id, artifact_id, artifact_root, fallback_query).await?
    } else {
        if options
            .as_ref()
            .is_some_and(|options| options.requires_preparation(liveness))
        {
            return Err(unavailable());
        }
        let definition = RemoteStreamDefinition {
            provider_id: Some(provider_id),
            url: Some(source_url),
            addon: Some(addon_id),
            duration: item
                .duration_seconds
                .map(|seconds| clock_duration(seconds.saturating_mul(1000))),
            fallback_query,
            liveness: Some(liveness),
            is_live: liveness.legacy_is_live(),
            ..Default::default()
        };
        if let Some(options) = options {
            crate::remote_stream::resolve_remote_stream_with_item_options(
                pool,
                &definition,
                runtime,
                &options,
            )
            .await
        } else {
            crate::remote_stream::resolve_remote_stream_with_addons(pool, &definition, runtime)
                .await
        }
        .map_err(|_| unavailable())?
    };
    // Publication can change while files or package metadata are being read.
    require_current_revision(pool, list_id, revision).await?;
    Ok(Some(resolved))
}

async fn require_current_revision(pool: &DbPool, list_id: i64, revision: i32) -> Result<()> {
    let current: Option<i32> =
        sqlx::query_scalar(r#"SELECT "PublishedRevision" FROM "AddonMediaList" WHERE "Id"=?"#)
            .bind(list_id)
            .fetch_optional(pool)
            .await
            .repository()?;
    if current != Some(revision) {
        return Err(unavailable());
    }
    Ok(())
}

async fn resolve_prepared(
    pool: &DbPool,
    list_id: i64,
    artifact_id: i64,
    root: &Path,
    fallback_query: Option<String>,
) -> Result<ResolvedRemoteStream> {
    let artifact = sqlx::query(r#"SELECT "RelativePath", "ByteLength", "DurationMilliseconds"
        FROM "AddonMediaListArtifact" WHERE "Id"=? AND "AddonMediaListId"=? AND "Kind"=1 AND "State"=2"#)
        .bind(artifact_id).bind(list_id).fetch_optional(pool).await.repository()?.ok_or_else(unavailable)?;
    let relative: String = artifact
        .try_get::<Option<String>, _>("RelativePath")
        .repository()?
        .ok_or_else(unavailable)?;
    let bytes = u64::try_from(artifact.try_get::<i64, _>("ByteLength").repository()?)
        .map_err(|_| unavailable())?;
    let duration = u64::try_from(
        artifact
            .try_get::<i64, _>("DurationMilliseconds")
            .repository()?,
    )
    .map_err(|_| unavailable())?;
    if bytes == 0 || duration == 0 {
        return Err(unavailable());
    }
    let root = tokio::fs::canonicalize(root)
        .await
        .map_err(|_| unavailable())?;
    let path = crate::addon_media_list_artifacts::confined_file(&root, &relative).await?;
    if tokio::fs::metadata(&path)
        .await
        .map_err(|_| unavailable())?
        .len()
        != bytes
    {
        return Err(unavailable());
    }
    Ok(ResolvedRemoteStream {
        source: RemoteStreamSource::Prepared(path),
        liveness: RemoteStreamLiveness::Finite,
        is_live: false,
        configured_duration: Some(clock_duration(duration)),
        fallback_query,
        uses_addon: false,
    })
}

fn clock_duration(milliseconds: u64) -> String {
    let seconds = milliseconds / 1000;
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        seconds / 3600,
        (seconds / 60) % 60,
        seconds % 60,
        milliseconds % 1000
    )
}

fn unavailable() -> RepositoryError {
    RepositoryError::unavailable(
        "published item playback",
        "the published item or its required processed media is unavailable",
    )
}

#[cfg(test)]
mod tests {
    use ersatzrs_core::repositories::AddonMediaListRevisionArtifacts;

    use super::*;

    async fn fixture() -> (tempfile::TempDir, DbPool) {
        let temporary = tempfile::tempdir().unwrap();
        let pool = crate::db::init(&format!(
            "sqlite:{}",
            temporary.path().join("fixture.db").display()
        ))
        .await
        .unwrap();
        sqlx::raw_sql(r#"
            INSERT INTO "AddonInstallation" ("AddonId","Version","Enabled","PackageSha256","ContentSha256","ManifestJson","InstallPath","InstalledAt","UpdatedAt")
                VALUES ('org.example.fixture','1.0.0',1,'fixture','fixture','{}','addons/fixture','2026-09-05T00:00:00Z','2026-09-05T00:00:00Z');
            INSERT INTO "MediaSource" ("Id") VALUES (1);
            INSERT INTO "Library" ("Id","MediaKind","MediaSourceId") VALUES (1,4,1);
            INSERT INTO "LibraryPath" ("Id","LibraryId","Path") VALUES (1,1,'streams');
            INSERT INTO "MediaItem" ("Id","LibraryPathId") VALUES (100,1);
            INSERT INTO "RemoteStream" ("Id","AddonId","ProviderId","Url") VALUES (100,'org.example.fixture','item','https://example.test/item');
            INSERT INTO "AddonMediaList" ("Id","AddonId","ProviderId","SourceUrl","Name","PublishedRevision")
                VALUES (1,'org.example.fixture','list','https://example.test/list','Fixture',1);
            INSERT INTO "AddonMediaListLibrary" ("AddonMediaListId","LibraryId","FolderName","ManifestPath")
                VALUES (1,1,'fixture','streams/list.yml');
            INSERT INTO "AddonMediaListItem" ("Id","AddonMediaListId","ProviderId","DisplayTitle","Kind","Rank","Title","SourceUrl","ProviderLiveness","EffectiveLiveness","Published","MediaItemId")
                VALUES (10,1,'item','Fixture',4,0,'Fixture','https://example.test/item',1,1,1,100);
            INSERT INTO "AddonMediaListArtifact" ("Id","AddonMediaListId","AddonMediaListItemId","CacheKey","Kind","State","RelativePath","ByteLength","DurationMilliseconds","CreatedAt","UpdatedAt")
                VALUES (1,1,10,'fixture',1,2,'objects/prepared.media',5,12500,'2026-09-05T00:00:00Z','2026-09-05T00:00:00Z');
        "#).execute(&pool).await.unwrap();
        tokio::fs::create_dir(temporary.path().join("objects"))
            .await
            .unwrap();
        tokio::fs::write(temporary.path().join("objects/prepared.media"), b"media")
            .await
            .unwrap();
        SqliteAddonMediaListRepository(pool.clone())
            .retain_published_revision(
                1,
                1,
                0,
                chrono::Utc::now(),
                &[],
                &AddonMediaListRevisionArtifacts {
                    ids: vec![1],
                    prepared_media: std::collections::BTreeMap::from([("item".into(), 1)]),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        (temporary, pool)
    }

    #[tokio::test]
    async fn published_prepared_playback_survives_draft_owner_deletion() {
        let (temporary, pool) = fixture().await;
        sqlx::query(r#"DELETE FROM "AddonMediaListItem" WHERE "Id"=10"#)
            .execute(&pool)
            .await
            .unwrap();
        let stream = resolve_published_remote_stream(
            &pool,
            100,
            &AddonRuntimeContext::default(),
            temporary.path(),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(stream.configured_duration.as_deref(), Some("00:00:12.500"));
        assert_eq!(
            stream.source,
            RemoteStreamSource::Prepared(temporary.path().join("objects/prepared.media"))
        );
        assert!(!stream.uses_addon);
    }

    async fn interval_fixture() -> (tempfile::TempDir, DbPool, ResolvedRemoteStream) {
        use sha2::{Digest as _, Sha256};
        use std::fmt::Write as _;

        let (temporary, pool) = fixture().await;
        let definition: ersatzrs_addon_contract::AddonCapability =
            serde_json::from_value(serde_json::json!({
                "id":"media-list.options.v1", "options":[{
                    "key":"cut", "label":{"en-US":"Cut"},
                    "description":{"en-US":"Fixture cut"}, "scope":"both",
                    "operation_targets":["preparation","playback"],
                    "applicability":"finite_only", "kind":"boolean",
                    "default":true, "requires_preparation":true
                }]
            }))
            .unwrap();
        let definition_json = serde_json::to_string(&definition).unwrap();
        let mut digest = String::new();
        for byte in Sha256::digest(definition_json.as_bytes()) {
            write!(digest, "{byte:02x}").unwrap();
        }
        sqlx::query(
            r#"INSERT INTO "AddonMediaListOptionSchema"
            ("Id","AddonId","CapabilityId","SchemaDigest","DefinitionJson","CreatedAt")
            VALUES (1,'org.example.fixture','media-list.options.v1',?,?,'2026-09-05T00:00:00Z')"#,
        )
        .bind(digest)
        .bind(definition_json)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(r#"UPDATE "AddonMediaList" SET "DraftOptionSchemaId"=1, "PublishedRevision"=2 WHERE "Id"=1"#)
            .execute(&pool).await.unwrap();
        let excluded = 2_000..4_000;
        let plan =
            ersatzrs_core::addon_media_list_playback::AddonMediaListPlaybackTimelinePlan::new(
                10_000,
                0..10_000,
                std::slice::from_ref(&excluded),
                "a".repeat(64),
            )
            .unwrap();
        SqliteAddonMediaListRepository(pool.clone())
            .retain_published_revision(
                1,
                2,
                0,
                chrono::Utc::now(),
                &[],
                &AddonMediaListRevisionArtifacts {
                    streaming_plans: std::collections::BTreeMap::from([("item".into(), plan)]),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        // The installation is deliberately unusable (manifest {} and no executable).
        // Selecting the publication must not inspect or invoke the provider.
        let resolved = resolve_published_remote_stream(
            &pool,
            100,
            &AddonRuntimeContext::default(),
            temporary.path(),
        )
        .await
        .unwrap()
        .unwrap();
        (temporary, pool, resolved)
    }

    #[tokio::test]
    async fn published_interval_intent_is_frozen_redacted_and_not_a_direct_input() {
        let (temporary, pool, first) = interval_fixture().await;
        sqlx::query(r#"UPDATE "AddonMediaListItem" SET "DurationSeconds"=999, "FragmentStartSeconds"=7 WHERE "Id"=10"#)
            .execute(&pool).await.unwrap();
        sqlx::query(
            r#"UPDATE "AddonMediaList" SET "DraftOptionValuesJson"='{"cut":false}' WHERE "Id"=1"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        let second = resolve_published_remote_stream(
            &pool,
            100,
            &AddonRuntimeContext::default(),
            temporary.path(),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(first, second);
        assert_eq!(second.configured_duration.as_deref(), Some("00:00:08.000"));
        let RemoteStreamSource::RetainedIntervals(pending) = &second.source else {
            panic!("expected frozen interval intent");
        };
        assert_eq!(pending.plan().source_window_milliseconds(), 0..10_000);
        assert_eq!(format!("{pending:?}"), "PublishedIntervalSource { .. }");
        assert!(matches!(
            crate::remote_stream::remote_stream_input_file(&second),
            Err(crate::remote_stream::RemoteStreamError::UnresolvedIntervals)
        ));
        assert!(crate::remote_stream::remote_stream_producer(&second).is_none());
        assert_eq!(
            crate::remote_stream::redact_remote_stream_output(
                "synthetic-private-capability",
                &second
            ),
            "published interval media diagnostics redacted"
        );
    }

    #[tokio::test]
    async fn interval_startup_load_obeys_deadline_and_cancellation() {
        use crate::addon_media_list_item_runtime::ItemRuntimeError;
        use std::time::Duration;

        let (_temporary, pool, stream) = interval_fixture().await;
        let RemoteStreamSource::RetainedIntervals(pending) = stream.source else {
            panic!("expected frozen interval intent");
        };
        let probe = crate::ffprobe::FfprobeExtractor::new("must-not-be-spawned");
        let context = || crate::published_interval_source::IntervalPlaybackContext {
            pool: &pool,
            probe: &probe,
            spawn_context: crate::remote_stream::RemoteStreamSpawnContext::probe(),
        };
        let mut held = Vec::new();
        for _ in 0..pool.options().get_max_connections() {
            held.push(pool.acquire().await.unwrap());
        }
        let (_sender, receiver) = tokio::sync::oneshot::channel();
        let result = tokio::time::timeout(
            Duration::from_secs(1),
            pending.prepare_playback_cancellable(
                context(),
                0,
                None,
                tokio::time::Instant::now() + Duration::from_millis(10),
                receiver,
            ),
        )
        .await
        .unwrap();
        assert!(matches!(result, Err(ItemRuntimeError::Failed)));
        let (sender, receiver) = tokio::sync::oneshot::channel();
        sender.send(()).unwrap();
        let result = tokio::time::timeout(
            Duration::from_secs(1),
            pending.prepare_playback_cancellable(
                context(),
                0,
                None,
                tokio::time::Instant::now() + Duration::from_secs(15),
                receiver,
            ),
        )
        .await
        .unwrap();
        assert!(matches!(result, Err(ItemRuntimeError::Failed)));
        let (_sender, receiver) = tokio::sync::oneshot::channel();
        assert!(matches!(
            pending
                .prepare_playback_cancellable(
                    context(),
                    0,
                    None,
                    tokio::time::Instant::now(),
                    receiver
                )
                .await,
            Err(ItemRuntimeError::Failed)
        ));
        drop(held);
    }

    #[tokio::test]
    async fn equal_source_urls_keep_distinct_list_and_provider_artifacts() {
        let (temporary, pool) = fixture().await;
        sqlx::raw_sql(r#"
            INSERT INTO "Library" ("Id","MediaKind","MediaSourceId") VALUES (2,4,1);
            INSERT INTO "LibraryPath" ("Id","LibraryId","Path") VALUES (2,2,'other-streams');
            INSERT INTO "MediaItem" ("Id","LibraryPathId") VALUES (200,2);
            INSERT INTO "RemoteStream" ("Id","AddonId","ProviderId","Url") VALUES (200,'org.example.fixture','other-item','https://example.test/item');
            INSERT INTO "AddonMediaList" ("Id","AddonId","ProviderId","SourceUrl","Name","PublishedRevision")
                VALUES (2,'org.example.fixture','other-list','https://example.test/other-list','Other fixture',1);
            INSERT INTO "AddonMediaListLibrary" ("AddonMediaListId","LibraryId","FolderName","ManifestPath")
                VALUES (2,2,'other-fixture','other-streams/list.yml');
            INSERT INTO "AddonMediaListItem" ("Id","AddonMediaListId","ProviderId","DisplayTitle","Kind","Rank","Title","SourceUrl","ProviderLiveness","EffectiveLiveness","Published","MediaItemId")
                VALUES (20,2,'other-item','Other fixture',4,0,'Other fixture','https://example.test/item',1,1,1,200);
            INSERT INTO "AddonMediaListArtifact" ("Id","AddonMediaListId","AddonMediaListItemId","CacheKey","Kind","State","RelativePath","ByteLength","DurationMilliseconds","CreatedAt","UpdatedAt")
                VALUES (2,2,20,'other-fixture',1,2,'objects/other.media',5,6000,'2026-09-05T00:00:00Z','2026-09-05T00:00:00Z');
        "#).execute(&pool).await.unwrap();
        tokio::fs::write(temporary.path().join("objects/other.media"), b"other")
            .await
            .unwrap();
        SqliteAddonMediaListRepository(pool.clone())
            .retain_published_revision(
                2,
                1,
                0,
                chrono::Utc::now(),
                &[],
                &AddonMediaListRevisionArtifacts {
                    ids: vec![2],
                    prepared_media: std::collections::BTreeMap::from([("other-item".into(), 2)]),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        for (media_id, expected_path) in [
            (100, "objects/prepared.media"),
            (200, "objects/other.media"),
        ] {
            let resolved = resolve_published_remote_stream(
                &pool,
                media_id,
                &AddonRuntimeContext::default(),
                temporary.path(),
            )
            .await
            .unwrap()
            .unwrap();
            assert_eq!(
                resolved.source,
                RemoteStreamSource::Prepared(temporary.path().join(expected_path))
            );
        }
    }

    #[tokio::test]
    async fn managed_failures_never_fall_back_to_raw_source() {
        for mutation in [
            r#"UPDATE "AddonMediaList" SET "PublishedRevision"=0"#,
            r#"UPDATE "RemoteStream" SET "ProviderId"='other'"#,
            r#"UPDATE "RemoteStream" SET "AddonId"=NULL"#,
            r#"UPDATE "RemoteStream" SET "Url"='https://example.test/other'"#,
            r#"UPDATE "AddonMediaListArtifact" SET "State"=0"#,
            r#"UPDATE "AddonMediaListArtifact" SET "ByteLength"=6"#,
            r#"UPDATE "AddonMediaListArtifact" SET "RelativePath"='../outside.media'"#,
            r#"DELETE FROM "AddonMediaListRevisionArtifact""#,
        ] {
            let (temporary, pool) = fixture().await;
            sqlx::query(mutation).execute(&pool).await.unwrap();
            assert!(
                resolve_published_remote_stream(
                    &pool,
                    100,
                    &AddonRuntimeContext::default(),
                    temporary.path()
                )
                .await
                .is_err(),
                "{mutation}"
            );
        }
    }

    #[tokio::test]
    async fn only_unmanaged_sources_return_none() {
        let (temporary, pool) = fixture().await;
        sqlx::query(r#"DELETE FROM "AddonMediaListLibrary""#)
            .execute(&pool)
            .await
            .unwrap();
        assert!(resolve_published_remote_stream(
            &pool,
            100,
            &AddonRuntimeContext::default(),
            temporary.path()
        )
        .await
        .unwrap()
        .is_none());
    }

    // POSIX package fixture only; no command is executed by these resolver tests.
    #[cfg(unix)]
    #[tokio::test]
    #[allow(clippy::too_many_lines)] // Keep the installed schema and v3/legacy compatibility assertions together.
    async fn direct_options_require_v3_and_leave_legacy_commands_unchanged() {
        use crate::addon_media_list_options::ResolvedAddonItemOptions;
        use ersatzrs_addon_contract::{
            AddonCapability, AddonMediaListOptionValue, AddonMediaListOptionValues,
            ENV_MEDIA_LIST_OPTIONS,
        };
        use ersatzrs_core::repositories::AddonMediaListOptionSchemaRef;
        use sha2::{Digest as _, Sha256};
        use std::fmt::Write as _;

        let temporary = tempfile::tempdir().unwrap();
        let pool = crate::addon_media_list::tests::fixture_media_list_addon_with_script(
            &temporary.path().join("package"),
            "#!/bin/sh\nexit 99\n",
            &["example.test"],
        )
        .await;
        let declaration: AddonCapability = serde_json::from_value(serde_json::json!({
            "id":"media-list.options.v1", "options":[{
                "key":"enabled", "label":{"en-US":"Enabled"}, "description":{"en-US":"Fixture"},
                "scope":"both", "operation_targets":["playback"], "applicability":"all",
                "kind":"boolean", "default":false
            }]
        }))
        .unwrap();
        let mut digest = String::with_capacity(64);
        for byte in Sha256::digest(serde_json::to_vec(&declaration).unwrap()) {
            write!(digest, "{byte:02x}").unwrap();
        }
        let original: String =
            sqlx::query_scalar(r#"SELECT "ManifestJson" FROM "AddonInstallation" LIMIT 1"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        let mut manifest: serde_json::Value = serde_json::from_str(&original).unwrap();
        manifest["manifest_version"] = serde_json::json!(5);
        manifest["capabilities"].as_array_mut().unwrap().extend([
            serde_json::to_value(&declaration).unwrap(),
            serde_json::json!({"id":"remote-stream.play.v2"}),
            serde_json::json!({"id":"remote-stream.play.v3"}),
        ]);
        sqlx::query(r#"UPDATE "AddonInstallation" SET "ManifestJson"=?"#)
            .bind(manifest.to_string())
            .execute(&pool)
            .await
            .unwrap();
        let options = ResolvedAddonItemOptions {
            schema: AddonMediaListOptionSchemaRef {
                addon_id: "org.ersatzrs.addon.fixture".into(),
                capability_id: declaration.id.clone(),
                schema_digest: digest,
            },
            values: AddonMediaListOptionValues {
                schema: declaration.id.clone(),
                values: std::collections::BTreeMap::from([(
                    "enabled".into(),
                    AddonMediaListOptionValue::Boolean(true),
                )]),
            },
            definition: declaration,
        };
        let definition = RemoteStreamDefinition {
            addon: Some("org.ersatzrs.addon.fixture".into()),
            url: Some("https://example.test/item".into()),
            liveness: Some(RemoteStreamLiveness::Finite),
            ..Default::default()
        };
        let runtime = AddonRuntimeContext {
            ffmpeg_path: Some("fixture-ffmpeg".into()),
            ffprobe_path: Some("fixture-ffprobe".into()),
        };
        let legacy_before =
            crate::remote_stream::resolve_remote_stream_with_addons(&pool, &definition, &runtime)
                .await
                .unwrap();
        let resolved = crate::remote_stream::resolve_remote_stream_with_item_options(
            &pool,
            &definition,
            &runtime,
            &options,
        )
        .await
        .unwrap();
        let RemoteStreamSource::Addon(command) = resolved.source else {
            panic!("expected provider command");
        };
        let envelope: AddonMediaListOptionValues =
            serde_json::from_str(&command.environment[ENV_MEDIA_LIST_OPTIONS]).unwrap();
        assert_eq!(envelope, options.values);
        assert_eq!(
            command.environment["ERSATZRS_ADDON_CAPABILITY"],
            "remote-stream.play.v3"
        );
        manifest["capabilities"]
            .as_array_mut()
            .unwrap()
            .retain(|capability| capability["id"] != "remote-stream.play.v3");
        sqlx::query(r#"UPDATE "AddonInstallation" SET "ManifestJson"=?"#)
            .bind(manifest.to_string())
            .execute(&pool)
            .await
            .unwrap();
        assert!(
            crate::remote_stream::resolve_remote_stream_with_item_options(
                &pool,
                &definition,
                &runtime,
                &options
            )
            .await
            .is_err()
        );
        assert_eq!(
            crate::remote_stream::resolve_remote_stream_with_addons(&pool, &definition, &runtime)
                .await
                .unwrap(),
            legacy_before
        );
        sqlx::raw_sql(r#"
            INSERT INTO "MediaSource" ("Id") VALUES (1);
            INSERT INTO "Library" ("Id","MediaKind","MediaSourceId") VALUES (1,4,1);
            INSERT INTO "LibraryPath" ("Id","LibraryId","Path") VALUES (1,1,'streams');
            INSERT INTO "MediaItem" ("Id","LibraryPathId") VALUES (100,1);
            INSERT INTO "RemoteStream" ("Id","AddonId","ProviderId","Url","Liveness") VALUES (100,'org.ersatzrs.addon.fixture','item','https://example.test/item',1);
            INSERT INTO "AddonMediaList" ("Id","AddonId","ProviderId","SourceUrl","Name","PublishedRevision")
                VALUES (1,'org.ersatzrs.addon.fixture','list','https://example.test/list','Fixture',1);
            INSERT INTO "AddonMediaListLibrary" ("AddonMediaListId","LibraryId","FolderName","ManifestPath")
                VALUES (1,1,'fixture','streams/list.yml');
            INSERT INTO "AddonMediaListItem" ("Id","AddonMediaListId","ProviderId","DisplayTitle","Kind","Rank","Title","Published","MediaItemId")
                VALUES (10,1,'item','Fixture',4,0,'Fixture',1,100);
            INSERT INTO "AddonMediaListRevision" ("AddonMediaListId","Revision","OptionValuesJson") VALUES (1,1,'{}');
        "#).execute(&pool).await.unwrap();
        assert_eq!(
            resolve_published_remote_stream(&pool, 100, &runtime, temporary.path())
                .await
                .unwrap(),
            Some(legacy_before)
        );
        sqlx::query(r#"INSERT INTO "AddonMediaListOptionSchema" ("Id","AddonId","CapabilityId","SchemaDigest","DefinitionJson","CreatedAt")
            VALUES (1,'org.ersatzrs.addon.fixture','media-list.options.v1',?,?,'2026-09-05T00:00:00Z')"#)
            .bind(&options.schema.schema_digest).bind(serde_json::to_string(&options.definition).unwrap()).execute(&pool).await.unwrap();
        sqlx::query(r#"UPDATE "AddonMediaListRevision" SET "OptionSchemaId"=1"#)
            .execute(&pool)
            .await
            .unwrap();
        assert!(
            resolve_published_remote_stream(&pool, 100, &runtime, temporary.path())
                .await
                .is_err()
        );
    }
}
