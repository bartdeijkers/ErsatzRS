//! Historical author intent; structural storage grants no publication rights.
use super::{
    failure, map_only, BoundedWriter, EncodedTeletextDesignDocuments, TeletextDesignDocumentBudget,
    TeletextDesignDocumentError, TeletextDesignDocumentFailure,
};
use ersatzrs_domain::{EntityId, TeletextSecondaryDefaultAgreement};
use serde::{Deserialize, Deserializer, Serialize};
use std::io::Write as _;

/// Producer ordinal in the immutable originals, bound to its selected profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TeletextSavedSecondaryDefaultAgreement {
    producer: usize,
    profile_id: EntityId,
    profile_revision: i64,
    agreement: TeletextSecondaryDefaultAgreement,
}
impl TeletextSavedSecondaryDefaultAgreement {
    /// Descriptive intent only. Append/restore rechecks its exact selected owner.
    #[must_use]
    pub const fn new(
        producer: usize,
        profile_id: EntityId,
        profile_revision: i64,
        agreement: TeletextSecondaryDefaultAgreement,
    ) -> Self {
        Self {
            producer,
            profile_id,
            profile_revision,
            agreement,
        }
    }
    #[must_use]
    pub const fn producer(self) -> usize {
        self.producer
    }
    #[must_use]
    pub const fn agreement(self) -> TeletextSecondaryDefaultAgreement {
        self.agreement
    }
}
impl<'de> Deserialize<'de> for TeletextSavedSecondaryDefaultAgreement {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            producer: usize,
            profile_id: EntityId,
            profile_revision: i64,
            agreement: TeletextSecondaryDefaultAgreement,
        }
        let f: Fields = map_only(d)?;
        Ok(Self::new(
            f.producer,
            f.profile_id,
            f.profile_revision,
            f.agreement,
        ))
    }
}
#[derive(Default, Serialize)]
pub(super) struct OptionalAgreements(pub Option<StoredAgreements>);
impl OptionalAgreements {
    pub(super) fn is_absent(&self) -> bool {
        self.0.is_none()
    }
}

impl<'de> Deserialize<'de> for OptionalAgreements {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        StoredAgreements::deserialize(d).map(|value| Self(Some(value)))
    }
}
impl OptionalAgreements {
    pub(super) fn restore_base(
        self,
        selection: &ersatzrs_domain::TeletextChannelSelectionSnapshot,
    ) -> Result<Vec<TeletextSavedSecondaryDefaultAgreement>, TeletextDesignDocumentError> {
        self.0
            .map(|agreements| {
                agreements.restore(
                    selection
                        .bindings
                        .iter()
                        .map(|binding| (binding.profile_id, binding.profile_revision)),
                )
            })
            .transpose()
            .map(Option::unwrap_or_default)
    }
    pub(super) fn restore_effective(
        self,
        selection: &ersatzrs_domain::TeletextEffectiveChannelSelectionSnapshot,
        producers: &[usize],
    ) -> Result<Vec<TeletextSavedSecondaryDefaultAgreement>, TeletextDesignDocumentError> {
        self.0
            .map(|agreements| agreements.restore(effective_identities(selection, producers)?))
            .transpose()
            .map(Option::unwrap_or_default)
    }
}
#[derive(Serialize)]
pub(super) struct StoredAgreements {
    schema_version: u16,
    bindings: Vec<TeletextSavedSecondaryDefaultAgreement>,
}
impl<'de> Deserialize<'de> for StoredAgreements {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            schema_version: u16,
            bindings: Vec<TeletextSavedSecondaryDefaultAgreement>,
        }
        // Parent streaming preflight bounds this sequence before allocation.
        let f: Fields = map_only(d)?;
        Ok(Self {
            schema_version: f.schema_version,
            bindings: f.bindings,
        })
    }
}
impl StoredAgreements {
    pub(super) fn restore(
        self,
        identities: impl IntoIterator<Item = (EntityId, i64)>,
    ) -> Result<Vec<TeletextSavedSecondaryDefaultAgreement>, TeletextDesignDocumentError> {
        if self.schema_version != 1 || self.bindings.is_empty() {
            return Err(invalid());
        }
        validate(&self.bindings, identities)?;
        Ok(self.bindings)
    }
}

pub(super) fn replacement(
    agreements: &[TeletextSavedSecondaryDefaultAgreement],
) -> OptionalAgreements {
    OptionalAgreements((!agreements.is_empty()).then(|| StoredAgreements {
        schema_version: 1,
        bindings: agreements.to_vec(),
    }))
}

/// Replace or withdraw context intent in a private concept. Original producer
/// response/request strings remain exact; outer storage spelling may normalize.
/// Callers retain the immutable root bytes for exact undo and receipt checks.
/// This function grants no author, current-head or publication authority.
/// # Errors
/// Rejects malformed originals, wrong profile associations and trusted limits.
pub fn replace_teletext_design_secondary_default_agreements(
    original_bytes: &[u8],
    agreements: &[TeletextSavedSecondaryDefaultAgreement],
    budget: TeletextDesignDocumentBudget,
) -> Result<EncodedTeletextDesignDocuments, TeletextDesignDocumentError> {
    let original = super::decode_teletext_design_documents(original_bytes, budget)?;
    validate(
        agreements,
        original
            .selection()
            .bindings
            .iter()
            .map(|binding| (binding.profile_id, binding.profile_revision)),
    )?;
    if original.secondary_default_agreements() == agreements {
        return EncodedTeletextDesignDocuments::from_saved_bytes(original_bytes.to_vec(), budget);
    }
    let mut envelope: super::Envelope =
        serde_json::from_slice(original_bytes).map_err(|_| invalid())?;
    envelope.secondary_default_agreements = replacement(agreements);
    EncodedTeletextDesignDocuments::from_saved_bytes(rewrite_bytes(&envelope, budget)?, budget)
}

pub(super) fn rewrite_bytes(
    value: &impl Serialize,
    budget: TeletextDesignDocumentBudget,
) -> Result<Vec<u8>, TeletextDesignDocumentError> {
    let mut writer = BoundedWriter {
        bytes: Vec::new(),
        maximum: budget.max_serialized_bytes,
        exceeded: false,
    };
    serde_json::to_writer(&mut writer, value).map_err(|_| {
        if writer.exceeded {
            failure(TeletextDesignDocumentFailure::Budget {
                dimension: "serialized",
                count: budget.max_serialized_bytes.saturating_add(1),
                maximum: budget.max_serialized_bytes,
            })
        } else {
            invalid()
        }
    })?;
    Ok(writer.bytes)
}

pub(super) fn validate_effective(
    agreements: &[TeletextSavedSecondaryDefaultAgreement],
    selection: &ersatzrs_domain::TeletextEffectiveChannelSelectionSnapshot,
    producers: &[usize],
) -> Result<(), TeletextDesignDocumentError> {
    validate(agreements, effective_identities(selection, producers)?)
}
fn validate(
    agreements: &[TeletextSavedSecondaryDefaultAgreement],
    identities: impl IntoIterator<Item = (EntityId, i64)>,
) -> Result<(), TeletextDesignDocumentError> {
    let mut entries = agreements.iter().peekable();
    for (producer, identity) in identities.into_iter().enumerate() {
        if let Some(entry) = entries.peek() {
            if entry.producer == producer {
                if (entry.profile_id, entry.profile_revision) != identity {
                    return Err(invalid());
                }
                entries.next();
            }
        }
    }
    // A duplicate, unsorted or out-of-range ordinal necessarily remains here.
    if entries.next().is_some() {
        return Err(invalid());
    }
    Ok(())
}
fn invalid() -> TeletextDesignDocumentError {
    failure(TeletextDesignDocumentFailure::SecondaryDefaultAgreement)
}

/// Append explicit intent without rewriting original bytes. Older closed readers
/// reject the new field. This grants no sidecar permission, CAS or publication.
/// # Errors
/// Rejects existing intent, empty/foreign associations and all trusted limits.
pub fn append_teletext_design_secondary_default_agreements(
    original_bytes: &[u8],
    agreements: &[TeletextSavedSecondaryDefaultAgreement],
    budget: TeletextDesignDocumentBudget,
) -> Result<EncodedTeletextDesignDocuments, TeletextDesignDocumentError> {
    let original = super::decode_teletext_design_documents(original_bytes, budget)?;
    if !original.secondary_default_agreements().is_empty() || agreements.is_empty() {
        return Err(invalid());
    }
    validate(
        agreements,
        original
            .selection()
            .bindings
            .iter()
            .map(|binding| (binding.profile_id, binding.profile_revision)),
    )?;
    EncodedTeletextDesignDocuments::from_saved_bytes(
        append_bytes(original_bytes, agreements, budget)?,
        budget,
    )
}
pub(super) fn append_bytes(
    original: &[u8],
    agreements: &[TeletextSavedSecondaryDefaultAgreement],
    budget: TeletextDesignDocumentBudget,
) -> Result<Vec<u8>, TeletextDesignDocumentError> {
    #[derive(Serialize)]
    struct Borrowed<'a> {
        schema_version: u16,
        bindings: &'a [TeletextSavedSecondaryDefaultAgreement],
    }
    let end = original
        .iter()
        .rposition(|b| !b.is_ascii_whitespace())
        .filter(|index| original[*index] == b'}')
        .ok_or_else(invalid)?;
    let mut writer = BoundedWriter {
        bytes: Vec::new(),
        maximum: budget.max_serialized_bytes,
        exceeded: false,
    };
    let write = |writer: &mut BoundedWriter| -> Result<(), serde_json::Error> {
        writer
            .write_all(&original[..end])
            .map_err(serde_json::Error::io)?;
        writer
            .write_all(b",\"secondary_default_agreements\":")
            .map_err(serde_json::Error::io)?;
        serde_json::to_writer(
            &mut *writer,
            &Borrowed {
                schema_version: 1,
                bindings: agreements,
            },
        )?;
        writer
            .write_all(&original[end..])
            .map_err(serde_json::Error::io)
    };
    write(&mut writer).map_err(|_| {
        if writer.exceeded {
            failure(TeletextDesignDocumentFailure::Budget {
                dimension: "serialized",
                count: budget.max_serialized_bytes.saturating_add(1),
                maximum: budget.max_serialized_bytes,
            })
        } else {
            invalid()
        }
    })?;
    Ok(writer.bytes)
}

pub(super) fn effective_identities(
    selection: &ersatzrs_domain::TeletextEffectiveChannelSelectionSnapshot,
    producers: &[usize],
) -> Result<Vec<(EntityId, i64)>, TeletextDesignDocumentError> {
    use ersatzrs_domain::{TeletextEffectiveBindingSnapshot as Binding, TeletextOverrideState};
    producers
        .iter()
        .map(|index| match selection.bindings.get(*index) {
            Some(Binding::Base(base)) => Ok((base.profile_id, base.profile_revision)),
            Some(Binding::Scripted(published)) => match &published.interval.state {
                TeletextOverrideState::Enabled(profile) => {
                    Ok((profile.profile_id, profile.profile_revision))
                }
                TeletextOverrideState::Off { .. } => Err(invalid()),
            },
            None => Err(invalid()),
        })
        .collect()
}

/// Effective/Scripted equivalent; Off members never acquire a policy.
/// # Errors
/// Rejects existing intent, foreign producer/profile mappings and trusted limits.
pub fn append_teletext_effective_design_secondary_default_agreements(
    original_bytes: &[u8],
    agreements: &[TeletextSavedSecondaryDefaultAgreement],
    budget: TeletextDesignDocumentBudget,
) -> Result<super::EncodedEffectiveTeletextDesignDocuments, TeletextDesignDocumentError> {
    let original = super::decode_teletext_effective_design_documents(original_bytes, budget)?;
    if !original.secondary_default_agreements().is_empty() || agreements.is_empty() {
        return Err(invalid());
    }
    validate(
        agreements,
        effective_identities(original.selection(), original.producer_bindings())?,
    )?;
    super::EncodedEffectiveTeletextDesignDocuments::from_saved_bytes(
        append_bytes(original_bytes, agreements, budget)?,
        budget,
    )
}
