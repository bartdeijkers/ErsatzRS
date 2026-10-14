//! Opaque effective observations; no settings or producer authority leave the host.

use chrono::{DateTime, Utc};
use ersatzrs_core::repositories::TeletextProfileMutation;
use ersatzrs_domain::{
    TeletextEffectiveBindingSnapshot as Binding,
    TeletextEffectiveChannelSelectionSnapshot as Selection, TeletextInstallationSnapshot,
    TeletextOverrideState, TeletextProfileSetting, MAX_TELETEXT_OVERRIDE_COMMANDS,
};
use std::fmt::Write as _;

use super::{
    before, check_deadline, failure, teletext_work, validate_teletext_profile_values,
    ApplicationServices, Digest as _, Instant, Operation, Sha256, TeletextAuthoringError,
    TeletextAuthoringStatus, TeletextProfileValue, TeletextSecretReferenceKind,
};

/// Public configuration identities and a bounded opaque whole-scope fence.
/// This observation never grants invocation, publication or sink authority.
pub struct TeletextAuthoringEffectiveObservation {
    pub selection_digest: String,
    pub producer_addon_ids: Vec<String>,
    pub valid_until: Option<DateTime<Utc>>,
    pub is_absent: bool,
}

/// Observe this channel's actual published Lua scope without producer invocation.
/// # Errors
/// Rejects missing Scripted scope, invalid configuration, budgets or expiry.
pub async fn observe_effective_teletext_authoring_selection(
    application: &ApplicationServices,
    channel_id: i64,
    deadline: Instant,
) -> Result<TeletextAuthoringEffectiveObservation, TeletextAuthoringError> {
    let operation = Operation::begin(application, deadline).await?;
    let selection = observed(application, channel_id, operation.deadline).await?;
    let maximum = operation.policy.maximum_producers();
    let budget = operation.policy.design;
    let deadline = operation.deadline;
    teletext_work::spawn_blocking(operation.guard.hold_work(), move || {
        let selection_digest = digest(&selection, maximum, budget, deadline)?;
        let producer_addon_ids = selection
            .bindings
            .iter()
            .filter(|binding| binding.enabled())
            .map(|binding| binding.addon_id().to_owned())
            .collect::<Vec<_>>();
        Ok(TeletextAuthoringEffectiveObservation {
            selection_digest,
            is_absent: producer_addon_ids.is_empty(),
            producer_addon_ids,
            valid_until: selection.valid_until,
        })
    })
    .await
    .map_err(|_| failure(TeletextAuthoringStatus::WorkerFailed))?
}

pub(super) async fn observed(
    application: &ApplicationServices,
    channel_id: i64,
    deadline: Instant,
) -> Result<Selection, TeletextAuthoringError> {
    if channel_id <= 0 {
        return Err(failure(TeletextAuthoringStatus::WrongChannel));
    }
    let selection = match before(
        deadline,
        application.observe_effective_channel_teletext_selection(channel_id, Utc::now()),
    )
    .await?
    .map_err(|_| failure(TeletextAuthoringStatus::StorageFailed))?
    {
        TeletextProfileMutation::Saved(selection) => selection,
        TeletextProfileMutation::Rejected(_) => {
            return Err(failure(TeletextAuthoringStatus::SelectionUnavailable))
        }
    };
    if selection.channel_id != channel_id
        || !selection
            .bindings
            .iter()
            .any(|binding| matches!(binding, Binding::Scripted(_)))
        || selection
            .valid_until
            .is_none_or(|boundary| boundary <= Utc::now())
    {
        return Err(failure(TeletextAuthoringStatus::SelectionChanged));
    }
    Ok(selection)
}

pub(super) async fn checked(
    selection: Selection,
    expected: String,
    operation: &Operation,
) -> Result<Selection, TeletextAuthoringError> {
    if expected.len() != 64
        || !expected
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(failure(TeletextAuthoringStatus::InvalidInput));
    }
    let deadline = operation.deadline;
    let maximum = operation.policy.maximum_producers();
    let budget = operation.policy.design;
    teletext_work::spawn_blocking(operation.guard.hold_work(), move || {
        if digest(&selection, maximum, budget, deadline)? != expected {
            return Err(failure(TeletextAuthoringStatus::SelectionChanged));
        }
        Ok(selection)
    })
    .await
    .map_err(|_| failure(TeletextAuthoringStatus::WorkerFailed))?
}

struct ScopeDigest {
    hash: Sha256,
    bytes: usize,
    maximum: usize,
    field_maximum: usize,
}
impl ScopeDigest {
    fn feed(&mut self, bytes: &[u8]) -> Result<(), TeletextAuthoringError> {
        self.bytes = self
            .bytes
            .checked_add(bytes.len())
            .ok_or_else(|| failure(TeletextAuthoringStatus::ResourceLimit))?;
        if self.bytes > self.maximum {
            return Err(failure(TeletextAuthoringStatus::ResourceLimit));
        }
        self.hash.update(bytes);
        Ok(())
    }
    fn text(&mut self, text: &str) -> Result<(), TeletextAuthoringError> {
        if text.len() > self.field_maximum {
            return Err(failure(TeletextAuthoringStatus::ResourceLimit));
        }
        self.feed(
            &u64::try_from(text.len())
                .map_err(|_| failure(TeletextAuthoringStatus::ResourceLimit))?
                .to_be_bytes(),
        )?;
        self.feed(text.as_bytes())
    }
    fn date(&mut self, date: DateTime<Utc>) -> Result<(), TeletextAuthoringError> {
        self.feed(&date.timestamp().to_be_bytes())?;
        self.feed(&date.timestamp_subsec_nanos().to_be_bytes())
    }
    fn producer(
        &mut self,
        identity: (i64, &str, i64, i64, u32),
        installation: &TeletextInstallationSnapshot,
        settings: &[TeletextProfileSetting],
    ) -> Result<(), TeletextAuthoringError> {
        let (channel, addon, profile, revision, contract) = identity;
        validate_teletext_profile_values("effective authoring observation", settings)
            .map_err(|_| failure(TeletextAuthoringStatus::SelectionUnavailable))?;
        self.feed(&contract.to_be_bytes())?;
        for id in [channel, profile, revision] {
            self.feed(&id.to_be_bytes())?;
        }
        for value in [
            addon,
            &installation.addon_id,
            &installation.state_revision,
            &installation.version,
            &installation.package_sha256,
            &installation.content_sha256,
            &installation.manifest_sha256,
        ] {
            self.text(value)?;
        }
        let mut settings = settings.iter().collect::<Vec<_>>();
        settings.sort_unstable_by(|left, right| left.key.cmp(&right.key));
        self.feed(
            &u64::try_from(settings.len())
                .map_err(|_| failure(TeletextAuthoringStatus::ResourceLimit))?
                .to_be_bytes(),
        )?;
        for setting in settings {
            self.text(&setting.key)?;
            let (kind, value) = match &setting.value {
                TeletextProfileValue::Literal(value) => (0, value),
                TeletextProfileValue::SecretReference {
                    kind: TeletextSecretReferenceKind::Environment,
                    reference,
                } => (1, reference),
                TeletextProfileValue::SecretReference {
                    kind: TeletextSecretReferenceKind::File,
                    reference,
                } => (2, reference),
            };
            self.feed(&[kind])?;
            self.text(value)?;
        }
        Ok(())
    }
}

fn digest(
    selection: &Selection,
    maximum_producers: usize,
    budget: ersatzrs_app::teletext_design_documents::TeletextDesignDocumentBudget,
    deadline: Instant,
) -> Result<String, TeletextAuthoringError> {
    check_deadline(deadline)?;
    if selection.bindings.is_empty()
        || selection.bindings.len() > 2 * MAX_TELETEXT_OVERRIDE_COMMANDS
        || selection
            .bindings
            .iter()
            .filter(|binding| binding.enabled())
            .count()
            > maximum_producers
    {
        return Err(failure(TeletextAuthoringStatus::ResourceLimit));
    }
    let mut writer = ScopeDigest {
        hash: Sha256::new(),
        bytes: 0,
        maximum: budget.max_context_bytes,
        field_maximum: budget.max_context_field_bytes,
    };
    writer.feed(b"ErsatzRS.Teletext.AuthoringEffectiveSelection.v1\0")?;
    writer.feed(&selection.channel_id.to_be_bytes())?;
    writer.date(
        selection
            .valid_until
            .ok_or_else(|| failure(TeletextAuthoringStatus::SelectionChanged))?,
    )?;
    writer.feed(
        &u64::try_from(selection.bindings.len())
            .map_err(|_| failure(TeletextAuthoringStatus::ResourceLimit))?
            .to_be_bytes(),
    )?;
    for binding in &selection.bindings {
        check_deadline(deadline)?;
        match binding {
            Binding::Base(base) => {
                writer.feed(&[0])?;
                writer.feed(&base.binding_revision.to_be_bytes())?;
                writer.producer(
                    (
                        base.channel_id,
                        &base.addon_id,
                        base.profile_id,
                        base.profile_revision,
                        base.contract.schema_version(),
                    ),
                    &base.installation,
                    &base.effective_settings,
                )?;
            }
            Binding::Scripted(published) => {
                writer.feed(&[1])?;
                writer.feed(&published.playout_id.to_be_bytes())?;
                writer.date(published.interval.start)?;
                writer.date(published.interval.finish)?;
                match &published.interval.state {
                    TeletextOverrideState::Off {
                        channel_id,
                        addon_id,
                    } => {
                        writer.feed(&[0])?;
                        writer.feed(&channel_id.to_be_bytes())?;
                        writer.text(addon_id)?;
                    }
                    TeletextOverrideState::Enabled(candidate) => {
                        writer.feed(&[1])?;
                        writer.producer(
                            (
                                candidate.channel_id,
                                &candidate.addon_id,
                                candidate.profile_id,
                                candidate.profile_revision,
                                candidate.contract.schema_version(),
                            ),
                            &candidate.installation,
                            &candidate.effective_settings,
                        )?;
                    }
                }
            }
        }
    }
    check_deadline(deadline)?;
    if selection
        .valid_until
        .is_none_or(|boundary| boundary <= Utc::now())
    {
        return Err(failure(TeletextAuthoringStatus::TimedOut));
    }
    let digest = writer.hash.finalize();
    let mut result = String::with_capacity(64);
    for byte in digest {
        write!(result, "{byte:02x}").map_err(|_| failure(TeletextAuthoringStatus::WorkerFailed))?;
    }
    Ok(result)
}
