//! ADR 0091: all operator values in a task share one `SQLite` write transaction.
use super::*;
use crate::repositories::common::encode_enum;
use ersatzrs_core::repositories::{AddonMediaListEditorItem, AddonMediaListEditorSave};

fn conflict(message: &str) -> RepositoryError {
    RepositoryError::Conflict(message.to_owned())
}

/// The same transaction snapshot is used at display and again under the save lock.
pub(super) async fn playback_baseline(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    list_id: EntityId,
) -> Result<(
    i32,
    ersatzrs_core::repositories::AddonMediaListPlaybackBaseline,
)> {
    use sha2::{Digest as _, Sha256};
    use std::fmt::Write as _;
    let row = sqlx::query(
        r#"SELECT "list"."AddonId", "list"."DeliveryMode",
        "list"."DeliveryCutoffAt", "list"."DeliveryCutoffIdentityId", "list"."DiskUsageWarningGb",
        "addon"."ManifestJson", "addon"."PackageSha256", "addon"."ContentSha256" FROM "AddonMediaList" AS "list"
        JOIN "AddonInstallation" AS "addon" ON "addon"."AddonId" = "list"."AddonId"
        WHERE "list"."Id" = ? AND "addon"."Enabled" = 1"#,
    )
    .bind(list_id)
    .fetch_optional(&mut **tx)
    .await
    .repository()?
    .ok_or_else(|| conflict("the installed enabled add-on is unavailable"))?;
    let options =
        super::super::addon_media_list_lifecycle::load_draft_options_in_tx(tx, list_id, None)
            .await?
            .ok_or_else(|| conflict("the saved draft is unavailable"))?;
    let manifest: String = row.try_get("ManifestJson").repository()?;
    let identity = serde_json::to_vec(&(
        manifest,
        row.try_get::<String, _>("PackageSha256").repository()?,
        row.try_get::<String, _>("ContentSha256").repository()?,
    ))
    .map_err(|_| conflict("invalid installed identity"))?;
    let installation_digest =
        Sha256::digest(&identity)
            .iter()
            .fold(String::with_capacity(64), |mut output, byte| {
                let _ = write!(output, "{byte:02x}");
                output
            });
    Ok((
        options.draft_revision,
        ersatzrs_core::repositories::AddonMediaListPlaybackBaseline {
            addon_id: row.try_get("AddonId").repository()?,
            installation_digest,
            delivery_mode: crate::repositories::common::decode_enum_column(
                "delivery mode",
                &row,
                "DeliveryMode",
            )?,
            cutoff_at: row.try_get("DeliveryCutoffAt").repository()?,
            cutoff_identity_id: row.try_get("DeliveryCutoffIdentityId").repository()?,
            disk_warning_gb: Some(
                u32::try_from(row.try_get::<i64, _>("DiskUsageWarningGb").repository()?)
                    .map_err(|_| conflict("invalid disk warning threshold"))?,
            ),
            schema: options.schema,
            list_values: options.list_values,
        },
    ))
}

#[expect(
    clippy::too_many_lines,
    reason = "The named editing task and durable reconciliation commit in one transaction"
)]
pub(super) async fn save(
    pool: &Pool<crate::db::Db>,
    save: &AddonMediaListEditorSave,
) -> Result<i32> {
    if save.list_id <= 0
        || save.expected_revision < 0
        || save.expected_revision == i32::MAX
        || save.items.len() > 10_000
    {
        return Err(conflict("invalid editing task scope"));
    }
    let repository = SqliteAddonMediaListRepository(pool.clone());
    let current =
        repository
            .get_admin_form(save.list_id)
            .await?
            .ok_or(RepositoryError::NotFound {
                kind: "add-on media list",
                id: save.list_id,
            })?;
    if let Some(mode) = save
        .delivery_mode
        .filter(|mode| *mode != current.delivery_mode)
    {
        require_delivery_capability(pool, &current.addon_id, mode).await?;
    }
    if let Some(settings) = &save.settings {
        if !settings.presentation_kind.is_projected() {
            return Err(conflict("the requested presentation is not supported"));
        }
        if settings.refresh_cadence != current.refresh_cadence
            && settings.refresh_cadence == AddonMediaListRefreshCadence::Adaptive
            && !InstalledAddonMediaListProvider::new(pool.clone())
                .supports_source_check(&current.addon_id)
                .await?
        {
            return Err(conflict("the add-on does not support adaptive refresh"));
        }
    }
    let changes_draft = save.metadata.is_some()
        || save.settings.is_some()
        || !save.items.is_empty()
        || save.options.is_some()
        || save
            .delivery_mode
            .is_some_and(|mode| mode != current.delivery_mode);
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    if changes_draft {
        crate::addon_media_list_sync::ensure_not_publishing_tx(&mut tx, save.list_id).await?;
    }
    let revision: i32 =
        sqlx::query_scalar(r#"SELECT "DraftRevision" FROM "AddonMediaList" WHERE "Id" = ?"#)
            .bind(save.list_id)
            .fetch_one(&mut *tx)
            .await
            .repository()?;
    if revision == i32::MAX {
        return Err(conflict("the saved revision limit has been reached"));
    }
    if let Some(expected) = &save.playback_baseline {
        if save.metadata.is_some()
            || save.settings.is_some()
            || !save.items.is_empty()
            || save.delivery_mode.is_none()
        {
            return Err(conflict("playback baseline requires a playback-only task"));
        }
        crate::addon_media_list_sync::ensure_not_publishing_tx(&mut tx, save.list_id).await?;
        let (_, persisted) = playback_baseline(&mut tx, save.list_id).await?;
        if expected != &persisted {
            return Err(conflict(
                "playback settings changed; review the current revision",
            ));
        }
        if save
            .delivery_mode
            .is_some_and(|mode| mode != ersatzrs_domain::AddonMediaListDeliveryMode::Stream)
        {
            #[derive(serde::Deserialize)]
            struct Declaration {
                #[serde(default)]
                capabilities: Vec<ersatzrs_addon_contract::AddonCapability>,
            }
            let manifest: String = sqlx::query_scalar(r#"SELECT "ManifestJson" FROM "AddonInstallation" WHERE "AddonId" = ? AND "Enabled" = 1"#)
                .bind(&persisted.addon_id).fetch_one(&mut *tx).await.repository()?;
            let declaration: Declaration = serde_json::from_str(&manifest)
                .map_err(|_| conflict("invalid installed capability declaration"))?;
            for required in [
                ersatzrs_addon_contract::CAPABILITY_MEDIA_LIST_PREPARE_V2,
                ersatzrs_addon_contract::CAPABILITY_MEDIA_LIST_ITEM_RUNTIME_V1,
            ] {
                if !declaration
                    .capabilities
                    .iter()
                    .any(|capability| capability.id == required)
                {
                    return Err(conflict(
                        "the installed add-on does not support this delivery mode",
                    ));
                }
            }
        }
        // Preparation used this revision. A second change must retry validation;
        // never relabel a prepared option snapshot with the newest revision.
        if save
            .options
            .as_ref()
            .is_some_and(|options| options.draft_revision != revision)
        {
            return Err(conflict(
                "option preparation changed; review the current revision",
            ));
        }
    } else if revision != save.expected_revision || current.draft_revision != revision {
        return Err(conflict(
            "the saved draft changed; review the current revision",
        ));
    }
    if let Some(metadata) = &save.metadata {
        save_metadata(&mut tx, save.list_id, metadata).await?;
    }
    if let Some(settings) = &save.settings {
        save_settings(&mut tx, save.list_id, settings, &current).await?;
    }
    if let Some(mode) = save.delivery_mode {
        crate::addon_media_list_delivery::save_in_tx(&mut tx, save.list_id, mode, Utc::now())
            .await?;
    }
    if let Some(warning) = save.disk_warning_gb {
        sqlx::query(r#"UPDATE "AddonMediaList" SET "DiskUsageWarningGb" = ? WHERE "Id" = ?"#)
            .bind(i64::from(warning))
            .bind(save.list_id)
            .execute(&mut *tx)
            .await
            .repository()?;
    }
    let mut seen = std::collections::BTreeSet::new();
    for item in &save.items {
        if !seen.insert(item.review.item_id) {
            return Err(conflict("duplicate item in editing task"));
        }
        save_item(&mut tx, save.list_id, item).await?;
    }
    if let Some(options) = &save.options {
        if options.list_id != save.list_id || options.draft_revision != revision {
            return Err(conflict("option scope or saved revision changed"));
        }
        super::super::addon_media_list_lifecycle::save_draft_options_tx(&mut tx, options, false)
            .await?;
    }
    if changes_draft {
        mark_list_ready_tx(&mut tx, save.list_id).await?;
    }
    // Preserve pending chapter work when another task changes its draft.
    if changes_draft
        && (save.items.iter().any(|item| item.chapters.is_some())
            || pending_reconcile(&mut tx, save.list_id).await?)
    {
        crate::repositories::managed_library_operation::enqueue_reconcile_tx(&mut tx, save.list_id)
            .await?;
    }
    tx.commit().await.repository()?;
    Ok(revision + i32::from(changes_draft))
}

async fn pending_reconcile(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    list_id: EntityId,
) -> Result<bool> {
    use ersatzrs_domain::{ManagedLibraryOperationKind, ManagedLibraryOperationState};
    sqlx::query_scalar(
        r#"SELECT EXISTS (
        SELECT 1 FROM "AddonMediaListLifecycleJob" AS "job"
        JOIN "ManagedLibraryOperation" AS "operation" ON "operation"."Id"="job"."OperationId"
        WHERE "job"."AddonMediaListId"=? AND "operation"."Kind"=?
          AND "operation"."State" IN (?, ?, ?))"#,
    )
    .bind(list_id)
    .bind(encode_enum(ManagedLibraryOperationKind::Reconcile))
    .bind(encode_enum(ManagedLibraryOperationState::Queued))
    .bind(encode_enum(ManagedLibraryOperationState::Running))
    .bind(encode_enum(ManagedLibraryOperationState::WaitingToRetry))
    .fetch_one(&mut **tx)
    .await
    .repository()
}

async fn save_item(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    list_id: EntityId,
    item: &AddonMediaListEditorItem,
) -> Result<()> {
    let edit = &item.review;
    if !matches!((edit.season, edit.episode), (None, None))
        && !matches!((edit.season, edit.episode), (Some(season), Some(episode)) if season >= 0 && episode >= 0)
    {
        return Err(conflict(
            "season and episode must be a non-negative pair or both unnumbered",
        ));
    }
    let row = sqlx::query(
        r#"SELECT "ProviderMetadataJson", "MetadataOverrideJson", "MetadataConflictJson",
        "ParentProviderId", "DurationSeconds", "ContentKind", "Included", "LivenessOverride"
        FROM "AddonMediaListItem" WHERE "Id" = ? AND "AddonMediaListId" = ?"#,
    )
    .bind(edit.item_id)
    .bind(list_id)
    .fetch_optional(&mut **tx)
    .await
    .repository()?
    .ok_or(RepositoryError::NotFound {
        kind: "add-on media list item",
        id: edit.item_id,
    })?;
    let provider: String = row.try_get("ProviderMetadataJson").repository()?;
    let overrides: String = row.try_get("MetadataOverrideJson").repository()?;
    let conflicts: Option<String> = row.try_get("MetadataConflictJson").repository()?;
    let mut form = metadata_form_from_json(&provider, &overrides, conflicts.as_deref())?;
    let previous = AddonMediaListItemReviewEdit {
        item_id: edit.item_id,
        content_kind: crate::repositories::common::decode_enum_column(
            "AddonMediaListItem.ContentKind",
            &row,
            "ContentKind",
        )?,
        included: decode_bool_column(&row, "Included")?,
        liveness_override: decode_optional_enum_column(
            "AddonMediaListItem.LivenessOverride",
            &row,
            "LivenessOverride",
        )?,
        season: form.metadata.season,
        episode: form.metadata.episode,
    };
    if item
        .expected_review
        .is_some_and(|expected| expected != previous)
    {
        return Err(conflict("the item changed while another task was saved"));
    }
    if let Some(metadata) = &item.metadata {
        if metadata.etag != form.etag
            || (metadata.metadata.title != form.metadata.title
                && metadata.metadata.title.trim().is_empty())
        {
            return Err(conflict(
                "item metadata is invalid or changed since loading",
            ));
        }
        if metadata.metadata.artwork != form.metadata.artwork
            || metadata.metadata.artwork_urls != form.metadata.artwork_urls
        {
            super::editor_artwork::validate(tx, list_id, Some(edit.item_id), &metadata.metadata)
                .await?;
        }
        form.metadata = metadata.metadata.clone();
        form.conflicts.clear();
    }
    form.metadata.season = edit.season;
    form.metadata.episode = edit.episode;
    form.conflicts.remove("season");
    form.conflicts.remove("episode");
    if let Some(chapters) = &item.chapters {
        validate_chapters(&row, chapters)?;
    }
    if let Some(replacement) = item.replacement {
        save_replacement(tx, edit.item_id, replacement).await?;
    }
    let overrides = metadata_override_json(&metadata_json_value(&provider)?, &form.metadata)?;
    let conflicts = serde_json::to_string(&form.conflicts)
        .map_err(|_| conflict("cannot encode metadata conflicts"))?;
    let liveness = edit.liveness_override.map(encode_enum);
    sqlx::query(r#"UPDATE "AddonMediaListItem" SET "ContentKind" = ?, "Included" = ?,
        "IncludedByUser" = CASE WHEN "Included" <> ? THEN 1 ELSE "IncludedByUser" END,
        "CollectionRole" = COALESCE(?, "CollectionRole"),
        "ChapterInput" = CASE WHEN ? THEN ? ELSE "ChapterInput" END,
        "LivenessOverride" = CASE WHEN "Kind" = 4 THEN ? ELSE "LivenessOverride" END,
        "EffectiveLiveness" = CASE WHEN "Kind" = 4 THEN COALESCE(?, "ProviderLiveness") ELSE "EffectiveLiveness" END,
        "LivenessConflict" = CASE WHEN "Kind" = 4 AND ? THEN 0 ELSE "LivenessConflict" END,
        "MetadataOverrideJson" = ?, "MetadataConflictJson" = ? WHERE "Id" = ? AND "AddonMediaListId" = ?"#)
        .bind(encode_enum(edit.content_kind)).bind(edit.included).bind(edit.included).bind(item.role.map(encode_enum))
        .bind(item.chapters.is_some()).bind(item.chapters.as_deref().map(str::trim).filter(|s| !s.is_empty()))
        .bind(liveness).bind(liveness).bind(edit.liveness_override != previous.liveness_override).bind(overrides).bind(conflicts)
        .bind(edit.item_id).bind(list_id).execute(&mut **tx).await.repository()?;
    Ok(())
}

async fn save_metadata(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    list_id: EntityId,
    metadata: &ersatzrs_core::repositories::AddonMediaListEditorMetadata,
) -> Result<()> {
    let row = sqlx::query(r#"SELECT "ProviderMetadataJson", "MetadataOverrideJson" FROM "AddonMediaList" WHERE "Id" = ?"#)
            .bind(list_id).fetch_one(&mut **tx).await.repository()?;
    let provider: String = row.try_get("ProviderMetadataJson").repository()?;
    let overrides: String = row.try_get("MetadataOverrideJson").repository()?;
    let current = metadata_form_from_json(&provider, &overrides, None)?;
    if current.etag != metadata.etag
        || (metadata.metadata.title != current.metadata.title
            && metadata.metadata.title.trim().is_empty())
    {
        return Err(conflict(
            "list metadata is invalid or changed since loading",
        ));
    }
    if metadata.metadata.artwork != current.metadata.artwork
        || metadata.metadata.artwork_urls != current.metadata.artwork_urls
    {
        super::editor_artwork::validate(tx, list_id, None, &metadata.metadata).await?;
    }
    let overrides = metadata_override_json(&metadata_json_value(&provider)?, &metadata.metadata)?;
    sqlx::query(r#"UPDATE "AddonMediaList" SET "MetadataOverrideJson" = ?, "MetadataConflictJson" = NULL WHERE "Id" = ?"#)
            .bind(overrides).bind(list_id).execute(&mut **tx).await.repository()?;
    Ok(())
}

async fn save_settings(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    list_id: EntityId,
    settings: &ersatzrs_core::repositories::AddonMediaListEditorSettings,
    current: &AddonMediaListAdminForm,
) -> Result<()> {
    let changed = settings.refresh_cadence != current.refresh_cadence;
    let now = Utc::now();
    let due = if changed {
        match settings.refresh_cadence {
            AddonMediaListRefreshCadence::Never => None,
            AddonMediaListRefreshCadence::Adaptive => Some(encode_datetime(now)),
            cadence => Some(encode_datetime(
                current
                    .last_refresh_succeeded_at
                    .as_deref()
                    .map(parse_admin_datetime)
                    .transpose()?
                    .and_then(|anchor| {
                        crate::addon_media_list_refresh::next_fixed_refresh_at(cadence, anchor)
                    })
                    .unwrap_or(now),
            )),
        }
    } else {
        current.next_refresh_at.clone()
    };
    sqlx::query(
        r#"UPDATE "AddonMediaList" SET "AutoRefresh" = ?, "RefreshCadence" = ?,
            "RefreshPublicationPolicy" = ?, "NextRefreshAt" = ?,
            "RefreshRetryAt" = CASE WHEN ? THEN NULL ELSE "RefreshRetryAt" END,
            "GeneratePlaylist" = ?, "DetailImportPolicy" = ?, "RequestedPresentationKind" = ?,
            "PresentationError" = NULL WHERE "Id" = ?"#,
    )
    .bind(settings.refresh_cadence == AddonMediaListRefreshCadence::Daily)
    .bind(encode_enum(settings.refresh_cadence))
    .bind(encode_enum(
        ersatzrs_domain::AddonMediaListRefreshPublicationPolicy::Automatic,
    ))
    .bind(&due)
    .bind(changed)
    .bind(settings.generate_playlist)
    .bind(encode_enum(settings.detail_policy))
    .bind(encode_enum(settings.presentation_kind))
    .bind(list_id)
    .execute(&mut **tx)
    .await
    .repository()?;
    if changed {
        sqlx::query(r#"UPDATE "AddonMediaListRefreshState" SET "State" = ?, "MissCount" = 0,
                "AttemptCount" = 0, "LeaseOwner" = NULL, "LeaseExpiresAt" = NULL,
                "NextEligibleAt" = ?, "DiagnosticCode" = NULL, "UpdatedAt" = ? WHERE "AddonMediaListId" = ?"#)
                .bind(encode_enum(ersatzrs_domain::AddonMediaListRefreshState::Idle)).bind(due)
                .bind(encode_datetime(now)).bind(list_id).execute(&mut **tx).await.repository()?;
    }
    Ok(())
}

async fn save_replacement(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    item_id: EntityId,
    replacement: Option<(EntityId, EntityId)>,
) -> Result<()> {
    if let Some((media, version)) = replacement {
        if sqlx::query(replacement_candidate_query(false))
            .bind(media)
            .bind(version)
            .fetch_optional(&mut **tx)
            .await
            .repository()?
            .is_none()
        {
            return Err(conflict(
                "replacement must be an available Local video version",
            ));
        }
        let now = encode_datetime(Utc::now());
        sqlx::query(r#"INSERT INTO "AddonMediaListItemReplacement" ("AddonMediaListItemId","MediaItemId","MediaVersionId","CreatedAt","UpdatedAt") VALUES (?,?,?,?,?)
                ON CONFLICT("AddonMediaListItemId") DO UPDATE SET "MediaItemId"=excluded."MediaItemId", "MediaVersionId"=excluded."MediaVersionId", "UpdatedAt"=excluded."UpdatedAt""#)
                .bind(item_id).bind(media).bind(version).bind(&now).bind(&now).execute(&mut **tx).await.repository()?;
    } else {
        sqlx::query(
            r#"DELETE FROM "AddonMediaListItemReplacement" WHERE "AddonMediaListItemId"=?"#,
        )
        .bind(item_id)
        .execute(&mut **tx)
        .await
        .repository()?;
    }
    Ok(())
}

fn validate_chapters(row: &sqlx::sqlite::SqliteRow, chapters: &str) -> Result<()> {
    if row
        .try_get::<Option<String>, _>("ParentProviderId")
        .repository()?
        .is_some()
    {
        return Err(conflict(
            "chapters can only be edited on a full source item",
        ));
    }
    ersatzrs_core::media_chapters::parse_chapter_input(
        chapters,
        row.try_get::<Option<i64>, _>("DurationSeconds")
            .repository()?
            .and_then(|n| u64::try_from(n).ok()),
    )
    .map_err(|_| conflict("invalid chapter times or ordering"))?;
    Ok(())
}
