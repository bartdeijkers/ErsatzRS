//! Live publication readiness for the workspace publication summary.
//!
//! Restores the guarantee the island workspace had before the ADR 0091 task
//! redesign: an operator watching a running preparation sees the primary action
//! change from "Prepare publication" to "Publish saved changes" without
//! reloading the page. The button stays a native `form=addon-list-workspace`
//! `intent=publish` submit, so the no-JS fallback, the server's revision
//! validation and unsaved input elsewhere in the form are unchanged. The action,
//! state, counts and keyed blocker messages follow the same readiness snapshot.

use leptos::prelude::*;

use super::super::{
    publication_blocker_group_label, publication_blocker_groups, publication_primary_action, tr,
    AddonMediaListPublicationReadiness, AddonMediaListReviewState, PublicationPrimaryAction,
    TextKey,
};
use super::FORM;

/// Only the poll loop and its stop condition consult in-flight preparation, and
/// both are gated with their transition tests to keep native builds free of
/// dead code under `clippy -D warnings`.
#[cfg(any(target_arch = "wasm32", test))]
use super::super::publication_preparation_active;

/// Cadence while the server still owes this list preparation or publication work.
#[cfg(target_arch = "wasm32")]
const ACTIVE_POLL_MS: u32 = 1_500;

/// Cadence while the list is merely waiting for an operator decision. A queued
/// job can still be admitted without a page load, so this state is watched too,
/// just less eagerly.
#[cfg(target_arch = "wasm32")]
const IDLE_POLL_MS: u32 = 5_000;

/// Ceiling for the error backoff. A failing poll must never become a hot loop.
#[cfg(target_arch = "wasm32")]
const MAX_ERROR_DELAY_MS: u32 = 30_000;

/// Whether a durable publication job is still moving on its own.
#[cfg(any(target_arch = "wasm32", test))]
fn publication_job_active(readiness: &AddonMediaListPublicationReadiness) -> bool {
    matches!(
        publication_primary_action(false, readiness),
        PublicationPrimaryAction::Queued
            | PublicationPrimaryAction::Publishing
            | PublicationPrimaryAction::Restoring
    )
}

/// Whether further polling can still change the primary action.
///
/// The terminal condition follows durable work, never blockers alone: a blocker
/// can be cleared by background metadata resolution without any operator edit,
/// so a blocked list keeps a slow watch rather than ending it. The watch ends
/// only once nothing is in flight and the operator either has the action they
/// were waiting for or the list is already published.
#[cfg(any(target_arch = "wasm32", test))]
fn settled(readiness: &AddonMediaListPublicationReadiness) -> bool {
    if publication_preparation_active(readiness)
        || publication_job_active(readiness)
        || readiness.optional_detail_pending > 0
    {
        return false;
    }
    matches!(
        publication_primary_action(false, readiness),
        PublicationPrimaryAction::Publish | PublicationPrimaryAction::Save
    ) || readiness.review_state == AddonMediaListReviewState::Published
}

/// Publication state heading, matching the server-rendered summary exactly.
fn state_key(readiness: &AddonMediaListPublicationReadiness) -> TextKey {
    if readiness.review_state == AddonMediaListReviewState::Published {
        TextKey::AddonWorkspacePublished
    } else if !readiness.blockers.is_empty() {
        TextKey::PublicationBlocked
    } else {
        TextKey::AddonWorkspacePending
    }
}

/// Primary-action label: "Prepare publication" only while preparation is owed.
fn action_key(readiness: &AddonMediaListPublicationReadiness) -> TextKey {
    if matches!(
        publication_primary_action(false, readiness),
        PublicationPrimaryAction::Prepare
    ) {
        TextKey::PreparePublication
    } else {
        TextKey::AddonWorkspacePublish
    }
}

/// Whether the primary action can be submitted right now.
fn action_enabled(readiness: &AddonMediaListPublicationReadiness) -> bool {
    matches!(
        publication_primary_action(false, readiness),
        PublicationPrimaryAction::Prepare | PublicationPrimaryAction::Publish
    )
}

/// Live publication overview: state, counts and the native publish submit.
///
/// The overview nodes stay mounted; keyed blocker messages update through
/// `For`. Native inputs outside this island are never replaced.
#[island]
pub fn PublicationOverviewIsland(
    list_id: i64,
    initial: AddonMediaListPublicationReadiness,
) -> impl IntoView {
    let readiness = RwSignal::new(initial);

    #[cfg(target_arch = "wasm32")]
    {
        use crate::admin::server_fns::addon_media_lists::get_addon_media_list_publication_readiness;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        // `on_cleanup` takes `impl FnOnce() + Send + Sync + 'static`, so the
        // cancellation flag has to be `Arc<AtomicBool>` even though this task is
        // single-threaded on wasm.
        let cancelled = Arc::new(AtomicBool::new(false));
        on_cleanup({
            let cancelled = Arc::clone(&cancelled);
            move || cancelled.store(true, Ordering::Relaxed)
        });
        leptos::task::spawn_local(async move {
            // Seeded from the server-rendered value, so a list that is already
            // settled never issues a request.
            let mut delay = readiness.with_untracked(|current| {
                if publication_preparation_active(current) || publication_job_active(current) {
                    ACTIVE_POLL_MS
                } else {
                    IDLE_POLL_MS
                }
            });
            while !cancelled.load(Ordering::Relaxed) && !readiness.with_untracked(settled) {
                gloo_timers::future::TimeoutFuture::new(delay).await;
                if cancelled.load(Ordering::Relaxed) {
                    break;
                }
                match get_addon_media_list_publication_readiness(list_id).await {
                    Ok(next) => {
                        // The owner can be disposed while the request is in
                        // flight; never write a disposed signal.
                        if cancelled.load(Ordering::Relaxed) {
                            break;
                        }
                        delay = if publication_preparation_active(&next)
                            || publication_job_active(&next)
                        {
                            ACTIVE_POLL_MS
                        } else {
                            IDLE_POLL_MS
                        };
                        readiness.set(next);
                    }
                    // Conservative on failure: keep the last server-confirmed
                    // state rather than enabling or relabelling the action on a
                    // guess, and back off so a persistent outage stays cheap.
                    Err(_) => {
                        delay = delay.saturating_mul(2).min(MAX_ERROR_DELAY_MS);
                    }
                }
            }
        });
    }

    #[cfg(not(target_arch = "wasm32"))]
    let _ = list_id;

    view! {
        <div class="publication-overview">
            <div class="publication-state">
                <strong>{move || readiness.with(|r| tr(state_key(r)))}</strong>
                <span>
                    {move || {
                        readiness
                            .with(|r| {
                                format!(
                                    "{} {} · {} {}",
                                    r.included_count,
                                    tr(TextKey::IncludeInPublication),
                                    r.blockers.len(),
                                    tr(TextKey::PublicationBlockers),
                                )
                            })
                    }}
                </span>
            </div>
            <button
                class="button primary"
                form=FORM
                name="intent"
                value="publish"
                disabled=move || readiness.with(|r| !action_enabled(r))
                formnovalidate
            >
                {move || readiness.with(|r| tr(action_key(r)))}
            </button>
        </div>
        <For
            each=move || readiness.with(|r| publication_blocker_groups(&r.blockers))
            key=|group| format!("{:?}:{}", group.kind, group.item_scoped)
            children=move |group| view! {
                <p class="notice error">{publication_blocker_group_label(&group)}</p>
            }
        />
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin::server_fns::addon_media_lists::{
        AddonMediaListPublicationBlocker, AddonMediaListPublicationBlockerKind,
    };

    #[test]
    fn publication_poll_waits_for_background_resolution_of_blockers() {
        let mut readiness = AddonMediaListPublicationReadiness {
            blockers: vec![AddonMediaListPublicationBlocker {
                kind: AddonMediaListPublicationBlockerKind::ItemTitleMissing,
                item_rank: Some(0),
                item_title: None,
                count: 1,
            }],
            required_playback_detail_pending: 1,
            ..Default::default()
        };
        assert!(!settled(&readiness));
        assert!(!action_enabled(&readiness));
        readiness.required_playback_detail_pending = 0;
        assert!(!settled(&readiness));
        readiness.blockers.clear();
        readiness.required_prepared_media_count = 1;
        assert!(!settled(&readiness));
        assert_eq!(action_key(&readiness), TextKey::PreparePublication);
        readiness.required_prepared_media_count = 0;
        assert!(settled(&readiness));
        assert!(action_enabled(&readiness));
        readiness.optional_detail_pending = 1;
        assert!(!settled(&readiness));
    }
}
