//! Discover receiving saved folders without publishing their newer drafts.
//!
//! Not yet installed in the runtime maintenance loop: file-first Save/replay and
//! the remaining local playback bindings must pass O-01 acceptance first.

use std::path::{Path, PathBuf};

use chrono::Utc;
use ersatzrs_addon_contract::AddonManifest;
use ersatzrs_core::repositories::{AddonMediaListPublicationEnqueue, RepositoryError, Result};
use sqlx::Row as _;

use super::model::SAVED_LIST_FILE_NAME;
use super::receiving_artwork::ArtworkStaging;
use crate::db::DbPool;
use crate::repositories::SqlxResultExt as _;

/// Registered jobs, repaired projections and deferred/refused folders this pass.
#[derive(Default)]
pub struct DiscoveryReport {
    pub admitted: Vec<AddonMediaListPublicationEnqueue>,
    pub repaired: usize,
    pub deferred: usize,
    pub refused: usize,
}

/// Discover and reconcile direct child folders under enabled installed add-on roots.
///
/// No provider enumeration, add-on installation, copied execution ownership or
/// saved-file replacement occurs. Changed mapped folders repair their local
/// projection unless a managed operation is active. Unsupported receiving media
/// bindings are refused, never converted to streaming. This entry point is under
/// integration test and is not yet called by runtime maintenance.
///
/// # Errors
/// Database/root enumeration failures abort the pass. Invalid individual copies
/// are counted and logged with redacted diagnostics without committing owners.
pub async fn discover_copied_lists(pool: &DbPool) -> Result<DiscoveryReport> {
    let roots = sqlx::query(
        r#"SELECT "source"."AddonId", "source"."RootPath", "installation"."ManifestJson"
           FROM "AddonMediaSource" AS "source"
           JOIN "AddonInstallation" AS "installation" ON "installation"."AddonId"="source"."AddonId"
           WHERE "installation"."Enabled"=1 ORDER BY "source"."AddonId""#,
    )
    .fetch_all(pool)
    .await
    .repository()?;
    let mut report = DiscoveryReport::default();
    for row in roots {
        let addon_id: String = row.try_get("AddonId").repository()?;
        let manifest_json: String = row.try_get("ManifestJson").repository()?;
        let installed: AddonManifest =
            serde_json::from_str(&manifest_json).map_err(|_| invalid())?;
        let root = PathBuf::from(row.try_get::<String, _>("RootPath").repository()?);
        let mut entries = tokio::fs::read_dir(&root).await.map_err(|_| invalid())?;
        while let Some(entry) = entries.next_entry().await.map_err(|_| invalid())? {
            // Publication backups and other reserved host trees are not copied lists.
            if crate::repositories::addon_publication_fs::is_reserved_entry(&entry.file_name()) {
                continue;
            }
            let folder = entry.path();
            if !entry.file_type().await.map_err(|_| invalid())?.is_dir() {
                continue;
            }
            match tokio::fs::symlink_metadata(folder.join(SAVED_LIST_FILE_NAME)).await {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(_) => {
                    report.refused += 1;
                    continue;
                }
                Ok(_) => {}
            }
            let mapped: Option<i64> = sqlx::query_scalar(
                r#"SELECT "AddonMediaListId" FROM "AddonMediaListLibrary"
                   WHERE LOWER("ManifestPath")=LOWER(?)"#,
            )
            .bind(folder.join("list.playlist.yml").to_string_lossy().as_ref())
            .fetch_optional(pool)
            .await
            .repository()?;
            if let Some(list_id) = mapped {
                match super::mapped::refresh(pool, list_id).await {
                    Ok(super::mapped::Repair::Unchanged) => {}
                    Ok(super::mapped::Repair::Deferred) => report.deferred += 1,
                    Ok(super::mapped::Repair::Updated(queued)) => {
                        report.repaired += 1;
                        report.admitted.extend(queued.map(|job| *job));
                    }
                    Err(error) => {
                        report.refused += 1;
                        tracing::warn!(list_id, reason = %error, "saved add-on list projection refresh refused");
                    }
                }
                continue;
            }
            match admit_folder(pool, &folder, &addon_id, &installed, &manifest_json).await {
                Ok(Some(job)) => report.admitted.push(job),
                Ok(None) => {}
                Err(error) => {
                    report.refused += 1;
                    // Do not expose copied paths, text or provider diagnostics.
                    tracing::warn!(addon_id, reason = %error, "saved add-on list folder admission refused");
                }
            }
        }
    }
    Ok(report)
}

#[expect(
    clippy::too_many_lines,
    reason = "Keep guarded file validation followed by one receiving transaction in order"
)]
async fn admit_folder(
    pool: &DbPool,
    folder: &Path,
    addon_id: &str,
    installed: &AddonManifest,
    manifest_json: &str,
) -> Result<Option<AddonMediaListPublicationEnqueue>> {
    let copied_folder = folder.to_owned();
    let (tree, document, digest, publication_digest) = tokio::task::spawn_blocking(move || {
        let metadata = std::fs::symlink_metadata(&copied_folder).map_err(|_| invalid())?;
        if metadata.is_symlink() || !metadata.is_dir() {
            return Err(invalid());
        }
        let tree = super::assembly::lock_managed_tree(&copied_folder)?;
        let (document, digest) = super::document_store::read(&tree)?;
        let publication_digest =
            super::document_store::publication_digest(document.published.as_ref())?;
        Ok((tree, document, digest, publication_digest))
    })
    .await
    .map_err(|_| invalid())??;
    if document.addon_id != addon_id {
        return Err(invalid());
    }
    super::receiving::validate_received_assets(folder, &document, installed)
        .await
        .map_err(|error| RepositoryError::Conflict(error.to_string()))?;
    let mut artwork = ArtworkStaging::new(pool).await?;
    artwork.stage_document(folder, &document).await?;

    // Validate the initial supported activation boundary before generating any
    // derived file. Binding richer playback is the same admission path's next
    // step, not permission to silently drop required playback content.
    if let Some(publication) = &document.published {
        super::activation::validate_supported_playback(publication)?;
    }
    let manifest_tree = tree.clone();
    let manifest_addon = document.addon_id.clone();
    let manifest_name = document
        .published
        .as_ref()
        .map_or(&document.draft.presentation.name, |publication| {
            &publication.presentation.name
        })
        .clone();
    let manifest_url = document.source_url.clone();
    tokio::task::spawn_blocking(move || {
        crate::addon_media_storage::ensure_receiving_playlist_manifest(
            manifest_tree.managed_root(),
            &manifest_addon,
            &manifest_name,
            &manifest_url,
            "",
        )
    })
    .await
    .map_err(|_| invalid())??;

    let now = Utc::now();
    let mut tx = artwork.begin_projection().await?;
    // Recheck local authorization/configuration after filesystem validation.
    let enabled: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(SELECT 1 FROM "AddonInstallation"
           WHERE "AddonId"=? AND "Enabled"=1 AND "ManifestJson"=?)"#,
    )
    .bind(addon_id)
    .bind(manifest_json)
    .fetch_one(&mut *tx)
    .await
    .repository()?;
    if !enabled {
        return Err(invalid());
    }
    let conflict: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(SELECT 1 FROM "AddonMediaList"
           WHERE "AddonId"=? AND ("ProviderId"=? OR "SourceUrl"=?))"#,
    )
    .bind(addon_id)
    .bind(&document.provider_id)
    .bind(&document.source_url)
    .fetch_one(&mut *tx)
    .await
    .repository()?;
    if conflict {
        return Err(RepositoryError::Conflict(
            "a local list already owns the copied list identity".into(),
        ));
    }
    let list_id = sqlx::query_scalar(
        r#"INSERT INTO "AddonMediaList"
           ("AddonId","ProviderId","SourceUrl","Name","DraftRevision","ReviewState","SavedDocumentDigest","SavedPublicationDigest")
           VALUES (?,?,?,?,1,2,?,?) RETURNING "Id""#,
    )
    .bind(addon_id)
    .bind(&document.provider_id)
    .bind(&document.source_url)
    .bind(&document.draft.presentation.name)
    .bind(digest)
    .bind(publication_digest)
    .fetch_one(&mut *tx)
    .await
    .repository()?;
    let library_id = crate::addon_media_storage::register_existing_list_library_in_transaction(
        &mut tx, list_id, addon_id, folder,
    )
    .await?;
    let projected =
        super::projection::project_draft_in_tx(&mut tx, list_id, &document, 1, now).await?;
    let queued = if let Some(publication) = &document.published {
        let snapshot = super::activation::snapshot_in_tx(
            &mut tx,
            list_id,
            publication,
            &projected.item_ids,
            1,
            1,
            now,
        )
        .await?;
        crate::repositories::insert_revision_snapshot_in_tx(&mut tx, &snapshot).await?;
        Some(
            crate::repositories::managed_library_operation::enqueue_saved_folder_publication_in_tx(
                &mut tx,
                list_id,
                library_id,
                1,
                snapshot.revision,
            )
            .await?,
        )
    } else {
        None
    };
    tx.commit().await.repository()?;
    drop(tree);
    Ok(queued)
}

fn invalid() -> RepositoryError {
    RepositoryError::Conflict("saved folder identity or local storage is unavailable".into())
}
