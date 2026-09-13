//! Strict retained-publication comparison for additions-only refresh admission.

use std::collections::BTreeMap;

use ersatzrs_core::repositories::{
    AddonMediaListAutoPublicationHoldReason as AutoPublicationHoldReason, RepositoryError, Result,
};
use ersatzrs_domain::{AddonMediaListAvailabilityReason, AddonMediaListItemAvailability, EntityId};
use sqlx::{Row, Transaction};

use super::addon_media_list_lifecycle::{
    capture_current_revision_snapshot, load_valid_revision_snapshot_in_tx,
};
use super::SqlxResultExt as _;
use crate::db::Db;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AutoPublicationAssessment {
    NoChanges,
    Eligible,
    Hold(AutoPublicationHoldReason),
}

/// Assess the exact saved draft in the caller's transaction. Eligibility still
/// requires playback preparation and the normal fenced publication lease.
#[allow(clippy::too_many_lines)] // Keep the complete strict eligibility decision in one transaction.
pub(crate) async fn assess_auto_publication_in_tx(
    tx: &mut Transaction<'_, Db>,
    list_id: EntityId,
) -> Result<AutoPublicationAssessment> {
    use AutoPublicationAssessment::{Eligible, Hold, NoChanges};
    use AutoPublicationHoldReason as Reason;

    let row = sqlx::query(
        r#"SELECT "DraftRevision", "PublishedRevision" FROM "AddonMediaList" WHERE "Id" = ?"#,
    )
    .bind(list_id)
    .fetch_optional(&mut **tx)
    .await
    .repository()?
    .ok_or(RepositoryError::NotFound {
        kind: "add-on media list",
        id: list_id,
    })?;
    let revision: i32 = row.try_get("PublishedRevision").repository()?;
    let draft_revision: i32 = row.try_get("DraftRevision").repository()?;
    if revision <= 0 {
        return Ok(Hold(Reason::BaselineUnavailable));
    }
    let baseline = match load_valid_revision_snapshot_in_tx(tx, list_id, revision).await {
        Ok(Some(snapshot)) => snapshot,
        Ok(None) | Err(RepositoryError::Decode { .. }) => {
            return Ok(Hold(Reason::BaselineUnavailable));
        }
        Err(error) => return Err(error),
    };
    let current = capture_current_revision_snapshot(
        tx,
        list_id,
        revision,
        draft_revision,
        baseline.completed_at,
        &[],
        &[],
    )
    .await?;
    if current.name != baseline.name
        || current.description != baseline.description
        || current.generate_playlist != baseline.generate_playlist
        || current.content_kind_override != baseline.content_kind_override
        || current.presentation_kind != baseline.presentation_kind
        || current.requested_presentation_kind != baseline.requested_presentation_kind
        || current.metadata_json != baseline.metadata_json
    {
        return Ok(Hold(Reason::ExistingChanges));
    }
    if current.options.schema != baseline.options.schema
        || current.options.list_values != baseline.options.list_values
    {
        return Ok(Hold(Reason::OptionsChanged));
    }
    let old_items = baseline
        .items
        .iter()
        .map(|item| (item.provider_id.as_str(), item))
        .collect::<BTreeMap<_, _>>();
    let new_items = current
        .items
        .iter()
        .map(|item| (item.provider_id.as_str(), item))
        .collect::<BTreeMap<_, _>>();
    let old_options = baseline
        .options
        .item_values
        .iter()
        .map(|item| (item.provider_id.as_str(), &item.values))
        .collect::<BTreeMap<_, _>>();
    let new_options = current
        .options
        .item_values
        .iter()
        .map(|item| (item.provider_id.as_str(), &item.values))
        .collect::<BTreeMap<_, _>>();
    let has_additions = current
        .items
        .iter()
        .any(|item| item.included && !old_items.contains_key(item.provider_id.as_str()));
    let retained_order = current
        .items
        .iter()
        .filter(|item| old_items.contains_key(item.provider_id.as_str()))
        .map(|item| item.provider_id.as_str())
        .collect::<Vec<_>>();
    if retained_order.len() == baseline.items.len()
        && retained_order
            != baseline
                .items
                .iter()
                .map(|item| item.provider_id.as_str())
                .collect::<Vec<_>>()
    {
        return Ok(Hold(Reason::ExistingChanges));
    }
    for (identity, old) in &old_items {
        let Some(new) = new_items.get(identity) else {
            return Ok(Hold(Reason::Removals));
        };
        if old.included && !new.included {
            return Ok(Hold(Reason::Removals));
        }
        // Discovery prepends/interleaves additions and renumbers retained rows.
        // Preserve retained relative order, while allowing that displacement.
        // Without additions, an absolute rank edit is still a draft change.
        let mut displaced = (*old).clone();
        if has_additions {
            displaced.rank = new.rank;
        }
        // Draft capture does not populate immutable publication playback choices.
        // Match saved-draft comparison semantics without hiding operator timing,
        // inclusion, metadata or relative-order changes.
        displaced.prepared_artifact_id = None;
        displaced.playback_timeline = None;
        let mut draft = (*new).clone();
        draft.prepared_artifact_id = None;
        draft.playback_timeline = None;
        if displaced != draft {
            return Ok(Hold(Reason::ExistingChanges));
        }
        if old_options.get(identity) != new_options.get(identity) {
            return Ok(Hold(Reason::OptionsChanged));
        }
    }
    let blockers = sqlx::query(
        r#"SELECT "item"."MetadataConflictJson", "item"."LivenessConflict",
                  "tombstone"."AddonMediaListItemId" AS "TombstoneId"
           FROM "AddonMediaListItem" AS "item"
           LEFT JOIN "AddonMediaListItemTombstone" AS "tombstone"
             ON "tombstone"."AddonMediaListItemId" = "item"."Id"
           WHERE "item"."AddonMediaListId" = ?"#,
    )
    .bind(list_id)
    .fetch_all(&mut **tx)
    .await
    .repository()?;
    for row in blockers {
        if row
            .try_get::<Option<i64>, _>("TombstoneId")
            .repository()?
            .is_some()
        {
            return Ok(Hold(Reason::Removals));
        }
        let conflict = row
            .try_get::<Option<String>, _>("MetadataConflictJson")
            .repository()?;
        if conflict
            .as_deref()
            .is_some_and(|value| !matches!(value.trim(), "" | "{}"))
            || super::common::decode_bool_column(&row, "LivenessConflict")?
        {
            return Ok(Hold(Reason::Conflicts));
        }
    }
    for item in &current.items {
        if matches!(
            item.availability_reason_code,
            Some(
                AddonMediaListAvailabilityReason::AuthenticationRequired
                    | AddonMediaListAvailabilityReason::ContentRestricted
            )
        ) {
            return Ok(Hold(Reason::Restricted));
        }
        if item.availability != AddonMediaListItemAvailability::Available
            || item.availability_reason_code.is_some()
            || item
                .availability_reason
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
        {
            return Ok(Hold(Reason::Unavailable));
        }
    }
    let additions = current
        .items
        .iter()
        .filter(|item| !old_items.contains_key(item.provider_id.as_str()))
        .collect::<Vec<_>>();
    if additions.is_empty() {
        return Ok(NoChanges);
    }
    // Previously retained review rows are not new identities. They must never
    // become incidental members of an automatic publication.
    if baseline
        .items
        .iter()
        .any(|item| item.included && !item.published)
        || additions
            .iter()
            .any(|item| !item.included || item.published)
    {
        return Ok(Hold(Reason::ExistingChanges));
    }
    Ok(Eligible)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ersatzrs_core::repositories::AddonMediaListLifecycleRepository as _;

    async fn fixture() -> sqlx::SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
        sqlx::raw_sql(r#"
            INSERT INTO "AddonInstallation" (
                "AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256",
                "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt"
            ) VALUES ('org.example.auto', '1.0.0', 1, 'package', 'content', '{}',
                      'addons/auto', '2026-09-04T00:00:00Z', '2026-09-04T00:00:00Z');
            INSERT INTO "AddonMediaList" (
                "Id", "AddonId", "ProviderId", "SourceUrl", "Name", "DraftRevision", "PublishedRevision"
            ) VALUES (1, 'org.example.auto', 'list', 'https://example.invalid/list', 'List', 1, 1);
            INSERT INTO "AddonMediaListItem" (
                "Id", "AddonMediaListId", "ProviderId", "DisplayTitle", "Title", "Kind", "Rank", "Published"
            ) VALUES (10, 1, 'old', 'Old', 'Old', 4, 0, 1);
        "#).execute(&pool).await.unwrap();
        super::super::make_repositories(pool.clone())
            .addon_media_lists
            .retain_published_revision(
                1,
                1,
                1,
                chrono::Utc::now(),
                &[],
                &ersatzrs_core::repositories::AddonMediaListRevisionArtifacts::default(),
            )
            .await
            .unwrap();
        pool
    }

    const ADDITION: &str = r#"INSERT INTO "AddonMediaListItem" (
        "Id", "AddonMediaListId", "ProviderId", "DisplayTitle", "Title", "Kind", "Rank"
        ) VALUES (11, 1, 'new', 'New', 'New', 4, 1)"#;

    #[tokio::test]
    async fn publication_playback_selections_allow_additions_but_preserve_operator_edit_holds() {
        use ersatzrs_core::{
            addon_media_list_playback::AddonMediaListPlaybackTimelinePlan,
            repositories::AddonMediaListRevisionArtifacts,
        };
        use AutoPublicationAssessment::{Eligible, Hold, NoChanges};
        use AutoPublicationHoldReason::ExistingChanges;

        for prepared_file in [false, true] {
            let pool = fixture().await;
            sqlx::raw_sql(r#"UPDATE "AddonMediaList" SET "DraftRevision"=2, "PublishedRevision"=2 WHERE "Id"=1;
                UPDATE "AddonMediaListItem" SET "ProviderLiveness"=1,"EffectiveLiveness"=1,"DurationSeconds"=20 WHERE "Id"=10;"#)
                .execute(&pool).await.unwrap();
            let mut artifacts = AddonMediaListRevisionArtifacts::default();
            if prepared_file {
                let id = sqlx::query(r#"INSERT INTO "AddonMediaListArtifact"
                    ("AddonMediaListId","AddonMediaListItemId","CacheKey","Kind","State","DurationMilliseconds","CreatedAt","UpdatedAt")
                    VALUES (1,10,'fixture-prepared',1,2,17000,'2026-09-06T00:00:00Z','2026-09-06T00:00:00Z')"#)
                    .execute(&pool).await.unwrap().last_insert_rowid();
                artifacts.ids.push(id);
                artifacts.prepared_media.insert("old".into(), id);
            } else {
                artifacts.streaming_plans.insert(
                    "old".into(),
                    AddonMediaListPlaybackTimelinePlan::new(
                        20_000,
                        0..20_000,
                        &[2_000..3_000, 4_000..6_000],
                        "a".repeat(64),
                    )
                    .unwrap(),
                );
            }
            super::super::make_repositories(pool.clone())
                .addon_media_lists
                .retain_published_revision(1, 2, 2, chrono::Utc::now(), &[], &artifacts)
                .await
                .unwrap();
            for (add, mutation, expected) in [
                (false, "SELECT 1", NoChanges),
                (true, "SELECT 1", Eligible),
                (
                    true,
                    r#"UPDATE "AddonMediaListItem" SET "Title"='Edited' WHERE "Id"=10"#,
                    Hold(ExistingChanges),
                ),
                (
                    true,
                    r#"UPDATE "AddonMediaListItem" SET "DurationSeconds"=21 WHERE "Id"=10"#,
                    Hold(ExistingChanges),
                ),
                (
                    true,
                    r#"UPDATE "AddonMediaListItem" SET "FragmentStartSeconds"=1 WHERE "Id"=10"#,
                    Hold(ExistingChanges),
                ),
                (
                    true,
                    r#"UPDATE "AddonMediaListItem" SET "FragmentEndSeconds"=19 WHERE "Id"=10"#,
                    Hold(ExistingChanges),
                ),
            ] {
                let mut tx = pool.begin().await.unwrap();
                if add {
                    sqlx::query(ADDITION).execute(&mut *tx).await.unwrap();
                }
                sqlx::query(mutation).execute(&mut *tx).await.unwrap();
                assert_eq!(
                    assess_auto_publication_in_tx(&mut tx, 1).await.unwrap(),
                    expected,
                    "prepared_file={prepared_file}, addition={add}, mutation={mutation}"
                );
                tx.rollback().await.unwrap();
            }
        }
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)] // Table-driven mutations share one real retained-snapshot fixture.
    async fn auto_eligibility_requires_strict_pure_additions() {
        use AutoPublicationAssessment::{Eligible, Hold, NoChanges};
        use AutoPublicationHoldReason as R;

        let pool = fixture().await;
        let cases = [
            ("unchanged", false, "SELECT 1", NoChanges),
            (
                "revision counter alone",
                false,
                r#"UPDATE "AddonMediaList" SET "DraftRevision" = 2"#,
                NoChanges,
            ),
            ("pure addition", true, "SELECT 1", Eligible),
            (
                "initial publication",
                true,
                r#"UPDATE "AddonMediaList" SET "PublishedRevision" = 0"#,
                Hold(R::BaselineUnavailable),
            ),
            (
                "missing baseline",
                true,
                r#"DELETE FROM "AddonMediaListRevision""#,
                Hold(R::BaselineUnavailable),
            ),
            (
                "corrupt baseline",
                true,
                r#"UPDATE "AddonMediaListRevision" SET "SnapshotDigest" = 'bad'"#,
                Hold(R::BaselineUnavailable),
            ),
            (
                "list name",
                true,
                r#"UPDATE "AddonMediaList" SET "Name" = 'Changed'"#,
                Hold(R::ExistingChanges),
            ),
            (
                "list metadata",
                true,
                r#"UPDATE "AddonMediaList" SET "MetadataOverrideJson" = '{"title":"Changed"}'"#,
                Hold(R::ExistingChanges),
            ),
            (
                "playlist setting",
                true,
                r#"UPDATE "AddonMediaList" SET "GeneratePlaylist" = 1 - "GeneratePlaylist""#,
                Hold(R::ExistingChanges),
            ),
            (
                "title",
                true,
                r#"UPDATE "AddonMediaListItem" SET "Title" = 'Changed' WHERE "Id" = 10"#,
                Hold(R::ExistingChanges),
            ),
            (
                "addition displaces absolute rank",
                true,
                r#"UPDATE "AddonMediaListItem" SET "Rank" = 2 WHERE "Id" = 10"#,
                Eligible,
            ),
            (
                "rank edit without additions",
                false,
                r#"UPDATE "AddonMediaListItem" SET "Rank" = 2 WHERE "Id" = 10"#,
                Hold(R::ExistingChanges),
            ),
            (
                "liveness",
                true,
                r#"UPDATE "AddonMediaListItem" SET "EffectiveLiveness" = 'live' WHERE "Id" = 10"#,
                Hold(R::ExistingChanges),
            ),
            (
                "chapter",
                true,
                r#"UPDATE "AddonMediaListItem" SET "ChapterInput" = '00:00 Intro' WHERE "Id" = 10"#,
                Hold(R::ExistingChanges),
            ),
            (
                "provider chapter",
                true,
                r#"UPDATE "AddonMediaListItem" SET "ProviderChapterInput" = '00:00 Intro' WHERE "Id" = 10"#,
                Hold(R::ExistingChanges),
            ),
            (
                "guid",
                true,
                r#"INSERT INTO "AddonMediaListItemGuid" ("AddonMediaListItemId", "Guid") VALUES (10, 'guid://changed')"#,
                Hold(R::ExistingChanges),
            ),
            (
                "excluded existing",
                true,
                r#"UPDATE "AddonMediaListItem" SET "Included" = 0 WHERE "Id" = 10"#,
                Hold(R::Removals),
            ),
            (
                "deleted existing",
                true,
                r#"DELETE FROM "AddonMediaListItem" WHERE "Id" = 10"#,
                Hold(R::Removals),
            ),
            (
                "excluded addition",
                true,
                r#"UPDATE "AddonMediaListItem" SET "Included" = 0 WHERE "Id" = 11"#,
                Hold(R::ExistingChanges),
            ),
            (
                "unknown addition",
                true,
                r#"UPDATE "AddonMediaListItem" SET "Availability" = 'unknown' WHERE "Id" = 11"#,
                Hold(R::Unavailable),
            ),
            (
                "unavailable addition",
                true,
                r#"UPDATE "AddonMediaListItem" SET "Availability" = 'unavailable' WHERE "Id" = 11"#,
                Hold(R::Unavailable),
            ),
            (
                "restricted addition",
                true,
                r#"UPDATE "AddonMediaListItem" SET "AvailabilityReasonCode" = 'content_restricted' WHERE "Id" = 11"#,
                Hold(R::Restricted),
            ),
            (
                "authentication",
                true,
                r#"UPDATE "AddonMediaListItem" SET "AvailabilityReasonCode" = 'authentication_required' WHERE "Id" = 11"#,
                Hold(R::Restricted),
            ),
            (
                "metadata conflict",
                true,
                r#"UPDATE "AddonMediaListItem" SET "MetadataConflictJson" = '{"title":true}' WHERE "Id" = 10"#,
                Hold(R::Conflicts),
            ),
            (
                "liveness conflict",
                true,
                r#"UPDATE "AddonMediaListItem" SET "LivenessConflict" = 1 WHERE "Id" = 10"#,
                Hold(R::Conflicts),
            ),
            (
                "replacement",
                true,
                r#"INSERT INTO "AddonMediaListItemReplacement" ("AddonMediaListItemId", "MediaItemId", "MediaVersionId", "CreatedAt", "UpdatedAt") VALUES (10, 100, 101, '2026-09-04T00:00:00Z', '2026-09-04T00:00:00Z')"#,
                Hold(R::ExistingChanges),
            ),
            (
                "schema",
                true,
                r#"INSERT INTO "AddonMediaListOptionSchema" ("Id", "AddonId", "CapabilityId", "SchemaDigest", "DefinitionJson", "CreatedAt") VALUES (1, 'org.example.auto', 'options', 'digest', '{}', '2026-09-04T00:00:00Z'); UPDATE "AddonMediaList" SET "DraftOptionSchemaId" = 1"#,
                Hold(R::OptionsChanged),
            ),
            (
                "list options",
                true,
                r#"UPDATE "AddonMediaList" SET "DraftOptionValuesJson" = '{"enabled":true}'"#,
                Hold(R::OptionsChanged),
            ),
            (
                "item options",
                true,
                r#"UPDATE "AddonMediaListItem" SET "DraftOptionOverridesJson" = '{"enabled":true}' WHERE "Id" = 10"#,
                Hold(R::OptionsChanged),
            ),
            (
                "tombstone",
                true,
                r#"INSERT INTO "AddonMediaListItemTombstone" ("AddonMediaListItemId", "MissingSince", "LastObservedAt", "ObservationCount", "LastReconcileGeneration") VALUES (10, '2026-09-04T00:00:00Z', '2026-09-04T00:00:00Z', 1, 1)"#,
                Hold(R::Removals),
            ),
        ];
        for (name, add, mutation, expected) in cases {
            let mut tx = pool.begin().await.unwrap();
            if add {
                sqlx::query(ADDITION).execute(&mut *tx).await.unwrap();
            }
            sqlx::raw_sql(mutation).execute(&mut *tx).await.unwrap();
            assert_eq!(
                assess_auto_publication_in_tx(&mut tx, 1).await.unwrap(),
                expected,
                "{name}"
            );
            tx.rollback().await.unwrap();
        }
    }

    #[tokio::test]
    async fn auto_eligibility_allows_addition_displacement_but_not_retained_reordering() {
        let pool = fixture().await;
        sqlx::query(ADDITION).execute(&pool).await.unwrap();
        sqlx::raw_sql(
            r#"UPDATE "AddonMediaListItem" SET "Published" = 1;
            UPDATE "AddonMediaList" SET "DraftRevision" = 2, "PublishedRevision" = 2"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        super::super::make_repositories(pool.clone())
            .addon_media_lists
            .retain_published_revision(
                1,
                2,
                2,
                chrono::Utc::now(),
                &[],
                &ersatzrs_core::repositories::AddonMediaListRevisionArtifacts::default(),
            )
            .await
            .unwrap();
        for (name, first_rank, second_rank, added_rank, expected) in [
            ("prepend", 1, 2, 0, AutoPublicationAssessment::Eligible),
            ("interleave", 0, 2, 1, AutoPublicationAssessment::Eligible),
            (
                "reversed retained",
                2,
                0,
                1,
                AutoPublicationAssessment::Hold(AutoPublicationHoldReason::ExistingChanges),
            ),
        ] {
            let mut tx = pool.begin().await.unwrap();
            sqlx::query(
                r#"UPDATE "AddonMediaListItem" SET "Rank" = CASE "Id" WHEN 10 THEN ? ELSE ? END"#,
            )
            .bind(first_rank)
            .bind(second_rank)
            .execute(&mut *tx)
            .await
            .unwrap();
            sqlx::query(r#"INSERT INTO "AddonMediaListItem" ("Id", "AddonMediaListId", "ProviderId", "DisplayTitle", "Title", "Kind", "Rank")
                VALUES (12, 1, 'newer', 'Newer', 'Newer', 4, ?)"#)
                .bind(added_rank).execute(&mut *tx).await.unwrap();
            assert_eq!(
                assess_auto_publication_in_tx(&mut tx, 1).await.unwrap(),
                expected,
                "{name}"
            );
            tx.rollback().await.unwrap();
        }
    }

    #[tokio::test]
    async fn auto_eligibility_never_admits_retained_unpublished_rows() {
        let pool = fixture().await;
        sqlx::query(ADDITION).execute(&pool).await.unwrap();
        sqlx::query(r#"UPDATE "AddonMediaList" SET "DraftRevision" = 2, "PublishedRevision" = 2"#)
            .execute(&pool)
            .await
            .unwrap();
        super::super::make_repositories(pool.clone())
            .addon_media_lists
            .retain_published_revision(
                1,
                2,
                2,
                chrono::Utc::now(),
                &[],
                &ersatzrs_core::repositories::AddonMediaListRevisionArtifacts::default(),
            )
            .await
            .unwrap();
        let mut tx = pool.begin().await.unwrap();
        sqlx::query(
            r#"INSERT INTO "AddonMediaListItem" (
            "Id", "AddonMediaListId", "ProviderId", "DisplayTitle", "Title", "Kind", "Rank"
            ) VALUES (12, 1, 'newer', 'Newer', 'Newer', 4, 2)"#,
        )
        .execute(&mut *tx)
        .await
        .unwrap();
        assert_eq!(
            assess_auto_publication_in_tx(&mut tx, 1).await.unwrap(),
            AutoPublicationAssessment::Hold(AutoPublicationHoldReason::ExistingChanges)
        );
    }
}
