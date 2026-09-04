use std::collections::BTreeMap;

use async_trait::async_trait;
use ersatzrs_addon_contract::{
    AddonCapability, AddonMediaListOptionValue as ContractOptionValue,
    AddonMediaListOptionValueScope, AddonMediaListOptionValues as ContractOptionValues,
};
use ersatzrs_core::repositories::{
    AddonMediaListArtifact, AddonMediaListDraftOptions, AddonMediaListImportCheckpoint,
    AddonMediaListImportLifecycle, AddonMediaListItemOptionValues, AddonMediaListItemTombstone,
    AddonMediaListLifecycleJob, AddonMediaListLifecycleRepository, AddonMediaListLifecycleSettings,
    AddonMediaListOptionSchema, AddonMediaListOptionSchemaRef, AddonMediaListOptionValues,
    AddonMediaListPublishedOptions, AddonProcessedArtifactPolicy, AddonToolMaintenanceAttempt,
    CreateAddonMediaListLifecycleJob, RepositoryError, Result,
    PROCESSED_ARTIFACT_GLOBAL_BYTE_LIMIT_KEY, PROCESSED_ARTIFACT_MAX_CONCURRENCY_KEY,
    PROCESSED_ARTIFACT_MINIMUM_FREE_BYTES_KEY, PROCESSED_ARTIFACT_RETENTION_SECONDS_KEY,
};
use ersatzrs_domain::{AddonMediaListRefreshCadence, EntityId};
use sha2::{Digest as _, Sha256};
use sqlx::Row;

use super::common::{
    decode_datetime, decode_enum_column, decode_optional_datetime, decode_optional_enum_column,
    encode_datetime, encode_enum, encode_optional_datetime,
};
use super::{SqliteAddonMediaListRepository, SqlxResultExt as _};

const OPTION_SCHEMA_MISMATCH: &str =
    "option schema does not match the saved capability id and digest";
const MAX_OPTION_SCHEMA_DEFINITION_BYTES: usize = 256 * 1024;
const TOOL_MAINTENANCE_HISTORY_LIMIT: i64 = 50;

#[async_trait]
impl AddonMediaListLifecycleRepository for SqliteAddonMediaListRepository {
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
        .bind(encode_enum(settings.publication_policy))
        .bind(encode_optional_datetime(settings.next_refresh_at))
        .bind(encode_optional_datetime(settings.retry_at))
        .bind(encode_optional_datetime(settings.last_success_at))
        .bind(byte_limit)
        // Only Daily is executable by the frozen Boolean scheduler. The
        // calendar scheduler introduced by AMM-L will own the other cadences.
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
        decode_and_validate_schema(schema)?;
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let installed: bool = sqlx::query_scalar(
            r#"SELECT EXISTS (
                   SELECT 1 FROM "AddonInstallation" WHERE "AddonId" = ?
               )"#,
        )
        .bind(&schema.schema.addon_id)
        .fetch_one(&mut *tx)
        .await
        .repository()?;
        if !installed {
            return Err(RepositoryError::Conflict(
                "option schemas can only be registered for an installed add-on".into(),
            ));
        }
        let existing = sqlx::query(
            r#"SELECT "SchemaDigest", "DefinitionJson"
               FROM "AddonMediaListOptionSchema"
               WHERE "AddonId" = ? AND "CapabilityId" = ?"#,
        )
        .bind(&schema.schema.addon_id)
        .bind(&schema.schema.capability_id)
        .fetch_optional(&mut *tx)
        .await
        .repository()?;
        if let Some(existing) = existing {
            let digest: String = existing.try_get("SchemaDigest").repository()?;
            let definition: String = existing.try_get("DefinitionJson").repository()?;
            if digest == schema.schema.schema_digest && definition == schema.definition_json {
                tx.commit().await.repository()?;
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
        .execute(&mut *tx)
        .await
        .repository()?;
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

    async fn save_draft_options(&self, options: &AddonMediaListDraftOptions) -> Result<i32> {
        let list_json = encode_option_values(&options.list_values)?;
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let schema_binding =
            resolve_schema(&mut tx, options.list_id, options.schema.as_ref()).await?;
        validate_stored_values(schema_binding.as_ref(), &options.list_values, true)?;
        for item in &options.item_values {
            validate_stored_values(schema_binding.as_ref(), &item.values, false)?;
        }
        let schema_id = schema_binding.as_ref().map(|(id, _)| *id);

        for item in &options.item_values {
            let Some(item_id) = item.item_id else {
                return Err(RepositoryError::Conflict(
                    "draft item option values require a current item id".into(),
                ));
            };
            let provider_id = sqlx::query_scalar::<_, String>(
                r#"SELECT "ProviderId" FROM "AddonMediaListItem"
                   WHERE "Id" = ? AND "AddonMediaListId" = ?"#,
            )
            .bind(item_id)
            .bind(options.list_id)
            .fetch_optional(&mut *tx)
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
                   "DraftRevision" = "DraftRevision" + 1
               WHERE "Id" = ? AND "DraftRevision" = ?"#,
        )
        .bind(schema_id)
        .bind(list_json)
        .bind(options.list_id)
        .bind(options.draft_revision)
        .execute(&mut *tx)
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
        .execute(&mut *tx)
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
            .execute(&mut *tx)
            .await
            .repository()?;
        }
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

    async fn import_checkpoint(
        &self,
        list_id: EntityId,
    ) -> Result<Option<AddonMediaListImportCheckpoint>> {
        let row = sqlx::query(
            r#"SELECT * FROM "AddonMediaListImportCheckpoint"
               WHERE "AddonMediaListId" = ?"#,
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
               WHERE "AddonMediaListId" = ?"#,
        )
        .bind(list_id)
        .fetch_optional(&mut *tx)
        .await
        .repository()?
        .as_ref()
        .map(decode_import_checkpoint_row)
        .transpose()?;
        if let Some(current) = current {
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
                return Ok(current);
            }
        }
        let updated = sqlx::query(
            r#"INSERT INTO "AddonMediaListImportCheckpoint" (
                   "AddonMediaListId", "RelativePath", "Generation", "Sha256", "EntryCount", "PromotedAt"
               ) VALUES (?, ?, ?, ?, ?, ?)
               ON CONFLICT("AddonMediaListId") DO UPDATE SET
                   "RelativePath" = excluded."RelativePath",
                   "Generation" = excluded."Generation",
                   "Sha256" = excluded."Sha256",
                   "EntryCount" = excluded."EntryCount",
                   "PromotedAt" = excluded."PromotedAt"
               WHERE excluded."Generation" > "AddonMediaListImportCheckpoint"."Generation"
                  OR (excluded."Generation" = "AddonMediaListImportCheckpoint"."Generation"
                      AND excluded."EntryCount" > "AddonMediaListImportCheckpoint"."EntryCount")"#,
        )
        .bind(list_id)
        .bind(relative_path)
        .bind(generation)
        .bind(sha256)
        .bind(entry_count)
        .bind(encode_datetime(promoted_at))
        .execute(&mut *tx)
        .await
        .repository()?;
        if updated.rows_affected() != 1 {
            return Err(RepositoryError::Conflict(
                "import checkpoint cannot regress or replace an established page identity".into(),
            ));
        }
        let row = sqlx::query(
            r#"SELECT * FROM "AddonMediaListImportCheckpoint"
               WHERE "AddonMediaListId" = ?"#,
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
                if tombstone.confirmed_at.is_some() {
                    return Err(RepositoryError::Conflict(
                        "initial tombstone observations cannot be pre-confirmed".into(),
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
                if tombstone.confirmed_at != current.confirmed_at {
                    return Err(RepositoryError::Conflict(
                        "tombstone observation updates cannot change confirmation".into(),
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
                 AND "ConfirmedAt" IS NULL"#,
        )
        .bind(encode_datetime(confirmed_at))
        .bind(item_id)
        .bind(expected_generation)
        .execute(&mut *tx)
        .await
        .repository()?;
        if updated.rows_affected() == 1 {
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
               "ObservationCount", "LastReconcileGeneration", "ConfirmedAt"
           ) VALUES (?, ?, ?, ?, ?, NULL)"#,
    )
    .bind(tombstone.item_id)
    .bind(encode_datetime(tombstone.missing_since))
    .bind(encode_datetime(tombstone.last_observed_at))
    .bind(i64::from(tombstone.observation_count))
    .bind(tombstone.last_reconcile_generation)
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
               "LastReconcileGeneration" = ?
           WHERE "AddonMediaListItemId" = ? AND "LastReconcileGeneration" = ?"#,
    )
    .bind(encode_datetime(tombstone.last_observed_at))
    .bind(i64::from(tombstone.observation_count))
    .bind(tombstone.last_reconcile_generation)
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
}

fn decode_required_datetime(
    kind: &'static str,
    value: impl AsRef<str>,
) -> Result<chrono::DateTime<chrono::Utc>> {
    decode_datetime(kind, value.as_ref())
}

async fn load_draft_options_in_tx(
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
        .map_err(|error| {
            RepositoryError::Conflict(format!(
                "option values do not match the registered schema: {error}"
            ))
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
