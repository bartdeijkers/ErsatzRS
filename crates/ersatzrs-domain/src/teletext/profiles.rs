//! Configuration identity for ADR 0122, without invocation or page readiness.
//!
//! Settings are unresolved literals or secret locations. No secret contents,
//! paths or setting values are included in their debug representation.

use std::{collections::BTreeSet, fmt};

use crate::EntityId;

/// Provisional host resource policies, not add-on theme or DVB semantics.
pub const MAX_TELETEXT_PROFILE_SETTINGS: usize = 256;
pub const MAX_TELETEXT_PROFILE_KEY_BYTES: usize = 128;
pub const MAX_TELETEXT_PROFILE_VALUE_BYTES: usize = 16 * 1024;
pub const MAX_TELETEXT_PROFILE_SETTINGS_BYTES: usize = 64 * 1024;
pub const MAX_TELETEXT_PROFILE_NAME_BYTES: usize = 256;

/// Host-supported authored contract chosen from the exact installed manifest.
/// This is an observed selection, not a producer negotiation or publication token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TeletextPageContract {
    V1,
    V2,
}

impl TeletextPageContract {
    /// Exact request/response version required for this selection.
    #[must_use]
    pub const fn schema_version(self) -> u32 {
        match self {
            Self::V1 => 1,
            Self::V2 => 2,
        }
    }

    /// Installed capability and fixed command environment value.
    #[must_use]
    pub const fn capability(self) -> &'static str {
        match self {
            Self::V1 => "teletext.pages.v1",
            Self::V2 => "teletext.pages.v2",
        }
    }
}

/// Exact database installation observation, not immutable executable assurance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TeletextInstallationSnapshot {
    pub addon_id: String,
    pub state_revision: String,
    pub version: String,
    pub package_sha256: String,
    pub content_sha256: String,
    pub manifest_sha256: String,
}

/// Location kind only; contents are never persisted in a profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TeletextSecretReferenceKind {
    Environment,
    File,
}

/// An unresolved value; manifest-specific meaning belongs to the adapter.
#[derive(Clone, PartialEq, Eq)]
pub enum TeletextProfileValue {
    Literal(String),
    SecretReference {
        kind: TeletextSecretReferenceKind,
        reference: String,
    },
}

impl fmt::Debug for TeletextProfileValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Literal(_) => formatter.write_str("Literal(<redacted>)"),
            Self::SecretReference { kind, .. } => formatter
                .debug_struct("SecretReference")
                .field("kind", kind)
                .finish_non_exhaustive(),
        }
    }
}

/// One override. Absence of its key inherits the captured installation value
/// and then the manifest default. Replacing a bag removes absent overrides.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TeletextProfileSetting {
    pub key: String,
    pub value: TeletextProfileValue,
}

/// Static structural failures; no submitted content is retained.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TeletextProfileValueError {
    #[error("teletext profile name is empty or exceeds its resource policy")]
    Name,
    #[error("teletext profile settings exceed their resource policy")]
    SettingsBound,
    #[error("teletext profile setting key is empty or duplicated")]
    SettingKey,
}

/// Check neutral resource constraints; manifest type/required checks are separate.
/// # Errors
/// Returns a static error for an invalid name, duplicate key or exceeded budget.
pub fn validate_teletext_profile_values(
    name: &str,
    settings: &[TeletextProfileSetting],
) -> Result<(), TeletextProfileValueError> {
    if name.trim().is_empty()
        || name.len() > MAX_TELETEXT_PROFILE_NAME_BYTES
        || name.chars().any(char::is_control)
    {
        return Err(TeletextProfileValueError::Name);
    }
    if settings.len() > MAX_TELETEXT_PROFILE_SETTINGS {
        return Err(TeletextProfileValueError::SettingsBound);
    }
    let mut keys = BTreeSet::new();
    let mut bytes = 0_usize;
    for setting in settings {
        if setting.key.is_empty() || !keys.insert(setting.key.as_str()) {
            return Err(TeletextProfileValueError::SettingKey);
        }
        let value = match &setting.value {
            TeletextProfileValue::Literal(value)
            | TeletextProfileValue::SecretReference {
                reference: value, ..
            } => value,
        };
        if setting.key.len() > MAX_TELETEXT_PROFILE_KEY_BYTES
            || value.len() > MAX_TELETEXT_PROFILE_VALUE_BYTES
            || value.contains('\0')
        {
            return Err(TeletextProfileValueError::SettingsBound);
        }
        bytes = bytes
            .saturating_add(setting.key.len())
            .saturating_add(value.len());
        if bytes > MAX_TELETEXT_PROFILE_SETTINGS_BYTES {
            return Err(TeletextProfileValueError::SettingsBound);
        }
    }
    Ok(())
}

/// Named shared configuration. A tombstone retains its dependent selections.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TeletextProfile {
    pub id: EntityId,
    pub addon_id: String,
    pub name: String,
    pub revision: i64,
    pub deleted: bool,
    pub settings: Vec<TeletextProfileSetting>,
    pub validated_installation: TeletextInstallationSnapshot,
}

/// Operator base selection, independent of future published Lua intervals.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChannelTeletextBinding {
    pub channel_id: EntityId,
    pub addon_id: String,
    pub profile_id: EntityId,
    pub enabled: bool,
    pub revision: i64,
}

/// Exact enabled base selection and unresolved effective settings observed in
/// one repository read transaction. This is neither source-byte assurance nor
/// executable/publication authority; consumers must compare a fresh snapshot.
/// The output channel is independent of any guide channels in a wire request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TeletextSelectionSnapshot {
    /// Selected within the same transaction as installation/effective settings.
    pub contract: TeletextPageContract,
    pub channel_id: EntityId,
    pub addon_id: String,
    pub binding_revision: i64,
    pub profile_id: EntityId,
    pub profile_revision: i64,
    pub installation: TeletextInstallationSnapshot,
    pub effective_settings: Vec<TeletextProfileSetting>,
}

/// Complete enabled base membership observed in one channel read transaction.
/// Bindings are ordered by add-on identity, exclude disabled selections, and
/// carry no page/publication authority. An empty vector proves no enabled base
/// bindings in that snapshot, not a missing teletext source inventory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TeletextChannelSelectionSnapshot {
    pub channel_id: EntityId,
    pub bindings: Vec<TeletextSelectionSnapshot>,
}

/// Configuration checks only: `Configured` grants no invocation, source-byte,
/// output/page-set, publication or playback-readiness authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TeletextProfileAvailability {
    Configured,
    MissingInstallation,
    DisabledInstallation,
    IncompatibleInstallation,
    InvalidSettings,
    DeletedProfile,
    StaleInstallation,
}

/// Installation state without its manifest, settings, executable path or secrets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TeletextInstallationRead {
    pub snapshot: Option<TeletextInstallationSnapshot>,
    pub availability: TeletextProfileAvailability,
}

/// Retained profile and current configuration-validation state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TeletextProfileRead {
    pub profile: TeletextProfile,
    pub availability: TeletextProfileAvailability,
}

/// Retained requested base state; disabled bindings still report configuration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChannelTeletextBindingRead {
    pub binding: ChannelTeletextBinding,
    pub availability: TeletextProfileAvailability,
}

#[cfg(test)]
#[path = "profiles_tests.rs"]
mod tests;
