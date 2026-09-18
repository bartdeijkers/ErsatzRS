//! Automatic saved-revision progress and explicit recovery for failed work.
//! Native forms save intent; publication is owned by the durable worker.

use leptos::prelude::*;
use std::fmt::Write as _;

#[cfg(any(target_arch = "wasm32", test))]
use super::super::AddonMediaListReviewState;
use super::super::{
    publication_blocker_group_label, publication_blocker_groups, publication_preparation_active,
    publication_primary_action, publication_state_text_key, tr, AddonMediaListPublicationReadiness,
    PublicationPrimaryAction, TextKey,
};
use super::FORM;
use crate::admin::server_fns::addon_media_lists::AddonMediaListPreparationFailureReason;
use ersatzrs_domain::AddonMediaListPublicationOutcome;

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
    readiness.review_state == AddonMediaListReviewState::Published
}

/// Operator guidance for one closed preparation failure category.
///
/// `InputChanged` is also recorded by older hosts for requests that were
/// invalid, so its text never claims that the operator changed anything.
pub(super) const fn preparation_failure_text_key(
    reason: AddonMediaListPreparationFailureReason,
) -> TextKey {
    match reason {
        AddonMediaListPreparationFailureReason::Failed => TextKey::PreparationFailureFailed,
        AddonMediaListPreparationFailureReason::InputChanged => {
            TextKey::PreparationFailureInputChanged
        }
        AddonMediaListPreparationFailureReason::InvalidRequest => {
            TextKey::PreparationFailureInvalidRequest
        }
        AddonMediaListPreparationFailureReason::Cancelled => TextKey::PreparationFailureCancelled,
        AddonMediaListPreparationFailureReason::AttemptsExhausted => {
            TextKey::PreparationFailureAttemptsExhausted
        }
        AddonMediaListPreparationFailureReason::StorageFull => {
            TextKey::PreparationFailureStorageFull
        }
        AddonMediaListPreparationFailureReason::ProcessRecoveryRequired => {
            TextKey::PreparationProcessRecoveryRequired
        }
        AddonMediaListPreparationFailureReason::Unsupported => {
            TextKey::PreparationFailureUnsupported
        }
    }
}

/// Distinct failure reasons for the saved revision, in server order.
///
/// Rows from earlier saved revisions are Activity history only; they must never
/// turn a current heading into a failure.
fn current_failure_reasons(
    readiness: &AddonMediaListPublicationReadiness,
) -> Vec<AddonMediaListPreparationFailureReason> {
    let mut reasons = Vec::new();
    if readiness.required_prepared_media_failed == 0 {
        return reasons;
    }
    for failure in &readiness.prepared_media_failures {
        if failure.saved_revision == readiness.saved_revision && !reasons.contains(&failure.reason)
        {
            reasons.push(failure.reason);
        }
    }
    reasons
}

/// Whether the primary action is a retry after failed preparation.
fn preparation_retry(readiness: &AddonMediaListPublicationReadiness) -> bool {
    readiness.required_prepared_media_failed > 0
        && matches!(
            publication_primary_action(false, readiness),
            PublicationPrimaryAction::Prepare
        )
}

/// Publication state heading, derived from the same decision as the primary
/// action so the two can never disagree.
///
/// Zero blockers only means the saved metadata is publishable. While media must
/// still be prepared, the heading says so instead of "ready to publish".
fn state_key(readiness: &AddonMediaListPublicationReadiness) -> TextKey {
    if readiness.saved_revision < 0 {
        return TextKey::AddonWorkspaceLoadIncomplete;
    }
    if restored_failure(readiness) {
        return TextKey::AddonWorkspacePublicationFailed;
    }
    match publication_primary_action(false, readiness) {
        PublicationPrimaryAction::Queued
        | PublicationPrimaryAction::Publishing
        | PublicationPrimaryAction::Restoring => readiness
            .publication
            .as_ref()
            .map_or(TextKey::Publishing, |publication| {
                publication_state_text_key(publication.state)
            }),
        PublicationPrimaryAction::PublishBlocked => TextKey::PublishBlocked,
        PublicationPrimaryAction::Prepare if preparation_retry(readiness) => {
            TextKey::AddonWorkspacePreparationFailed
        }
        PublicationPrimaryAction::Prepare => TextKey::AddonWorkspacePreparationRequired,
        PublicationPrimaryAction::Publish | PublicationPrimaryAction::Save => {
            TextKey::AddonWorkspacePending
        }
        PublicationPrimaryAction::None if !readiness.blockers.is_empty() => {
            TextKey::PublicationBlocked
        }
        PublicationPrimaryAction::None if publication_preparation_active(readiness) => {
            TextKey::AddonWorkspacePreparing
        }
        PublicationPrimaryAction::None => TextKey::AddonWorkspacePublished,
    }
}

fn restored_failure(readiness: &AddonMediaListPublicationReadiness) -> bool {
    readiness.publication.as_ref().is_some_and(|p| {
        p.saved_revision == readiness.saved_revision
            && p.outcome == Some(AddonMediaListPublicationOutcome::RestoredAfterFailure)
    })
}

/// Optional recovery label after preparation or publication failed.
fn action_key(readiness: &AddonMediaListPublicationReadiness) -> TextKey {
    if restored_failure(readiness) {
        TextKey::Retry
    } else {
        TextKey::RetryPreparation
    }
}

/// Optional recovery is available only after a technical failure.
fn action_enabled(readiness: &AddonMediaListPublicationReadiness) -> bool {
    preparation_retry(readiness)
        || (restored_failure(readiness)
            && !publication_job_active(readiness)
            && readiness.blockers.is_empty())
}

/// Counts shown beside the heading. Media still to prepare is listed next to
/// the blocker count, so "0 blockers" is never read as "prepared".
fn counts_text(readiness: &AddonMediaListPublicationReadiness) -> String {
    let mut text = format!(
        "{} {} · {} {}",
        readiness.included_count,
        tr(TextKey::IncludeInPublication),
        readiness.blockers.len(),
        tr(TextKey::PublicationBlockers),
    );
    if readiness.required_prepared_media_count > 0 {
        write!(
            text,
            " · {}: {}",
            tr(TextKey::ProcessedMediaRequired),
            readiness.required_prepared_media_count
        )
        .expect("writing a string is infallible");
    }
    text
}

/// Whether preparation counters describe work the operator may need to watch.
/// Finished history stays on Activity & recovery instead of pushing the item
/// list below the fold on Items.
fn preparation_needs_attention(readiness: &AddonMediaListPublicationReadiness) -> bool {
    let p = &readiness.preparation;
    publication_preparation_active(readiness)
        || p.queued_tasks > 0
        || p.running_tasks > 0
        || p.waiting_tasks > 0
        || p.unqueued_tasks > 0
        || p.failed_tasks > 0
}

/// Whether the latest publication job still needs to be shown on Items: it is
/// in flight, stopped without completing, or completed only by restoring the
/// previous revision after a failure.
fn publication_needs_attention(readiness: &AddonMediaListPublicationReadiness) -> bool {
    use ersatzrs_domain::AddonMediaListPublicationState;
    readiness.publication.as_ref().is_some_and(|p| {
        p.state != AddonMediaListPublicationState::Complete
            || p.outcome == Some(AddonMediaListPublicationOutcome::RestoredAfterFailure)
    })
}

/// Failed count shown in the preparation failure notice.
fn failed_count_text(readiness: &AddonMediaListPublicationReadiness) -> String {
    format!(
        "{}: {}",
        tr(TextKey::PreparationFailedItems),
        readiness.required_prepared_media_failed
    )
}

/// Live publication overview: state, counts and the native publish submit.
///
/// The overview nodes stay mounted; keyed blocker and failure messages update
/// through `For`. Native inputs outside this island are never replaced.
#[island]
pub fn PublicationOverviewIsland(
    list_id: i64,
    initial: AddonMediaListPublicationReadiness,
) -> impl IntoView {
    let readiness = RwSignal::new(initial);
    watch_readiness(list_id, readiness);
    overview_view(list_id, readiness)
}

fn watch_readiness(list_id: i64, readiness: RwSignal<AddonMediaListPublicationReadiness>) {
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (list_id, readiness);
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
}

/// Preparation counters. `only_attention` limits them to unfinished or failed
/// work (Items); Activity passes `false` to keep the complete history.
fn preparation_view(
    readiness: RwSignal<AddonMediaListPublicationReadiness>,
    only_attention: bool,
) -> impl IntoView {
    use super::super::{elapsed_between, format_progress_timestamp};
    view! {
        <section class="settings-section" data-testid="addon-preparation-tasks" hidden=move || readiness.with(|r| r.preparation.affected_items == 0 || (only_attention && !preparation_needs_attention(r)))>
            <h3>{tr(TextKey::PreparationTasks)}</h3>
            <dl class="facts">
                {[(TextKey::PreparationAffectedItems,0),(TextKey::PreparationIntervals,1),(TextKey::PreparationDownloads,2),(TextKey::PreparationSubtitles,3),(TextKey::PreparationQueued,4),(TextKey::PreparationRunning,5),(TextKey::PreparationWaiting,6),(TextKey::Completed,7),(TextKey::Failed,8),(TextKey::PreparationNotQueued,9)].into_iter().map(|(key,index)| view! {
                    <div><dt>{tr(key)}</dt><dd>{move || readiness.with(|r| {
                        let p=&r.preparation;
                        [p.affected_items,p.interval_tasks,p.download_tasks,p.subtitle_tasks,p.queued_tasks,p.running_tasks,p.waiting_tasks,p.completed_tasks,p.failed_tasks,p.unqueued_tasks][index]
                    })}</dd></div>
                }).collect_view()}
                <div><dt>{tr(TextKey::ElapsedTime)}</dt><dd>{move || readiness.with(|r| r.preparation.started_at.as_deref().and_then(|at| elapsed_between(at,r.preparation.completed_at.as_deref())).unwrap_or_else(||tr(TextKey::NotAvailable).into()))}</dd></div>
                <div><dt>{tr(TextKey::LastActivity)}</dt><dd>{move || readiness.with(|r| r.preparation.last_activity_at.as_deref().and_then(format_progress_timestamp).unwrap_or_else(||tr(TextKey::NotAvailable).into()))}</dd></div>
                <div><dt>{tr(TextKey::LastProgress)}</dt><dd>{move || readiness.with(|r| r.preparation.last_progress_at.as_deref().and_then(format_progress_timestamp).unwrap_or_else(||tr(TextKey::NotAvailable).into()))}</dd></div>
                <div hidden=move || readiness.with(|r| r.preparation.retry_at.is_none())><dt>{tr(TextKey::RefreshRetryAt)}</dt><dd>{move || readiness.with(|r| r.preparation.retry_at.as_deref().and_then(format_progress_timestamp).unwrap_or_default())}</dd></div>
            </dl>
        </section>
    }
}

fn overview_view(
    list_id: i64,
    readiness: RwSignal<AddonMediaListPublicationReadiness>,
) -> impl IntoView {
    view! {
        {preparation_view(readiness, true)}
        <div class="publication-overview">
            <div class="publication-state">
                <strong>{move || readiness.with(|r| tr(state_key(r)))}</strong>
                <span>{move || readiness.with(counts_text)}</span>
            </div>
            <button
                class="button primary"
                form=FORM
                name="intent"
                value="publish"
                hidden=move || readiness.with(|r| !action_enabled(r))
                disabled=move || readiness.with(|r| !action_enabled(r))
                formnovalidate
            >
                {move || readiness.with(|r| tr(action_key(r)))}
            </button>
        </div>
        <PublicationRecovery readiness list_id only_attention=true/>
        <div
            class="notice error"
            data-testid="addon-preparation-failed"
            hidden=move || readiness.with(|r| r.required_prepared_media_failed == 0)
        >
            <p>{move || readiness.with(failed_count_text)}</p>
            <For
                each=move || readiness.with(current_failure_reasons)
                key=|reason| format!("{reason:?}")
                children=move |reason| view! { <p>{tr(preparation_failure_text_key(reason))}</p> }
            />
            <button
                class="button secondary"
                form=FORM
                name="intent"
                value="navigate"
                formaction=format!("/media/addon-lists/{list_id}/activity")
                formnovalidate
            >
                {tr(TextKey::AddonWorkspaceActivity)}
            </button>
        </div>
        <For
            each=move || readiness.with(|r| r.blockers.iter().filter_map(|blocker| blocker.item_id.map(|id| (id, blocker.clone()))).collect::<Vec<_>>())
            key=|(id, blocker)| format!("{id}:{:?}", blocker.kind)
            children=move |(id, blocker)| view! {
                <p class="notice error"><button class="button secondary" form=FORM name="intent" value="navigate" formaction=format!("/media/addon-lists/{list_id}/items/{id}") formnovalidate>
                    {format!("{}: {}", blocker.item_title.unwrap_or_else(|| tr(TextKey::Items).to_owned()), tr(super::super::publication_blocker_text_key(blocker.kind)))}
                </button></p>
            }
        />
        <For
            each=move || readiness.with(|r| publication_blocker_groups(&r.blockers.iter().filter(|blocker| blocker.item_id.is_none()).cloned().collect::<Vec<_>>()))
            key=|group| format!("{:?}:{}", group.kind, group.item_scoped)
            children=move |group| view! {
                <p class="notice error">{publication_blocker_group_label(&group)}</p>
            }
        />
    }
}

/// Activity uses the same live recovery projection as Items, with native actions.
#[island]
pub fn PublicationRecoveryIsland(
    list_id: i64,
    initial: AddonMediaListPublicationReadiness,
) -> impl IntoView {
    let readiness = RwSignal::new(initial);
    watch_readiness(list_id, readiness);
    view! { <PublicationRecovery readiness list_id only_attention=false/>{preparation_view(readiness, false)} }
}

/// Latest publication job with its recovery actions. With `only_attention`
/// (Items) a successfully completed job is hidden; Activity always shows it.
#[component]
fn PublicationRecovery(
    readiness: RwSignal<AddonMediaListPublicationReadiness>,
    list_id: i64,
    only_attention: bool,
) -> impl IntoView {
    use super::super::publication_phase_text_key;
    view! {
        <section class=move || readiness.with(|r| if r.publication.as_ref().is_some_and(|p| p.outcome == Some(AddonMediaListPublicationOutcome::RestoredAfterFailure)) {"notice error"} else {"notice"}) hidden=move || readiness.with(|r| r.publication.is_none() || (only_attention && !publication_needs_attention(r))) data-testid="addon-publication-recovery">
            <strong>{move || readiness.with(|r| r.publication.as_ref().map(|p| tr(if p.outcome == Some(AddonMediaListPublicationOutcome::RestoredAfterFailure) {TextKey::AddonWorkspacePublicationFailed} else {publication_state_text_key(p.state)})))}</strong>
            <p>{move || readiness.with(|r| r.publication.as_ref().map(|p| tr(if p.outcome == Some(AddonMediaListPublicationOutcome::RestoredAfterFailure) {TextKey::AddonWorkspacePublicationRestoredHelp} else {publication_phase_text_key(p.phase)})))}</p>
            <p hidden=move || readiness.with(|r| !r.publication.as_ref().is_some_and(|p| p.outcome == Some(AddonMediaListPublicationOutcome::RestoredAfterFailure) && p.diagnostic_code.as_deref() == Some("parent_nfo_preservation_required")))>{tr(TextKey::AddonWorkspaceParentMetadataPreserved)}</p>
            <p>{move || readiness.with(|r| r.publication.as_ref().and_then(|p| p.diagnostic_code.clone()))}</p>
            <p>{tr(TextKey::PublicationAttempts)}": "{move || readiness.with(|r| r.publication.as_ref().map(|p| p.attempt_count))}</p>
            <p>{tr(TextKey::LastProgress)}": "{move || readiness.with(|r| r.publication.as_ref().map(|p| p.last_progress_at.clone()))}</p>
            <input form=FORM type="hidden" name="job_id" value=move || readiness.with(|r| r.publication.as_ref().map(|p| p.job_id.to_string()).unwrap_or_default())/>
            <input form=FORM type="hidden" name="generation" value=move || readiness.with(|r| r.publication.as_ref().map(|p| p.generation.to_string()).unwrap_or_default())/>
            <button class="button secondary" form=FORM name="intent" value="recover-retry" disabled=move || readiness.with(|r| !r.publication.as_ref().is_some_and(|p| p.can_retry)) formnovalidate>{tr(TextKey::Retry)}</button>
            <button class="button secondary" form=FORM name="intent" value="recover-restore" disabled=move || readiness.with(|r| !r.publication.as_ref().is_some_and(|p| p.can_restore)) formnovalidate>{tr(TextKey::RestorePreviousRevision)}</button>
            <button class="button secondary" form=FORM name="intent" value="navigate" formaction=format!("/media/addon-lists/{list_id}/activity") formnovalidate>{tr(TextKey::AddonWorkspaceActivity)}</button>
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin::server_fns::addon_media_lists::{
        AddonMediaListPreparationFailure, AddonMediaListPublicationBlocker,
        AddonMediaListPublicationBlockerKind,
    };

    #[test]
    fn restored_outcome_is_not_success_and_old_failure_does_not_override_new_draft() {
        use super::super::super::{
            AddonMediaListPublicationPhase, AddonMediaListPublicationState,
            AddonMediaListPublicationStatus,
        };
        let mut r = AddonMediaListPublicationReadiness {
            saved_revision: 4,
            publication: Some(AddonMediaListPublicationStatus {
                job_id: 1,
                operation_id: 1,
                generation: 1,
                saved_revision: 4,
                target_published_revision: 1,
                phase: AddonMediaListPublicationPhase::Complete,
                state: AddonMediaListPublicationState::Complete,
                outcome: Some(AddonMediaListPublicationOutcome::RestoredAfterFailure),
                attempt_count: 1,
                last_progress_at: String::new(),
                diagnostic_code: Some("publication_commit_failed".into()),
                recovery_diagnostic_present: false,
                can_retry: false,
                can_restore: false,
            }),
            ..Default::default()
        };
        assert_eq!(state_key(&r), TextKey::AddonWorkspacePublicationFailed);
        assert!(
            action_enabled(&r),
            "a technical failure offers an optional retry after restoration"
        );
        r.saved_revision = 5;
        assert!(!restored_failure(&r));
        r.saved_revision = 4;
        r.publication.as_mut().unwrap().outcome = Some(AddonMediaListPublicationOutcome::Published);
        assert!(
            !restored_failure(&r),
            "retained original diagnostics do not override a successful retry"
        );
    }

    #[test]
    fn items_hide_finished_history_but_keep_work_that_needs_attention() {
        use super::super::super::{
            AddonMediaListPublicationPhase, AddonMediaListPublicationState,
            AddonMediaListPublicationStatus,
        };
        let mut r = AddonMediaListPublicationReadiness::default();
        r.preparation.affected_items = 156;
        r.preparation.subtitle_tasks = 156;
        r.preparation.completed_tasks = 156;
        assert!(
            !preparation_needs_attention(&r),
            "finished preparation is Activity history"
        );
        for pending in [
            |r: &mut AddonMediaListPublicationReadiness| r.preparation.queued_tasks = 1,
            |r: &mut AddonMediaListPublicationReadiness| r.preparation.running_tasks = 1,
            |r: &mut AddonMediaListPublicationReadiness| r.preparation.waiting_tasks = 1,
            |r: &mut AddonMediaListPublicationReadiness| r.preparation.unqueued_tasks = 1,
            |r: &mut AddonMediaListPublicationReadiness| r.preparation.failed_tasks = 1,
            |r: &mut AddonMediaListPublicationReadiness| r.required_prepared_media_pending = 1,
        ] {
            let mut attention = r.clone();
            pending(&mut attention);
            assert!(preparation_needs_attention(&attention));
        }

        let job = |state, outcome| AddonMediaListPublicationStatus {
            job_id: 1,
            operation_id: 1,
            generation: 1,
            saved_revision: 4,
            target_published_revision: 1,
            phase: AddonMediaListPublicationPhase::Complete,
            state,
            outcome,
            attempt_count: 1,
            last_progress_at: String::new(),
            diagnostic_code: None,
            recovery_diagnostic_present: false,
            can_retry: false,
            can_restore: true,
        };
        assert!(!publication_needs_attention(&r));
        r.publication = Some(job(
            AddonMediaListPublicationState::Complete,
            Some(AddonMediaListPublicationOutcome::Published),
        ));
        assert!(
            !publication_needs_attention(&r),
            "a successful job is history"
        );
        for (state, outcome) in [
            (AddonMediaListPublicationState::Queued, None),
            (AddonMediaListPublicationState::Publishing, None),
            (AddonMediaListPublicationState::Failed, None),
            (AddonMediaListPublicationState::Expired, None),
            (
                AddonMediaListPublicationState::Complete,
                Some(AddonMediaListPublicationOutcome::RestoredAfterFailure),
            ),
        ] {
            r.publication = Some(job(state, outcome));
            assert!(publication_needs_attention(&r), "{state:?} {outcome:?}");
        }
    }

    #[test]
    fn publication_poll_waits_for_background_resolution_of_blockers() {
        let mut readiness = AddonMediaListPublicationReadiness {
            blockers: vec![AddonMediaListPublicationBlocker {
                item_id: None,
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
        assert_eq!(action_key(&readiness), TextKey::RetryPreparation);
        readiness.required_prepared_media_count = 0;
        assert!(!settled(&readiness));
        assert!(!action_enabled(&readiness));
        readiness.optional_detail_pending = 1;
        assert!(!settled(&readiness));
    }

    #[test]
    fn publication_heading_agrees_with_the_primary_action() {
        // The reported shape: a saved draft without blockers whose 14 media
        // files still have to be prepared.
        let mut readiness = AddonMediaListPublicationReadiness {
            included_count: 14,
            required_prepared_media_count: 14,
            saved_revision: 8,
            review_state: AddonMediaListReviewState::Draft,
            ..Default::default()
        };
        assert_eq!(
            state_key(&readiness),
            TextKey::AddonWorkspacePreparationRequired
        );
        assert_eq!(action_key(&readiness), TextKey::RetryPreparation);
        assert!(!action_enabled(&readiness));
        assert!(counts_text(&readiness).contains(": 14"));

        readiness.required_prepared_media_pending = 14;
        assert_eq!(state_key(&readiness), TextKey::AddonWorkspacePreparing);
        assert!(!action_enabled(&readiness));

        readiness.blockers.push(AddonMediaListPublicationBlocker {
            item_id: None,
            kind: AddonMediaListPublicationBlockerKind::NoIncludedItems,
            item_rank: None,
            item_title: None,
            count: 0,
        });
        assert_eq!(state_key(&readiness), TextKey::PublicationBlocked);
        readiness.blockers.clear();

        readiness.required_prepared_media_count = 0;
        readiness.required_prepared_media_pending = 0;
        assert_eq!(state_key(&readiness), TextKey::AddonWorkspacePending);
        assert_eq!(action_key(&readiness), TextKey::RetryPreparation);
        assert!(!counts_text(&readiness).contains(':'));

        readiness.review_state = AddonMediaListReviewState::Publishing;
        assert_eq!(state_key(&readiness), TextKey::PublishBlocked);

        readiness.review_state = AddonMediaListReviewState::Published;
        assert_eq!(state_key(&readiness), TextKey::AddonWorkspacePublished);
        assert!(!action_enabled(&readiness));
    }

    #[test]
    fn failed_preparation_is_retried_and_explained_for_the_saved_revision_only() {
        let failure = |saved_revision, reason, count| AddonMediaListPreparationFailure {
            saved_revision,
            reason,
            count,
        };
        // Earlier revision-7 failures stay history once the draft is revision 8.
        let mut readiness = AddonMediaListPublicationReadiness {
            included_count: 14,
            required_prepared_media_count: 14,
            saved_revision: 8,
            prepared_media_failures: vec![failure(
                7,
                AddonMediaListPreparationFailureReason::InputChanged,
                14,
            )],
            ..Default::default()
        };
        assert!(current_failure_reasons(&readiness).is_empty());
        assert_eq!(
            state_key(&readiness),
            TextKey::AddonWorkspacePreparationRequired
        );
        assert_eq!(action_key(&readiness), TextKey::RetryPreparation);

        readiness.required_prepared_media_failed = 14;
        readiness.prepared_media_failures.extend([
            failure(
                8,
                AddonMediaListPreparationFailureReason::InvalidRequest,
                12,
            ),
            failure(8, AddonMediaListPreparationFailureReason::StorageFull, 2),
        ]);
        assert_eq!(
            current_failure_reasons(&readiness),
            vec![
                AddonMediaListPreparationFailureReason::InvalidRequest,
                AddonMediaListPreparationFailureReason::StorageFull,
            ]
        );
        assert_eq!(
            state_key(&readiness),
            TextKey::AddonWorkspacePreparationFailed
        );
        assert_eq!(action_key(&readiness), TextKey::RetryPreparation);
        assert!(action_enabled(&readiness));

        // A retry in flight is preparation, not failure.
        readiness.required_prepared_media_pending = 14;
        assert_eq!(state_key(&readiness), TextKey::AddonWorkspacePreparing);
        assert!(!action_enabled(&readiness));
    }

    #[test]
    fn every_preparation_failure_has_distinct_guidance() {
        use AddonMediaListPreparationFailureReason as Reason;
        let keys = [
            Reason::Failed,
            Reason::InputChanged,
            Reason::InvalidRequest,
            Reason::Cancelled,
            Reason::AttemptsExhausted,
            Reason::StorageFull,
            Reason::ProcessRecoveryRequired,
            Reason::Unsupported,
        ]
        .map(preparation_failure_text_key);
        for (index, key) in keys.iter().enumerate() {
            assert!(!keys[..index].contains(key), "{key:?} is reused");
        }
    }
}
