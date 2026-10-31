//! Bounded refresh commands behind the existing optional generation port.

use std::time::Instant;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ersatzrs_core::repositories::{
    TeletextGenerationMutation, TeletextGuideGenerationCheck, TeletextGuideRefreshCandidate,
    TeletextGuideRefreshClaim, TeletextGuideRefreshCurrent, TeletextGuideRefreshCurrentMetadata,
    TeletextGuideRefreshFailure, TeletextGuideRefreshOutputCheck,
    TeletextGuideRefreshOutputConfirmation, TeletextGuideRefreshRepository,
    TeletextGuideRefreshRole, TeletextGuideRefreshSeed, TeletextGuideRuntimeSnapshot,
};
use ersatzrs_domain::EntityId;

use crate::{
    teletext_generations::TeletextGenerationStoreError,
    teletext_publications::TeletextPublicationValidationError, ApplicationServices,
};

type Error = TeletextGenerationStoreError;

/// Trusted compilation and editorial-rebase proof for a distinct runtime snapshot.
/// The repository separately checks current source, exact owner and selection.
#[async_trait]
pub trait TeletextGuideRefreshValidator: Send + Sync {
    async fn confirm_guide_refresh(
        &self,
        claim: &TeletextGuideRefreshClaim,
        candidate: &TeletextGuideRefreshCandidate,
        deadline: Instant,
    ) -> Result<(), TeletextPublicationValidationError>;
}

impl ApplicationServices {
    pub async fn confirm_guide_refresh_output(
        &self,
        confirmation: &TeletextGuideRefreshOutputConfirmation,
        now: DateTime<Utc>,
        check: TeletextGuideRefreshOutputCheck,
        max_metadata_bytes: usize,
    ) -> Result<TeletextGenerationMutation<()>, Error> {
        self.guide_refresh_repository()?
            .confirm_output(confirmation, now, &check, max_metadata_bytes)
            .await
            .map_err(|_| Error::StorageUnavailable)
    }

    fn guide_refresh_repository(&self) -> Result<&dyn TeletextGuideRefreshRepository, Error> {
        self.ports()
            .teletext_generations()
            .and_then(|repo| repo.guide_refresh())
            .ok_or(Error::Unavailable)
    }

    pub async fn scan_current_guide_refresh(
        &self,
        after_seed_id: Option<EntityId>,
        limit: usize,
    ) -> Result<Vec<TeletextGuideRefreshSeed>, Error> {
        self.guide_refresh_repository()?
            .scan_current(after_seed_id, limit)
            .await
            .map_err(|_| Error::StorageUnavailable)
    }

    pub async fn observe_current_guide_refresh(
        &self,
        channel_id: EntityId,
        role: TeletextGuideRefreshRole,
        max_metadata_bytes: usize,
    ) -> Result<Option<TeletextGuideRefreshCurrentMetadata>, Error> {
        self.guide_refresh_repository()?
            .observe_current(channel_id, role, max_metadata_bytes)
            .await
            .map_err(|_| Error::StorageUnavailable)
    }

    pub async fn guide_refresh_current(
        &self,
        channel_id: EntityId,
        role: TeletextGuideRefreshRole,
        max_document_bytes: usize,
    ) -> Result<Option<TeletextGuideRefreshCurrent>, Error> {
        self.guide_refresh_repository()?
            .current(channel_id, role, max_document_bytes)
            .await
            .map_err(|_| Error::StorageUnavailable)
    }

    pub async fn due_guide_refresh(
        &self,
        now: DateTime<Utc>,
        limit: usize,
    ) -> Result<Vec<TeletextGuideRefreshSeed>, Error> {
        self.guide_refresh_repository()?
            .due(now, limit)
            .await
            .map_err(|_| Error::StorageUnavailable)
    }

    pub async fn mark_guide_refresh_changed(
        &self,
        seed_id: EntityId,
        now: DateTime<Utc>,
    ) -> Result<TeletextGenerationMutation<()>, Error> {
        self.guide_refresh_repository()?
            .changed(seed_id, now)
            .await
            .map_err(|_| Error::StorageUnavailable)
    }

    pub async fn claim_guide_refresh(
        &self,
        seed_id: EntityId,
        now: DateTime<Utc>,
        lease_until: DateTime<Utc>,
        check: TeletextGuideGenerationCheck,
        max_document_bytes: usize,
    ) -> Result<TeletextGenerationMutation<Option<TeletextGuideRefreshClaim>>, Error> {
        self.guide_refresh_repository()?
            .claim(seed_id, now, lease_until, &check, max_document_bytes)
            .await
            .map_err(|_| Error::StorageUnavailable)
    }

    pub async fn heartbeat_guide_refresh(
        &self,
        claim: &TeletextGuideRefreshClaim,
        now: DateTime<Utc>,
        lease_until: DateTime<Utc>,
        check: TeletextGuideGenerationCheck,
    ) -> Result<TeletextGenerationMutation<()>, Error> {
        self.guide_refresh_repository()?
            .heartbeat(claim, now, lease_until, &check)
            .await
            .map_err(|_| Error::StorageUnavailable)
    }

    pub async fn complete_guide_refresh(
        &self,
        claim: &TeletextGuideRefreshClaim,
        candidate: &TeletextGuideRefreshCandidate,
        validator: &dyn TeletextGuideRefreshValidator,
        now: DateTime<Utc>,
        check: TeletextGuideGenerationCheck,
        max_document_bytes: usize,
    ) -> Result<TeletextGenerationMutation<TeletextGuideRuntimeSnapshot>, Error> {
        if Instant::now() >= check.deadline {
            return Err(TeletextPublicationValidationError::TimedOut.into());
        }
        if candidate
            .snapshot
            .qualification
            .validate_current(now)
            .is_err()
        {
            return Err(TeletextPublicationValidationError::Association.into());
        }
        let started = Instant::now();
        validator
            .confirm_guide_refresh(claim, candidate, check.deadline)
            .await?;
        if Instant::now() >= check.deadline {
            return Err(TeletextPublicationValidationError::TimedOut.into());
        }
        let elapsed = chrono::Duration::from_std(started.elapsed())
            .map_err(|_| TeletextPublicationValidationError::TimedOut)?;
        let now = now
            .checked_add_signed(elapsed)
            .ok_or(TeletextPublicationValidationError::TimedOut)?;
        self.guide_refresh_repository()?
            .complete(claim, candidate, now, &check, max_document_bytes)
            .await
            .map_err(|_| Error::StorageUnavailable)
    }

    pub async fn fail_guide_refresh(
        &self,
        claim: &TeletextGuideRefreshClaim,
        now: DateTime<Utc>,
        next_due_at: DateTime<Utc>,
        failure: TeletextGuideRefreshFailure,
    ) -> Result<TeletextGenerationMutation<()>, Error> {
        self.guide_refresh_repository()?
            .fail(claim, now, next_due_at, failure)
            .await
            .map_err(|_| Error::StorageUnavailable)
    }

    pub async fn revoke_guide_refresh(
        &self,
        seed_id: EntityId,
        now: DateTime<Utc>,
    ) -> Result<TeletextGenerationMutation<()>, Error> {
        self.guide_refresh_repository()?
            .revoke(seed_id, now)
            .await
            .map_err(|_| Error::StorageUnavailable)
    }
}
