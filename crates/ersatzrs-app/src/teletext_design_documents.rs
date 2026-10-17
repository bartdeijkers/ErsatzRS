//! Private original-document storage for ADR 0123, without invocation authority.
//!
//! Structural acceptance is not native compiler, edit-policy or publication
//! acceptance. Complete host validation before CAS/reload remains mandatory.

use crate::{
    teletext_pages::parse_page_set,
    teletext_pages_v2::parse_declared_service,
    teletext_pages_v3::parse_authored_service_with_metadata,
    teletext_profile_documents::{
        TeletextProfileDocument, TeletextProfileDocumentBudget, TeletextValidatedDocumentSource,
    },
};
use ersatzrs_core::repositories::{TeletextGuideSourceFence, MAX_TELETEXT_GUIDE_SOURCE_CHANNELS};
use ersatzrs_domain::{
    validate_teletext_profile_values, EntityId, TeletextChannelSelectionSnapshot,
    TeletextInstallationSnapshot, TeletextPageAddress, TeletextPageContract, TeletextPageNumber,
    TeletextProfileSetting, TeletextProfileValue, TeletextResourceIdentity,
    TeletextResourceUpdateCounter, TeletextSecretReferenceKind, TeletextSelectionSnapshot,
    TeletextSubcode, MAX_TELETEXT_PROFILE_SETTINGS, TELETEXT_DESIGN_DOCUMENT_VERSION,
};
use serde::{
    de::{DeserializeSeed, MapAccess, SeqAccess, Visitor},
    Deserialize, Deserializer, Serialize,
};
use std::{collections::BTreeSet, fmt, io, marker::PhantomData, mem::size_of};

#[path = "teletext_design_documents/guide_inputs.rs"]
mod guide_inputs;
pub use guide_inputs::TeletextSavedGuideInputs;
#[path = "teletext_design_documents/editorial_overlay.rs"]
mod editorial_overlay;
#[path = "teletext_design_documents/source_fence.rs"]
mod source_fence;
pub use editorial_overlay::{
    append_teletext_design_editorial_overlay, TeletextSavedEditorialOverlay,
    TeletextSavedEditorialProvenance,
};

pub(crate) const MAX_BINDINGS: usize = 1024;
const MAX_DEPTH: usize = 32;

/// Trusted storage limits, separate from producer/model and native admission.
#[derive(Debug, Clone, Copy)]
pub struct TeletextDesignDocumentBudget {
    pub max_serialized_bytes: usize,
    pub max_documents: usize,
    pub max_response_bytes: usize,
    /// Aggregate context includes exact requests when guide inputs are present.
    pub max_context_bytes: usize,
    /// Scalar context fields; request strings use their independent wire cap.
    pub max_context_field_bytes: usize,
    pub max_resource_transmissions: usize,
    pub validation: TeletextProfileDocumentBudget,
}

/// Explicit region actually used by original V1 lift, not a V2 restriction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TeletextSavedV1Region {
    WesternLatin,
}
impl<'de> Deserialize<'de> for TeletextSavedV1Region {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match String::deserialize(deserializer)?.as_str() {
            "western_latin" => Ok(Self::WesternLatin),
            _ => Err(serde::de::Error::custom("unknown saved region")),
        }
    }
}

/// Explicit host encoding input, not evidence of warm receiver history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TeletextSavedResourceUpdate {
    Changed,
    Unchanged,
}
impl<'de> Deserialize<'de> for TeletextSavedResourceUpdate {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match String::deserialize(deserializer)?.as_str() {
            "changed" => Ok(Self::Changed),
            "unchanged" => Ok(Self::Unchanged),
            _ => Err(serde::de::Error::custom("unknown saved update")),
        }
    }
}

/// Original host lifecycle record. Exact resource coverage belongs to the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TeletextSavedResourceTransmission {
    pub resource: TeletextResourceIdentity,
    pub counter: TeletextResourceUpdateCounter,
    pub update: TeletextSavedResourceUpdate,
    pub last_subpage: bool,
}

/// Original capture grants no editor/import/template/zone edit permissions.
/// Full editing policies remain a separately implemented versioned requirement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextSavedEditPolicy {
    OriginalOnly,
}

/// Borrow one complete candidate and its actual, explicit encoding inputs.
/// Structural association checks do not certify a detached candidate pairing.
#[derive(Clone, Copy)]
pub struct TeletextDesignDocumentInput<'a> {
    pub selection: &'a TeletextChannelSelectionSnapshot,
    pub generation: &'a str,
    pub sources: &'a [TeletextValidatedDocumentSource],
    pub entry_addon_id: Option<&'a str>,
    pub entry: Option<TeletextPageAddress>,
    pub v1_region: Option<TeletextSavedV1Region>,
    pub resource_transmissions: &'a [TeletextSavedResourceTransmission],
}
impl fmt::Debug for TeletextDesignDocumentInput<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TeletextDesignDocumentInput")
            .field("documents", &self.sources.len())
            .finish_non_exhaustive()
    }
}

/// Private storage bytes, never an editor sidecar or publication token.
pub struct EncodedTeletextDesignDocuments {
    bytes: Vec<u8>,
}
impl EncodedTeletextDesignDocuments {
    /// Restore bounded saved bytes without normalizing their original spelling.
    /// This structural check grants no native admission or save/publication right;
    /// the existing native validator and revision CAS remain mandatory.
    /// # Errors
    /// Rejects the same malformed associations, versions and limits as decoding.
    pub fn from_saved_bytes(
        bytes: Vec<u8>,
        budget: TeletextDesignDocumentBudget,
    ) -> Result<Self, TeletextDesignDocumentError> {
        decode_teletext_design_documents(&bytes, budget)?;
        Ok(Self { bytes })
    }

    #[must_use]
    pub const fn version(&self) -> u16 {
        TELETEXT_DESIGN_DOCUMENT_VERSION
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
impl fmt::Debug for EncodedTeletextDesignDocuments {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EncodedTeletextDesignDocuments")
            .field("bytes", &self.bytes.len())
            .finish_non_exhaustive()
    }
}

/// Historical source bytes, distinct from an authenticated invocation receipt.
pub struct TeletextSavedDocumentSource {
    contract: TeletextPageContract,
    generation: String,
    bytes: Vec<u8>,
    authoring: Option<ersatzrs_addon_contract::TeletextAuthoringMetadataWire>,
}
impl TeletextSavedDocumentSource {
    #[must_use]
    pub const fn contract(&self) -> TeletextPageContract {
        self.contract
    }
    #[must_use]
    pub fn generation(&self) -> &str {
        &self.generation
    }
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    /// Original producer metadata restored under its selected contract. The
    /// saved `OriginalOnly` policy still grants no editor/import permissions.
    #[must_use]
    pub const fn authoring(
        &self,
    ) -> Option<&ersatzrs_addon_contract::TeletextAuthoringMetadataWire> {
        self.authoring.as_ref()
    }
}
impl fmt::Debug for TeletextSavedDocumentSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TeletextSavedDocumentSource")
            .field("contract", &self.contract)
            .field("bytes", &self.bytes.len())
            .field("has_authoring", &self.authoring.is_some())
            .finish_non_exhaustive()
    }
}

/// Historical originals and context; no current selection/deadline authority.
pub struct RestoredTeletextDesignDocuments {
    parts: TeletextRestoredDesignDocumentParts,
}
/// Consuming native reconstruction inputs, still unpublished historical data.
pub struct TeletextRestoredDesignDocumentParts {
    pub selection: TeletextChannelSelectionSnapshot,
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
}
impl RestoredTeletextDesignDocuments {
    /// Structurally checked concept rows, never authenticated import/save or
    /// publication authority. Native restoration must apply or reject them.
    #[must_use]
    pub fn editorial_overlay(&self) -> Option<&TeletextSavedEditorialOverlay> {
        self.parts.editorial_overlay.as_ref()
    }

    /// Historical source metadata associated with exact stored guide inputs.
    /// This descriptive data establishes no current source or output authority.
    #[must_use]
    pub fn guide_source(&self) -> Option<&TeletextGuideSourceFence> {
        self.parts.guide_source.as_ref()
    }

    /// Exact historical requests; absence explicitly denotes legacy/static data.
    /// Presence validates data integrity, not source freshness or invocation.
    #[must_use]
    pub fn guide_inputs(&self) -> Option<&TeletextSavedGuideInputs> {
        self.parts.guide_inputs.as_ref()
    }

    #[must_use]
    pub const fn selection(&self) -> &TeletextChannelSelectionSnapshot {
        &self.parts.selection
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
    pub fn into_parts(self) -> TeletextRestoredDesignDocumentParts {
        self.parts
    }
}
impl fmt::Debug for TeletextRestoredDesignDocumentParts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TeletextRestoredDesignDocumentParts")
            .field("documents", &self.documents.len())
            .field("resources", &self.resource_transmissions.len())
            .field("guide_inputs", &self.guide_inputs)
            .field("guide_source", &self.guide_source)
            .field("editorial_overlay", &self.editorial_overlay)
            .finish_non_exhaustive()
    }
}
impl fmt::Debug for RestoredTeletextDesignDocuments {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.parts.fmt(f)
    }
}

/// Static/numeric failure, without raw JSON, keys, settings or source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("saved teletext documents at binding {binding:?}: {reason}")]
pub struct TeletextDesignDocumentError {
    pub binding: Option<usize>,
    pub reason: TeletextDesignDocumentFailure,
}
/// No raw parser messages or producer text escape this boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextDesignDocumentFailure {
    #[error("invalid editorial overlay or original-page association")]
    EditorialOverlay,
    #[error("invalid closed storage JSON")]
    Json,
    #[error("unknown saved document version")]
    Version,
    #[error("invalid original-only edit policy")]
    EditPolicy,
    #[error("invalid historical selection")]
    Context,
    #[error("original documents do not match historical selection")]
    Association,
    #[error("invalid original correlation or response")]
    Source,
    #[error("invalid historical guide inputs")]
    GuideInputs,
    #[error("invalid historical guide-source association")]
    GuideSource,
    #[error("original document failed structural validation")]
    Document,
    #[error("invalid original entry")]
    Entry,
    #[error("invalid original V1 region presence")]
    Region,
    #[error("duplicate original resource lifecycle")]
    Lifecycle,
    #[error("saved document nesting is too deep")]
    Depth,
    #[error("saved document storage arithmetic overflow")]
    Storage,
    #[error("{dimension} count {count} exceeds maximum {maximum}")]
    Budget {
        dimension: &'static str,
        count: usize,
        maximum: usize,
    },
}
fn failure(reason: TeletextDesignDocumentFailure) -> TeletextDesignDocumentError {
    TeletextDesignDocumentError {
        binding: None,
        reason,
    }
}
fn limit(
    count: usize,
    maximum: usize,
    dimension: &'static str,
) -> Result<(), TeletextDesignDocumentError> {
    if count > maximum {
        return Err(failure(TeletextDesignDocumentFailure::Budget {
            dimension,
            count,
            maximum,
        }));
    }
    Ok(())
}
fn storage<T>(count: usize) -> Result<(), TeletextDesignDocumentError> {
    let bytes = count
        .checked_mul(size_of::<T>())
        .ok_or_else(|| failure(TeletextDesignDocumentFailure::Storage))?;
    limit(bytes, isize::MAX as usize, "storage")
}

// Local map-only DTOs avoid adding Serde to invocation or checked snapshot types.
macro_rules! stored_map {
    ($name:ident { $($(#[$attribute:meta])* $field:ident: $ty:ty),* $(,)? }) => {
        #[derive(Serialize)] struct $name { $($(#[$attribute])* $field: $ty),* }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                #[derive(Deserialize)] #[serde(deny_unknown_fields)] struct Fields { $($(#[$attribute])* $field: $ty),* }
                let fields: Fields = map_only(deserializer)?;
                Ok(Self { $($field: fields.$field),* })
            }
        }
    };
}
fn map_only<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<T, D::Error> {
    struct Object<T>(PhantomData<T>);
    impl<'de, T: Deserialize<'de>> Visitor<'de> for Object<T> {
        type Value = T;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a saved object")
        }
        fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<T, A::Error> {
            T::deserialize(serde::de::value::MapAccessDeserializer::new(map))
        }
    }
    deserializer.deserialize_map(Object(PhantomData))
}
#[derive(Serialize)]
struct Required<T>(Option<T>);
impl<'de, T: Deserialize<'de>> Deserialize<'de> for Required<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Nullable<T>(PhantomData<T>);
        impl<'de, T: Deserialize<'de>> Visitor<'de> for Nullable<T> {
            type Value = Required<T>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a required null, string or object")
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(Required(None))
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                T::deserialize(serde::de::value::StrDeserializer::<E>::new(value))
                    .map(|value| Required(Some(value)))
            }
            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
                T::deserialize(serde::de::value::MapAccessDeserializer::new(map))
                    .map(|value| Required(Some(value)))
            }
        }
        // deserialize_any (not deserialize_option) makes missing fields fail.
        deserializer.deserialize_any(Nullable(PhantomData))
    }
}
stored_map!(Envelope { format_version: u16, selection: Selection, generation: String,
    entry_addon_id: Required<String>, entry: Required<Entry>, v1_region: Required<TeletextSavedV1Region>,
    edit_policy: EditPolicy, resource_transmissions: Vec<Transmission>, documents: Vec<Document>,
    #[serde(default)] guide_inputs: guide_inputs::OptionalGuideInputs,
    #[serde(default)] guide_source: source_fence::OptionalGuideSource,
    #[serde(default)] editorial_overlay: editorial_overlay::OptionalEditorialOverlay });
stored_map!(Selection { channel_id: EntityId, bindings: Vec<Member> });
stored_map!(Member { contract: u32, channel_id: EntityId, addon_id: String, binding_revision: i64,
    profile_id: EntityId, profile_revision: i64, installation: Installation, effective_settings: Vec<Setting> });
stored_map!(Installation {
    addon_id: String,
    state_revision: String,
    version: String,
    package_sha256: String,
    content_sha256: String,
    manifest_sha256: String
});
stored_map!(Setting {
    key: String,
    value: SettingValue
});
stored_map!(SettingValue {
    kind: String,
    value: String
});
stored_map!(Document {
    contract: u32,
    generation: String,
    response: String
});
stored_map!(Entry {
    page: TeletextPageNumber,
    subcode: TeletextSubcode
});
stored_map!(EditPolicy {
    policy_version: u16,
    kind: String
});
stored_map!(Transmission {
    resource: TeletextResourceIdentity,
    counter: TeletextResourceUpdateCounter,
    update: TeletextSavedResourceUpdate,
    last_subpage: bool
});

mod effective;
pub use effective::{
    append_teletext_effective_design_editorial_overlay, decode_teletext_effective_design_documents,
    encode_teletext_effective_design_documents_with_requests,
    encode_teletext_effective_design_documents_with_source,
    EncodedEffectiveTeletextDesignDocuments, RestoredEffectiveTeletextDesignDocuments,
    TeletextEffectiveDesignDocumentInput, TeletextRestoredEffectiveDesignDocumentParts,
};

impl From<&TeletextInstallationSnapshot> for Installation {
    fn from(v: &TeletextInstallationSnapshot) -> Self {
        Self {
            addon_id: v.addon_id.clone(),
            state_revision: v.state_revision.clone(),
            version: v.version.clone(),
            package_sha256: v.package_sha256.clone(),
            content_sha256: v.content_sha256.clone(),
            manifest_sha256: v.manifest_sha256.clone(),
        }
    }
}
impl From<Installation> for TeletextInstallationSnapshot {
    fn from(v: Installation) -> Self {
        Self {
            addon_id: v.addon_id,
            state_revision: v.state_revision,
            version: v.version,
            package_sha256: v.package_sha256,
            content_sha256: v.content_sha256,
            manifest_sha256: v.manifest_sha256,
        }
    }
}
impl From<&TeletextProfileSetting> for Setting {
    fn from(v: &TeletextProfileSetting) -> Self {
        let (kind, text) = match &v.value {
            TeletextProfileValue::Literal(text) => ("literal", text),
            TeletextProfileValue::SecretReference {
                kind: TeletextSecretReferenceKind::Environment,
                reference,
            } => ("environment", reference),
            TeletextProfileValue::SecretReference {
                kind: TeletextSecretReferenceKind::File,
                reference,
            } => ("file", reference),
        };
        Self {
            key: v.key.clone(),
            value: SettingValue {
                kind: kind.to_owned(),
                value: text.clone(),
            },
        }
    }
}
impl Setting {
    fn into_domain(self) -> Result<TeletextProfileSetting, TeletextDesignDocumentError> {
        let value = match self.value.kind.as_str() {
            "literal" => TeletextProfileValue::Literal(self.value.value),
            "environment" => TeletextProfileValue::SecretReference {
                kind: TeletextSecretReferenceKind::Environment,
                reference: self.value.value,
            },
            "file" => TeletextProfileValue::SecretReference {
                kind: TeletextSecretReferenceKind::File,
                reference: self.value.value,
            },
            _ => return Err(failure(TeletextDesignDocumentFailure::Context)),
        };
        Ok(TeletextProfileSetting {
            key: self.key,
            value,
        })
    }
}
fn contract(value: u32) -> Result<TeletextPageContract, TeletextDesignDocumentError> {
    match value {
        1 => Ok(TeletextPageContract::V1),
        2 => Ok(TeletextPageContract::V2),
        3 => Ok(TeletextPageContract::V3),
        _ => Err(failure(TeletextDesignDocumentFailure::Context)),
    }
}
impl From<&TeletextSelectionSnapshot> for Member {
    fn from(v: &TeletextSelectionSnapshot) -> Self {
        Self {
            contract: v.contract.schema_version(),
            channel_id: v.channel_id,
            addon_id: v.addon_id.clone(),
            binding_revision: v.binding_revision,
            profile_id: v.profile_id,
            profile_revision: v.profile_revision,
            installation: Installation::from(&v.installation),
            effective_settings: v.effective_settings.iter().map(Setting::from).collect(),
        }
    }
}
impl Member {
    fn into_domain(self) -> Result<TeletextSelectionSnapshot, TeletextDesignDocumentError> {
        Ok(TeletextSelectionSnapshot {
            contract: contract(self.contract)?,
            channel_id: self.channel_id,
            addon_id: self.addon_id,
            binding_revision: self.binding_revision,
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
#[derive(Serialize)]
struct BorrowedDocument<'a> {
    contract: u32,
    generation: &'a str,
    response: &'a str,
}
#[derive(Serialize)]
struct BorrowedEnvelope<'a> {
    format_version: u16,
    selection: Selection,
    generation: &'a str,
    entry_addon_id: Option<&'a str>,
    entry: Option<Entry>,
    v1_region: Option<TeletextSavedV1Region>,
    edit_policy: EditPolicy,
    resource_transmissions: &'a [TeletextSavedResourceTransmission],
    documents: Vec<BorrowedDocument<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    guide_inputs: Option<guide_inputs::BorrowedGuideInputs<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    guide_source: Option<&'a TeletextGuideSourceFence>,
}

/// Encode bounded structurally checked originals, without writes or packages.
/// Complete native host validation before revision CAS remains mandatory.
/// # Errors
/// Rejects strict source/context/association failures and trusted limits.
pub fn encode_teletext_design_documents(
    input: TeletextDesignDocumentInput<'_>,
    budget: TeletextDesignDocumentBudget,
) -> Result<EncodedTeletextDesignDocuments, TeletextDesignDocumentError> {
    encode_documents(input, None, None, budget)
}

/// Encode structurally checked originals with their exact preflight requests.
/// The independently versioned private subdocument preserves UTF-8 bytes and
/// selected contracts. Matching requests establish no source or invocation
/// authority. Complete native host validation before CAS remains mandatory.
/// # Errors
/// Rejects request shape, generation, selection/context differences and existing
/// source/model/storage limits, without producing a partial document.
pub fn encode_teletext_design_documents_with_requests(
    input: TeletextDesignDocumentInput<'_>,
    requests: &crate::teletext_channel_documents::TeletextChannelRequestTemplates,
    budget: TeletextDesignDocumentBudget,
) -> Result<EncodedTeletextDesignDocuments, TeletextDesignDocumentError> {
    encode_documents(input, Some(requests), None, budget)
}

/// Preserve exact guide inputs with their historical source-context association.
/// Matching fields/digest shape is data integrity, never current source authority.
/// Native validation and transactional publication fences remain separate.
/// # Errors
/// Rejects invalid fence shape, missing/mismatched requests, and existing bounds.
pub fn encode_teletext_design_documents_with_source(
    input: TeletextDesignDocumentInput<'_>,
    requests: &crate::teletext_channel_documents::TeletextChannelRequestTemplates,
    source: &TeletextGuideSourceFence,
    budget: TeletextDesignDocumentBudget,
) -> Result<EncodedTeletextDesignDocuments, TeletextDesignDocumentError> {
    encode_documents(input, Some(requests), Some(source), budget)
}

fn encode_documents(
    input: TeletextDesignDocumentInput<'_>,
    requests: Option<&crate::teletext_channel_documents::TeletextChannelRequestTemplates>,
    source: Option<&TeletextGuideSourceFence>,
    budget: TeletextDesignDocumentBudget,
) -> Result<EncodedTeletextDesignDocuments, TeletextDesignDocumentError> {
    validate_context(input.selection, input.generation, budget)?;
    let documents = borrow_sources(&input, budget)?;
    validate_transmissions(input.resource_transmissions, budget)?;
    let mut context_bytes = validate_capture_context(&input, budget)?;
    if let Some(source) = source {
        context_bytes = source_fence::admit(source, context_bytes, budget)?;
    }
    let guide_inputs = requests
        .map(|requests| {
            guide_inputs::borrow(
                requests,
                input.selection,
                input.generation,
                context_bytes,
                budget,
                source,
            )
        })
        .transpose()?;
    let parsed = parse_sources(
        input.selection,
        input.generation,
        &documents,
        budget.validation,
    )?;
    validate_entry(
        input.selection,
        input.entry_addon_id,
        input.entry,
        input.v1_region,
        &parsed.documents,
    )?;
    let envelope = BorrowedEnvelope {
        format_version: TELETEXT_DESIGN_DOCUMENT_VERSION,
        selection: Selection {
            channel_id: input.selection.channel_id,
            bindings: input.selection.bindings.iter().map(Member::from).collect(),
        },
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
    // Apply exactly the same context accounting/depth contract as the decoder.
    preflight_json(&writer.bytes, budget)?;
    Ok(EncodedTeletextDesignDocuments {
        bytes: writer.bytes,
    })
}
fn borrow_sources<'a>(
    input: &TeletextDesignDocumentInput<'a>,
    budget: TeletextDesignDocumentBudget,
) -> Result<Vec<BorrowedDocument<'a>>, TeletextDesignDocumentError> {
    limit(
        input.sources.len(),
        budget.max_documents.min(MAX_BINDINGS),
        "documents",
    )?;
    storage::<BorrowedDocument<'_>>(input.sources.len())?;
    if input.sources.len() != input.selection.bindings.len() {
        return Err(failure(TeletextDesignDocumentFailure::Association));
    }
    borrow_source_documents(input.sources, budget)
}

fn borrow_source_documents(
    sources: &[TeletextValidatedDocumentSource],
    budget: TeletextDesignDocumentBudget,
) -> Result<Vec<BorrowedDocument<'_>>, TeletextDesignDocumentError> {
    limit(
        sources.len(),
        budget.max_documents.min(MAX_BINDINGS),
        "documents",
    )?;
    storage::<BorrowedDocument<'_>>(sources.len())?;
    let mut bytes = 0_usize;
    for source in sources {
        bytes = bytes
            .checked_add(source.bytes().len())
            .ok_or_else(|| failure(TeletextDesignDocumentFailure::Storage))?;
        limit(
            source.bytes().len(),
            ersatzrs_addon_contract::MAX_TELETEXT_PAGES_OUTPUT_BYTES,
            "response",
        )?;
        limit(bytes, budget.max_response_bytes, "responses")?;
    }
    sources
        .iter()
        .map(|source| {
            Ok(BorrowedDocument {
                contract: source.contract().schema_version(),
                generation: source.generation(),
                response: std::str::from_utf8(source.bytes())
                    .map_err(|_| failure(TeletextDesignDocumentFailure::Source))?,
            })
        })
        .collect()
}

/// Restore checked historical originals without packages or invocation receipts.
/// The repository must first verify its digest over these exact bytes. Native
/// compilation and fresh promotion fences remain separate host operations.
/// # Errors
/// Rejects unknown/closed format, original-only policy, context or trusted limits.
pub fn decode_teletext_design_documents(
    bytes: &[u8],
    budget: TeletextDesignDocumentBudget,
) -> Result<RestoredTeletextDesignDocuments, TeletextDesignDocumentError> {
    preflight_json(bytes, budget)?;
    let envelope: Envelope =
        serde_json::from_slice(bytes).map_err(|_| failure(TeletextDesignDocumentFailure::Json))?;
    if envelope.format_version != TELETEXT_DESIGN_DOCUMENT_VERSION {
        return Err(failure(TeletextDesignDocumentFailure::Version));
    }
    if envelope.edit_policy.policy_version != 1 || envelope.edit_policy.kind != "original_only" {
        return Err(failure(TeletextDesignDocumentFailure::EditPolicy));
    }
    let selection = TeletextChannelSelectionSnapshot {
        channel_id: envelope.selection.channel_id,
        bindings: envelope
            .selection
            .bindings
            .into_iter()
            .map(Member::into_domain)
            .collect::<Result<_, _>>()?,
    };
    validate_context(&selection, &envelope.generation, budget)?;
    let guide_source = envelope.guide_source.0;
    if guide_source.is_some() && envelope.guide_inputs.0.is_none() {
        return Err(failure(TeletextDesignDocumentFailure::GuideSource));
    }
    let guide_inputs = envelope
        .guide_inputs
        .0
        .map(|inputs| {
            inputs.restore(
                &selection,
                &envelope.generation,
                budget,
                guide_source.as_ref(),
            )
        })
        .transpose()?;
    let borrowed = envelope
        .documents
        .iter()
        .map(|v| BorrowedDocument {
            contract: v.contract,
            generation: &v.generation,
            response: &v.response,
        })
        .collect::<Vec<_>>();
    let parsed = parse_sources(
        &selection,
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
        envelope.entry_addon_id.0.as_deref(),
        entry,
        envelope.v1_region.0,
        &parsed.documents,
    )?;
    let editorial_overlay = envelope
        .editorial_overlay
        .0
        .map(|overlay| overlay.restore(&parsed.documents, budget))
        .transpose()?;
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
    let sources = restore_original_sources(envelope.documents, parsed.authoring)?;
    Ok(RestoredTeletextDesignDocuments {
        parts: TeletextRestoredDesignDocumentParts {
            selection,
            generation: envelope.generation,
            sources,
            documents: parsed.documents,
            entry_addon_id: envelope.entry_addon_id.0,
            entry,
            v1_region: envelope.v1_region.0,
            resource_transmissions: transmissions,
            edit_policy: TeletextSavedEditPolicy::OriginalOnly,
            guide_inputs,
            guide_source,
            editorial_overlay,
        },
    })
}

// Parent preflight bounds document count and exact response bytes before either
// source vector is allocated; preserve each source string without reserialization.
fn restore_original_sources(
    documents: Vec<Document>,
    authoring: Vec<Option<ersatzrs_addon_contract::TeletextAuthoringMetadataWire>>,
) -> Result<Vec<TeletextSavedDocumentSource>, TeletextDesignDocumentError> {
    documents
        .into_iter()
        .zip(authoring)
        .map(|(v, authoring)| {
            Ok(TeletextSavedDocumentSource {
                contract: contract(v.contract)?,
                generation: v.generation,
                bytes: v.response.into_bytes(),
                authoring,
            })
        })
        .collect()
}
struct ParsedSources {
    documents: Vec<TeletextProfileDocument>,
    authoring: Vec<Option<ersatzrs_addon_contract::TeletextAuthoringMetadataWire>>,
}

fn parse_sources(
    selection: &TeletextChannelSelectionSnapshot,
    generation: &str,
    sources: &[BorrowedDocument<'_>],
    budget: TeletextProfileDocumentBudget,
) -> Result<ParsedSources, TeletextDesignDocumentError> {
    parse_source_documents(
        selection.bindings.iter().map(|binding| binding.contract),
        generation,
        sources,
        budget,
    )
}

fn parse_source_documents(
    contracts: impl ExactSizeIterator<Item = TeletextPageContract>,
    generation: &str,
    sources: &[BorrowedDocument<'_>],
    budget: TeletextProfileDocumentBudget,
) -> Result<ParsedSources, TeletextDesignDocumentError> {
    if contracts.len() != sources.len() {
        return Err(failure(TeletextDesignDocumentFailure::Association));
    }
    storage::<TeletextProfileDocument>(sources.len())?;
    storage::<Option<ersatzrs_addon_contract::TeletextAuthoringMetadataWire>>(sources.len())?;
    let mut documents = Vec::with_capacity(sources.len());
    let mut authoring = Vec::with_capacity(sources.len());
    for (index, (selected, source)) in contracts.zip(sources).enumerate() {
        let error = |reason| TeletextDesignDocumentError {
            binding: Some(index),
            reason,
        };
        if source.contract != selected.schema_version() || source.generation != generation {
            return Err(error(TeletextDesignDocumentFailure::Association));
        }
        let (document, metadata) = match selected {
            TeletextPageContract::V1 => {
                let base = parse_page_set(source.response.as_bytes(), generation)
                    .map_err(|_| error(TeletextDesignDocumentFailure::Document))?;
                budget
                    .service
                    .validate_base_with_page_metadata(&base)
                    .map_err(|_| error(TeletextDesignDocumentFailure::Document))?;
                (TeletextProfileDocument::Base(base), None)
            }
            TeletextPageContract::V2 => parse_declared_service(
                source.response.as_bytes(),
                generation,
                budget.service,
                budget.declarations,
            )
            .map(|v| (TeletextProfileDocument::Authored(Box::new(v)), None))
            .map_err(|_| error(TeletextDesignDocumentFailure::Document))?,
            TeletextPageContract::V3 => {
                let (service, metadata) = parse_authored_service_with_metadata(
                    source.response.as_bytes(),
                    generation,
                    budget.service,
                    budget.declarations,
                )
                .map_err(|_| error(TeletextDesignDocumentFailure::Document))?;
                (
                    TeletextProfileDocument::Authored(Box::new(service)),
                    Some(metadata),
                )
            }
        };
        documents.push(document);
        authoring.push(metadata);
    }
    Ok(ParsedSources {
        documents,
        authoring,
    })
}
fn validate_entry(
    selection: &TeletextChannelSelectionSnapshot,
    addon: Option<&str>,
    entry: Option<TeletextPageAddress>,
    region: Option<TeletextSavedV1Region>,
    documents: &[TeletextProfileDocument],
) -> Result<(), TeletextDesignDocumentError> {
    let has_v1 = selection
        .bindings
        .iter()
        .any(|v| v.contract == TeletextPageContract::V1);
    let index = addon.and_then(|addon| selection.bindings.iter().position(|v| v.addon_id == addon));
    validate_producer_entry(index, addon.is_some(), entry, region, documents, has_v1)
}

fn validate_producer_entry(
    index: Option<usize>,
    addon_present: bool,
    entry: Option<TeletextPageAddress>,
    region: Option<TeletextSavedV1Region>,
    documents: &[TeletextProfileDocument],
    has_v1: bool,
) -> Result<(), TeletextDesignDocumentError> {
    if has_v1 != region.is_some() {
        return Err(failure(TeletextDesignDocumentFailure::Region));
    }
    if documents.is_empty() {
        return if !addon_present && entry.is_none() {
            Ok(())
        } else {
            Err(failure(TeletextDesignDocumentFailure::Entry))
        };
    }
    let index = index.ok_or_else(|| failure(TeletextDesignDocumentFailure::Entry))?;
    let entry = entry.ok_or_else(|| failure(TeletextDesignDocumentFailure::Entry))?;
    let base = match documents
        .get(index)
        .ok_or_else(|| failure(TeletextDesignDocumentFailure::Entry))?
    {
        TeletextProfileDocument::Base(base) => base,
        TeletextProfileDocument::Authored(service) => service.semantic().base(),
    };
    if entry.page != base.initial_page || !base.pages.iter().any(|page| page.address == entry) {
        return Err(TeletextDesignDocumentError {
            binding: Some(index),
            reason: TeletextDesignDocumentFailure::Entry,
        });
    }
    Ok(())
}
fn validate_context(
    selection: &TeletextChannelSelectionSnapshot,
    generation: &str,
    budget: TeletextDesignDocumentBudget,
) -> Result<(), TeletextDesignDocumentError> {
    limit(
        selection.bindings.len(),
        budget.max_documents.min(MAX_BINDINGS),
        "bindings",
    )?;
    storage::<Member>(selection.bindings.len())?;
    if selection.channel_id <= 0
        || generation.is_empty()
        || generation.len() > ersatzrs_addon_contract::MAX_TELETEXT_PAGES_GENERATION_BYTES
    {
        return Err(failure(TeletextDesignDocumentFailure::Context));
    }
    let mut bytes = generation.len();
    for (index, binding) in selection.bindings.iter().enumerate() {
        // Raw settings storage is fenced before the duplicate-key set is created.
        limit(
            binding.effective_settings.len(),
            MAX_TELETEXT_PROFILE_SETTINGS,
            "settings",
        )?;
        storage::<Setting>(binding.effective_settings.len())?;
        for value in binding_strings(binding) {
            limit(value.len(), budget.max_context_field_bytes, "context field")?;
            bytes = bytes
                .checked_add(value.len())
                .ok_or_else(|| failure(TeletextDesignDocumentFailure::Storage))?;
        }
        limit(bytes, budget.max_context_bytes, "context")?;
        validate_binding(binding, selection.channel_id).map_err(|mut error| {
            error.binding = Some(index);
            error
        })?;
        if index > 0 && selection.bindings[index - 1].addon_id >= binding.addon_id {
            return Err(failure(TeletextDesignDocumentFailure::Association));
        }
    }
    limit(bytes, budget.max_context_bytes, "context")
}
fn binding_strings(binding: &TeletextSelectionSnapshot) -> impl Iterator<Item = &str> {
    profile_strings(
        &binding.addon_id,
        &binding.installation,
        &binding.effective_settings,
    )
}

fn profile_strings<'a>(
    addon_id: &'a str,
    v: &'a TeletextInstallationSnapshot,
    settings: &'a [TeletextProfileSetting],
) -> impl Iterator<Item = &'a str> {
    [
        addon_id,
        v.addon_id.as_str(),
        v.state_revision.as_str(),
        v.version.as_str(),
        v.package_sha256.as_str(),
        v.content_sha256.as_str(),
        v.manifest_sha256.as_str(),
    ]
    .into_iter()
    .chain(settings.iter().flat_map(|setting| {
        let value = match &setting.value {
            TeletextProfileValue::Literal(v)
            | TeletextProfileValue::SecretReference { reference: v, .. } => v,
        };
        [setting.key.as_str(), value.as_str()]
    }))
}
fn validate_binding(
    binding: &TeletextSelectionSnapshot,
    channel_id: EntityId,
) -> Result<(), TeletextDesignDocumentError> {
    if binding.binding_revision <= 0 {
        return Err(failure(TeletextDesignDocumentFailure::Context));
    }
    validate_profile_identity(
        binding.channel_id,
        channel_id,
        &binding.addon_id,
        binding.profile_id,
        binding.profile_revision,
        &binding.installation,
        &binding.effective_settings,
    )
}

fn validate_profile_identity(
    observed_channel: EntityId,
    channel_id: EntityId,
    addon_id: &str,
    profile_id: EntityId,
    profile_revision: i64,
    v: &TeletextInstallationSnapshot,
    settings: &[TeletextProfileSetting],
) -> Result<(), TeletextDesignDocumentError> {
    if observed_channel != channel_id
        || profile_id <= 0
        || profile_revision <= 0
        || v.addon_id != addon_id
        || v.state_revision.is_empty()
        || v.version.is_empty()
        || addon_id.len() > 255
        || ersatzrs_addon_contract::validate_addon_id(addon_id).is_err()
        || [&v.package_sha256, &v.content_sha256, &v.manifest_sha256]
            .iter()
            .any(|value| value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        || validate_teletext_profile_values("saved originals", settings).is_err()
    {
        return Err(failure(TeletextDesignDocumentFailure::Context));
    }
    Ok(())
}
fn validate_transmissions(
    values: &[TeletextSavedResourceTransmission],
    budget: TeletextDesignDocumentBudget,
) -> Result<(), TeletextDesignDocumentError> {
    limit(
        values.len(),
        budget.max_resource_transmissions,
        "resource transmissions",
    )?;
    storage::<TeletextSavedResourceTransmission>(values.len())?;
    let mut unique = BTreeSet::new();
    for value in values {
        if !unique.insert(value.resource) {
            return Err(failure(TeletextDesignDocumentFailure::Lifecycle));
        }
    }
    Ok(())
}

fn validate_capture_context(
    input: &TeletextDesignDocumentInput<'_>,
    budget: TeletextDesignDocumentBudget,
) -> Result<usize, TeletextDesignDocumentError> {
    let mut bytes = input.generation.len();
    let mut add = |text: &str| -> Result<(), TeletextDesignDocumentError> {
        limit(text.len(), budget.max_context_field_bytes, "context field")?;
        bytes = bytes
            .checked_add(text.len())
            .ok_or_else(|| failure(TeletextDesignDocumentFailure::Storage))?;
        limit(bytes, budget.max_context_bytes, "context")
    };
    for binding in &input.selection.bindings {
        for text in binding_strings(binding) {
            add(text)?;
        }
        for setting in &binding.effective_settings {
            add(match &setting.value {
                TeletextProfileValue::Literal(_) => "literal",
                TeletextProfileValue::SecretReference {
                    kind: TeletextSecretReferenceKind::Environment,
                    ..
                } => "environment",
                TeletextProfileValue::SecretReference {
                    kind: TeletextSecretReferenceKind::File,
                    ..
                } => "file",
            })?;
        }
    }
    for source in input.sources {
        add(source.generation())?;
    }
    if let Some(addon) = input.entry_addon_id {
        add(addon)?;
    }
    if input.v1_region.is_some() {
        add("western_latin")?;
    }
    add("original_only")?;
    for record in input.resource_transmissions {
        add(match record.update {
            TeletextSavedResourceUpdate::Changed => "changed",
            TeletextSavedResourceUpdate::Unchanged => "unchanged",
        })?;
        add(match record.resource.kind() {
            ersatzrs_domain::TeletextResourceKind::Pop => "pop",
            ersatzrs_domain::TeletextResourceKind::Gpop => "gpop",
            ersatzrs_domain::TeletextResourceKind::Drcs => "drcs",
            ersatzrs_domain::TeletextResourceKind::Gdrcs => "gdrcs",
            ersatzrs_domain::TeletextResourceKind::Mot => "mot",
            ersatzrs_domain::TeletextResourceKind::Mip => "mip",
        })?;
    }
    limit(bytes, budget.max_context_bytes, "context")?;
    Ok(bytes)
}

struct BoundedWriter {
    bytes: Vec<u8>,
    maximum: usize,
    exceeded: bool,
}
impl io::Write for BoundedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let count = self.bytes.len().checked_add(bytes.len());
        if count.is_none_or(|count| count > self.maximum || count > isize::MAX as usize) {
            self.exceeded = true;
            return Err(io::Error::other("saved output limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

// Streaming preflight retains one key, not a JSON tree. Owned DTO allocation
// follows its count/string/depth bounds. Escaped-string scratch remains within
// the total input cap; no total process RSS or CPU-cycle bound is claimed.
#[derive(Clone, Copy)]
enum Path {
    Other,
    Root,
    Selection,
    EffectiveSelection,
    EffectiveBindings,
    EffectiveBinding,
    PublishedOverride,
    OverrideState,
    ProducerBindings,
    Bindings,
    Binding,
    Settings,
    Documents,
    Document,
    Response,
    Transmissions,
    GuideInputs,
    GuideRequest,
    GuideSource,
    GuideChannels,
    GuideDigest,
    EditorialOverlay,
    EditorialPages,
    EditorialPage,
    EditorialRows,
    EditorialRow,
    EditorialCells,
    EditorialOperands,
    EditorialOperand,
    EditorialObjectOperands,
    EditorialObjectOperand,
    EditorialResourceOperands,
    EditorialResourceOperand,
    EditorialGlyphPrograms,
    EditorialGlyphProgram,
    EditorialResourcePrograms,
    EditorialResourceProgram,
    EditorialResourceProgramDefinition,
    EditorialResourceProgramCommands,
    EditorialResourceProgramCommand,
    EditorialResourceRetainedCommands,
    EditorialResourceRetainedCommand,
    EditorialProgramDefinition,
    EditorialProgramCommands,
    EditorialProgramCommand,
    EditorialRetainedCommands,
    EditorialRetainedCommand,
    EditorialBitmaps,
    EditorialBitmap,
    EditorialBitmapRows,
}
struct Scan {
    budget: TeletextDesignDocumentBudget,
    effective: bool,
    responses: usize,
    context: usize,
    editorial_items: usize,
    editorial_operands: usize,
    editorial_programs: usize,
    editorial_bytes: usize,
    error: Option<TeletextDesignDocumentError>,
}
struct ScanSeed<'a> {
    scan: &'a mut Scan,
    path: Path,
    depth: usize,
}
struct CountedSeed<'a> {
    inner: ScanSeed<'a>,
    count: usize,
    maximum: usize,
    dimension: &'static str,
}
impl<'de> DeserializeSeed<'de> for CountedSeed<'_> {
    type Value = ();
    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        self.inner
            .scan
            .check(limit(self.count, self.maximum, self.dimension))?;
        if matches!(self.inner.path, Path::EditorialPage) {
            self.inner
                .scan
                .check(storage::<editorial_overlay::StoredPage>(self.count))?;
        }
        if matches!(self.inner.path, Path::EditorialOperand) {
            self.inner
                .scan
                .check(storage::<editorial_overlay::StoredOperand>(self.count))?;
        }
        if matches!(self.inner.path, Path::EditorialObjectOperand) {
            self.inner.scan.check(storage::<
                editorial_overlay::object_operands::StoredObjectOperand,
            >(self.count))?;
        }
        if matches!(self.inner.path, Path::EditorialResourceOperand) {
            self.inner.scan.check(storage::<
                editorial_overlay::resource_object_operands::StoredResourceObjectOperand,
            >(self.count))?;
        }
        if matches!(self.inner.path, Path::EditorialGlyphProgram) {
            self.inner.scan.check(storage::<
                editorial_overlay::local_object_glyph_programs::StoredGlyphProgram,
            >(self.count))?;
        }
        if matches!(self.inner.path, Path::EditorialResourceProgram) {
            self.inner.scan.check(storage::<
                editorial_overlay::resource_object_programs::StoredResourceProgram,
            >(self.count))?;
        }
        if matches!(
            self.inner.path,
            Path::EditorialProgramCommand | Path::EditorialResourceProgramCommand
        ) {
            self.inner
                .scan
                .check(storage::<ersatzrs_domain::TeletextEnhancementCommand>(
                    self.count,
                ))?;
        }
        if matches!(
            self.inner.path,
            Path::EditorialRetainedCommand | Path::EditorialResourceRetainedCommand
        ) {
            self.inner
                .scan
                .check(storage::<Option<usize>>(self.count))?;
        }
        if matches!(self.inner.path, Path::EditorialBitmap) {
            self.inner
                .scan
                .check(storage::<editorial_overlay::StoredBitmap>(self.count))?;
        }
        self.inner.scan.editorial(self.inner.path)?;
        self.inner.deserialize(deserializer)
    }
}
impl Scan {
    fn editorial<E: serde::de::Error>(&mut self, path: Path) -> Result<(), E> {
        if !matches!(
            path,
            Path::EditorialPage
                | Path::EditorialRow
                | Path::EditorialOperand
                | Path::EditorialObjectOperand
                | Path::EditorialResourceOperand
                | Path::EditorialGlyphProgram
                | Path::EditorialResourceProgram
                | Path::EditorialProgramCommand
                | Path::EditorialResourceProgramCommand
                | Path::EditorialRetainedCommand
                | Path::EditorialResourceRetainedCommand
                | Path::EditorialBitmap
        ) {
            return Ok(());
        }
        if matches!(
            path,
            Path::EditorialOperand
                | Path::EditorialObjectOperand
                | Path::EditorialResourceOperand
                | Path::EditorialProgramCommand
                | Path::EditorialResourceProgramCommand
        ) {
            self.editorial_operands = self
                .editorial_operands
                .checked_add(1)
                .ok_or_else(|| E::custom("saved operand count overflow"))?;
            self.check(limit(
                self.editorial_operands,
                self.budget.validation.service.max_commands,
                "editorial operands",
            ))?;
        }
        if matches!(
            path,
            Path::EditorialGlyphProgram | Path::EditorialResourceProgram
        ) {
            self.editorial_programs = self
                .editorial_programs
                .checked_add(1)
                .ok_or_else(|| E::custom("saved program count overflow"))?;
            self.check(limit(
                self.editorial_programs,
                self.budget.validation.service.max_objects,
                "editorial object programs",
            ))?;
        }
        self.editorial_items = self
            .editorial_items
            .checked_add(1)
            .ok_or_else(|| E::custom("saved overlay storage overflow"))?;
        self.check(limit(
            self.editorial_items,
            self.budget.validation.service.max_structure_items,
            "editorial structure items",
        ))?;
        if matches!(path, Path::EditorialRow) {
            self.editorial_bytes = self
                .editorial_bytes
                .checked_add(40)
                .ok_or_else(|| E::custom("saved overlay storage overflow"))?;
            self.check(limit(
                self.editorial_bytes,
                self.budget.validation.service.max_base_content_bytes,
                "editorial content bytes",
            ))?;
        }
        Ok(())
    }
    fn check<E: serde::de::Error>(
        &mut self,
        result: Result<(), TeletextDesignDocumentError>,
    ) -> Result<(), E> {
        result.map_err(|error| {
            self.error = Some(error);
            E::custom("saved input limit")
        })
    }
    fn text<E: serde::de::Error>(&mut self, path: Path, text: &str) -> Result<(), E> {
        if matches!(path, Path::Response) {
            self.check(limit(
                text.len(),
                ersatzrs_addon_contract::MAX_TELETEXT_PAGES_OUTPUT_BYTES,
                "response",
            ))?;
            self.responses = self
                .responses
                .checked_add(text.len())
                .ok_or_else(|| E::custom("saved storage overflow"))?;
            self.check(limit(
                self.responses,
                self.budget.max_response_bytes,
                "responses",
            ))
        } else {
            let (maximum, dimension) = if matches!(path, Path::GuideRequest) {
                (
                    ersatzrs_addon_contract::MAX_TELETEXT_PAGES_INPUT_BYTES,
                    "guide request",
                )
            } else {
                (self.budget.max_context_field_bytes, "context field")
            };
            self.check(limit(text.len(), maximum, dimension))?;
            self.context = self
                .context
                .checked_add(text.len())
                .ok_or_else(|| E::custom("saved storage overflow"))?;
            self.check(limit(
                self.context,
                self.budget.max_context_bytes,
                "context",
            ))
        }
    }
}
impl<'de> DeserializeSeed<'de> for ScanSeed<'_> {
    type Value = ();
    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        if self.depth > MAX_DEPTH {
            return self
                .scan
                .check(Err(failure(TeletextDesignDocumentFailure::Depth)));
        }
        deserializer.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for ScanSeed<'_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("bounded saved JSON")
    }
    fn visit_bool<E: serde::de::Error>(self, _: bool) -> Result<(), E> {
        Ok(())
    }
    fn visit_i64<E: serde::de::Error>(self, _: i64) -> Result<(), E> {
        Ok(())
    }
    fn visit_u64<E: serde::de::Error>(self, _: u64) -> Result<(), E> {
        Ok(())
    }
    fn visit_f64<E: serde::de::Error>(self, _: f64) -> Result<(), E> {
        Ok(())
    }
    fn visit_unit<E: serde::de::Error>(self) -> Result<(), E> {
        Ok(())
    }
    fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<(), E> {
        self.scan.text(self.path, value)
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        let mut fields = 0;
        while let Some(key) = map.next_key::<String>()? {
            fields += 1;
            self.scan.check(limit(fields, 32, "object fields"))?;
            self.scan.check(limit(key.len(), 128, "key"))?;
            let path = match (self.path, key.as_str()) {
                (Path::Root, "selection") => Path::Selection,
                (Path::Root, "effective_selection") if self.scan.effective => {
                    Path::EffectiveSelection
                }
                (Path::Root, "producer_bindings") if self.scan.effective => Path::ProducerBindings,
                (Path::EffectiveSelection, "bindings") => Path::EffectiveBindings,
                (Path::EffectiveBinding, "base") | (Path::OverrideState, "candidate") => {
                    Path::Binding
                }
                (Path::EffectiveBinding, "scripted") => Path::PublishedOverride,
                (Path::PublishedOverride, "state") => Path::OverrideState,
                (Path::Root, "documents") => Path::Documents,
                (Path::Root, "resource_transmissions") => Path::Transmissions,
                (Path::Root, "guide_inputs") => Path::GuideInputs,
                (Path::GuideInputs, "v1" | "v2" | "v3") => Path::GuideRequest,
                (Path::Root, "guide_source") => Path::GuideSource,
                (Path::Root, "editorial_overlay") => Path::EditorialOverlay,
                (Path::EditorialOverlay, "pages") => Path::EditorialPages,
                (Path::EditorialOverlay, "local_program_operands") => Path::EditorialOperands,
                (Path::EditorialOverlay, "drcs_bitmap_overlays") => Path::EditorialBitmaps,
                (Path::EditorialOverlay, "local_object_operands") => Path::EditorialObjectOperands,
                (Path::EditorialOverlay, "resource_object_operands") => {
                    Path::EditorialResourceOperands
                }
                (Path::EditorialOverlay, "local_object_glyph_program_overlays") => {
                    Path::EditorialGlyphPrograms
                }
                (Path::EditorialOverlay, "resource_object_program_overlays") => {
                    Path::EditorialResourcePrograms
                }
                (Path::EditorialResourceProgram, "expected" | "replacement") => {
                    Path::EditorialResourceProgramDefinition
                }
                (Path::EditorialResourceProgramDefinition, "commands") => {
                    Path::EditorialResourceProgramCommands
                }
                (Path::EditorialResourceProgram, "retained_commands") => {
                    Path::EditorialResourceRetainedCommands
                }
                (Path::EditorialGlyphProgram, "expected" | "replacement") => {
                    Path::EditorialProgramDefinition
                }
                (Path::EditorialProgramDefinition, "commands") => Path::EditorialProgramCommands,
                (Path::EditorialGlyphProgram, "retained_commands") => {
                    Path::EditorialRetainedCommands
                }
                (Path::EditorialBitmap, "expected" | "replacement") => Path::EditorialBitmapRows,
                (Path::EditorialPage, "rows") => Path::EditorialRows,
                (Path::EditorialRow, "cells") => Path::EditorialCells,
                (Path::GuideSource, "channel_ids") => Path::GuideChannels,
                (Path::GuideSource, "context_digest") => Path::GuideDigest,
                (Path::Selection, "bindings") => Path::Bindings,
                (Path::Binding, "effective_settings") => Path::Settings,
                (Path::Document, "response") => Path::Response,
                _ => Path::Other,
            };
            map.next_value_seed(ScanSeed {
                scan: self.scan,
                path,
                depth: self.depth + 1,
            })?;
        }
        Ok(())
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<(), A::Error> {
        let (maximum, path, dimension) = match self.path {
            Path::EffectiveBindings => (
                2 * MAX_BINDINGS,
                Path::EffectiveBinding,
                "effective bindings",
            ),
            Path::ProducerBindings => (
                self.scan.budget.max_documents.min(MAX_BINDINGS),
                Path::Other,
                "producer bindings",
            ),
            Path::Bindings => (
                self.scan.budget.max_documents.min(MAX_BINDINGS),
                Path::Binding,
                "bindings",
            ),
            Path::Documents => (
                self.scan.budget.max_documents.min(MAX_BINDINGS),
                Path::Document,
                "documents",
            ),
            Path::Settings => (MAX_TELETEXT_PROFILE_SETTINGS, Path::Other, "settings"),
            Path::Transmissions => (
                self.scan.budget.max_resource_transmissions,
                Path::Other,
                "resource transmissions",
            ),
            Path::GuideChannels => (
                MAX_TELETEXT_GUIDE_SOURCE_CHANNELS,
                Path::Other,
                "guide channels",
            ),
            Path::GuideDigest => (32, Path::Other, "guide digest"),
            Path::EditorialPages => (
                self.scan.budget.validation.service.max_records,
                Path::EditorialPage,
                "editorial pages",
            ),
            Path::EditorialOperands => (
                self.scan.budget.validation.service.max_commands,
                Path::EditorialOperand,
                "editorial operands",
            ),
            Path::EditorialObjectOperands => (
                self.scan.budget.validation.service.max_commands,
                Path::EditorialObjectOperand,
                "editorial operands",
            ),
            Path::EditorialResourceOperands => (
                self.scan.budget.validation.service.max_commands,
                Path::EditorialResourceOperand,
                "editorial operands",
            ),
            Path::EditorialBitmaps => (
                self.scan.budget.validation.service.max_structure_items,
                Path::EditorialBitmap,
                "editorial bitmaps",
            ),
            Path::EditorialBitmapRows => (10, Path::Other, "editorial bitmap rows"),
            Path::EditorialRows => (25, Path::EditorialRow, "editorial rows"),
            Path::EditorialCells => (40, Path::Other, "editorial cells"),
            _ => glyph_program_sequence_limits(self.path, self.scan.budget),
        };
        let mut count = 0_usize;
        loop {
            let next = count
                .checked_add(1)
                .ok_or_else(|| serde::de::Error::custom("saved count overflow"))?;
            if sequence
                .next_element_seed(CountedSeed {
                    inner: ScanSeed {
                        scan: self.scan,
                        path,
                        depth: self.depth + 1,
                    },
                    count: next,
                    maximum,
                    dimension,
                })?
                .is_none()
            {
                break;
            }
            count = next;
        }
        Ok(())
    }
}
fn glyph_program_sequence_limits(
    path: Path,
    budget: TeletextDesignDocumentBudget,
) -> (usize, Path, &'static str) {
    match path {
        Path::EditorialGlyphPrograms => (
            budget.validation.service.max_objects,
            Path::EditorialGlyphProgram,
            "editorial glyph programs",
        ),
        Path::EditorialResourcePrograms => (
            budget.validation.service.max_objects,
            Path::EditorialResourceProgram,
            "editorial resource programs",
        ),
        Path::EditorialResourceProgramCommands => (
            budget.validation.service.max_commands,
            Path::EditorialResourceProgramCommand,
            "editorial resource program commands",
        ),
        Path::EditorialResourceRetainedCommands => (
            budget.validation.service.max_structure_items,
            Path::EditorialResourceRetainedCommand,
            "editorial resource retained commands",
        ),
        Path::EditorialProgramCommands => (
            budget.validation.service.max_commands.min(208),
            Path::EditorialProgramCommand,
            "editorial glyph program commands",
        ),
        Path::EditorialRetainedCommands => (
            budget.validation.service.max_structure_items.min(208),
            Path::EditorialRetainedCommand,
            "editorial retained commands",
        ),
        _ => (MAX_BINDINGS, Path::Other, "sequence"),
    }
}

fn preflight_json(
    bytes: &[u8],
    budget: TeletextDesignDocumentBudget,
) -> Result<(), TeletextDesignDocumentError> {
    preflight_json_for_format(bytes, budget, false)
}

fn preflight_json_for_format(
    bytes: &[u8],
    budget: TeletextDesignDocumentBudget,
    effective: bool,
) -> Result<(), TeletextDesignDocumentError> {
    limit(bytes.len(), budget.max_serialized_bytes, "serialized")?;
    limit(bytes.len(), isize::MAX as usize, "storage")?;
    storage::<Member>(budget.max_documents.min(MAX_BINDINGS))?;
    storage::<Transmission>(budget.max_resource_transmissions)?;
    let mut scan = Scan {
        budget,
        effective,
        responses: 0,
        context: 0,
        editorial_items: 0,
        editorial_operands: 0,
        editorial_programs: 0,
        editorial_bytes: 0,
        error: None,
    };
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    if (ScanSeed {
        scan: &mut scan,
        path: Path::Root,
        depth: 0,
    })
    .deserialize(&mut deserializer)
    .is_err()
    {
        return Err(scan
            .error
            .unwrap_or_else(|| failure(TeletextDesignDocumentFailure::Json)));
    }
    deserializer
        .end()
        .map_err(|_| failure(TeletextDesignDocumentFailure::Json))
}

#[cfg(test)]
#[path = "teletext_design_documents_tests.rs"]
mod tests;
