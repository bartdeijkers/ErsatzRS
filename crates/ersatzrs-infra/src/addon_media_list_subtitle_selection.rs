//! Saved subtitle requirements, without provider execution or implicit language fallback.

use std::collections::{BTreeMap, BTreeSet};

use ersatzrs_addon_contract::CAPABILITY_MEDIA_LIST_SUBTITLES_V1;
use ersatzrs_core::repositories::{RepositoryError, Result};
use ersatzrs_domain::AddonMediaListDeliveryMode;
use sqlx::Row as _;

use crate::{
    addon_media_list_item_jobs::{ItemOperationCompletion, ItemOperationInput},
    addon_media_list_item_runtime::InstalledItemRuntime,
    addon_media_list_options::load_draft_item_options,
    db::DbPool,
    repositories::{common::decode_datetime, SqlxResultExt as _},
};

pub(crate) struct SubtitleRequirement {
    pub item_id: i64,
    pub provider_id: String,
    pub pending: bool,
    /// Some(empty) is successful absence of requested human-authored tracks.
    pub artifacts: Option<Vec<i64>>,
}

/// Preparation v2 does not itself promise subtitle support.
pub(crate) async fn requested_preparation_languages(
    pool: &DbPool,
    list_id: i64,
) -> Result<Vec<String>> {
    // Read-only transaction: observe configured languages and provider capabilities together.
    let mut tx = pool.begin().await.repository()?;
    let languages = requested_preparation_languages_in_tx(&mut tx, list_id).await?;
    tx.commit().await.repository()?;
    Ok(languages)
}

pub(crate) async fn requested_preparation_languages_in_tx(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    list_id: i64,
) -> Result<Vec<String>> {
    let languages = crate::addon_media_list_subtitles::requested_languages_in_tx(tx).await?;
    if languages.is_empty() {
        return Ok(languages);
    }
    let manifest: String = sqlx::query_scalar(
        r#"SELECT "installation"."ManifestJson"
        FROM "AddonMediaList" AS "list" JOIN "AddonInstallation" AS "installation"
        ON "installation"."AddonId"="list"."AddonId" WHERE "list"."Id"=?"#,
    )
    .bind(list_id)
    .fetch_optional(&mut **tx)
    .await
    .repository()?
    .ok_or_else(stale)?;
    let manifest: ersatzrs_addon_contract::AddonManifest =
        serde_json::from_str(&manifest).map_err(|_| stale())?;
    if manifest.has_capability(CAPABILITY_MEDIA_LIST_SUBTITLES_V1) {
        Ok(languages)
    } else {
        Ok(Vec::new())
    }
}

#[allow(clippy::too_many_lines)] // Keep exact saved identity and artifact evidence validation together.
pub(crate) async fn subtitle_requirements(
    pool: &DbPool,
    list_id: i64,
    saved_revision: i32,
    excluded_item_ids: &[i64],
) -> Result<Vec<SubtitleRequirement>> {
    let languages = crate::addon_media_list_subtitles::requested_languages(pool).await?;
    if languages.is_empty() {
        return Ok(Vec::new());
    }
    let addon_id = require_revision(pool, list_id, saved_revision).await?;
    let installed = crate::addons::load_installed_addon(pool, &addon_id)
        .await
        .map_err(|_| stale())?
        .ok_or_else(stale)?;
    if !installed
        .manifest
        .has_capability(CAPABILITY_MEDIA_LIST_SUBTITLES_V1)
    {
        return Ok(Vec::new());
    }
    // Read-only transaction: load the saved delivery policy and its item cohorts.
    let mut tx = pool.begin().await.repository()?;
    let deliveries =
        crate::addon_media_list_delivery::load_item_deliveries_in_tx(&mut tx, list_id).await?;
    tx.commit().await.repository()?;
    let rows = sqlx::query(r#"SELECT "item"."Id", "item"."ProviderId", "item"."SourceUrl",
        "replacement"."MediaItemId" AS "ReplacementItemId", "replacement"."MediaVersionId" AS "ReplacementVersionId"
        FROM "AddonMediaListItem" AS "item"
        LEFT JOIN "AddonMediaListItemReplacement" AS "replacement" ON "replacement"."AddonMediaListItemId"="item"."Id"
        WHERE "item"."AddonMediaListId"=? AND "item"."Included"=1 AND "item"."Kind"=4
          AND COALESCE("item"."LivenessOverride", "item"."ProviderLiveness")=1
        ORDER BY "item"."Id""#).bind(list_id).fetch_all(pool).await.repository()?;
    let mut requirements = Vec::new();
    for row in rows {
        let item_id: i64 = row.try_get("Id").repository()?;
        let provider_id: String = row.try_get("ProviderId").repository()?;
        if excluded_item_ids.contains(&item_id)
            || deliveries.get(&provider_id) == Some(&AddonMediaListDeliveryMode::DownloadAhead)
        {
            continue;
        }
        if let Some((item, version)) = row
            .try_get::<Option<i64>, _>("ReplacementItemId")
            .repository()?
            .zip(
                row.try_get::<Option<i64>, _>("ReplacementVersionId")
                    .repository()?,
            )
        {
            if crate::repositories::load_replacement_candidate(pool, item, version)
                .await?
                .is_some()
            {
                continue;
            }
        }
        let source: Option<String> = row.try_get("SourceUrl").repository()?;
        let options = load_draft_item_options(pool, list_id, item_id).await?;
        let mut requirement = SubtitleRequirement {
            item_id,
            provider_id,
            pending: false,
            artifacts: None,
        };
        // Keep completed success eligible while another attempt is queued or failed.
        let jobs = sqlx::query(
            r#"SELECT "State", "InputJson", "OutcomeJson", "ContextFingerprint",
            "LeaseExpiresAt", "CompletedAt", "ArtifactId" FROM "AddonMediaListItemOperationJob"
            WHERE "AddonMediaListId"=? AND "AddonMediaListItemId"=? AND "SavedDraftRevision"=?
              AND "Kind"=3 AND "State" IN (0,1,2) ORDER BY "Id" DESC"#,
        )
        .bind(list_id)
        .bind(item_id)
        .bind(saved_revision)
        .fetch_all(pool)
        .await
        .repository()?;
        for job in jobs {
            let Ok(input) = serde_json::from_str::<ItemOperationInput>(
                &job.try_get::<String, _>("InputJson").repository()?,
            ) else {
                continue;
            };
            if input.source_url.as_str() != source.as_deref().unwrap_or_default()
                || input.provider_id != requirement.provider_id
                || !input.item_is_finite
                || input.requested_subtitle_languages != languages
                || input.requested_timing.is_some()
                || input.preparation_v2.is_some()
                || input.max_video_height.is_some()
                || input.addon_id().ok() != Some(addon_id.as_str())
                || input.resolved_options().ok().as_ref() != Some(&options)
                || input.base_fingerprint().ok().as_deref()
                    != Some(input.context_fingerprint.as_str())
                || job
                    .try_get::<String, _>("ContextFingerprint")
                    .repository()?
                    != input.context_fingerprint
            {
                continue;
            }
            let Ok(runtime) =
                InstalledItemRuntime::load(pool, &addon_id, Some(&input.package_identity)).await
            else {
                continue;
            };
            if options
                .as_ref()
                .is_some_and(|options| runtime.validate_options(options).is_err())
            {
                continue;
            }
            let state: i32 = job.try_get("State").repository()?;
            if state != 2 {
                requirement.pending |= state == 0
                    || job
                        .try_get::<Option<String>, _>("LeaseExpiresAt")
                        .repository()?
                        .and_then(|value| decode_datetime("subtitle lease", &value).ok())
                        .is_some_and(|expires| expires > chrono::Utc::now());
                continue;
            }
            if job
                .try_get::<Option<String>, _>("CompletedAt")
                .repository()?
                .is_none()
                || job
                    .try_get::<Option<i64>, _>("ArtifactId")
                    .repository()?
                    .is_some()
            {
                continue;
            }
            let completion = job
                .try_get::<Option<String>, _>("OutcomeJson")
                .repository()?
                .filter(|value| value.len() <= 65_536)
                .and_then(|value| serde_json::from_str::<ItemOperationCompletion>(&value).ok());
            if let Some(ItemOperationCompletion::Subtitles { artifact_ids }) = completion {
                if valid_artifacts(
                    pool,
                    list_id,
                    item_id,
                    &input.context_fingerprint,
                    &input.requested_subtitle_languages,
                    &artifact_ids,
                    0,
                )
                .await?
                {
                    requirement.artifacts = Some(artifact_ids);
                    break;
                }
            }
        }
        requirements.push(requirement);
    }
    require_revision(pool, list_id, saved_revision).await?;
    if crate::addon_media_list_subtitles::requested_languages(pool).await? != languages {
        return Err(stale());
    }
    Ok(requirements)
}

async fn valid_artifacts(
    pool: &DbPool,
    list_id: i64,
    item_id: i64,
    fingerprint: &str,
    requested_languages: &[String],
    ids: &[i64],
    timeline: i32,
) -> Result<bool> {
    if ids.len() > requested_languages.len()
        || ids.iter().collect::<BTreeSet<_>>().len() != ids.len()
    {
        return Ok(false);
    }
    let mut seen = BTreeSet::new();
    for id in ids {
        let language: Option<String> = sqlx::query_scalar(r#"SELECT "SubtitleLanguage" FROM "AddonMediaListArtifact"
            WHERE "Id"=? AND "AddonMediaListId"=? AND "AddonMediaListItemId"=? AND "Kind"=0 AND "State"=2
              AND "OperationContextFingerprint"=? AND "SubtitleTimeline"=? AND "SubtitleLanguage" IS NOT NULL
              AND "CompletedAt" IS NOT NULL AND "ByteLength">0 AND "RelativePath" IS NOT NULL AND "Sha256" IS NOT NULL"#)
            .bind(id).bind(list_id).bind(item_id).bind(fingerprint).bind(timeline).fetch_optional(pool).await.repository()?;
        let Some(language) = language else {
            return Ok(false);
        };
        let primary = language
            .split('-')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !seen.insert(primary.clone())
            || !requested_languages.iter().any(|requested| {
                requested
                    .split('-')
                    .next()
                    .is_some_and(|value| value.eq_ignore_ascii_case(&primary))
            })
        {
            return Ok(false);
        }
    }
    Ok(true)
}

pub(crate) async fn select_subtitle_artifacts(
    pool: &DbPool,
    list_id: i64,
    revision: i32,
    excluded: &[i64],
) -> Result<BTreeMap<String, Vec<i64>>> {
    let mut selected = BTreeMap::new();
    for requirement in subtitle_requirements(pool, list_id, revision, excluded).await? {
        let ids = requirement.artifacts.ok_or_else(stale)?;
        if !ids.is_empty() {
            selected.insert(requirement.provider_id, ids);
        }
    }
    let languages = requested_preparation_languages(pool, list_id).await?;
    if !languages.is_empty() {
        // The prepared selector proves the exact successful job and saved inputs.
        // Only that file's cache key may contribute its companion sidecars.
        let prepared = crate::addon_media_list_prepared_selection::select_prepared_media(
            pool, list_id, revision, excluded,
        )
        .await?;
        for (provider_id, artifact_id) in prepared {
            let (item_id, fingerprint): (i64, String) = sqlx::query_as(r#"SELECT "item"."Id", "artifact"."CacheKey"
                FROM "AddonMediaListArtifact" AS "artifact" JOIN "AddonMediaListItem" AS "item"
                ON "item"."Id"="artifact"."AddonMediaListItemId" AND "item"."AddonMediaListId"="artifact"."AddonMediaListId"
                WHERE "artifact"."Id"=? AND "artifact"."AddonMediaListId"=? AND "item"."ProviderId"=?
                  AND "artifact"."Kind"=1 AND "artifact"."State"=2"#)
                .bind(artifact_id).bind(list_id).bind(&provider_id).fetch_optional(pool).await.repository()?.ok_or_else(stale)?;
            let outcome: Option<String> = sqlx::query_scalar(
                r#"SELECT "OutcomeJson" FROM "AddonMediaListItemOperationJob"
                WHERE "AddonMediaListId"=? AND "AddonMediaListItemId"=? AND "Kind"=1 AND "State"=2
                  AND "ContextFingerprint"=? AND "ArtifactId"=? AND "SavedDraftRevision"=?
                ORDER BY "Id" DESC LIMIT 1"#,
            )
            .bind(list_id)
            .bind(item_id)
            .bind(&fingerprint)
            .bind(artifact_id)
            .bind(revision)
            .fetch_optional(pool)
            .await
            .repository()?
            .flatten();
            let completion = outcome
                .filter(|value| value.len() <= 65_536)
                .and_then(|value| serde_json::from_str::<ItemOperationCompletion>(&value).ok());
            let Some(ItemOperationCompletion::Prepared {
                artifact_id: completed_id,
                subtitle_artifact_ids: ids,
            }) = completion
            else {
                return Err(stale());
            };
            if completed_id != artifact_id {
                return Err(stale());
            }
            if !valid_artifacts(pool, list_id, item_id, &fingerprint, &languages, &ids, 1).await? {
                return Err(stale());
            }
            if !ids.is_empty() && selected.insert(provider_id, ids).is_some() {
                return Err(stale());
            }
        }
        if requested_preparation_languages(pool, list_id).await? != languages {
            return Err(stale());
        }
    }
    require_revision(pool, list_id, revision).await?;
    Ok(selected)
}

async fn require_revision(pool: &DbPool, list_id: i64, revision: i32) -> Result<String> {
    sqlx::query_scalar(
        r#"SELECT "AddonId" FROM "AddonMediaList" WHERE "Id"=? AND "DraftRevision"=?"#,
    )
    .bind(list_id)
    .bind(revision)
    .fetch_optional(pool)
    .await
    .repository()?
    .ok_or_else(stale)
}

fn stale() -> RepositoryError {
    RepositoryError::Conflict("saved subtitle acquisition is unavailable or stale".into())
}

// Reuse the installed POSIX fixture; selection must never execute its program.
#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn subtitle_selection_distinguishes_absence_staleness_and_artifact_clock() {
        let directory = tempfile::tempdir().unwrap();
        let pool = crate::addon_media_list::tests::fixture_media_list_addon_with_script(
            &directory.path().join("package"),
            "#!/bin/sh\nexit 99\n",
            &["example.test"],
        )
        .await;
        let raw: String =
            sqlx::query_scalar(r#"SELECT "ManifestJson" FROM "AddonInstallation" LIMIT 1"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        let mut manifest: serde_json::Value = serde_json::from_str(&raw).unwrap();
        manifest["manifest_version"] = serde_json::json!(5);
        manifest["capabilities"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"id": CAPABILITY_MEDIA_LIST_SUBTITLES_V1}));
        sqlx::query(r#"UPDATE "AddonInstallation" SET "ManifestJson"=?"#)
            .bind(manifest.to_string())
            .execute(&pool)
            .await
            .unwrap();
        sqlx::raw_sql(r#"INSERT INTO "AddonMediaList" ("Id","AddonId","ProviderId","SourceUrl","Name")
            VALUES(1,'org.ersatzrs.addon.fixture','list','https://example.test/list','Fixture');
            INSERT INTO "AddonMediaListItem" ("Id","AddonMediaListId","ProviderId","DisplayTitle","Kind","Rank","Title","SourceUrl","ProviderLiveness")
            VALUES(10,1,'item','Fixture',4,0,'Fixture','https://example.test/item',1);"#).execute(&pool).await.unwrap();
        assert!(subtitle_requirements(&pool, 1, 0, &[])
            .await
            .unwrap()
            .is_empty());
        sqlx::query(
            r#"INSERT INTO "ConfigElement" ("Key","Value") VALUES(?,'eng')
            ON CONFLICT("Key") DO UPDATE SET "Value"=excluded."Value";"#,
        )
        .bind(ersatzrs_domain::CONFIG_KEY_FFMPEG_PREFERRED_SUBTITLE_LANGUAGE_CODE)
        .execute(&pool)
        .await
        .unwrap();
        assert!(subtitle_requirements(&pool, 1, 0, &[]).await.unwrap()[0]
            .artifacts
            .is_none());
        let runtime = InstalledItemRuntime::load(&pool, "org.ersatzrs.addon.fixture", None)
            .await
            .unwrap();
        let mut input = ItemOperationInput {
            source_url: "https://example.test/item".into(),
            provider_id: "item".into(),
            context_fingerprint: String::new(),
            schema: None,
            definition: None,
            options: None,
            addon_id: Some("org.ersatzrs.addon.fixture".into()),
            preparation_v2: None,
            package_identity: runtime.package_identity().into(),
            tool_version: "1.0".into(),
            item_is_finite: true,
            max_video_height: None,
            requested_timing: None,
            requested_subtitle_languages: vec!["en".into()],
        };
        input.context_fingerprint = input.base_fingerprint().unwrap();
        let empty = serde_json::to_string(&ItemOperationCompletion::Subtitles {
            artifact_ids: vec![],
        })
        .unwrap();
        sqlx::query(r#"INSERT INTO "AddonMediaListItemOperationJob" ("AddonMediaListId","AddonMediaListItemId","Kind","SavedDraftRevision","InputJson","ContextFingerprint","State","OutcomeJson","CompletedAt","CreatedAt","UpdatedAt")
            VALUES(1,10,3,0,?,?,2,?,'2026-09-07T00:00:00Z','2026-09-07T00:00:00Z','2026-09-07T00:00:00Z')"#)
            .bind(serde_json::to_string(&input).unwrap()).bind(&input.context_fingerprint).bind(empty).execute(&pool).await.unwrap();
        assert_eq!(
            subtitle_requirements(&pool, 1, 0, &[]).await.unwrap()[0].artifacts,
            Some(vec![])
        );
        assert!(select_subtitle_artifacts(&pool, 1, 0, &[])
            .await
            .unwrap()
            .is_empty());
        sqlx::query(r#"INSERT INTO "AddonMediaListArtifact" ("Id","AddonMediaListId","AddonMediaListItemId","CacheKey","Kind","State","RelativePath","ByteLength","Sha256","OperationContextFingerprint","SubtitleLanguage","SubtitleTimeline","CompletedAt","CreatedAt","UpdatedAt")
            VALUES(90,1,10,'fixture',0,2,'fixture.en.srt',32,?,?,'en',0,'2026-09-07T00:00:00Z','2026-09-07T00:00:00Z','2026-09-07T00:00:00Z')"#)
            .bind("a".repeat(64)).bind(&input.context_fingerprint).execute(&pool).await.unwrap();
        sqlx::query(r#"UPDATE "AddonMediaListItemOperationJob" SET "OutcomeJson"=?"#)
            .bind(
                serde_json::to_string(&ItemOperationCompletion::Subtitles {
                    artifact_ids: vec![90],
                })
                .unwrap(),
            )
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            select_subtitle_artifacts(&pool, 1, 0, &[]).await.unwrap()["item"],
            vec![90]
        );
        sqlx::query(r#"UPDATE "AddonMediaListArtifact" SET "SubtitleTimeline"=1 WHERE "Id"=90"#)
            .execute(&pool)
            .await
            .unwrap();
        assert!(subtitle_requirements(&pool, 1, 0, &[]).await.unwrap()[0]
            .artifacts
            .is_none());
        sqlx::query(r#"UPDATE "AddonMediaListArtifact" SET "SubtitleTimeline"=0 WHERE "Id"=90"#)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(r#"UPDATE "ConfigElement" SET "Value"='nld' WHERE "Key"=?"#)
            .bind(ersatzrs_domain::CONFIG_KEY_FFMPEG_PREFERRED_SUBTITLE_LANGUAGE_CODE)
            .execute(&pool)
            .await
            .unwrap();
        assert!(subtitle_requirements(&pool, 1, 0, &[]).await.unwrap()[0]
            .artifacts
            .is_none());
        assert!(subtitle_requirements(&pool, 1, 0, &[10])
            .await
            .unwrap()
            .is_empty());
    }
}
