//! Native report acquisition from one complete, retained provider source.

use super::{
    auxiliary_command, headers, identities, local, optional_failure, teletext_probe,
    FfprobeExtractor, FfprobeOutput, LocalFileRevision, LocalMediaProbe, LocalTeletextAcquisition,
    LocalTeletextHeaderProblem, LocalTeletextHeaders, LocalTeletextSourceGuard,
    TeletextProbeEncoding, TeletextProbeSnapshot,
};
use crate::ffprobe::{
    FfprobeCommand, FfprobeError, LocalProbeCleanupScope, RetainedProviderSource,
};
use ersatzrs_core::locking::TeletextWorkHold;
use same_file::Handle;
use std::{path::Path, sync::Arc};
use tokio::time::Instant;

const LABEL: &str = "retained provider source native report";

/// All phases share the caller's absolute deadline and actual source owner.
/// A report grants no provider binding, database write or playback authority.
pub(super) async fn extract_retained_report(
    extractor: &FfprobeExtractor,
    source: Arc<RetainedProviderSource>,
    deadline: Instant,
    hold: TeletextWorkHold,
) -> crate::ffprobe::Result<LocalMediaProbe> {
    if LocalProbeCleanupScope::current().is_none() {
        return Err(FfprobeError::Cancelled { path: LABEL.into() });
    }
    let guard = open_current(source.clone(), None, deadline, hold.clone()).await?;
    let guarded = Arc::new(LocalTeletextSourceGuard {
        guard,
        retained_source: Some(source.clone()),
    });
    let primary = capture(
        extractor.command_for(source.path()),
        source.clone(),
        guarded.clone(),
        deadline,
        hold.clone(),
    )
    .await?;
    let media = crate::ffprobe::project_to_media_version(Path::new(LABEL), &primary);
    super::check_source_guard_owned(
        guarded.clone(),
        source.path().to_owned(),
        source.path().to_owned(),
        deadline,
        hold.clone(),
    )
    .await
    .map_err(|error| opaque(&error))?;
    let snapshot = teletext_probe::project(&primary, TeletextProbeEncoding::Ffprobe9Base64);
    let teletext = if identities(&snapshot).is_some_and(|ids| !ids.is_empty()) {
        match acquire(extractor, source, guarded, &snapshot, deadline, hold).await {
            Ok(acquired) => Some(Arc::new(acquired)),
            Err(error) => optional_failure(error)?,
        }
    } else {
        None
    };
    Ok(LocalMediaProbe { media, teletext })
}

async fn open_current(
    source: Arc<RetainedProviderSource>,
    expected: Option<Arc<LocalTeletextSourceGuard>>,
    deadline: Instant,
    hold: TeletextWorkHold,
) -> crate::ffprobe::Result<LocalFileRevision> {
    headers::run_source_guard_owned(deadline, hold, move |stop| {
        stop.check_source_guard()?;
        if expected
            .as_ref()
            .is_some_and(|guard| !guard.is_current(source.path()))
        {
            return Err(invalid());
        }
        let guard = LocalFileRevision::open(source.path()).map_err(|_| invalid())?;
        let input_handle = source.file.as_file().try_clone().map_err(|_| invalid())?;
        let input_handle = Handle::from_file(input_handle).map_err(|_| invalid())?;
        if guard.original != input_handle
            || guard.len != source.length
            || !guard.is_current(source.path())
            || expected
                .as_ref()
                .is_some_and(|expected| expected.guard.revision() != guard.revision())
        {
            return Err(invalid());
        }
        stop.check_source_guard()?;
        Ok(guard)
    })
    .await
    .map_err(|error| opaque(&error))
}

async fn capture(
    command: FfprobeCommand,
    source: Arc<RetainedProviderSource>,
    guarded: Arc<LocalTeletextSourceGuard>,
    deadline: Instant,
    hold: TeletextWorkHold,
) -> crate::ffprobe::Result<FfprobeOutput> {
    // Reuse the existing uncertain-cleanup branch: both the actual input and
    // guard (which owns the storage lease) survive an unconfirmed subprocess.
    let retention =
        local::ProbeRetention::new(hold, guarded).with_shared_snapshot(source.file.clone());
    let bytes = local::capture_retained(command, LABEL.into(), Some(deadline), Some(retention))
        .await
        .map_err(|error| opaque(&error))?;
    serde_json::from_slice(&bytes).map_err(|_| invalid())
}

async fn acquire(
    extractor: &FfprobeExtractor,
    source: Arc<RetainedProviderSource>,
    guarded: Arc<LocalTeletextSourceGuard>,
    primary: &TeletextProbeSnapshot,
    deadline: Instant,
    hold: TeletextWorkHold,
) -> crate::ffprobe::Result<LocalTeletextAcquisition> {
    let auxiliary = capture(
        auxiliary_command(extractor, source.path()),
        source.clone(),
        guarded.clone(),
        deadline,
        hold.clone(),
    )
    .await?;
    let snapshot = teletext_probe::project(&auxiliary, TeletextProbeEncoding::Ffprobe9Base64);
    if identities(primary) != identities(&snapshot) {
        return Err(invalid());
    }
    let guard = open_current(source.clone(), Some(guarded), deadline, hold.clone()).await?;
    let revision = guard.revision().ok_or_else(invalid)?;
    let mut acquired = LocalTeletextAcquisition {
        snapshot,
        revision,
        guard,
        headers: LocalTeletextHeaders::NotAcquired(LocalTeletextHeaderProblem::NoManagedScope),
    };
    headers::run_source_guard_owned(deadline, hold, move |stop| {
        stop.check_source_guard()?;
        acquired.headers = match headers::read_headers(&acquired, source.path(), stop) {
            Ok(headers) => LocalTeletextHeaders::Observed(headers),
            Err(problem) => LocalTeletextHeaders::NotAcquired(problem),
        };
        stop.check_source_guard()?;
        if !acquired.is_current(source.path()) {
            return Err(invalid());
        }
        stop.check_source_guard()?;
        Ok(acquired)
    })
    .await
    .map_err(|error| opaque(&error))
}

fn invalid() -> FfprobeError {
    FfprobeError::InvalidOutput { path: LABEL.into() }
}

fn opaque(error: &FfprobeError) -> FfprobeError {
    match error {
        FfprobeError::Cancelled { .. } => FfprobeError::Cancelled { path: LABEL.into() },
        FfprobeError::CleanupUnconfirmed { .. } => {
            FfprobeError::CleanupUnconfirmed { path: LABEL.into() }
        }
        FfprobeError::Timeout { .. } => FfprobeError::Timeout { path: LABEL.into() },
        FfprobeError::OutputTooLarge { .. } => FfprobeError::OutputTooLarge { path: LABEL.into() },
        _ => invalid(),
    }
}
