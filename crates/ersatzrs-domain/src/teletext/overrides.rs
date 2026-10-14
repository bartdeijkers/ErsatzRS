//! Candidate Lua controls; only atomic scheduling publication makes intervals live.

use std::fmt;

use chrono::{DateTime, Utc};

use super::{TeletextProfileCandidateSnapshot, TeletextProfileValue};
use crate::EntityId;

/// Provisional per-build admission bounds for Scripted teletext candidate work.
pub const MAX_TELETEXT_OVERRIDE_COMMANDS: usize = 1024;
pub const MAX_TELETEXT_OVERRIDE_SNAPSHOT_BYTES: usize = 4 * 1024 * 1024;

/// Membership and operator base state, observed without validating a profile or
/// package. It permits choosing an initial candidate, not invoking a producer.
#[derive(Clone, PartialEq, Eq)]
pub struct TeletextScriptedBaseSelection {
    pub channel_id: EntityId,
    pub addon_id: String,
    pub profile_id: EntityId,
    pub enabled: bool,
}

impl fmt::Debug for TeletextScriptedBaseSelection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TeletextScriptedBaseSelection")
            .field("channel_id", &self.channel_id)
            .field("profile_id", &self.profile_id)
            .field("enabled", &self.enabled)
            .finish_non_exhaustive()
    }
}

/// Explicit candidate override. Off needs only existing channel/add-on
/// membership; it must remain usable when the selected profile/package is invalid.
#[derive(Clone, PartialEq, Eq)]
pub enum TeletextOverrideState {
    Enabled(TeletextProfileCandidateSnapshot),
    Off {
        channel_id: EntityId,
        addon_id: String,
    },
}

impl TeletextOverrideState {
    #[must_use]
    pub const fn channel_id(&self) -> EntityId {
        match self {
            Self::Enabled(candidate) => candidate.channel_id,
            Self::Off { channel_id, .. } => *channel_id,
        }
    }

    #[must_use]
    pub fn addon_id(&self) -> &str {
        match self {
            Self::Enabled(candidate) => &candidate.addon_id,
            Self::Off { addon_id, .. } => addon_id,
        }
    }

    /// Retained representation and every variable identity/settings byte,
    /// including unresolved secret locations. Saturation never admits overflow.
    #[must_use]
    pub fn snapshot_bytes(&self) -> usize {
        let bytes = std::mem::size_of::<Self>().saturating_add(self.addon_id().len());
        let Self::Enabled(candidate) = self else {
            return bytes;
        };
        let installation = &candidate.installation;
        let bytes = [
            &installation.addon_id,
            &installation.state_revision,
            &installation.version,
            &installation.package_sha256,
            &installation.content_sha256,
            &installation.manifest_sha256,
        ]
        .iter()
        .fold(bytes, |bytes, value| bytes.saturating_add(value.len()));
        candidate
            .effective_settings
            .iter()
            .fold(bytes, |bytes, setting| {
                let value = match &setting.value {
                    TeletextProfileValue::Literal(value)
                    | TeletextProfileValue::SecretReference {
                        reference: value, ..
                    } => value,
                };
                bytes
                    .saturating_add(std::mem::size_of_val(setting))
                    .saturating_add(setting.key.len())
                    .saturating_add(value.len())
            })
    }
}

impl fmt::Debug for TeletextOverrideState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Enabled(candidate) => f.debug_tuple("Enabled").field(candidate).finish(),
            Self::Off { channel_id, .. } => f
                .debug_struct("Off")
                .field("channel_id", channel_id)
                .finish_non_exhaustive(),
        }
    }
}

/// Half-open [start, finish) candidate interval, owned by the state's output
/// channel. The publisher must recheck and persist it with its playout timeline;
/// this value alone is neither publication nor native generation authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TeletextOverrideInterval {
    pub start: DateTime<Utc>,
    pub finish: DateTime<Utc>,
    pub state: TeletextOverrideState,
}
