//! Optional accepted-original recovery on the ordinary final direct stream.

use std::sync::Arc;

use tokio::time::{timeout_at, Instant};

use crate::host::{
    state::AppState,
    teletext_runtime_policy::{
        parse_native_teletext_runtime_policy, NATIVE_TELETEXT_RUNTIME_POLICY_SETTING,
    },
};

use super::super::{
    teletext_accepted_output::load_accepted_teletext_output_intent,
    teletext_binding::TeletextOutputAdmission, teletext_live_output::TeletextLiveOutputIntent,
    teletext_selected_output::load_selected_teletext_output_candidate,
};

/// No producer invocation, publication, queue or independently renewed startup
/// deadline. Optional recovery receives at most half the remaining startup time,
/// including configuration access; ordinary playback retains the other half.
pub(super) async fn recover_optional_output(
    state: &AppState,
    channel_id: i64,
    startup_deadline: Instant,
) -> Option<TeletextOutputAdmission> {
    let recovery_deadline = optional_deadline(Instant::now(), startup_deadline)?;
    let configured = timeout_at(
        recovery_deadline,
        state
            .application
            .get_optional_setting(NATIVE_TELETEXT_RUNTIME_POLICY_SETTING),
    )
    .await;
    let raw = match configured {
        Ok(Ok(Some(raw))) => raw,
        Ok(Ok(None)) => return None,
        _ => {
            tracing::warn!(
                channel_id,
                "teletext output configuration unavailable; preserving ordinary playback"
            );
            return None;
        }
    };
    let policy = match parse_native_teletext_runtime_policy(&raw) {
        Ok(policy) => policy,
        Err(error) => {
            tracing::warn!(channel_id, %error, "teletext output configuration rejected; preserving ordinary playback");
            return None;
        }
    };
    let now = Instant::now();
    let bounded_deadline = now
        .checked_add(policy.recovery_timeout)?
        .min(recovery_deadline);
    // Version 1 retains its startup-only path without cloning encoding policy.
    // Live output also needs the exact explicit policy for later owned recovery.
    let live = policy
        .live_updates
        .map(|timing| (timing, policy.encoding.clone()));
    if let Some((timing, encoding)) = live {
        let initial =
            if let Ok(candidate) = load_selected_teletext_output_candidate(
                &state.application,
                channel_id,
                policy.design,
                policy.encoding,
                bounded_deadline.into_std(),
            )
            .await
            {
                candidate
            } else {
                tracing::warn!(channel_id,
                "selected teletext recovery rejected; starting live output without authored pages");
                None
            };
        let intent = TeletextLiveOutputIntent::new_selected(
            Arc::clone(&state.application),
            channel_id,
            policy.design,
            encoding,
            policy.source,
            timing,
            initial,
        )
        .ok()?;
        tracing::debug!(
            target: "ersatzrs_web::transport::iptv::teletext_provenance",
            event = "ordinary_live_intent",
            intent_id = %intent.trace_id(),
            channel_id,
            "ordinary response owns this live teletext intent"
        );
        return Some(TeletextOutputAdmission::with_optional_live(
            Arc::new(intent),
            startup_deadline,
        ));
    }
    let recovered = load_accepted_teletext_output_intent(
        &state.application,
        channel_id,
        policy.design,
        policy.encoding,
        policy.source,
        bounded_deadline.into_std(),
    )
    .await;
    match recovered {
        Ok(Some(intent)) => TeletextOutputAdmission::with_optional_accepted(
            Arc::new(intent),
            Arc::clone(&state.application),
            startup_deadline,
        )
        .ok(),
        Ok(None) => None,
        Err(_) => {
            tracing::warn!(
                channel_id,
                "selected teletext recovery rejected; preserving ordinary playback"
            );
            None
        }
    }
}

fn optional_deadline(now: Instant, startup_deadline: Instant) -> Option<Instant> {
    let remaining = startup_deadline.checked_duration_since(now)?;
    let optional = remaining / 2;
    if optional.is_zero() {
        return None;
    }
    now.checked_add(optional)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn optional_recovery_preserves_half_of_the_existing_startup_deadline() {
        let now = Instant::now();
        let startup = now + Duration::from_secs(20);
        assert_eq!(
            optional_deadline(now, startup),
            Some(now + Duration::from_secs(10))
        );
        assert_eq!(
            optional_deadline(now + Duration::from_secs(16), startup),
            Some(now + Duration::from_secs(18))
        );
        assert_eq!(optional_deadline(startup, startup), None);
        assert_eq!(
            optional_deadline(startup + Duration::from_nanos(1), startup),
            None
        );
    }
}
