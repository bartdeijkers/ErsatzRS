use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Instant,
};

use chrono::Utc;
use ersatzrs_core::programme::ProgrammeClock;

use super::{indexed, AppliedPlayout, PreparedPlayout, PublicationError};
use crate::next_engine::protocol::Playout;

/// Publication format is chosen only after qualifying the matching Next reader.
#[derive(Debug, Clone, Default)]
pub enum PlayoutLayout {
    /// Current upstream-compatible complete atomic file.
    #[default]
    Complete,
    /// ADR 0131's explicitly negotiated daily index and immutable chunks.
    Indexed(Arc<ProgrammeClock>),
}

pub(in crate::next_engine) enum PreparedPublication {
    Complete(PreparedPlayout),
    Indexed(indexed::PreparedIndex),
}

pub(in crate::next_engine) struct AppliedPublication(Option<AppliedPlayout>);

impl AppliedPublication {
    pub(in crate::next_engine) fn accept(&mut self) {
        if let Some(file) = &mut self.0 {
            file.accept();
        }
    }
    pub(in crate::next_engine) fn restore(&mut self) -> Result<(), PublicationError> {
        if let Some(file) = &mut self.0 {
            file.restore()?;
        }
        Ok(())
    }
}

pub(in crate::next_engine) fn prepare_layout(
    folder: &Path,
    playout: &Playout,
    layout: &PlayoutLayout,
    is_current: impl Fn() -> bool,
    deadline: Option<Instant>,
) -> Result<PreparedPublication, PublicationError> {
    match layout {
        PlayoutLayout::Complete => {
            if let Some(deadline) = deadline {
                return super::prepare_playout_checked(folder, playout, is_current, deadline)
                    .map(PreparedPublication::Complete);
            }
            super::validate(playout)?;
            super::prepare_with(
                folder,
                |file| {
                    serde_json::to_writer(file, playout)
                        .map_err(|_| PublicationError::Serialization)
                },
                &is_current,
                deadline,
            )
            .map(PreparedPublication::Complete)
        }
        PlayoutLayout::Indexed(clock) => indexed::prepare(
            &folder.join("indexed"),
            playout,
            clock,
            Utc::now(),
            &is_current,
            deadline,
        )
        .map(PreparedPublication::Indexed),
    }
}

pub(in crate::next_engine) fn publish_layout(
    folder: &Path,
    playout: &Playout,
    layout: &PlayoutLayout,
    ready: &AtomicBool,
    is_current: impl Fn() -> bool,
    deadline: Option<Instant>,
    committed: impl FnOnce(),
) -> Result<(), PublicationError> {
    match layout {
        PlayoutLayout::Indexed(clock) if !ready.load(Ordering::Acquire) => {
            return recover_layout(
                folder,
                playout,
                clock,
                ready,
                &is_current,
                deadline,
                committed,
            );
        }
        _ => {}
    }
    let result = prepare_layout(folder, playout, layout, &is_current, deadline)
        .and_then(|prepared| prepared.publish(&is_current));
    match (layout, &result) {
        (PlayoutLayout::Indexed(clock), Err(error)) if requires_index_recovery(error) => {
            ready.store(false, Ordering::Release);
            recover_layout(
                folder,
                playout,
                clock,
                ready,
                &is_current,
                deadline,
                committed,
            )
        }
        _ => {
            result?;
            committed();
            Ok(())
        }
    }
}

fn recover_layout(
    folder: &Path,
    playout: &Playout,
    clock: &ProgrammeClock,
    ready: &AtomicBool,
    is_current: &impl Fn() -> bool,
    deadline: Option<Instant>,
    committed: impl FnOnce(),
) -> Result<(), PublicationError> {
    indexed::recover(
        &folder.join("indexed"),
        playout,
        clock,
        Utc::now(),
        is_current,
        deadline,
        committed,
    )?;
    ready.store(true, Ordering::Release);
    Ok(())
}

pub(in crate::next_engine) fn requires_index_recovery(error: &PublicationError) -> bool {
    matches!(
        error,
        PublicationError::InvalidIndex
            | PublicationError::RecoveryRequired
            | PublicationError::Io {
                operation: "inspect indexed file",
                kind: std::io::ErrorKind::NotFound
            }
    )
}

impl PreparedPublication {
    pub(in crate::next_engine) fn restrict_deadline(&mut self, deadline: Instant) {
        match self {
            Self::Complete(file) => file.restrict_deadline(deadline),
            Self::Indexed(index) => index.restrict_deadline(deadline),
        }
    }
    pub(in crate::next_engine) fn publish(
        self,
        is_current: impl Fn() -> bool,
    ) -> Result<(), PublicationError> {
        match self {
            Self::Complete(file) => file.publish(is_current),
            Self::Indexed(index) => index.publish(is_current),
        }
    }
    pub(in crate::next_engine) fn publish_reversible(
        self,
        is_current: impl Fn() -> bool,
    ) -> Result<AppliedPublication, PublicationError> {
        match self {
            Self::Complete(file) => file.publish_reversible(is_current).map(Some),
            Self::Indexed(index) => index.publish_reversible(is_current),
        }
        .map(AppliedPublication)
    }
}
