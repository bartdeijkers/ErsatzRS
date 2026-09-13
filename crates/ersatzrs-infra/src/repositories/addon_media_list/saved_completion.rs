//! Save ordinary publication content before acknowledging journal completion.

use std::sync::Arc;

use super::*;
use crate::addon_media_list_saved::{
    assembly, capture, document_store, mapped::MappedList, published, validation,
};

pub(super) struct Completion {
    mapped: MappedList,
    list_id: EntityId,
    acknowledge_draft: bool,
}

/// Called after retention/cleanup, with the publication's own claim validated.
/// No write transaction is held while installing required publication assets.
pub(super) async fn prepare(
    repository: &SqliteAddonMediaListRepository,
    list_id: EntityId,
    operation_id: EntityId,
    generation: i32,
) -> Result<Option<Completion>> {
    let Some(mut mapped) = MappedList::load(&repository.0, list_id).await? else {
        return Ok(None);
    };
    let saved_folder: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(SELECT 1 FROM "AddonMediaListPublicationJob" AS "job"
           JOIN "AddonMediaListPublicationEvent" AS "event" ON "event"."PublicationJobId"="job"."Id"
           WHERE "job"."AddonMediaListId"=? AND "job"."OperationId"=? AND "job"."Generation"=?
             AND "event"."DedupeKey"='enqueued' AND "event"."Kind"=?
             AND "event"."EnqueueTrigger"=?)"#,
    )
    .bind(list_id)
    .bind(operation_id)
    .bind(generation)
    .bind(crate::repositories::common::encode_enum(
        ersatzrs_domain::AddonMediaListPublicationEventKind::Enqueued,
    ))
    .bind(crate::repositories::common::encode_enum(
        ersatzrs_domain::AddonMediaListPublicationEnqueueTrigger::SavedFolder,
    ))
    .fetch_one(&repository.0)
    .await
    .repository()?;
    if saved_folder {
        // Receiving counters, artwork handles and completion timestamps are local.
        // Activation must not rebase the independent incoming saved document.
        return Ok(None);
    }
    let mut tx = repository.0.begin().await.repository()?;
    mapped.verify_publication_identity(&mut tx).await?;
    let divergent = mapped.digest != mapped.previous_digest;
    let matching_draft = if divergent {
        let captured = capture::draft_in_tx(&mut tx, list_id).await?;
        captured.addon_id == mapped.document.addon_id
            && captured.provider_id == mapped.document.provider_id
            && captured.source_url == mapped.document.source_url
            && captured.draft == mapped.document.draft
    } else {
        true
    };
    let captured = published::in_tx(&mut tx, list_id)
        .await?
        .ok_or_else(invalid)?;
    tx.commit().await.repository()?;
    if captured.publication.revision != mapped.published_revision {
        return Err(invalid());
    }
    let (storage_parent, storage_owner) = revision_blob_storage(&repository.0).await?;
    let artifact_root = crate::file_system_layout::addon_processed_media_folder();
    let publication = assembly::assemble_publication(
        &repository.0,
        &assembly::AssemblyContext {
            list_id,
            tree: Arc::clone(&mapped.tree),
            storage_parent: &storage_parent,
            storage_owner: &storage_owner,
            artifact_root: &artifact_root,
            installed: &mapped.installed,
        },
        captured,
    )
    .await?;
    let intended_publication = Some(publication);
    let intended_digest = document_store::publication_digest(intended_publication.as_ref())?;
    if divergent
        && mapped.previous_publication_digest.as_ref() != Some(&mapped.publication_digest)
        && mapped.publication_digest != intended_digest
    {
        // A copied publication unrelated to this completion stays authoritative
        // on disk; it must not be replaced by the older local operation.
        return Err(RepositoryError::Conflict(
            "the list folder contains a different saved publication; resolve the folder before retrying".into(),
        ));
    }
    // Exact equality recognizes our own retry without acknowledging a changed
    // draft. Concrete replacement stubs remain conservatively unequal.
    let acknowledge_draft =
        !divergent || (matching_draft && mapped.publication_digest == intended_digest);
    mapped.document.published = intended_publication;
    validation::validate_content(&mapped.document, &mapped.installed).map_err(|_| invalid())?;
    Ok(Some(Completion {
        mapped,
        list_id,
        acknowledge_draft,
    }))
}

impl Completion {
    /// The caller has fenced journal-clear in this same transaction. Hold the
    /// returned tree guard through commit; a failed commit leaves the file ahead.
    pub async fn write(
        self,
        tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    ) -> Result<Arc<assembly::ManagedTreeGuard>> {
        self.mapped.verify_publication_identity(tx).await?;
        let expected_revision = self.mapped.published_revision;
        let expected_digest = self.mapped.previous_digest;
        let expected_manifest = self.mapped.manifest_json;
        let tree = Arc::clone(&self.mapped.tree);
        let document = self.mapped.document;
        let actual_digest = self.mapped.digest;
        let (digest, publication_digest) = tokio::task::spawn_blocking(move || {
            let publication_digest =
                document_store::publication_digest(document.published.as_ref())?;
            // Use the current guarded file digest, not SQLite's previous digest.
            // Retrying the same completion re-establishes durability even when a
            // previous rename succeeded but its directory synchronization failed.
            let digest = document_store::replace(&tree, &document, &actual_digest)?;
            Ok((digest, publication_digest))
        })
        .await
        .map_err(|_| crate::addon_media_list_saved::writer::pending())??;
        let updated = sqlx::query(
            r#"UPDATE "AddonMediaList" SET "SavedDocumentDigest"=?, "SavedPublicationDigest"=?
               WHERE "Id"=? AND "PublishedRevision"=? AND "SavedDocumentDigest"=?
                 AND EXISTS(SELECT 1 FROM "AddonInstallation" AS "addon"
                     WHERE "addon"."AddonId"="AddonMediaList"."AddonId"
                       AND "addon"."Enabled"=1 AND "addon"."ManifestJson"=?)"#,
        )
        // Do not acknowledge an unprojected file draft. Once this operation is
        // terminal, mapped repair will project it without activating publication
        // again because its publication digest is advanced in this transaction.
        .bind(if self.acknowledge_draft { digest } else { expected_digest.clone() })
        .bind(publication_digest)
        .bind(self.list_id)
        .bind(expected_revision)
        .bind(expected_digest)
        .bind(expected_manifest)
        .execute(&mut **tx)
        .await
        .map_err(|_| crate::addon_media_list_saved::writer::pending())?;
        if updated.rows_affected() != 1 {
            return Err(crate::addon_media_list_saved::writer::pending());
        }
        Ok(self.mapped.tree)
    }
}

fn invalid() -> RepositoryError {
    RepositoryError::Conflict("the completed saved publication is unavailable or changed".into())
}
