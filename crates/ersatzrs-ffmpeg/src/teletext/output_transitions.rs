//! Nonterminal proposals against an exact sink-acknowledged physical boundary.

use super::{
    Arc, Duration, EncodedTeletextAuthoredService, TeletextCarousel, TeletextOutputAcknowledgement,
    TeletextOutputError, TeletextOutputGate, TeletextPsiError, TeletextServiceDescriptor,
};

impl TeletextOutputGate {
    /// Offer the first complete-cycle end within a bounded ordinary window.
    /// This never retains an entire unbounded cycle or releases source bytes.
    /// PMT fragments/capacity still require normal preparation; a caller must
    /// not retry an unchanged capped prefix indefinitely after false readiness.
    /// # Errors
    /// Preserves policy, source, cursor and arithmetic errors without mutation.
    pub fn transition_window_end(
        &self,
        intervals: usize,
    ) -> Result<Option<u64>, TeletextOutputError> {
        let end = self.window_end(intervals)?;
        if end.is_none() || !self.payload.active || !self.payload.signalling_ready {
            return Ok(end);
        }
        let intervals = self
            .carousel
            .as_ref()
            .ok_or(TeletextOutputError::Configuration)?
            .batches_until_boundary(intervals)?
            .unwrap_or(intervals);
        self.window_end(intervals)
    }

    /// Whether this gate is currently waiting to sink-acknowledge changed PSI.
    /// False is not independent receiver or initial-publication proof.
    #[must_use]
    pub const fn signalling_transition_pending(&self) -> bool {
        !self.payload.signalling_ready
    }

    /// Logical authored payload selection; actual delivery still requires ACK.
    #[must_use]
    pub const fn authored_payload_selected(&self) -> bool {
        self.payload.active
    }

    /// Replace only at this exact acknowledged end-of-cycle boundary.
    /// Complete native content/resource lifecycle and continuing source ownership
    /// are retained. Changed PSI is announced and acknowledged before new PES.
    /// Neither the receipt nor this method grants durable generation authority.
    /// # Errors
    /// Invalid proposals preserve the current generation/PSI/counters. Actual
    /// source/deadline failures retain the normal terminal output fence.
    pub fn replace_authored_at_acknowledged_boundary(
        &mut self,
        receipt: &TeletextOutputAcknowledgement,
        service: &Arc<EncodedTeletextAuthoredService>,
        now: Duration,
    ) -> Result<(), TeletextOutputError> {
        self.check_transition(receipt, now)?;
        self.check_warm_metadata(service)?;
        let first = if self.carousel.is_none() {
            let mut carousel = TeletextCarousel::new_authored_shared(Arc::clone(service))?;
            carousel.advance_empty_at_boundary(self.batch_ticks)?;
            Some(carousel)
        } else {
            None
        };
        let prepared = self
            .carousel
            .as_ref()
            .map(|carousel| {
                carousel.prepare_authored_shared_replacement_at_boundary(Arc::clone(service))
            })
            .transpose()?;
        let descriptor = TeletextServiceDescriptor::from_authored_service(service);
        let changed = !self.payload.active || self.service.as_ref() != Some(&descriptor);
        if changed {
            let installer = self
                .installer
                .as_mut()
                .ok_or(TeletextOutputError::Configuration)?;
            let plan = installer.prepare_reconfiguration(Some(&descriptor))?;
            // The installer rejects without mutation. The sealed carousel commit
            // is infallible and no cursor/history work intervenes after preflight.
            installer.reconfigure(plan)?;
        }
        if let (Some(carousel), Some(prepared)) = (self.carousel.as_mut(), prepared) {
            carousel.commit_prepared_replacement(prepared);
        } else {
            self.carousel = first;
        }
        self.service = Some(descriptor);
        self.payload.active = true;
        self.payload.signalling_ready = !changed;
        self.completed_cycle = false;
        self.last_ack = None;
        Ok(())
    }

    /// Stop PES and withdraw only the owned PMT entry, preserving AV/PCR positions.
    /// The next window must advertise the changed table before acknowledgement.
    /// This complete-cycle primitive does not settle host immediate-disable timing
    /// or prove that a receiver has forgotten cached pages/magazine metadata.
    /// # Errors
    /// Rejects foreign/stale/mid-cycle/pending/expired boundaries or invalid PSI.
    /// A rejected proposal does not invalidate the continuing current service.
    pub fn withdraw_at_acknowledged_boundary(
        &mut self,
        receipt: &TeletextOutputAcknowledgement,
        now: Duration,
    ) -> Result<(), TeletextOutputError> {
        self.check_transition(receipt, now)?;
        if !self.payload.active {
            return Ok(());
        }
        let installer = self
            .installer
            .as_mut()
            .ok_or(TeletextOutputError::Configuration)?;
        let plan = installer.prepare_reconfiguration(None)?;
        installer.reconfigure(plan)?;
        self.payload.active = false;
        self.payload.signalling_ready = false;
        self.last_ack = None;
        Ok(())
    }

    fn check_transition(
        &mut self,
        receipt: &TeletextOutputAcknowledgement,
        now: Duration,
    ) -> Result<(), TeletextOutputError> {
        self.check_deadline(now)?;
        self.ensure_source_open()?;
        if self.pending.is_some() {
            return Err(TeletextOutputError::Pending);
        }
        if !Arc::ptr_eq(&self.epoch, &receipt.epoch)
            || !self
                .last_ack
                .as_ref()
                .is_some_and(|stamp| Arc::ptr_eq(stamp, &receipt.stamp))
            || receipt.end_packet != self.source_start
            || receipt.batch_ticks != self.batch_ticks
        {
            return Err(TeletextOutputError::Acknowledgement);
        }
        if !receipt.cycle_boundary
            || !self
                .carousel
                .as_ref()
                .is_none_or(TeletextCarousel::is_cycle_boundary)
            || !self.payload.signalling_ready
        {
            return Err(TeletextOutputError::Boundary);
        }
        if !self.source_ready()? {
            return Err(TeletextPsiError::Incomplete.into());
        }
        Ok(())
    }

    fn check_warm_metadata(
        &self,
        next: &EncodedTeletextAuthoredService,
    ) -> Result<(), TeletextOutputError> {
        let Some(previous) = &self.warm_service else {
            return Ok(());
        };
        let old = previous.declared();
        let new = next.declared();
        if old
            .semantic()
            .magazine_presentations()
            .keys()
            .any(|magazine| {
                !new.semantic()
                    .magazine_presentations()
                    .contains_key(magazine)
            })
            || old.mots().iter().any(|mot| {
                !new.mots().iter().any(|other| {
                    mot.identity().page().magazine() == other.identity().page().magazine()
                })
            })
        {
            return Err(TeletextOutputError::CacheReconciliationRequired);
        }
        Ok(())
    }
}
