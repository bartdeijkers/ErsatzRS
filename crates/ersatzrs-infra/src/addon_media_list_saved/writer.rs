//! Draft writes for an already-adopted authoritative list folder.
//!
//! Adoption and other content writers remain part of the same O-01 integration.

use std::sync::Arc;

use ersatzrs_core::repositories::{AddonMediaListEditorSave, RepositoryError, Result};
use sqlx::Transaction;

use super::writer_assets::PreparedAssets;
use super::{assembly, capture, document_store, mapped::MappedList, validation};
use crate::db::{Db, DbPool};
use crate::repositories::SqlxResultExt as _;

pub(crate) struct SavedWriter {
    mapped: MappedList,
    assets: PreparedAssets,
    list_id: i64,
}

impl SavedWriter {
    /// Provider updates retain operator assets and never repair a divergent file
    /// through their own job. Recovery must reconcile authority before retrying.
    /// The caller acquires the tree before its transaction and checks managed
    /// availability before capture, since Restore may be materializing a draft.
    pub(crate) async fn capture_provider_update(
        mapped: MappedList,
        tx: &mut Transaction<'_, Db>,
        list_id: i64,
    ) -> Result<Self> {
        if mapped.digest != mapped.previous_digest {
            return Err(RepositoryError::Conflict(
                "the saved list folder changed; refresh its database projection before retrying provider results".into(),
            ));
        }
        validation::validate_content(&mapped.document, &mapped.installed)
            .map_err(|_| conflict())?;
        let captured = capture::draft_in_tx(tx, list_id).await?;
        let assets = PreparedAssets::reuse(&mapped.document, &captured)?;
        Ok(Self {
            mapped,
            assets,
            list_id,
        })
    }

    /// Import phases own their existing import-job fence, not a managed lease.
    /// Like operator Save, reserved draft writes may coexist with root scans;
    /// publication, reset and other mutating managed operations remain blocking.
    pub(crate) async fn capture_import(
        mapped: Option<MappedList>,
        tx: &mut Transaction<'_, Db>,
        list_id: i64,
    ) -> Result<Option<Self>> {
        let Some(mapped) = mapped else {
            return Ok(None);
        };
        if !mapped.fence_operator_save(tx).await? {
            return Err(conflict());
        }
        // Bound the enclosing import/refresh future at the adopted-list capture.
        Box::pin(Self::capture_provider_update(mapped, tx, list_id))
            .await
            .map(Some)
    }

    /// Preflight only adopted lists, before acquiring any database write lock.
    pub(crate) async fn prepare(
        pool: &DbPool,
        save: &AddonMediaListEditorSave,
    ) -> Result<Option<Self>> {
        let Some(mut mapped) = MappedList::load(pool, save.list_id).await? else {
            return Ok(None);
        };
        match mapped.repair(pool).await? {
            super::mapped::Repair::Unchanged => {}
            super::mapped::Repair::Deferred => return Err(conflict()),
            super::mapped::Repair::Updated(_) => return Err(RepositoryError::Conflict(
                "the database was refreshed from the saved list folder; review the recovered draft before saving".into(),
            )),
        }
        validation::validate_content(&mapped.document, &mapped.installed)
            .map_err(|_| conflict())?;
        let mut tx = pool.begin().await.repository()?;
        let captured = capture::draft_in_tx(&mut tx, save.list_id).await?;
        tx.commit().await.repository()?;
        if captured.draft.revision != save.expected_revision
            || captured.addon_id != mapped.document.addon_id
            || captured.provider_id != mapped.document.provider_id
            || captured.source_url != mapped.document.source_url
        {
            return Err(conflict());
        }
        let assets = PreparedAssets::prepare(
            pool,
            Arc::clone(&mapped.tree),
            &mapped.document,
            &captured,
            save,
        )
        .await?;
        Ok(Some(Self {
            mapped,
            assets,
            list_id: save.list_id,
        }))
    }

    /// Recheck authority and mutating managed operations, including warning-only Saves.
    pub(crate) async fn fence(&self, tx: &mut Transaction<'_, Db>) -> Result<()> {
        if !self.mapped.fence_operator_save(tx).await? {
            return Err(conflict());
        }
        Ok(())
    }

    /// Persist the resulting draft, retaining the independent published section.
    pub(crate) async fn write(
        self,
        tx: &mut Transaction<'_, Db>,
    ) -> Result<Arc<assembly::ManagedTreeGuard>> {
        self.write_inner(tx, false).await
    }

    /// Import may add/remove items or clear a re-adopted replacement choice;
    /// it must never invent an operator-selected local asset.
    pub(crate) async fn write_import(
        self,
        tx: &mut Transaction<'_, Db>,
    ) -> Result<Arc<assembly::ManagedTreeGuard>> {
        self.write_inner(tx, true).await
    }

    async fn write_inner(
        mut self,
        tx: &mut Transaction<'_, Db>,
        provider_changes: bool,
    ) -> Result<Arc<assembly::ManagedTreeGuard>> {
        let mut captured = capture::draft_in_tx(tx, self.list_id).await?;
        if provider_changes {
            self.assets.accept_provider_changes(&captured)?;
            self.mapped
                .document
                .provider_id
                .clone_from(&captured.provider_id);
        }
        self.assets.apply(&mut captured)?;
        self.mapped.document.draft = captured.draft;
        validation::validate_content(&self.mapped.document, &self.mapped.installed)
            .map_err(|_| conflict())?;
        // The blocking worker keeps tree ownership if its async waiter is cancelled.
        let tree = Arc::clone(&self.mapped.tree);
        let document = self.mapped.document;
        let previous = self.mapped.digest;
        let digest = tokio::task::spawn_blocking(move || {
            document_store::replace(&tree, &document, &previous)
        })
        .await
        .map_err(|_| pending())??;
        sqlx::query(r#"UPDATE "AddonMediaList" SET "SavedDocumentDigest"=? WHERE "Id"=?"#)
            .bind(&digest)
            .bind(self.list_id)
            .execute(&mut **tx)
            .await
            .map_err(|_| pending())?;
        Ok(self.mapped.tree)
    }
}

pub(crate) fn pending() -> RepositoryError {
    RepositoryError::Conflict(
        "saved to the list folder; database refresh is pending before another edit".into(),
    )
}

fn conflict() -> RepositoryError {
    RepositoryError::Conflict("saved list ownership or content changed, or a managed operation is active; refresh and retry".into())
}
