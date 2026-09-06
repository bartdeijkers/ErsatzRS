//! Frozen, provider-neutral option resolution for ADR 0085 item operations.
//!
//! Installed manifests never supply executable defaults to an unbound draft or
//! legacy publication. A retained schema owns the meaning of every saved value.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use ersatzrs_addon_contract::{
    AddonCapability, AddonMediaListOptionApplicability, AddonMediaListOptionOperation,
    AddonMediaListOptionValue, AddonMediaListOptionValueScope, AddonMediaListOptionValues,
};
use ersatzrs_core::repositories::{
    AddonMediaListLifecycleRepository, AddonMediaListOptionSchemaRef,
    AddonMediaListOptionValues as StoredValues, RepositoryError, Result,
};
use ersatzrs_domain::{EntityId, RemoteStreamLiveness};
use sha2::{Digest as _, Sha256};
use sqlx::Row;

use crate::db::DbPool;
use crate::repositories::common::decode_enum_column;
use crate::repositories::{SqliteAddonMediaListRepository, SqlxResultExt as _};

/// Typed effective values and the exact immutable declaration that defines them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedAddonItemOptions {
    pub schema: AddonMediaListOptionSchemaRef,
    pub definition: AddonCapability,
    pub values: AddonMediaListOptionValues,
}

impl ResolvedAddonItemOptions {
    /// Send only declared applicable values to an explicitly supported operation.
    ///
    /// # Errors
    /// Returns an error if the filtered values do not satisfy the retained schema.
    pub fn for_operation(
        &self,
        operation: AddonMediaListOptionOperation,
        liveness: RemoteStreamLiveness,
    ) -> Result<AddonMediaListOptionValues> {
        let item_is_finite = liveness == RemoteStreamLiveness::Finite;
        let values = self
            .definition
            .options
            .iter()
            .filter(|definition| {
                definition.operation_targets.contains(&operation)
                    && (definition.applicability != AddonMediaListOptionApplicability::FiniteOnly
                        || item_is_finite)
            })
            .filter_map(|definition| {
                self.values
                    .values
                    .get(&definition.key)
                    .map(|value| (definition.key.clone(), value.clone()))
            })
            .collect();
        let envelope = AddonMediaListOptionValues {
            schema: self.values.schema.clone(),
            values,
        };
        self.definition
            .validate_option_values(&envelope, operation, item_is_finite)
            .map_err(|_| invalid_options())?;
        Ok(envelope)
    }

    /// Whether the finite item must have a processed artifact before publication.
    #[must_use]
    pub fn requires_preparation(&self, liveness: RemoteStreamLiveness) -> bool {
        liveness == RemoteStreamLiveness::Finite
            && self.definition.options.iter().any(|definition| {
                definition.requires_preparation
                    && self.values.values.get(&definition.key)
                        == Some(&AddonMediaListOptionValue::Boolean(true))
            })
    }

    /// Reject a package update that cannot execute the retained schema unchanged.
    ///
    /// # Errors
    /// Returns an error if the installed schema is missing or invalid, or its
    /// add-on identity or digest differs from the saved schema.
    pub fn validate_installed_schema(
        &self,
        manifest: &ersatzrs_addon_contract::AddonManifest,
    ) -> Result<()> {
        let snapshot = manifest
            .media_list_option_schema_snapshot()
            .map_err(|_| invalid_options())?
            .ok_or_else(invalid_options)?;
        if manifest.id != self.schema.addon_id
            || sha256_hex(snapshot.as_bytes()) != self.schema.schema_digest
        {
            return Err(invalid_options());
        }
        Ok(())
    }

    /// Stable work identity; changes invalidate candidate work, not old publications.
    ///
    /// # Errors
    /// Returns an error if the context cannot be serialized for hashing.
    pub fn context_fingerprint(
        &self,
        source_url: &str,
        provider_id: &str,
        package_identity: &str,
        tool_version: &str,
        max_video_height: Option<u32>,
    ) -> Result<String> {
        let bytes = serde_json::to_vec(&(
            &self.schema,
            &self.values,
            source_url,
            provider_id,
            package_identity,
            tool_version,
            max_video_height,
        ))
        .map_err(|_| invalid_options())?;
        Ok(sha256_hex(&bytes))
    }
}

/// Saved-draft option state for one current item; no provisional package defaults.
///
/// # Errors
/// Returns an error for a missing list or item, invalid retained options, or a
/// database failure.
pub async fn load_draft_item_options(
    pool: &DbPool,
    list_id: EntityId,
    item_id: EntityId,
) -> Result<Option<ResolvedAddonItemOptions>> {
    let repository = SqliteAddonMediaListRepository(pool.clone());
    // The caller's durable generation/revision fence must also cover completion.
    let draft =
        repository
            .draft_options(list_id, None)
            .await?
            .ok_or(RepositoryError::NotFound {
                kind: "add-on media list",
                id: list_id,
            })?;
    repository.option_items(list_id, &[item_id]).await?;
    let Some(schema) = draft.schema else {
        return Ok(None);
    };
    let overrides = draft
        .item_values
        .iter()
        .find(|item| item.item_id == Some(item_id))
        .map(|item| &item.values);
    load_effective_options(&repository, schema, &draft.list_values, overrides)
        .await
        .map(Some)
}

/// Options for an explicitly owned completed publication, never the latest draft.
///
/// # Errors
/// Returns an error for a missing or mismatched publication owner, invalid
/// retained options or liveness, or a database failure.
pub async fn load_published_item_options(
    pool: &DbPool,
    list_id: EntityId,
    revision: i32,
    provider_id: &str,
    source_url: &str,
) -> Result<(RemoteStreamLiveness, Option<ResolvedAddonItemOptions>)> {
    // Unique keys (list, revision) and (revision row, provider) make this one row.
    let row = sqlx::query(
        r#"SELECT "item"."SourceUrl", "item"."EffectiveLiveness"
           FROM "AddonMediaListRevision" AS "revision"
           JOIN "AddonMediaListRevisionItem" AS "item"
             ON "item"."AddonMediaListRevisionId" = "revision"."Id"
           WHERE "revision"."AddonMediaListId" = ? AND "revision"."Revision" = ?
             AND "revision"."CompletedAt" IS NOT NULL
             AND "item"."ProviderId" = ? AND "item"."Published" = 1"#,
    )
    .bind(list_id)
    .bind(revision)
    .bind(provider_id)
    .fetch_optional(pool)
    .await
    .repository()?
    .ok_or_else(|| RepositoryError::Conflict("the published option owner is unavailable".into()))?;
    if row
        .try_get::<Option<String>, _>("SourceUrl")
        .repository()?
        .as_deref()
        != Some(source_url)
    {
        return Err(RepositoryError::Conflict(
            "the published source does not match its option owner".into(),
        ));
    }
    let liveness = decode_enum_column("RemoteStreamLiveness", &row, "EffectiveLiveness")?;
    let repository = SqliteAddonMediaListRepository(pool.clone());
    let published = repository
        .published_options(list_id, revision, None)
        .await?
        .ok_or_else(|| {
            RepositoryError::Conflict("the published option snapshot is unavailable".into())
        })?;
    let Some(schema) = published.schema else {
        return Ok((liveness, None));
    };
    let overrides = published
        .item_values
        .iter()
        .find(|item| item.provider_id == provider_id)
        .map(|item| &item.values);
    let options =
        load_effective_options(&repository, schema, &published.list_values, overrides).await?;
    Ok((liveness, Some(options)))
}

async fn load_effective_options(
    repository: &SqliteAddonMediaListRepository,
    schema: AddonMediaListOptionSchemaRef,
    list_values: &StoredValues,
    item_values: Option<&StoredValues>,
) -> Result<ResolvedAddonItemOptions> {
    let stored = repository
        .option_schema(&schema)
        .await?
        .ok_or_else(invalid_options)?;
    let definition: AddonCapability =
        serde_json::from_str(&stored.definition_json).map_err(|_| invalid_options())?;
    resolve_values(schema, definition, list_values, item_values)
}

fn resolve_values(
    schema: AddonMediaListOptionSchemaRef,
    definition: AddonCapability,
    list_values: &StoredValues,
    item_values: Option<&StoredValues>,
) -> Result<ResolvedAddonItemOptions> {
    let list = wire_values(&schema.capability_id, list_values);
    definition
        .validate_stored_option_values(&list, AddonMediaListOptionValueScope::ListDefault)
        .map_err(|_| invalid_options())?;
    let mut values = definition
        .options
        .iter()
        .map(|definition| (definition.key.clone(), definition.default.clone()))
        .collect::<BTreeMap<_, _>>();
    values.extend(list.values);
    if let Some(item_values) = item_values {
        let item = wire_values(&schema.capability_id, item_values);
        definition
            .validate_stored_option_values(&item, AddonMediaListOptionValueScope::ItemOverride)
            .map_err(|_| invalid_options())?;
        values.extend(item.values);
    }
    Ok(ResolvedAddonItemOptions {
        values: AddonMediaListOptionValues {
            schema: schema.capability_id.clone(),
            values,
        },
        schema,
        definition,
    })
}

fn wire_values(schema: &str, stored: &StoredValues) -> AddonMediaListOptionValues {
    AddonMediaListOptionValues {
        schema: schema.into(),
        values: stored
            .iter()
            .map(|(key, value)| {
                (
                    key.clone(),
                    match value {
                        ersatzrs_domain::AddonMediaListOptionValue::Boolean(value) => {
                            AddonMediaListOptionValue::Boolean(*value)
                        }
                        ersatzrs_domain::AddonMediaListOptionValue::Enum(value) => {
                            AddonMediaListOptionValue::Enum(value.clone())
                        }
                    },
                )
            })
            .collect(),
    }
}

fn invalid_options() -> RepositoryError {
    RepositoryError::Conflict("the saved item options do not match their declared schema".into())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut result = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        let _ = write!(result, "{byte:02x}");
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use ersatzrs_domain::AddonMediaListOptionValue as StoredValue;

    fn fixture(overrides: &StoredValues) -> ResolvedAddonItemOptions {
        let definition: AddonCapability = serde_json::from_value(serde_json::json!({
            "id": "media-list.options.v1", "options": [{
                "key": "fixture_transform", "label": {"en-US": "Transform"},
                "description": {"en-US": "Prepare finite media"}, "scope": "both",
                "operation_targets": ["preparation"], "applicability": "finite_only",
                "kind": "boolean", "default": true, "requires_preparation": true
            }, {
                "key": "fixture_access", "label": {"en-US": "Access"},
                "description": {"en-US": "Choose item access"}, "scope": "both",
                "operation_targets": ["enrich", "preparation", "playback", "test_access"],
                "applicability": "all", "kind": "enum", "default": "none",
                "choices": [{"value": "none", "label": {"en-US": "None"}},
                    {"value": "selected", "label": {"en-US": "Selected"}}]
            }]
        }))
        .unwrap();
        let schema = AddonMediaListOptionSchemaRef {
            addon_id: "org.example.options".into(),
            capability_id: definition.id.clone(),
            schema_digest: sha256_hex(&serde_json::to_vec(&definition).unwrap()),
        };
        resolve_values(schema, definition, &StoredValues::new(), Some(overrides)).unwrap()
    }

    #[test]
    fn option_operation_filter_keeps_live_playback_without_finite_preparation() {
        let options = fixture(&StoredValues::from([(
            "fixture_access".into(),
            StoredValue::Enum("selected".into()),
        )]));
        assert!(options.requires_preparation(RemoteStreamLiveness::Finite));
        assert!(!options.requires_preparation(RemoteStreamLiveness::Live));
        assert!(!options.requires_preparation(RemoteStreamLiveness::Unknown));
        let playback = options
            .for_operation(
                AddonMediaListOptionOperation::Playback,
                RemoteStreamLiveness::Live,
            )
            .unwrap();
        assert_eq!(playback.values.len(), 1);
        assert_eq!(
            playback.values["fixture_access"],
            AddonMediaListOptionValue::Enum("selected".into())
        );
        let preparation = options
            .for_operation(
                AddonMediaListOptionOperation::Preparation,
                RemoteStreamLiveness::Finite,
            )
            .unwrap();
        assert_eq!(preparation.values.len(), 2);
        let disabled = fixture(&StoredValues::from([(
            "fixture_transform".into(),
            StoredValue::Boolean(false),
        )]));
        assert!(!disabled.requires_preparation(RemoteStreamLiveness::Finite));
    }

    #[test]
    fn option_work_identity_changes_with_each_execution_input() {
        let options = fixture(&StoredValues::new());
        let fingerprint = |url, provider, package, tool, height| {
            options
                .context_fingerprint(url, provider, package, tool, height)
                .unwrap()
        };
        let baseline = fingerprint(
            "https://example.invalid/a",
            "a",
            "package-1",
            "tool-1",
            Some(1080),
        );
        assert_eq!(baseline.len(), 64);
        assert_eq!(
            baseline,
            fingerprint(
                "https://example.invalid/a",
                "a",
                "package-1",
                "tool-1",
                Some(1080)
            )
        );
        for changed in [
            fingerprint(
                "https://example.invalid/b",
                "a",
                "package-1",
                "tool-1",
                Some(1080),
            ),
            fingerprint(
                "https://example.invalid/a",
                "b",
                "package-1",
                "tool-1",
                Some(1080),
            ),
            fingerprint(
                "https://example.invalid/a",
                "a",
                "package-2",
                "tool-1",
                Some(1080),
            ),
            fingerprint(
                "https://example.invalid/a",
                "a",
                "package-1",
                "tool-2",
                Some(1080),
            ),
            fingerprint(
                "https://example.invalid/a",
                "a",
                "package-1",
                "tool-1",
                Some(720),
            ),
        ] {
            assert_ne!(baseline, changed);
        }
        let changed = fixture(&StoredValues::from([(
            "fixture_access".into(),
            StoredValue::Enum("selected".into()),
        )]));
        assert_ne!(
            baseline,
            changed
                .context_fingerprint(
                    "https://example.invalid/a",
                    "a",
                    "package-1",
                    "tool-1",
                    Some(1080)
                )
                .unwrap()
        );
    }

    #[test]
    fn option_resolution_rejects_undeclared_stored_values_before_filtering() {
        let options = fixture(&StoredValues::new());
        assert!(resolve_values(
            options.schema,
            options.definition,
            &StoredValues::from([("undeclared".into(), StoredValue::Boolean(true)),]),
            None
        )
        .is_err());
    }
}
