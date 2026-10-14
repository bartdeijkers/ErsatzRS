//! Optional source discovery on the existing final-output forwarding boundary.

use std::sync::atomic::AtomicU8;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::Notify,
    time::Instant,
};

use super::{
    retained_stream_buffer::{forward_retained_ts, ReserveError, ReservePolicy},
    teletext_binding::TeletextOutputBinding,
    teletext_optional_output::{
        forward_optional_seeded_retained_output, forward_optional_seeded_teletext_output,
        replay_direct_prefix, replay_retained_prefix, retained_startup_budget,
        SeededOptionalTeletextStartup,
    },
    teletext_output::TeletextPumpError,
    teletext_retained_output::{allocation_model, RetainedTeletextError},
    teletext_source_probe::{
        probe_teletext_source_optional, TeletextSourceProbeError, TeletextSourceProbeFailure,
        TeletextSourceProbePolicy,
    },
    teletext_startup::TeletextStartupPrefix,
};

async fn prepare_source(
    source: &mut (impl AsyncRead + Unpin),
    binding: &TeletextOutputBinding,
) -> Result<(TeletextOutputBinding, TeletextStartupPrefix), TeletextSourceProbeFailure> {
    let invalid = || TeletextSourceProbeFailure {
        raw_prefix: Vec::new(),
        reason: TeletextSourceProbeError::Configuration,
    };
    let discovery = binding.source_discovery().ok_or_else(invalid)?;
    let maximum_bytes = discovery.maximum_bytes;
    let maximum_time = discovery.maximum_time;
    let deadline = Instant::now()
        .checked_add(maximum_time)
        .ok_or_else(invalid)?;
    let descriptor = binding.optional_service_descriptor();
    let admitted = probe_teletext_source_optional(
        source,
        descriptor.as_ref(),
        binding.config().elementary_pid,
        TeletextSourceProbePolicy {
            maximum_bytes,
            deadline,
            first_byte_timeout: binding.first_byte_timeout(),
        },
    )
    .await?;
    match binding.with_discovered_source(admitted.plan.program_number(), admitted.plan.pcr_pid()) {
        Ok(binding) => Ok((binding, admitted.prefix)),
        Err(_) => Err(TeletextSourceProbeFailure {
            raw_prefix: admitted.prefix.into_bytes(),
            reason: TeletextSourceProbeError::Configuration,
        }),
    }
}

fn terminal_source_error(reason: &TeletextSourceProbeError) -> Option<TeletextPumpError> {
    match reason {
        TeletextSourceProbeError::Read(kind) => Some(TeletextPumpError::Read(*kind)),
        TeletextSourceProbeError::Empty => Some(TeletextPumpError::EmptyOutput),
        TeletextSourceProbeError::FirstByteTimeout => Some(TeletextPumpError::FirstByteTimeout),
        _ => None,
    }
}

pub(super) async fn forward_discovered_direct(
    source: &mut (impl AsyncRead + Unpin),
    sink: &mut (impl AsyncWrite + Unpin),
    binding: &TeletextOutputBinding,
) -> Result<u64, TeletextPumpError> {
    match prepare_source(source, binding).await {
        Ok((binding, prefix)) => {
            let maximum = binding
                .optional_startup_bytes()
                .ok_or(TeletextPumpError::Configuration)?;
            let result =
                forward_optional_seeded_teletext_output(source, sink, &binding, prefix, maximum)
                    .await?;
            if let Some(rejection) = result.rejection {
                tracing::debug!(%rejection, "discovered teletext startup rejected; preserving raw playback");
            }
            Ok(result.bytes)
        }
        Err(failure) => {
            if let Some(error) = terminal_source_error(&failure.reason) {
                return Err(error);
            }
            tracing::debug!(reason = %failure.reason, "teletext source discovery rejected; preserving raw playback");
            replay_direct_prefix(source, sink, failure.raw_prefix).await
        }
    }
}

pub(super) async fn forward_discovered_retained(
    source: &mut (impl AsyncRead + Unpin),
    sink: &mut (impl AsyncWrite + Unpin),
    policy: ReservePolicy,
    terminal: &AtomicU8,
    startup: Instant,
    cancel: &Notify,
    binding: &TeletextOutputBinding,
) -> Result<(), RetainedTeletextError> {
    let maximum = binding
        .optional_startup_bytes()
        .ok_or(RetainedTeletextError::Configuration)?;
    // Charge both the probe's owned capacity and the eventual startup journal
    // before reading any source bytes. The strict gate's separate model remains.
    if retained_discovery_budget(policy, binding, maximum).is_err() {
        tracing::debug!(
            "teletext discovery exceeds retained startup budget; preserving raw playback"
        );
        return forward_retained_ts(source, sink, policy, terminal, startup, cancel)
            .await
            .map_err(Into::into);
    }
    let prepared = tokio::select! {
        biased;
        () = cancel.notified() => return Err(ReserveError::Cancelled.into()),
        () = tokio::time::sleep_until(startup) => return Err(ReserveError::Deadline.into()),
        result = prepare_source(source, binding) => result,
    };
    match prepared {
        Ok((binding, prefix)) => {
            let result = forward_optional_seeded_retained_output(
                source,
                sink,
                policy,
                terminal,
                startup,
                cancel,
                SeededOptionalTeletextStartup {
                    binding: &binding,
                    maximum_bytes: maximum,
                    prefix,
                },
            )
            .await?;
            if let Some(rejection) = result.rejection {
                tracing::debug!(%rejection, "discovered retained teletext rejected; preserving raw playback");
            }
            Ok(())
        }
        Err(failure) => {
            if let Some(error) = terminal_source_error(&failure.reason) {
                return Err(match error {
                    TeletextPumpError::Read(kind) => RetainedTeletextError::Read(kind),
                    TeletextPumpError::EmptyOutput => ReserveError::InvalidOutput.into(),
                    _ => ReserveError::Deadline.into(),
                });
            }
            tracing::debug!(reason = %failure.reason, "retained teletext source discovery rejected; preserving raw playback");
            replay_retained_prefix(
                source,
                sink,
                policy,
                terminal,
                startup,
                cancel,
                failure.raw_prefix,
            )
            .await
            .map(|_| ())
        }
    }
}

fn retained_discovery_budget(
    policy: ReservePolicy,
    binding: &TeletextOutputBinding,
    journal: usize,
) -> Result<(), RetainedTeletextError> {
    retained_startup_budget(policy, binding, journal)?;
    let (base, _) = allocation_model(binding.config())?;
    let probe = binding
        .source_discovery()
        .ok_or(RetainedTeletextError::Configuration)?
        .maximum_bytes;
    base.checked_add(journal)
        .and_then(|bytes| bytes.checked_add(probe))
        .and_then(|bytes| bytes.checked_add(256 * 188))
        .filter(|bytes| *bytes <= 32 * 1024 * 1024)
        .ok_or(RetainedTeletextError::Configuration)?;
    Ok(())
}

#[cfg(test)]
#[path = "teletext_discovery_output_tests.rs"]
mod tests;
