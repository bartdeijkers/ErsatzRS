//! Ownership fence for the existing detail queue, including crash checkpoints.

use ersatzrs_core::repositories::{RepositoryError, Result};
use ersatzrs_domain::RemoteStreamLiveness;
use serde_json::Value;

use crate::{
    addon_media_list_options::{
        load_draft_item_options, load_published_item_options, ResolvedAddonItemOptions,
    },
    db::{Db, DbPool},
    repositories::SqlxResultExt as _,
};

pub(crate) async fn capture(
    tx: &mut sqlx::Transaction<'_, Db>,
    list_id: i64,
    item_id: i64,
    purpose: i32,
) -> Result<String> {
    // All joins use primary/unique keys: exactly one context for one queue item.
    // Do not include list-wide DraftRevision: a sibling detail merge advances
    // it without changing this item's inputs. Saved values themselves are fenced.
    let context: String = sqlx::query_scalar(r#"WITH "owner" AS (
        SELECT "list".*, "item"."Id" AS "ItemId", "item"."ProviderId" AS "ItemProviderId",
            "item"."SourceUrl" AS "ItemSourceUrl", "item"."DraftOptionOverridesJson",
            COALESCE("item"."LivenessOverride", "item"."ProviderLiveness") AS "ItemLiveness",
            CASE WHEN ? = 0 AND "item"."Published" = 1 AND "list"."PublishedRevision" > 0 THEN 1 ELSE 0 END AS "UsePublished"
        FROM "AddonMediaList" AS "list" JOIN "AddonMediaListItem" AS "item"
            ON "item"."AddonMediaListId" = "list"."Id"
        WHERE "list"."Id" = ? AND "item"."Id" = ?)
        SELECT json_object(
            'purpose', ?, 'published_revision', CASE WHEN "o"."UsePublished" = 1 THEN "o"."PublishedRevision" END,
            'current_publication', "o"."PublishedRevision",
            'addon_id', "o"."AddonId", 'provider_id', "o"."ItemProviderId", 'list_source', "o"."SourceUrl",
            'source_url', CASE WHEN "o"."UsePublished" = 1 THEN COALESCE("ri"."SourceUrl", "o"."ItemSourceUrl") ELSE "o"."ItemSourceUrl" END,
            'liveness', CASE WHEN "o"."UsePublished" = 1 THEN COALESCE("ri"."EffectiveLiveness", "o"."ItemLiveness") ELSE "o"."ItemLiveness" END,
            'schema_id', CASE WHEN "o"."UsePublished" = 1 THEN "r"."OptionSchemaId" ELSE "o"."DraftOptionSchemaId" END,
            'list_values', CASE WHEN "o"."UsePublished" = 1 THEN "r"."OptionValuesJson" ELSE "o"."DraftOptionValuesJson" END,
            'item_values', CASE WHEN "o"."UsePublished" = 1 THEN COALESCE("ro"."OptionOverridesJson", '{}') ELSE "o"."DraftOptionOverridesJson" END,
            'package_revision', "a"."StateRevision", 'package_content', "a"."ContentSha256", 'package_enabled', "a"."Enabled",
            'overview', "d"."OverviewFingerprint", 'overview_source', json_extract("d"."OverviewRecordJson", '$.source_url'))
        FROM "owner" AS "o"
        JOIN "AddonMediaListDetailJob" AS "d" ON "d"."AddonMediaListItemId" = "o"."ItemId"
        LEFT JOIN "AddonInstallation" AS "a" ON "a"."AddonId" = "o"."AddonId"
        LEFT JOIN "AddonMediaListRevision" AS "r" ON "r"."AddonMediaListId" = "o"."Id" AND "r"."Revision" = "o"."PublishedRevision"
        LEFT JOIN "AddonMediaListRevisionItem" AS "ri" ON "ri"."AddonMediaListRevisionId" = "r"."Id" AND "ri"."ProviderId" = "o"."ItemProviderId"
        LEFT JOIN "AddonMediaListRevisionItemOption" AS "ro" ON "ro"."AddonMediaListRevisionId" = "r"."Id" AND "ro"."ProviderId" = "o"."ItemProviderId""#)
        .bind(purpose).bind(list_id).bind(item_id).bind(purpose)
        .fetch_optional(&mut **tx).await.repository()?.ok_or_else(changed)?;
    validate_published_source(&decode(&context)?)?;
    Ok(context)
}

fn validate_published_source(context: &Value) -> Result<()> {
    if !context["schema_id"].is_null()
        && !context["published_revision"].is_null()
        && context["source_url"] != context["overview_source"]
    {
        return Err(RepositoryError::Conflict(
            "published enrichment requires its retained source; request prepublication details for the changed draft source".into(),
        ));
    }
    Ok(())
}

pub(crate) async fn validate(
    tx: &mut sqlx::Transaction<'_, Db>,
    list_id: i64,
    item_id: i64,
    purpose: i32,
    expected: Option<&str>,
) -> Result<()> {
    let current = capture(tx, list_id, item_id, purpose).await?;
    match expected {
        Some(expected) if expected == current => Ok(()),
        Some(expected)
            if decode(expected)?["schema_id"].is_null()
                && decode(&current)?["schema_id"].is_null() =>
        {
            Ok(())
        }
        // Preserve legacy no-schema checkpoint replay, never adopt new defaults.
        None if decode(&current)?["schema_id"].is_null() => Ok(()),
        _ => Err(changed()),
    }
}

pub(crate) async fn resolve(
    pool: &DbPool,
    list_id: i64,
    item_id: i64,
    context: &str,
) -> Result<(Option<ResolvedAddonItemOptions>, RemoteStreamLiveness)> {
    let value = decode(context)?;
    let liveness = match value["liveness"].as_i64() {
        Some(1) => RemoteStreamLiveness::Finite,
        Some(2) => RemoteStreamLiveness::Live,
        _ => RemoteStreamLiveness::Unknown,
    };
    if value["schema_id"].is_null() {
        return Ok((None, liveness));
    }
    if let Some(revision) = value["published_revision"].as_i64() {
        let (liveness, options) = load_published_item_options(
            pool,
            list_id,
            i32::try_from(revision).map_err(|_| changed())?,
            value["provider_id"].as_str().ok_or_else(changed)?,
            value["source_url"].as_str().ok_or_else(changed)?,
        )
        .await?;
        Ok((options, liveness))
    } else {
        Ok((
            load_draft_item_options(pool, list_id, item_id).await?,
            liveness,
        ))
    }
}

fn decode(context: &str) -> Result<Value> {
    serde_json::from_str(context).map_err(|_| changed())
}

fn changed() -> RepositoryError {
    RepositoryError::Conflict("the saved detail option context changed".into())
}

#[cfg(test)]
mod tests {
    use super::validate_published_source;

    #[test]
    fn retained_options_never_follow_a_changed_draft_source() {
        let mut context = serde_json::json!({
            "schema_id": 1, "published_revision": 2,
            "source_url": "https://example.test/published", "overview_source": "https://example.test/draft",
        });
        assert!(validate_published_source(&context).is_err());
        context["overview_source"] = context["source_url"].clone();
        assert!(validate_published_source(&context).is_ok());
        context["overview_source"] = serde_json::json!("https://example.test/draft");
        context["published_revision"] = serde_json::Value::Null;
        assert!(validate_published_source(&context).is_ok());
        context["published_revision"] = serde_json::json!(2);
        context["schema_id"] = serde_json::Value::Null;
        assert!(validate_published_source(&context).is_ok());
    }
}
