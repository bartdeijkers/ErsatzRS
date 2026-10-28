//! Version 2 historical originals, with distinct effective origins and no live authority.

use chrono::{DateTime, Utc};
use ersatzrs_domain::{
    EntityId, TeletextEffectiveBindingSnapshot as Binding,
    TeletextEffectiveChannelSelectionSnapshot as Selection, TeletextInstallationSnapshot,
    TeletextOverrideInterval, TeletextOverrideState, TeletextPageAddress, TeletextPageContract,
    TeletextProfileCandidateSnapshot, TeletextProfileSetting, TeletextPublishedOverride,
    MAX_TELETEXT_PROFILE_SETTINGS, TELETEXT_EFFECTIVE_DESIGN_DOCUMENT_VERSION,
};

use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;

use super::{
    borrow_source_documents, contract, editorial_overlay, failure, guide_inputs, limit, map_only,
    parse_source_documents, preflight_json_for_format, profile_strings, restore_original_sources,
    source_fence, storage, validate_binding, validate_producer_entry, validate_profile_identity,
    validate_transmissions, BorrowedDocument, BoundedWriter, Document, EditPolicy, Entry,
    Installation, Member, Required, Setting, TeletextDesignDocumentBudget,
    TeletextDesignDocumentError, TeletextDesignDocumentFailure, TeletextGuideSourceFence,
    TeletextProfileDocument, TeletextSavedDocumentSource, TeletextSavedEditPolicy,
    TeletextSavedEditorialOverlay, TeletextSavedGuideInputs, TeletextSavedResourceTransmission,
    TeletextSavedV1Region, TeletextValidatedDocumentSource, Transmission, MAX_BINDINGS,
};
use crate::teletext_channel_documents::TeletextChannelRequestTemplates;

/// Borrow an entire checked effective candidate, including non-producing Off members.
/// Native confirmation remains separate from structural capture.
#[derive(Clone, Copy)]
pub struct TeletextEffectiveDesignDocumentInput<'a> {
    pub selection: &'a Selection,
    pub producer_bindings: &'a [usize],
    pub generation: &'a str,
    pub sources: &'a [TeletextValidatedDocumentSource],
    pub entry_addon_id: Option<&'a str>,
    pub entry: Option<TeletextPageAddress>,
    pub v1_region: Option<TeletextSavedV1Region>,
    pub resource_transmissions: &'a [TeletextSavedResourceTransmission],
}

impl fmt::Debug for TeletextEffectiveDesignDocumentInput<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TeletextEffectiveDesignDocumentInput")
            .field("binding_count", &self.selection.bindings.len())
            .field("document_count", &self.sources.len())
            .finish_non_exhaustive()
    }
}

/// Private versioned storage bytes, never a native or published interval token.
pub struct EncodedEffectiveTeletextDesignDocuments {
    bytes: Vec<u8>,
}
impl EncodedEffectiveTeletextDesignDocuments {
    /// Restore exact saved spelling under the existing structural decoder limits.
    /// Native confirmation, revision CAS and explicit publication remain separate.
    /// # Errors
    /// Rejects invalid effective envelopes, associations, versions and bounds.
    pub fn from_saved_bytes(
        bytes: Vec<u8>,
        budget: TeletextDesignDocumentBudget,
    ) -> Result<Self, TeletextDesignDocumentError> {
        decode_teletext_effective_design_documents(&bytes, budget)?;
        Ok(Self { bytes })
    }

    #[must_use]
    pub const fn version(&self) -> u16 {
        TELETEXT_EFFECTIVE_DESIGN_DOCUMENT_VERSION
    }
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}
impl fmt::Debug for EncodedEffectiveTeletextDesignDocuments {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EncodedEffectiveTeletextDesignDocuments")
            .field("bytes", &self.bytes.len())
            .finish_non_exhaustive()
    }
}

/// Validated historical context; expiry does not prevent historical preview.
pub struct RestoredEffectiveTeletextDesignDocuments {
    parts: TeletextRestoredEffectiveDesignDocumentParts,
}
/// Move projection for native reconstruction, not current producer authority.
pub struct TeletextRestoredEffectiveDesignDocumentParts {
    pub selection: Selection,
    pub producer_bindings: Vec<usize>,
    pub generation: String,
    pub sources: Vec<TeletextSavedDocumentSource>,
    pub documents: Vec<TeletextProfileDocument>,
    pub entry_addon_id: Option<String>,
    pub entry: Option<TeletextPageAddress>,
    pub v1_region: Option<TeletextSavedV1Region>,
    pub resource_transmissions: Vec<TeletextSavedResourceTransmission>,
    pub edit_policy: TeletextSavedEditPolicy,
    pub guide_inputs: Option<TeletextSavedGuideInputs>,
    pub guide_source: Option<TeletextGuideSourceFence>,
    pub editorial_overlay: Option<TeletextSavedEditorialOverlay>,
    pub secondary_default_agreements: Vec<super::TeletextSavedSecondaryDefaultAgreement>,
}
impl RestoredEffectiveTeletextDesignDocuments {
    /// Intent bound to the producing profile, including its Scripted origin.
    #[must_use]
    pub fn secondary_default_agreements(&self) -> &[super::TeletextSavedSecondaryDefaultAgreement] {
        &self.parts.secondary_default_agreements
    }

    #[must_use]
    pub const fn selection(&self) -> &Selection {
        &self.parts.selection
    }
    #[must_use]
    pub fn producer_bindings(&self) -> &[usize] {
        &self.parts.producer_bindings
    }
    #[must_use]
    pub fn generation(&self) -> &str {
        &self.parts.generation
    }
    #[must_use]
    pub fn sources(&self) -> &[TeletextSavedDocumentSource] {
        &self.parts.sources
    }
    #[must_use]
    pub fn documents(&self) -> &[TeletextProfileDocument] {
        &self.parts.documents
    }
    #[must_use]
    pub fn entry_addon_id(&self) -> Option<&str> {
        self.parts.entry_addon_id.as_deref()
    }
    #[must_use]
    pub const fn entry(&self) -> Option<TeletextPageAddress> {
        self.parts.entry
    }
    #[must_use]
    pub const fn v1_region(&self) -> Option<TeletextSavedV1Region> {
        self.parts.v1_region
    }
    #[must_use]
    pub fn resource_transmissions(&self) -> &[TeletextSavedResourceTransmission] {
        &self.parts.resource_transmissions
    }
    #[must_use]
    pub const fn edit_policy(&self) -> TeletextSavedEditPolicy {
        self.parts.edit_policy
    }
    #[must_use]
    pub fn guide_inputs(&self) -> Option<&TeletextSavedGuideInputs> {
        self.parts.guide_inputs.as_ref()
    }
    #[must_use]
    pub fn guide_source(&self) -> Option<&TeletextGuideSourceFence> {
        self.parts.guide_source.as_ref()
    }
    #[must_use]
    pub fn editorial_overlay(&self) -> Option<&TeletextSavedEditorialOverlay> {
        self.parts.editorial_overlay.as_ref()
    }
    #[must_use]
    pub fn into_parts(self) -> TeletextRestoredEffectiveDesignDocumentParts {
        self.parts
    }
}
impl fmt::Debug for TeletextRestoredEffectiveDesignDocumentParts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TeletextRestoredEffectiveDesignDocumentParts")
            .field("binding_count", &self.selection.bindings.len())
            .field("document_count", &self.documents.len())
            .field("resource_count", &self.resource_transmissions.len())
            .field("editorial_overlay", &self.editorial_overlay)
            .finish_non_exhaustive()
    }
}
impl fmt::Debug for RestoredEffectiveTeletextDesignDocuments {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.parts.fmt(f)
    }
}

// Explicit required nulls make every origin/state shape closed. A candidate
// carries no base binding revision; the base DTO is used only for actual bases.
stored_map!(EffectiveEnvelope { format_version: u16, effective_selection: EffectiveSelection,
    producer_bindings: Vec<usize>, generation: String, entry_addon_id: Required<String>,
    entry: Required<Entry>, v1_region: Required<TeletextSavedV1Region>, edit_policy: EditPolicy,
    resource_transmissions: Vec<Transmission>, documents: Vec<Document>,
    guide_inputs: guide_inputs::StoredGuideInputs,
    #[serde(default, skip_serializing_if = "source_fence::OptionalGuideSource::is_absent")] guide_source: source_fence::OptionalGuideSource,
    #[serde(default, skip_serializing_if = "editorial_overlay::OptionalEditorialOverlay::is_absent")] editorial_overlay: editorial_overlay::OptionalEditorialOverlay,
    #[serde(default, skip_serializing_if = "super::secondary_defaults::OptionalAgreements::is_absent")] secondary_default_agreements: super::secondary_defaults::OptionalAgreements });
stored_map!(EffectiveSelection { channel_id: EntityId, bindings: Vec<EffectiveMember>,
    valid_until: Required<DateTime<Utc>> });
stored_map!(EffectiveMember { origin: String, base: Required<Member>, scripted: Required<Published> });
stored_map!(Published { playout_id: EntityId, start: DateTime<Utc>, finish: DateTime<Utc>, state: State });
stored_map!(State { kind: String, channel_id: EntityId, addon_id: String, candidate: Required<Candidate> });
stored_map!(Candidate { contract: u32, channel_id: EntityId, addon_id: String, profile_id: EntityId,
    profile_revision: i64, installation: Installation, effective_settings: Vec<Setting> });

impl From<&TeletextProfileCandidateSnapshot> for Candidate {
    fn from(v: &TeletextProfileCandidateSnapshot) -> Self {
        Self {
            contract: v.contract.schema_version(),
            channel_id: v.channel_id,
            addon_id: v.addon_id.clone(),
            profile_id: v.profile_id,
            profile_revision: v.profile_revision,
            installation: Installation::from(&v.installation),
            effective_settings: v.effective_settings.iter().map(Setting::from).collect(),
        }
    }
}
impl Candidate {
    fn into_domain(self) -> Result<TeletextProfileCandidateSnapshot, TeletextDesignDocumentError> {
        Ok(TeletextProfileCandidateSnapshot {
            contract: contract(self.contract)?,
            channel_id: self.channel_id,
            addon_id: self.addon_id,
            profile_id: self.profile_id,
            profile_revision: self.profile_revision,
            installation: self.installation.into(),
            effective_settings: self
                .effective_settings
                .into_iter()
                .map(Setting::into_domain)
                .collect::<Result<_, _>>()?,
        })
    }
}
impl From<&Binding> for EffectiveMember {
    fn from(binding: &Binding) -> Self {
        match binding {
            Binding::Base(base) => Self {
                origin: "base".to_owned(),
                base: Required(Some(Member::from(base))),
                scripted: Required(None),
            },
            Binding::Scripted(published) => {
                let state = &published.interval.state;
                let candidate = match state {
                    TeletextOverrideState::Enabled(candidate) => Some(Candidate::from(candidate)),
                    TeletextOverrideState::Off { .. } => None,
                };
                Self {
                    origin: "scripted".to_owned(),
                    base: Required(None),
                    scripted: Required(Some(Published {
                        playout_id: published.playout_id,
                        start: published.interval.start,
                        finish: published.interval.finish,
                        state: State {
                            kind: if candidate.is_some() {
                                "enabled"
                            } else {
                                "off"
                            }
                            .to_owned(),
                            channel_id: state.channel_id(),
                            addon_id: state.addon_id().to_owned(),
                            candidate: Required(candidate),
                        },
                    })),
                }
            }
        }
    }
}
impl EffectiveMember {
    fn into_domain(self) -> Result<Binding, TeletextDesignDocumentError> {
        match (self.origin.as_str(), self.base.0, self.scripted.0) {
            ("base", Some(base), None) => Ok(Binding::Base(base.into_domain()?)),
            ("scripted", None, Some(published)) => {
                let state = match (published.state.kind.as_str(), published.state.candidate.0) {
                    ("off", None) => TeletextOverrideState::Off {
                        channel_id: published.state.channel_id,
                        addon_id: published.state.addon_id,
                    },
                    ("enabled", Some(candidate)) => {
                        let candidate = candidate.into_domain()?;
                        if candidate.channel_id != published.state.channel_id
                            || candidate.addon_id != published.state.addon_id
                        {
                            return Err(failure(TeletextDesignDocumentFailure::Association));
                        }
                        TeletextOverrideState::Enabled(candidate)
                    }
                    _ => return Err(failure(TeletextDesignDocumentFailure::Context)),
                };
                Ok(Binding::Scripted(TeletextPublishedOverride {
                    playout_id: published.playout_id,
                    interval: TeletextOverrideInterval {
                        start: published.start,
                        finish: published.finish,
                        state,
                    },
                }))
            }
            _ => Err(failure(TeletextDesignDocumentFailure::Context)),
        }
    }
}

#[derive(Serialize)]
struct BorrowedEffectiveEnvelope<'a> {
    format_version: u16,
    effective_selection: EffectiveSelection,
    producer_bindings: &'a [usize],
    generation: &'a str,
    entry_addon_id: Option<&'a str>,
    entry: Option<Entry>,
    v1_region: Option<TeletextSavedV1Region>,
    edit_policy: EditPolicy,
    resource_transmissions: &'a [TeletextSavedResourceTransmission],
    documents: Vec<BorrowedDocument<'a>>,
    guide_inputs: guide_inputs::BorrowedGuideInputs<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    guide_source: Option<&'a TeletextGuideSourceFence>,
}

/// Capture bounded effective originals and exact requests; no invocation or publication.
/// # Errors
/// Rejects invalid scope/association, content, encoding context or storage bounds.
pub fn encode_teletext_effective_design_documents_with_requests(
    input: TeletextEffectiveDesignDocumentInput<'_>,
    requests: &TeletextChannelRequestTemplates,
    budget: TeletextDesignDocumentBudget,
) -> Result<EncodedEffectiveTeletextDesignDocuments, TeletextDesignDocumentError> {
    encode_documents(input, requests, None, budget)
}

/// Preserve exact original requests and their historical guide-source association.
/// # Errors
/// Rejects invalid source data or mismatched requests, in addition to normal bounds.
pub fn encode_teletext_effective_design_documents_with_source(
    input: TeletextEffectiveDesignDocumentInput<'_>,
    requests: &TeletextChannelRequestTemplates,
    source: &TeletextGuideSourceFence,
    budget: TeletextDesignDocumentBudget,
) -> Result<EncodedEffectiveTeletextDesignDocuments, TeletextDesignDocumentError> {
    encode_documents(input, requests, Some(source), budget)
}

fn encode_documents(
    input: TeletextEffectiveDesignDocumentInput<'_>,
    requests: &TeletextChannelRequestTemplates,
    source: Option<&TeletextGuideSourceFence>,
    budget: TeletextDesignDocumentBudget,
) -> Result<EncodedEffectiveTeletextDesignDocuments, TeletextDesignDocumentError> {
    let context_bytes = validate_context(
        input.selection,
        input.producer_bindings,
        input.generation,
        budget,
    )?;
    let documents = borrow_source_documents(input.sources, budget)?;
    let contracts = producer_contracts(input.selection, input.producer_bindings)?;
    let parsed = parse_source_documents(
        contracts.iter().copied(),
        input.generation,
        &documents,
        budget.validation,
    )?;
    validate_entry(
        input.selection,
        input.producer_bindings,
        input.entry_addon_id,
        input.entry,
        input.v1_region,
        &parsed.documents,
    )?;
    validate_transmissions(input.resource_transmissions, budget)?;
    let context_bytes = source.map_or(Ok(context_bytes), |source| {
        source_fence::admit(source, context_bytes, budget)
    })?;
    let guide_inputs = guide_inputs::borrow_for_contracts(
        requests,
        guide_inputs::contract_presence(contracts),
        input.generation,
        context_bytes,
        budget,
        source,
    )?;
    let envelope = BorrowedEffectiveEnvelope {
        format_version: TELETEXT_EFFECTIVE_DESIGN_DOCUMENT_VERSION,
        effective_selection: EffectiveSelection {
            channel_id: input.selection.channel_id,
            bindings: input
                .selection
                .bindings
                .iter()
                .map(EffectiveMember::from)
                .collect(),
            valid_until: Required(input.selection.valid_until),
        },
        producer_bindings: input.producer_bindings,
        generation: input.generation,
        entry_addon_id: input.entry_addon_id,
        entry: input.entry.map(|v| Entry {
            page: v.page,
            subcode: v.subcode,
        }),
        v1_region: input.v1_region,
        edit_policy: EditPolicy {
            policy_version: 1,
            kind: "original_only".to_owned(),
        },
        resource_transmissions: input.resource_transmissions,
        documents,
        guide_inputs,
        guide_source: source,
    };
    let mut writer = BoundedWriter {
        bytes: Vec::new(),
        maximum: budget.max_serialized_bytes,
        exceeded: false,
    };
    serde_json::to_writer(&mut writer, &envelope).map_err(|_| {
        if writer.exceeded {
            failure(TeletextDesignDocumentFailure::Budget {
                dimension: "serialized",
                count: budget.max_serialized_bytes.saturating_add(1),
                maximum: budget.max_serialized_bytes,
            })
        } else {
            failure(TeletextDesignDocumentFailure::Json)
        }
    })?;
    preflight_json_for_format(&writer.bytes, budget, true)?;
    Ok(EncodedEffectiveTeletextDesignDocuments {
        bytes: writer.bytes,
    })
}

/// Decode historical originals without profile/package reads or producer replay.
/// Expired scope is preserved for preview; fresh live admission is separate.
/// # Errors
/// Rejects unknown/closed formats, invalid origins/content/associations or bounds.
pub fn decode_teletext_effective_design_documents(
    bytes: &[u8],
    budget: TeletextDesignDocumentBudget,
) -> Result<RestoredEffectiveTeletextDesignDocuments, TeletextDesignDocumentError> {
    preflight_json_for_format(bytes, budget, true)?;
    storage::<EffectiveMember>(2 * MAX_BINDINGS)?;
    let envelope: EffectiveEnvelope =
        serde_json::from_slice(bytes).map_err(|_| failure(TeletextDesignDocumentFailure::Json))?;
    if envelope.format_version != TELETEXT_EFFECTIVE_DESIGN_DOCUMENT_VERSION {
        return Err(failure(TeletextDesignDocumentFailure::Version));
    }
    if envelope.edit_policy.policy_version != 1 || envelope.edit_policy.kind != "original_only" {
        return Err(failure(TeletextDesignDocumentFailure::EditPolicy));
    }
    let selection = Selection {
        channel_id: envelope.effective_selection.channel_id,
        bindings: envelope
            .effective_selection
            .bindings
            .into_iter()
            .map(EffectiveMember::into_domain)
            .collect::<Result<_, _>>()?,
        valid_until: envelope.effective_selection.valid_until.0,
    };
    validate_context(
        &selection,
        &envelope.producer_bindings,
        &envelope.generation,
        budget,
    )?;
    let contracts = producer_contracts(&selection, &envelope.producer_bindings)?;
    let guide_source = envelope.guide_source.0;
    let guide_inputs = envelope.guide_inputs.restore_for_contracts(
        guide_inputs::contract_presence(contracts.iter().copied()),
        &envelope.generation,
        budget,
        guide_source.as_ref(),
    )?;
    let borrowed = envelope
        .documents
        .iter()
        .map(|v| BorrowedDocument {
            contract: v.contract,
            generation: &v.generation,
            response: &v.response,
        })
        .collect::<Vec<_>>();
    let parsed = parse_source_documents(
        contracts.into_iter(),
        &envelope.generation,
        &borrowed,
        budget.validation,
    )?;
    let entry = envelope.entry.0.map(|v| TeletextPageAddress {
        page: v.page,
        subcode: v.subcode,
    });
    validate_entry(
        &selection,
        &envelope.producer_bindings,
        envelope.entry_addon_id.0.as_deref(),
        entry,
        envelope.v1_region.0,
        &parsed.documents,
    )?;
    let transmissions = envelope
        .resource_transmissions
        .into_iter()
        .map(|v| TeletextSavedResourceTransmission {
            resource: v.resource,
            counter: v.counter,
            update: v.update,
            last_subpage: v.last_subpage,
        })
        .collect::<Vec<_>>();
    validate_transmissions(&transmissions, budget)?;
    let secondary_default_agreements = envelope
        .secondary_default_agreements
        .restore_effective(&selection, &envelope.producer_bindings)?;
    let editorial_overlay = envelope
        .editorial_overlay
        .0
        .map(|overlay| overlay.restore(&parsed.documents, budget))
        .transpose()?;
    let sources = restore_original_sources(envelope.documents, parsed.authoring)?;
    Ok(RestoredEffectiveTeletextDesignDocuments {
        parts: TeletextRestoredEffectiveDesignDocumentParts {
            selection,
            producer_bindings: envelope.producer_bindings,
            generation: envelope.generation,
            sources,
            documents: parsed.documents,
            entry_addon_id: envelope.entry_addon_id.0,
            entry,
            v1_region: envelope.v1_region.0,
            resource_transmissions: transmissions,
            edit_policy: TeletextSavedEditPolicy::OriginalOnly,
            guide_inputs: Some(guide_inputs),
            guide_source,
            editorial_overlay,
            secondary_default_agreements,
        },
    })
}

/// Append bounded concept rows to exact role-2 originals, preserving every
/// original byte, origin, Off member, producer mapping and transition boundary.
/// Provenance is structural only: the native host must authenticate receipts,
/// enforce editable regions and validate output before saving or publication.
/// # Errors
/// Rejects cross-role/invalid originals, existing overlays, invalid owners and
/// budgets without modifying stored originals or granting edit authority.
pub fn append_teletext_effective_design_editorial_overlay(
    original_bytes: &[u8],
    overlay: &TeletextSavedEditorialOverlay,
    budget: TeletextDesignDocumentBudget,
) -> Result<EncodedEffectiveTeletextDesignDocuments, TeletextDesignDocumentError> {
    let original = decode_teletext_effective_design_documents(original_bytes, budget)?;
    if original.editorial_overlay().is_some() {
        return Err(failure(TeletextDesignDocumentFailure::EditorialOverlay));
    }
    editorial_overlay::validate_association(overlay, original.documents(), budget)?;
    drop(original);
    let bytes = editorial_overlay::append_overlay_bytes(original_bytes, overlay, budget)?;
    decode_teletext_effective_design_documents(&bytes, budget)?;
    Ok(EncodedEffectiveTeletextDesignDocuments { bytes })
}

fn binding_contract(binding: &Binding) -> Option<TeletextPageContract> {
    match binding {
        Binding::Base(base) => Some(base.contract),
        Binding::Scripted(published) => match &published.interval.state {
            TeletextOverrideState::Enabled(candidate) => Some(candidate.contract),
            TeletextOverrideState::Off { .. } => None,
        },
    }
}

fn producer_contracts(
    selection: &Selection,
    producers: &[usize],
) -> Result<Vec<TeletextPageContract>, TeletextDesignDocumentError> {
    producers
        .iter()
        .map(|index| {
            selection
                .bindings
                .get(*index)
                .and_then(binding_contract)
                .ok_or_else(|| failure(TeletextDesignDocumentFailure::Association))
        })
        .collect()
}

fn validate_context(
    selection: &Selection,
    producers: &[usize],
    generation: &str,
    budget: TeletextDesignDocumentBudget,
) -> Result<usize, TeletextDesignDocumentError> {
    limit(
        selection.bindings.len(),
        2 * MAX_BINDINGS,
        "effective bindings",
    )?;
    storage::<EffectiveMember>(selection.bindings.len())?;
    limit(
        producers.len(),
        budget.max_documents.min(MAX_BINDINGS),
        "producer bindings",
    )?;
    storage::<usize>(producers.len())?;
    if selection.channel_id <= 0
        || generation.is_empty()
        || generation.len() > ersatzrs_addon_contract::MAX_TELETEXT_PAGES_GENERATION_BYTES
    {
        return Err(failure(TeletextDesignDocumentFailure::Context));
    }
    if producers.iter().copied().ne(selection
        .bindings
        .iter()
        .enumerate()
        .filter(|(_, binding)| binding.enabled())
        .map(|(index, _)| index))
    {
        return Err(failure(TeletextDesignDocumentFailure::Association));
    }
    let mut bytes = generation.len();
    let mut latest_start = None;
    let mut earliest_finish = None;
    for (index, binding) in selection.bindings.iter().enumerate() {
        if binding.channel_id() != selection.channel_id
            || index > 0 && selection.bindings[index - 1].addon_id() >= binding.addon_id()
        {
            return Err(failure(TeletextDesignDocumentFailure::Association));
        }
        match binding {
            Binding::Base(base) => {
                validate_binding(base, selection.channel_id)?;
                admit_profile(
                    &base.addon_id,
                    &base.installation,
                    &base.effective_settings,
                    &mut bytes,
                    budget,
                )?;
            }
            Binding::Scripted(published) => {
                validate_published(published, selection.channel_id)?;
                latest_start = Some(
                    latest_start.map_or(published.interval.start, |start: DateTime<Utc>| {
                        start.max(published.interval.start)
                    }),
                );
                earliest_finish = Some(
                    earliest_finish.map_or(published.interval.finish, |finish: DateTime<Utc>| {
                        finish.min(published.interval.finish)
                    }),
                );
                match &published.interval.state {
                    TeletextOverrideState::Enabled(candidate) => admit_profile(
                        &candidate.addon_id,
                        &candidate.installation,
                        &candidate.effective_settings,
                        &mut bytes,
                        budget,
                    )?,
                    TeletextOverrideState::Off { addon_id, .. } => {
                        limit(
                            addon_id.len(),
                            budget.max_context_field_bytes,
                            "context field",
                        )?;
                        bytes = bytes
                            .checked_add(addon_id.len())
                            .ok_or_else(|| failure(TeletextDesignDocumentFailure::Storage))?;
                    }
                }
            }
        }
        limit(bytes, budget.max_context_bytes, "context")?;
    }
    if let (Some(start), Some(finish)) = (latest_start, earliest_finish) {
        if selection
            .valid_until
            .is_none_or(|boundary| boundary <= start || boundary > finish)
        {
            return Err(failure(TeletextDesignDocumentFailure::Context));
        }
    }
    limit(bytes, budget.max_context_bytes, "context")?;
    Ok(bytes)
}

fn admit_profile(
    addon: &str,
    installation: &TeletextInstallationSnapshot,
    settings: &[TeletextProfileSetting],
    bytes: &mut usize,
    budget: TeletextDesignDocumentBudget,
) -> Result<(), TeletextDesignDocumentError> {
    limit(settings.len(), MAX_TELETEXT_PROFILE_SETTINGS, "settings")?;
    storage::<Setting>(settings.len())?;
    for value in profile_strings(addon, installation, settings) {
        limit(value.len(), budget.max_context_field_bytes, "context field")?;
        *bytes = bytes
            .checked_add(value.len())
            .ok_or_else(|| failure(TeletextDesignDocumentFailure::Storage))?;
        limit(*bytes, budget.max_context_bytes, "context")?;
    }
    Ok(())
}

fn validate_published(
    published: &TeletextPublishedOverride,
    channel: EntityId,
) -> Result<(), TeletextDesignDocumentError> {
    let interval = &published.interval;
    if published.playout_id <= 0
        || interval.start >= interval.finish
        || interval.state.channel_id() != channel
        || interval.state.addon_id().len() > 255
        || ersatzrs_addon_contract::validate_addon_id(interval.state.addon_id()).is_err()
    {
        return Err(failure(TeletextDesignDocumentFailure::Context));
    }
    if let TeletextOverrideState::Enabled(candidate) = &interval.state {
        validate_profile_identity(
            candidate.channel_id,
            channel,
            &candidate.addon_id,
            candidate.profile_id,
            candidate.profile_revision,
            &candidate.installation,
            &candidate.effective_settings,
        )?;
    }
    Ok(())
}

fn validate_entry(
    selection: &Selection,
    producers: &[usize],
    addon: Option<&str>,
    entry: Option<TeletextPageAddress>,
    region: Option<TeletextSavedV1Region>,
    documents: &[TeletextProfileDocument],
) -> Result<(), TeletextDesignDocumentError> {
    let index = addon.and_then(|addon| {
        producers
            .iter()
            .position(|index| selection.bindings[*index].addon_id() == addon)
    });
    let has_v1 = producers.iter().any(|index| {
        binding_contract(&selection.bindings[*index]) == Some(TeletextPageContract::V1)
    });
    validate_producer_entry(index, addon.is_some(), entry, region, documents, has_v1)
}

#[cfg(test)]
#[path = "effective_tests.rs"]
mod tests;

/// Replace or withdraw the producer/profile-bound context of an effective
/// concept, retaining Scripted origins, Off members and original source bytes.
/// Immutable root receipts and native authorization remain caller duties.
/// # Errors
/// Rejects foreign associations, malformed originals and existing storage caps.
pub fn replace_teletext_effective_design_secondary_default_agreements(
    original_bytes: &[u8],
    agreements: &[super::TeletextSavedSecondaryDefaultAgreement],
    budget: TeletextDesignDocumentBudget,
) -> Result<EncodedEffectiveTeletextDesignDocuments, TeletextDesignDocumentError> {
    let original = decode_teletext_effective_design_documents(original_bytes, budget)?;
    super::secondary_defaults::validate_effective(
        agreements,
        original.selection(),
        original.producer_bindings(),
    )?;
    if original.secondary_default_agreements() == agreements {
        return EncodedEffectiveTeletextDesignDocuments::from_saved_bytes(
            original_bytes.to_vec(),
            budget,
        );
    }
    let mut envelope: EffectiveEnvelope = serde_json::from_slice(original_bytes)
        .map_err(|_| failure(TeletextDesignDocumentFailure::SecondaryDefaultAgreement))?;
    envelope.secondary_default_agreements = super::secondary_defaults::replacement(agreements);
    EncodedEffectiveTeletextDesignDocuments::from_saved_bytes(
        super::secondary_defaults::rewrite_bytes(&envelope, budget)?,
        budget,
    )
}
