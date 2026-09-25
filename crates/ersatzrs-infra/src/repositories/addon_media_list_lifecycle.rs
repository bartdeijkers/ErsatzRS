use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};

use async_trait::async_trait;
use ersatzrs_addon_contract::{
    AddonCapability, AddonMediaListOptionValue as ContractOptionValue,
    AddonMediaListOptionValueScope, AddonMediaListOptionValues as ContractOptionValues,
};
use ersatzrs_core::addon_media_list_playback::AddonMediaListPlaybackTimelinePlan;
use ersatzrs_core::repositories::{
    AddonMediaListArtifact, AddonMediaListDraftDiff, AddonMediaListDraftOptions,
    AddonMediaListImportCheckpoint, AddonMediaListImportLifecycle, AddonMediaListItemOptionValues,
    AddonMediaListItemTombstone, AddonMediaListLifecycleJob, AddonMediaListLifecycleRepository,
    AddonMediaListLifecycleSettings, AddonMediaListOptionItem, AddonMediaListOptionSchema,
    AddonMediaListOptionSchemaRef, AddonMediaListOptionValues, AddonMediaListPublishedOptions,
    AddonMediaListReferencePins, AddonMediaListResetImpact, AddonMediaListRevisionArtifacts,
    AddonMediaListRevisionDiff, AddonMediaListRevisionFile, AddonMediaListRevisionFileBlob,
    AddonMediaListRevisionItem, AddonMediaListRevisionRestoreFence, AddonMediaListRevisionSnapshot,
    AddonMediaListRevisionSummary, AddonProcessedArtifactPolicy, AddonToolMaintenanceAttempt,
    CreateAddonMediaListLifecycleJob, RepositoryError, Result,
    ADDON_LIST_TOTAL_DISK_WARNING_GB_DEFAULT, ADDON_LIST_TOTAL_DISK_WARNING_GB_KEY,
    PROCESSED_ARTIFACT_GLOBAL_BYTE_LIMIT_KEY, PROCESSED_ARTIFACT_MAX_CONCURRENCY_KEY,
    PROCESSED_ARTIFACT_MINIMUM_FREE_BYTES_KEY, PROCESSED_ARTIFACT_RETENTION_SECONDS_KEY,
};
use ersatzrs_domain::{
    AddonMediaListArtifactKind, AddonMediaListArtifactState, AddonMediaListItemAvailability,
    AddonMediaListRefreshCadence, EntityId, ManagedLibraryOperationState, RemoteStreamLiveness,
};
use sha2::{Digest as _, Sha256};
use sqlx::Row;

use super::common::{
    decode_bool_column, decode_datetime, decode_enum_column, decode_optional_datetime,
    decode_optional_enum_column, encode_datetime, encode_enum, encode_optional_datetime,
};
use super::{SqliteAddonMediaListRepository, SqlxResultExt as _};

const OPTION_SCHEMA_MISMATCH: &str =
    "option schema does not match the saved capability id and digest";
const MAX_OPTION_SCHEMA_DEFINITION_BYTES: usize = 256 * 1024;
const TOOL_MAINTENANCE_HISTORY_LIMIT: i64 = 50;

#[async_trait]
impl AddonMediaListLifecycleRepository for SqliteAddonMediaListRepository {
    async fn disk_warning_gb(&self, list_id: Option<EntityId>) -> Result<u32> {
        let value = if let Some(id) = list_id {
            sqlx::query_scalar::<_, i64>(
                r#"SELECT "DiskUsageWarningGb" FROM "AddonMediaList" WHERE "Id" = ?"#,
            )
            .bind(id)
            .fetch_optional(&self.0)
            .await
            .repository()?
            .ok_or(RepositoryError::NotFound {
                kind: "add-on media list",
                id,
            })?
            .to_string()
        } else {
            sqlx::query_scalar::<_, String>(
                r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#,
            )
            .bind(ADDON_LIST_TOTAL_DISK_WARNING_GB_KEY)
            .fetch_optional(&self.0)
            .await
            .repository()?
            .unwrap_or_else(|| ADDON_LIST_TOTAL_DISK_WARNING_GB_DEFAULT.to_string())
        };
        value.parse().map_err(|_| RepositoryError::Decode {
            kind: "add-on list disk warning GB",
            value,
        })
    }

    async fn save_disk_warning_gb(&self, list_id: Option<EntityId>, warning_gb: u32) -> Result<()> {
        if let Some(id) = list_id {
            let result = sqlx::query(
                r#"UPDATE "AddonMediaList" SET "DiskUsageWarningGb" = ? WHERE "Id" = ?"#,
            )
            .bind(i64::from(warning_gb))
            .bind(id)
            .execute(&self.0)
            .await
            .repository()?;
            if result.rows_affected() == 0 {
                return Err(RepositoryError::NotFound {
                    kind: "add-on media list",
                    id,
                });
            }
        } else {
            sqlx::query(
                r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES (?, ?)
                   ON CONFLICT("Key") DO UPDATE SET "Value" = excluded."Value""#,
            )
            .bind(ADDON_LIST_TOTAL_DISK_WARNING_GB_KEY)
            .bind(warning_gb.to_string())
            .execute(&self.0)
            .await
            .repository()?;
        }
        Ok(())
    }

    async fn lifecycle_settings(
        &self,
        list_id: EntityId,
    ) -> Result<Option<AddonMediaListLifecycleSettings>> {
        let row = sqlx::query(
            r#"SELECT "RefreshCadence", "RefreshPublicationPolicy", "NextRefreshAt",
                      "RefreshRetryAt", "LastRefreshSucceededAt", "ProcessedArtifactByteLimit"
               FROM "AddonMediaList" WHERE "Id" = ?"#,
        )
        .bind(list_id)
        .fetch_optional(&self.0)
        .await
        .repository()?;

        row.map(|row| {
            Ok(AddonMediaListLifecycleSettings {
                cadence: decode_enum_column(
                    "AddonMediaListRefreshCadence",
                    &row,
                    "RefreshCadence",
                )?,
                publication_policy: decode_enum_column(
                    "AddonMediaListRefreshPublicationPolicy",
                    &row,
                    "RefreshPublicationPolicy",
                )?,
                next_refresh_at: decode_optional_datetime(
                    "AddonMediaList.NextRefreshAt",
                    row.try_get("NextRefreshAt").repository()?,
                )?,
                retry_at: decode_optional_datetime(
                    "AddonMediaList.RefreshRetryAt",
                    row.try_get("RefreshRetryAt").repository()?,
                )?,
                last_success_at: decode_optional_datetime(
                    "AddonMediaList.LastRefreshSucceededAt",
                    row.try_get("LastRefreshSucceededAt").repository()?,
                )?,
                processed_artifact_byte_limit: row
                    .try_get::<Option<i64>, _>("ProcessedArtifactByteLimit")
                    .repository()?
                    .map(|value| decode_u64("processed artifact byte limit", value))
                    .transpose()?,
            })
        })
        .transpose()
    }

    async fn save_lifecycle_settings(
        &self,
        list_id: EntityId,
        settings: &AddonMediaListLifecycleSettings,
    ) -> Result<()> {
        let byte_limit = settings
            .processed_artifact_byte_limit
            .map(|value| encode_i64("processed artifact byte limit", value))
            .transpose()?;
        let updated = sqlx::query(
            r#"UPDATE "AddonMediaList"
               SET "RefreshCadence" = ?, "RefreshPublicationPolicy" = ?,
                   "NextRefreshAt" = ?, "RefreshRetryAt" = ?,
                   "LastRefreshSucceededAt" = ?, "ProcessedArtifactByteLimit" = ?,
                   "AutoRefresh" = ?
               WHERE "Id" = ?"#,
        )
        .bind(encode_enum(settings.cadence))
        .bind(encode_enum(ersatzrs_domain::AddonMediaListRefreshPublicationPolicy::Automatic))
        .bind(encode_optional_datetime(settings.next_refresh_at))
        .bind(encode_optional_datetime(settings.retry_at))
        .bind(encode_optional_datetime(settings.last_success_at))
        .bind(byte_limit)
        // Keep the retired Boolean column truthful for older readers without
        // collapsing the other cadences into its former daily-only meaning.
        .bind(settings.cadence == AddonMediaListRefreshCadence::Daily)
        .bind(list_id)
        .execute(&self.0)
        .await
        .repository()?;
        if updated.rows_affected() == 0 {
            return Err(RepositoryError::NotFound {
                kind: "add-on media list",
                id: list_id,
            });
        }
        Ok(())
    }

    async fn processed_artifact_policy(&self) -> Result<AddonProcessedArtifactPolicy> {
        let rows = sqlx::query(
            r#"SELECT "Key", "Value" FROM "ConfigElement"
               WHERE "Key" IN (?, ?, ?, ?)"#,
        )
        .bind(PROCESSED_ARTIFACT_GLOBAL_BYTE_LIMIT_KEY)
        .bind(PROCESSED_ARTIFACT_MINIMUM_FREE_BYTES_KEY)
        .bind(PROCESSED_ARTIFACT_RETENTION_SECONDS_KEY)
        .bind(PROCESSED_ARTIFACT_MAX_CONCURRENCY_KEY)
        .fetch_all(&self.0)
        .await
        .repository()?;
        let mut values = BTreeMap::new();
        for row in rows {
            values.insert(
                row.try_get::<String, _>("Key").repository()?,
                row.try_get::<String, _>("Value").repository()?,
            );
        }
        Ok(AddonProcessedArtifactPolicy {
            global_byte_limit: decode_config_u64(
                &values,
                PROCESSED_ARTIFACT_GLOBAL_BYTE_LIMIT_KEY,
            )?,
            minimum_free_bytes: decode_config_u64(
                &values,
                PROCESSED_ARTIFACT_MINIMUM_FREE_BYTES_KEY,
            )?,
            retention_seconds: decode_config_u64(
                &values,
                PROCESSED_ARTIFACT_RETENTION_SECONDS_KEY,
            )?,
            max_preparation_concurrency: decode_config_u64(
                &values,
                PROCESSED_ARTIFACT_MAX_CONCURRENCY_KEY,
            )?
            .map(|value| {
                u32::try_from(value).map_err(|_| RepositoryError::Decode {
                    kind: "processed artifact maximum concurrency",
                    value: value.to_string(),
                })
            })
            .transpose()?,
        })
    }

    async fn save_processed_artifact_policy(
        &self,
        policy: &AddonProcessedArtifactPolicy,
    ) -> Result<()> {
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let values = [
            (
                PROCESSED_ARTIFACT_GLOBAL_BYTE_LIMIT_KEY,
                policy.global_byte_limit.map(|value| value.to_string()),
            ),
            (
                PROCESSED_ARTIFACT_MINIMUM_FREE_BYTES_KEY,
                policy.minimum_free_bytes.map(|value| value.to_string()),
            ),
            (
                PROCESSED_ARTIFACT_RETENTION_SECONDS_KEY,
                policy.retention_seconds.map(|value| value.to_string()),
            ),
            (
                PROCESSED_ARTIFACT_MAX_CONCURRENCY_KEY,
                policy
                    .max_preparation_concurrency
                    .map(|value| value.to_string()),
            ),
        ];
        for (key, value) in values {
            if let Some(value) = value {
                sqlx::query(
                    r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES (?, ?)
                       ON CONFLICT("Key") DO UPDATE SET "Value" = excluded."Value""#,
                )
                .bind(key)
                .bind(value)
                .execute(&mut *tx)
                .await
                .repository()?;
            } else {
                sqlx::query(r#"DELETE FROM "ConfigElement" WHERE "Key" = ?"#)
                    .bind(key)
                    .execute(&mut *tx)
                    .await
                    .repository()?;
            }
        }
        tx.commit().await.repository()
    }

    async fn register_option_schema(&self, schema: &AddonMediaListOptionSchema) -> Result<()> {
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        register_option_schema_in_tx(&mut tx, schema).await?;
        tx.commit().await.repository()
    }

    async fn option_schema(
        &self,
        schema: &AddonMediaListOptionSchemaRef,
    ) -> Result<Option<AddonMediaListOptionSchema>> {
        let row = sqlx::query(
            r#"SELECT "DefinitionJson", "CreatedAt"
               FROM "AddonMediaListOptionSchema"
               WHERE "AddonId" = ? AND "CapabilityId" = ? AND "SchemaDigest" = ?"#,
        )
        .bind(&schema.addon_id)
        .bind(&schema.capability_id)
        .bind(&schema.schema_digest)
        .fetch_optional(&self.0)
        .await
        .repository()?;
        row.map(|row| {
            let definition_json: String = row.try_get("DefinitionJson").repository()?;
            let stored = AddonMediaListOptionSchema {
                schema: schema.clone(),
                definition_json,
                created_at: decode_required_datetime(
                    "AddonMediaListOptionSchema.CreatedAt",
                    row.try_get::<String, _>("CreatedAt").repository()?,
                )?,
            };
            decode_and_validate_schema(&stored)?;
            Ok(stored)
        })
        .transpose()
    }

    async fn draft_options(
        &self,
        list_id: EntityId,
        expected_schema: Option<&AddonMediaListOptionSchemaRef>,
    ) -> Result<Option<AddonMediaListDraftOptions>> {
        // read-only transaction: list and item option rows form one snapshot.
        let mut tx = self.0.begin().await.repository()?;
        let result = load_draft_options_in_tx(&mut tx, list_id, expected_schema).await?;
        tx.commit().await.repository()?;
        Ok(result)
    }

    async fn option_items(
        &self,
        list_id: EntityId,
        item_ids: &[EntityId],
    ) -> Result<Vec<AddonMediaListOptionItem>> {
        if item_ids.len() > 10_000
            || item_ids.iter().collect::<BTreeSet<_>>().len() != item_ids.len()
        {
            return Err(RepositoryError::Conflict(
                "option item lookup requires at most 10000 distinct item identities".into(),
            ));
        }
        // read-only transaction: every selection chunk sees one item snapshot.
        let mut tx = self.0.begin().await.repository()?;
        let mut items = Vec::with_capacity(item_ids.len());
        for chunk in item_ids.chunks(900) {
            let mut query = sqlx::QueryBuilder::<crate::db::Db>::new(
                r#"SELECT "Id", "ProviderId",
                          COALESCE("LivenessOverride", "ProviderLiveness") AS "Liveness"
                   FROM "AddonMediaListItem" WHERE "AddonMediaListId" = "#,
            );
            query.push_bind(list_id).push(r#" AND "Id" IN ("#);
            let mut separated = query.separated(", ");
            for item_id in chunk {
                separated.push_bind(*item_id);
            }
            separated.push_unseparated(")");
            for row in query.build().fetch_all(&mut *tx).await.repository()? {
                items.push(AddonMediaListOptionItem {
                    item_id: row.try_get("Id").repository()?,
                    provider_id: row.try_get("ProviderId").repository()?,
                    liveness: decode_enum_column("RemoteStreamLiveness", &row, "Liveness")?,
                });
            }
        }
        if items.len() != item_ids.len() {
            return Err(RepositoryError::Conflict(
                "option item identity does not belong to the saved list".into(),
            ));
        }
        tx.commit().await.repository()?;
        Ok(items)
    }

    async fn save_draft_options(&self, options: &AddonMediaListDraftOptions) -> Result<i32> {
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        save_draft_options_tx(&mut tx, options, true).await?;
        tx.commit().await.repository()?;
        Ok(options.draft_revision + 1)
    }

    async fn retain_published_options(
        &self,
        list_id: EntityId,
        revision: i32,
        source_draft_revision: i32,
        completed_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<()> {
        let completed_at = decode_datetime(
            "AddonMediaListRevision.CompletedAt",
            &encode_datetime(completed_at),
        )?;
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let draft = load_draft_options_in_tx(&mut tx, list_id, None)
            .await?
            .ok_or(RepositoryError::NotFound {
                kind: "add-on media list",
                id: list_id,
            })?;
        if draft.draft_revision != source_draft_revision {
            return Err(RepositoryError::Conflict(
                "add-on media-list draft revision changed".into(),
            ));
        }
        let mut item_values = draft
            .item_values
            .iter()
            .map(|item| AddonMediaListItemOptionValues {
                item_id: None,
                provider_id: item.provider_id.clone(),
                values: item.values.clone(),
            })
            .collect::<Vec<_>>();
        item_values.sort_by(|left, right| left.provider_id.cmp(&right.provider_id));
        let desired = AddonMediaListPublishedOptions {
            list_id,
            revision,
            source_draft_revision: Some(source_draft_revision),
            schema: draft.schema.clone(),
            list_values: draft.list_values.clone(),
            item_values,
            completed_at: Some(completed_at),
        };
        if let Some(existing) =
            load_published_options_in_tx(&mut tx, list_id, revision, None).await?
        {
            return if existing == desired {
                tx.commit().await.repository()
            } else {
                Err(RepositoryError::Conflict(
                    "published option revision is immutable".into(),
                ))
            };
        }

        let schema_id = resolve_schema(&mut tx, list_id, desired.schema.as_ref())
            .await?
            .map(|(id, _)| id);
        let inserted = sqlx::query(
            r#"INSERT INTO "AddonMediaListRevision" (
                  "AddonMediaListId", "Revision", "SourceDraftRevision", "OptionSchemaId",
                  "OptionValuesJson", "CompletedAt"
               ) VALUES (?, ?, ?, ?, ?, ?)"#,
        )
        .bind(list_id)
        .bind(revision)
        .bind(source_draft_revision)
        .bind(schema_id)
        .bind(encode_option_values(&desired.list_values)?)
        .bind(encode_datetime(completed_at))
        .execute(&mut *tx)
        .await
        .repository()?;
        let revision_id = inserted.last_insert_rowid();
        for item in &desired.item_values {
            if item.values.is_empty() {
                continue;
            }
            sqlx::query(
                r#"INSERT INTO "AddonMediaListRevisionItemOption" (
                      "AddonMediaListRevisionId", "ProviderId", "OptionOverridesJson"
                   ) VALUES (?, ?, ?)"#,
            )
            .bind(revision_id)
            .bind(&item.provider_id)
            .bind(encode_option_values(&item.values)?)
            .execute(&mut *tx)
            .await
            .repository()?;
        }
        tx.commit().await.repository()
    }

    async fn published_options(
        &self,
        list_id: EntityId,
        revision: i32,
        expected_schema: Option<&AddonMediaListOptionSchemaRef>,
    ) -> Result<Option<AddonMediaListPublishedOptions>> {
        // read-only transaction: revision and item option rows form one snapshot.
        let mut tx = self.0.begin().await.repository()?;
        let result =
            load_published_options_in_tx(&mut tx, list_id, revision, expected_schema).await?;
        tx.commit().await.repository()?;
        Ok(result)
    }

    async fn register_revision_file_blob(
        &self,
        list_id: EntityId,
        storage_relative_path: &str,
        sha256: &str,
        byte_length: u64,
        created_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<AddonMediaListRevisionFileBlob> {
        validate_revision_relative_path(storage_relative_path)?;
        if sha256.trim().is_empty() {
            return Err(RepositoryError::Conflict(
                "a retained revision file requires a content digest".into(),
            ));
        }
        let byte_length = encode_i64("retained revision file byte length", byte_length)?;
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        if let Some(row) = sqlx::query(
            r#"SELECT * FROM "AddonMediaListRevisionFileBlob"
               WHERE "AddonMediaListId" = ? AND "Sha256" = ?"#,
        )
        .bind(list_id)
        .bind(sha256)
        .fetch_optional(&mut *tx)
        .await
        .repository()?
        {
            let existing = decode_revision_file_blob_row(&row)?;
            if existing.storage_relative_path != storage_relative_path
                || existing.byte_length
                    != decode_u64("retained revision file byte length", byte_length)?
            {
                return Err(RepositoryError::Conflict(
                    "retained revision file content identity changed".into(),
                ));
            }
            tx.commit().await.repository()?;
            return Ok(existing);
        }
        let inserted = sqlx::query(
            r#"INSERT INTO "AddonMediaListRevisionFileBlob" (
                   "AddonMediaListId", "StorageRelativePath", "Sha256", "ByteLength", "CreatedAt"
               ) VALUES (?, ?, ?, ?, ?)"#,
        )
        .bind(list_id)
        .bind(storage_relative_path)
        .bind(sha256)
        .bind(byte_length)
        .bind(encode_datetime(created_at))
        .execute(&mut *tx)
        .await
        .repository()?;
        let blob = AddonMediaListRevisionFileBlob {
            id: inserted.last_insert_rowid(),
            list_id,
            storage_relative_path: storage_relative_path.to_owned(),
            sha256: sha256.to_owned(),
            byte_length: decode_u64("retained revision file byte length", byte_length)?,
            created_at,
        };
        tx.commit().await.repository()?;
        Ok(blob)
    }

    async fn retain_published_revision(
        &self,
        list_id: EntityId,
        revision: i32,
        source_draft_revision: i32,
        completed_at: chrono::DateTime<chrono::Utc>,
        files: &[AddonMediaListRevisionFile],
        artifacts: &AddonMediaListRevisionArtifacts,
    ) -> Result<AddonMediaListRevisionSnapshot> {
        let completed_at = decode_datetime(
            "AddonMediaListRevision.CompletedAt",
            &encode_datetime(completed_at),
        )?;
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        if let Some(existing) =
            load_valid_revision_snapshot_in_tx(&mut tx, list_id, revision).await?
        {
            let mut expected_files = files.to_vec();
            expected_files.sort_by(|left, right| {
                left.published_relative_path
                    .cmp(&right.published_relative_path)
            });
            let mut expected_artifacts = artifacts.ids.clone();
            expected_artifacts.sort_unstable();
            expected_artifacts.dedup();
            if existing.source_draft_revision != source_draft_revision
                || encode_datetime(existing.completed_at) != encode_datetime(completed_at)
                || existing.files != expected_files
                || existing.artifact_ids != expected_artifacts
                || existing
                    .items
                    .iter()
                    .filter_map(|item| {
                        item.prepared_artifact_id
                            .map(|id| (item.provider_id.clone(), id))
                    })
                    .collect::<BTreeMap<_, _>>()
                    != artifacts.prepared_media
                || existing
                    .items
                    .iter()
                    .filter_map(|item| {
                        item.playback_timeline
                            .clone()
                            .map(|plan| (item.provider_id.clone(), plan))
                    })
                    .collect::<BTreeMap<_, _>>()
                    != artifacts.streaming_plans
                || existing
                    .items
                    .iter()
                    .filter(|item| !item.subtitle_artifact_ids.is_empty())
                    .map(|item| (item.provider_id.clone(), item.subtitle_artifact_ids.clone()))
                    .collect::<BTreeMap<_, _>>()
                    != artifacts.subtitles
            {
                return Err(RepositoryError::Conflict(
                    "completed publication revision is immutable".into(),
                ));
            }
            tx.commit().await.repository()?;
            return Ok(existing);
        }
        let legacy_exists: bool = sqlx::query_scalar(
            r#"SELECT EXISTS (
                   SELECT 1 FROM "AddonMediaListRevision"
                   WHERE "AddonMediaListId" = ? AND "Revision" = ?
               )"#,
        )
        .bind(list_id)
        .bind(revision)
        .fetch_one(&mut *tx)
        .await
        .repository()?;
        if legacy_exists {
            return Err(RepositoryError::Conflict(
                "a legacy publication revision cannot be made restorable after the fact".into(),
            ));
        }

        let mut snapshot = capture_current_revision_snapshot(
            &mut tx,
            list_id,
            revision,
            source_draft_revision,
            completed_at,
            files,
            &artifacts.ids,
        )
        .await?;
        bind_prepared_revision_items_in_tx(&mut tx, &mut snapshot, &artifacts.prepared_media)
            .await?;
        bind_streaming_revision_items(&mut snapshot, &artifacts.streaming_plans)?;
        bind_subtitle_revision_items_in_tx(&mut tx, &mut snapshot, &artifacts.subtitles).await?;
        snapshot.snapshot_digest = revision_snapshot_digest(&snapshot)?;
        insert_revision_snapshot_in_tx(&mut tx, &snapshot).await?;
        tx.commit().await.repository()?;

        Ok(snapshot)
    }

    async fn revision_history(
        &self,
        list_id: EntityId,
    ) -> Result<Vec<AddonMediaListRevisionSummary>> {
        // read-only transaction: keep summary and child rows on one snapshot.
        // A digest or required-field failure makes only that row non-restorable;
        // legacy/corrupt history must remain visible to the operator.
        let mut tx = self.0.begin().await.repository()?;
        let rows = sqlx::query(
            r#"SELECT "revision"."Revision", "revision"."SourceDraftRevision",
                      "revision"."CompletedAt", "revision"."SnapshotDigest",
                      (SELECT COUNT(*) FROM "AddonMediaListRevisionItem" AS "item"
                       WHERE "item"."AddonMediaListRevisionId" = "revision"."Id"
                         AND "item"."Published" = 1) AS "PublishedItemCount",
                      (SELECT COUNT(*) FROM "AddonMediaListRevisionFile" AS "file"
                       WHERE "file"."AddonMediaListRevisionId" = "revision"."Id") AS "FileCount",
                      (SELECT COUNT(*) FROM "AddonMediaListRevisionArtifact" AS "artifact"
                       WHERE "artifact"."AddonMediaListRevisionId" = "revision"."Id") AS "ArtifactCount"
               FROM "AddonMediaListRevision" AS "revision"
               WHERE "revision"."AddonMediaListId" = ?
               ORDER BY "revision"."Revision" DESC"#,
        )
        .bind(list_id)
        .fetch_all(&mut *tx)
        .await
        .repository()?;
        let mut history = Vec::with_capacity(rows.len());
        for row in &rows {
            let revision: i32 = row.try_get("Revision").repository()?;
            let restorable =
                match load_valid_revision_snapshot_in_tx(&mut tx, list_id, revision).await {
                    Ok(Some(_)) => true,
                    Ok(None) | Err(RepositoryError::Decode { .. }) => false,
                    Err(error) => return Err(error),
                };
            history.push(decode_revision_summary_row(row, restorable)?);
        }
        tx.commit().await.repository()?;
        Ok(history)
    }

    async fn revision_snapshot(
        &self,
        list_id: EntityId,
        revision: i32,
    ) -> Result<Option<AddonMediaListRevisionSnapshot>> {
        // read-only transaction: revision, item, file, and artifact rows form one snapshot.
        let mut tx = self.0.begin().await.repository()?;
        let result = load_valid_revision_snapshot_in_tx(&mut tx, list_id, revision).await?;
        tx.commit().await.repository()?;
        Ok(result)
    }

    async fn materialize_revision_as_draft(
        &self,
        fence: &AddonMediaListRevisionRestoreFence,
    ) -> Result<i32> {
        materialize_revision_as_draft(&self.0, fence).await
    }

    async fn compare_revisions(
        &self,
        list_id: EntityId,
        left_revision: i32,
        right_revision: i32,
    ) -> Result<AddonMediaListRevisionDiff> {
        let left = self
            .revision_snapshot(list_id, left_revision)
            .await?
            .ok_or_else(|| RepositoryError::Conflict("left revision is not restorable".into()))?;
        let right = self
            .revision_snapshot(list_id, right_revision)
            .await?
            .ok_or_else(|| RepositoryError::Conflict("right revision is not restorable".into()))?;
        Ok(compare_revision_snapshots(&left, &right))
    }

    async fn saved_draft_diff(&self, list_id: EntityId) -> Result<AddonMediaListDraftDiff> {
        // read-only transaction: draft items and the retained publication form one comparison.
        let mut tx = self.0.begin().await.repository()?;
        let revisions = sqlx::query(
            r#"SELECT "DraftRevision", "PublishedRevision" FROM "AddonMediaList" WHERE "Id" = ?"#,
        )
        .bind(list_id)
        .fetch_optional(&mut *tx)
        .await
        .repository()?
        .ok_or(RepositoryError::NotFound {
            kind: "add-on media list",
            id: list_id,
        })?;
        let draft_revision: i32 = revisions.try_get("DraftRevision").repository()?;
        let published_revision: i32 = revisions.try_get("PublishedRevision").repository()?;
        let snapshot = if published_revision > 0 {
            load_valid_revision_snapshot_in_tx(&mut tx, list_id, published_revision).await?
        } else {
            None
        };
        let result = calculate_saved_draft_diff_in_tx(
            &mut tx,
            list_id,
            draft_revision,
            published_revision,
            snapshot.as_ref(),
        )
        .await?;
        tx.commit().await.repository()?;
        Ok(result)
    }

    async fn reset_impact(
        &self,
        list_id: EntityId,
        as_of: chrono::DateTime<chrono::Utc>,
    ) -> Result<AddonMediaListResetImpact> {
        calculate_reset_impact(&self.0, list_id, as_of).await
    }

    async fn prune_unpinned_revisions(
        &self,
        list_id: EntityId,
        as_of: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<i32>> {
        prune_unpinned_revision_rows(&self.0, list_id, as_of).await
    }

    async fn prune_all_unpinned_revisions(
        &self,
        as_of: chrono::DateTime<chrono::Utc>,
    ) -> Result<u64> {
        let list_ids = sqlx::query_scalar::<_, EntityId>(
            r#"SELECT DISTINCT "AddonMediaListId" FROM "AddonMediaListRevision"
               WHERE "SnapshotDigest" IS NOT NULL AND "CompletedAt" IS NOT NULL
               ORDER BY "AddonMediaListId""#,
        )
        .fetch_all(&self.0)
        .await
        .repository()?;
        let mut removed = 0_u64;
        for list_id in list_ids {
            removed = removed.saturating_add(
                prune_unpinned_revision_rows(&self.0, list_id, as_of)
                    .await?
                    .len() as u64,
            );
        }
        Ok(removed)
    }

    async fn artifact_reference_pins(
        &self,
        artifact_id: EntityId,
        as_of: chrono::DateTime<chrono::Utc>,
    ) -> Result<AddonMediaListReferencePins> {
        artifact_reference_pins(&self.0, artifact_id, as_of).await
    }

    async fn revision_file_blob_reference_pins(
        &self,
        blob_id: EntityId,
        as_of: chrono::DateTime<chrono::Utc>,
    ) -> Result<AddonMediaListReferencePins> {
        revision_file_blob_reference_pins(&self.0, blob_id, as_of).await
    }

    async fn import_checkpoint(
        &self,
        list_id: EntityId,
    ) -> Result<Option<AddonMediaListImportCheckpoint>> {
        let row = sqlx::query(
            r#"SELECT * FROM "AddonMediaListImportCheckpoint"
               WHERE "AddonMediaListId" = ? AND "Committed" = 1"#,
        )
        .bind(list_id)
        .fetch_optional(&self.0)
        .await
        .repository()?;
        row.as_ref().map(decode_import_checkpoint_row).transpose()
    }

    async fn replace_import_checkpoint(
        &self,
        list_id: EntityId,
        relative_path: &str,
        generation: i32,
        sha256: &str,
        entry_count: u64,
        promoted_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<AddonMediaListImportCheckpoint> {
        let entry_count = encode_i64("add-on media-list checkpoint entry count", entry_count)?;
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let current = sqlx::query(
            r#"SELECT * FROM "AddonMediaListImportCheckpoint"
               WHERE "AddonMediaListId" = ? AND "Committed" = 1"#,
        )
        .bind(list_id)
        .fetch_optional(&mut *tx)
        .await
        .repository()?
        .as_ref()
        .map(decode_import_checkpoint_row)
        .transpose()?;
        if let Some(current) = current.as_ref() {
            let entry_count = u64::try_from(entry_count).unwrap_or_default();
            if generation < current.generation
                || (generation == current.generation && entry_count < current.entry_count)
                || (generation == current.generation
                    && entry_count == current.entry_count
                    && (relative_path != current.relative_path || sha256 != current.sha256))
            {
                return Err(RepositoryError::Conflict(
                    "import checkpoint cannot regress or replace an established page identity"
                        .into(),
                ));
            }
            if generation == current.generation
                && entry_count == current.entry_count
                && relative_path == current.relative_path
                && sha256 == current.sha256
            {
                tx.commit().await.repository()?;
                return Ok(current.clone());
            }
        }
        let updated = if let Some(current) = current {
            sqlx::query(
                r#"UPDATE "AddonMediaListImportCheckpoint"
                   SET "RelativePath" = ?, "Generation" = ?, "Sha256" = ?,
                       "EntryCount" = ?, "PromotedAt" = ?
                   WHERE "Id" = ? AND "Committed" = 1"#,
            )
            .bind(relative_path)
            .bind(generation)
            .bind(sha256)
            .bind(entry_count)
            .bind(encode_datetime(promoted_at))
            .bind(current.id)
            .execute(&mut *tx)
            .await
            .repository()?
        } else {
            sqlx::query(
                r#"INSERT INTO "AddonMediaListImportCheckpoint" (
                       "AddonMediaListId", "RelativePath", "Generation", "Sha256",
                       "EntryCount", "PromotedAt", "Committed"
                   ) VALUES (?, ?, ?, ?, ?, ?, 1)"#,
            )
            .bind(list_id)
            .bind(relative_path)
            .bind(generation)
            .bind(sha256)
            .bind(entry_count)
            .bind(encode_datetime(promoted_at))
            .execute(&mut *tx)
            .await
            .repository()?
        };
        if updated.rows_affected() != 1 {
            return Err(RepositoryError::Conflict(
                "import checkpoint cannot regress or replace an established page identity".into(),
            ));
        }
        let row = sqlx::query(
            r#"SELECT * FROM "AddonMediaListImportCheckpoint"
               WHERE "AddonMediaListId" = ? AND "Committed" = 1"#,
        )
        .bind(list_id)
        .fetch_one(&mut *tx)
        .await
        .repository()?;
        let checkpoint = decode_import_checkpoint_row(&row)?;
        tx.commit().await.repository()?;
        Ok(checkpoint)
    }

    async fn import_lifecycle(
        &self,
        job_id: EntityId,
    ) -> Result<Option<AddonMediaListImportLifecycle>> {
        let row = sqlx::query(
            r#"SELECT "Id", "Purpose", "Mode", "IncrementalStrategy", "ExaminedCount",
                      "ArchivedSkippedCount", "EmittedCount", "CheckpointId"
               FROM "AddonMediaListImportJob" WHERE "Id" = ?"#,
        )
        .bind(job_id)
        .fetch_optional(&self.0)
        .await
        .repository()?;
        row.as_ref().map(decode_import_lifecycle_row).transpose()
    }

    async fn save_import_lifecycle(
        &self,
        lifecycle: &AddonMediaListImportLifecycle,
        expected_lease_owner: &str,
        active_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<()> {
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let list_id = sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "AddonMediaListId" FROM "AddonMediaListImportJob" WHERE "Id" = ?"#,
        )
        .bind(lifecycle.job_id)
        .fetch_optional(&mut *tx)
        .await
        .repository()?
        .ok_or(RepositoryError::NotFound {
            kind: "add-on media-list import job",
            id: lifecycle.job_id,
        })?;
        if let Some(checkpoint_id) = lifecycle.checkpoint_id {
            let checkpoint_list_id = sqlx::query_scalar::<_, EntityId>(
                r#"SELECT "AddonMediaListId" FROM "AddonMediaListImportCheckpoint"
                   WHERE "Id" = ?"#,
            )
            .bind(checkpoint_id)
            .fetch_optional(&mut *tx)
            .await
            .repository()?
            .ok_or(RepositoryError::NotFound {
                kind: "add-on media-list import checkpoint",
                id: checkpoint_id,
            })?;
            if checkpoint_list_id != list_id {
                return Err(RepositoryError::Conflict(
                    "an import job checkpoint must belong to the same media list".into(),
                ));
            }
        }
        let updated = sqlx::query(
            r#"UPDATE "AddonMediaListImportJob"
               SET "Purpose" = ?, "Mode" = ?, "IncrementalStrategy" = ?,
                   "ExaminedCount" = ?, "ArchivedSkippedCount" = ?, "EmittedCount" = ?,
                   "CheckpointId" = ?
               WHERE "Id" = ? AND "State" IN (?, ?) AND "LeaseOwner" = ?
                 AND "LeaseExpiresAt" IS NOT NULL AND "LeaseExpiresAt" > ?"#,
        )
        .bind(encode_enum(lifecycle.purpose))
        .bind(encode_enum(lifecycle.mode))
        .bind(encode_enum(lifecycle.strategy))
        .bind(encode_i64(
            "import examined count",
            lifecycle.examined_count,
        )?)
        .bind(encode_i64(
            "import archived-skipped count",
            lifecycle.archived_skipped_count,
        )?)
        .bind(encode_i64("import emitted count", lifecycle.emitted_count)?)
        .bind(lifecycle.checkpoint_id)
        .bind(lifecycle.job_id)
        .bind(encode_enum(
            ersatzrs_domain::AddonMediaListImportState::Discovering,
        ))
        .bind(encode_enum(
            ersatzrs_domain::AddonMediaListImportState::Verifying,
        ))
        .bind(expected_lease_owner)
        .bind(encode_datetime(active_at))
        .execute(&mut *tx)
        .await
        .repository()?;
        if updated.rows_affected() != 1 {
            return Err(RepositoryError::Conflict(
                "the add-on media-list import lease is stale or inactive".into(),
            ));
        }
        tx.commit().await.repository()
    }

    async fn lifecycle_job(
        &self,
        operation_id: EntityId,
    ) -> Result<Option<AddonMediaListLifecycleJob>> {
        let row =
            sqlx::query(r#"SELECT * FROM "AddonMediaListLifecycleJob" WHERE "OperationId" = ?"#)
                .bind(operation_id)
                .fetch_optional(&self.0)
                .await
                .repository()?;
        row.as_ref().map(decode_lifecycle_job_row).transpose()
    }

    async fn create_lifecycle_job(
        &self,
        command: &CreateAddonMediaListLifecycleJob,
    ) -> Result<AddonMediaListLifecycleJob> {
        let CreateAddonMediaListLifecycleJob {
            operation_id,
            list_id,
            generation,
            kind,
            saved_draft_revision,
            saved_published_revision,
            created_at,
        } = *command;
        let created_at_text = encode_datetime(created_at);
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        validate_lifecycle_job_admission(&mut tx, command).await?;
        let result = sqlx::query(
            r#"INSERT INTO "AddonMediaListLifecycleJob" (
                   "OperationId", "AddonMediaListId", "Generation", "Kind", "Phase",
                   "SavedDraftRevision", "SavedPublishedRevision", "CreatedAt", "UpdatedAt"
               ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(operation_id)
        .bind(list_id)
        .bind(generation)
        .bind(encode_enum(kind))
        .bind(encode_enum(
            ersatzrs_domain::AddonMediaListLifecyclePhase::AwaitingLease,
        ))
        .bind(saved_draft_revision)
        .bind(saved_published_revision)
        .bind(&created_at_text)
        .bind(&created_at_text)
        .execute(&mut *tx)
        .await
        .repository()?;
        let job = AddonMediaListLifecycleJob {
            id: result.last_insert_rowid(),
            operation_id,
            list_id,
            generation,
            kind,
            phase: ersatzrs_domain::AddonMediaListLifecyclePhase::AwaitingLease,
            saved_draft_revision,
            saved_published_revision,
            candidate_import_job_id: None,
            created_at,
            updated_at: created_at,
            completed_at: None,
        };
        tx.commit().await.repository()?;
        Ok(job)
    }

    async fn save_lifecycle_job(
        &self,
        job: &AddonMediaListLifecycleJob,
        expected_updated_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<()> {
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let stored = sqlx::query(
            r#"SELECT "OperationId", "AddonMediaListId"
               FROM "AddonMediaListLifecycleJob" WHERE "Id" = ?"#,
        )
        .bind(job.id)
        .fetch_optional(&mut *tx)
        .await
        .repository()?
        .ok_or(RepositoryError::NotFound {
            kind: "add-on media-list lifecycle job",
            id: job.id,
        })?;
        let stored_operation_id: EntityId = stored.try_get("OperationId").repository()?;
        let stored_list_id: EntityId = stored.try_get("AddonMediaListId").repository()?;
        if stored_operation_id != job.operation_id || stored_list_id != job.list_id {
            return Err(RepositoryError::Conflict(
                "lifecycle job ownership cannot be changed".into(),
            ));
        }
        if let Some(candidate_import_job_id) = job.candidate_import_job_id {
            let candidate_list_id = sqlx::query_scalar::<_, EntityId>(
                r#"SELECT "AddonMediaListId" FROM "AddonMediaListImportJob" WHERE "Id" = ?"#,
            )
            .bind(candidate_import_job_id)
            .fetch_optional(&mut *tx)
            .await
            .repository()?
            .ok_or(RepositoryError::NotFound {
                kind: "add-on media-list import job",
                id: candidate_import_job_id,
            })?;
            if candidate_list_id != job.list_id {
                return Err(RepositoryError::Conflict(
                    "a lifecycle candidate import must belong to the same media list".into(),
                ));
            }
        }
        let updated = sqlx::query(
            r#"UPDATE "AddonMediaListLifecycleJob"
               SET "Phase" = ?, "CandidateImportJobId" = ?, "UpdatedAt" = ?, "CompletedAt" = ?
               WHERE "Id" = ? AND "OperationId" = ? AND "UpdatedAt" = ?"#,
        )
        .bind(encode_enum(job.phase))
        .bind(job.candidate_import_job_id)
        .bind(encode_datetime(job.updated_at))
        .bind(encode_optional_datetime(job.completed_at))
        .bind(job.id)
        .bind(job.operation_id)
        .bind(encode_datetime(expected_updated_at))
        .execute(&mut *tx)
        .await
        .repository()?;
        if updated.rows_affected() != 1 {
            return Err(RepositoryError::Conflict(
                "the add-on media-list lifecycle job was updated by another worker".into(),
            ));
        }
        tx.commit().await.repository()
    }

    async fn item_tombstone(
        &self,
        item_id: EntityId,
    ) -> Result<Option<AddonMediaListItemTombstone>> {
        let row = sqlx::query(
            r#"SELECT * FROM "AddonMediaListItemTombstone" WHERE "AddonMediaListItemId" = ?"#,
        )
        .bind(item_id)
        .fetch_optional(&self.0)
        .await
        .repository()?;
        row.as_ref().map(decode_tombstone_row).transpose()
    }

    async fn save_item_tombstone(
        &self,
        tombstone: &AddonMediaListItemTombstone,
        expected_generation: Option<i32>,
    ) -> Result<()> {
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let current = load_item_tombstone_in_tx(&mut tx, tombstone.item_id).await?;
        match (current, expected_generation) {
            (None, None) => {
                if tombstone.confirmed_at.is_some() || tombstone.reappeared_at.is_some() {
                    return Err(RepositoryError::Conflict(
                        "initial tombstone observations cannot be pre-confirmed or reappeared"
                            .into(),
                    ));
                }
                insert_item_tombstone_in_tx(&mut tx, tombstone).await?;
            }
            (None, Some(_)) => {
                return Err(RepositoryError::NotFound {
                    kind: "add-on media-list item tombstone",
                    id: tombstone.item_id,
                });
            }
            (Some(current), Some(expected)) if current.last_reconcile_generation == expected => {
                if tombstone.last_reconcile_generation == current.last_reconcile_generation {
                    if tombstones_are_identical(&current, tombstone) {
                        tx.commit().await.repository()?;
                        return Ok(());
                    }
                    return Err(RepositoryError::Conflict(
                        "same-generation tombstone observations must be identical".into(),
                    ));
                }
                if tombstone.last_reconcile_generation < current.last_reconcile_generation {
                    return Err(RepositoryError::Conflict(
                        "the add-on media-list item tombstone generation is stale".into(),
                    ));
                }
                if tombstone.confirmed_at != current.confirmed_at
                    || tombstone.reappeared_at != current.reappeared_at
                {
                    return Err(RepositoryError::Conflict(
                        "tombstone observation updates cannot change confirmation or reappearance"
                            .into(),
                    ));
                }
                if tombstone.observation_count <= current.observation_count {
                    return Err(RepositoryError::Conflict(
                        "a newer tombstone generation must advance the observation count".into(),
                    ));
                }
                update_item_tombstone_in_tx(&mut tx, tombstone, expected).await?;
            }
            (Some(_), None | Some(_)) => {
                return Err(RepositoryError::Conflict(
                    "the add-on media-list item tombstone generation is stale".into(),
                ));
            }
        }
        tx.commit().await.repository()
    }

    async fn confirm_item_tombstone(
        &self,
        item_id: EntityId,
        expected_generation: i32,
        confirmed_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<()> {
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let updated = sqlx::query(
            r#"UPDATE "AddonMediaListItemTombstone" SET "ConfirmedAt" = ?
               WHERE "AddonMediaListItemId" = ? AND "LastReconcileGeneration" = ?
                 AND "ConfirmedAt" IS NULL AND "ReappearedAt" IS NULL"#,
        )
        .bind(encode_datetime(confirmed_at))
        .bind(item_id)
        .bind(expected_generation)
        .execute(&mut *tx)
        .await
        .repository()?;
        if updated.rows_affected() == 1 {
            let marked = sqlx::query(
                r#"UPDATE "AddonMediaList"
                   SET "ReviewState" = CASE WHEN "ReviewState" = 0 THEN 0 ELSE 1 END,
                       "ReviewError" = NULL, "DraftRevision" = "DraftRevision" + 1
                   WHERE "Id" = (
                       SELECT "AddonMediaListId" FROM "AddonMediaListItem" WHERE "Id" = ?
                   )
                     AND "ReviewState" <> 3
                     AND ("PublicationJournalJson" IS NULL
                          OR TRIM("PublicationJournalJson") = '')
                     AND NOT EXISTS (
                         SELECT 1
                         FROM "AddonMediaListPublicationJob" AS "job"
                         JOIN "ManagedLibraryOperation" AS "operation"
                           ON "operation"."Id" = "job"."OperationId"
                         WHERE "job"."AddonMediaListId" = "AddonMediaList"."Id"
                           AND "operation"."State" IN (?, ?, ?)
                     )"#,
            )
            .bind(item_id)
            .bind(encode_enum(ManagedLibraryOperationState::Queued))
            .bind(encode_enum(ManagedLibraryOperationState::Running))
            .bind(encode_enum(ManagedLibraryOperationState::WaitingToRetry))
            .execute(&mut *tx)
            .await
            .repository()?;
            if marked.rows_affected() != 1 {
                return Err(RepositoryError::Conflict(
                    "the add-on media list changed while removal was confirmed".to_owned(),
                ));
            }
            return tx.commit().await.repository();
        }
        let exists: bool = sqlx::query_scalar(
            r#"SELECT EXISTS (
                   SELECT 1 FROM "AddonMediaListItemTombstone"
                   WHERE "AddonMediaListItemId" = ?
               )"#,
        )
        .bind(item_id)
        .fetch_one(&mut *tx)
        .await
        .repository()?;
        if exists {
            Err(RepositoryError::Conflict(
                "the add-on media-list item tombstone confirmation is stale or already saved"
                    .into(),
            ))
        } else {
            Err(RepositoryError::NotFound {
                kind: "add-on media-list item tombstone",
                id: item_id,
            })
        }
    }

    async fn clear_item_tombstone(
        &self,
        item_id: EntityId,
        expected_generation: i32,
    ) -> Result<()> {
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let deleted = sqlx::query(
            r#"DELETE FROM "AddonMediaListItemTombstone"
               WHERE "AddonMediaListItemId" = ? AND "LastReconcileGeneration" = ?"#,
        )
        .bind(item_id)
        .bind(expected_generation)
        .execute(&mut *tx)
        .await
        .repository()?;
        if deleted.rows_affected() == 1 {
            return tx.commit().await.repository();
        }
        let exists: bool = sqlx::query_scalar(
            r#"SELECT EXISTS (
                   SELECT 1 FROM "AddonMediaListItemTombstone"
                   WHERE "AddonMediaListItemId" = ?
               )"#,
        )
        .bind(item_id)
        .fetch_one(&mut *tx)
        .await
        .repository()?;
        if exists {
            Err(RepositoryError::Conflict(
                "the add-on media-list item tombstone generation is stale".into(),
            ))
        } else {
            Err(RepositoryError::NotFound {
                kind: "add-on media-list item tombstone",
                id: item_id,
            })
        }
    }

    async fn artifact(&self, artifact_id: EntityId) -> Result<Option<AddonMediaListArtifact>> {
        let row = sqlx::query(r#"SELECT * FROM "AddonMediaListArtifact" WHERE "Id" = ?"#)
            .bind(artifact_id)
            .fetch_optional(&self.0)
            .await
            .repository()?;
        row.as_ref().map(decode_artifact_row).transpose()
    }

    async fn create_artifact(
        &self,
        list_id: EntityId,
        item_id: Option<EntityId>,
        cache_key: &str,
        kind: ersatzrs_domain::AddonMediaListArtifactKind,
        reserved_bytes: u64,
        created_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<AddonMediaListArtifact> {
        let reserved_bytes = encode_i64("artifact reserved bytes", reserved_bytes)?;
        let created_at_text = encode_datetime(created_at);
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let list_exists: bool =
            sqlx::query_scalar(r#"SELECT EXISTS (SELECT 1 FROM "AddonMediaList" WHERE "Id" = ?)"#)
                .bind(list_id)
                .fetch_one(&mut *tx)
                .await
                .repository()?;
        if !list_exists {
            return Err(RepositoryError::NotFound {
                kind: "add-on media list",
                id: list_id,
            });
        }
        if let Some(item_id) = item_id {
            let item_list_id = sqlx::query_scalar::<_, EntityId>(
                r#"SELECT "AddonMediaListId" FROM "AddonMediaListItem" WHERE "Id" = ?"#,
            )
            .bind(item_id)
            .fetch_optional(&mut *tx)
            .await
            .repository()?
            .ok_or(RepositoryError::NotFound {
                kind: "add-on media-list item",
                id: item_id,
            })?;
            if item_list_id != list_id {
                return Err(RepositoryError::Conflict(
                    "an artifact item must belong to the same media list".into(),
                ));
            }
        }
        let result = sqlx::query(
            r#"INSERT INTO "AddonMediaListArtifact" (
                   "AddonMediaListId", "AddonMediaListItemId", "CacheKey", "Kind", "State",
                   "ReservedBytes", "CreatedAt", "UpdatedAt"
               ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(list_id)
        .bind(item_id)
        .bind(cache_key)
        .bind(encode_enum(kind))
        .bind(encode_enum(
            ersatzrs_domain::AddonMediaListArtifactState::Reserved,
        ))
        .bind(reserved_bytes)
        .bind(&created_at_text)
        .bind(&created_at_text)
        .execute(&mut *tx)
        .await
        .repository()?;
        let artifact = AddonMediaListArtifact {
            id: result.last_insert_rowid(),
            list_id,
            item_id,
            cache_key: cache_key.to_owned(),
            kind,
            state: ersatzrs_domain::AddonMediaListArtifactState::Reserved,
            relative_path: None,
            reserved_bytes: u64::try_from(reserved_bytes).unwrap_or_default(),
            byte_length: None,
            sha256: None,
            diagnostic_code: None,
            created_at,
            updated_at: created_at,
            last_used_at: None,
            completed_at: None,
        };
        tx.commit().await.repository()?;
        Ok(artifact)
    }

    async fn save_artifact(
        &self,
        artifact: &AddonMediaListArtifact,
        expected_updated_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<()> {
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let updated = sqlx::query(
            r#"UPDATE "AddonMediaListArtifact"
               SET "State" = ?, "RelativePath" = ?, "ReservedBytes" = ?, "ByteLength" = ?,
                   "Sha256" = ?, "DiagnosticCode" = ?, "UpdatedAt" = ?, "LastUsedAt" = ?,
                   "CompletedAt" = ?
               WHERE "Id" = ? AND "AddonMediaListId" = ? AND "UpdatedAt" = ?"#,
        )
        .bind(encode_enum(artifact.state))
        .bind(&artifact.relative_path)
        .bind(encode_i64(
            "artifact reserved bytes",
            artifact.reserved_bytes,
        )?)
        .bind(
            artifact
                .byte_length
                .map(|value| encode_i64("artifact byte length", value))
                .transpose()?,
        )
        .bind(&artifact.sha256)
        .bind(&artifact.diagnostic_code)
        .bind(encode_datetime(artifact.updated_at))
        .bind(encode_optional_datetime(artifact.last_used_at))
        .bind(encode_optional_datetime(artifact.completed_at))
        .bind(artifact.id)
        .bind(artifact.list_id)
        .bind(encode_datetime(expected_updated_at))
        .execute(&mut *tx)
        .await
        .repository()?;
        if updated.rows_affected() != 1 {
            let exists: bool = sqlx::query_scalar(
                r#"SELECT EXISTS (SELECT 1 FROM "AddonMediaListArtifact" WHERE "Id" = ?)"#,
            )
            .bind(artifact.id)
            .fetch_one(&mut *tx)
            .await
            .repository()?;
            return if exists {
                Err(RepositoryError::Conflict(
                    "the add-on media-list artifact was updated by another worker".into(),
                ))
            } else {
                Err(RepositoryError::NotFound {
                    kind: "add-on media-list artifact",
                    id: artifact.id,
                })
            };
        }
        tx.commit().await.repository()
    }

    async fn revision_artifact_ids(
        &self,
        list_id: EntityId,
        revision: i32,
    ) -> Result<Vec<EntityId>> {
        sqlx::query_scalar(
            r#"SELECT "reference"."AddonMediaListArtifactId"
               FROM "AddonMediaListRevisionArtifact" AS "reference"
               JOIN "AddonMediaListRevision" AS "revision"
                 ON "revision"."Id" = "reference"."AddonMediaListRevisionId"
               WHERE "revision"."AddonMediaListId" = ? AND "revision"."Revision" = ?
               ORDER BY "reference"."AddonMediaListArtifactId""#,
        )
        .bind(list_id)
        .bind(revision)
        .fetch_all(&self.0)
        .await
        .repository()
    }

    async fn link_revision_artifact(
        &self,
        list_id: EntityId,
        revision: i32,
        artifact_id: EntityId,
    ) -> Result<()> {
        let inserted = sqlx::query(
            r#"INSERT INTO "AddonMediaListRevisionArtifact" (
                   "AddonMediaListRevisionId", "AddonMediaListArtifactId"
               )
               SELECT "revision"."Id", "artifact"."Id"
               FROM "AddonMediaListRevision" AS "revision"
               JOIN "AddonMediaListArtifact" AS "artifact"
                 ON "artifact"."Id" = ? AND "artifact"."AddonMediaListId" = "revision"."AddonMediaListId"
               WHERE "revision"."AddonMediaListId" = ? AND "revision"."Revision" = ?
               ON CONFLICT DO NOTHING"#,
        )
        .bind(artifact_id)
        .bind(list_id)
        .bind(revision)
        .execute(&self.0)
        .await
        .repository()?;
        if inserted.rows_affected() == 0
            && !self
                .revision_artifact_ids(list_id, revision)
                .await?
                .contains(&artifact_id)
        {
            return Err(RepositoryError::Conflict(
                "revision and artifact must exist and belong to the same add-on media list".into(),
            ));
        }
        Ok(())
    }

    async fn unlink_revision_artifact(
        &self,
        list_id: EntityId,
        revision: i32,
        artifact_id: EntityId,
    ) -> Result<()> {
        sqlx::query(
            r#"DELETE FROM "AddonMediaListRevisionArtifact"
               WHERE "AddonMediaListArtifactId" = ?
                 AND "AddonMediaListRevisionId" IN (
                     SELECT "Id" FROM "AddonMediaListRevision"
                     WHERE "AddonMediaListId" = ? AND "Revision" = ?
                 )"#,
        )
        .bind(artifact_id)
        .bind(list_id)
        .bind(revision)
        .execute(&self.0)
        .await
        .repository()?;
        Ok(())
    }

    async fn tool_maintenance_history(
        &self,
        addon_id: &str,
        tool_key: &str,
    ) -> Result<Vec<AddonToolMaintenanceAttempt>> {
        let rows = sqlx::query(
            r#"SELECT * FROM "AddonToolMaintenanceJob"
               WHERE "AddonId" = ? AND "ToolKey" = ?
               ORDER BY "CreatedAt" DESC, "Id" DESC LIMIT ?"#,
        )
        .bind(addon_id)
        .bind(tool_key)
        .bind(TOOL_MAINTENANCE_HISTORY_LIMIT)
        .fetch_all(&self.0)
        .await
        .repository()?;
        rows.iter().map(decode_tool_attempt_row).collect()
    }

    async fn create_tool_maintenance_attempt(
        &self,
        addon_id: &str,
        tool_key: &str,
        trigger: ersatzrs_domain::AddonToolMaintenanceTrigger,
        created_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<AddonToolMaintenanceAttempt> {
        let created_at_text = encode_datetime(created_at);
        let result = sqlx::query(
            r#"INSERT INTO "AddonToolMaintenanceJob" (
                   "AddonId", "ToolKey", "Trigger", "State", "CreatedAt", "UpdatedAt"
               ) VALUES (?, ?, ?, ?, ?, ?)"#,
        )
        .bind(addon_id)
        .bind(tool_key)
        .bind(encode_enum(trigger))
        .bind(encode_enum(
            ersatzrs_domain::AddonToolMaintenanceState::Queued,
        ))
        .bind(&created_at_text)
        .bind(&created_at_text)
        .execute(&self.0)
        .await
        .repository()?;
        Ok(AddonToolMaintenanceAttempt {
            id: result.last_insert_rowid(),
            addon_id: addon_id.to_owned(),
            tool_key: tool_key.to_owned(),
            trigger,
            state: ersatzrs_domain::AddonToolMaintenanceState::Queued,
            attempt_count: 0,
            next_eligible_at: None,
            before_version: None,
            before_sha256: None,
            after_version: None,
            after_sha256: None,
            validation: ersatzrs_domain::AddonToolMaintenanceValidation::NotRun,
            rollback: ersatzrs_domain::AddonToolMaintenanceRollback::NotRequired,
            outcome: None,
            diagnostic_code: None,
            diagnostic: None,
            created_at,
            updated_at: created_at,
            completed_at: None,
        })
    }

    async fn save_tool_maintenance_attempt(
        &self,
        attempt: &AddonToolMaintenanceAttempt,
        expected_updated_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<()> {
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let updated = sqlx::query(
            r#"UPDATE "AddonToolMaintenanceJob"
               SET "State" = ?, "AttemptCount" = ?, "NextEligibleAt" = ?,
                   "BeforeVersion" = ?, "BeforeSha256" = ?, "AfterVersion" = ?,
                   "AfterSha256" = ?, "Validation" = ?, "Rollback" = ?, "Outcome" = ?,
                   "DiagnosticCode" = ?, "Diagnostic" = ?, "UpdatedAt" = ?, "CompletedAt" = ?
               WHERE "Id" = ? AND "AddonId" = ? AND "ToolKey" = ? AND "UpdatedAt" = ?"#,
        )
        .bind(encode_enum(attempt.state))
        .bind(attempt.attempt_count)
        .bind(encode_optional_datetime(attempt.next_eligible_at))
        .bind(&attempt.before_version)
        .bind(&attempt.before_sha256)
        .bind(&attempt.after_version)
        .bind(&attempt.after_sha256)
        .bind(encode_enum(attempt.validation))
        .bind(encode_enum(attempt.rollback))
        .bind(attempt.outcome.map(encode_enum))
        .bind(&attempt.diagnostic_code)
        .bind(&attempt.diagnostic)
        .bind(encode_datetime(attempt.updated_at))
        .bind(encode_optional_datetime(attempt.completed_at))
        .bind(attempt.id)
        .bind(&attempt.addon_id)
        .bind(&attempt.tool_key)
        .bind(encode_datetime(expected_updated_at))
        .execute(&mut *tx)
        .await
        .repository()?;
        if updated.rows_affected() != 1 {
            let exists: bool = sqlx::query_scalar(
                r#"SELECT EXISTS (SELECT 1 FROM "AddonToolMaintenanceJob" WHERE "Id" = ?)"#,
            )
            .bind(attempt.id)
            .fetch_one(&mut *tx)
            .await
            .repository()?;
            return if exists {
                Err(RepositoryError::Conflict(
                    "the add-on tool maintenance attempt was updated by another worker".into(),
                ))
            } else {
                Err(RepositoryError::NotFound {
                    kind: "add-on tool maintenance attempt",
                    id: attempt.id,
                })
            };
        }
        tx.commit().await.repository()
    }
}

const REVISION_SNAPSHOT_VERSION: i32 = 1;
const RETAINED_COMPLETE_REVISION_COUNT: usize = 3;

#[allow(clippy::too_many_lines)] // Capture the complete revision under one transaction boundary.
pub(crate) async fn capture_current_revision_snapshot(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    list_id: EntityId,
    revision: i32,
    source_draft_revision: i32,
    completed_at: chrono::DateTime<chrono::Utc>,
    files: &[AddonMediaListRevisionFile],
    artifact_ids: &[EntityId],
) -> Result<AddonMediaListRevisionSnapshot> {
    let row = sqlx::query(
        r#"SELECT "Name", "Description", "GeneratePlaylist", "ContentKindOverride",
                  "PresentationKind", "RequestedPresentationKind", "ProviderMetadataJson",
                  "MetadataOverrideJson", "DraftRevision", "PublishedRevision"
           FROM "AddonMediaList" WHERE "Id" = ?"#,
    )
    .bind(list_id)
    .fetch_optional(&mut **tx)
    .await
    .repository()?
    .ok_or(RepositoryError::NotFound {
        kind: "add-on media list",
        id: list_id,
    })?;
    let current_draft_revision: i32 = row.try_get("DraftRevision").repository()?;
    let current_published_revision: i32 = row.try_get("PublishedRevision").repository()?;
    if current_draft_revision != source_draft_revision || current_published_revision != revision {
        return Err(RepositoryError::Conflict(
            "the published add-on media-list revision changed before retention".into(),
        ));
    }
    let metadata_json = merge_canonical_metadata(
        row.try_get::<String, _>("ProviderMetadataJson")
            .repository()?
            .as_str(),
        row.try_get::<String, _>("MetadataOverrideJson")
            .repository()?
            .as_str(),
    )?;
    let draft =
        load_draft_options_in_tx(tx, list_id, None)
            .await?
            .ok_or(RepositoryError::NotFound {
                kind: "add-on media list",
                id: list_id,
            })?;
    if draft.draft_revision != source_draft_revision {
        return Err(RepositoryError::Conflict(
            "the add-on media-list draft changed while retaining its publication".into(),
        ));
    }
    let mut option_items = draft
        .item_values
        .into_iter()
        .map(|item| AddonMediaListItemOptionValues {
            item_id: None,
            provider_id: item.provider_id,
            values: item.values,
        })
        .collect::<Vec<_>>();
    option_items.sort_by(|left, right| left.provider_id.cmp(&right.provider_id));
    let options = AddonMediaListPublishedOptions {
        list_id,
        revision,
        source_draft_revision: Some(source_draft_revision),
        schema: draft.schema,
        list_values: draft.list_values,
        item_values: option_items,
        completed_at: Some(completed_at),
    };

    let mut items = load_current_revision_items_in_tx(tx, list_id).await?;
    items.sort_by(|left, right| {
        left.rank
            .cmp(&right.rank)
            .then_with(|| left.provider_id.cmp(&right.provider_id))
    });

    let mut retained_files = files.to_vec();
    retained_files.sort_by(|left, right| {
        left.published_relative_path
            .cmp(&right.published_relative_path)
    });
    let mut seen_paths = BTreeSet::new();
    for file in &retained_files {
        validate_revision_relative_path(&file.published_relative_path)?;
        validate_revision_relative_path(&file.storage_relative_path)?;
        if !seen_paths.insert(file.published_relative_path.as_str()) {
            return Err(RepositoryError::Conflict(
                "a retained revision contains the same published path twice".into(),
            ));
        }
        let stored = sqlx::query(
            r#"SELECT * FROM "AddonMediaListRevisionFileBlob"
               WHERE "Id" = ? AND "AddonMediaListId" = ?"#,
        )
        .bind(file.blob_id)
        .bind(list_id)
        .fetch_optional(&mut **tx)
        .await
        .repository()?
        .as_ref()
        .map(decode_revision_file_blob_row)
        .transpose()?
        .ok_or_else(|| {
            RepositoryError::Conflict(
                "a retained revision file blob does not belong to the media list".into(),
            )
        })?;
        if stored.storage_relative_path != file.storage_relative_path
            || stored.sha256 != file.sha256
            || stored.byte_length != file.byte_length
        {
            return Err(RepositoryError::Conflict(
                "a retained revision file does not match its immutable blob".into(),
            ));
        }
    }

    let mut artifacts = artifact_ids.to_vec();
    artifacts.sort_unstable();
    artifacts.dedup();
    for artifact_id in &artifacts {
        let belongs: bool = sqlx::query_scalar(
            r#"SELECT EXISTS (
                   SELECT 1 FROM "AddonMediaListArtifact"
                   WHERE "Id" = ? AND "AddonMediaListId" = ?
               )"#,
        )
        .bind(artifact_id)
        .bind(list_id)
        .fetch_one(&mut **tx)
        .await
        .repository()?;
        if !belongs {
            return Err(RepositoryError::Conflict(
                "a retained revision artifact does not belong to the media list".into(),
            ));
        }
    }

    Ok(AddonMediaListRevisionSnapshot {
        list_id,
        revision,
        source_draft_revision,
        completed_at,
        name: row.try_get("Name").repository()?,
        description: row.try_get("Description").repository()?,
        generate_playlist: decode_bool_column(&row, "GeneratePlaylist")?,
        content_kind_override: decode_enum_column(
            "AddonMediaListContentKind",
            &row,
            "ContentKindOverride",
        )?,
        presentation_kind: decode_enum_column(
            "AddonMediaListPresentationKind",
            &row,
            "PresentationKind",
        )?,
        requested_presentation_kind: decode_enum_column(
            "AddonMediaListPresentationKind",
            &row,
            "RequestedPresentationKind",
        )?,
        metadata_json,
        options,
        items,
        files: retained_files,
        artifact_ids: artifacts,
        snapshot_digest: String::new(),
    })
}

fn bind_streaming_revision_items(
    snapshot: &mut AddonMediaListRevisionSnapshot,
    plans: &BTreeMap<String, AddonMediaListPlaybackTimelinePlan>,
) -> Result<()> {
    for (provider_id, plan) in plans {
        let item = snapshot.items.iter_mut().find(|item| item.provider_id == *provider_id)
            .filter(|item| item.published
                && item.replacement_media_item_id.is_none()
                && item.prepared_artifact_id.is_none()
                && item.effective_liveness == RemoteStreamLiveness::Finite)
            .ok_or_else(|| RepositoryError::Conflict(
                "streaming plan must belong to a published finite item without a replacement or prepared file".into()
            ))?;
        item.playback_timeline = Some(plan.clone());
    }
    Ok(())
}

async fn bind_subtitle_revision_items_in_tx(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    snapshot: &mut AddonMediaListRevisionSnapshot,
    subtitles: &BTreeMap<String, Vec<EntityId>>,
) -> Result<()> {
    for (provider_id, ids) in subtitles {
        let item = snapshot
            .items
            .iter_mut()
            .find(|item| {
                item.provider_id == *provider_id
                    && item.published
                    && item.replacement_media_item_id.is_none()
            })
            .ok_or_else(invalid_playback_timeline)?;
        if ids.is_empty() || ids.len() > 2 || (ids.len() == 2 && ids[0] == ids[1]) {
            return Err(invalid_playback_timeline());
        }
        let mut languages = std::collections::BTreeSet::new();
        for id in ids {
            if !snapshot.artifact_ids.contains(id) {
                return Err(invalid_playback_timeline());
            }
            let language: Option<String> = sqlx::query_scalar(
                r#"SELECT "SubtitleLanguage" FROM "AddonMediaListArtifact"
                WHERE "Id"=? AND "AddonMediaListId"=? AND "Kind"=0 AND "State"=2
                  AND "SubtitleTimeline"=? AND "CompletedAt" IS NOT NULL"#,
            )
            .bind(id)
            .bind(snapshot.list_id)
            .bind(i32::from(item.prepared_artifact_id.is_some()))
            .fetch_optional(&mut **tx)
            .await
            .repository()?
            .flatten();
            let language = language.ok_or_else(invalid_playback_timeline)?;
            if !languages.insert(
                language
                    .split('-')
                    .next()
                    .unwrap_or_default()
                    .to_ascii_lowercase(),
            ) {
                return Err(invalid_playback_timeline());
            }
        }
        item.subtitle_artifact_ids.clone_from(ids);
    }
    Ok(())
}

/// Bind at most one Ready processed file to each published logical item.
async fn bind_prepared_revision_items_in_tx(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    snapshot: &mut AddonMediaListRevisionSnapshot,
    prepared_media: &BTreeMap<String, EntityId>,
) -> Result<()> {
    for (provider_id, artifact_id) in prepared_media {
        let item = snapshot
            .items
            .iter_mut()
            .find(|item| item.provider_id == *provider_id)
            .filter(|item| {
                item.published
                    && item.replacement_media_item_id.is_none()
                    && item.effective_liveness == RemoteStreamLiveness::Finite
            })
            .ok_or_else(|| {
                RepositoryError::Conflict(
                    "prepared media must belong to a published provider item".into(),
                )
            })?;
        if !snapshot.artifact_ids.contains(artifact_id) {
            return Err(RepositoryError::Conflict(
                "prepared media requires a retained artifact pin".into(),
            ));
        }
        let owned: bool = sqlx::query_scalar(
            r#"SELECT EXISTS (
                   SELECT 1 FROM "AddonMediaListArtifact" AS "artifact"
                   LEFT JOIN "AddonMediaListItem" AS "item"
                     ON "item"."Id" = "artifact"."AddonMediaListItemId"
                    AND "item"."AddonMediaListId" = "artifact"."AddonMediaListId"
                   WHERE "artifact"."Id" = ? AND "artifact"."AddonMediaListId" = ?
                     AND ("item"."ProviderId" = ? OR EXISTS (
                         SELECT 1 FROM "AddonMediaListRevisionItem" AS "retained_item"
                         JOIN "AddonMediaListRevision" AS "revision"
                           ON "revision"."Id" = "retained_item"."AddonMediaListRevisionId"
                         WHERE "revision"."AddonMediaListId" = "artifact"."AddonMediaListId"
                           AND "retained_item"."PreparedArtifactId" = "artifact"."Id"
                           AND "retained_item"."ProviderId" = ?
                           AND "retained_item"."SourceUrl" IS ?
                     )) AND "artifact"."Kind" = ?
                     AND "artifact"."State" = ? AND "artifact"."DurationMilliseconds" > 0
               )"#,
        )
        .bind(artifact_id)
        .bind(snapshot.list_id)
        .bind(provider_id)
        .bind(provider_id)
        .bind(&item.source_url)
        .bind(encode_enum(AddonMediaListArtifactKind::ProcessedMedia))
        .bind(encode_enum(AddonMediaListArtifactState::Ready))
        .fetch_one(&mut **tx)
        .await
        .repository()?;
        if !owned {
            return Err(RepositoryError::Conflict(
                "prepared media is not an admitted file owned by this item".into(),
            ));
        }
        item.prepared_artifact_id = Some(*artifact_id);
    }
    Ok(())
}

async fn load_current_revision_items_in_tx(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    list_id: EntityId,
) -> Result<Vec<AddonMediaListRevisionItem>> {
    let rows = sqlx::query(
        r#"SELECT "item"."ProviderId", "item"."Rank", "item"."Kind",
                  "item"."ContentKind", "item"."Included", "item"."Published",
                  "item"."DisplayTitle", "item"."Title", "item"."Year",
                  "item"."Season", "item"."Episode", "item"."SourceUrl",
                  "item"."AdditionalImageUrls",
                  "item"."DurationSeconds", "item"."ParentProviderId",
                  "item"."FragmentStartSeconds", "item"."FragmentEndSeconds",
                  COALESCE("item"."ChapterInput", '') AS "ChapterInput",
                  COALESCE("item"."ProviderChapterInput", '') AS "ProviderChapterInput",
                  "item"."CollectionRole", "item"."Availability",
                  "item"."AvailabilityReason", "item"."AvailabilityReasonCode",
                  "item"."ProviderLiveness", "item"."LivenessOverride",
                  "item"."EffectiveLiveness", "item"."ProviderMetadataJson",
                  "item"."MetadataOverrideJson",
                  CASE WHEN "item"."Published" = 1 THEN "item"."MediaItemId" END
                      AS "MediaItemId",
                  "replacement"."MediaItemId" AS "ReplacementMediaItemId",
                  "replacement"."MediaVersionId" AS "ReplacementMediaVersionId"
           FROM "AddonMediaListItem" AS "item"
           LEFT JOIN "AddonMediaListItemReplacement" AS "replacement"
             ON "replacement"."AddonMediaListItemId" = "item"."Id"
           WHERE "item"."AddonMediaListId" = ?
           ORDER BY "item"."Rank", "item"."ProviderId""#,
    )
    .bind(list_id)
    .fetch_all(&mut **tx)
    .await
    .repository()?;
    let mut items = rows
        .iter()
        .map(decode_current_revision_item)
        .collect::<Result<Vec<_>>>()?;
    let guid_rows = sqlx::query(
        r#"SELECT "item"."ProviderId", "guid"."Guid"
           FROM "AddonMediaListItemGuid" AS "guid"
           JOIN "AddonMediaListItem" AS "item"
             ON "item"."Id" = "guid"."AddonMediaListItemId"
           WHERE "item"."AddonMediaListId" = ?
           ORDER BY "item"."ProviderId", "guid"."Guid", "guid"."Id""#,
    )
    .bind(list_id)
    .fetch_all(&mut **tx)
    .await
    .repository()?;
    let mut guids = BTreeMap::<String, Vec<String>>::new();
    for row in guid_rows {
        let values = guids
            .entry(row.try_get("ProviderId").repository()?)
            .or_default();
        let guid: String = row.try_get("Guid").repository()?;
        if values.last() != Some(&guid) {
            values.push(guid);
        }
    }
    for item in &mut items {
        item.guids = guids.remove(&item.provider_id).unwrap_or_default();
    }
    Ok(items)
}

fn decode_current_revision_item(row: &crate::db::DbRow) -> Result<AddonMediaListRevisionItem> {
    let metadata_json = merge_canonical_metadata(
        &row.try_get::<String, _>("ProviderMetadataJson")
            .repository()?,
        &row.try_get::<String, _>("MetadataOverrideJson")
            .repository()?,
    )?;
    Ok(AddonMediaListRevisionItem {
        provider_id: row.try_get("ProviderId").repository()?,
        guids: Vec::new(),
        rank: row.try_get("Rank").repository()?,
        kind: decode_enum_column("AddonMediaListItemKind", row, "Kind")?,
        content_kind: decode_enum_column("AddonMediaListContentKind", row, "ContentKind")?,
        included: decode_bool_column(row, "Included")?,
        published: decode_bool_column(row, "Published")?,
        display_title: row.try_get("DisplayTitle").repository()?,
        title: row.try_get("Title").repository()?,
        year: row.try_get("Year").repository()?,
        season: row.try_get("Season").repository()?,
        episode: row.try_get("Episode").repository()?,
        source_url: row.try_get("SourceUrl").repository()?,
        additional_image_urls: row.try_get("AdditionalImageUrls").repository()?,
        duration_seconds: decode_optional_u64_column(row, "DurationSeconds")?,
        parent_provider_id: row.try_get("ParentProviderId").repository()?,
        fragment_start_seconds: decode_optional_u64_column(row, "FragmentStartSeconds")?,
        fragment_end_seconds: decode_optional_u64_column(row, "FragmentEndSeconds")?,
        chapter_input: row.try_get("ChapterInput").repository()?,
        provider_chapter_input: row.try_get("ProviderChapterInput").repository()?,
        collection_role: decode_enum_column("AddonMediaListItemRole", row, "CollectionRole")?,
        availability: decode_enum_column("AddonMediaListItemAvailability", row, "Availability")?,
        availability_reason: row.try_get("AvailabilityReason").repository()?,
        availability_reason_code: decode_optional_enum_column(
            "AddonMediaListAvailabilityReason",
            row,
            "AvailabilityReasonCode",
        )?,
        provider_liveness: decode_enum_column("RemoteStreamLiveness", row, "ProviderLiveness")?,
        liveness_override: decode_optional_enum_column(
            "RemoteStreamLiveness",
            row,
            "LivenessOverride",
        )?,
        effective_liveness: decode_enum_column("RemoteStreamLiveness", row, "EffectiveLiveness")?,
        metadata_digest: sha256_hex(metadata_json.as_bytes()),
        metadata_json,
        replacement_media_item_id: row.try_get("ReplacementMediaItemId").repository()?,
        replacement_media_version_id: row.try_get("ReplacementMediaVersionId").repository()?,
        published_media_item_id: row.try_get("MediaItemId").repository()?,
        prepared_artifact_id: None,
        subtitle_artifact_ids: Vec::new(),
        playback_timeline: None,
    })
}

async fn insert_revision_snapshot_in_tx(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    snapshot: &AddonMediaListRevisionSnapshot,
) -> Result<()> {
    let schema_id = resolve_schema(tx, snapshot.list_id, snapshot.options.schema.as_ref())
        .await?
        .map(|(id, _)| id);
    let inserted = sqlx::query(
        r#"INSERT INTO "AddonMediaListRevision" (
               "AddonMediaListId", "Revision", "SourceDraftRevision", "OptionSchemaId",
               "OptionValuesJson", "CompletedAt", "SnapshotVersion", "SnapshotDigest",
               "Name", "Description", "GeneratePlaylist", "ContentKindOverride",
               "PresentationKind", "RequestedPresentationKind", "MetadataJson"
           ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(snapshot.list_id)
    .bind(snapshot.revision)
    .bind(snapshot.source_draft_revision)
    .bind(schema_id)
    .bind(encode_option_values(&snapshot.options.list_values)?)
    .bind(encode_datetime(snapshot.completed_at))
    .bind(REVISION_SNAPSHOT_VERSION)
    .bind(&snapshot.snapshot_digest)
    .bind(&snapshot.name)
    .bind(&snapshot.description)
    .bind(snapshot.generate_playlist)
    .bind(encode_enum(snapshot.content_kind_override))
    .bind(encode_enum(snapshot.presentation_kind))
    .bind(encode_enum(snapshot.requested_presentation_kind))
    .bind(&snapshot.metadata_json)
    .execute(&mut **tx)
    .await
    .repository()?;
    let revision_id = inserted.last_insert_rowid();

    for option in &snapshot.options.item_values {
        if option.values.is_empty() {
            continue;
        }
        sqlx::query(
            r#"INSERT INTO "AddonMediaListRevisionItemOption" (
                   "AddonMediaListRevisionId", "ProviderId", "OptionOverridesJson"
               ) VALUES (?, ?, ?)"#,
        )
        .bind(revision_id)
        .bind(&option.provider_id)
        .bind(encode_option_values(&option.values)?)
        .execute(&mut **tx)
        .await
        .repository()?;
    }
    for item in &snapshot.items {
        insert_revision_item_in_tx(tx, revision_id, item).await?;
    }
    for file in &snapshot.files {
        sqlx::query(
            r#"INSERT INTO "AddonMediaListRevisionFile" (
                   "AddonMediaListRevisionId", "AddonMediaListRevisionFileBlobId",
                   "PublishedRelativePath", "ProviderId"
               ) VALUES (?, ?, ?, ?)"#,
        )
        .bind(revision_id)
        .bind(file.blob_id)
        .bind(&file.published_relative_path)
        .bind(&file.provider_id)
        .execute(&mut **tx)
        .await
        .repository()?;
    }
    for artifact_id in &snapshot.artifact_ids {
        sqlx::query(
            r#"INSERT INTO "AddonMediaListRevisionArtifact" (
                   "AddonMediaListRevisionId", "AddonMediaListArtifactId"
               ) VALUES (?, ?)"#,
        )
        .bind(revision_id)
        .bind(artifact_id)
        .execute(&mut **tx)
        .await
        .repository()?;
    }
    Ok(())
}

async fn insert_revision_item_in_tx(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    revision_id: EntityId,
    item: &AddonMediaListRevisionItem,
) -> Result<()> {
    sqlx::query(
        r#"INSERT INTO "AddonMediaListRevisionItem" (
               "AddonMediaListRevisionId", "ProviderId", "GuidsJson", "Rank", "Kind", "ContentKind",
               "Included", "Published", "DisplayTitle", "Title", "Year", "Season", "Episode",
               "SourceUrl", "AdditionalImageUrls", "DurationSeconds", "ParentProviderId", "FragmentStartSeconds",
               "FragmentEndSeconds", "ChapterInput", "ProviderChapterInput", "CollectionRole",
               "Availability", "AvailabilityReason", "AvailabilityReasonCode",
               "ProviderLiveness", "LivenessOverride", "EffectiveLiveness", "MetadataJson",
               "MetadataDigest", "ReplacementMediaItemId", "ReplacementMediaVersionId",
               "PublishedMediaItemId", "PreparedArtifactId", "PlaybackTimelineJson", "SubtitleArtifactIdsJson"
           ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(revision_id)
    .bind(&item.provider_id)
    .bind(encode_string_vec(&item.guids)?)
    .bind(item.rank)
    .bind(encode_enum(item.kind))
    .bind(encode_enum(item.content_kind))
    .bind(item.included)
    .bind(item.published)
    .bind(&item.display_title)
    .bind(&item.title)
    .bind(item.year)
    .bind(item.season)
    .bind(item.episode)
    .bind(&item.source_url)
    .bind(&item.additional_image_urls)
    .bind(item.duration_seconds.map(|value| encode_i64("revision item duration", value)).transpose()?)
    .bind(&item.parent_provider_id)
    .bind(item.fragment_start_seconds.map(|value| encode_i64("revision fragment start", value)).transpose()?)
    .bind(item.fragment_end_seconds.map(|value| encode_i64("revision fragment finish", value)).transpose()?)
    .bind(&item.chapter_input)
    .bind(&item.provider_chapter_input)
    .bind(encode_enum(item.collection_role))
    .bind(encode_enum(item.availability))
    .bind(&item.availability_reason)
    .bind(item.availability_reason_code.map(encode_enum))
    .bind(encode_enum(item.provider_liveness))
    .bind(item.liveness_override.map(encode_enum))
    .bind(encode_enum(item.effective_liveness))
    .bind(&item.metadata_json)
    .bind(&item.metadata_digest)
    .bind(item.replacement_media_item_id)
    .bind(item.replacement_media_version_id)
    .bind(item.published_media_item_id)
    .bind(item.prepared_artifact_id)
    .bind(item.playback_timeline.as_ref().map(serde_json::to_string).transpose()
        .map_err(|_| invalid_playback_timeline())?)
    .bind(serde_json::to_string(&item.subtitle_artifact_ids).map_err(|_| invalid_playback_timeline())?)
    .execute(&mut **tx)
    .await
    .repository()?;
    Ok(())
}

/// Load and integrity-check one exact retained revision from a single database
/// snapshot. Callers decide whether a decode failure is fatal or merely makes
/// an operator-facing history row non-restorable.
#[allow(clippy::too_many_lines)] // Keep reconstruction and integrity checks in one database snapshot.
pub(crate) async fn load_valid_revision_snapshot_in_tx(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    list_id: EntityId,
    revision: i32,
) -> Result<Option<AddonMediaListRevisionSnapshot>> {
    let row = sqlx::query(
        r#"SELECT "Id", "SourceDraftRevision", "CompletedAt", "SnapshotVersion",
                  "SnapshotDigest", "Name", "Description", "GeneratePlaylist",
                  "ContentKindOverride", "PresentationKind", "RequestedPresentationKind",
                  "MetadataJson"
           FROM "AddonMediaListRevision"
           WHERE "AddonMediaListId" = ? AND "Revision" = ?"#,
    )
    .bind(list_id)
    .bind(revision)
    .fetch_optional(&mut **tx)
    .await
    .repository()?;
    let Some(row) = row else {
        return Ok(None);
    };
    let snapshot_digest: Option<String> = row.try_get("SnapshotDigest").repository()?;
    let Some(snapshot_digest) = snapshot_digest else {
        return Ok(None);
    };
    let snapshot_version: Option<i32> = row.try_get("SnapshotVersion").repository()?;
    if snapshot_version != Some(REVISION_SNAPSHOT_VERSION) {
        return Err(RepositoryError::Decode {
            kind: "add-on media-list revision snapshot version",
            value: snapshot_version.map_or_else(|| "missing".into(), |value| value.to_string()),
        });
    }
    let source_draft_revision = required_revision_value(&row, "SourceDraftRevision")?;
    let completed_at = decode_optional_datetime(
        "AddonMediaListRevision.CompletedAt",
        row.try_get("CompletedAt").repository()?,
    )?
    .ok_or_else(|| RepositoryError::Decode {
        kind: "add-on media-list revision snapshot",
        value: "complete snapshot is missing its completion time".into(),
    })?;
    let revision_id: EntityId = row.try_get("Id").repository()?;
    let options = load_published_options_in_tx(tx, list_id, revision, None)
        .await?
        .ok_or_else(|| RepositoryError::Decode {
            kind: "add-on media-list revision snapshot",
            value: "snapshot option row is missing".into(),
        })?;
    let item_rows = sqlx::query(
        r#"SELECT * FROM "AddonMediaListRevisionItem"
           WHERE "AddonMediaListRevisionId" = ? ORDER BY "Rank", "ProviderId""#,
    )
    .bind(revision_id)
    .fetch_all(&mut **tx)
    .await
    .repository()?;
    let items = item_rows
        .iter()
        .map(decode_revision_item_row)
        .collect::<Result<Vec<_>>>()?;
    let file_rows = sqlx::query(
        r#"SELECT "file"."AddonMediaListRevisionFileBlobId", "file"."PublishedRelativePath",
                  "file"."ProviderId", "blob"."StorageRelativePath", "blob"."Sha256",
                  "blob"."ByteLength"
           FROM "AddonMediaListRevisionFile" AS "file"
           JOIN "AddonMediaListRevisionFileBlob" AS "blob"
             ON "blob"."Id" = "file"."AddonMediaListRevisionFileBlobId"
           WHERE "file"."AddonMediaListRevisionId" = ?
           ORDER BY "file"."PublishedRelativePath""#,
    )
    .bind(revision_id)
    .fetch_all(&mut **tx)
    .await
    .repository()?;
    let files = file_rows
        .iter()
        .map(decode_revision_file_row)
        .collect::<Result<Vec<_>>>()?;
    let artifact_ids = sqlx::query_scalar::<_, EntityId>(
        r#"SELECT "AddonMediaListArtifactId" FROM "AddonMediaListRevisionArtifact"
           WHERE "AddonMediaListRevisionId" = ? ORDER BY "AddonMediaListArtifactId""#,
    )
    .bind(revision_id)
    .fetch_all(&mut **tx)
    .await
    .repository()?;
    let snapshot = AddonMediaListRevisionSnapshot {
        list_id,
        revision,
        source_draft_revision,
        completed_at,
        name: required_string_value(&row, "Name")?,
        description: row.try_get("Description").repository()?,
        generate_playlist: decode_required_optional_bool(&row, "GeneratePlaylist")?,
        content_kind_override: decode_required_optional_enum(
            "AddonMediaListContentKind",
            &row,
            "ContentKindOverride",
        )?,
        presentation_kind: decode_required_optional_enum(
            "AddonMediaListPresentationKind",
            &row,
            "PresentationKind",
        )?,
        requested_presentation_kind: decode_required_optional_enum(
            "AddonMediaListPresentationKind",
            &row,
            "RequestedPresentationKind",
        )?,
        metadata_json: required_string_value(&row, "MetadataJson")?,
        options,
        items,
        files,
        artifact_ids,
        snapshot_digest,
    };
    let actual_digest = revision_snapshot_digest(&snapshot)?;
    if actual_digest != snapshot.snapshot_digest {
        return Err(RepositoryError::Decode {
            kind: "add-on media-list revision snapshot digest",
            value: format!("revision {revision} failed integrity validation"),
        });
    }
    Ok(Some(snapshot))
}

fn decode_revision_item_row(row: &crate::db::DbRow) -> Result<AddonMediaListRevisionItem> {
    let playback_timeline = row
        .try_get::<Option<String>, _>("PlaybackTimelineJson")
        .repository()?
        .map(|json| {
            if json.len() > 65_536 {
                return Err(invalid_playback_timeline());
            }
            serde_json::from_str::<AddonMediaListPlaybackTimelinePlan>(&json)
                .map_err(|_| invalid_playback_timeline())
        })
        .transpose()?;
    Ok(AddonMediaListRevisionItem {
        provider_id: row.try_get("ProviderId").repository()?,
        guids: decode_string_vec(
            "add-on media-list revision item GUIDs",
            &row.try_get::<String, _>("GuidsJson").repository()?,
        )?,
        rank: row.try_get("Rank").repository()?,
        kind: decode_enum_column("AddonMediaListItemKind", row, "Kind")?,
        content_kind: decode_enum_column("AddonMediaListContentKind", row, "ContentKind")?,
        included: decode_bool_column(row, "Included")?,
        published: decode_bool_column(row, "Published")?,
        display_title: row.try_get("DisplayTitle").repository()?,
        title: row.try_get("Title").repository()?,
        year: row.try_get("Year").repository()?,
        season: row.try_get("Season").repository()?,
        episode: row.try_get("Episode").repository()?,
        source_url: row.try_get("SourceUrl").repository()?,
        additional_image_urls: row.try_get("AdditionalImageUrls").repository()?,
        duration_seconds: decode_optional_u64_column(row, "DurationSeconds")?,
        parent_provider_id: row.try_get("ParentProviderId").repository()?,
        fragment_start_seconds: decode_optional_u64_column(row, "FragmentStartSeconds")?,
        fragment_end_seconds: decode_optional_u64_column(row, "FragmentEndSeconds")?,
        chapter_input: row.try_get("ChapterInput").repository()?,
        provider_chapter_input: row.try_get("ProviderChapterInput").repository()?,
        collection_role: decode_enum_column("AddonMediaListItemRole", row, "CollectionRole")?,
        availability: decode_enum_column("AddonMediaListItemAvailability", row, "Availability")?,
        availability_reason: row.try_get("AvailabilityReason").repository()?,
        availability_reason_code: decode_optional_enum_column(
            "AddonMediaListAvailabilityReason",
            row,
            "AvailabilityReasonCode",
        )?,
        provider_liveness: decode_enum_column("RemoteStreamLiveness", row, "ProviderLiveness")?,
        liveness_override: decode_optional_enum_column(
            "RemoteStreamLiveness",
            row,
            "LivenessOverride",
        )?,
        effective_liveness: decode_enum_column("RemoteStreamLiveness", row, "EffectiveLiveness")?,
        metadata_json: row.try_get("MetadataJson").repository()?,
        metadata_digest: row.try_get("MetadataDigest").repository()?,
        replacement_media_item_id: row.try_get("ReplacementMediaItemId").repository()?,
        replacement_media_version_id: row.try_get("ReplacementMediaVersionId").repository()?,
        published_media_item_id: row.try_get("PublishedMediaItemId").repository()?,
        prepared_artifact_id: row.try_get("PreparedArtifactId").repository()?,
        subtitle_artifact_ids: serde_json::from_str(
            &row.try_get::<String, _>("SubtitleArtifactIdsJson")
                .repository()?,
        )
        .map_err(|_| invalid_playback_timeline())?,
        playback_timeline,
    })
}

fn invalid_playback_timeline() -> RepositoryError {
    RepositoryError::Decode {
        kind: "add-on media-list playback timeline",
        value: "invalid retained interval plan".into(),
    }
}

fn decode_revision_file_blob_row(row: &crate::db::DbRow) -> Result<AddonMediaListRevisionFileBlob> {
    Ok(AddonMediaListRevisionFileBlob {
        id: row.try_get("Id").repository()?,
        list_id: row.try_get("AddonMediaListId").repository()?,
        storage_relative_path: row.try_get("StorageRelativePath").repository()?,
        sha256: row.try_get("Sha256").repository()?,
        byte_length: decode_u64(
            "retained revision file byte length",
            row.try_get("ByteLength").repository()?,
        )?,
        created_at: decode_required_datetime(
            "AddonMediaListRevisionFileBlob.CreatedAt",
            row.try_get::<String, _>("CreatedAt").repository()?,
        )?,
    })
}

fn decode_revision_file_row(row: &crate::db::DbRow) -> Result<AddonMediaListRevisionFile> {
    Ok(AddonMediaListRevisionFile {
        blob_id: row
            .try_get("AddonMediaListRevisionFileBlobId")
            .repository()?,
        published_relative_path: row.try_get("PublishedRelativePath").repository()?,
        provider_id: row.try_get("ProviderId").repository()?,
        storage_relative_path: row.try_get("StorageRelativePath").repository()?,
        sha256: row.try_get("Sha256").repository()?,
        byte_length: decode_u64(
            "retained revision file byte length",
            row.try_get("ByteLength").repository()?,
        )?,
    })
}

fn decode_revision_summary_row(
    row: &crate::db::DbRow,
    restorable: bool,
) -> Result<AddonMediaListRevisionSummary> {
    let source_draft_revision = row.try_get("SourceDraftRevision").repository()?;
    let completed_at = decode_optional_datetime(
        "AddonMediaListRevision.CompletedAt",
        row.try_get("CompletedAt").repository()?,
    )?;
    let snapshot_digest: Option<String> = row.try_get("SnapshotDigest").repository()?;
    Ok(AddonMediaListRevisionSummary {
        revision: row.try_get("Revision").repository()?,
        source_draft_revision,
        completed_at,
        restorable,
        snapshot_digest,
        published_item_count: decode_u64(
            "revision published item count",
            row.try_get("PublishedItemCount").repository()?,
        )?,
        file_count: decode_u64(
            "revision file count",
            row.try_get("FileCount").repository()?,
        )?,
        artifact_count: decode_u64(
            "revision artifact count",
            row.try_get("ArtifactCount").repository()?,
        )?,
    })
}

fn revision_snapshot_digest(snapshot: &AddonMediaListRevisionSnapshot) -> Result<String> {
    let mut payload = snapshot.clone();
    payload.snapshot_digest.clear();
    let bytes = serde_json::to_vec(&payload).map_err(|error| RepositoryError::Decode {
        kind: "add-on media-list revision snapshot",
        value: error.to_string(),
    })?;
    Ok(sha256_hex(&bytes))
}

fn compare_revision_snapshots(
    left: &AddonMediaListRevisionSnapshot,
    right: &AddonMediaListRevisionSnapshot,
) -> AddonMediaListRevisionDiff {
    let left_items = left
        .items
        .iter()
        .filter(|item| item.published)
        .map(|item| (item.provider_id.as_str(), item))
        .collect::<BTreeMap<_, _>>();
    let right_items = right
        .items
        .iter()
        .filter(|item| item.published)
        .map(|item| (item.provider_id.as_str(), item))
        .collect::<BTreeMap<_, _>>();
    let items_added = right_items
        .keys()
        .filter(|provider_id| !left_items.contains_key(**provider_id))
        .count() as u64;
    let items_removed = left_items
        .keys()
        .filter(|provider_id| !right_items.contains_key(**provider_id))
        .count() as u64;
    let items_changed = left_items
        .iter()
        .filter(|(provider_id, item)| {
            right_items
                .get(**provider_id)
                .is_some_and(|right| **item != *right)
        })
        .count() as u64;

    let left_files = left
        .files
        .iter()
        .map(|file| (file.published_relative_path.as_str(), file))
        .collect::<BTreeMap<_, _>>();
    let right_files = right
        .files
        .iter()
        .map(|file| (file.published_relative_path.as_str(), file))
        .collect::<BTreeMap<_, _>>();
    let files_added = right_files
        .keys()
        .filter(|path| !left_files.contains_key(**path))
        .count() as u64;
    let files_removed = left_files
        .keys()
        .filter(|path| !right_files.contains_key(**path))
        .count() as u64;
    let files_changed = left_files
        .iter()
        .filter(|(path, file)| {
            right_files
                .get(**path)
                .is_some_and(|right| **file != *right)
        })
        .count() as u64;
    let left_artifacts = left.artifact_ids.iter().copied().collect::<BTreeSet<_>>();
    let right_artifacts = right.artifact_ids.iter().copied().collect::<BTreeSet<_>>();
    AddonMediaListRevisionDiff {
        list_metadata_changed: left.name != right.name
            || left.description != right.description
            || left.generate_playlist != right.generate_playlist
            || left.content_kind_override != right.content_kind_override
            || left.presentation_kind != right.presentation_kind
            || left.requested_presentation_kind != right.requested_presentation_kind
            || left.metadata_json != right.metadata_json,
        effective_options_changed: left.options.schema != right.options.schema
            || left.options.list_values != right.options.list_values
            || left.options.item_values != right.options.item_values,
        items_added,
        items_removed,
        items_changed,
        files_added,
        files_removed,
        files_changed,
        artifacts_added: right_artifacts.difference(&left_artifacts).count() as u64,
        artifacts_removed: left_artifacts.difference(&right_artifacts).count() as u64,
    }
}

#[allow(clippy::too_many_lines)] // Restore fencing, draft replacement, and marker commit are atomic.
async fn materialize_revision_as_draft(
    pool: &sqlx::Pool<crate::db::Db>,
    fence: &AddonMediaListRevisionRestoreFence,
) -> Result<i32> {
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    let snapshot =
        load_valid_revision_snapshot_in_tx(&mut tx, fence.list_id, fence.source_revision)
            .await?
            .ok_or_else(|| {
                RepositoryError::Conflict("restore revision is not restorable".into())
            })?;
    let active_at = encode_datetime(fence.active_at);
    let row = sqlx::query(
        r#"SELECT "job"."RestoreMaterializedAt", "list"."DraftRevision"
           FROM "AddonMediaListPublicationJob" AS "job"
           JOIN "ManagedLibraryOperation" AS "operation"
             ON "operation"."Id" = "job"."OperationId"
           JOIN "ManagedLibraryLease" AS "lease"
             ON "lease"."OperationId" = "operation"."Id"
            AND "lease"."OwnerToken" = "operation"."OwnerToken"
           JOIN "AddonMediaList" AS "list"
             ON "list"."Id" = "job"."AddonMediaListId"
           WHERE "job"."OperationId" = ? AND "job"."AddonMediaListId" = ?
             AND "job"."Generation" = ? AND "job"."SourcePublishedRevision" = ?
             AND "job"."SavedRevision" = ? AND "operation"."State" = ?
             AND "operation"."OwnerToken" = ? AND "operation"."LeaseOwner" = ?
             AND "lease"."LeaseOwner" = ? AND "lease"."LeaseExpiresAt" > ?"#,
    )
    .bind(fence.operation_id)
    .bind(fence.list_id)
    .bind(fence.generation)
    .bind(fence.source_revision)
    .bind(fence.target_draft_revision)
    .bind(encode_enum(ManagedLibraryOperationState::Running))
    .bind(&fence.owner_token)
    .bind(&fence.lease_owner)
    .bind(&fence.lease_owner)
    .bind(&active_at)
    .fetch_optional(&mut *tx)
    .await
    .repository()?
    .ok_or_else(|| {
        RepositoryError::Conflict(
            "restore materialization requires the exact live publication lease".into(),
        )
    })?;
    let current_draft_revision: i32 = row.try_get("DraftRevision").repository()?;
    let materialized_at: Option<String> = row.try_get("RestoreMaterializedAt").repository()?;
    if materialized_at.is_some() {
        if current_draft_revision == fence.target_draft_revision {
            tx.commit().await.repository()?;
            return Ok(current_draft_revision);
        }
        return Err(RepositoryError::Conflict(
            "the materialized restore draft changed after its durable marker".into(),
        ));
    }
    if current_draft_revision != fence.target_draft_revision.saturating_sub(1) {
        return Err(RepositoryError::Conflict(
            "restore materialization cannot replace an ambiguously changed draft".into(),
        ));
    }

    let schema_id = resolve_schema(&mut tx, fence.list_id, snapshot.options.schema.as_ref())
        .await?
        .map(|(id, _)| id);
    let updated = sqlx::query(
        r#"UPDATE "AddonMediaList"
           SET "Name" = ?, "Description" = ?, "GeneratePlaylist" = ?,
               "ContentKindOverride" = ?, "RequestedPresentationKind" = ?,
               "ProviderMetadataJson" = '{}',
               "MetadataOverrideJson" = ?, "MetadataConflictJson" = NULL,
               "DraftOptionSchemaId" = ?, "DraftOptionValuesJson" = ?,
               "DraftRevision" = ?
           WHERE "Id" = ? AND "DraftRevision" = ?
             AND EXISTS (
                 SELECT 1
                 FROM "AddonMediaListPublicationJob" AS "job"
                 JOIN "ManagedLibraryOperation" AS "operation"
                   ON "operation"."Id" = "job"."OperationId"
                 JOIN "ManagedLibraryLease" AS "lease"
                   ON "lease"."OperationId" = "operation"."Id"
                  AND "lease"."OwnerToken" = "operation"."OwnerToken"
                 WHERE "job"."AddonMediaListId" = "AddonMediaList"."Id"
                   AND "job"."OperationId" = ? AND "job"."Generation" = ?
                   AND "job"."SourcePublishedRevision" = ?
                   AND "job"."SavedRevision" = ?
                   AND "job"."RestoreMaterializedAt" IS NULL
                   AND "operation"."State" = ? AND "operation"."OwnerToken" = ?
                   AND "operation"."LeaseOwner" = ? AND "lease"."LeaseOwner" = ?
                   AND "lease"."LeaseExpiresAt" > ?
             )"#,
    )
    .bind(&snapshot.name)
    .bind(&snapshot.description)
    .bind(snapshot.generate_playlist)
    .bind(encode_enum(snapshot.content_kind_override))
    .bind(encode_enum(snapshot.requested_presentation_kind))
    .bind(&snapshot.metadata_json)
    .bind(schema_id)
    .bind(encode_option_values(&snapshot.options.list_values)?)
    .bind(fence.target_draft_revision)
    .bind(fence.list_id)
    .bind(current_draft_revision)
    .bind(fence.operation_id)
    .bind(fence.generation)
    .bind(fence.source_revision)
    .bind(fence.target_draft_revision)
    .bind(encode_enum(ManagedLibraryOperationState::Running))
    .bind(&fence.owner_token)
    .bind(&fence.lease_owner)
    .bind(&fence.lease_owner)
    .bind(&active_at)
    .execute(&mut *tx)
    .await
    .repository()?;
    if updated.rows_affected() != 1 {
        return Err(RepositoryError::Conflict(
            "restore materialization lost its draft or lease fence".into(),
        ));
    }

    sqlx::query(
        r#"UPDATE "AddonMediaListItem"
           SET "Included" = 0, "IncludedByUser" = 1
           WHERE "AddonMediaListId" = ?"#,
    )
    .bind(fence.list_id)
    .execute(&mut *tx)
    .await
    .repository()?;
    let empty_options = AddonMediaListOptionValues::new();
    let option_values = snapshot
        .options
        .item_values
        .iter()
        .map(|values| (values.provider_id.as_str(), &values.values))
        .collect::<BTreeMap<_, _>>();
    for item in &snapshot.items {
        let draft_option_json = encode_option_values(
            option_values
                .get(item.provider_id.as_str())
                .copied()
                .unwrap_or(&empty_options),
        )?;
        upsert_restored_draft_item_in_tx(&mut tx, fence.list_id, item, &draft_option_json).await?;
        replace_restored_item_guids_in_tx(&mut tx, fence.list_id, item).await?;
    }
    for item in &snapshot.items {
        let restored_item_id: EntityId = sqlx::query_scalar(
            r#"SELECT "Id" FROM "AddonMediaListItem"
               WHERE "AddonMediaListId" = ? AND "ProviderId" = ?"#,
        )
        .bind(fence.list_id)
        .bind(&item.provider_id)
        .fetch_one(&mut *tx)
        .await
        .repository()?;
        sqlx::query(
            r#"DELETE FROM "AddonMediaListItemReplacement" WHERE "AddonMediaListItemId" = ?"#,
        )
        .bind(restored_item_id)
        .execute(&mut *tx)
        .await
        .repository()?;
        match (
            item.replacement_media_item_id,
            item.replacement_media_version_id,
        ) {
            (Some(media_item_id), Some(media_version_id)) => {
                sqlx::query(
                    r#"INSERT INTO "AddonMediaListItemReplacement" (
                           "AddonMediaListItemId", "MediaItemId", "MediaVersionId",
                           "CreatedAt", "UpdatedAt"
                       )
                       SELECT "Id", ?, ?, ?, ? FROM "AddonMediaListItem"
                       WHERE "AddonMediaListId" = ? AND "ProviderId" = ?"#,
                )
                .bind(media_item_id)
                .bind(media_version_id)
                .bind(&active_at)
                .bind(&active_at)
                .bind(fence.list_id)
                .bind(&item.provider_id)
                .execute(&mut *tx)
                .await
                .repository()?;
            }
            (None, None) => {}
            _ => {
                return Err(RepositoryError::Decode {
                    kind: "add-on media-list revision replacement",
                    value: item.provider_id.clone(),
                });
            }
        }
    }
    let marked = sqlx::query(
        r#"UPDATE "AddonMediaListPublicationJob"
           SET "RestoreMaterializedAt" = ?, "UpdatedAt" = ?
           WHERE "OperationId" = ? AND "AddonMediaListId" = ? AND "Generation" = ?
             AND "SourcePublishedRevision" = ? AND "SavedRevision" = ?
             AND "RestoreMaterializedAt" IS NULL
             AND EXISTS (
                 SELECT 1 FROM "AddonMediaList"
                 WHERE "Id" = ? AND "DraftRevision" = ?
             )"#,
    )
    .bind(&active_at)
    .bind(&active_at)
    .bind(fence.operation_id)
    .bind(fence.list_id)
    .bind(fence.generation)
    .bind(fence.source_revision)
    .bind(fence.target_draft_revision)
    .bind(fence.list_id)
    .bind(fence.target_draft_revision)
    .execute(&mut *tx)
    .await
    .repository()?;
    if marked.rows_affected() != 1 {
        return Err(RepositoryError::Conflict(
            "restore materialization could not persist its durable marker".into(),
        ));
    }
    tx.commit().await.repository()?;
    Ok(fence.target_draft_revision)
}

#[allow(clippy::too_many_lines)] // Keep the restored item and its owned metadata writes together.
async fn upsert_restored_draft_item_in_tx(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    list_id: EntityId,
    item: &AddonMediaListRevisionItem,
    draft_option_json: &str,
) -> Result<()> {
    let duration = item
        .duration_seconds
        .map(|value| encode_i64("revision item duration", value))
        .transpose()?;
    let fragment_start = item
        .fragment_start_seconds
        .map(|value| encode_i64("revision fragment start", value))
        .transpose()?;
    let fragment_end = item
        .fragment_end_seconds
        .map(|value| encode_i64("revision fragment finish", value))
        .transpose()?;
    let existing: Option<EntityId> = sqlx::query_scalar(
        r#"SELECT "Id" FROM "AddonMediaListItem"
           WHERE "AddonMediaListId" = ? AND "ProviderId" = ?"#,
    )
    .bind(list_id)
    .bind(&item.provider_id)
    .fetch_optional(&mut **tx)
    .await
    .repository()?;
    if let Some(item_id) = existing {
        sqlx::query(
            r#"UPDATE "AddonMediaListItem"
               SET "Rank" = ?, "Kind" = ?, "ContentKind" = ?, "Included" = ?,
                   "IncludedByUser" = 1,
                   "DisplayTitle" = ?, "Title" = ?, "Year" = ?, "Season" = ?,
                   "Episode" = ?, "SourceUrl" = ?, "AdditionalImageUrls" = ?,
                   "DurationSeconds" = ?, "ParentProviderId" = ?,
                   "FragmentStartSeconds" = ?, "FragmentEndSeconds" = ?,
                   "ChapterInput" = ?, "ProviderChapterInput" = ?, "CollectionRole" = ?,
                   "Availability" = ?, "AvailabilityReason" = ?,
                   "AvailabilityReasonCode" = ?, "ProviderLiveness" = ?,
                   "LivenessOverride" = ?, "EffectiveLiveness" = ?, "LivenessConflict" = 0,
                   "ProviderMetadataJson" = '{}',
                   "MetadataOverrideJson" = ?, "MetadataConflictJson" = NULL,
                   "DraftOptionOverridesJson" = ?
               WHERE "Id" = ? AND "AddonMediaListId" = ?"#,
        )
        .bind(item.rank)
        .bind(encode_enum(item.kind))
        .bind(encode_enum(item.content_kind))
        .bind(item.included)
        .bind(&item.display_title)
        .bind(&item.title)
        .bind(item.year)
        .bind(item.season)
        .bind(item.episode)
        .bind(&item.source_url)
        .bind(&item.additional_image_urls)
        .bind(duration)
        .bind(&item.parent_provider_id)
        .bind(fragment_start)
        .bind(fragment_end)
        .bind(&item.chapter_input)
        .bind(&item.provider_chapter_input)
        .bind(encode_enum(item.collection_role))
        .bind(encode_enum(item.availability))
        .bind(&item.availability_reason)
        .bind(item.availability_reason_code.map(encode_enum))
        .bind(encode_enum(item.provider_liveness))
        .bind(item.liveness_override.map(encode_enum))
        .bind(encode_enum(item.effective_liveness))
        .bind(&item.metadata_json)
        .bind(draft_option_json)
        .bind(item_id)
        .bind(list_id)
        .execute(&mut **tx)
        .await
        .repository()?;
    } else {
        sqlx::query(
            r#"INSERT INTO "AddonMediaListItem" (
                   "AddonMediaListId", "ProviderId", "Rank", "Kind", "ContentKind",
                   "Included", "Published", "DisplayTitle", "Title", "Year", "Season",
                   "Episode", "SourceUrl", "AdditionalImageUrls", "DurationSeconds",
                   "ParentProviderId", "FragmentStartSeconds", "FragmentEndSeconds",
                   "ChapterInput", "ProviderChapterInput", "CollectionRole", "Availability",
                   "AvailabilityReason", "AvailabilityReasonCode", "ProviderLiveness",
                   "LivenessOverride", "EffectiveLiveness", "LivenessConflict",
                   "ProviderMetadataJson", "MetadataOverrideJson", "MetadataConflictJson",
                   "DraftOptionOverridesJson"
               ) VALUES (?, ?, ?, ?, ?, ?, 0, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, '{}', ?, NULL, ?)"#,
        )
        .bind(list_id)
        .bind(&item.provider_id)
        .bind(item.rank)
        .bind(encode_enum(item.kind))
        .bind(encode_enum(item.content_kind))
        .bind(item.included)
        .bind(&item.display_title)
        .bind(&item.title)
        .bind(item.year)
        .bind(item.season)
        .bind(item.episode)
        .bind(&item.source_url)
        .bind(&item.additional_image_urls)
        .bind(duration)
        .bind(&item.parent_provider_id)
        .bind(fragment_start)
        .bind(fragment_end)
        .bind(&item.chapter_input)
        .bind(&item.provider_chapter_input)
        .bind(encode_enum(item.collection_role))
        .bind(encode_enum(item.availability))
        .bind(&item.availability_reason)
        .bind(item.availability_reason_code.map(encode_enum))
        .bind(encode_enum(item.provider_liveness))
        .bind(item.liveness_override.map(encode_enum))
        .bind(encode_enum(item.effective_liveness))
        .bind(&item.metadata_json)
        .bind(draft_option_json)
        .execute(&mut **tx)
        .await
        .repository()?;
    }
    Ok(())
}

async fn replace_restored_item_guids_in_tx(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    list_id: EntityId,
    item: &AddonMediaListRevisionItem,
) -> Result<()> {
    let item_id: EntityId = sqlx::query_scalar(
        r#"SELECT "Id" FROM "AddonMediaListItem"
           WHERE "AddonMediaListId" = ? AND "ProviderId" = ?"#,
    )
    .bind(list_id)
    .bind(&item.provider_id)
    .fetch_one(&mut **tx)
    .await
    .repository()?;
    sqlx::query(r#"DELETE FROM "AddonMediaListItemGuid" WHERE "AddonMediaListItemId" = ?"#)
        .bind(item_id)
        .execute(&mut **tx)
        .await
        .repository()?;
    for guid in &item.guids {
        sqlx::query(
            r#"INSERT INTO "AddonMediaListItemGuid" ("AddonMediaListItemId", "Guid")
               VALUES (?, ?)"#,
        )
        .bind(item_id)
        .bind(guid)
        .execute(&mut **tx)
        .await
        .repository()?;
    }
    Ok(())
}

#[allow(clippy::too_many_lines)] // Keep the related review counters in one pass over the draft.
async fn calculate_saved_draft_diff_in_tx(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    list_id: EntityId,
    draft_revision: i32,
    published_revision: i32,
    snapshot: Option<&AddonMediaListRevisionSnapshot>,
) -> Result<AddonMediaListDraftDiff> {
    let published_items = snapshot
        .map(|snapshot| {
            snapshot
                .items
                .iter()
                .filter(|item| item.published)
                .map(|item| (item.provider_id.as_str(), item))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let current_items = load_current_revision_items_in_tx(tx, list_id).await?;
    let current_items = current_items
        .iter()
        .map(|item| (item.provider_id.as_str(), item))
        .collect::<BTreeMap<_, _>>();
    let draft_options = load_draft_options_in_tx(tx, list_id, None).await?;
    let current_item_options = draft_options
        .as_ref()
        .map(|options| item_option_values_by_provider(&options.item_values))
        .unwrap_or_default();
    let published_item_options = snapshot
        .map(|snapshot| item_option_values_by_provider(&snapshot.options.item_values))
        .unwrap_or_default();
    let rows = sqlx::query(
        r#"SELECT "item"."Id", "item"."ProviderId", "item"."Included", "item"."Availability",
                  "item"."MetadataConflictJson", "item"."LivenessConflict",
                  "tombstone"."MissingSince", "tombstone"."ConfirmedAt",
                  "tombstone"."ReappearedAt", "tombstone"."LastReconcileGeneration"
           FROM "AddonMediaListItem" AS "item"
           LEFT JOIN "AddonMediaListItemTombstone" AS "tombstone"
             ON "tombstone"."AddonMediaListItemId" = "item"."Id"
           WHERE "item"."AddonMediaListId" = ?
           ORDER BY "item"."ProviderId""#,
    )
    .bind(list_id)
    .fetch_all(&mut **tx)
    .await
    .repository()?;
    let mut result = AddonMediaListDraftDiff {
        draft_revision,
        published_revision: (published_revision > 0).then_some(published_revision),
        exact: snapshot.is_some(),
        ..AddonMediaListDraftDiff::default()
    };
    for row in rows {
        let item_id: EntityId = row.try_get("Id").repository()?;
        let provider_id: String = row.try_get("ProviderId").repository()?;
        let included = decode_bool_column(&row, "Included")?;
        let availability = decode_enum_column::<AddonMediaListItemAvailability>(
            "AddonMediaListItemAvailability",
            &row,
            "Availability",
        )?;
        if result.exact && included && !published_items.contains_key(provider_id.as_str()) {
            result.added += 1;
            result.added_item_ids.push(item_id);
        }
        let publication_changed = included
            && published_items
                .get(provider_id.as_str())
                .is_some_and(|published| {
                    current_items
                        .get(provider_id.as_str())
                        .is_some_and(|current| {
                            draft_publication_item_changed(current, published)
                                || effective_item_options_changed(
                                    draft_options.as_ref(),
                                    snapshot.map(|snapshot| &snapshot.options),
                                    current_item_options.get(provider_id.as_str()).copied(),
                                    published_item_options.get(provider_id.as_str()).copied(),
                                )
                        })
                });
        if publication_changed {
            result.metadata_changed += 1;
            result.metadata_changed_item_ids.push(item_id);
        }
        if !included {
            result.excluded += 1;
            result.excluded_item_ids.push(item_id);
        }
        if availability == AddonMediaListItemAvailability::Unknown {
            result.unknown += 1;
            result.unknown_item_ids.push(item_id);
        }
        if availability == AddonMediaListItemAvailability::Unavailable {
            result.unavailable += 1;
            result.unavailable_item_ids.push(item_id);
        }
        let metadata_conflict = row
            .try_get::<Option<String>, _>("MetadataConflictJson")
            .repository()?
            .is_some_and(|value| {
                let value = value.trim();
                !value.is_empty() && value != "{}"
            });
        if metadata_conflict || decode_bool_column(&row, "LivenessConflict")? {
            result.conflicted += 1;
            result.conflicted_item_ids.push(item_id);
        }
        let missing_since = row
            .try_get::<Option<String>, _>("MissingSince")
            .repository()?;
        let confirmed_at = row
            .try_get::<Option<String>, _>("ConfirmedAt")
            .repository()?;
        let reappeared_at = row
            .try_get::<Option<String>, _>("ReappearedAt")
            .repository()?;
        if reappeared_at.is_some() {
            result.reappeared += 1;
            result.reappeared_item_ids.push(item_id);
        } else if confirmed_at.is_some() {
            result.removal_candidates += 1;
            result.removal_candidate_item_ids.push(item_id);
        } else if missing_since.is_some() {
            result.pending_removals += 1;
            result.pending_removal_item_ids.push(item_id);
            result.pending_removal_generations.insert(
                item_id,
                row.try_get("LastReconcileGeneration").repository()?,
            );
        }
    }
    Ok(result)
}

fn item_option_values_by_provider(
    values: &[AddonMediaListItemOptionValues],
) -> BTreeMap<&str, &AddonMediaListOptionValues> {
    values
        .iter()
        .map(|item| (item.provider_id.as_str(), &item.values))
        .collect()
}

fn effective_item_options_changed(
    current: Option<&AddonMediaListDraftOptions>,
    published: Option<&AddonMediaListPublishedOptions>,
    current_item: Option<&AddonMediaListOptionValues>,
    published_item: Option<&AddonMediaListOptionValues>,
) -> bool {
    let (Some(current), Some(published)) = (current, published) else {
        return current.is_some() != published.is_some();
    };
    if current.schema != published.schema {
        return true;
    }
    let mut current_effective = current.list_values.clone();
    if let Some(values) = current_item {
        current_effective.extend(values.clone());
    }
    let mut published_effective = published.list_values.clone();
    if let Some(values) = published_item {
        published_effective.extend(values.clone());
    }
    current_effective != published_effective
}

fn draft_publication_item_changed(
    current: &AddonMediaListRevisionItem,
    published: &AddonMediaListRevisionItem,
) -> bool {
    // Inclusion has its own compact review counters, while Published and its
    // generated media id and playback selections describe the immutable live
    // projection rather than the saved draft. Draft loaders do not populate
    // these fields; comparing them would permanently flag unchanged items.
    // Source timing, metadata and effective options are compared separately.
    let mut current = current.clone();
    let mut published = published.clone();
    current.included = true;
    published.included = true;
    current.published = false;
    published.published = false;
    current.published_media_item_id = None;
    published.published_media_item_id = None;
    current.prepared_artifact_id = None;
    published.prepared_artifact_id = None;
    current.playback_timeline = None;
    published.playback_timeline = None;
    current.subtitle_artifact_ids.clear();
    published.subtitle_artifact_ids.clear();
    current != published
}

#[allow(clippy::too_many_lines)] // Keep impact projections with their final revision consistency check.
async fn calculate_reset_impact(
    pool: &sqlx::Pool<crate::db::Db>,
    list_id: EntityId,
    as_of: chrono::DateTime<chrono::Utc>,
) -> Result<AddonMediaListResetImpact> {
    let list = sqlx::query(
        r#"SELECT "DraftRevision", "PublishedRevision", "PlaylistId"
           FROM "AddonMediaList" WHERE "Id" = ?"#,
    )
    .bind(list_id)
    .fetch_optional(pool)
    .await
    .repository()?
    .ok_or(RepositoryError::NotFound {
        kind: "add-on media list",
        id: list_id,
    })?;
    let draft_revision: i32 = list.try_get("DraftRevision").repository()?;
    let published_revision: i32 = list.try_get("PublishedRevision").repository()?;
    let playlist_id: Option<EntityId> = list.try_get("PlaylistId").repository()?;
    // Reset deletes every projected media item under every path owned by the
    // bound managed library. Derive impact from that same boundary rather than
    // from the narrower, mutable AddonMediaListItem publication flags.
    let reset_media_ids = sqlx::query_scalar::<_, EntityId>(
        r#"SELECT DISTINCT "media"."Id"
           FROM "MediaItem" AS "media"
           JOIN "LibraryPath" AS "path" ON "path"."Id" = "media"."LibraryPathId"
           JOIN "AddonMediaListLibrary" AS "binding"
             ON "binding"."LibraryId" = "path"."LibraryId"
           WHERE "binding"."AddonMediaListId" = ?
           ORDER BY "media"."Id""#,
    )
    .bind(list_id)
    .fetch_all(pool)
    .await
    .repository()?
    .into_iter()
    .collect::<BTreeSet<_>>();
    let published_items = reset_media_ids.len() as u64;
    let generated_playlists = if let Some(playlist_id) = playlist_id {
        i64::from(
            sqlx::query_scalar::<_, bool>(
                r#"SELECT EXISTS (SELECT 1 FROM "Playlist" WHERE "Id" = ?)"#,
            )
            .bind(playlist_id)
            .fetch_one(pool)
            .await
            .repository()?,
        )
    } else {
        0
    };

    let scheduling =
        crate::repositories::scheduling::SqliteSchedulingContentRepository::new(pool.clone());
    let mut affected_playouts = scheduling
        .classic_playouts_affected_by_scan(&reset_media_ids, playlist_id)
        .await?
        .into_iter()
        .collect::<BTreeSet<_>>();
    let as_of = encode_datetime(as_of);
    // Persisted playout items are the common dependency projection for every
    // schedule kind, including Block, Sequential, Scripted, and ExternalJson.
    let media_playouts = playout_ids_for_media(pool, &reset_media_ids, None).await?;
    affected_playouts.extend(media_playouts.iter().copied());

    let mut channel_ids = BTreeSet::new();
    let mut schedule_ids = BTreeSet::new();
    if !affected_playouts.is_empty() {
        let rows = sqlx::query(r#"SELECT "Id", "ChannelId", "ProgramScheduleId" FROM "Playout""#)
            .fetch_all(pool)
            .await
            .repository()?;
        for row in rows {
            let playout_id: EntityId = row.try_get("Id").repository()?;
            if !affected_playouts.contains(&playout_id) {
                continue;
            }
            channel_ids.insert(row.try_get::<EntityId, _>("ChannelId").repository()?);
            if let Some(schedule_id) = row
                .try_get::<Option<EntityId>, _>("ProgramScheduleId")
                .repository()?
            {
                schedule_ids.insert(schedule_id);
            }
        }
        let rows = sqlx::query(
            r#"SELECT "PlayoutId", "ProgramScheduleId" FROM "ProgramScheduleAlternate""#,
        )
        .fetch_all(pool)
        .await
        .repository()?;
        for row in rows {
            let playout_id: EntityId = row.try_get("PlayoutId").repository()?;
            if affected_playouts.contains(&playout_id) {
                schedule_ids.insert(row.try_get("ProgramScheduleId").repository()?);
            }
        }
    }
    let active_or_future_playouts =
        active_or_future_playout_count(pool, &affected_playouts, &as_of).await?;
    let unchanged = sqlx::query_scalar::<_, bool>(
        r#"SELECT EXISTS (
               SELECT 1 FROM "AddonMediaList"
               WHERE "Id" = ? AND "DraftRevision" = ? AND "PublishedRevision" = ?
           )"#,
    )
    .bind(list_id)
    .bind(draft_revision)
    .bind(published_revision)
    .fetch_one(pool)
    .await
    .repository()?;
    if !unchanged {
        return Err(RepositoryError::Conflict(
            "the add-on media-list changed while calculating reset impact".into(),
        ));
    }
    Ok(AddonMediaListResetImpact {
        draft_revision,
        published_revision,
        published_items,
        generated_playlists: decode_u64("reset generated playlist count", generated_playlists)?,
        channels: channel_ids.len() as u64,
        schedules: schedule_ids.len() as u64,
        active_or_future_playouts,
    })
}

async fn playout_ids_for_media(
    pool: &sqlx::Pool<crate::db::Db>,
    media_ids: &BTreeSet<EntityId>,
    as_of: Option<&str>,
) -> Result<BTreeSet<EntityId>> {
    if media_ids.is_empty() {
        return Ok(BTreeSet::new());
    }
    let mut query = sqlx::QueryBuilder::<crate::db::Db>::new(
        r#"SELECT DISTINCT "PlayoutId" FROM "PlayoutItem" WHERE "#,
    );
    if let Some(as_of) = as_of {
        query.push(r#""Finish" >= "#);
        query.push_bind(as_of);
        query.push(" AND ");
    }
    query.push(r#""MediaItemId" IN ("#);
    let mut separated = query.separated(", ");
    for media_id in media_ids {
        separated.push_bind(media_id);
    }
    separated.push_unseparated(")");
    query
        .build_query_scalar::<EntityId>()
        .fetch_all(pool)
        .await
        .repository()
        .map(|values| values.into_iter().collect())
}

async fn active_or_future_playout_count(
    pool: &sqlx::Pool<crate::db::Db>,
    playout_ids: &BTreeSet<EntityId>,
    as_of: &str,
) -> Result<u64> {
    if playout_ids.is_empty() {
        return Ok(0);
    }
    let mut query = sqlx::QueryBuilder::<crate::db::Db>::new(
        r#"SELECT COUNT(DISTINCT "PlayoutId") FROM "PlayoutItem" WHERE "Finish" >= "#,
    );
    query.push_bind(as_of);
    query.push(r#" AND "PlayoutId" IN ("#);
    let mut separated = query.separated(", ");
    for playout_id in playout_ids {
        separated.push_bind(playout_id);
    }
    separated.push_unseparated(")");
    let count: i64 = query
        .build_query_scalar()
        .fetch_one(pool)
        .await
        .repository()?;
    decode_u64("active or future playout count", count)
}

async fn prune_unpinned_revision_rows(
    pool: &sqlx::Pool<crate::db::Db>,
    list_id: EntityId,
    as_of: chrono::DateTime<chrono::Utc>,
) -> Result<Vec<i32>> {
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    if list_has_active_work_in_tx(&mut tx, list_id).await? {
        tx.commit().await.repository()?;
        return Ok(Vec::new());
    }
    let current_revision: i32 =
        sqlx::query_scalar(r#"SELECT "PublishedRevision" FROM "AddonMediaList" WHERE "Id" = ?"#)
            .bind(list_id)
            .fetch_optional(&mut *tx)
            .await
            .repository()?
            .ok_or(RepositoryError::NotFound {
                kind: "add-on media list",
                id: list_id,
            })?;
    let rows = sqlx::query(
        r#"SELECT "Id", "Revision" FROM "AddonMediaListRevision"
           WHERE "AddonMediaListId" = ? AND "SnapshotDigest" IS NOT NULL
             AND "CompletedAt" IS NOT NULL
           ORDER BY "Revision" DESC"#,
    )
    .bind(list_id)
    .fetch_all(&mut *tx)
    .await
    .repository()?;
    let as_of = encode_datetime(as_of);
    let mut removed = Vec::new();
    for row in rows.into_iter().skip(RETAINED_COMPLETE_REVISION_COUNT) {
        let revision_id: EntityId = row.try_get("Id").repository()?;
        let revision: i32 = row.try_get("Revision").repository()?;
        if revision == current_revision
            || revision_has_active_playout_pin_in_tx(&mut tx, revision_id, &as_of).await?
        {
            continue;
        }
        let deleted = sqlx::query(
            r#"DELETE FROM "AddonMediaListRevision"
               WHERE "Id" = ? AND "AddonMediaListId" = ? AND "Revision" <> ?"#,
        )
        .bind(revision_id)
        .bind(list_id)
        .bind(current_revision)
        .execute(&mut *tx)
        .await
        .repository()?;
        if deleted.rows_affected() == 1 {
            removed.push(revision);
        }
    }
    tx.commit().await.repository()?;
    Ok(removed)
}

async fn list_has_active_work_in_tx(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    list_id: EntityId,
) -> Result<bool> {
    sqlx::query_scalar(
        r#"SELECT EXISTS (
               SELECT 1
               FROM "AddonMediaListLibrary" AS "binding"
               JOIN "ManagedLibraryOperation" AS "operation"
                 ON "operation"."LibraryId" = "binding"."LibraryId"
               WHERE "binding"."AddonMediaListId" = ?
                 AND "operation"."State" IN (?, ?, ?)
           )"#,
    )
    .bind(list_id)
    .bind(encode_enum(ManagedLibraryOperationState::Queued))
    .bind(encode_enum(ManagedLibraryOperationState::Running))
    .bind(encode_enum(ManagedLibraryOperationState::WaitingToRetry))
    .fetch_one(&mut **tx)
    .await
    .repository()
}

async fn revision_has_active_playout_pin_in_tx(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    revision_id: EntityId,
    as_of: &str,
) -> Result<bool> {
    sqlx::query_scalar(
        r#"SELECT EXISTS (
               SELECT 1
               FROM "AddonMediaListRevisionItem" AS "item"
               JOIN "PlayoutItem" AS "playout_item"
                 ON "playout_item"."MediaItemId" IN (
                    "item"."PublishedMediaItemId", "item"."ReplacementMediaItemId"
                 )
               WHERE "item"."AddonMediaListRevisionId" = ?
                 AND "playout_item"."Finish" >= ?
           )"#,
    )
    .bind(revision_id)
    .bind(as_of)
    .fetch_one(&mut **tx)
    .await
    .repository()
}

async fn artifact_reference_pins(
    pool: &sqlx::Pool<crate::db::Db>,
    artifact_id: EntityId,
    as_of: chrono::DateTime<chrono::Utc>,
) -> Result<AddonMediaListReferencePins> {
    let row = sqlx::query(
        r#"SELECT "artifact"."AddonMediaListId", "artifact"."AddonMediaListItemId",
                  "item"."Published", "item"."MediaItemId",
                  "replacement"."MediaItemId" AS "ReplacementMediaItemId"
           FROM "AddonMediaListArtifact" AS "artifact"
           LEFT JOIN "AddonMediaListItem" AS "item"
             ON "item"."Id" = "artifact"."AddonMediaListItemId"
           LEFT JOIN "AddonMediaListItemReplacement" AS "replacement"
             ON "replacement"."AddonMediaListItemId" = "item"."Id"
           WHERE "artifact"."Id" = ?"#,
    )
    .bind(artifact_id)
    .fetch_optional(pool)
    .await
    .repository()?
    .ok_or(RepositoryError::NotFound {
        kind: "add-on media-list artifact",
        id: artifact_id,
    })?;
    let list_id: EntityId = row.try_get("AddonMediaListId").repository()?;
    let retained_revision: bool = sqlx::query_scalar(
        r#"SELECT EXISTS (
               SELECT 1 FROM "AddonMediaListRevisionArtifact"
               WHERE "AddonMediaListArtifactId" = ?
           )"#,
    )
    .bind(artifact_id)
    .fetch_one(pool)
    .await
    .repository()?;
    let active_work = list_has_active_work(pool, list_id).await?;
    let published_projection = row
        .try_get::<Option<i64>, _>("Published")
        .repository()?
        .is_some_and(|value| value != 0);
    let media_ids = [
        row.try_get::<Option<EntityId>, _>("MediaItemId")
            .repository()?,
        row.try_get::<Option<EntityId>, _>("ReplacementMediaItemId")
            .repository()?,
    ]
    .into_iter()
    .flatten()
    .collect::<BTreeSet<_>>();
    let active_or_future_playout =
        !playout_ids_for_media(pool, &media_ids, Some(&encode_datetime(as_of)))
            .await?
            .is_empty();
    Ok(AddonMediaListReferencePins {
        retained_revision,
        active_work,
        published_projection,
        active_or_future_playout,
    })
}

async fn revision_file_blob_reference_pins(
    pool: &sqlx::Pool<crate::db::Db>,
    blob_id: EntityId,
    as_of: chrono::DateTime<chrono::Utc>,
) -> Result<AddonMediaListReferencePins> {
    let list_id: EntityId = sqlx::query_scalar(
        r#"SELECT "AddonMediaListId" FROM "AddonMediaListRevisionFileBlob" WHERE "Id" = ?"#,
    )
    .bind(blob_id)
    .fetch_optional(pool)
    .await
    .repository()?
    .ok_or(RepositoryError::NotFound {
        kind: "add-on media-list revision file blob",
        id: blob_id,
    })?;
    let retained_revision: bool = sqlx::query_scalar(
        r#"SELECT EXISTS (
               SELECT 1 FROM "AddonMediaListRevisionFile"
               WHERE "AddonMediaListRevisionFileBlobId" = ?
           )"#,
    )
    .bind(blob_id)
    .fetch_one(pool)
    .await
    .repository()?;
    let published_projection: bool = sqlx::query_scalar(
        r#"SELECT EXISTS (
               SELECT 1
               FROM "AddonMediaListRevisionFile" AS "file"
               JOIN "AddonMediaListRevision" AS "revision"
                 ON "revision"."Id" = "file"."AddonMediaListRevisionId"
               JOIN "AddonMediaList" AS "list"
                 ON "list"."Id" = "revision"."AddonMediaListId"
                AND "list"."PublishedRevision" = "revision"."Revision"
               WHERE "file"."AddonMediaListRevisionFileBlobId" = ?
           )"#,
    )
    .bind(blob_id)
    .fetch_one(pool)
    .await
    .repository()?;
    let active_or_future_playout: bool = sqlx::query_scalar(
        r#"SELECT EXISTS (
               SELECT 1
               FROM "AddonMediaListRevisionFile" AS "file"
               JOIN "AddonMediaListRevisionItem" AS "item"
                 ON "item"."AddonMediaListRevisionId" = "file"."AddonMediaListRevisionId"
                AND ("file"."ProviderId" IS NULL
                     OR "file"."ProviderId" = "item"."ProviderId")
               JOIN "PlayoutItem" AS "playout_item"
                 ON "playout_item"."MediaItemId" IN (
                    "item"."PublishedMediaItemId", "item"."ReplacementMediaItemId"
                 )
               WHERE "file"."AddonMediaListRevisionFileBlobId" = ?
                 AND "playout_item"."Finish" >= ?
           )"#,
    )
    .bind(blob_id)
    .bind(encode_datetime(as_of))
    .fetch_one(pool)
    .await
    .repository()?;
    Ok(AddonMediaListReferencePins {
        retained_revision,
        active_work: list_has_active_work(pool, list_id).await?,
        published_projection,
        active_or_future_playout,
    })
}

async fn list_has_active_work(pool: &sqlx::Pool<crate::db::Db>, list_id: EntityId) -> Result<bool> {
    sqlx::query_scalar(
        r#"SELECT EXISTS (
               SELECT 1
               FROM "AddonMediaListLibrary" AS "binding"
               JOIN "ManagedLibraryOperation" AS "operation"
                 ON "operation"."LibraryId" = "binding"."LibraryId"
               WHERE "binding"."AddonMediaListId" = ?
                 AND "operation"."State" IN (?, ?, ?)
           )"#,
    )
    .bind(list_id)
    .bind(encode_enum(ManagedLibraryOperationState::Queued))
    .bind(encode_enum(ManagedLibraryOperationState::Running))
    .bind(encode_enum(ManagedLibraryOperationState::WaitingToRetry))
    .fetch_one(pool)
    .await
    .repository()
}

fn merge_canonical_metadata(provider_json: &str, override_json: &str) -> Result<String> {
    let mut provider =
        serde_json::from_str::<serde_json::Value>(provider_json).map_err(|error| {
            RepositoryError::Decode {
                kind: "add-on metadata JSON",
                value: error.to_string(),
            }
        })?;
    let overrides = serde_json::from_str::<serde_json::Value>(override_json).map_err(|error| {
        RepositoryError::Decode {
            kind: "add-on metadata JSON",
            value: error.to_string(),
        }
    })?;
    if let (Some(provider), Some(overrides)) = (provider.as_object_mut(), overrides.as_object()) {
        for (key, value) in overrides {
            provider.insert(key.clone(), value.clone());
        }
    }
    serde_json::to_string(&canonical_json_value(provider)).map_err(|error| {
        RepositoryError::Decode {
            kind: "add-on metadata JSON",
            value: error.to_string(),
        }
    })
}

fn canonical_json_value(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(values) => {
            let ordered = values
                .into_iter()
                .map(|(key, value)| (key, canonical_json_value(value)))
                .collect::<BTreeMap<_, _>>();
            serde_json::Value::Object(ordered.into_iter().collect())
        }
        serde_json::Value::Array(values) => {
            serde_json::Value::Array(values.into_iter().map(canonical_json_value).collect())
        }
        value => value,
    }
}

fn validate_revision_relative_path(value: &str) -> Result<()> {
    let path = Path::new(value);
    if value.trim().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::RootDir))
    {
        return Err(RepositoryError::Conflict(
            "retained revision paths must be relative to host-owned storage".into(),
        ));
    }
    Ok(())
}

fn decode_optional_u64_column(row: &crate::db::DbRow, column: &'static str) -> Result<Option<u64>> {
    row.try_get::<Option<i64>, _>(column)
        .repository()?
        .map(|value| decode_u64(column, value))
        .transpose()
}

fn required_revision_value(row: &crate::db::DbRow, column: &'static str) -> Result<i32> {
    row.try_get::<Option<i32>, _>(column)
        .repository()?
        .ok_or_else(|| RepositoryError::Decode {
            kind: "add-on media-list revision snapshot",
            value: format!("missing {column}"),
        })
}

fn required_string_value(row: &crate::db::DbRow, column: &'static str) -> Result<String> {
    row.try_get::<Option<String>, _>(column)
        .repository()?
        .ok_or_else(|| RepositoryError::Decode {
            kind: "add-on media-list revision snapshot",
            value: format!("missing {column}"),
        })
}

fn decode_required_optional_bool(row: &crate::db::DbRow, column: &'static str) -> Result<bool> {
    match row.try_get::<Option<i64>, _>(column).repository()? {
        Some(0) => Ok(false),
        Some(1) => Ok(true),
        Some(value) => Err(RepositoryError::Decode {
            kind: "add-on media-list revision Boolean",
            value: value.to_string(),
        }),
        None => Err(RepositoryError::Decode {
            kind: "add-on media-list revision snapshot",
            value: format!("missing {column}"),
        }),
    }
}

fn decode_required_optional_enum<T>(
    kind: &'static str,
    row: &crate::db::DbRow,
    column: &'static str,
) -> Result<T>
where
    T: crate::repositories::common::DbEnum,
{
    decode_optional_enum_column(kind, row, column)?.ok_or_else(|| RepositoryError::Decode {
        kind: "add-on media-list revision snapshot",
        value: format!("missing {column}"),
    })
}

async fn validate_lifecycle_job_admission(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    command: &CreateAddonMediaListLifecycleJob,
) -> Result<()> {
    let list_row = sqlx::query(
        r#"SELECT "binding"."LibraryId", "list"."DraftRevision",
                  "list"."PublishedRevision"
           FROM "AddonMediaList" AS "list"
           LEFT JOIN "AddonMediaListLibrary" AS "binding"
             ON "binding"."AddonMediaListId" = "list"."Id"
           WHERE "list"."Id" = ?"#,
    )
    .bind(command.list_id)
    .fetch_optional(&mut **tx)
    .await
    .repository()?
    .ok_or(RepositoryError::NotFound {
        kind: "add-on media list",
        id: command.list_id,
    })?;
    let bound_library_id: EntityId = list_row
        .try_get::<Option<EntityId>, _>("LibraryId")
        .repository()?
        .ok_or_else(|| {
            RepositoryError::Conflict("a lifecycle job requires a managed-library binding".into())
        })?;
    let current_draft_revision: i32 = list_row.try_get("DraftRevision").repository()?;
    let current_published_revision: i32 = list_row.try_get("PublishedRevision").repository()?;
    if command.saved_draft_revision != current_draft_revision
        || command.saved_published_revision != current_published_revision
    {
        return Err(RepositoryError::Conflict(
            "a lifecycle job must save the current draft and published revisions".into(),
        ));
    }

    let operation_row = sqlx::query(
        r#"SELECT "LibraryId", "Kind", "State"
           FROM "ManagedLibraryOperation" WHERE "Id" = ?"#,
    )
    .bind(command.operation_id)
    .fetch_optional(&mut **tx)
    .await
    .repository()?
    .ok_or(RepositoryError::NotFound {
        kind: "managed library operation",
        id: command.operation_id,
    })?;
    let operation_library_id: EntityId = operation_row.try_get("LibraryId").repository()?;
    if bound_library_id != operation_library_id {
        return Err(RepositoryError::Conflict(
            "a lifecycle operation must belong to the media list's managed library".into(),
        ));
    }
    let operation_kind = decode_enum_column::<ersatzrs_domain::ManagedLibraryOperationKind>(
        "ManagedLibraryOperationKind",
        &operation_row,
        "Kind",
    )?;
    let expected_operation_kind = match command.kind {
        ersatzrs_domain::AddonMediaListLifecycleJobKind::ResetReload => {
            ersatzrs_domain::ManagedLibraryOperationKind::Reset
        }
        ersatzrs_domain::AddonMediaListLifecycleJobKind::Reconcile => {
            ersatzrs_domain::ManagedLibraryOperationKind::Reconcile
        }
    };
    if operation_kind != expected_operation_kind {
        return Err(RepositoryError::Conflict(
            "the managed operation kind does not match the lifecycle job".into(),
        ));
    }
    let operation_state = decode_enum_column::<ersatzrs_domain::ManagedLibraryOperationState>(
        "ManagedLibraryOperationState",
        &operation_row,
        "State",
    )?;
    if operation_state != ersatzrs_domain::ManagedLibraryOperationState::Queued {
        return Err(RepositoryError::Conflict(
            "a lifecycle job requires a queued managed operation".into(),
        ));
    }
    Ok(())
}

fn decode_import_checkpoint_row(row: &crate::db::DbRow) -> Result<AddonMediaListImportCheckpoint> {
    Ok(AddonMediaListImportCheckpoint {
        id: row.try_get("Id").repository()?,
        list_id: row.try_get("AddonMediaListId").repository()?,
        relative_path: row.try_get("RelativePath").repository()?,
        generation: row.try_get("Generation").repository()?,
        sha256: row.try_get("Sha256").repository()?,
        entry_count: decode_u64(
            "add-on media-list checkpoint entry count",
            row.try_get("EntryCount").repository()?,
        )?,
        promoted_at: decode_required_datetime(
            "AddonMediaListImportCheckpoint.PromotedAt",
            row.try_get::<String, _>("PromotedAt").repository()?,
        )?,
    })
}

fn decode_import_lifecycle_row(row: &crate::db::DbRow) -> Result<AddonMediaListImportLifecycle> {
    Ok(AddonMediaListImportLifecycle {
        job_id: row.try_get("Id").repository()?,
        purpose: decode_enum_column("AddonMediaListImportPurpose", row, "Purpose")?,
        mode: decode_enum_column("AddonMediaListImportMode", row, "Mode")?,
        strategy: decode_enum_column(
            "AddonMediaListIncrementalStrategy",
            row,
            "IncrementalStrategy",
        )?,
        examined_count: decode_u64(
            "add-on media-list import examined count",
            row.try_get("ExaminedCount").repository()?,
        )?,
        archived_skipped_count: decode_u64(
            "add-on media-list import archived-skipped count",
            row.try_get("ArchivedSkippedCount").repository()?,
        )?,
        emitted_count: decode_u64(
            "add-on media-list import emitted count",
            row.try_get("EmittedCount").repository()?,
        )?,
        checkpoint_id: row.try_get("CheckpointId").repository()?,
    })
}

fn decode_lifecycle_job_row(row: &crate::db::DbRow) -> Result<AddonMediaListLifecycleJob> {
    Ok(AddonMediaListLifecycleJob {
        id: row.try_get("Id").repository()?,
        operation_id: row.try_get("OperationId").repository()?,
        list_id: row.try_get("AddonMediaListId").repository()?,
        generation: row.try_get("Generation").repository()?,
        kind: decode_enum_column("AddonMediaListLifecycleJobKind", row, "Kind")?,
        phase: decode_enum_column("AddonMediaListLifecyclePhase", row, "Phase")?,
        saved_draft_revision: row.try_get("SavedDraftRevision").repository()?,
        saved_published_revision: row.try_get("SavedPublishedRevision").repository()?,
        candidate_import_job_id: row.try_get("CandidateImportJobId").repository()?,
        created_at: decode_required_datetime(
            "AddonMediaListLifecycleJob.CreatedAt",
            row.try_get::<String, _>("CreatedAt").repository()?,
        )?,
        updated_at: decode_required_datetime(
            "AddonMediaListLifecycleJob.UpdatedAt",
            row.try_get::<String, _>("UpdatedAt").repository()?,
        )?,
        completed_at: decode_optional_datetime(
            "AddonMediaListLifecycleJob.CompletedAt",
            row.try_get("CompletedAt").repository()?,
        )?,
    })
}

fn decode_tombstone_row(row: &crate::db::DbRow) -> Result<AddonMediaListItemTombstone> {
    let observation_count = decode_u64(
        "add-on media-list tombstone observation count",
        row.try_get("ObservationCount").repository()?,
    )?;
    Ok(AddonMediaListItemTombstone {
        item_id: row.try_get("AddonMediaListItemId").repository()?,
        missing_since: decode_required_datetime(
            "AddonMediaListItemTombstone.MissingSince",
            row.try_get::<String, _>("MissingSince").repository()?,
        )?,
        last_observed_at: decode_required_datetime(
            "AddonMediaListItemTombstone.LastObservedAt",
            row.try_get::<String, _>("LastObservedAt").repository()?,
        )?,
        observation_count: u32::try_from(observation_count).map_err(|_| {
            RepositoryError::Decode {
                kind: "add-on media-list tombstone observation count",
                value: observation_count.to_string(),
            }
        })?,
        last_reconcile_generation: row.try_get("LastReconcileGeneration").repository()?,
        confirmed_at: decode_optional_datetime(
            "AddonMediaListItemTombstone.ConfirmedAt",
            row.try_get("ConfirmedAt").repository()?,
        )?,
        reappeared_at: decode_optional_datetime(
            "AddonMediaListItemTombstone.ReappearedAt",
            row.try_get("ReappearedAt").repository()?,
        )?,
    })
}

async fn load_item_tombstone_in_tx(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    item_id: EntityId,
) -> Result<Option<AddonMediaListItemTombstone>> {
    sqlx::query(
        r#"SELECT * FROM "AddonMediaListItemTombstone"
           WHERE "AddonMediaListItemId" = ?"#,
    )
    .bind(item_id)
    .fetch_optional(&mut **tx)
    .await
    .repository()?
    .as_ref()
    .map(decode_tombstone_row)
    .transpose()
}

async fn insert_item_tombstone_in_tx(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    tombstone: &AddonMediaListItemTombstone,
) -> Result<()> {
    sqlx::query(
        r#"INSERT INTO "AddonMediaListItemTombstone" (
               "AddonMediaListItemId", "MissingSince", "LastObservedAt",
               "ObservationCount", "LastReconcileGeneration", "ConfirmedAt", "ReappearedAt"
           ) VALUES (?, ?, ?, ?, ?, NULL, ?)"#,
    )
    .bind(tombstone.item_id)
    .bind(encode_datetime(tombstone.missing_since))
    .bind(encode_datetime(tombstone.last_observed_at))
    .bind(i64::from(tombstone.observation_count))
    .bind(tombstone.last_reconcile_generation)
    .bind(tombstone.reappeared_at.map(encode_datetime))
    .execute(&mut **tx)
    .await
    .repository()?;
    Ok(())
}

async fn update_item_tombstone_in_tx(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    tombstone: &AddonMediaListItemTombstone,
    expected_generation: i32,
) -> Result<()> {
    let updated = sqlx::query(
        r#"UPDATE "AddonMediaListItemTombstone"
           SET "LastObservedAt" = ?, "ObservationCount" = ?,
               "LastReconcileGeneration" = ?, "ReappearedAt" = ?
           WHERE "AddonMediaListItemId" = ? AND "LastReconcileGeneration" = ?"#,
    )
    .bind(encode_datetime(tombstone.last_observed_at))
    .bind(i64::from(tombstone.observation_count))
    .bind(tombstone.last_reconcile_generation)
    .bind(tombstone.reappeared_at.map(encode_datetime))
    .bind(tombstone.item_id)
    .bind(expected_generation)
    .execute(&mut **tx)
    .await
    .repository()?;
    if updated.rows_affected() == 1 {
        Ok(())
    } else {
        Err(RepositoryError::Conflict(
            "the add-on media-list item tombstone changed concurrently".into(),
        ))
    }
}

fn decode_artifact_row(row: &crate::db::DbRow) -> Result<AddonMediaListArtifact> {
    Ok(AddonMediaListArtifact {
        id: row.try_get("Id").repository()?,
        list_id: row.try_get("AddonMediaListId").repository()?,
        item_id: row.try_get("AddonMediaListItemId").repository()?,
        cache_key: row.try_get("CacheKey").repository()?,
        kind: decode_enum_column("AddonMediaListArtifactKind", row, "Kind")?,
        state: decode_enum_column("AddonMediaListArtifactState", row, "State")?,
        relative_path: row.try_get("RelativePath").repository()?,
        reserved_bytes: decode_u64(
            "add-on media-list artifact reserved bytes",
            row.try_get("ReservedBytes").repository()?,
        )?,
        byte_length: row
            .try_get::<Option<i64>, _>("ByteLength")
            .repository()?
            .map(|value| decode_u64("add-on media-list artifact byte length", value))
            .transpose()?,
        sha256: row.try_get("Sha256").repository()?,
        diagnostic_code: row.try_get("DiagnosticCode").repository()?,
        created_at: decode_required_datetime(
            "AddonMediaListArtifact.CreatedAt",
            row.try_get::<String, _>("CreatedAt").repository()?,
        )?,
        updated_at: decode_required_datetime(
            "AddonMediaListArtifact.UpdatedAt",
            row.try_get::<String, _>("UpdatedAt").repository()?,
        )?,
        last_used_at: decode_optional_datetime(
            "AddonMediaListArtifact.LastUsedAt",
            row.try_get("LastUsedAt").repository()?,
        )?,
        completed_at: decode_optional_datetime(
            "AddonMediaListArtifact.CompletedAt",
            row.try_get("CompletedAt").repository()?,
        )?,
    })
}

fn decode_tool_attempt_row(row: &crate::db::DbRow) -> Result<AddonToolMaintenanceAttempt> {
    Ok(AddonToolMaintenanceAttempt {
        id: row.try_get("Id").repository()?,
        addon_id: row.try_get("AddonId").repository()?,
        tool_key: row.try_get("ToolKey").repository()?,
        trigger: decode_enum_column("AddonToolMaintenanceTrigger", row, "Trigger")?,
        state: decode_enum_column("AddonToolMaintenanceState", row, "State")?,
        attempt_count: row.try_get("AttemptCount").repository()?,
        next_eligible_at: decode_optional_datetime(
            "AddonToolMaintenanceJob.NextEligibleAt",
            row.try_get("NextEligibleAt").repository()?,
        )?,
        before_version: row.try_get("BeforeVersion").repository()?,
        before_sha256: row.try_get("BeforeSha256").repository()?,
        after_version: row.try_get("AfterVersion").repository()?,
        after_sha256: row.try_get("AfterSha256").repository()?,
        validation: decode_enum_column("AddonToolMaintenanceValidation", row, "Validation")?,
        rollback: decode_enum_column("AddonToolMaintenanceRollback", row, "Rollback")?,
        outcome: decode_optional_enum_column("AddonToolMaintenanceOutcome", row, "Outcome")?,
        diagnostic_code: row.try_get("DiagnosticCode").repository()?,
        diagnostic: row.try_get("Diagnostic").repository()?,
        created_at: decode_required_datetime(
            "AddonToolMaintenanceJob.CreatedAt",
            row.try_get::<String, _>("CreatedAt").repository()?,
        )?,
        updated_at: decode_required_datetime(
            "AddonToolMaintenanceJob.UpdatedAt",
            row.try_get::<String, _>("UpdatedAt").repository()?,
        )?,
        completed_at: decode_optional_datetime(
            "AddonToolMaintenanceJob.CompletedAt",
            row.try_get("CompletedAt").repository()?,
        )?,
    })
}

fn tombstones_are_identical(
    left: &AddonMediaListItemTombstone,
    right: &AddonMediaListItemTombstone,
) -> bool {
    left.item_id == right.item_id
        && encode_datetime(left.missing_since) == encode_datetime(right.missing_since)
        && encode_datetime(left.last_observed_at) == encode_datetime(right.last_observed_at)
        && left.observation_count == right.observation_count
        && left.last_reconcile_generation == right.last_reconcile_generation
        && left.confirmed_at.map(encode_datetime) == right.confirmed_at.map(encode_datetime)
        && left.reappeared_at.map(encode_datetime) == right.reappeared_at.map(encode_datetime)
}

fn decode_required_datetime(
    kind: &'static str,
    value: impl AsRef<str>,
) -> Result<chrono::DateTime<chrono::Utc>> {
    decode_datetime(kind, value.as_ref())
}

/// Register and bind defaults as part of creating a list or changing its provider.
/// Existing unbound drafts are adopted only by explicit application preparation.
impl SqliteAddonMediaListRepository {
    pub(crate) async fn initialize_option_defaults_in_tx(
        tx: &mut sqlx::Transaction<'_, crate::db::Db>,
        list_id: EntityId,
    ) -> Result<()> {
        // Only the schema declaration is needed; schema-less legacy installations
        // need no binding and retain their existing preparation contract.
        #[derive(serde::Deserialize)]
        struct Declaration {
            #[serde(default)]
            capabilities: Vec<AddonCapability>,
        }

        let mut draft = load_draft_options_in_tx(tx, list_id, None).await?.ok_or(
            RepositoryError::NotFound {
                kind: "add-on media list",
                id: list_id,
            },
        )?;
        if draft.schema.is_some() {
            return Ok(());
        }
        let installed = sqlx::query(
            r#"SELECT "addon"."AddonId", "addon"."ManifestJson"
               FROM "AddonInstallation" AS "addon"
               JOIN "AddonMediaList" AS "list" ON "list"."AddonId" = "addon"."AddonId"
               WHERE "list"."Id" = ? AND "addon"."Enabled" = 1"#,
        )
        .bind(list_id)
        .fetch_optional(&mut **tx)
        .await
        .repository()?;
        let Some(installed) = installed else {
            return Ok(());
        };
        let declaration: Declaration = serde_json::from_str(
            &installed
                .try_get::<String, _>("ManifestJson")
                .repository()?,
        )
        .map_err(|_| RepositoryError::Conflict("invalid installed option declaration".into()))?;
        let Some(definition) = declaration.capabilities.into_iter().find(|capability| {
            capability.id == ersatzrs_addon_contract::CAPABILITY_MEDIA_LIST_OPTIONS_V1
        }) else {
            return Ok(());
        };
        if !draft.list_values.is_empty() || !draft.item_values.is_empty() {
            return Err(RepositoryError::Conflict(
                "unbound draft options cannot be adopted as defaults".into(),
            ));
        }
        let definition_json = serde_json::to_string(&definition).map_err(|_| {
            RepositoryError::Conflict("invalid installed option declaration".into())
        })?;
        let schema = AddonMediaListOptionSchema {
            schema: AddonMediaListOptionSchemaRef {
                addon_id: installed.try_get("AddonId").repository()?,
                capability_id: definition.id,
                schema_digest: sha256_hex(definition_json.as_bytes()),
            },
            definition_json,
            created_at: chrono::Utc::now(),
        };
        match register_option_schema_in_tx(tx, &schema).await {
            // Keep imports usable after an incompatible package schema update.
            // Never replace a retained schema or bind the new draft to old defaults;
            // explicit option saving/preparation reports the existing conflict.
            Err(RepositoryError::Conflict(reason)) if reason == OPTION_SCHEMA_MISMATCH => {
                return Ok(());
            }
            result => result?,
        }
        draft.schema = Some(schema.schema);
        // Creation/provider-change owns the revision increment in this transaction.
        save_draft_options_tx(tx, &draft, false).await
    }
}

async fn register_option_schema_in_tx(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    schema: &AddonMediaListOptionSchema,
) -> Result<()> {
    decode_and_validate_schema(schema)?;
    let installed: bool = sqlx::query_scalar(
        r#"SELECT EXISTS (SELECT 1 FROM "AddonInstallation" WHERE "AddonId" = ?)"#,
    )
    .bind(&schema.schema.addon_id)
    .fetch_one(&mut **tx)
    .await
    .repository()?;
    if !installed {
        return Err(RepositoryError::Conflict(
            "option schemas can only be registered for an installed add-on".into(),
        ));
    }
    let existing = sqlx::query(
        r#"SELECT "SchemaDigest", "DefinitionJson" FROM "AddonMediaListOptionSchema"
           WHERE "AddonId" = ? AND "CapabilityId" = ?"#,
    )
    .bind(&schema.schema.addon_id)
    .bind(&schema.schema.capability_id)
    .fetch_optional(&mut **tx)
    .await
    .repository()?;
    if let Some(existing) = existing {
        let digest: String = existing.try_get("SchemaDigest").repository()?;
        let definition: String = existing.try_get("DefinitionJson").repository()?;
        if digest == schema.schema.schema_digest && definition == schema.definition_json {
            return Ok(());
        }
        return Err(RepositoryError::Conflict(OPTION_SCHEMA_MISMATCH.into()));
    }
    sqlx::query(
        r#"INSERT INTO "AddonMediaListOptionSchema" (
           "AddonId", "CapabilityId", "SchemaDigest", "DefinitionJson", "CreatedAt"
           ) VALUES (?, ?, ?, ?, ?)"#,
    )
    .bind(&schema.schema.addon_id)
    .bind(&schema.schema.capability_id)
    .bind(&schema.schema.schema_digest)
    .bind(&schema.definition_json)
    .bind(encode_datetime(schema.created_at))
    .execute(&mut **tx)
    .await
    .repository()?;
    Ok(())
}

pub(super) async fn load_draft_options_in_tx(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    list_id: EntityId,
    expected_schema: Option<&AddonMediaListOptionSchemaRef>,
) -> Result<Option<AddonMediaListDraftOptions>> {
    let row = sqlx::query(
        r#"SELECT "list"."DraftRevision", "list"."DraftOptionValuesJson",
                  "schema"."AddonId", "schema"."CapabilityId", "schema"."SchemaDigest"
           FROM "AddonMediaList" AS "list"
           LEFT JOIN "AddonMediaListOptionSchema" AS "schema"
             ON "schema"."Id" = "list"."DraftOptionSchemaId"
           WHERE "list"."Id" = ?"#,
    )
    .bind(list_id)
    .fetch_optional(&mut **tx)
    .await
    .repository()?;
    let Some(row) = row else {
        return Ok(None);
    };
    let schema = decode_schema_ref(&row)?;
    ensure_expected_schema(schema.as_ref(), expected_schema)?;
    let item_values = load_draft_item_values_in_tx(tx, list_id).await?;
    Ok(Some(AddonMediaListDraftOptions {
        list_id,
        draft_revision: row.try_get("DraftRevision").repository()?,
        schema,
        list_values: decode_option_values(
            "add-on media-list draft option values",
            row.try_get::<String, _>("DraftOptionValuesJson")
                .repository()?,
        )?,
        item_values,
    }))
}

async fn load_draft_item_values_in_tx(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    list_id: EntityId,
) -> Result<Vec<AddonMediaListItemOptionValues>> {
    let rows = sqlx::query(
        r#"SELECT "Id", "ProviderId", "DraftOptionOverridesJson"
           FROM "AddonMediaListItem"
           WHERE "AddonMediaListId" = ? AND "DraftOptionOverridesJson" <> '{}'
           ORDER BY "Rank", "Id""#,
    )
    .bind(list_id)
    .fetch_all(&mut **tx)
    .await
    .repository()?;
    rows.into_iter()
        .map(|row| {
            Ok(AddonMediaListItemOptionValues {
                item_id: Some(row.try_get("Id").repository()?),
                provider_id: row.try_get("ProviderId").repository()?,
                values: decode_option_values(
                    "add-on media-list draft item option values",
                    row.try_get::<String, _>("DraftOptionOverridesJson")
                        .repository()?,
                )?,
            })
        })
        .collect()
}

async fn load_published_options_in_tx(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    list_id: EntityId,
    revision: i32,
    expected_schema: Option<&AddonMediaListOptionSchemaRef>,
) -> Result<Option<AddonMediaListPublishedOptions>> {
    let row = sqlx::query(
        r#"SELECT "revision"."Id", "revision"."SourceDraftRevision",
                  "revision"."OptionValuesJson", "revision"."CompletedAt",
                  "schema"."AddonId", "schema"."CapabilityId", "schema"."SchemaDigest"
           FROM "AddonMediaListRevision" AS "revision"
           LEFT JOIN "AddonMediaListOptionSchema" AS "schema"
             ON "schema"."Id" = "revision"."OptionSchemaId"
           WHERE "revision"."AddonMediaListId" = ? AND "revision"."Revision" = ?"#,
    )
    .bind(list_id)
    .bind(revision)
    .fetch_optional(&mut **tx)
    .await
    .repository()?;
    let Some(row) = row else {
        return Ok(None);
    };
    let schema = decode_schema_ref(&row)?;
    ensure_expected_schema(schema.as_ref(), expected_schema)?;
    let revision_id: EntityId = row.try_get("Id").repository()?;
    let item_rows = sqlx::query(
        r#"SELECT "ProviderId", "OptionOverridesJson"
           FROM "AddonMediaListRevisionItemOption"
           WHERE "AddonMediaListRevisionId" = ? ORDER BY "ProviderId""#,
    )
    .bind(revision_id)
    .fetch_all(&mut **tx)
    .await
    .repository()?;
    let item_values = item_rows
        .into_iter()
        .map(|item| {
            Ok(AddonMediaListItemOptionValues {
                item_id: None,
                provider_id: item.try_get("ProviderId").repository()?,
                values: decode_option_values(
                    "add-on media-list published item option values",
                    item.try_get::<String, _>("OptionOverridesJson")
                        .repository()?,
                )?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Some(AddonMediaListPublishedOptions {
        list_id,
        revision,
        source_draft_revision: row.try_get("SourceDraftRevision").repository()?,
        schema,
        list_values: decode_option_values(
            "add-on media-list published option values",
            row.try_get::<String, _>("OptionValuesJson").repository()?,
        )?,
        item_values,
        completed_at: decode_optional_datetime(
            "AddonMediaListRevision.CompletedAt",
            row.try_get("CompletedAt").repository()?,
        )?,
    }))
}

async fn resolve_schema(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    list_id: EntityId,
    schema: Option<&AddonMediaListOptionSchemaRef>,
) -> Result<Option<(EntityId, AddonCapability)>> {
    let addon_id =
        sqlx::query_scalar::<_, String>(r#"SELECT "AddonId" FROM "AddonMediaList" WHERE "Id" = ?"#)
            .bind(list_id)
            .fetch_optional(&mut **tx)
            .await
            .repository()?
            .ok_or(RepositoryError::NotFound {
                kind: "add-on media list",
                id: list_id,
            })?;
    let Some(schema) = schema else {
        return Ok(None);
    };
    if schema.addon_id != addon_id {
        return Err(RepositoryError::Conflict(OPTION_SCHEMA_MISMATCH.into()));
    }
    let row = sqlx::query(
        r#"SELECT "Id", "DefinitionJson" FROM "AddonMediaListOptionSchema"
           WHERE "AddonId" = ? AND "CapabilityId" = ? AND "SchemaDigest" = ?"#,
    )
    .bind(&schema.addon_id)
    .bind(&schema.capability_id)
    .bind(&schema.schema_digest)
    .fetch_optional(&mut **tx)
    .await
    .repository()?;
    let row = row.ok_or_else(|| RepositoryError::Conflict(OPTION_SCHEMA_MISMATCH.into()))?;
    let id = row.try_get("Id").repository()?;
    let definition_json: String = row.try_get("DefinitionJson").repository()?;
    let capability = decode_capability(&definition_json, &schema.capability_id)?;
    Ok(Some((id, capability)))
}

fn decode_and_validate_schema(schema: &AddonMediaListOptionSchema) -> Result<AddonCapability> {
    if schema.schema.addon_id.trim().is_empty()
        || schema.schema.capability_id.trim().is_empty()
        || schema.schema.schema_digest.trim().is_empty()
    {
        return Err(RepositoryError::Conflict(
            "option schema identity must not be empty".into(),
        ));
    }
    if schema.definition_json.len() > MAX_OPTION_SCHEMA_DEFINITION_BYTES {
        return Err(RepositoryError::Conflict(
            "option schema definition exceeds the storage limit".into(),
        ));
    }
    if sha256_hex(schema.definition_json.as_bytes()) != schema.schema.schema_digest {
        return Err(RepositoryError::Conflict(
            "option schema digest does not match its definition".into(),
        ));
    }
    decode_capability(&schema.definition_json, &schema.schema.capability_id)
}

fn decode_capability(definition_json: &str, capability_id: &str) -> Result<AddonCapability> {
    if definition_json.len() > MAX_OPTION_SCHEMA_DEFINITION_BYTES {
        return Err(RepositoryError::Conflict(
            "option schema definition exceeds the storage limit".into(),
        ));
    }
    let capability: AddonCapability =
        serde_json::from_str(definition_json).map_err(|error| RepositoryError::Decode {
            kind: "add-on media-list option schema",
            value: error.to_string(),
        })?;
    if capability.id != capability_id {
        return Err(RepositoryError::Conflict(OPTION_SCHEMA_MISMATCH.into()));
    }
    for scope in [
        AddonMediaListOptionValueScope::ListDefault,
        AddonMediaListOptionValueScope::ItemOverride,
    ] {
        capability
            .validate_stored_option_values(
                &ContractOptionValues {
                    schema: capability_id.to_owned(),
                    values: BTreeMap::new(),
                },
                scope,
            )
            .map_err(|error| {
                RepositoryError::Conflict(format!("option schema is invalid: {error}"))
            })?;
    }
    Ok(capability)
}

fn validate_stored_values(
    schema: Option<&(EntityId, AddonCapability)>,
    values: &AddonMediaListOptionValues,
    list_default: bool,
) -> Result<()> {
    let Some((_, capability)) = schema else {
        return if values.is_empty() {
            Ok(())
        } else {
            Err(RepositoryError::Conflict(OPTION_SCHEMA_MISMATCH.into()))
        };
    };
    let values = ContractOptionValues {
        schema: capability.id.clone(),
        values: values
            .iter()
            .map(|(key, value)| {
                let value = match value {
                    ersatzrs_domain::AddonMediaListOptionValue::Boolean(value) => {
                        ContractOptionValue::Boolean(*value)
                    }
                    ersatzrs_domain::AddonMediaListOptionValue::Enum(value) => {
                        ContractOptionValue::Enum(value.clone())
                    }
                };
                (key.clone(), value)
            })
            .collect(),
    };
    capability
        .validate_stored_option_values(
            &values,
            if list_default {
                AddonMediaListOptionValueScope::ListDefault
            } else {
                AddonMediaListOptionValueScope::ItemOverride
            },
        )
        .map_err(|error| RepositoryError::Validation {
            field: "options",
            message: format!("option values do not match the registered schema: {error}"),
        })
}

fn decode_schema_ref(row: &crate::db::DbRow) -> Result<Option<AddonMediaListOptionSchemaRef>> {
    let addon_id: Option<String> = row.try_get("AddonId").repository()?;
    let capability_id: Option<String> = row.try_get("CapabilityId").repository()?;
    let schema_digest: Option<String> = row.try_get("SchemaDigest").repository()?;
    match (addon_id, capability_id, schema_digest) {
        (None, None, None) => Ok(None),
        (Some(addon_id), Some(capability_id), Some(schema_digest)) => {
            Ok(Some(AddonMediaListOptionSchemaRef {
                addon_id,
                capability_id,
                schema_digest,
            }))
        }
        _ => Err(RepositoryError::Decode {
            kind: "add-on media-list option schema reference",
            value: "partial schema identity".into(),
        }),
    }
}

fn ensure_expected_schema(
    actual: Option<&AddonMediaListOptionSchemaRef>,
    expected: Option<&AddonMediaListOptionSchemaRef>,
) -> Result<()> {
    if expected.is_none() || actual == expected {
        Ok(())
    } else {
        Err(RepositoryError::Conflict(OPTION_SCHEMA_MISMATCH.into()))
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

fn encode_option_values(values: &AddonMediaListOptionValues) -> Result<String> {
    serde_json::to_string(values).map_err(|error| RepositoryError::Decode {
        kind: "add-on media-list option values",
        value: error.to_string(),
    })
}

fn decode_option_values(
    kind: &'static str,
    json: impl AsRef<str>,
) -> Result<AddonMediaListOptionValues> {
    serde_json::from_str(json.as_ref()).map_err(|error| RepositoryError::Decode {
        kind,
        value: error.to_string(),
    })
}

fn encode_string_vec(values: &[String]) -> Result<String> {
    serde_json::to_string(values).map_err(|error| RepositoryError::Decode {
        kind: "add-on media-list revision string list",
        value: error.to_string(),
    })
}

fn decode_string_vec(kind: &'static str, json: &str) -> Result<Vec<String>> {
    serde_json::from_str(json).map_err(|error| RepositoryError::Decode {
        kind,
        value: error.to_string(),
    })
}

fn decode_config_u64(values: &BTreeMap<String, String>, key: &str) -> Result<Option<u64>> {
    values
        .get(key)
        .map(|value| {
            value.parse().map_err(|_| RepositoryError::Decode {
                kind: "processed artifact policy",
                value: value.clone(),
            })
        })
        .transpose()
}

fn encode_i64(kind: &'static str, value: u64) -> Result<i64> {
    i64::try_from(value).map_err(|_| RepositoryError::Decode {
        kind,
        value: value.to_string(),
    })
}

fn decode_u64(kind: &'static str, value: i64) -> Result<u64> {
    u64::try_from(value).map_err(|_| RepositoryError::Decode {
        kind,
        value: value.to_string(),
    })
}

/// Share option validation and writes with a coherent editor transaction.
pub(super) async fn save_draft_options_tx(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    options: &AddonMediaListDraftOptions,
    advance_revision: bool,
) -> Result<()> {
    let list_json = encode_option_values(&options.list_values)?;
    crate::addon_media_list_sync::ensure_not_publishing_tx(tx, options.list_id).await?;
    let schema_binding = resolve_schema(tx, options.list_id, options.schema.as_ref()).await?;
    validate_stored_values(schema_binding.as_ref(), &options.list_values, true)?;
    for item in &options.item_values {
        validate_stored_values(schema_binding.as_ref(), &item.values, false)?;
    }
    let schema_id = schema_binding.as_ref().map(|(id, _)| *id);

    let mut item_ids = BTreeSet::new();
    for item in &options.item_values {
        let Some(item_id) = item.item_id else {
            return Err(RepositoryError::Conflict(
                "draft item option values require a current item id".into(),
            ));
        };
        if !item_ids.insert(item_id) {
            return Err(RepositoryError::Conflict(
                "draft item option values contain a duplicate item identity".into(),
            ));
        }
        let provider_id = sqlx::query_scalar::<_, String>(
            r#"SELECT "ProviderId" FROM "AddonMediaListItem"
                   WHERE "Id" = ? AND "AddonMediaListId" = ?"#,
        )
        .bind(item_id)
        .bind(options.list_id)
        .fetch_optional(&mut **tx)
        .await
        .repository()?
        .ok_or(RepositoryError::NotFound {
            kind: "add-on media list item",
            id: item_id,
        })?;
        if provider_id != item.provider_id {
            return Err(RepositoryError::Conflict(
                "draft item option identity does not match the saved provider identity".into(),
            ));
        }
    }

    let updated = sqlx::query(
        r#"UPDATE "AddonMediaList"
               SET "DraftOptionSchemaId" = ?, "DraftOptionValuesJson" = ?,
                   "DraftRevision" = "DraftRevision" + ?
               WHERE "Id" = ? AND "DraftRevision" = ?"#,
    )
    .bind(schema_id)
    .bind(list_json)
    .bind(i32::from(advance_revision))
    .bind(options.list_id)
    .bind(options.draft_revision)
    .execute(&mut **tx)
    .await
    .repository()?;
    if updated.rows_affected() == 0 {
        return Err(RepositoryError::Conflict(
            "add-on media-list draft revision changed".into(),
        ));
    }

    sqlx::query(
        r#"UPDATE "AddonMediaListItem" SET "DraftOptionOverridesJson" = '{}'
               WHERE "AddonMediaListId" = ?"#,
    )
    .bind(options.list_id)
    .execute(&mut **tx)
    .await
    .repository()?;
    for item in options
        .item_values
        .iter()
        .filter(|item| !item.values.is_empty())
    {
        sqlx::query(
            r#"UPDATE "AddonMediaListItem" SET "DraftOptionOverridesJson" = ?
                   WHERE "Id" = ? AND "AddonMediaListId" = ?"#,
        )
        .bind(encode_option_values(&item.values)?)
        .bind(item.item_id)
        .bind(options.list_id)
        .execute(&mut **tx)
        .await
        .repository()?;
    }
    Ok(())
}

#[cfg(test)]
mod default_binding_tests {
    use super::*;

    async fn fixture(mode: i32, declares_schema: bool) -> crate::db::DbPool {
        let pool = crate::db::init("sqlite::memory:").await.unwrap();
        let capabilities = if declares_schema {
            serde_json::json!([{
                "id": "media-list.options.v1",
                "options": [{
                    "key": "enabled", "kind": "boolean", "scope": "both",
                    "label": {"en-US": "Enabled"},
                    "description": {"en-US": "Prepare finite media"},
                    "default": true, "operation_targets": ["preparation"],
                    "applicability": "finite_only", "requires_preparation": true
                }]
            }])
        } else {
            serde_json::json!([])
        };
        sqlx::query(
            r#"INSERT INTO "AddonInstallation" (
            "AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256",
            "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt"
            ) VALUES ('org.example.options','1',1,'package','content',?,'fixture',
            '2026-09-01 00:00:00','2026-09-01 00:00:00')"#,
        )
        .bind(serde_json::json!({"capabilities": capabilities}).to_string())
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"INSERT INTO "AddonMediaList" (
            "Id","AddonId","ProviderId","SourceUrl","Name","DeliveryMode","DraftRevision",
            "DeliveryCutoffAt","DeliveryCutoffIdentityId")
            VALUES (1,'org.example.options','list','https://example.test/list','Fixture',?,4,
                CASE WHEN ?=2 THEN '2026-09-01 00:00:00' END, CASE WHEN ?=2 THEN 0 END)"#,
        )
        .bind(mode)
        .bind(mode)
        .bind(mode)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"INSERT INTO "AddonMediaListItem" (
            "Id","AddonMediaListId","ProviderId","DisplayTitle","Kind","Rank","Title",
            "SourceUrl","ProviderLiveness") VALUES
            (10,1,'item','Fixture',4,0,'Fixture','https://example.test/item',1)"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    #[tokio::test]
    async fn playback_baseline_option_save_is_atomic_and_publication_isolated() {
        use ersatzrs_core::repositories::{
            AddonMediaListEditorSave, AddonMediaListRepository as _,
        };
        use ersatzrs_domain::{AddonMediaListDeliveryMode, AddonMediaListOptionValue};
        let pool = fixture(0, true).await;
        let mut tx = crate::db::begin_write(&pool).await.unwrap();
        SqliteAddonMediaListRepository::initialize_option_defaults_in_tx(&mut tx, 1)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        let repository = SqliteAddonMediaListRepository(pool.clone());
        let (revision, baseline) = repository.playback_baseline(1).await.unwrap();
        sqlx::query(r#"INSERT INTO "AddonMediaListRevision" (
            "AddonMediaListId", "Revision", "SourceDraftRevision", "CompletedAt", "OptionSchemaId", "OptionValuesJson")
            SELECT 1, 1, "DraftRevision", '2026-09-01 00:00:00', "DraftOptionSchemaId", '{"enabled":true}'
            FROM "AddonMediaList" WHERE "Id"=1"#).execute(&pool).await.unwrap();
        sqlx::query(
            r#"UPDATE "AddonMediaList" SET "DraftRevision"="DraftRevision"+1 WHERE "Id"=1"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        let mut options = repository.draft_options(1, None).await.unwrap().unwrap();
        options
            .list_values
            .insert("enabled".into(), AddonMediaListOptionValue::Boolean(false));
        let mut save = AddonMediaListEditorSave {
            list_id: 1,
            expected_revision: revision,
            playback_baseline: Some(baseline),
            metadata: None,
            settings: None,
            delivery_mode: Some(AddonMediaListDeliveryMode::Stream),
            disk_warning_gb: Some(42),
            options: Some(options),
            items: Vec::new(),
        };
        assert_eq!(
            repository.save_editor_task(&save).await.unwrap(),
            revision + 2
        );
        let published: String = sqlx::query_scalar(
            r#"SELECT "OptionValuesJson" FROM "AddonMediaListRevision" WHERE "AddonMediaListId"=1"#,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(published, r#"{"enabled":true}"#);
        assert!(repository.save_editor_task(&save).await.is_err());
        let (current, baseline) = repository.playback_baseline(1).await.unwrap();
        save.expected_revision = current;
        save.playback_baseline = Some(baseline.clone());
        save.disk_warning_gb = Some(99);
        let options = save.options.as_mut().unwrap();
        options.draft_revision = current;
        options.list_values.insert(
            "enabled".into(),
            AddonMediaListOptionValue::Enum("invalid".into()),
        );
        assert!(repository.save_editor_task(&save).await.is_err());
        assert_eq!(
            repository.playback_baseline(1).await.unwrap(),
            (current, baseline)
        );
    }

    #[tokio::test]
    async fn creation_defaults_resolve_in_both_delivery_modes_without_editing_publications() {
        for mode in [1, 2] {
            let pool = fixture(mode, true).await;
            sqlx::query(
                r#"INSERT INTO "AddonMediaListRevision" (
                "AddonMediaListId","Revision","SourceDraftRevision","CompletedAt")
                VALUES (1,1,3,'2026-09-01 00:00:00')"#,
            )
            .execute(&pool)
            .await
            .unwrap();
            let mut tx = crate::db::begin_write(&pool).await.unwrap();
            SqliteAddonMediaListRepository::initialize_option_defaults_in_tx(&mut tx, 1)
                .await
                .unwrap();
            tx.commit().await.unwrap();
            let repository = SqliteAddonMediaListRepository(pool.clone());
            let draft = repository.draft_options(1, None).await.unwrap().unwrap();
            assert!(draft.schema.is_some());
            assert_eq!(draft.draft_revision, 4);
            assert!(draft.list_values.is_empty());
            assert!(draft.item_values.is_empty());
            let effective = crate::addon_media_list_options::load_draft_item_options(&pool, 1, 10)
                .await
                .unwrap()
                .unwrap();
            assert!(effective.requires_preparation(RemoteStreamLiveness::Finite));
            assert!(!effective.requires_preparation(RemoteStreamLiveness::Live));
            let retained: Option<i64> = sqlx::query_scalar(
                r#"SELECT "OptionSchemaId" FROM "AddonMediaListRevision" WHERE "AddonMediaListId"=1"#,
            ).fetch_one(&pool).await.unwrap();
            assert!(retained.is_none());
            let mut tx = crate::db::begin_write(&pool).await.unwrap();
            SqliteAddonMediaListRepository::initialize_option_defaults_in_tx(&mut tx, 1)
                .await
                .unwrap();
            tx.commit().await.unwrap();
            assert_eq!(
                repository.draft_options(1, None).await.unwrap().unwrap(),
                draft
            );
        }
    }

    #[tokio::test]
    async fn default_initialization_keeps_schema_less_and_saved_custom_options_unchanged() {
        let pool = fixture(1, false).await;
        let mut tx = crate::db::begin_write(&pool).await.unwrap();
        SqliteAddonMediaListRepository::initialize_option_defaults_in_tx(&mut tx, 1)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        assert!(
            crate::addon_media_list_options::load_draft_item_options(&pool, 1, 10)
                .await
                .unwrap()
                .is_none()
        );

        let pool = fixture(1, true).await;
        let mut tx = crate::db::begin_write(&pool).await.unwrap();
        SqliteAddonMediaListRepository::initialize_option_defaults_in_tx(&mut tx, 1)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        let repository = SqliteAddonMediaListRepository(pool.clone());
        let mut draft = repository.draft_options(1, None).await.unwrap().unwrap();
        draft.list_values.insert(
            "enabled".into(),
            ersatzrs_domain::AddonMediaListOptionValue::Boolean(false),
        );
        draft.draft_revision = repository.save_draft_options(&draft).await.unwrap();
        let mut tx = crate::db::begin_write(&pool).await.unwrap();
        SqliteAddonMediaListRepository::initialize_option_defaults_in_tx(&mut tx, 1)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        assert_eq!(
            repository.draft_options(1, None).await.unwrap().unwrap(),
            draft
        );
    }

    #[tokio::test]
    async fn default_binding_schema_change_preserves_import_and_retained_schema() {
        let pool = fixture(1, true).await;
        let mut tx = crate::db::begin_write(&pool).await.unwrap();
        SqliteAddonMediaListRepository::initialize_option_defaults_in_tx(&mut tx, 1)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        let repository = SqliteAddonMediaListRepository(pool.clone());
        let retained = repository.draft_options(1, None).await.unwrap().unwrap();
        let original: String = sqlx::query_scalar(
            r#"SELECT "ManifestJson" FROM "AddonInstallation" WHERE "AddonId"='org.example.options'"#,
        ).fetch_one(&pool).await.unwrap();
        let mut updated: serde_json::Value = serde_json::from_str(&original).unwrap();
        updated["capabilities"][0]["options"][0]["default"] = false.into();
        sqlx::query(r#"UPDATE "AddonInstallation" SET "ManifestJson"=?"#)
            .bind(updated.to_string())
            .execute(&pool)
            .await
            .unwrap();
        let mut tx = crate::db::begin_write(&pool).await.unwrap();
        sqlx::query(
            r#"INSERT INTO "AddonMediaList" (
            "Id","AddonId","ProviderId","SourceUrl","Name") VALUES
            (2,'org.example.options','new-list','https://example.test/new-list','New fixture')"#,
        )
        .execute(&mut *tx)
        .await
        .unwrap();
        SqliteAddonMediaListRepository::initialize_option_defaults_in_tx(&mut tx, 2)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        assert!(repository
            .draft_options(2, None)
            .await
            .unwrap()
            .unwrap()
            .schema
            .is_none());
        assert_eq!(
            repository.draft_options(1, None).await.unwrap().unwrap(),
            retained
        );
        let effective = crate::addon_media_list_options::load_draft_item_options(&pool, 1, 10)
            .await
            .unwrap()
            .unwrap();
        assert!(effective.requires_preparation(RemoteStreamLiveness::Finite));
    }

    #[tokio::test]
    async fn explicit_default_binding_advances_once_and_rejects_stale_reuse() {
        let pool = fixture(1, true).await;
        let mut tx = crate::db::begin_write(&pool).await.unwrap();
        SqliteAddonMediaListRepository::initialize_option_defaults_in_tx(&mut tx, 1)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        let repository = SqliteAddonMediaListRepository(pool.clone());
        let draft = repository.draft_options(1, None).await.unwrap().unwrap();
        sqlx::query(r#"UPDATE "AddonMediaList" SET "DraftOptionSchemaId"=NULL WHERE "Id"=1"#)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(repository.save_draft_options(&draft).await.unwrap(), 5);
        assert!(repository.save_draft_options(&draft).await.is_err());
        let saved = repository.draft_options(1, None).await.unwrap().unwrap();
        assert_eq!(saved.draft_revision, 5);
        assert_eq!(saved.schema, draft.schema);
    }
}
