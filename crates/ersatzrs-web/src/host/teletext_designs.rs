//! Durable, fully compiled originals without active-publication authority.

use std::{fmt, fmt::Write as _, time::Instant};

use async_trait::async_trait;
use ersatzrs_app::{
    teletext_design_documents::{
        decode_teletext_design_documents, encode_teletext_design_documents,
        EncodedTeletextDesignDocuments, TeletextDesignDocumentBudget, TeletextDesignDocumentError,
        TeletextDesignDocumentInput, TeletextSavedDocumentSource, TeletextSavedEditPolicy,
        TeletextSavedResourceTransmission, TeletextSavedResourceUpdate, TeletextSavedV1Region,
    },
    teletext_designs::{
        TeletextDesignStoreError, TeletextOriginalDesignValidationError,
        TeletextOriginalDesignValidator,
    },
    ApplicationServices,
};
use ersatzrs_core::repositories::TeletextDesignMutation;
use ersatzrs_domain::{
    EntityId, TeletextChannelSelectionSnapshot, TeletextDesignHead,
    TeletextProducerCompositionMetadata, TeletextProducerPresentation,
};
use ersatzrs_ffmpeg::teletext::{
    EncodedTeletextAuthoredService, TeletextEncodingRegion, TeletextResourceFullUpdate,
    TeletextServiceResourceTransmission,
};
use sha2::{Digest, Sha256};

use super::teletext_channel_documents::{
    encode_original_channel_documents, ChannelTeletextDocumentEncodingError,
    EncodedChannelTeletextDocumentCandidate, EncodedChannelTeletextOriginals,
    TeletextChannelDocumentEncodingPolicy,
};

#[derive(Debug, thiserror::Error)]
pub enum NativeTeletextDesignError {
    #[error(transparent)]
    Storage(#[from] TeletextDesignStoreError),
    #[error(transparent)]
    Document(#[from] TeletextDesignDocumentError),
    #[error("saved teletext native encoding: {0}")]
    Encoding(Box<ChannelTeletextDocumentEncodingError>),
    #[error("teletext design operation exceeded its deadline")]
    TimedOut,
    #[error("teletext design worker failed")]
    WorkerFailed,
    #[error("teletext resource lifecycle count exceeds storage policy")]
    LifecycleBudget,
}

/// Saved immutable original and its associated complete preview candidate.
/// Saving does not create a generation, enable teletext or change any output.
pub struct SavedOriginalTeletextDesign {
    head: TeletextDesignHead,
    candidate: EncodedChannelTeletextDocumentCandidate,
    digest: String,
}
impl SavedOriginalTeletextDesign {
    #[must_use]
    pub const fn head(&self) -> &TeletextDesignHead {
        &self.head
    }
    #[must_use]
    pub const fn candidate(&self) -> &EncodedChannelTeletextDocumentCandidate {
        &self.candidate
    }
    #[must_use]
    pub fn digest(&self) -> &str {
        &self.digest
    }
}
impl fmt::Debug for SavedOriginalTeletextDesign {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SavedOriginalTeletextDesign")
            .field("head", &self.head)
            .finish_non_exhaustive()
    }
}

/// Historical preview, independently recompiled without invoking any package.
/// Historical settings and lifecycle describe originals, not current authority.
pub struct RestoredOriginalTeletextDesign {
    selection: TeletextChannelSelectionSnapshot,
    sources: Vec<TeletextSavedDocumentSource>,
    digest: String,
    originals: EncodedChannelTeletextOriginals,
    edit_policy: TeletextSavedEditPolicy,
}
impl RestoredOriginalTeletextDesign {
    #[must_use]
    pub const fn selection(&self) -> &TeletextChannelSelectionSnapshot {
        &self.selection
    }
    #[must_use]
    pub fn sources(&self) -> &[TeletextSavedDocumentSource] {
        &self.sources
    }
    #[must_use]
    pub fn digest(&self) -> &str {
        &self.digest
    }
    #[must_use]
    pub const fn encoded(&self) -> Option<&EncodedTeletextAuthoredService> {
        self.originals.encoded.as_ref()
    }
    #[must_use]
    pub fn producers(&self) -> &[TeletextProducerPresentation] {
        &self.originals.producers
    }
    #[must_use]
    pub const fn metadata(&self) -> Option<&TeletextProducerCompositionMetadata> {
        self.originals.metadata.as_ref()
    }
    #[must_use]
    pub const fn v1_region(&self) -> Option<TeletextEncodingRegion> {
        self.originals.v1_region
    }
    #[must_use]
    pub fn transmissions(&self) -> &[TeletextServiceResourceTransmission] {
        &self.originals.transmissions
    }
    #[must_use]
    pub const fn edit_policy(&self) -> TeletextSavedEditPolicy {
        self.edit_policy
    }
}
impl fmt::Debug for RestoredOriginalTeletextDesign {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RestoredOriginalTeletextDesign")
            .field("documents", &self.sources.len())
            .finish_non_exhaustive()
    }
}

fn check_deadline(deadline: Instant) -> Result<(), NativeTeletextDesignError> {
    if Instant::now() >= deadline {
        Err(NativeTeletextDesignError::TimedOut)
    } else {
        Ok(())
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut digest = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        let _ = write!(digest, "{byte:02x}");
    }
    digest
}

// Constructed only alongside capture of a private, complete native candidate.
struct NativeOriginalDesignConfirmation {
    digest: String,
    deadline: Instant,
}
#[async_trait]
impl TeletextOriginalDesignValidator for NativeOriginalDesignConfirmation {
    async fn confirm_original(
        &self,
        document: EncodedTeletextDesignDocuments,
    ) -> Result<EncodedTeletextDesignDocuments, TeletextOriginalDesignValidationError> {
        let expected = self.digest.clone();
        let deadline = self.deadline;
        tokio::task::spawn_blocking(move || {
            if Instant::now() >= deadline {
                return Err(TeletextOriginalDesignValidationError::TimedOut);
            }
            let actual = sha256_hex(document.bytes());
            if actual != expected {
                return Err(TeletextOriginalDesignValidationError::Association);
            }
            if Instant::now() >= deadline {
                return Err(TeletextOriginalDesignValidationError::TimedOut);
            }
            Ok(document)
        })
        .await
        .map_err(|_| TeletextOriginalDesignValidationError::WorkerFailed)?
    }
}

/// Capture exact original sources from a complete native candidate and save CAS.
/// The fresh CPU/admission deadline is distinct from its invocation receipt.
/// Once admitted, repository I/O uses the normal `SQLite` timeout/transaction
/// contract. The deadline does not cancel or undo a committed save.
/// No active configuration, output, package or playout notification is changed.
/// # Errors
/// Rejects codec, worker or admission-deadline failures before saving; storage
/// failures retain the repository's atomic no-partial-write contract.
pub async fn save_original_teletext_design_candidate(
    application: &ApplicationServices,
    name: String,
    existing: Option<(EntityId, i64)>,
    candidate: EncodedChannelTeletextDocumentCandidate,
    budget: TeletextDesignDocumentBudget,
    deadline: Instant,
) -> Result<TeletextDesignMutation<SavedOriginalTeletextDesign>, NativeTeletextDesignError> {
    check_deadline(deadline)?;
    let (candidate, document, digest) = tokio::task::spawn_blocking(move || {
        check_deadline(deadline)?;
        if candidate.transmissions().len() > budget.max_resource_transmissions {
            return Err(NativeTeletextDesignError::LifecycleBudget);
        }
        let transmissions: Vec<_> = candidate
            .transmissions()
            .iter()
            .map(|record| TeletextSavedResourceTransmission {
                resource: record.resource,
                counter: record.counter,
                last_subpage: record.last_subpage,
                update: match record.update {
                    TeletextResourceFullUpdate::Changed => TeletextSavedResourceUpdate::Changed,
                    TeletextResourceFullUpdate::Unchanged => TeletextSavedResourceUpdate::Unchanged,
                },
            })
            .collect();
        let document = encode_teletext_design_documents(
            TeletextDesignDocumentInput {
                selection: candidate.selection(),
                generation: candidate.generation(),
                sources: candidate.sources(),
                entry_addon_id: candidate.entry_addon_id(),
                entry: candidate.metadata().map(|metadata| metadata.entry),
                v1_region: candidate.v1_region().map(|region| match region {
                    TeletextEncodingRegion::WesternLatin => TeletextSavedV1Region::WesternLatin,
                }),
                resource_transmissions: &transmissions,
            },
            budget,
        )?;
        let digest = sha256_hex(document.bytes());
        check_deadline(deadline)?;
        Ok::<_, NativeTeletextDesignError>((candidate, document, digest))
    })
    .await
    .map_err(|_| NativeTeletextDesignError::WorkerFailed)??;
    let confirmation = NativeOriginalDesignConfirmation {
        digest: digest.clone(),
        deadline,
    };
    Ok(
        match application
            .save_original_teletext_design(
                name,
                existing,
                document,
                &confirmation,
                budget.max_serialized_bytes,
            )
            .await?
        {
            TeletextDesignMutation::Saved(head) => {
                TeletextDesignMutation::Saved(SavedOriginalTeletextDesign {
                    head,
                    candidate,
                    digest,
                })
            }
            TeletextDesignMutation::Rejected(reason) => TeletextDesignMutation::Rejected(reason),
        },
    )
}

/// Reload immutable originals and compile the same checked preview model.
/// No producer runs and no historical selection is promoted into a live fence.
/// Caller policy supplies bounds; saved exact entry/region/lifecycle supply content.
/// # Errors
/// Missing revision returns None; corrupt/unsupported/bounded/native failures are errors.
pub async fn reload_original_teletext_design_revision(
    application: &ApplicationServices,
    id: EntityId,
    revision: i64,
    budget: TeletextDesignDocumentBudget,
    mut policy: TeletextChannelDocumentEncodingPolicy,
    deadline: Instant,
) -> Result<Option<RestoredOriginalTeletextDesign>, NativeTeletextDesignError> {
    check_deadline(deadline)?;
    let Some(stored) = application
        .teletext_design_revision(id, revision, budget.max_serialized_bytes)
        .await?
    else {
        return Ok(None);
    };
    let restored = tokio::task::spawn_blocking(move || {
        check_deadline(deadline)?;
        let parts = decode_teletext_design_documents(&stored.document, budget)?.into_parts();
        drop(stored.document);
        policy.entry = parts.entry;
        policy.transmissions = parts
            .resource_transmissions
            .iter()
            .map(|record| TeletextServiceResourceTransmission {
                resource: record.resource,
                counter: record.counter,
                last_subpage: record.last_subpage,
                update: match record.update {
                    TeletextSavedResourceUpdate::Changed => TeletextResourceFullUpdate::Changed,
                    TeletextSavedResourceUpdate::Unchanged => TeletextResourceFullUpdate::Unchanged,
                },
            })
            .collect();
        let region = parts.v1_region.map(|region| match region {
            TeletextSavedV1Region::WesternLatin => TeletextEncodingRegion::WesternLatin,
        });
        let originals = encode_original_channel_documents(
            &parts.selection,
            parts.entry_addon_id.as_deref(),
            parts.documents,
            deadline,
            region,
            policy,
        )
        .map_err(|error| NativeTeletextDesignError::Encoding(Box::new(error)))?;
        check_deadline(deadline)?;
        Ok::<_, NativeTeletextDesignError>(RestoredOriginalTeletextDesign {
            selection: parts.selection,
            sources: parts.sources,
            digest: stored.digest,
            originals,
            edit_policy: parts.edit_policy,
        })
    })
    .await
    .map_err(|_| NativeTeletextDesignError::WorkerFailed)??;
    Ok(Some(restored))
}
