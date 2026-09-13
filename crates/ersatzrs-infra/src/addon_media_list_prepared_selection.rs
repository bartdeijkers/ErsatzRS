//! Select completed preparation from the saved draft without invoking provider tools.

use std::collections::{BTreeMap, BTreeSet};

use ersatzrs_core::addon_media_list_playback::AddonMediaListPlaybackTimelinePlan;
use ersatzrs_core::repositories::{RepositoryError, Result};
use ersatzrs_domain::{AddonMediaListItemKind, RemoteStreamLiveness};
use sqlx::Row as _;

use crate::{
    addon_media_list_item_jobs::{
        ItemOperationCompletion, ItemOperationInput, ItemOperationTiming,
    },
    addon_media_list_item_runtime::InstalledItemRuntime,
    addon_media_list_options::load_draft_item_options,
    db::DbPool,
    repositories::{
        common::{decode_datetime, decode_enum_column, encode_enum},
        SqlxResultExt as _,
    },
};

/// Select only admitted jobs for this saved revision; never probe tools under publication ownership.
pub(crate) struct PreparationRequirement {
    pub item_id: i64,
    pub provider_id: String,
    pub artifact_id: Option<i64>,
    pub pending: bool,
}

/// Saved interval evidence required for server-side edited streaming.
pub(crate) struct IntervalMetadataRequirement {
    pub item_id: i64,
    pub provider_id: String,
    pub plan: Option<AddonMediaListPlaybackTimelinePlan>,
    pub pending: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SelectionMode {
    #[allow(dead_code)] // Preserved Download ahead selection for AMM-M-01B.
    PreparedFile,
    StreamingIntervals,
}

struct SelectedRequirement {
    preparation: PreparationRequirement,
    plan: Option<AddonMediaListPlaybackTimelinePlan>,
}

/// Classify required preparation without executing a provider or tool probe.
#[allow(dead_code)] // Preserved and tested Download ahead path; M-01A defaults to streaming.
pub(crate) async fn preparation_requirements(
    pool: &DbPool,
    list_id: i64,
    saved_draft_revision: i32,
    excluded_item_ids: &[i64],
) -> Result<Vec<PreparationRequirement>> {
    Ok(classify_requirements(
        pool,
        list_id,
        saved_draft_revision,
        excluded_item_ids,
        SelectionMode::PreparedFile,
    )
    .await?
    .into_iter()
    .map(|selected| selected.preparation)
    .collect())
}

/// Classify current interval evidence without invoking provider tools or using files.
pub(crate) async fn interval_metadata_requirements(
    pool: &DbPool,
    list_id: i64,
    saved_draft_revision: i32,
    excluded_item_ids: &[i64],
) -> Result<Vec<IntervalMetadataRequirement>> {
    Ok(classify_requirements(
        pool,
        list_id,
        saved_draft_revision,
        excluded_item_ids,
        SelectionMode::StreamingIntervals,
    )
    .await?
    .into_iter()
    .map(|selected| IntervalMetadataRequirement {
        item_id: selected.preparation.item_id,
        provider_id: selected.preparation.provider_id,
        plan: selected.plan,
        pending: selected.preparation.pending,
    })
    .collect())
}

#[allow(clippy::too_many_lines)] // Keep saved inputs and their ready/pending evidence checks together.
async fn classify_requirements(
    pool: &DbPool,
    list_id: i64,
    saved_draft_revision: i32,
    excluded_item_ids: &[i64],
    mode: SelectionMode,
) -> Result<Vec<SelectedRequirement>> {
    require_revision(pool, list_id, saved_draft_revision).await?;
    let requested_languages = if mode == SelectionMode::PreparedFile {
        crate::addon_media_list_subtitle_selection::requested_preparation_languages(pool, list_id)
            .await?
    } else {
        Vec::new()
    };
    // Read-only transaction: take one consistent delivery-policy/identity snapshot.
    let mut delivery_tx = pool.begin().await.repository()?;
    let deliveries =
        crate::addon_media_list_delivery::load_item_deliveries_in_tx(&mut delivery_tx, list_id)
            .await?;
    delivery_tx.commit().await.repository()?;
    let excluded = excluded_item_ids.iter().copied().collect::<BTreeSet<_>>();
    let rows = sqlx::query(r#"SELECT "item"."Id", "item"."ProviderId", "item"."SourceUrl",
        "replacement"."MediaItemId" AS "ReplacementItemId", "replacement"."MediaVersionId" AS "ReplacementVersionId",
        COALESCE("item"."LivenessOverride", "item"."ProviderLiveness") AS "Liveness",
        "item"."DurationSeconds", "item"."FragmentStartSeconds", "item"."FragmentEndSeconds"
        FROM "AddonMediaListItem" AS "item"
        LEFT JOIN "AddonMediaListItemReplacement" AS "replacement" ON "replacement"."AddonMediaListItemId"="item"."Id"
        WHERE "item"."AddonMediaListId"=? AND "item"."Included"=1 AND "item"."Kind"=?
          AND "item"."InDraft"=1
          AND "item"."SavedReplacementJson" IS NULL
        ORDER BY "item"."Id""#)
        .bind(list_id).bind(encode_enum(AddonMediaListItemKind::RemoteStream)).fetch_all(pool).await.repository()?;
    let mut requirements = Vec::new();
    for row in rows {
        let item_id: i64 = row.try_get("Id").repository()?;
        if excluded.contains(&item_id) {
            continue;
        }
        if let Some((replacement_item, replacement_version)) = row
            .try_get::<Option<i64>, _>("ReplacementItemId")
            .repository()?
            .zip(
                row.try_get::<Option<i64>, _>("ReplacementVersionId")
                    .repository()?,
            )
        {
            if crate::repositories::load_replacement_candidate(
                pool,
                replacement_item,
                replacement_version,
            )
            .await?
            .is_some()
            {
                continue;
            }
        }
        let liveness = decode_enum_column("RemoteStreamLiveness", &row, "Liveness")?;
        if liveness != RemoteStreamLiveness::Finite {
            continue;
        }
        let options = load_draft_item_options(pool, list_id, item_id).await?;
        let provider_id: String = row.try_get("ProviderId").repository()?;
        let download = deliveries.get(&provider_id)
            == Some(&ersatzrs_domain::AddonMediaListDeliveryMode::DownloadAhead);
        if match mode {
            SelectionMode::PreparedFile => !download,
            SelectionMode::StreamingIntervals => {
                download
                    || !options
                        .as_ref()
                        .is_some_and(|options| options.requires_preparation(liveness))
            }
        } {
            continue;
        }
        let source_url: Option<String> = row.try_get("SourceUrl").repository()?;
        let mut requirement = PreparationRequirement {
            item_id,
            provider_id,
            artifact_id: None,
            pending: false,
        };
        let mut plan = None;
        let kind = match mode {
            SelectionMode::PreparedFile => 1,
            SelectionMode::StreamingIntervals => 2,
        };
        let jobs = sqlx::query(r#"SELECT "job"."State", "job"."InputJson", "job"."OutcomeJson", "job"."ContextFingerprint",
            "job"."ArtifactId", "job"."LeaseExpiresAt", "job"."CompletedAt", "job"."ExpiresAt", "artifact"."CacheKey", "artifact"."DurationMilliseconds"
            FROM "AddonMediaListItemOperationJob" AS "job"
            LEFT JOIN "AddonMediaListArtifact" AS "artifact" ON "artifact"."Id"="job"."ArtifactId"
              AND "artifact"."AddonMediaListId"=? AND "artifact"."AddonMediaListItemId"=? AND "artifact"."Kind"=1 AND "artifact"."State"=2
            WHERE "job"."Id" IN (
                (SELECT MAX("Id") FROM "AddonMediaListItemOperationJob" WHERE "AddonMediaListId"=? AND "AddonMediaListItemId"=? AND "SavedDraftRevision"=? AND "Kind"=? AND "State"=2),
                (SELECT MAX("Id") FROM "AddonMediaListItemOperationJob" WHERE "AddonMediaListId"=? AND "AddonMediaListItemId"=? AND "SavedDraftRevision"=? AND "Kind"=? AND "State" IN (0,1)))"#)
            .bind(list_id).bind(item_id).bind(list_id).bind(item_id).bind(saved_draft_revision).bind(kind)
            .bind(list_id).bind(item_id).bind(saved_draft_revision).bind(kind).fetch_all(pool).await.repository()?;
        for job in jobs {
            let Some(source_url) = source_url.as_deref() else {
                continue;
            };
            let Ok(input) = serde_json::from_str::<ItemOperationInput>(
                &job.try_get::<String, _>("InputJson").repository()?,
            ) else {
                continue;
            };
            let Ok(runtime) =
                InstalledItemRuntime::load(pool, input.addon_id()?, Some(&input.package_identity))
                    .await
            else {
                continue;
            };
            if options
                .as_ref()
                .is_some_and(|options| runtime.validate_options(options).is_err())
            {
                continue;
            }
            let base_fingerprint = input.base_fingerprint()?;
            let fingerprint = match mode {
                SelectionMode::PreparedFile => base_fingerprint,
                SelectionMode::StreamingIntervals => {
                    let Ok(timing) = saved_timing(&row) else {
                        continue;
                    };
                    if input.requested_timing.as_ref() != Some(&timing) {
                        continue;
                    }
                    let Ok(fingerprint) = timing.bind_context_fingerprint(&base_fingerprint) else {
                        continue;
                    };
                    fingerprint
                }
            };
            if !input.item_is_finite
                || input.source_url != source_url
                || input.provider_id != requirement.provider_id
                || input.resolved_options()? != options
                || (mode == SelectionMode::PreparedFile && input.preparation_v2 != Some(true))
                || (mode == SelectionMode::PreparedFile
                    && input.requested_subtitle_languages != requested_languages)
                || input.context_fingerprint != fingerprint
                || job
                    .try_get::<String, _>("ContextFingerprint")
                    .repository()?
                    != fingerprint
            {
                continue;
            }
            let state: i32 = job.try_get("State").repository()?;
            if state != 2 {
                requirement.pending = state == 0
                    || job
                        .try_get::<Option<String>, _>("LeaseExpiresAt")
                        .repository()?
                        .and_then(|value| decode_datetime("preparation lease", &value).ok())
                        .is_some_and(|expires| expires > chrono::Utc::now());
                continue;
            }
            let artifact_id: Option<i64> = job.try_get("ArtifactId").repository()?;
            let completion: Option<ItemOperationCompletion> = job
                .try_get::<Option<String>, _>("OutcomeJson")
                .repository()?
                .filter(|value| value.len() <= 65_536)
                .and_then(|value| serde_json::from_str(&value).ok());
            if mode == SelectionMode::StreamingIntervals {
                if let Some(ItemOperationCompletion::IntervalPlan { plan: candidate }) = completion
                {
                    if artifact_id.is_none()
                        && job
                            .try_get::<Option<String>, _>("CompletedAt")
                            .repository()?
                            .is_some()
                        && job
                            .try_get::<Option<String>, _>("ExpiresAt")
                            .repository()?
                            .is_none()
                        && candidate.context_fingerprint() == fingerprint
                        && input.requested_timing.as_ref().is_some_and(|timing| {
                            timing
                                .source_window_milliseconds(
                                    candidate.source_duration_milliseconds(),
                                )
                                .is_ok_and(|window| {
                                    window == candidate.source_window_milliseconds()
                                })
                        })
                    {
                        plan = Some(candidate);
                    }
                }
                continue;
            }
            if let Some(artifact_id) = artifact_id {
                if matches!(completion, Some(ItemOperationCompletion::Prepared { artifact_id: completed_id, .. }) if completed_id == artifact_id)
                    && job
                        .try_get::<Option<String>, _>("CacheKey")
                        .repository()?
                        .as_deref()
                        == Some(fingerprint.as_str())
                    && job
                        .try_get::<Option<i64>, _>("DurationMilliseconds")
                        .repository()?
                        .is_some_and(|duration| duration > 0)
                {
                    requirement.artifact_id = Some(artifact_id);
                }
            }
        }
        requirements.push(SelectedRequirement {
            preparation: requirement,
            plan,
        });
    }
    require_revision(pool, list_id, saved_draft_revision).await?;
    if mode == SelectionMode::PreparedFile
        && crate::addon_media_list_subtitle_selection::requested_preparation_languages(
            pool, list_id,
        )
        .await?
            != requested_languages
    {
        return Err(stale());
    }
    Ok(requirements)
}

fn saved_timing(row: &sqlx::sqlite::SqliteRow) -> Result<ItemOperationTiming> {
    let seconds = |column| -> Result<Option<u64>> {
        row.try_get::<Option<i64>, _>(column)
            .repository()?
            .map(|value| u64::try_from(value).map_err(|_| stale()))
            .transpose()
    };
    Ok(ItemOperationTiming {
        item_duration_seconds: seconds("DurationSeconds")?,
        fragment_start_seconds: seconds("FragmentStartSeconds")?,
        fragment_end_seconds: seconds("FragmentEndSeconds")?,
    })
}

/// Select all required interval plans for an exact saved draft, without file fallback.
pub(crate) async fn select_streaming_plans(
    pool: &DbPool,
    list_id: i64,
    saved_draft_revision: i32,
    excluded_item_ids: &[i64],
) -> Result<BTreeMap<String, AddonMediaListPlaybackTimelinePlan>> {
    interval_metadata_requirements(pool, list_id, saved_draft_revision, excluded_item_ids)
        .await?
        .into_iter()
        .map(|requirement| Ok((requirement.provider_id, requirement.plan.ok_or_else(stale)?)))
        .collect()
}

/// Select only admitted jobs for this saved revision; never probe tools under publication ownership.
#[allow(dead_code)] // Preserved and tested Download ahead path for AMM-M-01B.
pub(crate) async fn select_prepared_media(
    pool: &DbPool,
    list_id: i64,
    saved_draft_revision: i32,
    excluded_item_ids: &[i64],
) -> Result<BTreeMap<String, i64>> {
    preparation_requirements(pool, list_id, saved_draft_revision, excluded_item_ids)
        .await?
        .into_iter()
        .map(|requirement| {
            Ok((
                requirement.provider_id,
                requirement.artifact_id.ok_or_else(stale)?,
            ))
        })
        .collect()
}

async fn require_revision(pool: &DbPool, list_id: i64, revision: i32) -> Result<()> {
    let current: Option<i32> =
        sqlx::query_scalar(r#"SELECT "DraftRevision" FROM "AddonMediaList" WHERE "Id"=?"#)
            .bind(list_id)
            .fetch_optional(pool)
            .await
            .repository()?;
    if current != Some(revision) {
        return Err(stale());
    }
    Ok(())
}

fn stale() -> RepositoryError {
    RepositoryError::Conflict(
        "required processed media is missing or no longer matches the saved draft".into(),
    )
}

// Reuse the existing POSIX package fixture; selection never executes its script.
#[cfg(all(test, unix))]
mod tests {
    use std::fmt::Write as _;

    use sha2::{Digest as _, Sha256};

    use super::*;

    async fn fixture() -> (tempfile::TempDir, DbPool, i64) {
        let temporary = tempfile::tempdir().unwrap();
        let pool = crate::addon_media_list::tests::fixture_media_list_addon_with_script(
            &temporary.path().join("package"),
            "#!/bin/sh\nexit 99\n",
            &["example.test"],
        )
        .await;
        let definition: ersatzrs_addon_contract::AddonCapability = serde_json::from_value(serde_json::json!({
            "id":"media-list.options.v1", "options":[{
                "key":"remove", "label":{"en-US":"Remove"}, "description":{"en-US":"Fixture"},
                "scope":"both", "operation_targets":["preparation"], "applicability":"finite_only",
                "kind":"boolean", "default":true, "requires_preparation":true
            }]
        })).unwrap();
        let definition_json = serde_json::to_string(&definition).unwrap();
        let mut digest = String::with_capacity(64);
        for byte in Sha256::digest(definition_json.as_bytes()) {
            write!(digest, "{byte:02x}").unwrap();
        }
        let manifest_json: String =
            sqlx::query_scalar(r#"SELECT "ManifestJson" FROM "AddonInstallation" LIMIT 1"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        let mut manifest: serde_json::Value = serde_json::from_str(&manifest_json).unwrap();
        manifest["manifest_version"] = serde_json::json!(5);
        manifest["capabilities"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::to_value(&definition).unwrap());
        sqlx::query(r#"UPDATE "AddonInstallation" SET "ManifestJson"=?"#)
            .bind(manifest.to_string())
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(r#"INSERT INTO "AddonMediaListOptionSchema" ("Id","AddonId","CapabilityId","SchemaDigest","DefinitionJson","CreatedAt")
            VALUES (1,'org.ersatzrs.addon.fixture','media-list.options.v1',?,?,'2026-09-05T00:00:00Z')"#)
            .bind(digest).bind(definition_json).execute(&pool).await.unwrap();
        sqlx::raw_sql(r#"INSERT INTO "AddonMediaList" ("Id","AddonId","ProviderId","SourceUrl","Name","DraftOptionSchemaId")
            VALUES (1,'org.ersatzrs.addon.fixture','list','https://example.test/list','Fixture',1);
            INSERT INTO "AddonMediaListItem" ("Id","AddonMediaListId","ProviderId","DisplayTitle","Kind","Rank","Title","SourceUrl","ProviderLiveness")
            VALUES (10,1,'item','Fixture',4,0,'Fixture','https://example.test/item',1);"#)
            .execute(&pool).await.unwrap();
        let options = load_draft_item_options(&pool, 1, 10)
            .await
            .unwrap()
            .unwrap();
        let runtime = InstalledItemRuntime::load(&pool, &options.schema.addon_id, None)
            .await
            .unwrap();
        let fingerprint = options
            .context_fingerprint(
                "https://example.test/item",
                "item",
                runtime.package_identity(),
                "1.0",
                Some(720),
            )
            .unwrap();
        let mut input = ItemOperationInput {
            source_url: "https://example.test/item".into(),
            provider_id: "item".into(),
            context_fingerprint: fingerprint.clone(),
            schema: Some(options.schema),
            definition: Some(options.definition),
            options: Some(options.values),
            addon_id: None,
            preparation_v2: Some(true),
            package_identity: runtime.package_identity().into(),
            tool_version: "1.0".into(),
            item_is_finite: true,
            max_video_height: Some(720),
            requested_timing: None,
            requested_subtitle_languages: Vec::new(),
        };
        input.context_fingerprint = input.base_fingerprint().unwrap();
        let fingerprint = input.context_fingerprint.clone();
        sqlx::query(r#"UPDATE "AddonMediaList" SET "DeliveryMode"=1 WHERE "Id"=1"#)
            .execute(&pool)
            .await
            .unwrap();
        let artifact_id = sqlx::query(r#"INSERT INTO "AddonMediaListArtifact" ("AddonMediaListId","AddonMediaListItemId","CacheKey","Kind","State","DurationMilliseconds","CreatedAt","UpdatedAt")
            VALUES (1,10,?,1,2,12500,'2026-09-05T00:00:00Z','2026-09-05T00:00:00Z')"#)
            .bind(&fingerprint).execute(&pool).await.unwrap().last_insert_rowid();
        sqlx::query(r#"INSERT INTO "AddonMediaListItemOperationJob" ("AddonMediaListId","AddonMediaListItemId","Kind","SavedDraftRevision","InputJson","ContextFingerprint","State","ArtifactId","OutcomeJson","CreatedAt","UpdatedAt")
            VALUES (1,10,1,0,?,?,2,?,?,'2026-09-05T00:00:00Z','2026-09-05T00:00:00Z')"#)
            .bind(serde_json::to_string(&input).unwrap()).bind(fingerprint).bind(artifact_id)
            .bind(serde_json::to_string(&ItemOperationCompletion::Prepared { artifact_id, subtitle_artifact_ids: Vec::new() }).unwrap())
            .execute(&pool).await.unwrap();
        (temporary, pool, artifact_id)
    }

    #[tokio::test]
    async fn preparation_feedback_counts_current_failures_and_retains_previous_revision() {
        use crate::addon_media_list_preparation_status::preparation_failures;
        use ersatzrs_core::repositories::AddonMediaListPreparationFailureReason;

        let (_directory, pool, _) = fixture().await;
        sqlx::query(
            r#"UPDATE "AddonMediaListItemOperationJob"
            SET "State"=3,"OutcomeJson"='{"Failed":"InvalidRequest"}'"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        let missing = BTreeSet::from([(10, 1)]);
        let (failed, reasons) = preparation_failures(&pool, 1, 0, &missing).await.unwrap();
        assert_eq!(failed, 1);
        assert_eq!(reasons.len(), 1);
        assert_eq!(reasons[0].count, 1);
        assert_eq!(reasons[0].saved_revision, 0);
        assert_eq!(
            reasons[0].reason,
            AddonMediaListPreparationFailureReason::InvalidRequest
        );
        let (failed, previous) = preparation_failures(&pool, 1, 1, &missing).await.unwrap();
        assert_eq!(
            failed, 0,
            "historical failures cannot block a newer revision"
        );
        assert_eq!(previous, reasons);
        for (list, missing) in [
            (2, missing.clone()),
            (1, BTreeSet::new()),
            (1, BTreeSet::from([(10, 2)])),
        ] {
            assert_eq!(
                preparation_failures(&pool, list, 0, &missing)
                    .await
                    .unwrap(),
                (0, Vec::new())
            );
        }
        sqlx::query(
            r#"INSERT INTO "AddonMediaListItemOperationJob"
            ("AddonMediaListId","AddonMediaListItemId","Kind","SavedDraftRevision",
             "InputJson","ContextFingerprint","State","CreatedAt","UpdatedAt")
            VALUES (1,10,1,1,'{}','fixture',0,'2026-09-12T00:00:00Z','2026-09-12T00:00:00Z')"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(
            preparation_failures(&pool, 1, 1, &missing).await.unwrap(),
            (0, Vec::new()),
            "an active retry replaces the failed attempt in current feedback"
        );
        sqlx::query(r#"UPDATE "AddonMediaListItemOperationJob" SET "State"=3,"OutcomeJson"='{"Failed":"newer-unknown-code"}'
            WHERE "SavedDraftRevision"=1"#).execute(&pool).await.unwrap();
        let (failed, reasons) = preparation_failures(&pool, 1, 1, &missing).await.unwrap();
        assert_eq!(failed, 1);
        assert_eq!(
            reasons[0].reason,
            AddonMediaListPreparationFailureReason::Failed
        );
        assert_eq!(reasons[0].saved_revision, 1);
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)] // Exact successful media/caption context and settings transitions.
    async fn prepared_selection_fences_enabled_and_cleared_subtitle_languages() {
        let (_directory, pool, artifact_id) = fixture().await;
        let raw: String =
            sqlx::query_scalar(r#"SELECT "ManifestJson" FROM "AddonInstallation" LIMIT 1"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        let mut manifest: serde_json::Value = serde_json::from_str(&raw).unwrap();
        manifest["capabilities"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"id":"media-list.subtitles.v1"}));
        sqlx::query(r#"UPDATE "AddonInstallation" SET "ManifestJson"=?"#)
            .bind(manifest.to_string())
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            r#"INSERT INTO "ConfigElement" ("Key","Value") VALUES(?,'eng')
            ON CONFLICT("Key") DO UPDATE SET "Value"=excluded."Value";"#,
        )
        .bind(ersatzrs_domain::CONFIG_KEY_FFMPEG_PREFERRED_SUBTITLE_LANGUAGE_CODE)
        .execute(&pool)
        .await
        .unwrap();
        assert!(preparation_requirements(&pool, 1, 0, &[]).await.unwrap()[0]
            .artifact_id
            .is_none());
        let raw: String = sqlx::query_scalar(
            r#"SELECT "InputJson" FROM "AddonMediaListItemOperationJob" LIMIT 1"#,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let mut input: ItemOperationInput = serde_json::from_str(&raw).unwrap();
        let runtime = InstalledItemRuntime::load(&pool, input.addon_id().unwrap(), None)
            .await
            .unwrap();
        input.package_identity = runtime.package_identity().to_owned();
        input.requested_subtitle_languages = vec!["en".into()];
        input.context_fingerprint = input.base_fingerprint().unwrap();
        sqlx::query(
            r#"UPDATE "AddonMediaListItemOperationJob" SET "InputJson"=?,"ContextFingerprint"=?"#,
        )
        .bind(serde_json::to_string(&input).unwrap())
        .bind(&input.context_fingerprint)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(r#"UPDATE "AddonMediaListArtifact" SET "CacheKey"=? WHERE "Id"=?"#)
            .bind(&input.context_fingerprint)
            .bind(artifact_id)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            preparation_requirements(&pool, 1, 0, &[]).await.unwrap()[0].artifact_id,
            Some(artifact_id)
        );
        sqlx::query(r#"INSERT INTO "AddonMediaListArtifact" ("Id","AddonMediaListId","AddonMediaListItemId","CacheKey","Kind","State","RelativePath","ByteLength","Sha256","OperationContextFingerprint","SubtitleLanguage","SubtitleTimeline","CompletedAt","CreatedAt","UpdatedAt")
            VALUES(90,1,10,'caption-fixture',0,2,'fixture.en.srt',32,?,?,'en',1,'2026-09-07T00:00:00Z','2026-09-07T00:00:00Z','2026-09-07T00:00:00Z')"#)
            .bind("a".repeat(64)).bind(&input.context_fingerprint).execute(&pool).await.unwrap();
        // A failed earlier attempt can leave promoted captions with this same
        // context. A later successful no-human result must not discover them.
        assert!(
            crate::addon_media_list_subtitle_selection::select_subtitle_artifacts(&pool, 1, 0, &[])
                .await
                .unwrap()
                .is_empty()
        );
        sqlx::query(
            r#"UPDATE "AddonMediaListArtifact" SET "PreparedSubtitleIdsJson"='[90]' WHERE "Id"=?"#,
        )
        .bind(artifact_id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(r#"UPDATE "AddonMediaListItemOperationJob" SET "OutcomeJson"=?"#)
            .bind(
                serde_json::to_string(&ItemOperationCompletion::Prepared {
                    artifact_id,
                    subtitle_artifact_ids: vec![90],
                })
                .unwrap(),
            )
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            crate::addon_media_list_subtitle_selection::select_subtitle_artifacts(&pool, 1, 0, &[])
                .await
                .unwrap()["item"],
            vec![90]
        );
        sqlx::query(
            r#"UPDATE "AddonMediaListArtifact" SET "OperationContextFingerprint"=? WHERE "Id"=90"#,
        )
        .bind("b".repeat(64))
        .execute(&pool)
        .await
        .unwrap();
        assert!(
            crate::addon_media_list_subtitle_selection::select_subtitle_artifacts(&pool, 1, 0, &[])
                .await
                .is_err()
        );
        sqlx::query(r#"UPDATE "AddonMediaListArtifact" SET "OperationContextFingerprint"=?,"SubtitleTimeline"=0 WHERE "Id"=90"#)
            .bind(&input.context_fingerprint).execute(&pool).await.unwrap();
        assert!(
            crate::addon_media_list_subtitle_selection::select_subtitle_artifacts(&pool, 1, 0, &[])
                .await
                .is_err()
        );
        sqlx::query(r#"UPDATE "ConfigElement" SET "Value"='' WHERE "Key"=?"#)
            .bind(ersatzrs_domain::CONFIG_KEY_FFMPEG_PREFERRED_SUBTITLE_LANGUAGE_CODE)
            .execute(&pool)
            .await
            .unwrap();
        assert!(preparation_requirements(&pool, 1, 0, &[]).await.unwrap()[0]
            .artifact_id
            .is_none());
    }

    #[tokio::test]
    async fn file_requirements_classify_only_current_pending_work() {
        let (_temporary, pool, artifact_id) = fixture().await;
        sqlx::query(r#"UPDATE "AddonMediaListItem" SET "EffectiveLiveness"=1, "DurationSeconds"=NULL WHERE "Id"=10"#)
            .execute(&pool).await.unwrap();
        let requirements = preparation_requirements(&pool, 1, 0, &[]).await.unwrap();
        assert_eq!(requirements[0].artifact_id, Some(artifact_id));
        sqlx::query(r#"UPDATE "AddonMediaListItemOperationJob" SET "State"=0,"ArtifactId"=NULL,"OutcomeJson"=NULL"#)
            .execute(&pool).await.unwrap();
        let waiting = preparation_requirements(&pool, 1, 0, &[]).await.unwrap();
        assert_eq!(waiting.len(), 1);
        assert!(waiting[0].pending);
        assert!(waiting[0].artifact_id.is_none());
        sqlx::query(r#"UPDATE "AddonMediaListItemOperationJob" SET "State"=1,"LeaseExpiresAt"='2000-01-01 00:00:00'"#)
            .execute(&pool).await.unwrap();
        assert!(!preparation_requirements(&pool, 1, 0, &[]).await.unwrap()[0].pending);
        sqlx::query(r#"UPDATE "AddonMediaListItemOperationJob" SET "State"=0"#)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(r#"UPDATE "AddonMediaListItem" SET "SourceUrl"='https://example.test/changed' WHERE "Id"=10"#)
            .execute(&pool).await.unwrap();
        let stale = preparation_requirements(&pool, 1, 0, &[]).await.unwrap();
        assert_eq!(stale.len(), 1);
        assert!(!stale[0].pending);
        sqlx::query(r#"UPDATE "AddonMediaListItem" SET "ProviderLiveness"=2,"EffectiveLiveness"=2 WHERE "Id"=10"#)
            .execute(&pool).await.unwrap();
        assert!(preparation_requirements(&pool, 1, 0, &[])
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn selects_exact_saved_artifact_without_running_provider() {
        let (_temporary, pool, artifact_id) = fixture().await;
        assert_eq!(
            select_prepared_media(&pool, 1, 0, &[]).await.unwrap(),
            BTreeMap::from([("item".into(), artifact_id)])
        );
        assert!(select_prepared_media(&pool, 1, 0, &[10])
            .await
            .unwrap()
            .is_empty());
        assert!(select_prepared_media(&pool, 1, 1, &[]).await.is_err());
    }

    async fn streaming_fixture() -> (
        tempfile::TempDir,
        DbPool,
        AddonMediaListPlaybackTimelinePlan,
    ) {
        let (temporary, pool, _) = fixture().await;
        sqlx::query(r#"UPDATE "AddonMediaList" SET "DeliveryMode"=0 WHERE "Id"=1"#)
            .execute(&pool)
            .await
            .unwrap();
        let encoded: String = sqlx::query_scalar(
            r#"SELECT "InputJson" FROM "AddonMediaListItemOperationJob" WHERE "Kind"=1"#,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let mut input: ItemOperationInput = serde_json::from_str(&encoded).unwrap();
        input.preparation_v2 = None;
        input.context_fingerprint = input.base_fingerprint().unwrap();
        let timing = ItemOperationTiming {
            item_duration_seconds: None,
            fragment_start_seconds: None,
            fragment_end_seconds: None,
        };
        input.context_fingerprint = timing
            .bind_context_fingerprint(&input.context_fingerprint)
            .unwrap();
        input.requested_timing = Some(timing);
        let plan = AddonMediaListPlaybackTimelinePlan::new(
            20_000,
            0..20_000,
            &[2_000..4_000, 3_000..5_000],
            input.context_fingerprint.clone(),
        )
        .unwrap();
        let completion = ItemOperationCompletion::IntervalPlan { plan: plan.clone() };
        sqlx::query(r#"INSERT INTO "AddonMediaListItemOperationJob"
            ("AddonMediaListId","AddonMediaListItemId","Kind","SavedDraftRevision","InputJson","ContextFingerprint","State","OutcomeJson","CreatedAt","UpdatedAt","CompletedAt")
            VALUES (1,10,2,0,?,?,2,?,'2026-09-06T00:00:00Z','2026-09-06T00:00:00Z','2026-09-06T00:00:00Z')"#)
            .bind(serde_json::to_string(&input).unwrap()).bind(&input.context_fingerprint)
            .bind(serde_json::to_string(&completion).unwrap()).execute(&pool).await.unwrap();
        (temporary, pool, plan)
    }

    #[tokio::test]
    async fn new_only_selects_mixed_cohorts_with_and_without_removal() {
        for removal_enabled in [true, false] {
            let (_temporary, pool, plan) = streaming_fixture().await;
            let mut tx = pool.begin().await.unwrap();
            crate::addon_media_list_delivery::save_in_tx(
                &mut tx,
                1,
                ersatzrs_domain::AddonMediaListDeliveryMode::DownloadAheadNewVideos,
                chrono::Utc::now(),
            )
            .await
            .unwrap();
            tx.commit().await.unwrap();
            sqlx::query(r#"INSERT INTO "AddonMediaListItem" ("Id","AddonMediaListId","ProviderId","DisplayTitle","Kind","Rank","Title","SourceUrl","ProviderLiveness")
                VALUES (11,1,'new-item','New fixture',4,1,'New fixture','https://example.test/new-item',1)"#)
                .execute(&pool).await.unwrap();
            if !removal_enabled {
                sqlx::query(r#"UPDATE "AddonMediaListItem" SET "DraftOptionOverridesJson"='{"remove":false}' WHERE "AddonMediaListId"=1"#)
                    .execute(&pool).await.unwrap();
            }

            let encoded: String = sqlx::query_scalar(
                r#"SELECT "InputJson" FROM "AddonMediaListItemOperationJob" WHERE "Kind"=1 AND "AddonMediaListItemId"=10"#,
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            let mut input: ItemOperationInput = serde_json::from_str(&encoded).unwrap();
            let options = load_draft_item_options(&pool, 1, 11)
                .await
                .unwrap()
                .unwrap();
            input.source_url = "https://example.test/new-item".into();
            input.provider_id = "new-item".into();
            input.schema = Some(options.schema);
            input.definition = Some(options.definition);
            input.options = Some(options.values);
            input.context_fingerprint = input.base_fingerprint().unwrap();
            let artifact_id = sqlx::query(r#"INSERT INTO "AddonMediaListArtifact" ("AddonMediaListId","AddonMediaListItemId","CacheKey","Kind","State","DurationMilliseconds","CreatedAt","UpdatedAt")
                VALUES (1,11,?,1,2,12500,'2026-09-05T00:00:00Z','2026-09-05T00:00:00Z')"#)
                .bind(&input.context_fingerprint).execute(&pool).await.unwrap().last_insert_rowid();
            sqlx::query(r#"INSERT INTO "AddonMediaListItemOperationJob" ("AddonMediaListId","AddonMediaListItemId","Kind","SavedDraftRevision","InputJson","ContextFingerprint","State","ArtifactId","OutcomeJson","CreatedAt","UpdatedAt")
                VALUES (1,11,1,0,?,?,2,?,?,'2026-09-05T00:00:00Z','2026-09-05T00:00:00Z')"#)
                .bind(serde_json::to_string(&input).unwrap()).bind(&input.context_fingerprint).bind(artifact_id)
            .bind(serde_json::to_string(&ItemOperationCompletion::Prepared { artifact_id, subtitle_artifact_ids: Vec::new() }).unwrap())
                .execute(&pool).await.unwrap();

            assert_eq!(
                select_prepared_media(&pool, 1, 0, &[]).await.unwrap(),
                BTreeMap::from([("new-item".into(), artifact_id)]),
            );
            let expected_plans = if removal_enabled {
                BTreeMap::from([("item".into(), plan)])
            } else {
                BTreeMap::new()
            };
            assert_eq!(
                select_streaming_plans(&pool, 1, 0, &[]).await.unwrap(),
                expected_plans,
            );

            // Neither cohort may fall back to the other's transport evidence.
            sqlx::query(
                r#"DELETE FROM "AddonMediaListItemOperationJob" WHERE "AddonMediaListItemId"=11"#,
            )
            .execute(&pool)
            .await
            .unwrap();
            let requirements = preparation_requirements(&pool, 1, 0, &[]).await.unwrap();
            assert_eq!(requirements.len(), 1);
            assert_eq!(requirements[0].item_id, 11);
            assert!(requirements[0].artifact_id.is_none());
            assert!(select_prepared_media(&pool, 1, 0, &[]).await.is_err());
        }
    }

    #[tokio::test]
    async fn streaming_selects_current_canonical_plan_without_provider_execution() {
        // The package script exits 99: successful selection uses persisted evidence only.
        let (_temporary, pool, plan) = streaming_fixture().await;
        let requirements = interval_metadata_requirements(&pool, 1, 0, &[])
            .await
            .unwrap();
        assert_eq!(requirements.len(), 1);
        assert_eq!(requirements[0].item_id, 10);
        assert_eq!(requirements[0].provider_id, "item");
        assert_eq!(requirements[0].plan, Some(plan.clone()));
        assert!(!requirements[0].pending);
        assert_eq!(plan.duration_milliseconds(), 17_000);
        assert_eq!(plan.excluded_intervals_milliseconds().len(), 1);
        assert_eq!(plan.excluded_intervals_milliseconds()[0], 2_000..5_000);
        assert_eq!(
            select_streaming_plans(&pool, 1, 0, &[]).await.unwrap(),
            BTreeMap::from([("item".into(), plan)])
        );
        assert!(select_streaming_plans(&pool, 1, 0, &[10])
            .await
            .unwrap()
            .is_empty());
        assert!(select_streaming_plans(&pool, 1, 1, &[]).await.is_err());
    }

    #[tokio::test]
    async fn streaming_never_uses_prepared_file_as_interval_evidence() {
        let (_temporary, pool, artifact_id) = fixture().await;
        assert_eq!(
            select_prepared_media(&pool, 1, 0, &[]).await.unwrap()["item"],
            artifact_id
        );
        assert!(select_streaming_plans(&pool, 1, 0, &[])
            .await
            .unwrap()
            .is_empty());
        sqlx::query(r#"UPDATE "AddonMediaList" SET "DeliveryMode"=0 WHERE "Id"=1"#)
            .execute(&pool)
            .await
            .unwrap();
        assert!(select_prepared_media(&pool, 1, 0, &[])
            .await
            .unwrap()
            .is_empty());
        let missing = interval_metadata_requirements(&pool, 1, 0, &[])
            .await
            .unwrap();
        assert_eq!(missing.len(), 1);
        assert!(missing[0].plan.is_none());
        assert!(!missing[0].pending);
        assert!(select_streaming_plans(&pool, 1, 0, &[]).await.is_err());
    }

    #[tokio::test]
    async fn streaming_rejects_changed_timing_and_corrupt_interval_evidence() {
        for mutation in [
            r#"UPDATE "AddonMediaListItem" SET "DurationSeconds"=20 WHERE "Id"=10"#,
            r#"UPDATE "AddonMediaListItem" SET "FragmentStartSeconds"=1 WHERE "Id"=10"#,
            r#"UPDATE "AddonMediaListItem" SET "FragmentEndSeconds"=19 WHERE "Id"=10"#,
            r#"UPDATE "AddonMediaListItem" SET "SourceUrl"='https://example.test/changed' WHERE "Id"=10"#,
            r#"UPDATE "AddonInstallation" SET "StateRevision"='changed'"#,
            r#"UPDATE "AddonMediaListItemOperationJob" SET "ContextFingerprint"='changed' WHERE "Kind"=2"#,
            r#"UPDATE "AddonMediaListItemOperationJob" SET "InputJson"=json_remove("InputJson",'$.requested_timing') WHERE "Kind"=2"#,
            r#"UPDATE "AddonMediaListItemOperationJob" SET "OutcomeJson"=json_set("OutcomeJson",'$.IntervalPlan.plan.version',2) WHERE "Kind"=2"#,
            r#"UPDATE "AddonMediaListItemOperationJob" SET "OutcomeJson"=json_set("OutcomeJson",'$.IntervalPlan.plan.context_fingerprint','changed') WHERE "Kind"=2"#,
            r#"UPDATE "AddonMediaListItemOperationJob" SET "OutcomeJson"=json_set("OutcomeJson",'$.IntervalPlan.plan.source_window_milliseconds.start',1000) WHERE "Kind"=2"#,
            r#"UPDATE "AddonMediaListItemOperationJob" SET "ArtifactId"=(SELECT MIN("Id") FROM "AddonMediaListArtifact") WHERE "Kind"=2"#,
            r#"UPDATE "AddonMediaListItemOperationJob" SET "CompletedAt"=NULL WHERE "Kind"=2"#,
            r#"DELETE FROM "AddonMediaListItemOperationJob" WHERE "Kind"=2"#,
        ] {
            let (_temporary, pool, _) = streaming_fixture().await;
            sqlx::query(mutation).execute(&pool).await.unwrap();
            let requirements = interval_metadata_requirements(&pool, 1, 0, &[])
                .await
                .unwrap();
            assert!(requirements[0].plan.is_none(), "{mutation}");
            assert!(!requirements[0].pending, "{mutation}");
            assert!(
                select_streaming_plans(&pool, 1, 0, &[]).await.is_err(),
                "{mutation}"
            );
        }
    }

    #[tokio::test]
    async fn streaming_pending_requires_current_timing_and_live_lease() {
        let (_temporary, pool, _) = streaming_fixture().await;
        sqlx::query(r#"UPDATE "AddonMediaListItemOperationJob" SET "State"=0,"OutcomeJson"=NULL,"CompletedAt"=NULL WHERE "Kind"=2"#)
            .execute(&pool).await.unwrap();
        assert!(
            interval_metadata_requirements(&pool, 1, 0, &[])
                .await
                .unwrap()[0]
                .pending
        );
        sqlx::query(r#"UPDATE "AddonMediaListItemOperationJob" SET "State"=1,"LeaseExpiresAt"='2000-01-01 00:00:00' WHERE "Kind"=2"#)
            .execute(&pool).await.unwrap();
        assert!(
            !interval_metadata_requirements(&pool, 1, 0, &[])
                .await
                .unwrap()[0]
                .pending
        );
        sqlx::query(r#"UPDATE "AddonMediaListItemOperationJob" SET "LeaseExpiresAt"='2999-01-01 00:00:00' WHERE "Kind"=2"#)
            .execute(&pool).await.unwrap();
        assert!(
            interval_metadata_requirements(&pool, 1, 0, &[])
                .await
                .unwrap()[0]
                .pending
        );
        sqlx::query(r#"UPDATE "AddonMediaListItem" SET "DurationSeconds"=20 WHERE "Id"=10"#)
            .execute(&pool)
            .await
            .unwrap();
        assert!(
            !interval_metadata_requirements(&pool, 1, 0, &[])
                .await
                .unwrap()[0]
                .pending
        );
    }

    #[tokio::test]
    async fn skips_only_usable_local_replacements() {
        let (_temporary, pool, _) = fixture().await;
        sqlx::raw_sql(r#"INSERT INTO "MediaSource" ("Id") VALUES (900);
            INSERT INTO "LocalMediaSource" ("Id") VALUES (900);
            INSERT INTO "Library" ("Id","Name","MediaKind","MediaSourceId") VALUES (900,'Local',1,900);
            INSERT INTO "LibraryPath" ("Id","LibraryId","Path") VALUES (900,900,'fixture-local');
            INSERT INTO "MediaItem" ("Id","LibraryPathId","State") VALUES (900,900,0);
            INSERT INTO "Movie" ("Id") VALUES (900);
            INSERT INTO "MediaVersion" ("Id","MovieId","Duration","DateAdded","DateUpdated","Height","Width","VideoScanKind")
                VALUES (901,900,'00:01:00','2026-09-05 00:00:00','2026-09-05 00:00:00',1080,1920,0);
            INSERT INTO "MediaFile" ("MediaVersionId","Path","PathHash") VALUES (901,'fixture-local/replacement.mkv','fixture');
            INSERT INTO "MediaStream" ("MediaVersionId","Index","MediaStreamKind","Channels","Default","Forced","AttachedPic")
                VALUES (901,0,1,0,1,0,0);"#)
            .execute(&pool).await.unwrap();
        sqlx::query(
            r#"INSERT INTO "AddonMediaListItemReplacement" (
            "AddonMediaListItemId", "MediaItemId", "MediaVersionId", "CreatedAt", "UpdatedAt")
            VALUES (10,900,901,'2026-09-05T00:00:00Z','2026-09-05T00:00:00Z')"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        assert!(preparation_requirements(&pool, 1, 0, &[])
            .await
            .unwrap()
            .is_empty());
        sqlx::query(r#"DELETE FROM "AddonMediaListItemOperationJob""#)
            .execute(&pool)
            .await
            .unwrap();
        assert!(select_prepared_media(&pool, 1, 0, &[])
            .await
            .unwrap()
            .is_empty());
        // A replacement row remains after its local media becomes unavailable.
        sqlx::query(r#"UPDATE "MediaItem" SET "State"=1 WHERE "Id"=900"#)
            .execute(&pool)
            .await
            .unwrap();
        let requirements = preparation_requirements(&pool, 1, 0, &[]).await.unwrap();
        assert_eq!(requirements.len(), 1);
        assert_eq!(requirements[0].item_id, 10);
        assert_eq!(requirements[0].artifact_id, None);
        assert!(!requirements[0].pending);
        assert!(select_prepared_media(&pool, 1, 0, &[]).await.is_err());
    }

    #[tokio::test]
    async fn unresolved_replacement_never_prepares_original_source() {
        let (_temporary, pool, _) = fixture().await;
        sqlx::query(r#"UPDATE "AddonMediaListItem" SET "SavedReplacementJson"='{"file":null}' WHERE "Id"=10"#)
            .execute(&pool).await.unwrap();
        assert!(preparation_requirements(&pool, 1, 0, &[])
            .await
            .unwrap()
            .is_empty());
        sqlx::query(r#"UPDATE "AddonMediaList" SET "DeliveryMode"=0 WHERE "Id"=1"#)
            .execute(&pool)
            .await
            .unwrap();
        assert!(interval_metadata_requirements(&pool, 1, 0, &[])
            .await
            .unwrap()
            .is_empty());
        sqlx::query(r#"UPDATE "AddonMediaListItem" SET "SavedReplacementJson"=NULL WHERE "Id"=10"#)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            interval_metadata_requirements(&pool, 1, 0, &[])
                .await
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn rejects_stale_or_missing_required_artifacts() {
        for mutation in [
            r#"UPDATE "AddonMediaListArtifact" SET "State"=0"#,
            r#"UPDATE "AddonMediaListArtifact" SET "CacheKey"='changed'"#,
            r#"UPDATE "AddonMediaListItem" SET "SourceUrl"='https://example.test/changed'"#,
            r#"UPDATE "AddonInstallation" SET "StateRevision"='changed'"#,
            r#"DELETE FROM "AddonMediaListItemOperationJob""#,
        ] {
            let (_temporary, pool, _) = fixture().await;
            sqlx::query(mutation).execute(&pool).await.unwrap();
            assert!(
                select_prepared_media(&pool, 1, 0, &[]).await.is_err(),
                "{mutation}"
            );
        }
    }

    #[tokio::test]
    async fn skips_nonfinite_but_requires_download_when_removal_is_disabled() {
        let (_temporary, pool, _) = fixture().await;
        sqlx::query(r#"DELETE FROM "AddonMediaListItemOperationJob""#)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(r#"UPDATE "AddonMediaListItem" SET "ProviderLiveness"=2"#)
            .execute(&pool)
            .await
            .unwrap();
        assert!(select_prepared_media(&pool, 1, 0, &[])
            .await
            .unwrap()
            .is_empty());
        sqlx::query(r#"UPDATE "AddonMediaListItem" SET "ProviderLiveness"=1, "DraftOptionOverridesJson"='{"remove":false}'"#)
            .execute(&pool).await.unwrap();
        let requirements = preparation_requirements(&pool, 1, 0, &[]).await.unwrap();
        assert_eq!(requirements.len(), 1);
        assert_eq!(requirements[0].item_id, 10);
        assert_eq!(requirements[0].artifact_id, None);
        assert!(!requirements[0].pending);
        assert!(select_prepared_media(&pool, 1, 0, &[]).await.is_err());
    }
}
