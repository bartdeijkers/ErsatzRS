//! Repair an adopted folder's local projection before another edit or scan.

use std::{path::PathBuf, sync::Arc};

use chrono::Utc;
use ersatzrs_addon_contract::AddonManifest;
use ersatzrs_core::repositories::{AddonMediaListPublicationEnqueue, RepositoryError, Result};
use sqlx::{Row as _, Transaction};

use super::{
    assembly, document_store, model::SavedListDocument, receiving_artwork::ArtworkStaging,
};
use crate::db::{Db, DbPool};
use crate::repositories::SqlxResultExt as _;

pub(crate) enum Repair {
    Unchanged,
    Deferred,
    Updated(Option<Box<AddonMediaListPublicationEnqueue>>),
}

/// One guarded file and the local ownership observed before staging any assets.
pub(crate) struct MappedList {
    pub tree: Arc<assembly::ManagedTreeGuard>,
    pub document: SavedListDocument,
    pub digest: String,
    pub installed: AddonManifest,
    pub manifest_json: String,
    pub published_revision: i32,
    list_id: i64,
    pub previous_digest: String,
    pub publication_digest: String,
    pub previous_publication_digest: Option<String>,
    manifest_path: String,
    root: String,
}

impl MappedList {
    pub async fn load(pool: &DbPool, list_id: i64) -> Result<Option<Self>> {
        let row = sqlx::query(
            r#"SELECT "list"."SavedDocumentDigest", "list"."SavedPublicationDigest",
               "list"."PublishedRevision", "library"."ManifestPath", "source"."RootPath",
               "addon"."ManifestJson", "addon"."Enabled"
               FROM "AddonMediaList" AS "list"
               LEFT JOIN "AddonMediaListLibrary" AS "library" ON "library"."AddonMediaListId"="list"."Id"
               LEFT JOIN "AddonMediaSource" AS "source" ON "source"."AddonId"="list"."AddonId"
               LEFT JOIN "AddonInstallation" AS "addon" ON "addon"."AddonId"="list"."AddonId"
               WHERE "list"."Id"=?"#,
        ).bind(list_id).fetch_optional(pool).await.repository()?.ok_or_else(conflict)?;
        let Some(previous_digest) = row
            .try_get::<Option<String>, _>("SavedDocumentDigest")
            .repository()?
        else {
            return Ok(None);
        };
        if !row
            .try_get::<Option<bool>, _>("Enabled")
            .repository()?
            .unwrap_or(false)
        {
            return Err(conflict());
        }
        let manifest_json: String = row.try_get("ManifestJson").repository()?;
        let installed = serde_json::from_str(&manifest_json).map_err(|_| conflict())?;
        let manifest_path: String = row.try_get("ManifestPath").repository()?;
        let root: String = row.try_get("RootPath").repository()?;
        let path = PathBuf::from(&manifest_path);
        let folder = path.parent().ok_or_else(conflict)?.to_path_buf();
        if !folder.is_absolute() || folder.parent() != Some(PathBuf::from(&root).as_path()) {
            return Err(conflict());
        }
        let (tree, document, digest, publication_digest) = tokio::task::spawn_blocking(move || {
            let metadata = std::fs::symlink_metadata(&folder).map_err(|_| conflict())?;
            if !metadata.is_dir() || metadata.is_symlink() {
                return Err(conflict());
            }
            let tree = assembly::lock_managed_tree(&folder)?;
            let (document, digest) = document_store::read(&tree)?;
            let publication_digest =
                document_store::publication_digest(document.published.as_ref())?;
            Ok((tree, document, digest, publication_digest))
        })
        .await
        .map_err(|_| conflict())??;
        Ok(Some(Self {
            tree,
            document,
            digest,
            installed,
            manifest_json,
            published_revision: row.try_get("PublishedRevision").repository()?,
            list_id,
            previous_digest,
            publication_digest,
            previous_publication_digest: row.try_get("SavedPublicationDigest").repository()?,
            manifest_path,
            root,
        }))
    }

    /// Recheck the exact local mapping and all active managed operations. A
    /// queued `SavedFolder` job may coexist with draft repair, just as with Save.
    /// Returns false for a busy owner; identity changes remain errors.
    pub async fn fence(&self, tx: &mut Transaction<'_, Db>) -> Result<bool> {
        let available: Option<bool> = sqlx::query_scalar(
            r#"SELECT "list"."ReviewState"<>3
                 AND "list"."PublicationJournalJson" IS NULL
                 AND NOT EXISTS(SELECT 1 FROM "ManagedLibraryLease" AS "lease"
                     WHERE "lease"."LibraryId"="library"."LibraryId" AND "lease"."LeaseExpiresAt">?)
               FROM "AddonMediaList" AS "list"
               JOIN "AddonInstallation" AS "addon" ON "addon"."AddonId"="list"."AddonId"
               JOIN "AddonMediaSource" AS "source" ON "source"."AddonId"="list"."AddonId"
               JOIN "AddonMediaListLibrary" AS "library" ON "library"."AddonMediaListId"="list"."Id"
               WHERE "list"."Id"=? AND "list"."SavedDocumentDigest"=?
                 AND "list"."SavedPublicationDigest" IS ? AND "list"."PublishedRevision"=?
                 AND "list"."AddonId"=? AND "list"."ProviderId"=?
                 AND "addon"."Enabled"=1 AND "addon"."ManifestJson"=?
                 AND "source"."RootPath"=? AND "library"."ManifestPath"=?
                 AND NOT EXISTS(SELECT 1 FROM "AddonMediaList" AS "other"
                     WHERE "other"."Id"<>"list"."Id" AND "other"."AddonId"="list"."AddonId"
                       AND "other"."SourceUrl"=?)"#,
        )
        .bind(crate::repositories::common::encode_datetime(Utc::now()))
        .bind(self.list_id)
        .bind(&self.previous_digest)
        .bind(&self.previous_publication_digest)
        .bind(self.published_revision)
        .bind(&self.document.addon_id)
        .bind(&self.document.provider_id)
        .bind(&self.manifest_json)
        .bind(&self.root)
        .bind(&self.manifest_path)
        .bind(&self.document.source_url)
        .fetch_optional(&mut **tx)
        .await
        .repository()?;
        available.ok_or_else(conflict)
    }

    /// Reuse mapping checks when the caller separately authorizes its own live
    /// publication lease. Busy state is expected here, never an identity bypass.
    pub async fn verify_publication_identity(&self, tx: &mut Transaction<'_, Db>) -> Result<()> {
        self.fence(tx).await.map(|_| ())
    }

    /// Files stay untouched. Project once, using a new local edit fence, while
    /// preserving local hiding, active publication and durable job ownership.
    pub async fn repair(&mut self, pool: &DbPool) -> Result<Repair> {
        if self.digest == self.previous_digest {
            return Ok(Repair::Unchanged);
        }
        super::receiving::validate_received_assets(
            self.tree.managed_root(),
            &self.document,
            &self.installed,
        )
        .await
        .map_err(|error| RepositoryError::Conflict(error.to_string()))?;
        let publication_changed =
            self.previous_publication_digest.as_ref() != Some(&self.publication_digest);
        if publication_changed {
            let publication = self.document.published.as_ref().ok_or_else(|| {
                RepositoryError::Conflict(
                    "removing a saved publication requires the existing reset operation".into(),
                )
            })?;
            super::activation::validate_supported_playback(publication)?;
        }
        let mut artwork = ArtworkStaging::new(pool).await?;
        artwork
            .stage_document(self.tree.managed_root(), &self.document)
            .await?;
        let mut tx = artwork.begin_projection().await?;
        if !self.fence(&mut tx).await? {
            return Ok(Repair::Deferred);
        }
        let revision: i32 =
            sqlx::query_scalar(r#"SELECT "DraftRevision" FROM "AddonMediaList" WHERE "Id"=?"#)
                .bind(self.list_id)
                .fetch_one(&mut *tx)
                .await
                .repository()?;
        let revision = revision.checked_add(1).ok_or_else(conflict)?;
        sqlx::query(
            r#"UPDATE "AddonMediaList" SET "DraftRevision"=?,
            "ReviewState"=CASE WHEN "ReviewState"=0 THEN 0 ELSE 1 END WHERE "Id"=?"#,
        )
        .bind(revision)
        .bind(self.list_id)
        .execute(&mut *tx)
        .await
        .repository()?;
        let now = Utc::now();
        let projected = super::projection::project_draft_in_tx(
            &mut tx,
            self.list_id,
            &self.document,
            revision,
            now,
        )
        .await?;
        let queued = if publication_changed {
            let publication = self.document.published.as_ref().ok_or_else(conflict)?;
            let source_revision: i32 = sqlx::query_scalar(
                r#"SELECT COALESCE(MAX("Revision"),0) FROM "AddonMediaListRevision" WHERE "AddonMediaListId"=?"#,
            ).bind(self.list_id).fetch_one(&mut *tx).await.repository()?;
            let source_revision = source_revision.checked_add(1).ok_or_else(conflict)?;
            let snapshot = super::activation::snapshot_in_tx(
                &mut tx,
                self.list_id,
                publication,
                &projected.item_ids,
                source_revision,
                revision,
                now,
            )
            .await?;
            crate::repositories::insert_revision_snapshot_in_tx(&mut tx, &snapshot).await?;
            let library_id = sqlx::query_scalar(
                r#"SELECT "LibraryId" FROM "AddonMediaListLibrary" WHERE "AddonMediaListId"=?"#,
            )
            .bind(self.list_id)
            .fetch_one(&mut *tx)
            .await
            .repository()?;
            Some(crate::repositories::managed_library_operation::enqueue_saved_folder_publication_in_tx(
                &mut tx, self.list_id, library_id, revision, source_revision).await?)
        } else {
            None
        };
        sqlx::query(r#"UPDATE "AddonMediaList" SET "SavedDocumentDigest"=?, "SavedPublicationDigest"=? WHERE "Id"=?"#)
            .bind(&self.digest).bind(&self.publication_digest).bind(self.list_id)
            .execute(&mut *tx).await.repository()?;
        tx.commit().await.repository()?;
        self.previous_digest.clone_from(&self.digest);
        self.previous_publication_digest = Some(self.publication_digest.clone());
        Ok(Repair::Updated(queued.map(Box::new)))
    }
}

pub(crate) async fn refresh(pool: &DbPool, list_id: i64) -> Result<Repair> {
    let Some(mut mapped) = MappedList::load(pool, list_id).await? else {
        return Ok(Repair::Unchanged);
    };
    mapped.repair(pool).await
}

fn conflict() -> RepositoryError {
    RepositoryError::Conflict("saved list ownership or content changed, or a managed operation is active; refresh and retry".into())
}
