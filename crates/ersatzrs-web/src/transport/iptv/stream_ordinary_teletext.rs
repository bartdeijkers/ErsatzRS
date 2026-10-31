//! Optional accepted-original recovery on an ordinary final MPEG-TS stream.

use std::sync::Arc;

use tokio::time::{timeout_at, Instant};

use crate::host::{
    state::AppState,
    teletext_runtime_policy::{
        parse_native_teletext_runtime_policy, NativeTeletextRuntimePolicy,
        NATIVE_TELETEXT_RUNTIME_POLICY_SETTING,
    },
};

use super::super::{
    teletext_accepted_output::load_accepted_teletext_output_intent,
    teletext_binding::{TeletextOutputAdmission, TeletextOutputRole},
    teletext_live_output::TeletextLiveOutputIntent,
    teletext_selected_output::{
        load_selected_teletext_output_candidate,
        load_selected_teletext_output_candidate_with_guide, GuideRuntime,
    },
};

/// No producer invocation, publication, queue or independently renewed startup
/// deadline. Optional recovery receives at most half the remaining startup time,
/// including configuration access; ordinary playback retains the other half.
pub(super) async fn recover_optional_output(
    state: &AppState,
    channel_id: i64,
    role: TeletextOutputRole,
    startup_deadline: Instant,
) -> Option<TeletextOutputAdmission> {
    TeletextOutputAdmission::validate_optional_role(role).ok()?;
    let recovery_deadline = optional_deadline(Instant::now(), startup_deadline)?;
    let policy = configured_policy(state, channel_id, recovery_deadline).await?;
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
        let guide = policy.guide_limits.map(|limits| {
            Arc::new(GuideRuntime {
                pool: state.pool.clone(),
                limits,
            })
        });
        let recovered = if let Some(guide) = &guide {
            load_selected_teletext_output_candidate_with_guide(
                &state.application,
                channel_id,
                policy.design,
                policy.encoding,
                bounded_deadline.into_std(),
                Some(guide),
            )
            .await
        } else {
            load_selected_teletext_output_candidate(
                &state.application,
                channel_id,
                policy.design,
                policy.encoding,
                bounded_deadline.into_std(),
            )
            .await
        };
        let initial = if let Ok(candidate) = recovered {
            candidate
        } else {
            tracing::warn!(
                channel_id,
                "selected teletext recovery rejected; starting live output without authored pages"
            );
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
        .ok()?
        .with_guide_runtime(guide);
        tracing::debug!(
            target: "ersatzrs_web::transport::iptv::teletext_provenance",
            event = "ordinary_live_intent",
            intent_id = %intent.trace_id(),
            channel_id,
            "ordinary response owns this live teletext intent"
        );
        return TeletextOutputAdmission::with_optional_live(
            role,
            Arc::new(intent),
            startup_deadline,
        )
        .ok();
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
            role,
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

async fn configured_policy(
    state: &AppState,
    channel_id: i64,
    deadline: Instant,
) -> Option<NativeTeletextRuntimePolicy> {
    let configured = timeout_at(
        deadline,
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
    match parse_native_teletext_runtime_policy(&raw) {
        Ok(policy) => Some(policy),
        Err(error) => {
            tracing::warn!(channel_id, %error, "teletext output configuration rejected; preserving ordinary playback");
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

    #[tokio::test]
    async fn nonfinal_recovery_rejects_before_opening_configuration_storage() {
        let state = AppState::for_tests(crate::host::state::IptvAccessTokenVerifier::disabled());
        assert_eq!(state.pool.size(), 0);
        let deadline = Instant::now() + Duration::from_secs(20);
        for role in [
            TeletextOutputRole::InternalItem,
            TeletextOutputRole::HlsWorker,
            TeletextOutputRole::OtherContainer,
            TeletextOutputRole::CapacitySlate,
        ] {
            assert!(tokio::time::timeout(
                Duration::from_millis(50),
                recover_optional_output(&state, 1, role, deadline),
            )
            .await
            .expect("unsupported role must not wait on storage")
            .is_none());
            assert_eq!(state.pool.size(), 0, "unsupported role accessed storage");
        }
    }
}
