//! One bounded durable guide refresh worker; manual authoring authority is immutable.
use super::{
    teletext_authoring::{
        observe_guide_refresh_seed_configuration_with_charge, GuideRefreshPreparationContext,
    },
    teletext_guide_generations::GuideGenerationContext,
    teletext_guide_refresh_native::{
        observe_guide_refresh_snapshot_configuration, prepare_current_guide_refresh,
        GuideRefreshNativeError,
    },
    teletext_runtime_policy::{
        parse_native_teletext_authoring_policy, parse_native_teletext_runtime_policy,
        NATIVE_TELETEXT_AUTHORING_POLICY_SETTING, NATIVE_TELETEXT_RUNTIME_POLICY_SETTING,
    },
    teletext_work,
};
use chrono::{DateTime, Utc};
use ersatzrs_app::ApplicationServices;
use ersatzrs_core::{
    locking::TeletextWorkGuard,
    repositories::{
        TeletextGenerationMutation, TeletextGuideRefreshClaim, TeletextGuideRefreshCurrentMetadata,
        TeletextGuideRefreshFailure, TeletextGuideRefreshSeed, TeletextGuideRefreshSelection,
        TeletextProfileMutation, MAX_TELETEXT_GUIDE_REFRESH_BATCH,
    },
};
use ersatzrs_domain::{
    EntityId, TeletextChannelSelectionSnapshot, TeletextEffectiveBindingSnapshot,
};
use ersatzrs_infra::db::DbPool;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

// Existing accepted coalescing interval, also used while opt-in is absent.
const COALESCING: Duration = Duration::from_mins(1);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Configuration {
    Current,
    Changed,
    Disabled,
    Deferred,
    NeedsReview,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Observation {
    Current,
    Changed,
    Disabled,
    Deferred,
    NeedsReview,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum JobStop {
    Complete,
    Failed(TeletextGuideRefreshFailure),
    Cancelled,
    LeaseLost,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ConfigurationIdentity {
    seed_id: EntityId,
    runtime_id: Option<EntityId>,
    digest: String,
}
impl ConfigurationIdentity {
    fn from_metadata(metadata: &TeletextGuideRefreshCurrentMetadata) -> Self {
        Self {
            seed_id: metadata.seed.id,
            runtime_id: metadata.runtime.as_ref().map(|snapshot| snapshot.id),
            digest: metadata.runtime.as_ref().map_or_else(
                || metadata.seed.generation.request.digest.clone(),
                |snapshot| snapshot.document_digest.clone(),
            ),
        }
    }
}
struct ConfigurationCache {
    values: BTreeMap<ConfigurationIdentity, (TeletextGuideRefreshSelection, usize)>,
    maximum: usize,
    charged: usize,
}
impl ConfigurationCache {
    fn new() -> Self {
        Self {
            values: BTreeMap::new(),
            maximum: 0,
            charged: 0,
        }
    }
    fn set_maximum(&mut self, maximum: usize) {
        if maximum < self.maximum {
            self.values.clear();
            self.charged = 0;
        }
        self.maximum = maximum;
    }
    fn get(&self, key: &ConfigurationIdentity) -> Option<TeletextGuideRefreshSelection> {
        self.values.get(key).map(|(selection, _)| selection.clone())
    }
    fn insert(
        &mut self,
        key: ConfigurationIdentity,
        selection: TeletextGuideRefreshSelection,
        charge: usize,
    ) {
        if charge > self.maximum || charge == 0 {
            return;
        }
        let previous: Vec<_> = self
            .values
            .keys()
            .filter(|old| old.seed_id == key.seed_id)
            .cloned()
            .collect();
        for old in previous {
            if let Some((_, bytes)) = self.values.remove(&old) {
                self.charged -= bytes;
            }
        }
        while self.values.len() >= MAX_TELETEXT_GUIDE_REFRESH_BATCH
            || self
                .charged
                .checked_add(charge)
                .is_none_or(|sum| sum > self.maximum)
        {
            if let Some((_, (_, bytes))) = self.values.pop_first() {
                self.charged -= bytes;
            } else {
                return;
            }
        }
        self.values.insert(key, (selection, charge));
        self.charged += charge;
    }
}

/// Existing task lifecycle owns this worker. There is no extra queue, detached
/// producer, playback reservation, package activation or manual pointer mutation.
pub(crate) async fn run_teletext_guide_refresh(
    application: Arc<ApplicationServices>,
    pool: DbPool,
    shutdown: CancellationToken,
) {
    let mut cursor = None;
    let mut cache = ConfigurationCache::new();
    loop {
        let mut cadence = COALESCING;
        let turn = async {
            let raw = tokio::select! { biased; ()=shutdown.cancelled()=>return, raw=application.get_optional_setting(NATIVE_TELETEXT_RUNTIME_POLICY_SETTING)=>raw };
            let Some(runtime_raw) = raw.ok().flatten() else {
                return;
            };
            let Ok(runtime) = parse_native_teletext_runtime_policy(&runtime_raw) else {
                return;
            };
            if !runtime.guide_refresh {
                return;
            }
            let Some(updates) = runtime.live_updates else {
                return;
            };
            cadence = updates.poll_interval();
            cache.set_maximum(runtime.design.max_serialized_bytes);
            let seeds = tokio::select! {biased; ()=shutdown.cancelled()=>return, seeds=application.scan_current_guide_refresh(cursor,1)=>seeds};
            let Ok(seeds) = seeds else {
                tracing::warn!("guide refresh discovery is unavailable");
                return;
            };
            // One discovery observation and one due native attempt per turn.
            // Keyset continuation wraps, so later seeds cannot starve at256.
            if let Some(seed) = seeds.first() {
                cursor = Some(seed.id);
                observe_seed(&application, &pool, seed, &shutdown, cadence, &mut cache).await;
            } else {
                cursor = None;
            }
            if shutdown.is_cancelled() {
                return;
            }
            let seeds = tokio::select! { biased; ()=shutdown.cancelled()=>return, seeds=application.due_guide_refresh(Utc::now(), 1)=>seeds };
            let Ok(seeds) = seeds else {
                return;
            };
            if let Some(seed) = seeds.first() {
                Box::pin(run_due(
                    &application,
                    &pool,
                    seed,
                    &runtime_raw,
                    &shutdown,
                    cadence,
                    &mut cache,
                ))
                .await;
            }
        };
        // Each owned stage handles cancellation before releasing its real holds;
        // the outer loop never abandons an in-flight owner during host shutdown.
        Box::pin(turn).await;
        if shutdown.is_cancelled() {
            break;
        }
        tokio::select! { () = shutdown.cancelled() => break, () = tokio::time::sleep(cadence) => {} }
    }
}

async fn context(
    application: &ApplicationServices,
    shutdown: &CancellationToken,
) -> Result<(TeletextWorkGuard, GuideRefreshPreparationContext, String), GuideRefreshNativeError> {
    let started = Instant::now();
    let raw = tokio::select! { biased; ()=shutdown.cancelled()=>return Err(GuideRefreshNativeError::Unavailable), raw=application.get_optional_setting(NATIVE_TELETEXT_AUTHORING_POLICY_SETTING)=>raw }.map_err(|_| GuideRefreshNativeError::Unavailable)?.ok_or(GuideRefreshNativeError::InvalidPolicy)?;
    let policy = parse_native_teletext_authoring_policy(&raw)
        .map_err(|_| GuideRefreshNativeError::InvalidPolicy)?;
    let deadline = started
        .checked_add(policy.operation_timeout)
        .ok_or(GuideRefreshNativeError::InvalidPolicy)?;
    let guard = tokio::select! { biased; ()=shutdown.cancelled()=>return Err(GuideRefreshNativeError::Unavailable), guard=teletext_work::begin(application, Some(deadline))=>guard }.map_err(|_| GuideRefreshNativeError::Busy)?;
    let context = GuideRefreshPreparationContext::from_policy(policy, started, guard.hold_work())?;
    Ok((guard, context, raw))
}

async fn current_selection(
    application: &ApplicationServices,
    metadata: &TeletextGuideRefreshCurrentMetadata,
    context: &GuideRefreshPreparationContext,
    cache: &mut ConfigurationCache,
) -> Result<TeletextGuideRefreshSelection, GuideRefreshNativeError> {
    let key = ConfigurationIdentity::from_metadata(metadata);
    if let Some(selection) = cache.get(&key) {
        return Ok(selection);
    }
    let (selection, charged) = if let Some(runtime) = metadata.runtime.as_ref() {
        let mut budget = context.design_budget();
        budget.max_serialized_bytes = budget.max_serialized_bytes.min(cache.maximum);
        let current = application
            .guide_refresh_current(
                metadata.seed.generation.request.channel_id,
                metadata.seed.role,
                budget.max_serialized_bytes,
            )
            .await
            .map_err(|_| GuideRefreshNativeError::Unavailable)?
            .ok_or(GuideRefreshNativeError::Unavailable)?;
        if current.seed != metadata.seed {
            return Err(GuideRefreshNativeError::Unavailable);
        }
        let snapshot = current
            .runtime
            .ok_or(GuideRefreshNativeError::Unavailable)?;
        if snapshot.id != runtime.id
            || snapshot.saved.document_digest != runtime.document_digest
            || snapshot.saved.version != runtime.version
            || snapshot.saved.document_version != runtime.document_version
        {
            return Err(GuideRefreshNativeError::Unavailable);
        }
        observe_guide_refresh_snapshot_configuration(
            &metadata.seed,
            &snapshot,
            budget,
            context.deadline(),
            context.hold(),
        )
        .await?
    } else {
        let mut budget = context.design_budget();
        budget.max_serialized_bytes = budget.max_serialized_bytes.min(cache.maximum);
        observe_guide_refresh_seed_configuration_with_charge(
            application,
            &metadata.seed,
            budget,
            context.deadline(),
            context.hold(),
        )
        .await?
    };
    let channel = match &selection {
        TeletextGuideRefreshSelection::Base(selection) => selection.channel_id,
        TeletextGuideRefreshSelection::Effective(selection) => selection.channel_id,
    };
    if channel != metadata.seed.generation.request.channel_id {
        return Err(GuideRefreshNativeError::NeedsReview);
    }
    cache.insert(key, selection.clone(), charged);
    Ok(selection)
}

fn base_configuration(
    expected: &TeletextChannelSelectionSnapshot,
    current: &TeletextChannelSelectionSnapshot,
) -> Configuration {
    let before: BTreeSet<_> = expected
        .bindings
        .iter()
        .map(|binding| binding.addon_id.as_str())
        .collect();
    let after: BTreeSet<_> = current
        .bindings
        .iter()
        .map(|binding| binding.addon_id.as_str())
        .collect();
    if after.is_empty() || !before.is_subset(&after) {
        Configuration::Disabled
    } else if before != after {
        Configuration::NeedsReview
    } else if current == expected {
        Configuration::Current
    } else {
        Configuration::Changed
    }
}

async fn configuration(
    application: &ApplicationServices,
    expected: &TeletextGuideRefreshSelection,
    now: DateTime<Utc>,
) -> Result<Configuration, GuideRefreshNativeError> {
    match expected {
        TeletextGuideRefreshSelection::Base(expected) => {
            let current = match application.observe_channel_teletext_document_selection(expected.channel_id).await {
                Ok(current) => current,
                Err(ersatzrs_app::teletext_channel_documents::ChannelTeletextDocumentError::SelectionRejected(_)) => return Ok(Configuration::Disabled),
                Err(_) => return Err(GuideRefreshNativeError::Unavailable),
            };
            let result = base_configuration(expected, &current);
            if !matches!(result, Configuration::Current | Configuration::Changed) {
                return Ok(result);
            }
            let effective = application
                .observe_effective_channel_teletext_selection(expected.channel_id, now)
                .await
                .map_err(|_| GuideRefreshNativeError::Unavailable)?;
            if matches!(effective, TeletextProfileMutation::Saved(ref scope) if scope.bindings.iter().any(|binding| matches!(binding,TeletextEffectiveBindingSnapshot::Scripted(_))))
            {
                return Ok(Configuration::Deferred);
            }
            Ok(result)
        }
        TeletextGuideRefreshSelection::Effective(expected) => {
            let starts = expected
                .bindings
                .iter()
                .filter_map(|binding| match binding {
                    TeletextEffectiveBindingSnapshot::Scripted(scope) => Some(scope.interval.start),
                    TeletextEffectiveBindingSnapshot::Base(_) => None,
                });
            if starts.max().is_some_and(|start| start > now) {
                return Ok(Configuration::Deferred);
            }
            if expected.valid_until.is_some_and(|until| now >= until) {
                return Ok(Configuration::Disabled);
            }
            let TeletextProfileMutation::Saved(current) = application
                .observe_effective_channel_teletext_selection(expected.channel_id, now)
                .await
                .map_err(|_| GuideRefreshNativeError::Unavailable)?
            else {
                return Ok(Configuration::Disabled);
            };
            if current.bindings.iter().all(|binding| !binding.enabled()) {
                return Ok(Configuration::Disabled);
            }
            let before: BTreeSet<_> = expected
                .bindings
                .iter()
                .map(TeletextEffectiveBindingSnapshot::addon_id)
                .collect();
            let after: BTreeSet<_> = current
                .bindings
                .iter()
                .map(TeletextEffectiveBindingSnapshot::addon_id)
                .collect();
            if !before.is_subset(&after) {
                return Ok(Configuration::Disabled);
            }
            if before != after {
                return Ok(Configuration::NeedsReview);
            }
            let scripted = |binding: &TeletextEffectiveBindingSnapshot| match binding {
                TeletextEffectiveBindingSnapshot::Scripted(scope) => Some(scope.clone()),
                TeletextEffectiveBindingSnapshot::Base(_) => None,
            };
            if expected.valid_until != current.valid_until
                || !expected
                    .bindings
                    .iter()
                    .filter_map(scripted)
                    .eq(current.bindings.iter().filter_map(scripted))
            {
                return Ok(Configuration::Disabled);
            }
            Ok(if &current == expected {
                Configuration::Current
            } else {
                Configuration::Changed
            })
        }
    }
}

async fn inspect_seed(
    application: &ApplicationServices,
    pool: &DbPool,
    seed: &TeletextGuideRefreshSeed,
    context: &GuideRefreshPreparationContext,
    cache: &mut ConfigurationCache,
) -> Result<Observation, GuideRefreshNativeError> {
    let metadata = application
        .observe_current_guide_refresh(
            seed.generation.request.channel_id,
            seed.role,
            context.design_budget().max_serialized_bytes,
        )
        .await
        .map_err(|_| GuideRefreshNativeError::Unavailable)?
        .ok_or(GuideRefreshNativeError::Unavailable)?;
    if metadata.seed.id != seed.id || metadata.revoked {
        return Ok(Observation::Deferred);
    }
    let expected = current_selection(application, &metadata, context, cache).await?;
    match configuration(application, &expected, Utc::now()).await? {
        Configuration::Changed => return Ok(Observation::Changed),
        Configuration::Disabled => return Ok(Observation::Disabled),
        Configuration::Deferred => return Ok(Observation::Deferred),
        Configuration::NeedsReview => return Ok(Observation::NeedsReview),
        Configuration::Current => {}
    }
    let qualification = metadata
        .runtime
        .as_ref()
        .map_or(&metadata.seed.qualification, |runtime| {
            &runtime.qualification
        });
    if Utc::now() >= qualification.valid_until {
        return Ok(Observation::Changed);
    }
    let operation = GuideGenerationContext {
        application,
        pool,
        budget: context.design_budget(),
        encoding: context.encoding_policy(),
        limits: context.guide_limits()?,
        deadline: context.deadline(),
        hold: context.hold(),
    };
    match super::teletext_guide_generations::capture(&operation, &qualification.source).await {
        Ok(_) => Ok(Observation::Current),
        Err(_) => Ok(Observation::Changed),
    }
}

async fn observe_seed(
    application: &ApplicationServices,
    pool: &DbPool,
    seed: &TeletextGuideRefreshSeed,
    shutdown: &CancellationToken,
    cadence: Duration,
    cache: &mut ConfigurationCache,
) {
    let Ok((guard, context, _)) = context(application, shutdown).await else {
        return;
    };
    let deadline = context.deadline();
    let outcome = {
        let work = inspect_seed(application, pool, seed, &context, cache);
        tokio::pin!(work);
        tokio::select! { biased; () = shutdown.cancelled() => None,
        result = tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), &mut work) => Some(result.unwrap_or(Err(GuideRefreshNativeError::TimedOut))) }
    };
    drop(context);
    if !settle(&guard, deadline, cadence).await {
        tracing::warn!(
            seed_id = seed.id,
            "guide refresh source observation cleanup is unconfirmed"
        );
        return;
    }
    if shutdown.is_cancelled() {
        return;
    }
    match outcome {
        Some(Ok(Observation::Changed) | Err(_)) => {
            let _ = tokio::select! { biased; ()=shutdown.cancelled()=>None,
            result=tokio::time::timeout_at(tokio::time::Instant::from_std(deadline),application.mark_guide_refresh_changed(seed.id,Utc::now()))=>Some(result) };
        }
        Some(Ok(Observation::Disabled)) => {
            let _ = tokio::select! { biased; ()=shutdown.cancelled()=>None,
            result=tokio::time::timeout_at(tokio::time::Instant::from_std(deadline),application.revoke_guide_refresh(seed.id,Utc::now()))=>Some(result) };
        }
        Some(Ok(Observation::NeedsReview)) => {
            let _ = tokio::select! { biased; ()=shutdown.cancelled()=>None,
            result=tokio::time::timeout_at(tokio::time::Instant::from_std(deadline),application.mark_guide_refresh_changed(seed.id,Utc::now()))=>Some(result) };
        }
        _ => {}
    }
}

async fn qualify_due(
    application: &ApplicationServices,
    seed: &TeletextGuideRefreshSeed,
    context: &GuideRefreshPreparationContext,
    cache: &mut ConfigurationCache,
    shutdown: &CancellationToken,
) -> Option<Result<Configuration, GuideRefreshNativeError>> {
    let deadline = context.deadline();
    let work = async {
        let metadata = application
            .observe_current_guide_refresh(
                seed.generation.request.channel_id,
                seed.role,
                cache.maximum,
            )
            .await
            .map_err(|_| GuideRefreshNativeError::Unavailable)?
            .ok_or(GuideRefreshNativeError::Unavailable)?;
        if metadata.seed.id != seed.id || metadata.revoked {
            return Ok(Configuration::Deferred);
        }
        let selection = current_selection(application, &metadata, context, cache).await?;
        configuration(application, &selection, Utc::now()).await
    };
    tokio::pin!(work);
    tokio::select! { biased; () = shutdown.cancelled() => None,
    result = tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), &mut work) => Some(result.unwrap_or(Err(GuideRefreshNativeError::TimedOut))) }
}

async fn prepare_and_complete(
    application: &ApplicationServices,
    pool: &DbPool,
    claim: &TeletextGuideRefreshClaim,
    context: GuideRefreshPreparationContext,
    runtime_raw: &str,
    authoring_raw: &str,
    shutdown: &CancellationToken,
) -> Result<JobStop, TeletextGuideRefreshFailure> {
    let maximum = context.design_budget().max_serialized_bytes;
    let prepared = prepare_current_guide_refresh(application, pool, claim, context)
        .await
        .map_err(GuideRefreshNativeError::failure)?;
    if !same_settings(application, runtime_raw, authoring_raw).await || shutdown.is_cancelled() {
        return Err(TeletextGuideRefreshFailure::SourceUnavailable);
    }
    let result = application
        .complete_guide_refresh(
            claim,
            prepared.candidate(),
            &prepared,
            Utc::now(),
            prepared.source_check(),
            maximum,
        )
        .await;
    match result {
        Ok(TeletextGenerationMutation::Accepted(_)) => Ok(JobStop::Complete),
        // A draft/pending conflict may atomically stop promotion for
        // review without any source loss. Its repository-owned state
        // must not be overwritten by a synthetic preparation failure.
        Ok(TeletextGenerationMutation::Rejected(
            ersatzrs_core::repositories::TeletextGenerationRejection::GenerationConflict
            | ersatzrs_core::repositories::TeletextGenerationRejection::AlreadyAccepted,
        )) => Ok(JobStop::LeaseLost),
        _ => Err(TeletextGuideRefreshFailure::SourceUnavailable),
    }
}

async fn heartbeat_claim(
    application: &ApplicationServices,
    claim: &TeletextGuideRefreshClaim,
    heartbeat_check: &ersatzrs_core::repositories::TeletextGuideGenerationCheck,
    runtime_raw: &str,
    authoring_raw: &str,
    cadence: Duration,
) -> JobStop {
    let deadline = heartbeat_check.deadline;
    loop {
        tokio::time::sleep(cadence).await;
        if !same_settings(application, runtime_raw, authoring_raw).await {
            return JobStop::Failed(TeletextGuideRefreshFailure::SourceUnavailable);
        }
        let now = Utc::now();
        let Some(until) = wall_deadline(now, deadline) else {
            return JobStop::Failed(TeletextGuideRefreshFailure::PreparationFailed);
        };
        let check = ersatzrs_core::repositories::TeletextGuideGenerationCheck {
            limits: heartbeat_check.limits,
            deadline,
            hold: heartbeat_check.hold.clone(),
        };
        if !matches!(
            application
                .heartbeat_guide_refresh(claim, now, until, check)
                .await,
            Ok(TeletextGenerationMutation::Accepted(()))
        ) {
            return JobStop::LeaseLost;
        }
    }
}

struct DueOperation<'a> {
    application: &'a ApplicationServices,
    pool: &'a DbPool,
    seed: &'a TeletextGuideRefreshSeed,
    runtime_raw: &'a str,
    shutdown: &'a CancellationToken,
    cadence: Duration,
}

async fn run_due(
    application: &ApplicationServices,
    pool: &DbPool,
    seed: &TeletextGuideRefreshSeed,
    runtime_raw: &str,
    shutdown: &CancellationToken,
    cadence: Duration,
    cache: &mut ConfigurationCache,
) {
    let Ok((guard, context, authoring_raw)) = context(application, shutdown).await else {
        return;
    };
    let deadline = context.deadline();
    // Recheck the immutable-origin recipe before claiming or running any producer.
    let qualification = qualify_due(application, seed, &context, cache, shutdown).await;
    let needs_review = matches!(
        qualification,
        Some(Ok(Configuration::NeedsReview) | Err(GuideRefreshNativeError::NeedsReview))
    );
    if !needs_review
        && !matches!(
            qualification,
            Some(Ok(Configuration::Current | Configuration::Changed))
        )
    {
        let disabled = matches!(qualification, Some(Ok(Configuration::Disabled)));
        drop(context);
        let _ = settle(&guard, deadline, cadence).await;
        if disabled && !shutdown.is_cancelled() {
            let _ = tokio::time::timeout_at(
                tokio::time::Instant::from_std(deadline),
                application.revoke_guide_refresh(seed.id, Utc::now()),
            )
            .await;
        }
        return;
    }
    let operation = DueOperation {
        application,
        pool,
        seed,
        runtime_raw,
        shutdown,
        cadence,
    };
    Box::pin(claim_and_refresh(
        &operation,
        guard,
        context,
        &authoring_raw,
        needs_review,
    ))
    .await;
}

async fn claim_and_refresh(
    operation: &DueOperation<'_>,
    guard: TeletextWorkGuard,
    context: GuideRefreshPreparationContext,
    authoring_raw: &str,
    needs_review: bool,
) {
    let DueOperation {
        application,
        seed,
        shutdown,
        cadence,
        ..
    } = *operation;
    let deadline = context.deadline();
    let now = Utc::now();
    let Some(lease_until) = wall_deadline(now, deadline) else {
        return;
    };
    let Ok(check) = context.source_check() else {
        return;
    };
    let claimed = {
        let work = application.claim_guide_refresh(
            seed.id,
            now,
            lease_until,
            check,
            context.design_budget().max_serialized_bytes,
        );
        tokio::pin!(work);
        tokio::select! { biased; ()=shutdown.cancelled()=>None,
        result=tokio::time::timeout_at(tokio::time::Instant::from_std(deadline),&mut work)=>result.ok() }
    };
    let Some(Ok(TeletextGenerationMutation::Accepted(Some(claim)))) = claimed else {
        drop(context);
        let _ = settle(&guard, finalization_deadline(), cadence).await;
        return;
    };
    if needs_review {
        drop(context);
        finalize_claim(
            application,
            &claim,
            &guard,
            JobStop::Failed(TeletextGuideRefreshFailure::NeedsReview),
            cadence,
        )
        .await;
        return;
    }
    let Some(outcome) = execute_claim(operation, &claim, context, authoring_raw).await else {
        return;
    };
    finalize_claim(application, &claim, &guard, outcome, cadence).await;
}

async fn finalize_claim(
    application: &ApplicationServices,
    claim: &TeletextGuideRefreshClaim,
    guard: &TeletextWorkGuard,
    outcome: JobStop,
    cadence: Duration,
) {
    // Preparation and heartbeat futures have stopped. This bounded allowance
    // permits only actual-worker settlement and the existing same-owner failure
    // CAS; it cannot extend preparation, publication, source or lease authority.
    let deadline = finalization_deadline();
    let bookkeeping = async {
        if matches!(outcome, JobStop::Failed(_) | JobStop::Cancelled) {
            let failure = match outcome {
                JobStop::Failed(failure) => failure,
                _ => TeletextGuideRefreshFailure::PreparationFailed,
            };
            record_failure(application, claim, failure, deadline).await;
        }
    };
    // Persistence shares the drain deadline instead of waiting for cleanup to
    // exhaust it. The guard remains owned until both terminal paths complete.
    let (settled, ()) = tokio::join!(settle(guard, deadline, cadence), bookkeeping);
    if !settled {
        tracing::warn!(
            seed_id = claim.seed.id,
            "guide refresh producer cleanup is unconfirmed; capacity is quarantined"
        );
    }
}

async fn execute_claim(
    operation: &DueOperation<'_>,
    claim: &TeletextGuideRefreshClaim,
    context: GuideRefreshPreparationContext,
    authoring_raw: &str,
) -> Option<JobStop> {
    let DueOperation {
        application,
        pool,
        runtime_raw,
        shutdown,
        cadence,
        ..
    } = *operation;
    let deadline = context.deadline();
    let Ok(heartbeat_check) = context.source_check() else {
        return None;
    };
    let outcome = {
        let work = prepare_and_complete(
            application,
            pool,
            claim,
            context,
            runtime_raw,
            authoring_raw,
            shutdown,
        );
        let heartbeat = heartbeat_claim(
            application,
            claim,
            &heartbeat_check,
            runtime_raw,
            authoring_raw,
            cadence,
        );
        tokio::pin!(work);
        tokio::pin!(heartbeat);
        tokio::select! { biased;
            ()=shutdown.cancelled()=>JobStop::Cancelled,
            result=tokio::time::timeout_at(tokio::time::Instant::from_std(deadline),&mut work)=>match result {Ok(Ok(stop))=>stop,Ok(Err(failure))=>JobStop::Failed(failure),Err(_)=>JobStop::Failed(TeletextGuideRefreshFailure::PreparationFailed)},
            stop=&mut heartbeat=>stop,
        }
    };
    drop(heartbeat_check);
    Some(outcome)
}

async fn same_settings(application: &ApplicationServices, runtime: &str, authoring: &str) -> bool {
    let (current_runtime, current_authoring) = tokio::join!(
        application.get_optional_setting(NATIVE_TELETEXT_RUNTIME_POLICY_SETTING),
        application.get_optional_setting(NATIVE_TELETEXT_AUTHORING_POLICY_SETTING)
    );
    matches!((current_runtime,current_authoring),(Ok(Some(current_runtime)),Ok(Some(current_authoring))) if current_runtime==runtime&&current_authoring==authoring)
}
fn wall_deadline(now: DateTime<Utc>, deadline: Instant) -> Option<DateTime<Utc>> {
    now.checked_add_signed(
        chrono::Duration::from_std(deadline.checked_duration_since(Instant::now())?).ok()?,
    )
}
async fn record_failure(
    application: &ApplicationServices,
    claim: &TeletextGuideRefreshClaim,
    failure: TeletextGuideRefreshFailure,
    deadline: Instant,
) {
    let now = Utc::now();
    let Some(next) = now.checked_add_signed(chrono::Duration::seconds(60)) else {
        return;
    };
    // This terminal-only budget does not renew the lease. The same-owner CAS
    // may record an expired lease, but can never replace a reclaimed owner.
    let _ = tokio::time::timeout_at(
        tokio::time::Instant::from_std(deadline),
        application.fail_guide_refresh(claim, now, next, failure),
    )
    .await;
}

fn finalization_deadline() -> Instant {
    let now = Instant::now();
    now.checked_add(super::bootstrap::SHUTDOWN_DRAIN_TIMEOUT)
        .unwrap_or(now)
}

async fn settle(guard: &TeletextWorkGuard, deadline: Instant, cadence: Duration) -> bool {
    let bound = deadline.min(
        Instant::now()
            .checked_add(super::bootstrap::SHUTDOWN_DRAIN_TIMEOUT)
            .unwrap_or(deadline),
    );
    let settled = tokio::time::timeout_at(tokio::time::Instant::from_std(bound), async {
        while !guard.is_settled() {
            tokio::time::sleep(cadence).await;
        }
    })
    .await
    .is_ok()
        || guard.is_settled();
    if !settled {
        guard.hold_work().retain_until_restart();
    }
    settled
}

#[cfg(test)]
mod tests;
